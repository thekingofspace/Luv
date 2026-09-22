use std::ffi::c_void;
use std::fmt;

use mlua::{Lua, LuaString, Result, Table, Value};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

use crate::datatypes::{Color, UDim};

const MAX_DEPTH: usize = 128;
const TOML_DATETIME: &str = "$__toml_private_datetime";

#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Data>),
    Object(Vec<(String, Data)>),
}

impl Data {
    fn into_key(self) -> String {
        match self {
            Data::String(text) => text,
            Data::Null => "null".to_owned(),
            Data::Bool(value) => value.to_string(),
            Data::Integer(value) => value.to_string(),
            Data::Float(value) => value.to_string(),
            other => serde_json::to_string(&other).unwrap_or_default(),
        }
    }
}

impl Serialize for Data {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Data::Null => serializer.serialize_unit(),
            Data::Bool(value) => serializer.serialize_bool(*value),
            Data::Integer(value) => serializer.serialize_i64(*value),
            Data::Float(value) => serializer.serialize_f64(*value),
            Data::String(value) => serializer.serialize_str(value),
            Data::Array(items) => {
                let mut sequence = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    sequence.serialize_element(item)?;
                }
                sequence.end()
            }
            Data::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Data {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_any(DataVisitor)
    }
}

struct DataVisitor;

