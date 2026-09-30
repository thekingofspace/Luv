use std::cell::RefCell;
use std::mem;
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{
    AnyUserData, Function, Lua, MetaMethod, MultiValue, Result, Thread, UserData, UserDataFields, UserDataMethods, Value,
};

use crate::runtime::{Scheduler, Waiter};

fn period(seconds: Option<f64>) -> Duration {
    Duration::from_secs_f64(seconds.unwrap_or(0.0).max(0.0))
}

async fn rest(period: Duration) {
    if period.is_zero() {
        tokio::task::yield_now().await;
    } else {
        tokio::time::sleep(period).await;
    }
}

fn coroutine(lua: &Lua, scheduler: &Scheduler, name: &str, target: Value) -> Result<Thread> {
    match target {
        Value::Function(function) => lua.create_thread(function),
        Value::Thread(thread) => match scheduler.is_driving(&thread) {
            true => Err(mlua::Error::runtime(format!(
                "{name} was given a coroutine that luv is already running"
            ))),
            false => Ok(thread),
        },
        other => Err(mlua::Error::runtime(format!(
            "{name} needs a function or a coroutine, got a {}",
            other.type_name()
        ))),
    }
}

fn launch(lua: &Lua, name: &str, target: Value, args: MultiValue, immediate: bool) -> Result<Thread> {
    let scheduler = Scheduler::get(lua)?;
    let thread = coroutine(lua, &scheduler, name, target)?;
    scheduler.resume(thread.clone(), args, immediate);
    Ok(thread)
}

struct Runs {
    live: usize,
    waiters: Vec<Waiter>,
}

impl Runs {
    fn finish(&mut self) -> Vec<Waiter> {
        self.live -= 1;
        match self.live {
            0 => mem::take(&mut self.waiters),
            _ => Vec::new(),
        }
    }
}

pub struct Task {
    body: Function,
    exclusive: bool,
    runs: Rc<RefCell<Runs>>,
}

impl Task {
    pub const CLASS_NAME: &'static str = "Task";

    fn new(body: Function, exclusive: bool) -> Self {
        Self {
            body,
            exclusive,
            runs: Rc::new(RefCell::new(Runs {
                live: 0,
                waiters: Vec::new(),
            })),
        }
    }

    fn call(lua: &Lua, handle: &AnyUserData, args: MultiValue) -> Result<Value> {
        let (body, exclusive, runs) = {
            let this = handle.borrow::<Task>()?;
            (this.body.clone(), this.exclusive, this.runs.clone())
        };
        if exclusive && runs.borrow().live > 0 {
            return Ok(Value::Nil);
        }
        runs.borrow_mut().live += 1;
        let done = runs.clone();
        let thread = Scheduler::get(lua)?.start_then(lua, body, args, true, move |_| {
            for waiter in done.borrow_mut().finish() {
                waiter.wake(Ok(MultiValue::new()));
            }
        })?;
        Ok(Value::Thread(thread))
    }
}

impl UserData for Task {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("ClassName", |_, _| Ok(Self::CLASS_NAME));
        fields.add_field_method_get("Exclusive", |_, this| Ok(this.exclusive));
        fields.add_field_method_get("Running", |_, this| Ok(this.runs.borrow().live));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Task({} running)", this.runs.borrow().live))
        });
        methods.add_meta_function(MetaMethod::Call, |lua, (handle, args): (AnyUserData, MultiValue)| {
            Task::call(lua, &handle, args)
        });
        methods.add_function("Run", |lua, (handle, args): (AnyUserData, MultiValue)| {
            Task::call(lua, &handle, args)
        });
        methods.add_async_function("Wait", |lua, handle: AnyUserData| async move {
            let runs = handle.borrow::<Task>()?.runs.clone();
            let live = runs.borrow().live;
            let wait = match live {
                0 => None,
                _ => {
                    let (waiter, wait) = Scheduler::get(&lua)?.waiter();
                    runs.borrow_mut().waiters.push(waiter);
                    Some(wait)
                }
            };
            if let Some(wait) = wait {
                wait.wait().await?;
            }
            Ok(())
        });
    }
}

pub fn install(lua: &Lua) -> Result<()> {
    let library = lua.create_table()?;

    library.set(
        "wait",
        lua.create_async_function(|_, seconds: Option<f64>| async move {
            let started = Instant::now();
            rest(period(seconds)).await;
            Ok(started.elapsed().as_secs_f64())
        })?,
    )?;
    library.set(
        "spawn",
        lua.create_function(|lua, (target, args): (Value, MultiValue)| {
            launch(lua, "task.spawn", target, args, true)
        })?,
    )?;
    library.set(
        "defer",
        lua.create_function(|lua, (target, args): (Value, MultiValue)| {
            launch(lua, "task.defer", target, args, false)
        })?,
    )?;
    library.set(
        "delay",
        lua.create_function(|lua, (seconds, target, args): (f64, Value, MultiValue)| {
            let scheduler = Scheduler::get(lua)?;
            let thread = coroutine(lua, &scheduler, "task.delay", target)?;
            let waiting = thread.clone();
            let later = scheduler.clone();
            let wait = period(Some(seconds));
            scheduler.spawn_task(async move {
                rest(wait).await;
                later.resume(waiting, args, true);
            });
            Ok(thread)
        })?,
    )?;
    library.set(
        "create",
        lua.create_function(|_, (body, exclusive): (Function, Option<bool>)| {
            Ok(Task::new(body, exclusive.unwrap_or(false)))
        })?,
    )?;
    library.set(
        "count",
        lua.create_function(|lua, ()| Ok(Scheduler::get(lua)?.driving()))?,
    )?;
    library.set(
        "parallel",
        lua.create_function(|lua, (target, args): (Value, MultiValue)| {
            super::parallel::spawn_once(lua, &target, args)
        })?,
    )?;
    let marker = lua.create_function(|_, ()| Ok(()))?;
    library.set("desynchronize", marker.clone())?;
    library.set("synchronize", marker)?;
    lua.globals().set("task", library)
}
