use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Instant;

use mlua::{
    AnyUserData, FromLua, Lua, MetaMethod, ObjectLike, Result, Table, UserData, UserDataFields, UserDataMethods, Value,
};

use super::Graph;
use super::packet::AudioPacket;
use crate::audio::specs::{Family, Param, Range, SPEAKER_DIRECTION, SPEAKER_POSITION, Spec};
use crate::audio::{
    Action, Event, MeterShared, Message, NodeId, PlayerShared, Processor, RawFormat, Role, SampleFormat, StreamShared,
    parse,
};
use crate::datatypes::{EnumItem, UDim};
use crate::objects::window::fire;
use crate::objects::{BaseGameObject, GameObject, Signal};

const METHODS: &str = "luv.sound.methods";

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Transport {
    Stopped,
    Playing,
    Paused,
}

pub(super) struct PlayerState {
    pub shared: Arc<PlayerShared>,
    pub length: f64,
    pub rate: u32,
    pub channels: usize,
    pub asset: Option<String>,
    pub transport: Transport,
    pub epoch: u64,
    pub hint: f64,
}

impl PlayerState {
    fn position(&self) -> f64 {
        match self.transport {
            Transport::Stopped => self.hint,
            _ if self.shared.epoch() >= self.epoch => self.shared.position(),
            _ => self.hint,
        }
    }
}

pub(super) struct StreamState {
    pub shared: Arc<StreamShared>,
    pub pushed: f64,
    pub floor: f64,
}

pub(super) struct Fade {
    from: f64,
    to: f64,
    started: Instant,
    seconds: f64,
}

impl Fade {
    fn current(&self) -> f64 {
        if self.seconds <= 0.0 {
            return self.to;
        }
        let progress = (self.started.elapsed().as_secs_f64() / self.seconds).min(1.0);
        self.from + (self.to - self.from) * progress
    }
}

pub(super) enum State {
    Player(PlayerState),
    Stream(StreamState),
    Speaker(Option<String>),
    Gain(Option<Fade>),
    Meter(Arc<MeterShared>),
    Plain,
}

pub struct SoundObject {
    base: BaseGameObject,
    id: NodeId,
    spec: &'static Spec,
    graph: Weak<Graph>,
    params: Vec<f64>,
    ports: [Option<AnyUserData>; 2],
    signals: Vec<AnyUserData>,
    state: State,
}

fn role(family: Family) -> Role {
    match family {
        Family::Player | Family::Stream => Role::Source,
        Family::Modifier => Role::Modifier,
        Family::Speaker => Role::Speaker,
        Family::Capture => Role::Capture,
    }
}

pub(super) fn create(
    lua: &Lua,
    graph: &Rc<Graph>,
    spec: &'static Spec,
    state: State,
    mut processor: Box<dyn Processor>,
    config: Option<Table>,
) -> Result<AnyUserData> {
    graph.alive()?;
    let id = graph.next_id();
    for (index, param) in spec.params.iter().enumerate() {
        processor.param(index, param.default);
    }
    let signals = spec
        .signals
        .iter()
        .map(|name| lua.create_userdata(Signal::named(*name)))
        .collect::<Result<Vec<_>>>()?;
    let weak = Rc::downgrade(graph);
    let input = if spec.input {
        Some(lua.create_userdata(Port::<true> {
            graph: weak.clone(),
            node: id,
        })?)
    } else {
        None
    };
    let output = if spec.output {
        Some(lua.create_userdata(Port::<false> {
            graph: weak.clone(),
            node: id,
        })?)
    } else {
        None
    };
    let userdata = lua.create_userdata(SoundObject {
        base: BaseGameObject::new(spec.class),
        id,
        spec,
        graph: weak,
        params: spec.params.iter().map(|param| param.default).collect(),
        ports: [input, output],
        signals,
        state,
    })?;
    graph.nodes.borrow_mut().insert(id, userdata.clone());
    graph.send(Action::Add {
        node: id,
        role: role(spec.family),
        processor,
    });
    if let Some(config) = config {
        let applied = config.pairs::<Value, Value>().try_for_each(|pair| {
            let (key, value) = pair?;
            match key {
                Value::String(key) => userdata.set(key, value),
                other => Err(runtime(format!("config keys must be strings, got {}", other.type_name()))),
            }
        });
        if let Err(error) = applied {
            if let Ok(mut object) = userdata.borrow_mut::<SoundObject>() {
                object.destroy();
            }
            return Err(error);
        }
    }
    Ok(userdata)
}

