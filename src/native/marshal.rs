use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;

use mlua::{AnyUserData, Buffer, Lua, Result, Value};

use super::memory::{Block, Hold, Pointer};
use super::types::{ArrayLayout, CType, StructLayout, WIDE_SIZE};
use crate::datatypes::{Color, UDim};

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

#[derive(Default)]
pub struct Scratch {
    pub blocks: Vec<Block>,
    pub holds: Vec<Hold>,
    pub returns: Vec<(usize, Buffer)>,
}

pub struct Writer<'a> {
    scratch: Option<&'a mut Scratch>,
}

pub unsafe fn wide_unit(base: *const u8, index: usize) -> u32 {
    unsafe {
        let at = base.add(index * WIDE_SIZE);
        if WIDE_SIZE == 2 {
            u32::from(ptr::read_unaligned(at as *const u16))
        } else {
            ptr::read_unaligned(at as *const u32)
        }
    }
}

pub fn decode_wide(units: &[u32]) -> String {
    if WIDE_SIZE == 2 {
        let units: Vec<u16> = units.iter().map(|unit| *unit as u16).collect();
        String::from_utf16_lossy(&units)
    } else {
        units
            .iter()
            .map(|unit| char::from_u32(*unit).unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    }
}

pub fn encode_wide(text: &str) -> Vec<u8> {
    if WIDE_SIZE == 2 {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    } else {
        text.chars().flat_map(|character| (character as u32).to_le_bytes()).collect()
    }
}

pub unsafe fn read_wide_string(address: usize) -> Vec<u32> {
    let mut units = Vec::new();
    loop {
        let unit = unsafe { wide_unit(address as *const u8, units.len()) };
        if unit == 0 {
            return units;
        }
        units.push(unit);
    }
}

fn whole(value: &Value, what: &str) -> Result<i128> {
    match value {
        Value::Integer(number) => Ok(i128::from(*number)),
        Value::Number(number) if number.is_finite() && number.fract() == 0.0 && number.abs() < 1.7e38 => {
            Ok(*number as i128)
        }
        Value::Boolean(flag) => Ok(i128::from(*flag)),
        other => Err(runtime(format!("{what} expects a whole number, got {}", describe(other)))),
    }
}

fn describe(value: &Value) -> String {
    match value {
        Value::Number(number) => format!("{number}"),
        Value::Integer(number) => format!("{number}"),
        other => other.type_name().to_owned(),
    }
}

fn integer<T: TryFrom<i128>>(value: &Value, ty: &CType) -> Result<T> {
    let number = whole(value, &ty.describe())?;
    T::try_from(number).map_err(|_| runtime(format!("{number} does not fit in a {}", ty.describe())))
}

fn number(value: &Value, ty: &CType) -> Result<f64> {
    match value {
        Value::Integer(number) => Ok(*number as f64),
        Value::Number(number) => Ok(*number),
        other => Err(runtime(format!("{} expects a number, got {}", ty.describe(), other.type_name()))),
    }
}

unsafe fn put<T>(dest: *mut u8, value: T) {
    unsafe { ptr::write_unaligned(dest as *mut T, value) };
}

unsafe fn get<T: Copy>(source: *const u8) -> T {
    unsafe { ptr::read_unaligned(source as *const T) }
}

impl Writer<'_> {
    pub fn memory() -> Writer<'static> {
        Writer { scratch: None }
    }

    pub fn call(scratch: &mut Scratch) -> Writer<'_> {
        Writer { scratch: Some(scratch) }
    }

    fn scratch(&mut self, what: &str) -> Result<&mut Scratch> {
        self.scratch.as_deref_mut().ok_or_else(|| {
            runtime(format!(
                "a {what} can only be passed straight into a function call, use DLL.String or DLL.Alloc for memory that has to outlive the call"
            ))
        })
    }

    fn temporary(&mut self, what: &str, block: Option<Block>, copy_back: Option<Buffer>) -> Result<usize> {
        let scratch = self.scratch(what)?;
        let block = block.ok_or_else(|| runtime("the memory could not be allocated"))?;
        let address = block.address();
        if let Some(buffer) = copy_back {
            scratch.returns.push((scratch.blocks.len(), buffer));
        }
        scratch.blocks.push(block);
        Ok(address)
    }

    fn keep(&mut self, pointer: &Pointer) -> Result<usize> {
        let hold = pointer.owner.hold()?;
        if let (Some(scratch), Some(hold)) = (self.scratch.as_deref_mut(), hold) {
            scratch.holds.push(hold);
        }
        Ok(pointer.address)
    }

    fn address(&mut self, value: &Value) -> Result<usize> {
        match value {
            Value::Nil => Ok(0),
            Value::UserData(userdata) => {
                let pointer = Pointer::from_userdata(userdata)?;
                self.keep(&pointer)
            }
            Value::Buffer(buffer) => {
                let block = Block::copied(&buffer.to_vec(), 0);
                self.temporary("buffer", block, Some(buffer.clone()))
            }
            Value::String(text) => {
                let block = Block::copied(&text.as_bytes(), 1);
                self.temporary("string", block, None)
            }
            other => Err(runtime(format!(
                "pointer expects a Pointer, buffer, string or nil, got {}",
                other.type_name()
            ))),
        }
    }

    fn text(&mut self, value: &Value, wide: bool) -> Result<usize> {
        match value {
            Value::String(text) if wide => {
                let text = text.to_str()?;
                let block = Block::copied(&encode_wide(&text), WIDE_SIZE);
                self.temporary("string", block, None)
            }
            Value::String(text) => {
                let block = Block::copied(&text.as_bytes(), 1);
                self.temporary("string", block, None)
            }
            Value::Nil | Value::UserData(_) | Value::Buffer(_) => self.address(value),
            other => Err(runtime(format!(
                "{} expects a string, Pointer or nil, got {}",
                if wide { "wstring" } else { "string" },
                other.type_name()
            ))),
        }
    }

    unsafe fn copy_from(&mut self, userdata: &AnyUserData, size: usize, dest: *mut u8) -> Result<bool> {
        let Ok(pointer) = Pointer::from_userdata(userdata) else {
            return Ok(false);
        };
        let access = pointer.access(0, size)?;
        unsafe { ptr::copy(access.as_ptr(), dest, size) };
        Ok(true)
    }

    unsafe fn structure(&mut self, lua: &Lua, layout: &StructLayout, value: &Value, dest: *mut u8) -> Result<()> {
        unsafe { ptr::write_bytes(dest, 0, layout.size) };
        match value {
            Value::Table(table) => {
                for field in &layout.fields {
                    let item: Value = table.get(field.name.as_str())?;
                    if item.is_nil() {
                        continue;
                    }
                    unsafe { self.write(lua, &field.ty, &item, dest.add(field.offset)) }
                        .map_err(|error| runtime(format!("field '{}': {error}", field.name)))?;
                }
                Ok(())
            }
            Value::UserData(userdata) => {
                let floats = layout.fields.iter().all(|field| field.ty.is_float());
                let components = if let Ok(udim) = userdata.borrow::<UDim>() {
                    Some(vec![udim.x, udim.y, udim.z])
                } else if let Ok(color) = userdata.borrow::<Color>() {
                    Some(vec![color.r, color.g, color.b, color.a])
                } else {
                    None
                };
                if let Some(components) = components {
                    if !floats || layout.fields.len() > components.len() {
                        return Err(runtime(format!(
                            "a UDim or Color only fills structs of up to {} float fields",
                            components.len()
                        )));
                    }
                    for (field, component) in layout.fields.iter().zip(components) {
                        unsafe { self.write(lua, &field.ty, &Value::Number(component), dest.add(field.offset)) }?;
                    }
                    return Ok(());
                }
                if unsafe { self.copy_from(userdata, layout.size, dest) }? {
                    return Ok(());
                }
                Err(runtime("a struct expects a table of fields or a Pointer to copy from"))
            }
            other => Err(runtime(format!(
                "a struct expects a table of fields or a Pointer to copy from, got {}",
                other.type_name()
            ))),
        }
    }

    unsafe fn array(&mut self, lua: &Lua, layout: &ArrayLayout, value: &Value, dest: *mut u8) -> Result<()> {
        let stride = layout.element.size();
        let size = stride * layout.length;
        unsafe { ptr::write_bytes(dest, 0, size) };
        match value {
            Value::Table(table) => {
                for index in 0..layout.length {
                    let item: Value = table.raw_get(index + 1)?;
                    if item.is_nil() {
                        continue;
                    }
                    unsafe { self.write(lua, &layout.element, &item, dest.add(index * stride)) }
                        .map_err(|error| runtime(format!("element #{}: {error}", index + 1)))?;
                }
                Ok(())
            }
            Value::String(_) | Value::Buffer(_) if stride == 1 => {
                let bytes = super::memory::raw_bytes(value, "the array")?;
                let count = bytes.len().min(size);
                unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), dest, count) };
                Ok(())
            }
            Value::UserData(userdata) => {
                if unsafe { self.copy_from(userdata, size, dest) }? {
                    return Ok(());
                }
                Err(runtime("an array expects a table of elements or a Pointer to copy from"))
            }
            other => Err(runtime(format!(
                "an array expects a table of elements, got {}",
                other.type_name()
            ))),
        }
    }

    pub unsafe fn write(&mut self, lua: &Lua, ty: &CType, value: &Value, dest: *mut u8) -> Result<()> {
        unsafe {
            match ty {
                CType::Void => return Err(runtime("void has no value to write")),
                CType::Bool => match value {
                    Value::Boolean(flag) => put(dest, u8::from(*flag)),
                    Value::Nil => put(dest, 0u8),
                    other => return Err(runtime(format!("bool expects a boolean, got {}", other.type_name()))),
                },
                CType::I8 => put(dest, integer::<i8>(value, ty)?),
                CType::U8 => put(dest, integer::<u8>(value, ty)?),
                CType::I16 => put(dest, integer::<i16>(value, ty)?),
                CType::U16 => put(dest, integer::<u16>(value, ty)?),
                CType::I32 => put(dest, integer::<i32>(value, ty)?),
                CType::U32 => put(dest, integer::<u32>(value, ty)?),
                CType::I64 => put(dest, integer::<i64>(value, ty)?),
                CType::U64 => put(dest, integer::<u64>(value, ty)?),
                CType::ISize => put(dest, integer::<isize>(value, ty)?),
                CType::USize => put(dest, integer::<usize>(value, ty)?),
                CType::F32 => put(dest, number(value, ty)? as f32),
                CType::F64 => put(dest, number(value, ty)?),
                CType::Pointer => put(dest, self.address(value)?),
                CType::String => put(dest, self.text(value, false)?),
                CType::WString => put(dest, self.text(value, true)?),
                CType::Struct(layout) => self.structure(lua, layout, value, dest)?,
                CType::Array(layout) => self.array(lua, layout, value, dest)?,
            }
        }
        Ok(())
    }
}

