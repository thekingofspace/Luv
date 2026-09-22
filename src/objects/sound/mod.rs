mod node;
mod packet;

pub use node::{Port, SoundObject};
pub use packet::AudioPacket;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::mem;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use mlua::{AnyUserData, Lua, MetaMethod, Result, Table, UserData, UserDataFields, UserDataMethods, Value};
use tokio::sync::{Notify, mpsc};

use self::node::{PlayerState, State, StreamState, Transport};
use super::window::fire;
use super::{Asset, GameObject, Signal};
use crate::api::load_asset;
use crate::audio::specs::{FROM_BYTES, FROM_STRING, Family, MODIFIERS, SOUND_NODE, Spec, TO_BYTES, TO_SPEAKER, modifier_spec};
use crate::audio::{
    Action, AudioSystem, Batch, Capture, Event, EventReceiver, GraphId, Listener, MeterShared, NodeId, Pcm, Player,
    PlayerShared, Speaker, Stream, StreamShared, decode, modifier,
};
use crate::runtime::{Engine, Scheduler};
use crate::window::WindowSystem;

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

fn closed() -> mlua::Error {
    runtime("this Sound API belongs to a window that is closed")
}

pub(super) struct Graph {
    audio: Arc<AudioSystem>,
    id: GraphId,
    open: Cell<bool>,
    pending: RefCell<Vec<Action>>,
    wake: Rc<Notify>,
    nodes: RefCell<BTreeMap<NodeId, AnyUserData>>,
    links: RefCell<BTreeSet<(NodeId, NodeId)>>,
    next: Cell<NodeId>,
    volume: Cell<f64>,
    listener: Cell<Listener>,
    held: RefCell<Vec<NodeId>>,
    activation: AnyUserData,
}

impl Graph {
    fn alive(&self) -> Result<()> {
        if self.open.get() { Ok(()) } else { Err(closed()) }
    }

    fn next_id(&self) -> NodeId {
        let id = self.next.get();
        self.next.set(id + 1);
        id
    }

    fn send(&self, action: Action) {
        self.pending.borrow_mut().push(action);
        self.wake.notify_one();
    }

    fn flush(&self) {
        let actions = mem::take(&mut *self.pending.borrow_mut());
        if !actions.is_empty() {
            self.audio.send(Batch {
                graph: self.id,
                actions,
            });
        }
    }

    fn reaches(&self, from: NodeId, to: NodeId) -> bool {
        let links = self.links.borrow();
        let mut stack = vec![from];
        let mut seen = BTreeSet::new();
        while let Some(node) = stack.pop() {
            if node == to {
                return true;
            }
            if !seen.insert(node) {
                continue;
            }
            stack.extend(links.iter().filter(|(source, _)| *source == node).map(|(_, target)| *target));
        }
        false
    }

    fn link(&self, from: NodeId, to: NodeId) -> Result<bool> {
        self.alive()?;
        if from == to {
            return Err(runtime("a sound node cannot link to itself"));
        }
        if self.links.borrow().contains(&(from, to)) {
            return Ok(false);
        }
        if self.reaches(to, from) {
            return Err(runtime("linking these nodes would make the sound loop back into itself"));
        }
        self.links.borrow_mut().insert((from, to));
        self.send(Action::Link(from, to));
        Ok(true)
    }

    fn unlink(&self, from: NodeId, to: NodeId) -> bool {
        if !self.links.borrow_mut().remove(&(from, to)) {
            return false;
        }
        self.send(Action::Unlink(from, to));
        true
    }

    fn detach(&self, node: NodeId) {
        self.nodes.borrow_mut().remove(&node);
        self.links.borrow_mut().retain(|(from, to)| *from != node && *to != node);
        self.held.borrow_mut().retain(|held| *held != node);
        self.send(Action::Remove(node));
    }

    fn players(&self) -> Vec<AnyUserData> {
        self.nodes
            .borrow()
            .values()
            .filter(|node| {
                node.borrow::<SoundObject>()
                    .is_ok_and(|object| object.spec().family == Family::Player)
            })
            .cloned()
            .collect()
    }

