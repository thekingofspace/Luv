use std::cell::{Cell, RefCell};
use std::mem;
use std::rc::Rc;
use std::time::Duration;

use mlua::{
    AnyUserData, Function, Lua, MetaMethod, MultiValue, Result, Table, UserData, UserDataFields, UserDataMethods, Value,
};

use super::joined;
use crate::runtime::{Scheduler, Waiter};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pending,
    Resolved,
    Rejected,
    Cancelled,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Resolved => "resolved",
            Status::Rejected => "rejected",
            Status::Cancelled => "cancelled",
        }
    }
}

enum Step {
    Then,
    Catch,
    Finally,
}

struct Link {
    step: Step,
    handler: Function,
    next: Shared,
}

type Hook = Box<dyn FnOnce(&Lua, Status, &MultiValue) -> Result<()>>;

struct State {
    status: Status,
    values: MultiValue,
    links: Vec<Link>,
    hooks: Vec<Hook>,
    waiters: Vec<Waiter>,
}

type Shared = Rc<RefCell<State>>;

fn blank() -> Shared {
    Rc::new(RefCell::new(State {
        status: Status::Pending,
        values: MultiValue::new(),
        links: Vec::new(),
        hooks: Vec::new(),
        waiters: Vec::new(),
    }))
}

fn outcome(state: &Shared) -> Option<(Status, MultiValue)> {
    let this = state.borrow();
    match this.status {
        Status::Pending => None,
        status => Some((status, this.values.clone())),
    }
}

fn message(error: &mlua::Error) -> String {
    match error {
        mlua::Error::RuntimeError(text) => text.clone(),
        mlua::Error::CallbackError { cause, .. } => message(cause),
        mlua::Error::WithContext { context, cause } => format!("{context}: {}", message(cause)),
        other => other.to_string(),
    }
}

fn failure(lua: &Lua, error: &mlua::Error) -> Result<MultiValue> {
    let text = lua.create_string(message(error))?;
    Ok(MultiValue::from_vec(vec![Value::String(text)]))
}

fn settle(lua: &Lua, state: &Shared, status: Status, values: MultiValue) -> Result<()> {
    let (links, hooks, waiters) = {
        let mut this = state.borrow_mut();
        if this.status != Status::Pending {
            return Ok(());
        }
        this.status = status;
        this.values = values.clone();
        (
            mem::take(&mut this.links),
            mem::take(&mut this.hooks),
            mem::take(&mut this.waiters),
        )
    };
    for waiter in waiters {
        waiter.wake(Ok(MultiValue::new()));
    }
    for hook in hooks {
        hook(lua, status, &values)?;
    }
    for link in links {
        follow(lua, link, status, &values)?;
    }
    Ok(())
}

fn run(
    lua: &Lua,
    handler: Function,
    args: MultiValue,
    then: impl FnOnce(&Lua, Result<MultiValue>) -> Result<()> + 'static,
) -> Result<()> {
    let cell = Rc::new(RefCell::new(Some(then)));
    let body = lua.create_async_function(move |lua, (handler, args): (Function, MultiValue)| {
        let cell = cell.clone();
        async move {
            let result = handler.call_async::<MultiValue>(args).await;
            let taken = cell.borrow_mut().take();
            match taken {
                Some(then) => then(&lua, result),
                None => Ok(()),
            }
        }
    })?;
    Scheduler::get(lua)?.start(lua, body, joined(Value::Function(handler), args), true)?;
    Ok(())
}

fn attach(
    lua: &Lua,
    state: &Shared,
    hook: impl FnOnce(&Lua, Status, &MultiValue) -> Result<()> + 'static,
) -> Result<()> {
    match outcome(state) {
        Some((status, values)) => hook(lua, status, &values),
        None => {
            state.borrow_mut().hooks.push(Box::new(hook));
            Ok(())
        }
    }
}

fn inner_state(values: &MultiValue) -> Option<Shared> {
    match values.front() {
        Some(Value::UserData(data)) => data.borrow::<Promise>().ok().map(|promise| promise.state.clone()),
        _ => None,
    }
}

fn adopt(lua: &Lua, next: &Shared, values: MultiValue) -> Result<()> {
    match inner_state(&values) {
        Some(inner) => {
            let target = next.clone();
            attach(lua, &inner, move |lua, status, values| {
                settle(lua, &target, status, values.clone())
            })
        }
        None => settle(lua, next, Status::Resolved, values),
    }
}

fn chain(lua: &Lua, handler: Function, args: MultiValue, next: Shared) -> Result<()> {
    run(lua, handler, args, move |lua, result| match result {
        Ok(values) => adopt(lua, &next, values),
        Err(error) => {
            let reason = failure(lua, &error)?;
            settle(lua, &next, Status::Rejected, reason)
        }
    })
}

