pub const MAGIC: &[u8; 4] = b"LUVA";
const VERSION: u8 = 1;
pub const HEADER: usize = 20;
pub const MIN_RATE: u32 = 1_000;
pub const MAX_RATE: u32 = 384_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleFormat {
    Float32,
    Int16,
}

impl SampleFormat {
    pub const NAMES: [&'static str; 2] = ["Float32", "Int16"];

    pub fn name(self) -> &'static str {
        match self {
            SampleFormat::Float32 => "Float32",
            SampleFormat::Int16 => "Int16",
        }
    }

    pub fn from_index(index: usize) -> SampleFormat {
        if index == 1 { SampleFormat::Int16 } else { SampleFormat::Float32 }
    }

    pub fn index(self) -> usize {
        match self {
            SampleFormat::Float32 => 0,
            SampleFormat::Int16 => 1,
        }
    }

    pub fn bytes(self) -> usize {
        match self {
            SampleFormat::Float32 => 4,
            SampleFormat::Int16 => 2,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Chunk {
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl Chunk {
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate.max(1))
    }

    #[inline]
    pub fn frame(&self, index: usize) -> (f32, f32) {
        (self.samples[index * 2], self.samples[index * 2 + 1])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawFormat {
    pub rate: u32,
    pub channels: u8,
    pub format: SampleFormat,
}

#[derive(Clone, Debug)]
pub struct PacketData {
    pub rate: u32,
    pub channels: u8,
    pub format: SampleFormat,
    pub frames: u32,
    pub sequence: u32,
    pub peak: f32,
    pub loudness: f32,
    pub payload: Vec<u8>,
}

fn write_samples(samples: impl Iterator<Item = f32>, format: SampleFormat, payload: &mut Vec<u8>) {
    for sample in samples {
        match format {
            SampleFormat::Float32 => payload.extend_from_slice(&sample.to_le_bytes()),
            SampleFormat::Int16 => {
                let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
}

fn read_sample(bytes: &[u8], format: SampleFormat) -> f32 {
    match format {
        SampleFormat::Float32 => {
            let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if value.is_finite() { value } else { 0.0 }
        }
        SampleFormat::Int16 => f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0,
    }
}

fn to_stereo(bytes: &[u8], channels: u8, format: SampleFormat) -> Vec<f32> {
    let size = format.bytes();
    let frame = size * usize::from(channels);
    let mut samples = Vec::with_capacity(bytes.len() / frame * 2);
    for chunk in bytes.chunks_exact(frame) {
        let left = read_sample(&chunk[..size], format);
        let right = if channels > 1 { read_sample(&chunk[size..size * 2], format) } else { left };
        samples.push(left);
        samples.push(right);
    }
    samples
}

impl PacketData {
    pub fn encode(stereo: &[f32], rate: u32, channels: u8, format: SampleFormat, sequence: u32) -> PacketData {
        let frames = stereo.len() / 2;
        let mut payload = Vec::with_capacity(frames * usize::from(channels) * format.bytes());
        let mut peak = 0.0f32;
        let mut energy = 0.0f64;
        for frame in stereo.chunks_exact(2) {
            peak = peak.max(frame[0].abs()).max(frame[1].abs());
            energy += f64::from(frame[0] * frame[0] + frame[1] * frame[1]) * 0.5;
        }
        if channels == 1 {
            write_samples(stereo.chunks_exact(2).map(|frame| (frame[0] + frame[1]) * 0.5), format, &mut payload);
        } else {
            write_samples(stereo.iter().copied(), format, &mut payload);
        }
        PacketData {
            rate,
            channels,
            format,
            frames: frames as u32,
            sequence,
            peak,
            loudness: if frames == 0 { 0.0 } else { (energy / frames as f64).sqrt() as f32 },
            payload,
        }
    }

    pub fn seconds(&self) -> f64 {
        f64::from(self.frames) / f64::from(self.rate.max(1))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER + self.payload.len());
        bytes.extend_from_slice(MAGIC);
        bytes.push(VERSION);
        bytes.push(self.format.index() as u8);
        bytes.push(self.channels);
        bytes.push(0);
        bytes.extend_from_slice(&self.rate.to_le_bytes());
        bytes.extend_from_slice(&self.frames.to_le_bytes());
        bytes.extend_from_slice(&self.sequence.to_le_bytes());
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    pub fn samples(&self) -> Vec<f32> {
        let size = self.format.bytes();
        self.payload.chunks_exact(size).map(|bytes| read_sample(bytes, self.format)).collect()
    }

    pub fn chunk(&self) -> Chunk {
        Chunk {
            rate: self.rate,
            samples: to_stereo(&self.payload, self.channels, self.format),
        }
    }
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

pub fn parse(data: &[u8], raw: RawFormat) -> Result<Chunk, String> {
    if data.len() >= HEADER && &data[..4] == MAGIC {
        if data[4] != VERSION {
            return Err(format!("the audio packet uses version {}, only version {VERSION} is supported", data[4]));
        }
        let format = match data[5] {
            0 => SampleFormat::Float32,
            1 => SampleFormat::Int16,
            other => return Err(format!("the audio packet uses an unknown sample format {other}")),
        };
        let channels = data[6];
        if !(1..=2).contains(&channels) {
            return Err(format!("audio packets hold 1 or 2 channels, this one says {channels}"));
        }
        let rate = word(data, 8);
        if !(MIN_RATE..=MAX_RATE).contains(&rate) {
            return Err(format!("the audio packet has an unsupported sample rate of {rate}"));
        }
        let frames = word(data, 12) as usize;
        let expected = frames * usize::from(channels) * format.bytes();
        let payload = &data[HEADER..];
        if payload.len() != expected {
            return Err(format!(
                "the audio packet should hold {expected} bytes of samples but holds {}",
                payload.len()
            ));
        }
        return Ok(Chunk {
            rate,
            samples: to_stereo(payload, channels, format),
        });
    }
    let frame = raw.format.bytes() * usize::from(raw.channels);
    if !data.len().is_multiple_of(frame) {
        return Err(format!(
            "raw audio must be whole frames of {frame} bytes ({} {} channel{}), got {} bytes",
            raw.format.name(),
            raw.channels,
            if raw.channels == 1 { "" } else { "s" },
            data.len()
        ));
    }
    Ok(Chunk {
        rate: raw.rate,
        samples: to_stereo(data, raw.channels, raw.format),
    })
}
