use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use mlua::AnyUserData;
use tokio::sync::{Notify, oneshot};

use super::alpha;
use super::Renderable;
use super::object::{Kind, Object, Slot};
use crate::graphics::geometry::{self, Hit, Query};
use crate::graphics::picture;
use crate::graphics::protocol::{
    Capture, FontId, FrameInfo, ObjectId, Release, RenderCommand, Resource, SceneDelta, ShaderId, SlotContent,
    SlotWrite, TextureId,
};
use crate::graphics::reflect::ShaderLayout;
use crate::graphics::renderer::RendererHandle;
use crate::graphics::text::Font;
use crate::objects::GameObject;
use crate::runtime::Scheduler;
use crate::window::{RenderTarget, WindowEvents, WindowId};

type Pixels = (u32, u32, Vec<u8>);

pub struct AlphaMask {
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<u8>,
}

impl AlphaMask {
    fn build(width: u32, height: u32, rgba: &[u8]) -> AlphaMask {
        AlphaMask {
            width,
            height,
            alpha: rgba.iter().skip(3).step_by(4).copied().collect(),
        }
    }

    pub fn at(&self, x: u32, y: u32) -> u8 {
        if self.width == 0 || self.height == 0 {
            return 0;
        }
        let x = x.min(self.width - 1) as usize;
        let y = y.min(self.height - 1) as usize;
        self.alpha.get(y * self.width as usize + x).copied().unwrap_or(0)
    }
}

struct TextureEntry {
    id: TextureId,
    count: usize,
    pixels: Option<Pixels>,
    uploaded: bool,
    _data: Arc<[u8]>,
}

struct FontEntry {
    id: FontId,
    count: usize,
    font: Font,
    uploaded: bool,
}

struct ShaderEntry {
    count: usize,
    layout: Arc<ShaderLayout>,
    uploaded: bool,
}

#[derive(Default)]
pub struct Resources {
    next: u64,
    active: bool,
    disabled: bool,
    textures: HashMap<usize, TextureEntry>,
    texture_keys: HashMap<TextureId, usize>,
    fonts: HashMap<usize, FontEntry>,
    font_keys: HashMap<FontId, usize>,
    shaders: HashMap<ShaderId, ShaderEntry>,
    uploads: Vec<Resource>,
    releases: Vec<Release>,
    decode: Vec<(TextureId, Arc<[u8]>, String)>,
    masks: HashMap<TextureId, Arc<AlphaMask>>,
    mask_wanted: HashSet<TextureId>,
    mask_decode: Vec<(TextureId, Arc<[u8]>, String)>,
    failed: HashSet<TextureId>,
    pinned: HashSet<TextureId>,
}

fn key(data: &Arc<[u8]>) -> usize {
    Arc::as_ptr(data) as *const u8 as usize
}

