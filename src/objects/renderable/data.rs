use std::collections::BTreeMap;

use mlua::{AnyUserData, Lua, Table, Value};
use wgpu::naga::{ArraySize, Handle, Scalar, ScalarKind, Type, TypeInner};

use crate::datatypes::{Color, EnumItem, UDim};
use crate::graphics::geometry::NO_OBJECT;
use crate::graphics::protocol::ObjectId;
use crate::graphics::reflect::ShaderLayout;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bytes {
    pub bytes: Vec<u8>,
    pub references: BTreeMap<u32, ObjectId>,
}

impl Bytes {
    pub fn zeroed(size: usize) -> Bytes {
        Bytes {
            bytes: vec![0; size],
            references: BTreeMap::new(),
        }
    }

    fn put(&mut self, offset: usize, data: &[u8]) {
        let end = offset + data.len();
        if self.bytes.len() < end {
            self.bytes.resize(end, 0);
        }
        self.bytes[offset..end].copy_from_slice(data);
        self.forget(offset, end);
    }

    fn zero(&mut self, start: usize, end: usize) {
        if self.bytes.len() < end {
            self.bytes.resize(end, 0);
        }
        self.bytes[start..end].fill(0);
        self.forget(start, end);
    }

    fn truncate(&mut self, length: usize) {
        self.bytes.truncate(length);
        self.forget(length, usize::MAX);
    }

    fn forget(&mut self, start: usize, end: usize) {
        let start = start.min(u32::MAX as usize) as u32;
        let end = end.min(u32::MAX as usize) as u32;
        self.references.retain(|offset, _| *offset < start || *offset >= end);
    }

    fn get(&self, offset: usize, length: usize) -> Option<&[u8]> {
        self.bytes.get(offset..offset + length)
    }
}

pub struct Writer<'a> {
    pub layout: &'a ShaderLayout,
    pub resolve: &'a dyn Fn(&AnyUserData) -> Result<Option<ObjectId>, String>,
}

fn describe(value: &Value) -> String {
    match value {
        Value::UserData(userdata) => userdata
            .type_name()
            .ok()
            .map(|name| name.to_string_lossy())
            .unwrap_or_else(|| "userdata".to_owned()),
        other => other.type_name().to_owned(),
    }
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(integer) => Some(*integer as f64),
        Value::Number(number) => Some(*number),
        Value::Boolean(boolean) => Some(if *boolean { 1.0 } else { 0.0 }),
        Value::UserData(userdata) if userdata.is::<EnumItem>() => {
            userdata.borrow::<EnumItem>().ok().map(|item| f64::from(item.value))
        }
        _ => None,
    }
}

fn integer(value: f64, low: f64, high: f64, kind: &str) -> Result<f64, String> {
    if value.fract() != 0.0 || !value.is_finite() {
        return Err(format!("expected a whole number for a {kind} value, got {value}"));
    }
    if value < low || value > high {
        return Err(format!("{value} does not fit in a {kind} value"));
    }
    Ok(value)
}

fn scalar_name(scalar: Scalar) -> String {
    let prefix = match scalar.kind {
        ScalarKind::Float | ScalarKind::AbstractFloat => "f",
        ScalarKind::Sint | ScalarKind::AbstractInt => "i",
        ScalarKind::Uint => "u",
        ScalarKind::Bool => return "bool".to_owned(),
    };
    format!("{prefix}{}", u32::from(scalar.width) * 8)
}

fn scalar_bytes(scalar: Scalar, value: f64) -> Result<Vec<u8>, String> {
    let name = scalar_name(scalar);
    Ok(match (scalar.kind, scalar.width) {
        (ScalarKind::Float, 4) => (value as f32).to_le_bytes().to_vec(),
        (ScalarKind::Float, 8) => value.to_le_bytes().to_vec(),
        (ScalarKind::Sint, 4) => (integer(value, f64::from(i32::MIN), f64::from(i32::MAX), &name)? as i32)
            .to_le_bytes()
            .to_vec(),
        (ScalarKind::Uint, 4) => (integer(value, 0.0, f64::from(u32::MAX), &name)? as u32)
            .to_le_bytes()
            .to_vec(),
        (ScalarKind::Sint, 8) => (integer(value, i64::MIN as f64, i64::MAX as f64, &name)? as i64)
            .to_le_bytes()
            .to_vec(),
        (ScalarKind::Uint, 8) => (integer(value, 0.0, u64::MAX as f64, &name)? as u64)
            .to_le_bytes()
            .to_vec(),
        (ScalarKind::Bool, _) => u32::from(value != 0.0).to_le_bytes().to_vec(),
        _ => return Err(format!("{name} values cannot be written from Luau")),
    })
}

