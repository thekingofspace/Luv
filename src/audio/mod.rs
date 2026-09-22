mod decode;
mod device;
mod dsp;
mod effects;
mod packet;
mod render;
mod sources;
mod spatial;
pub mod specs;

pub use decode::{Pcm, decode};
pub use effects::{MeterShared, modifier};
pub use packet::{Chunk, PacketData, RawFormat, SampleFormat, parse};
pub use render::Renderer;
pub use sources::{Capture, Player, PlayerShared, Stream, StreamShared};
pub use spatial::Speaker;

use std::collections::{BTreeMap, HashMap};
use std::mem;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use crossbeam_channel::Sender;
use tokio::sync::mpsc;

pub const BLOCK: usize = 128;
pub const DEFAULT_RATE: u32 = 48_000;
pub const DECLICK: f32 = 0.01;
pub const SMOOTHING: f32 = 0.02;

pub type Block = [[f32; BLOCK]; 2];
pub const SILENCE: Block = [[0.0; BLOCK]; 2];

pub type GraphId = u64;
pub type NodeId = u64;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    value: f32,
    target: f32,
    step: f32,
}

impl Ramp {
    pub const fn new(value: f32) -> Self {
        Self {
            value,
            target: value,
            step: 0.0,
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn settled(&self) -> bool {
        self.step == 0.0
    }

    pub fn jump(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.step = 0.0;
    }

    pub fn go(&mut self, target: f32, frames: f32) {
        self.target = target;
        if frames <= 1.0 || self.value == target {
            self.jump(target);
        } else {
            self.step = (target - self.value) / frames;
        }
    }

    #[inline]
    pub fn advance(&mut self) -> f32 {
        if self.step != 0.0 {
            self.value += self.step;
            if (self.step > 0.0 && self.value >= self.target) || (self.step < 0.0 && self.value <= self.target) {
                self.value = self.target;
                self.step = 0.0;
            }
        }
        self.value
    }
}

pub fn frames(block: &mut Block) -> impl Iterator<Item = [&mut f32; 2]> {
    let [left, right] = block;
    left.iter_mut().zip(right.iter_mut()).map(|(left, right)| [left, right])
}

pub fn apply(block: &mut Block, ramp: &mut Ramp) {
    for [left, right] in frames(block) {
        let gain = ramp.advance();
        *left *= gain;
        *right *= gain;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub position: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Source,
    Modifier,
    Speaker,
    Capture,
}

pub enum Message {
    Play { from: f64, fade: bool, epoch: u64 },
    Pause { epoch: u64 },
    Resume { epoch: u64 },
    Stop { epoch: u64 },
    Seek { to: f64, epoch: u64 },
    OneShot { volume: f32 },
    Push(Chunk),
    Clear { consumed: f64 },
    Fade { to: f32, seconds: f32 },
}

pub enum Action {
    Open(EventSender),
    Close,
    Focus(bool),
    Volume(f32),
    Listener(Listener),
    Add {
        node: NodeId,
        role: Role,
        processor: Box<dyn Processor>,
    },
    Remove(NodeId),
    Link(NodeId, NodeId),
    Unlink(NodeId, NodeId),
    Param(NodeId, usize, f64),
    Message(NodeId, Message),
    Route(NodeId, Option<String>),
}

pub struct Batch {
    pub graph: GraphId,
    pub actions: Vec<Action>,
}

pub enum Event {
    Ended { node: NodeId, epoch: u64 },
    Looped { node: NodeId, epoch: u64, count: u64 },
    Packet { node: NodeId, packet: PacketData },
    Drained { node: NodeId },
    Garbage(Box<dyn Send>),
    Closed,
}

pub type EventSender = mpsc::UnboundedSender<Event>;
pub type EventReceiver = mpsc::UnboundedReceiver<Event>;

pub enum Control {
    Attach { name: String, outlet: rtrb::Producer<f32> },
    Detach { name: String },
}

pub struct Context<'a> {
    pub rate: f32,
    pub node: NodeId,
    pub events: &'a EventSender,
    pub linked: bool,
    pub focused: bool,
    pub listener: &'a Listener,
}

impl Context<'_> {
    pub fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }
}

