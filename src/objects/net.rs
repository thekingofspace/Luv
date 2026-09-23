use std::cell::{Cell, RefCell};
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use mlua::{AnyUserData, IntoLuaMulti, Lua, Result, UserData, UserDataFields, UserDataMethods, UserDataRef, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Notify};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Error as SocketError, Message};

use super::{BaseGameObject, GameObject, Signal};
use crate::runtime::Scheduler;

const READ_SIZE: usize = 64 * 1024;

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

pub fn payload(value: Value) -> Result<Vec<u8>> {
    match value {
        Value::String(text) => Ok(text.as_bytes().to_vec()),
        Value::Buffer(buffer) => Ok(buffer.to_vec()),
        other => Err(runtime(format!("expected a string or buffer to send, got {}", other.type_name()))),
    }
}

fn fire(lua: &Lua, scheduler: &Scheduler, signal: &AnyUserData, args: impl IntoLuaMulti) {
    if !signal.borrow::<Signal>().is_ok_and(|signal| signal.is_listened()) {
        return;
    }
    let fired = args
        .into_lua_multi(lua)
        .and_then(|args| Signal::fire(lua, signal, args));
    if let Err(error) = fired {
        scheduler.report(error);
    }
}

fn signal(lua: &Lua, name: &str) -> Result<AnyUserData> {
    lua.create_userdata(Signal::named(name))
}

fn retire(signals: &[&AnyUserData]) {
    for signal in signals {
        if let Ok(mut signal) = signal.borrow_mut::<Signal>() {
            signal.destroy();
        }
    }
}

struct Link {
    open: Cell<bool>,
    stop: Notify,
}

impl Link {
    fn new() -> Rc<Link> {
        Rc::new(Link {
            open: Cell::new(true),
            stop: Notify::new(),
        })
    }

    fn close(&self) {
        if self.open.replace(false) {
            self.stop.notify_one();
        }
    }

    fn ensure_open(&self, class: &str) -> Result<()> {
        if self.open.get() {
            Ok(())
        } else {
            Err(runtime(format!("the {class} is closed")))
        }
    }
}

pub struct TcpSocket {
    base: BaseGameObject,
    link: Rc<Link>,
    writer: Rc<Mutex<Option<OwnedWriteHalf>>>,
    remote: SocketAddr,
    local: SocketAddr,
    received: AnyUserData,
    closed: AnyUserData,
}

impl TcpSocket {
    pub const CLASS_NAME: &'static str = "TcpSocket";

    pub fn start(lua: &Lua, stream: TcpStream) -> Result<AnyUserData> {
        let scheduler = Scheduler::get(lua)?;
        let remote = stream.peer_addr().map_err(mlua::Error::external)?;
        let local = stream.local_addr().map_err(mlua::Error::external)?;
        let _ = stream.set_nodelay(true);
        let (reader, writer) = stream.into_split();
        let link = Link::new();
        let writer = Rc::new(Mutex::new(Some(writer)));
        let received = signal(lua, "Received")?;
        let closed = signal(lua, "Closed")?;
        let userdata = lua.create_userdata(TcpSocket {
            base: BaseGameObject::new(Self::CLASS_NAME),
            link: link.clone(),
            writer: writer.clone(),
            remote,
            local,
            received: received.clone(),
            closed: closed.clone(),
        })?;
        let lua = lua.clone();
        let reporter = scheduler.clone();
        scheduler.spawn_task(async move {
            let reason = read_stream(&lua, &reporter, reader, &link, &received).await;
            link.open.set(false);
            if let Some(mut writer) = writer.lock().await.take() {
                let _ = writer.shutdown().await;
            }
            fire(&lua, &reporter, &closed, reason);
        });
        Ok(userdata)
    }
}

