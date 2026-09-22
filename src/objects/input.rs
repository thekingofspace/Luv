use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use mlua::{AnyUserData, Lua, MetaMethod, Result, Table, UserData, UserDataFields, UserDataMethods, Value};
use tokio::sync::Notify;

use super::window::fire;
use super::{GameObject, Signal};
use crate::datatypes::enums::{
    CONTROLLER_AXIS, CONTROLLER_BUTTON, CONTROLLER_STICK, KEY_CODE, MOUSE_BUTTON, MOUSE_ICON, MOUSE_LOCK_MODE,
};
use crate::datatypes::{EnumItem, UDim};
use crate::runtime::Scheduler;
use crate::window::{
    ControllerEvent, ControllerId, ControllerState, Controllers, InputDevices, InputKind, TouchPhase, Vibration,
    WindowChange, WindowEvent, WindowId, WindowSystem,
};

pub const INPUT_APIS: [&str; 4] = ["Input", "Mouse", "Controller", "Touch"];

type Pads = BTreeMap<ControllerId, ControllerState>;

fn item(lua: &Lua, enum_type: &str, name: &str) -> Result<Value> {
    EnumItem::find(enum_type, name).map_or(Ok(Value::Nil), |item| item.canonical(lua))
}

fn signal(lua: &Lua, name: &str) -> Result<AnyUserData> {
    lua.create_userdata(Signal::named(name))
}

fn destroy_signals<'a>(signals: impl IntoIterator<Item = &'a AnyUserData>) {
    for signal in signals {
        if let Ok(mut signal) = signal.borrow_mut::<Signal>() {
            signal.destroy();
        }
    }
}

fn closed_error() -> mlua::Error {
    mlua::Error::runtime("this input API belongs to a window that is closed")
}

fn alive(inputs: &Weak<Inputs>) -> Result<Rc<Inputs>> {
    inputs.upgrade().filter(|inputs| inputs.open.get()).ok_or_else(closed_error)
}

fn items(lua: &Lua, enum_type: &str, names: impl IntoIterator<Item = &'static str>) -> Result<Table> {
    let values = names
        .into_iter()
        .map(|name| item(lua, enum_type, name))
        .collect::<Result<Vec<_>>>()?;
    lua.create_sequence_from(values)
}

struct KeyboardSignals {
    key_down: AnyUserData,
    key_up: AnyUserData,
    text_input: AnyUserData,
    activation_changed: AnyUserData,
}

impl KeyboardSignals {
    fn create(lua: &Lua) -> Result<Self> {
        Ok(Self {
            key_down: signal(lua, "KeyDown")?,
            key_up: signal(lua, "KeyUp")?,
            text_input: signal(lua, "TextInput")?,
            activation_changed: signal(lua, "ActivationChanged")?,
        })
    }

    fn all(&self) -> [&AnyUserData; 4] {
        [&self.key_down, &self.key_up, &self.text_input, &self.activation_changed]
    }
}

struct MouseSignals {
    moved: AnyUserData,
    button_down: AnyUserData,
    button_up: AnyUserData,
    scrolled: AnyUserData,
    entered: AnyUserData,
    left: AnyUserData,
    activation_changed: AnyUserData,
}

impl MouseSignals {
    fn create(lua: &Lua) -> Result<Self> {
        Ok(Self {
            moved: signal(lua, "Moved")?,
            button_down: signal(lua, "ButtonDown")?,
            button_up: signal(lua, "ButtonUp")?,
            scrolled: signal(lua, "Scrolled")?,
            entered: signal(lua, "Entered")?,
            left: signal(lua, "Left")?,
            activation_changed: signal(lua, "ActivationChanged")?,
        })
    }

    fn all(&self) -> [&AnyUserData; 7] {
        [
            &self.moved,
            &self.button_down,
            &self.button_up,
            &self.scrolled,
            &self.entered,
            &self.left,
            &self.activation_changed,
        ]
    }
}

