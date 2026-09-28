use std::f32::consts::TAU;
use std::sync::Arc;

use super::super::dsp::{Biquad, DelayLine, Shape, coefficient, decibels, flush, mix_gains};
use super::super::{BLOCK, Block, Ramp, SMOOTHING, frames};
use super::{Effect, ProbeShared, pan_frame};

const LOW_BAND: f32 = 40.0;
const HIGH_BAND: f32 = 16_000.0;
const SILENT_DB: f32 = -70.0;
const EXCITE_DRIVE: f32 = 6.0;
const MAX_HAAS: f32 = 0.05;

pub(super) struct DcBlock {
    rate: f32,
    frequency: f32,
    pole: f32,
    input: [f32; 2],
    output: [f32; 2],
}

impl DcBlock {
    pub(super) fn new(rate: f32) -> Self {
        let mut block = Self {
            rate,
            frequency: 20.0,
            pole: 0.0,
            input: [0.0; 2],
            output: [0.0; 2],
        };
        block.design();
        block
    }

    fn design(&mut self) {
        self.pole = (-TAU * self.frequency.max(1.0) / self.rate.max(1.0)).exp();
    }
}

impl Effect for DcBlock {
    fn process(&mut self, block: &mut Block) {
        for (channel, samples) in block.iter_mut().enumerate() {
            for sample in samples.iter_mut() {
                let output = *sample - self.input[channel] + self.pole * self.output[channel];
                self.input[channel] = *sample;
                self.output[channel] = flush(output);
                *sample = output;
            }
        }
    }

    fn param(&mut self, _index: usize, value: f32) {
        self.frequency = value.max(1.0);
        self.design();
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        self.design();
    }

    fn reset(&mut self) {
        self.input = [0.0; 2];
        self.output = [0.0; 2];
    }
}

pub(super) struct SoftClip {
    ceiling: f32,
    knee: f32,
}

impl SoftClip {
    pub(super) fn new() -> Self {
        Self {
            ceiling: decibels(-1.0),
            knee: 0.5,
        }
    }
}

fn soft_clip(sample: f32, ceiling: f32, start: f32) -> f32 {
    if !sample.is_finite() {
        return 0.0;
    }
    let size = sample.abs();
    if size <= start {
        return sample;
    }
    let room = ceiling - start;
    if room <= 0.0 {
        return ceiling.copysign(sample);
    }
    (start + room * ((size - start) / room).tanh()).copysign(sample)
}