fn follow(lua: &Lua, link: Link, status: Status, values: &MultiValue) -> Result<()> {
    let Link { step, handler, next } = link;
    match (step, status) {
        (Step::Then, Status::Resolved) | (Step::Catch, Status::Rejected) => chain(lua, handler, values.clone(), next),
        (Step::Finally, _) => {
            let carried = values.clone();
            let label = Value::String(lua.create_string(status.label())?);
            run(
                lua,
                handler,
                MultiValue::from_vec(vec![label]),
                move |lua, result| match result {
                    Ok(_) => settle(lua, &next, status, carried),
                    Err(error) => {
                        let reason = failure(lua, &error)?;
                        settle(lua, &next, Status::Rejected, reason)
                    }
                },
            )
        }
        _ => settle(lua, &next, status, values.clone()),
    }
}

fn link(lua: &Lua, state: &Shared, step: Step, handler: Function) -> Result<Promise> {
    let next = blank();
    let entry = Link {
        step,
        handler,
        next: next.clone(),
    };
    match outcome(state) {
        Some((status, values)) => follow(lua, entry, status, &values)?,
        None => state.borrow_mut().links.push(entry),
    }
    Ok(Promise::of(next))
}

pub struct Promise {
    state: Shared,
}

impl Promise {
    pub const CLASS_NAME: &'static str = "Promise";

    fn of(state: Shared) -> Self {
        Self { state }
    }

    fn shared(handle: &AnyUserData) -> Result<Shared> {
        Ok(handle.borrow::<Promise>()?.state.clone())
    }

    async fn settled(lua: &Lua, handle: &AnyUserData) -> Result<(Status, MultiValue)> {
        let state = Self::shared(handle)?;
        let wait = match outcome(&state) {
            Some(_) => None,
            None => {
                let (waiter, wait) = Scheduler::get(lua)?.waiter();
                state.borrow_mut().waiters.push(waiter);
                Some(wait)
            }
        };
        if let Some(wait) = wait {
            wait.wait().await?;
        }
        outcome(&state).ok_or_else(|| mlua::Error::runtime("the promise never settled"))
    }
}

impl UserData for Promise {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("ClassName", |_, _| Ok(Self::CLASS_NAME));
        fields.add_field_method_get("Status", |_, this| Ok(this.state.borrow().status.label()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Promise({})", this.state.borrow().status.label()))
        });
        methods.add_function("AndThen", |lua, (handle, handler): (AnyUserData, Function)| {
            let state = Promise::shared(&handle)?;
            link(lua, &state, Step::Then, handler)
        });
        methods.add_function("Catch", |lua, (handle, handler): (AnyUserData, Function)| {
            let state = Promise::shared(&handle)?;
            link(lua, &state, Step::Catch, handler)
        });
        methods.add_function("Finally", |lua, (handle, handler): (AnyUserData, Function)| {
            let state = Promise::shared(&handle)?;
            link(lua, &state, Step::Finally, handler)
        });
        methods.add_function("Cancel", |lua, handle: AnyUserData| {
            let state = Promise::shared(&handle)?;
            if outcome(&state).is_some() {
                return Ok(false);
            }
            settle(lua, &state, Status::Cancelled, MultiValue::new())?;
            Ok(true)
        });
        methods.add_async_function("Await", |lua, handle: AnyUserData| async move {
            match Promise::settled(&lua, &handle).await? {
                (Status::Resolved, values) => Ok(values),
                (Status::Cancelled, _) => Err(mlua::Error::runtime("the promise was cancelled")),
                (_, reason) => Err(mlua::Error::runtime(match reason.front() {
                    Some(Value::String(text)) => text.to_string_lossy(),
                    Some(other) => format!("the promise was rejected with a {}", other.type_name()),
                    None => "the promise was rejected".to_owned(),
                })),
            }
        });
        methods.add_async_function("AwaitStatus", |lua, handle: AnyUserData| async move {
            let (status, values) = Promise::settled(&lua, &handle).await?;
            let label = Value::String(lua.create_string(status.label())?);
            Ok(joined(label, values))
        });
    }
}

fn started(lua: &Lua, runner: &Function, body: Function, args: MultiValue) -> Result<AnyUserData> {
    let state = blank();
    let handle = lua.create_userdata(Promise::of(state.clone()))?;
    let target = state.clone();
    let resolve = lua.create_function(move |lua, values: MultiValue| settle(lua, &target, Status::Resolved, values))?;
    let target = state;
    let reject = lua.create_function(move |lua, values: MultiValue| settle(lua, &target, Status::Rejected, values))?;
    let mut all = MultiValue::with_capacity(args.len() + 4);
    all.push_back(Value::UserData(handle.clone()));
    all.push_back(Value::Function(body));
    all.push_back(Value::Function(resolve));
    all.push_back(Value::Function(reject));
    for value in args {
        all.push_back(value);
    }
    Scheduler::get(lua)?.start(lua, runner.clone(), all, true)?;
    Ok(handle)
}

fn called(lua: &Lua, body: Function, args: MultiValue) -> Result<Promise> {
    let state = blank();
    let target = state.clone();
    run(lua, body, args, move |lua, result| match result {
        Ok(values) => adopt(lua, &target, values),
        Err(error) => {
            let reason = failure(lua, &error)?;
            settle(lua, &target, Status::Rejected, reason)
        }
    })?;
    Ok(Promise::of(state))
}

type Entry = (usize, Option<Shared>, Value);

