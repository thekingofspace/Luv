use std::cell::RefCell;
use std::num::NonZeroU32;

use aws_lc_rs::encoding::AsBigEndian;
use aws_lc_rs::signature::{EcdsaKeyPair, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use aws_lc_rs::{aead, agreement, digest, pbkdf2, signature};
use base64::Engine as _;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::{DecodePaddingMode, general_purpose};
use mlua::{Lua, MetaMethod, Result, Table, UserData, UserDataFields, UserDataMethods, UserDataRef, Value};

use super::random::format_uuid;
use crate::datatypes::EnumItem;
use crate::datatypes::enums::{CIPHER_ALGORITHM, HASH_ALGORITHM, KEY_ALGORITHM};

const OFFLOAD: usize = 64 * 1024;
const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_RANDOM: usize = 64 * 1024 * 1024;
const SALT_LENGTH: usize = 16;

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

fn bytes(value: Value, what: &str) -> Result<Vec<u8>> {
    match value {
        Value::String(text) => Ok(text.as_bytes().to_vec()),
        Value::Buffer(buffer) => Ok(buffer.to_vec()),
        Value::Nil => Ok(Vec::new()),
        other => Err(runtime(format!("{what} must be a string or buffer, got {}", other.type_name()))),
    }
}

fn required(value: Value, what: &str) -> Result<Vec<u8>> {
    if value.is_nil() {
        return Err(runtime(format!("{what} must be a string or buffer, got nil")));
    }
    bytes(value, what)
}

fn length(what: &str, value: f64, limit: usize) -> Result<usize> {
    if value.is_finite() && value.fract() == 0.0 && value >= 1.0 && value <= limit as f64 {
        Ok(value as usize)
    } else {
        Err(runtime(format!("{what} must be a whole number between 1 and {limit}, got {value}")))
    }
}

async fn offload<T: Send + 'static>(heavy: bool, work: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    if !heavy {
        return Ok(work());
    }
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| runtime(format!("the crypto work stopped unexpectedly: {error}")))
}

fn secure_bytes(count: usize) -> Result<Vec<u8>> {
    let mut output = vec![0u8; count];
    aws_lc_rs::rand::fill(&mut output).map_err(|_| runtime("the system random number generator failed"))?;
    Ok(output)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hash {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Sha3_256,
    Sha3_384,
    Sha3_512,
    Blake3,
}

impl Hash {
    fn of(item: EnumItem) -> Result<Hash> {
        Ok(match item.of(HASH_ALGORITHM)?.name {
            "MD5" => Hash::Md5,
            "SHA1" => Hash::Sha1,
            "SHA224" => Hash::Sha224,
            "SHA256" => Hash::Sha256,
            "SHA384" => Hash::Sha384,
            "SHA512" => Hash::Sha512,
            "SHA3_256" => Hash::Sha3_256,
            "SHA3_384" => Hash::Sha3_384,
            "SHA3_512" => Hash::Sha3_512,
            _ => Hash::Blake3,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Hash::Md5 => "MD5",
            Hash::Sha1 => "SHA1",
            Hash::Sha224 => "SHA224",
            Hash::Sha256 => "SHA256",
            Hash::Sha384 => "SHA384",
            Hash::Sha512 => "SHA512",
            Hash::Sha3_256 => "SHA3_256",
            Hash::Sha3_384 => "SHA3_384",
            Hash::Sha3_512 => "SHA3_512",
            Hash::Blake3 => "BLAKE3",
        }
    }

    fn block(self) -> usize {
        match self {
            Hash::Sha384 | Hash::Sha512 => 128,
            Hash::Sha3_256 => 136,
            Hash::Sha3_384 => 104,
            Hash::Sha3_512 => 72,
            _ => 64,
        }
    }

    fn output(self) -> usize {
        match self {
            Hash::Md5 => 16,
            Hash::Sha1 => 20,
            Hash::Sha224 => 28,
            Hash::Sha256 | Hash::Sha3_256 | Hash::Blake3 => 32,
            Hash::Sha384 | Hash::Sha3_384 => 48,
            Hash::Sha512 | Hash::Sha3_512 => 64,
        }
    }

    fn algorithm(self) -> &'static digest::Algorithm {
        match self {
            Hash::Sha1 => &digest::SHA1_FOR_LEGACY_USE_ONLY,
            Hash::Sha224 => &digest::SHA224,
            Hash::Sha384 => &digest::SHA384,
            Hash::Sha512 => &digest::SHA512,
            Hash::Sha3_256 => &digest::SHA3_256,
            Hash::Sha3_384 => &digest::SHA3_384,
            Hash::Sha3_512 => &digest::SHA3_512,
            _ => &digest::SHA256,
        }
    }

    fn pbkdf2(self) -> Option<pbkdf2::Algorithm> {
        match self {
            Hash::Sha1 => Some(pbkdf2::PBKDF2_HMAC_SHA1),
            Hash::Sha256 => Some(pbkdf2::PBKDF2_HMAC_SHA256),
            Hash::Sha384 => Some(pbkdf2::PBKDF2_HMAC_SHA384),
            Hash::Sha512 => Some(pbkdf2::PBKDF2_HMAC_SHA512),
            _ => None,
        }
    }
}

#[derive(Clone)]
enum State {
    Sha(digest::Context),
    Md5(md5::Md5),
    Blake3(Box<blake3::Hasher>),
}

impl State {
    fn new(hash: Hash) -> State {
        match hash {
            Hash::Md5 => State::Md5(<md5::Md5 as md5::Digest>::new()),
            Hash::Blake3 => State::Blake3(Box::new(blake3::Hasher::new())),
            other => State::Sha(digest::Context::new(other.algorithm())),
        }
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            State::Sha(context) => context.update(data),
            State::Md5(hasher) => md5::Digest::update(hasher, data),
            State::Blake3(hasher) => {
                hasher.update(data);
            }
        }
    }

    fn finish(self) -> Vec<u8> {
        match self {
            State::Sha(context) => context.finish().as_ref().to_vec(),
            State::Md5(hasher) => md5::Digest::finalize(hasher).to_vec(),
            State::Blake3(hasher) => hasher.finalize().as_bytes().to_vec(),
        }
    }
}

