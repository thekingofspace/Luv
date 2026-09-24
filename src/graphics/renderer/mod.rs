mod atlas;
mod pipelines;
mod query;

use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::mem::size_of;
use std::ops::Range;
use std::os::raw::{c_char, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use bytemuck::{Pod, Zeroable};

use self::atlas::{Atlas, AtlasFull, GlyphEntry, GlyphKey};
use self::pipelines::{Custom, CustomKey, Pipelines, Stage, engine_layout};
use self::query::QueryState;
use super::geometry::{self, GpuObject, Hit, NO_OBJECT, ObjectKind, Query};
use super::gpu::Gpu;
use super::hook::{self, HookFunction, RenderContext, VulkanHandles};
use super::protocol::{
    Blend, Body, Capture, FontId, FrameInfo, NativeHook, ObjectId, Release, RenderCommand, Resource, SceneDelta,
    ShaderId, SlotContent, Snapshot, TextureId, Transform,
};
use super::reflect::{BindingKind, SampleKind, ShaderLayout, Target as DataTarget};
use super::text::{Font, PlacedGlyph};
use crate::window::{LiveResize, PendingSurface, RenderTarget, WindowEvent, WindowEvents, WindowId};

pub const INSTANCE_SHAPE: u32 = 0;
pub const INSTANCE_IMAGE: u32 = 1;
pub const INSTANCE_GLYPH: u32 = 2;
pub const FLAG_PIXELATED: u32 = 1;
const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const RECTANGLE: u32 = 0;
const MAX_NATIVE_BYTES: u64 = 256 * 1024 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Instance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub anchor: [f32; 2],
    pub rotation: f32,
    pub kind: u32,
    pub color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub uv: [f32; 4],
    pub shape: u32,
    pub stroke: f32,
    pub flags: u32,
    pub object: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct FrameUniform {
    resolution: [f32; 2],
    scale: f32,
    time: f32,
    delta: f32,
    frame: u32,
    padding: [f32; 2],
}

pub type Map<K, V> = HashMap<K, V, rustc_hash::FxBuildHasher>;

pub struct RendererHandle {
    commands: Arc<Sender<RenderCommand>>,
}

impl RendererHandle {
    pub fn spawn(window: WindowId, target: RenderTarget, events: WindowEvents) -> Result<RendererHandle, String> {
        let (commands, receiver) = mpsc::channel();
        let commands = Arc::new(commands);
        if let RenderTarget::Window { resize, .. } = &target {
            let prompt = Arc::downgrade(&commands);
            resize.set_redraw(Some(Box::new(move || {
                if let Some(commands) = prompt.upgrade() {
                    let _ = commands.send(RenderCommand::Redraw);
                }
            })));
        }
        thread::Builder::new()
            .name(format!("renderer for window {window}"))
            .spawn(move || run(window, target, events, receiver))
            .map_err(|error| format!("cannot start the renderer: {error}"))?;
        Ok(RendererHandle { commands })
    }

    pub fn send(&self, command: RenderCommand) -> bool {
        self.commands.send(command).is_ok()
    }
}

fn run(window: WindowId, target: RenderTarget, events: WindowEvents, receiver: Receiver<RenderCommand>) {
    let resize = match &target {
        RenderTarget::Window { resize, .. } => Some(resize.clone()),
        RenderTarget::Offscreen => None,
    };
    let mut renderer = Renderer {
        window,
        events,
        reported: HashSet::new(),
        scene: Scene::default(),
        graphics: None,
    };
    match Gpu::get().and_then(|gpu| Graphics::new(gpu, target)) {
        Ok(graphics) => renderer.graphics = Some(graphics),
        Err(error) => renderer.report(format!("rendering is unavailable: {error}")),
    }
    let mut last = None;
    while let Ok(first) = receiver.recv() {
        let mut present = None;
        let mut redraw = false;
        let mut next = Some(first);
        while let Some(command) = next.take() {
            match command {
                RenderCommand::Delta(delta) => {
                    for error in renderer.scene.apply(delta, renderer.graphics.as_mut()) {
                        renderer.report(error);
                    }
                }
                RenderCommand::Present(frame) => present = Some(frame),
                RenderCommand::Redraw => redraw = true,
                RenderCommand::Query(query, reply) => {
                    let _ = reply.send(renderer.query(&query));
                }
                RenderCommand::Capture(frame, reply) => {
                    let _ = reply.send(renderer.capture(&frame));
                }
            }
            next = receiver.try_recv().ok();
        }
        if let Some(frame) = present.or(if redraw { last } else { None }) {
            last = Some(frame);
            renderer.present(&frame);
        }
    }
    if let Some(resize) = resize {
        resize.set_redraw(None);
    }
}

struct Grow {
    buffer: wgpu::Buffer,
    label: &'static str,
    usage: wgpu::BufferUsages,
}

impl Grow {
    fn new(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages, size: u64) -> Grow {
        Grow {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            }),
            label,
            usage,
        }
    }

    fn ensure(&mut self, device: &wgpu::Device, size: u64) -> bool {
        if self.buffer.size() >= size {
            return false;
        }
        *self = Grow::new(device, self.label, self.usage, size.next_power_of_two());
        true
    }
}

struct GpuTexture {
    view: wgpu::TextureView,
}

struct SurfaceTarget {
    window: Arc<winit::window::Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    configured: bool,
    copyable: bool,
    resize: Arc<LiveResize>,
}

impl SurfaceTarget {
    fn new(
        gpu: &Gpu,
        window: Arc<winit::window::Window>,
        surface: &PendingSurface,
        resize: Arc<LiveResize>,
    ) -> Result<SurfaceTarget, String> {
        let surface = surface.take()?;
        if !gpu.adapter.is_surface_supported(&surface) {
            return Err("the GPU cannot present to this window".to_owned());
        }
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| "the GPU cannot present to this window".to_owned())?;
        let present_mode = if capabilities.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };
        let alpha_mode = if capabilities.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            wgpu::CompositeAlphaMode::Auto
        };
        let copyable = capabilities.usages.contains(wgpu::TextureUsages::COPY_SRC);
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if copyable {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let size = window.inner_size();
        Ok(SurfaceTarget {
            window,
            surface,
            config: wgpu::SurfaceConfiguration {
                usage,
                format,
                color_space: wgpu::SurfaceColorSpace::Auto,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode,
                desired_maximum_frame_latency: 2,
                alpha_mode,
                view_formats: Vec::new(),
            },
            configured: false,
            copyable,
            resize,
        })
    }

    fn configure(&mut self, device: &wgpu::Device, size: winit::dpi::PhysicalSize<u32>) {
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(device, &self.config);
        self.configured = true;
    }

    fn acquire(&mut self, device: &wgpu::Device) -> Option<wgpu::SurfaceTexture> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        if !self.configured || self.config.width != size.width || self.config.height != size.height {
            self.configure(device, size);
        }
        for _ in 0..2 {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    return Some(texture);
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    self.configure(device, self.window.inner_size());
                }
                _ => return None,
            }
        }
        None
    }
}

enum Target {
    Surface(SurfaceTarget),
    Offscreen(Option<(wgpu::Texture, [u32; 2])>),
}

struct Backdrop {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
    format: wgpu::TextureFormat,
}

struct PostTargets {
    targets: [(wgpu::Texture, wgpu::TextureView); 2],
    groups: [Option<wgpu::BindGroup>; 2],
    size: [u32; 2],
    format: wgpu::TextureFormat,
}

#[derive(Clone)]
struct View {
    view: wgpu::TextureView,
    texture: wgpu::Texture,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    scale: f64,
    copyable: bool,
}

struct Graphics {
    gpu: Arc<Gpu>,
    target: Target,
    engine: wgpu::BindGroupLayout,
    frame: wgpu::Buffer,
    instances: Grow,
    objects: Grow,
    outlines: Grow,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    comparison: wgpu::Sampler,
    white: GpuTexture,
    textures: Map<TextureId, GpuTexture>,
    atlas: Atlas,
    backdrop: Option<Backdrop>,
    post: Option<PostTargets>,
    groups: Map<TextureSlot, wgpu::BindGroup>,
    pipelines: Pipelines,
    query: Option<QueryState>,
    texture_generation: u64,
    frame_number: u32,
}

