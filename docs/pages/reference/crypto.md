# Crypto

Hashes, keys, encryption, signatures and secure random bytes.

```luau
local Crypto = import("Crypto")
```

## Description

Most results are raw bytes in a Luau string. Turn them into text with [ToHex](#tohex) or [ToBase64](#tobase64) before you print them or save them as text.

Inputs of the type `Bytes` take a string or a buffer. An optional input that is `nil` counts as empty. Other types are errors, like `the key must be a string or buffer, got number`.

You pick algorithms with enum items: [HashAlgorithm](enums.md#hashalgorithm), [CipherAlgorithm](enums.md#cipheralgorithm) and [KeyAlgorithm](enums.md#keyalgorithm). Strings do not work. An item of the wrong enum is an error, like `expected an enum.HashAlgorithm item, got enum.KeyAlgorithm.Ed25519`.

Functions with "yes" in the Yields column can yield. They run inputs larger than 64 KiB on another thread and yield the calling coroutine while that runs. Small inputs run right away. `Pbkdf2`, `HashPassword` and `VerifyPassword` always run on another thread.

## Sizes

All sizes are in bytes.

| HashAlgorithm | Output |
| --- | --- |
| `MD5` | 16 |
| `SHA1` | 20 |
| `SHA224` | 28 |
| `SHA256` | 32 |
| `SHA384` | 48 |
| `SHA512` | 64 |
| `SHA3_256` | 32 |
| `SHA3_384` | 48 |
| `SHA3_512` | 64 |
| `BLAKE3` | 32 |

| CipherAlgorithm | Key | Added by Encrypt |
| --- | --- | --- |
| `AES128GCM` | 16 | 28 |
| `AES256GCM` | 32 | 28 |
| `ChaCha20Poly1305` | 32 | 28 |

| KeyAlgorithm | Private key | Public key | Signature | Used with |
| --- | --- | --- | --- | --- |
| `Ed25519` | 32 | 32 | 64 | [Sign](#sign) and [Verify](#verify) |
| `EcdsaP256` | 32 | 65 | 64 | [Sign](#sign) and [Verify](#verify) |
| `EcdsaP384` | 48 | 97 | 96 | [Sign](#sign) and [Verify](#verify) |
| `X25519` | 32 | 32 | none | [SharedSecret](#sharedsecret) |

Key formats:

- An Ed25519 private key is the 32 byte seed.
- An ECDSA private key is the secret number in big endian order.
- An ECDSA public key uses the uncompressed form. It starts with the byte `0x04`.
- An ECDSA signature is `r` and then `s`, each with a fixed size.

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [Hash](#hash)(algorithm, data) | `string` | yes |
| [Hasher](#hasher)(algorithm) | [Hasher](hasher.md) | no |
| [Hmac](#hmac)(algorithm, key, data) | `string` | yes |
| [Hkdf](#hkdf)(algorithm, secret, salt, info, length) | `string` | yes |
| [Pbkdf2](#pbkdf2)(algorithm, password, salt, iterations, length) | `string` | yes |
| [HashPassword](#hashpassword)(password) | `string` | yes |
| [VerifyPassword](#verifypassword)(password, hash) | `boolean` | yes |
| [RandomBytes](#randombytes)(count) | `string` | yes |
| [RandomUUID](#randomuuid)() | `string` | no |
| [GenerateKey](#generatekey)(algorithm) | `string` | no |
| [Encrypt](#encrypt)(algorithm, key, data, associatedData) | `string` | yes |
| [Decrypt](#decrypt)(algorithm, key, data, associatedData) | `string?` | yes |
| [GenerateKeyPair](#generatekeypair)(algorithm) | [KeyPair](#keypair) | no |
| [GetPublicKey](#getpublickey)(algorithm, privateKey) | `string` | no |
| [Sign](#sign)(algorithm, privateKey, message) | `string` | yes |
| [Verify](#verify)(algorithm, publicKey, message, signature) | `boolean` | yes |
| [SharedSecret](#sharedsecret)(algorithm, privateKey, peerPublicKey) | `string` | no |
| [Equals](#equals)(first, second) | `boolean` | no |
| [ToHex](#tohex)(data) | `string` | no |
| [FromHex](#fromhex)(text) | `string` | no |
| [ToBase64](#tobase64)(data, urlSafe) | `string` | no |
| [FromBase64](#frombase64)(text) | `string` | no |

## Function descriptions

### Hash

```luau
Crypto.Hash(algorithm: HashAlgorithmEnum, data: Bytes): string
```

Returns the hash of `data` as raw bytes. The size depends on the algorithm. See [Sizes](#sizes).

```luau
local Crypto = import("Crypto")

local digest = Crypto.Hash(enum.HashAlgorithm.SHA256, "abc")
print(Crypto.ToHex(digest))
```

This prints `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`.

### Hasher

```luau
Crypto.Hasher(algorithm: HashAlgorithmEnum): Hasher
```

Returns a new [Hasher](hasher.md). It hashes data that comes in parts. It does not yield.

### Hmac

```luau
Crypto.Hmac(algorithm: HashAlgorithmEnum, key: Bytes, data: Bytes): string
```

Returns the HMAC of `data` with `key`. It has the size of the hash. Every HashAlgorithm works.

```luau
local Crypto = import("Crypto")

local tag = Crypto.Hmac(enum.HashAlgorithm.SHA256, "server key", "score=1200")
print(Crypto.ToHex(tag))
```

### Hkdf

```luau
Crypto.Hkdf(algorithm: HashAlgorithmEnum, secret: Bytes, salt: Bytes?, info: Bytes?, length: number): string
```

Makes `length` bytes of key from `secret` with HKDF, as in RFC 5869. `salt` and `info` can be `nil`. `length` is a whole number from 1 to 255 times the hash size. For SHA256 that is 8160. Other lengths are an error, like `the length must be a whole number between 1 and 8160, got 0`.

### Pbkdf2

```luau
Crypto.Pbkdf2(algorithm: HashAlgorithmEnum, password: Bytes, salt: Bytes, iterations: number, length: number): string
```

Makes `length` bytes of key from a password with PBKDF2. It always runs on another thread.

- `iterations` is a whole number from 1 to 4294967295.
- `length` is a whole number from 1 to 1048576.

Other values are an error, like `the iteration count must be a whole number between 1 and 4294967295, got 0`.

```luau
local Crypto = import("Crypto")

local salt = Crypto.RandomBytes(16)
local key = Crypto.Pbkdf2(enum.HashAlgorithm.SHA256, "correct horse", salt, 100000, 32)
print(#key)
```

### HashPassword

```luau
Crypto.HashPassword(password: Bytes): string
```

Hashes a password with Argon2id and a new random salt. The result is text that starts with `$argon2id$`. It holds the salt and the settings, so you only need to store this text. The same password gives a different result each time. It always runs on another thread.

### VerifyPassword

```luau
Crypto.VerifyPassword(password: Bytes, hash: string): boolean
```

Returns `true` when `password` matches a result of [HashPassword](#hashpassword). It returns `false` for a wrong password and for a `hash` that is not valid. It always runs on another thread.

```luau
local Crypto = import("Crypto")

local stored = Crypto.HashPassword("hunter2")
print(Crypto.VerifyPassword("hunter2", stored))
print(Crypto.VerifyPassword("hunter3", stored))
```

### RandomBytes

```luau
Crypto.RandomBytes(count: number): string
```

Returns `count` random bytes from the secure random source of the system. `count` is a whole number from 1 to 67108864, which is 64 MiB. Other values are an error, like `the byte count must be a whole number between 1 and 67108864, got 0`.

### RandomUUID

```luau
Crypto.RandomUUID(): string
```

Returns a random version 4 UUID in lowercase, like `"3f2a9c1e-5b7d-4e2a-9c1f-0a6b8d4e2f71"`. It uses the secure random source. It does not yield.

### GenerateKey

```luau
Crypto.GenerateKey(algorithm: CipherAlgorithmEnum): string
```

Returns a new random key with the right size for [Encrypt](#encrypt) and [Decrypt](#decrypt). See [Sizes](#sizes). It does not yield.

### Encrypt

```luau
Crypto.Encrypt(algorithm: CipherAlgorithmEnum, key: Bytes, data: Bytes, associatedData: Bytes?): string
```

Encrypts `data` with `key`. The result is a random 12 byte nonce, then the encrypted data, then a 16 byte tag. So it is 28 bytes longer than `data`. Each call uses a new nonce, so the same input gives a new result each time.

`associatedData` is not stored in the result. Pass the same value to [Decrypt](#decrypt).

A key with the wrong size is an error, like `a AES256GCM key must be 32 bytes long, got 9 bytes`.

```luau
local Crypto = import("Crypto")

local cipher = enum.CipherAlgorithm.AES256GCM
local key = Crypto.GenerateKey(cipher)
local sealed = Crypto.Encrypt(cipher, key, "secret save data", "slot-1")
print(Crypto.Decrypt(cipher, key, sealed, "slot-1"))
```

### Decrypt

```luau
Crypto.Decrypt(algorithm: CipherAlgorithmEnum, key: Bytes, data: Bytes, associatedData: Bytes?): string?
```

Decrypts a result of [Encrypt](#encrypt) and returns the original data. It returns `nil` when the data was changed, the key is wrong, `associatedData` does not match or the data is too short. It only errors when the key has the wrong size.

### GenerateKeyPair

```luau
Crypto.GenerateKeyPair(algorithm: KeyAlgorithmEnum): KeyPair
```

Returns a new [KeyPair](#keypair). Ed25519, EcdsaP256 and EcdsaP384 keys are for [Sign](#sign) and [Verify](#verify). X25519 keys are for [SharedSecret](#sharedsecret). It does not yield.

### GetPublicKey

```luau
Crypto.GetPublicKey(algorithm: KeyAlgorithmEnum, privateKey: Bytes): string
```

Returns the public key of a private key. A private key that is not valid is an error, like `the private key is not a valid Ed25519 key`. It does not yield.

### Sign

```luau
Crypto.Sign(algorithm: KeyAlgorithmEnum, privateKey: Bytes, message: Bytes): string
```

Signs `message` and returns the signature. Ed25519 gives the same signature each time. EcdsaP256 and EcdsaP384 give a new signature each time, and each one is valid. EcdsaP256 hashes the message with SHA256 and EcdsaP384 with SHA384.

X25519 keys are an error: `X25519 keys are for SharedSecret, sign with Ed25519, EcdsaP256 or EcdsaP384`. A private key that is not valid is an error too.

```luau
local Crypto = import("Crypto")

local algorithm = enum.KeyAlgorithm.Ed25519
local pair = Crypto.GenerateKeyPair(algorithm)
local signature = Crypto.Sign(algorithm, pair.PrivateKey, "level data")
print(Crypto.Verify(algorithm, pair.PublicKey, "level data", signature))
```

### Verify

```luau
Crypto.Verify(algorithm: KeyAlgorithmEnum, publicKey: Bytes, message: Bytes, signature: Bytes): boolean
```

Returns `true` when `signature` is a valid signature of `message` for `publicKey`. It returns `false` for a wrong signature and for a key or signature that is not valid. X25519 keys are an error.

### SharedSecret

```luau
Crypto.SharedSecret(algorithm: KeyAlgorithmEnum, privateKey: Bytes, peerPublicKey: Bytes): string
```

Returns a 32 byte secret from your X25519 private key and the public key of the other side. The other side gets the same bytes from its private key and your public key. Pass the result through [Hkdf](#hkdf) before you use it as a key. It does not yield.

Other algorithms are an error: `SharedSecret needs X25519 keys, Ed25519 and ECDSA keys are for signing`. A public key that is not valid is an error too: `the peer's public key is not a valid X25519 key`.

```luau
local Crypto = import("Crypto")

local x25519 = enum.KeyAlgorithm.X25519
local mine = Crypto.GenerateKeyPair(x25519)
local theirs = Crypto.GenerateKeyPair(x25519)
local secret = Crypto.SharedSecret(x25519, mine.PrivateKey, theirs.PublicKey)
local key = Crypto.Hkdf(enum.HashAlgorithm.SHA256, secret, nil, "session", 32)
print(#key)
```

### Equals

```luau
Crypto.Equals(first: Bytes, second: Bytes): boolean
```

Returns `true` when both values hold the same bytes. The comparison runs in constant time. Use it to compare hashes and tags. It does not yield.

### ToHex

```luau
Crypto.ToHex(data: Bytes): string
```

Returns the bytes as lowercase hex text. It does not yield.

### FromHex

```luau
Crypto.FromHex(text: string): string
```

Turns hex text into bytes. Upper and lower case both work. Spaces around the text and a `0x` in front are ignored. Text with an odd length or other characters is an error: `the text is not valid hex`. It does not yield.

### ToBase64

```luau
Crypto.ToBase64(data: Bytes, urlSafe: boolean?): string
```

Returns the bytes as Base64 text with `=` at the end as padding. With `urlSafe` set to `true`, it uses `-` and `_` in place of `+` and `/` and leaves out the padding. It does not yield.

### FromBase64

```luau
Crypto.FromBase64(text: string): string
```

Turns Base64 text into bytes. It reads the normal form and the URL safe form. Spaces and new lines are ignored and the padding is optional. Text that is not valid is an error: `the text is not valid base64: <reason>`. It does not yield.

```luau
local Crypto = import("Crypto")

local text = Crypto.ToBase64("hello?>")
print(text, Crypto.FromBase64(text))
print(Crypto.ToBase64("hello?>", true))
```

This prints `aGVsbG8/Pg==` and then `aGVsbG8_Pg`.

## KeyPair

The table that [GenerateKeyPair](#generatekeypair) returns.

| Name | Type | Description |
| --- | --- | --- |
| `PrivateKey` | `string` | The private key as raw bytes. Keep it secret. |
| `PublicKey` | `string` | The public key as raw bytes. |

See [Sizes](#sizes) for the length of each key.
