use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

use futures_core::Stream;
use mlua::thread::AsyncThread;
use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Result, Thread, Value};
use tokio::sync::{Notify, oneshot, watch};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Running,
    Closing,
    Finished,
}

type IdleHook = Box<dyn Fn() + Send + Sync>;

pub struct Activity {
    count: AtomicUsize,
    closing: AtomicUsize,
    phase: watch::Sender<Phase>,
    idle: OnceLock<IdleHook>,
}

impl Activity {
    pub fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            closing: AtomicUsize::new(0),
            phase: watch::Sender::new(Phase::Running),
            idle: OnceLock::new(),
        }
    }

    pub fn on_idle(&self, hook: impl Fn() + Send + Sync + 'static) {
        let _ = self.idle.set(Box::new(hook));
    }

    pub fn enter(&self, amount: usize) {
        self.count.fetch_add(amount, Ordering::SeqCst);
    }

    pub fn exit(&self) {
        if self.count.fetch_sub(1, Ordering::SeqCst) == 1 && self.phase() == Phase::Running {
            match self.idle.get() {
                Some(hook) => hook(),
                None => self.stop(),
            }
        }
    }

    pub fn phase(&self) -> Phase {
        *self.phase.borrow()
    }

    pub fn begin_closing(&self) -> bool {
        self.phase.send_if_modified(|phase| {
            let starting = *phase == Phase::Running;
            if starting {
                *phase = Phase::Closing;
            }
            starting
        })
    }

    pub fn closing_enter(&self, amount: usize) {
        self.closing.fetch_add(amount, Ordering::SeqCst);
    }

    pub fn closing_exit(&self) {
        if self.closing.fetch_sub(1, Ordering::SeqCst) == 1 && self.phase() == Phase::Closing {
            self.stop();
        }
    }

    pub fn stop(&self) {
        self.phase.send_replace(Phase::Finished);
    }

    pub fn is_closing(&self) -> bool {
        self.phase() >= Phase::Closing
    }

    pub fn is_finished(&self) -> bool {
        self.phase() == Phase::Finished
    }

    pub async fn closing(&self) {
        let mut receiver = self.phase.subscribe();
        let _ = receiver.wait_for(|phase| *phase != Phase::Running).await;
    }

    pub async fn finished(&self) {
        let mut receiver = self.phase.subscribe();
        let _ = receiver.wait_for(|phase| *phase == Phase::Finished).await;
    }
}

impl Default for Activity {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct Tracker {
    local: Rc<Cell<usize>>,
    idle: Rc<Notify>,
    activity: Arc<Activity>,
}

impl Tracker {
    pub fn new(activity: Arc<Activity>) -> Self {
        Self {
            local: Rc::new(Cell::new(0)),
            idle: Rc::new(Notify::new()),
            activity,
        }
    }

    pub fn enter(&self) {
        self.local.set(self.local.get() + 1);
        self.activity.enter(1);
    }

    pub fn adopt(&self) {
        self.local.set(self.local.get() + 1);
    }

    pub fn exit(&self) {
        let remaining = self.local.get() - 1;
        self.local.set(remaining);
        if remaining == 0 {
            self.idle.notify_one();
        }
        self.activity.exit();
    }

    pub fn active(&self) -> usize {
        self.local.get()
    }

