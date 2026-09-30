use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use mlua::{Lua, MultiValue, Result, Table, Value, Vector};
use tokio::sync::mpsc;

use super::imports;
use super::scheduler::Activity;
use crate::datatypes::{Color, EnumItem, UDim};

const MAX_DEPTH: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub enum Packet {
    Nil,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    Vector(f32, f32, f32),
    String(Box<[u8]>),
    Buffer(Box<[u8]>),
    Table(Box<[(Packet, Packet)]>),
    Import(Box<str>),
    Module(Box<str>),
    UDim([f64; 3]),
    Color([f64; 4]),
    EnumItem(Box<str>, Box<str>),
}

pub type Payload = Arc<[Packet]>;

pub fn encode(lua: &Lua, values: MultiValue) -> Result<Payload> {
    values
        .into_iter()
        .map(|value| encode_value(lua, value).map_err(mlua::Error::runtime))
        .collect::<Result<Vec<_>>>()
        .map(Payload::from)
}

pub fn encode_value(lua: &Lua, value: Value) -> std::result::Result<Packet, String> {
    Encoder {
        lua,
        visiting: Vec::new(),
        modules: None,
    }
    .encode(value, 0)
}

const LOADER_CACHE: &str = "__MLUA_LOADER_CACHE";

fn loaded_modules(lua: &Lua) -> HashMap<*const c_void, String> {
    let Ok(Some(cache)) = lua.named_registry_value::<Option<Table>>(LOADER_CACHE) else {
        return HashMap::new();
    };
    cache
        .pairs::<String, Value>()
        .filter_map(|pair| {
            let (path, value) = pair.ok()?;
            let Value::Table(table) = value else { return None };
            Some((table.to_pointer(), path))
        })
        .collect()
}

fn require_path(lua: &Lua, path: &str) -> Result<Value> {
    let cache = match lua.named_registry_value::<Option<Table>>(LOADER_CACHE)? {
        Some(cache) => cache,
        None => {
            let cache = lua.create_table()?;
            lua.set_named_registry_value(LOADER_CACHE, &cache)?;
            cache
        }
    };
    let cached: Value = cache.raw_get(path)?;
    if !cached.is_nil() {
        return Ok(cached);
    }
    let engine = lua
        .app_data_ref::<Arc<super::Engine>>()
        .map(|engine| engine.clone())
        .ok_or_else(|| mlua::Error::runtime("the luv engine is not running"))?;
    let chunk = super::load_unit(lua, engine.vfs().as_ref(), path, 0)?;
    let loaded = match chunk.call::<Value>(())? {
        Value::Nil => Value::Boolean(true),
        value => value,
    };
    cache.raw_set(path, loaded.clone())?;
    Ok(loaded)
}

pub fn decode(lua: &Lua, packets: &[Packet]) -> Result<MultiValue> {
    packets.iter().map(|packet| decode_value(lua, packet)).collect()
}

pub fn decode_value(lua: &Lua, packet: &Packet) -> Result<Value> {
    Ok(match packet {
        Packet::Nil => Value::Nil,
        Packet::Boolean(value) => Value::Boolean(*value),
        Packet::Integer(value) => Value::Integer(*value),
        Packet::Number(value) => Value::Number(*value),
        Packet::Vector(x, y, z) => Value::Vector(Vector::new(*x, *y, *z)),
        Packet::String(bytes) => Value::String(lua.create_string(bytes)?),
        Packet::Buffer(bytes) => Value::Buffer(lua.create_buffer(bytes)?),
        Packet::Table(entries) => {
            let table = lua.create_table_with_capacity(0, entries.len())?;
            for (key, value) in entries {
                table.raw_set(decode_value(lua, key)?, decode_value(lua, value)?)?;
            }
            Value::Table(table)
        }
        Packet::Import(name) => imports::get(lua, name)?,
        Packet::Module(path) => require_path(lua, path)?,
        Packet::UDim([x, y, z]) => Value::UserData(lua.create_userdata(UDim::new(*x, *y, *z))?),
        Packet::Color([r, g, b, a]) => Value::UserData(lua.create_userdata(Color::new(*r, *g, *b, *a))?),
        Packet::EnumItem(enum_type, name) => EnumItem::find(enum_type, name)
            .ok_or_else(|| mlua::Error::runtime(format!("enum.{enum_type}.{name} does not exist")))?
            .canonical(lua)?,
    })
}

struct Encoder<'a> {
    lua: &'a Lua,
    visiting: Vec<*const c_void>,
    modules: Option<HashMap<*const c_void, String>>,
}