    fn close(&self) {
        if !self.open.replace(false) {
            return;
        }
        let nodes: Vec<AnyUserData> = self.nodes.borrow().values().cloned().collect();
        for node in nodes {
            if let Ok(mut object) = node.borrow_mut::<SoundObject>() {
                object.destroy();
            }
        }
        self.nodes.borrow_mut().clear();
        self.links.borrow_mut().clear();
        if let Ok(mut signal) = self.activation.borrow_mut::<Signal>() {
            signal.destroy();
        }
        self.pending.borrow_mut().push(Action::Close);
        self.flush();
    }
}

impl Drop for Graph {
    fn drop(&mut self) {
        if self.open.replace(false) {
            self.pending.borrow_mut().push(Action::Close);
            self.flush();
        }
    }
}

async fn run(lua: Lua, graph: Rc<Graph>, mut events: EventReceiver, connections: mpsc::UnboundedReceiver<bool>) {
    let scheduler = Scheduler::get(&lua).ok();
    let mut connections = Some(connections);
    loop {
        tokio::select! {
            biased;
            _ = graph.wake.notified() => graph.flush(),
            event = events.recv() => match event {
                None | Some(Event::Closed) => break,
                Some(event) => {
                    if let Err(error) = SoundObject::handle(&lua, &graph, event)
                        && let Some(scheduler) = &scheduler
                    {
                        scheduler.report(error);
                    }
                }
            },
            connected = async {
                match connections.as_mut() {
                    Some(connections) => connections.recv().await,
                    None => std::future::pending().await,
                }
            } => match connected {
                Some(connected) => {
                    if graph.open.get()
                        && let Err(error) = fire(&lua, &graph.activation, connected)
                        && let Some(scheduler) = &scheduler
                    {
                        scheduler.report(error);
                    }
                }
                None => connections = None,
            },
        }
    }
    graph.flush();
}

pub struct Sounds {
    system: Arc<dyn WindowSystem>,
    open: Cell<bool>,
    focused: Cell<bool>,
    graph: RefCell<Option<Rc<Graph>>>,
    api: RefCell<Option<AnyUserData>>,
}

impl Sounds {
    pub fn new(system: Arc<dyn WindowSystem>) -> Rc<Sounds> {
        Rc::new(Sounds {
            system,
            open: Cell::new(true),
            focused: Cell::new(false),
            graph: RefCell::new(None),
            api: RefCell::new(None),
        })
    }

    pub fn api(&self, lua: &Lua) -> Result<AnyUserData> {
        if !self.open.get() {
            return Err(closed());
        }
        if let Some(api) = self.api.borrow().as_ref() {
            return Ok(api.clone());
        }
        let audio = self.system.audio();
        let (sender, events) = mpsc::unbounded_channel();
        let connections = audio.subscribe();
        let id = audio.open(sender);
        let graph = Rc::new(Graph {
            audio,
            id,
            open: Cell::new(true),
            pending: RefCell::new(Vec::new()),
            wake: Rc::new(Notify::new()),
            nodes: RefCell::new(BTreeMap::new()),
            links: RefCell::new(BTreeSet::new()),
            next: Cell::new(1),
            volume: Cell::new(1.0),
            listener: Cell::new(Listener::default()),
            held: RefCell::new(Vec::new()),
            activation: lua.create_userdata(Signal::named("ActivationChanged"))?,
        });
        graph.send(Action::Focus(self.focused.get()));
        tokio::task::spawn_local(run(lua.clone(), graph.clone(), events, connections));
        let listener = lua.create_userdata(ListenerApi {
            graph: Rc::downgrade(&graph),
        })?;
        let api = lua.create_userdata(SoundApi {
            graph: Rc::downgrade(&graph),
            listener,
        })?;
        *self.graph.borrow_mut() = Some(graph);
        *self.api.borrow_mut() = Some(api.clone());
        Ok(api)
    }

    pub fn focus(&self, focused: bool) {
        self.focused.set(focused);
        if let Some(graph) = self.graph.borrow().as_ref() {
            graph.send(Action::Focus(focused));
        }
    }