fn number(param: &Param, value: &Value) -> Result<f64> {
    let number = match value {
        Value::Integer(integer) => *integer as f64,
        Value::Number(number) => *number,
        other => {
            return Err(runtime(format!("{} must be a number, got {}", param.name, other.type_name())));
        }
    };
    if !number.is_finite() {
        return Err(runtime(format!("{} must be a finite number", param.name)));
    }
    Ok(number)
}

fn convert(lua: &Lua, param: &Param, value: Value) -> Result<f64> {
    match param.range {
        Range::Number(min, max) => Ok(number(param, &value)?.clamp(min, max)),
        Range::Integer(min, max) => Ok(number(param, &value)?.round().clamp(min, max)),
        Range::Flag => match value {
            Value::Boolean(flag) => Ok(if flag { 1.0 } else { 0.0 }),
            other => Err(runtime(format!("{} must be a boolean, got {}", param.name, other.type_name()))),
        },
        Range::Choice(enum_type) => Ok(f64::from(EnumItem::from_lua(value, lua)?.of(enum_type)?.value)),
    }
}

fn present(lua: &Lua, param: &Param, value: f64) -> Result<Value> {
    Ok(match param.range {
        Range::Number(..) => Value::Number(value),
        Range::Integer(..) => Value::Integer(value as i64),
        Range::Flag => Value::Boolean(value != 0.0),
        Range::Choice(enum_type) => match EnumItem::named(enum_type, value as u32) {
            Some(item) => item.canonical(lua)?,
            None => Value::Nil,
        },
    })
}

fn vector(values: &[f64]) -> Value {
    Value::Vector(mlua::Vector::new(values[0] as f32, values[1] as f32, values[2] as f32))
}

fn coordinates(name: &str, value: &Value) -> Result<[f64; 3]> {
    let coordinates = match value {
        Value::Vector(vector) => [f64::from(vector.x()), f64::from(vector.y()), f64::from(vector.z())],
        Value::UserData(userdata) if userdata.is::<UDim>() => {
            let udim = *userdata.borrow::<UDim>()?;
            [udim.x, udim.y, udim.z]
        }
        other => {
            return Err(runtime(format!("{name} must be a vector, got {}", other.type_name())));
        }
    };
    if coordinates.iter().any(|value| !value.is_finite()) {
        return Err(runtime(format!("{name} must hold finite numbers")));
    }
    Ok(coordinates)
}

impl SoundObject {
    pub(super) fn id(&self) -> NodeId {
        self.id
    }

