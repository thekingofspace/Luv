# Plugin API for Rust

The Rust side of native plugins, from `native/luv.rs`.

## Description

`luv init` writes `native/luv.rs` next to `native/luv.h`. It has the same types, constants and functions as the C header, plus a few helpers. It only uses the Rust standard library.

Everything that works the same as in C is explained on [Plugin API for C](native-c.md). That includes `luv_register`, the execution modes, every `LuvApi` function and the render context. This page covers what is different in Rust. For a guide, see [Native plugins](../manual/native-plugins.md).

## Crate setup

A Rust plugin is a crate in its own folder inside `native`.

```tree
my-game/
├── build.toml
└── native/
    ├── luv.h
    ├── luv.rs
    └── counter/
        ├── Cargo.toml
        └── src/
            └── lib.rs
```

| Path | What it is |
| --- | --- |
| `native/luv.rs` | The plugin API for Rust. `luv init` keeps it up to date, so do not edit it. |
| `native/counter/` | The crate. luv treats any folder in `native` with a `Cargo.toml` as a Rust plugin. |
| `native/counter/Cargo.toml` | The crate manifest. It must build a `cdylib`. |
| `native/counter/src/lib.rs` | The code of the plugin. |

```toml
[package]
name = "counter"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[profile.release]
panic = "abort"

[workspace]
```

- `crate-type = ["cdylib"]` is required. Without it the build stops with ``native/counter/Cargo.toml needs `crate-type = ["cdylib"]` in its [lib] section to build a native library``.
- The empty `[workspace]` table keeps the crate out of any Cargo workspace in a parent folder.
- The library is named after `name` in `[lib]`, or after the package name. Each `-` becomes `_`. So this crate builds `counter.dll` on Windows and `libcounter.so` on Linux.

Each time luv builds native plugins, it runs Cargo for every crate:

```shell
cargo build --release --manifest-path native/counter/Cargo.toml --target-dir build/native-target
```

Cargo decides what to rebuild. luv then copies the library into the build folder when it changed. You need Rust installed. Without Cargo the build stops with `cannot run cargo to build ..., install Rust from https://rustup.rs`.

## Including luv.rs

Add `luv.rs` as a module with a `#[path]` attribute:

```rust
#[path = "../../luv.rs"]
mod luv;

use luv::*;
```

The path is relative to `src/lib.rs`, so `../../luv.rs` is `native/luv.rs`. The file allows dead code, so the parts you do not use give no warnings.

## Differences from C

Function pointers that can be NULL are `Option` fields in Rust:

| Struct | Field | Rust type |
| --- | --- | --- |
| `LuvMethod` | `function` | `Option<LuvFunction>` |
| `LuvProperty` | `get`, `set` | `Option<LuvFunction>` |
| `LuvClassInfo` | `destroy` | `Option<LuvDestroy>` |
| `LuvVulkan` | `get_instance_proc_addr` | `Option<LuvGetInstanceProcAddr>` |

Everything else matches `luv.h`:

- The fields of `LuvApi` are `unsafe extern "C" fn` pointers. Call them inside `unsafe`, with parentheses around the field, like `unsafe { (api.push_number)(call, 1.0) }`.
- `LuvCall`, `LuvClass`, `LuvRegistry` and `LuvRef` have no fields. You only use them through raw pointers.
- The type aliases are `LuvFunction`, `LuvDestroy`, `LuvRegister`, `LuvRenderHook`, `LuvWriteData`, `LuvWriteTexture`, `LuvSetDrawCounts` and `LuvGetInstanceProcAddr`.
- The mode constants like `LUV_INLINE` are `u32`. The result codes and argument kinds are `i32`.
- Export your functions with `#[unsafe(no_mangle)]` and `extern "C"`.

## Method and property lists

| Item | What it does |
| --- | --- |
| `LuvMethod::new(name, function, flags)` | Makes a method entry. `name` is a `&'static CStr`, like `c"Increment"`. |
| `LuvMethod::END` | The empty entry that ends a method list. |
| `LuvProperty::new(name, get, set)` | Makes a property entry. `get` and `set` are `Option<LuvFunction>`. |
| `LuvProperty::END` | The empty entry that ends a property list. |

