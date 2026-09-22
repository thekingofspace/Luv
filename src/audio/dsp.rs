use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
}

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: [f32; 2],
    s2: [f32; 2],
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            s1: [0.0; 2],
            s2: [0.0; 2],
        }
    }
}

impl Biquad {
    pub fn design(&mut self, shape: Shape, rate: f32, frequency: f32, q: f32, gain: f32) {
        let rate = f64::from(rate.max(1.0));
        let frequency = f64::from(frequency).clamp(1.0, rate * 0.49);
        let omega = 2.0 * PI * frequency / rate;
        let (sin, cos) = omega.sin_cos();
        let q = f64::from(q).max(0.01);
        let alpha = sin / (2.0 * q);
        let amplitude = 10f64.powf(f64::from(gain) / 40.0);
        let root = amplitude.sqrt();
        let shelf = sin / 2.0 * 2f64.sqrt();
        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::LowPass => {
                let b = (1.0 - cos) / 2.0;
                (b, 1.0 - cos, b, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
            }
            Shape::HighPass => {
                let b = (1.0 + cos) / 2.0;
                (b, -(1.0 + cos), b, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
            }
            Shape::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            Shape::Notch => (1.0, -2.0 * cos, 1.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            Shape::Peak => (
                1.0 + alpha * amplitude,
                -2.0 * cos,
                1.0 - alpha * amplitude,
                1.0 + alpha / amplitude,
                -2.0 * cos,
                1.0 - alpha / amplitude,
            ),
            Shape::LowShelf => (
                amplitude * ((amplitude + 1.0) - (amplitude - 1.0) * cos + 2.0 * root * shelf),
                2.0 * amplitude * ((amplitude - 1.0) - (amplitude + 1.0) * cos),
                amplitude * ((amplitude + 1.0) - (amplitude - 1.0) * cos - 2.0 * root * shelf),
                (amplitude + 1.0) + (amplitude - 1.0) * cos + 2.0 * root * shelf,
                -2.0 * ((amplitude - 1.0) + (amplitude + 1.0) * cos),
                (amplitude + 1.0) + (amplitude - 1.0) * cos - 2.0 * root * shelf,
            ),
            Shape::HighShelf => (
                amplitude * ((amplitude + 1.0) + (amplitude - 1.0) * cos + 2.0 * root * shelf),
                -2.0 * amplitude * ((amplitude - 1.0) + (amplitude + 1.0) * cos),
                amplitude * ((amplitude + 1.0) + (amplitude - 1.0) * cos - 2.0 * root * shelf),
                (amplitude + 1.0) - (amplitude - 1.0) * cos + 2.0 * root * shelf,
                2.0 * ((amplitude - 1.0) - (amplitude + 1.0) * cos),
                (amplitude + 1.0) - (amplitude - 1.0) * cos - 2.0 * root * shelf,
            ),
        };
        self.b0 = (b0 / a0) as f32;
        self.b1 = (b1 / a0) as f32;
        self.b2 = (b2 / a0) as f32;
        self.a1 = (a1 / a0) as f32;
        self.a2 = (a2 / a0) as f32;
    }

    #[inline]
    pub fn run(&mut self, channel: usize, input: f32) -> f32 {
        let output = self.b0 * input + self.s1[channel];
        self.s1[channel] = self.b1 * input - self.a1 * output + self.s2[channel];
        self.s2[channel] = self.b2 * input - self.a2 * output;
        output
    }

    pub fn settle(&mut self) {
        for state in self.s1.iter_mut().chain(self.s2.iter_mut()) {
            *state = flush(*state);
        }
    }

    pub fn clear(&mut self) {
        self.s1 = [0.0; 2];
        self.s2 = [0.0; 2];
    }
}

#[derive(Clone, Debug)]
pub struct DelayLine {
    buffer: Vec<f32>,
    write: usize,
}

impl DelayLine {
    pub fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(4)],
            write: 0,
        }
    }

    pub fn fit(&mut self, length: usize) {
        if self.buffer.len() != length.max(4) {
            *self = Self::new(length);
        }
    }

    pub fn capacity(&self) -> f32 {
        (self.buffer.len() - 2) as f32
    }

    #[inline]
    pub fn push(&mut self, sample: f32) {
        self.buffer[self.write] = sample;
        self.write += 1;
        if self.write == self.buffer.len() {
            self.write = 0;
        }
    }

    #[inline]
    pub fn read(&self, delay: f32) -> f32 {
        let length = self.buffer.len();
        let delay = delay.clamp(0.0, self.capacity());
        let mut position = self.write as f32 - 1.0 - delay;
        if position < 0.0 {
            position += length as f32;
        }
        let index = position as usize % length;
        let fraction = position - position.floor();
        let first = self.buffer[index];
        let second = self.buffer[(index + 1) % length];
        first + (second - first) * fraction
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
    }
}

#[inline]
pub fn flush(value: f32) -> f32 {
    if value.abs() < 1.0e-18 { 0.0 } else { value }
}

pub fn mix_gains(mix: f32) -> (f32, f32) {
    let mix = mix.clamp(0.0, 1.0);
    ((2.0 * (1.0 - mix)).min(1.0), (2.0 * mix).min(1.0))
}

pub fn decibels(gain: f32) -> f32 {
    10f32.powf(gain / 20.0)
}

pub fn coefficient(seconds: f32, rate: f32) -> f32 {
    if seconds <= 0.0 {
        1.0
    } else {
        1.0 - (-1.0 / (seconds * rate)).exp()
    }
}

pub fn one_pole(frequency: f32, rate: f32) -> f32 {
    if frequency >= rate * 0.45 {
        1.0
    } else {
        1.0 - (-2.0 * std::f32::consts::PI * frequency.max(1.0) / rate).exp()
    }
}
