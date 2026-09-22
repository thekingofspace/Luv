use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::dsp::{Biquad, DelayLine, Shape, coefficient, decibels, flush, mix_gains};
use super::{BLOCK, Block, Context, DECLICK, Message, Processor, Ramp, SMOOTHING, apply, frames};

trait Effect: Send {
    fn process(&mut self, block: &mut Block);

    fn param(&mut self, index: usize, value: f32);

    fn rate(&mut self, rate: f32);

    fn reset(&mut self) {}

    fn settle(&mut self) {}

    fn message(&mut self, _message: Message) {}
}

struct Modifier {
    effect: Box<dyn Effect>,
    rate: f32,
    enabled: bool,
    wet: Ramp,
    started: bool,
}

impl Processor for Modifier {
    fn process(&mut self, _context: &Context<'_>, input: &Block, output: &mut Block) {
        if !self.started {
            self.started = true;
            self.wet.jump(if self.enabled { 1.0 } else { 0.0 });
            self.effect.settle();
        }
        *output = *input;
        if !self.enabled && self.wet.settled() && self.wet.value() == 0.0 {
            return;
        }
        self.effect.process(output);
        if !self.wet.settled() || self.wet.value() != 1.0 {
            for frame in 0..BLOCK {
                let wet = self.wet.advance();
                for channel in 0..2 {
                    let dry = input[channel][frame];
                    output[channel][frame] = dry + (output[channel][frame] - dry) * wet;
                }
            }
        }
    }

    fn param(&mut self, index: usize, value: f64) {
        if index > 0 {
            self.effect.param(index - 1, value as f32);
            return;
        }
        let enabled = value != 0.0;
        if enabled == self.enabled {
            return;
        }
        self.enabled = enabled;
        if enabled {
            if self.wet.value() == 0.0 {
                self.effect.reset();
            }
            self.wet.go(1.0, DECLICK * self.rate);
        } else {
            self.wet.go(0.0, DECLICK * self.rate);
        }
    }

    fn message(&mut self, _context: &Context<'_>, message: Message) {
        self.effect.message(message);
    }

    fn rate(&mut self, rate: f32) {
        if rate != self.rate {
            self.rate = rate;
            self.effect.rate(rate);
        }
    }
}

struct Gain {
    rate: f32,
    volume: Ramp,
}

impl Effect for Gain {
    fn process(&mut self, block: &mut Block) {
        apply(block, &mut self.volume);
    }

    fn param(&mut self, _index: usize, value: f32) {
        self.volume.go(value, SMOOTHING * self.rate);
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn settle(&mut self) {
        self.volume.jump(self.volume.target());
    }

    fn message(&mut self, message: Message) {
        if let Message::Fade { to, seconds } = message {
            self.volume.go(to, seconds * self.rate);
        }
    }
}

struct Pan {
    rate: f32,
    pan: Ramp,
}

fn pan_frame(pan: f32, left: f32, right: f32) -> (f32, f32) {
    if pan <= 0.0 {
        let angle = (pan + 1.0) * PI / 2.0;
        (left + right * angle.cos(), right * angle.sin())
    } else {
        let angle = pan * PI / 2.0;
        (left * angle.cos(), right + left * angle.sin())
    }
}

impl Effect for Pan {
    fn process(&mut self, block: &mut Block) {
        for [left, right] in frames(block) {
            let pan = self.pan.advance();
            (*left, *right) = pan_frame(pan, *left, *right);
        }
    }