fn is_word(scalar: Scalar) -> bool {
    scalar.width == 4 && matches!(scalar.kind, ScalarKind::Uint | ScalarKind::Sint)
}

fn components(value: &Value) -> Option<Vec<f64>> {
    match value {
        Value::Vector(vector) => Some(vec![
            f64::from(vector.x()),
            f64::from(vector.y()),
            f64::from(vector.z()),
        ]),
        Value::UserData(userdata) if userdata.is::<UDim>() => {
            let udim = *userdata.borrow::<UDim>().ok()?;
            Some(vec![udim.x, udim.y, udim.z])
        }
        Value::UserData(userdata) if userdata.is::<Color>() => {
            let color = *userdata.borrow::<Color>().ok()?;
            Some(vec![color.r, color.g, color.b, color.a])
        }
        Value::Table(table) => table
            .sequence_values::<Value>()
            .map(|item| item.ok().as_ref().and_then(number))
            .collect(),
        other => number(other).map(|value| vec![value]),
    }
}

fn column_stride(rows: u8, scalar: Scalar) -> usize {
    let rows = if rows == 2 { 2 } else { 4 };
    rows * usize::from(scalar.width)
}

impl Writer<'_> {
    fn size(&self, ty: Handle<Type>) -> usize {
        self.layout.type_layout(ty).size as usize
    }

    fn inner(&self, ty: Handle<Type>) -> &TypeInner {
        &self.layout.module.types[ty].inner
    }

    pub fn write(&self, ty: Handle<Type>, value: &Value, offset: usize, out: &mut Bytes) -> Result<(), String> {
        if let Value::Buffer(buffer) = value {
            let data = buffer.to_vec();
            let limit = self.runtime_limit(ty);
            if limit.is_none_or(|limit| data.len() <= limit) {
                out.put(offset, &data);
                if let Some(limit) = limit {
                    out.zero(offset + data.len(), offset + limit);
                } else {
                    out.truncate(offset + data.len());
                }
                return Ok(());
            }
            return Err(format!(
                "the buffer is {} bytes but the value only holds {} bytes",
                data.len(),
                limit.unwrap_or_default()
            ));
        }
        match self.inner(ty) {
            TypeInner::Scalar(scalar) | TypeInner::Atomic(scalar) => self.write_scalar(*scalar, value, offset, out),
            TypeInner::Vector { size, scalar } => {
                let count = *size as usize;
                let values = components(value)
                    .ok_or_else(|| format!("expected a udim, color, vector or table of numbers, got {}", describe(value)))?;
                for index in 0..count {
                    let component = values.get(index).copied().unwrap_or(0.0);
                    out.put(offset + index * usize::from(scalar.width), &scalar_bytes(*scalar, component)?);
                }
                Ok(())
            }
            TypeInner::Matrix { columns, rows, scalar } => {
                let (columns, rows) = (*columns as usize, *rows as usize);
                let Value::Table(table) = value else {
                    return Err(format!("expected a table of numbers for a matrix, got {}", describe(value)));
                };
                let mut values = Vec::with_capacity(columns * rows);
                for item in table.sequence_values::<Value>() {
                    let item = item.map_err(|error| error.to_string())?;
                    match &item {
                        Value::Table(_) => values.extend(components(&item).unwrap_or_default()),
                        other => values.push(number(other).ok_or_else(|| "matrices only hold numbers".to_owned())?),
                    }
                }
                let stride = column_stride(rows as u8, *scalar);
                for column in 0..columns {
                    for row in 0..rows {
                        let component = values.get(column * rows + row).copied().unwrap_or(0.0);
                        out.put(
                            offset + column * stride + row * usize::from(scalar.width),
                            &scalar_bytes(*scalar, component)?,
                        );
                    }
                }
                Ok(())
            }
            TypeInner::Array { base, size, stride } => self.write_array(*base, *size, *stride as usize, value, offset, out),
            TypeInner::Struct { members, .. } => {
                let Value::Table(table) = value else {
                    return Err(format!("expected a table of struct members, got {}", describe(value)));
                };
                for pair in table.pairs::<Value, Value>() {
                    let (key, item) = pair.map_err(|error| error.to_string())?;
                    let Value::String(name) = &key else {
                        return Err(format!("struct member names must be strings, got {}", key.type_name()));
                    };
                    let name = name.to_string_lossy();
                    let member = members
                        .iter()
                        .find(|member| member.name.as_deref() == Some(name.as_str()))
                        .ok_or_else(|| format!("the struct has no member '{name}'"))?;
                    self.write(member.ty, &item, offset + member.offset as usize, out)
                        .map_err(|error| format!("{name}: {error}"))?;
                }
                Ok(())
            }
            _ => Err("this kind of shader value cannot be written from Luau".to_owned()),
        }
    }

    fn runtime_limit(&self, ty: Handle<Type>) -> Option<usize> {
        match self.inner(ty) {
            TypeInner::Array {
                size: ArraySize::Dynamic,
                ..
            } => None,
            TypeInner::Struct { members, .. } => match members.last() {
                Some(last) if self.runtime_limit(last.ty).is_none() => None,
                _ => Some(self.size(ty)),
            },
            _ => Some(self.size(ty)),
        }
    }

    fn write_scalar(&self, scalar: Scalar, value: &Value, offset: usize, out: &mut Bytes) -> Result<(), String> {
        if let Value::UserData(userdata) = value
            && let Some(target) = (self.resolve)(userdata)?
        {
            if !is_word(scalar) {
                return Err(format!(
                    "a Renderable can only be written to a u32 or i32 value, not {}",
                    scalar_name(scalar)
                ));
            }
            out.put(offset, &NO_OBJECT.to_le_bytes());
            out.references.insert(offset as u32, target);
            return Ok(());
        }
        let number = number(value).ok_or_else(|| format!("expected a number, got {}", describe(value)))?;
        out.put(offset, &scalar_bytes(scalar, number)?);
        Ok(())
    }

    fn write_array(
        &self,
        base: Handle<Type>,
        size: ArraySize,
        stride: usize,
        value: &Value,
        offset: usize,
        out: &mut Bytes,
    ) -> Result<(), String> {
        let capacity = match size {
            ArraySize::Constant(count) => Some(count.get() as usize),
            ArraySize::Dynamic => None,
            ArraySize::Pending(_) => return Err("arrays sized by overrides cannot be written".to_owned()),
        };
        let items: Vec<Value> = match value {
            Value::String(text) => {
                let TypeInner::Scalar(scalar) = self.inner(base) else {
                    return Err("strings can only be written to arrays of u32 or i32".to_owned());
                };
                if !is_word(*scalar) {
                    return Err("strings can only be written to arrays of u32 or i32".to_owned());
                }
                text.to_str()
                    .map_err(|error| error.to_string())?
                    .chars()
                    .map(|character| Value::Integer(i64::from(u32::from(character))))
                    .collect()
            }
            Value::Table(table) => table
                .sequence_values::<Value>()
                .collect::<mlua::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?,
            other => return Err(format!("expected a table, string or buffer for an array, got {}", describe(other))),
        };
        if let Some(capacity) = capacity
            && items.len() > capacity
        {
            return Err(format!("the array holds {capacity} items but {} were given", items.len()));
        }
        for (index, item) in items.iter().enumerate() {
            self.write(base, item, offset + index * stride, out)
                .map_err(|error| format!("item {}: {error}", index + 1))?;
        }
        let written = offset + items.len() * stride;
        match capacity {
            Some(capacity) => out.zero(written, offset + capacity * stride),
            None => {
                out.truncate(written);
                if out.bytes.len() < written {
                    out.bytes.resize(written, 0);
                }
            }
        }
        Ok(())
    }
}