    pub(super) fn spec(&self) -> &'static Spec {
        self.spec
    }

    pub(super) fn graph(&self) -> Result<Rc<Graph>> {
        self.graph
            .upgrade()
            .filter(|graph| graph.open.get())
            .ok_or_else(|| runtime("this sound node belongs to a window that is closed"))
    }

    fn alive(&self) -> Result<Rc<Graph>> {
        self.base.ensure_alive()?;
        self.graph()
    }

    pub(super) fn signal(&self, name: &str) -> Option<AnyUserData> {
        self.spec.signal(name).map(|index| self.signals[index].clone())
    }

    pub(super) fn player(&self) -> Option<&PlayerState> {
        match &self.state {
            State::Player(player) => Some(player),
            _ => None,
        }
    }

    pub(super) fn player_mut(&mut self) -> Option<&mut PlayerState> {
        match &mut self.state {
            State::Player(player) => Some(player),
            _ => None,
        }
    }

    fn set_param(&mut self, index: usize, value: f64) -> Result<()> {
        let graph = self.alive()?;
        self.params[index] = value;
        graph.send(Action::Param(self.id, index, value));
        Ok(())
    }

    fn raw_format(&self) -> RawFormat {
        RawFormat {
            rate: self.params[3] as u32,
            channels: self.params[4] as u8,
            format: SampleFormat::from_index(self.params[5] as usize),
        }
    }

    fn special(&self, key: &str) -> Result<Option<Value>> {
        Ok(Some(match (&self.state, key) {
            (State::Player(player), "PlayPosition") => Value::Number(player.position()),
            (State::Player(player), "Length") => Value::Number(player.length),
            (State::Player(player), "IsPlaying") => Value::Boolean(player.transport == Transport::Playing),
            (State::Player(player), "IsPaused") => Value::Boolean(player.transport == Transport::Paused),
            (State::Player(player), "SampleRate") => Value::Integer(i64::from(player.rate)),
            (State::Player(player), "Channels") => Value::Integer(player.channels as i64),
            (State::Stream(stream), "Buffered") => {
                Value::Number((stream.pushed - stream.shared.consumed().max(stream.floor)).max(0.0))
            }
            (State::Stream(stream), "IsPlaying") => Value::Boolean(stream.shared.playing()),
            (State::Speaker(_), "Position") => vector(&self.params[SPEAKER_POSITION..SPEAKER_POSITION + 3]),
            (State::Speaker(_), "Direction") => vector(&self.params[SPEAKER_DIRECTION..SPEAKER_DIRECTION + 3]),
            (State::Gain(Some(fade)), "Volume") => Value::Number(fade.current()),
            (State::Meter(meter), "Peak") => Value::Number(f64::from(meter.peak())),
            (State::Meter(meter), "Loudness") => Value::Number(f64::from(meter.loudness())),
            _ => return Ok(None),
        }))
    }

    fn read_only(&self, key: &str) -> bool {
        matches!(key, "ClassName" | "Input" | "Output")
            || self.spec.signal(key).is_some()
            || self.spec.has_method(key)
            || matches!(
                (&self.state, key),
                (
                    State::Player(_),
                    "Length" | "IsPlaying" | "IsPaused" | "SampleRate" | "Channels" | "Asset"
                ) | (State::Stream(_), "Buffered" | "IsPlaying")
                    | (State::Meter(_), "Peak" | "Loudness")
            )
    }

    fn assign(&mut self, lua: &Lua, key: &str, value: Value) -> Result<()> {
        match (&self.state, key) {
            (State::Player(_), "PlayPosition") => {
                let seconds = number(&self.spec.params[0], &value)
                    .map_err(|_| runtime("PlayPosition must be a finite number of seconds"))?;
                let graph = self.alive()?;
                let id = self.id;
                let Some(player) = self.player_mut() else {
                    return Ok(());
                };
                let to = seconds.clamp(0.0, player.length);
                player.hint = to;
                if player.transport != Transport::Stopped {
                    player.epoch += 1;
                    graph.send(Action::Message(id, Message::Seek { to, epoch: player.epoch }));
                }
                return Ok(());
            }
            (State::Speaker(_), "Position" | "Direction") => {
                let values = coordinates(key, &value)?;
                let start = if key == "Position" { SPEAKER_POSITION } else { SPEAKER_DIRECTION };
                for (offset, value) in values.into_iter().enumerate() {
                    self.set_param(start + offset, value)?;
                }
                return Ok(());
            }
            (State::Speaker(current), "Device") => {
                let device = match value {
                    Value::Nil => None,
                    Value::String(name) => Some(name.to_str()?.to_string()),
                    other => {
                        return Err(runtime(format!("Device must be a device name or nil, got {}", other.type_name())));
                    }
                };
                if *current == device {
                    return Ok(());
                }
                let graph = self.alive()?;
                if let Some(previous) = current {
                    graph.audio.release(previous);
                }
                if let Some(next) = &device {
                    graph.audio.request(next);
                }
                graph.send(Action::Route(self.id, device.clone()));
                self.state = State::Speaker(device);
                return Ok(());
            }
            _ => {}
        }
        if let Some(index) = self.spec.param(key) {
            let converted = convert(lua, &self.spec.params[index], value)?;
            if let State::Gain(fade) = &mut self.state {
                *fade = None;
            }
            return self.set_param(index, converted);
        }
        if self.read_only(key) {
            return Err(runtime(format!("{key} is read-only on {}", self.spec.class)));
        }
        Err(runtime(format!("{key} is not a valid member of {}", self.spec.class)))
    }

    pub(super) fn handle(lua: &Lua, graph: &Graph, event: Event) -> Result<()> {
        let node = match &event {
            Event::Ended { node, .. } | Event::Looped { node, .. } | Event::Packet { node, .. } | Event::Drained { node } => {
                *node
            }
            Event::Garbage(_) | Event::Closed => return Ok(()),
        };
        let Some(userdata) = graph.nodes.borrow().get(&node).cloned() else {
            return Ok(());
        };
        match event {
            Event::Ended { epoch, .. } => {
                let signal = {
                    let mut object = userdata.borrow_mut::<SoundObject>()?;
                    let signal = object.signal("Ended");
                    let Some(player) = object.player_mut() else {
                        return Ok(());
                    };
                    if player.epoch != epoch || player.transport != Transport::Playing {
                        return Ok(());
                    }
                    player.transport = Transport::Stopped;
                    player.hint = 0.0;
                    signal
                };
                signal.map_or(Ok(()), |signal| fire(lua, &signal, ()))
            }
            Event::Looped { epoch, count, .. } => {
                let signal = {
                    let object = userdata.borrow::<SoundObject>()?;
                    if object.player().is_none_or(|player| player.epoch != epoch) {
                        return Ok(());
                    }
                    object.signal("Looped")
                };
                signal.map_or(Ok(()), |signal| fire(lua, &signal, count))
            }
            Event::Packet { packet, .. } => {
                let Some(signal) = userdata.borrow::<SoundObject>()?.signal("OnIncoming") else {
                    return Ok(());
                };
                if !signal.borrow::<Signal>().is_ok_and(|signal| signal.is_listened()) {
                    return Ok(());
                }
                let packet = lua.create_userdata(AudioPacket::new(packet))?;
                fire(lua, &signal, packet)
            }
            Event::Drained { .. } => {
                let signal = userdata.borrow::<SoundObject>()?.signal("Drained");
                signal.map_or(Ok(()), |signal| fire(lua, &signal, ()))
            }
            Event::Garbage(_) | Event::Closed => Ok(()),
        }
    }
}

