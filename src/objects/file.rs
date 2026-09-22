use std::cell::Cell;
use std::io::{self, SeekFrom};
use std::rc::Rc;
use std::sync::Arc;

use mlua::{AnyUserData, Lua, MultiValue, Result, UserData, UserDataFields, UserDataMethods, Value};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Mutex, MutexGuard};

use super::{BaseGameObject, GameObject};

const CHUNK_SIZE: usize = 64 * 1024;
const MAX_NUMERAL: usize = 200;

pub enum Handle {
    File(tokio::fs::File),
    Memory(io::Cursor<Arc<[u8]>>),
    Stdin(tokio::io::Stdin),
    Stdout(tokio::io::Stdout),
    Stderr(tokio::io::Stderr),
    ChildStdin(tokio::process::ChildStdin),
    ChildStdout(tokio::process::ChildStdout),
    ChildStderr(tokio::process::ChildStderr),
}

impl Handle {
    async fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Handle::File(file) => file.read(buffer).await,
            Handle::Memory(cursor) => io::Read::read(cursor, buffer),
            Handle::Stdin(stdin) => stdin.read(buffer).await,
            Handle::ChildStdout(pipe) => pipe.read(buffer).await,
            Handle::ChildStderr(pipe) => pipe.read(buffer).await,
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "file is not readable")),
        }
    }

    async fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        match self {
            Handle::File(file) => {
                file.write_all(data).await?;
                file.flush().await
            }
            Handle::Stdout(stdout) => {
                stdout.write_all(data).await?;
                stdout.flush().await
            }
            Handle::Stderr(stderr) => {
                stderr.write_all(data).await?;
                stderr.flush().await
            }
            Handle::ChildStdin(pipe) => pipe.write_all(data).await,
            Handle::Memory(_) => Err(read_only()),
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "file is not writable")),
        }
    }

    async fn flush(&mut self) -> io::Result<()> {
        match self {
            Handle::File(file) => file.flush().await,
            Handle::Stdout(stdout) => stdout.flush().await,
            Handle::Stderr(stderr) => stderr.flush().await,
            Handle::ChildStdin(pipe) => pipe.flush().await,
            _ => Ok(()),
        }
    }

    async fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        match self {
            Handle::File(file) => file.seek(position).await,
            Handle::Memory(cursor) => io::Seek::seek(cursor, position),
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "cannot seek on this file")),
        }
    }

    fn is_standard(&self) -> bool {
        matches!(self, Handle::Stdin(_) | Handle::Stdout(_) | Handle::Stderr(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Number,
    All,
    Line,
    LineWithEnding,
    Bytes(usize),
}

impl Format {
    pub fn parse(values: &MultiValue, first_argument: usize) -> Result<Vec<Format>> {
        if values.is_empty() {
            return Ok(vec![Format::Line]);
        }
        values
            .iter()
            .enumerate()
            .map(|(index, value)| Format::from_value(value, index + first_argument))
            .collect()
    }

    fn from_value(value: &Value, argument: usize) -> Result<Format> {
        let invalid = || mlua::Error::runtime(format!("bad argument #{argument} to 'read' (invalid format)"));
        match value {
            Value::Integer(count) => usize::try_from(*count).map(Format::Bytes).map_err(|_| invalid()),
            Value::Number(count) if *count >= 0.0 && count.fract() == 0.0 => Ok(Format::Bytes(*count as usize)),
            Value::String(format) => match format.as_bytes().strip_prefix(b"*").unwrap_or(&format.as_bytes()).first() {
                Some(b'n') => Ok(Format::Number),
                Some(b'a') => Ok(Format::All),
                Some(b'l') => Ok(Format::Line),
                Some(b'L') => Ok(Format::LineWithEnding),
                _ => Err(invalid()),
            },
            _ => Err(invalid()),
        }
    }
}

pub enum Chunk {
    Bytes(Vec<u8>),
    Number(f64),
}

pub struct Stream {
    handle: Option<Handle>,
    buffer: Vec<u8>,
    cursor: usize,
    text: bool,
}

impl Stream {
    pub fn new(handle: Handle, text: bool) -> Self {
        Self {
            handle: Some(handle),
            buffer: Vec::new(),
            cursor: 0,
            text,
        }
    }

    pub fn is_open(&self) -> bool {
        self.handle.is_some()
    }

    fn handle(&mut self) -> io::Result<&mut Handle> {
        self.handle.as_mut().ok_or_else(closed_io)
    }

    async fn fill(&mut self) -> io::Result<bool> {
        if self.cursor < self.buffer.len() {
            return Ok(true);
        }
        let Stream { handle, buffer, cursor, .. } = self;
        let handle = handle.as_mut().ok_or_else(closed_io)?;
        buffer.resize(CHUNK_SIZE, 0);
        *cursor = 0;
        match handle.read(buffer).await {
            Ok(read) => {
                buffer.truncate(read);
                Ok(read > 0)
            }
            Err(error) => {
                buffer.clear();
                Err(error)
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.buffer.get(self.cursor).copied()
    }

    pub async fn read(&mut self, format: Format) -> io::Result<Option<Chunk>> {
        Ok(match format {
            Format::Number => self.read_number().await?.map(Chunk::Number),
            Format::All => Some(Chunk::Bytes(self.read_all().await?)),
            Format::Line => self.read_line(false).await?.map(Chunk::Bytes),
            Format::LineWithEnding => self.read_line(true).await?.map(Chunk::Bytes),
            Format::Bytes(count) => self.read_bytes(count).await?.map(Chunk::Bytes),
        })
    }

    async fn read_line(&mut self, keep_ending: bool) -> io::Result<Option<Vec<u8>>> {
        let mut line = Vec::new();
        let mut found = false;
        while self.fill().await? {
            found = true;
            let available = &self.buffer[self.cursor..];
            if let Some(index) = available.iter().position(|byte| *byte == b'\n') {
                line.extend_from_slice(&available[..=index]);
                self.cursor += index + 1;
                break;
            }
            line.extend_from_slice(available);
            self.cursor = self.buffer.len();
        }
        if !found {
            return Ok(None);
        }
        if !keep_ending && line.last() == Some(&b'\n') {
            line.pop();
            if self.text && line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        Ok(Some(line))
    }

    async fn read_all(&mut self) -> io::Result<Vec<u8>> {
        let mut data = Vec::new();
        while self.fill().await? {
            data.extend_from_slice(&self.buffer[self.cursor..]);
            self.cursor = self.buffer.len();
        }
        Ok(data)
    }

    async fn read_bytes(&mut self, count: usize) -> io::Result<Option<Vec<u8>>> {
        if count == 0 {
            return Ok(self.fill().await?.then(Vec::new));
        }
        let mut data = Vec::new();
        while data.len() < count && self.fill().await? {
            let available = &self.buffer[self.cursor..];
            let take = available.len().min(count - data.len());
            data.extend_from_slice(&available[..take]);
            self.cursor += take;
        }
        Ok((!data.is_empty()).then_some(data))
    }

    async fn accept(&mut self, numeral: &mut Vec<u8>, test: impl Fn(u8) -> bool) -> io::Result<bool> {
        if numeral.len() >= MAX_NUMERAL || !self.fill().await? {
            return Ok(false);
        }
        match self.peek() {
            Some(byte) if test(byte) => {
                numeral.push(byte);
                self.cursor += 1;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn read_number(&mut self) -> io::Result<Option<f64>> {
        while self.fill().await? && self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.cursor += 1;
        }
        let mut numeral = Vec::new();
        self.accept(&mut numeral, |byte| byte == b'-' || byte == b'+').await?;
        let mut hex = false;
        if self.accept(&mut numeral, |byte| byte == b'0').await? {
            hex = self.accept(&mut numeral, |byte| byte == b'x' || byte == b'X').await?;
        }
        let digit = move |byte: u8| if hex { byte.is_ascii_hexdigit() } else { byte.is_ascii_digit() };
        while self.accept(&mut numeral, digit).await? {}
        if self.accept(&mut numeral, |byte| byte == b'.').await? {
            while self.accept(&mut numeral, digit).await? {}
        }
        let exponent = move |byte: u8| if hex { byte == b'p' || byte == b'P' } else { byte == b'e' || byte == b'E' };
        if self.accept(&mut numeral, exponent).await? {
            self.accept(&mut numeral, |byte| byte == b'-' || byte == b'+').await?;
            while self.accept(&mut numeral, |byte| byte.is_ascii_digit()).await? {}
        }
        Ok(parse_number(&numeral))
    }

    pub async fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let unread = self.buffer.len() - self.cursor;
        let handle = self.handle()?;
        if unread > 0
            && let Handle::File(file) = handle
        {
            file.seek(SeekFrom::Current(-(unread as i64))).await?;
        }
        self.buffer.clear();
        self.cursor = 0;
        self.handle()?.write_all(data).await
    }

    pub async fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let unread = (self.buffer.len() - self.cursor) as i64;
        let position = match position {
            SeekFrom::Current(offset) => SeekFrom::Current(offset - unread),
            other => other,
        };
        let result = self.handle()?.seek(position).await;
        if result.is_ok() {
            self.buffer.clear();
            self.cursor = 0;
        }
        result
    }

    pub async fn flush(&mut self) -> io::Result<()> {
        self.handle()?.flush().await
    }

    pub async fn close(&mut self) -> io::Result<()> {
        if self.handle()?.is_standard() {
            return Err(io::Error::other("cannot close standard file"));
        }
        let mut handle = self.handle.take().ok_or_else(closed_io)?;
        self.buffer.clear();
        self.cursor = 0;
        handle.flush().await
    }

    fn discard(&mut self) {
        if !self.handle.as_ref().is_some_and(Handle::is_standard) {
            self.handle = None;
        }
        self.buffer.clear();
        self.cursor = 0;
    }
}

pub fn read_only() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "the game's files are read-only")
}

fn closed_io() -> io::Error {
    io::Error::other("attempt to use a closed file")
}

fn closed_file() -> mlua::Error {
    mlua::Error::runtime("attempt to use a closed file")
}

fn parse_number(numeral: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(numeral).ok()?;
    let (sign, body) = match text.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, text.strip_prefix('+').unwrap_or(text)),
    };
    let value = match body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        Some(hex) => parse_hex(hex)?,
        None if body.starts_with(|c: char| c.is_ascii_digit() || c == '.') => body.parse::<f64>().ok()?,
        None => return None,
    };
    Some(sign * value)
}

