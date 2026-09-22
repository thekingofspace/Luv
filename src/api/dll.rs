use std::ptr;

use mlua::{Function, Lua, Result, Table, Value};

use crate::native::{
    ArrayType, CType, Callback, EXTENSION, Library, NativeFunction, Pointer, StructType, allocate,
};

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

fn size(what: &str, value: f64) -> Result<usize> {
    if value.is_finite() && value.fract() == 0.0 && value >= 0.0 && value <= isize::MAX as f64 {
        Ok(value as usize)
    } else {
        Err(runtime(format!("{what} must be a whole number of at least 0, got {value}")))
    }
}

pub fn create(lua: &Lua) -> Result<Table> {
    let dll = lua.create_table()?;

    dll.set("Load", lua.create_async_function(|lua, path: String| Library::load(lua, path))?)?;
    dll.set(
        "Function",
        lua.create_function(
            |lua, (pointer, result, arguments, options): (Pointer, Value, Option<Table>, Option<Table>)| {
                let name = format!("function at 0x{:x}", pointer.address);
                NativeFunction::create(lua, name, pointer, &result, arguments, options)
            },
        )?,
    )?;
    dll.set(
        "Callback",
        lua.create_function(|lua, (result, arguments, handler): (Value, Option<Table>, Function)| {
            Callback::create(lua, &result, arguments, handler)
        })?,
    )?;
    dll.set("Struct", lua.create_function(|_, fields: Table| StructType::define(fields))?)?;
    dll.set(
        "Array",
        lua.create_function(|_, (element, length): (Value, f64)| ArrayType::define(&element, length))?,
    )?;
    dll.set(
        "Alloc",
        lua.create_function(|_, bytes: f64| {
            let bytes = size("the size", bytes)?;
            crate::native::Pointer::zeroed(bytes).ok_or_else(|| runtime("the memory could not be allocated"))
        })?,
    )?;
    dll.set(
        "New",
        lua.create_function(|lua, (ty, value): (Value, Option<Value>)| allocate(lua, &CType::parse_value(&ty)?, value))?,
    )?;
    dll.set(
        "String",
        lua.create_function(|_, (text, wide): (mlua::LuaString, Option<bool>)| {
            let pointer = if wide.unwrap_or(false) {
                Pointer::wide_string(&text.to_str()?)
            } else {
                Pointer::string(&text.as_bytes())
            };
            pointer.ok_or_else(|| runtime("the memory could not be allocated"))
        })?,
    )?;
    dll.set(
        "Pointer",
        lua.create_function(|_, address: f64| {
            if !(address.is_finite() && address.fract() == 0.0 && address >= 0.0 && address <= usize::MAX as f64) {
                return Err(runtime(format!("an address must be a whole number of at least 0, got {address}")));
            }
            Ok(Pointer::foreign(address as usize))
        })?,
    )?;
    dll.set("Null", Pointer::foreign(0))?;
    dll.set(
        "SizeOf",
        lua.create_function(|_, ty: Value| Ok(CType::parse_value(&ty)?.size()))?,
    )?;
    dll.set(
        "AlignOf",
        lua.create_function(|_, ty: Value| Ok(CType::parse_value(&ty)?.align()))?,
    )?;
    dll.set(
        "Copy",
        lua.create_function(|_, (destination, source, bytes): (Pointer, Pointer, f64)| {
            let bytes = size("the size", bytes)?;
            let to = destination.access(0, bytes)?;
            let from = source.access(0, bytes)?;
            unsafe { ptr::copy(from.as_ptr(), to.as_ptr(), bytes) };
            Ok(())
        })?,
    )?;
    dll.set(
        "Fill",
        lua.create_function(|_, (destination, value, bytes): (Pointer, f64, f64)| {
            let bytes = size("the size", bytes)?;
            if !(value.fract() == 0.0 && (0.0..=255.0).contains(&value)) {
                return Err(runtime(format!("the fill value must be a byte from 0 to 255, got {value}")));
            }
            let to = destination.access(0, bytes)?;
            unsafe { ptr::write_bytes(to.as_ptr(), value as u8, bytes) };
            Ok(())
        })?,
    )?;
    dll.set("Extension", format!(".{EXTENSION}"))?;

    dll.set_readonly(true);
    Ok(dll)
}