impl Resources {
    fn allocate(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    pub fn acquire_texture(&mut self, data: Arc<[u8]>, name: &str) -> TextureId {
        let slot = key(&data);
        if let Some(entry) = self.textures.get_mut(&slot) {
            entry.count += 1;
            return entry.id;
        }
        let id = self.allocate();
        if !self.disabled {
            self.decode.push((id, data.clone(), name.to_owned()));
        }
        self.textures.insert(
            slot,
            TextureEntry {
                id,
                count: 1,
                pixels: None,
                uploaded: false,
                _data: data,
            },
        );
        self.texture_keys.insert(id, slot);
        id
    }

    pub fn release_texture(&mut self, id: TextureId) {
        let Some(slot) = self.texture_keys.get(&id).copied() else {
            return;
        };
        let Some(entry) = self.textures.get_mut(&slot) else {
            return;
        };
        entry.count -= 1;
        if entry.count == 0 {
            let entry = self.textures.remove(&slot);
            self.texture_keys.remove(&id);
            if entry.is_some_and(|entry| entry.uploaded) {
                self.releases.push(Release::Texture(id));
            }
        }
    }

    pub fn mask(&self, id: TextureId) -> Option<Arc<AlphaMask>> {
        self.masks.get(&id).cloned()
    }

    pub fn want_mask(&mut self, id: TextureId, name: &str) {
        if self.masks.contains_key(&id) || !self.mask_wanted.insert(id) {
            return;
        }
        let Some(data) = self
            .texture_keys
            .get(&id)
            .and_then(|slot| self.textures.get(slot))
            .map(|entry| entry._data.clone())
        else {
            return;
        };
        self.mask_decode.push((id, data, name.to_owned()));
    }

    fn mask_ready(&mut self, id: TextureId, mask: AlphaMask) {
        self.masks.insert(id, Arc::new(mask));
    }

    pub fn pin(&mut self, id: TextureId) {
        self.pinned.insert(id);
    }

    pub fn texture_failed(&mut self, id: TextureId) {
        self.failed.insert(id);
    }

    pub fn texture_settled(&self, id: TextureId) -> bool {
        if self.disabled || self.failed.contains(&id) {
            return true;
        }
        self.texture_keys
            .get(&id)
            .and_then(|slot| self.textures.get(slot))
            .is_none_or(|entry| entry.uploaded || entry.pixels.is_some())
    }

    fn texture_ready(&mut self, id: TextureId, pixels: Pixels) {
        if self.mask_wanted.contains(&id) && !self.masks.contains_key(&id) {
            let (width, height, rgba) = &pixels;
            self.masks.insert(id, Arc::new(AlphaMask::build(*width, *height, rgba)));
        }
        let Some(entry) = self.texture_keys.get(&id).and_then(|slot| self.textures.get_mut(slot)) else {
            return;
        };
        if self.active {
            let (width, height, rgba) = pixels;
            self.uploads.push(Resource::Texture { id, width, height, rgba });
            entry.uploaded = true;
        } else if !self.disabled {
            entry.pixels = Some(pixels);
        }
    }

    pub fn acquire_font(&mut self, data: Arc<[u8]>) -> Option<(FontId, Font)> {
        let slot = key(&data);
        if let Some(entry) = self.fonts.get_mut(&slot) {
            entry.count += 1;
            return Some((entry.id, entry.font.clone()));
        }
        let font = Font::parse(data.clone())?;
        let id = self.allocate();
        let uploaded = self.active;
        if uploaded {
            self.uploads.push(Resource::Font { id, data });
        }
        self.fonts.insert(
            slot,
            FontEntry {
                id,
                count: 1,
                font: font.clone(),
                uploaded,
            },
        );
        self.font_keys.insert(id, slot);
        Some((id, font))
    }

    pub fn release_font(&mut self, id: FontId) {
        let Some(slot) = self.font_keys.get(&id).copied() else {
            return;
        };
        let Some(entry) = self.fonts.get_mut(&slot) else {
            return;
        };
        entry.count -= 1;
        if entry.count == 0 {
            let entry = self.fonts.remove(&slot);
            self.font_keys.remove(&id);
            if entry.is_some_and(|entry| entry.uploaded) {
                self.releases.push(Release::Font(id));
            }
        }
    }

    pub fn acquire_shader(&mut self, id: ShaderId, layout: Arc<ShaderLayout>) {
        if let Some(entry) = self.shaders.get_mut(&id) {
            entry.count += 1;
            return;
        }
        let uploaded = self.active;
        if uploaded {
            self.uploads.push(Resource::Shader {
                id,
                layout: layout.clone(),
            });
        }
        self.shaders.insert(
            id,
            ShaderEntry {
                count: 1,
                layout,
                uploaded,
            },
        );
    }

    pub fn release_shader(&mut self, id: ShaderId) {
        let Some(entry) = self.shaders.get_mut(&id) else {
            return;
        };
        entry.count -= 1;
        if entry.count == 0 {
            let entry = self.shaders.remove(&id);
            if entry.is_some_and(|entry| entry.uploaded) {
                self.releases.push(Release::Shader(id));
            }
        }
    }

    pub fn release_object(&mut self, object: &Object) {
        if let Some(image) = &object.image {
            self.release_texture(image.texture);
        }
        if let Some(text) = &object.text {
            self.release_font(text.font);
        }
        for loaded in &object.shaders {
            self.release_shader(loaded.id);
        }
        for slot in object.slots.values() {
            if let Slot::Texture { texture, .. } = slot {
                self.release_texture(*texture);
            }
        }
    }

    fn activate(&mut self) {
        self.active = true;
        for (id, slot) in &self.texture_keys {
            let Some(entry) = self.textures.get_mut(slot) else {
                continue;
            };
            if let Some((width, height, rgba)) = entry.pixels.take() {
                self.uploads.push(Resource::Texture {
                    id: *id,
                    width,
                    height,
                    rgba,
                });
                entry.uploaded = true;
            }
        }
        for entry in self.fonts.values_mut() {
            self.uploads.push(Resource::Font {
                id: entry.id,
                data: entry.font.data().clone(),
            });
            entry.uploaded = true;
        }
        for (id, entry) in &mut self.shaders {
            self.uploads.push(Resource::Shader {
                id: *id,
                layout: entry.layout.clone(),
            });
            entry.uploaded = true;
        }
    }

    fn disable(&mut self) {
        self.disabled = true;
        self.decode.clear();
        for entry in self.textures.values_mut() {
            entry.pixels = None;
        }
    }
}

pub struct Entry {
    pub userdata: AnyUserData,
    pub object: Object,
    dirty: bool,
    synced: bool,
    slots: BTreeSet<(u32, u32)>,
}

enum Link {
    Waiting,
    Disabled,
    Active(RendererHandle),
}

#[derive(Default)]
struct Slab {
    slots: Vec<Option<Entry>>,
    generations: Vec<u32>,
    free: Vec<u32>,
    len: usize,
}

fn split(id: ObjectId) -> (usize, u32) {
    ((id & 0xFFFF_FFFF) as usize, (id >> 32) as u32)
}

impl Slab {
    fn reserve(&mut self) -> ObjectId {
        let index = match self.free.pop() {
            Some(index) => index as usize,
            None => {
                self.slots.push(None);
                self.generations.push(1);
                self.slots.len() - 1
            }
        };
        (u64::from(self.generations[index]) << 32) | index as u64
    }