fn parse_hex(text: &str) -> Option<f64> {
    let (mantissa, exponent) = match text.find(['p', 'P']) {
        Some(index) => (&text[..index], text[index + 1..].parse::<i32>().ok()?),
        None => (text, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let mut value = 0.0f64;
    for digit in whole.chars() {
        value = value * 16.0 + digit.to_digit(16)? as f64;
    }
    let mut scale = 1.0 / 16.0;
    for digit in fraction.chars() {
        value += digit.to_digit(16)? as f64 * scale;
        scale /= 16.0;
    }
    Some(value * 2f64.powi(exponent))
}

pub fn fail(lua: &Lua, error: &io::Error) -> MultiValue {
    let message = lua
        .create_string(error.to_string())
        .map(Value::String)
        .unwrap_or(Value::Nil);
    MultiValue::from_vec(vec![
        Value::Nil,
        message,
        Value::Integer(error.raw_os_error().unwrap_or(0) as i64),
    ])
}

pub struct Shared {
    stream: Mutex<Stream>,
    closing: Cell<bool>,
}

pub struct Session<'a> {
    guard: MutexGuard<'a, Stream>,
    closing: &'a Cell<bool>,
}

impl std::ops::Deref for Session<'_> {
    type Target = Stream;

    fn deref(&self) -> &Stream {
        &self.guard
    }
}