async fn read_stream(
    lua: &Lua,
    scheduler: &Scheduler,
    mut reader: OwnedReadHalf,
    link: &Link,
    received: &AnyUserData,
) -> String {
    let mut buffer = vec![0; READ_SIZE];
    loop {
        tokio::select! {
            _ = link.stop.notified() => return "closed".to_owned(),
            read = reader.read(&mut buffer) => match read {
                Ok(0) => return "the connection was closed by the other side".to_owned(),
                Ok(count) => match lua.create_string(&buffer[..count]) {
                    Ok(data) => fire(lua, scheduler, received, data),
                    Err(error) => scheduler.report(error),
                },
                Err(error) => return error.to_string(),
            },
        }
    }
}

impl GameObject for TcpSocket {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.link.close();
        retire(&[&self.received, &self.closed]);
    }
}

impl UserData for TcpSocket {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("IsOpen", |_, this| Ok(this.link.open.get()));
        fields.add_field_method_get("RemoteHost", |_, this| Ok(this.remote.ip().to_string()));
        fields.add_field_method_get("RemotePort", |_, this| Ok(this.remote.port()));
        fields.add_field_method_get("LocalPort", |_, this| Ok(this.local.port()));
        fields.add_field_method_get("Received", |_, this| Ok(this.received.clone()));
        fields.add_field_method_get("Closed", |_, this| Ok(this.closed.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_async_method("Send", |_, this: UserDataRef<Self>, data: Value| {
            let (link, writer) = (this.link.clone(), this.writer.clone());
            drop(this);
            async move {
                link.ensure_open("TcpSocket")?;
                let data = payload(data)?;
                let mut writer = writer.lock().await;
                let writer = writer.as_mut().ok_or_else(|| runtime("the TcpSocket is closed"))?;
                writer
                    .write_all(&data)
                    .await
                    .map_err(|error| runtime(format!("cannot send over the TcpSocket: {error}")))
            }
        });
        methods.add_method("Close", |_, this, ()| {
            this.link.close();
            Ok(())
        });
    }
}

pub struct TcpServer {
    base: BaseGameObject,
    link: Rc<Link>,
    address: SocketAddr,
    connected: AnyUserData,
    closed: AnyUserData,
}

impl TcpServer {
    pub const CLASS_NAME: &'static str = "TcpServer";
    pub const WEBSOCKET_CLASS_NAME: &'static str = "WebSocketServer";

    pub fn start(lua: &Lua, listener: TcpListener, websocket: bool) -> Result<AnyUserData> {
        let scheduler = Scheduler::get(lua)?;
        let address = listener.local_addr().map_err(mlua::Error::external)?;
        let link = Link::new();
        let connected = signal(lua, "Connected")?;
        let closed = signal(lua, "Closed")?;
        let class = if websocket { Self::WEBSOCKET_CLASS_NAME } else { Self::CLASS_NAME };
        let userdata = lua.create_userdata(TcpServer {
            base: BaseGameObject::new(class),
            link: link.clone(),
            address,
            connected: connected.clone(),
            closed: closed.clone(),
        })?;
        let lua = lua.clone();
        let reporter = scheduler.clone();
        scheduler.spawn_task(async move {
            loop {
                tokio::select! {
                    _ = link.stop.notified() => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) if websocket => {
                            let lua = lua.clone();
                            let connected = connected.clone();
                            let reporter = reporter.clone();
                            let link = link.clone();
                            tokio::task::spawn_local(async move {
                                match tokio_tungstenite::accept_async(stream).await {
                                    Ok(socket) if link.open.get() => match WebSocket::start(&lua, socket) {
                                        Ok(socket) => fire(&lua, &reporter, &connected, socket),
                                        Err(error) => reporter.report(error),
                                    },
                                    _ => {}
                                }
                            });
                        }
                        Ok((stream, _)) => match TcpSocket::start(&lua, stream) {
                            Ok(socket) => fire(&lua, &reporter, &connected, socket),
                            Err(error) => reporter.report(error),
                        },
                        Err(error) => reporter.report(runtime(format!("the server could not accept a connection: {error}"))),
                    },
                }
            }
            link.open.set(false);
            drop(listener);
            fire(&lua, &reporter, &closed, ());
        });
        Ok(userdata)
    }
}

impl GameObject for TcpServer {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.link.close();
        retire(&[&self.connected, &self.closed]);
    }
}