    fn cancel(&mut self, id: ObjectId) {
        let (index, generation) = split(id);
        if self.generations.get(index) == Some(&generation) && self.slots[index].is_none() {
            self.generations[index] = self.generations[index].wrapping_add(1).max(1);
            self.free.push(index as u32);
        }
    }

    fn insert(&mut self, id: ObjectId, entry: Entry) {
        let (index, generation) = split(id);
        if self.generations.get(index) == Some(&generation) && self.slots[index].is_none() {
            self.slots[index] = Some(entry);
            self.len += 1;
        }
    }

    fn get(&self, id: ObjectId) -> Option<&Entry> {
        let (index, generation) = split(id);
        if self.generations.get(index) != Some(&generation) {
            return None;
        }
        self.slots[index].as_ref()
    }

    fn get_mut(&mut self, id: ObjectId) -> Option<&mut Entry> {
        let (index, generation) = split(id);
        if self.generations.get(index) != Some(&generation) {
            return None;
        }
        self.slots[index].as_mut()
    }

    fn remove(&mut self, id: ObjectId) -> Option<Entry> {
        let (index, generation) = split(id);
        if self.generations.get(index) != Some(&generation) {
            return None;
        }
        let entry = self.slots[index].take()?;
        self.generations[index] = self.generations[index].wrapping_add(1).max(1);
        self.free.push(index as u32);
        self.len -= 1;
        Some(entry)
    }

    fn ids(&self) -> Vec<ObjectId> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.is_some())
            .map(|(index, _)| (u64::from(self.generations[index]) << 32) | index as u64)
            .collect()
    }

    fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.slots.iter().flatten()
    }

    fn take(&mut self) -> Vec<Entry> {
        self.len = 0;
        self.free.clear();
        self.generations.clear();
        self.slots.drain(..).flatten().collect()
    }
}

