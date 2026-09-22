# Native plugins

A native plugin is C, C++ or Rust code that luv builds together with your game. Luau calls it through the [DLL](../reference/dll.md) library. This page shows how the `native` folder works, how to call C functions, how to add classes to Luau and how plugins ship.

## The native folder

luv builds everything in the `native` folder of your project. `luv init` puts `luv.h` and `luv.rs` there for you.

```tree
my-game/
├── build.toml
├── native/
│   ├── luv.h
│   ├── luv.rs
│   ├── mathlib.c
│   ├── physics/
│   │   ├── shapes.c
│   │   └── solver.c
│   ├── counter/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   └── prebuilt.dll
└── src/
    └── main.luau
```

| Entry | What luv makes from it |
| --- | --- |
| `luv.h`, `luv.rs` | Nothing. These are the plugin API for C and for Rust. See [Plugin API for C](../reference/native-c.md) and [Plugin API for Rust](../reference/native-rust.md). |
| `mathlib.c` | `mathlib.dll` on Windows or `libmathlib.so` on Linux. Each C or C++ file right inside `native` becomes its own library, named after the file. |
| `physics/` | `physics.dll` or `libphysics.so`. A folder with C or C++ files becomes one library, named after the folder. luv also finds the files in its subfolders. |
| `counter/` | `counter.dll` or `libcounter.so`. A folder with a `Cargo.toml` is a Rust crate. The library is named after the crate. |
| `prebuilt.dll` | A copy with the same name. luv copies ready made libraries as they are. |

More rules:

- C and C++ files end in `.c`, `.cc`, `.cpp` or `.cxx`. A library with any C++ file is built as C++.
- Ready made libraries are `.dll` files on Windows. On Linux they are `.so` files, and files like `libsdk.so.1`. So one project can hold both builds of the same library.
- luv skips every other file, like headers.
- The `native` folder is on the include path. So `#include "luv.h"` works from any subfolder.
- Two entries that make the same file name stop the build.

