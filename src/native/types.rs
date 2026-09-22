use std::collections::HashSet;
use std::sync::Arc;

use libffi::middle::Type;
use mlua::{AnyUserData, FromLua, Lua, MetaMethod, Result, Table, UserData, UserDataFields, UserDataMethods, Value};

use super::marshal::Writer;
use super::memory::{Block, Pointer};

pub const POINTER_SIZE: usize = size_of::<usize>();
#[cfg(windows)]
pub const WIDE_SIZE: usize = 2;
#[cfg(not(windows))]
pub const WIDE_SIZE: usize = 4;
#[cfg(windows)]
const LONG_SIZE: usize = 4;
#[cfg(not(windows))]
const LONG_SIZE: usize = 8;
const CHAR_SIGNED: bool = !cfg!(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "arm")));

pub const TYPE_NAMES: &str = "void, bool, i8, u8, i16, u16, i32, u32, i64, u64, isize, usize, f32, f64, pointer, \
                              string, wstring, char, uchar, short, ushort, int, uint, long, ulong, longlong, \
                              ulonglong, float, double, size_t, ssize_t, intptr_t or uintptr_t";

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

#[derive(Clone, Debug, PartialEq)]
pub enum CType {
    Void,
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    ISize,
    USize,
    F32,
    F64,
    Pointer,
    String,
    WString,
    Struct(Arc<StructLayout>),
    Array(Arc<ArrayLayout>),
}

#[derive(Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: CType,
    pub offset: usize,
}

#[derive(Debug, PartialEq)]
pub struct StructLayout {
    pub fields: Vec<Field>,
    pub size: usize,
    pub align: usize,
}

#[derive(Debug, PartialEq)]
pub struct ArrayLayout {
    pub element: CType,
    pub length: usize,
}