fn player_call(this: &AnyUserData, name: &str) -> Result<()> {
    let object = this.borrow::<SoundObject>()?;
    object.base.ensure_alive()?;
    if object.spec.family != Family::Player {
        return Err(runtime(format!("{name} is not a valid member of {}", object.spec.class)));
    }
    Ok(())
}

pub(super) fn play(lua: &Lua, this: &AnyUserData) -> Result<()> {
    player_call(this, "Play")?;
    let started = {
        let mut object = this.borrow_mut::<SoundObject>()?;
        let graph = object.alive()?;
        let id = object.id;
        let signal = object.signal("Started");
        let Some(player) = object.player_mut() else {
            return Ok(());
        };
        let from = if player.transport == Transport::Stopped { player.hint } else { 0.0 };
        player.epoch += 1;
        player.transport = Transport::Playing;
        player.hint = from;
        graph.send(Action::Message(
            id,
            Message::Play {
                from,
                fade: from > 0.0,
                epoch: player.epoch,
            },
        ));
        signal
    };
    started.map_or(Ok(()), |signal| fire(lua, &signal, ()))
}

pub(super) fn pause(lua: &Lua, this: &AnyUserData) -> Result<bool> {
    player_call(this, "Pause")?;
    let paused = {
        let mut object = this.borrow_mut::<SoundObject>()?;
        let graph = object.alive()?;
        let id = object.id;
        let signal = object.signal("Paused");
        let Some(player) = object.player_mut() else {
            return Ok(false);
        };
        if player.transport != Transport::Playing {
            return Ok(false);
        }
        player.hint = player.position();
        player.epoch += 1;
        player.transport = Transport::Paused;
        graph.send(Action::Message(id, Message::Pause { epoch: player.epoch }));
        signal
    };
    paused.map_or(Ok(()), |signal| fire(lua, &signal, ()))?;
    Ok(true)
}

pub(super) fn resume(lua: &Lua, this: &AnyUserData) -> Result<()> {
    player_call(this, "Resume")?;
    let resumed = {
        let mut object = this.borrow_mut::<SoundObject>()?;
        let graph = object.alive()?;
        let id = object.id;
        let signal = object.signal("Resumed");
        let Some(player) = object.player_mut() else {
            return Ok(());
        };
        match player.transport {
            Transport::Playing => return Ok(()),
            Transport::Stopped => None,
            Transport::Paused => {
                player.epoch += 1;
                player.transport = Transport::Playing;
                graph.send(Action::Message(id, Message::Resume { epoch: player.epoch }));
                Some(signal)
            }
        }
    };
    match resumed {
        None => play(lua, this),
        Some(signal) => signal.map_or(Ok(()), |signal| fire(lua, &signal, ())),
    }
}

