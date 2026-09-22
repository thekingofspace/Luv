use crate::datatypes::enums::{AUDIO_FORMAT, ROLL_OFF_MODE};

const WIDE: f64 = 1.0e12;
const HEARING: f64 = 24_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Range {
    Number(f64, f64),
    Integer(f64, f64),
    Flag,
    Choice(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub name: &'static str,
    pub default: f64,
    pub range: Range,
    pub hidden: bool,
}

const fn number(name: &'static str, default: f64, min: f64, max: f64) -> Param {
    Param {
        name,
        default,
        range: Range::Number(min, max),
        hidden: false,
    }
}

const fn integer(name: &'static str, default: f64, min: f64, max: f64) -> Param {
    Param {
        name,
        default,
        range: Range::Integer(min, max),
        hidden: false,
    }
}

const fn flag(name: &'static str, default: bool) -> Param {
    Param {
        name,
        default: if default { 1.0 } else { 0.0 },
        range: Range::Flag,
        hidden: false,
    }
}

const fn choice(name: &'static str, enum_type: &'static str, default: u32) -> Param {
    Param {
        name,
        default: default as f64,
        range: Range::Choice(enum_type),
        hidden: false,
    }
}

const fn hidden(name: &'static str) -> Param {
    Param {
        name,
        default: 0.0,
        range: Range::Number(-WIDE, WIDE),
        hidden: true,
    }
}

const fn volume() -> Param {
    number("Volume", 1.0, 0.0, 10.0)
}

const fn mix(default: f64) -> Param {
    number("Mix", default, 0.0, 1.0)
}

const fn frequency(name: &'static str, default: f64) -> Param {
    number(name, default, 10.0, HEARING)
}

const fn decibels(name: &'static str) -> Param {
    number(name, 0.0, -48.0, 48.0)
}

const ENABLED: Param = flag("Enabled", true);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Player,
    Stream,
    Speaker,
    Capture,
    Modifier,
}

#[derive(Debug)]
pub struct Spec {
    pub class: &'static str,
    pub family: Family,
    pub input: bool,
    pub output: bool,
    pub params: &'static [Param],
    pub signals: &'static [&'static str],
    pub methods: &'static [&'static str],
}

impl Spec {
    pub fn param(&self, name: &str) -> Option<usize> {
        self.params.iter().position(|param| param.name == name && !param.hidden)
    }

    pub fn index(&self, name: &str) -> Option<usize> {
        self.params.iter().position(|param| param.name == name)
    }

    pub fn signal(&self, name: &str) -> Option<usize> {
        self.signals.iter().position(|signal| *signal == name)
    }

    pub fn has_method(&self, name: &str) -> bool {
        self.methods.contains(&name)
    }
}

const PLAYER_PARAMS: &[Param] = &[
    volume(),
    number("PlaybackSpeed", 1.0, 0.01, 32.0),
    flag("Looping", false),
    number("LoopStart", 0.0, 0.0, WIDE),
    number("LoopEnd", 0.0, 0.0, WIDE),
];
const PLAYER_SIGNALS: &[&str] = &["Started", "Stopped", "Paused", "Resumed", "Ended", "Looped"];
const PLAYER_METHODS: &[&str] = &["Play", "Stop", "Pause", "Resume", "PlayOneShot"];

pub const SOUND_NODE: Spec = Spec {
    class: "SoundNode",
    family: Family::Player,
    input: true,
    output: false,
    params: PLAYER_PARAMS,
    signals: PLAYER_SIGNALS,
    methods: PLAYER_METHODS,
};

pub const FROM_STRING: Spec = Spec {
    class: "FromString",
    family: Family::Player,
    input: true,
    output: false,
    params: PLAYER_PARAMS,
    signals: PLAYER_SIGNALS,
    methods: PLAYER_METHODS,
};

pub const FROM_BYTES: Spec = Spec {
    class: "FromBytes",
    family: Family::Stream,
    input: true,
    output: false,
    params: &[
        volume(),
        number("MaxBuffered", 1.0, 0.02, 60.0),
        number("Prebuffer", 0.05, 0.0, 10.0),
        integer("SampleRate", 48_000.0, 1_000.0, 384_000.0),
        integer("Channels", 2.0, 1.0, 2.0),
        choice("Format", AUDIO_FORMAT, 0),
    ],
    signals: &["Drained"],
    methods: &["Push", "Clear"],
};

pub const SPEAKER_POSITION: usize = 3;
pub const SPEAKER_DIRECTION: usize = 6;

pub const TO_SPEAKER: Spec = Spec {
    class: "ToSpeaker",
    family: Family::Speaker,
    input: false,
    output: true,
    params: &[
        volume(),
        flag("OwnedByWindow", true),
        flag("Spatial", false),
        hidden("PositionX"),
        hidden("PositionY"),
        hidden("PositionZ"),
        hidden("DirectionX"),
        hidden("DirectionY"),
        hidden("DirectionZ"),
        number("MinDistance", 50.0, 0.0, WIDE),
        number("MaxDistance", 2000.0, 0.0, WIDE),
        choice("RollOffMode", ROLL_OFF_MODE, 1),
        number("ConeInnerAngle", 360.0, 0.0, 360.0),
        number("ConeOuterAngle", 360.0, 0.0, 360.0),
        number("ConeOuterVolume", 0.0, 0.0, 1.0),
        flag("Binaural", true),
    ],
    signals: &[],
    methods: &[],
};