impl std::ops::DerefMut for Session<'_> {
    fn deref_mut(&mut self) -> &mut Stream {
        &mut self.guard
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        if self.closing.get() {
            self.guard.discard();
        }
    }
}

impl Shared {
    pub async fn session(&self) -> Result<Session<'_>> {
        let guard = self.stream.lock().await;
        if !guard.is_open() || self.closing.get() {
            return Err(closed_file());
        }
        Ok(Session {
            guard,
            closing: &self.closing,
        })
    }

    pub async fn read_formats(&self, lua: &Lua, formats: &[Format]) -> Result<std::result::Result<MultiValue, io::Error>> {
        let mut stream = self.session().await?;
        let mut results = MultiValue::with_capacity(formats.len());
        for format in formats {
            match stream.read(*format).await {
                Ok(Some(Chunk::Bytes(bytes))) => results.push_back(Value::String(lua.create_string(bytes)?)),
                Ok(Some(Chunk::Number(number))) => results.push_back(Value::Number(number)),
                Ok(None) => {
                    results.push_back(Value::Nil);
                    break;
                }
                Err(error) => return Ok(Err(error)),
            }
        }
        Ok(Ok(results))
    }

    pub async fn close(&self) -> Result<io::Result<()>> {
        let mut stream = self.session().await?;
        Ok(stream.close().await)
    }
}

pub struct File {
    base: BaseGameObject,
    shared: Rc<Shared>,
}

impl File {
    pub const CLASS_NAME: &'static str = "File";

    pub fn new(name: impl Into<String>, handle: Handle, text: bool) -> Self {
        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(name);
        Self {
            base,
            shared: Rc::new(Shared {
                stream: Mutex::new(Stream::new(handle, text)),
                closing: Cell::new(false),
            }),
        }
    }

    pub fn shared(file: &AnyUserData) -> Result<Rc<Shared>> {
        let this = file.borrow::<File>()?;
        if this.base.is_destroyed() {
            return Err(closed_file());
        }
        Ok(this.shared.clone())
    }

    pub fn is_closed(&self) -> bool {
        self.base.is_destroyed()
            || self.shared.closing.get()
            || self.shared.stream.try_lock().is_ok_and(|stream| !stream.is_open())
    }