impl<'de> Visitor<'de> for DataVisitor {
    type Value = Data;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("any value")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> std::result::Result<Data, E> {
        Ok(Data::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Data, E> {
        Ok(Data::Integer(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Data, E> {
        Ok(i64::try_from(value).map_or(Data::Float(value as f64), Data::Integer))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<Data, E> {
        Ok(Data::Float(value))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Data, E> {
        Ok(Data::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> std::result::Result<Data, E> {
        Ok(Data::String(value))
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> std::result::Result<Data, E> {
        Ok(Data::String(String::from_utf8_lossy(value).into_owned()))
    }

    fn visit_none<E: de::Error>(self) -> std::result::Result<Data, E> {
        Ok(Data::Null)
    }

    fn visit_unit<E: de::Error>(self) -> std::result::Result<Data, E> {
        Ok(Data::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> std::result::Result<Data, D::Error> {
        Data::deserialize(deserializer)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> std::result::Result<Data, D::Error> {
        Data::deserialize(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> std::result::Result<Data, A::Error> {
        let mut items = Vec::with_capacity(sequence.size_hint().unwrap_or(0));
        while let Some(item) = sequence.next_element::<Data>()? {
            items.push(item);
        }
        Ok(Data::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Data, A::Error> {
        let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
        while let Some(key) = map.next_key::<Data>()? {
            let value = map.next_value::<Data>()?;
            entries.push((key.into_key(), value));
        }
        if let [(key, Data::String(datetime))] = entries.as_slice()
            && key == TOML_DATETIME
        {
            return Ok(Data::String(datetime.clone()));
        }
        Ok(Data::Object(entries))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Jsonc,
    Toml,
    Yaml,
}

impl Format {
    pub fn parse(name: &str) -> Result<Format> {
        match name.to_ascii_lowercase().as_str() {
            "json" => Ok(Format::Json),
            "jsonc" => Ok(Format::Jsonc),
            "toml" => Ok(Format::Toml),
            "yaml" | "yml" => Ok(Format::Yaml),
            _ => Err(mlua::Error::runtime(format!(
                "unknown format '{name}', expected \"json\", \"jsonc\", \"toml\" or \"yaml\""
            ))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Jsonc => "jsonc",
            Format::Toml => "toml",
            Format::Yaml => "yaml",
        }
    }

    pub fn encode(self, data: Data, pretty: bool) -> std::result::Result<String, String> {
        let text = match self {
            Format::Json | Format::Jsonc if pretty => serde_json::to_string_pretty(&data).map_err(|error| error.to_string()),
            Format::Json | Format::Jsonc => serde_json::to_string(&data).map_err(|error| error.to_string()),
            Format::Toml => {
                let data = match data {
                    Data::Array(items) if items.is_empty() => Data::Object(Vec::new()),
                    Data::Object(entries) => Data::Object(entries),
                    _ => return Err("toml documents must be tables".to_owned()),
                };
                if pretty {
                    toml::to_string_pretty(&data).map_err(|error| error.to_string())
                } else {
                    toml::to_string(&data).map_err(|error| error.to_string())
                }
            }
            Format::Yaml if pretty => serde_yaml_ng::to_string(&data).map_err(|error| error.to_string()),
            Format::Yaml => serde_json::to_string(&data).map_err(|error| error.to_string()),
        }?;
        Ok(text)
    }

    pub fn decode(self, text: &[u8]) -> std::result::Result<Data, String> {
        let text = std::str::from_utf8(text).map_err(|_| "the text is not valid UTF-8".to_owned())?;
        match self {
            Format::Json => serde_json::from_str(text).map_err(|error| error.to_string()),
            Format::Jsonc => serde_json::from_str(&strip_jsonc(text)).map_err(|error| error.to_string()),
            Format::Toml => toml::from_str(text).map_err(|error| error.to_string()),
            Format::Yaml => serde_yaml_ng::from_str(text).map_err(|error| error.to_string()),
        }
    }
}

pub fn strip_jsonc(source: &str) -> String {
    let mut stripped = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    while let Some(ch) = chars.next() {
        if in_string {
            stripped.push(ch);
            match ch {
                '\\' => {
                    if let Some(escaped) = chars.next() {
                        stripped.push(escaped);
                    }
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                stripped.push(ch);
            }
            '/' if chars.peek() == Some(&'/') => {
                while chars.peek().is_some_and(|next| *next != '\n') {
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    if next == '\n' {
                        stripped.push('\n');
                    }
                    previous = next;
                }
                stripped.push(' ');
            }
            _ => stripped.push(ch),
        }
    }
    remove_trailing_commas(&stripped)
}

fn remove_trailing_commas(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut result = String::with_capacity(source.len());
    let mut in_string = false;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            result.push(ch);
            if ch == '\\' {
                if let Some(escaped) = chars.get(index + 1) {
                    result.push(*escaped);
                    index += 1;
                }
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
            result.push(ch);
        } else if ch == ',' {
            let next = chars[index + 1..].iter().find(|next| !next.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
        index += 1;
    }
    result
}

fn number(value: f64) -> Data {
    if value.fract() == 0.0 && value.is_finite() && value.abs() < 9.0e15 {
        Data::Integer(value as i64)
    } else {
        Data::Float(value)
    }
}

pub fn from_lua(value: Value) -> Result<Data> {
    Converter { visiting: Vec::new() }.convert(value, 0)
}

struct Converter {
    visiting: Vec<*const c_void>,
}

impl Converter {
    fn convert(&mut self, value: Value, depth: usize) -> Result<Data> {
        Ok(match value {
            Value::Nil => Data::Null,
            Value::Boolean(value) => Data::Bool(value),
            Value::Integer(value) => Data::Integer(value),
            Value::Number(value) => number(value),
            Value::Vector(vector) => Data::Array(vec![
                number(vector.x() as f64),
                number(vector.y() as f64),
                number(vector.z() as f64),
            ]),
            Value::String(text) => Data::String(
                text.to_str()
                    .map_err(|_| mlua::Error::runtime("strings must be valid UTF-8 to be encoded"))?
                    .to_string(),
            ),
            Value::Table(table) => self.table(table, depth)?,
            Value::UserData(userdata) if userdata.is::<UDim>() => {
                let udim = *userdata.borrow::<UDim>()?;
                Data::Object(vec![
                    ("X".to_owned(), number(udim.x)),
                    ("Y".to_owned(), number(udim.y)),
                    ("Z".to_owned(), number(udim.z)),
                ])
            }
            Value::UserData(userdata) if userdata.is::<Color>() => {
                let color = *userdata.borrow::<Color>()?;
                Data::Object(vec![
                    ("A".to_owned(), number(color.a)),
                    ("B".to_owned(), number(color.b)),
                    ("G".to_owned(), number(color.g)),
                    ("R".to_owned(), number(color.r)),
                ])
            }
            other => {
                return Err(mlua::Error::runtime(format!("{} values cannot be encoded", other.type_name())));
            }
        })
    }

    fn table(&mut self, table: Table, depth: usize) -> Result<Data> {
        if depth >= MAX_DEPTH {
            return Err(mlua::Error::runtime(format!(
                "tables nested deeper than {MAX_DEPTH} levels cannot be encoded"
            )));
        }
        let pointer = table.to_pointer();
        if self.visiting.contains(&pointer) {
            return Err(mlua::Error::runtime("tables that contain themselves cannot be encoded"));
        }
        self.visiting.push(pointer);

        let pairs = table.pairs::<Value, Value>().collect::<Result<Vec<_>>>()?;
        let index = |key: &Value| match key {
            Value::Integer(index) => usize::try_from(*index).ok(),
            Value::Number(index) if index.fract() == 0.0 && *index >= 1.0 => Some(*index as usize),
            _ => None,
        };
        let is_array = pairs
            .iter()
            .all(|(key, _)| index(key).is_some_and(|position| position >= 1 && position <= pairs.len()));

        let data = if is_array {
            let mut items = vec![Data::Null; pairs.len()];
            for (key, value) in pairs {
                let position = index(&key).unwrap_or(1) - 1;
                items[position] = self.convert(value, depth + 1)?;
            }
            Data::Array(items)
        } else {
            let mut entries = Vec::with_capacity(pairs.len());
            for (key, value) in pairs {
                let key = match key {
                    Value::String(key) => key
                        .to_str()
                        .map_err(|_| mlua::Error::runtime("table keys must be valid UTF-8 to be encoded"))?
                        .to_string(),
                    Value::Integer(key) => key.to_string(),
                    Value::Number(key) => key.to_string(),
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "table keys must be strings or numbers to be encoded, got {}",
                            other.type_name()
                        )));
                    }
                };
                entries.push((key, self.convert(value, depth + 1)?));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Data::Object(entries)
        };

        self.visiting.pop();
        Ok(data)
    }
}

pub fn to_lua(lua: &Lua, data: Data) -> Result<Value> {
    Ok(match data {
        Data::Null => Value::Nil,
        Data::Bool(value) => Value::Boolean(value),
        Data::Integer(value) => Value::Number(value as f64),
        Data::Float(value) => Value::Number(value),
        Data::String(text) => Value::String(lua.create_string(text)?),
        Data::Array(items) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for (index, item) in items.into_iter().enumerate() {
                table.raw_set(index + 1, to_lua(lua, item)?)?;
            }
            Value::Table(table)
        }
        Data::Object(entries) => {
            let table = lua.create_table_with_capacity(0, entries.len())?;
            for (key, value) in entries {
                table.raw_set(key, to_lua(lua, value)?)?;
            }
            Value::Table(table)
        }
    })
}

pub fn create(lua: &Lua) -> Result<Table> {
    let serde = lua.create_table()?;

    serde.set(
        "Encode",
        lua.create_async_function(|lua, (format, value, pretty): (String, Value, Option<bool>)| {
            let prepared = Format::parse(&format).and_then(|format| Ok((format, from_lua(value)?)));
            async move {
                let (format, data) = prepared?;
                let pretty = pretty.unwrap_or(false);
                let text = tokio::task::spawn_blocking(move || format.encode(data, pretty))
                    .await
                    .map_err(mlua::Error::external)?
                    .map_err(|error| mlua::Error::runtime(format!("cannot encode {}: {error}", format.name())))?;
                lua.create_string(text)
            }
        })?,
    )?;

    serde.set(
        "Decode",
        lua.create_async_function(|lua, (format, text): (String, LuaString)| {
            let prepared = Format::parse(&format).map(|format| (format, text.as_bytes().to_vec()));
            async move {
                let (format, text) = prepared?;
                let data = tokio::task::spawn_blocking(move || format.decode(&text))
                    .await
                    .map_err(mlua::Error::external)?
                    .map_err(|error| mlua::Error::runtime(format!("cannot decode {}: {error}", format.name())))?;
                to_lua(&lua, data)
            }
        })?,
    )?;

    serde.set_readonly(true);
    Ok(serde)
}
