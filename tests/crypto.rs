mod common;

use common::{main_script, run_source};
use mlua::Table;

async fn evaluate(source: &str) -> common::Outcome {
    let dir = main_script(source);
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    outcome
}

fn text(results: &Table, name: &str) -> String {
    results.get::<String>(name).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn flag(results: &Table, name: &str) -> bool {
    results.get::<bool>(name).unwrap_or_else(|error| panic!("{name}: {error}"))
}

#[tokio::test]
async fn hashes_match_published_vectors() {
    let outcome = evaluate(
        r#"
local Crypto = import("Crypto")
results = {}
for _, name in { "MD5", "SHA1", "SHA224", "SHA256", "SHA384", "SHA512", "SHA3_256", "BLAKE3" } do
    results[name] = Crypto.ToHex(Crypto.Hash(enum.HashAlgorithm[name], "abc"))
end
results.fromBuffer = Crypto.ToHex(Crypto.Hash(enum.HashAlgorithm.SHA256, buffer.fromstring("abc")))
local hasher = Crypto.Hasher(enum.HashAlgorithm.SHA256)
hasher:Update("a")
hasher:Update(buffer.fromstring("bc"))
results.incremental = Crypto.ToHex(hasher:Finish())
hasher:Update("abc")
results.reused = Crypto.ToHex(hasher:Finish())
results.hasherKind = hasher.Algorithm.Name
"#,
    )
    .await;
    let results: Table = outcome.global("results");
    let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_eq!(text(&results, "MD5"), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(text(&results, "SHA1"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(text(&results, "SHA224"), "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7");
    assert_eq!(text(&results, "SHA256"), abc);
    assert_eq!(
        text(&results, "SHA384"),
        "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
    );
    assert_eq!(
        text(&results, "SHA512"),
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
    assert_eq!(text(&results, "SHA3_256"), "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532");
    assert_eq!(text(&results, "BLAKE3"), "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85");
    assert_eq!(text(&results, "fromBuffer"), abc);
    assert_eq!(text(&results, "incremental"), abc);
    assert_eq!(text(&results, "reused"), abc);
    assert_eq!(text(&results, "hasherKind"), "SHA256");
}

#[tokio::test]
async fn key_derivation_matches_published_vectors() {
    let outcome = evaluate(
        r#"
local Crypto = import("Crypto")
results = {}
results.hmacSha256 = Crypto.ToHex(Crypto.Hmac(enum.HashAlgorithm.SHA256, "Jefe", "what do ya want for nothing?"))
results.hmacMd5 = Crypto.ToHex(Crypto.Hmac(enum.HashAlgorithm.MD5, "Jefe", "what do ya want for nothing?"))
results.hkdf = Crypto.ToHex(Crypto.Hkdf(
    enum.HashAlgorithm.SHA256,
    string.rep("\x0b", 22),
    Crypto.FromHex("000102030405060708090a0b0c"),
    Crypto.FromHex("f0f1f2f3f4f5f6f7f8f9"),
    42
))
results.pbkdf2 = Crypto.ToHex(Crypto.Pbkdf2(enum.HashAlgorithm.SHA256, "password", "salt", 1, 32))
results.pbkdf2Slow = Crypto.ToHex(Crypto.Pbkdf2(enum.HashAlgorithm.SHA256, "password", "salt", 4096, 32))
local first = Crypto.Hmac(enum.HashAlgorithm.SHA3_256, "password", "salt\0\0\0\1")
local second = Crypto.Hmac(enum.HashAlgorithm.SHA3_256, "password", first)
local combined = buffer.create(32)
for index = 0, 31 do
    buffer.writeu8(combined, index, bit32.bxor(string.byte(first, index + 1), string.byte(second, index + 1)))
end
results.genericMatches = Crypto.Pbkdf2(enum.HashAlgorithm.SHA3_256, "password", "salt", 2, 32) == buffer.tostring(combined)
results.equal = Crypto.Equals("same", buffer.fromstring("same"))
results.unequal = Crypto.Equals("same", "diff") or Crypto.Equals("same", "longer")
"#,
    )
    .await;
    let results: Table = outcome.global("results");
    assert_eq!(text(&results, "hmacSha256"), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    assert_eq!(text(&results, "hmacMd5"), "750c783e6ab0b503eaa86e310a5db738");
    assert_eq!(
        text(&results, "hkdf"),
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
    );
    assert_eq!(text(&results, "pbkdf2"), "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b");
    assert_eq!(text(&results, "pbkdf2Slow"), "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a");
    assert!(flag(&results, "genericMatches"));
    assert!(flag(&results, "equal"));
    assert!(!flag(&results, "unequal"));
}

#[tokio::test]
async fn ciphers_round_trip_and_reject_tampering() {
    let outcome = evaluate(
        r#"
local Crypto = import("Crypto")
results = {}
for _, name in { "AES128GCM", "AES256GCM", "ChaCha20Poly1305" } do
    local cipher = enum.CipherAlgorithm[name]
    local key = Crypto.GenerateKey(cipher)
    local sealed = Crypto.Encrypt(cipher, key, "secret message", "header")
    local opened = Crypto.Decrypt(cipher, key, sealed, "header")
    local tampered = string.sub(sealed, 1, 20) .. string.char((string.byte(sealed, 21) + 1) % 256) .. string.sub(sealed, 22)
    results[name] = {
        keyLength = #key,
        sealedLength = #sealed,
        opened = opened,
        tampered = Crypto.Decrypt(cipher, key, tampered, "header") == nil,
        wrongKey = Crypto.Decrypt(cipher, Crypto.GenerateKey(cipher), sealed, "header") == nil,
        wrongHeader = Crypto.Decrypt(cipher, key, sealed, "other") == nil,
        short = Crypto.Decrypt(cipher, key, "tiny") == nil,
        fresh = Crypto.Encrypt(cipher, key, "secret message", "header") ~= sealed,
    }
end
local ok, message = pcall(Crypto.Encrypt, enum.CipherAlgorithm.AES256GCM, "short key", "data")
results.badKey = not ok and string.find(tostring(message), "must be 32 bytes long", 1, true) ~= nil
"#,
    )
    .await;
    let results: Table = outcome.global("results");
    for (name, key) in [("AES128GCM", 16), ("AES256GCM", 32), ("ChaCha20Poly1305", 32)] {
        let entry: Table = results.get(name).unwrap();
        assert_eq!(entry.get::<i64>("keyLength").unwrap(), key, "{name}");
        assert_eq!(entry.get::<i64>("sealedLength").unwrap(), 12 + 14 + 16, "{name}");
        assert_eq!(entry.get::<String>("opened").unwrap(), "secret message", "{name}");
        for check in ["tampered", "wrongKey", "wrongHeader", "short", "fresh"] {
            assert!(entry.get::<bool>(check).unwrap(), "{name} {check}");
        }
    }
    assert!(flag(&results, "badKey"));
}

#[tokio::test]
async fn signatures_and_key_exchange_match_published_vectors() {
    let outcome = evaluate(
        r#"
local Crypto = import("Crypto")
local hex = Crypto.FromHex
results = {}
local seed = hex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
results.edPublic = Crypto.ToHex(Crypto.GetPublicKey(enum.KeyAlgorithm.Ed25519, seed))
results.edSignature = Crypto.ToHex(Crypto.Sign(enum.KeyAlgorithm.Ed25519, seed, ""))
results.edVerifies = Crypto.Verify(enum.KeyAlgorithm.Ed25519, Crypto.GetPublicKey(enum.KeyAlgorithm.Ed25519, seed), "", Crypto.Sign(enum.KeyAlgorithm.Ed25519, seed, ""))

for _, name in { "Ed25519", "EcdsaP256", "EcdsaP384" } do
    local algorithm = enum.KeyAlgorithm[name]
    local pair = Crypto.GenerateKeyPair(algorithm)
    local signature = Crypto.Sign(algorithm, pair.PrivateKey, "payload")
    results[name] = {
        verifies = Crypto.Verify(algorithm, pair.PublicKey, "payload", signature),
        rejects = not Crypto.Verify(algorithm, pair.PublicKey, "payload!", signature),
        publicMatches = Crypto.GetPublicKey(algorithm, pair.PrivateKey) == pair.PublicKey,
        privateLength = #pair.PrivateKey,
        publicLength = #pair.PublicKey,
        signatureLength = #signature,
    }
end

local alice = hex("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")
local bob = hex("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb")
results.alicePublic = Crypto.ToHex(Crypto.GetPublicKey(enum.KeyAlgorithm.X25519, alice))
results.shared = Crypto.ToHex(Crypto.SharedSecret(enum.KeyAlgorithm.X25519, alice, Crypto.GetPublicKey(enum.KeyAlgorithm.X25519, bob)))
local left, right = Crypto.GenerateKeyPair(enum.KeyAlgorithm.X25519), Crypto.GenerateKeyPair(enum.KeyAlgorithm.X25519)
results.agree = Crypto.SharedSecret(enum.KeyAlgorithm.X25519, left.PrivateKey, right.PublicKey)
    == Crypto.SharedSecret(enum.KeyAlgorithm.X25519, right.PrivateKey, left.PublicKey)
local ok, message = pcall(Crypto.Sign, enum.KeyAlgorithm.X25519, left.PrivateKey, "x")
results.exchangeCannotSign = not ok and string.find(tostring(message), "SharedSecret", 1, true) ~= nil
"#,
    )
    .await;
    let results: Table = outcome.global("results");
    assert_eq!(text(&results, "edPublic"), "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    assert_eq!(
        text(&results, "edSignature"),
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    );
    assert!(flag(&results, "edVerifies"));
    for (name, private, public, signature) in [
        ("Ed25519", 32, 32, 64),
        ("EcdsaP256", 32, 65, 64),
        ("EcdsaP384", 48, 97, 96),
    ] {
        let entry: Table = results.get(name).unwrap();
        for check in ["verifies", "rejects", "publicMatches"] {
            assert!(entry.get::<bool>(check).unwrap(), "{name} {check}");
        }
        assert_eq!(entry.get::<i64>("privateLength").unwrap(), private, "{name}");
        assert_eq!(entry.get::<i64>("publicLength").unwrap(), public, "{name}");
        assert_eq!(entry.get::<i64>("signatureLength").unwrap(), signature, "{name}");
    }
    assert_eq!(text(&results, "alicePublic"), "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
    assert_eq!(text(&results, "shared"), "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
    assert!(flag(&results, "agree"));
    assert!(flag(&results, "exchangeCannotSign"));
}

#[tokio::test]
async fn passwords_encodings_and_random_values() {
    let outcome = evaluate(
        r#"
local Crypto = import("Crypto")
results = {}
local hash = Crypto.HashPassword("hunter2")
results.phc = string.sub(hash, 1, 10) == "$argon2id$"
results.accepts = Crypto.VerifyPassword("hunter2", hash)
results.rejects = not Crypto.VerifyPassword("hunter3", hash)
results.garbage = not Crypto.VerifyPassword("hunter2", "not a hash")
results.salted = Crypto.HashPassword("hunter2") ~= hash
results.hex = Crypto.ToHex("\0\255\16")
results.unhex = Crypto.FromHex("00FF10") == "\0\255\16"
results.base64 = Crypto.ToBase64("hello?>")
results.base64Url = Crypto.ToBase64("hello?>", true)
results.fromStandard = Crypto.FromBase64("aGVsbG8/Pg==")
results.fromUrl = Crypto.FromBase64("aGVsbG8_Pg")
local badHex = pcall(Crypto.FromHex, "abc")
results.badHex = not badHex
local first, second = Crypto.RandomBytes(32), Crypto.RandomBytes(32)
results.randomLength = #first
results.randomDiffers = first ~= second
results.uuid = Crypto.RandomUUID()
"#,
    )
    .await;
    let results: Table = outcome.global("results");
    for check in ["phc", "accepts", "rejects", "garbage", "salted", "unhex", "badHex", "randomDiffers"] {
        assert!(flag(&results, check), "{check}");
    }
    assert_eq!(text(&results, "hex"), "00ff10");
    assert_eq!(text(&results, "base64"), "aGVsbG8/Pg==");
    assert_eq!(text(&results, "base64Url"), "aGVsbG8_Pg");
    assert_eq!(text(&results, "fromStandard"), "hello?>");
    assert_eq!(text(&results, "fromUrl"), "hello?>");
    assert_eq!(results.get::<i64>("randomLength").unwrap(), 32);
    let uuid = text(&results, "uuid");
    assert_eq!(uuid.len(), 36);
    assert_eq!(&uuid[14..15], "4");
}

#[tokio::test]
async fn heavy_work_only_blocks_the_calling_coroutine() {
    let dir = main_script(
        r#"
local Crypto = import("Crypto")
ticks = 0
local running = true
coroutine.wrap(function()
    while running do
        ticks += 1
        sleep(1)
    end
end)()
local big = buffer.create(48 * 1024 * 1024)
local before = ticks
digest = Crypto.ToHex(Crypto.Hash(enum.HashAlgorithm.SHA256, big))
duringHash = ticks - before
before = ticks
Crypto.Pbkdf2(enum.HashAlgorithm.SHA256, "password", "salt", 300000, 32)
duringDerive = ticks - before
running = false
"#,
    );
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let expected = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, &vec![0u8; 48 * 1024 * 1024]);
    let expected: String = expected.as_ref().iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(outcome.global::<String>("digest"), expected);
    assert!(outcome.global::<i64>("duringHash") > 0, "the VM stalled while hashing");
    assert!(outcome.global::<i64>("duringDerive") > 0, "the VM stalled during PBKDF2");
}
