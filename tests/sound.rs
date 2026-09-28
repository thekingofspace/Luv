mod common;

use std::f64::consts::TAU;
use std::fs;
use std::sync::Arc;

use common::{Outcome, run_with, workspace};
use luv::project::Project;
use luv::runtime::Engine;
use luv::window::{HeadlessWindows, WindowEvent, WindowSystem};
use mlua::Table;

const RATE: u32 = 48_000;

fn wav(channels: u16, samples: &[f32]) -> Vec<u8> {
    let data = samples.len() * 2;
    let mut bytes = Vec::with_capacity(44 + data);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data as u32).to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    bytes
}

fn tone(frequency: f64, seconds: f64, amplitude: f64) -> Vec<u8> {
    let frames = (seconds * f64::from(RATE)) as usize;
    let samples: Vec<f32> = (0..frames)
        .map(|frame| (amplitude * (TAU * frequency * frame as f64 / f64::from(RATE)).sin()) as f32)
        .collect();
    wav(1, &samples)
}

fn opus_tone(frequency: f64, seconds: f64, amplitude: f64) -> Vec<u8> {
    use opus_pure::{Application, OggOpusWriter, OpusEncoder, OpusHead};
    let mut encoder = OpusEncoder::new(48_000, 1, Application::Audio).unwrap();
    let mut head = OpusHead::new(1, 48_000).unwrap();
    head.pre_skip = encoder.lookahead() as u16;
    let mut writer = OggOpusWriter::new(Vec::new(), head).unwrap();
    let frames = (seconds * 48_000.0) as usize;
    let samples: Vec<f32> = (0..frames)
        .map(|frame| (amplitude * (TAU * frequency * frame as f64 / 48_000.0).sin()) as f32)
        .collect();
    let mut packet = vec![0u8; 4000];
    for chunk in samples.chunks(960) {
        let mut input = chunk.to_vec();
        input.resize(960, 0.0);
        let size = encoder.encode(&input, 960, &mut packet).unwrap();
        writer.write_packet(&packet[..size]).unwrap();
    }
    writer.finish().unwrap()
}

fn stats(lua: &mlua::Lua, samples: &[f32], threshold: f64) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let mut peak = [0.0f64; 2];
    let mut jump = 0.0f64;
    let mut jumps = 0;
    let mut energy = 0.0f64;
    let mut previous: Option<[f64; 2]> = None;
    for frame in samples.chunks_exact(2) {
        let current = [f64::from(frame[0]), f64::from(frame[1])];
        for channel in 0..2 {
            peak[channel] = peak[channel].max(current[channel].abs());
            energy += current[channel] * current[channel];
            if let Some(previous) = previous {
                let step = (current[channel] - previous[channel]).abs();
                jump = f64::max(jump, step);
                if step > threshold {
                    jumps += 1;
                }
            }
        }
        previous = Some(current);
    }
    let frames = samples.len() / 2;
    table.set("frames", frames)?;
    table.set("left", peak[0])?;
    table.set("right", peak[1])?;
    table.set("peak", peak[0].max(peak[1]))?;
    table.set("jump", jump)?;
    table.set("jumps", jumps)?;
    table.set("rms", if frames == 0 { 0.0 } else { (energy / (frames * 2) as f64).sqrt() })?;
    Ok(table)
}

async fn run_sound(files: Vec<(&'static str, Vec<u8>)>, source: &str) -> (Outcome, Arc<HeadlessWindows>) {
    let dir = workspace(&[("src/main.luau", source)]);
    for (path, bytes) in &files {
        let target = dir.path().join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
    let headless = Arc::new(HeadlessWindows::new());
    let project = Project::load(dir.path()).unwrap();
    let simulator = headless.clone();
    let outcome = run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        let windows = simulator.clone();
        builder.game("Fixture", ".").windows(simulator.clone()).setup(move |lua| {
            let find = {
                let windows = windows.clone();
                move |title: &str| {
                    windows
                        .find(title)
                        .ok_or_else(|| mlua::Error::runtime(format!("no window titled {title}")))
                }
            };
            let focus = {
                let (windows, find) = (windows.clone(), find.clone());
                lua.create_function(move |_, (title, focused): (String, bool)| {
                    Ok(windows.simulate(find(&title)?, WindowEvent::Focused(focused)))
                })?
            };
            let speaker = {
                let windows = windows.clone();
                lua.create_function(move |lua, (device, threshold): (Option<String>, Option<f64>)| {
                    let samples = windows.audio().take_capture(device.as_deref());
                    stats(lua, &samples, threshold.unwrap_or(0.05))
                })?
            };
            let devices = {
                let windows = windows.clone();
                lua.create_function(move |_, list: Vec<String>| {
                    windows.audio().set_simulated_devices(list);
                    Ok(())
                })?
            };
            let resident = lua.create_function(|lua, ()| {
                let engine = lua
                    .app_data_ref::<Arc<Engine>>()
                    .map(|engine| engine.clone())
                    .ok_or_else(|| mlua::Error::runtime("no engine"))?;
                Ok(engine.sounds().resident())
            })?;
            lua.globals().set("simulateFocus", focus)?;
            lua.globals().set("takeSpeaker", speaker)?;
            lua.globals().set("setAudioDevices", devices)?;
            lua.globals().set("residentSounds", resident)
        })
    })
    .await;
    (outcome, headless)
}

fn sided(seconds: f64, left: f64, right: f64) -> Vec<u8> {
    let frames = (seconds * f64::from(RATE)) as usize;
    let mut samples = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let phase = TAU * 375.0 * frame as f64 / f64::from(RATE);
        samples.push((left * phase.sin()) as f32);
        samples.push((right * phase.sin()) as f32);
    }
    wav(2, &samples)
}

fn tone_asset() -> Vec<(&'static str, Vec<u8>)> {
    vec![("assets/tone.wav", tone(375.0, 1.0, 0.5))]
}

const OPEN: &str = r#"
local Window = import("Window")
local window = Window.new({ Title = "Sound" })
sleep(60)
local Sound = window:GetAPI("Sound")
"#;

fn script(body: &str) -> String {
    format!("{OPEN}\n{body}\nwindow:Close()\n")
}

fn number(results: &Table, key: &str) -> f64 {
    results.get::<f64>(key).unwrap_or_else(|error| panic!("{key}: {error}"))
}

fn flag(results: &Table, key: &str) -> bool {
    results.get::<bool>(key).unwrap_or_else(|error| panic!("{key}: {error}"))
}

