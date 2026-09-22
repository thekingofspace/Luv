use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::dsp::{Biquad, Shape};
use super::packet::{Chunk, PacketData, SampleFormat};
use super::{BLOCK, Block, Context, DECLICK, Event, Message, Pcm, Processor, Ramp, SILENCE, SMOOTHING, apply, frames};

const TAILS: usize = 8;
const QUIET_PACKET: f32 = 1.0e-4;

pub struct PlayerShared {
    position: AtomicU64,
    epoch: AtomicU64,
}

impl PlayerShared {
    pub fn new() -> Arc<PlayerShared> {
        Arc::new(PlayerShared {
            position: AtomicU64::new(0f64.to_bits()),
            epoch: AtomicU64::new(0),
        })
    }

    pub fn position(&self) -> f64 {
        f64::from_bits(self.position.load(Ordering::Acquire))
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy)]
struct Voice {
    position: f64,
    gain: Ramp,
    volume: f32,
    looping: bool,
    alive: bool,
}

impl Voice {
    const DEAD: Voice = Voice {
        position: 0.0,
        gain: Ramp::new(0.0),
        volume: 1.0,
        looping: false,
        alive: false,
    };
}

enum Step {
    Continue,
    Looped(u64),
    Ended,
}

#[inline]
fn fetch(pcm: &Pcm, index: isize, looping: bool, region: (isize, isize)) -> (f32, f32) {
    let mut index = index;
    if looping && index >= region.1 && region.1 > region.0 {
        index = region.0 + (index - region.1) % (region.1 - region.0);
    }
    if index < 0 || index as usize >= pcm.frames() {
        (0.0, 0.0)
    } else {
        pcm.frame(index as usize)
    }
}

#[inline]
fn hermite(t: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    let c1 = 0.5 * (c - a);
    let c2 = a - 2.5 * b + 2.0 * c - 0.5 * d;
    let c3 = 0.5 * (d - a) + 1.5 * (b - c);
    ((c3 * t + c2) * t + c1) * t + b
}

#[inline]
fn sample(pcm: &Pcm, position: f64, looping: bool, region: (isize, isize)) -> (f32, f32) {
    let base = position.floor();
    let index = base as isize;
    let fraction = (position - base) as f32;
    if fraction == 0.0 {
        return fetch(pcm, index, looping, region);
    }
    let p0 = fetch(pcm, index - 1, looping, region);
    let p1 = fetch(pcm, index, looping, region);
    let p2 = fetch(pcm, index + 1, looping, region);
    let p3 = fetch(pcm, index + 2, looping, region);
    (
        hermite(fraction, p0.0, p1.0, p2.0, p3.0),
        hermite(fraction, p0.1, p1.1, p2.1, p3.1),
    )
}

#[inline]
fn advance(voice: &mut Voice, step: f64, region: (f64, f64), length: f64) -> Step {
    voice.position += step;
    if voice.looping && voice.position >= region.1 {
        let span = region.1 - region.0;
        let mut wraps = 0;
        while voice.position >= region.1 {
            voice.position -= span;
            wraps += 1;
        }
        Step::Looped(wraps)
    } else if voice.position >= length {
        Step::Ended
    } else {
        Step::Continue
    }
}

pub struct Player {
    pcm: Arc<Pcm>,
    shared: Arc<PlayerShared>,
    rate: f32,
    main: Voice,
    playing: bool,
    tails: [Voice; TAILS],
    volume: Ramp,
    speed: f64,
    looping: bool,
    loop_start: f64,
    loop_end: f64,
    epoch: u64,
    loops: u64,
    started: bool,
}

impl Player {
    pub fn new(pcm: Arc<Pcm>, shared: Arc<PlayerShared>, rate: f32) -> Player {
        Player {
            pcm,
            shared,
            rate,
            main: Voice::DEAD,
            playing: false,
            tails: [Voice::DEAD; TAILS],
            volume: Ramp::new(1.0),
            speed: 1.0,
            looping: false,
            loop_start: 0.0,
            loop_end: 0.0,
            epoch: 0,
            loops: 0,
            started: false,
        }
    }