pub(super) fn stop(lua: &Lua, this: &AnyUserData) -> Result<()> {
    player_call(this, "Stop")?;
    let stopped = {
        let mut object = this.borrow_mut::<SoundObject>()?;
        let graph = object.alive()?;
        let id = object.id;
        let signal = object.signal("Stopped");
        let Some(player) = object.player_mut() else {
            return Ok(());
        };
        let was = player.transport;
        player.epoch += 1;
        player.transport = Transport::Stopped;
        player.hint = 0.0;
        graph.send(Action::Message(id, Message::Stop { epoch: player.epoch }));
        signal.filter(|_| was != Transport::Stopped)
    };
    stopped.map_or(Ok(()), |signal| fire(lua, &signal, ()))
}

fn one_shot(this: &AnyUserData, volume: Option<f64>) -> Result<()> {
    player_call(this, "PlayOneShot")?;
    let object = this.borrow::<SoundObject>()?;
    let graph = object.alive()?;
    let volume = volume.unwrap_or(1.0);
    if !volume.is_finite() {
        return Err(runtime("PlayOneShot needs a finite volume"));
    }
    graph.send(Action::Message(
        object.id,
        Message::OneShot {
            volume: volume.clamp(0.0, 10.0) as f32,
        },
    ));
    Ok(())
}

fn push(this: &AnyUserData, data: Value) -> Result<()> {
    let (graph, id, raw) = {
        let object = this.borrow::<SoundObject>()?;
        if object.spec.family != Family::Stream {
            return Err(runtime(format!("Push is not a valid member of {}", object.spec.class)));
        }
        (object.alive()?, object.id, object.raw_format())
    };
    let chunk = match &data {
        Value::String(text) => parse(&text.as_bytes(), raw).map_err(runtime)?,
        Value::Buffer(buffer) => parse(&buffer.to_vec(), raw).map_err(runtime)?,
        Value::UserData(userdata) if userdata.is::<AudioPacket>() => userdata.borrow::<AudioPacket>()?.data().chunk(),
        other => {
            return Err(runtime(format!(
                "Push expects a string, a buffer or an AudioPacket, got {}",
                other.type_name()
            )));
        }
    };
    if chunk.frames() == 0 {
        return Ok(());
    }
    if let State::Stream(stream) = &mut this.borrow_mut::<SoundObject>()?.state {
        stream.pushed += chunk.seconds();
    }
    graph.send(Action::Message(id, Message::Push(chunk)));
    Ok(())
}

fn clear(this: &AnyUserData) -> Result<()> {
    let mut object = this.borrow_mut::<SoundObject>()?;
    if object.spec.family != Family::Stream {
        return Err(runtime(format!("Clear is not a valid member of {}", object.spec.class)));
    }
    let graph = object.alive()?;
    let id = object.id;
    if let State::Stream(stream) = &mut object.state {
        stream.floor = stream.pushed;
        graph.send(Action::Message(id, Message::Clear { consumed: stream.pushed }));
    }
    Ok(())
}

fn fade(this: &AnyUserData, volume: f64, seconds: f64) -> Result<()> {
    let mut object = this.borrow_mut::<SoundObject>()?;
    let graph = object.alive()?;
    let State::Gain(active) = &object.state else {
        return Err(runtime(format!("Fade is not a valid member of {}", object.spec.class)));
    };
    if !volume.is_finite() || !seconds.is_finite() || seconds < 0.0 {
        return Err(runtime("Fade needs a finite volume and a duration of 0 seconds or more"));
    }
    let index = object.spec.param("Volume").unwrap_or(1);
    let from = active.as_ref().map_or(object.params[index], Fade::current);
    let to = volume.clamp(0.0, 10.0);
    object.params[index] = to;
    object.state = State::Gain(Some(Fade {
        from,
        to,
        started: Instant::now(),
        seconds,
    }));
    graph.send(Action::Message(
        object.id,
        Message::Fade {
            to: to as f32,
            seconds: seconds as f32,
        },
    ));
    Ok(())
}

