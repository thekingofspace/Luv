use std::mem;
use std::sync::Arc;

use mlua::{AnyUserData, Function, Lua, MultiValue, Result, UserData, UserDataFields, UserDataMethods};

use super::{BaseGameObject, GameObject};
use crate::runtime::{Bus, Packet, Scheduler, Waiter, decode, encode};

struct Subscription {
    id: u64,
    topic: String,
    handler: Function,
}

pub struct Messenger {
    base: BaseGameObject,
    bus: Arc<Bus>,
    next_id: u64,
    subscriptions: Vec<Subscription>,
    waiters: Vec<(String, Waiter)>,
}

impl Messenger {
    pub const CLASS_NAME: &'static str = "Messenger";

    pub fn new(bus: Arc<Bus>) -> Self {
        Self {
            base: BaseGameObject::new(Self::CLASS_NAME),
            bus,
            next_id: 0,
            subscriptions: Vec::new(),
            waiters: Vec::new(),
        }
    }

    pub fn subscribe(&mut self, topic: impl Into<String>, handler: Function) -> Result<u64> {
        self.base.ensure_alive()?;
        self.next_id += 1;
        self.subscriptions.push(Subscription {
            id: self.next_id,
            topic: topic.into(),
            handler,
        });
        Ok(self.next_id)
    }

    pub fn unsubscribe(&mut self, id: u64) -> bool {
        let before = self.subscriptions.len();
        self.subscriptions.retain(|subscription| subscription.id != id);
        self.subscriptions.len() != before
    }

    pub fn is_listening(&self) -> bool {
        !self.subscriptions.is_empty() || !self.waiters.is_empty()
    }

    pub fn publish(&self, lua: &Lua, topic: &str, args: MultiValue) -> Result<()> {
        self.base.ensure_alive()?;
        let payload = encode(lua, args)
            .map_err(|error| mlua::Error::runtime(format!("cannot fire '{topic}': {error}")))?;
        self.bus.publish(topic, payload);
        Ok(())
    }

    pub async fn wait(lua: &Lua, messenger: &AnyUserData, topic: String) -> Result<MultiValue> {
        let (waiter, wait) = Scheduler::get(lua)?.waiter();
        {
            let mut this = messenger.borrow_mut::<Messenger>()?;
            this.base.ensure_alive()?;
            this.waiters.push((topic, waiter));
        }
        wait.wait().await
    }

    pub fn deliver(lua: &Lua, messenger: &AnyUserData, topic: &str, payload: &[Packet]) -> Result<()> {
        let (handlers, waiters) = {
            let mut this = messenger.borrow_mut::<Messenger>()?;
            let handlers: Vec<Function> = this
                .subscriptions
                .iter()
                .filter(|subscription| subscription.topic == topic)
                .map(|subscription| subscription.handler.clone())
                .collect();
            let (waiters, rest): (Vec<_>, Vec<_>) =
                mem::take(&mut this.waiters).into_iter().partition(|(waiting, _)| waiting == topic);
            this.waiters = rest;
            (handlers, waiters)
        };
        if handlers.is_empty() && waiters.is_empty() {
            return Ok(());
        }

        match decode(lua, payload) {
            Ok(values) => {
                for (_, waiter) in waiters {
                    waiter.wake(Ok(values.clone()));
                }
                let scheduler = Scheduler::get(lua)?;
                for handler in handlers {
                    scheduler.spawn(lua, handler, values.clone());
                }
                Ok(())
            }
            Err(error) => {
                for (_, waiter) in waiters {
                    waiter.wake(Err(error.clone()));
                }
                Err(error)
            }
        }
    }
}

impl GameObject for Messenger {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.subscriptions.clear();
        for (topic, waiter) in self.waiters.drain(..) {
            waiter.wake(Err(mlua::Error::runtime(format!(
                "Messenger was destroyed while waiting for '{topic}'"
            ))));
        }
    }
}

impl UserData for Messenger {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);

        methods.add_method_mut("Subscribe", |_, this, (topic, handler): (String, Function)| {
            this.subscribe(topic, handler)
        });
        methods.add_method_mut("Unsubscribe", |_, this, id: u64| Ok(this.unsubscribe(id)));
        methods.add_method("Fire", |lua, this, (topic, args): (String, MultiValue)| {
            this.publish(lua, &topic, args)
        });
        methods.add_async_function(
            "Wait",
            |lua, (messenger, topic): (AnyUserData, String)| async move {
                Messenger::wait(&lua, &messenger, topic).await
            },
        );
    }
}