struct TouchSignals {
    started: AnyUserData,
    moved: AnyUserData,
    ended: AnyUserData,
    cancelled: AnyUserData,
    activation_changed: AnyUserData,
}

impl TouchSignals {
    fn create(lua: &Lua) -> Result<Self> {
        Ok(Self {
            started: signal(lua, "Started")?,
            moved: signal(lua, "Moved")?,
            ended: signal(lua, "Ended")?,
            cancelled: signal(lua, "Cancelled")?,
            activation_changed: signal(lua, "ActivationChanged")?,
        })
    }

    fn all(&self) -> [&AnyUserData; 5] {
        [
            &self.started,
            &self.moved,
            &self.ended,
            &self.cancelled,
            &self.activation_changed,
        ]
    }
}

struct ControllerSignals {
    button_down: AnyUserData,
    button_up: AnyUserData,
    axis_changed: AnyUserData,
    connected: AnyUserData,
    disconnected: AnyUserData,
    activation_changed: AnyUserData,
}

impl ControllerSignals {
    fn create(lua: &Lua) -> Result<Self> {
        Ok(Self {
            button_down: signal(lua, "ButtonDown")?,
            button_up: signal(lua, "ButtonUp")?,
            axis_changed: signal(lua, "AxisChanged")?,
            connected: signal(lua, "Connected")?,
            disconnected: signal(lua, "Disconnected")?,
            activation_changed: signal(lua, "ActivationChanged")?,
        })
    }

    fn all(&self) -> [&AnyUserData; 6] {
        [
            &self.button_down,
            &self.button_up,
            &self.axis_changed,
            &self.connected,
            &self.disconnected,
            &self.activation_changed,
        ]
    }
}

type Slot<S> = RefCell<Option<(AnyUserData, Rc<S>)>>;

fn signals_of<S>(slot: &Slot<S>) -> Option<Rc<S>> {
    slot.borrow().as_ref().map(|(_, signals)| signals.clone())
}

fn cached<S>(slot: &Slot<S>, create: impl FnOnce() -> Result<(AnyUserData, Rc<S>)>) -> Result<AnyUserData> {
    if let Some((api, _)) = slot.borrow().as_ref() {
        return Ok(api.clone());
    }
    let (api, signals) = create()?;
    *slot.borrow_mut() = Some((api.clone(), signals));
    Ok(api)
}

pub struct Inputs {
    system: Arc<dyn WindowSystem>,
    id: WindowId,
    open: Cell<bool>,
    focused: Cell<bool>,
    closed: Rc<Notify>,
    keys: RefCell<BTreeSet<&'static str>>,
    buttons: RefCell<BTreeSet<&'static str>>,
    position: Cell<(f64, f64)>,
    inside: Cell<bool>,
    touches: RefCell<BTreeMap<u64, (f64, f64)>>,
    icon: Cell<&'static str>,
    visible: Cell<bool>,
    lock: Cell<&'static str>,
    controllers: RefCell<Option<Arc<Controllers>>>,
    devices: RefCell<Option<Arc<InputDevices>>>,
    watching: Cell<bool>,
    pads: Cell<Option<bool>>,
    keyboard: Slot<KeyboardSignals>,
    mouse: Slot<MouseSignals>,
    touch: Slot<TouchSignals>,
    controller: Slot<ControllerSignals>,
}

impl Inputs {
    pub fn new(system: Arc<dyn WindowSystem>, id: WindowId) -> Rc<Inputs> {
        Rc::new(Inputs {
            system,
            id,
            open: Cell::new(true),
            focused: Cell::new(false),
            closed: Rc::new(Notify::new()),
            keys: RefCell::new(BTreeSet::new()),
            buttons: RefCell::new(BTreeSet::new()),
            position: Cell::new((0.0, 0.0)),
            inside: Cell::new(false),
            touches: RefCell::new(BTreeMap::new()),
            icon: Cell::new("Default"),
            visible: Cell::new(true),
            lock: Cell::new("None"),
            controllers: RefCell::new(None),
            devices: RefCell::new(None),
            watching: Cell::new(false),
            pads: Cell::new(None),
            keyboard: RefCell::new(None),
            mouse: RefCell::new(None),
            touch: RefCell::new(None),
            controller: RefCell::new(None),
        })
    }