pub unsafe fn read(lua: &Lua, ty: &CType, source: *const u8) -> Result<Value> {
    unsafe {
        Ok(match ty {
            CType::Void => Value::Nil,
            CType::Bool => Value::Boolean(get::<u8>(source) != 0),
            CType::I8 => Value::Number(f64::from(get::<i8>(source))),
            CType::U8 => Value::Number(f64::from(get::<u8>(source))),
            CType::I16 => Value::Number(f64::from(get::<i16>(source))),
            CType::U16 => Value::Number(f64::from(get::<u16>(source))),
            CType::I32 => Value::Number(f64::from(get::<i32>(source))),
            CType::U32 => Value::Number(f64::from(get::<u32>(source))),
            CType::I64 => Value::Number(get::<i64>(source) as f64),
            CType::U64 => Value::Number(get::<u64>(source) as f64),
            CType::ISize => Value::Number(get::<isize>(source) as f64),
            CType::USize => Value::Number(get::<usize>(source) as f64),
            CType::F32 => Value::Number(f64::from(get::<f32>(source))),
            CType::F64 => Value::Number(get::<f64>(source)),
            CType::Pointer => match get::<usize>(source) {
                0 => Value::Nil,
                address => Value::UserData(lua.create_userdata(Pointer::foreign(address))?),
            },
            CType::String => match get::<usize>(source) {
                0 => Value::Nil,
                address => Value::String(lua.create_string(CStr::from_ptr(address as *const c_char).to_bytes())?),
            },
            CType::WString => match get::<usize>(source) {
                0 => Value::Nil,
                address => Value::String(lua.create_string(decode_wide(&read_wide_string(address)))?),
            },
            CType::Struct(layout) => {
                let table = lua.create_table_with_capacity(0, layout.fields.len())?;
                for field in &layout.fields {
                    table.raw_set(field.name.as_str(), read(lua, &field.ty, source.add(field.offset))?)?;
                }
                Value::Table(table)
            }
            CType::Array(layout) => {
                let table = lua.create_table_with_capacity(layout.length, 0)?;
                let stride = layout.element.size();
                for index in 0..layout.length {
                    table.raw_set(index + 1, read(lua, &layout.element, source.add(index * stride))?)?;
                }
                Value::Table(table)
            }
        })
    }
}