fn create_texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn extent(size: [u32; 2]) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    }
}

impl Graphics {
    fn new(gpu: Arc<Gpu>, target: RenderTarget) -> Result<Graphics, String> {
        let target = match target {
            RenderTarget::Window { window, surface, resize } => {
                Target::Surface(SurfaceTarget::new(&gpu, window, &surface, resize)?)
            }
            RenderTarget::Offscreen => Target::Offscreen(None),
        };
        let device = &gpu.device;
        let engine = engine_layout(device);
        let frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let storage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let instances = Grow::new(device, "instances", storage, 64 * size_of::<Instance>() as u64);
        let objects = Grow::new(device, "objects", storage, 64 * size_of::<GpuObject>() as u64);
        let outlines = Grow::new(device, "outlines", storage, 64 * size_of::<[f32; 2]>() as u64);
        let sampler = |filter: wgpu::FilterMode, compare: Option<wgpu::CompareFunction>| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("renderable sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: filter,
                min_filter: filter,
                compare,
                ..wgpu::SamplerDescriptor::default()
            })
        };
        let linear = sampler(wgpu::FilterMode::Linear, None);
        let nearest = sampler(wgpu::FilterMode::Nearest, None);
        let comparison = sampler(wgpu::FilterMode::Linear, Some(wgpu::CompareFunction::LessEqual));
        let white_texture = create_texture(
            device,
            "white",
            [1, 1],
            OFFSCREEN_FORMAT,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        gpu.queue.write_texture(
            white_texture.as_image_copy(),
            &[255; 4],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            extent([1, 1]),
        );
        let white = GpuTexture {
            view: white_texture.create_view(&wgpu::TextureViewDescriptor::default()),
        };
        let atlas = Atlas::new(device);
        let pipelines = Pipelines::new(device, &engine);
        Ok(Graphics {
            target,
            engine,
            frame,
            instances,
            objects,
            outlines,
            linear,
            nearest,
            comparison,
            white,
            textures: Map::default(),
            atlas,
            backdrop: None,
            post: None,
            groups: Map::default(),
            pipelines,
            query: None,
            texture_generation: 0,
            frame_number: 0,
            gpu,
        })
    }

    fn add_texture(&mut self, id: TextureId, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
        let limit = self.gpu.device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > limit || height > limit || rgba.len() < (width * height * 4) as usize {
            return Err(format!(
                "an image of {width}x{height} cannot be drawn, images must be between 1x1 and {limit}x{limit} pixels"
            ));
        }
        let texture = create_texture(
            &self.gpu.device,
            "image",
            [width, height],
            OFFSCREEN_FORMAT,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        self.gpu.queue.write_texture(
            texture.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            extent([width, height]),
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.textures.insert(id, GpuTexture { view });
        self.texture_generation += 1;
        Ok(())
    }

    fn remove_texture(&mut self, id: TextureId) {
        if self.textures.remove(&id).is_some() {
            self.groups.remove(&TextureSlot::Image(id));
            self.texture_generation += 1;
        }
    }

    fn texture_view(&self, slot: TextureSlot) -> &wgpu::TextureView {
        match slot {
            TextureSlot::White => &self.white.view,
            TextureSlot::Atlas => &self.atlas.view,
            TextureSlot::Image(id) => self.textures.get(&id).map_or(&self.white.view, |texture| &texture.view),
        }
    }

    fn ensure_backdrop(&mut self, format: wgpu::TextureFormat, size: [u32; 2]) {
        if self
            .backdrop
            .as_ref()
            .is_some_and(|backdrop| backdrop.format == format && backdrop.size == size)
        {
            return;
        }
        let texture = create_texture(
            &self.gpu.device,
            "backdrop",
            size,
            format,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.backdrop = Some(Backdrop {
            texture,
            view,
            size,
            format,
        });
        self.groups.clear();
    }

    fn post_targets(&mut self, format: wgpu::TextureFormat, size: [u32; 2]) -> [(wgpu::Texture, wgpu::TextureView); 2] {
        if let Some(post) = &self.post
            && post.format == format
            && post.size == size
        {
            return post.targets.clone();
        }
        let create = || {
            let texture = create_texture(
                &self.gpu.device,
                "post process",
                size,
                format,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };
        let targets = [create(), create()];
        self.post = Some(PostTargets {
            targets: targets.clone(),
            groups: [None, None],
            size,
            format,
        });
        targets
    }

    fn post_group(&mut self, index: usize) -> Option<wgpu::BindGroup> {
        if let Some(post) = &self.post
            && let Some(group) = &post.groups[index]
        {
            return Some(group.clone());
        }
        let input = &self.post.as_ref()?.targets[index].1.clone();
        let group = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post process"),
            layout: &self.engine,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.frame.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.instances.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.linear),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.objects.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: self.outlines.buffer.as_entire_binding(),
                },
            ],
        });
        if let Some(post) = &mut self.post {
            post.groups[index] = Some(group.clone());
        }
        Some(group)
    }

    fn group(&mut self, slot: TextureSlot) -> wgpu::BindGroup {
        if let Some(group) = self.groups.get(&slot) {
            return group.clone();
        }
        let backdrop = self.backdrop.as_ref().map_or(&self.white.view, |backdrop| &backdrop.view);
        let group = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("engine"),
            layout: &self.engine,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.frame.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.instances.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(self.texture_view(slot)),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.linear),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.objects.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: self.outlines.buffer.as_entire_binding(),
                },
            ],
        });
        self.groups.insert(slot, group.clone());
        group
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TextureSlot {
    White,
    Image(TextureId),
    Atlas,
}

impl TextureSlot {
    fn of(body: &Body) -> TextureSlot {
        match body {
            Body::Image { texture, .. } => TextureSlot::Image(*texture),
            Body::Text { .. } => TextureSlot::Atlas,
            _ => TextureSlot::White,
        }
    }
}

enum DrawItem {
    Batch {
        blend: Blend,
        texture: TextureSlot,
        range: Range<u32>,
    },
    Custom(ObjectId),
}

struct Prepared {
    pipeline: wgpu::RenderPipeline,
    groups: Vec<(u32, wgpu::BindGroup)>,
    vertices: Range<u32>,
    instances: Range<u32>,
    backdrop: bool,
}

struct Bound {
    key: CustomKey,
    version: (u64, u64, u64),
    buffers: Map<(u32, u32), wgpu::Buffer>,
    groups: Vec<(u32, wgpu::BindGroup)>,
}

struct NativeTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
}

struct Tracked {
    snapshot: Snapshot,
    slot: u32,
    range: Range<u32>,
    data: Map<(u32, u32), SlotContent>,
    native: Map<(u32, u32), NativeTexture>,
    references: bool,
    data_version: u64,
    bound: Option<Bound>,
}

type NativeResult = Result<(), (i32, String)>;

struct HookRun {
    scene: *mut Scene,
    graphics: *mut Graphics,
    object: ObjectId,
    errors: *mut Vec<String>,
}

fn placement(body: &Body) -> [f32; 7] {
    match body {
        Body::Custom { .. } | Body::Post { .. } => [0.0; 7],
        Body::Shape { transform, .. } | Body::Image { transform, .. } | Body::Text { transform, .. } => [
            transform.position[0] as f32,
            transform.position[1] as f32,
            transform.size[0] as f32,
            transform.size[1] as f32,
            transform.anchor[0] as f32,
            transform.anchor[1] as f32,
            transform.rotation.to_degrees() as f32,
        ],
    }
}

unsafe fn hook_run<'a>(context: *mut RenderContext) -> Option<&'a mut HookRun> {
    if context.is_null() {
        return None;
    }
    let run = unsafe { (*context).engine } as *mut HookRun;
    if run.is_null() { None } else { Some(unsafe { &mut *run }) }
}

unsafe fn hook_name(name: *const c_char) -> Option<String> {
    if name.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned())
}