    fn fade(&self) -> f32 {
        DECLICK * self.rate
    }

    fn audible(&self) -> bool {
        self.playing || self.tails.iter().any(|tail| tail.alive)
    }

    fn region(&self) -> (f64, f64) {
        let length = self.pcm.frames() as f64;
        let rate = f64::from(self.pcm.rate());
        let start = (self.loop_start * rate).clamp(0.0, length);
        let end = if self.loop_end > 0.0 {
            (self.loop_end * rate).clamp(0.0, length)
        } else {
            length
        };
        if end - start < 1.0 { (0.0, length) } else { (start, end) }
    }

    fn add_tail(&mut self, voice: Voice) {
        let slot = match self.tails.iter().position(|tail| !tail.alive) {
            Some(slot) => slot,
            None => self
                .tails
                .iter()
                .enumerate()
                .min_by(|a, b| (a.1.gain.value() * a.1.volume).total_cmp(&(b.1.gain.value() * b.1.volume)))
                .map_or(0, |(slot, _)| slot),
        };
        self.tails[slot] = voice;
    }

    fn retire(&mut self) {
        if self.playing && self.main.gain.value() > 0.0 {
            let mut tail = self.main;
            tail.gain.go(0.0, self.fade());
            self.add_tail(tail);
        }
    }

    fn quiet_tails(&mut self) {
        let fade = self.fade();
        for tail in self.tails.iter_mut().filter(|tail| tail.alive) {
            tail.gain.go(0.0, fade);
        }
    }

    fn frames_at(&self, seconds: f64) -> f64 {
        (seconds * f64::from(self.pcm.rate())).clamp(0.0, self.pcm.frames() as f64)
    }

    fn publish(&self) {
        let seconds = self.main.position / f64::from(self.pcm.rate());
        self.shared.position.store(seconds.to_bits(), Ordering::Release);
    }

    fn settle(&mut self, epoch: u64) {
        self.epoch = epoch;
        self.publish();
        self.shared.epoch.store(epoch, Ordering::Release);
    }
}

impl Processor for Player {
    fn process(&mut self, context: &Context<'_>, _input: &Block, output: &mut Block) {
        *output = SILENCE;
        self.started = true;
        let step = f64::from(self.pcm.rate()) / f64::from(self.rate) * self.speed;
        let region = self.region();
        let bounds = (region.0.floor() as isize, region.1.floor() as isize);
        let length = self.pcm.frames() as f64;
        self.main.looping = self.looping;
        if self.playing {
            for [left, right] in frames(output) {
                let gain = self.main.gain.advance();
                if gain > 0.0 {
                    let (first, second) = sample(&self.pcm, self.main.position, self.main.looping, bounds);
                    *left += first * gain;
                    *right += second * gain;
                }
                match advance(&mut self.main, step, region, length) {
                    Step::Continue => {}
                    Step::Looped(wraps) => {
                        self.loops += wraps;
                        context.emit(Event::Looped {
                            node: context.node,
                            epoch: self.epoch,
                            count: self.loops,
                        });
                    }
                    Step::Ended => {
                        self.playing = false;
                        self.main.position = 0.0;
                        self.main.gain.jump(0.0);
                        context.emit(Event::Ended {
                            node: context.node,
                            epoch: self.epoch,
                        });
                        break;
                    }
                }
            }
        }
        let Player { pcm, tails, .. } = self;
        for tail in tails.iter_mut().filter(|tail| tail.alive) {
            for [left, right] in frames(output) {
                let gain = tail.gain.advance() * tail.volume;
                if gain != 0.0 {
                    let (first, second) = sample(pcm, tail.position, tail.looping, bounds);
                    *left += first * gain;
                    *right += second * gain;
                }
                if matches!(advance(tail, step, region, length), Step::Ended)
                    || (tail.gain.settled() && tail.gain.value() == 0.0)
                {
                    tail.alive = false;
                    break;
                }
            }
        }
        if !self.volume.settled() || self.volume.value() != 1.0 {
            apply(output, &mut self.volume);
        }
        self.publish();
    }

