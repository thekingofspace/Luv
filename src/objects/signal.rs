use std::mem;

use mlua::{AnyUserData, Function, Lua, MultiValue, Result, UserData, UserDataFields, UserDataMethods};

use super::{BaseGameObject, GameObject};
use crate::runtime::{Scheduler, Tracker, Waiter};

struct KeepAlive {
    tracker: Tracker,
    holding: bool,
}

pub struct Signal {
    base: BaseGameObject,
    handlers: Vec<(String, Function)>,
    waiters: Vec<Waiter>,
    keep_alive: Option<KeepAlive>,
}

impl Signal {
    pub const CLASS_NAME: &'static str = "Signal";

    pub fn new() -> Self {
        Self {
            base: BaseGameObject::new(Self::CLASS_NAME),
            handlers: Vec::new(),
            waiters: Vec::new(),
            keep_alive: None,
        }
    }

    pub fn named(name: impl Into<String>) -> Self {
        let mut signal = Self::new();
        signal.base.set_name(name);
        signal
    }

    pub fn keep_alive_while_listened(mut self, tracker: Tracker) -> Self {
        self.keep_alive = Some(KeepAlive { tracker, holding: false });
        self
    }

    pub fn is_listened(&self) -> bool {
        !self.base.is_destroyed() && (!self.handlers.is_empty() || !self.waiters.is_empty())
    }

    fn refresh(&mut self) {
        let listened = self.is_listened();
        if let Some(keep_alive) = &mut self.keep_alive
            && keep_alive.holding != listened
        {
            if listened {
                keep_alive.tracker.enter();
            } else {
                keep_alive.tracker.exit();
            }
            keep_alive.holding = listened;
        }
    }

    pub fn bind_handler(&mut self, id: impl Into<String>, handler: Function) -> Result<()> {
        self.base.ensure_alive()?;
        let id = id.into();
        if self.is_bound(&id) {
            return Err(mlua::Error::runtime(format!(
                "handler '{id}' is already bound to {}, call UnBind(\"{id}\") before binding it again",
                self.base.name()
            )));
        }
        self.handlers.push((id, handler));
        self.refresh();
        Ok(())
    }

    pub fn unbind(&mut self, id: &str) -> Option<Function> {
        let index = self.handlers.iter().position(|(bound, _)| bound == id)?;
        let handler = self.handlers.remove(index).1;
        self.refresh();
        Some(handler)
    }

    pub fn is_bound(&self, id: &str) -> bool {
        self.handlers.iter().any(|(bound, _)| bound == id)
    }

    pub fn handler(&self, id: &str) -> Option<Function> {
        self.handlers
            .iter()
            .find(|(bound, _)| bound == id)
            .map(|(_, handler)| handler.clone())
    }

    pub fn handler_ids(&self) -> Vec<String> {
        self.handlers.iter().map(|(id, _)| id.clone()).collect()
    }

    pub fn fire(lua: &Lua, signal: &AnyUserData, args: MultiValue) -> Result<()> {
        let scheduler = Scheduler::get(lua)?;
        let (ids, waiters) = {
            let mut this = signal.borrow_mut::<Signal>()?;
            this.base.ensure_alive()?;
            (this.handler_ids(), mem::take(&mut this.waiters))
        };
        for id in ids {
            let handler = signal.borrow::<Signal>()?.handler(&id);
            if let Some(handler) = handler {
                scheduler.spawn(lua, handler, args.clone());
            }
        }
        for waiter in waiters {
            waiter.wake(Ok(args.clone()));
        }
        signal.borrow_mut::<Signal>()?.refresh();
        Ok(())
    }

    pub async fn invoke(signal: &AnyUserData, id: &str, args: MultiValue) -> Result<MultiValue> {
        let handler = {
            let this = signal.borrow::<Signal>()?;
            this.base.ensure_alive()?;
            this.handler(id).ok_or_else(|| {
                mlua::Error::runtime(format!("no handler is bound to '{id}' on {}", this.base.name()))
            })?
        };
        handler.call_async(args).await
    }

    pub async fn wait(lua: &Lua, signal: &AnyUserData) -> Result<MultiValue> {
        let (waiter, wait) = Scheduler::get(lua)?.waiter();
        {
            let mut this = signal.borrow_mut::<Signal>()?;
            this.base.ensure_alive()?;
            this.waiters.push(waiter);
            this.refresh();
        }
        wait.wait().await
    }
}

impl Drop for Signal {
    fn drop(&mut self) {
        if let Some(keep_alive) = &self.keep_alive
            && keep_alive.holding
        {
            keep_alive.tracker.exit();
        }
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::new()
    }
}

impl GameObject for Signal {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.handlers.clear();
        let message = format!("{} was destroyed while it was being waited on", self.base.name());
        for waiter in self.waiters.drain(..) {
            waiter.wake(Err(mlua::Error::runtime(message.clone())));
        }
        self.refresh();
    }
}

impl UserData for Signal {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);

        methods.add_method_mut("BindHandler", |_, this, (id, handler): (String, Function)| {
            this.bind_handler(id, handler)
        });
        methods.add_method_mut("UnBind", |_, this, id: String| Ok(this.unbind(&id).is_some()));
        methods.add_method("IsBound", |_, this, id: String| Ok(this.is_bound(&id)));
        methods.add_function("Fire", |lua, (signal, args): (AnyUserData, MultiValue)| {
            Signal::fire(lua, &signal, args)
        });
        methods.add_async_function(
            "Invoke",
            |_, (signal, id, args): (AnyUserData, String, MultiValue)| async move {
                Signal::invoke(&signal, &id, args).await
            },
        );
        methods.add_async_function("Wait", |lua, signal: AnyUserData| async move {
            Signal::wait(&lua, &signal).await
        });
    }
}