fn hook_outcome(run: &mut HookRun, outcome: NativeResult) -> i32 {
    match outcome {
        Ok(()) => hook::OK,
        Err((code, message)) => {
            unsafe { (*run.errors).push(message) };
            code
        }
    }
}

unsafe extern "C" fn hook_write_data(
    context: *mut RenderContext,
    name: *const c_char,
    offset: u64,
    data: *const c_void,
    length: u64,
) -> i32 {
    panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        let Some(run) = hook_run(context) else {
            return hook::INVALID;
        };
        let Some(name) = hook_name(name) else {
            return hook::INVALID;
        };
        if (data.is_null() && length > 0) || length > MAX_NATIVE_BYTES {
            return hook::INVALID;
        }
        let bytes = if length == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(data as *const u8, length as usize)
        };
        let outcome = (*run.scene).write_native(run.object, &name, offset, bytes);
        hook_outcome(run, outcome)
    }))
    .unwrap_or(hook::INVALID)
}

unsafe extern "C" fn hook_write_texture(
    context: *mut RenderContext,
    name: *const c_char,
    width: u32,
    height: u32,
    rgba: *const c_void,
) -> i32 {
    panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        let Some(run) = hook_run(context) else {
            return hook::INVALID;
        };
        let Some(name) = hook_name(name) else {
            return hook::INVALID;
        };
        let length = u64::from(width) * u64::from(height) * 4;
        if rgba.is_null() || width == 0 || height == 0 || length > MAX_NATIVE_BYTES {
            return hook::INVALID;
        }
        let pixels = std::slice::from_raw_parts(rgba as *const u8, length as usize);
        let outcome = (*run.scene).write_native_texture(&mut *run.graphics, run.object, &name, [width, height], pixels);
        hook_outcome(run, outcome)
    }))
    .unwrap_or(hook::INVALID)
}

unsafe extern "C" fn hook_set_draw_counts(context: *mut RenderContext, vertices: u32, instances: u32) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if let Some(run) = hook_run(context) {
            (*run.scene).set_draw_counts(run.object, vertices, instances);
        }
    }));
}

#[derive(Default)]
struct Scene {
    objects: Map<ObjectId, Tracked>,
    table: Vec<GpuObject>,
    free: Vec<u32>,
    table_dirty: Option<Range<usize>>,
    slot_generation: u64,
    fonts: Map<FontId, Font>,
    shaders: Map<ShaderId, Arc<ShaderLayout>>,
    structure: bool,
    hooked: usize,
    outlines: Vec<[f32; 2]>,
    outline_codes: Map<ObjectId, u32>,
    outlines_dirty: bool,
    outlines_stale: bool,
    patched: Vec<ObjectId>,
    instances: Vec<Instance>,
    dirty: Option<Range<usize>>,
    draw: Vec<DrawItem>,
    post: Vec<ObjectId>,
    scale: f64,
}

fn structural(old: &Snapshot, new: &Snapshot) -> bool {
    if old.order != new.order || old.z_index != new.z_index || old.blend != new.blend || old.shaders != new.shaders {
        return true;
    }
    match (&old.body, &new.body) {
        (
            Body::Custom {
                vertex_count,
                instance_count,
            },
            Body::Custom {
                vertex_count: vertices,
                instance_count: instances,
            },
        ) => vertex_count != vertices || instance_count != instances,
        (
            Body::Post {
                vertex_count,
                instance_count,
            },
            Body::Post {
                vertex_count: vertices,
                instance_count: instances,
            },
        ) => vertex_count != vertices || instance_count != instances,
        (Body::Shape { .. }, Body::Shape { .. }) => false,
        (Body::Image { texture, .. }, Body::Image { texture: other, .. }) => texture != other,
        (
            Body::Text {
                content,
                stroke,
                background,
                ..
            },
            Body::Text {
                content: other,
                stroke: other_stroke,
                background: other_background,
                ..
            },
        ) => {
            !Arc::ptr_eq(content, other)
                && (content.font != other.font
                    || content.glyphs.len() != other.glyphs.len()
                    || content.decorations.len() != other.decorations.len())
                || (*stroke > 0.0) != (*other_stroke > 0.0)
                || (background[3] > 0.0) != (other_background[3] > 0.0)
        }
        _ => true,
    }
}

fn outlined(body: &Body) -> bool {
    matches!(body, Body::Shape { outline: Some(_), .. })
}

fn table_entry(snapshot: &Snapshot) -> GpuObject {
    let (kind, color) = match &snapshot.body {
        Body::Custom { .. } | Body::Post { .. } => (ObjectKind::Renderable, [0.0; 4]),
        Body::Shape { color, .. } => (ObjectKind::Shape, *color),
        Body::Image { color, .. } => (ObjectKind::Image, *color),
        Body::Text { color, .. } => (ObjectKind::Text, *color),
    };
    let collider = snapshot.body.collider(snapshot.id);
    GpuObject::new(snapshot.id, kind, collider.as_ref(), color, snapshot.z_index)
}

fn to_world(transform: &Transform, local: [f64; 2]) -> [f64; 2] {
    let (sin, cos) = transform.rotation.sin_cos();
    let x = local[0] - transform.anchor[0] * transform.size[0];
    let y = local[1] - transform.anchor[1] * transform.size[1];
    [
        transform.position[0] + x * cos - y * sin,
        transform.position[1] + x * sin + y * cos,
    ]
}

fn narrow<const N: usize>(values: [f64; N]) -> [f32; N] {
    values.map(|value| value as f32)
}

fn quad(transform: &Transform, local: [f64; 2], size: [f64; 2], slot: u32) -> Instance {
    Instance {
        position: narrow(to_world(transform, local)),
        size: narrow(size),
        rotation: transform.rotation as f32,
        uv: [0.0, 0.0, 1.0, 1.0],
        object: slot,
        ..Instance::default()
    }
}

impl Scene {
    fn allocate(&mut self) -> u32 {
        self.slot_generation += 1;
        match self.free.pop() {
            Some(slot) => slot,
            None => {
                self.table.push(GpuObject::EMPTY);
                (self.table.len() - 1) as u32
            }
        }
    }

    fn release(&mut self, slot: u32) {
        self.slot_generation += 1;
        if let Some(entry) = self.table.get_mut(slot as usize) {
            *entry = GpuObject::EMPTY;
        }
        self.free.push(slot);
        self.touch_table(slot);
    }

    fn touch_table(&mut self, slot: u32) {
        let slot = slot as usize;
        self.table_dirty = Some(match self.table_dirty.take() {
            Some(range) => range.start.min(slot)..range.end.max(slot + 1),
            None => slot..slot + 1,
        });
    }

