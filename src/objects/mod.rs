mod asset;
mod child;
pub(crate) mod external;
pub mod file;
mod input;
mod messenger;
mod net;
mod renderable;
mod shader;
mod signal;
mod sound;
mod window;

pub use asset::Asset;
pub use child::{Child, ExitStatus};
pub use external::External;
pub use file::File;
pub use messenger::Messenger;
pub use net::{TcpServer, TcpSocket, UdpSocket, WebSocket};
pub use renderable::{Kind as RenderableKind, Renderable, Scene};
pub use shader::{Shader, ShaderCombo};
pub use signal::Signal;
pub use sound::{AudioPacket, Port, SoundObject, Sounds};
pub use window::{Window, WindowHost};

use mlua::{MetaMethod, UserData, UserDataFields, UserDataMethods};

pub struct BaseGameObject {
    class_name: &'static str,
    name: String,
    destroyed: bool,
}

impl BaseGameObject {
    pub fn new(class_name: &'static str) -> Self {
        Self {
            class_name,
            name: class_name.to_owned(),
            destroyed: false,
        }
    }

    pub fn class_name(&self) -> &'static str {
        self.class_name
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn is_destroyed(&self) -> bool {
        self.destroyed
    }

    pub fn ensure_alive(&self) -> mlua::Result<()> {
        if self.destroyed {
            return Err(mlua::Error::runtime(format!(
                "{} '{}' has been destroyed",
                self.class_name, self.name
            )));
        }
        Ok(())
    }
}

pub trait GameObject: UserData + Sized + 'static {
    fn base(&self) -> &BaseGameObject;

    fn base_mut(&mut self) -> &mut BaseGameObject;

    fn on_destroy(&mut self) {}

    fn destroy(&mut self) {
        let base = self.base_mut();
        if !base.destroyed {
            base.destroyed = true;
            self.on_destroy();
        }
    }

    fn add_base_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("ClassName", |_, this| Ok(this.base().class_name()));
        fields.add_field_method_get("Name", |_, this| Ok(this.base().name().to_owned()));
        fields.add_field_method_set("Name", |_, this, name: String| {
            this.base_mut().set_name(name);
            Ok(())
        });
    }

    fn add_base_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("Destroy", |_, this, ()| {
            this.destroy();
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(this.base().name().to_owned()));
    }
}

impl GameObject for BaseGameObject {
    fn base(&self) -> &BaseGameObject {
        self
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        self
    }
}

impl UserData for BaseGameObject {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
    }
}
