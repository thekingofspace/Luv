#![allow(dead_code)]

use std::ffi::{CStr, CString, c_char, c_void};

pub const LUV_RENDER_VERSION: u32 = 1;

pub const LUV_OK: i32 = 0;
pub const LUV_UNKNOWN_NAME: i32 = -1;
pub const LUV_OUT_OF_RANGE: i32 = -2;
pub const LUV_WRONG_KIND: i32 = -3;
pub const LUV_INVALID: i32 = -4;

pub type LuvGetInstanceProcAddr = unsafe extern "system" fn(*mut c_void, *const c_char) -> *mut c_void;
pub type LuvWriteData = unsafe extern "C" fn(*mut LuvRenderContext, *const c_char, u64, *const c_void, u64) -> i32;
pub type LuvWriteTexture = unsafe extern "C" fn(*mut LuvRenderContext, *const c_char, u32, u32, *const c_void) -> i32;
pub type LuvSetDrawCounts = unsafe extern "C" fn(*mut LuvRenderContext, u32, u32);
pub type LuvRenderHook = unsafe extern "C" fn(*mut LuvRenderContext);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LuvVulkan {
    pub instance: *mut c_void,
    pub physical_device: *mut c_void,
    pub device: *mut c_void,
    pub queue: *mut c_void,
    pub queue_family: u32,
    pub queue_index: u32,
    pub get_instance_proc_addr: Option<LuvGetInstanceProcAddr>,
}

#[repr(C)]
pub struct LuvRenderContext {
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
    pub write_data: LuvWriteData,
    pub write_texture: LuvWriteTexture,
    pub set_draw_counts: LuvSetDrawCounts,
    pub vulkan: *const LuvVulkan,
    pub engine: *mut c_void,
}

impl LuvRenderContext {
    pub fn user_data<T>(&self) -> Option<&T> {
        unsafe { (self.user_data as *const T).as_ref() }
    }

    pub fn write_data<T: Copy>(&mut self, name: &CStr, offset: u64, data: &[T]) -> i32 {
        let write = self.write_data;
        unsafe { write(self, name.as_ptr(), offset, data.as_ptr().cast(), size_of_val(data) as u64) }
    }

    pub fn write_texture(&mut self, name: &CStr, width: u32, height: u32, rgba: &[u8]) -> i32 {
        if rgba.len() < width as usize * height as usize * 4 {
            return LUV_INVALID;
        }
        let write = self.write_texture;
        unsafe { write(self, name.as_ptr(), width, height, rgba.as_ptr().cast()) }
    }

    pub fn set_draw_counts(&mut self, vertex_count: u32, instance_count: u32) {
        let set = self.set_draw_counts;
        unsafe { set(self, vertex_count, instance_count) }
    }

    pub fn vulkan(&self) -> Option<&LuvVulkan> {
        unsafe { self.vulkan.as_ref() }
    }
}

pub const LUV_API_VERSION: u32 = 1;

pub const LUV_WORKER: u32 = 0;
pub const LUV_INLINE: u32 = 1;
pub const LUV_PARALLEL: u32 = 2;

pub const LUV_KIND_NONE: i32 = -1;
pub const LUV_KIND_NIL: i32 = 0;
pub const LUV_KIND_BOOLEAN: i32 = 1;
pub const LUV_KIND_NUMBER: i32 = 2;
pub const LUV_KIND_STRING: i32 = 3;
pub const LUV_KIND_UDIM: i32 = 4;
pub const LUV_KIND_COLOR: i32 = 5;
pub const LUV_KIND_OBJECT: i32 = 6;
pub const LUV_KIND_POINTER: i32 = 7;
pub const LUV_KIND_VALUE: i32 = 8;

#[repr(C)]
pub struct LuvCall {
    _private: [u8; 0],
}

#[repr(C)]
pub struct LuvClass {
    _private: [u8; 0],
}

#[repr(C)]
pub struct LuvRegistry {
    _private: [u8; 0],
}

#[repr(C)]
pub struct LuvRef {
    _private: [u8; 0],
}

pub type LuvFunction = unsafe extern "C" fn(*mut LuvCall);
pub type LuvDestroy = unsafe extern "C" fn(*mut c_void);
pub type LuvRegister = unsafe extern "C" fn(*const LuvApi, *mut LuvRegistry) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LuvMethod {
    pub name: *const c_char,
    pub function: Option<LuvFunction>,
    pub flags: u32,
}

unsafe impl Sync for LuvMethod {}

impl LuvMethod {
    pub const END: LuvMethod = LuvMethod {
        name: std::ptr::null(),
        function: None,
        flags: 0,
    };

