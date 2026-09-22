use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::graphics::{POST_SHADER, QUAD_SHADER};
use crate::graphics::gpu::block_on;
use crate::graphics::protocol::{Blend, ShaderId};
use crate::graphics::reflect::{BindingKind, SampleKind, ShaderLayout};

pub fn engine_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let visibility = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
    let buffer = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let texture = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("engine"),
        entries: &[
            buffer(0, wgpu::BufferBindingType::Uniform),
            buffer(1, wgpu::BufferBindingType::Storage { read_only: true }),
            texture(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            buffer(4, wgpu::BufferBindingType::Storage { read_only: true }),
            texture(5),
        ],
    })
}

pub fn blend_state(blend: Blend, premultiplied: bool) -> Option<wgpu::BlendState> {
    use wgpu::{BlendComponent, BlendFactor, BlendOperation};
    let keep_alpha = BlendComponent {
        src_factor: BlendFactor::Zero,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    };
    match blend {
        Blend::Alpha => Some(if premultiplied {
            wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
        } else {
            wgpu::BlendState::ALPHA_BLENDING
        }),
        Blend::Additive => Some(wgpu::BlendState {
            color: BlendComponent {
                src_factor: if premultiplied { BlendFactor::One } else { BlendFactor::SrcAlpha },
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            alpha: keep_alpha,
        }),
        Blend::Multiply => Some(wgpu::BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::Dst,
                dst_factor: if premultiplied { BlendFactor::OneMinusSrcAlpha } else { BlendFactor::Zero },
                operation: BlendOperation::Add,
            },
            alpha: keep_alpha,
        }),
        Blend::Opaque => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Stage {
    pub shader: ShaderId,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CustomKey {
    pub vertex: Option<Stage>,
    pub fragment: Option<Stage>,
    pub blend: Blend,
    pub format: wgpu::TextureFormat,
    pub post: bool,
}

impl CustomKey {
    fn uses(&self, shader: ShaderId) -> bool {
        [&self.vertex, &self.fragment]
            .into_iter()
            .flatten()
            .any(|stage| stage.shader == shader)
    }
}

pub struct GroupLayout {
    pub group: u32,
    pub layout: wgpu::BindGroupLayout,
    pub bindings: Vec<(u32, BindingKind)>,
}

pub struct Custom {
    pub pipeline: wgpu::RenderPipeline,
    pub groups: Vec<GroupLayout>,
    pub backdrop: bool,
}

pub struct Pipelines {
    builtin_module: wgpu::ShaderModule,
    post_module: wgpu::ShaderModule,
    builtin_layout: wgpu::PipelineLayout,
    builtin: HashMap<(Blend, wgpu::TextureFormat), wgpu::RenderPipeline>,
    modules: HashMap<ShaderId, wgpu::ShaderModule>,
    custom: HashMap<CustomKey, Result<Arc<Custom>, String>>,
}

fn render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    vertex: (&wgpu::ShaderModule, &str),
    fragment: (&wgpu::ShaderModule, &str),
    blend: Option<wgpu::BlendState>,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("renderable"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: vertex.0,
            entry_point: Some(vertex.1),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: fragment.0,
            entry_point: Some(fragment.1),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn compatible(first: BindingKind, second: BindingKind) -> bool {
    match (first, second) {
        (BindingKind::Uniform { .. }, BindingKind::Uniform { .. }) => true,
        (BindingKind::Storage { read_only: a, .. }, BindingKind::Storage { read_only: b, .. }) => a == b,
        (first, second) => first == second,
    }
}

fn merge(first: BindingKind, second: BindingKind) -> BindingKind {
    match (first, second) {
        (BindingKind::Uniform { size: a }, BindingKind::Uniform { size: b }) => BindingKind::Uniform { size: a.max(b) },
        (BindingKind::Storage { size: a, read_only }, BindingKind::Storage { size: b, .. }) => BindingKind::Storage {
            size: a.max(b),
            read_only,
        },
        (first, _) => first,
    }
}

fn layout_entry(binding: u32, kind: BindingKind) -> wgpu::BindGroupLayoutEntry {
    let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
    let (visibility, ty) = match kind {
        BindingKind::Uniform { .. } => (
            both,
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
        ),
        BindingKind::Storage { read_only, .. } => (
            if read_only { both } else { wgpu::ShaderStages::FRAGMENT },
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
        ),
        BindingKind::Texture { sample, multisampled } => (
            both,
            wgpu::BindingType::Texture {
                sample_type: match sample {
                    SampleKind::Float => wgpu::TextureSampleType::Float { filterable: !multisampled },
                    SampleKind::Sint => wgpu::TextureSampleType::Sint,
                    SampleKind::Uint => wgpu::TextureSampleType::Uint,
                    SampleKind::Depth => wgpu::TextureSampleType::Depth,
                },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled,
            },
        ),
        BindingKind::Sampler { comparison } => (
            both,
            wgpu::BindingType::Sampler(if comparison {
                wgpu::SamplerBindingType::Comparison
            } else {
                wgpu::SamplerBindingType::Filtering
            }),
        ),
    };
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    }
}

impl Pipelines {
    pub fn new(device: &wgpu::Device, engine: &wgpu::BindGroupLayout) -> Pipelines {
        let builtin_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("renderables"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(QUAD_SHADER)),
        });
        let post_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post process"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(POST_SHADER)),
        });
        let builtin_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("renderables"),
            bind_group_layouts: &[Some(engine)],
            immediate_size: 0,
        });
        Pipelines {
            builtin_module,
            post_module,
            builtin_layout,
            builtin: HashMap::new(),
            modules: HashMap::new(),
            custom: HashMap::new(),
        }
    }

    pub fn builtin(&mut self, device: &wgpu::Device, blend: Blend, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        self.builtin
            .entry((blend, format))
            .or_insert_with(|| {
                render_pipeline(
                    device,
                    &self.builtin_layout,
                    (&self.builtin_module, "vs_main"),
                    (&self.builtin_module, "fs_main"),
                    blend_state(blend, true),
                    format,
                )
            })
            .clone()
    }

    pub fn forget(&mut self, shader: ShaderId) {
        self.modules.remove(&shader);
        self.custom.retain(|key, _| !key.uses(shader));
    }

    fn module(&mut self, device: &wgpu::Device, id: ShaderId, layout: &ShaderLayout) -> wgpu::ShaderModule {
        self.modules
            .entry(id)
            .or_insert_with(|| {
                device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&layout.name),
                    source: wgpu::ShaderSource::Naga(Cow::Owned((*layout.module).clone())),
                })
            })
            .clone()
    }

    pub fn custom(
        &mut self,
        device: &wgpu::Device,
        engine: &wgpu::BindGroupLayout,
        key: &CustomKey,
        shaders: &HashMap<ShaderId, Arc<ShaderLayout>>,
    ) -> Result<Arc<Custom>, String> {
        if let Some(result) = self.custom.get(key) {
            return result.clone();
        }
        let result = self.create_custom(device, engine, key, shaders).map(Arc::new);
        self.custom.insert(key.clone(), result.clone());
        result
    }

    fn create_custom(
        &mut self,
        device: &wgpu::Device,
        engine: &wgpu::BindGroupLayout,
        key: &CustomKey,
        shaders: &HashMap<ShaderId, Arc<ShaderLayout>>,
    ) -> Result<Custom, String> {
        let lookup = |stage: &Option<Stage>| -> Result<Option<Arc<ShaderLayout>>, String> {
            match stage {
                Some(stage) => shaders
                    .get(&stage.shader)
                    .cloned()
                    .map(Some)
                    .ok_or_else(|| "a loaded shader is no longer available".to_owned()),
                None => Ok(None),
            }
        };
        let vertex_layout = lookup(&key.vertex)?;
        let fragment_layout = lookup(&key.fragment)?;

        let vertex_module = match (&key.vertex, &vertex_layout) {
            (Some(stage), Some(layout)) => self.module(device, stage.shader, layout),
            _ if key.post => self.post_module.clone(),
            _ => self.builtin_module.clone(),
        };
        let fragment_module = match (&key.fragment, &fragment_layout) {
            (Some(stage), Some(layout)) => self.module(device, stage.shader, layout),
            _ => self.builtin_module.clone(),
        };
        let vertex_entry = key
            .vertex
            .as_ref()
            .map_or(if key.post { "vs_post" } else { "vs_main" }, |stage| stage.entry.as_str());
        let fragment_entry = key.fragment.as_ref().map_or("fs_main", |stage| stage.entry.as_str());

        let mut groups: BTreeMap<u32, BTreeMap<u32, (BindingKind, String)>> = BTreeMap::new();
        for layout in [&vertex_layout, &fragment_layout].into_iter().flatten() {
            for (_, binding) in layout.data_bindings() {
                let slots = groups.entry(binding.group).or_default();
                match slots.get_mut(&binding.binding) {
                    Some((kind, owner)) => {
                        if !compatible(*kind, binding.kind) {
                            return Err(format!(
                                "shaders '{owner}' and '{}' both use @group({}) @binding({}) but one declares {} and the other {}",
                                layout.name,
                                binding.group,
                                binding.binding,
                                kind.describe(),
                                binding.kind.describe()
                            ));
                        }
                        *kind = merge(*kind, binding.kind);
                    }
                    None => {
                        slots.insert(binding.binding, (binding.kind, layout.name.clone()));
                    }
                }
            }
        }

        let group_layouts: Vec<GroupLayout> = groups
            .iter()
            .map(|(group, slots)| {
                let entries: Vec<_> = slots.iter().map(|(binding, (kind, _))| layout_entry(*binding, *kind)).collect();
                GroupLayout {
                    group: *group,
                    layout: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("renderable data"),
                        entries: &entries,
                    }),
                    bindings: slots.iter().map(|(binding, (kind, _))| (*binding, *kind)).collect(),
                }
            })
            .collect();
        let highest = group_layouts.iter().map(|group| group.group).max().unwrap_or(0);
        let mut layouts: Vec<Option<&wgpu::BindGroupLayout>> = vec![Some(engine)];
        for group in 1..=highest {
            layouts.push(group_layouts.iter().find(|layout| layout.group == group).map(|layout| &layout.layout));
        }

        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("renderable"),
            bind_group_layouts: &layouts,
            immediate_size: 0,
        });
        let pipeline = render_pipeline(
            device,
            &pipeline_layout,
            (&vertex_module, vertex_entry),
            (&fragment_module, fragment_entry),
            blend_state(key.blend, false),
            key.format,
        );
        if let Some(error) = block_on(scope.pop()) {
            let names: Vec<&str> = [&vertex_layout, &fragment_layout]
                .into_iter()
                .flatten()
                .map(|layout| layout.name.as_str())
                .collect();
            return Err(format!("cannot draw with shader '{}': {error}", names.join("' and '")));
        }

        let backdrop = vertex_layout.as_ref().is_some_and(|layout| layout.vertex_backdrop)
            || fragment_layout.as_ref().is_some_and(|layout| layout.fragment_backdrop);
        Ok(Custom {
            pipeline,
            groups: group_layouts,
            backdrop,
        })
    }
}