    fn apply(&mut self, delta: SceneDelta, mut graphics: Option<&mut Graphics>) -> Vec<String> {
        let mut errors = Vec::new();
        for resource in delta.resources {
            match resource {
                Resource::Texture {
                    id,
                    width,
                    height,
                    rgba,
                } => {
                    if let Some(graphics) = graphics.as_deref_mut()
                        && let Err(error) = graphics.add_texture(id, width, height, &rgba)
                    {
                        errors.push(error);
                    }
                    self.structure = true;
                }
                Resource::Font { id, data } => {
                    if let Some(font) = Font::parse(data) {
                        self.fonts.insert(id, font);
                        self.structure = true;
                    }
                }
                Resource::Shader { id, layout } => {
                    self.shaders.insert(id, layout);
                    self.structure = true;
                }
            }
        }
        for id in delta.removals {
            if let Some(tracked) = self.objects.remove(&id) {
                self.outlines_stale |= outlined(&tracked.snapshot.body);
                self.hooked -= usize::from(tracked.snapshot.hook.is_some());
                self.release(tracked.slot);
                self.structure = true;
            }
        }
        for snapshot in delta.upserts {
            let entry = table_entry(&snapshot);
            let id = snapshot.id;
            self.outlines_stale |= outlined(&snapshot.body);
            let slot = match self.objects.get_mut(&id) {
                Some(tracked) => {
                    self.outlines_stale |= outlined(&tracked.snapshot.body);
                    if structural(&tracked.snapshot, &snapshot) {
                        self.structure = true;
                    } else {
                        self.patched.push(id);
                    }
                    self.hooked -= usize::from(tracked.snapshot.hook.is_some());
                    self.hooked += usize::from(snapshot.hook.is_some());
                    tracked.snapshot = snapshot;
                    tracked.slot
                }
                None => {
                    let slot = self.allocate();
                    self.hooked += usize::from(snapshot.hook.is_some());
                    self.objects.insert(
                        id,
                        Tracked {
                            snapshot,
                            slot,
                            range: 0..0,
                            data: Map::default(),
                            native: Map::default(),
                            references: false,
                            data_version: 0,
                            bound: None,
                        },
                    );
                    self.structure = true;
                    slot
                }
            };
            self.table[slot as usize] = entry;
            self.touch_table(slot);
        }
        for write in delta.slots {
            if let Some(tracked) = self.objects.get_mut(&write.object) {
                tracked.native.remove(&(write.group, write.binding));
                if let SlotContent::Buffer { references, .. } = &write.content
                    && !references.is_empty()
                {
                    tracked.references = true;
                }
                tracked.data.insert((write.group, write.binding), write.content);
                tracked.data_version += 1;
            }
        }
        for release in delta.releases {
            match release {
                Release::Texture(id) => {
                    if let Some(graphics) = graphics.as_deref_mut() {
                        graphics.remove_texture(id);
                    }
                    self.structure = true;
                }
                Release::Font(id) => {
                    self.fonts.remove(&id);
                    if let Some(graphics) = graphics.as_deref_mut() {
                        graphics.atlas.forget_font(id);
                    }
                }
                Release::Shader(id) => {
                    self.shaders.remove(&id);
                    if let Some(graphics) = graphics.as_deref_mut() {
                        graphics.pipelines.forget(id);
                    }
                }
            }
        }
        errors
    }

    fn emit(&self, graphics: &mut Graphics, tracked: &Tracked, out: &mut Vec<Instance>) -> Result<(), AtlasFull> {
        let slot = tracked.slot;
        match &tracked.snapshot.body {
            Body::Custom { .. } | Body::Post { .. } => {}
            Body::Shape {
                transform,
                color,
                shape,
                outline: _,
                stroke_color,
                stroke,
            } => out.push(Instance {
                position: narrow(transform.position),
                size: narrow(transform.size),
                anchor: narrow(transform.anchor),
                rotation: transform.rotation as f32,
                kind: INSTANCE_SHAPE,
                color: *color,
                stroke_color: *stroke_color,
                uv: [0.0, 0.0, 1.0, 1.0],
                shape: self
                    .outline_codes
                    .get(&tracked.snapshot.id)
                    .copied()
                    .unwrap_or_else(|| shape.index()),
                stroke: *stroke,
                flags: 0,
                object: slot,
            }),
            Body::Image {
                transform,
                color,
                texture,
                uv,
                pixelated,
            } => {
                if graphics.textures.contains_key(texture) {
                    out.push(Instance {
                        position: narrow(transform.position),
                        size: narrow(transform.size),
                        anchor: narrow(transform.anchor),
                        rotation: transform.rotation as f32,
                        kind: INSTANCE_IMAGE,
                        color: *color,
                        uv: *uv,
                        flags: if *pixelated { FLAG_PIXELATED } else { 0 },
                        object: slot,
                        ..Instance::default()
                    });
                }
            }
            Body::Text {
                transform,
                color,
                background,
                stroke_color,
                stroke,
                content,
            } => {
                if background[3] > 0.0 {
                    out.push(Instance {
                        position: narrow(transform.position),
                        size: narrow(transform.size),
                        anchor: narrow(transform.anchor),
                        rotation: transform.rotation as f32,
                        kind: INSTANCE_SHAPE,
                        color: *background,
                        uv: [0.0, 0.0, 1.0, 1.0],
                        shape: RECTANGLE,
                        object: slot,
                        ..Instance::default()
                    });
                }
                let Some(font) = self.fonts.get(&content.font) else {
                    return Ok(());
                };
                let scale = self.scale.max(1e-6);
                let snap = transform.rotation == 0.0;
                let pixels = f64::from(content.size) * scale;
                let key = |glyph: u16, outline: f64| GlyphKey {
                    font: content.font,
                    glyph,
                    size: GlyphKey::quantize(pixels),
                    bold: content.bold,
                    italic: content.italic,
                    stroke: GlyphKey::quantize(outline * scale),
                };
                let place = |entry: GlyphEntry, glyph: &PlacedGlyph, tint: [f32; 4]| {
                    let local = [
                        f64::from(glyph.x) + f64::from(entry.left) / scale,
                        f64::from(glyph.y) - f64::from(entry.top) / scale,
                    ];
                    let mut world = to_world(transform, local);
                    if snap {
                        world = world.map(|value| (value * scale).round() / scale);
                    }
                    Instance {
                        position: narrow(world),
                        size: [(f64::from(entry.width) / scale) as f32, (f64::from(entry.height) / scale) as f32],
                        rotation: transform.rotation as f32,
                        kind: INSTANCE_GLYPH,
                        color: tint,
                        uv: entry.uv,
                        object: slot,
                        ..Instance::default()
                    }
                };
                let queue = graphics.gpu.queue.clone();
                if *stroke > 0.0 {
                    for glyph in &content.glyphs {
                        if let Some(entry) = graphics.atlas.glyph(&queue, font, key(glyph.id, f64::from(*stroke)))? {
                            out.push(place(entry, glyph, *stroke_color));
                        }
                    }
                }
                for glyph in &content.glyphs {
                    if let Some(entry) = graphics.atlas.glyph(&queue, font, key(glyph.id, 0.0))? {
                        out.push(place(entry, glyph, *color));
                    }
                }
                for decoration in &content.decorations {
                    let mut line = quad(
                        transform,
                        [f64::from(decoration[0]), f64::from(decoration[1])],
                        [f64::from(decoration[2]), f64::from(decoration[3])],
                        slot,
                    );
                    line.kind = INSTANCE_SHAPE;
                    line.shape = RECTANGLE;
                    line.color = *color;
                    out.push(line);
                }
            }
        }
        Ok(())
    }

    fn sync_outlines(&mut self) {
        if !self.outlines_stale {
            return;
        }
        self.outlines_stale = false;
        self.intern_outlines();
    }

    fn intern_outlines(&mut self) {
        self.outlines.clear();
        self.outline_codes.clear();
        let mut seen: HashMap<usize, u32> = HashMap::new();
        let mut stamped: Vec<(ObjectId, u32, u32)> = Vec::new();
        for tracked in self.objects.values() {
            let Body::Shape {
                outline: Some(outline),
                ..
            } = &tracked.snapshot.body
            else {
                continue;
            };
            let count = outline.len() as u32;
            if !(3..=geometry::MAX_OUTLINE_POINTS).contains(&count) {
                continue;
            }
            let key = Arc::as_ptr(outline) as *const u8 as usize;
            let code = match seen.get(&key) {
                Some(code) => *code,
                None => {
                    let offset = self.outlines.len() as u32;
                    if offset + count > geometry::CUSTOM_OFFSET {
                        continue;
                    }
                    self.outlines
                        .extend(outline.iter().map(|point| [point[0] as f32, point[1] as f32]));
                    let code = geometry::custom_shape(offset, count);
                    seen.insert(key, code);
                    code
                }
            };
            self.outline_codes.insert(tracked.snapshot.id, code);
            stamped.push((tracked.snapshot.id, tracked.slot, code));
        }
        for (_, slot, code) in stamped {
            if let Some(entry) = self.table.get_mut(slot as usize) {
                entry.shape = code;
            }
            self.touch_table(slot);
        }
        self.outlines_dirty = true;
    }