    fn position(&self) -> UDim {
        let (x, y) = self.position.get();
        UDim::new(x, y, 0.0)
    }

    fn controllers(&self) -> Arc<Controllers> {
        self.controllers
            .borrow_mut()
            .get_or_insert_with(|| self.system.controllers())
            .clone()
    }

    fn devices(&self) -> Arc<InputDevices> {
        self.devices
            .borrow_mut()
            .get_or_insert_with(|| self.system.input_devices())
            .clone()
    }

    fn present(&self, kind: InputKind) -> bool {
        self.devices().present(kind)
    }

    fn pads_connected(&self) -> bool {
        self.controllers().read(|pads| !pads.is_empty())
    }

    fn watch(self: &Rc<Self>, lua: &Lua) -> Result<()> {
        if self.watching.replace(true) {
            return Ok(());
        }
        let mut events = self.devices().subscribe();
        let inputs = Rc::downgrade(self);
        let closed = self.closed.clone();
        let scheduler = Scheduler::get(lua)?;
        let lua = lua.clone();
        tokio::task::spawn_local(async move {
            loop {
                let received = tokio::select! {
                    received = events.recv() => received,
                    () = closed.notified() => None,
                };
                let Some((kind, present)) = received else {
                    break;
                };
                let Some(inputs) = inputs.upgrade().filter(|inputs| inputs.open.get()) else {
                    break;
                };
                let signal = match kind {
                    InputKind::Keyboard => signals_of(&inputs.keyboard).map(|signals| signals.activation_changed.clone()),
                    InputKind::Mouse => signals_of(&inputs.mouse).map(|signals| signals.activation_changed.clone()),
                    InputKind::Touch => signals_of(&inputs.touch).map(|signals| signals.activation_changed.clone()),
                };
                if let Some(signal) = signal
                    && let Err(error) = fire(&lua, &signal, present)
                {
                    scheduler.report(error);
                }
            }
        });
        Ok(())
    }

    pub fn api(self: &Rc<Self>, lua: &Lua, name: &str) -> Result<Option<AnyUserData>> {
        let inputs = Rc::downgrade(self);
        if name != "Controller" {
            self.watch(lua)?;
        }
        let api = match name {
            "Input" => cached(&self.keyboard, || {
                let signals = Rc::new(KeyboardSignals::create(lua)?);
                let api = lua.create_userdata(InputApi {
                    inputs,
                    signals: signals.clone(),
                })?;
                Ok((api, signals))
            })?,
            "Mouse" => cached(&self.mouse, || {
                let signals = Rc::new(MouseSignals::create(lua)?);
                let api = lua.create_userdata(MouseApi {
                    inputs,
                    signals: signals.clone(),
                })?;
                Ok((api, signals))
            })?,
            "Touch" => cached(&self.touch, || {
                let signals = Rc::new(TouchSignals::create(lua)?);
                let api = lua.create_userdata(TouchApi {
                    inputs,
                    signals: signals.clone(),
                })?;
                Ok((api, signals))
            })?,
            "Controller" => cached(&self.controller, || {
                let signals = Rc::new(ControllerSignals::create(lua)?);
                let controllers = self.controllers();
                self.pads.set(Some(self.pads_connected()));
                self.listen(lua, &controllers, signals.clone())?;
                let api = lua.create_userdata(ControllerApi {
                    inputs,
                    controllers,
                    signals: signals.clone(),
                })?;
                Ok((api, signals))
            })?,
            _ => return Ok(None),
        };
        Ok(Some(api))
    }

