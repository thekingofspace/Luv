use std::sync::Arc;

use wgpu::naga::proc::{Layouter, TypeLayout};
use wgpu::naga::valid::ModuleInfo;
use wgpu::naga::{
    AddressSpace, ArraySize, Handle, ImageClass, ImageDimension, Module, ScalarKind, ShaderStage, StorageAccess, Type,
    TypeInner,
};

pub const ENGINE_GROUP: u32 = 0;
pub const MAX_GROUP: u32 = 3;
pub const OBJECTS_BINDING: u32 = 4;
pub const BACKDROP_BINDING: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SampleKind {
    Float,
    Sint,
    Uint,
    Depth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingKind {
    Uniform { size: u32 },
    Storage { size: u32, read_only: bool },
    Texture { sample: SampleKind, multisampled: bool },
    Sampler { comparison: bool },
}

impl BindingKind {
    pub fn is_buffer(self) -> bool {
        matches!(self, BindingKind::Uniform { .. } | BindingKind::Storage { .. })
    }

    pub fn describe(self) -> &'static str {
        match self {
            BindingKind::Uniform { .. } => "a uniform buffer",
            BindingKind::Storage { .. } => "a storage buffer",
            BindingKind::Texture { .. } => "a texture",
            BindingKind::Sampler { .. } => "a sampler",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub name: String,
    pub group: u32,
    pub binding: u32,
    pub kind: BindingKind,
    pub ty: Handle<Type>,
    pub runtime: Option<(u32, u32)>,
}

#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub binding: usize,
    pub offset: u32,
    pub ty: Handle<Type>,
}

pub struct ShaderLayout {
    pub name: String,
    pub module: Arc<Module>,
    layouter: Layouter,
    pub bindings: Vec<Binding>,
    pub vertex: Option<Arc<str>>,
    pub fragment: Option<Arc<str>>,
    pub vertex_backdrop: bool,
    pub fragment_backdrop: bool,
}

fn runtime_array(module: &Module, ty: Handle<Type>) -> Option<(u32, u32)> {
    match &module.types[ty].inner {
        TypeInner::Array {
            size: ArraySize::Dynamic,
            stride,
            ..
        } => Some((0, *stride)),
        TypeInner::Struct { members, .. } => {
            let last = members.last()?;
            runtime_array(module, last.ty).map(|(offset, stride)| (last.offset + offset, stride))
        }
        _ => None,
    }
}

fn engine_binding(binding: u32, kind: BindingKind) -> bool {
    match binding {
        0 => matches!(kind, BindingKind::Uniform { .. }),
        1 => matches!(kind, BindingKind::Storage { read_only: true, .. }),
        2 => matches!(
            kind,
            BindingKind::Texture {
                sample: SampleKind::Float,
                multisampled: false
            }
        ),
        3 => matches!(kind, BindingKind::Sampler { comparison: false }),
        OBJECTS_BINDING => matches!(kind, BindingKind::Storage { read_only: true, .. }),
        BACKDROP_BINDING => matches!(
            kind,
            BindingKind::Texture {
                sample: SampleKind::Float,
                multisampled: false
            }
        ),
        _ => false,
    }
}

impl ShaderLayout {
    pub fn new(name: impl Into<String>, module: Arc<Module>, info: &ModuleInfo) -> Result<ShaderLayout, String> {
        let name = name.into();
        let mut layouter = Layouter::default();
        layouter
            .update(module.to_ctx())
            .map_err(|error| format!("{name}: cannot lay out the shader's types: {error}"))?;

        let mut bindings = Vec::new();
        let mut backdrop = None;
        for (handle, global) in module.global_variables.iter() {
            let label = global
                .name
                .clone()
                .or_else(|| module.types[global.ty].name.clone())
                .unwrap_or_default();
            if global.space == AddressSpace::Immediate {
                return Err(format!("{name}: `{label}` uses push constants, which renderables do not support"));
            }
            let Some(resource) = &global.binding else {
                continue;
            };
            let size = layouter[global.ty].size;
            let kind = match (global.space, &module.types[global.ty].inner) {
                (AddressSpace::Uniform, _) => BindingKind::Uniform { size },
                (AddressSpace::Storage { access }, _) => BindingKind::Storage {
                    size,
                    read_only: !access.contains(StorageAccess::STORE),
                },
                (AddressSpace::Handle, TypeInner::Image { dim, arrayed, class }) => {
                    if *dim != ImageDimension::D2 || *arrayed {
                        return Err(format!("{name}: `{label}` must be a 2D texture"));
                    }
                    match class {
                        ImageClass::Sampled { kind, multi } => BindingKind::Texture {
                            sample: match kind {
                                ScalarKind::Sint => SampleKind::Sint,
                                ScalarKind::Uint => SampleKind::Uint,
                                _ => SampleKind::Float,
                            },
                            multisampled: *multi,
                        },
                        ImageClass::Depth { multi } => BindingKind::Texture {
                            sample: SampleKind::Depth,
                            multisampled: *multi,
                        },
                        _ => return Err(format!("{name}: `{label}` must be a sampled texture")),
                    }
                }
                (AddressSpace::Handle, TypeInner::Sampler { comparison }) => BindingKind::Sampler {
                    comparison: *comparison,
                },
                _ => return Err(format!("{name}: `{label}` is a kind of resource renderables do not support")),
            };
            if resource.group == ENGINE_GROUP {
                if !engine_binding(resource.binding, kind) {
                    return Err(format!(
                        "{name}: `{label}` uses @group(0) @binding({}), but group 0 is reserved for the engine \
                         (0 = frame, 1 = instances, 2 = image, 3 = image_sampler, 4 = objects, 5 = backdrop)",
                        resource.binding
                    ));
                }
                if resource.binding == BACKDROP_BINDING {
                    backdrop = Some(handle);
                }
            } else if resource.group > MAX_GROUP {
                return Err(format!(
                    "{name}: `{label}` uses @group({}), but renderable data can only use groups 1 to {MAX_GROUP}",
                    resource.group
                ));
            }
            bindings.push(Binding {
                name: label,
                group: resource.group,
                binding: resource.binding,
                kind,
                ty: global.ty,
                runtime: runtime_array(&module, global.ty),
            });
        }

        let entry = |stage: ShaderStage| {
            module
                .entry_points
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.stage == stage)
                .map(|(index, entry)| {
                    let reads = backdrop.is_some_and(|handle| !info.get_entry_point(index)[handle].is_empty());
                    (entry.name.clone(), reads)
                })
        };
        let vertex = entry(ShaderStage::Vertex);
        let fragment = entry(ShaderStage::Fragment);
        Ok(ShaderLayout {
            name,
            vertex_backdrop: vertex.as_ref().is_some_and(|(_, reads)| *reads),
            fragment_backdrop: fragment.as_ref().is_some_and(|(_, reads)| *reads),
            vertex: vertex.map(|(name, _)| Arc::from(name.as_str())),
            fragment: fragment.map(|(name, _)| Arc::from(name.as_str())),
            module,
            layouter,
            bindings,
        })
    }

    pub fn type_layout(&self, ty: Handle<Type>) -> TypeLayout {
        self.layouter[ty]
    }

    pub fn data_bindings(&self) -> impl Iterator<Item = (usize, &Binding)> {
        self.bindings
            .iter()
            .enumerate()
            .filter(|(_, binding)| binding.group != ENGINE_GROUP)
    }

    fn member(&self, ty: Handle<Type>, name: &str) -> Option<(u32, Handle<Type>)> {
        match &self.module.types[ty].inner {
            TypeInner::Struct { members, .. } => members
                .iter()
                .find(|member| member.name.as_deref() == Some(name))
                .map(|member| (member.offset, member.ty)),
            _ => None,
        }
    }

    pub fn resolve(&self, path: &str) -> Result<Target, String> {
        let mut segments = path.split('.');
        let first = segments.next().unwrap_or_default();
        let rest: Vec<&str> = segments.collect();
        if first.is_empty() || rest.iter().any(|segment| segment.is_empty()) {
            return Err(format!("'{path}' is not a valid shader data name"));
        }

        let mut candidates = Vec::new();
        for (index, binding) in self.data_bindings() {
            if binding.name == first {
                candidates.push(Target {
                    binding: index,
                    offset: 0,
                    ty: binding.ty,
                });
            } else if binding.kind.is_buffer()
                && let Some((offset, ty)) = self.member(binding.ty, first)
            {
                candidates.push(Target {
                    binding: index,
                    offset,
                    ty,
                });
            }
        }
        let target = match candidates.len() {
            0 => {
                let known = self
                    .data_bindings()
                    .map(|(_, binding)| binding.name.as_str())
                    .filter(|name| !name.is_empty())
                    .collect::<Vec<_>>();
                return Err(if known.is_empty() {
                    format!("shader '{}' has no data named '{first}' because it declares no data bindings", self.name)
                } else {
                    format!("shader '{}' has no data named '{first}', it declares {}", self.name, known.join(", "))
                });
            }
            1 => candidates[0],
            _ => {
                return Err(format!(
                    "'{first}' is ambiguous in shader '{}', qualify it with the binding name like 'binding.{first}'",
                    self.name
                ));
            }
        };

        let mut target = target;
        let mut walked = first.to_owned();
        for segment in rest {
            let (offset, ty) = self
                .member(target.ty, segment)
                .ok_or_else(|| format!("'{walked}' in shader '{}' has no member '{segment}'", self.name))?;
            target.offset += offset;
            target.ty = ty;
            walked.push('.');
            walked.push_str(segment);
        }
        Ok(target)
    }
}