    fn rebuild(&mut self, graphics: &mut Graphics) -> Result<(), AtlasFull> {
        self.sync_outlines();
        let mut order: Vec<(f64, u64, ObjectId)> = self
            .objects
            .values()
            .map(|tracked| (tracked.snapshot.z_index, tracked.snapshot.order, tracked.snapshot.id))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut instances = Vec::with_capacity(self.instances.len());
        let mut draw = Vec::new();
        let mut post = Vec::new();
        let mut ranges = Vec::with_capacity(order.len());
        for (_, _, id) in order {
            let tracked = &self.objects[&id];
            if matches!(tracked.snapshot.body, Body::Post { .. }) {
                post.push(id);
                continue;
            }
            let start = instances.len() as u32;
            self.emit(graphics, tracked, &mut instances)?;
            let range = start..instances.len() as u32;
            ranges.push((id, range.clone()));
            let custom = !tracked.snapshot.shaders.is_empty() || matches!(tracked.snapshot.body, Body::Custom { .. });
            if custom {
                draw.push(DrawItem::Custom(id));
                continue;
            }
            if range.is_empty() {
                continue;
            }
            let slot = TextureSlot::of(&tracked.snapshot.body);
            let blend = tracked.snapshot.blend;
            if let Some(DrawItem::Batch {
                blend: last_blend,
                texture,
                range: last,
            }) = draw.last_mut()
                && *last_blend == blend
                && last.end == range.start
                && (*texture == slot || *texture == TextureSlot::White || slot == TextureSlot::White)
            {
                if *texture == TextureSlot::White {
                    *texture = slot;
                }
                last.end = range.end;
                continue;
            }
            draw.push(DrawItem::Batch {
                blend,
                texture: slot,
                range,
            });
        }
        for (id, range) in ranges {
            if let Some(tracked) = self.objects.get_mut(&id) {
                tracked.range = range;
            }
        }
        self.dirty = Some(0..instances.len());
        self.instances = instances;
        self.draw = draw;
        self.post = post;
        self.structure = false;
        self.patched.clear();
        Ok(())
    }

    fn patch(&mut self, graphics: &mut Graphics) -> Result<(), AtlasFull> {
        let mut buffer = Vec::new();
        for id in std::mem::take(&mut self.patched) {
            let Some(tracked) = self.objects.get(&id) else {
                continue;
            };
            buffer.clear();
            self.emit(graphics, tracked, &mut buffer)?;
            let range = tracked.range.start as usize..tracked.range.end as usize;
            if buffer.len() != range.len() {
                self.structure = true;
                return Ok(());
            }
            self.instances[range.clone()].copy_from_slice(&buffer);
            self.dirty = Some(match self.dirty.take() {
                Some(dirty) => dirty.start.min(range.start)..dirty.end.max(range.end),
                None => range,
            });
        }
        Ok(())
    }

    fn prepare(&mut self, graphics: &mut Graphics, scale: f64, errors: &mut Vec<String>) {
        if (scale - self.scale).abs() > 1e-9 {
            self.scale = scale;
            self.structure = true;
        }
        for _ in 0..8 {
            let result = if self.structure {
                self.rebuild(graphics)
            } else if !self.patched.is_empty() {
                self.patch(graphics)
            } else {
                break;
            };
            if result.is_err() {
                if !graphics.atlas.grow(&graphics.gpu.device) {
                    errors.push("too much text is visible at once, some glyphs could not be drawn".to_owned());
                }
                graphics.groups.clear();
                self.structure = true;
            }
        }
        self.upload(graphics);
    }

    fn upload(&mut self, graphics: &mut Graphics) {
        self.sync_outlines();
        let device = graphics.gpu.device.clone();
        let instance_bytes = (self.instances.len().max(1) * size_of::<Instance>()) as u64;
        if graphics.instances.ensure(&device, instance_bytes) {
            graphics.groups.clear();
            if let Some(post) = &mut graphics.post {
                post.groups = [None, None];
            }
            self.dirty = Some(0..self.instances.len());
        }
        if let Some(range) = self.dirty.take()
            && !range.is_empty()
        {
            graphics.gpu.queue.write_buffer(
                &graphics.instances.buffer,
                (range.start * size_of::<Instance>()) as u64,
                bytemuck::cast_slice(&self.instances[range]),
            );
        }
        let table_bytes = (self.table.len().max(1) * size_of::<GpuObject>()) as u64;
        let outline_bytes = (self.outlines.len().max(1) * size_of::<[f32; 2]>()) as u64;
        if graphics.outlines.ensure(&device, outline_bytes) {
            graphics.groups.clear();
            if let Some(post) = &mut graphics.post {
                post.groups = [None, None];
            }
            self.outlines_dirty = true;
        }
        if self.outlines_dirty && !self.outlines.is_empty() {
            graphics
                .gpu
                .queue
                .write_buffer(&graphics.outlines.buffer, 0, bytemuck::cast_slice(&self.outlines));
        }
        self.outlines_dirty = false;
        if graphics.objects.ensure(&device, table_bytes) {
            graphics.groups.clear();
            if let Some(post) = &mut graphics.post {
                post.groups = [None, None];
            }
            self.table_dirty = Some(0..self.table.len());
        }
        if let Some(range) = self.table_dirty.take()
            && range.end <= self.table.len()
            && !range.is_empty()
        {
            graphics.gpu.queue.write_buffer(
                &graphics.objects.buffer,
                (range.start * size_of::<GpuObject>()) as u64,
                bytemuck::cast_slice(&self.table[range]),
            );
        }
    }

    fn custom_key(&self, snapshot: &Snapshot, format: wgpu::TextureFormat) -> Result<Option<CustomKey>, String> {
        let custom = matches!(snapshot.body, Body::Custom { .. });
        let mut vertex = None;
        let mut fragment = None;
        for id in snapshot.shaders.iter() {
            let Some(layout) = self.shaders.get(id) else {
                continue;
            };
            if let Some(entry) = &layout.vertex {
                vertex = Some(Stage {
                    shader: *id,
                    entry: entry.clone(),
                });
            }
            if let Some(entry) = &layout.fragment {
                fragment = Some(Stage {
                    shader: *id,
                    entry: entry.clone(),
                });
            }
        }
        if custom && (vertex.is_none() || fragment.is_none()) {
            if snapshot.shaders.is_empty() {
                return Ok(None);
            }
            let missing = if vertex.is_none() { "vertex" } else { "fragment" };
            return Err(format!(
                "a Renderable needs shaders with both a vertex and a fragment entry point, no {missing} entry point is loaded"
            ));
        }
        if vertex.is_none() && fragment.is_none() {
            return Ok(None);
        }
        Ok(Some(CustomKey {
            vertex,
            fragment,
            blend: snapshot.blend,
            format,
            post: false,
        }))
    }

    fn post_key(&self, snapshot: &Snapshot, format: wgpu::TextureFormat) -> Result<Option<CustomKey>, String> {
        let mut vertex = None;
        let mut fragment = None;
        for id in snapshot.shaders.iter() {
            let Some(layout) = self.shaders.get(id) else {
                continue;
            };
            if let Some(entry) = &layout.vertex {
                vertex = Some(Stage {
                    shader: *id,
                    entry: entry.clone(),
                });
            }
            if let Some(entry) = &layout.fragment {
                fragment = Some(Stage {
                    shader: *id,
                    entry: entry.clone(),
                });
            }
        }
        if snapshot.shaders.is_empty() {
            return Ok(None);
        }
        if fragment.is_none() {
            return Err("a PostProcess needs a shader with a fragment entry point".to_owned());
        }
        Ok(Some(CustomKey {
            vertex,
            fragment,
            blend: Blend::Opaque,
            format,
            post: true,
        }))
    }