struct State {
    closed: bool,
    next_order: u64,
    entries: Slab,
    dirty: Vec<ObjectId>,
    removed: Vec<ObjectId>,
    resources: Resources,
    link: Link,
}

impl State {
    fn touch(&mut self, id: ObjectId) {
        if let Some(entry) = self.entries.get_mut(id)
            && !entry.dirty
        {
            entry.dirty = true;
            self.dirty.push(id);
        }
    }

    fn delta(&mut self) -> SceneDelta {
        let mut delta = SceneDelta {
            resources: std::mem::take(&mut self.resources.uploads),
            removals: std::mem::take(&mut self.removed),
            releases: std::mem::take(&mut self.resources.releases),
            ..SceneDelta::default()
        };
        for id in std::mem::take(&mut self.dirty) {
            let Some(entry) = self.entries.get_mut(id) else {
                continue;
            };
            if !entry.dirty {
                continue;
            }
            entry.dirty = false;
            if !entry.object.sink {
                if entry.synced {
                    delta.removals.push(id);
                    entry.synced = false;
                }
                entry.slots.clear();
                continue;
            }
            delta.upserts.push(entry.object.snapshot(id));
            let keys: Vec<(u32, u32)> = if entry.synced {
                std::mem::take(&mut entry.slots).into_iter().collect()
            } else {
                entry.slots.clear();
                entry.object.slots.keys().copied().collect()
            };
            for (group, binding) in keys {
                let content = match entry.object.slots.get(&(group, binding)) {
                    Some(slot) => slot.content(),
                    None => SlotContent::Buffer {
                        bytes: Vec::new(),
                        references: Vec::new(),
                    },
                };
                delta.slots.push(SlotWrite {
                    object: id,
                    group,
                    binding,
                    content,
                });
            }
            entry.synced = true;
        }
        delta
    }

    fn discard(&mut self) {
        for id in std::mem::take(&mut self.dirty) {
            if let Some(entry) = self.entries.get_mut(id) {
                entry.dirty = false;
                entry.slots.clear();
            }
        }
        self.removed.clear();
        self.resources.uploads.clear();
        self.resources.releases.clear();
    }
}

fn decode(data: &[u8], name: &str) -> Result<Pixels, String> {
    picture::decode(data, Some(name))
}

pub struct Scene {
    window: WindowId,
    events: WindowEvents,
    scheduler: Scheduler,
    me: Weak<Scene>,
    state: RefCell<State>,
    decoding: Cell<usize>,
    decoded: Notify,
    masking: Cell<bool>,
}

pub struct Ordering {
    pub z_index: f64,
    pub order: u64,
}

impl Scene {
    pub fn new(window: WindowId, events: WindowEvents, scheduler: Scheduler) -> Rc<Scene> {
        Rc::new_cyclic(|me| Scene {
            window,
            events,
            scheduler,
            me: me.clone(),
            state: RefCell::new(State {
                closed: false,
                next_order: 0,
                entries: Slab::default(),
                dirty: Vec::new(),
                removed: Vec::new(),
                resources: Resources::default(),
                link: Link::Waiting,
            }),
            decoding: Cell::new(0),
            decoded: Notify::new(),
            masking: Cell::new(false),
        })
    }

    pub fn me(&self) -> &Weak<Scene> {
        &self.me
    }

    pub fn is_closed(&self) -> bool {
        self.state.borrow().closed
    }

    pub fn allocate(&self) -> Option<(ObjectId, u64)> {
        let mut state = self.state.borrow_mut();
        if state.closed {
            return None;
        }
        state.next_order += 1;
        let order = state.next_order;
        Some((state.entries.reserve(), order))
    }

    pub fn cancel(&self, id: ObjectId) {
        self.state.borrow_mut().entries.cancel(id);
    }

    pub fn insert(&self, id: ObjectId, userdata: AnyUserData, object: Object) {
        let mut state = self.state.borrow_mut();
        state.entries.insert(
            id,
            Entry {
                userdata,
                object,
                dirty: false,
                synced: false,
                slots: BTreeSet::new(),
            },
        );
        state.touch(id);
    }

