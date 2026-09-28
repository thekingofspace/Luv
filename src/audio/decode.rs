use std::io::{Cursor, ErrorKind};
use std::sync::Arc;

use opus_decoder::OpusDecoder;
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::packet::Packet;

const MAX_SAMPLES: usize = 1 << 28;
const SCALE: f32 = 1.0 / 32768.0;

pub struct Pcm {
    rate: u32,
    channels: usize,
    frames: usize,
    samples: Box<[i16]>,
}

impl Pcm {
    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.rate)
    }

    #[inline]
    pub fn frame(&self, index: usize) -> (f32, f32) {
        if self.channels == 1 {
            let sample = f32::from(self.samples[index]) * SCALE;
            (sample, sample)
        } else {
            let base = index * 2;
            (
                f32::from(self.samples[base]) * SCALE,
                f32::from(self.samples[base + 1]) * SCALE,
            )
        }
    }
}

fn quantize(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

struct Sink {
    rate: Option<u32>,
    layout: Option<usize>,
    samples: Vec<i16>,
}

impl Sink {
    fn push(&mut self, interleaved: &[f32], channels: usize) -> Result<(), String> {
        let channels = channels.max(1);
        let kept = *self.layout.get_or_insert(channels.min(2));
        for frame in interleaved.chunks_exact(channels) {
            self.samples.push(quantize(frame[0]));
            if kept == 2 {
                self.samples.push(quantize(frame.get(1).copied().unwrap_or(frame[0])));
            }
        }
        if self.samples.len() > MAX_SAMPLES {
            return Err("the sound is too long to load into memory".to_owned());
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Pcm, String> {
        let rate = self
            .rate
            .filter(|rate| *rate > 0)
            .ok_or_else(|| "the sound does not say its sample rate".to_owned())?;
        let channels = self.layout.unwrap_or(1);
        let frames = self.samples.len() / channels;
        if frames == 0 {
            return Err("the sound does not hold any audio".to_owned());
        }
        self.samples.truncate(frames * channels);
        Ok(Pcm {
            rate,
            channels,
            frames,
            samples: self.samples.into_boxed_slice(),
        })
    }
}

fn next_packet(format: &mut dyn FormatReader, track: u32) -> Result<Option<Packet>, String> {
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) | Err(Error::ResetRequired) => return Ok(None),
            Err(Error::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(format!("the sound could not be read: {error}")),
        };
        if packet.track_id == track {
            return Ok(Some(packet));
        }
    }
}

fn decode_opus(format: &mut dyn FormatReader, track: u32, params: &AudioCodecParameters, sink: &mut Sink) -> Result<(), String> {
    let channels = params.channels.as_ref().map_or(2, |channels| channels.count()).max(1);
    if channels > 2 {
        return Err(format!("Opus sounds with {channels} channels are not supported, use mono or stereo"));
    }
    let mut decoder =
        OpusDecoder::new(48_000, channels).map_err(|error| format!("the Opus stream cannot be decoded: {error}"))?;
    let mut pcm = vec![0.0f32; OpusDecoder::MAX_FRAME_SIZE_48K * channels];
    sink.rate = Some(48_000);
    while let Some(packet) = next_packet(format, track)? {
        let Ok(frames) = decoder.decode_float(&packet.data, &mut pcm, false) else {
            continue;
        };
        let start = (packet.trim_start.get() as usize).min(frames);
        let end = frames.saturating_sub(packet.trim_end.get() as usize).max(start);
        sink.push(&pcm[start * channels..end * channels], channels)?;
    }
    Ok(())
}

pub fn decode(data: Arc<[u8]>, extension: Option<&str>) -> Result<Pcm, String> {
    let source = MediaSourceStream::new(Box::new(Cursor::new(data)), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, source, FormatOptions::default(), MetadataOptions::default())
        .map_err(|error| format!("the sound format is not supported: {error}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| "the file does not hold an audio track".to_owned())?;
    let track_id = track.id;
    let expected = track.num_frames;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| "the audio track does not describe its codec".to_owned())?
        .clone();
    let mut sink = Sink {
        rate: params.sample_rate,
        layout: params.channels.as_ref().map(|channels| channels.count().clamp(1, 2)),
        samples: Vec::new(),
    };
    if let (Some(frames), Some(kept)) = (expected, sink.layout) {
        sink.samples.reserve((frames as usize).saturating_mul(kept).min(MAX_SAMPLES));
    }
    if params.codec == CODEC_ID_OPUS {
        decode_opus(format.as_mut(), track_id, &params, &mut sink)?;
        return sink.finish();
    }
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|error| format!("the audio codec is not supported: {error}"))?;
    let mut scratch: Vec<f32> = Vec::new();
    while let Some(packet) = next_packet(format.as_mut(), track_id)? {
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(Error::DecodeError(_) | Error::IoError(_)) => continue,
            Err(error) => return Err(format!("the sound could not be decoded: {error}")),
        };
        let count = decoded.spec().channels().count().max(1);
        sink.rate.get_or_insert(decoded.spec().rate());
        decoded.copy_to_vec_interleaved(&mut scratch);
        sink.push(&scratch, count)?;
    }
    sink.finish()
}

impl Pcm {
    pub fn from_interleaved(rate: u32, channels: usize, interleaved: &[f32]) -> Result<Pcm, String> {
        let channels = channels.clamp(1, 2);
        let frames = interleaved.len() / channels;
        if frames == 0 {
            return Err("the sound does not hold any audio".to_owned());
        }
        if rate == 0 {
            return Err("the sound needs a sample rate above 0".to_owned());
        }
        let samples = interleaved[..frames * channels].iter().map(|sample| quantize(*sample)).collect();
        Ok(Pcm {
            rate,
            channels,
            frames,
            samples,
        })
    }

    pub fn peak(&self) -> f32 {
        let loudest = self.samples.iter().map(|sample| sample.unsigned_abs()).max().unwrap_or(0);
        f32::from(loudest) * SCALE
    }

    pub fn memory(&self) -> usize {
        self.samples.len() * 2
    }

    pub fn wav(&self) -> Vec<u8> {
        let channels = self.channels as u16;
        let data = (self.samples.len() * 2) as u32;
        let block = channels * 2;
        let mut bytes = Vec::with_capacity(44 + data as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&self.rate.to_le_bytes());
        bytes.extend_from_slice(&(self.rate * u32::from(block)).to_le_bytes());
        bytes.extend_from_slice(&block.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data.to_le_bytes());
        for sample in self.samples.iter() {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }
}