Both `new` functions are `const fn`, and both types are `Sync`. So the lists can be `static` arrays:

```rust
static METHODS: [LuvMethod; 2] = [LuvMethod::new(c"Increment", counter_increment, LUV_INLINE), LuvMethod::END];
static PROPERTIES: [LuvProperty; 2] = [LuvProperty::new(c"Count", Some(counter_count), None), LuvProperty::END];
```

## Helpers on LuvApi

Each of these is an `unsafe fn`.

| Helper | Returns | Description |
| --- | --- | --- |
| `this::<T>(call)` | `Option<&mut T>` | `self_data` as your type. `None` outside of methods and property functions. |
| `object::<T>(call, index, class)` | `Option<&mut T>` | `check_object` as your type. Records an error when the argument is wrong. |
| `try_object::<T>(call, index, class)` | `Option<&mut T>` | `to_object` as your type. Records no error. |
| `push_new(call, class, value)` | `bool` | Calls `push_object` and moves `value` into the new object. Returns `false` when `push_object` failed. |
| `check_str(call, index)` | `Option<&str>` | `check_string` as a `&str`. Records an error and returns `None` when the argument is not a string. Returns `None` without an error when the text is not UTF-8. |
| `push_str(call, text)` | nothing | Pushes a `&str` with `push_bytes`. |
| `fail_with(call, message)` | nothing | Calls `fail` with a `&str`. Zero bytes in it are removed. |
| `log(message)` | nothing | Calls `print` with a `&str`. Zero bytes in it are removed. |

## Helpers on LuvRenderContext

| Helper | Returns | Description |
| --- | --- | --- |
| `user_data::<T>()` | `Option<&T>` | `user_data` as a shared reference to your type. `None` when it is NULL. |
| `write_data(name, offset, data)` | `i32` | Calls `write_data` with a `&CStr` name and a slice of any `Copy` type. The length is the byte size of the slice. |
| `write_texture(name, width, height, rgba)` | `i32` | Calls `write_texture` with a byte slice. Returns `LUV_INVALID` when the slice is shorter than `width * height * 4`. |
| `set_draw_counts(vertex_count, instance_count)` | nothing | Calls `set_draw_counts`. |
| `vulkan()` | `Option<&LuvVulkan>` | The Vulkan handles, or `None`. |

`user_data` only gives a shared reference. To change the data, cast the `user_data` field yourself, like `context.user_data.cast::<Settings>()`.

```rust
#[unsafe(no_mangle)]
pub extern "C" fn tint_render(context: *mut LuvRenderContext) {
    let Some(context) = (unsafe { context.as_mut() }) else {
        return;
    };
    let pulse = (context.time as f32).sin() * 0.5 + 0.5;
    context.write_data(c"tint", 0, &[pulse, 0.2, 1.0 - pulse, 1.0]);
}
```

See [Render hooks](../manual/render-hooks.md) for a full example.

## Rust data in objects

luv frees the memory of an object after `destroy` runs. It never runs `Drop` for your type. When your type owns heap data, like a `String` or a `Vec`, drop it in `destroy`:

```rust
use std::ffi::c_void;
use std::ptr;

struct Player {
    name: String,
    score: f64,
}

unsafe extern "C" fn player_destroy(data: *mut c_void) {
    unsafe { ptr::drop_in_place(data.cast::<Player>()) };
}
```

Then set `destroy: Some(player_destroy)` in the `LuvClassInfo`. The data of an object is aligned to 16 bytes.

## A full example

This plugin makes a `Counter` class with a static `new`, a method and two properties. It keeps the `LuvApi` and the class in atomics so every function can reach them.

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

The game uses it like any other plugin:

```luau
local DLL = import("DLL")

local plugin = DLL.Load("./counter")
local Counter = plugin.Exports.Counter
local counter = Counter.new(10, 5)
print(counter:Increment(), counter.Count)
counter.Step = 2
print(counter:Increment())
```

This prints `15 15` and then `17`.