    pub fn close(&self) {
        if !self.open.replace(false) {
            return;
        }
        self.api.borrow_mut().take();
        let graph = self.graph.borrow_mut().take();
        if let Some(graph) = graph {
            graph.close();
        }
    }
}

fn engine(lua: &Lua) -> Result<Arc<Engine>> {
    lua.app_data_ref::<Arc<Engine>>()
        .map(|engine| engine.clone())
        .ok_or_else(|| runtime("the luv engine is not running"))
}

fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase())
}

async fn load_pcm(engine: &Arc<Engine>, key: Option<String>, bytes: Arc<[u8]>, extension: Option<String>) -> Result<Arc<Pcm>> {
    if let Some(key) = &key
        && let Some(pcm) = engine.sounds().get(key)
    {
        return Ok(pcm);
    }
    let named = key.clone();
    let decoded = tokio::task::spawn_blocking(move || decode(bytes, extension.as_deref()))
        .await
        .map_err(mlua::Error::external)?
        .map_err(|error| match &named {
            Some(name) => runtime(format!("cannot load sound '{name}': {error}")),
            None => runtime(format!("cannot load the sound: {error}")),
        })?;
    let pcm = Arc::new(decoded);
    Ok(match key {
        Some(key) => engine.sounds().share(&key, pcm),
        None => pcm,
    })
}

fn player(
    lua: &Lua,
    graph: &Rc<Graph>,
    spec: &'static Spec,
    pcm: Arc<Pcm>,
    asset: Option<String>,
    config: Option<Table>,
) -> Result<AnyUserData> {
    let shared = PlayerShared::new();
    let state = State::Player(PlayerState {
        shared: shared.clone(),
        length: pcm.seconds(),
        rate: pcm.rate(),
        channels: pcm.channels(),
        asset,
        transport: Transport::Stopped,
        epoch: 0,
        hint: 0.0,
    });
    let processor = Box::new(Player::new(pcm, shared, graph.audio.rate() as f32));
    node::create(lua, graph, spec, state, processor, config)
}

pub struct SoundApi {
    graph: Weak<Graph>,
    listener: AnyUserData,
}

impl SoundApi {
    fn graph(&self) -> Result<Rc<Graph>> {
        self.graph.upgrade().filter(|graph| graph.open.get()).ok_or_else(closed)
    }
}

fn graph_of(api: &AnyUserData) -> Result<Rc<Graph>> {
    api.borrow::<SoundApi>()?.graph()
}