fn hash_of(hash: Hash, data: &[u8]) -> Vec<u8> {
    let mut state = State::new(hash);
    state.update(data);
    state.finish()
}

struct Keyed {
    inner: State,
    outer: State,
}

impl Keyed {
    fn new(hash: Hash, key: &[u8]) -> Keyed {
        let mut block = if key.len() > hash.block() { hash_of(hash, key) } else { key.to_vec() };
        block.resize(hash.block(), 0);
        let mut inner = State::new(hash);
        inner.update(&block.iter().map(|byte| byte ^ 0x36).collect::<Vec<u8>>());
        let mut outer = State::new(hash);
        outer.update(&block.iter().map(|byte| byte ^ 0x5c).collect::<Vec<u8>>());
        Keyed { inner, outer }
    }

    fn sign(&self, parts: &[&[u8]]) -> Vec<u8> {
        let mut inner = self.inner.clone();
        for part in parts {
            inner.update(part);
        }
        let mut outer = self.outer.clone();
        outer.update(&inner.finish());
        outer.finish()
    }
}

fn hkdf(hash: Hash, secret: &[u8], salt: &[u8], info: &[u8], length: usize) -> Vec<u8> {
    let zeros = vec![0u8; hash.output()];
    let salt = if salt.is_empty() { &zeros[..] } else { salt };
    let key = Keyed::new(hash, salt).sign(&[secret]);
    let keyed = Keyed::new(hash, &key);
    let mut output = Vec::with_capacity(length + hash.output());
    let mut previous = Vec::new();
    let mut counter = 1u8;
    while output.len() < length {
        previous = keyed.sign(&[&previous, info, &[counter]]);
        output.extend_from_slice(&previous);
        counter = counter.wrapping_add(1);
    }
    output.truncate(length);
    output
}

