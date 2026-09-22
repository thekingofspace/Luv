use std::f32::consts::{FRAC_PI_4, SQRT_2};

use super::dsp::{DelayLine, flush, one_pole};
use super::{BLOCK, Block, Context, Listener, Processor, Ramp, SMOOTHING};

const FOCUS_FADE: f32 = 0.05;
const BLEND_FADE: f32 = 0.05;
const MAX_ITD: f32 = 0.00066;
const LINE: usize = 512;
const OPEN: f32 = 20_000.0;

type Vector = [f32; 3];

fn subtract(a: Vector, b: Vector) -> Vector {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vector, b: Vector) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vector, b: Vector) -> Vector {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn length(a: Vector) -> f32 {
    dot(a, a).sqrt()
}

fn normalize(a: Vector) -> Option<Vector> {
    let size = length(a);
    (size > 1.0e-6 && size.is_finite()).then(|| [a[0] / size, a[1] / size, a[2] / size])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RollOff {
    Inverse,
    Linear,
    LinearSquare,
    InverseTapered,
}

impl RollOff {
    fn from_index(index: f64) -> RollOff {
        match index as usize {
            0 => RollOff::Inverse,
            2 => RollOff::LinearSquare,
            3 => RollOff::InverseTapered,
            _ => RollOff::Linear,
        }
    }
}

struct Targets {
    gains: [f32; 2],
    delays: [f32; 2],
    filters: [f32; 2],
}

pub struct Speaker {
    rate: f32,
    volume: Ramp,
    focus: Ramp,
    blend: Ramp,
    owned: bool,
    spatial: bool,
    position: Vector,
    direction: Vector,
    min_distance: f32,
    max_distance: f32,
    rolloff: RollOff,
    cone_inner: f32,
    cone_outer: f32,
    cone_volume: f32,
    binaural: bool,
    gains: [f32; 2],
    delays: [f32; 2],
    filters: [f32; 2],
    states: [f32; 2],
    line: DelayLine,
    primed: bool,
    started: bool,
}

impl Speaker {
    pub fn new(rate: f32) -> Speaker {
        Speaker {
            rate,
            volume: Ramp::new(1.0),
            focus: Ramp::new(1.0),
            blend: Ramp::new(0.0),
            owned: true,
            spatial: false,
            position: [0.0; 3],
            direction: [0.0; 3],
            min_distance: 50.0,
            max_distance: 2000.0,
            rolloff: RollOff::Linear,
            cone_inner: 360.0,
            cone_outer: 360.0,
            cone_volume: 0.0,
            binaural: true,
            gains: [1.0; 2],
            delays: [0.0; 2],
            filters: [1.0; 2],
            states: [0.0; 2],
            line: DelayLine::new(LINE),
            primed: false,
            started: false,
        }
    }

    fn attenuation(&self, distance: f32) -> f32 {
        let min = self.min_distance.max(0.0);
        let max = self.max_distance.max(min + 0.001);
        let linear = if distance <= min {
            1.0
        } else if distance >= max {
            0.0
        } else {
            1.0 - (distance - min) / (max - min)
        };
        let reference = min.max(0.001);
        let inverse = if distance <= reference { 1.0 } else { reference / distance.min(max) };
        match self.rolloff {
            RollOff::Linear => linear,
            RollOff::LinearSquare => linear * linear,
            RollOff::Inverse => inverse,
            RollOff::InverseTapered => inverse.min(linear * linear),
        }
    }

    fn cone(&self, toward: Option<Vector>) -> f32 {
        let (Some(toward), Some(facing)) = (toward, normalize(self.direction)) else {
            return 1.0;
        };
        let inner = self.cone_inner.clamp(0.0, 360.0) / 2.0;
        let outer = self.cone_outer.clamp(0.0, 360.0).max(inner * 2.0) / 2.0;
        if inner >= 180.0 {
            return 1.0;
        }
        let angle = dot(facing, [-toward[0], -toward[1], -toward[2]]).clamp(-1.0, 1.0).acos().to_degrees();
        if angle <= inner {
            1.0
        } else if angle >= outer {
            self.cone_volume
        } else {
            1.0 + (self.cone_volume - 1.0) * (angle - inner) / (outer - inner)
        }
    }

    fn targets(&self, listener: &Listener) -> Targets {
        let relative = subtract(self.position, listener.position);
        let distance = length(relative);
        let forward = normalize(listener.forward).unwrap_or([0.0, 0.0, -1.0]);
        let up = normalize(listener.up).unwrap_or([0.0, 1.0, 0.0]);
        let right = normalize(cross(forward, up)).unwrap_or([1.0, 0.0, 0.0]);
        let toward = (distance > 1.0e-4).then(|| {
            [
                relative[0] / distance,
                relative[1] / distance,
                relative[2] / distance,
            ]
        });
        let (pan, front) = toward.map_or((0.0, 0.0), |toward| {
            (dot(toward, right).clamp(-1.0, 1.0), dot(toward, forward).clamp(-1.0, 1.0))
        });
        let attenuation = self.attenuation(distance) * self.cone(toward);
        let spread = if self.binaural { 0.8 } else { 1.0 };
        let angle = (spread * pan + 1.0) * FRAC_PI_4;
        let gains = [
            (SQRT_2 * angle.cos()).min(1.0) * attenuation,
            (SQRT_2 * angle.sin()).min(1.0) * attenuation,
        ];
        if !self.binaural {
            return Targets {
                gains,
                delays: [0.0; 2],
                filters: [1.0; 2],
            };
        }
        let itd = MAX_ITD * pan.abs() * self.rate;
        let rear = if front < 0.0 { OPEN - 13_000.0 * -front } else { OPEN };
        let shadow = OPEN * (1.0 - 0.85 * pan.abs());
        let far = one_pole(shadow.min(rear), self.rate);
        let near = one_pole(rear, self.rate);
        if pan > 0.0 {
            Targets {
                gains,
                delays: [itd, 0.0],
                filters: [far, near],
            }
        } else {
            Targets {
                gains,
                delays: [0.0, itd],
                filters: [near, far],
            }
        }
    }
}

impl Processor for Speaker {
    fn process(&mut self, context: &Context<'_>, input: &Block, output: &mut Block) {
        let focus = if self.owned && !context.focused { 0.0 } else { 1.0 };
        let blend = if self.spatial { 1.0 } else { 0.0 };
        if !self.started {
            self.started = true;
            self.focus.jump(focus);
            self.blend.jump(blend);
        }
        if self.focus.target() != focus {
            self.focus.go(focus, FOCUS_FADE * self.rate);
        }
        if self.blend.target() != blend {
            self.blend.go(blend, BLEND_FADE * self.rate);
        }
        let spatial = self.blend.value() > 0.0 || !self.blend.settled();
        let flat = self.blend.value() < 1.0 || !self.blend.settled();
        let targets = spatial.then(|| self.targets(context.listener));
        match &targets {
            Some(targets) if !self.primed => {
                self.gains = targets.gains;
                self.delays = targets.delays;
                self.filters = targets.filters;
                self.states = [0.0; 2];
                self.line.clear();
                self.primed = true;
            }
            Some(_) => {}
            None => self.primed = false,
        }
        for frame in 0..BLOCK {
            let gain = self.volume.advance() * self.focus.advance();
            let mix = self.blend.advance();
            let left = input[0][frame];
            let right = input[1][frame];
            let mut result = [0.0f32; 2];
            if flat {
                result[0] = left * (1.0 - mix);
                result[1] = right * (1.0 - mix);
            }
            if let Some(targets) = &targets {
                self.line.push((left + right) * 0.5);
                let progress = (frame + 1) as f32 / BLOCK as f32;
                for (ear, value) in result.iter_mut().enumerate() {
                    let delay = self.delays[ear] + (targets.delays[ear] - self.delays[ear]) * progress;
                    let filter = self.filters[ear] + (targets.filters[ear] - self.filters[ear]) * progress;
                    let level = self.gains[ear] + (targets.gains[ear] - self.gains[ear]) * progress;
                    let delayed = self.line.read(delay);
                    self.states[ear] += filter * (delayed - self.states[ear]);
                    *value += self.states[ear] * level * mix;
                }
            }
            output[0][frame] = result[0] * gain;
            output[1][frame] = result[1] * gain;
        }
        if let Some(targets) = targets {
            self.gains = targets.gains;
            self.delays = targets.delays;
            self.filters = targets.filters;
            self.states = [flush(self.states[0]), flush(self.states[1])];
        }
    }

    fn param(&mut self, index: usize, value: f64) {
        let number = value as f32;
        match index {
            0 if self.started => self.volume.go(number, SMOOTHING * self.rate),
            0 => self.volume.jump(number),
            1 => self.owned = value != 0.0,
            2 => self.spatial = value != 0.0,
            3..=5 => self.position[index - 3] = number,
            6..=8 => self.direction[index - 6] = number,
            9 => self.min_distance = number.max(0.0),
            10 => self.max_distance = number.max(0.0),
            11 => self.rolloff = RollOff::from_index(value),
            12 => self.cone_inner = number,
            13 => self.cone_outer = number,
            14 => self.cone_volume = number.clamp(0.0, 1.0),
            15 => self.binaural = value != 0.0,
            _ => {}
        }
    }

    fn rate(&mut self, rate: f32) {
        self.rate = rate;
        self.primed = false;
    }
}
