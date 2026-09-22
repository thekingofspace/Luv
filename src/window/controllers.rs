use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use tokio::sync::mpsc;

const POLL: Duration = Duration::from_millis(16);

pub type ControllerId = usize;

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerEvent {
    Connected { name: String, vibrates: bool },
    Disconnected,
    Button { button: &'static str, pressed: bool },
    Axis { axis: &'static str, value: f64 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControllerState {
    pub name: String,
    pub vibrates: bool,
    pub buttons: BTreeSet<&'static str>,
    pub axes: BTreeMap<&'static str, f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vibration {
    pub id: Option<ControllerId>,
    pub strength: f64,
    pub duration: Duration,
}

pub type ControllerListener = mpsc::UnboundedReceiver<(ControllerId, ControllerEvent)>;

#[derive(Default)]
pub struct Controllers {
    pads: Mutex<BTreeMap<ControllerId, ControllerState>>,
    listeners: Mutex<Vec<mpsc::UnboundedSender<(ControllerId, ControllerEvent)>>>,
    commands: Option<Sender<Vibration>>,
    vibrations: Mutex<Vec<Vibration>>,
}

impl Controllers {
    pub fn simulated() -> Arc<Controllers> {
        Arc::new(Controllers::default())
    }

    pub fn system() -> Arc<Controllers> {
        static SYSTEM: OnceLock<Arc<Controllers>> = OnceLock::new();
        SYSTEM
            .get_or_init(|| {
                let (sender, receiver) = crossbeam_channel::unbounded();
                let hub = Arc::new(Controllers {
                    commands: Some(sender),
                    ..Controllers::default()
                });
                let poller = hub.clone();
                let _ = thread::Builder::new()
                    .name("luv-controllers".to_owned())
                    .spawn(move || poller.poll(receiver));
                hub
            })
            .clone()
    }

    pub fn publish(&self, id: ControllerId, event: ControllerEvent) {
        {
            let mut pads = self.pads.lock().unwrap_or_else(PoisonError::into_inner);
            match &event {
                ControllerEvent::Connected { name, vibrates } => {
                    let pad = pads.entry(id).or_default();
                    pad.name = name.clone();
                    pad.vibrates = *vibrates;
                }
                ControllerEvent::Disconnected => {
                    pads.remove(&id);
                }
                ControllerEvent::Button { button, pressed } => {
                    let pad = pads.entry(id).or_default();
                    if *pressed {
                        pad.buttons.insert(button);
                    } else {
                        pad.buttons.remove(button);
                    }
                }
                ControllerEvent::Axis { axis, value } => {
                    pads.entry(id).or_default().axes.insert(axis, *value);
                }
            }
        }
        self.listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|listener| listener.send((id, event.clone())).is_ok());
    }

    pub fn subscribe(&self) -> ControllerListener {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.listeners.lock().unwrap_or_else(PoisonError::into_inner).push(sender);
        receiver
    }

    pub fn snapshot(&self) -> BTreeMap<ControllerId, ControllerState> {
        self.pads.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn read<R>(&self, read: impl FnOnce(&BTreeMap<ControllerId, ControllerState>) -> R) -> R {
        read(&self.pads.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn vibrate(&self, vibration: Vibration) {
        match &self.commands {
            Some(commands) => {
                let _ = commands.send(vibration);
            }
            None => self.vibrations.lock().unwrap_or_else(PoisonError::into_inner).push(vibration),
        }
    }

    pub fn take_vibrations(&self) -> Vec<Vibration> {
        std::mem::take(&mut *self.vibrations.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn poll(&self, commands: Receiver<Vibration>) {
        let Ok(mut gilrs) = gilrs::Gilrs::new() else {
            return;
        };
        let present: Vec<(ControllerId, ControllerEvent)> = gilrs
            .gamepads()
            .map(|(id, gamepad)| (usize::from(id), connected(&gamepad)))
            .collect();
        for (id, event) in present {
            self.publish(id, event);
        }
        let mut playing: Vec<(Option<ControllerId>, Instant, gilrs::ff::Effect)> = Vec::new();
        loop {
            for vibration in commands.try_iter() {
                playing.retain(|(id, _, _)| vibration.id.is_some() && *id != vibration.id);
                if vibration.strength > 0.0
                    && !vibration.duration.is_zero()
                    && let Some(effect) = rumble(&mut gilrs, &vibration)
                {
                    playing.push((vibration.id, Instant::now() + vibration.duration, effect));
                }
            }
            let now = Instant::now();
            playing.retain(|(_, until, _)| *until > now);
            let Some(event) = gilrs.next_event_blocking(Some(POLL)) else {
                continue;
            };
            let id = usize::from(event.id);
            let translated = match event.event {
                gilrs::EventType::Connected => Some(connected(&gilrs.gamepad(event.id))),
                other => translate(other),
            };
            if let Some(translated) = translated {
                self.publish(id, translated);
            }
        }
    }
}

fn connected(gamepad: &gilrs::Gamepad<'_>) -> ControllerEvent {
    ControllerEvent::Connected {
        name: gamepad.name().to_owned(),
        vibrates: gamepad.is_ff_supported(),
    }
}

fn rumble(gilrs: &mut gilrs::Gilrs, vibration: &Vibration) -> Option<gilrs::ff::Effect> {
    use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};
    let targets: Vec<gilrs::GamepadId> = gilrs
        .gamepads()
        .filter(|(id, gamepad)| {
            gamepad.is_ff_supported() && vibration.id.is_none_or(|wanted| usize::from(*id) == wanted)
        })
        .map(|(id, _)| id)
        .collect();
    if targets.is_empty() {
        return None;
    }
    let milliseconds = u32::try_from(vibration.duration.as_millis()).unwrap_or(u32::MAX);
    let scheduling = Replay {
        play_for: Ticks::from_ms(milliseconds),
        with_delay: Ticks::from_ms(u32::MAX),
        ..Replay::default()
    };
    let magnitude = (vibration.strength.clamp(0.0, 1.0) * f64::from(u16::MAX)) as u16;
    let effect = EffectBuilder::new()
        .add_effect(BaseEffect {
            kind: BaseEffectType::Strong { magnitude },
            scheduling,
            ..BaseEffect::default()
        })
        .add_effect(BaseEffect {
            kind: BaseEffectType::Weak { magnitude },
            scheduling,
            ..BaseEffect::default()
        })
        .gamepads(&targets)
        .finish(gilrs)
        .ok()?;
    effect.play().ok()?;
    Some(effect)
}

fn button(button: gilrs::Button) -> Option<&'static str> {
    use gilrs::Button;
    Some(match button {
        Button::South => "A",
        Button::East => "B",
        Button::West => "X",
        Button::North => "Y",
        Button::LeftTrigger => "LeftBumper",
        Button::RightTrigger => "RightBumper",
        Button::LeftTrigger2 => "LeftTrigger",
        Button::RightTrigger2 => "RightTrigger",
        Button::Select => "Select",
        Button::Start => "Start",
        Button::Mode => "Home",
        Button::LeftThumb => "LeftStick",
        Button::RightThumb => "RightStick",
        Button::DPadUp => "DPadUp",
        Button::DPadDown => "DPadDown",
        Button::DPadLeft => "DPadLeft",
        Button::DPadRight => "DPadRight",
        _ => return None,
    })
}

fn axis(axis: gilrs::Axis) -> Option<&'static str> {
    use gilrs::Axis;
    Some(match axis {
        Axis::LeftStickX => "LeftStickX",
        Axis::LeftStickY => "LeftStickY",
        Axis::RightStickX => "RightStickX",
        Axis::RightStickY => "RightStickY",
        Axis::LeftZ => "LeftTrigger",
        Axis::RightZ => "RightTrigger",
        _ => return None,
    })
}

fn translate(event: gilrs::EventType) -> Option<ControllerEvent> {
    use gilrs::EventType;
    Some(match event {
        EventType::ButtonPressed(pressed, _) => ControllerEvent::Button {
            button: button(pressed)?,
            pressed: true,
        },
        EventType::ButtonReleased(released, _) => ControllerEvent::Button {
            button: button(released)?,
            pressed: false,
        },
        EventType::ButtonChanged(gilrs::Button::LeftTrigger2, value, _) => ControllerEvent::Axis {
            axis: "LeftTrigger",
            value: f64::from(value),
        },
        EventType::ButtonChanged(gilrs::Button::RightTrigger2, value, _) => ControllerEvent::Axis {
            axis: "RightTrigger",
            value: f64::from(value),
        },
        EventType::AxisChanged(changed, value, _) => ControllerEvent::Axis {
            axis: axis(changed)?,
            value: f64::from(value),
        },
        EventType::Disconnected => ControllerEvent::Disconnected,
        _ => return None,
    })
}
