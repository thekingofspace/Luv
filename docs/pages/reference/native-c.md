# Plugin API for C

The C side of native plugins, from `native/luv.h`.

## Description

`luv init` writes `native/luv.h` into your project. Running it again after you update luv replaces the file with the new version, so do not edit it. For Rust, see [Plugin API for Rust](native-rust.md). For a guide with full examples, see [Native plugins](../manual/native-plugins.md).

A plugin is a normal library. Every function or variable that you mark with `LUV_EXPORT` can be used with [DLL](dll.md). The plugin API adds more on top: classes, functions and events that show up in Luau through [Library.Exports](library.md#exports).

`LUV_EXPORT` exports a symbol on Windows and on Linux. In C++ files, also mark exported symbols `extern "C"`. `LUV_EXPORT` does not do that for you.

```c
#include "luv.h"

LUV_EXPORT int32_t triple(int32_t value) {
    return value * 3;
}
```

## luv_register

```c
LUV_EXPORT int32_t luv_register(const LuvApi* api, LuvRegistry* registry);
```

luv calls `luv_register` each time [DLL.Load](dll.md#load) loads your library. It runs on the thread of the library. A library without it still loads, with an empty `Exports` table.

- Save `api` in a global variable. It stays valid until the game ends.
- `registry` is only valid while `luv_register` runs.
- Check `api->version` first. Return `LUV_INVALID` when it is lower than `LUV_API_VERSION`.
- Define your classes with `define_class` and your functions with `define_function`.
- Return `LUV_OK`.

Any other return value makes `DLL.Load` fail with `luv_register in <path> failed with code <value>`. A class or function that is not valid also makes it fail, even when you return `LUV_OK`. The message then lists the problems.

```c
#include "luv.h"

static const LuvApi* api;

static void hello(LuvCall* call) {
    api->push_string(call, "hello from C");
}

static const LuvMethod functions[] = {
    {"hello", hello, LUV_INLINE},
    {0},
};

LUV_EXPORT int32_t luv_register(const LuvApi* given, LuvRegistry* registry) {
    api = given;
    if (api->version < LUV_API_VERSION) {
        return LUV_INVALID;
    }
    for (const LuvMethod* function = functions; function->name; function++) {
        api->define_function(registry, function);
    }
    return LUV_OK;
}
```

## Constants

### Result codes

| Name | Value | Meaning |
| --- | --- | --- |
| `LUV_OK` | 0 | It worked. `luv_register` must return it. |
| `LUV_UNKNOWN_NAME` | -1 | `write_data` or `write_texture` found no shader data with that name. |
| `LUV_OUT_OF_RANGE` | -2 | `write_data` or `write_texture` got data that does not fit. |
| `LUV_WRONG_KIND` | -3 | `write_data` or `write_texture` got a name of the wrong kind of shader data. |
| `LUV_INVALID` | -4 | Bad input to `define_function`, `send_event`, `write_data` or `write_texture`. Return it from `luv_register` to stop the load. |
| `LUV_OFF_THREAD` | -5 | An engine function was called from a thread that does not run Luau. See [The game thread](#the-game-thread). |

### Modes

| Name | Value |
| --- | --- |
| `LUV_WORKER` | 0 |
| `LUV_INLINE` | 1 |
| `LUV_PARALLEL` | 2 |

See [Execution modes](#execution-modes).

### Argument kinds

`arg_kind` returns one of these.

| Name | Value | Luau values |
| --- | --- | --- |
| `LUV_KIND_NONE` | -1 | There is no argument at that index. |
| `LUV_KIND_NIL` | 0 | `nil` |
| `LUV_KIND_BOOLEAN` | 1 | `boolean` |
| `LUV_KIND_NUMBER` | 2 | `number` |
| `LUV_KIND_STRING` | 3 | `string` or `buffer`. Both arrive as a copy. |
| `LUV_KIND_UDIM` | 4 | [UDim](udim.md) |
| `LUV_KIND_COLOR` | 5 | [Color](color.md) |
| `LUV_KIND_OBJECT` | 6 | An object of a plugin class. |
| `LUV_KIND_POINTER` | 7 | [Pointer](pointer.md), [NativeFunction](nativefunction.md) or [Callback](callback.md). |
| `LUV_KIND_VALUE` | 8 | Anything else, like a table, a function or a [Window](window.md). |
| `LUV_KIND_BUFFER` | 9 | A `buffer`. Only [LuvValue](#luvvalue) uses it. `arg_kind` reports a buffer as `LUV_KIND_STRING`. |

### Versions

| Name | Value | Meaning |
| --- | --- | --- |
| `LUV_API_VERSION` | 2 | The version of `LuvApi`. Compare it with `api->version`. |
| `LUV_RENDER_VERSION` | 1 | The version of `LuvRenderContext`. Compare it with `context->version`. |

## Classes

### LuvClassInfo

Describes one class for `define_class`.

```c
typedef struct LuvClassInfo {
    const char* name;
    uint64_t size;
    void (*destroy)(void* data);
    const LuvMethod* methods;
    const LuvProperty* properties;
    const LuvMethod* statics;
    const LuvProperty* static_properties;
} LuvClassInfo;
```

| Field | Type | Description |
| --- | --- | --- |
| `name` | `const char*` | The class name. `typeof` returns it in Luau. |
| `size` | `uint64_t` | The size of the data of each object, in bytes. At most 1 GiB. |
| `destroy` | `void (*)(void*)` | Runs when an object goes away. Can be NULL. See [Destructors and object lifetime](#destructors-and-object-lifetime). |
| `methods` | `const LuvMethod*` | Methods and operators of each object. Can be NULL. |
| `properties` | `const LuvProperty*` | Properties of each object. Can be NULL. |
| `statics` | `const LuvMethod*` | Functions on the class table, like `new`. Can be NULL. |
| `static_properties` | `const LuvProperty*` | Values on the class table, like `zero`. Can be NULL. |

luv copies the names while `define_class` runs. So the info struct and the lists can live on the stack. The functions must stay in your library.

### LuvMethod

```c
typedef void (*LuvFunction)(LuvCall* call);

typedef struct LuvMethod {
    const char* name;
    LuvFunction function;
    uint32_t flags;
} LuvMethod;
```

| Field | Type | Description |
| --- | --- | --- |
| `name` | `const char*` | The name in Luau. |
| `function` | `LuvFunction` | The function to run. It cannot be NULL. |
| `flags` | `uint32_t` | `LUV_WORKER`, `LUV_INLINE` or `LUV_PARALLEL`. See [Execution modes](#execution-modes). |

End each list with `{0}`. A list can hold up to 4096 entries.

### LuvProperty

```c
typedef struct LuvProperty {
    const char* name;
    LuvFunction get;
    LuvFunction set;
} LuvProperty;
```

| Field | Type | Description |
| --- | --- | --- |
| `name` | `const char*` | The name in Luau. |
| `get` | `LuvFunction` | Pushes the value. NULL makes the property write only. |
| `set` | `LuvFunction` | Gets the new value as argument 0. NULL makes the property read only. |

A property needs `get`, `set` or both. End each list with `{0}`.

A static property with a `set` function can be written, like `Vec3.origin = point`.

### Names

- Class, service, method, property and function names use letters, digits and `_`. They cannot start with a digit.
- Method and static names cannot start with `__`. The only exceptions are these operator methods: `__add`, `__sub`, `__mul`, `__div`, `__idiv`, `__mod`, `__pow`, `__unm`, `__eq`, `__lt`, `__le`, `__len`, `__concat`, `__call`, `__tostring`, `__index` and `__newindex`.
- Operators only go in `methods`, not in `statics`.
- A name can appear once in each list. A method and a property cannot share a name. A static and a static property cannot share a name.
- Keep class names unique across your libraries. `find_class` returns the first class with a name.

## Execution modes

The `flags` of a method, static or exported function pick where it runs.

| Mode | Value | Where it runs | Yields in Luau |
| --- | --- | --- | --- |
| `LUV_INLINE` | 1 | Right away, on the thread that runs Luau. | no |
| `LUV_WORKER` | 0 | On the thread of the library. One call at a time, in order, together with plain function calls from [NativeFunction](nativefunction.md). | yes |
| `LUV_PARALLEL` | 2 | On a thread from a pool. Calls can run at the same time. | yes |

These always run inline, whatever their flags say:

- Property getters and setters.
- Static property getters.
- Operators.

`LUV_INLINE` is the fastest, but Luau waits for it. Keep inline functions short. Use `LUV_WORKER` or `LUV_PARALLEL` for slow work. The calling coroutine then yields and the game keeps running.

Other plugin code runs off the Luau thread too:

- `luv_register` and destructors run on the thread of the library.
- Render hooks run on the renderer thread of the window.

luv never locks the data of your objects. An inline method, a worker method and a render hook can use one object at the same time. Guard shared data with your own lock.

luv reads every argument before the function runs. Strings and buffers are copied. So Luau can change its values while a worker call runs.

## How exports look in Luau

[Library.Exports](library.md#exports) is a read only table with one entry for each class and each exported function.

A class entry is a read only table:

- Static functions are called with a dot, like `Vec3.new(1, 2, 3)`.
- Static properties are read with a dot, like `Vec3.zero`.
- `tostring(Vec3)` returns the class name.
- Other keys error with `'nope' is not a valid member of Vec3`.

An object of the class works like this:

- `typeof(object)` returns the class name.
- Methods are called with a colon, like `a:Dot(b)`. Calling one with a dot errors with `Vec3:Dot must be called on a Vec3 with ':'`.
- Properties are read and set with a dot. A property without a getter errors with `Vec3.X cannot be read`. A property without a setter errors with `Vec3.Magnitude is read only`.
- Reading a key tries methods, then properties, then the `__index` operator. Setting a key tries properties, then the `__newindex` operator. Other keys error with `'Nope' is not a valid member of Vec3`.
- The same object always gives the same Luau value.

Operators get every operand as an argument, starting at index 0. They get no `self`, so read the operands with `check_object` or `to_object`. `__unm`, `__len` and `__tostring` get one argument. `__index` gets the object and the key. `__newindex` gets the object, the key and the value. `__call` gets the object and then the call arguments.

When a class has no operator for something:

| Operator | What happens |
| --- | --- |
| `__tostring` | Returns the class name. |
| `__eq` | `true` only for the same object. |
| `__call` | Errors with `attempt to call a Vec3 value`. |
| Any other | Errors with `Vec3 does not support the __sub operator`. |

A game can use at most 64 plugin classes. Using more errors with `cannot use the native class X, a game can use at most 64 native classes`.

Errors from a call start with the name of the member. `Vec3:Dot` is a method, `Vec3.new` is a static, `Vec3.X` is a property, `Vec3.__add` is an operator and `hello` is an exported function. An example is `Vec3:Dot: argument #1 must be a Vec3, got number`.

This C class shows the parts together:

```c
#include <math.h>
#include <stddef.h>
#include <stdio.h>
#include "luv.h"

typedef struct Vec3 {
    double x;
    double y;
    double z;
} Vec3;

static const LuvApi* api;
static const LuvClass* vec3_class;

static Vec3* push_vec3(LuvCall* call, double x, double y, double z) {
    Vec3* value = (Vec3*)api->push_object(call, vec3_class);
    if (value) {
        value->x = x;
        value->y = y;
        value->z = z;
    }
    return value;
}

static void vec3_new(LuvCall* call) {
    push_vec3(call, api->opt_number(call, 0, 0), api->opt_number(call, 1, 0), api->opt_number(call, 2, 0));
}

static void vec3_zero(LuvCall* call) {
    push_vec3(call, 0, 0, 0);
}

static void vec3_get_x(LuvCall* call) {
    api->push_number(call, ((Vec3*)api->self_data(call))->x);
}

static void vec3_set_x(LuvCall* call) {
    ((Vec3*)api->self_data(call))->x = api->check_number(call, 0);
}

static void vec3_magnitude(LuvCall* call) {
    Vec3* self = (Vec3*)api->self_data(call);
    api->push_number(call, sqrt(self->x * self->x + self->y * self->y + self->z * self->z));
}

static void vec3_dot(LuvCall* call) {
    Vec3* self = (Vec3*)api->self_data(call);
    Vec3* other = (Vec3*)api->check_object(call, 0, vec3_class);
    if (other) {
        api->push_number(call, self->x * other->x + self->y * other->y + self->z * other->z);
    }
}

static void vec3_add(LuvCall* call) {
    Vec3* left = (Vec3*)api->check_object(call, 0, vec3_class);
    Vec3* right = (Vec3*)api->check_object(call, 1, vec3_class);
    if (left && right) {
        push_vec3(call, left->x + right->x, left->y + right->y, left->z + right->z);
    }
}

static void vec3_mul(LuvCall* call) {
    Vec3* value = (Vec3*)api->to_object(call, 0, vec3_class);
    double factor;
    if (value) {
        factor = api->check_number(call, 1);
    } else {
        factor = api->check_number(call, 0);
        value = (Vec3*)api->check_object(call, 1, vec3_class);
    }
    if (value) {
        push_vec3(call, value->x * factor, value->y * factor, value->z * factor);
    }
}

static void vec3_tostring(LuvCall* call) {
    Vec3* value = (Vec3*)api->check_object(call, 0, vec3_class);
    char text[128];
    if (value) {
        snprintf(text, sizeof text, "Vec3(%g, %g, %g)", value->x, value->y, value->z);
        api->push_string(call, text);
    }
}

static const LuvMethod vec3_methods[] = {
    {"Dot", vec3_dot, LUV_INLINE},
    {"__add", vec3_add, 0},
    {"__mul", vec3_mul, 0},
    {"__tostring", vec3_tostring, 0},
    {0},
};

static const LuvProperty vec3_properties[] = {
    {"X", vec3_get_x, vec3_set_x},
    {"Magnitude", vec3_magnitude, NULL},
    {0},
};

static const LuvMethod vec3_statics[] = {
    {"new", vec3_new, LUV_INLINE},
    {0},
};

static const LuvProperty vec3_static_properties[] = {
    {"zero", vec3_zero, NULL},
    {0},
};

LUV_EXPORT int32_t luv_register(const LuvApi* given, LuvRegistry* registry) {
    api = given;
    if (api->version < LUV_API_VERSION) {
        return LUV_INVALID;
    }
    LuvClassInfo info = {"Vec3", sizeof(Vec3), NULL, vec3_methods, vec3_properties, vec3_statics, vec3_static_properties};
    vec3_class = api->define_class(registry, &info);
    return vec3_class ? LUV_OK : LUV_INVALID;
}
```

Luau uses it like this:

```luau
local DLL = import("DLL")

local vectors = DLL.Load("./vectors")
local Vec3 = vectors.Exports.Vec3
local a = Vec3.new(1, 2, 3)
local b = Vec3.new(4, 5, 6)
a.X = 10
print(typeof(a), tostring(a + b), tostring(2 * a))
print(a:Dot(b), Vec3.new(3, 4, 0).Magnitude, tostring(Vec3.zero))
```

## Services

A service is a table that Luau gets from [import](globals.md#import), next to the libraries that luv ships with. It holds functions and properties, and no objects. Use it when your plugin is a whole feature instead of a few loose functions.

### LuvServiceInfo

```c
typedef struct LuvServiceInfo {
    const char* name;
    const LuvMethod* functions;
    const LuvProperty* properties;
} LuvServiceInfo;
```

| Field | Type | Description |
| --- | --- | --- |
| `name` | `const char*` | The name for `import`. |
| `functions` | `const LuvMethod*` | Functions on the table, called with a dot. Can be NULL. |
| `properties` | `const LuvProperty*` | Values on the table, read and written with a dot. Can be NULL. |

A service works like the class table of a class with no objects. `tostring` gives its name. Other keys error with `'Nope' is not a valid member of Fixture`. The functions cannot be replaced from Luau.

The name arrives in Luau when [DLL.Load](dll.md#load) finishes, so load the library before you import the service.

```c
static void physics_gravity_get(LuvCall* call) {
    api->push_number(call, gravity);
}

static void physics_gravity_set(LuvCall* call) {
    gravity = api->check_number(call, 0);
}

static void physics_step(LuvCall* call) {
    api->push_number(call, advance(api->check_number(call, 0)));
}

static const LuvMethod physics_functions[] = {
    {"Step", physics_step, LUV_INLINE},
    {0},
};

static const LuvProperty physics_properties[] = {
    {"Gravity", physics_gravity_get, physics_gravity_set},
    {0},
};

LUV_EXPORT int32_t luv_register(const LuvApi* given, LuvRegistry* registry) {
    api = given;
    if (api->version < LUV_API_VERSION) {
        return LUV_INVALID;
    }
    LuvServiceInfo info = {"Physics", physics_functions, physics_properties};
    return api->define_service(registry, &info) ? LUV_OK : LUV_INVALID;
}
```

```luau
local DLL = import("DLL")

DLL.Load("./physics")
local Physics = import("Physics")
Physics.Gravity = 9.81
print(Physics.Step(1 / 60))
```

A service name cannot match a library that luv already ships, like `Net` or `Window`. `DLL.Load` then fails with `'Net' cannot be a service because luv already imports a library with that name`. [Library:GetServices](library.md#getservices) lists the names a library added.

To type the service in Luau, put a type file next to your sources. See [Type files](../manual/native-plugins.md#type-files).

## LuvValue

The engine functions pass Luau values in and out with one struct.

```c
typedef struct LuvValue {
    int32_t kind;
    uint32_t flags;
    double numbers[4];
    const void* data;
    uint64_t length;
    LuvRef* handle;
} LuvValue;
```

| `kind` | What to fill in | What you get back |
| --- | --- | --- |
| `LUV_KIND_NIL` | nothing | nothing |
| `LUV_KIND_BOOLEAN` | `numbers[0]`, 0 for false | `numbers[0]` |
| `LUV_KIND_NUMBER` | `numbers[0]` | `numbers[0]` |
| `LUV_KIND_STRING` | `data` and `length` | `data` and `length`, with a zero byte after the text |
| `LUV_KIND_BUFFER` | `data` and `length` | `data` and `length` |
| `LUV_KIND_UDIM` | `numbers[0]` to `numbers[2]` | the same three |
| `LUV_KIND_COLOR` | `numbers[0]` to `numbers[3]` | the same four |
| `LUV_KIND_POINTER` | `data` as the address | `data` as the address |
| `LUV_KIND_OBJECT` | `handle` | `handle`, and `data` as the object data |
| `LUV_KIND_VALUE` | `handle` | `handle` |

`flags` is not used yet. Set it to 0.

luv copies the bytes you pass in, so they only need to live while the call runs. The bytes you get back live until your function returns.

A `handle` you get back is yours. Call `release` on it when you are done, the same as one from `retain`. A `handle` you pass in stays yours.

`luv.h` builds one for you:

```c
LuvValue nothing = luv_nil();
LuvValue yes = luv_boolean(1);
LuvValue count = luv_number(12);
LuvValue name = luv_bytes("player", 6);
LuvValue samples = luv_buffer(pcm, sizeof pcm);
LuvValue size = luv_udim(64, 48, 0);
LuvValue tint = luv_color(1, 0, 0, 1);
LuvValue held = luv_held(handle);
```

## LuvApi

`luv_register` gets a `const LuvApi*`. Each function in it is a field that you call through the pointer, like `api->push_number(call, 1)`.

| Field | Type | Description |
| --- | --- | --- |
| `version` | `uint32_t` | The API version of the running luv. |
| `struct_size` | `uint32_t` | The size of `LuvApi` in bytes. |

A `LuvCall*` is only valid while your function runs. An event call is valid from `begin_event` until `send_event`. Do not use one call from two threads at the same time. Argument indexes start at 0 and do not count `self`.

### Registration

| Function | Returns | Description |
| --- | --- | --- |
| `define_class(registry, info)` | `const LuvClass*` | Defines a class from a `LuvClassInfo`. Returns NULL and records the problem when the class is not valid. When the same library defines the same name again, it returns the class from before. |
| `define_function(registry, function)` | `int32_t` | Exports one `LuvMethod` as a Luau function. Returns `LUV_OK`, or `LUV_INVALID` and records the problem. The name cannot match another function or a class defined before it. |
| `find_class(name)` | `const LuvClass*` | Finds a class from any loaded library by its name. Returns NULL when there is none. |
| `class_name(cls)` | `const char*` | The name of a class, or NULL. |

A `LuvClass*` stays valid until the game ends.

### Arguments

| Function | Returns | Description |
| --- | --- | --- |
| `arg_count(call)` | `int32_t` | The number of arguments, not counting `self`. |
| `arg_kind(call, index)` | `int32_t` | The [kind](#argument-kinds) of an argument, or `LUV_KIND_NONE`. |
| `arg_class(call, index)` | `const LuvClass*` | The class of an object argument, or NULL. |
| `self_data(call)` | `void*` | The data of `self` in methods and property functions. NULL in statics, exported functions and operators. |

### Check and opt

A `check_` function records an error when the argument has the wrong type, and then returns 0 or NULL. Your function keeps running, so handle that value. When your function returns, the call raises the first error and drops every pushed result. The message looks like `argument #1 must be a number, got string`. The argument number in it starts at 1.

An `opt_` function returns the fallback when the argument is missing or `nil`. A wrong type still records an error.

| Function | Returns | Description |
| --- | --- | --- |
| `check_boolean(call, index)` | `int32_t` | 1 or 0. |
| `opt_boolean(call, index, fallback)` | `int32_t` | 1, 0 or the fallback. |
| `check_number(call, index)` | `double` | The number. |
| `opt_number(call, index, fallback)` | `double` | The number or the fallback. |
| `check_string(call, index, length)` | `const char*` | A copy of the text with a zero byte at the end. It stays valid until your function returns. Buffers work too. When `length` is not NULL, it gets the byte count without the zero. The text can hold zero bytes. |
| `opt_string(call, index, fallback, length)` | `const char*` | The text or the fallback. `length` gets the length of the one that is returned. |
| `check_udim(call, index, xyz)` | `int32_t` | Writes `X`, `Y` and `Z` into `double xyz[3]` and returns 1. Returns 0 for anything but a UDim. |
| `check_color(call, index, rgba)` | `int32_t` | Writes `R`, `G`, `B` and `A` into `double rgba[4]` and returns 1. Returns 0 for anything but a Color. |
| `check_object(call, index, cls)` | `void*` | The data of an object of class `cls`. With `cls` set to NULL, any plugin object works. |
| `to_object(call, index, cls)` | `void*` | Like `check_object`, but it records no error. Use it to try each operand of an operator. |
| `check_pointer(call, index)` | `void*` | The address of a Pointer, NativeFunction or Callback. The data of a plugin object. A copy of a string or buffer. NULL for `nil`. |

### Push

Pushed values are the results of the call, in order. A function can push any number of them.

| Function | Returns | Description |
| --- | --- | --- |
| `push_nil(call)` | nothing | Pushes `nil`. |
| `push_boolean(call, value)` | nothing | Pushes `false` for 0 and `true` for anything else. |
| `push_number(call, value)` | nothing | Pushes a number. |
| `push_string(call, text)` | nothing | Pushes a copy of text that ends with a zero byte. NULL pushes `nil`. |
| `push_bytes(call, data, length)` | nothing | Pushes `length` bytes as a Luau string. Length 0 pushes `""`. NULL data with a length above 0 pushes `nil`. |
| `push_udim(call, x, y, z)` | nothing | Pushes a [UDim](udim.md). |
| `push_color(call, r, g, b, a)` | nothing | Pushes a [Color](color.md). |
| `push_object(call, cls)` | `void*` | Makes a new object of class `cls` and pushes it. Returns its data for you to fill. The data is filled with zeros and aligned to 16 bytes. Returns NULL and records an error when the class is unknown or memory runs out. |
| `push_argument(call, index)` | nothing | Pushes the argument at `index` as the same Luau value. A missing index pushes `nil`. |
| `push_self(call)` | nothing | Pushes `self` as the same Luau value. Pushes `nil` outside of methods and property functions. |
| `push_pointer(call, address)` | nothing | Pushes a foreign [Pointer](pointer.md). NULL pushes `nil`. |
| `push_ref(call, ref)` | nothing | Pushes the value that `ref` holds. NULL pushes `nil`. |

### Failing and logging

| Function | Returns | Description |
| --- | --- | --- |
| `fail(call, message)` | nothing | Makes the call error with the member name, a colon and `message`. NULL gives `the native call failed`. The first error wins. |
| `print(message)` | nothing | Prints a line to the standard output. |
| `warn(message)` | nothing | Prints `warning: ` and the message to the standard error. |

`print` and `warn` work from any thread.

### Refs and events

A ref keeps a Luau value alive after the call ends. An event calls a Luau function later, from any thread.

| Function | Returns | Description |
| --- | --- | --- |
| `retain(call, index)` | `LuvRef*` | Keeps the argument at `index` alive. Only works for arguments of kind `LUV_KIND_VALUE`, like tables and functions. Returns NULL for other kinds. The ref stays valid even when the call fails. |
| `release(ref)` | nothing | Lets go of the value. Call it once for each ref. Works from any thread. |
| `begin_event(ref)` | `LuvCall*` | Starts an event for a retained function. Push the arguments into the call it returns. Returns NULL when `ref` is NULL. |
| `send_event(event)` | `int32_t` | Sends the event and frees it. Returns `LUV_OK`, or `LUV_INVALID` when the event recorded an error or the game has closed. Works from any thread. |

luv runs the function on the Luau thread, in a new coroutine, with the pushed values as arguments. Events arrive in the order you send them. Return values are ignored. Errors are reported like other uncaught errors. When the value is not a function, luv reports `a native library sent an event to a table instead of a function`.

`push_argument` and `push_self` push `nil` in an event. `push_object` works.

A ref, or a thread that sends events, does not keep the game running on its own. If nothing else keeps it running, the game can end before your events arrive.

```c
static void countdown(LuvCall* call) {
    LuvRef* handler = api->retain(call, 0);
    int32_t from = (int32_t)api->check_number(call, 1);
    if (!handler) {
        api->fail(call, "countdown needs a function");
        return;
    }
    for (int32_t value = from; value > 0; value--) {
        LuvCall* event = api->begin_event(handler);
        api->push_number(event, value);
        api->send_event(event);
    }
    api->release(handler);
}
```

### The game thread

Luau runs on one thread. The functions in [The engine](#the-engine), [Members](#members) and [Timed work](#timed-work) need that thread, because they read and write Luau values.

They work in:

- A function, method, property or operator with `LUV_INLINE`.
- A signal handler or function value made with `LUV_INLINE`.
- A task from `schedule` with `LUV_INLINE`.

They do not work in a `LUV_WORKER` or `LUV_PARALLEL` call, in a render hook or on a thread of your own. There they fail the call with `read_member needs the game thread, register it with LUV_INLINE` and return `LUV_OFF_THREAD` or NULL. Use `on_game_thread` to check first, or use [From any thread](#from-any-thread) instead.

These functions call back into Luau. Keep them short. A long one holds up every frame.

### Values

| Function | Returns | Description |
| --- | --- | --- |
| `on_game_thread(call)` | `int32_t` | 1 when this call runs on the thread that runs Luau, 0 when it does not. |
| `call_data(call)` | `void*` | The `data` pointer you gave to `new_function`, `connect` or `schedule`. NULL in every other call. |
| `arg_value(call, index, out)` | `int32_t` | Writes the argument at `index` into `out`. Returns `LUV_OK`, or `LUV_OUT_OF_RANGE` with a kind of `LUV_KIND_NONE`. Works from any thread. |
| `push_value(call, value)` | nothing | Pushes a `LuvValue` as a result. NULL pushes `nil`. Works from any thread. |
| `push_buffer(call, length)` | `void*` | Pushes a Luau `buffer` of `length` bytes and returns memory to fill. The bytes start at zero. The memory lives until your function returns. Returns NULL when the length does not fit. Works from any thread. |
| `push_asset(call, name, data, length)` | `int32_t` | Copies `length` bytes and pushes them as an [Asset](asset.md), without touching the `assets` folder. Returns `LUV_OK`, or a code below 0 and fails the call. Works from any thread. |

`push_asset` is how a plugin hands Luau a picture it made or fetched. Give the name a real extension, like `avatar.png`, because luv reads the format from the bytes first and falls back to the name. The Asset works anywhere one from [Asset.Load](asset.md#load) does, so a [RenderableImage](renderableimage.md) can take it straight.

```c
static void badge(LuvCall* call) {
    uint64_t length = 0;
    const unsigned char* bytes = build_png(&length);
    api->push_asset(call, "badge.png", bytes, length);
}
```

```luau
local DLL = import("DLL")

local plugin = DLL.Load("./badges")
local image = plugin.Exports.badge()
Renderable.new("RenderableImage", { Image = image, Position = udim.new(40, 40) })
```

`arg_value` gives a `handle` for a table, a function or an engine object. That handle is yours, so `release` it when you are done.

`push_buffer` is the short way to hand raw bytes to Luau. A [FromBytes](frombytes.md) node takes them straight, so a plugin can make sound and push it in.

```c
static void samples(LuvCall* call) {
    uint64_t frames = (uint64_t)api->check_number(call, 0);
    int16_t* room = (int16_t*)api->push_buffer(call, frames * 2 * sizeof(int16_t));
    if (room) {
        fill_tone(room, frames);
    }
}
```

### The engine

| Function | Returns | Description |
| --- | --- | --- |
| `get_import(call, name)` | `LuvRef*` | The library or service that [import](globals.md#import) gives for `name`. NULL when there is none. |
| `get_global(call, name)` | `LuvRef*` | A global from Luau. NULL when the read fails. A missing name gives a ref that holds `nil`. |
| `set_global(call, name, value)` | `int32_t` | Sets a global. Returns `LUV_OK` or `LUV_INVALID`. |
| `get_api(call, window, name)` | `LuvRef*` | The same as `window:GetAPI(name)`. NULL when the window or the name is wrong. |
| `new_table(call)` | `LuvRef*` | A new empty Luau table. |
| `new_signal(call, name)` | `LuvRef*` | A new [Signal](signal.md). NULL or an empty name gives an unnamed one. |
| `new_function(call, name, function, data, flags)` | `LuvRef*` | A Luau function that runs `function`. `data` comes back from `call_data`. `flags` picks the [mode](#execution-modes). |

`name` is only used in error messages for `new_function`.

A function value made with `LUV_WORKER` or `LUV_PARALLEL` yields the coroutine that calls it, the same as a worker method.

Keep the `data` pointer alive for as long as the function or task can run. luv never frees it.

### Members

These work on the value a ref holds. The value can be an engine object like a [Window](window.md) or a [Renderable](renderable.md), a table, or anything else with fields.

| Function | Returns | Description |
| --- | --- | --- |
| `read_member(call, target, name, out)` | `int32_t` | Reads `target.name` into `out`. Returns `LUV_OK`, or `LUV_UNKNOWN_NAME` and fails the call. |
| `write_member(call, target, name, value)` | `int32_t` | Sets `target.name`. Returns `LUV_OK`, or `LUV_UNKNOWN_NAME` and fails the call. |
| `call_member(call, target, name, args, count, results, limit)` | `int32_t` | Calls `target:name(...)` with `count` arguments. Writes up to `limit` results into `results` and returns how many it wrote. Returns a result code below 0 when the call fails. |
| `construct(call, api, name, args, count)` | `LuvRef*` | Calls `api.name(...)` with a dot and holds the first result. NULL when the call fails. |
| `connect(call, signal, id, function, data, flags)` | `int32_t` | Binds `function` to a [Signal](signal.md) under `id`, the same as `signal:BindHandler(id, handler)`. Returns `LUV_OK` or a code below 0. |

`call_member` passes the target as `self`, like a colon call in Luau. `construct` does not, like a dot call. Pass NULL and 0 when there are no arguments. At most 64 arguments and 64 results.

A method that yields, like a query or a network read, cannot run through `call_member`. Send it with `post_call` instead, which runs it in its own coroutine.

This makes a shape in a window and keeps it:

```c
static void make_shape(LuvCall* call) {
    LuvValue window = luv_nil();
    if (api->arg_value(call, 0, &window) != LUV_OK || !window.handle) {
        api->fail(call, "make_shape needs a window");
        return;
    }
    LuvRef* renderable = api->get_api(call, window.handle, "Renderable");
    LuvValue class_name = luv_bytes("RenderableShape", 15);
    LuvRef* shape = api->construct(call, renderable, "new", &class_name, 1);
    if (shape) {
        LuvValue size = luv_udim(64, 48, 0);
        api->write_member(call, shape, "Size", &size);
        api->push_ref(call, shape);
        api->release(shape);
    }
    api->release(renderable);
    api->release(window.handle);
}
```

### From any thread

| Function | Returns | Description |
| --- | --- | --- |
| `post_call(target, name, args, count)` | `int32_t` | Asks luv to call `target:name(...)` later. Returns `LUV_OK`, or `LUV_INVALID` when the game has closed. |
| `post_write(target, name, value)` | `int32_t` | Asks luv to set `target.name` later. Returns `LUV_OK` or `LUV_INVALID`. |

Both copy what you pass and return right away. luv runs them on the game thread, in the order you sent them, together with the events from `send_event`. A method runs in its own coroutine, so one that yields is fine.

Nothing comes back. When you need the answer, call from an inline function instead.

```c
static void nudge(LuvCall* call) {
    LuvValue target = luv_nil();
    if (api->arg_value(call, 0, &target) == LUV_OK && target.handle) {
        LuvValue position = luv_udim(10, 20, 0);
        api->post_write(target.handle, "Position", &position);
        api->release(target.handle);
    }
}
```

### Timed work

| Function | Returns | Description |
| --- | --- | --- |
| `schedule(call, name, function, data, seconds, flags)` | `LuvTask*` | Runs `function` again and again with a gap of `seconds`. NULL when the gap is not a number of at least 0 or the call is not on the game thread. |
| `cancel(task)` | nothing | Stops the task and frees the handle. Call it once for each task. Works from any thread. |

The gap is at least one thousandth of a second, so 0 means every millisecond. The first run waits one gap.

`name` is only used in error messages. `data` comes back from `call_data`. `flags` picks the [mode](#execution-modes), and only `LUV_INLINE` can reach the engine.

A task does not keep the game running. It stops when the game ends, when its library is destroyed, and when you cancel it. Errors from it are reported like other uncaught errors.

```c
static void beat(LuvCall* call) {
    Sim* sim = (Sim*)api->call_data(call);
    api->post_call(sim->listener, "Fire", NULL, 0);
}

static void start(LuvCall* call) {
    sim.task = api->schedule(call, "sim", beat, &sim, 1.0 / 60.0, LUV_INLINE);
}
```

## Destructors and object lifetime

- `push_object` makes an object. Its data is `size` bytes, filled with zeros and aligned to 16 bytes. The data never moves.
- An object lives while Luau code, a Pointer, a render hook or a running call uses it.
- Then luv calls `destroy(data)` on the thread of the library, after the calls that already wait there. If the library was destroyed with [Library:Destroy](library.md#destroy), `destroy` runs right away on the thread that let go of the object.
- After `destroy` returns, luv frees the data. Do not free it yourself.
- Native code cannot keep an object alive on its own. Do not use the data of an object after the call that got it has returned, unless Luau still holds the object.

This class owns memory that it got from `malloc`. Its destructor frees that memory, but not the object itself:

```c
#include <stdlib.h>
#include "luv.h"

typedef struct Sprite {
    uint8_t* pixels;
    int32_t width;
    int32_t height;
} Sprite;

static void sprite_destroy(void* data) {
    Sprite* sprite = (Sprite*)data;
    free(sprite->pixels);
}
```

Pass the destructor in the class info, like `LuvClassInfo info = {"Sprite", sizeof(Sprite), sprite_destroy, methods, NULL, statics, NULL};`.

## LuvRenderContext

A render hook is an exported function that luv calls every frame. Set it with the `RenderHook` property of a [Renderable](renderable.md) or a [PostProcess](postprocess.md). See [Render hooks](../manual/render-hooks.md) for a guide.

```c
typedef void (*LuvRenderHook)(LuvRenderContext* context);
```

The context is only valid while the hook runs.

| Field | Type | Description |
| --- | --- | --- |
| `version` | `uint32_t` | `LUV_RENDER_VERSION`. |
| `struct_size` | `uint32_t` | The size of `LuvRenderContext` in bytes. |
| `user_data` | `void*` | The address from `RenderHookData`, or NULL. |
| `renderable` | `uint64_t` | An id for the object being drawn. |
| `time` | `double` | Seconds since the window opened. |
| `delta` | `double` | Seconds since the last frame. |
| `frame` | `uint32_t` | The frame number. |
| `width`, `height` | `float` | The size of the window in the same units as UDim positions. |
| `scale` | `float` | The display scale of the window. |
| `position` | `float[2]` | The `Position` of a RenderableShape, RenderableImage or RenderableText. 0 for other objects. |
| `size` | `float[2]` | The size the object is drawn at. 0 for other objects. |
| `anchor` | `float[2]` | The `AnchorPoint`. 0 for other objects. |
| `rotation` | `float` | The `Rotation` in degrees. 0 for other objects. |
| `write_data` | function | See [write_data](#write-data). |
| `write_texture` | function | See [write_texture](#write-texture). |
| `set_draw_counts` | function | See [set_draw_counts](#set-draw-counts). |
| `vulkan` | `const LuvVulkan*` | The Vulkan handles of luv, or NULL when luv does not draw with Vulkan. |
| `engine` | `void*` | Used by luv. Do not touch it. |

### LuvVulkan

| Field | Type | Description |
| --- | --- | --- |
| `instance` | `void*` | The `VkInstance`. |
| `physical_device` | `void*` | The `VkPhysicalDevice`. |
| `device` | `void*` | The `VkDevice`. |
| `queue` | `void*` | The `VkQueue` that luv draws with. |
| `queue_family` | `uint32_t` | The family index of that queue. |
| `queue_index` | `uint32_t` | The index of that queue in its family. |
| `get_instance_proc_addr` | function | `vkGetInstanceProcAddr`. |

luv holds its lock on the GPU queue while hooks run.

### write_data

```c
int32_t (*write_data)(LuvRenderContext* context, const char* name, uint64_t offset, const void* data, uint64_t length);
```

Copies `length` bytes into the shader data called `name`, starting `offset` bytes in. luv looks for the name in every shader loaded on the object. The name can be:

- A binding, like `vertices`.
- A field of a uniform or storage buffer, like `tint`.
- A path through nested fields, like `params.tint`.

The bytes must follow the WGSL memory layout. A storage buffer that ends with an array without a length grows to fit, up to 256 MiB.

| Return value | When |
| --- | --- |
| `LUV_OK` | The data was written. |
| `LUV_UNKNOWN_NAME` | No loaded shader has data with that name. |
| `LUV_WRONG_KIND` | The name is not a uniform or storage buffer. |
| `LUV_OUT_OF_RANGE` | The bytes do not fit in the buffer. |
| `LUV_INVALID` | `name` is NULL, `data` is NULL with a length above 0, the length is over 256 MiB, or the object is gone. |

### write_texture

```c
int32_t (*write_texture)(LuvRenderContext* context, const char* name, uint32_t width, uint32_t height, const void* rgba);
```

Uploads `width * height * 4` bytes of RGBA pixels to the texture binding called `name`. The binding must be a `texture_2d<f32>`. luv makes a new texture when the size changes.

| Return value | When |
| --- | --- |
| `LUV_OK` | The pixels were uploaded. |
| `LUV_UNKNOWN_NAME` | No loaded shader has a binding with that name. |
| `LUV_WRONG_KIND` | The binding is not a `texture_2d<f32>`. |
| `LUV_OUT_OF_RANGE` | The texture is larger than the GPU allows. |
| `LUV_INVALID` | `name` or `rgba` is NULL, the width or height is 0, the pixels are over 256 MiB, or the object is gone. |

When `write_data` or `write_texture` fails for a reason other than a bad argument, luv also reports the error as a game error. Each message is reported once for each window. An example is `a RenderHook wrote 'missing', but shader 'hooked' has no data named 'missing', it declares params, pattern`.

### set_draw_counts

```c
void (*set_draw_counts)(LuvRenderContext* context, uint32_t vertex_count, uint32_t instance_count);
```

Sets how many vertices and instances a plain `Renderable` draws. It does nothing for other kinds of objects. It does not change the `VertexCount` and `InstanceCount` properties that Luau sees.

## Errors

These messages come from `luv_register`. `DLL.Load` shows them after `luv_register in <path> failed`.

| Message | Cause |
| --- | --- |
| `a class needs a name made of letters, digits and underscores` | The class name is missing or not valid. |
| `Vec3 objects cannot be larger than 1073741824 bytes` | `size` is over 1 GiB. |
| `Vec3.Dot has no function` | A method or static has a NULL function. |
| `Vec3: '__foo' is not a valid method name, operators must be one of ...` | A method name is not valid. |
| `Vec3: 'x-y' is not a valid property name` | A property name is not valid. |
| `Vec3: '__new' is not a valid function name` | A static name is not valid. |
| `Vec3:Dot is defined twice` | Two methods share a name. |
| `Vec3.X is defined twice` | Two properties, operators or statics share a name. |
| `Vec3.X needs a getter or a setter` | A property has neither. |
| `Vec3.X is both a method and a property` | A method and a property share a name. |
| `Vec3.zero is both a function and a property` | A static and a static property share a name. |
| `a member list has more than 4096 entries, is it missing its {0} terminator?` | A list has no `{0}` at the end. |
| `an exported function needs a name made of letters, digits and underscores` | A function name is not valid. |
| `the exported function x has no function` | An exported function is NULL. |
| `x is exported twice` | Two exports share a name. |

These messages come from calls:

| Message | Cause |
| --- | --- |
| `Vec3:Dot: argument #1 must be a Vec3, got number` | A `check_` function failed. |
| `hello: <message>` | The function called `fail`. |
| `cannot call Vec3:Grow because its library was unloaded` | A `LUV_WORKER` member was called after [Library:Destroy](library.md#destroy). |
| `push_object was given a class that was never defined` | `push_object` got a bad class. |
