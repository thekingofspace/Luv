# Serde

Turns Luau values into JSON, JSONC, TOML or YAML text and back.

```luau
local Serde = import("Serde")
```

## Description

[Encode](#encode) turns a value into text. [Decode](#decode) turns text into a value. Both yield the calling coroutine while the work runs on another thread.

The `format` argument is one of the names in [Formats](#formats).

## Formats

| Format | Encode | Decode |
| --- | --- | --- |
| `"json"` | Compact JSON. With `pretty`, each level is indented by two spaces. | JSON. |
| `"jsonc"` | The same as `"json"`. | JSON that can have `//` and `/* */` comments and trailing commas. |
| `"toml"` | TOML. The value must be a table that is not a list. `{}` gives `""`. | TOML. Dates and times become strings like `"1979-05-27T07:32:00Z"`. |
| `"yaml"` | Compact JSON, which is also valid YAML. With `pretty`, block style YAML. | YAML. |

The names are not case sensitive, and `"yml"` works too. Another name errors with `unknown format 'xml', expected "json", "jsonc", "toml" or "yaml"`.

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [Encode](#encode)(format, value, pretty) | `string` | yes |
| [Decode](#decode)(format, text) | `any` | yes |

## Function descriptions

### Encode

```luau
Serde.Encode(format: SerdeFormat, value: any, pretty: boolean?): string
```

Turns `value` into text. `pretty` defaults to `false`. See [How values convert](#how-values-convert) for the rules.

```luau
local Serde = import("Serde")

local text = Serde.Encode("json", { name = "Luv", tags = { "a", "b" } })
print(text)
```

This prints `{"name":"Luv","tags":["a","b"]}`.

It errors when a value cannot be encoded:

| Message | Cause |
| --- | --- |
| `function values cannot be encoded` | The value holds a function. A buffer, a thread or an object other than UDim and Color gives the same message with its own type. |
| `strings must be valid UTF-8 to be encoded` | A string holds raw bytes. Turn them into text with [Crypto.ToBase64](crypto.md#tobase64) first. |
| `table keys must be valid UTF-8 to be encoded` | A string key holds raw bytes. |
| `table keys must be strings or numbers to be encoded, got boolean` | A table key has another type. |
| `tables that contain themselves cannot be encoded` | A table contains itself. |
| `tables nested deeper than 128 levels cannot be encoded` | The tables are nested 128 levels deep or more. |
| `cannot encode toml: toml documents must be tables` | TOML got a value that is not a table, or a list. |

### Decode

```luau
Serde.Decode(format: SerdeFormat, text: string): any
```

Turns text into a Luau value. The text must be valid UTF-8. It errors with `cannot decode <format>: <message>` when the text is not valid. The message says where the problem is.

```luau
local Serde = import("Serde")

local config = Serde.Decode("toml", "title = \"Game\"\n[window]\nwidth = 1280\n")
print(config.title, config.window.width)
```

## How values convert

When encoding:

| Luau value | Becomes |
| --- | --- |
| `nil` | `null` |
| `boolean` | A boolean. |
| a whole `number` | An integer. `2.0` becomes `2`. This works for numbers between `-9e15` and `9e15`. |
| any other `number` | A decimal number. In JSON, `0/0` and `math.huge` become `null`. |
| `string` | A string. It must be valid UTF-8. |
| `vector` | A list of three numbers. |
| [UDim](udim.md) | An object with the keys `X`, `Y` and `Z`. |
| [Color](color.md) | An object with the keys `R`, `G`, `B` and `A`. |
| a table with the keys 1 to n and no holes | A list. |
| an empty table | An empty list, `[]` in JSON. |
| any other table | An object. Number keys become strings. The keys are sorted. |

When decoding:

| Text | Becomes |
| --- | --- |
| `null` | `nil` |
| a boolean | `boolean` |
| a number | `number` |
| a string | `string` |
| a list | A table with the keys 1 to n. |
| an object | A table with string keys. |
| a TOML date or time | `string` |

Things to know:

- `null` inside an object removes that key. `null` inside a list leaves a hole.
- Decoded tables are plain tables. A UDim comes back as a table with `X`, `Y` and `Z`. Build it again with `udim.new`. A Color comes back with `R`, `G`, `B` and `A`. Build it again with `color.new`.
- An empty object `{}` decodes to an empty table. Encoding that table gives `[]`.
- A table with holes, like `{ [1] = "a", [3] = "c" }`, becomes the object `{"1":"a","3":"c"}`.
- A table with both list items and named keys becomes an object. `{ "first", name = "second" }` becomes `{"1":"first","name":"second"}`.
- YAML keys that are not strings become strings, like `"true"` or `"1"`.