Every library ends up in the build folder, next to the `.luvit` file. See [The build folder](../start/project-layout.md#the-build-folder).

## When luv builds

| Command | Builds native plugins |
| --- | --- |
| `luv test` | yes |
| `luv build` | yes |
| `luv package` | yes |
| `luv run` | no |

luv only builds what changed:

- A C or C++ library is built again when its file in the build folder is missing, or older than one of its source files or `native/luv.h`. Other headers are not checked. After you change only a header, save a source file again or delete the library in the build folder.
- For a Rust crate, luv runs Cargo every time. Cargo builds again what changed. luv copies the library into the build folder when it changed.
- A ready made library is copied when its size or date changed.

`luv test` prints a line like `Built native library mathlib.dll` for each library it built again.

On Windows, luv builds C and C++ with the Microsoft compiler from Visual Studio or the Visual Studio Build Tools. On Linux, it uses the C compiler of the system. Rust plugins need Rust and Cargo.

## Calling C functions

Any function or variable marked with `LUV_EXPORT` can be called from Luau. It does not need the plugin API.

```c title="native/mathlib.c"
#include <stdint.h>
#include <string.h>
#include "luv.h"

LUV_EXPORT int32_t add(int32_t a, int32_t b) {
    return a + b;
}

LUV_EXPORT const char* greeting(void) {
    return "hello from C";
}

LUV_EXPORT void fill(uint8_t* data, size_t length, uint8_t value) {
    memset(data, value, length);
}
```

Luau loads the library and describes each function with [type names](../reference/dll.md#type-names):

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local add = mathlib:GetFunction("add", "int", { "int", "int" })
local greeting = mathlib:GetFunction("greeting", "string")
local fill = mathlib:GetFunction("fill", "void", { "pointer", "usize", "u8" })

print(add(40, 2))
print(greeting())

local bytes = buffer.create(4)
fill(bytes, 4, 65)
print(buffer.tostring(bytes))
```

This prints `42`, `hello from C` and `AAAA`.

- Each call runs on the thread of the library and yields the calling coroutine. The game keeps running. See [NativeFunction](../reference/nativefunction.md).
- A buffer passed as a `pointer` is copied back after the call. So C can fill it.
- Structs, arrays, callbacks and memory are covered on [StructType](../reference/structtype.md), [ArrayType](../reference/arraytype.md), [Callback](../reference/callback.md) and [Pointer](../reference/pointer.md).

## A plugin class

The plugin API adds classes and functions straight to Luau. A library that exports `luv_register` gets it called when [DLL.Load](../reference/dll.md#load) loads it. In there, the plugin defines its classes and functions. They show up in [Library.Exports](../reference/library.md#exports).

This `Counter` class has a static `new`, a method `Increment`, a read only property `Count` and a property `Step`. The C and the Rust version do the same thing.

```c title="native/counter.c"
#include <stddef.h>
#include "luv.h"

typedef struct Counter {
    double count;
    double step;
} Counter;

static const LuvApi* api;
static const LuvClass* counter_class;

static void counter_new(LuvCall* call) {
    Counter* counter = (Counter*)api->push_object(call, counter_class);
    if (counter) {
        counter->count = api->opt_number(call, 0, 0);
        counter->step = api->opt_number(call, 1, 1);
    }
}

static void counter_increment(LuvCall* call) {
    Counter* counter = (Counter*)api->self_data(call);
    counter->count += counter->step;
    api->push_number(call, counter->count);
}

static void counter_count(LuvCall* call) {
    api->push_number(call, ((Counter*)api->self_data(call))->count);
}

static void counter_step(LuvCall* call) {
    api->push_number(call, ((Counter*)api->self_data(call))->step);
}

static void counter_set_step(LuvCall* call) {
    ((Counter*)api->self_data(call))->step = api->check_number(call, 0);
}

static const LuvMethod methods[] = {
    {"Increment", counter_increment, LUV_INLINE},
    {0},
};

static const LuvProperty properties[] = {
    {"Count", counter_count, NULL},
    {"Step", counter_step, counter_set_step},
    {0},
};

static const LuvMethod statics[] = {
    {"new", counter_new, LUV_INLINE},
    {0},
};

LUV_EXPORT int32_t luv_register(const LuvApi* given, LuvRegistry* registry) {
    api = given;
    if (api->version < LUV_API_VERSION) {
        return LUV_INVALID;
    }
    LuvClassInfo info = {"Counter", sizeof(Counter), NULL, methods, properties, statics, NULL};
    counter_class = api->define_class(registry, &info);
    return counter_class ? LUV_OK : LUV_INVALID;
}
```

```rust title="native/counter/src/lib.rs"
#[path = "../../luv.rs"]
mod luv;

use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use luv::*;

static API: AtomicPtr<LuvApi> = AtomicPtr::new(ptr::null_mut());
static COUNTER: AtomicPtr<LuvClass> = AtomicPtr::new(ptr::null_mut());

fn api() -> &'static LuvApi {
    unsafe { &*API.load(Ordering::Acquire) }
}

#[repr(C)]
struct Counter {
    count: f64,
    step: f64,
}

unsafe extern "C" fn counter_new(call: *mut LuvCall) {
    let api = api();
    let count = unsafe { (api.opt_number)(call, 0, 0.0) };
    let step = unsafe { (api.opt_number)(call, 1, 1.0) };
    unsafe { api.push_new(call, COUNTER.load(Ordering::Acquire), Counter { count, step }) };
}

unsafe extern "C" fn counter_increment(call: *mut LuvCall) {
    let api = api();
    if let Some(counter) = unsafe { api.this::<Counter>(call) } {
        counter.count += counter.step;
        unsafe { (api.push_number)(call, counter.count) };
    }
}

unsafe extern "C" fn counter_count(call: *mut LuvCall) {
    let api = api();
    if let Some(counter) = unsafe { api.this::<Counter>(call) } {
        unsafe { (api.push_number)(call, counter.count) };
    }
}

unsafe extern "C" fn counter_step(call: *mut LuvCall) {
    let api = api();
    if let Some(counter) = unsafe { api.this::<Counter>(call) } {
        unsafe { (api.push_number)(call, counter.step) };
    }
}

unsafe extern "C" fn counter_set_step(call: *mut LuvCall) {
    let api = api();
    let step = unsafe { (api.check_number)(call, 0) };
    if let Some(counter) = unsafe { api.this::<Counter>(call) } {
        counter.step = step;
    }
}

static METHODS: [LuvMethod; 2] = [LuvMethod::new(c"Increment", counter_increment, LUV_INLINE), LuvMethod::END];
static PROPERTIES: [LuvProperty; 3] = [
    LuvProperty::new(c"Count", Some(counter_count), None),
    LuvProperty::new(c"Step", Some(counter_step), Some(counter_set_step)),
    LuvProperty::END,
];
static STATICS: [LuvMethod; 2] = [LuvMethod::new(c"new", counter_new, LUV_INLINE), LuvMethod::END];

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luv_register(given: *const LuvApi, registry: *mut LuvRegistry) -> i32 {
    let Some(api) = (unsafe { given.as_ref() }) else {
        return LUV_INVALID;
    };
    if api.version < LUV_API_VERSION {
        return LUV_INVALID;
    }
    API.store(given.cast_mut(), Ordering::Release);
    let info = LuvClassInfo {
        name: c"Counter".as_ptr(),
        size: size_of::<Counter>() as u64,
        destroy: None,
        methods: METHODS.as_ptr(),
        properties: PROPERTIES.as_ptr(),
        statics: STATICS.as_ptr(),
        static_properties: ptr::null(),
    };
    let class = unsafe { (api.define_class)(registry, &info) };
    COUNTER.store(class.cast_mut(), Ordering::Release);
    if class.is_null() { LUV_INVALID } else { LUV_OK }
}
```

Both versions build a library named `counter`, so keep only one of them in a project. The Rust crate also needs a `Cargo.toml`. See [Crate setup](../reference/native-rust.md#crate-setup).

The game uses the class like this:

```luau
local DLL = import("DLL")

local plugin = DLL.Load("./counter")
local Counter = plugin.Exports.Counter
local counter = Counter.new(10, 5)
print(counter:Increment(), counter.Count)
counter.Step = 2
print(counter:Increment(), typeof(counter))
```

This prints `15 15` and then `17 Counter`.

How the parts fit together:

- `push_object` makes a new object, pushes it as a result and returns its data. The data starts as zeros.
- `self_data` gives the data of the object a method or property was called on.
- `check_number` and `opt_number` read arguments. Index 0 is the first argument after `self`.
- `push_number` adds a result. A function can push any number of results.
- `luv_register` must return `LUV_OK`. Otherwise `DLL.Load` fails.

Every function of the API is listed on [Plugin API for C](../reference/native-c.md#luvapi). Operators, static properties and destructors are there too.

## Execution modes and thread safety

The flags of a method or function pick where it runs:

| Mode | Where it runs | Yields in Luau |
| --- | --- | --- |
| `LUV_INLINE` | Right away, on the thread that runs Luau. | no |
| `LUV_WORKER` | On the thread of the library, one call at a time. | yes |
| `LUV_PARALLEL` | On a thread from a pool, at the same time as other calls. | yes |

Use `LUV_INLINE` for short work, like reading a field. Luau waits for it. Use `LUV_WORKER` or `LUV_PARALLEL` for slow work, like loading a file. The calling coroutine then yields, and the rest of the game keeps running. Property functions and operators always run inline. See [Execution modes](../reference/native-c.md#execution-modes).

luv does not lock the data of your objects. A worker method, a parallel method, a render hook and inline code can all use one object at the same time. Guard data that more than one of them use with a lock. The particle system in the pong example locks its data, because its inline methods and its [render hook](render-hooks.md) run on different threads.

## Events back into Luau

A plugin can call a Luau function later, from any thread. Keep the function alive with `retain`, then send events with `begin_event` and `send_event`.

```c title="native/events.c"
#include "luv.h"

static const LuvApi* api;

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
        api->push_string(event, "tick");
        api->send_event(event);
    }
    api->release(handler);
}

