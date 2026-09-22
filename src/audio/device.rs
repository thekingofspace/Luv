use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, TryLockError};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, SampleFormat, SizedSample, StreamConfig, SupportedStreamConfig};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use super::render::write_frame;
use super::{AudioStatus, Control, Renderer, lock};

const TICK: Duration = Duration::from_millis(10);
const IDLE_POLL: Duration = Duration::from_millis(250);
const RETRY: Duration = Duration::from_secs(3);
const LINGER: Duration = Duration::from_secs(1);
const FOLLOW: Duration = Duration::from_secs(2);
const CAPTURE_SECONDS: usize = 20;

type Errors = (Sender<(Option<String>, ErrorKind)>, Receiver<(Option<String>, ErrorKind)>);

pub(super) fn spawn(renderer: Renderer, status: Arc<AudioStatus>, wake: Receiver<()>, control: Sender<Control>, desktop: bool) {
    let renderer = Arc::new(Mutex::new(renderer));
    let _ = thread::Builder::new()
        .name("luv-audio".to_owned())
        .spawn(move || Host::new(renderer, status, control, desktop).run(wake));
}

fn name_of(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|description| description.name().to_owned())
}

pub(super) fn list() -> Vec<String> {
    let host = cpal::default_host();
    let mut names: Vec<String> = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for name in devices.filter_map(|device| name_of(&device)) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

struct Output {
    _stream: cpal::Stream,
    device: Option<cpal::DeviceId>,
}

struct Secondary {
    _stream: Option<cpal::Stream>,
    consumer: Option<rtrb::Consumer<f32>>,
}

struct Drain {
    consumer: rtrb::Consumer<f32>,
    ratio: f64,
    position: f64,
    previous: (f32, f32),
    next: (f32, f32),
    primed: bool,
    target: usize,
}

impl Drain {
    fn new(consumer: rtrb::Consumer<f32>, input: u32, output: u32) -> Drain {
        Drain {
            consumer,
            ratio: f64::from(input) / f64::from(output.max(1)),
            position: 1.0,
            previous: (0.0, 0.0),
            next: (0.0, 0.0),
            primed: false,
            target: (input as usize * 3 / 100).max(256),
        }
    }

    fn pull(&mut self) -> Option<(f32, f32)> {
        if self.consumer.slots() < 2 {
            return None;
        }
        let left = self.consumer.pop().ok()?;
        let right = self.consumer.pop().ok()?;
        Some((left, right))
    }

    fn fill(&mut self, out: &mut [f32], channels: usize) {
        let available = self.consumer.slots() / 2;
        if !self.primed {
            if available < self.target {
                out.fill(0.0);
                return;
            }
            self.primed = true;
        }
        let error = (available as f64 - self.target as f64) / self.target as f64;
        let step = self.ratio * (1.0 + (error * 0.002).clamp(-0.005, 0.005));
        for frame in out.chunks_mut(channels) {
            while self.position >= 1.0 {
                self.position -= 1.0;
                self.previous = self.next;
                match self.pull() {
                    Some(sample) => self.next = sample,
                    None => {
                        self.primed = false;
                        self.next = (0.0, 0.0);
                    }
                }
            }
            if !self.primed {
                write_frame(frame, 0.0, 0.0);
                continue;
            }
            let fraction = self.position as f32;
            let left = self.previous.0 + (self.next.0 - self.previous.0) * fraction;
            let right = self.previous.1 + (self.next.1 - self.previous.1) * fraction;
            write_frame(frame, left, right);
            self.position += step;
        }
    }
}

#[derive(Default)]
struct Clock {
    started: Option<Instant>,
    rendered: u64,
    scratch: Vec<f32>,
}

impl Clock {
    fn tick(&mut self, renderer: &Mutex<Renderer>) -> Option<(u64, &[f32])> {
        let mut renderer = lock(renderer);
        let rate = u64::from(renderer.rate().max(1));
        let now = Instant::now();
        let started = *self.started.get_or_insert(now);
        let due = (now.duration_since(started).as_secs_f64() * rate as f64) as u64;
        let mut frames = due.saturating_sub(self.rendered);
        if frames > rate / 4 {
            self.rendered = due - rate / 4;
            frames = rate / 4;
        }
        if frames == 0 {
            return None;
        }
        self.scratch.resize(frames as usize * 2, 0.0);
        renderer.render(&mut self.scratch, 2);
        self.rendered += frames;
        Some((rate, &self.scratch))
    }

    fn stop(&mut self) {
        self.started = None;
        self.rendered = 0;
    }
}

struct Host {
    renderer: Arc<Mutex<Renderer>>,
    status: Arc<AudioStatus>,
    control: Sender<Control>,
    desktop: bool,
    host: Option<cpal::Host>,
    output: Option<Output>,
    primary: Option<String>,
    secondaries: HashMap<String, Secondary>,
    secondary_rate: u32,
    failed: Option<Instant>,
    searched: Option<Instant>,
    idle: Option<Instant>,
    checked: Option<Instant>,
    errors: Errors,
    clock: Clock,
}

impl Host {
    fn new(renderer: Arc<Mutex<Renderer>>, status: Arc<AudioStatus>, control: Sender<Control>, desktop: bool) -> Self {
        Self {
            renderer,
            status,
            control,
            desktop,
            host: None,
            output: None,
            primary: None,
            secondaries: HashMap::new(),
            secondary_rate: 0,
            failed: None,
            searched: None,
            idle: None,
            checked: None,
            errors: crossbeam_channel::unbounded(),
            clock: Clock::default(),
        }
    }

    fn run(mut self, wake: Receiver<()>) {
        loop {
            if self.output.is_none() && self.clock.started.is_none() {
                lock(&self.renderer).drain();
            }
            let wanted = self.status.graphs.load(Ordering::Acquire) > 0;
            if self.desktop {
                self.check_errors();
                self.presence();
                self.manage(wanted);
            } else {
                self.simulate();
            }
            self.route(wanted || self.output.is_some());
            let ticking = wanted && self.output.is_none();
            if ticking {
                self.tick();
            } else {
                self.clock.stop();
            }
            let timeout = if ticking { TICK } else { IDLE_POLL };
            match wake.recv_timeout(timeout) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn tick(&mut self) {
        let connected = self.status.connected.load(Ordering::Acquire);
        let Some((rate, samples)) = self.clock.tick(&self.renderer) else {
            return;
        };
        let limit = rate as usize * 2 * CAPTURE_SECONDS;
        if self.desktop {
            return;
        }
        if connected && let Some(primary) = &self.primary {
            self.status.record(primary, samples, limit);
        }
        let mut drained = Vec::new();
        for (name, secondary) in &mut self.secondaries {
            if let Some(consumer) = secondary.consumer.as_mut() {
                drained.clear();
                while let Ok(sample) = consumer.pop() {
                    drained.push(sample);
                }
                self.status.record(name, &drained, limit);
            }
        }
    }

    fn check_errors(&mut self) {
        while let Ok((device, kind)) = self.errors.1.try_recv() {
            if matches!(kind, ErrorKind::Xrun | ErrorKind::RealtimeDenied | ErrorKind::DeviceChanged) {
                continue;
            }
            match device {
                None => {
                    self.output = None;
                    self.failed = None;
                    self.checked = None;
                }
                Some(name) => {
                    self.close_secondary(&name);
                    self.searched = Some(Instant::now());
                }
            }
        }
    }

    fn presence(&mut self) {
        let now = Instant::now();
        if self.checked.is_some_and(|checked| now.duration_since(checked) < FOLLOW) {
            return;
        }
        self.checked = Some(now);
        let host = self.host.get_or_insert_with(cpal::default_host);
        let default = host.default_output_device();
        self.status.publish(default.is_some());
        if self.output.is_none() {
            self.status.set_device(default.as_ref().and_then(name_of));
        }
        if let Some(output) = &self.output {
            let current = default.as_ref().and_then(|device| device.id().ok());
            if current.is_none() || current != output.device {
                self.output = None;
                self.failed = None;
            }
        }
    }

    fn manage(&mut self, wanted: bool) {
        let now = Instant::now();
        if wanted {
            self.idle = None;
        } else if self.output.is_some() || !self.secondaries.is_empty() {
            let idle = *self.idle.get_or_insert(now);
            if now.duration_since(idle) >= LINGER {
                self.output = None;
                self.idle = None;
                self.primary = None;
            }
        }
        if wanted && self.output.is_none() && self.failed.is_none_or(|failed| now.duration_since(failed) >= RETRY) {
            match self.open() {
                Ok(output) => {
                    self.output = Some(output);
                    self.failed = None;
                    self.status.publish(true);
                }
                Err(_) => {
                    self.failed = Some(now);
                    self.primary = None;
                    self.status.set_device(None);
                }
            }
        }
    }

    fn open(&mut self) -> Result<Output, String> {
        let host = self.host.get_or_insert_with(cpal::default_host);
        let device = host
            .default_output_device()
            .ok_or_else(|| "no audio output device is available".to_owned())?;
        let id = device.id().ok();
        let supported = device.default_output_config().map_err(|error| error.to_string())?;
        let rate = supported.sample_rate();
        let channels = usize::from(supported.channels()).max(1);
        lock(&self.renderer).set_rate(rate);
        self.status.rate.store(rate, Ordering::Release);
        let renderer = self.renderer.clone();
        let fill = move |samples: &mut [f32]| match renderer.try_lock() {
            Ok(mut renderer) => renderer.render(samples, channels),
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().render(samples, channels),
            Err(TryLockError::WouldBlock) => samples.fill(0.0),
        };
        let stream = open_stream(&device, supported, fill, self.errors.0.clone(), None)?;
        let name = name_of(&device);
        self.status.set_device(name.clone());
        self.primary = name;
        Ok(Output {
            _stream: stream,
            device: id,
        })
    }

    fn simulate(&mut self) {
        let devices = self.status.simulated();
        self.status.publish(!devices.is_empty());
        let primary = devices.first().cloned();
        if primary != self.primary {
            self.primary = primary.clone();
            self.status.set_device(primary);
        }
        let missing: Vec<String> = self
            .secondaries
            .keys()
            .filter(|name| !devices.contains(name))
            .cloned()
            .collect();
        for name in missing {
            self.close_secondary(&name);
        }
    }

    fn close_secondary(&mut self, name: &str) {
        if self.secondaries.remove(name).is_some() {
            let _ = self.control.send(Control::Detach { name: name.to_owned() });
        }
    }

    fn route(&mut self, wanted: bool) {
        let rate = self.status.rate.load(Ordering::Acquire);
        if rate != self.secondary_rate {
            self.secondary_rate = rate;
            let open: Vec<String> = self.secondaries.keys().cloned().collect();
            for name in open {
                self.close_secondary(&name);
            }
        }
        let requested = if wanted { self.status.requested() } else { Vec::new() };
        let stale: Vec<String> = self
            .secondaries
            .keys()
            .filter(|name| !requested.contains(name) || self.primary.as_ref() == Some(*name))
            .cloned()
            .collect();
        for name in stale {
            self.close_secondary(&name);
        }
        let missing: Vec<String> = requested
            .into_iter()
            .filter(|name| !self.secondaries.contains_key(name) && self.primary.as_ref() != Some(name))
            .collect();
        if missing.is_empty() {
            return;
        }
        if self.desktop {
            let now = Instant::now();
            if self.searched.is_some_and(|searched| now.duration_since(searched) < RETRY) {
                return;
            }
            self.searched = Some(now);
            self.open_secondaries(&missing, rate);
        } else {
            let devices = self.status.simulated();
            for name in missing.into_iter().filter(|name| devices.contains(name)) {
                let (outlet, consumer) = rtrb::RingBuffer::new(rate as usize * 2);
                let _ = self.control.send(Control::Attach {
                    name: name.clone(),
                    outlet,
                });
                self.secondaries.insert(
                    name,
                    Secondary {
                        _stream: None,
                        consumer: Some(consumer),
                    },
                );
            }
        }
    }

    fn open_secondaries(&mut self, missing: &[String], rate: u32) {
        let host = self.host.get_or_insert_with(cpal::default_host);
        let Ok(devices) = host.output_devices() else {
            return;
        };
        let mut found: Vec<(String, cpal::Device)> = Vec::new();
        for device in devices {
            if let Some(name) = name_of(&device)
                && missing.contains(&name)
                && !found.iter().any(|(known, _)| *known == name)
            {
                found.push((name, device));
            }
        }
        for (name, device) in found {
            let Ok(supported) = device.default_output_config() else {
                continue;
            };
            let channels = usize::from(supported.channels()).max(1);
            let (outlet, consumer) = rtrb::RingBuffer::new(rate as usize * 2);
            let mut drain = Drain::new(consumer, rate, supported.sample_rate());
            let fill = move |samples: &mut [f32]| drain.fill(samples, channels);
            let Ok(stream) = open_stream(&device, supported, fill, self.errors.0.clone(), Some(name.clone())) else {
                continue;
            };
            let _ = self.control.send(Control::Attach {
                name: name.clone(),
                outlet,
            });
            self.secondaries.insert(
                name,
                Secondary {
                    _stream: Some(stream),
                    consumer: None,
                },
            );
        }
    }
}

fn open_stream<F>(
    device: &cpal::Device,
    supported: SupportedStreamConfig,
    fill: F,
    errors: Sender<(Option<String>, ErrorKind)>,
    name: Option<String>,
) -> Result<cpal::Stream, String>
where
    F: FnMut(&mut [f32]) + Send + 'static,
{
    let format = supported.sample_format();
    let config = supported.config();
    let stream = match format {
        SampleFormat::F32 => build::<f32, F>(device, config, fill, errors, name),
        SampleFormat::F64 => build::<f64, F>(device, config, fill, errors, name),
        SampleFormat::I16 => build::<i16, F>(device, config, fill, errors, name),
        SampleFormat::I32 => build::<i32, F>(device, config, fill, errors, name),
        SampleFormat::I8 => build::<i8, F>(device, config, fill, errors, name),
        SampleFormat::U8 => build::<u8, F>(device, config, fill, errors, name),
        SampleFormat::U16 => build::<u16, F>(device, config, fill, errors, name),
        SampleFormat::U32 => build::<u32, F>(device, config, fill, errors, name),
        other => return Err(format!("the output device uses the unsupported sample format {other}")),
    }
    .map_err(|error| error.to_string())?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
}

fn build<T, F>(
    device: &cpal::Device,
    config: StreamConfig,
    mut fill: F,
    errors: Sender<(Option<String>, ErrorKind)>,
    name: Option<String>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
    F: FnMut(&mut [f32]) + Send + 'static,
{
    let mut scratch: Vec<f32> = Vec::with_capacity(16_384);
    device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            if scratch.len() < data.len() {
                scratch.resize(data.len(), 0.0);
            }
            let samples = &mut scratch[..data.len()];
            fill(samples);
            for (target, sample) in data.iter_mut().zip(samples.iter()) {
                *target = T::from_sample(*sample);
            }
        },
        move |error: cpal::Error| {
            let _ = errors.send((name.clone(), error.kind()));
        },
        None,
    )
}