    fn resolve_post(&mut self, graphics: &mut Graphics, format: wgpu::TextureFormat, errors: &mut Vec<String>) -> Vec<Prepared> {
        let device = graphics.gpu.device.clone();
        let mut passes = Vec::new();
        for index in 0..self.post.len() {
            let id = self.post[index];
            let Some(tracked) = self.objects.get(&id) else {
                continue;
            };
            let key = match self.post_key(&tracked.snapshot, format) {
                Ok(Some(key)) => key,
                Ok(None) => continue,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };
            let (vertices, instances) = match (&key.vertex, &tracked.snapshot.body) {
                (
                    Some(_),
                    Body::Post {
                        vertex_count,
                        instance_count,
                    },
                ) => (0..*vertex_count, 0..*instance_count),
                _ => (0..3, 0..1),
            };
            let custom = match graphics.pipelines.custom(&device, &graphics.engine, &key, &self.shaders) {
                Ok(custom) => custom,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };
            let groups = self.bind(graphics, id, &key, &custom);
            passes.push(Prepared {
                pipeline: custom.pipeline.clone(),
                groups,
                vertices,
                instances,
                backdrop: false,
            });
        }
        passes
    }

    fn bind(&mut self, graphics: &Graphics, id: ObjectId, key: &CustomKey, custom: &Custom) -> Vec<(u32, wgpu::BindGroup)> {
        let slot_generation = self.slot_generation;
        let references = {
            let Some(tracked) = self.objects.get(&id) else {
                return Vec::new();
            };
            let version = (
                tracked.data_version,
                graphics.texture_generation,
                if tracked.references { slot_generation } else { 0 },
            );
            if let Some(bound) = &tracked.bound
                && bound.key == *key
                && bound.version == version
            {
                return bound.groups.clone();
            }
            tracked.references
        };
        let slots: Map<ObjectId, u32> = if references {
            self.objects.iter().map(|(id, tracked)| (*id, tracked.slot)).collect()
        } else {
            Map::default()
        };
        let Some(tracked) = self.objects.get_mut(&id) else {
            return Vec::new();
        };
        let version = (
            tracked.data_version,
            graphics.texture_generation,
            if references { slot_generation } else { 0 },
        );

        let device = &graphics.gpu.device;
        let mut buffers = tracked.bound.take().map(|bound| bound.buffers).unwrap_or_default();
        let mut groups = Vec::new();
        for group in &custom.groups {
            enum Resource {
                Buffer(wgpu::Buffer),
                View(wgpu::TextureView),
                Sampler(wgpu::Sampler),
            }
            let mut resources = Vec::new();
            for (binding, kind) in &group.bindings {
                let content = tracked.data.get(&(group.group, *binding));
                let resource = match kind {
                    BindingKind::Uniform { size } | BindingKind::Storage { size, .. } => {
                        let mut bytes = match content {
                            Some(SlotContent::Buffer { bytes, references }) => {
                                let mut bytes = bytes.clone();
                                for (offset, target) in references {
                                    let value = slots.get(target).copied().unwrap_or(NO_OBJECT);
                                    let offset = *offset as usize;
                                    if let Some(place) = bytes.get_mut(offset..offset + 4) {
                                        place.copy_from_slice(&value.to_le_bytes());
                                    }
                                }
                                bytes
                            }
                            _ => Vec::new(),
                        };
                        let needed = (bytes.len() as u64).max(u64::from(*size)).max(16).next_multiple_of(16);
                        bytes.resize(needed as usize, 0);
                        let usage = match kind {
                            BindingKind::Uniform { .. } => wgpu::BufferUsages::UNIFORM,
                            _ => wgpu::BufferUsages::STORAGE,
                        } | wgpu::BufferUsages::COPY_DST;
                        let buffer = match buffers.get(&(group.group, *binding)) {
                            Some(buffer) if buffer.size() >= needed && buffer.usage() == usage => buffer.clone(),
                            _ => device.create_buffer(&wgpu::BufferDescriptor {
                                label: Some("renderable data"),
                                size: needed,
                                usage,
                                mapped_at_creation: false,
                            }),
                        };
                        graphics.gpu.queue.write_buffer(&buffer, 0, &bytes);
                        buffers.insert((group.group, *binding), buffer.clone());
                        Resource::Buffer(buffer)
                    }
                    BindingKind::Texture { .. } => Resource::View(match (tracked.native.get(&(group.group, *binding)), content) {
                        (Some(native), _) => native.view.clone(),
                        (None, Some(SlotContent::Texture(texture))) => {
                            graphics.texture_view(TextureSlot::Image(*texture)).clone()
                        }
                        _ => graphics.white.view.clone(),
                    }),
                    BindingKind::Sampler { comparison: true } => Resource::Sampler(graphics.comparison.clone()),
                    BindingKind::Sampler { .. } => Resource::Sampler(match content {
                        Some(SlotContent::Sampler { pixelated: true }) => graphics.nearest.clone(),
                        _ => graphics.linear.clone(),
                    }),
                };
                resources.push((*binding, resource));
            }
            let entries: Vec<wgpu::BindGroupEntry> = resources
                .iter()
                .map(|(binding, resource)| wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: match resource {
                        Resource::Buffer(buffer) => buffer.as_entire_binding(),
                        Resource::View(view) => wgpu::BindingResource::TextureView(view),
                        Resource::Sampler(sampler) => wgpu::BindingResource::Sampler(sampler),
                    },
                })
                .collect();
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("renderable data"),
                layout: &group.layout,
                entries: &entries,
            });
            groups.push((group.group, bind_group));
        }
        tracked.bound = Some(Bound {
            key: key.clone(),
            version,
            buffers,
            groups: groups.clone(),
        });
        groups
    }

    fn resolve(&mut self, graphics: &mut Graphics, view: &View, errors: &mut Vec<String>) -> Vec<Prepared> {
        struct Plan {
            item: usize,
            custom: Option<(ObjectId, CustomKey, Arc<Custom>)>,
        }
        let device = graphics.gpu.device.clone();
        let mut plans = Vec::with_capacity(self.draw.len());
        for (index, item) in self.draw.iter().enumerate() {
            let DrawItem::Custom(id) = item else {
                plans.push(Plan { item: index, custom: None });
                continue;
            };
            let Some(tracked) = self.objects.get(id) else {
                continue;
            };
            match self.custom_key(&tracked.snapshot, view.format) {
                Ok(Some(key)) => match graphics.pipelines.custom(&device, &graphics.engine, &key, &self.shaders) {
                    Ok(custom) => plans.push(Plan {
                        item: index,
                        custom: Some((*id, key, custom)),
                    }),
                    Err(error) => errors.push(error),
                },
                Ok(None) => {
                    if !matches!(tracked.snapshot.body, Body::Custom { .. }) && !tracked.range.is_empty() {
                        plans.push(Plan { item: index, custom: None });
                    }
                }
                Err(error) => errors.push(error),
            }
        }
        if view.copyable
            && plans
                .iter()
                .any(|plan| plan.custom.as_ref().is_some_and(|(_, _, custom)| custom.backdrop))
        {
            graphics.ensure_backdrop(view.format, view.size);
        }

        let mut prepared = Vec::with_capacity(plans.len());
        for plan in plans {
            match (&self.draw[plan.item], plan.custom) {
                (DrawItem::Batch { blend, texture, range }, _) => {
                    let (blend, texture, range) = (*blend, *texture, range.clone());
                    prepared.push(Prepared {
                        pipeline: graphics.pipelines.builtin(&device, blend, view.format),
                        groups: vec![(0, graphics.group(texture))],
                        vertices: 0..6,
                        instances: range,
                        backdrop: false,
                    });
                }
                (DrawItem::Custom(id), Some((_, key, custom))) => {
                    let id = *id;
                    let Some(tracked) = self.objects.get(&id) else {
                        continue;
                    };
                    let slot = TextureSlot::of(&tracked.snapshot.body);
                    let (vertices, instances) = match tracked.snapshot.body {
                        Body::Custom {
                            vertex_count,
                            instance_count,
                        } => (0..vertex_count, 0..instance_count),
                        _ => (0..6, tracked.range.clone()),
                    };
                    let mut groups = vec![(0, graphics.group(slot))];
                    groups.extend(self.bind(graphics, id, &key, &custom));
                    prepared.push(Prepared {
                        pipeline: custom.pipeline.clone(),
                        groups,
                        vertices,
                        instances,
                        backdrop: custom.backdrop,
                    });
                }
                (DrawItem::Custom(id), None) => {
                    let id = *id;
                    let Some(tracked) = self.objects.get(&id) else {
                        continue;
                    };
                    let (slot, blend, range) = (
                        TextureSlot::of(&tracked.snapshot.body),
                        tracked.snapshot.blend,
                        tracked.range.clone(),
                    );
                    prepared.push(Prepared {
                        pipeline: graphics.pipelines.builtin(&device, blend, view.format),
                        groups: vec![(0, graphics.group(slot))],
                        vertices: 0..6,
                        instances: range,
                        backdrop: false,
                    });
                }
            }
        }
        prepared
    }

    fn native_binding(&self, id: ObjectId, name: &str) -> Result<(Arc<ShaderLayout>, DataTarget), (i32, String)> {
        let tracked = self
            .objects
            .get(&id)
            .ok_or_else(|| (hook::INVALID, "a RenderHook ran for a renderable that no longer exists".to_owned()))?;
        let mut reason = None;
        for shader in tracked.snapshot.shaders.iter() {
            let Some(layout) = self.shaders.get(shader) else {
                continue;
            };
            match layout.resolve(name) {
                Ok(target) => return Ok((layout.clone(), target)),
                Err(error) => reason = Some(error),
            }
        }
        Err((
            hook::UNKNOWN_NAME,
            format!(
                "a RenderHook wrote '{name}', but {}",
                reason.unwrap_or_else(|| "its renderable has no shaders loaded".to_owned())
            ),
        ))
    }

    fn write_native(&mut self, id: ObjectId, name: &str, offset: u64, bytes: &[u8]) -> NativeResult {
        let (layout, target) = self.native_binding(id, name)?;
        let binding = &layout.bindings[target.binding];
        let (size, runtime) = match binding.kind {
            BindingKind::Uniform { size } => (size, None),
            BindingKind::Storage { size, .. } => (size, binding.runtime),
            other => {
                return Err((
                    hook::WRONG_KIND,
                    format!("a RenderHook wrote bytes to '{name}', but it is {}", other.describe()),
                ));
            }
        };
        let start = u64::from(target.offset) + offset;
        let end = start + bytes.len() as u64;
        if (runtime.is_none() && end > u64::from(size)) || end > MAX_NATIVE_BYTES {
            return Err((
                hook::OUT_OF_RANGE,
                format!("a RenderHook wrote bytes {start} to {end} of '{name}', which only holds {size} bytes"),
            ));
        }
        let Some(tracked) = self.objects.get_mut(&id) else {
            return Err((hook::INVALID, "a RenderHook ran for a renderable that no longer exists".to_owned()));
        };
        let initial = runtime.map_or(size as usize, |(offset, _)| offset as usize);
        let content = tracked
            .data
            .entry((binding.group, binding.binding))
            .or_insert_with(|| SlotContent::Buffer {
                bytes: vec![0; initial],
                references: Vec::new(),
            });
        if !matches!(content, SlotContent::Buffer { .. }) {
            *content = SlotContent::Buffer {
                bytes: vec![0; initial],
                references: Vec::new(),
            };
        }
        if let SlotContent::Buffer { bytes: stored, .. } = content {
            let (start, end) = (start as usize, end as usize);
            if stored.len() < end {
                stored.resize(end, 0);
            }
            stored[start..end].copy_from_slice(bytes);
        }
        tracked.data_version += 1;
        Ok(())
    }

    fn write_native_texture(
        &mut self,
        graphics: &mut Graphics,
        id: ObjectId,
        name: &str,
        size: [u32; 2],
        pixels: &[u8],
    ) -> NativeResult {
        let (layout, target) = self.native_binding(id, name)?;
        let binding = &layout.bindings[target.binding];
        if !matches!(
            binding.kind,
            BindingKind::Texture {
                sample: SampleKind::Float,
                multisampled: false
            }
        ) {
            return Err((
                hook::WRONG_KIND,
                format!("a RenderHook wrote a texture to '{name}', but it is {}", binding.kind.describe()),
            ));
        }
        let limit = graphics.gpu.device.limits().max_texture_dimension_2d;
        if size[0] > limit || size[1] > limit {
            return Err((
                hook::OUT_OF_RANGE,
                format!(
                    "a RenderHook wrote a {}x{} texture, textures can be at most {limit}x{limit}",
                    size[0], size[1]
                ),
            ));
        }
        let Some(tracked) = self.objects.get_mut(&id) else {
            return Err((hook::INVALID, "a RenderHook ran for a renderable that no longer exists".to_owned()));
        };
        let key = (binding.group, binding.binding);
        if tracked.native.get(&key).is_none_or(|native| native.size != size) {
            let texture = create_texture(
                &graphics.gpu.device,
                "native",
                size,
                OFFSCREEN_FORMAT,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            tracked.native.insert(key, NativeTexture { texture, view, size });
            tracked.data_version += 1;
        }
        if let Some(native) = tracked.native.get(&key) {
            graphics.gpu.queue.write_texture(
                native.texture.as_image_copy(),
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size[0] * 4),
                    rows_per_image: Some(size[1]),
                },
                extent(size),
            );
        }
        Ok(())
    }

    fn set_draw_counts(&mut self, id: ObjectId, vertices: u32, instances: u32) {
        if let Some(tracked) = self.objects.get_mut(&id)
            && let Body::Custom {
                vertex_count,
                instance_count,
            } = &mut tracked.snapshot.body
        {
            *vertex_count = vertices;
            *instance_count = instances;
        }
    }

    fn run_hooks(&mut self, graphics: &mut Graphics, view: &View, frame: &FrameInfo, errors: &mut Vec<String>) {
        if self.hooked == 0 {
            return;
        }
        let mut hooks: Vec<(f64, u64, ObjectId, NativeHook, [f32; 7])> = self
            .objects
            .values()
            .filter_map(|tracked| {
                let hook = tracked.snapshot.hook.clone()?;
                Some((
                    tracked.snapshot.z_index,
                    tracked.snapshot.order,
                    tracked.snapshot.id,
                    hook,
                    placement(&tracked.snapshot.body),
                ))
            })
            .collect();
        if hooks.is_empty() {
            return;
        }
        hooks.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let gpu = graphics.gpu.clone();
        let _queue = gpu.lock_queue();
        let vulkan = gpu
            .vulkan
            .as_ref()
            .map_or(ptr::null(), |handles| handles as *const VulkanHandles);
        let scale = view.scale.max(1e-6);
        let number = graphics.frame_number;
        for (_, _, id, hook, place) in hooks {
            let mut run = HookRun {
                scene: self as *mut Scene,
                graphics: graphics as *mut Graphics,
                object: id,
                errors: errors as *mut Vec<String>,
            };
            let mut context = RenderContext {
                version: hook::VERSION,
                struct_size: size_of::<RenderContext>() as u32,
                user_data: hook.data as *mut c_void,
                renderable: id,
                time: frame.time,
                delta: frame.delta,
                frame: number,
                width: (f64::from(view.size[0]) / scale) as f32,
                height: (f64::from(view.size[1]) / scale) as f32,
                scale: scale as f32,
                position: [place[0], place[1]],
                size: [place[2], place[3]],
                anchor: [place[4], place[5]],
                rotation: place[6],
                write_data: hook_write_data,
                write_texture: hook_write_texture,
                set_draw_counts: hook_set_draw_counts,
                vulkan,
                engine: &mut run as *mut HookRun as *mut c_void,
            };
            let function = unsafe { std::mem::transmute::<usize, HookFunction>(hook.function) };
            unsafe { function(&mut context) };
            drop(hook);
        }
    }

    fn colliders(&self) -> Vec<geometry::Collider> {
        self.objects
            .values()
            .filter_map(|tracked| tracked.snapshot.body.collider(tracked.snapshot.id))
            .collect()
    }
}