pub fn align_up(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

fn sized(size: usize, signed: bool) -> CType {
    match (size, signed) {
        (4, true) => CType::I32,
        (4, false) => CType::U32,
        (_, true) => CType::I64,
        (_, false) => CType::U64,
    }
}

impl CType {
    pub fn named(name: &str) -> Option<CType> {
        Some(match name {
            "void" => CType::Void,
            "bool" => CType::Bool,
            "i8" => CType::I8,
            "u8" | "uchar" => CType::U8,
            "char" if CHAR_SIGNED => CType::I8,
            "char" => CType::U8,
            "i16" | "short" => CType::I16,
            "u16" | "ushort" => CType::U16,
            "i32" | "int" => CType::I32,
            "u32" | "uint" => CType::U32,
            "i64" | "longlong" => CType::I64,
            "u64" | "ulonglong" => CType::U64,
            "long" => sized(LONG_SIZE, true),
            "ulong" => sized(LONG_SIZE, false),
            "isize" | "ssize_t" | "intptr_t" => CType::ISize,
            "usize" | "size_t" | "uintptr_t" => CType::USize,
            "f32" | "float" => CType::F32,
            "f64" | "double" => CType::F64,
            "pointer" => CType::Pointer,
            "string" => CType::String,
            "wstring" => CType::WString,
            _ => return None,
        })
    }

    pub fn size(&self) -> usize {
        match self {
            CType::Void => 0,
            CType::Bool | CType::I8 | CType::U8 => 1,
            CType::I16 | CType::U16 => 2,
            CType::I32 | CType::U32 | CType::F32 => 4,
            CType::I64 | CType::U64 | CType::F64 => 8,
            CType::ISize | CType::USize | CType::Pointer | CType::String | CType::WString => POINTER_SIZE,
            CType::Struct(layout) => layout.size,
            CType::Array(layout) => layout.element.size() * layout.length,
        }
    }

    pub fn align(&self) -> usize {
        match self {
            CType::Void => 1,
            CType::Struct(layout) => layout.align,
            CType::Array(layout) => layout.element.align(),
            other => other.size(),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            CType::Void => "void".to_owned(),
            CType::Bool => "bool".to_owned(),
            CType::I8 => "i8".to_owned(),
            CType::U8 => "u8".to_owned(),
            CType::I16 => "i16".to_owned(),
            CType::U16 => "u16".to_owned(),
            CType::I32 => "i32".to_owned(),
            CType::U32 => "u32".to_owned(),
            CType::I64 => "i64".to_owned(),
            CType::U64 => "u64".to_owned(),
            CType::ISize => "isize".to_owned(),
            CType::USize => "usize".to_owned(),
            CType::F32 => "f32".to_owned(),
            CType::F64 => "f64".to_owned(),
            CType::Pointer => "pointer".to_owned(),
            CType::String => "string".to_owned(),
            CType::WString => "wstring".to_owned(),
            CType::Struct(layout) => format!("struct of {} bytes", layout.size),
            CType::Array(layout) => format!("{}[{}]", layout.element.describe(), layout.length),
        }
    }

    pub fn is_float(&self) -> bool {
        matches!(self, CType::F32 | CType::F64)
    }

    pub fn ffi(&self) -> Type {
        match self {
            CType::Void => Type::void(),
            CType::Bool | CType::U8 => Type::u8(),
            CType::I8 => Type::i8(),
            CType::I16 => Type::i16(),
            CType::U16 => Type::u16(),
            CType::I32 => Type::i32(),
            CType::U32 => Type::u32(),
            CType::I64 => Type::i64(),
            CType::U64 => Type::u64(),
            CType::ISize => Type::isize(),
            CType::USize => Type::usize(),
            CType::F32 => Type::f32(),
            CType::F64 => Type::f64(),
            CType::Pointer | CType::String | CType::WString => Type::pointer(),
            CType::Struct(layout) => Type::structure(layout.fields.iter().map(|field| field.ty.ffi()).collect::<Vec<_>>()),
            CType::Array(layout) => Type::structure((0..layout.length).map(|_| layout.element.ffi()).collect::<Vec<_>>()),
        }
    }

    pub fn parse(value: &Value) -> Result<CType> {
        match value {
            Value::String(name) => {
                let name = name.to_str()?;
                CType::named(&name).ok_or_else(|| {
                    runtime(format!("'{name}' is not a DLL type, the types are {TYPE_NAMES}, or a StructType or ArrayType"))
                })
            }
            Value::UserData(userdata) => {
                if let Ok(structure) = userdata.borrow::<StructType>() {
                    return Ok(CType::Struct(structure.0.clone()));
                }
                if let Ok(array) = userdata.borrow::<ArrayType>() {
                    return Ok(CType::Array(array.0.clone()));
                }
                Err(runtime("expected a DLL type name, StructType or ArrayType"))
            }
            other => Err(runtime(format!(
                "expected a DLL type name, StructType or ArrayType, got {}",
                other.type_name()
            ))),
        }
    }

    pub fn parse_value(value: &Value) -> Result<CType> {
        let ty = CType::parse(value)?;
        if ty == CType::Void {
            return Err(runtime("void has no values, it can only be a return type"));
        }
        Ok(ty)
    }

    pub fn parse_list(list: Option<Table>) -> Result<Vec<CType>> {
        let Some(list) = list else {
            return Ok(Vec::new());
        };
        list.sequence_values::<Value>()
            .enumerate()
            .map(|(index, value)| {
                let ty = CType::parse_value(&value?).map_err(|error| runtime(format!("argument type #{}: {error}", index + 1)))?;
                if matches!(ty, CType::Array(_)) {
                    return Err(runtime(format!(
                        "argument type #{}: arrays cannot be passed by value, pass a pointer instead",
                        index + 1
                    )));
                }
                Ok(ty)
            })
            .collect()
    }

    pub fn parse_result(value: &Value) -> Result<CType> {
        let ty = CType::parse(value)?;
        if matches!(ty, CType::Array(_)) {
            return Err(runtime("arrays cannot be returned by value, return a pointer instead"));
        }
        Ok(ty)
    }
}

impl FromLua for CType {
    fn from_lua(value: Value, _: &Lua) -> Result<Self> {
        CType::parse(&value)
    }
}

#[derive(Clone)]
pub struct StructType(pub Arc<StructLayout>);

#[derive(Clone)]
pub struct ArrayType(pub Arc<ArrayLayout>);

fn field_parts(entry: &Table) -> Result<(Value, Value)> {
    let name = match entry.raw_get::<Value>(1)? {
        Value::Nil => entry.get::<Value>("Name")?,
        name => name,
    };
    let ty = match entry.raw_get::<Value>(2)? {
        Value::Nil => entry.get::<Value>("Type")?,
        ty => ty,
    };
    Ok((name, ty))
}

impl StructType {
    pub const TYPE_NAME: &'static str = "StructType";

    pub fn define(fields: Table) -> Result<StructType> {
        let mut layout = Vec::new();
        let mut names = HashSet::new();
        let (mut offset, mut align) = (0usize, 1usize);
        for (index, entry) in fields.sequence_values::<Value>().enumerate() {
            let number = index + 1;
            let Value::Table(entry) = entry? else {
                return Err(runtime(format!("field #{number} must be a table like {{ \"x\", \"f32\" }}")));
            };
            let (name, ty) = field_parts(&entry)?;
            let Value::String(name) = name else {
                return Err(runtime(format!("field #{number} needs a name")));
            };
            let name = name.to_str()?.to_string();
            if name.is_empty() {
                return Err(runtime(format!("field #{number} needs a name")));
            }
            if !names.insert(name.clone()) {
                return Err(runtime(format!("the struct has more than one field named '{name}'")));
            }
            let ty = CType::parse_value(&ty).map_err(|error| runtime(format!("field '{name}': {error}")))?;
            offset = align_up(offset, ty.align());
            align = align.max(ty.align());
            let size = ty.size();
            layout.push(Field { name, ty, offset });
            offset += size;
        }
        if layout.is_empty() {
            return Err(runtime("a struct needs at least one field"));
        }
        Ok(StructType(Arc::new(StructLayout {
            fields: layout,
            size: align_up(offset, align),
            align,
        })))
    }
}

impl UserData for StructType {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::TYPE_NAME);
        fields.add_field_method_get("Size", |_, this| Ok(this.0.size));
        fields.add_field_method_get("Alignment", |_, this| Ok(this.0.align));
        fields.add_field_method_get("Fields", |lua, this| {
            lua.create_sequence_from(this.0.fields.iter().map(|field| field.name.clone()))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Offset", |_, this, name: String| {
            this.0
                .fields
                .iter()
                .find(|field| field.name == name)
                .map(|field| field.offset)
                .ok_or_else(|| runtime(format!("the struct has no field named '{name}'")))
        });
        methods.add_method("New", |lua, this, values: Option<Value>| {
            allocate(lua, &CType::Struct(this.0.clone()), values)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("{}({} bytes)", Self::TYPE_NAME, this.0.size))
        });
        methods.add_meta_function(MetaMethod::Eq, |_, (left, right): (AnyUserData, AnyUserData)| {
            Ok(match (left.borrow::<StructType>(), right.borrow::<StructType>()) {
                (Ok(left), Ok(right)) => left.0 == right.0,
                _ => false,
            })
        });
    }
}