    fn param(&mut self, index: usize, value: f64) {
        match index {
            0 if self.started && self.audible() => self.volume.go(value as f32, SMOOTHING * self.rate),
            0 => self.volume.jump(value as f32),
            1 => self.speed = value.max(0.0),
            2 => self.looping = value != 0.0,
            3 => self.loop_start = value.max(0.0),
            4 => self.loop_end = value.max(0.0),
            _ => {}
        }
    }

    fn message(&mut self, _context: &Context<'_>, message: Message) {
        let fade = self.fade();
        match message {
            Message::Play { from, fade: smooth, epoch } => {
                self.retire();
                let mut gain = Ramp::new(if smooth { 0.0 } else { 1.0 });
                gain.go(1.0, fade);
                self.main = Voice {
                    position: self.frames_at(from),
                    gain,
                    volume: 1.0,
                    looping: self.looping,
                    alive: true,
                };
                self.playing = true;
                self.loops = 0;
                self.settle(epoch);
            }
            Message::Pause { epoch } => {
                if self.playing {
                    self.retire();
                    self.quiet_tails();
                    self.main.gain.jump(0.0);
                    self.playing = false;
                }
                self.settle(epoch);
            }
            Message::Resume { epoch } => {
                if !self.playing {
                    self.playing = true;
                    self.main.gain.jump(0.0);
                    self.main.gain.go(1.0, fade);
                }
                self.settle(epoch);
            }
            Message::Stop { epoch } => {
                self.retire();
                self.quiet_tails();
                self.playing = false;
                self.main.position = 0.0;
                self.main.gain.jump(0.0);
                self.settle(epoch);
            }
            Message::Seek { to, epoch } => {
                let target = self.frames_at(to);
                if self.playing {
                    self.retire();
                    self.main.position = target;
                    self.main.gain.jump(0.0);
                    self.main.gain.go(1.0, fade);
                } else {
                    self.main.position = target;
                }
                self.settle(epoch);
            }
            Message::OneShot { volume } => self.add_tail(Voice {
                position: 0.0,
                gain: Ramp::new(1.0),
                volume,
                looping: false,
                alive: true,
            }),
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }
}

pub struct StreamShared {
    consumed: AtomicU64,
    playing: AtomicBool,
}

impl StreamShared {
    pub fn new() -> Arc<StreamShared> {
        Arc::new(StreamShared {
            consumed: AtomicU64::new(0f64.to_bits()),
            playing: AtomicBool::new(false),
        })
    }

    pub fn consumed(&self) -> f64 {
        f64::from_bits(self.consumed.load(Ordering::Acquire))
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
    }
}

pub struct Stream {
    shared: Arc<StreamShared>,
    rate: f32,
    queue: VecDeque<Chunk>,
    offset: f64,
    queued: f64,
    consumed: f64,
    playing: bool,
    envelope: Ramp,
    last: (f32, f32),
    volume: Ramp,
    max_buffered: f64,
    prebuffer: f64,
    started: bool,
}

impl Stream {
    pub fn new(shared: Arc<StreamShared>, rate: f32) -> Stream {
        Stream {
            shared,
            rate,
            queue: VecDeque::with_capacity(64),
            offset: 0.0,
            queued: 0.0,
            consumed: 0.0,
            playing: false,
            envelope: Ramp::new(0.0),
            last: (0.0, 0.0),
            volume: Ramp::new(1.0),
            max_buffered: 1.0,
            prebuffer: 0.05,
            started: false,
        }
    }

    fn front_offset(&self) -> f64 {
        self.queue
            .front()
            .map_or(0.0, |front| self.offset / f64::from(front.rate.max(1)))
    }

    fn remaining(&self) -> f64 {
        (self.queued - self.front_offset()).max(0.0)
    }

    fn pop(&mut self) {
        if let Some(front) = self.queue.pop_front() {
            self.consumed += front.seconds();
            self.queued -= front.seconds();
        }
        if self.queue.is_empty() {
            self.queued = 0.0;
        }
    }

