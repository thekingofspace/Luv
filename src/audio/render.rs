use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crossbeam_channel::Receiver;

use super::{
    Action, AudioStatus, BLOCK, Batch, Block, Context, Control, DECLICK, Event, EventSender, GraphId, Listener, NodeId,
    Processor, Ramp, Role, SILENCE, SMOOTHING, apply,
};

const LINK_FADE: f32 = 0.005;
const CLOSE_FADE: f32 = 0.02;
const QUIET: f32 = 1.0e-6;

struct Link {
    from: usize,
    gain: Ramp,
}

struct Slot {
    id: NodeId,
    role: Role,
    processor: Box<dyn Processor>,
    links: Vec<Link>,
    level: Ramp,
    dying: bool,
    silent: bool,
    bus: usize,
    moving: Option<usize>,
}

struct Bus {
    name: String,
    mix: Block,
    outlet: Option<rtrb::Producer<f32>>,
}

struct Graph {
    id: GraphId,
    events: EventSender,
    index: HashMap<NodeId, usize>,
    slots: Vec<Option<Slot>>,
    outputs: Vec<Block>,
    free: Vec<usize>,
    order: Vec<usize>,
    placed: Vec<bool>,
    dirty: bool,
    focused: bool,
    listener: Listener,
    volume: Ramp,
    fade: Ramp,
    closing: bool,
}