impl UserData for TcpServer {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("IsOpen", |_, this| Ok(this.link.open.get()));
        fields.add_field_method_get("Host", |_, this| Ok(this.address.ip().to_string()));
        fields.add_field_method_get("Port", |_, this| Ok(this.address.port()));
        fields.add_field_method_get("Connected", |_, this| Ok(this.connected.clone()));
        fields.add_field_method_get("Closed", |_, this| Ok(this.closed.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("Close", |_, this, ()| {
            this.link.close();
            Ok(())
        });
    }
}

pub struct UdpSocket {
    base: BaseGameObject,
    link: Rc<Link>,
    socket: Rc<tokio::net::UdpSocket>,
    address: SocketAddr,
    received: AnyUserData,
    closed: AnyUserData,
}

impl UdpSocket {
    pub const CLASS_NAME: &'static str = "UdpSocket";

    pub fn start(lua: &Lua, socket: tokio::net::UdpSocket) -> Result<AnyUserData> {
        let scheduler = Scheduler::get(lua)?;
        let address = socket.local_addr().map_err(mlua::Error::external)?;
        let socket = Rc::new(socket);
        let link = Link::new();
        let received = signal(lua, "Received")?;
        let closed = signal(lua, "Closed")?;
        let userdata = lua.create_userdata(UdpSocket {
            base: BaseGameObject::new(Self::CLASS_NAME),
            link: link.clone(),
            socket: socket.clone(),
            address,
            received: received.clone(),
            closed: closed.clone(),
        })?;
        let lua = lua.clone();
        let reporter = scheduler.clone();
        scheduler.spawn_task(async move {
            let mut buffer = vec![0; READ_SIZE];
            loop {
                tokio::select! {
                    _ = link.stop.notified() => break,
                    read = socket.recv_from(&mut buffer) => match read {
                        Ok((count, from)) => match lua.create_string(&buffer[..count]) {
                            Ok(data) => fire(&lua, &reporter, &received, (data, from.ip().to_string(), from.port())),
                            Err(error) => reporter.report(error),
                        },
                        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
                        Err(error) => {
                            reporter.report(runtime(format!("the UdpSocket stopped receiving: {error}")));
                            break;
                        }
                    },
                }
            }
            link.open.set(false);
            fire(&lua, &reporter, &closed, ());
        });
        Ok(userdata)
    }
}

impl GameObject for UdpSocket {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.link.close();
        retire(&[&self.received, &self.closed]);
    }
}

impl UserData for UdpSocket {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("IsOpen", |_, this| Ok(this.link.open.get()));
        fields.add_field_method_get("Host", |_, this| Ok(this.address.ip().to_string()));
        fields.add_field_method_get("Port", |_, this| Ok(this.address.port()));
        fields.add_field_method_get("Received", |_, this| Ok(this.received.clone()));
        fields.add_field_method_get("Closed", |_, this| Ok(this.closed.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_async_method(
            "Send",
            |_, this: UserDataRef<Self>, (data, host, port): (Value, String, u16)| {
                let (link, socket) = (this.link.clone(), this.socket.clone());
                drop(this);
                async move {
                    link.ensure_open("UdpSocket")?;
                    let data = payload(data)?;
                    socket
                        .send_to(&data, (host.as_str(), port))
                        .await
                        .map(|_| ())
                        .map_err(|error| runtime(format!("cannot send to {host}:{port}: {error}")))
                }
            },
        );
        methods.add_method("Close", |_, this, ()| {
            this.link.close();
            Ok(())
        });
    }
}

type Outgoing = Pin<Box<dyn Sink<Message, Error = SocketError>>>;
type Incoming = Pin<Box<dyn Stream<Item = std::result::Result<Message, SocketError>>>>;

pub struct WebSocket {
    base: BaseGameObject,
    link: Rc<Link>,
    sink: Rc<Mutex<Option<Outgoing>>>,
    goodbye: Rc<RefCell<Option<(u16, String)>>>,
    received: AnyUserData,
    closed: AnyUserData,
}

impl WebSocket {
    pub const CLASS_NAME: &'static str = "WebSocket";