static const LuvMethod functions[] = {
    {"countdown", countdown, LUV_WORKER},
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

The Luau side passes a function. The open window keeps the game running while the events come in:

```luau
local DLL = import("DLL")
local Window = import("Window")

Window.new({ Title = "Events" })
local events = DLL.Load("./events")
events.Exports.countdown(function(value: number, word: string)
	print(word, value)
end, 3)
```

This prints `tick 3`, `tick 2` and `tick 1`.

- `retain` only works on tables, functions and other values of kind `LUV_KIND_VALUE`. It returns NULL for anything else.
- Each event runs the function in a new coroutine, on the thread that runs Luau. Events arrive in the order they were sent.
- Call `release` once for each ref when you no longer need it.
- Refs and events do not keep the game running. If nothing else keeps it running, the game can end before the events arrive.

## Loading plugins

Load a plugin with [DLL.Load](../reference/dll.md#load) and leave out the extension:

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
print(mathlib.Path)
```

luv adds `.dll` on Windows. On Linux it adds `.so` and also tries a `lib` in front of the name. So the same line finds `mathlib.dll` and `libmathlib.so`.

With `luv test`, luv looks next to the `luv` program, then in the build folder, then in the project folder. In a packed game it looks next to the game program. The full order for each command is on [DLL.Load](../reference/dll.md#load).

Plugins in a container load the same way. See [Native plugins in a container](containers.md#native-plugins-in-a-container).

## Shipping plugins

luv never packs native libraries into the game file. They sit next to it:

- `luv build` leaves them in the build folder, next to the `.luvit` file. `luv run` finds them there.
- `luv package` copies them into `build/package/`, next to the game program.

```tree
build/package/
├── My-Game.exe
├── counter.dll
└── mathlib.dll
```

| Path | What it is |
| --- | --- |
| `My-Game.exe` | The game program. |
| `counter.dll`, `mathlib.dll` | The plugins built from `native/`. On Linux they are `libcounter.so` and `libmathlib.so`. |

Ship the whole folder. A `.dll` or `.so` file outside `native/` does not ship, and luv warns you about it. A package only runs on the system it was made on, and so do its plugins. See [Shipping your game](shipping.md).

## Errors

| Message | Cause |
| --- | --- |
| `no C compiler was found to build the native library 'mathlib': ...` | luv found no C compiler. |
| `the native library 'mathlib' did not compile:` | The compiler failed. Its output follows the message. |
| ``.../Cargo.toml needs `crate-type = ["cdylib"]` in its [lib] section to build a native library`` | The crate does not build a `cdylib`. |
| `cannot run cargo to build ... install Rust from https://rustup.rs` | Cargo is not installed. |
| `cargo could not build ...` | Cargo failed. Its output is printed above the message. |
| `... and ... both produce a native library named counter.dll` | Two entries make the same file. |
| `luv_register in ... failed with code -4` | `luv_register` did not return `LUV_OK`. |
