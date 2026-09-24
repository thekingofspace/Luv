# DLL

Loads native libraries, calls their functions and works with native memory.

```luau
local DLL = import("DLL")
```

## Description

`DLL` works with `.dll` files on Windows and `.so` files on Linux. You can load the libraries that luv builds from the `native` folder of your project. You can also load libraries that come with the system.

[DLL.Load](#load) gives you a [Library](library.md). From it you get [NativeFunction](nativefunction.md) objects that you call like Luau functions. Each call runs on another thread and yields only the calling coroutine. The rest of the game keeps running.

luv needs the C type of every value it passes. You write a type as a name like `"i32"` or `"string"`, or as a [StructType](structtype.md) or [ArrayType](arraytype.md). See [Type names](#type-names) and [Converting values](#converting-values).

Libraries made with the luv plugin API can also add classes and functions to Luau. You find them in [Library.Exports](library.md#exports). To write your own native code, see [Native plugins](../manual/native-plugins.md).

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [Load](#load)(path) | [Library](library.md) | yes |
| [Function](#function)(pointer, returnType, argumentTypes, options) | [NativeFunction](nativefunction.md) | no |
| [Callback](#callback)(returnType, argumentTypes, handler) | [Callback](callback.md) | no |
| [Struct](#struct)(fields) | [StructType](structtype.md) | no |
| [Array](#array)(element, length) | [ArrayType](arraytype.md) | no |
| [Alloc](#alloc)(size) | [Pointer](pointer.md) | no |
| [New](#new)(valueType, value) | [Pointer](pointer.md) | no |
| [String](#string)(text, wide) | [Pointer](pointer.md) | no |
| [Pointer](#pointer)(address) | [Pointer](pointer.md) | no |
| [SizeOf](#sizeof)(valueType) | `number` | no |
| [AlignOf](#alignof)(valueType) | `number` | no |
| [Copy](#copy)(destination, source, size) | nothing | no |
| [Fill](#fill)(destination, value, size) | nothing | no |

## Values

| Name | Type | Description |
| --- | --- | --- |
| [Null](#null) | [Pointer](pointer.md) | A pointer to address 0. |
| [Extension](#extension) | `string` | `".dll"` on Windows and `".so"` on Linux. |

## Function descriptions

### Load

```luau
DLL.Load(path: string): Library
```

Loads a native library and returns a [Library](library.md). This yields the calling coroutine while the library loads.

When the last part of the path has no extension, luv tries these file names in order:

1. The name as written.
2. The name with `.dll` on Windows or `.so` on Linux.
3. On Linux only, the name with `lib` in front and `.so` at the end. luv skips this when the name already starts with `lib`.

So leave the extension out. luv builds `native/particles.c` into `particles.dll` on Windows and `libparticles.so` on Linux. `DLL.Load("./particles")` finds both.

An absolute path is used as it is. A relative path is looked up in a few folders, in this order.

With `luv test`:

1. The folder of the `luv` program.
2. The build folder of the project.
3. The project folder.

With `luv run`:

1. The folder of the `luv` program.
2. The folder of the `.luvit` file.

In a packed game:

1. The folder of the game program.

A bare name like `"kernel32"` or `"libc.so.6"` has no `/` or `\` and does not start with a dot. luv looks for it in the same folders first. When it is not there, luv lets the system find it the normal way.

On Windows, luv looks for the DLLs that a library needs in the folder of that library first.

When the library exports a `luv_register` function, luv runs it right after loading. The classes and functions it adds show up in [Library.Exports](library.md#exports). See [Plugin API for C](native-c.md#luv-register).

A library can also add [services](library.md#services). Each one becomes a name for [import](globals.md#import) once `Load` finishes, so load the library before you import them.

It errors when:

- The path is empty. The message is `DLL.Load needs the path of a library`.
- No file was found. The message is `cannot find './name', looked for ...` and lists every path luv tried.
- The system could not load the file. The message starts with `cannot load`.
- `luv_register` failed. The message starts with `luv_register in <path> failed`.
- A service name is already an import. The message starts with `'Net' cannot be a service because`.

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
print(mathlib.Path)

local ok, message = pcall(DLL.Load, "./missing")
print(ok, message)
```

### Function

```luau
DLL.Function(pointer: NativeHandle, returnType: DLLType, argumentTypes: { DLLType }?, options: NativeFunctionOptions?): NativeFunction
```

Makes a [NativeFunction](nativefunction.md) for the code at `pointer`. `pointer` can be a [Pointer](pointer.md), a [NativeFunction](nativefunction.md), a [Callback](callback.md) or a plugin object. The new function is named `function at 0x` followed by the address in hex.

[Library:GetFunction](library.md#getfunction) is easier for named functions. Use `Function` for function pointers you get from C, for example from a struct field or a return value.

A function made from a library symbol runs on the thread of that library. Other functions run on one shared thread. See [Which thread runs a call](nativefunction.md#which-thread-runs-a-call). `options` is a [NativeFunctionOptions](nativefunction.md#nativefunctionoptions) table.

It errors with `cannot make a NativeFunction from a null Pointer` when the address is 0.

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local add = DLL.Function(mathlib:GetSymbol("add"), "int", { "int", "int" })
print(add(40, 2))
```

### Callback

```luau
DLL.Callback(returnType: DLLType, argumentTypes: { DLLType }?, handler: (...any) -> any): Callback
```

Wraps a Luau function so that C code can call it. Pass the [Callback](callback.md), or its `Pointer`, to a C function that takes a function pointer.

The argument types come second. Pass `nil` or `{}` when the C side gives no arguments. The return type cannot be an ArrayType, and the argument types cannot be `"void"` or an ArrayType.

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local apply = mathlib:GetFunction("apply", "i32", { "pointer", "i32" })
local double = DLL.Callback("i32", { "i32" }, function(value: number)
	return value * 2
end)
print(apply(double, 20))
```

### Struct

```luau
DLL.Struct(fields: { StructField }): StructType
```

Makes a [StructType](structtype.md) from a list of fields. Each field is `{ "name", type }` or `{ Name = "name", Type = type }`. The layout follows the C rules. See [StructType](structtype.md) for the details and errors.

```luau
local DLL = import("DLL")

local Vec2 = DLL.Struct({ { "x", "f32" }, { "y", "f32" } })
print(Vec2.Size, Vec2:Offset("y"))
```

### Array

```luau
DLL.Array(element: DLLType, length: number): ArrayType
```

Makes an [ArrayType](arraytype.md) with `length` values of type `element`. Arrays can be struct fields and can live in memory. They cannot be arguments or return values. Pass a pointer to them instead.

```luau
local DLL = import("DLL")

local Numbers = DLL.Array("i32", 5)
local numbers = Numbers:New({ 1, 2, 3, 4, 5 })
print(Numbers.Size, numbers:Read("i32", 8))
```

### Alloc

```luau
DLL.Alloc(size: number): Pointer
```

Allocates `size` bytes filled with zeros and returns an owned [Pointer](pointer.md). The memory is aligned to 16 bytes. `size` can be 0. luv frees the memory when nothing uses it anymore. See [Ownership](pointer.md#ownership).

It errors with `the size must be a whole number of at least 0, got -1` for a bad size.

```luau
local DLL = import("DLL")

local memory = DLL.Alloc(16)
memory:Write("u16", 513, 2)
print(memory:Read("u16", 2), memory.Size)
```

### New

```luau
DLL.New(valueType: DLLType, value: any?): Pointer
```

Allocates memory for one value of `valueType`, fills it with zeros and then writes `value` into it. Returns an owned [Pointer](pointer.md). The value follows the rules in [Luau to C](#luau-to-c).

`New` cannot copy a Luau string or buffer into a `string` or `pointer` value. Make the text with [DLL.String](#string) and store that Pointer instead. `"void"` errors with `void has no values, it can only be a return type`.

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local setOut = mathlib:GetFunction("set_out", "void", { "pointer", "i32" })
local out = DLL.New("i32")
setOut(out, 77)
print(out:Read("i32"))
```

### String

```luau
DLL.String(text: string, wide: boolean?): Pointer
```

Copies `text` into owned memory with a zero byte at the end. With `wide` set to `true`, the text is stored as `wchar_t` characters with a wide zero at the end. The `Size` of the Pointer includes the end mark.

Use it when C keeps the text after a call returns. For text that C only reads during a call, pass the Luau string itself. Keep the Pointer in a variable for as long as C uses it.

```luau
local DLL = import("DLL")

local name = DLL.String("Player One")
local wide = DLL.String("wide ✓", true)
print(name:ReadString(), wide:ReadWideString(), name.Size)
```

### Pointer

```luau
DLL.Pointer(address: number): Pointer
```

Makes a foreign [Pointer](pointer.md) to `address`. luv does not check reads and writes through it. The address must be a whole number of at least 0.

```luau
local DLL = import("DLL")

print(DLL.Pointer(1234) == DLL.Pointer(1234))
print(tostring(DLL.Pointer(4096)))
```

### SizeOf

```luau
DLL.SizeOf(valueType: DLLType): number
```

Returns the size of a type in bytes. `"void"` errors.

```luau
local DLL = import("DLL")

local Record = DLL.Struct({ { "id", "i32" }, { "weight", "f64" } })
print(DLL.SizeOf("i32"), DLL.SizeOf(Record))
```

### AlignOf

```luau
DLL.AlignOf(valueType: DLLType): number
```

Returns the alignment of a type in bytes. `"void"` errors.

```luau
local DLL = import("DLL")

print(DLL.AlignOf("double"), DLL.AlignOf(DLL.Array("u16", 4)))
```

### Copy

```luau
DLL.Copy(destination: Pointer, source: Pointer, size: number)
```

Copies `size` bytes from `source` to `destination`. The two areas can overlap. Owned memory and plugin objects are bounds checked. Both sides can also be a NativeFunction, a Callback or a plugin object.

```luau
local DLL = import("DLL")

local from = DLL.String("hello")
local to = DLL.Alloc(8)
DLL.Copy(to, from, 6)
print(to:ReadString())
```

### Fill

```luau
DLL.Fill(destination: Pointer, value: number, size: number)
```

Sets `size` bytes to `value`. `value` must be a whole number from 0 to 255. Otherwise it errors with `the fill value must be a byte from 0 to 255, got 300`.

```luau
local DLL = import("DLL")

local memory = DLL.Alloc(4)
DLL.Fill(memory, 7, 4)
print(memory:Read("u8", 3))
```

## Value descriptions

### Null

```luau
DLL.Null: Pointer
```

A foreign [Pointer](pointer.md) to address 0. Pass it where C expects NULL. `nil` works the same way.

Reading or writing through it errors with `cannot access memory through a null Pointer`. A C function that returns NULL gives `nil`, not `DLL.Null`.

### Extension

```luau
DLL.Extension: string
```

`".dll"` on Windows and `".so"` on Linux. [DLL.Load](#load) adds the extension for you, so you rarely need this.

## Type names

Every value that goes to C needs a type. The type names are strings. Many C names work as other names for the same type. luv is a 64 bit program, so pointers are 8 bytes. Each of these types is aligned to its size.

| Name | C type | Size in bytes |
| --- | --- | --- |
| `"void"` | `void` | none. Only as a return type. |
| `"bool"` | `bool` | 1 |
| `"i8"` | `int8_t` | 1 |
| `"u8"`, `"uchar"` | `uint8_t` | 1 |
| `"char"` | `char` | 1. It is signed like `"i8"`, except on Linux on ARM where it is unsigned like `"u8"`. |
| `"i16"`, `"short"` | `int16_t` | 2 |
| `"u16"`, `"ushort"` | `uint16_t` | 2 |
| `"i32"`, `"int"` | `int32_t` | 4 |
| `"u32"`, `"uint"` | `uint32_t` | 4 |
| `"i64"`, `"longlong"` | `int64_t` | 8 |
| `"u64"`, `"ulonglong"` | `uint64_t` | 8 |
| `"long"` | `long` | 4 on Windows, 8 on Linux |
| `"ulong"` | `unsigned long` | 4 on Windows, 8 on Linux |
| `"isize"`, `"ssize_t"`, `"intptr_t"` | `intptr_t` | 8 |
| `"usize"`, `"size_t"`, `"uintptr_t"` | `size_t` | 8 |
| `"f32"`, `"float"` | `float` | 4 |
| `"f64"`, `"double"` | `double` | 8 |
| `"pointer"` | `void*` | 8 |
| `"string"` | `const char*` | 8 |
| `"wstring"` | `const wchar_t*` | 8 |

`wchar_t` is 2 bytes of UTF-16 on Windows and 4 bytes on Linux.

A type in a signature is a `DLLType`. That is one of these names, a [StructType](structtype.md) or an [ArrayType](arraytype.md). `DLLTypeName` means one of the names only.

Error messages use the first name of each row. So `"int"` shows up as `i32`. An unknown name errors with `'int32' is not a DLL type, the types are ...` and lists every name.

## NativeHandle

A `NativeHandle` is a value that points at native code. It is a [Pointer](pointer.md), a [NativeFunction](nativefunction.md) or a [Callback](callback.md). [DLL.Function](#function) takes one. Plugin objects work there too.

## Converting values

### Luau to C

These rules apply to call arguments, [Pointer:Write](pointer.md#write), [DLL.New](#new), `New` on a StructType or ArrayType, and the result of a [Callback](callback.md).

| Type | Luau values | Notes |
| --- | --- | --- |
| `bool` | `boolean`, `nil` | `nil` is `false`. |
| Whole number types | `number`, `boolean` | The number must be whole and fit the type. `true` is 1 and `false` is 0. |
| `f32`, `f64` | `number` | |
| `pointer` | [Pointer](pointer.md), [NativeFunction](nativefunction.md), [Callback](callback.md), plugin object, `buffer`, `string`, `nil` | `nil` is NULL. A plugin object gives the address of its data. |
| `string` | `string`, `nil`, Pointer, `buffer` | A string is copied with a zero byte at the end. |
| `wstring` | `string`, `nil`, Pointer, `buffer` | A string is converted to `wchar_t` characters with a zero at the end. |
| [StructType](structtype.md) | table, [UDim](udim.md), [Color](color.md), Pointer | See [Values](structtype.md#values). |
| [ArrayType](arraytype.md) | table, `string`, `buffer`, Pointer | See [Values](arraytype.md#values). |

Strings and buffers turn into temporary copies:

- A copy lives until the call returns. C must not keep its address.
- After the call, luv copies a buffer back. So C can fill a buffer.
- A string gets a zero byte at the end. A buffer does not.
- Temporary copies only work in calls, including string fields of a struct argument. Everywhere else they error with `a string can only be passed straight into a function call, use DLL.String or DLL.Alloc for memory that has to outlive the call`.

A wrong value errors with a message like `i32 expects a whole number, got 1.5`, `300 does not fit in a u8` or `pointer expects a Pointer, buffer, string or nil, got number`.

### C to Luau

These rules apply to return values, [Pointer:Read](pointer.md#read) and the arguments a [Callback](callback.md) gets.

| Type | Luau value |
| --- | --- |
| `void` | `nil` |
| `bool` | `boolean` |
| Number types | `number` |
| `pointer` | A foreign [Pointer](pointer.md), or `nil` for NULL. |
| `string` | A copy of the text, or `nil` for NULL. |
| `wstring` | The text as UTF-8, or `nil` for NULL. |
| [StructType](structtype.md) | A table with one key for each field. |
| [ArrayType](arraytype.md) | An array table that starts at 1. |

luv never frees memory that C returns. If the library wants you to free it, call its own free function.

### 64 bit numbers

Luau numbers are doubles. `i64`, `u64`, `isize` and `usize` values above 2^53 lose precision on the way in and on the way out. To pass an exact 64 bit value, write its 8 bytes with [Pointer:WriteBuffer](pointer.md#writebuffer) and pass a pointer to them.