fn peak(block: &Block) -> f32 {
    block
        .iter()
        .flat_map(|channel| channel.iter())
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

fn accumulate(target: &mut Block, source: &Block, gains: &[f32; BLOCK]) {
    for frame in 0..BLOCK {
        target[0][frame] += source[0][frame] * gains[frame];
        target[1][frame] += source[1][frame] * gains[frame];
    }
}

impl Graph {
    fn new(id: GraphId, events: EventSender) -> Self {
        Self {
            id,
            events,
            index: HashMap::new(),
            slots: Vec::new(),
            outputs: Vec::new(),
            free: Vec::new(),
            order: Vec::new(),
            placed: Vec::new(),
            dirty: false,
            focused: false,
            listener: Listener::default(),
            volume: Ramp::new(1.0),
            fade: Ramp::new(1.0),
            closing: false,
        }
    }

    fn finished(&self) -> bool {
        self.closing && self.fade.settled() && self.fade.value() == 0.0
    }

    fn slot(&mut self, node: NodeId) -> Option<&mut Slot> {
        let index = *self.index.get(&node)?;
        self.slots[index].as_mut()
    }

    fn route(&mut self, node: NodeId, bus: usize, rate: f32) {
        let Some(slot) = self.slot(node) else {
            return;
        };
        if slot.bus == bus && slot.moving.is_none() {
            return;
        }
        if slot.silent || slot.dying {
            slot.bus = bus;
            slot.moving = None;
        } else {
            slot.moving = Some(bus);
            slot.level.go(0.0, DECLICK * rate);
        }
    }

    fn apply(&mut self, action: Action, rate: f32) {
        match action {
            Action::Open(_) | Action::Route(..) => {}
            Action::Close => {
                self.closing = true;
                self.fade.go(0.0, CLOSE_FADE * rate);
            }
            Action::Focus(focused) => self.focused = focused,
            Action::Volume(volume) => self.volume.go(volume, SMOOTHING * rate),
            Action::Listener(listener) => self.listener = listener,
            Action::Add {
                node,
                role,
                mut processor,
            } => {
                if self.index.contains_key(&node) {
                    return;
                }
                processor.rate(rate);
                let slot = Slot {
                    id: node,
                    role,
                    processor,
                    links: Vec::new(),
                    level: Ramp::new(1.0),
                    dying: false,
                    silent: true,
                    bus: 0,
                    moving: None,
                };
                let index = match self.free.pop() {
                    Some(index) => {
                        self.slots[index] = Some(slot);
                        self.outputs[index] = SILENCE;
                        index
                    }
                    None => {
                        self.slots.push(Some(slot));
                        self.outputs.push(SILENCE);
                        self.slots.len() - 1
                    }
                };
                self.index.insert(node, index);
                self.dirty = true;
            }
            Action::Remove(node) => {
                if let Some(slot) = self.slot(node) {
                    slot.dying = true;
                    slot.moving = None;
                    if slot.silent {
                        slot.level.jump(0.0);
                    } else {
                        slot.level.go(0.0, DECLICK * rate);
                    }
                    self.dirty = true;
                }
            }
            Action::Link(from, to) => {
                let (Some(&source), Some(&target)) = (self.index.get(&from), self.index.get(&to)) else {
                    return;
                };
                let silent = self.slots[source].as_ref().is_none_or(|slot| slot.silent || slot.dying);
                let Some(slot) = self.slots[target].as_mut().filter(|slot| !slot.dying) else {
                    return;
                };
                match slot.links.iter_mut().find(|link| link.from == source) {
                    Some(link) => link.gain.go(1.0, LINK_FADE * rate),
                    None => {
                        let mut gain = Ramp::new(if silent { 1.0 } else { 0.0 });
                        gain.go(1.0, LINK_FADE * rate);
                        slot.links.push(Link { from: source, gain });
                    }
                }
                self.dirty = true;
            }
            Action::Unlink(from, to) => {
                let (Some(&source), Some(&target)) = (self.index.get(&from), self.index.get(&to)) else {
                    return;
                };
                let silent = self.slots[source].as_ref().is_none_or(|slot| slot.silent);
                if let Some(link) = self.slots[target]
                    .as_mut()
                    .and_then(|slot| slot.links.iter_mut().find(|link| link.from == source))
                {
                    if silent {
                        link.gain.jump(0.0);
                    } else {
                        link.gain.go(0.0, LINK_FADE * rate);
                    }
                }
            }
            Action::Param(node, index, value) => {
                if let Some(slot) = self.slot(node) {
                    slot.processor.param(index, value);
                }
            }
            Action::Message(node, message) => {
                let Graph {
                    index: nodes,
                    slots,
                    events,
                    focused,
                    listener,
                    ..
                } = self;
                if let Some(slot) = nodes.get(&node).and_then(|slot| slots[*slot].as_mut()) {
                    let context = Context {
                        rate,
                        node,
                        events,
                        linked: !slot.links.is_empty(),
                        focused: *focused,
                        listener,
                    };
                    slot.processor.message(&context, message);
                }
            }
        }
    }

    fn sort(&mut self) {
        let count = self.slots.len();
        self.order.clear();
        self.placed.clear();
        self.placed.resize(count, false);
        loop {
            let mut progressed = false;
            for index in 0..count {
                if self.placed[index] {
                    continue;
                }
                let ready = match &self.slots[index] {
                    Some(slot) => slot.links.iter().all(|link| self.placed[link.from]),
                    None => {
                        self.placed[index] = true;
                        continue;
                    }
                };
                if ready {
                    self.order.push(index);
                    self.placed[index] = true;
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        for index in 0..count {
            if !self.placed[index] {
                self.order.push(index);
            }
        }
        self.dirty = false;
    }

    fn process(&mut self, rate: f32, input: &mut Block, primary: &mut Block, buses: &mut [Bus]) {
        if self.dirty {
            self.sort();
        }
        let Graph {
            slots,
            outputs,
            order,
            events,
            focused,
            listener,
            ..
        } = self;
        for &index in order.iter() {
            let Some(slot) = slots[index].as_mut() else {
                continue;
            };
            *input = SILENCE;
            for link in slot.links.iter_mut() {
                let source = &outputs[link.from];
                if link.gain.settled() {
                    let gain = link.gain.value();
                    if gain == 0.0 {
                        continue;
                    }
                    for channel in 0..2 {
                        for (sum, sample) in input[channel].iter_mut().zip(source[channel].iter()) {
                            *sum += sample * gain;
                        }
                    }
                } else {
                    for frame in 0..BLOCK {
                        let gain = link.gain.advance();
                        input[0][frame] += source[0][frame] * gain;
                        input[1][frame] += source[1][frame] * gain;
                    }
                }
            }
            slot.links.retain(|link| !(link.gain.settled() && link.gain.target() == 0.0));
            let context = Context {
                rate,
                node: slot.id,
                events,
                linked: !slot.links.is_empty(),
                focused: *focused,
                listener,
            };
            let output = &mut outputs[index];
            slot.processor.process(&context, input, output);
            if !slot.level.settled() || slot.level.value() != 1.0 {
                apply(output, &mut slot.level);
            }
            slot.silent = peak(output) < QUIET;
            if let Some(bus) = slot.moving
                && slot.level.settled()
                && slot.level.value() == 0.0
            {
                slot.bus = bus;
                slot.moving = None;
                slot.level.go(1.0, DECLICK * rate);
            }
        }

        let mut gains = [0.0f32; BLOCK];
        for gain in gains.iter_mut() {
            *gain = self.volume.advance() * self.fade.advance();
        }
        for (index, slot) in self.slots.iter().enumerate() {
            let Some(slot) = slot.as_ref().filter(|slot| slot.role == Role::Speaker) else {
                continue;
            };
            let target = match buses.get_mut(slot.bus.wrapping_sub(1)) {
                Some(bus) if bus.outlet.is_some() => &mut bus.mix,
                _ => &mut *primary,
            };
            accumulate(target, &self.outputs[index], &gains);
        }
        self.collect();
    }

    fn collect(&mut self) {
        for index in 0..self.slots.len() {
            let dead = self.slots[index]
                .as_ref()
                .is_some_and(|slot| slot.dying && slot.level.settled() && slot.level.value() == 0.0);
            if !dead {
                continue;
            }
            let Some(slot) = self.slots[index].take() else {
                continue;
            };
            self.index.remove(&slot.id);
            self.free.push(index);
            for other in self.slots.iter_mut().flatten() {
                other.links.retain(|link| link.from != index);
            }
            self.dirty = true;
            let _ = self.events.send(Event::Garbage(Box::new(slot)));
        }
    }

    fn retire(self) {
        let Graph { slots, events, .. } = self;
        for slot in slots.into_iter().flatten() {
            let _ = events.send(Event::Garbage(Box::new(slot)));
        }
        let _ = events.send(Event::Closed);
    }
}

pub struct Renderer {
    commands: Receiver<Batch>,
    controls: Receiver<Control>,
    status: Arc<AudioStatus>,
    rate: f32,
    graphs: Vec<Graph>,
    buses: Vec<Bus>,
    mix: Block,
    input: Block,
    cursor: usize,
}

impl Renderer {
    pub(super) fn new(commands: Receiver<Batch>, controls: Receiver<Control>, rate: u32, status: Arc<AudioStatus>) -> Self {
        Self {
            commands,
            controls,
            status,
            rate: rate as f32,
            graphs: Vec::new(),
            buses: Vec::new(),
            mix: SILENCE,
            input: SILENCE,
            cursor: BLOCK,
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate as u32
    }

    pub fn set_rate(&mut self, rate: u32) {
        let rate = rate as f32;
        if rate == self.rate || rate <= 0.0 {
            return;
        }
        self.rate = rate;
        for graph in &mut self.graphs {
            for slot in graph.slots.iter_mut().flatten() {
                slot.processor.rate(rate);
            }
        }
    }

    fn bus(&mut self, name: String) -> usize {
        match self.buses.iter().position(|bus| bus.name == name) {
            Some(index) => index + 1,
            None => {
                self.buses.push(Bus {
                    name,
                    mix: SILENCE,
                    outlet: None,
                });
                self.buses.len()
            }
        }
    }

    pub fn drain(&mut self) {
        while let Ok(control) = self.controls.try_recv() {
            match control {
                Control::Attach { name, outlet } => {
                    let index = self.bus(name) - 1;
                    self.buses[index].outlet = Some(outlet);
                }
                Control::Detach { name } => {
                    if let Some(bus) = self.buses.iter_mut().find(|bus| bus.name == name) {
                        bus.outlet = None;
                    }
                }
            }
        }
        while let Ok(batch) = self.commands.try_recv() {
            self.apply(batch);
        }
    }

    fn apply(&mut self, batch: Batch) {
        for action in batch.actions {
            match action {
                Action::Open(events) => {
                    if !self.graphs.iter().any(|graph| graph.id == batch.graph) {
                        self.graphs.push(Graph::new(batch.graph, events));
                        self.status.graphs.store(self.graphs.len(), Ordering::Release);
                    }
                }
                Action::Route(node, device) => {
                    let bus = device.map_or(0, |device| self.bus(device));
                    let rate = self.rate;
                    if let Some(graph) = self.graphs.iter_mut().find(|graph| graph.id == batch.graph) {
                        graph.route(node, bus, rate);
                    }
                }
                action => {
                    let rate = self.rate;
                    if let Some(graph) = self.graphs.iter_mut().find(|graph| graph.id == batch.graph) {
                        graph.apply(action, rate);
                    }
                }
            }
        }
    }

    fn next_block(&mut self) {
        self.drain();
        self.mix = SILENCE;
        let Renderer {
            graphs,
            buses,
            input,
            mix,
            rate,
            ..
        } = self;
        for graph in graphs.iter_mut() {
            graph.process(*rate, input, mix, buses);
        }
        if self.graphs.iter().any(Graph::finished) {
            let (finished, kept): (Vec<Graph>, Vec<Graph>) = self.graphs.drain(..).partition(Graph::finished);
            self.graphs = kept;
            self.status.graphs.store(self.graphs.len(), Ordering::Release);
            for graph in finished {
                graph.retire();
            }
        }
        clamp(&mut self.mix);
        for bus in &mut self.buses {
            let Some(outlet) = bus.outlet.as_mut() else {
                continue;
            };
            clamp(&mut bus.mix);
            if outlet.slots() >= BLOCK * 2 {
                for frame in 0..BLOCK {
                    let _ = outlet.push(bus.mix[0][frame]);
                    let _ = outlet.push(bus.mix[1][frame]);
                }
            }
            bus.mix = SILENCE;
        }
    }

    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        let mut frame = 0;
        while frame < frames {
            if self.cursor >= BLOCK {
                self.next_block();
                self.cursor = 0;
            }
            let count = (BLOCK - self.cursor).min(frames - frame);
            for offset in 0..count {
                let left = self.mix[0][self.cursor + offset];
                let right = self.mix[1][self.cursor + offset];
                let start = (frame + offset) * channels;
                write_frame(&mut out[start..start + channels], left, right);
            }
            self.cursor += count;
            frame += count;
        }
        for sample in &mut out[frames * channels..] {
            *sample = 0.0;
        }
    }
}

const KNEE: f32 = 0.8;

fn soften(sample: f32) -> f32 {
    if !sample.is_finite() {
        return 0.0;
    }
    let size = sample.abs();
    if size <= KNEE {
        return sample;
    }
    let room = 1.0 - KNEE;
    (KNEE + room * ((size - KNEE) / room).tanh()).copysign(sample)
}

fn clamp(block: &mut Block) {
    for channel in block.iter_mut() {
        for sample in channel.iter_mut() {
            *sample = soften(*sample);
        }
    }
}

pub(super) fn write_frame(target: &mut [f32], left: f32, right: f32) {
    if target.len() == 1 {
        target[0] = (left + right) * 0.5;
    } else {
        target[0] = left;
        target[1] = right;
        for extra in &mut target[2..] {
            *extra = 0.0;
        }
    }
}