    fn param(&mut self, _index: usize, value: f32) {
        self.pan.go(value.clamp(-1.0, 1.0), SMOOTHING * self.rate);
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn settle(&mut self) {
        self.pan.jump(self.pan.target());
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Frequency,
    Q,
    Gain,
}

struct Smoothed {
    target: f32,
    current: f32,
    logarithmic: bool,
}

impl Smoothed {
    fn new(value: f32, logarithmic: bool) -> Self {
        Self {
            target: value,
            current: value,
            logarithmic,
        }
    }

    fn step(&mut self) -> bool {
        if self.current == self.target {
            return false;
        }
        let next = if self.logarithmic && self.current > 0.0 && self.target > 0.0 {
            self.current * (self.target / self.current).powf(0.3)
        } else {
            self.current + (self.target - self.current) * 0.3
        };
        self.current = if (next - self.target).abs() <= self.target.abs() * 1.0e-4 + 1.0e-4 {
            self.target
        } else {
            next
        };
        true
    }

    fn snap(&mut self) {
        self.current = self.target;
    }
}

struct Filter {
    shape: Shape,
    layout: &'static [Field],
    rate: f32,
    biquad: Biquad,
    frequency: Smoothed,
    q: Smoothed,
    gain: Smoothed,
    fresh: bool,
}

impl Filter {
    fn new(shape: Shape, layout: &'static [Field], rate: f32) -> Self {
        Self {
            shape,
            layout,
            rate,
            biquad: Biquad::default(),
            frequency: Smoothed::new(1000.0, true),
            q: Smoothed::new(0.707, true),
            gain: Smoothed::new(0.0, false),
            fresh: true,
        }
    }

    fn design(&mut self) {
        self.biquad.design(self.shape, self.rate, self.frequency.current, self.q.current, self.gain.current);
    }
}

impl Effect for Filter {
    fn process(&mut self, block: &mut Block) {
        if self.fresh {
            self.frequency.snap();
            self.q.snap();
            self.gain.snap();
            self.design();
            self.fresh = false;
        } else {
            let changed = self.frequency.step() | self.q.step() | self.gain.step();
            if changed {
                self.design();
            }
        }
        for (channel, samples) in block.iter_mut().enumerate() {
            for sample in samples.iter_mut() {
                *sample = self.biquad.run(channel, *sample);
            }
        }
        self.biquad.settle();
    }

    fn param(&mut self, index: usize, value: f32) {
        match self.layout.get(index) {
            Some(Field::Frequency) => self.frequency.target = value,
            Some(Field::Q) => self.q.target = value,
            Some(Field::Gain) => self.gain.target = value,
            None => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        self.fresh = true;
    }

    fn reset(&mut self) {
        self.biquad.clear();
    }
}

struct Equalizer {
    rate: f32,
    low: Biquad,
    mid: Biquad,
    high: Biquad,
    gains: [Smoothed; 3],
    frequencies: [Smoothed; 2],
    fresh: bool,
}

impl Equalizer {
    fn new(rate: f32) -> Self {
        Self {
            rate,
            low: Biquad::default(),
            mid: Biquad::default(),
            high: Biquad::default(),
            gains: [
                Smoothed::new(0.0, false),
                Smoothed::new(0.0, false),
                Smoothed::new(0.0, false),
            ],
            frequencies: [Smoothed::new(400.0, true), Smoothed::new(4000.0, true)],
            fresh: true,
        }
    }

    fn design(&mut self) {
        let low = self.frequencies[0].current.max(10.0);
        let high = self.frequencies[1].current.max(low * 1.01);
        let center = (low * high).sqrt();
        let q = (center / (high - low)).clamp(0.1, 10.0);
        self.low.design(Shape::LowShelf, self.rate, low, 0.707, self.gains[0].current);
        self.mid.design(Shape::Peak, self.rate, center, q, self.gains[1].current);
        self.high.design(Shape::HighShelf, self.rate, high, 0.707, self.gains[2].current);
    }
}

impl Effect for Equalizer {
    fn process(&mut self, block: &mut Block) {
        let mut changed = self.fresh;
        for smoothed in self.gains.iter_mut().chain(self.frequencies.iter_mut()) {
            if self.fresh {
                smoothed.snap();
            } else {
                changed |= smoothed.step();
            }
        }
        self.fresh = false;
        if changed {
            self.design();
        }
        for (channel, samples) in block.iter_mut().enumerate() {
            for sample in samples.iter_mut() {
                let low = self.low.run(channel, *sample);
                let mid = self.mid.run(channel, low);
                *sample = self.high.run(channel, mid);
            }
        }
        self.low.settle();
        self.mid.settle();
        self.high.settle();
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0..=2 => self.gains[index].target = value,
            3 | 4 => self.frequencies[index - 3].target = value,
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        self.fresh = true;
    }

    fn reset(&mut self) {
        self.low.clear();
        self.mid.clear();
        self.high.clear();
    }
}

const MAX_ECHO: f32 = 5.0;

struct Echo {
    rate: f32,
    lines: [DelayLine; 2],
    delay: Ramp,
    feedback: f32,
    mix: f32,
    ping_pong: bool,
}

impl Echo {
    fn new(rate: f32) -> Self {
        let length = (MAX_ECHO * rate) as usize + 8;
        Self {
            rate,
            lines: [DelayLine::new(length), DelayLine::new(length)],
            delay: Ramp::new(0.3),
            feedback: 0.4,
            mix: 0.5,
            ping_pong: false,
        }
    }
}

impl Effect for Echo {
    fn process(&mut self, block: &mut Block) {
        let (dry, wet) = mix_gains(self.mix);
        for [left, right] in frames(block) {
            let delay = (self.delay.advance() * self.rate - 1.0).max(0.0);
            let first = self.lines[0].read(delay);
            let second = self.lines[1].read(delay);
            if self.ping_pong {
                self.lines[0].push(flush((*left + *right) * 0.5 + second * self.feedback));
                self.lines[1].push(flush(first * self.feedback));
            } else {
                self.lines[0].push(flush(*left + first * self.feedback));
                self.lines[1].push(flush(*right + second * self.feedback));
            }
            *left = *left * dry + first * wet;
            *right = *right * dry + second * wet;
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.delay.go(value.clamp(0.001, MAX_ECHO), 0.05 * self.rate),
            1 => self.feedback = value.clamp(0.0, 0.98),
            2 => self.mix = value,
            3 => self.ping_pong = value != 0.0,
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        let length = (MAX_ECHO * rate) as usize + 8;
        for line in &mut self.lines {
            line.fit(length);
        }
    }

    fn reset(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
    }

    fn settle(&mut self) {
        self.delay.jump(self.delay.target());
    }
}

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;

struct Comb {
    buffer: Vec<f32>,
    index: usize,
    store: f32,
}

impl Comb {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
            store: 0.0,
        }
    }

    #[inline]
    fn run(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let output = self.buffer[self.index];
        self.store = flush(output * (1.0 - damp) + self.store * damp);
        self.buffer[self.index] = flush(input + self.store * feedback);
        self.index += 1;
        if self.index == self.buffer.len() {
            self.index = 0;
        }
        output
    }
}

struct AllPass {
    buffer: Vec<f32>,
    index: usize,
}

impl AllPass {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
        }
    }

