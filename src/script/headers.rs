const PREFIX: &str = "---@";
const SCANNED: usize = 16 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Headers {
    pub start: bool,
    pub start_async: bool,
    pub boot: bool,
    pub boot_ready: bool,
    pub capture: Option<Option<String>>,
}

impl Headers {
    pub fn is_booted(&self) -> bool {
        self.start || self.start_async || self.boot || self.boot_ready
    }
}

fn quoted(tail: &str) -> Option<String> {
    let inner = tail
        .trim()
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .or_else(|| tail.trim().strip_prefix('(').and_then(|rest| rest.strip_suffix(')')))?
        .trim();
    let quote = inner.chars().next().filter(|quote| *quote == '"' || *quote == '\'')?;
    inner
        .strip_prefix(quote)
        .and_then(|rest| rest.strip_suffix(quote))
        .map(str::to_owned)
}

pub fn headers(source: &[u8]) -> Headers {
    let text = String::from_utf8_lossy(&source[..source.len().min(SCANNED)]);
    let mut found = Headers::default();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix(PREFIX) else {
            if trimmed.starts_with("--") {
                continue;
            }
            break;
        };
        let split = rest
            .find(|character: char| !character.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        let (tag, tail) = rest.split_at(split);
        match tag {
            "start" => found.start = true,
            "startasync" => found.start_async = true,
            "boot" => found.boot = true,
            "bootready" => found.boot_ready = true,
            "capture" => found.capture = Some(quoted(tail)),
            _ => {}
        }
    }
    found
}

pub struct Compiled<'a> {
    pub path: &'a str,
    pub seconds: f64,
    pub chunks: usize,
    pub lines: usize,
    pub bytes: usize,
}

fn seconds(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() { "0".to_owned() } else { text.to_owned() }
}

pub fn capture_line(message: Option<&str>, compiled: &Compiled<'_>) -> String {
    let name = compiled.path.rsplit('/').next().unwrap_or(compiled.path);
    let time = seconds(compiled.seconds);
    match message {
        None => format!(
            "{name} finished compiling in {time}s with {} parallel chunk{}",
            compiled.chunks,
            if compiled.chunks == 1 { "" } else { "s" }
        ),
        Some(message) => {
            let filled = message
                .replace("%time%", &time)
                .replace("%chunks%", &compiled.chunks.to_string())
                .replace("%name%", name)
                .replace("%path%", compiled.path)
                .replace("%lines%", &compiled.lines.to_string())
                .replace("%size%", &compiled.bytes.to_string());
            format!("{name} finished compiling {filled}")
        }
    }
}
