use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::mem;
use std::rc::Rc;
use std::sync::Arc;

use mlua::{
    AnyUserData, Function, Lua, MetaMethod, MultiValue, Result, Table, Thread, UserData, UserDataFields,
    UserDataMethods, Value, WeakLua,
};

use crate::objects::Signal;
use crate::runtime::{Engine, Message, Packet, Scheduler};

const MAX_FRAMES: usize = 64;
const RECENT: usize = 50;
const CHUNK: &str = "luv.exception";
const MAIN: &str = "main";

const TRACEBACK: &str = "
stack traceback:";

pub fn error_text(error: &mlua::Error) -> String {
    let text = full_text(error);
    match text.find(TRACEBACK) {
        Some(end) => text[..end].to_owned(),
        None => text,
    }
}

fn full_text(error: &mlua::Error) -> String {
    match error {
        mlua::Error::RuntimeError(text) => text.clone(),
        mlua::Error::CallbackError { cause, .. } => full_text(cause),
        mlua::Error::WithContext { context, cause } => format!("{context}: {}", full_text(cause)),
        mlua::Error::SyntaxError { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Frame {
    source: String,
    line: Option<i64>,
    name: Option<String>,
    native: bool,
}

impl Frame {
    fn new(source: Option<&str>, line: Option<i64>, name: Option<&str>, native: bool) -> Frame {
        let source = source.unwrap_or("[C]");
        let source = source.strip_prefix('@').or_else(|| source.strip_prefix('=')).unwrap_or(source);
        Frame {
            native: native || source == "[C]",
            source: source.to_owned(),
            line: line.filter(|line| *line > 0),
            name: name.filter(|name| !name.is_empty()).map(str::to_owned),
        }
    }

    fn describe(&self) -> String {
        let place = match self.line {
            Some(line) => format!("{}:{line}", self.source),
            None => self.source.clone(),
        };
        match &self.name {
            Some(name) => format!("{place} in {name}"),
            None => place,
        }
    }

    fn table(&self, lua: &Lua) -> Result<Table> {
        let table = lua.create_table_with_capacity(0, 4)?;
        table.raw_set("Source", self.source.as_str())?;
        table.raw_set("Line", self.line)?;
        table.raw_set("Name", self.name.as_deref())?;
        table.raw_set("IsNative", self.native)?;
        table.set_readonly(true);
        Ok(table)
    }

    fn packet(&self) -> Packet {
        Packet::Table(
            vec![
                (text("Source"), text(&self.source)),
                (text("Line"), self.line.map_or(Packet::Nil, Packet::Integer)),
                (text("Name"), self.name.as_deref().map_or(Packet::Nil, text)),
                (text("IsNative"), Packet::Boolean(self.native)),
            ]
            .into(),
        )
    }

    fn from_packet(packet: &Packet) -> Option<Frame> {
        let source = string(field(packet, "Source")?)?;
        let line = match field(packet, "Line") {
            Some(Packet::Integer(line)) => Some(*line),
            Some(Packet::Number(line)) => Some(*line as i64),
            _ => None,
        };
        let name = field(packet, "Name").and_then(string);
        let native = matches!(field(packet, "IsNative"), Some(Packet::Boolean(true)));
        Some(Frame {
            source,
            line,
            name,
            native,
        })
    }
}

fn text(value: &str) -> Packet {
    Packet::String(value.as_bytes().into())
}

fn string(packet: &Packet) -> Option<String> {
    match packet {
        Packet::String(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}

fn field<'a>(packet: &'a Packet, name: &str) -> Option<&'a Packet> {
    let Packet::Table(entries) = packet else { return None };
    entries
        .iter()
        .find(|(key, _)| matches!(key, Packet::String(key) if &**key == name.as_bytes()))
        .map(|(_, value)| value)
}

struct Record {
    message: String,
    value: Option<Value>,
    frames: Vec<Frame>,
    thread: String,
    caught: bool,
    time: f64,
}

impl Record {
    fn packet(&self) -> Packet {
        let frames: Vec<(Packet, Packet)> = self
            .frames
            .iter()
            .enumerate()
            .map(|(index, frame)| (Packet::Integer(index as i64 + 1), frame.packet()))
            .collect();
        Packet::Table(
            vec![
                (text("Message"), text(&self.message)),
                (text("Thread"), text(&self.thread)),
                (text("Caught"), Packet::Boolean(self.caught)),
                (text("Time"), Packet::Number(self.time)),
                (text("Stack"), Packet::Table(frames.into())),
            ]
            .into(),
        )
    }

    fn from_packet(packet: &Packet) -> Option<Record> {
        let frames = match field(packet, "Stack") {
            Some(Packet::Table(entries)) => entries.iter().filter_map(|(_, frame)| Frame::from_packet(frame)).collect(),
            _ => Vec::new(),
        };
        Some(Record {
            message: string(field(packet, "Message")?)?,
            value: None,
            frames,
            thread: field(packet, "Thread").and_then(string).unwrap_or_else(|| MAIN.to_owned()),
            caught: matches!(field(packet, "Caught"), Some(Packet::Boolean(true))),
            time: match field(packet, "Time") {
                Some(Packet::Number(time)) => *time,
                _ => 0.0,
            },
        })
    }

    fn located(&self) -> (Option<String>, Option<i64>) {
        if let Some(frame) = self.frames.iter().find(|frame| !frame.native && frame.line.is_some()) {
            return (Some(frame.source.clone()), frame.line);
        }
        match position(&self.message) {
            Some((source, line)) => (Some(source), Some(line)),
            None => (None, None),
        }
    }

    fn traceback(&self) -> String {
        traceback(&self.message, &self.frames)
    }
}

fn position(message: &str) -> Option<(String, i64)> {
    let (head, _) = message.split_once(": ")?;
    let (path, line) = head.rsplit_once(':')?;
    let line = line.parse().ok()?;
    (path.ends_with(".luau") || path.ends_with(".lua")).then(|| (path.to_owned(), line))
}

fn traceback(message: &str, frames: &[Frame]) -> String {
    let mut text = message.to_owned();
    for frame in frames {
        text.push_str("\n  at ");
        text.push_str(&frame.describe());
    }
    text
}

fn tidy(mut frames: Vec<Frame>) -> Vec<Frame> {
    while frames.first().is_some_and(|frame| frame.native) {
        frames.remove(0);
    }
    let mut index = 0;
    while index < frames.len() {
        if frames[index].source == CHUNK {
            frames.remove(index);
            if index > 0 && frames[index - 1].native && frames[index - 1].name.as_deref() == Some("xpcall") {
                frames.remove(index - 1);
                index -= 1;
            }
            continue;
        }
        index += 1;
    }
    frames
}

fn current_frames(lua: &Lua, level: usize) -> Vec<Frame> {
    let mut frames = Vec::new();
    for level in level..level + MAX_FRAMES {
        let frame = lua.inspect_stack(level, |debug| {
            let source = debug.source();
            Frame::new(
                source.source.as_deref(),
                debug.current_line().map(|line| line as i64),
                debug.names().name.as_deref(),
                source.what == "C",
            )
        });
        match frame {
            Some(frame) => frames.push(frame),
            None => break,
        }
    }
    frames
}

pub struct Exceptions {
    lua: WeakLua,
    engine: Arc<Engine>,
    thread: String,
    main: bool,
    info: Function,
    tostring: Function,
    raised: AnyUserData,
    guard: Function,
    recent: RefCell<VecDeque<Table>>,
    count: Cell<u64>,
    queue: RefCell<Vec<Record>>,
    flushing: Cell<bool>,
    attempt: Function,
}

type Hub = Rc<Exceptions>;

fn hub(lua: &Lua) -> Result<Hub> {
    lua.app_data_ref::<Hub>()
        .map(|hub| hub.clone())
        .ok_or_else(|| mlua::Error::runtime("the Exception service is not running"))
}

impl Exceptions {
    fn thread_frames(&self, thread: &Thread) -> Vec<Frame> {
        let mut frames = Vec::new();
        for level in 0..MAX_FRAMES {
            let found = self
                .info
                .call::<(Option<String>, Option<i64>, Option<String>)>((thread.clone(), level, "sln"));
            let Ok((Some(source), line, name)) = found else {
                break;
            };
            frames.push(Frame::new(Some(&source), line, name.as_deref(), false));
        }
        tidy(frames)
    }

    fn record(&self, message: String, value: Option<Value>, frames: Vec<Frame>, caught: bool) -> Record {
        Record {
            message,
            value,
            frames,
            thread: self.thread.clone(),
            caught,
            time: self.engine.uptime(),
        }
    }

    fn uncaught(&self, error: &mlua::Error, thread: Option<&Thread>) {
        let frames = thread.map(|thread| self.thread_frames(thread)).unwrap_or_default();
        let record = self.record(error_text(error), None, frames, false);
        if !self.main
            && let Some(main) = self.engine.main_mailbox()
        {
            self.engine.bus().send_to(main, Message::Exception(record.packet()));
        }
        self.push(record);
    }

    fn push(&self, record: Record) {
        self.count.set(self.count.get() + 1);
        self.queue.borrow_mut().push(record);
        if self.flushing.replace(true) {
            return;
        }
        let weak = self.lua.clone();
        tokio::task::spawn_local(async move {
            if let Some(lua) = weak.try_upgrade()
                && let Ok(hub) = hub(&lua)
            {
                hub.flush(&lua);
            }
        });
    }

    fn table(&self, lua: &Lua, record: &Record) -> Result<Table> {
        let table = lua.create_table_with_capacity(0, 9)?;
        let (source, line) = record.located();
        table.raw_set("Message", record.message.as_str())?;
        match &record.value {
            Some(value) => table.raw_set("Value", value.clone())?,
            None => table.raw_set("Value", record.message.as_str())?,
        }
        table.raw_set("Source", source)?;
        table.raw_set("Line", line)?;
        let stack = lua.create_table_with_capacity(record.frames.len(), 0)?;
        for frame in &record.frames {
            stack.raw_push(frame.table(lua)?)?;
        }
        stack.set_readonly(true);
        table.raw_set("Stack", stack)?;
        table.raw_set("Traceback", record.traceback())?;
        table.raw_set("Thread", record.thread.as_str())?;
        table.raw_set("Caught", record.caught)?;
        table.raw_set("Time", record.time)?;
        table.set_readonly(true);
        Ok(table)
    }

    fn flush(&self, lua: &Lua) {
        self.flushing.set(false);
        let records = mem::take(&mut *self.queue.borrow_mut());
        let Ok(scheduler) = Scheduler::get(lua) else {
            return;
        };
        for record in records {
            let table = match self.table(lua, &record) {
                Ok(table) => table,
                Err(error) => {
                    scheduler.report_quiet(error);
                    continue;
                }
            };
            {
                let mut recent = self.recent.borrow_mut();
                if recent.len() == RECENT {
                    recent.pop_front();
                }
                recent.push_back(table.clone());
            }
            let guard = self.guard.clone();
            let fired = Signal::fire_with(
                lua,
                &self.raised,
                MultiValue::from_vec(vec![Value::Table(table)]),
                |scheduler, lua, handler, args| {
                    let mut call = MultiValue::with_capacity(args.len() + 1);
                    call.push_back(Value::Function(handler));
                    call.extend(args);
                    scheduler.spawn(lua, guard.clone(), call);
                },
            );
            if let Err(error) = fired {
                scheduler.report_quiet(error);
            }
        }
    }

    fn describe(&self, value: &Value) -> String {
        match value {
            Value::String(text) => text.to_string_lossy(),
            other => self
                .tostring
                .call::<String>(other.clone())
                .unwrap_or_else(|_| other.type_name().to_owned()),
        }
    }
}

pub fn receive(lua: &Lua, packet: &Packet) {
    if let (Ok(hub), Some(record)) = (hub(lua), Record::from_packet(packet)) {
        hub.push(record);
    }
}

fn guard(lua: &Lua) -> Result<Function> {
    lua.create_async_function(|lua, (handler, args): (Function, MultiValue)| async move {
        if let Err(error) = handler.call_async::<()>(args).await {
            Scheduler::get(&lua)?.report_quiet(error);
        }
        Ok(())
    })
}

fn wrappers(lua: &Lua) -> Result<(Function, Function)> {
    let capture = lua.create_function(|lua, error: Value| {
        let hub = hub(lua)?;
        let frames = tidy(current_frames(lua, 1));
        let message = hub.describe(&error);
        hub.push(hub.record(message, Some(error.clone()), frames, true));
        Ok(error)
    })?;
    let attempt = lua.create_function(|lua, error: Value| {
        let hub = hub(lua)?;
        let frames = tidy(current_frames(lua, 1));
        let message = hub.describe(&error);
        let record = hub.record(message, Some(error), frames, true);
        hub.table(lua, &record)
    })?;
    let xpcall: Function = lua.globals().get("xpcall")?;
    lua.load(
        r#"
        local xpcall, capture, attempt = ...
        local function epcall(body, ...)
            return xpcall(body, capture, ...)
        end
        local function try(body, ...)
            return xpcall(body, attempt, ...)
        end
        return epcall, try
        "#,
    )
    .set_name(format!("={CHUNK}"))
    .call((xpcall, capture, attempt))
}

pub fn install(lua: &Lua, engine: &Arc<Engine>, scheduler: &Scheduler, label: Option<&str>) -> Result<()> {
    let debug: Table = lua.globals().get("debug")?;
    let (epcall, attempt) = wrappers(lua)?;
    let hub = Rc::new(Exceptions {
        lua: lua.weak(),
        engine: engine.clone(),
        thread: label.unwrap_or(MAIN).to_owned(),
        main: label.is_none(),
        info: debug.get("info")?,
        tostring: lua.globals().get("tostring")?,
        raised: lua.create_userdata(Signal::named("Raised"))?,
        guard: guard(lua)?,
        recent: RefCell::new(VecDeque::with_capacity(RECENT)),
        count: Cell::new(0),
        queue: RefCell::new(Vec::new()),
        flushing: Cell::new(false),
        attempt,
    });
    lua.set_app_data(hub.clone());
    let weak = Rc::downgrade(&hub);
    scheduler.set_hook(move |error, thread| {
        if let Some(hub) = weak.upgrade() {
            hub.uncaught(error, thread);
        }
    });
    lua.globals().set("epcall", epcall)
}

pub struct ExceptionApi;

impl UserData for ExceptionApi {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "ExceptionAPI");
        fields.add_field_function_get("Raised", |lua, _| Ok(hub(lua)?.raised.clone()));
        fields.add_field_function_get("Count", |lua, _| Ok(hub(lua)?.count.get()));
        fields.add_field_function_get("Try", |lua, _| Ok(hub(lua)?.attempt.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Exception"));
        methods.add_function("GetStack", |lua, level: Option<usize>| {
            let frames = current_frames(lua, level.unwrap_or(1).max(1));
            let stack = lua.create_table_with_capacity(frames.len(), 0)?;
            for frame in &frames {
                stack.raw_push(frame.table(lua)?)?;
            }
            Ok(stack)
        });
        methods.add_function("Caller", |lua, level: Option<usize>| {
            let level = level.unwrap_or(1).max(1) + 1;
            match current_frames(lua, level).into_iter().next() {
                Some(frame) => Ok(Value::Table(frame.table(lua)?)),
                None => Ok(Value::Nil),
            }
        });
        methods.add_function("Traceback", |lua, (message, level): (Option<Value>, Option<usize>)| {
            let hub = hub(lua)?;
            let message = match &message {
                None | Some(Value::Nil) => String::from("traceback"),
                Some(value) => hub.describe(value),
            };
            Ok(traceback(&message, &current_frames(lua, level.unwrap_or(1).max(1))))
        });
        methods.add_function("GetRecent", |lua, ()| {
            let hub = hub(lua)?;
            let recent = hub.recent.borrow();
            lua.create_sequence_from(recent.iter().cloned())
        });
        methods.add_function("ClearRecent", |lua, ()| {
            hub(lua)?.recent.borrow_mut().clear();
            Ok(())
        });
    }
}

pub fn create(lua: &Lua) -> Result<Value> {
    Ok(Value::UserData(lua.create_userdata(ExceptionApi)?))
}
