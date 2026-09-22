# Hasher

Hashes data that comes in parts.

## Description

You get a Hasher from [Crypto.Hasher](crypto.md#hasher). Call [Update](#update) for each part and [Finish](#finish) at the end. The result is the same as [Crypto.Hash](crypto.md#hash) of all parts joined together.

A Hasher is not a game object. It has no `ClassName`, `Name` or `Destroy`. `typeof` gives `"Hasher"`, and `tostring` gives text like `"Hasher(SHA256)"`.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `Algorithm` | [HashAlgorithm](enums.md#hashalgorithm) | The algorithm of this Hasher. Read only. |

## Methods

### Update

```luau
hasher:Update(data: Bytes)
```

Adds `data` to the hash. `data` is a string or a buffer. `nil` is an error. A part larger than 64 KiB is hashed on another thread, and the call yields the calling coroutine. While that runs, another `Update` or a `Finish` on the same Hasher errors with `the Hasher is still busy with another Update`.

### Finish

```luau
hasher:Finish(): string
```

Returns the hash as raw bytes and resets the Hasher. You can then use it again for new data. It does not yield.

```luau
local Crypto = import("Crypto")

local hasher = Crypto.Hasher(enum.HashAlgorithm.SHA256)
hasher:Update("a")
hasher:Update(buffer.fromstring("bc"))
print(Crypto.ToHex(hasher:Finish()))
print(hasher.Algorithm.Name)
```

The first line is the SHA256 hash of `"abc"`. The second line is `SHA256`.
