use std::sync::Arc;

use wgpu::naga;

use super::combine::{self, Segment};

use naga::back::spv;
use naga::valid::{Capabilities, ModuleInfo, ValidationFlags, Validator};
use naga::{Module, ShaderStage};

pub const SPIRV_VERSION: (u8, u8) = (1, 3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Wgsl,
    Glsl,
    Spirv,
}

impl Language {
    pub fn parse(name: &str) -> Option<Language> {
        match name.to_ascii_lowercase().as_str() {
            "wgsl" => Some(Language::Wgsl),
            "glsl" => Some(Language::Glsl),
            "spirv" | "spv" | "spir-v" => Some(Language::Spirv),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Language::Wgsl => "wgsl",
            Language::Glsl => "glsl",
            Language::Spirv => "spirv",
        }
    }

    pub fn from_path(path: &str) -> Option<(Language, Option<ShaderStage>)> {
        let (_, extension) = path.rsplit_once('.')?;
        Some(match extension.to_ascii_lowercase().as_str() {
            "wgsl" => (Language::Wgsl, None),
            "glsl" => (Language::Glsl, None),
            "vert" => (Language::Glsl, Some(ShaderStage::Vertex)),
            "frag" => (Language::Glsl, Some(ShaderStage::Fragment)),
            "comp" => (Language::Glsl, Some(ShaderStage::Compute)),
            "spv" => (Language::Spirv, None),
            _ => return None,
        })
    }
}

pub fn parse_stage(name: &str) -> Option<ShaderStage> {
    match name.to_ascii_lowercase().as_str() {
        "vertex" => Some(ShaderStage::Vertex),
        "fragment" => Some(ShaderStage::Fragment),
        "compute" => Some(ShaderStage::Compute),
        "task" => Some(ShaderStage::Task),
        "mesh" => Some(ShaderStage::Mesh),
        _ => None,
    }
}

pub fn stage_name(stage: ShaderStage) -> &'static str {
    match stage {
        ShaderStage::Vertex => "vertex",
        ShaderStage::Fragment => "fragment",
        ShaderStage::Compute => "compute",
        ShaderStage::Task => "task",
        ShaderStage::Mesh => "mesh",
        ShaderStage::RayGeneration => "rayGeneration",
        ShaderStage::Miss => "miss",
        ShaderStage::AnyHit => "anyHit",
        ShaderStage::ClosestHit => "closestHit",
    }
}

#[derive(Clone, Debug)]
pub struct ShaderSource {
    pub name: String,
    pub language: Language,
    pub stage: Option<ShaderStage>,
    pub code: Vec<u8>,
    pub segments: Option<Arc<[Segment]>>,
}

fn chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(inner) = source {
        message.push_str(": ");
        message.push_str(&inner.to_string());
        source = inner.source();
    }
    message
}

#[derive(Clone, Debug)]
pub struct EntryPoint {
    pub name: String,
    pub stage: ShaderStage,
    pub workgroup_size: [u32; 3],
}

#[derive(Clone, Debug)]
pub struct CompiledShader {
    pub module: Arc<Module>,
    pub info: Arc<ModuleInfo>,
    pub spirv: Arc<[u32]>,
    pub entry_points: Vec<EntryPoint>,
}

pub fn compile(source: &ShaderSource) -> Result<CompiledShader, String> {
    let name = source.name.as_str();
    let text = match source.language {
        Language::Spirv => None,
        _ => Some(std::str::from_utf8(&source.code).map_err(|_| format!("{name} is not valid UTF-8 text"))?),
    };

    let segments = source.segments.as_deref();
    let module = match (source.language, text) {
        (Language::Wgsl, Some(text)) => naga::front::wgsl::parse_str(text).map_err(|error| match segments {
            Some(segments) => combine::describe(
                name,
                text,
                segments,
                error.location(text).map(|location| location.offset as usize),
                error.message(),
            ),
            None => error.emit_to_string_with_path(text, name),
        })?,
        (Language::Glsl, Some(text)) => {
            let stage = source.stage.ok_or_else(|| {
                format!("{name} is GLSL, so it needs a Stage of \"vertex\", \"fragment\" or \"compute\"")
            })?;
            naga::front::glsl::Frontend::default()
                .parse(&naga::front::glsl::Options::from(stage), text)
                .map_err(|errors| match segments {
                    Some(segments) => errors
                        .errors
                        .iter()
                        .map(|error| {
                            combine::describe(
                                name,
                                text,
                                segments,
                                error.location(text).map(|location| location.offset as usize),
                                &error.kind.to_string(),
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    None => errors.emit_to_string_with_path(text, name),
                })?
        }
        _ => naga::front::spv::parse_u8_slice(&source.code, &naga::front::spv::Options::default())
            .map_err(|error| format!("{name}: {error}"))?,
    };

    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| match (text, segments) {
            (Some(text), Some(segments)) => combine::describe(
                name,
                text,
                segments,
                error.location(text).map(|location| location.offset as usize),
                &chain(error.as_inner()),
            ),
            (Some(text), None) => error.emit_to_string_with_path(text, name),
            (None, _) => format!("{name}: {error}"),
        })?;

    let options = spv::Options {
        lang_version: SPIRV_VERSION,
        flags: spv::WriterFlags::ADJUST_COORDINATE_SPACE
            | spv::WriterFlags::LABEL_VARYINGS
            | spv::WriterFlags::CLAMP_FRAG_DEPTH,
        ..spv::Options::default()
    };
    let spirv = spv::write_vec(&module, &info, &options, None).map_err(|error| format!("{name}: {error}"))?;

    let entry_points = module
        .entry_points
        .iter()
        .map(|entry| EntryPoint {
            name: entry.name.clone(),
            stage: entry.stage,
            workgroup_size: entry.workgroup_size,
        })
        .collect();

    Ok(CompiledShader {
        module: Arc::new(module),
        info: Arc::new(info),
        spirv: spirv.into(),
        entry_points,
    })
}