    fn listen(self: &Rc<Self>, lua: &Lua, controllers: &Controllers, signals: Rc<ControllerSignals>) -> Result<()> {
        let mut events = controllers.subscribe();
        let inputs = Rc::downgrade(self);
        let closed = self.closed.clone();
        let scheduler = Scheduler::get(lua)?;
        let lua = lua.clone();
        tokio::task::spawn_local(async move {
            loop {
                if !inputs.upgrade().is_some_and(|inputs| inputs.open.get()) {
                    break;
                }
                let received = tokio::select! {
                    received = events.recv() => received,
                    () = closed.notified() => None,
                };
                let Some((id, event)) = received else {
                    break;
                };
                let Some(inputs) = inputs.upgrade() else {
                    break;
                };
                if let Err(error) = inputs.controller_event(&lua, &signals, id, event) {
                    scheduler.report(error);
                }
            }
        });
        Ok(())
    }

    fn controller_event(&self, lua: &Lua, signals: &ControllerSignals, id: ControllerId, event: ControllerEvent) -> Result<()> {
        if !self.open.get() {
            return Ok(());
        }
        match event {
            ControllerEvent::Connected { name, .. } => {
                fire(lua, &signals.connected, (id, name))?;
                self.pad_activation(lua, signals)
            }
            ControllerEvent::Disconnected => {
                fire(lua, &signals.disconnected, id)?;
                self.pad_activation(lua, signals)
            }
            _ if !self.focused.get() => Ok(()),
            ControllerEvent::Button { button, pressed } => {
                let signal = if pressed { &signals.button_down } else { &signals.button_up };
                fire(lua, signal, (item(lua, CONTROLLER_BUTTON, button)?, id))
            }
            ControllerEvent::Axis { axis, value } => {
                fire(lua, &signals.axis_changed, (item(lua, CONTROLLER_AXIS, axis)?, value, id))
            }
        }
    }

    fn pad_activation(&self, lua: &Lua, signals: &ControllerSignals) -> Result<()> {
        let connected = self.pads_connected();
        if self.pads.replace(Some(connected)) == Some(connected) {
            return Ok(());
        }
        fire(lua, &signals.activation_changed, connected)
    }

    pub fn handle(&self, lua: &Lua, event: WindowEvent) -> Result<()> {
        match event {
            WindowEvent::MouseMoved { x, y } => {
                let (old_x, old_y) = self.position.replace((x, y));
                match signals_of(&self.mouse).filter(|_| self.focused.get()) {
                    Some(signals) => fire(lua, &signals.moved, (self.position(), UDim::new(x - old_x, y - old_y, 0.0))),
                    None => Ok(()),
                }
            }
            WindowEvent::MouseInside(inside) => {
                if self.inside.replace(inside) == inside || !self.focused.get() {
                    return Ok(());
                }
                match signals_of(&self.mouse) {
                    Some(signals) => fire(lua, if inside { &signals.entered } else { &signals.left }, ()),
                    None => Ok(()),
                }
            }
            _ if !self.focused.get() => Ok(()),
            WindowEvent::Key { key, pressed } => self.key(lua, key, pressed),
            WindowEvent::Text(text) => match signals_of(&self.keyboard) {
                Some(signals) => fire(lua, &signals.text_input, text),
                None => Ok(()),
            },
            WindowEvent::MouseMotion { x, y } => match signals_of(&self.mouse).filter(|_| self.lock.get() == "Locked") {
                Some(signals) => fire(lua, &signals.moved, (self.position(), UDim::new(x, y, 0.0))),
                None => Ok(()),
            },
            WindowEvent::MouseButton { button, pressed } => {
                let changed = if pressed {
                    self.buttons.borrow_mut().insert(button)
                } else {
                    self.buttons.borrow_mut().remove(button)
                };
                match signals_of(&self.mouse).filter(|_| changed) {
                    Some(signals) => fire(
                        lua,
                        if pressed { &signals.button_down } else { &signals.button_up },
                        (item(lua, MOUSE_BUTTON, button)?, self.position()),
                    ),
                    None => Ok(()),
                }
            }
            WindowEvent::MouseWheel { x, y } => match signals_of(&self.mouse) {
                Some(signals) => fire(lua, &signals.scrolled, UDim::new(x, y, 0.0)),
                None => Ok(()),
            },
            WindowEvent::Touch { id, phase, x, y, force } => self.touch_event(lua, id, phase, (x, y), force),
            _ => Ok(()),
        }
    }