fn method_table(lua: &Lua) -> Result<Table> {
    if let Ok(table) = lua.named_registry_value::<Table>(METHODS) {
        return Ok(table);
    }
    let table = lua.create_table()?;
    table.set("Play", lua.create_function(|lua, this: AnyUserData| play(lua, &this))?)?;
    table.set("Stop", lua.create_function(|lua, this: AnyUserData| stop(lua, &this))?)?;
    table.set("Pause", lua.create_function(|lua, this: AnyUserData| pause(lua, &this).map(|_| ()))?)?;
    table.set("Resume", lua.create_function(|lua, this: AnyUserData| resume(lua, &this))?)?;
    table.set(
        "PlayOneShot",
        lua.create_function(|_, (this, volume): (AnyUserData, Option<f64>)| one_shot(&this, volume))?,
    )?;
    table.set("Push", lua.create_function(|_, (this, data): (AnyUserData, Value)| push(&this, data))?)?;
    table.set("Clear", lua.create_function(|_, this: AnyUserData| clear(&this))?)?;
    table.set(
        "Fade",
        lua.create_function(|_, (this, volume, seconds): (AnyUserData, f64, f64)| fade(&this, volume, seconds))?,
    )?;
    lua.set_named_registry_value(METHODS, table.clone())?;
    Ok(table)
}

impl GameObject for SoundObject {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        for signal in &self.signals {
            if let Ok(mut signal) = signal.borrow_mut::<Signal>() {
                signal.destroy();
            }
        }
        if let Some(graph) = self.graph.upgrade() {
            if let State::Speaker(Some(device)) = &self.state {
                graph.audio.release(device);
            }
            graph.detach(self.id);
        }
        self.state = State::Plain;
    }
}

impl UserData for SoundObject {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_meta_function(MetaMethod::Index, |lua, (this, key): (AnyUserData, String)| {
            let object = this.borrow::<SoundObject>()?;
            object.base.ensure_alive()?;
            match key.as_str() {
                "Input" if object.spec.input => return Ok(object.ports[0].clone().map_or(Value::Nil, Value::UserData)),
                "Output" if object.spec.output => {
                    return Ok(object.ports[1].clone().map_or(Value::Nil, Value::UserData));
                }
                "Device" if matches!(object.state, State::Speaker(_)) => {
                    return match &object.state {
                        State::Speaker(Some(device)) => Ok(Value::String(lua.create_string(device)?)),
                        _ => Ok(Value::Nil),
                    };
                }
                "Asset" if object.player().is_some() => {
                    return match object.player().and_then(|player| player.asset.as_ref()) {
                        Some(asset) => Ok(Value::String(lua.create_string(asset)?)),
                        None => Ok(Value::Nil),
                    };
                }
                _ => {}
            }
            if let Some(value) = object.special(&key)? {
                return Ok(value);
            }
            if let Some(index) = object.spec.param(&key) {
                return present(lua, &object.spec.params[index], object.params[index]);
            }
            if let Some(signal) = object.signal(&key) {
                return Ok(Value::UserData(signal));
            }
            if object.spec.has_method(&key) {
                return method_table(lua)?.get(key.as_str());
            }
            Err(runtime(format!("{key} is not a valid member of {}", object.spec.class)))
        });
        methods.add_meta_function(
            MetaMethod::NewIndex,
            |lua, (this, key, value): (AnyUserData, String, Value)| {
                let mut object = this.borrow_mut::<SoundObject>()?;
                object.base.ensure_alive()?;
                object.assign(lua, &key, value)
            },
        );
    }
}

pub struct Port<const INPUT: bool> {
    graph: Weak<Graph>,
    node: NodeId,
}

impl<const INPUT: bool> Port<INPUT> {
    const NAME: &'static str = if INPUT { "NodeInput" } else { "NodeOutput" };
    const SIDE: &'static str = if INPUT { "Input" } else { "Output" };
    const OTHER: &'static str = if INPUT { "Output" } else { "Input" };

    fn graph(&self) -> Result<Rc<Graph>> {
        self.graph
            .upgrade()
            .filter(|graph| graph.open.get())
            .ok_or_else(|| runtime("this sound node belongs to a window that is closed"))
    }

    fn owner(&self, graph: &Graph) -> Result<AnyUserData> {
        graph
            .nodes
            .borrow()
            .get(&self.node)
            .cloned()
            .ok_or_else(|| runtime("this sound node has been destroyed"))
    }

