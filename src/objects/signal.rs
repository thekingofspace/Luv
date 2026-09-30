use std::mem;
use std::sync::Arc;

use mlua::{AnyUserData, Function, Lua, MultiValue, Result, UserData, UserDataFields, UserDataMethods, Value};

use super::{BaseGameObject, GameObject};
use crate::api::thread::ThreadHandle;
use crate::concurrency::parallel;
use crate::runtime::{Engine, Launch, Message, Scheduler, Tracker, Waiter, encode};

struct Worker {
    id: u64,
    engine: Arc<Engine>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.engine.bus().send_to(self.id, Message::Stop);
    }
}

struct KeepAlive {
    tracker: Tracker,
    holding: bool,
}

pub struct Signal {
    base: BaseGameObject,
    handlers: Vec<(String, Function)>,
    parallel: Vec<(String, Worker)>,
    waiters: Vec<Waiter>,
    keep_alive: Option<KeepAlive>,
}

impl Signal {
    pub const CLASS_NAME: &'static str = "Signal";

    pub fn new() -> Self {
        Self {
            base: BaseGameObject::new(Self::CLASS_NAME),
            handlers: Vec::new(),
            parallel: Vec::new(),
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
        !self.base.is_destroyed()
            && (!self.handlers.is_empty() || !self.waiters.is_empty() || !self.parallel.is_empty())
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
        self.handlers.iter().any(|(bound, _)| bound == id) || self.parallel.iter().any(|(bound, _)| bound == id)
    }

    fn unbind_any(&mut self, id: &str) -> bool {
        if self.unbind(id).is_some() {
            return true;
        }
        let Some(index) = self.parallel.iter().position(|(bound, _)| bound == id) else {
            return false;
        };
        self.parallel.remove(index);
        self.refresh();
        true
    }

    pub fn bind_parallel(lua: &Lua, signal: &AnyUserData, id: String, target: Value) -> Result<ThreadHandle> {
        let function = parallel::expect(
            &target,
            "BindParallel",
            "signal:BindParallel(\"id\", function(...) end)",
        )?;
        let label = {
            let this = signal.borrow::<Signal>()?;
            this.base.ensure_alive()?;
            if this.is_bound(&id) {
                return Err(mlua::Error::runtime(format!(
                    "handler '{id}' is already bound to {}, call UnBind(\"{id}\") before binding it again",
                    this.base.name()
                )));
            }
            format!("BindParallel '{id}' on {} at {}", this.base.name(), function.place())
        };
        let engine = lua
            .app_data_ref::<Arc<Engine>>()
            .map(|engine| engine.clone())
            .ok_or_else(|| mlua::Error::runtime("the luv engine is not running"))?;
        let worker = function.spawn(lua, label, Launch::Bound)?;
        let mut this = signal.borrow_mut::<Signal>()?;
        this.parallel.push((id, Worker { id: worker, engine }));
        this.refresh();
        Ok(ThreadHandle::new(worker))
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
        Self::fire_with(lua, signal, args, |scheduler, lua, handler, args| {
            scheduler.spawn(lua, handler, args);
        })
    }

    pub fn fire_with(
        lua: &Lua,
        signal: &AnyUserData,
        args: MultiValue,
        spawn: impl Fn(&Scheduler, &Lua, Function, MultiValue),
    ) -> Result<()> {
        let scheduler = Scheduler::get(lua)?;
        let (ids, waiters, workers, name) = {
            let mut this = signal.borrow_mut::<Signal>()?;
            this.base.ensure_alive()?;
            let workers: Vec<(String, u64, Arc<Engine>)> = this
                .parallel
                .iter()
                .map(|(id, worker)| (id.clone(), worker.id, worker.engine.clone()))
                .collect();
            (this.handler_ids(), mem::take(&mut this.waiters), workers, this.base.name().to_owned())
        };
        if !workers.is_empty() {
            match encode(lua, args.clone()) {
                Ok(payload) => {
                    for (_, worker, engine) in &workers {
                        engine.bus().send_to(*worker, Message::Call(payload.clone()));
                    }
                }
                Err(error) => {
                    let ids: Vec<&str> = workers.iter().map(|(id, _, _)| id.as_str()).collect();
                    scheduler.report(mlua::Error::runtime(format!(
                        "{name} could not send its values to the parallel handler {}: {error}",
                        ids.join(", ")
                    )));
                }
            }
        }
        for id in ids {
            let handler = signal.borrow::<Signal>()?.handler(&id);
            if let Some(handler) = handler {
                spawn(&scheduler, lua, handler, args.clone());
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
            if this.parallel.iter().any(|(bound, _)| bound == id) {
                return Err(mlua::Error::runtime(format!(
                    "handler '{id}' on {} runs on its own thread, so it cannot be invoked",
                    this.base.name()
                )));
            }
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
        self.parallel.clear();
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
        methods.add_method_mut("UnBind", |_, this, id: String| Ok(this.unbind_any(&id)));
        methods.add_function(
            "BindParallel",
            |lua, (signal, id, target): (AnyUserData, String, Value)| Signal::bind_parallel(lua, &signal, id, target),
        );
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