    fn key(&self, lua: &Lua, key: &'static str, pressed: bool) -> Result<()> {
        let changed = if pressed {
            self.keys.borrow_mut().insert(key)
        } else {
            self.keys.borrow_mut().remove(key)
        };
        match signals_of(&self.keyboard).filter(|_| changed) {
            Some(signals) => fire(
                lua,
                if pressed { &signals.key_down } else { &signals.key_up },
                item(lua, KEY_CODE, key)?,
            ),
            None => Ok(()),
        }
    }

    fn touch_event(&self, lua: &Lua, id: u64, phase: TouchPhase, (x, y): (f64, f64), force: Option<f64>) -> Result<()> {
        let previous = match phase {
            TouchPhase::Started | TouchPhase::Moved => self.touches.borrow_mut().insert(id, (x, y)),
            TouchPhase::Ended | TouchPhase::Cancelled => self.touches.borrow_mut().remove(&id),
        };
        let Some(signals) = signals_of(&self.touch) else {
            return Ok(());
        };
        let position = UDim::new(x, y, 0.0);
        match phase {
            TouchPhase::Started => fire(lua, &signals.started, (id, position, force)),
            TouchPhase::Moved => {
                let delta = previous.map_or(UDim::ZERO, |(old_x, old_y)| UDim::new(x - old_x, y - old_y, 0.0));
                fire(lua, &signals.moved, (id, position, delta, force))
            }
            TouchPhase::Ended if previous.is_some() => fire(lua, &signals.ended, (id, position)),
            TouchPhase::Cancelled if previous.is_some() => fire(lua, &signals.cancelled, id),
            _ => Ok(()),
        }
    }

    pub fn focus(&self, lua: &Lua, focused: bool) -> Result<()> {
        if self.focused.replace(focused) == focused || focused {
            return Ok(());
        }
        let keys = std::mem::take(&mut *self.keys.borrow_mut());
        let buttons = std::mem::take(&mut *self.buttons.borrow_mut());
        let touches = std::mem::take(&mut *self.touches.borrow_mut());
        if let Some(signals) = signals_of(&self.keyboard) {
            for key in keys {
                fire(lua, &signals.key_up, item(lua, KEY_CODE, key)?)?;
            }
        }
        if let Some(signals) = signals_of(&self.mouse) {
            for button in buttons {
                fire(lua, &signals.button_up, (item(lua, MOUSE_BUTTON, button)?, self.position()))?;
            }
        }
        if let Some(signals) = signals_of(&self.touch) {
            for id in touches.into_keys() {
                fire(lua, &signals.cancelled, id)?;
            }
        }
        let Some(signals) = signals_of(&self.controller) else {
            return Ok(());
        };
        for (id, pad) in self.controllers().snapshot() {
            for button in pad.buttons {
                fire(lua, &signals.button_up, (item(lua, CONTROLLER_BUTTON, button)?, id))?;
            }
            for (axis, value) in pad.axes {
                if value != 0.0 {
                    fire(lua, &signals.axis_changed, (item(lua, CONTROLLER_AXIS, axis)?, 0.0, id))?;
                }
            }
        }
        Ok(())
    }

    pub fn close(&self) {
        if !self.open.replace(false) {
            return;
        }
        self.focused.set(false);
        self.closed.notify_waiters();
        self.keys.borrow_mut().clear();
        self.buttons.borrow_mut().clear();
        self.touches.borrow_mut().clear();
        self.keyboard.borrow_mut().take();
        self.mouse.borrow_mut().take();
        self.touch.borrow_mut().take();
        self.controller.borrow_mut().take();
    }