pub const TO_BYTES: Spec = Spec {
    class: "ToBytes",
    family: Family::Capture,
    input: false,
    output: true,
    params: &[
        flag("Enabled", true),
        integer("SampleRate", 48_000.0, 1_000.0, 384_000.0),
        integer("Channels", 2.0, 1.0, 2.0),
        choice("Format", AUDIO_FORMAT, 1),
        number("PacketDuration", 0.02, 0.0025, 1.0),
        flag("SkipSilence", false),
    ],
    signals: &["OnIncoming"],
    methods: &[],
};

const fn modifier(class: &'static str, params: &'static [Param]) -> Spec {
    Spec {
        class,
        family: Family::Modifier,
        input: true,
        output: true,
        params,
        signals: &[],
        methods: &[],
    }
}

pub const MODIFIERS: &[Spec] = &[
    Spec {
        methods: &["Fade"],
        ..modifier("Gain", &[ENABLED, volume()])
    },
    modifier("Pan", &[ENABLED, number("Pan", 0.0, -1.0, 1.0)]),
    modifier(
        "LowPass",
        &[ENABLED, frequency("Cutoff", 1000.0), number("Resonance", 0.707, 0.1, 20.0)],
    ),
    modifier(
        "HighPass",
        &[ENABLED, frequency("Cutoff", 1000.0), number("Resonance", 0.707, 0.1, 20.0)],
    ),
    modifier("BandPass", &[ENABLED, frequency("Frequency", 1000.0), number("Q", 1.0, 0.1, 40.0)]),
    modifier("Notch", &[ENABLED, frequency("Frequency", 1000.0), number("Q", 1.0, 0.1, 40.0)]),
    modifier(
        "Peak",
        &[ENABLED, frequency("Frequency", 1000.0), number("Q", 1.0, 0.1, 40.0), decibels("Gain")],
    ),
    modifier("LowShelf", &[ENABLED, frequency("Frequency", 200.0), decibels("Gain")]),
    modifier("HighShelf", &[ENABLED, frequency("Frequency", 4000.0), decibels("Gain")]),
    modifier(
        "Equalizer",
        &[
            ENABLED,
            decibels("LowGain"),
            decibels("MidGain"),
            decibels("HighGain"),
            frequency("LowFrequency", 400.0),
            frequency("HighFrequency", 4000.0),
        ],
    ),
    modifier(
        "Echo",
        &[
            ENABLED,
            number("Delay", 0.3, 0.001, 5.0),
            number("Feedback", 0.4, 0.0, 0.95),
            mix(0.5),
            flag("PingPong", false),
        ],
    ),
    modifier(
        "Reverb",
        &[
            ENABLED,
            number("RoomSize", 0.6, 0.0, 1.0),
            number("Damping", 0.5, 0.0, 1.0),
            number("Width", 1.0, 0.0, 1.0),
            mix(0.35),
            number("PreDelay", 0.02, 0.0, 0.5),
        ],
    ),
    modifier(
        "Chorus",
        &[ENABLED, number("Rate", 0.8, 0.0, 20.0), number("Depth", 0.5, 0.0, 1.0), mix(0.5)],
    ),
    modifier(
        "Flanger",
        &[
            ENABLED,
            number("Rate", 0.25, 0.0, 20.0),
            number("Depth", 0.7, 0.0, 1.0),
            number("Feedback", 0.5, -0.95, 0.95),
            mix(0.5),
        ],
    ),
    modifier(
        "Phaser",
        &[
            ENABLED,
            number("Rate", 0.5, 0.0, 20.0),
            number("Depth", 0.7, 0.0, 1.0),
            number("Feedback", 0.5, 0.0, 0.95),
            mix(0.5),
        ],
    ),
    modifier("Tremolo", &[ENABLED, number("Rate", 5.0, 0.0, 40.0), number("Depth", 0.5, 0.0, 1.0)]),
    modifier("Vibrato", &[ENABLED, number("Rate", 5.0, 0.0, 40.0), number("Depth", 0.3, 0.0, 1.0)]),
    modifier(
        "Distortion",
        &[
            ENABLED,
            number("Drive", 0.5, 0.0, 1.0),
            number("Tone", 8000.0, 200.0, 20_000.0),
            mix(1.0),
        ],
    ),
    modifier(
        "BitCrusher",
        &[
            ENABLED,
            number("Bits", 8.0, 1.0, 24.0),
            integer("Downsample", 1.0, 1.0, 64.0),
            mix(1.0),
        ],
    ),
    modifier(
        "Compressor",
        &[
            ENABLED,
            number("Threshold", -20.0, -80.0, 0.0),
            number("Ratio", 4.0, 1.0, 50.0),
            number("Attack", 0.01, 0.0, 1.0),
            number("Release", 0.1, 0.0, 5.0),
            number("MakeupGain", 0.0, -24.0, 48.0),
        ],
    ),
    modifier(
        "Limiter",
        &[ENABLED, number("Threshold", -1.0, -40.0, 0.0), number("Release", 0.05, 0.0, 5.0)],
    ),
    modifier(
        "NoiseGate",
        &[
            ENABLED,
            number("Threshold", -50.0, -100.0, 0.0),
            number("Attack", 0.005, 0.0, 1.0),
            number("Release", 0.1, 0.0, 5.0),
            number("Hold", 0.05, 0.0, 5.0),
        ],
    ),
    modifier("PitchShift", &[ENABLED, number("Pitch", 1.0, 0.25, 4.0)]),
    modifier(
        "RingModulator",
        &[ENABLED, number("Frequency", 440.0, 0.0, 20_000.0), mix(1.0)],
    ),
    modifier("StereoWidth", &[ENABLED, number("Width", 1.0, 0.0, 3.0)]),
    modifier("Meter", &[ENABLED]),
];

pub fn modifier_spec(class: &str) -> Option<&'static Spec> {
    MODIFIERS.iter().find(|spec| spec.class == class)
}