fn pbkdf2_derive(hash: Hash, password: &[u8], salt: &[u8], iterations: NonZeroU32, length: usize) -> Vec<u8> {
    let mut output = vec![0u8; length];
    if let Some(algorithm) = hash.pbkdf2() {
        pbkdf2::derive(algorithm, iterations, salt, password, &mut output);
        return output;
    }
    let keyed = Keyed::new(hash, password);
    for (index, chunk) in output.chunks_mut(hash.output()).enumerate() {
        let block = (index as u32 + 1).to_be_bytes();
        let mut current = keyed.sign(&[salt, &block]);
        let mut total = current.clone();
        for _ in 1..iterations.get() {
            current = keyed.sign(&[&current]);
            for (sum, byte) in total.iter_mut().zip(&current) {
                *sum ^= byte;
            }
        }
        chunk.copy_from_slice(&total[..chunk.len()]);
    }
    output
}

#[derive(Clone, Copy)]
enum Cipher {
    Aes128,
    Aes256,
    ChaCha,
}

impl Cipher {
    fn of(item: EnumItem) -> Result<Cipher> {
        Ok(match item.of(CIPHER_ALGORITHM)?.name {
            "AES128GCM" => Cipher::Aes128,
            "AES256GCM" => Cipher::Aes256,
            _ => Cipher::ChaCha,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Cipher::Aes128 => "AES128GCM",
            Cipher::Aes256 => "AES256GCM",
            Cipher::ChaCha => "ChaCha20Poly1305",
        }
    }

    fn algorithm(self) -> &'static aead::Algorithm {
        match self {
            Cipher::Aes128 => &aead::AES_128_GCM,
            Cipher::Aes256 => &aead::AES_256_GCM,
            Cipher::ChaCha => &aead::CHACHA20_POLY1305,
        }
    }

    fn key(self, key: &[u8]) -> std::result::Result<aead::LessSafeKey, String> {
        let algorithm = self.algorithm();
        aead::UnboundKey::new(algorithm, key)
            .map(aead::LessSafeKey::new)
            .map_err(|_| {
                format!(
                    "a {} key must be {} bytes long, got {} bytes",
                    self.name(),
                    algorithm.key_len(),
                    key.len()
                )
            })
    }
}

fn seal(cipher: Cipher, key: &[u8], mut data: Vec<u8>, extra: &[u8]) -> std::result::Result<Vec<u8>, String> {
    let key = cipher.key(key)?;
    let mut nonce = [0u8; aead::NONCE_LEN];
    aws_lc_rs::rand::fill(&mut nonce).map_err(|_| "the system random number generator failed".to_owned())?;
    key.seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::from(extra), &mut data)
        .map_err(|_| "the data could not be encrypted".to_owned())?;
    let mut sealed = nonce.to_vec();
    sealed.extend_from_slice(&data);
    Ok(sealed)
}

