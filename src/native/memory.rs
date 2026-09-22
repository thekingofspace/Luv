use std::alloc::{self, Layout};
use std::any::Any;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr::{self, NonNull};
use std::sync::{Arc, PoisonError, RwLock};

use mlua::{
    AnyUserData, FromLua, Lua, LuaString, MetaMethod, Result, UserData, UserDataFields, UserDataMethods, Value,
};

use super::callback::{Callback, Stub};
use super::classes::{self, ObjectData};
use super::library::{LibraryShared, NativeFunction};
use super::marshal::{self, Writer};
use super::types::{CType, WIDE_SIZE};

pub const ALIGNMENT: usize = 16;

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

pub struct Block {
    pointer: NonNull<u8>,
    layout: Layout,
    size: usize,
}

unsafe impl Send for Block {}
unsafe impl Sync for Block {}

impl Block {
    pub fn zeroed(size: usize) -> Option<Block> {
        let layout = Layout::from_size_align(size.max(1), ALIGNMENT).ok()?;
        let pointer = NonNull::new(unsafe { alloc::alloc_zeroed(layout) })?;
        Some(Block { pointer, layout, size })
    }

    pub fn copied(bytes: &[u8], terminator: usize) -> Option<Block> {
        let block = Block::zeroed(bytes.len() + terminator)?;
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), block.as_ptr(), bytes.len()) };
        Some(block)
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.pointer.as_ptr()
    }

    pub fn address(&self) -> usize {
        self.pointer.as_ptr() as usize
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.as_ptr(), self.size) }
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        unsafe { alloc::dealloc(self.pointer.as_ptr(), self.layout) };
    }
}

pub struct Allocation {
    block: RwLock<Option<Arc<Block>>>,
    base: usize,
    size: usize,
}

impl Allocation {
    pub fn new(block: Block) -> Arc<Allocation> {
        Arc::new(Allocation {
            base: block.address(),
            size: block.len(),
            block: RwLock::new(Some(Arc::new(block))),
        })
    }

    pub fn live(&self) -> Option<Arc<Block>> {
        self.block.read().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn free(&self) -> bool {
        self.block.write().unwrap_or_else(PoisonError::into_inner).take().is_some()
    }
}

#[derive(Clone)]
pub enum Owner {
    Foreign,
    Memory(Arc<Allocation>),
    Library(Arc<LibraryShared>),
    Callback(Arc<Stub>),
    Object(Arc<ObjectData>),
}

#[derive(Clone)]
pub enum Hold {
    Block(Arc<Block>),
    Library(Arc<libloading::Library>),
    Callback(Arc<Stub>),
    Object(Arc<ObjectData>),
}

impl Hold {
    pub fn erase(self) -> Arc<dyn Any + Send + Sync> {
        match self {
            Hold::Block(block) => block,
            Hold::Library(library) => library,
            Hold::Callback(stub) => stub,
            Hold::Object(object) => object,
        }
    }
}

fn freed() -> mlua::Error {
    runtime("the memory behind this Pointer has been freed")
}

impl Owner {
    pub fn hold(&self) -> Result<Option<Hold>> {
        match self {
            Owner::Foreign => Ok(None),
            Owner::Memory(allocation) => allocation.live().map(|block| Some(Hold::Block(block))).ok_or_else(freed),
            Owner::Library(library) => Ok(Some(Hold::Library(library.library.clone()))),
            Owner::Callback(stub) => Ok(Some(Hold::Callback(stub.clone()))),
            Owner::Object(object) => Ok(Some(Hold::Object(object.clone()))),
        }
    }
}

#[derive(Clone)]
pub struct Pointer {
    pub address: usize,
    pub owner: Owner,
}

pub struct Access {
    pub address: usize,
    pub limit: Option<usize>,
    _hold: Option<Arc<Block>>,
}

impl Access {
    pub fn as_ptr(&self) -> *mut u8 {
        self.address as *mut u8
    }
}

fn offset_of(value: Option<f64>) -> Result<isize> {
    let offset = value.unwrap_or(0.0);
    if offset.is_finite() && offset.fract() == 0.0 && offset.abs() <= isize::MAX as f64 {
        Ok(offset as isize)
    } else {
        Err(runtime(format!("an offset must be a whole number, got {offset}")))
    }
}

fn count_of(what: &str, value: f64) -> Result<usize> {
    if value.is_finite() && value.fract() == 0.0 && value >= 0.0 && value <= isize::MAX as f64 {
        Ok(value as usize)
    } else {
        Err(runtime(format!("{what} must be a whole number of at least 0, got {value}")))
    }
}

impl Pointer {
    pub const TYPE_NAME: &'static str = "Pointer";

    pub fn foreign(address: usize) -> Pointer {
        Pointer {
            address,
            owner: Owner::Foreign,
        }
    }