struct Renderer {
    window: WindowId,
    events: WindowEvents,
    reported: HashSet<String>,
    scene: Scene,
    graphics: Option<Graphics>,
}

fn logical_size(frame: &FrameInfo) -> [u32; 2] {
    [
        frame.width.round().clamp(1.0, 16384.0) as u32,
        frame.height.round().clamp(1.0, 16384.0) as u32,
    ]
}

fn begin_pass<'encoder>(
    encoder: &'encoder mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'encoder> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("frame"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

fn read_texture(gpu: &Gpu, texture: &wgpu::Texture, size: [u32; 2]) -> Result<Vec<u8>, String> {
    let row = (size[0] * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture"),
        size: u64::from(row) * u64::from(size[1]),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("capture") });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(size[1]),
            },
        },
        extent(size),
    );
    {
        let _queue = gpu.lock_queue();
        gpu.queue.submit([encoder.finish()]);
    }
    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("the capture did not finish: {error}"))?;
    receiver
        .recv()
        .map_err(|_| "the capture was abandoned".to_owned())?
        .map_err(|error| format!("cannot read the capture: {error}"))?;
    let rgba = {
        let data = slice
            .get_mapped_range()
            .map_err(|error| format!("cannot read the capture: {error}"))?;
        let mut rgba = Vec::with_capacity((size[0] * size[1] * 4) as usize);
        for line in data.chunks_exact(row as usize) {
            rgba.extend_from_slice(&line[..(size[0] * 4) as usize]);
        }
        rgba
    };
    buffer.unmap();
    Ok(rgba)
}