fn gather(list: &Table) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for (index, value) in list.sequence_values::<Value>().enumerate() {
        let value = value?;
        let state = match &value {
            Value::UserData(data) => data.borrow::<Promise>().ok().map(|promise| promise.state.clone()),
            _ => None,
        };
        entries.push((index + 1, state, value));
    }
    Ok(entries)
}

fn all(lua: &Lua, list: Table) -> Result<Promise> {
    let entries = gather(&list)?;
    let next = blank();
    let results = lua.create_table()?;
    let total = entries.len();
    let done = Rc::new(Cell::new(0usize));
    if total == 0 {
        let values = MultiValue::from_vec(vec![Value::Table(results)]);
        settle(lua, &next, Status::Resolved, values)?;
        return Ok(Promise::of(next));
    }
    for (index, state, value) in entries {
        match state {
            Some(state) => {
                let target = next.clone();
                let slot = results.clone();
                let done = done.clone();
                attach(lua, &state, move |lua, status, values| {
                    if status != Status::Resolved {
                        return settle(lua, &target, status, values.clone());
                    }
                    slot.raw_set(index, values.front().cloned().unwrap_or(Value::Nil))?;
                    done.set(done.get() + 1);
                    if done.get() < total {
                        return Ok(());
                    }
                    settle(lua, &target, Status::Resolved, MultiValue::from_vec(vec![Value::Table(slot)]))
                })?;
            }
            None => {
                results.raw_set(index, value)?;
                done.set(done.get() + 1);
                if done.get() == total {
                    let values = MultiValue::from_vec(vec![Value::Table(results.clone())]);
                    settle(lua, &next, Status::Resolved, values)?;
                }
            }
        }
    }
    Ok(Promise::of(next))
}

fn race(lua: &Lua, list: Table) -> Result<Promise> {
    let entries = gather(&list)?;
    if entries.is_empty() {
        return Err(mlua::Error::runtime("promise.race needs at least one promise"));
    }
    let next = blank();
    for (_, state, value) in entries {
        match state {
            Some(state) => {
                let target = next.clone();
                attach(lua, &state, move |lua, status, values| {
                    settle(lua, &target, status, values.clone())
                })?;
            }
            None => settle(lua, &next, Status::Resolved, MultiValue::from_vec(vec![value]))?,
        }
    }
    Ok(Promise::of(next))
}

fn runner(lua: &Lua) -> Result<Function> {
    lua.create_async_function(
        |lua, (handle, body, args): (AnyUserData, Function, MultiValue)| async move {
            match body.call_async::<MultiValue>(args).await {
                Ok(_) => Ok(()),
                Err(error) => {
                    let state = Promise::shared(&handle)?;
                    let reason = failure(&lua, &error)?;
                    settle(&lua, &state, Status::Rejected, reason)
                }
            }
        },
    )
}

fn sleeper(lua: &Lua) -> Result<Function> {
    lua.create_async_function(
        |lua, (handle, seconds, args): (AnyUserData, f64, MultiValue)| async move {
            let period = Duration::from_secs_f64(seconds.max(0.0));
            if period.is_zero() {
                tokio::task::yield_now().await;
            } else {
                tokio::time::sleep(period).await;
            }
            let state = Promise::shared(&handle)?;
            settle(&lua, &state, Status::Resolved, args)
        },
    )
}

pub fn install(lua: &Lua) -> Result<()> {
    let library = lua.create_table()?;
    let spawner = runner(lua)?;
    let sleeper = sleeper(lua)?;

    library.set(
        "new",
        lua.create_function(move |lua, (body, args): (Function, MultiValue)| started(lua, &spawner, body, args))?,
    )?;
    library.set(
        "call",
        lua.create_function(|lua, (body, args): (Function, MultiValue)| called(lua, body, args))?,
    )?;
    library.set(
        "resolve",
        lua.create_function(|lua, values: MultiValue| {
            let state = blank();
            settle(lua, &state, Status::Resolved, values)?;
            Ok(Promise::of(state))
        })?,
    )?;
    library.set(
        "reject",
        lua.create_function(|lua, values: MultiValue| {
            let state = blank();
            settle(lua, &state, Status::Rejected, values)?;
            Ok(Promise::of(state))
        })?,
    )?;
    library.set("all", lua.create_function(|lua, list: Table| all(lua, list))?)?;
    library.set("race", lua.create_function(|lua, list: Table| race(lua, list))?)?;
    library.set(
        "delay",
        lua.create_function(move |lua, (seconds, args): (f64, MultiValue)| {
            let handle = lua.create_userdata(Promise::of(blank()))?;
            let mut all = MultiValue::with_capacity(args.len() + 2);
            all.push_back(Value::UserData(handle.clone()));
            all.push_back(Value::Number(seconds));
            for value in args {
                all.push_back(value);
            }
            Scheduler::get(lua)?.start(lua, sleeper.clone(), all, true)?;
            Ok(handle)
        })?,
    )?;
    library.set(
        "is",
        lua.create_function(|_, value: Value| {
            Ok(match value {
                Value::UserData(data) => data.is::<Promise>(),
                _ => false,
            })
        })?,
    )?;
    lua.globals().set("promise", library)
}