pub struct Reader<'a> {
    pub lua: &'a Lua,
    pub layout: &'a ShaderLayout,
    pub resolve: &'a dyn Fn(ObjectId) -> Option<AnyUserData>,
}

fn read_scalar(scalar: Scalar, data: &[u8]) -> Option<Value> {
    let word = |length: usize| data.get(..length);
    Some(match (scalar.kind, scalar.width) {
        (ScalarKind::Float, 4) => Value::Number(f64::from(f32::from_le_bytes(word(4)?.try_into().ok()?))),
        (ScalarKind::Float, 8) => Value::Number(f64::from_le_bytes(word(8)?.try_into().ok()?)),
        (ScalarKind::Sint, 4) => Value::Integer(i64::from(i32::from_le_bytes(word(4)?.try_into().ok()?))),
        (ScalarKind::Uint, 4) => Value::Integer(i64::from(u32::from_le_bytes(word(4)?.try_into().ok()?))),
        (ScalarKind::Sint, 8) => Value::Integer(i64::from_le_bytes(word(8)?.try_into().ok()?)),
        (ScalarKind::Uint, 8) => Value::Number(u64::from_le_bytes(word(8)?.try_into().ok()?) as f64),
        (ScalarKind::Bool, _) => Value::Boolean(u32::from_le_bytes(word(4)?.try_into().ok()?) != 0),
        _ => return None,
    })
}

