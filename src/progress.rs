use std::io::{IsTerminal, Write};
use std::sync::{Mutex, PoisonError};

const WIDTH: usize = 30;

struct Bar {
    stage: String,
    done: usize,
    total: usize,
}

pub struct Progress {
    print: bool,
    terminal: bool,
    lines: Mutex<Vec<String>>,
    bar: Mutex<Option<Bar>>,
}

impl Progress {
    pub fn quiet() -> Self {
        Self {
            print: false,
            terminal: false,
            lines: Mutex::new(Vec::new()),
            bar: Mutex::new(None),
        }
    }

    pub fn console() -> Self {
        Self {
            print: true,
            terminal: std::io::stderr().is_terminal(),
            lines: Mutex::new(Vec::new()),
            bar: Mutex::new(None),
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn clear(&self, error: &mut impl Write) {
        if self.terminal {
            let _ = write!(error, "\r{}\r", " ".repeat(WIDTH + 60));
        }
    }

    fn draw(&self, bar: &Bar, error: &mut impl Write) {
        if !self.terminal || bar.total == 0 {
            return;
        }
        let share = bar.done as f64 / bar.total as f64;
        let filled = ((share * WIDTH as f64).round() as usize).min(WIDTH);
        let _ = write!(
            error,
            "\r[{}{}] {:>3}% {} {}/{}",
            "=".repeat(filled),
            " ".repeat(WIDTH - filled),
            (share * 100.0).round() as usize,
            bar.stage,
            bar.done,
            bar.total
        );
        let _ = error.flush();
    }

    pub fn line(&self, text: impl Into<String>) {
        let text = text.into();
        if self.print {
            let mut error = std::io::stderr().lock();
            self.clear(&mut error);
            let _ = writeln!(error, "{text}");
            if let Some(bar) = self.bar.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
                self.draw(bar, &mut error);
            }
        }
        self.lines.lock().unwrap_or_else(PoisonError::into_inner).push(text);
    }

    pub fn advance(&self, stage: &str, done: usize, total: usize) {
        let bar = Bar {
            stage: stage.to_owned(),
            done,
            total,
        };
        if self.print {
            self.draw(&bar, &mut std::io::stderr().lock());
        }
        *self.bar.lock().unwrap_or_else(PoisonError::into_inner) = Some(bar);
    }

    pub fn done(&self) {
        if self.print {
            self.clear(&mut std::io::stderr().lock());
        }
        self.bar.lock().unwrap_or_else(PoisonError::into_inner).take();
    }
}