    pub async fn idle(&self) {
        self.idle.notified().await;
    }
}

pub struct Waiter {
    sender: oneshot::Sender<Result<MultiValue>>,
    tracker: Tracker,
}

impl Waiter {
    pub fn wake(self, result: Result<MultiValue>) {
        self.tracker.enter();
        if self.sender.send(result).is_err() {
            self.tracker.exit();
        }
    }
}

pub struct Wait {
    receiver: oneshot::Receiver<Result<MultiValue>>,
    tracker: Tracker,
}

impl Wait {
    pub async fn wait(self) -> Result<MultiValue> {
        self.tracker.exit();
        match self.receiver.await {
            Ok(result) => result,
            Err(_) => {
                self.tracker.enter();
                Err(mlua::Error::runtime("the wait was cancelled"))
            }
        }
    }
}

type Reporter = Rc<dyn Fn(mlua::Error)>;

#[derive(Clone)]
pub struct Scheduler {
    tracker: Tracker,
    reporter: Reporter,
    driving: Rc<RefCell<Vec<Thread>>>,
}

impl Scheduler {
    pub fn new(tracker: Tracker, reporter: impl Fn(mlua::Error) + 'static) -> Self {
        Self {
            tracker,
            reporter: Rc::new(reporter),
            driving: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn get(lua: &Lua) -> Result<Scheduler> {
        lua.app_data_ref::<Scheduler>()
            .map(|scheduler| scheduler.clone())
            .ok_or_else(|| mlua::Error::runtime("the luv scheduler is not running"))
    }

    pub fn tracker(&self) -> &Tracker {
        &self.tracker
    }

    pub fn report(&self, error: mlua::Error) {
        (self.reporter)(error);
    }

    pub fn waiter(&self) -> (Waiter, Wait) {
        let (sender, receiver) = oneshot::channel();
        (
            Waiter {
                sender,
                tracker: self.tracker.clone(),
            },
            Wait {
                receiver,
                tracker: self.tracker.clone(),
            },
        )
    }

    pub fn is_driving(&self, thread: &Thread) -> bool {
        self.driving.borrow().contains(thread)
    }

    pub fn spawn(&self, lua: &Lua, function: Function, args: impl IntoLuaMulti) {
        self.spawn_then(lua, function, args, || {});
    }

    pub fn spawn_then(&self, lua: &Lua, function: Function, args: impl IntoLuaMulti, then: impl FnOnce() + 'static) {
        self.spawn_returning(lua, function, args, move |_| then());
    }

    pub fn spawn_returning(
        &self,
        lua: &Lua,
        function: Function,
        args: impl IntoLuaMulti,
        then: impl FnOnce(Option<MultiValue>) + 'static,
    ) {
        self.tracker.enter();
        self.launch(lua, function, args, then);
    }

    pub fn spawn_entered(&self, lua: &Lua, function: Function, args: impl IntoLuaMulti) {
        self.launch(lua, function, args, |_| {});
    }

    fn launch(
        &self,
        lua: &Lua,
        function: Function,
        args: impl IntoLuaMulti,
        then: impl FnOnce(Option<MultiValue>) + 'static,
    ) {
        match lua.create_thread(function) {
            Ok(thread) => self.drive(thread, args, true, then),
            Err(error) => self.settle(Some(Err(error)), then),
        }
    }

    pub fn spawn_task(&self, task: impl Future<Output = ()> + 'static) {
        self.tracker.enter();
        let tracker = self.tracker.clone();
        tokio::task::spawn_local(async move {
            task.await;
            tracker.exit();
        });
    }

    pub fn adopt(&self, thread: Thread) {
        self.tracker.enter();
        self.drive(thread, (), false, |_| {});
    }

    fn drive(
        &self,
        thread: Thread,
        args: impl IntoLuaMulti,
        immediate: bool,
        then: impl FnOnce(Option<MultiValue>) + 'static,
    ) {
        let mut stream = match thread.clone().into_async::<MultiValue>(args) {
            Ok(stream) => Box::pin(stream),
            Err(error) => return self.settle(Some(Err(error)), then),
        };

        if immediate {
            let mut context = Context::from_waker(Waker::noop());
            if let Poll::Ready(result) = self.step(stream.as_mut(), &mut context) {
                return self.settle(result, then);
            }
        }

        self.driving.borrow_mut().push(thread.clone());
        let scheduler = self.clone();
        tokio::task::spawn_local(async move {
            let result = poll_fn(|context| scheduler.step(stream.as_mut(), context)).await;
            scheduler.driving.borrow_mut().retain(|driven| *driven != thread);
            scheduler.settle(result, then);
        });
    }

    fn step(&self, stream: Pin<&mut AsyncThread<MultiValue>>, context: &mut Context<'_>) -> Poll<Option<Result<MultiValue>>> {
        self.tracker.enter();
        let result = stream.poll_next(context);
        self.tracker.exit();
        result
    }

    fn settle(&self, result: Option<Result<MultiValue>>, then: impl FnOnce(Option<MultiValue>)) {
        let values = match result {
            Some(Ok(values)) => Some(values),
            Some(Err(error)) => {
                self.report(error);
                None
            }
            None => None,
        };
        self.tracker.exit();
        then(values);
    }
}

pub fn install_coroutine_library(lua: &Lua) -> Result<()> {
    let coroutine: mlua::Table = lua.globals().get("coroutine")?;
    let adopt = lua.create_function(|lua, thread: Thread| {
        Scheduler::get(lua)?.adopt(thread);
        Ok(())
    })?;
    let driving = lua.create_function(|lua, thread: Thread| Ok(Scheduler::get(lua)?.is_driving(&thread)))?;

    let (resume, wrap): (Function, Function) = lua
        .load(
            r#"
            local create, nativeResume, pending, adopt, driving = ...
            local pack, unpack = table.pack, table.unpack

            local function resume(co, ...)
                if driving(co) then
                    return false, "cannot resume a coroutine that is waiting on the engine"
                end
                local results = pack(nativeResume(co, ...))
                if results[1] and results.n == 2 and results[2] == pending then
                    adopt(co)
                    return true
                end
                return unpack(results, 1, results.n)
            end

            local function wrap(f)
                local co = create(f)
                return function(...)
                    local results = pack(resume(co, ...))
                    if not results[1] then
                        error(results[2], 0)
                    end
                    return unpack(results, 2, results.n)
                end
            end

            return resume, wrap
            "#,
        )
        .set_name("=luv.coroutine")
        .call((
            coroutine.get::<Function>("create")?,
            coroutine.get::<Function>("resume")?,
            Value::LightUserData(Lua::poll_pending()),
            adopt,
            driving,
        ))?;

    coroutine.set("resume", resume)?;
    coroutine.set("wrap", wrap)?;
    Ok(())
}