    pub fn read<R>(&self, id: ObjectId, action: impl FnOnce(&mut Object) -> R) -> Option<R> {
        let mut state = self.state.borrow_mut();
        state.entries.get_mut(id).map(|entry| action(&mut entry.object))
    }

    pub fn write<R>(
        &self,
        id: ObjectId,
        action: impl FnOnce(&mut Object, &mut Resources, &mut BTreeSet<(u32, u32)>) -> R,
    ) -> Option<R> {
        let mut state = self.state.borrow_mut();
        let State {
            entries,
            resources,
            dirty,
            ..
        } = &mut *state;
        let entry = entries.get_mut(id)?;
        let result = action(&mut entry.object, resources, &mut entry.slots);
        if !entry.dirty {
            entry.dirty = true;
            dirty.push(id);
        }
        Some(result)
    }

    pub fn write_changed(
        &self,
        id: ObjectId,
        action: impl FnOnce(&mut Object, &mut Resources, &mut BTreeSet<(u32, u32)>) -> mlua::Result<bool>,
    ) -> Option<mlua::Result<()>> {
        let mut state = self.state.borrow_mut();
        let State {
            entries,
            resources,
            dirty,
            ..
        } = &mut *state;
        let entry = entries.get_mut(id)?;
        let result = action(&mut entry.object, resources, &mut entry.slots);
        if !matches!(result, Ok(false)) && !entry.dirty {
            entry.dirty = true;
            dirty.push(id);
        }
        Some(result.map(|_| ()))
    }

    pub fn remove(&self, id: ObjectId) {
        let entry = {
            let mut state = self.state.borrow_mut();
            let Some(entry) = state.entries.remove(id) else {
                return;
            };
            if entry.synced {
                state.removed.push(id);
            }
            state.resources.release_object(&entry.object);
            entry
        };
        drop(entry);
    }

    pub fn userdata(&self, id: ObjectId) -> Option<AnyUserData> {
        self.state.borrow().entries.get(id).map(|entry| entry.userdata.clone())
    }

    pub fn ordering(&self, id: ObjectId) -> Option<Ordering> {
        self.state.borrow().entries.get(id).map(|entry| Ordering {
            z_index: entry.object.z_index,
            order: entry.object.order,
        })
    }

    pub fn renderables(&self) -> Vec<AnyUserData> {
        let state = self.state.borrow();
        let mut entries: Vec<&Entry> = state
            .entries
            .entries()
            .filter(|entry| entry.object.kind != Kind::Post)
            .collect();
        entries.sort_by_key(|entry| entry.object.order);
        entries.into_iter().map(|entry| entry.userdata.clone()).collect()
    }

    pub fn post_processes(&self) -> Vec<AnyUserData> {
        let state = self.state.borrow();
        let mut entries: Vec<&Entry> = state
            .entries
            .entries()
            .filter(|entry| entry.object.kind == Kind::Post)
            .collect();
        entries.sort_by(|a, b| {
            a.object
                .z_index
                .total_cmp(&b.object.z_index)
                .then(a.object.order.cmp(&b.object.order))
        });
        entries.into_iter().map(|entry| entry.userdata.clone()).collect()
    }

    pub fn count(&self) -> usize {
        self.state.borrow().entries.len
    }

    pub fn spawn_masks(&self) {
        self.masking.set(true);
        let pending = std::mem::take(&mut self.state.borrow_mut().resources.mask_decode);
        for (id, data, name) in pending {
            let scene = self.me.clone();
            let reporter = self.scheduler.clone();
            self.decoding.set(self.decoding.get() + 1);
            self.scheduler.spawn_task(async move {
                let label = name.clone();
                let result = tokio::task::spawn_blocking(move || decode(&data, &label))
                    .await
                    .unwrap_or_else(|error| Err(error.to_string()));
                let Some(scene) = scene.upgrade() else {
                    return;
                };
                match result {
                    Ok((width, height, rgba)) => scene
                        .state
                        .borrow_mut()
                        .resources
                        .mask_ready(id, AlphaMask::build(width, height, &rgba)),
                    Err(error) => {
                        reporter.report(mlua::Error::runtime(format!("cannot read the image '{name}': {error}")))
                    }
                }
                scene.decoding.set(scene.decoding.get() - 1);
                scene.decoded.notify_waiters();
            });
        }
    }