    pub fn owned(block: Block) -> Pointer {
        Pointer {
            address: block.address(),
            owner: Owner::Memory(Allocation::new(block)),
        }
    }

    pub fn zeroed(size: usize) -> Option<Pointer> {
        Block::zeroed(size).map(Pointer::owned)
    }

    pub fn string(bytes: &[u8]) -> Option<Pointer> {
        Block::copied(bytes, 1).map(Pointer::owned)
    }

    pub fn wide_string(text: &str) -> Option<Pointer> {
        Block::copied(&marshal::encode_wide(text), WIDE_SIZE).map(Pointer::owned)
    }

    pub fn from_value(value: &Value) -> Result<Option<Pointer>> {
        match value {
            Value::Nil => Ok(None),
            Value::UserData(userdata) => Pointer::from_userdata(userdata).map(Some),
            other => Err(runtime(format!("expected a Pointer, got {}", other.type_name()))),
        }
    }

    pub fn from_userdata(userdata: &AnyUserData) -> Result<Pointer> {
        if let Ok(pointer) = userdata.borrow::<Pointer>() {
            return Ok(pointer.clone());
        }
        if let Some(object) = classes::object_of(userdata) {
            return Ok(Pointer {
                address: object.address(),
                owner: Owner::Object(object),
            });
        }
        if let Ok(callback) = userdata.borrow::<Callback>() {
            return callback.pointer();
        }
        if let Ok(function) = userdata.borrow::<NativeFunction>() {
            return Ok(function.pointer());
        }
        Err(runtime("expected a Pointer, Callback, NativeFunction or native object"))
    }

    pub fn access(&self, offset: isize, length: usize) -> Result<Access> {
        let address = self
            .address
            .checked_add_signed(offset)
            .ok_or_else(|| runtime("the offset moves the Pointer outside of memory"))?;
        if address == 0 {
            return Err(runtime("cannot access memory through a null Pointer"));
        }
        match &self.owner {
            Owner::Memory(allocation) => {
                let block = allocation.live().ok_or_else(freed)?;
                let end = allocation.base + allocation.size;
                if address < allocation.base || address.checked_add(length).is_none_or(|last| last > end) {
                    return Err(runtime(format!(
                        "accessing {length} bytes at offset {} is outside the {} bytes of this memory",
                        address as isize - allocation.base as isize,
                        allocation.size
                    )));
                }
                Ok(Access {
                    address,
                    limit: Some(end - address),
                    _hold: Some(block),
                })
            }
            Owner::Object(object) => {
                let end = object.address() + object.size();
                if address < object.address() || address.checked_add(length).is_none_or(|last| last > end) {
                    return Err(runtime(format!(
                        "accessing {length} bytes at offset {} is outside the {} bytes of this {}",
                        address as isize - object.address() as isize,
                        object.size(),
                        object.class_name()
                    )));
                }
                Ok(Access {
                    address,
                    limit: Some(end - address),
                    _hold: None,
                })
            }
            _ => Ok(Access {
                address,
                limit: None,
                _hold: None,
            }),
        }
    }

    pub fn remaining(&self) -> Option<usize> {
        match &self.owner {
            Owner::Memory(allocation) => {
                allocation.live()?;
                (allocation.base + allocation.size).checked_sub(self.address)
            }
            Owner::Object(object) => (object.address() + object.size()).checked_sub(self.address),
            _ => None,
        }
    }

    pub fn offset(&self, bytes: isize) -> Result<Pointer> {
        let address = self
            .address
            .checked_add_signed(bytes)
            .ok_or_else(|| runtime("the offset moves the Pointer outside of memory"))?;
        Ok(Pointer {
            address,
            owner: self.owner.clone(),
        })
    }

    fn read_string(&self, lua: &Lua, length: Option<f64>, offset: Option<f64>) -> Result<Value> {
        let offset = offset_of(offset)?;
        match length {
            Some(length) => {
                let length = count_of("the length", length)?;
                let access = self.access(offset, length)?;
                let bytes = unsafe { std::slice::from_raw_parts(access.as_ptr(), length) };
                Ok(Value::String(lua.create_string(bytes)?))
            }
            None => {
                let access = self.access(offset, 0)?;
                let bytes = match access.limit {
                    Some(limit) => {
                        let bytes = unsafe { std::slice::from_raw_parts(access.as_ptr(), limit) };
                        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(limit);
                        &bytes[..end]
                    }
                    None => unsafe { CStr::from_ptr(access.as_ptr() as *const c_char) }.to_bytes(),
                };
                Ok(Value::String(lua.create_string(bytes)?))
            }
        }
    }