impl UserData for SoundApi {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "SoundAPI");
        fields.add_field_method_get("Volume", |_, this| Ok(this.graph()?.volume.get()));
        fields.add_field_method_set("Volume", |_, this, volume: f64| {
            if !volume.is_finite() {
                return Err(runtime("Volume must be a finite number"));
            }
            let graph = this.graph()?;
            let volume = volume.clamp(0.0, 10.0);
            graph.volume.set(volume);
            graph.send(Action::Volume(volume as f32));
            Ok(())
        });
        fields.add_field_method_get("Listener", |_, this| Ok(this.listener.clone()));
        fields.add_field_method_get("SampleRate", |_, this| Ok(this.graph()?.audio.rate()));
        fields.add_field_method_get("DefaultDevice", |_, this| Ok(this.graph()?.audio.device()));
        fields.add_field_method_get("IsConnected", |_, this| Ok(this.graph()?.audio.connected()));
        fields.add_field_method_get("ActivationChanged", |_, this| Ok(this.graph()?.activation.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_function(
            "SoundNode",
            |lua, (api, source, config): (AnyUserData, Value, Option<Table>)| async move {
                let graph = graph_of(&api)?;
                let engine = engine(&lua)?;
                let (key, bytes, kind) = match source {
                    Value::String(path) => {
                        let path = path.to_str()?.to_string();
                        let (relative, data) = load_asset(engine.clone(), path).await?;
                        let kind = extension(&relative);
                        (relative, data, kind)
                    }
                    Value::UserData(userdata) if userdata.is::<Asset>() => {
                        let asset = userdata.borrow::<Asset>()?;
                        (
                            asset.path().to_owned(),
                            asset.data()?,
                            asset.extension().map(str::to_ascii_lowercase),
                        )
                    }
                    other => {
                        return Err(runtime(format!(
                            "SoundNode expects an Asset or an asset path, got {}",
                            other.type_name()
                        )));
                    }
                };
                let pcm = load_pcm(&engine, Some(key.clone()), bytes, kind).await?;
                player(&lua, &graph, &SOUND_NODE, pcm, Some(key), config)
            },
        );
        methods.add_async_function(
            "FromString",
            |lua, (api, data, config): (AnyUserData, Value, Option<Table>)| async move {
                let graph = graph_of(&api)?;
                let engine = engine(&lua)?;
                let bytes: Arc<[u8]> = match data {
                    Value::String(text) => Arc::from(&*text.as_bytes()),
                    Value::Buffer(buffer) => Arc::from(buffer.to_vec()),
                    other => {
                        return Err(runtime(format!(
                            "FromString expects the encoded sound as a string or a buffer, got {}",
                            other.type_name()
                        )));
                    }
                };
                let pcm = load_pcm(&engine, None, bytes, None).await?;
                player(&lua, &graph, &FROM_STRING, pcm, None, config)
            },
        );
        methods.add_function("FromBytes", |lua, (api, config): (AnyUserData, Option<Table>)| {
            let graph = graph_of(&api)?;
            let shared = StreamShared::new();
            let processor = Box::new(Stream::new(shared.clone(), graph.audio.rate() as f32));
            let state = State::Stream(StreamState {
                shared,
                pushed: 0.0,
                floor: 0.0,
            });
            node::create(lua, &graph, &FROM_BYTES, state, processor, config)
        });
        methods.add_function("ToSpeaker", |lua, (api, config): (AnyUserData, Option<Table>)| {
            let graph = graph_of(&api)?;
            let processor = Box::new(Speaker::new(graph.audio.rate() as f32));
            node::create(lua, &graph, &TO_SPEAKER, State::Speaker(None), processor, config)
        });
        methods.add_function("ToBytes", |lua, (api, config): (AnyUserData, Option<Table>)| {
            let graph = graph_of(&api)?;
            let processor = Box::new(Capture::new(graph.audio.rate() as f32));
            node::create(lua, &graph, &TO_BYTES, State::Plain, processor, config)
        });
        methods.add_function(
            "Modifier",
            |lua, (api, kind, config): (AnyUserData, String, Option<Table>)| {
                let graph = graph_of(&api)?;
                let Some(spec) = modifier_spec(&kind) else {
                    let known: Vec<&str> = MODIFIERS.iter().map(|spec| spec.class).collect();
                    return Err(runtime(format!(
                        "'{kind}' is not a sound modifier, the modifiers are {}",
                        known.join(", ")
                    )));
                };
                let meter = (kind == "Meter").then(MeterShared::new);
                let processor = modifier(&kind, graph.audio.rate() as f32, meter.clone())
                    .ok_or_else(|| runtime(format!("'{kind}' is not a sound modifier")))?;
                let state = match (kind.as_str(), meter) {
                    ("Gain", _) => State::Gain(None),
                    (_, Some(meter)) => State::Meter(meter),
                    _ => State::Plain,
                };
                node::create(lua, &graph, spec, state, processor, config)
            },
        );
        methods.add_function("GetNodes", |lua, api: AnyUserData| {
            let graph = graph_of(&api)?;
            let nodes: Vec<AnyUserData> = graph.nodes.borrow().values().cloned().collect();
            lua.create_sequence_from(nodes)
        });
        methods.add_function("StopAll", |lua, api: AnyUserData| {
            let graph = graph_of(&api)?;
            graph.held.borrow_mut().clear();
            for node in graph.players() {
                node::stop(lua, &node)?;
            }
            Ok(())
        });
        methods.add_function("PauseAll", |lua, api: AnyUserData| {
            let graph = graph_of(&api)?;
            for node in graph.players() {
                if node::pause(lua, &node)? {
                    let id = node.borrow::<SoundObject>()?.id();
                    graph.held.borrow_mut().push(id);
                }
            }
            Ok(())
        });
        methods.add_function("ResumeAll", |lua, api: AnyUserData| {
            let graph = graph_of(&api)?;
            let held = mem::take(&mut *graph.held.borrow_mut());
            for id in held {
                let node = graph.nodes.borrow().get(&id).cloned();
                let Some(node) = node else {
                    continue;
                };
                let paused = node
                    .borrow::<SoundObject>()?
                    .player()
                    .is_some_and(|player| player.transport == Transport::Paused);
                if paused {
                    node::resume(lua, &node)?;
                }
            }
            Ok(())
        });
        methods.add_async_function("GetDevices", |lua, api: AnyUserData| async move {
            let audio = graph_of(&api)?.audio.clone();
            let devices = tokio::task::spawn_blocking(move || audio.devices())
                .await
                .map_err(mlua::Error::external)?;
            lua.create_sequence_from(devices)
        });
        methods.add_async_function("DeviceExists", |_, (api, name): (AnyUserData, String)| async move {
            let audio = graph_of(&api)?.audio.clone();
            let devices = tokio::task::spawn_blocking(move || audio.devices())
                .await
                .map_err(mlua::Error::external)?;
            Ok(devices.contains(&name))
        });
    }
}