    pub fn spawn_decodes(&self) {
        let pending = std::mem::take(&mut self.state.borrow_mut().resources.decode);
        for (id, data, name) in pending {
            let scene = self.me.clone();
            let reporter = self.scheduler.clone();
            self.decoding.set(self.decoding.get() + 1);
            self.scheduler.spawn_task(async move {
                let label = name.clone();
                let result = tokio::task::spawn_blocking(move || decode(&data, &label))
                    .await
                    .unwrap_or_else(|error| Err(error.to_string()));
                let Some(scene) = scene.upgrade() else {
                    return;
                };
                match result {
                    Ok(pixels) => scene.state.borrow_mut().resources.texture_ready(id, pixels),
                    Err(error) => {
                        scene.state.borrow_mut().resources.texture_failed(id);
                        reporter.report(mlua::Error::runtime(format!("cannot draw the image '{name}': {error}")));
                    }
                }
                scene.decoding.set(scene.decoding.get() - 1);
                scene.decoded.notify_waiters();
            });
        }
    }

    pub fn preload(&self, data: Arc<[u8]>, name: &str) -> TextureId {
        let id = {
            let mut state = self.state.borrow_mut();
            let id = state.resources.acquire_texture(data, name);
            state.resources.pin(id);
            id
        };
        self.spawn_decodes();
        id
    }

    pub fn texture_settled(&self, id: TextureId) -> bool {
        self.state.borrow().resources.texture_settled(id)
    }

    pub async fn wait_for_textures(&self, ids: &[TextureId]) {
        loop {
            let decoded = self.decoded.notified();
            if ids.iter().all(|id| self.texture_settled(*id)) {
                return;
            }
            decoded.await;
        }
    }

    pub async fn warm(&self, ids: &[ObjectId]) {
        if ids.is_empty() {
            return;
        }
        self.flush();
        let (sender, receiver) = oneshot::channel();
        if self.send(RenderCommand::Warm(ids.to_vec(), sender)) {
            let _ = receiver.await;
        }
    }

    pub fn settled(&self, id: ObjectId) -> bool {
        let mut state = self.state.borrow_mut();
        let Some(entry) = state.entries.get_mut(id) else {
            return true;
        };
        let Some(image) = entry.object.image.as_ref().map(|image| image.texture) else {
            return true;
        };
        state.resources.texture_settled(image)
    }

    pub async fn wait_until_settled(&self, ids: &[ObjectId]) {
        loop {
            let decoded = self.decoded.notified();
            if ids.iter().all(|id| self.settled(*id)) {
                return;
            }
            decoded.await;
        }
    }

    async fn settle(&self) {
        loop {
            let decoded = self.decoded.notified();
            if self.decoding.get() == 0 {
                return;
            }
            decoded.await;
        }
    }

    pub fn attach(&self, target: Option<RenderTarget>) {
        let mut state = self.state.borrow_mut();
        if state.closed || !matches!(state.link, Link::Waiting) {
            return;
        }
        let Some(target) = target else {
            state.link = Link::Disabled;
            state.resources.disable();
            state.discard();
            return;
        };
        match RendererHandle::spawn(self.window, target, self.events.clone()) {
            Ok(handle) => {
                state.link = Link::Active(handle);
                state.resources.activate();
                for id in state.entries.ids() {
                    if let Some(entry) = state.entries.get_mut(id) {
                        entry.synced = false;
                    }
                    state.touch(id);
                }
            }
            Err(error) => {
                state.link = Link::Disabled;
                state.resources.disable();
                state.discard();
                drop(state);
                self.scheduler.report(mlua::Error::runtime(error));
            }
        }
    }