    pub fn destroy(&self) {
        if let Some(signals) = signals_of(&self.keyboard) {
            destroy_signals(signals.all());
        }
        if let Some(signals) = signals_of(&self.mouse) {
            destroy_signals(signals.all());
        }
        if let Some(signals) = signals_of(&self.touch) {
            destroy_signals(signals.all());
        }
        if let Some(signals) = signals_of(&self.controller) {
            destroy_signals(signals.all());
        }
        self.close();
    }
}

pub struct InputApi {
    inputs: Weak<Inputs>,
    signals: Rc<KeyboardSignals>,
}

impl UserData for InputApi {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "InputAPI");
        fields.add_field_method_get("KeyDown", |_, this| Ok(this.signals.key_down.clone()));
        fields.add_field_method_get("KeyUp", |_, this| Ok(this.signals.key_up.clone()));
        fields.add_field_method_get("TextInput", |_, this| Ok(this.signals.text_input.clone()));
        fields.add_field_method_get("ActivationChanged", |_, this| Ok(this.signals.activation_changed.clone()));
        fields.add_field_method_get("IsConnected", |_, this| {
            Ok(alive(&this.inputs)?.present(InputKind::Keyboard))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("IsKeyDown", |_, this, key: EnumItem| {
            let key = key.of(KEY_CODE)?.name;
            let inputs = alive(&this.inputs)?;
            Ok(inputs.focused.get() && inputs.keys.borrow().contains(key))
        });
        methods.add_method("GetKeysDown", |lua, this, ()| {
            let inputs = alive(&this.inputs)?;
            let keys: Vec<&'static str> = inputs.keys.borrow().iter().copied().collect();
            items(lua, KEY_CODE, keys)
        });
    }
}

pub struct MouseApi {
    inputs: Weak<Inputs>,
    signals: Rc<MouseSignals>,
}

impl UserData for MouseApi {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "MouseAPI");
        fields.add_field_method_get("Moved", |_, this| Ok(this.signals.moved.clone()));
        fields.add_field_method_get("ButtonDown", |_, this| Ok(this.signals.button_down.clone()));
        fields.add_field_method_get("ButtonUp", |_, this| Ok(this.signals.button_up.clone()));
        fields.add_field_method_get("Scrolled", |_, this| Ok(this.signals.scrolled.clone()));
        fields.add_field_method_get("Entered", |_, this| Ok(this.signals.entered.clone()));
        fields.add_field_method_get("Left", |_, this| Ok(this.signals.left.clone()));
        fields.add_field_method_get("ActivationChanged", |_, this| Ok(this.signals.activation_changed.clone()));
        fields.add_field_method_get("IsConnected", |_, this| Ok(alive(&this.inputs)?.present(InputKind::Mouse)));
        fields.add_field_method_get("Position", |_, this| Ok(alive(&this.inputs)?.position()));
        fields.add_field_method_get("IsInside", |_, this| Ok(alive(&this.inputs)?.inside.get()));
        fields.add_field_method_get("Icon", |lua, this| item(lua, MOUSE_ICON, alive(&this.inputs)?.icon.get()));
        fields.add_field_method_set("Icon", |_, this, icon: EnumItem| {
            let icon = icon.of(MOUSE_ICON)?.name;
            let inputs = alive(&this.inputs)?;
            if inputs.icon.replace(icon) != icon {
                inputs.system.change(inputs.id, WindowChange::CursorIcon(icon));
            }
            Ok(())
        });
        fields.add_field_method_get("Visible", |_, this| Ok(alive(&this.inputs)?.visible.get()));
        fields.add_field_method_set("Visible", |_, this, visible: bool| {
            let inputs = alive(&this.inputs)?;
            if inputs.visible.replace(visible) != visible {
                inputs.system.change(inputs.id, WindowChange::CursorVisible(visible));
            }
            Ok(())
        });
        fields.add_field_method_get("LockMode", |lua, this| {
            item(lua, MOUSE_LOCK_MODE, alive(&this.inputs)?.lock.get())
        });
        fields.add_field_method_set("LockMode", |_, this, mode: EnumItem| {
            let mode = mode.of(MOUSE_LOCK_MODE)?.name;
            let inputs = alive(&this.inputs)?;
            if inputs.lock.replace(mode) != mode {
                inputs.system.change(inputs.id, WindowChange::CursorLock(mode));
            }
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("IsButtonDown", |_, this, button: EnumItem| {
            let button = button.of(MOUSE_BUTTON)?.name;
            let inputs = alive(&this.inputs)?;
            Ok(inputs.focused.get() && inputs.buttons.borrow().contains(button))
        });
        methods.add_method("GetButtonsDown", |lua, this, ()| {
            let inputs = alive(&this.inputs)?;
            let buttons: Vec<&'static str> = inputs.buttons.borrow().iter().copied().collect();
            items(lua, MOUSE_BUTTON, buttons)
        });
    }
}