fn direction(name: &str, value: Value) -> Result<[f32; 3]> {
    let Value::Vector(vector) = value else {
        return Err(runtime(format!("{name} must be a vector, got {}", value.type_name())));
    };
    let values = [vector.x(), vector.y(), vector.z()];
    if values.iter().any(|value| !value.is_finite()) {
        return Err(runtime(format!("{name} must hold finite numbers")));
    }
    Ok(values)
}

fn point(name: &str, value: Value) -> Result<[f32; 3]> {
    match &value {
        Value::UserData(userdata) if userdata.is::<crate::datatypes::UDim>() => {
            let udim = *userdata.borrow::<crate::datatypes::UDim>()?;
            let values = [udim.x as f32, udim.y as f32, udim.z as f32];
            if values.iter().any(|value| !value.is_finite()) {
                return Err(runtime(format!("{name} must hold finite numbers")));
            }
            Ok(values)
        }
        _ => direction(name, value),
    }
}

fn to_vector(values: [f32; 3]) -> mlua::Vector {
    mlua::Vector::new(values[0], values[1], values[2])
}

pub struct ListenerApi {
    graph: Weak<Graph>,
}

impl ListenerApi {
    fn update(&self, change: impl FnOnce(&mut Listener)) -> Result<()> {
        let graph = self.graph.upgrade().filter(|graph| graph.open.get()).ok_or_else(closed)?;
        let mut listener = graph.listener.get();
        change(&mut listener);
        graph.listener.set(listener);
        graph.send(Action::Listener(listener));
        Ok(())
    }

    fn read(&self) -> Result<Listener> {
        self.graph
            .upgrade()
            .filter(|graph| graph.open.get())
            .map(|graph| graph.listener.get())
            .ok_or_else(closed)
    }
}

impl UserData for ListenerApi {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "SoundListener");
        fields.add_field_method_get("Position", |_, this| Ok(to_vector(this.read()?.position)));
        fields.add_field_method_set("Position", |_, this, value: Value| {
            let position = point("Position", value)?;
            this.update(|listener| listener.position = position)
        });
        fields.add_field_method_get("Forward", |_, this| Ok(to_vector(this.read()?.forward)));
        fields.add_field_method_set("Forward", |_, this, value: Value| {
            let forward = direction("Forward", value)?;
            if forward.iter().all(|value| *value == 0.0) {
                return Err(runtime("Forward cannot be a zero vector"));
            }
            this.update(|listener| listener.forward = forward)
        });
        fields.add_field_method_get("Up", |_, this| Ok(to_vector(this.read()?.up)));
        fields.add_field_method_set("Up", |_, this, value: Value| {
            let up = direction("Up", value)?;
            if up.iter().all(|value| *value == 0.0) {
                return Err(runtime("Up cannot be a zero vector"));
            }
            this.update(|listener| listener.up = up)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("SoundListener"));
    }
}