    #[inline]
    fn run(&mut self, input: f32) -> f32 {
        let stored = self.buffer[self.index];
        let output = stored - input;
        self.buffer[self.index] = flush(input + stored * 0.5);
        self.index += 1;
        if self.index == self.buffer.len() {
            self.index = 0;
        }
        output
    }
}

struct Reverb {
    rate: f32,
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<AllPass>; 2],
    predelay: DelayLine,
    room: f32,
    damping: f32,
    width: f32,
    mix: f32,
    wait: f32,
}

impl Reverb {
    fn new(rate: f32) -> Self {
        let mut reverb = Self {
            rate,
            combs: [Vec::new(), Vec::new()],
            allpasses: [Vec::new(), Vec::new()],
            predelay: DelayLine::new(8),
            room: 0.6,
            damping: 0.5,
            width: 1.0,
            mix: 0.35,
            wait: 0.02,
        };
        reverb.build();
        reverb
    }

    fn build(&mut self) {
        let scale = self.rate / 44_100.0;
        let size = |length: usize, channel: usize| ((length + channel * SPREAD) as f32 * scale) as usize;
        for channel in 0..2 {
            self.combs[channel] = COMBS.iter().map(|length| Comb::new(size(*length, channel))).collect();
            self.allpasses[channel] = ALLPASSES.iter().map(|length| AllPass::new(size(*length, channel))).collect();
        }
        self.predelay = DelayLine::new((0.5 * self.rate) as usize + 8);
    }
}