pub trait Processor: Send {
    fn process(&mut self, context: &Context<'_>, input: &Block, output: &mut Block);

    fn param(&mut self, _index: usize, _value: f64) {}

    fn message(&mut self, _context: &Context<'_>, _message: Message) {}

    fn rate(&mut self, _rate: f32) {}
}

pub struct AudioStatus {
    rate: AtomicU32,
    graphs: AtomicUsize,
    connected: AtomicBool,
    device: Mutex<Option<String>>,
    requested: Mutex<BTreeMap<String, usize>>,
    listeners: Mutex<Vec<mpsc::UnboundedSender<bool>>>,
    simulated: Mutex<Vec<String>>,
    captures: Mutex<HashMap<String, Vec<f32>>>,
}

impl AudioStatus {
    fn new(simulated: Vec<String>) -> Self {
        Self {
            rate: AtomicU32::new(DEFAULT_RATE),
            graphs: AtomicUsize::new(0),
            connected: AtomicBool::new(true),
            device: Mutex::new(simulated.first().cloned()),
            requested: Mutex::new(BTreeMap::new()),
            listeners: Mutex::new(Vec::new()),
            simulated: Mutex::new(simulated),
            captures: Mutex::new(HashMap::new()),
        }
    }

    fn set_device(&self, device: Option<String>) {
        *lock(&self.device) = device;
    }

    fn device(&self) -> Option<String> {
        lock(&self.device).clone()
    }

    fn requested(&self) -> Vec<String> {
        lock(&self.requested).keys().cloned().collect()
    }

    fn simulated(&self) -> Vec<String> {
        lock(&self.simulated).clone()
    }

    fn publish(&self, connected: bool) {
        if self.connected.swap(connected, Ordering::AcqRel) == connected {
            return;
        }
        lock(&self.listeners).retain(|listener| listener.send(connected).is_ok());
    }

    fn record(&self, device: &str, samples: &[f32], limit: usize) {
        let mut captures = lock(&self.captures);
        let captured = captures.entry(device.to_owned()).or_default();
        captured.extend_from_slice(samples);
        if captured.len() > limit {
            let excess = captured.len() - limit;
            captured.drain(..excess);
        }
    }
}

pub struct AudioSystem {
    commands: Sender<Batch>,
    wake: Sender<()>,
    status: Arc<AudioStatus>,
    desktop: bool,
}

static NEXT_GRAPH: AtomicU64 = AtomicU64::new(1);

pub const SIMULATED_DEVICES: [&str; 2] = ["Simulated Speakers", "Simulated Headphones"];

impl AudioSystem {
    pub fn system() -> Arc<AudioSystem> {
        static SYSTEM: OnceLock<Arc<AudioSystem>> = OnceLock::new();
        SYSTEM.get_or_init(|| AudioSystem::start(true)).clone()
    }

    pub fn simulated() -> Arc<AudioSystem> {
        AudioSystem::start(false)
    }

    fn start(desktop: bool) -> Arc<AudioSystem> {
        let (commands, receiver) = crossbeam_channel::unbounded();
        let (wake, woken) = crossbeam_channel::unbounded();
        let (control, controls) = crossbeam_channel::unbounded();
        let simulated = if desktop {
            Vec::new()
        } else {
            SIMULATED_DEVICES.iter().map(|name| (*name).to_owned()).collect()
        };
        let status = Arc::new(AudioStatus::new(simulated));
        let renderer = Renderer::new(receiver, controls, DEFAULT_RATE, status.clone());
        device::spawn(renderer, status.clone(), woken, control, desktop);
        Arc::new(AudioSystem {
            commands,
            wake,
            status,
            desktop,
        })
    }

    pub fn rate(&self) -> u32 {
        self.status.rate.load(Ordering::Acquire)
    }

    pub fn device(&self) -> Option<String> {
        self.status.device()
    }

    pub fn connected(&self) -> bool {
        self.status.connected.load(Ordering::Acquire)
    }

    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<bool> {
        let (sender, receiver) = mpsc::unbounded_channel();
        lock(&self.status.listeners).push(sender);
        receiver
    }

    pub fn open(&self, events: EventSender) -> GraphId {
        let graph = NEXT_GRAPH.fetch_add(1, Ordering::Relaxed);
        self.send(Batch {
            graph,
            actions: vec![Action::Open(events)],
        });
        self.wake();
        graph
    }

    pub fn send(&self, batch: Batch) {
        let _ = self.commands.send(batch);
    }

    fn wake(&self) {
        let _ = self.wake.send(());
    }

    pub fn request(&self, device: &str) {
        *lock(&self.status.requested).entry(device.to_owned()).or_insert(0) += 1;
        self.wake();
    }

    pub fn release(&self, device: &str) {
        let mut requested = lock(&self.status.requested);
        if let Some(count) = requested.get_mut(device) {
            *count -= 1;
            if *count == 0 {
                requested.remove(device);
            }
        }
        drop(requested);
        self.wake();
    }

    pub fn devices(&self) -> Vec<String> {
        if self.desktop {
            device::list()
        } else {
            self.status.simulated()
        }
    }

    pub fn set_simulated_devices(&self, devices: Vec<String>) {
        if self.desktop {
            return;
        }
        *lock(&self.status.simulated) = devices;
        self.wake();
    }

    pub fn take_capture(&self, device: Option<&str>) -> Vec<f32> {
        let name = match device {
            Some(device) => device.to_owned(),
            None => match self.status.simulated().first() {
                Some(primary) => primary.clone(),
                None => return Vec::new(),
            },
        };
        lock(&self.status.captures).get_mut(&name).map(mem::take).unwrap_or_default()
    }
}
