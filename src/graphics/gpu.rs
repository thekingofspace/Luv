use std::future::Future;
use std::os::raw::c_void;
use std::pin::pin;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

use ash::vk::Handle;

use super::hook::VulkanHandles;

pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub vulkan: Option<VulkanHandles>,
}

static GPU: OnceLock<Result<Arc<Gpu>, String>> = OnceLock::new();
static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();
static QUEUE: Mutex<()> = Mutex::new(());

#[cfg(windows)]
const QUIET_LAYERS: [(&str, &str); 1] = [("DISABLE_TWITCH_VULKAN_OVERLAY", "1")];

#[cfg(windows)]
fn quiet_layers() {
    for (variable, value) in QUIET_LAYERS {
        if std::env::var_os(variable).is_none() {
            unsafe { std::env::set_var(variable, value) };
        }
    }
}

#[cfg(not(windows))]
fn quiet_layers() {}

pub fn instance() -> &'static wgpu::Instance {
    INSTANCE.get_or_init(|| {
        quiet_layers();
        wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        })
    })
}

pub fn warm_up() {
    let _ = thread::Builder::new().name("gpu warm up".to_owned()).spawn(|| {
        let _ = Gpu::get();
    });
}

struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        thread::park();
    }
}

impl Gpu {
    pub fn get() -> Result<Arc<Gpu>, String> {
        GPU.get_or_init(Self::create).clone()
    }

    pub fn lock_queue(&self) -> MutexGuard<'static, ()> {
        QUEUE.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn vulkan(device: &wgpu::Device) -> Option<VulkanHandles> {
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }?;
        let shared = hal.shared_instance();
        Some(VulkanHandles {
            instance: shared.raw_instance().handle().as_raw() as usize as *mut c_void,
            physical_device: hal.raw_physical_device().as_raw() as usize as *mut c_void,
            device: hal.raw_device().handle().as_raw() as usize as *mut c_void,
            queue: hal.raw_queue().as_raw() as usize as *mut c_void,
            queue_family: hal.queue_family_index(),
            queue_index: hal.queue_index(),
            get_instance_proc_addr: shared.entry().static_fn().get_instance_proc_addr as *const c_void,
        })
    }

    fn create() -> Result<Arc<Gpu>, String> {
        let instance = instance().clone();
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|error| format!("no Vulkan GPU is available: {error}"))?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("luv"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|error| format!("the Vulkan GPU could not be opened: {error}"))?;
        device.on_uncaptured_error(Arc::new(|error: wgpu::Error| eprintln!("error: gpu: {error}")));
        let vulkan = Self::vulkan(&device);
        Ok(Arc::new(Gpu {
            instance,
            adapter,
            device,
            queue,
            vulkan,
        }))
    }
}
