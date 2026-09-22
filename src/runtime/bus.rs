use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use mlua::{Lua, MultiValue, Result, Value, Vector};
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
    }
    .encode(value, 0)
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