impl Effect for SoftClip {
    fn process(&mut self, block: &mut Block) {
        let start = self.ceiling * (1.0 - self.knee);
        for sample in block.iter_mut().flatten() {
            *sample = soft_clip(*sample, self.ceiling, start);
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.ceiling = decibels(value.min(0.0)),
            1 => self.knee = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn rate(&mut self, _rate: f32) {}
}

pub(super) struct AutoGain {
    rate: f32,
    target: f32,
    speed: f32,
    most: f32,
    level: f32,
    gain: f32,
    probe: Arc<ProbeShared>,
}

impl AutoGain {
    pub(super) fn new(rate: f32, probe: Arc<ProbeShared>) -> Self {
        Self {
            rate,
            target: -18.0,
            speed: 1.0,
            most: 18.0,
            level: 0.0,
            gain: 0.0,
            probe,
        }
    }
}

impl Effect for AutoGain {
    fn process(&mut self, block: &mut Block) {
        let follow = coefficient(self.speed, self.rate);
        for [left, right] in frames(block) {
            let energy = (*left * *left + *right * *right) * 0.5;
            self.level += (energy - self.level) * follow;
        }
        self.level = flush(self.level);
        let loudness = 10.0 * self.level.max(1.0e-12).log10();
        let wanted = if loudness > SILENT_DB {
            (self.target - loudness).clamp(-self.most, self.most)
        } else {
            self.gain
        };
        let step = 1.0 - (-(BLOCK as f32) / (self.speed.max(0.01) * self.rate)).exp();
        let next = self.gain + (wanted - self.gain) * step;
        let from = decibels(self.gain);
        let to = decibels(next);
        let slope = (to - from) / BLOCK as f32;
        for (index, [left, right]) in frames(block).enumerate() {
            let gain = from + slope * index as f32;
            *left *= gain;
            *right *= gain;
        }
        self.gain = next;
        self.probe.set(0, self.gain);
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.target = value.min(0.0),
            1 => self.speed = value.max(0.01),
            2 => self.most = value.max(0.0),
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn reset(&mut self) {
        self.level = 0.0;
        self.gain = 0.0;
        self.probe.set(0, 0.0);
    }

    fn tail(&self) -> f32 {
        0.5
    }
}

pub(super) struct Expander {
    rate: f32,
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    envelope: f32,
}

impl Expander {
    pub(super) fn new(rate: f32) -> Self {
        Self {
            rate,
            threshold: -40.0,
            ratio: 2.0,
            attack: 0.005,
            release: 0.1,
            envelope: 0.0,
        }
    }
}

impl Effect for Expander {
    fn process(&mut self, block: &mut Block) {
        let attack = coefficient(self.attack, self.rate);
        let release = coefficient(self.release, self.rate);
        let slope = self.ratio - 1.0;
        for [left, right] in frames(block) {
            let peak = left.abs().max(right.abs()).max(1.0e-6);
            let under = self.threshold - 20.0 * peak.log10();
            let target = if under > 0.0 { (under * slope).min(96.0) } else { 0.0 };
            let speed = if target < self.envelope { attack } else { release };
            self.envelope += (target - self.envelope) * speed;
            let gain = decibels(-self.envelope);
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
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn tail(&self) -> f32 {
        self.release + 0.25
    }
}

pub(super) struct Exciter {
    rate: f32,
    frequency: f32,
    amount: f32,
    split: Biquad,
    clean: Biquad,
    designed: bool,
}

impl Exciter {
    pub(super) fn new(rate: f32) -> Self {
        Self {
            rate,
            frequency: 3000.0,
            amount: 0.3,
            split: Biquad::default(),
            clean: Biquad::default(),
            designed: false,
        }
    }
}

impl Effect for Exciter {
    fn process(&mut self, block: &mut Block) {
        if !self.designed {
            self.split.design(Shape::HighPass, self.rate, self.frequency, 0.707, 0.0);
            self.clean.design(Shape::HighPass, self.rate, self.frequency, 0.707, 0.0);
            self.designed = true;
        }
        for (channel, samples) in block.iter_mut().enumerate() {
            for sample in samples.iter_mut() {
                let high = self.split.run(channel, *sample);
                let driven = (high * EXCITE_DRIVE).tanh() / EXCITE_DRIVE;
                let extra = self.clean.run(channel, driven);
                *sample += extra * self.amount;
            }
        }
        self.split.settle();
        self.clean.settle();
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => {
                self.frequency = value;
                self.designed = false;
            }
            1 => self.amount = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        self.designed = false;
    }

    fn reset(&mut self) {
        self.split.clear();
        self.clean.clear();
    }
}

pub(super) struct AutoWah {
    rate: f32,
    sensitivity: f32,
    base: f32,
    range: f32,
    q: f32,
    mix: f32,
    envelope: f32,
    filter: Biquad,
}

impl AutoWah {
    pub(super) fn new(rate: f32) -> Self {
        Self {
            rate,
            sensitivity: 0.5,
            base: 250.0,
            range: 3.0,
            q: 4.0,
            mix: 1.0,
            envelope: 0.0,
            filter: Biquad::default(),
        }
    }
}

impl Effect for AutoWah {
    fn process(&mut self, block: &mut Block) {
        let attack = coefficient(0.01, self.rate);
        let release = coefficient(0.15, self.rate);
        for [left, right] in frames(block) {
            let level = left.abs().max(right.abs());
            let speed = if level > self.envelope { attack } else { release };
            self.envelope += (level - self.envelope) * speed;
        }
        self.envelope = flush(self.envelope);
        let opened = (self.envelope * self.sensitivity * 8.0).min(1.0);
        let cutoff = (self.base * 2f32.powf(self.range * opened)).min(self.rate * 0.45);
        self.filter.design(Shape::LowPass, self.rate, cutoff, self.q, 0.0);
        let (dry, wet) = mix_gains(self.mix);
        for (channel, samples) in block.iter_mut().enumerate() {
            for sample in samples.iter_mut() {
                let filtered = self.filter.run(channel, *sample);
                *sample = *sample * dry + filtered * wet;
            }
        }
        self.filter.settle();
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.sensitivity = value.clamp(0.0, 1.0),
            1 => self.base = value.max(10.0),
            2 => self.range = value.clamp(0.0, 8.0),
            3 => self.q = value.max(0.1),
            4 => self.mix = value,
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
        self.filter.clear();
    }
}

pub(super) struct Haas {
    rate: f32,
    lines: [DelayLine; 2],
    delay: Ramp,
}

impl Haas {
    pub(super) fn new(rate: f32) -> Self {
        let length = (MAX_HAAS * rate) as usize + 8;
        Self {
            rate,
            lines: [DelayLine::new(length), DelayLine::new(length)],
            delay: Ramp::new(0.012),
        }
    }
}

impl Effect for Haas {
    fn process(&mut self, block: &mut Block) {
        for [left, right] in frames(block) {
            let delay = self.delay.advance();
            let frames = delay.abs() * self.rate;
            self.lines[0].push(*left);
            self.lines[1].push(*right);
            if delay < 0.0 {
                *left = self.lines[0].read(frames);
            } else {
                *right = self.lines[1].read(frames);
            }
        }
    }

    fn param(&mut self, _index: usize, value: f32) {
        self.delay.go(value.clamp(-MAX_HAAS, MAX_HAAS), SMOOTHING * self.rate);
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        let length = (MAX_HAAS * rate) as usize + 8;
        for line in &mut self.lines {
            line.fit(length);
        }
    }

    fn settle(&mut self) {
        self.delay.jump(self.delay.target());
    }

    fn reset(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
    }

    fn tail(&self) -> f32 {
        if self.delay.settled() { MAX_HAAS * 2.0 } else { f32::INFINITY }
    }
}

pub(super) struct AutoPan {
    rate: f32,
    phase: f32,
    speed: f32,
    depth: f32,
}

impl AutoPan {
    pub(super) fn new(rate: f32) -> Self {
        Self {
            rate,
            phase: 0.0,
            speed: 0.5,
            depth: 1.0,
        }
    }
}

impl Effect for AutoPan {
    fn process(&mut self, block: &mut Block) {
        let step = self.speed / self.rate;
        for [left, right] in frames(block) {
            self.phase = (self.phase + step).fract();
            let pan = self.depth * (TAU * self.phase).sin();
            (*left, *right) = pan_frame(pan, *left, *right);
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

pub(super) struct Transient {
    rate: f32,
    attack: f32,
    sustain: f32,
    quick: f32,
    slow: f32,
    held: f32,
    fading: f32,
}

impl Transient {
    pub(super) fn new(rate: f32) -> Self {
        Self {
            rate,
            attack: 0.0,
            sustain: 0.0,
            quick: 0.0,
            slow: 0.0,
            held: 0.0,
            fading: 0.0,
        }
    }
}

fn follow(envelope: f32, level: f32, rise: f32, fall: f32) -> f32 {
    let speed = if level > envelope { rise } else { fall };
    envelope + (level - envelope) * speed
}

impl Effect for Transient {
    fn process(&mut self, block: &mut Block) {
        if self.attack == 0.0 && self.sustain == 0.0 {
            return;
        }
        let rise_quick = coefficient(0.001, self.rate);
        let rise_slow = coefficient(0.03, self.rate);
        let fall_quick = coefficient(0.03, self.rate);
        let fall_slow = coefficient(0.3, self.rate);
        for [left, right] in frames(block) {
            let level = left.abs().max(right.abs());
            self.quick = follow(self.quick, level, rise_quick, fall_slow);
            self.slow = follow(self.slow, level, rise_slow, fall_slow);
            self.held = follow(self.held, level, rise_quick, fall_slow);
            self.fading = follow(self.fading, level, rise_quick, fall_quick);
            let punch = ((self.quick + 1.0e-6) / (self.slow + 1.0e-6)).ln();
            let body = ((self.held + 1.0e-6) / (self.fading + 1.0e-6)).ln();
            let gain = (punch * self.attack * 2.0 + body * self.sustain * 2.0).clamp(-2.77, 2.77).exp();
            *left *= gain;
            *right *= gain;
        }
        self.quick = flush(self.quick);
        self.slow = flush(self.slow);
        self.held = flush(self.held);
        self.fading = flush(self.fading);
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.attack = value.clamp(-1.0, 1.0),
            1 => self.sustain = value.clamp(-1.0, 1.0),
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
    }

    fn reset(&mut self) {
        self.quick = 0.0;
        self.slow = 0.0;
        self.held = 0.0;
        self.fading = 0.0;
    }

    fn tail(&self) -> f32 {
        1.0
    }
}

pub const MAX_BANDS: usize = 32;

pub(super) struct Spectrum {
    rate: f32,
    count: usize,
    smoothing: f32,
    filters: Vec<Biquad>,
    levels: [f32; MAX_BANDS],
    designed: bool,
    probe: Arc<ProbeShared>,
}

impl Spectrum {
    pub(super) fn new(rate: f32, probe: Arc<ProbeShared>) -> Self {
        Self {
            rate,
            count: 8,
            smoothing: 0.6,
            filters: vec![Biquad::default(); MAX_BANDS],
            levels: [0.0; MAX_BANDS],
            designed: false,
            probe,
        }
    }

    fn design(&mut self) {
        let count = self.count.max(1);
        let top = HIGH_BAND.min(self.rate * 0.45);
        let ratio = if count > 1 { (top / LOW_BAND).powf(1.0 / (count - 1) as f32) } else { 2.0 };
        let q = (ratio.sqrt() / (ratio - 1.0)).clamp(0.5, 30.0);
        for (index, filter) in self.filters.iter_mut().take(count).enumerate() {
            let center = if count > 1 { LOW_BAND * ratio.powi(index as i32) } else { 1000.0 };
            filter.design(Shape::BandPass, self.rate, center, q, 0.0);
            filter.clear();
        }
        self.levels = [0.0; MAX_BANDS];
        self.probe.set_count(count);
        self.designed = true;
    }
}

impl Effect for Spectrum {
    fn process(&mut self, block: &mut Block) {
        if !self.designed {
            self.design();
        }
        let keep = self.smoothing.clamp(0.0, 0.99);
        for index in 0..self.count {
            let filter = &mut self.filters[index];
            let mut energy = 0.0f32;
            for (left, right) in block[0].iter().zip(block[1].iter()) {
                let sample = (left + right) * 0.5;
                let band = filter.run(0, sample);
                energy += band * band;
            }
            filter.settle();
            let level = (energy / BLOCK as f32).sqrt() * 2.0;
            self.levels[index] = flush(self.levels[index] * keep + level * (1.0 - keep));
            self.probe.set(index, self.levels[index]);
        }
    }

    fn param(&mut self, index: usize, value: f32) {
        match index {
            0 => {
                let count = (value.round() as usize).clamp(1, MAX_BANDS);
                if count != self.count {
                    self.count = count;
                    self.designed = false;
                }
            }
            1 => self.smoothing = value.clamp(0.0, 0.99),
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        self.designed = false;
    }

    fn reset(&mut self) {
        self.designed = false;
    }

    fn tail(&self) -> f32 {
        3.0
    }
}

pub fn band_center(index: usize, count: usize, rate: f32) -> f32 {
    let count = count.max(1);
    if count == 1 {
        return 1000.0;
    }
    let top = HIGH_BAND.min(rate * 0.45);
    let ratio = (top / LOW_BAND).powf(1.0 / (count - 1) as f32);
    LOW_BAND * ratio.powi(index as i32)
}
