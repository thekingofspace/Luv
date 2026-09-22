pub mod combine;
pub mod geometry;
pub mod gpu;
pub mod hook;
pub mod picture;
pub mod protocol;
pub mod reflect;
pub mod renderer;
pub mod shader;
pub mod text;

pub const PRELUDE: &str = include_str!("shaders/prelude.wgsl");
pub const QUAD_SHADER: &str = concat!(include_str!("shaders/prelude.wgsl"), "\n", include_str!("shaders/quad.wgsl"));
pub const POST_SHADER: &str = concat!(include_str!("shaders/prelude.wgsl"), "\n", include_str!("shaders/post.wgsl"));
pub const QUERY_SHADER: &str = concat!(include_str!("shaders/prelude.wgsl"), "\n", include_str!("shaders/query.wgsl"));
