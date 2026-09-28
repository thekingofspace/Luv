use std::sync::Arc;

use tokio::sync::mpsc;

use super::specs::modifier_spec;
use super::{
    BLOCK, Context, DECLICK, Event, Listener, Message, Pcm, Player, PlayerShared, Processor, SILENCE, frames, modifier,
};

pub const MAX_BAKE_SECONDS: f64 = 600.0;
const QUIET: f32 = 1.0 / 32768.0;

pub struct Stage {
    pub class: String,
    pub params: Vec<(usize, f64)>,
}

pub struct Recipe {
    pub rate: u32,
    pub stages: Vec<Stage>,
    pub start: f64,
    pub length: Option<f64>,
    pub speed: f64,
    pub volume: f64,
    pub tail: f64,
    pub normalize: Option<f64>,
    pub channels: Option<usize>,
}

fn chain(stages: &[Stage], rate: f32) -> Result<Vec<Box<dyn Processor>>, String> {
    let mut processors = Vec::with_capacity(stages.len());
    for stage in stages {
        let spec = modifier_spec(&stage.class).ok_or_else(|| format!("'{}' is not a sound modifier", stage.class))?;
        let mut processor =
            modifier(&stage.class, rate, None, None).ok_or_else(|| format!("'{}' is not a sound modifier", stage.class))?;
        processor.rate(rate);
        for (index, param) in spec.params.iter().enumerate() {
            processor.param(index, param.default);
        }
        for (index, value) in &stage.params {
            processor.param(*index, *value);
        }
        processors.push(processor);
    }
    Ok(processors)
}

fn trimmed(samples: &mut Vec<f32>) {
    let frames = samples.len() / 2;
    let last = (0..frames)
        .rev()
        .find(|frame| samples[frame * 2].abs() > QUIET || samples[frame * 2 + 1].abs() > QUIET)
        .map_or(1, |frame| frame + 1);
    samples.truncate(last * 2);
}

fn normalized(samples: &mut [f32], target: f64) {
    let peak = samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    if peak <= QUIET {
        return;
    }
    let gain = 10f32.powf(target.min(0.0) as f32 / 20.0) / peak;
    for sample in samples.iter_mut() {
        *sample *= gain;
    }
}

fn is_mono(samples: &[f32]) -> bool {
    samples.chunks_exact(2).all(|frame| (frame[0] - frame[1]).abs() <= QUIET)
}

pub fn bake(pcm: Arc<Pcm>, recipe: Recipe) -> Result<Pcm, String> {
    let rate = recipe.rate.max(1);
    let seconds = f64::from(rate);
    let (events, mut received) = mpsc::unbounded_channel();
    let listener = Listener::default();
    let context = Context {
        rate: rate as f32,
        node: 1,
        events: &events,
        linked: true,
        focused: true,
        listener: &listener,
    };
    let mut player = Player::new(pcm, PlayerShared::new(), rate as f32);
    player.rate(rate as f32);
    player.param(0, recipe.volume);
    player.param(1, recipe.speed);
    player.message(
        &context,
        Message::Play {
            from: recipe.start.max(0.0),
            fade: false,
            epoch: 1,
        },
    );
    let mut processors = chain(&recipe.stages, rate as f32)?;

    let limit = (MAX_BAKE_SECONDS * seconds) as usize;
    let cut = recipe.length.map(|length| (length.max(0.0) * seconds) as usize);
    let fade = (DECLICK * rate as f32).max(1.0);
    let tail = (recipe.tail.max(0.0) * seconds) as usize;
    let mut samples: Vec<f32> = Vec::new();
    let mut rendered = 0usize;
    let mut finished = false;
    let mut after = 0usize;
    loop {
        let mut block = SILENCE;
        if !finished {
            player.process(&context, &SILENCE, &mut block);
            while let Ok(event) = received.try_recv() {
                if matches!(event, Event::Ended { .. }) {
                    finished = true;
                }
            }
            if let Some(cut) = cut {
                for (frame, [left, right]) in frames(&mut block).enumerate() {
                    let remaining = cut as f32 - (rendered + frame) as f32;
                    let gain = (remaining / fade).clamp(0.0, 1.0);
                    *left *= gain;
                    *right *= gain;
                }
                if rendered + BLOCK >= cut {
                    finished = true;
                }
            }
        }
        for processor in processors.iter_mut() {
            let input = block;
            processor.process(&context, &input, &mut block);
        }
        for (left, right) in block[0].iter().zip(block[1].iter()) {
            samples.push(*left);
            samples.push(*right);
        }
        rendered += BLOCK;
        if finished {
            after += BLOCK;
            if after >= tail {
                break;
            }
        }
        if rendered > limit {
            return Err(format!("a baked sound can be at most {MAX_BAKE_SECONDS} seconds long"));
        }
    }

    trimmed(&mut samples);
    if let Some(target) = recipe.normalize {
        normalized(&mut samples, target);
    }
    let channels = match recipe.channels {
        Some(channels) => channels.clamp(1, 2),
        None if is_mono(&samples) => 1,
        None => 2,
    };
    if channels == 1 {
        let mono: Vec<f32> = samples.chunks_exact(2).map(|frame| (frame[0] + frame[1]) * 0.5).collect();
        return Pcm::from_interleaved(rate, 1, &mono);
    }
    Pcm::from_interleaved(rate, 2, &samples)
}