    fn trim(&mut self) {
        while self.queue.len() > 1 && self.remaining() > self.max_buffered {
            self.pop();
            self.offset = 0.0;
        }
    }

    fn read(&mut self) -> Option<(f32, f32)> {
        loop {
            let front = self.queue.front()?;
            let frames = front.frames();
            let rate = front.rate.max(1);
            if self.offset >= frames as f64 {
                self.pop();
                self.offset -= frames as f64;
                if let Some(next) = self.queue.front() {
                    self.offset *= f64::from(next.rate.max(1)) / f64::from(rate);
                } else {
                    self.offset = 0.0;
                }
                continue;
            }
            let index = self.offset as usize;
            let fraction = (self.offset - index as f64) as f32;
            let first = front.frame(index);
            let second = if index + 1 < frames {
                front.frame(index + 1)
            } else {
                self.queue
                    .get(1)
                    .filter(|next| next.frames() > 0)
                    .map_or(first, |next| next.frame(0))
            };
            self.offset += f64::from(rate) / f64::from(self.rate);
            return Some((
                first.0 + (second.0 - first.0) * fraction,
                first.1 + (second.1 - first.1) * fraction,
            ));
        }
    }
}

impl Processor for Stream {
    fn process(&mut self, context: &Context<'_>, _input: &Block, output: &mut Block) {
        *output = SILENCE;
        self.started = true;
        self.trim();
        let fade = DECLICK * self.rate;
        if !self.playing && !self.queue.is_empty() && self.remaining() >= self.prebuffer.min(self.max_buffered * 0.5) {
            self.playing = true;
            self.envelope.jump(0.0);
            self.envelope.go(1.0, fade);
        }
        for [left, right] in frames(output) {
            if self.playing {
                match self.read() {
                    Some(sample) => {
                        self.last = sample;
                        let gain = self.envelope.advance();
                        *left = sample.0 * gain;
                        *right = sample.1 * gain;
                        continue;
                    }
                    None => {
                        self.playing = false;
                        self.envelope.go(0.0, fade);
                        context.emit(Event::Drained { node: context.node });
                    }
                }
            }
            if self.envelope.value() > 0.0 {
                let gain = self.envelope.advance();
                *left = self.last.0 * gain;
                *right = self.last.1 * gain;
            }
        }
        if !self.volume.settled() || self.volume.value() != 1.0 {
            apply(output, &mut self.volume);
        }
        let consumed = self.consumed + self.front_offset();
        self.shared.consumed.store(consumed.to_bits(), Ordering::Release);
        self.shared.playing.store(self.playing, Ordering::Release);
    }

    fn param(&mut self, index: usize, value: f64) {
        match index {
            0 if self.started => self.volume.go(value as f32, SMOOTHING * self.rate),
            0 => self.volume.jump(value as f32),
            1 => self.max_buffered = value.max(0.001),
            2 => self.prebuffer = value.max(0.0),
            _ => {}
        }
    }

    fn message(&mut self, _context: &Context<'_>, message: Message) {
        match message {
            Message::Push(chunk) => {
                self.queued += chunk.seconds();
                self.queue.push_back(chunk);
            }
            Message::Clear { consumed } => {
                self.queue.clear();
                self.queued = 0.0;
                self.offset = 0.0;
                self.consumed = consumed;
                if self.playing {
                    self.playing = false;
                    self.envelope.go(0.0, DECLICK * self.rate);
                }
                self.shared.consumed.store(consumed.to_bits(), Ordering::Release);
            }
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }
}

struct Resampler {
    ratio: f64,
    position: f64,
    previous: (f32, f32),
    filters: [Biquad; 2],
    filtering: bool,
}

impl Resampler {
    fn new(input: f32, output: u32) -> Resampler {
        let mut resampler = Resampler {
            ratio: 1.0,
            position: 0.0,
            previous: (0.0, 0.0),
            filters: [Biquad::default(); 2],
            filtering: false,
        };
        resampler.configure(input, output);
        resampler
    }