pub struct TouchApi {
    inputs: Weak<Inputs>,
    signals: Rc<TouchSignals>,
}

impl UserData for TouchApi {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "TouchAPI");
        fields.add_field_method_get("Started", |_, this| Ok(this.signals.started.clone()));
        fields.add_field_method_get("Moved", |_, this| Ok(this.signals.moved.clone()));
        fields.add_field_method_get("Ended", |_, this| Ok(this.signals.ended.clone()));
        fields.add_field_method_get("Cancelled", |_, this| Ok(this.signals.cancelled.clone()));
        fields.add_field_method_get("ActivationChanged", |_, this| Ok(this.signals.activation_changed.clone()));
        fields.add_field_method_get("IsConnected", |_, this| Ok(alive(&this.inputs)?.present(InputKind::Touch)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetTouches", |lua, this, ()| {
            let inputs = alive(&this.inputs)?;
            let list = lua.create_table()?;
            for (id, (x, y)) in inputs.touches.borrow().iter() {
                let entry = lua.create_table()?;
                entry.set("Id", *id)?;
                entry.set("Position", UDim::new(*x, *y, 0.0))?;
                list.raw_push(entry)?;
            }
            Ok(list)
        });
    }
}

fn pressed(pads: &Pads, button: &str, id: Option<ControllerId>) -> bool {
    match id {
        Some(id) => pads.get(&id).is_some_and(|pad| pad.buttons.contains(button)),
        None => pads.values().any(|pad| pad.buttons.contains(button)),
    }
}

fn axis_of(pad: &ControllerState, axis: &str) -> f64 {
    pad.axes.get(axis).copied().unwrap_or(0.0)
}

fn axis_value(pads: &Pads, axis: &str, id: Option<ControllerId>) -> f64 {
    match id {
        Some(id) => pads.get(&id).map_or(0.0, |pad| axis_of(pad, axis)),
        None => pads
            .values()
            .map(|pad| axis_of(pad, axis))
            .fold(0.0, |strongest: f64, value| if value.abs() > strongest.abs() { value } else { strongest }),
    }
}

fn stick_value(pads: &Pads, (x, y): (&str, &str), id: Option<ControllerId>) -> (f64, f64) {
    let read = |pad: &ControllerState| (axis_of(pad, x), axis_of(pad, y));
    match id {
        Some(id) => pads.get(&id).map_or((0.0, 0.0), read),
        None => pads.values().map(read).fold((0.0, 0.0), |strongest: (f64, f64), value| {
            if value.0.hypot(value.1) > strongest.0.hypot(strongest.1) { value } else { strongest }
        }),
    }
}

pub struct ControllerApi {
    inputs: Weak<Inputs>,
    controllers: Arc<Controllers>,
    signals: Rc<ControllerSignals>,
}

impl ControllerApi {
    fn focused(&self) -> Result<bool> {
        Ok(alive(&self.inputs)?.focused.get())
    }