    fn target(&self, graph: &Rc<Graph>, value: &Value) -> Result<NodeId> {
        let wrong = || {
            runtime(format!(
                "an {} can only link to an {} or to a node that has one",
                Self::SIDE,
                Self::OTHER
            ))
        };
        let (node, owner) = match value {
            Value::UserData(userdata) if userdata.is::<Port<true>>() => {
                if INPUT {
                    return Err(wrong());
                }
                let port = userdata.borrow::<Port<true>>()?;
                (port.node, port.graph.clone())
            }
            Value::UserData(userdata) if userdata.is::<Port<false>>() => {
                if !INPUT {
                    return Err(wrong());
                }
                let port = userdata.borrow::<Port<false>>()?;
                (port.node, port.graph.clone())
            }
            Value::UserData(userdata) if userdata.is::<SoundObject>() => {
                let object = userdata.borrow::<SoundObject>()?;
                object.base.ensure_alive()?;
                let has = if INPUT { object.spec.output } else { object.spec.input };
                if !has {
                    return Err(runtime(format!(
                        "{} has no {} to link to",
                        object.spec.class,
                        Self::OTHER
                    )));
                }
                (object.id, object.graph.clone())
            }
            _ => return Err(wrong()),
        };
        if !Weak::ptr_eq(&owner, &Rc::downgrade(graph)) {
            return Err(runtime("sound nodes from different windows cannot be linked"));
        }
        if !graph.nodes.borrow().contains_key(&node) {
            return Err(runtime("the node to link to has been destroyed"));
        }
        Ok(node)
    }

    fn pair(&self, other: NodeId) -> (NodeId, NodeId) {
        if INPUT { (self.node, other) } else { (other, self.node) }
    }

    fn links(&self, graph: &Graph) -> Vec<NodeId> {
        graph
            .links
            .borrow()
            .iter()
            .filter_map(|(from, to)| {
                if INPUT && *from == self.node {
                    Some(*to)
                } else if !INPUT && *to == self.node {
                    Some(*from)
                } else {
                    None
                }
            })
            .collect()
    }
}

impl<const INPUT: bool> UserData for Port<INPUT> {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::NAME);
        fields.add_field_method_get("Node", |_, this| {
            let graph = this.graph()?;
            this.owner(&graph)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Link", |_, this, other: Value| {
            let graph = this.graph()?;
            this.owner(&graph)?;
            let other = this.target(&graph, &other)?;
            let (from, to) = this.pair(other);
            graph.link(from, to)
        });
        methods.add_method("Unlink", |_, this, other: Option<Value>| {
            let graph = this.graph()?;
            let targets = match other {
                Some(other) => vec![this.target(&graph, &other)?],
                None => this.links(&graph),
            };
            let mut removed = false;
            for target in targets {
                let (from, to) = this.pair(target);
                removed |= graph.unlink(from, to);
            }
            Ok(removed)
        });
        methods.add_method("IsLinked", |_, this, other: Option<Value>| {
            let graph = this.graph()?;
            match other {
                Some(other) => {
                    let (from, to) = this.pair(this.target(&graph, &other)?);
                    Ok(graph.links.borrow().contains(&(from, to)))
                }
                None => Ok(!this.links(&graph).is_empty()),
            }
        });
        methods.add_method("GetLinks", |lua, this, ()| {
            let graph = this.graph()?;
            let nodes = graph.nodes.borrow();
            let ports: Vec<AnyUserData> = this
                .links(&graph)
                .into_iter()
                .filter_map(|node| nodes.get(&node).cloned())
                .filter_map(|node| {
                    let object = node.borrow::<SoundObject>().ok()?;
                    object.ports[if INPUT { 1 } else { 0 }].clone()
                })
                .collect();
            lua.create_sequence_from(ports)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            let class = this
                .graph
                .upgrade()
                .and_then(|graph| {
                    let node = graph.nodes.borrow().get(&this.node).cloned()?;
                    let name = node.borrow::<SoundObject>().ok()?.base().name().to_owned();
                    Some(name)
                })
                .unwrap_or_else(|| "Node".to_owned());
            Ok(format!("{class}.{}", Self::SIDE))
        });
    }
}
