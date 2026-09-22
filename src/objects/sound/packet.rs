use mlua::{MetaMethod, UserData, UserDataFields, UserDataMethods, Value};

use crate::audio::PacketData;
use crate::datatypes::EnumItem;
use crate::datatypes::enums::AUDIO_FORMAT;

pub struct AudioPacket {
    data: PacketData,
}

impl AudioPacket {
    pub const TYPE_NAME: &'static str = "AudioPacket";

    pub fn new(data: PacketData) -> Self {
        Self { data }
    }

    pub fn data(&self) -> &PacketData {
        &self.data
    }
}

impl UserData for AudioPacket {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::TYPE_NAME);
        fields.add_field_method_get("SampleRate", |_, this| Ok(this.data.rate));
        fields.add_field_method_get("Channels", |_, this| Ok(this.data.channels));
        fields.add_field_method_get("Frames", |_, this| Ok(this.data.frames));
        fields.add_field_method_get("Duration", |_, this| Ok(this.data.seconds()));
        fields.add_field_method_get("Sequence", |_, this| Ok(this.data.sequence));
        fields.add_field_method_get("Peak", |_, this| Ok(f64::from(this.data.peak)));
        fields.add_field_method_get("Loudness", |_, this| Ok(f64::from(this.data.loudness)));
        fields.add_field_method_get("Format", |lua, this| {
            EnumItem::named(AUDIO_FORMAT, this.data.format.index() as u32).map_or(Ok(Value::Nil), |item| item.canonical(lua))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("ToString", |lua, this, ()| lua.create_string(this.data.to_bytes()));
        methods.add_method("ToBuffer", |lua, this, ()| lua.create_buffer(this.data.to_bytes()));
        methods.add_method("GetSamples", |lua, this, channel: Option<u8>| {
            let samples = this.data.samples();
            let channels = usize::from(this.data.channels.max(1));
            match channel {
                None => lua.create_sequence_from(samples.into_iter().map(f64::from)),
                Some(channel) if (1..=channels).contains(&usize::from(channel)) => lua.create_sequence_from(
                    samples
                        .into_iter()
                        .skip(usize::from(channel) - 1)
                        .step_by(channels)
                        .map(f64::from),
                ),
                Some(channel) => Err(mlua::Error::runtime(format!(
                    "the packet has {channels} channel{}, channel {channel} does not exist",
                    if channels == 1 { "" } else { "s" }
                ))),
            }
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "AudioPacket({} frames, {} Hz, {} channel{})",
                this.data.frames,
                this.data.rate,
                this.data.channels,
                if this.data.channels == 1 { "" } else { "s" }
            ))
        });
        methods.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.data.frames));
    }
}