impl Effect for Reverb {
    fn process(&mut self, block: &mut Block) {
        let (dry, wet) = mix_gains(self.mix);
        let feedback = self.room * 0.28 + 0.7;
        let damp = self.damping * 0.4;
        let first = wet * (self.width / 2.0 + 0.5);
        let second = wet * ((1.0 - self.width) / 2.0);
        let wait = self.wait * self.rate;
        for [left, right] in frames(block) {
            self.predelay.push((*left + *right) * 0.015);
            let input = self.predelay.read(wait);
            let mut outputs = [0.0f32; 2];
            for (channel, output) in outputs.iter_mut().enumerate() {
                let mut sum = 0.0;
                for comb in self.combs[channel].iter_mut() {
                    sum += comb.run(input, feedback, damp);
                }
                for allpass in self.allpasses[channel].iter_mut() {
                    sum = allpass.run(sum);
                }
                *output = sum;
            }
            *left = outputs[0] * first + outputs[1] * second + *left * dry;
            *right = outputs[1] * first + outputs[0] * second + *right * dry;
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.room = value.clamp(0.0, 1.0),
            1 => self.damping = value.clamp(0.0, 1.0),
            2 => self.width = value.clamp(0.0, 1.0),
            3 => self.mix = value,
            4 => self.wait = value.clamp(0.0, 0.5),
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        self.build();
    }

    fn reset(&mut self) {
        for channel in 0..2 {
            for comb in &mut self.combs[channel] {
                comb.buffer.fill(0.0);
                comb.store = 0.0;
            }
            for allpass in &mut self.allpasses[channel] {
                allpass.buffer.fill(0.0);
            }
        }
        self.predelay.clear();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sweep {
    Rate,
    Depth,
    Feedback,
    Mix,
}

struct Modulated {
    rate: f32,
    layout: &'static [Sweep],
    lines: [DelayLine; 2],
    phase: f32,
    speed: f32,
    depth: f32,
    feedback: f32,
    mix: f32,
    base: f32,
    sweep: f32,
    spread: f32,
}

impl Modulated {
    fn new(rate: f32, layout: &'static [Sweep], base: f32, sweep: f32, spread: f32) -> Self {
        let length = ((base + sweep) * rate) as usize + 16;
        Self {
            rate,
            layout,
            lines: [DelayLine::new(length), DelayLine::new(length)],
            phase: 0.0,
            speed: 1.0,
            depth: 0.5,
            feedback: 0.0,
            mix: 1.0,
            base,
            sweep,
            spread,
        }
    }
}

impl Effect for Modulated {
    fn process(&mut self, block: &mut Block) {
        let (dry, wet) = mix_gains(self.mix);
        let step = self.speed / self.rate;
        for frame in frames(block) {
            self.phase = (self.phase + step).fract();
            for (channel, (line, sample)) in self.lines.iter_mut().zip(frame).enumerate() {
                let lfo = (TAU * (self.phase + channel as f32 * self.spread)).sin();
                let delay = (self.base + self.sweep * self.depth * 0.5 * (1.0 + lfo)) * self.rate;
                let input = *sample;
                let delayed = line.read(delay);
                line.push(flush(input + delayed * self.feedback));
                *sample = input * dry + delayed * wet;
            }
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match self.layout.get(index) {
            Some(Sweep::Rate) => self.speed = value.max(0.0),
            Some(Sweep::Depth) => self.depth = value.clamp(0.0, 1.0),
            Some(Sweep::Feedback) => self.feedback = value.clamp(-0.95, 0.95),
            Some(Sweep::Mix) => self.mix = value,
            None => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        let length = ((self.base + self.sweep) * rate) as usize + 16;
        for line in &mut self.lines {
            line.fit(length);
        }
    }

    fn reset(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
    }
}

struct Phaser {
    rate: f32,
    phase: f32,
    speed: f32,
    depth: f32,
    feedback: f32,
    mix: f32,
    stages: [[(f32, f32); 6]; 2],
    last: [f32; 2],
}

impl Effect for Phaser {
    fn process(&mut self, block: &mut Block) {
        let (dry, wet) = mix_gains(self.mix);
        let step = self.speed / self.rate;
        for frame in frames(block) {
            self.phase = (self.phase + step).fract();
            let lfo = 0.5 + 0.5 * (TAU * self.phase).sin();
            let frequency = (300.0 * 8f32.powf(lfo * self.depth)).min(self.rate * 0.45);
            let tangent = (PI * frequency / self.rate).tan();
            let coefficient = (tangent - 1.0) / (tangent + 1.0);
            for (channel, sample) in frame.into_iter().enumerate() {
                let input = *sample;
                let mut signal = input + self.last[channel] * self.feedback;
                for (previous_in, previous_out) in self.stages[channel].iter_mut() {
                    let output = coefficient * signal + *previous_in - coefficient * *previous_out;
                    *previous_in = signal;
                    *previous_out = flush(output);
                    signal = output;
                }
                self.last[channel] = flush(signal);
                *sample = input * dry + signal * wet;
            }
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.speed = value.max(0.0),
            1 => self.depth = value.clamp(0.0, 1.0),
            2 => self.feedback = value.clamp(0.0, 0.95),
            3 => self.mix = value,
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn reset(&mut self) {
        self.stages = [[(0.0, 0.0); 6]; 2];
        self.last = [0.0; 2];
    }
}

struct Tremolo {
    rate: f32,
    phase: f32,
    speed: f32,
    depth: f32,
}

impl Effect for Tremolo {
    fn process(&mut self, block: &mut Block) {
        let step = self.speed / self.rate;
        for [left, right] in frames(block) {
            self.phase = (self.phase + step).fract();
            let gain = 1.0 - self.depth * (0.5 - 0.5 * (TAU * self.phase).cos());
            *left *= gain;
            *right *= gain;
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.speed = value.max(0.0),
            1 => self.depth = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }
}

struct Distortion {
    rate: f32,
    drive: f32,
    tone: f32,
    mix: f32,
    filter: Biquad,
    designed: bool,
}

impl Effect for Distortion {
    fn process(&mut self, block: &mut Block) {
        if !self.designed {
            self.filter.design(Shape::LowPass, self.rate, self.tone, 0.707, 0.0);
            self.designed = true;
        }
        let (dry, wet) = mix_gains(self.mix);
        let gain = 1.0 + 30.0 * self.drive;
        let normal = 1.0 / gain.tanh();
        for (channel, samples) in block.iter_mut().enumerate() {
            for sample in samples.iter_mut() {
                let shaped = self.filter.run(channel, (*sample * gain).tanh() * normal);
                *sample = *sample * dry + shaped * wet;
            }
        }
        self.filter.settle();
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.drive = value.clamp(0.0, 1.0),
            1 => {
                self.tone = value;
                self.designed = false;
            }
            2 => self.mix = value,
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        self.designed = false;
    }

    fn reset(&mut self) {
        self.filter.clear();
    }
}

struct BitCrusher {
    bits: f32,
    downsample: u32,
    mix: f32,
    counter: u32,
    held: [f32; 2],
}

impl Effect for BitCrusher {
    fn process(&mut self, block: &mut Block) {
        let (dry, wet) = mix_gains(self.mix);
        let levels = 2f32.powf(self.bits - 1.0);
        for frame in frames(block) {
            let hold = self.counter == 0;
            self.counter = (self.counter + 1) % self.downsample.max(1);
            for (held, sample) in self.held.iter_mut().zip(frame) {
                if hold {
                    *held = (*sample * levels).round() / levels;
                }
                *sample = *sample * dry + *held * wet;
            }
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.bits = value.clamp(1.0, 24.0),
            1 => {
                self.downsample = value.round().clamp(1.0, 64.0) as u32;
                self.counter = 0;
            }
            2 => self.mix = value,
            _ => {}
        }
    }

    fn rate(&mut self, _rate: f32) {}
}

struct Compressor {
    rate: f32,
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    makeup: f32,
    envelope: f32,
}

impl Effect for Compressor {
    fn process(&mut self, block: &mut Block) {
        let attack = coefficient(self.attack, self.rate);
        let release = coefficient(self.release, self.rate);
        let slope = 1.0 - 1.0 / self.ratio.max(1.0);
        for [left, right] in frames(block) {
            let peak = left.abs().max(right.abs()).max(1.0e-6);
            let over = 20.0 * peak.log10() - self.threshold;
            let target = if over > 0.0 { over * slope } else { 0.0 };
            let speed = if target > self.envelope { attack } else { release };
            self.envelope += (target - self.envelope) * speed;
            let gain = decibels(self.makeup - self.envelope);
            *left *= gain;
            *right *= gain;
        }
        self.envelope = flush(self.envelope);
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.threshold = value,
            1 => self.ratio = value.max(1.0),
            2 => self.attack = value.max(0.0),
            3 => self.release = value.max(0.0),
            4 => self.makeup = value,
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }
}

struct Limiter {
    rate: f32,
    threshold: f32,
    release: f32,
    gain: f32,
}

impl Effect for Limiter {
    fn process(&mut self, block: &mut Block) {
        let ceiling = decibels(self.threshold);
        let release = coefficient(self.release, self.rate);
        for [left, right] in frames(block) {
            let peak = left.abs().max(right.abs());
            let target = if peak > ceiling { ceiling / peak } else { 1.0 };
            if target < self.gain {
                self.gain = target;
            } else {
                self.gain += (target - self.gain) * release;
            }
            *left *= self.gain;
            *right *= self.gain;
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.threshold = value.min(0.0),
            1 => self.release = value.max(0.0),
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn reset(&mut self) {
        self.gain = 1.0;
    }
}

struct NoiseGate {
    rate: f32,
    threshold: f32,
    attack: f32,
    release: f32,
    hold: f32,
    envelope: f32,
    held: f32,
    gain: f32,
}

impl Effect for NoiseGate {
    fn process(&mut self, block: &mut Block) {
        let open_level = decibels(self.threshold);
        let attack = coefficient(self.attack, self.rate);
        let release = coefficient(self.release, self.rate);
        let follow = coefficient(0.01, self.rate);
        for [left, right] in frames(block) {
            let level = left.abs().max(right.abs());
            self.envelope = if level > self.envelope {
                level
            } else {
                self.envelope + (level - self.envelope) * follow
            };
            if self.envelope >= open_level {
                self.held = self.hold * self.rate;
            } else if self.held > 0.0 {
                self.held -= 1.0;
            }
            let target = if self.envelope >= open_level || self.held > 0.0 { 1.0 } else { 0.0 };
            let speed = if target > self.gain { attack } else { release };
            self.gain += (target - self.gain) * speed;
            *left *= self.gain;
            *right *= self.gain;
        }
        self.envelope = flush(self.envelope);
        self.gain = flush(self.gain);
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.threshold = value,
            1 => self.attack = value.max(0.0),
            2 => self.release = value.max(0.0),
            3 => self.hold = value.max(0.0),
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
        self.held = 0.0;
        self.gain = 0.0;
    }
}

const GRAIN: f32 = 0.05;

struct PitchShift {
    rate: f32,
    lines: [DelayLine; 2],
    phase: f32,
    pitch: f32,
}

impl PitchShift {
    fn new(rate: f32) -> Self {
        let length = (GRAIN * rate) as usize + 16;
        Self {
            rate,
            lines: [DelayLine::new(length), DelayLine::new(length)],
            phase: 0.0,
            pitch: 1.0,
        }
    }
}

impl Effect for PitchShift {
    fn process(&mut self, block: &mut Block) {
        let window = GRAIN * self.rate;
        let step = (1.0 - self.pitch) / window;
        for frame in frames(block) {
            self.phase = (self.phase + step).rem_euclid(1.0);
            let other = (self.phase + 0.5).fract();
            let first = 1.0 - (2.0 * self.phase - 1.0).abs();
            let second = 1.0 - (2.0 * other - 1.0).abs();
            for (line, sample) in self.lines.iter_mut().zip(frame) {
                line.push(*sample);
                *sample = line.read(self.phase * window) * first + line.read(other * window) * second;
            }
        }
    }

    fn param(&mut self, _index: usize, value: f32) {
        self.pitch = value.clamp(0.25, 4.0);
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        let length = (GRAIN * rate) as usize + 16;
        for line in &mut self.lines {
            line.fit(length);
        }
    }

    fn reset(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
    }
}

struct RingModulator {
    rate: f32,
    phase: f32,
    frequency: f32,
    mix: f32,
}

impl Effect for RingModulator {
    fn process(&mut self, block: &mut Block) {
        let (dry, wet) = mix_gains(self.mix);
        let step = self.frequency / self.rate;
        for frame in frames(block) {
            self.phase = (self.phase + step).fract();
            let carrier = (TAU * self.phase).sin();
            for sample in frame {
                *sample = *sample * dry + *sample * carrier * wet;
            }
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.frequency = value.max(0.0),
            1 => self.mix = value,
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }
}

struct StereoWidth {
    rate: f32,
    width: Ramp,
}

impl Effect for StereoWidth {
    fn process(&mut self, block: &mut Block) {
        for [left, right] in frames(block) {
            let width = self.width.advance();
            let mid = (*left + *right) * 0.5;
            let side = (*left - *right) * 0.5 * width;
            *left = mid + side;
            *right = mid - side;
        }
    }

    fn param(&mut self, _index: usize, value: f32) {
        self.width.go(value.max(0.0), SMOOTHING * self.rate);
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn settle(&mut self) {
        self.width.jump(self.width.target());
    }
}

pub struct MeterShared {
    peak: AtomicU32,
    loudness: AtomicU32,
}

impl MeterShared {
    pub fn new() -> Arc<MeterShared> {
        Arc::new(MeterShared {
            peak: AtomicU32::new(0),
            loudness: AtomicU32::new(0),
        })
    }

    pub fn peak(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Acquire))
    }

    pub fn loudness(&self) -> f32 {
        f32::from_bits(self.loudness.load(Ordering::Acquire))
    }
}

struct Meter {
    rate: f32,
    shared: Arc<MeterShared>,
    peak: f32,
    energy: f32,
}

impl Effect for Meter {
    fn process(&mut self, block: &mut Block) {
        let mut peak = 0.0f32;
        let mut energy = 0.0f32;
        for sample in block.iter().flatten() {
            peak = peak.max(sample.abs());
            energy += sample * sample;
        }
        let seconds = BLOCK as f32 / self.rate;
        let fall = 10f32.powf(-seconds);
        let smoothing = (-seconds / 0.3).exp();
        self.peak = peak.max(self.peak * fall);
        self.energy = flush(self.energy * smoothing + energy / (BLOCK * 2) as f32 * (1.0 - smoothing));
        self.shared.peak.store(self.peak.to_bits(), Ordering::Release);
        self.shared.loudness.store(self.energy.sqrt().to_bits(), Ordering::Release);
    }

    fn param(&mut self, _index: usize, _value: f32) {}

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn reset(&mut self) {
        self.peak = 0.0;
        self.energy = 0.0;
    }
}

const PASS: &[Field] = &[Field::Frequency, Field::Q];
const PEAK: &[Field] = &[Field::Frequency, Field::Q, Field::Gain];
const SHELF: &[Field] = &[Field::Frequency, Field::Gain];
const CHORUS: &[Sweep] = &[Sweep::Rate, Sweep::Depth, Sweep::Mix];
const FLANGER: &[Sweep] = &[Sweep::Rate, Sweep::Depth, Sweep::Feedback, Sweep::Mix];
const VIBRATO: &[Sweep] = &[Sweep::Rate, Sweep::Depth];

fn effect(class: &str, rate: f32, meter: Option<Arc<MeterShared>>) -> Option<Box<dyn Effect>> {
    Some(match class {
        "Gain" => Box::new(Gain {
            rate,
            volume: Ramp::new(1.0),
        }),
        "Pan" => Box::new(Pan {
            rate,
            pan: Ramp::new(0.0),
        }),
        "LowPass" => Box::new(Filter::new(Shape::LowPass, PASS, rate)),
        "HighPass" => Box::new(Filter::new(Shape::HighPass, PASS, rate)),
        "BandPass" => Box::new(Filter::new(Shape::BandPass, PASS, rate)),
        "Notch" => Box::new(Filter::new(Shape::Notch, PASS, rate)),
        "Peak" => Box::new(Filter::new(Shape::Peak, PEAK, rate)),
        "LowShelf" => Box::new(Filter::new(Shape::LowShelf, SHELF, rate)),
        "HighShelf" => Box::new(Filter::new(Shape::HighShelf, SHELF, rate)),
        "Equalizer" => Box::new(Equalizer::new(rate)),
        "Echo" => Box::new(Echo::new(rate)),
        "Reverb" => Box::new(Reverb::new(rate)),
        "Chorus" => Box::new(Modulated::new(rate, CHORUS, 0.012, 0.008, 0.25)),
        "Flanger" => Box::new(Modulated::new(rate, FLANGER, 0.001, 0.004, 0.25)),
        "Vibrato" => Box::new(Modulated::new(rate, VIBRATO, 0.005, 0.004, 0.0)),
        "Phaser" => Box::new(Phaser {
            rate,
            phase: 0.0,
            speed: 0.5,
            depth: 0.7,
            feedback: 0.5,
            mix: 0.5,
            stages: [[(0.0, 0.0); 6]; 2],
            last: [0.0; 2],
        }),
        "Tremolo" => Box::new(Tremolo {
            rate,
            phase: 0.0,
            speed: 5.0,
            depth: 0.5,
        }),
        "Distortion" => Box::new(Distortion {
            rate,
            drive: 0.5,
            tone: 8000.0,
            mix: 1.0,
            filter: Biquad::default(),
            designed: false,
        }),
        "BitCrusher" => Box::new(BitCrusher {
            bits: 8.0,
            downsample: 1,
            mix: 1.0,
            counter: 0,
            held: [0.0; 2],
        }),
        "Compressor" => Box::new(Compressor {
            rate,
            threshold: -20.0,
            ratio: 4.0,
            attack: 0.01,
            release: 0.1,
            makeup: 0.0,
            envelope: 0.0,
        }),
        "Limiter" => Box::new(Limiter {
            rate,
            threshold: -1.0,
            release: 0.05,
            gain: 1.0,
        }),
        "NoiseGate" => Box::new(NoiseGate {
            rate,
            threshold: -50.0,
            attack: 0.005,
            release: 0.1,
            hold: 0.05,
            envelope: 0.0,
            held: 0.0,
            gain: 0.0,
        }),
        "PitchShift" => Box::new(PitchShift::new(rate)),
        "RingModulator" => Box::new(RingModulator {
            rate,
            phase: 0.0,
            frequency: 440.0,
            mix: 1.0,
        }),
        "StereoWidth" => Box::new(StereoWidth {
            rate,
            width: Ramp::new(1.0),
        }),
        "Meter" => Box::new(Meter {
            rate,
            shared: meter.unwrap_or_else(MeterShared::new),
            peak: 0.0,
            energy: 0.0,
        }),
        _ => return None,
    })
}

pub fn modifier(class: &str, rate: f32, meter: Option<Arc<MeterShared>>) -> Option<Box<dyn Processor>> {
    let effect = effect(class, rate, meter)?;
    Some(Box::new(Modifier {
        effect,
        rate,
        enabled: true,
        wet: Ramp::new(1.0),
        started: false,
    }))
}