    pub async fn read(lua: &Lua, file: &AnyUserData, formats: MultiValue) -> Result<MultiValue> {
        let formats = Format::parse(&formats, 1)?;
        let shared = File::shared(file)?;
        Ok(match shared.read_formats(lua, &formats).await? {
            Ok(results) => results,
            Err(error) => fail(lua, &error),
        })
    }

    pub async fn write(lua: &Lua, file: &AnyUserData, values: MultiValue) -> Result<MultiValue> {
        let mut data = Vec::new();
        for (index, value) in values.into_iter().enumerate() {
            let type_name = value.type_name();
            let writable = matches!(value, Value::String(_) | Value::Integer(_) | Value::Number(_));
            match lua.coerce_string(value)? {
                Some(text) if writable => data.extend_from_slice(&text.as_bytes()),
                _ => {
                    return Err(mlua::Error::runtime(format!(
                        "bad argument #{} to 'write' (string expected, got {type_name})",
                        index + 1
                    )));
                }
            }
        }
        let shared = File::shared(file)?;
        let mut stream = shared.session().await?;
        Ok(match stream.write(&data).await {
            Ok(()) => MultiValue::from_vec(vec![Value::UserData(file.clone())]),
            Err(error) => fail(lua, &error),
        })
    }

    pub fn lines(lua: &Lua, shared: Rc<Shared>, formats: Vec<Format>, close_at_end: bool) -> Result<mlua::Function> {
        lua.create_async_function(move |lua, ()| {
            let shared = shared.clone();
            let formats = formats.clone();
            async move {
                let results = shared
                    .read_formats(&lua, &formats)
                    .await?
                    .map_err(|error| mlua::Error::runtime(error.to_string()))?;
                if close_at_end && results.front().is_none_or(Value::is_nil) {
                    let _ = shared.close().await;
                }
                Ok(results)
            }
        })
    }

    pub async fn close(lua: &Lua, file: &AnyUserData) -> Result<MultiValue> {
        let shared = File::shared(file)?;
        Ok(match shared.close().await? {
            Ok(()) => MultiValue::from_vec(vec![Value::Boolean(true)]),
            Err(error) => fail(lua, &error),
        })
    }
}

impl GameObject for File {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.shared.closing.set(true);
        if let Ok(mut stream) = self.shared.stream.try_lock() {
            stream.discard();
        }
    }
}

impl UserData for File {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);

        methods.add_async_function("read", |lua, (file, formats): (AnyUserData, MultiValue)| async move {
            File::read(&lua, &file, formats).await
        });
        methods.add_async_function("write", |lua, (file, values): (AnyUserData, MultiValue)| async move {
            File::write(&lua, &file, values).await
        });
        methods.add_function("lines", |lua, (file, formats): (AnyUserData, MultiValue)| {
            let formats = Format::parse(&formats, 1)?;
            File::lines(lua, File::shared(&file)?, formats, false)
        });
        methods.add_async_function(
            "seek",
            |lua, (file, whence, offset): (AnyUserData, Option<String>, Option<i64>)| async move {
                let offset = offset.unwrap_or(0);
                let position = match whence.as_deref().unwrap_or("cur") {
                    "set" => SeekFrom::Start(u64::try_from(offset).map_err(|_| {
                        mlua::Error::runtime("bad argument #2 to 'seek' (offset must not be negative)")
                    })?),
                    "cur" => SeekFrom::Current(offset),
                    "end" => SeekFrom::End(offset),
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "bad argument #1 to 'seek' (invalid option '{other}')"
                        )));
                    }
                };
                let shared = File::shared(&file)?;
                let mut stream = shared.session().await?;
                Ok(match stream.seek(position).await {
                    Ok(position) => MultiValue::from_vec(vec![Value::Number(position as f64)]),
                    Err(error) => fail(&lua, &error),
                })
            },
        );
        methods.add_async_function("flush", |lua, file: AnyUserData| async move {
            let shared = File::shared(&file)?;
            let mut stream = shared.session().await?;
            Ok(match stream.flush().await {
                Ok(()) => MultiValue::from_vec(vec![Value::UserData(file.clone())]),
                Err(error) => fail(&lua, &error),
            })
        });
        methods.add_async_function("close", |lua, file: AnyUserData| async move {
            File::close(&lua, &file).await
        });
        methods.add_method("setvbuf", |_, _, (mode, _size): (String, Option<i64>)| match mode.as_str() {
            "no" | "full" | "line" => Ok(true),
            other => Err(mlua::Error::runtime(format!(
                "bad argument #1 to 'setvbuf' (invalid option '{other}')"
            ))),
        });
    }
}
