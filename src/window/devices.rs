use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::Duration;

use tokio::sync::mpsc;

const POLL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InputKind {
    Keyboard,
    Mouse,
    Touch,
}

impl InputKind {
    pub const ALL: [InputKind; 3] = [InputKind::Keyboard, InputKind::Mouse, InputKind::Touch];

    fn index(self) -> usize {
        match self {
            InputKind::Keyboard => 0,
            InputKind::Mouse => 1,
            InputKind::Touch => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            InputKind::Keyboard => "Keyboard",
            InputKind::Mouse => "Mouse",
            InputKind::Touch => "Touch",
        }
    }

    pub fn from_name(name: &str) -> Option<InputKind> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

pub type DeviceListener = mpsc::UnboundedReceiver<(InputKind, bool)>;

pub struct InputDevices {
    present: Mutex<[bool; 3]>,
    listeners: Mutex<Vec<mpsc::UnboundedSender<(InputKind, bool)>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl InputDevices {
    fn with(present: [bool; 3]) -> Arc<InputDevices> {
        Arc::new(InputDevices {
            present: Mutex::new(present),
            listeners: Mutex::new(Vec::new()),
        })
    }

    pub fn simulated() -> Arc<InputDevices> {
        InputDevices::with([true, true, true])
    }

    pub fn system() -> Arc<InputDevices> {
        static SYSTEM: OnceLock<Arc<InputDevices>> = OnceLock::new();
        SYSTEM
            .get_or_init(|| {
                let devices = InputDevices::with(detect().unwrap_or([true, true, false]));
                let watcher = Arc::downgrade(&devices);
                let _ = thread::Builder::new().name("luv-input-devices".to_owned()).spawn(move || {
                    loop {
                        thread::sleep(POLL);
                        let Some(devices) = watcher.upgrade() else {
                            break;
                        };
                        if let Some(found) = detect() {
                            for kind in InputKind::ALL {
                                devices.set(kind, found[kind.index()]);
                            }
                        }
                    }
                });
                devices
            })
            .clone()
    }

    pub fn present(&self, kind: InputKind) -> bool {
        lock(&self.present)[kind.index()]
    }

    pub fn set(&self, kind: InputKind, present: bool) {
        {
            let mut current = lock(&self.present);
            if current[kind.index()] == present {
                return;
            }
            current[kind.index()] = present;
        }
        lock(&self.listeners).retain(|listener| listener.send((kind, present)).is_ok());
    }

    pub fn subscribe(&self) -> DeviceListener {
        let (sender, receiver) = mpsc::unbounded_channel();
        lock(&self.listeners).push(sender);
        receiver
    }
}

#[cfg(windows)]
fn detect() -> Option<[bool; 3]> {
    use std::ptr;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawInputDevice {
        device: isize,
        kind: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetRawInputDeviceList(list: *mut RawInputDevice, count: *mut u32, size: u32) -> u32;
        fn GetSystemMetrics(index: i32) -> i32;
    }

    const MOUSE_PRESENT: i32 = 19;
    const MAXIMUM_TOUCHES: i32 = 95;
    const FAILED: u32 = u32::MAX;

    let size = size_of::<RawInputDevice>() as u32;
    let mut count = 0u32;
    if unsafe { GetRawInputDeviceList(ptr::null_mut(), &mut count, size) } == FAILED {
        return None;
    }
    let mut devices = vec![RawInputDevice { device: 0, kind: 0 }; count as usize + 8];
    let mut capacity = devices.len() as u32;
    let found = unsafe { GetRawInputDeviceList(devices.as_mut_ptr(), &mut capacity, size) };
    if found == FAILED {
        return None;
    }
    devices.truncate(found as usize);
    let keyboard = devices.iter().any(|device| device.kind == 1);
    let mouse = devices.iter().any(|device| device.kind == 0) || unsafe { GetSystemMetrics(MOUSE_PRESENT) } != 0;
    let touch = unsafe { GetSystemMetrics(MAXIMUM_TOUCHES) } > 0;
    Some([keyboard, mouse, touch])
}

#[cfg(target_os = "linux")]
fn detect() -> Option<[bool; 3]> {
    let listing = std::fs::read_to_string("/proc/bus/input/devices").ok()?;
    let mut found = [false; 3];
    for block in listing.split("\n\n") {
        let mut handlers: Vec<&str> = Vec::new();
        let mut events = 0u64;
        let mut properties = 0u64;
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("H: Handlers=") {
                handlers = rest.split_whitespace().collect();
            } else if let Some(rest) = line.strip_prefix("B: EV=") {
                events = u64::from_str_radix(rest.trim(), 16).unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("B: PROP=") {
                properties = u64::from_str_radix(rest.trim(), 16).unwrap_or(0);
            }
        }
        if handlers.contains(&"kbd") && events & 0x10_0000 != 0 {
            found[0] = true;
        }
        if handlers.iter().any(|handler| handler.starts_with("mouse")) {
            found[1] = true;
        }
        if properties & 0x2 != 0 && events & 0x8 != 0 {
            found[2] = true;
        }
    }
    Some(found)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn detect() -> Option<[bool; 3]> {
    None
}