    fn vibrate(&self, strength: f64, seconds: f64, id: Option<ControllerId>) -> Result<()> {
        alive(&self.inputs)?;
        if !strength.is_finite() {
            return Err(mlua::Error::runtime("the vibration strength must be a number between 0 and 1"));
        }
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|_| mlua::Error::runtime("the vibration duration must be a number of seconds, 0 or more"))?;
        self.controllers.vibrate(Vibration {
            id,
            strength: strength.clamp(0.0, 1.0),
            duration,
        });
        Ok(())
    }
}

impl UserData for ControllerApi {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "ControllerAPI");
        fields.add_field_method_get("ButtonDown", |_, this| Ok(this.signals.button_down.clone()));
        fields.add_field_method_get("ButtonUp", |_, this| Ok(this.signals.button_up.clone()));
        fields.add_field_method_get("AxisChanged", |_, this| Ok(this.signals.axis_changed.clone()));
        fields.add_field_method_get("Connected", |_, this| Ok(this.signals.connected.clone()));
        fields.add_field_method_get("Disconnected", |_, this| Ok(this.signals.disconnected.clone()));
        fields.add_field_method_get("ActivationChanged", |_, this| Ok(this.signals.activation_changed.clone()));
        fields.add_field_method_get("IsConnected", |_, this| {
            alive(&this.inputs)?;
            Ok(this.controllers.read(|pads| !pads.is_empty()))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetControllers", |lua, this, ()| {
            alive(&this.inputs)?;
            let list = lua.create_table()?;
            for (id, pad) in this.controllers.snapshot() {
                let entry = lua.create_table()?;
                entry.set("Id", id)?;
                entry.set("Name", pad.name)?;
                entry.set("CanVibrate", pad.vibrates)?;
                list.raw_push(entry)?;
            }
            Ok(list)
        });
        methods.add_method("IsControllerConnected", |_, this, id: ControllerId| {
            alive(&this.inputs)?;
            Ok(this.controllers.read(|pads| pads.contains_key(&id)))
        });
        methods.add_method("IsButtonDown", |_, this, (button, id): (EnumItem, Option<ControllerId>)| {
            let button = button.of(CONTROLLER_BUTTON)?.name;
            Ok(this.focused()? && this.controllers.read(|pads| pressed(pads, button, id)))
        });
        methods.add_method("GetButtonsDown", |lua, this, id: Option<ControllerId>| {
            let held: BTreeSet<&'static str> = if this.focused()? {
                this.controllers.read(|pads| {
                    pads.iter()
                        .filter(|(pad, _)| id.is_none_or(|wanted| **pad == wanted))
                        .flat_map(|(_, state)| state.buttons.iter().copied())
                        .collect()
                })
            } else {
                BTreeSet::new()
            };
            items(lua, CONTROLLER_BUTTON, held)
        });
        methods.add_method("GetAxis", |_, this, (axis, id): (EnumItem, Option<ControllerId>)| {
            let axis = axis.of(CONTROLLER_AXIS)?.name;
            if !this.focused()? {
                return Ok(0.0);
            }
            Ok(this.controllers.read(|pads| axis_value(pads, axis, id)))
        });
        methods.add_method("GetStick", |_, this, (stick, id): (EnumItem, Option<ControllerId>)| {
            let axes = match stick.of(CONTROLLER_STICK)?.name {
                "Left" => ("LeftStickX", "LeftStickY"),
                _ => ("RightStickX", "RightStickY"),
            };
            if !this.focused()? {
                return Ok(UDim::ZERO);
            }
            let (x, y) = this.controllers.read(|pads| stick_value(pads, axes, id));
            Ok(UDim::new(x, y, 0.0))
        });
        methods.add_method(
            "Vibrate",
            |_, this, (strength, seconds, id): (f64, f64, Option<ControllerId>)| this.vibrate(strength, seconds, id),
        );
        methods.add_method("StopVibrating", |_, this, id: Option<ControllerId>| this.vibrate(0.0, 0.0, id));
    }
}