    pub fn start<S>(lua: &Lua, stream: S) -> Result<AnyUserData>
    where
        S: Sink<Message, Error = SocketError> + Stream<Item = std::result::Result<Message, SocketError>> + 'static,
    {
        let scheduler = Scheduler::get(lua)?;
        let (sink, source) = stream.split();
        let sink: Rc<Mutex<Option<Outgoing>>> = Rc::new(Mutex::new(Some(Box::pin(sink))));
        let mut source: Incoming = Box::pin(source);
        let link = Link::new();
        let goodbye = Rc::new(RefCell::new(None));
        let received = signal(lua, "Received")?;
        let closed = signal(lua, "Closed")?;
        let userdata = lua.create_userdata(WebSocket {
            base: BaseGameObject::new(Self::CLASS_NAME),
            link: link.clone(),
            sink: sink.clone(),
            goodbye: goodbye.clone(),
            received: received.clone(),
            closed: closed.clone(),
        })?;
        let lua = lua.clone();
        let reporter = scheduler.clone();
        scheduler.spawn_task(async move {
            let (code, reason) = loop {
                tokio::select! {
                    _ = link.stop.notified() => {
                        let (code, reason) = goodbye.borrow_mut().take().unwrap_or((1000, String::new()));
                        if let Some(sink) = sink.lock().await.as_mut() {
                            let frame = CloseFrame { code: CloseCode::from(code), reason: reason.clone().into() };
                            let _ = sink.send(Message::Close(Some(frame))).await;
                        }
                        break (code, reason);
                    }
                    message = source.next() => match message {
                        Some(Ok(Message::Text(text))) => match lua.create_string(text.as_bytes()) {
                            Ok(text) => fire(&lua, &reporter, &received, (text, false)),
                            Err(error) => reporter.report(error),
                        },
                        Some(Ok(Message::Binary(data))) => match lua.create_string(&data) {
                            Ok(data) => fire(&lua, &reporter, &received, (data, true)),
                            Err(error) => reporter.report(error),
                        },
                        Some(Ok(Message::Close(frame))) => {
                            break frame.map_or((1005, String::new()), |frame| (u16::from(frame.code), frame.reason.to_string()));
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => break (1006, error.to_string()),
                        None => break (1006, "the connection was closed by the other side".to_owned()),
                    },
                }
            };
            link.open.set(false);
            sink.lock().await.take();
            fire(&lua, &reporter, &closed, (code, reason));
        });
        Ok(userdata)
    }
}

impl GameObject for WebSocket {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.link.close();
        retire(&[&self.received, &self.closed]);
    }
}

impl UserData for WebSocket {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("IsOpen", |_, this| Ok(this.link.open.get()));
        fields.add_field_method_get("Received", |_, this| Ok(this.received.clone()));
        fields.add_field_method_get("Closed", |_, this| Ok(this.closed.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_async_method(
            "Send",
            |_, this: UserDataRef<Self>, (data, binary): (Value, Option<bool>)| {
                let (link, sink) = (this.link.clone(), this.sink.clone());
                drop(this);
                async move {
                    link.ensure_open("WebSocket")?;
                    let message = match data {
                        Value::String(text) if !binary.unwrap_or(false) => match text.to_str() {
                            Ok(text) => Message::text(text.to_string()),
                            Err(_) => Message::binary(text.as_bytes().to_vec()),
                        },
                        other => Message::binary(payload(other)?),
                    };
                    let mut sink = sink.lock().await;
                    let sink = sink.as_mut().ok_or_else(|| runtime("the WebSocket is closed"))?;
                    sink.send(message)
                        .await
                        .map_err(|error| runtime(format!("cannot send over the WebSocket: {error}")))
                }
            },
        );
        methods.add_method("Close", |_, this, (code, reason): (Option<u16>, Option<String>)| {
            if this.link.open.get() {
                *this.goodbye.borrow_mut() = Some((code.unwrap_or(1000), reason.unwrap_or_default()));
                this.link.close();
            }
            Ok(())
        });
    }
}