    fn configure(&mut self, input: f32, output: u32) {
        self.ratio = f64::from(input) / f64::from(output.max(1));
        self.position = 0.0;
        self.previous = (0.0, 0.0);
        self.filtering = self.ratio > 1.0001;
        if self.filtering {
            let cutoff = output as f32 * 0.45;
            self.filters[0].design(Shape::LowPass, input, cutoff, 0.5412, 0.0);
            self.filters[1].design(Shape::LowPass, input, cutoff, 1.3066, 0.0);
        }
        for filter in &mut self.filters {
            filter.clear();
        }
    }

    fn run(&mut self, input: &Block, out: &mut Vec<f32>) {
        let mut block = *input;
        if self.filtering {
            for (channel, samples) in block.iter_mut().enumerate() {
                for sample in samples.iter_mut() {
                    let first = self.filters[0].run(channel, *sample);
                    *sample = self.filters[1].run(channel, first);
                }
            }
            for filter in &mut self.filters {
                filter.settle();
            }
        }
        if (self.ratio - 1.0).abs() < 1.0e-9 {
            for (left, right) in block[0].iter().zip(block[1].iter()) {
                out.push(*left);
                out.push(*right);
            }
            return;
        }
        while self.position < (BLOCK - 1) as f64 {
            let base = self.position.floor();
            let fraction = (self.position - base) as f32;
            let index = base as isize;
            let first = if index < 0 {
                self.previous
            } else {
                (block[0][index as usize], block[1][index as usize])
            };
            let next = (index + 1) as usize;
            let second = (block[0][next], block[1][next]);
            out.push(first.0 + (second.0 - first.0) * fraction);
            out.push(first.1 + (second.1 - first.1) * fraction);
            self.position += self.ratio;
        }
        self.position -= BLOCK as f64;
        self.previous = (block[0][BLOCK - 1], block[1][BLOCK - 1]);
    }
}

pub struct Capture {
    rate: f32,
    enabled: bool,
    target: u32,
    channels: u8,
    format: SampleFormat,
    duration: f64,
    skip_silence: bool,
    resampler: Resampler,
    pending: Vec<f32>,
    sequence: u32,
}

impl Capture {
    pub fn new(rate: f32) -> Capture {
        Capture {
            rate,
            enabled: true,
            target: 48_000,
            channels: 2,
            format: SampleFormat::Int16,
            duration: 0.02,
            skip_silence: false,
            resampler: Resampler::new(rate, 48_000),
            pending: Vec::with_capacity(8192),
            sequence: 0,
        }
    }
}

impl Processor for Capture {
    fn process(&mut self, context: &Context<'_>, input: &Block, output: &mut Block) {
        *output = SILENCE;
        if !self.enabled || !context.linked {
            return;
        }
        self.resampler.run(input, &mut self.pending);
        let frames = ((self.duration * f64::from(self.target)).round() as usize).max(1);
        while self.pending.len() >= frames * 2 {
            let packet = PacketData::encode(
                &self.pending[..frames * 2],
                self.target,
                self.channels,
                self.format,
                self.sequence,
            );
            self.sequence = self.sequence.wrapping_add(1);
            if !(self.skip_silence && packet.peak < QUIET_PACKET) {
                context.emit(Event::Packet {
                    node: context.node,
                    packet,
                });
            }
            self.pending.drain(..frames * 2);
        }
    }

    fn param(&mut self, index: usize, value: f64) {
        match index {
            0 => {
                self.enabled = value != 0.0;
                if !self.enabled {
                    self.pending.clear();
                }
            }
            1 => {
                self.target = value.round().max(1.0) as u32;
                self.resampler.configure(self.rate, self.target);
                self.pending.clear();
            }
            2 => self.channels = if value >= 2.0 { 2 } else { 1 },
            3 => self.format = SampleFormat::from_index(value as usize),
            4 => self.duration = value.max(0.001),
            5 => self.skip_silence = value != 0.0,
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        if rate != self.rate {
            self.rate = rate;
            self.resampler.configure(rate, self.target);
            self.pending.clear();
        }
    }
}