impl ArrayType {
    pub const TYPE_NAME: &'static str = "ArrayType";

    pub fn define(element: &Value, length: f64) -> Result<ArrayType> {
        let element = CType::parse_value(element)?;
        if !(length.is_finite() && length.fract() == 0.0 && length >= 1.0 && length <= (1u64 << 32) as f64) {
            return Err(runtime(format!("an array length must be a whole number of at least 1, got {length}")));
        }
        Ok(ArrayType(Arc::new(ArrayLayout {
            element,
            length: length as usize,
        })))
    }
}

impl UserData for ArrayType {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::TYPE_NAME);
        fields.add_field_method_get("Size", |_, this| Ok(CType::Array(this.0.clone()).size()));
        fields.add_field_method_get("Alignment", |_, this| Ok(this.0.element.align()));
        fields.add_field_method_get("Length", |_, this| Ok(this.0.length));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("New", |lua, this, values: Option<Value>| {
            allocate(lua, &CType::Array(this.0.clone()), values)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("{}({})", Self::TYPE_NAME, CType::Array(this.0.clone()).describe()))
        });
    }
}

pub fn allocate(lua: &Lua, ty: &CType, value: Option<Value>) -> Result<Pointer> {
    if *ty == CType::Void {
        return Err(runtime("void has no size to allocate"));
    }
    let block = Block::zeroed(ty.size()).ok_or_else(|| runtime("the memory could not be allocated"))?;
    if let Some(value) = value
        && !value.is_nil()
    {
        unsafe { Writer::memory().write(lua, ty, &value, block.as_ptr()) }?;
    }
    Ok(Pointer::owned(block))
}