    fn read_wide(&self, lua: &Lua, length: Option<f64>, offset: Option<f64>) -> Result<LuaString> {
        let offset = offset_of(offset)?;
        let units = match length {
            Some(length) => {
                let length = count_of("the length", length)?;
                let access = self.access(offset, length * WIDE_SIZE)?;
                (0..length).map(|index| unsafe { marshal::wide_unit(access.as_ptr(), index) }).collect()
            }
            None => {
                let access = self.access(offset, 0)?;
                let limit = access.limit.map(|limit| limit / WIDE_SIZE);
                let mut units = Vec::new();
                loop {
                    if limit.is_some_and(|limit| units.len() >= limit) {
                        break;
                    }
                    let unit = unsafe { marshal::wide_unit(access.as_ptr(), units.len()) };
                    if unit == 0 {
                        break;
                    }
                    units.push(unit);
                }
                units
            }
        };
        lua.create_string(marshal::decode_wide(&units))
    }

    fn write_bytes(&self, bytes: &[u8], offset: Option<f64>, terminator: usize) -> Result<()> {
        let access = self.access(offset_of(offset)?, bytes.len() + terminator)?;
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), access.as_ptr(), bytes.len());
            ptr::write_bytes(access.as_ptr().add(bytes.len()), 0, terminator);
        }
        Ok(())
    }
}

pub fn raw_bytes(value: &Value, what: &str) -> Result<Vec<u8>> {
    match value {
        Value::String(text) => Ok(text.as_bytes().to_vec()),
        Value::Buffer(buffer) => Ok(buffer.to_vec()),
        other => Err(runtime(format!("{what} must be a string or buffer, got {}", other.type_name()))),
    }
}

impl FromLua for Pointer {
    fn from_lua(value: Value, _: &Lua) -> Result<Self> {
        match value {
            Value::UserData(userdata) => Pointer::from_userdata(&userdata),
            other => Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: Self::TYPE_NAME.to_owned(),
                message: None,
            }),
        }
    }
}

impl UserData for Pointer {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::TYPE_NAME);
        fields.add_field_method_get("Address", |_, this| Ok(this.address as f64));
        fields.add_field_method_get("IsNull", |_, this| Ok(this.address == 0));
        fields.add_field_method_get("Size", |_, this| Ok(this.remaining()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Read", |lua, this, (ty, offset): (Value, Option<f64>)| {
            let ty = CType::parse_value(&ty)?;
            let access = this.access(offset_of(offset)?, ty.size())?;
            unsafe { marshal::read(lua, &ty, access.as_ptr()) }
        });
        methods.add_method("Write", |lua, this, (ty, value, offset): (Value, Value, Option<f64>)| {
            let ty = CType::parse_value(&ty)?;
            let access = this.access(offset_of(offset)?, ty.size())?;
            unsafe { Writer::memory().write(lua, &ty, &value, access.as_ptr()) }
        });
        methods.add_method("ReadString", |lua, this, (length, offset): (Option<f64>, Option<f64>)| {
            this.read_string(lua, length, offset)
        });
        methods.add_method("ReadWideString", |lua, this, (length, offset): (Option<f64>, Option<f64>)| {
            this.read_wide(lua, length, offset)
        });
        methods.add_method("WriteString", |_, this, (text, offset): (LuaString, Option<f64>)| {
            this.write_bytes(&text.as_bytes(), offset, 1)
        });
        methods.add_method("WriteWideString", |_, this, (text, offset): (String, Option<f64>)| {
            let bytes = marshal::encode_wide(&text);
            this.write_bytes(&bytes, offset, WIDE_SIZE)
        });
        methods.add_method("ReadBuffer", |lua, this, (length, offset): (f64, Option<f64>)| {
            let length = count_of("the length", length)?;
            let access = this.access(offset_of(offset)?, length)?;
            lua.create_buffer(unsafe { std::slice::from_raw_parts(access.as_ptr(), length) })
        });
        methods.add_method("WriteBuffer", |_, this, (data, offset): (Value, Option<f64>)| {
            let bytes = raw_bytes(&data, "the data")?;
            this.write_bytes(&bytes, offset, 0)
        });
        methods.add_method("Offset", |_, this, bytes: f64| this.offset(offset_of(Some(bytes))?));
        methods.add_method("Free", |_, this, ()| match &this.owner {
            Owner::Memory(allocation) => {
                if allocation.base != this.address {
                    return Err(runtime("only the Pointer returned by DLL.Alloc, DLL.New or DLL.String can free its memory"));
                }
                if !allocation.free() {
                    return Err(freed());
                }
                Ok(())
            }
            _ => Err(runtime("only memory from DLL.Alloc, DLL.New or DLL.String can be freed")),
        });
        methods.add_meta_function(MetaMethod::Eq, |_, (left, right): (Pointer, Pointer)| {
            Ok(left.address == right.address)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(if this.address == 0 {
                format!("{}(null)", Self::TYPE_NAME)
            } else {
                format!("{}(0x{:x})", Self::TYPE_NAME, this.address)
            })
        });
    }
}
