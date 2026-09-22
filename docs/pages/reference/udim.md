# UDim

Three numbers named `X`, `Y` and `Z`. luv uses UDim for sizes, positions and directions.

```luau
local size = udim.new(640, 360)
```

## Description

Make a UDim with [udim.new](#new). `X` and `Y` are the first two numbers. `Z` is a third number that defaults to 0.

A UDim never changes. Its fields are read only, and setting one raises an error. Math on a UDim gives you a new UDim.

`==` compares the numbers. Two UDims with the same numbers are still different keys in a table.

You can send a UDim with [Messenger](messenger.md) and use it inside a parallel block. The other thread gets a copy. [Serde](serde.md) encodes a UDim as a table with `X`, `Y` and `Z` keys.

`typeof` returns `"UDim"`.

## Constructors

### new

```luau
udim.new(x: number?, y: number?, z: number?): UDim
```

Makes a UDim. Each missing number is 0.

### zero

```luau
udim.zero: UDim
```

A UDim with all three numbers at 0. It is equal to `udim.new()`.

```luau
local position = udim.new(100, 50)
print(position, udim.new(), udim.zero == udim.new(0, 0, 0))
```

This prints:

```text
UDim(100, 50, 0)	UDim(0, 0, 0)	true
```

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `X` | `number` | The first number. Read only. |
| `Y` | `number` | The second number. Read only. |
| `Z` | `number` | The third number. Read only. |

## Methods

### Lerp

```luau
value:Lerp(goal: UDim, alpha: number): UDim
```

Returns a UDim between this one and `goal`. An `alpha` of 0 gives this UDim. An `alpha` of 1 gives `goal`. `alpha` is not clamped, so other values go past either end.

```luau
local from = udim.new(0, 0)
local to = udim.new(200, 100)
print(from:Lerp(to, 0.25))
```

This prints `UDim(50, 25, 0)`.

## Operators

In this table `a` and `b` are UDims and `n` is a number.

| Operation | Result |
| --- | --- |
| `a + b` | Adds the matching numbers. |
| `a - b` | Subtracts the matching numbers. |
| `a * b` | Multiplies the matching numbers. |
| `a * n` or `n * a` | Multiplies every number by `n`. |
| `a / b` | Divides the matching numbers. |
| `a / n` | Divides every number by `n`. |
| `n / a` | Divides `n` by every number. |
| `-a` | Flips the sign of every number. |
| `a == b` | `true` when all three numbers match. |

`+` and `-` need a UDim on both sides, so `a + 1` raises an error. UDim has no `<`, `%`, `//` or `^`.

```luau
local a = udim.new(10, 20, 1)
local b = udim.new(1, 2, 3)
print(a + b)
print(a * 2)
print(a / b)
```

This prints:

```text
UDim(11, 22, 4)
UDim(20, 40, 2)
UDim(10, 10, 0.3333333333333333)
```

## tostring

`tostring` gives `UDim(x, y, z)`. Whole numbers print without a decimal point. A negated 0 prints as `-0`.