impl Renderer {
    fn report(&mut self, message: String) {
        if self.reported.insert(message.clone()) {
            let _ = self.events.send((self.window, WindowEvent::RenderError(message)));
        }
    }

    fn query(&mut self, query: &Query) -> Result<Vec<Hit>, String> {
        let Some(graphics) = self.graphics.as_mut() else {
            return Ok(geometry::run(&self.scene.colliders(), query));
        };
        self.scene.upload(graphics);
        let state = graphics
            .query
            .get_or_insert_with(|| QueryState::new(&graphics.gpu.device));
        state.run(
            &graphics.gpu,
            &graphics.objects.buffer,
            &graphics.outlines.buffer,
            self.scene.table.len() as u32,
            query,
        )
    }

    fn render(&mut self, view: &View, frame: &FrameInfo) {
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };
        let mut errors = Vec::new();
        self.scene.run_hooks(graphics, view, frame, &mut errors);
        self.scene.prepare(graphics, view.scale, &mut errors);
        let scale = view.scale.max(1e-6);
        graphics.gpu.queue.write_buffer(
            &graphics.frame,
            0,
            bytemuck::bytes_of(&FrameUniform {
                resolution: [
                    (f64::from(view.size[0]) / scale) as f32,
                    (f64::from(view.size[1]) / scale) as f32,
                ],
                scale: scale as f32,
                time: frame.time as f32,
                delta: frame.delta as f32,
                frame: graphics.frame_number,
                padding: [0.0; 2],
            }),
        );
        graphics.frame_number = graphics.frame_number.wrapping_add(1);
        let passes = self.scene.resolve_post(graphics, view.format, &mut errors);
        let targets = (!passes.is_empty()).then(|| graphics.post_targets(view.format, view.size));
        let final_view = view;
        let staged = targets.as_ref().map(|targets| View {
            view: targets[0].1.clone(),
            texture: targets[0].0.clone(),
            format: final_view.format,
            size: final_view.size,
            scale: final_view.scale,
            copyable: true,
        });
        let view = staged.as_ref().unwrap_or(final_view);
        let prepared = self.scene.resolve(graphics, view, &mut errors);

        let background = wgpu::Color {
            r: f64::from(frame.background[0]),
            g: f64::from(frame.background[1]),
            b: f64::from(frame.background[2]),
            a: f64::from(frame.background[3]),
        };
        let mut encoder = graphics
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let mut segments = Vec::new();
        let mut start = 0;
        for (index, item) in prepared.iter().enumerate() {
            if item.backdrop && view.copyable && index > start {
                segments.push(start..index);
                start = index;
            }
        }
        segments.push(start..prepared.len());
        let mut cleared = false;
        for segment in segments {
            let copy = view.copyable && prepared.get(segment.start).is_some_and(|item| item.backdrop);
            if copy && let Some(backdrop) = &graphics.backdrop {
                if !cleared {
                    drop(begin_pass(&mut encoder, &view.view, wgpu::LoadOp::Clear(background)));
                    cleared = true;
                }
                encoder.copy_texture_to_texture(
                    view.texture.as_image_copy(),
                    backdrop.texture.as_image_copy(),
                    extent(view.size),
                );
            }
            let load = if cleared {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(background)
            };
            cleared = true;
            let mut render_pass = begin_pass(&mut encoder, &view.view, load);
            for item in &prepared[segment] {
                render_pass.set_pipeline(&item.pipeline);
                for (index, group) in &item.groups {
                    render_pass.set_bind_group(*index, group, &[]);
                }
                render_pass.draw(item.vertices.clone(), item.instances.clone());
            }
        }
        if let Some(targets) = &targets {
            for (index, pass) in passes.iter().enumerate() {
                let Some(group) = graphics.post_group(index % 2) else {
                    continue;
                };
                let output = if index + 1 == passes.len() {
                    &final_view.view
                } else {
                    &targets[(index + 1) % 2].1
                };
                let mut render_pass = begin_pass(&mut encoder, output, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
                render_pass.set_pipeline(&pass.pipeline);
                render_pass.set_bind_group(0, &group, &[]);
                for (index, group) in &pass.groups {
                    render_pass.set_bind_group(*index, group, &[]);
                }
                render_pass.draw(pass.vertices.clone(), pass.instances.clone());
            }
        }
        {
            let _queue = graphics.gpu.lock_queue();
            graphics.gpu.queue.submit([encoder.finish()]);
        }
        for error in errors {
            self.report(error);
        }
    }

    fn present(&mut self, frame: &FrameInfo) {
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };
        let gpu = graphics.gpu.clone();
        let device = gpu.device.clone();
        let (view, surface) = match &mut graphics.target {
            Target::Surface(target) => {
                let Some(texture) = target.acquire(&device) else {
                    return;
                };
                let view = View {
                    view: texture.texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    texture: texture.texture.clone(),
                    format: target.config.format,
                    size: [target.config.width, target.config.height],
                    scale: target.window.scale_factor(),
                    copyable: target.copyable,
                };
                (view, Some((texture, target.window.clone(), target.resize.clone())))
            }
            Target::Offscreen(offscreen) => {
                let size = logical_size(frame);
                let texture = match offscreen {
                    Some((texture, existing)) if *existing == size => texture.clone(),
                    _ => {
                        let texture = create_texture(
                            &device,
                            "offscreen",
                            size,
                            OFFSCREEN_FORMAT,
                            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                        );
                        *offscreen = Some((texture.clone(), size));
                        texture
                    }
                };
                let view = View {
                    view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    texture,
                    format: OFFSCREEN_FORMAT,
                    size,
                    scale: 1.0,
                    copyable: true,
                };
                (view, None)
            }
        };
        self.render(&view, frame);
        if let Some((texture, window, resize)) = surface {
            window.pre_present_notify();
            {
                let _queue = gpu.lock_queue();
                gpu.queue.present(texture);
            }
            resize.presented(view.size);
        }
    }

    fn capture(&mut self, frame: &FrameInfo) -> Result<Capture, String> {
        let (gpu, size, scale) = {
            let graphics = self
                .graphics
                .as_ref()
                .ok_or_else(|| "rendering is unavailable".to_owned())?;
            let (size, scale) = match &graphics.target {
                Target::Surface(target) => {
                    let size = target.window.inner_size();
                    ([size.width.max(1), size.height.max(1)], target.window.scale_factor())
                }
                Target::Offscreen(_) => (logical_size(frame), 1.0),
            };
            (graphics.gpu.clone(), size, scale)
        };
        let texture = create_texture(
            &gpu.device,
            "capture",
            size,
            OFFSCREEN_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let view = View {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture: texture.clone(),
            format: OFFSCREEN_FORMAT,
            size,
            scale,
            copyable: true,
        };
        self.render(&view, frame);
        let rgba = read_texture(&gpu, &texture, size)?;
        Ok(Capture {
            width: size[0],
            height: size[1],
            rgba,
        })
    }
}
