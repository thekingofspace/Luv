use std::os::raw::{c_char, c_void};

pub const VERSION: u32 = 1;
pub const OK: i32 = 0;
pub const UNKNOWN_NAME: i32 = -1;
pub const OUT_OF_RANGE: i32 = -2;
pub const WRONG_KIND: i32 = -3;
pub const INVALID: i32 = -4;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VulkanHandles {
    pub instance: *mut c_void,
    pub physical_device: *mut c_void,
    pub device: *mut c_void,
    pub queue: *mut c_void,
    pub queue_family: u32,
    pub queue_index: u32,
    pub get_instance_proc_addr: *const c_void,
}

unsafe impl Send for VulkanHandles {}
unsafe impl Sync for VulkanHandles {}

pub type WriteData = unsafe extern "C" fn(*mut RenderContext, *const c_char, u64, *const c_void, u64) -> i32;
pub type WriteTexture = unsafe extern "C" fn(*mut RenderContext, *const c_char, u32, u32, *const c_void) -> i32;
pub type SetDrawCounts = unsafe extern "C" fn(*mut RenderContext, u32, u32);
pub type HookFunction = unsafe extern "C" fn(*mut RenderContext);

#[repr(C)]
pub struct RenderContext {
    pub version: u32,
    pub struct_size: u32,
    pub user_data: *mut c_void,
    pub renderable: u64,
    pub time: f64,
    pub delta: f64,
    pub frame: u32,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub anchor: [f32; 2],
    pub rotation: f32,
    pub write_data: WriteData,
    pub write_texture: WriteTexture,
    pub set_draw_counts: SetDrawCounts,
    pub vulkan: *const VulkanHandles,
    pub engine: *mut c_void,
}