impl Encoder<'_> {
    fn encode(&mut self, value: Value, depth: usize) -> std::result::Result<Packet, String> {
        Ok(match value {
            Value::Nil => Packet::Nil,
            Value::Boolean(value) => Packet::Boolean(value),
            Value::Integer(value) => Packet::Integer(value),
            Value::Number(value) => Packet::Number(value),
            Value::Vector(value) => Packet::Vector(value.x(), value.y(), value.z()),
            Value::String(value) => Packet::String(value.as_bytes().to_vec().into()),
            Value::Buffer(value) => Packet::Buffer(value.to_vec().into()),
            Value::UserData(ref userdata) if userdata.is::<UDim>() => {
                let udim = *userdata.borrow::<UDim>().map_err(|error| error.to_string())?;
                Packet::UDim([udim.x, udim.y, udim.z])
            }
            Value::UserData(ref userdata) if userdata.is::<Color>() => {
                let color = *userdata.borrow::<Color>().map_err(|error| error.to_string())?;
                Packet::Color([color.r, color.g, color.b, color.a])
            }
            Value::UserData(ref userdata) if userdata.is::<EnumItem>() => {
                let item = *userdata.borrow::<EnumItem>().map_err(|error| error.to_string())?;
                Packet::EnumItem(item.enum_type.into(), item.name.into())
            }
            Value::Table(_) | Value::UserData(_) => {
                if let Some(name) = imports::name_of(self.lua, &value).map_err(|error| error.to_string())? {
                    return Ok(Packet::Import(name.into()));
                }
                let Value::Table(table) = value else {
                    return Err(format!("{} objects cannot be sent between threads", object_name(&value)));
                };
                let modules = self.modules.get_or_insert_with(|| loaded_modules(self.lua));
                if let Some(path) = modules.get(&table.to_pointer()) {
                    return Ok(Packet::Module(path.as_str().into()));
                }
                if depth >= MAX_DEPTH {
                    return Err(format!("tables nested deeper than {MAX_DEPTH} levels cannot be sent between threads"));
                }
                let pointer = table.to_pointer();
                if self.visiting.contains(&pointer) {
                    return Err("tables that contain themselves cannot be sent between threads".to_owned());
                }
                self.visiting.push(pointer);
                let mut entries = Vec::new();
                for pair in table.pairs::<Value, Value>() {
                    let (key, value) = pair.map_err(|error| error.to_string())?;
                    entries.push((self.encode(key, depth + 1)?, self.encode(value, depth + 1)?));
                }
                self.visiting.pop();
                Packet::Table(entries.into())
            }
            other => return Err(format!("{} values cannot be sent between threads", other.type_name())),
        })
    }
}

fn object_name(value: &Value) -> String {
    match value {
        Value::UserData(userdata) => userdata
            .type_name()
            .ok()
            .and_then(|name| name.to_str().ok().map(|name| name.to_string()))
            .unwrap_or_else(|| "userdata".to_owned()),
        other => other.type_name().to_owned(),
    }
}

pub enum Message {
    Topic { topic: Arc<str>, payload: Payload },
    Exception(Packet),
    Call(Payload),
    Stop,
    Registry { name: Arc<str>, id: Arc<str> },
    Close,
}

pub struct Mailbox {
    pub id: u64,
    pub receiver: mpsc::UnboundedReceiver<Message>,
}

pub struct Bus {
    activity: Arc<Activity>,
    next: AtomicU64,
    mailboxes: Mutex<Vec<(u64, mpsc::UnboundedSender<Message>)>>,
}

impl Bus {
    pub fn new(activity: Arc<Activity>) -> Self {
        Self {
            activity,
            next: AtomicU64::new(0),
            mailboxes: Mutex::new(Vec::new()),
        }
    }

    pub fn activity(&self) -> &Arc<Activity> {
        &self.activity
    }

    pub fn open(&self) -> Mailbox {
        let (sender, receiver) = mpsc::unbounded_channel();
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.mailboxes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((id, sender));
        Mailbox { id, receiver }
    }

    pub fn close(&self, id: u64) {
        self.mailboxes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(mailbox, _)| *mailbox != id);
    }

    pub fn publish(&self, topic: &str, payload: Payload) {
        let topic: Arc<str> = Arc::from(topic);
        let failed = {
            let mailboxes = self.mailboxes.lock().unwrap_or_else(PoisonError::into_inner);
            self.activity.enter(mailboxes.len());
            mailboxes
                .iter()
                .filter(|(_, sender)| {
                    let message = Message::Topic {
                        topic: topic.clone(),
                        payload: payload.clone(),
                    };
                    sender.send(message).is_err()
                })
                .count()
        };
        for _ in 0..failed {
            self.activity.exit();
        }
    }

    pub fn broadcast_except(&self, sender: Option<u64>, make: impl Fn() -> Message) {
        let mailboxes = self.mailboxes.lock().unwrap_or_else(PoisonError::into_inner);
        for (id, mailbox) in mailboxes.iter() {
            if Some(*id) == sender {
                continue;
            }
            self.activity.enter(1);
            if mailbox.send(make()).is_err() {
                self.activity.exit();
            }
        }
    }

    pub fn send_to(&self, id: u64, message: Message) -> bool {
        let mailboxes = self.mailboxes.lock().unwrap_or_else(PoisonError::into_inner);
        let Some((_, sender)) = mailboxes.iter().find(|(mailbox, _)| *mailbox == id) else {
            return false;
        };
        self.activity.enter(1);
        if sender.send(message).is_err() {
            self.activity.exit();
            return false;
        }
        true
    }

    pub fn begin_close(&self) {
        if !self.activity.begin_closing() {
            return;
        }
        let (count, failed) = {
            let mailboxes = self.mailboxes.lock().unwrap_or_else(PoisonError::into_inner);
            self.activity.closing_enter(mailboxes.len());
            let failed = mailboxes
                .iter()
                .filter(|(_, sender)| sender.send(Message::Close).is_err())
                .count();
            (mailboxes.len(), failed)
        };
        if count == 0 {
            self.activity.stop();
        }
        for _ in 0..failed {
            self.activity.closing_exit();
        }
    }
}