fn open(cipher: Cipher, key: &[u8], sealed: &[u8], extra: &[u8]) -> std::result::Result<Option<Vec<u8>>, String> {
    let key = cipher.key(key)?;
    if sealed.len() < aead::NONCE_LEN + cipher.algorithm().tag_len() {
        return Ok(None);
    }
    let (nonce, body) = sealed.split_at(aead::NONCE_LEN);
    let Ok(nonce) = aead::Nonce::try_assume_unique_for_key(nonce) else {
        return Ok(None);
    };
    let mut body = body.to_vec();
    Ok(key
        .open_in_place(nonce, aead::Aad::from(extra), &mut body)
        .ok()
        .map(|plain| plain.to_vec()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyKind {
    Ed25519,
    P256,
    P384,
    X25519,
}

impl KeyKind {
    fn of(item: EnumItem) -> Result<KeyKind> {
        Ok(match item.of(KEY_ALGORITHM)?.name {
            "Ed25519" => KeyKind::Ed25519,
            "EcdsaP256" => KeyKind::P256,
            "EcdsaP384" => KeyKind::P384,
            _ => KeyKind::X25519,
        })
    }

    fn name(self) -> &'static str {
        match self {
            KeyKind::Ed25519 => "Ed25519",
            KeyKind::P256 => "EcdsaP256",
            KeyKind::P384 => "EcdsaP384",
            KeyKind::X25519 => "X25519",
        }
    }

    fn signing(self) -> Result<KeyKind> {
        if self == KeyKind::X25519 {
            return Err(runtime("X25519 keys are for SharedSecret, sign with Ed25519, EcdsaP256 or EcdsaP384"));
        }
        Ok(self)
    }

    fn ecdsa(self) -> (&'static signature::EcdsaSigningAlgorithm, &'static agreement::Algorithm) {
        match self {
            KeyKind::P384 => (&signature::ECDSA_P384_SHA384_FIXED_SIGNING, &agreement::ECDH_P384),
            _ => (&signature::ECDSA_P256_SHA256_FIXED_SIGNING, &agreement::ECDH_P256),
        }
    }
}

fn rejected(kind: KeyKind, what: &str) -> String {
    format!("the {what} is not a valid {} key", kind.name())
}

fn ecdsa_pair(kind: KeyKind, private: &[u8]) -> std::result::Result<EcdsaKeyPair, String> {
    let (signing, curve) = kind.ecdsa();
    let public = agreement::PrivateKey::from_private_key(curve, private)
        .map_err(|_| rejected(kind, "private key"))?
        .compute_public_key()
        .map_err(|_| rejected(kind, "private key"))?;
    EcdsaKeyPair::from_private_key_and_public_key(signing, private, public.as_ref())
        .map_err(|_| rejected(kind, "private key"))
}

fn generate(kind: KeyKind) -> std::result::Result<(Vec<u8>, Vec<u8>), String> {
    let failed = || format!("a {} key could not be generated", kind.name());
    match kind {
        KeyKind::Ed25519 => {
            let pair = Ed25519KeyPair::generate().map_err(|_| failed())?;
            let seed = pair.seed().map_err(|_| failed())?;
            let seed = AsBigEndian::as_be_bytes(&seed).map_err(|_| failed())?;
            Ok((seed.as_ref().to_vec(), pair.public_key().as_ref().to_vec()))
        }
        KeyKind::P256 | KeyKind::P384 => {
            let pair = EcdsaKeyPair::generate(kind.ecdsa().0).map_err(|_| failed())?;
            let private = AsBigEndian::as_be_bytes(&pair.private_key()).map_err(|_| failed())?;
            Ok((private.as_ref().to_vec(), pair.public_key().as_ref().to_vec()))
        }
        KeyKind::X25519 => {
            let private = agreement::PrivateKey::generate(&agreement::X25519).map_err(|_| failed())?;
            let public = private.compute_public_key().map_err(|_| failed())?;
            let seed: aws_lc_rs::encoding::Curve25519SeedBin =
                AsBigEndian::as_be_bytes(&private).map_err(|_| failed())?;
            Ok((seed.as_ref().to_vec(), public.as_ref().to_vec()))
        }
    }
}

fn public_key(kind: KeyKind, private: &[u8]) -> std::result::Result<Vec<u8>, String> {
    match kind {
        KeyKind::Ed25519 => Ed25519KeyPair::from_seed_unchecked(private)
            .map(|pair| pair.public_key().as_ref().to_vec())
            .map_err(|_| rejected(kind, "private key")),
        KeyKind::P256 | KeyKind::P384 => ecdsa_pair(kind, private).map(|pair| pair.public_key().as_ref().to_vec()),
        KeyKind::X25519 => agreement::PrivateKey::from_private_key(&agreement::X25519, private)
            .and_then(|key| key.compute_public_key().map_err(Into::into))
            .map(|public| public.as_ref().to_vec())
            .map_err(|_| rejected(kind, "private key")),
    }
}

fn sign(kind: KeyKind, private: &[u8], message: &[u8]) -> std::result::Result<Vec<u8>, String> {
    match kind {
        KeyKind::Ed25519 => Ed25519KeyPair::from_seed_unchecked(private)
            .map(|pair| pair.sign(message).as_ref().to_vec())
            .map_err(|_| rejected(kind, "private key")),
        _ => ecdsa_pair(kind, private)?
            .sign(&aws_lc_rs::rand::SystemRandom::new(), message)
            .map(|signature| signature.as_ref().to_vec())
            .map_err(|_| "the message could not be signed".to_owned()),
    }
}

fn verify(kind: KeyKind, public: &[u8], message: &[u8], signature: &[u8]) -> bool {
    let algorithm: &'static dyn signature::VerificationAlgorithm = match kind {
        KeyKind::Ed25519 => &signature::ED25519,
        KeyKind::P256 => &signature::ECDSA_P256_SHA256_FIXED,
        _ => &signature::ECDSA_P384_SHA384_FIXED,
    };
    UnparsedPublicKey::new(algorithm, public).verify(message, signature).is_ok()
}

fn shared_secret(private: &[u8], peer: &[u8]) -> std::result::Result<Vec<u8>, String> {
    let key = agreement::PrivateKey::from_private_key(&agreement::X25519, private)
        .map_err(|_| rejected(KeyKind::X25519, "private key"))?;
    agreement::agree(
        &key,
        agreement::UnparsedPublicKey::new(&agreement::X25519, peer),
        "the peer's public key is not a valid X25519 key".to_owned(),
        |secret| Ok(secret.to_vec()),
    )
}

fn to_hex(data: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(data.len() * 2);
    for byte in data {
        text.push(DIGITS[usize::from(byte >> 4)] as char);
        text.push(DIGITS[usize::from(byte & 15)] as char);
    }
    text
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    let text = text.strip_prefix("0x").unwrap_or(text);
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let digit = |byte: u8| (byte as char).to_digit(16).map(|value| value as u8);
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

fn base64_engine(url: bool) -> GeneralPurpose {
    let alphabet = if url { &base64::alphabet::URL_SAFE } else { &base64::alphabet::STANDARD };
    GeneralPurpose::new(
        alphabet,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    )
}

pub struct Hasher {
    hash: Hash,
    state: RefCell<Option<State>>,
}

impl Hasher {
    pub const TYPE_NAME: &'static str = "Hasher";

    fn busy() -> mlua::Error {
        runtime("the Hasher is still busy with another Update")
    }
}

impl UserData for Hasher {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::TYPE_NAME);
        fields.add_field_method_get("Algorithm", |lua, this| {
            EnumItem::find(HASH_ALGORITHM, this.hash.name()).map_or(Ok(Value::Nil), |item| item.canonical(lua))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("Update", |_, this: UserDataRef<Self>, data: Value| async move {
            let data = required(data, "the data")?;
            let mut state = this.state.borrow_mut().take().ok_or_else(Hasher::busy)?;
            let heavy = data.len() > OFFLOAD;
            let result = offload(heavy, move || {
                state.update(&data);
                state
            })
            .await;
            match result {
                Ok(state) => {
                    *this.state.borrow_mut() = Some(state);
                    Ok(())
                }
                Err(error) => {
                    *this.state.borrow_mut() = Some(State::new(this.hash));
                    Err(error)
                }
            }
        });
        methods.add_method("Finish", |lua, this, ()| {
            let state = this.state.borrow_mut().take().ok_or_else(Hasher::busy)?;
            *this.state.borrow_mut() = Some(State::new(this.hash));
            lua.create_string(state.finish())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("{}({})", Self::TYPE_NAME, this.hash.name()))
        });
    }
}

fn failure(message: String) -> mlua::Error {
    runtime(message)
}

pub fn create(lua: &Lua) -> Result<Table> {
    let crypto = lua.create_table()?;

    crypto.set(
        "Hash",
        lua.create_async_function(|lua, (algorithm, data): (EnumItem, Value)| async move {
            let hash = Hash::of(algorithm)?;
            let data = bytes(data, "the data")?;
            let digest = offload(data.len() > OFFLOAD, move || hash_of(hash, &data)).await?;
            lua.create_string(digest)
        })?,
    )?;
    crypto.set(
        "Hasher",
        lua.create_function(|_, algorithm: EnumItem| {
            let hash = Hash::of(algorithm)?;
            Ok(Hasher {
                hash,
                state: RefCell::new(Some(State::new(hash))),
            })
        })?,
    )?;
    crypto.set(
        "Hmac",
        lua.create_async_function(|lua, (algorithm, key, data): (EnumItem, Value, Value)| async move {
            let hash = Hash::of(algorithm)?;
            let (key, data) = (bytes(key, "the key")?, bytes(data, "the data")?);
            let heavy = data.len() + key.len() > OFFLOAD;
            let tag = offload(heavy, move || Keyed::new(hash, &key).sign(&[&data])).await?;
            lua.create_string(tag)
        })?,
    )?;
    crypto.set(
        "Hkdf",
        lua.create_async_function(
            |lua, (algorithm, secret, salt, info, size): (EnumItem, Value, Value, Value, f64)| async move {
                let hash = Hash::of(algorithm)?;
                let size = length("the length", size, 255 * hash.output())?;
                let (secret, salt, info) = (bytes(secret, "the secret")?, bytes(salt, "the salt")?, bytes(info, "the info")?);
                let heavy = secret.len() + salt.len() + info.len() > OFFLOAD;
                let key = offload(heavy, move || hkdf(hash, &secret, &salt, &info, size)).await?;
                lua.create_string(key)
            },
        )?,
    )?;
    crypto.set(
        "Pbkdf2",
        lua.create_async_function(
            |lua, (algorithm, password, salt, iterations, size): (EnumItem, Value, Value, f64, f64)| async move {
                let hash = Hash::of(algorithm)?;
                let size = length("the length", size, MAX_OUTPUT)?;
                let iterations = length("the iteration count", iterations, u32::MAX as usize)?;
                let iterations = NonZeroU32::new(iterations as u32).ok_or_else(|| runtime("at least 1 iteration is needed"))?;
                let (password, salt) = (bytes(password, "the password")?, bytes(salt, "the salt")?);
                let key = offload(true, move || pbkdf2_derive(hash, &password, &salt, iterations, size)).await?;
                lua.create_string(key)
            },
        )?,
    )?;
    crypto.set(
        "HashPassword",
        lua.create_async_function(|_, password: Value| async move {
            use argon2::password_hash::PasswordHasher;
            let password = bytes(password, "the password")?;
            let salt = secure_bytes(SALT_LENGTH)?;
            offload(true, move || {
                argon2::Argon2::default()
                    .hash_password_with_salt(&password, &salt)
                    .map(|hash| hash.to_string())
                    .map_err(|error| format!("the password could not be hashed: {error}"))
            })
            .await?
            .map_err(failure)
        })?,
    )?;
    crypto.set(
        "VerifyPassword",
        lua.create_async_function(|_, (password, hash): (Value, String)| async move {
            use argon2::password_hash::PasswordVerifier;
            let password = bytes(password, "the password")?;
            offload(true, move || argon2::Argon2::default().verify_password(&password, hash.as_str()).is_ok()).await
        })?,
    )?;
    crypto.set(
        "RandomBytes",
        lua.create_async_function(|lua, count: f64| async move {
            let count = length("the byte count", count, MAX_RANDOM)?;
            let random = offload(count > OFFLOAD, move || secure_bytes(count)).await??;
            lua.create_string(random)
        })?,
    )?;
    crypto.set(
        "RandomUUID",
        lua.create_function(|_, ()| {
            let random = secure_bytes(16)?;
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(&random);
            Ok(format_uuid(bytes))
        })?,
    )?;
    crypto.set(
        "GenerateKey",
        lua.create_function(|lua, algorithm: EnumItem| {
            let cipher = Cipher::of(algorithm)?;
            lua.create_string(secure_bytes(cipher.algorithm().key_len())?)
        })?,
    )?;
    crypto.set(
        "Encrypt",
        lua.create_async_function(
            |lua, (algorithm, key, data, extra): (EnumItem, Value, Value, Value)| async move {
                let cipher = Cipher::of(algorithm)?;
                let (key, data, extra) = (required(key, "the key")?, bytes(data, "the data")?, bytes(extra, "the associated data")?);
                let heavy = data.len() + extra.len() > OFFLOAD;
                let sealed = offload(heavy, move || seal(cipher, &key, data, &extra)).await?.map_err(failure)?;
                lua.create_string(sealed)
            },
        )?,
    )?;
    crypto.set(
        "Decrypt",
        lua.create_async_function(
            |lua, (algorithm, key, data, extra): (EnumItem, Value, Value, Value)| async move {
                let cipher = Cipher::of(algorithm)?;
                let (key, data, extra) = (required(key, "the key")?, bytes(data, "the data")?, bytes(extra, "the associated data")?);
                let heavy = data.len() + extra.len() > OFFLOAD;
                match offload(heavy, move || open(cipher, &key, &data, &extra)).await?.map_err(failure)? {
                    Some(plain) => Ok(Value::String(lua.create_string(plain)?)),
                    None => Ok(Value::Nil),
                }
            },
        )?,
    )?;
    crypto.set(
        "GenerateKeyPair",
        lua.create_function(|lua, algorithm: EnumItem| {
            let (private, public) = generate(KeyKind::of(algorithm)?).map_err(failure)?;
            let pair = lua.create_table()?;
            pair.set("PrivateKey", lua.create_string(private)?)?;
            pair.set("PublicKey", lua.create_string(public)?)?;
            Ok(pair)
        })?,
    )?;
    crypto.set(
        "GetPublicKey",
        lua.create_function(|lua, (algorithm, private): (EnumItem, Value)| {
            let public = public_key(KeyKind::of(algorithm)?, &required(private, "the private key")?).map_err(failure)?;
            lua.create_string(public)
        })?,
    )?;
    crypto.set(
        "Sign",
        lua.create_async_function(
            |lua, (algorithm, private, message): (EnumItem, Value, Value)| async move {
                let kind = KeyKind::of(algorithm)?.signing()?;
                let (private, message) = (required(private, "the private key")?, bytes(message, "the message")?);
                let heavy = message.len() > OFFLOAD;
                let signed = offload(heavy, move || sign(kind, &private, &message)).await?.map_err(failure)?;
                lua.create_string(signed)
            },
        )?,
    )?;
    crypto.set(
        "Verify",
        lua.create_async_function(
            |_, (algorithm, public, message, signature): (EnumItem, Value, Value, Value)| async move {
                let kind = KeyKind::of(algorithm)?.signing()?;
                let (public, message, signature) = (
                    required(public, "the public key")?,
                    bytes(message, "the message")?,
                    bytes(signature, "the signature")?,
                );
                let heavy = message.len() > OFFLOAD;
                offload(heavy, move || verify(kind, &public, &message, &signature)).await
            },
        )?,
    )?;
    crypto.set(
        "SharedSecret",
        lua.create_function(|lua, (algorithm, private, peer): (EnumItem, Value, Value)| {
            if KeyKind::of(algorithm)? != KeyKind::X25519 {
                return Err(runtime("SharedSecret needs X25519 keys, Ed25519 and ECDSA keys are for signing"));
            }
            let secret = shared_secret(&required(private, "the private key")?, &required(peer, "the peer's public key")?)
                .map_err(failure)?;
            lua.create_string(secret)
        })?,
    )?;
    crypto.set(
        "Equals",
        lua.create_function(|_, (left, right): (Value, Value)| {
            let (left, right) = (bytes(left, "the first value")?, bytes(right, "the second value")?);
            Ok(left.len() == right.len() && aws_lc_rs::constant_time::verify_slices_are_equal(&left, &right).is_ok())
        })?,
    )?;
    crypto.set("ToHex", lua.create_function(|_, data: Value| Ok(to_hex(&bytes(data, "the data")?)))?)?;
    crypto.set(
        "FromHex",
        lua.create_function(|lua, text: String| {
            let decoded = from_hex(&text).ok_or_else(|| runtime("the text is not valid hex"))?;
            lua.create_string(decoded)
        })?,
    )?;
    crypto.set(
        "ToBase64",
        lua.create_function(|_, (data, url): (Value, Option<bool>)| {
            let data = bytes(data, "the data")?;
            Ok(if url.unwrap_or(false) {
                general_purpose::URL_SAFE_NO_PAD.encode(data)
            } else {
                general_purpose::STANDARD.encode(data)
            })
        })?,
    )?;
    crypto.set(
        "FromBase64",
        lua.create_function(|lua, text: String| {
            let cleaned: String = text.chars().filter(|character| !character.is_ascii_whitespace()).collect();
            let url = cleaned.contains(['-', '_']);
            let decoded = base64_engine(url)
                .decode(cleaned.as_bytes())
                .map_err(|error| runtime(format!("the text is not valid base64: {error}")))?;
            lua.create_string(decoded)
        })?,
    )?;

    crypto.set_readonly(true);
    Ok(crypto)
}
