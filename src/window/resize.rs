use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

type Redraw = Box<dyn Fn() + Send + Sync>;

pub struct LiveResize {
    dragging: AtomicBool,
    wanted: Mutex<Option<[u32; 2]>>,
    redraw: Mutex<Option<Redraw>>,
    #[cfg(windows)]
    event: event::Event,
}

impl LiveResize {
    pub fn new() -> LiveResize {
        LiveResize {
            dragging: AtomicBool::new(false),
            wanted: Mutex::new(None),
            redraw: Mutex::new(None),
            #[cfg(windows)]
            event: event::Event::new(),
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.load(Ordering::SeqCst)
    }

    pub fn set_dragging(&self, dragging: bool) -> bool {
        self.dragging.swap(dragging, Ordering::SeqCst)
    }

    pub fn set_redraw(&self, redraw: Option<Redraw>) {
        *self.redraw.lock().unwrap_or_else(PoisonError::into_inner) = redraw;
    }

    pub fn presented(&self, size: [u32; 2]) {
        let mut wanted = self.wanted.lock().unwrap_or_else(PoisonError::into_inner);
        if *wanted == Some(size) {
            *wanted = None;
            #[cfg(windows)]
            self.event.set();
        }
    }

    #[cfg(windows)]
    pub fn wait_for(&self, size: [u32; 2], timeout: std::time::Duration) {
        {
            let redraw = self.redraw.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(redraw) = redraw.as_ref() else {
                return;
            };
            *self.wanted.lock().unwrap_or_else(PoisonError::into_inner) = Some(size);
            self.event.reset();
            redraw();
        }
        self.event.wait(timeout);
        *self.wanted.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

impl Default for LiveResize {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
mod event {
    use std::os::raw::c_void;
    use std::time::{Duration, Instant};

    const QS_SENDMESSAGE: u32 = 0x0040;
    const PM_QS_SENDMESSAGE: u32 = QS_SENDMESSAGE << 16;
    const WAIT_OBJECT_0: u32 = 0;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateEventW(attributes: *const c_void, manual: i32, initial: i32, name: *const u16) -> isize;
        fn SetEvent(event: isize) -> i32;
        fn ResetEvent(event: isize) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MsgWaitForMultipleObjectsEx(count: u32, handles: *const isize, milliseconds: u32, wake: u32, flags: u32) -> u32;
        fn PeekMessageW(message: *mut c_void, window: isize, first: u32, last: u32, remove: u32) -> i32;
    }

    pub struct Event(isize);

    unsafe impl Send for Event {}
    unsafe impl Sync for Event {}

    impl Event {
        pub fn new() -> Event {
            Event(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })
        }

        pub fn set(&self) {
            if self.0 != 0 {
                unsafe { SetEvent(self.0) };
            }
        }

        pub fn reset(&self) {
            if self.0 != 0 {
                unsafe { ResetEvent(self.0) };
            }
        }

        pub fn wait(&self, timeout: Duration) {
            if self.0 == 0 {
                return;
            }
            let deadline = Instant::now() + timeout;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return;
                }
                let milliseconds = remaining.as_millis().clamp(1, u128::from(u32::MAX)) as u32;
                let result = unsafe { MsgWaitForMultipleObjectsEx(1, &self.0, milliseconds, QS_SENDMESSAGE, 0) };
                if result != WAIT_OBJECT_0 + 1 {
                    return;
                }
                let mut message = [0u64; 8];
                unsafe { PeekMessageW(message.as_mut_ptr().cast(), 0, 0, 0, PM_QS_SENDMESSAGE) };
            }
        }
    }

    impl Drop for Event {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe { CloseHandle(self.0) };
            }
        }
    }
}
