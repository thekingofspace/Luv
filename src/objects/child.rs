use std::sync::Arc;

use mlua::{AnyUserData, Lua, Result, Table, UserData, UserDataFields, UserDataMethods};
use tokio::sync::{Notify, watch};

use super::file::{File, Handle};
use super::{BaseGameObject, GameObject};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitStatus {
    pub ok: bool,
    pub code: i32,
}

impl ExitStatus {
    pub fn from_std(status: std::io::Result<std::process::ExitStatus>) -> Self {
        match status {
            Ok(status) => Self {
                ok: status.success(),
                code: status.code().unwrap_or(-1),
            },
            Err(_) => Self { ok: false, code: -1 },
        }
    }

    pub fn to_table(self, lua: &Lua) -> Result<Table> {
        let table = lua.create_table()?;
        table.set("ok", self.ok)?;
        table.set("code", self.code)?;
        Ok(table)
    }
}

pub struct Child {
    base: BaseGameObject,
    pid: Option<u32>,
    stdin: Option<AnyUserData>,
    stdout: Option<AnyUserData>,
    stderr: Option<AnyUserData>,
    status: watch::Receiver<Option<ExitStatus>>,
    kill: Arc<Notify>,
}

impl Child {
    pub const CLASS_NAME: &'static str = "Child";

    pub fn spawn(lua: &Lua, name: &str, mut process: tokio::process::Child) -> Result<Self> {
        let pipe = |label: &str, handle: Handle| lua.create_userdata(File::new(format!("{label} of {name}"), handle, true));
        let stdin = process.stdin.take().map(|pipe_| pipe("stdin", Handle::ChildStdin(pipe_))).transpose()?;
        let stdout = process.stdout.take().map(|pipe_| pipe("stdout", Handle::ChildStdout(pipe_))).transpose()?;
        let stderr = process.stderr.take().map(|pipe_| pipe("stderr", Handle::ChildStderr(pipe_))).transpose()?;

        let pid = process.id();
        let (sender, status) = watch::channel(None);
        let kill = Arc::new(Notify::new());
        let killed = kill.clone();
        tokio::spawn(async move {
            let exited = tokio::select! {
                status = process.wait() => Some(status),
                () = killed.notified() => None,
            };
            let status = match exited {
                Some(status) => status,
                None => {
                    let _ = process.start_kill();
                    process.wait().await
                }
            };
            sender.send_replace(Some(ExitStatus::from_std(status)));
        });

        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(name);
        Ok(Self {
            base,
            pid,
            stdin,
            stdout,
            stderr,
            status,
            kill,
        })
    }

    pub fn kill(&self) {
        self.kill.notify_one();
    }

    pub async fn wait(child: &AnyUserData) -> Result<ExitStatus> {
        let mut status = child.borrow::<Child>()?.status.clone();
        let exited = status
            .wait_for(Option::is_some)
            .await
            .map_err(|_| mlua::Error::runtime("the process status is no longer available"))?;
        (*exited).ok_or_else(|| mlua::Error::runtime("the process status is no longer available"))
    }
}

impl GameObject for Child {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.kill();
    }
}

impl UserData for Child {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Pid", |_, this| Ok(this.pid));
        fields.add_field_method_get("Stdin", |_, this| Ok(this.stdin.clone()));
        fields.add_field_method_get("Stdout", |_, this| Ok(this.stdout.clone()));
        fields.add_field_method_get("Stderr", |_, this| Ok(this.stderr.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("Kill", |_, this, ()| {
            this.kill();
            Ok(())
        });
        methods.add_async_function("Wait", |lua, child: AnyUserData| async move {
            Child::wait(&child).await?.to_table(&lua)
        });
    }
}