#[tokio::test]
async fn sound_nodes_play_pause_loop_and_end() {
    let (outcome, _) = run_sound(
        vec![("assets/short.wav", tone(375.0, 0.3, 0.5))],
        &script(
            r#"
local short = Sound:SoundNode("short.wav", { Name = "Short" })
local speaker = Sound:ToSpeaker()
short.Input:Link(speaker.Output)
events = {}
for _, name in { "Started", "Stopped", "Paused", "Resumed", "Ended", "Looped" } do
    short[name]:BindHandler("log", function()
        table.insert(events, name)
    end)
end
results = {
    class = short.ClassName,
    name = short.Name,
    length = short.Length,
    rate = short.SampleRate,
    channels = short.Channels,
    asset = short.Asset,
    idle = not short.IsPlaying and not short.IsPaused,
}
takeSpeaker()
short:Play()
results.playing = short.IsPlaying
sleep(120)
results.position = short.PlayPosition
results.playingPeak = takeSpeaker().peak
short:Pause()
results.paused = short.IsPaused and not short.IsPlaying
local pausedAt = short.PlayPosition
sleep(80)
takeSpeaker()
sleep(80)
results.pausedPeak = takeSpeaker().peak
results.held = math.abs(short.PlayPosition - pausedAt) < 0.05
short:Resume()
short.Ended:Wait()
results.endedStopped = not short.IsPlaying and short.PlayPosition == 0
short.Looping = true
short:Play()
local count = short.Looped:Wait()
results.loops = count
short:Stop()
results.stopped = not short.IsPlaying and short.PlayPosition == 0
short.PlayPosition = 0.2
results.startsAt = short.PlayPosition
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<String>("class").unwrap(), "SoundNode");
    assert_eq!(results.get::<String>("name").unwrap(), "Short");
    assert!((number(&results, "length") - 0.3).abs() < 0.001);
    assert_eq!(number(&results, "rate"), 48_000.0);
    assert_eq!(number(&results, "channels"), 1.0);
    assert_eq!(results.get::<String>("asset").unwrap(), "short.wav");
    let position = number(&results, "position");
    assert!(position > 0.04 && position < 0.25, "position {position}");
    let peak = number(&results, "playingPeak");
    assert!((peak - 0.5).abs() < 0.05, "peak {peak}");
    assert!(number(&results, "pausedPeak") < 0.001);
    assert_eq!(number(&results, "loops"), 1.0);
    assert!((number(&results, "startsAt") - 0.2).abs() < 1.0e-9);
    for key in ["idle", "playing", "paused", "held", "endedStopped", "stopped"] {
        assert!(flag(&results, key), "{key}");
    }
    let events: Vec<String> = outcome.global("events");
    assert_eq!(
        events,
        ["Started", "Paused", "Resumed", "Ended", "Started", "Looped", "Stopped"]
    );
}

#[tokio::test]
async fn pausing_seeking_and_rerouting_never_clicks() {
    let (outcome, _) = run_sound(
        tone_asset(),
        &script(
            r#"
local tone = Sound:SoundNode("tone.wav")
local speaker = Sound:ToSpeaker()
tone.Input:Link(speaker.Output)
takeSpeaker()
tone:Play()
sleep(100)
tone:Pause()
sleep(50)
tone:Resume()
sleep(50)
tone.PlayPosition = 0.5
sleep(50)
tone.Volume = 0.2
sleep(50)
tone.Volume = 1
sleep(50)
speaker.Output:Unlink(tone.Input)
sleep(50)
tone.Input:Link(speaker.Output)
sleep(50)
tone:Play()
sleep(50)
tone:Stop()
sleep(60)
capture = takeSpeaker(nil, 0.04)
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let capture: Table = outcome.global("capture");
    let peak = number(&capture, "peak");
    assert!(peak > 0.45, "the tone should be heard, peak {peak}");
    let jump = number(&capture, "jump");
    assert!(jump < 0.04, "the output jumped by {jump}, which clicks");
    assert_eq!(number(&capture, "jumps"), 0.0);
}

#[tokio::test]
async fn outputs_layer_every_linked_sound() {
    let (outcome, _) = run_sound(
        tone_asset(),
        &script(
            r#"
local first = Sound:SoundNode("tone.wav")
local second = Sound:SoundNode("tone.wav")
local speaker = Sound:ToSpeaker()
first.Input:Link(speaker.Output)
second.Input:Link(speaker)
first.Volume = 0.5
second.Volume = 0.5
takeSpeaker()
first:Play()
second:Play()
sleep(150)
layered = takeSpeaker().peak
results = {
    shared = residentSounds(),
    links = #speaker.Output:GetLinks(),
}
first:Destroy()
second:Destroy()
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let layered: f64 = outcome.global("layered");
    assert!((layered - 0.5).abs() < 0.05, "two half-volume tones should layer to 0.5, got {layered}");
    let results: Table = outcome.global("results");
    assert_eq!(number(&results, "shared"), 1.0);
    assert_eq!(number(&results, "links"), 2.0);
}

#[tokio::test]
async fn modifiers_shape_the_sound_on_its_way_out() {
    let (outcome, _) = run_sound(
        vec![
            ("assets/tone.wav", tone(375.0, 1.0, 0.5)),
            ("assets/high.wav", tone(6000.0, 1.0, 0.5)),
        ],
        &script(
            r#"
local function capture(source, chain)
    local bytes = Sound:ToBytes({ Format = enum.AudioFormat.Float32 })
    local state = { left = 0, right = 0, count = 0, finite = true, node = bytes }
    bytes.OnIncoming:BindHandler("collect", function(packet)
        state.count += 1
        for index, sample in packet:GetSamples() do
            if sample ~= sample or math.abs(sample) == math.huge then
                state.finite = false
            end
            if index % 2 == 1 then
                state.left = math.max(state.left, math.abs(sample))
            else
                state.right = math.max(state.right, math.abs(sample))
            end
        end
    end)
    local previous = source
    for _, node in chain do
        previous.Input:Link(node.Output)
        previous = node
    end
    previous.Input:Link(bytes.Output)
    return state, bytes
end

local tone = Sound:SoundNode("tone.wav", { Looping = true })
local high = Sound:SoundNode("high.wav", { Looping = true })
local gain = Sound:Modifier("Gain", { Volume = 0.5 })
local pan = Sound:Modifier("Pan", { Pan = -1 })
local lowPass = Sound:Modifier("LowPass", { Cutoff = 200 })
local highPass = Sound:Modifier("HighPass", { Cutoff = 8000 })
local bypassed = Sound:Modifier("Gain", { Volume = 0, Enabled = false })
local meter = Sound:Modifier("Meter")
local halved = capture(tone, { gain })
local panned = capture(tone, { pan })
local lowered = capture(high, { lowPass })
local raised = capture(tone, { highPass })
local passed = capture(tone, { bypassed })
local metered = capture(tone, { meter })
tone:Play()
high:Play()
sleep(300)
results = {
    halved = halved.left,
    pannedLeft = panned.left,
    pannedRight = panned.right,
    lowered = lowered.left,
    raised = raised.left,
    passed = passed.left,
    meterPeak = meter.Peak,
    meterLoudness = meter.Loudness,
    count = halved.count,
}

local kinds = {
    "Gain", "Pan", "LowPass", "HighPass", "BandPass", "Notch", "Peak", "LowShelf", "HighShelf", "Equalizer",
    "Echo", "Reverb", "Chorus", "Flanger", "Phaser", "Tremolo", "Vibrato", "Distortion", "BitCrusher",
    "Compressor", "Limiter", "NoiseGate", "PitchShift", "RingModulator", "StereoWidth", "Meter",
}
local states = {}
for _, kind in kinds do
    local modifier = Sound:Modifier(kind)
    assert(modifier.ClassName == kind, kind)
    assert(modifier.Enabled == true, kind)
    states[kind] = capture(tone, { modifier })
end
sleep(250)
broken = {}
for kind, state in states do
    if not state.finite or state.left > 4 or state.right > 4 or state.count == 0 then
        table.insert(broken, kind)
    end
    if state.left < 0.01 and state.right < 0.01 then
        table.insert(broken, kind .. " is silent")
    end
end

for _, state in { halved, panned, lowered, raised, passed, metered } do
    state.node:Destroy()
end
for _, state in states do
    state.node:Destroy()
end
sleep(50)

local echo = Sound:Modifier("Echo", { Delay = 0.05, Feedback = 0.5, Mix = 1 })
local tail = capture(tone, { echo })
sleep(150)
tone:Stop()
high:Stop()
sleep(40)
tail.left = 0
local before = tail.count
sleep(150)
results.echoTail = tail.left
results.tailPackets = tail.count - before
results.unknown = tostring(select(2, pcall(function()
    Sound:Modifier("Kazoo")
end)))
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let halved = number(&results, "halved");
    assert!((halved - 0.25).abs() < 0.03, "gain 0.5 should halve the tone, got {halved}");
    let left = number(&results, "pannedLeft");
    let right = number(&results, "pannedRight");
    assert!(left > 0.9 && right < 0.01, "hard left pan gave {left} and {right}");
    let lowered = number(&results, "lowered");
    assert!(lowered < 0.05, "a 200 Hz low pass should remove a 6 kHz tone, got {lowered}");
    let raised = number(&results, "raised");
    assert!(raised < 0.05, "an 8 kHz high pass should remove a 375 Hz tone, got {raised}");
    let passed = number(&results, "passed");
    assert!((passed - 0.5).abs() < 0.05, "a disabled modifier should pass the sound through, got {passed}");
    let meter = number(&results, "meterPeak");
    assert!(meter > 0.4 && meter < 0.6, "meter peak {meter}");
    let loudness = number(&results, "meterLoudness");
    assert!(loudness > 0.2 && loudness < 0.36, "meter loudness {loudness}");
    assert!(number(&results, "count") > 5.0);
    let tail = number(&results, "echoTail");
    let packets = number(&results, "tailPackets");
    assert!(tail > 0.02, "the echo should keep ringing after the tone stops, got {tail} over {packets} packets");
    assert!(results.get::<String>("unknown").unwrap().contains("'Kazoo' is not a sound modifier"));
    let broken: Vec<String> = outcome.global("broken");
    assert!(broken.is_empty(), "modifiers misbehaved: {broken:?}");
}

#[tokio::test]
async fn bytes_travel_between_nodes_as_packets_and_strings() {
    let (outcome, _) = run_sound(
        tone_asset(),
        &script(
            r#"
local tone = Sound:SoundNode("tone.wav", { Looping = true })
local bytes = Sound:ToBytes({ Format = enum.AudioFormat.Float32 })
local compact = Sound:ToBytes({ SampleRate = 16000, Channels = 1, PacketDuration = 0.04 })
local stream = Sound:FromBytes()
local speaker = Sound:ToSpeaker()
tone.Input:Link(bytes.Output)
tone.Input:Link(compact.Output)
stream.Input:Link(speaker.Output)
packets = {}
local first
bytes.OnIncoming:BindHandler("forward", function(packet)
    first = first or packet
    table.insert(packets, packet.Sequence)
    stream:Push(packet:ToString())
end)
local small
compact.OnIncoming:BindHandler("inspect", function(packet)
    small = small or packet
end)
tone:Play()
sleep(400)
local heard = takeSpeaker()
tone:Stop()
sleep(100)
bytes.OnIncoming:UnBind("forward")
results = {
    heard = heard.peak,
    rate = first.SampleRate,
    channels = first.Channels,
    frames = first.Frames,
    duration = first.Duration,
    format = first.Format.Name,
    size = #first:ToString(),
    bufferSize = buffer.len(first:ToBuffer()),
    samples = #first:GetSamples(),
    left = #first:GetSamples(1),
    text = tostring(first),
    smallRate = small.SampleRate,
    smallChannels = small.Channels,
    smallFrames = small.Frames,
    smallFormat = small.Format.Name,
    smallSize = #small:ToString(),
}

local raw = Sound:FromBytes({ Format = enum.AudioFormat.Int16, Channels = 1, SampleRate = 24000, Prebuffer = 0 })
raw.Input:Link(speaker.Output)
local drained = false
raw.Drained:BindHandler("done", function()
    drained = true
end)
local parts = {}
for index = 0, 2399 do
    parts[#parts + 1] = string.pack("<i2", math.floor(16000 * math.sin(index / 24000 * 2 * math.pi * 300)))
end
takeSpeaker()
raw:Push(table.concat(parts))
results.buffered = raw.Buffered
sleep(200)
results.rawHeard = takeSpeaker().peak
results.drained = drained
results.bufferedAfter = raw.Buffered

local direct = Sound:FromBytes()
direct.Input:Link(speaker.Output)
direct:Push(first)
direct:Push(first:ToBuffer())
results.directBuffered = direct.Buffered
direct:Clear()
results.cleared = direct.Buffered
results.badPush = tostring(select(2, pcall(function()
    raw:Push("abc")
end)))
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let heard = number(&results, "heard");
    assert!((heard - 0.5).abs() < 0.06, "the pushed packets should play back, peak {heard}");
    assert_eq!(number(&results, "rate"), 48_000.0);
    assert_eq!(number(&results, "channels"), 2.0);
    assert_eq!(number(&results, "frames"), 960.0);
    assert!((number(&results, "duration") - 0.02).abs() < 1.0e-9);
    assert_eq!(results.get::<String>("format").unwrap(), "Float32");
    assert_eq!(number(&results, "size"), 20.0 + 960.0 * 2.0 * 4.0);
    assert_eq!(number(&results, "bufferSize"), 20.0 + 960.0 * 2.0 * 4.0);
    assert_eq!(number(&results, "samples"), 1920.0);
    assert_eq!(number(&results, "left"), 960.0);
    assert_eq!(results.get::<String>("text").unwrap(), "AudioPacket(960 frames, 48000 Hz, 2 channels)");
    assert_eq!(number(&results, "smallRate"), 16_000.0);
    assert_eq!(number(&results, "smallChannels"), 1.0);
    assert_eq!(number(&results, "smallFrames"), 640.0);
    assert_eq!(results.get::<String>("smallFormat").unwrap(), "Int16");
    assert_eq!(number(&results, "smallSize"), 20.0 + 640.0 * 2.0);
    assert!((number(&results, "buffered") - 0.1).abs() < 0.001);
    let raw = number(&results, "rawHeard");
    assert!(raw > 0.4 && raw < 0.6, "raw pushed audio peak {raw}");
    assert!(flag(&results, "drained"));
    assert!(number(&results, "bufferedAfter") < 0.001);
    assert!((number(&results, "directBuffered") - 0.04).abs() < 0.001);
    assert_eq!(number(&results, "cleared"), 0.0);
    assert!(results.get::<String>("badPush").unwrap().contains("whole frames"));
    let packets: Vec<u32> = outcome.global("packets");
    assert!(packets.len() > 10);
    assert!(packets.windows(2).all(|pair| pair[1] == pair[0] + 1));
}

#[tokio::test]
async fn speakers_mute_while_their_window_is_unfocused() {
    let (outcome, _) = run_sound(
        tone_asset(),
        &script(
            r#"
local tone = Sound:SoundNode("tone.wav", { Looping = true })
local speaker = Sound:ToSpeaker()
tone.Input:Link(speaker.Output)
tone:Play()
sleep(100)
results = { owned = speaker.OwnedByWindow, focused = takeSpeaker().peak }
simulateFocus("Sound", false)
sleep(120)
takeSpeaker()
local before = tone.PlayPosition
sleep(100)
results.unfocused = takeSpeaker().peak
results.advanced = tone.PlayPosition ~= before and tone.IsPlaying
simulateFocus("Sound", true)
sleep(120)
takeSpeaker()
sleep(60)
results.refocused = takeSpeaker().peak
speaker.OwnedByWindow = false
simulateFocus("Sound", false)
sleep(120)
takeSpeaker()
sleep(60)
results.unowned = takeSpeaker().peak
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(flag(&results, "owned"));
    assert!(number(&results, "focused") > 0.45);
    assert!(number(&results, "unfocused") < 0.001, "an owned speaker should be muted without focus");
    assert!(flag(&results, "advanced"), "the sound should keep playing while muted");
    assert!(number(&results, "refocused") > 0.45);
    assert!(number(&results, "unowned") > 0.45, "a speaker not owned by the window keeps playing");
}

#[tokio::test]
async fn spatial_speakers_pan_and_fade_with_distance() {
    let (outcome, _) = run_sound(
        tone_asset(),
        &script(
            r#"
local tone = Sound:SoundNode("tone.wav", { Looping = true })
local speaker = Sound:ToSpeaker({ Spatial = true, Binaural = false })
tone.Input:Link(speaker.Output)
tone:Play()
local function listen(position)
    speaker.Position = position
    sleep(80)
    takeSpeaker()
    sleep(80)
    return takeSpeaker()
end
local left = listen(vector.create(-500, 0, 0))
local right = listen(vector.create(500, 0, 0))
local far = listen(vector.create(5000, 0, 0))
Sound.Listener.Position = vector.create(5000, 0, 0)
local moved = listen(udim.new(5000, 0, 0))
speaker.Binaural = true
local binaural = listen(vector.create(5400, 0, 0))
results = {
    leftLeft = left.left,
    leftRight = left.right,
    rightLeft = right.left,
    rightRight = right.right,
    far = far.peak,
    movedLeft = moved.left,
    movedRight = moved.right,
    binauralLeft = binaural.left,
    binauralRight = binaural.right,
    position = speaker.Position,
    listener = Sound.Listener.Position,
    mode = speaker.RollOffMode.Name,
}
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(number(&results, "leftLeft") > 3.0 * number(&results, "leftRight"));
    assert!(number(&results, "rightRight") > 3.0 * number(&results, "rightLeft"));
    assert!(number(&results, "far") < 0.001, "beyond MaxDistance the speaker is silent");
    let (moved_left, moved_right) = (number(&results, "movedLeft"), number(&results, "movedRight"));
    assert!((moved_left - moved_right).abs() < 0.02 && moved_left > 0.45, "{moved_left} {moved_right}");
    let (binaural_left, binaural_right) = (number(&results, "binauralLeft"), number(&results, "binauralRight"));
    assert!(binaural_right > binaural_left && binaural_left > 0.02, "{binaural_left} {binaural_right}");
    let position: mlua::Vector = results.get("position").unwrap();
    assert_eq!((position.x(), position.y(), position.z()), (5400.0, 0.0, 0.0));
    let listener: mlua::Vector = results.get("listener").unwrap();
    assert_eq!(listener.x(), 5000.0);
    assert_eq!(results.get::<String>("mode").unwrap(), "Linear");
}

#[tokio::test]
async fn speakers_target_output_devices() {
    let (outcome, _) = run_sound(
        tone_asset(),
        &script(
            r#"
local tone = Sound:SoundNode("tone.wav", { Looping = true })
local speaker = Sound:ToSpeaker()
tone.Input:Link(speaker.Output)
local changes = {}
Sound.ActivationChanged:BindHandler("log", function(connected)
    table.insert(changes, connected)
end)
results = {
    devices = Sound:GetDevices(),
    exists = Sound:DeviceExists("Simulated Headphones"),
    missing = Sound:DeviceExists("Nope"),
    default = Sound.DefaultDevice,
    connected = Sound.IsConnected,
    device = speaker.Device,
}
tone:Play()
speaker.Device = "Simulated Headphones"
sleep(150)
takeSpeaker()
takeSpeaker("Simulated Headphones")
sleep(100)
results.headphones = takeSpeaker("Simulated Headphones").peak
results.speakers = takeSpeaker().peak
results.chosen = speaker.Device
setAudioDevices({ "Simulated Speakers" })
sleep(150)
takeSpeaker()
sleep(100)
results.fallback = takeSpeaker().peak
speaker.Device = "Nope"
sleep(100)
takeSpeaker()
sleep(80)
results.unknownDevice = takeSpeaker().peak
setAudioDevices({})
sleep(80)
results.disconnected = not Sound.IsConnected
setAudioDevices({ "Simulated Speakers", "Simulated Headphones" })
sleep(80)
results.reconnected = Sound.IsConnected
results.changes = changes
speaker.Device = nil
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let devices: Vec<String> = results.get("devices").unwrap();
    assert_eq!(devices, ["Simulated Speakers", "Simulated Headphones"]);
    assert!(flag(&results, "exists"));
    assert!(!flag(&results, "missing"));
    assert_eq!(results.get::<String>("default").unwrap(), "Simulated Speakers");
    assert!(flag(&results, "connected"));
    assert!(results.get::<Option<String>>("device").unwrap().is_none());
    assert!(number(&results, "headphones") > 0.45);
    assert!(number(&results, "speakers") < 0.001);
    assert_eq!(results.get::<String>("chosen").unwrap(), "Simulated Headphones");
    assert!(number(&results, "fallback") > 0.45, "without headphones the speaker falls back to the default");
    assert!(number(&results, "unknownDevice") > 0.45);
    assert!(flag(&results, "disconnected"));
    assert!(flag(&results, "reconnected"));
    let changes: Vec<bool> = results.get("changes").unwrap();
    assert_eq!(changes, [false, true]);
}

#[tokio::test]
async fn closing_the_window_stops_its_sounds_and_frees_them() {
    let (outcome, _) = run_sound(
        tone_asset(),
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Sound" })
sleep(60)
local Sound = window:GetAPI("Sound")
local tone = Sound:SoundNode("tone.wav", { Looping = true })
local speaker = Sound:ToSpeaker()
tone.Input:Link(speaker.Output)
tone:Play()
sleep(100)
results = { before = takeSpeaker().peak, resident = residentSounds() }
window:Close()
sleep(100)
takeSpeaker()
sleep(100)
results.after = takeSpeaker().peak
results.destroyed = tostring(select(2, pcall(function()
    return tone.IsPlaying
end)))
results.closed = tostring(select(2, pcall(function()
    return Sound:ToSpeaker()
end)))
collectgarbage()
sleep(50)
results.freed = residentSounds()
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(number(&results, "before") > 0.45);
    assert_eq!(number(&results, "resident"), 1.0);
    assert!(number(&results, "after") < 0.001);
    assert!(results.get::<String>("destroyed").unwrap().contains("has been destroyed"));
    assert!(results.get::<String>("closed").unwrap().contains("window that is closed"));
    assert_eq!(number(&results, "freed"), 0.0);
}

#[tokio::test]
async fn bulk_update_sets_sound_properties() {
    let (outcome, _) = run_sound(
        tone_asset(),
        &script(
            r#"
local Bulk = import("Bulk")
local tone = Sound:SoundNode("tone.wav")
local speaker = Sound:ToSpeaker()
local echo = Sound:Modifier("Echo")
local gain = Sound:Modifier("Gain")
Bulk.BulkUpdate({
    [tone] = { Volume = 0.25, Looping = true, PlaybackSpeed = 2, LoopEnd = 0.5 },
    [speaker] = { Spatial = true, Position = vector.create(1, 2, 3), RollOffMode = enum.RollOffMode.Inverse, Device = "Simulated Headphones" },
    [echo] = { Delay = 0.5, Mix = 0.25, PingPong = true, Enabled = false },
    [Sound.Listener] = { Position = vector.create(4, 5, 6), Forward = vector.create(0, 0, 1) },
    [Sound] = { Volume = 0.5 },
})
gain:Fade(0, 0)
results = {
    volume = tone.Volume,
    looping = tone.Looping,
    speed = tone.PlaybackSpeed,
    loopEnd = tone.LoopEnd,
    spatial = speaker.Spatial,
    position = speaker.Position,
    mode = speaker.RollOffMode.Name,
    device = speaker.Device,
    delay = echo.Delay,
    mix = echo.Mix,
    pingPong = echo.PingPong,
    enabled = echo.Enabled,
    listener = Sound.Listener.Position,
    forward = Sound.Listener.Forward,
    master = Sound.Volume,
    faded = gain.Volume,
    clamped = (function()
        tone.Volume = 50
        return tone.Volume
    end)(),
}
local function failure(updates)
    local ok, message = pcall(Bulk.BulkUpdate, updates)
    assert(not ok, "expected a failure")
    return tostring(message)
end
errors = {
    failure({ [tone] = { Volume = "loud" } }),
    failure({ [tone] = { Loudness = 1 } }),
    failure({ [tone] = { IsPlaying = true } }),
    failure({ [speaker] = { Position = 5 } }),
    failure({ [speaker] = { RollOffMode = enum.AudioFormat.Int16 } }),
}
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(number(&results, "volume"), 0.25);
    assert!(flag(&results, "looping"));
    assert_eq!(number(&results, "speed"), 2.0);
    assert_eq!(number(&results, "loopEnd"), 0.5);
    assert!(flag(&results, "spatial"));
    let position: mlua::Vector = results.get("position").unwrap();
    assert_eq!((position.x(), position.y(), position.z()), (1.0, 2.0, 3.0));
    assert_eq!(results.get::<String>("mode").unwrap(), "Inverse");
    assert_eq!(results.get::<String>("device").unwrap(), "Simulated Headphones");
    assert_eq!(number(&results, "delay"), 0.5);
    assert_eq!(number(&results, "mix"), 0.25);
    assert!(flag(&results, "pingPong"));
    assert!(!flag(&results, "enabled"));
    let listener: mlua::Vector = results.get("listener").unwrap();
    assert_eq!((listener.x(), listener.y(), listener.z()), (4.0, 5.0, 6.0));
    let forward: mlua::Vector = results.get("forward").unwrap();
    assert_eq!(forward.z(), 1.0);
    assert_eq!(number(&results, "master"), 0.5);
    assert_eq!(number(&results, "faded"), 0.0);
    assert_eq!(number(&results, "clamped"), 10.0);
    let errors: Vec<String> = outcome.global("errors");
    let expected = [
        "Volume must be a number",
        "Loudness is not a valid member of SoundNode",
        "IsPlaying is read-only on SoundNode",
        "Position must be a vector",
        "expected an enum.RollOffMode item",
    ];
    for (error, expected) in errors.iter().zip(expected) {
        assert!(error.contains(expected), "{error:?} should contain {expected:?}");
    }
}

#[tokio::test]
async fn sound_links_follow_their_rules() {
    let (outcome, _) = run_sound(
        tone_asset(),
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Sound" })
local other = Window.new({ Title = "Other" })
sleep(60)
local Sound = window:GetAPI("Sound")
local OtherSound = other:GetAPI("Sound")
local tone = Sound:SoundNode("tone.wav")
local speaker = Sound:ToSpeaker()
local echo = Sound:Modifier("Echo")
local gain = Sound:Modifier("Gain")
local elsewhere = OtherSound:ToSpeaker()
local function message(action)
    local ok, err = pcall(action)
    assert(not ok, "expected an error")
    return tostring(err)
end
results = {
    linked = tone.Input:Link(echo.Output),
    again = echo.Output:Link(tone.Input),
    shortcut = echo.Input:Link(speaker),
    inputType = typeof(tone.Input),
    outputType = typeof(speaker.Output),
    owner = speaker.Output.Node == speaker,
    isLinked = tone.Input:IsLinked(echo.Output),
    anyLinked = speaker.Output:IsLinked(),
    links = #echo.Output:GetLinks(),
    linkedPort = echo.Output:GetLinks()[1] == tone.Input,
    noInput = message(function()
        return speaker.Input
    end),
    sameSide = message(function()
        tone.Input:Link(echo.Input)
    end),
    loop = message(function()
        echo.Input:Link(gain.Output)
        gain.Input:Link(echo.Output)
    end),
    self = message(function()
        echo.Input:Link(echo.Output)
    end),
    otherWindow = message(function()
        tone.Input:Link(elsewhere.Output)
    end),
    noOutput = message(function()
        tone.Input:Link(tone)
    end),
}
results.unlinked = speaker.Output:Unlink(echo.Input)
results.stillLinked = speaker.Output:IsLinked()
results.unlinkAll = tone.Input:Unlink()
results.nodes = #Sound:GetNodes()
gain:Destroy()
results.afterDestroy = #Sound:GetNodes()
results.echoLinks = #echo.Output:GetLinks()
window:Close()
other:Close()
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(flag(&results, "linked"));
    assert!(!flag(&results, "again"));
    assert!(flag(&results, "shortcut"));
    assert_eq!(results.get::<String>("inputType").unwrap(), "NodeInput");
    assert_eq!(results.get::<String>("outputType").unwrap(), "NodeOutput");
    assert!(flag(&results, "owner"));
    assert!(flag(&results, "isLinked"));
    assert!(flag(&results, "anyLinked"));
    assert_eq!(number(&results, "links"), 1.0);
    assert!(flag(&results, "linkedPort"));
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert!(text("noInput").contains("Input is not a valid member of ToSpeaker"));
    assert!(text("sameSide").contains("an Input can only link to an Output"));
    assert!(text("loop").contains("loop back into itself"));
    assert!(text("self").contains("cannot link to itself"));
    assert!(text("otherWindow").contains("different windows"));
    assert!(text("noOutput").contains("SoundNode has no Output"));
    assert!(flag(&results, "unlinked"));
    assert!(!flag(&results, "stillLinked"));
    assert!(flag(&results, "unlinkAll"));
    assert_eq!(number(&results, "nodes"), 4.0);
    assert_eq!(number(&results, "afterDestroy"), 3.0);
    assert_eq!(number(&results, "echoLinks"), 0.0);
}

#[tokio::test]
async fn opus_sounds_decode_and_play() {
    let (outcome, _) = run_sound(
        vec![("assets/voice.opus", opus_tone(375.0, 0.5, 0.5))],
        &script(
            r#"
local voice = Sound:SoundNode("voice")
local speaker = Sound:ToSpeaker()
voice.Input:Link(speaker.Output)
results = { length = voice.Length, rate = voice.SampleRate, channels = voice.Channels, asset = voice.Asset }
takeSpeaker()
voice:Play()
sleep(250)
results.peak = takeSpeaker().peak
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let length = number(&results, "length");
    assert!((length - 0.5).abs() < 0.03, "length {length}");
    assert_eq!(number(&results, "rate"), 48_000.0);
    assert_eq!(number(&results, "channels"), 1.0);
    assert_eq!(results.get::<String>("asset").unwrap(), "voice.opus");
    let peak = number(&results, "peak");
    assert!(peak > 0.4 && peak < 0.6, "peak {peak}");
}

#[tokio::test]
async fn sounds_load_from_strings_buffers_and_many_formats() {
    let stereo: Vec<f32> = (0..4800)
        .flat_map(|frame| {
            let value = (0.25 * (TAU * 375.0 * f64::from(frame) / f64::from(RATE)).sin()) as f32;
            [value, -value]
        })
        .collect();
    let (outcome, _) = run_sound(
        vec![("assets/stereo.wav", wav(2, &stereo)), ("assets/broken.wav", b"not a sound".to_vec())],
        &script(
            r#"
local Asset = import("Asset")
local data = Asset.LoadString("stereo.wav")
local fromString = Sound:FromString(data)
local fromBuffer = Sound:FromString(buffer.fromstring(data))
local fromAsset = Sound:SoundNode(Asset.Load("stereo.wav"))
results = {
    class = fromString.ClassName,
    channels = fromString.Channels,
    length = fromString.Length,
    asset = fromString.Asset,
    bufferLength = fromBuffer.Length,
    assetName = fromAsset.Asset,
    broken = tostring(select(2, pcall(function()
        Sound:SoundNode("broken.wav")
    end))),
    missing = tostring(select(2, pcall(function()
        Sound:SoundNode("missing.wav")
    end))),
}
local speaker = Sound:ToSpeaker()
fromString.Input:Link(speaker.Output)
fromString:Play()
sleep(60)
local heard = takeSpeaker()
results.left = heard.left
results.right = heard.right
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<String>("class").unwrap(), "FromString");
    assert_eq!(number(&results, "channels"), 2.0);
    assert!((number(&results, "length") - 0.1).abs() < 0.001);
    assert!(results.get::<Option<String>>("asset").unwrap().is_none());
    assert!((number(&results, "bufferLength") - 0.1).abs() < 0.001);
    assert_eq!(results.get::<String>("assetName").unwrap(), "stereo.wav");
    assert!(results.get::<String>("broken").unwrap().contains("cannot load sound 'broken.wav'"));
    assert!(results.get::<String>("missing").unwrap().contains("cannot load asset"));
    assert!(number(&results, "left") > 0.2);
    assert!(number(&results, "right") > 0.2);
}

#[tokio::test]
async fn a_sideloaded_asset_plays_like_one_from_the_assets_folder() {
    let (outcome, _) = run_sound(
        vec![("assets/tone.wav", tone(375.0, 0.4, 0.5))],
        &script(
            r#"
local Asset = import("Asset")

local bytes = Asset.LoadString("tone.wav")
local sideloaded = Asset.FromBytes("copy.wav", bytes)

local node = Sound:SoundNode(sideloaded, { Name = "Sideloaded" })
local speaker = Sound:ToSpeaker()
node.Input:Link(speaker.Output)

results = {
    class = node.ClassName,
    name = node.Name,
    channels = node.Channels,
    rate = node.SampleRate,
    seconds = node.Length,
    assetSize = sideloaded.Size,
}
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<String>("class").unwrap(), "SoundNode");
    assert_eq!(results.get::<String>("name").unwrap(), "Sideloaded");
    assert_eq!(results.get::<f64>("channels").unwrap(), 1.0);
    assert!(results.get::<f64>("assetSize").unwrap() > 100.0);
    let seconds = results.get::<f64>("seconds").unwrap();
    assert!((seconds - 0.4).abs() < 0.05, "expected about 0.4 seconds, got {seconds}");
}

#[tokio::test]
async fn the_sound_api_reports_dropped_audio_blocks() {
    let (outcome, _) = run_sound(
        tone_asset(),
        &script(
            r#"
local node = Sound:SoundNode("tone.wav", { Looping = true })
node.Input:Link(Sound:ToSpeaker().Output)
node:Play()
sleep(60)
results = {
    late = Sound.LateBlocks,
    skipped = Sound.SkippedBlocks,
    lateKind = type(Sound.LateBlocks),
    skippedKind = type(Sound.SkippedBlocks),
}
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<String>("lateKind").unwrap(), "number");
    assert_eq!(results.get::<String>("skippedKind").unwrap(), "number");
    assert_eq!(results.get::<f64>("late").unwrap(), 0.0, "a healthy run drops no blocks");
    assert_eq!(results.get::<f64>("skipped").unwrap(), 0.0);
}

#[tokio::test]
async fn a_speaker_shapes_what_it_sends_to_the_device() {
    let (outcome, _) = run_sound(
        vec![
            ("assets/sided.wav", sided(0.5, 0.6, 0.2)),
            ("assets/low.wav", tone(120.0, 0.5, 0.3)),
            ("assets/high.wav", tone(9000.0, 0.5, 0.3)),
        ],
        &script(
            r#"
local function play(file: string, settings)
    local node = Sound:SoundNode(file, { Looping = true })
    local speaker = Sound:ToSpeaker(settings)
    node.Input:Link(speaker.Output)
    node:Play()
    sleep(50)
    takeSpeaker()
    sleep(90)
    local taken = takeSpeaker()
    node:Destroy()
    speaker:Destroy()
    sleep(70)
    takeSpeaker()
    return taken
end

local plain = play("sided.wav", nil)
local mono = play("sided.wav", { Channels = 1 })
local hardLeft = play("sided.wav", { Balance = -1 })
local hardRight = play("sided.wav", { Balance = 1 })

local flatLow = play("low.wav", nil)
local liftedLow = play("low.wav", { Bass = 12 })
local flatHigh = play("high.wav", nil)
local cutHigh = play("high.wav", { Treble = -24 })

local speaker = Sound:ToSpeaker()
results = {
    channels = speaker.Channels,
    balance = speaker.Balance,
    bass = speaker.Bass,
    treble = speaker.Treble,
    plainLeft = plain.left,
    plainRight = plain.right,
    monoLeft = mono.left,
    monoRight = mono.right,
    leftOnly = hardLeft.right,
    leftKept = hardLeft.left,
    rightOnly = hardRight.left,
    rightKept = hardRight.right,
    flatLow = flatLow.rms,
    liftedLow = liftedLow.rms,
    flatHigh = flatHigh.rms,
    cutHigh = cutHigh.rms,
    clamped = (function() speaker.Channels = 5 return speaker.Channels end)(),
}
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let number = |key: &str| results.get::<f64>(key).unwrap();

    assert_eq!(number("channels"), 2.0);
    assert_eq!(number("balance"), 0.0);
    assert_eq!(number("bass"), 0.0);
    assert_eq!(number("treble"), 0.0);

    assert!(
        number("plainLeft") > number("plainRight") * 2.0,
        "the source is louder on the left, got {} and {}",
        number("plainLeft"),
        number("plainRight")
    );
    let (left, right) = (number("monoLeft"), number("monoRight"));
    assert!((left - right).abs() < 0.01, "Channels = 1 should match both sides, got {left} and {right}");

    assert!(number("leftOnly") < 0.02, "Balance = -1 should silence the right, got {}", number("leftOnly"));
    assert!(number("leftKept") > 0.3);
    assert!(number("rightOnly") < 0.02, "Balance = 1 should silence the left, got {}", number("rightOnly"));
    assert!(number("rightKept") > 0.1);

    assert!(
        number("liftedLow") > number("flatLow") * 1.5,
        "Bass = 12 should lift a 120 Hz tone, got {} against {}",
        number("liftedLow"),
        number("flatLow")
    );
    assert!(
        number("cutHigh") < number("flatHigh") * 0.5,
        "Treble = -24 should cut a 9 kHz tone, got {} against {}",
        number("cutHigh"),
        number("flatHigh")
    );
    assert_eq!(number("clamped"), 2.0, "Channels holds to 1 or 2");
}

#[tokio::test]
async fn the_new_modifiers_shape_the_sound() {
    let (outcome, _) = run_sound(
        vec![
            ("assets/tone.wav", tone(375.0, 1.0, 0.5)),
            ("assets/quiet.wav", tone(375.0, 2.0, 0.05)),
        ],
        &script(
            r#"
local function through(asset, chain, seconds)
    local node = Sound:SoundNode(asset, { Looping = true })
    local speaker = Sound:ToSpeaker()
    local previous = node
    for _, modifier in chain do
        previous.Input:Link(modifier.Output)
        previous = modifier
    end
    previous.Input:Link(speaker.Output)
    node:Play()
    sleep(seconds * 1000)
    takeSpeaker()
    sleep(150)
    local measured = takeSpeaker()
    node:Destroy()
    speaker:Destroy()
    for _, modifier in chain do
        modifier:Destroy()
    end
    sleep(40)
    takeSpeaker()
    return measured
end

local kinds = {
    "AllPass", "DcBlock", "SoftClip", "AutoGain", "Expander", "Exciter", "AutoWah", "Haas", "AutoPan",
    "Transient", "Spectrum",
}
for _, kind in kinds do
    local modifier = Sound:Modifier(kind)
    assert(modifier.ClassName == kind, kind)
    assert(modifier.Enabled == true, kind)
    modifier:Destroy()
end

results = {}
results.plain = through("tone.wav", {}, 0.1).peak
results.clipped = through("tone.wav", { Sound:Modifier("SoftClip", { Ceiling = -12, Knee = 0 }) }, 0.1).peak

local autoGain = Sound:Modifier("AutoGain", { Target = -6, Speed = 0.1 })
results.quietPlain = through("quiet.wav", {}, 0.1).peak
local node = Sound:SoundNode("quiet.wav", { Looping = true })
local speaker = Sound:ToSpeaker()
node.Input:Link(autoGain.Output)
autoGain.Input:Link(speaker.Output)
node:Play()
sleep(900)
takeSpeaker()
sleep(150)
results.lifted = takeSpeaker().peak
results.currentGain = autoGain.CurrentGain
node:Destroy()
speaker:Destroy()
sleep(40)
takeSpeaker()

results.expanded = through("quiet.wav", { Sound:Modifier("Expander", { Threshold = -10, Ratio = 4 }) }, 0.2).peak

local spectrum = Sound:Modifier("Spectrum", { Bands = 10 })
local listened = Sound:SoundNode("tone.wav", { Looping = true })
local out = Sound:ToSpeaker()
listened.Input:Link(spectrum.Output)
spectrum.Input:Link(out.Output)
listened:Play()
sleep(300)
local levels = spectrum:GetLevels()
local frequencies = spectrum:GetFrequencies()
results.bandCount = #levels
results.frequencyCount = #frequencies
local loudest, loudestIndex = 0, 0
for index, level in levels do
    if level > loudest then
        loudest, loudestIndex = level, index
    end
end
results.loudestFrequency = frequencies[loudestIndex]
local rising = true
for index = 2, #frequencies do
    rising = rising and frequencies[index] > frequencies[index - 1]
end
results.rising = rising
listened:Destroy()
out:Destroy()
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let plain = number(&results, "plain");
    assert!(plain > 0.45, "the plain tone should reach the speaker, got {plain}");
    let clipped = number(&results, "clipped");
    assert!(clipped < 0.26, "SoftClip at -12 dB should hold the peak near 0.25, got {clipped}");
    assert!(clipped > 0.2, "SoftClip should not silence the tone, got {clipped}");
    let lifted = number(&results, "lifted");
    let quiet = number(&results, "quietPlain");
    assert!(lifted > quiet * 3.0, "AutoGain should lift a quiet tone, got {lifted} from {quiet}");
    assert!(number(&results, "currentGain") > 6.0, "AutoGain should report the gain it uses");
    let expanded = number(&results, "expanded");
    assert!(expanded < quiet * 0.5, "Expander should push a quiet tone down, got {expanded} from {quiet}");
    assert_eq!(number(&results, "bandCount"), 10.0);
    assert_eq!(number(&results, "frequencyCount"), 10.0);
    assert!(flag(&results, "rising"), "the band frequencies should rise");
    let loudest = number(&results, "loudestFrequency");
    assert!(
        (200.0..700.0).contains(&loudest),
        "a 375 Hz tone should land in the band near it, got {loudest}"
    );
}

#[tokio::test]
async fn quiet_modifiers_rest_without_cutting_tails() {
    let (outcome, _) = run_sound(
        vec![
            ("assets/tone.wav", tone(375.0, 1.0, 0.5)),
            ("assets/blip.wav", tone(375.0, 0.1, 0.5)),
        ],
        &script(
            r#"
local idle = Sound:SoundNode("tone.wav")
local speaker = Sound:ToSpeaker()
local previous = idle
local chain = {}
for index = 1, 20 do
    local modifier = Sound:Modifier(if index % 2 == 0 then "LowPass" else "Gain")
    previous.Input:Link(modifier.Output)
    previous = modifier
    table.insert(chain, modifier)
end
previous.Input:Link(speaker.Output)
sleep(700)
results = {}
results.restingIdle = Sound:GetStats().RestingNodes
idle:Play()
sleep(200)
results.restingPlaying = Sound:GetStats().RestingNodes
takeSpeaker()
sleep(100)
results.playedPeak = takeSpeaker().peak
idle:Stop()
sleep(100)

local blip = Sound:SoundNode("blip.wav")
local echo = Sound:Modifier("Echo", { Delay = 0.4, Feedback = 0, Mix = 1 })
local out = Sound:ToSpeaker()
blip.Input:Link(echo.Output)
echo.Input:Link(out.Output)
sleep(700)
blip:Play()
sleep(250)
takeSpeaker()
sleep(300)
results.echoPeak = takeSpeaker().peak

local fader = Sound:Modifier("Gain")
local faded = Sound:SoundNode("tone.wav")
local third = Sound:ToSpeaker()
faded.Input:Link(fader.Output)
fader.Input:Link(third.Output)
sleep(700)
fader:Fade(0, 0.3)
sleep(600)
takeSpeaker()
faded:Play()
sleep(100)
takeSpeaker()
sleep(100)
results.fadedPeak = takeSpeaker().peak
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(
        number(&results, "restingIdle") >= 20.0,
        "a chain with nothing playing into it should rest, got {}",
        number(&results, "restingIdle")
    );
    assert!(
        number(&results, "restingPlaying") < number(&results, "restingIdle"),
        "a chain with sound going through should wake up"
    );
    assert!(number(&results, "playedPeak") > 0.4, "a woken chain should pass the sound");
    assert!(
        number(&results, "echoPeak") > 0.2,
        "an echo that arrives after the sound ends must not be cut, got {}",
        number(&results, "echoPeak")
    );
    assert!(
        number(&results, "fadedPeak") < 0.01,
        "a fade that ran while the chain was quiet must still finish, got {}",
        number(&results, "fadedPeak")
    );
}

#[tokio::test]
async fn baking_renders_a_sound_through_its_modifiers_once() {
    let (outcome, _) = run_sound(
        vec![("assets/tone.wav", tone(375.0, 1.0, 0.5))],
        &script(
            r#"
results = {}
local halved = Sound:Bake("tone.wav", { Modifiers = { { Kind = "Gain", Volume = 0.5 } } })
results.class = halved.ClassName
results.halvedPeak = halved.Peak
results.halvedDuration = halved.Duration
results.halvedChannels = halved.Channels
results.halvedRate = halved.SampleRate
results.rate = Sound.SampleRate
results.memory = halved.Memory
results.source = halved.Source

local loud = Sound:Bake("tone.wav", { Normalize = -6 })
results.normalPeak = loud.Peak

local echoed = Sound:Bake("tone.wav", {
    Modifiers = { { Kind = "Echo", Delay = 0.3, Feedback = 0, Mix = 0.5 } },
    Tail = 1,
})
results.echoDuration = echoed.Duration

local panned = Sound:Bake("tone.wav", { Modifiers = { { Kind = "Pan", Pan = -1 } } })
results.pannedChannels = panned.Channels

local cut = Sound:Bake("tone.wav", { Length = 0.25 })
results.cutDuration = cut.Duration

local mono = Sound:Bake("tone.wav", { Modifiers = { { Kind = "Pan", Pan = -1 } }, Channels = 1 })
results.forcedChannels = mono.Channels

local node = Sound:SoundNode(halved, { Looping = true })
node.Input:Link(Sound:ToSpeaker().Output)
node:Play()
sleep(100)
takeSpeaker()
sleep(150)
results.playedPeak = takeSpeaker().peak
results.nodeLength = node.Length
node:Destroy()

local bytes = halved:GetBytes()
results.bytesKind = typeof(bytes)
results.header = buffer.readstring(bytes, 0, 4)
local reloaded = Sound:FromString(bytes)
results.reloadedLength = reloaded.Length
reloaded:Destroy()

local function fails(config)
    local ok, problem = pcall(function()
        return Sound:Bake("tone.wav", config)
    end)
    return if ok then "no error" else tostring(problem)
end
results.badKind = fails({ Modifiers = { { Kind = "Nope" } } })
results.noKind = fails({ Modifiers = { { Volume = 1 } } })
results.badField = fails({ Modifiers = { { Kind = "Gain", Loudness = 2 } } })
results.badChannels = fails({ Channels = 3 })

halved:Destroy()
local ok = pcall(function()
    return halved.Duration
end)
results.destroyedReads = ok
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(text("class"), "BakedSound");
    let peak = number(&results, "halvedPeak");
    assert!((peak - 0.25).abs() < 0.01, "a Gain of 0.5 should halve a 0.5 tone, got {peak}");
    let duration = number(&results, "halvedDuration");
    assert!((duration - 1.0).abs() < 0.01, "the bake should keep the length, got {duration}");
    assert_eq!(number(&results, "halvedChannels"), 1.0, "a sound that is the same on both sides is kept as one");
    assert_eq!(number(&results, "halvedRate"), number(&results, "rate"));
    assert!(number(&results, "memory") > 90_000.0);
    assert_eq!(text("source"), "tone.wav");
    let normal = number(&results, "normalPeak");
    assert!((normal - 0.501).abs() < 0.01, "Normalize = -6 should land on 0.5, got {normal}");
    let echoed = number(&results, "echoDuration");
    assert!(
        (1.25..1.4).contains(&echoed),
        "the echo tail should be kept and the silence after it trimmed, got {echoed}"
    );
    assert_eq!(number(&results, "pannedChannels"), 2.0);
    let cut = number(&results, "cutDuration");
    assert!((cut - 0.25).abs() < 0.01, "Length should cut the source, got {cut}");
    assert_eq!(number(&results, "forcedChannels"), 1.0);
    let played = number(&results, "playedPeak");
    assert!((played - 0.25).abs() < 0.02, "a baked sound should play as it was baked, got {played}");
    assert!((number(&results, "nodeLength") - 1.0).abs() < 0.01);
    assert_eq!(text("bytesKind"), "buffer");
    assert_eq!(text("header"), "RIFF");
    assert!((number(&results, "reloadedLength") - 1.0).abs() < 0.01);
    assert!(text("badKind").contains("'Nope' is not a sound modifier"), "{}", text("badKind"));
    assert!(text("noKind").contains("needs a Kind"), "{}", text("noKind"));
    assert!(text("badField").contains("Loudness is not a valid member of Gain"), "{}", text("badField"));
    assert!(text("badChannels").contains("Channels must be 1 or 2"), "{}", text("badChannels"));
    assert!(!flag(&results, "destroyedReads"));
}

#[tokio::test]
async fn the_sound_api_reports_load_peaks_and_clipping() {
    let (outcome, _) = run_sound(
        vec![("assets/loud.wav", tone(375.0, 1.0, 1.0))],
        &script(
            r#"
results = {}
local node = Sound:SoundNode("loud.wav", { Looping = true })
local boost = Sound:Modifier("Gain", { Volume = 3 })
node.Input:Link(boost.Output)
boost.Input:Link(Sound:ToSpeaker().Output)
Sound:ResetStats()
node:Play()
sleep(300)
local stats = Sound:GetStats()
results.load = stats.Load
results.busiest = stats.BusiestBlock
results.peak = stats.Peak
results.clipped = stats.ClippedBlocks
results.nodes = stats.Nodes
results.voices = stats.Voices
results.loadField = Sound.Load
results.peakField = Sound.Peak
results.clippedField = Sound.ClippedBlocks
node:Stop()
sleep(100)
Sound:ResetStats()
local cleared = Sound:GetStats()
results.clearedPeak = cleared.Peak
results.clearedClipped = cleared.ClippedBlocks
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let load = number(&results, "load");
    assert!(load > 0.0 && load < 1.0, "the load should be a share of the budget, got {load}");
    assert!(number(&results, "busiest") >= load * 0.5);
    assert!(number(&results, "peak") > 2.5, "the peak should be read before the clipper, got {}", number(&results, "peak"));
    assert!(number(&results, "clipped") > 10.0, "a tone at three times full scale should clip");
    assert!(number(&results, "nodes") >= 3.0);
    assert_eq!(number(&results, "voices"), 1.0);
    assert!(number(&results, "loadField") > 0.0);
    assert!(number(&results, "peakField") > 2.5);
    assert!(number(&results, "clippedField") > 10.0);
    assert!(number(&results, "clearedPeak") < 0.01, "ResetStats should clear the peak");
    assert_eq!(number(&results, "clearedClipped"), 0.0);
}

#[tokio::test]
async fn max_voices_steals_the_oldest_and_respects_priority() {
    let (outcome, _) = run_sound(
        tone_asset(),
        &script(
            r#"
results = {}
Sound.MaxVoices = 2
local first = Sound:SoundNode("tone.wav", { Looping = true })
local second = Sound:SoundNode("tone.wav", { Looping = true })
local third = Sound:SoundNode("tone.wav", { Looping = true })
local stopped = {}
first.Stopped:BindHandler("log", function()
    table.insert(stopped, "first")
end)
results.firstPlayed = first:Play()
sleep(10)
results.secondPlayed = second:Play()
sleep(10)
results.thirdPlayed = third:Play()
results.voices = Sound.Voices
results.firstPlaying = first.IsPlaying
results.thirdPlaying = third.IsPlaying
results.stoppedFirst = stopped[1] == "first"
results.restarted = third:Play()
results.voicesAfterRestart = Sound.Voices

local boss = Sound:SoundNode("tone.wav", { Looping = true, Priority = 5 })
local guard = Sound:SoundNode("tone.wav", { Looping = true, Priority = 5 })
second:Stop()
third:Stop()
boss:Play()
guard:Play()
local footstep = Sound:SoundNode("tone.wav", { Looping = true, Priority = 1 })
results.footstepPlayed = footstep:Play()
results.bossPlaying = boss.IsPlaying
results.priority = footstep.Priority

local stats = Sound:GetStats()
results.stolen = stats.StolenVoices
results.refused = stats.RefusedVoices
results.maxVoices = stats.MaxVoices

Sound.MaxVoices = 0
results.unlimited = footstep:Play()
results.allVoices = Sound.Voices
local ok = pcall(function()
    Sound.MaxVoices = -1
end)
results.negative = ok
Sound:StopAll()
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(flag(&results, "firstPlayed"));
    assert!(flag(&results, "secondPlayed"));
    assert!(flag(&results, "thirdPlayed"), "a new sound should take the place of the oldest");
    assert_eq!(number(&results, "voices"), 2.0);
    assert!(!flag(&results, "firstPlaying"), "the oldest should be stopped");
    assert!(flag(&results, "thirdPlaying"));
    assert!(flag(&results, "stoppedFirst"), "the stolen sound should fire Stopped");
    assert!(flag(&results, "restarted"), "playing a sound that already plays needs no new voice");
    assert_eq!(number(&results, "voicesAfterRestart"), 2.0);
    assert!(!flag(&results, "footstepPlayed"), "a low priority sound should not steal from higher ones");
    assert!(flag(&results, "bossPlaying"));
    assert_eq!(number(&results, "priority"), 1.0);
    assert_eq!(number(&results, "stolen"), 1.0);
    assert_eq!(number(&results, "refused"), 1.0);
    assert_eq!(number(&results, "maxVoices"), 2.0);
    assert!(flag(&results, "unlimited"));
    assert_eq!(number(&results, "allVoices"), 3.0);
    assert!(!flag(&results, "negative"));
}

#[tokio::test]
async fn preloading_decodes_sounds_ahead_of_time() {
    let (outcome, _) = run_sound(
        vec![
            ("assets/tone.wav", tone(375.0, 1.0, 0.5)),
            ("assets/short.wav", tone(375.0, 0.5, 0.5)),
        ],
        &script(
            r#"
results = {}
results.before = residentSounds()
results.seconds = Sound:Preload("tone.wav", "short.wav")
sleep(50)
results.after = residentSounds()
results.unloadedOne = Sound:Unload("short.wav")
results.unloadedRest = Sound:Unload()
results.unloadedNone = Sound:Unload()
local ok, problem = pcall(function()
    return Sound:Preload(5)
end)
results.badSource = if ok then "no error" else tostring(problem)
"#,
        ),
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!((number(&results, "seconds") - 1.5).abs() < 0.01);
    assert!(
        number(&results, "after") >= number(&results, "before") + 2.0,
        "both sounds should stay decoded, got {} then {}",
        number(&results, "before"),
        number(&results, "after")
    );
    assert_eq!(number(&results, "unloadedOne"), 1.0);
    assert_eq!(number(&results, "unloadedRest"), 1.0);
    assert_eq!(number(&results, "unloadedNone"), 0.0);
    let problem = results.get::<String>("badSource").unwrap();
    assert!(problem.contains("Preload expects an Asset, a BakedSound or an asset path"), "{problem}");
}