    fn send(&self, command: RenderCommand) -> bool {
        match &self.state.borrow().link {
            Link::Active(handle) => handle.send(command),
            _ => false,
        }
    }

    pub fn flush(&self) {
        let delta = {
            let mut state = self.state.borrow_mut();
            if !matches!(state.link, Link::Active(_)) {
                if matches!(state.link, Link::Disabled) {
                    state.discard();
                }
                return;
            }
            state.delta()
        };
        if !delta.is_empty() {
            self.send(RenderCommand::Delta(delta));
        }
    }

    pub fn present(&self, frame: FrameInfo) {
        self.flush();
        self.send(RenderCommand::Present(frame));
    }

    fn cpu_query(&self, query: &Query) -> Vec<Hit> {
        let mut state = self.state.borrow_mut();
        let colliders: Vec<_> = state
            .entries
            .ids()
            .into_iter()
            .filter_map(|id| {
                let entry = state.entries.get_mut(id)?;
                if !entry.object.sink {
                    return None;
                }
                entry.object.collider(id)
            })
            .collect();
        geometry::run(&colliders, query)
    }

    pub async fn query(&self, query: Query) -> Result<Vec<Hit>, String> {
        if self.masking.get() {
            self.settle().await;
        }
        self.flush();
        let (sender, receiver) = oneshot::channel();
        let hits = if self.send(RenderCommand::Query(query, sender)) {
            let hits = receiver
                .await
                .map_err(|_| "the renderer stopped before the query finished".to_owned())??;
            match query {
                Query::Ray { .. } => {
                    let mut state = self.state.borrow_mut();
                    hits.into_iter()
                        .map(|hit| {
                            state
                                .entries
                                .get_mut(hit.id)
                                .and_then(|entry| entry.object.collider(hit.id))
                                .and_then(|collider| geometry::test(&collider, &query))
                                .unwrap_or(hit)
                        })
                        .collect()
                }
                _ => hits,
            }
        } else {
            self.cpu_query(&query)
        };
        Ok(self.refine_alpha(&query, hits))
    }

    fn refine_alpha(&self, query: &Query, hits: Vec<Hit>) -> Vec<Hit> {
        let mut state = self.state.borrow_mut();
        if state.entries.ids().is_empty() {
            return hits;
        }
        let mut kept = Vec::with_capacity(hits.len());
        for hit in hits {
            let Some(entry) = state.entries.get_mut(hit.id) else {
                kept.push(hit);
                continue;
            };
            let Some(threshold) = entry.object.hit_threshold else {
                kept.push(hit);
                continue;
            };
            let Some(image) = &entry.object.image else {
                kept.push(hit);
                continue;
            };
            let texture = image.texture;
            let uv = entry.object.uv();
            let Some(collider) = entry.object.collider(hit.id) else {
                kept.push(hit);
                continue;
            };
            let Some(mask) = state.resources.mask(texture) else {
                kept.push(hit);
                continue;
            };
            if let Some(hit) = alpha::refine(&mask, threshold, uv, &collider, query, hit) {
                kept.push(hit);
            }
        }
        kept
    }

    pub async fn capture(&self, frame: FrameInfo) -> Result<Capture, String> {
        self.settle().await;
        self.flush();
        let (sender, receiver) = oneshot::channel();
        if !self.send(RenderCommand::Capture(frame, sender)) {
            return Err("this window is not being rendered".to_owned());
        }
        receiver
            .await
            .map_err(|_| "the renderer stopped before the capture finished".to_owned())?
    }

    pub fn close(&self) {
        let entries = {
            let mut state = self.state.borrow_mut();
            if state.closed {
                return;
            }
            state.closed = true;
            state.link = Link::Disabled;
            state.dirty.clear();
            state.removed.clear();
            state.resources = Resources {
                disabled: true,
                ..Resources::default()
            };
            state.entries.take()
        };
        for entry in &entries {
            if let Ok(mut renderable) = entry.userdata.borrow_mut::<Renderable>() {
                renderable.destroy();
            }
        }
    }
}