    pub const fn new(name: &'static CStr, function: LuvFunction, flags: u32) -> LuvMethod {
        LuvMethod {
            name: name.as_ptr(),
            function: Some(function),
            flags,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LuvProperty {
    pub name: *const c_char,
    pub get: Option<LuvFunction>,
    pub set: Option<LuvFunction>,
}

unsafe impl Sync for LuvProperty {}

impl LuvProperty {
    pub const END: LuvProperty = LuvProperty {
        name: std::ptr::null(),
        get: None,
        set: None,
    };

    pub const fn new(name: &'static CStr, get: Option<LuvFunction>, set: Option<LuvFunction>) -> LuvProperty {
        LuvProperty {
            name: name.as_ptr(),
            get,
            set,
        }
    }
}

#[repr(C)]
pub struct LuvClassInfo {
    pub name: *const c_char,
    pub size: u64,
    pub destroy: Option<LuvDestroy>,
    pub methods: *const LuvMethod,
    pub properties: *const LuvProperty,
    pub statics: *const LuvMethod,
    pub static_properties: *const LuvProperty,
}

#[repr(C)]
pub struct LuvApi {
    pub version: u32,
    pub struct_size: u32,
    pub define_class: unsafe extern "C" fn(*mut LuvRegistry, *const LuvClassInfo) -> *const LuvClass,
    pub define_function: unsafe extern "C" fn(*mut LuvRegistry, *const LuvMethod) -> i32,
    pub find_class: unsafe extern "C" fn(*const c_char) -> *const LuvClass,
    pub class_name: unsafe extern "C" fn(*const LuvClass) -> *const c_char,
    pub arg_count: unsafe extern "C" fn(*mut LuvCall) -> i32,
    pub arg_kind: unsafe extern "C" fn(*mut LuvCall, i32) -> i32,
    pub arg_class: unsafe extern "C" fn(*mut LuvCall, i32) -> *const LuvClass,
    pub check_boolean: unsafe extern "C" fn(*mut LuvCall, i32) -> i32,
    pub opt_boolean: unsafe extern "C" fn(*mut LuvCall, i32, i32) -> i32,
    pub check_number: unsafe extern "C" fn(*mut LuvCall, i32) -> f64,
    pub opt_number: unsafe extern "C" fn(*mut LuvCall, i32, f64) -> f64,
    pub check_string: unsafe extern "C" fn(*mut LuvCall, i32, *mut u64) -> *const c_char,
    pub opt_string: unsafe extern "C" fn(*mut LuvCall, i32, *const c_char, *mut u64) -> *const c_char,
    pub check_udim: unsafe extern "C" fn(*mut LuvCall, i32, *mut f64) -> i32,
    pub check_color: unsafe extern "C" fn(*mut LuvCall, i32, *mut f64) -> i32,
    pub check_object: unsafe extern "C" fn(*mut LuvCall, i32, *const LuvClass) -> *mut c_void,
    pub to_object: unsafe extern "C" fn(*mut LuvCall, i32, *const LuvClass) -> *mut c_void,
    pub check_pointer: unsafe extern "C" fn(*mut LuvCall, i32) -> *mut c_void,
    pub self_data: unsafe extern "C" fn(*mut LuvCall) -> *mut c_void,
    pub push_nil: unsafe extern "C" fn(*mut LuvCall),
    pub push_boolean: unsafe extern "C" fn(*mut LuvCall, i32),
    pub push_number: unsafe extern "C" fn(*mut LuvCall, f64),
    pub push_string: unsafe extern "C" fn(*mut LuvCall, *const c_char),
    pub push_bytes: unsafe extern "C" fn(*mut LuvCall, *const c_void, u64),
    pub push_udim: unsafe extern "C" fn(*mut LuvCall, f64, f64, f64),
    pub push_color: unsafe extern "C" fn(*mut LuvCall, f64, f64, f64, f64),
    pub push_object: unsafe extern "C" fn(*mut LuvCall, *const LuvClass) -> *mut c_void,
    pub push_argument: unsafe extern "C" fn(*mut LuvCall, i32),
    pub push_self: unsafe extern "C" fn(*mut LuvCall),
    pub push_pointer: unsafe extern "C" fn(*mut LuvCall, *mut c_void),
    pub push_ref: unsafe extern "C" fn(*mut LuvCall, *mut LuvRef),
    pub fail: unsafe extern "C" fn(*mut LuvCall, *const c_char),
    pub retain: unsafe extern "C" fn(*mut LuvCall, i32) -> *mut LuvRef,
    pub release: unsafe extern "C" fn(*mut LuvRef),
    pub begin_event: unsafe extern "C" fn(*mut LuvRef) -> *mut LuvCall,
    pub send_event: unsafe extern "C" fn(*mut LuvCall) -> i32,
    pub print: unsafe extern "C" fn(*const c_char),
    pub warn: unsafe extern "C" fn(*const c_char),
}

impl LuvApi {
    pub unsafe fn this<'a, T>(&self, call: *mut LuvCall) -> Option<&'a mut T> {
        unsafe { (self.self_data)(call).cast::<T>().as_mut() }
    }

    pub unsafe fn object<'a, T>(&self, call: *mut LuvCall, index: i32, class: *const LuvClass) -> Option<&'a mut T> {
        unsafe { (self.check_object)(call, index, class).cast::<T>().as_mut() }
    }

    pub unsafe fn try_object<'a, T>(&self, call: *mut LuvCall, index: i32, class: *const LuvClass) -> Option<&'a mut T> {
        unsafe { (self.to_object)(call, index, class).cast::<T>().as_mut() }
    }

    pub unsafe fn push_new<T>(&self, call: *mut LuvCall, class: *const LuvClass, value: T) -> bool {
        let data = unsafe { (self.push_object)(call, class) }.cast::<T>();
        if data.is_null() {
            return false;
        }
        unsafe { data.write(value) };
        true
    }

    pub unsafe fn check_str<'a>(&self, call: *mut LuvCall, index: i32) -> Option<&'a str> {
        let mut length = 0;
        let text = unsafe { (self.check_string)(call, index, &mut length) };
        if text.is_null() {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(text.cast::<u8>(), length as usize) };
        std::str::from_utf8(bytes).ok()
    }

    pub unsafe fn push_str(&self, call: *mut LuvCall, text: &str) {
        unsafe { (self.push_bytes)(call, text.as_ptr().cast(), text.len() as u64) }
    }

    pub unsafe fn fail_with(&self, call: *mut LuvCall, message: &str) {
        let message = CString::new(message.replace('\0', "")).unwrap_or_default();
        unsafe { (self.fail)(call, message.as_ptr()) }
    }

    pub unsafe fn log(&self, message: &str) {
        let message = CString::new(message.replace('\0', "")).unwrap_or_default();
        unsafe { (self.print)(message.as_ptr()) }
    }
}