impl Reader<'_> {
    pub fn read(&self, ty: Handle<Type>, data: &Bytes, offset: usize) -> mlua::Result<Value> {
        let module = &self.layout.module;
        let lua = self.lua;
        Ok(match &module.types[ty].inner {
            TypeInner::Scalar(scalar) | TypeInner::Atomic(scalar) => {
                if let Some(target) = data.references.get(&(offset as u32)) {
                    return Ok((self.resolve)(*target).map_or(Value::Nil, Value::UserData));
                }
                data.get(offset, usize::from(scalar.width))
                    .and_then(|bytes| read_scalar(*scalar, bytes))
                    .unwrap_or(Value::Nil)
            }
            TypeInner::Vector { size, scalar } => {
                let count = *size as usize;
                let mut values = Vec::with_capacity(count);
                for index in 0..count {
                    let item = data
                        .get(offset + index * usize::from(scalar.width), usize::from(scalar.width))
                        .and_then(|bytes| read_scalar(*scalar, bytes))
                        .unwrap_or(Value::Integer(0));
                    values.push(item);
                }
                let float = |index: usize| match values.get(index) {
                    Some(Value::Number(number)) => *number,
                    Some(Value::Integer(integer)) => *integer as f64,
                    _ => 0.0,
                };
                if scalar.kind == ScalarKind::Float {
                    match count {
                        4 => Value::UserData(lua.create_userdata(Color::new(float(0), float(1), float(2), float(3)))?),
                        3 => Value::UserData(lua.create_userdata(UDim::new(float(0), float(1), float(2)))?),
                        _ => Value::UserData(lua.create_userdata(UDim::new(float(0), float(1), 0.0))?),
                    }
                } else {
                    Value::Table(lua.create_sequence_from(values)?)
                }
            }
            TypeInner::Matrix { columns, rows, scalar } => {
                let stride = column_stride(*rows as u8, *scalar);
                let table = lua.create_table()?;
                for column in 0..*columns as usize {
                    for row in 0..*rows as usize {
                        let item = data
                            .get(offset + column * stride + row * usize::from(scalar.width), usize::from(scalar.width))
                            .and_then(|bytes| read_scalar(*scalar, bytes))
                            .unwrap_or(Value::Number(0.0));
                        table.push(item)?;
                    }
                }
                Value::Table(table)
            }
            TypeInner::Array { base, size, stride } => {
                let stride = *stride as usize;
                let count = match size {
                    ArraySize::Constant(count) => count.get() as usize,
                    _ => data.bytes.len().saturating_sub(offset) / stride.max(1),
                };
                let table: Table = lua.create_table_with_capacity(count, 0)?;
                for index in 0..count {
                    table.push(self.read(*base, data, offset + index * stride)?)?;
                }
                Value::Table(table)
            }
            TypeInner::Struct { members, .. } => {
                let table = lua.create_table()?;
                for member in members {
                    if let Some(name) = &member.name {
                        table.set(name.as_str(), self.read(member.ty, data, offset + member.offset as usize)?)?;
                    }
                }
                Value::Table(table)
            }
            _ => Value::Nil,
        })
    }
}
