#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod api;
pub mod audio;
pub mod builder;
pub mod datatypes;
pub mod graphics;
pub mod luaurc;
pub mod native;
pub mod objects;
pub mod packager;
pub mod plugins;
pub mod project;
pub mod runtime;
pub mod script;
pub mod typegen;
pub mod vfs;
pub mod window;
