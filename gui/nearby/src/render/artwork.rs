mod resources;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use std::{cell::Cell, rc::Rc};

use mirui::app::{RendererFactory, plugins::ImageResourcesPlugin};
use mirui::core::cache::{Handle, MaxSize};
use mirui::core::model::ModelHandle;
use mirui::core::reactive::Effect;
use mirui::core::resource::{ResourceHandle, ResourceManager};
use mirui::prelude::*;
use mirui::render::texture::{ColorFormat, Texture, TextureMeta};
use mirui::surface::Surface;
use mirui::ui::widgets::Image;

use super::consoles::console;
use super::{state::DisplayState, style::*};

const PREVIEW_WIDTH: u32 = 256;
const PREVIEW_HEIGHT: u32 = 192;
const DEBOUNCE: Duration = Duration::from_millis(120);
const CACHE_ENTRIES: usize = 16;
const MAX_LOADING: usize = 2;
const TEXTURE_BUDGET: usize = CACHE_ENTRIES * PREVIEW_WIDTH as usize * PREVIEW_HEIGHT as usize * 4;
type ArtworkKey = (String, Option<(u64, Option<std::time::SystemTime>)>);

#[model]
pub(super) struct CoverBounds {
    #[observe]
    pub size: LayoutSize,
    #[observe]
    binding_revision: u64,
    source: String,
    resource: Option<ResourceHandle<Texture<'static>>>,
    signal: Option<Rc<mirui::core::reactive::Signal<()>>>,
}

impl Default for CoverBounds {
    fn default() -> Self {
        Self {
            size: LayoutSize {
                width: Fixed::ZERO,
                height: Fixed::ZERO,
            },
            binding_revision: 0,
            source: String::new(),
            resource: None,
            signal: None,
        }
    }
}

#[model]
impl CoverBounds {
    pub fn resize(&mut self, size: LayoutSize) {
        self.size = size;
    }

    fn bind(
        &mut self,
        source: String,
        resource: Option<ResourceHandle<Texture<'static>>>,
        signal: Option<Rc<mirui::core::reactive::Signal<()>>>,
    ) {
        if self.source == source {
            return;
        }
        self.source = source;
        self.resource = resource;
        self.signal = signal;
        self.binding_revision = self.binding_revision.wrapping_add(1);
    }

    fn source_value(&self) -> String {
        self.source.clone()
    }

    fn resolved(&self) -> Option<Handle<Texture<'static>>> {
        self.signal.as_ref()?.get();
        let texture = self.resource.as_ref()?.get();
        (texture.cache_revision != 0).then_some(texture)
    }
}

impl CoverBoundsHandle {
    fn image(&self) -> Option<Handle<Texture<'static>>> {
        self.binding_revision();
        self.resolved()
    }

    pub(super) fn token(&self) -> Option<u64> {
        self.image().map(|texture| texture.cache_revision)
    }

    fn aspect(&self) -> Fixed {
        self.image().map_or(Fixed::ONE, |texture| {
            Fixed::from_int(i32::from(texture.width()))
                / Fixed::from_int(i32::from(texture.height()))
        })
    }

    fn source(&self) -> String {
        if self.token().is_some() {
            self.source_value()
        } else {
            String::new()
        }
    }
}

pub(super) fn save_source(
    path: String,
    revision: Option<(u64, Option<std::time::SystemTime>)>,
) -> String {
    if path.is_empty() {
        String::new()
    } else {
        resources::token(&(path, revision))
    }
}

fn source_key(path: String) -> ArtworkKey {
    let stamp = std::fs::metadata(&path)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()));
    (path, stamp)
}

#[derive(Clone)]
enum Target {
    Display(super::state::DisplayHandle),
    Row(super::rows::RowHandle),
}

#[derive(Clone)]
struct ArtworkTarget {
    target: Target,
    path: String,
}

impl ArtworkTarget {
    fn bind(&self, source: Option<&Lease>, textures: &ResourceManager<Texture<'static>>) {
        let bounds = match &self.target {
            Target::Display(display)
                if display.callback_ready() && display.artwork() == self.path =>
            {
                display.cover_bounds()
            }
            Target::Row(row) if row.callback_ready() && row.artwork() == self.path => {
                row.cover_bounds()
            }
            _ => return,
        };
        match source {
            Some(source) => bounds.bind(
                source.source.clone(),
                Some(source.resource.clone()),
                Some(textures.subscribe(&source.source)),
            ),
            None => bounds.bind(String::new(), None, None),
        };
    }
}

// Active demand and presentation only; decoded pixels reside in ResourceManager.
struct Lease {
    key: ArtworkKey,
    source: String,
    resource: ResourceHandle<Texture<'static>>,
    ready: Rc<Cell<bool>>,
    version_checked: bool,
    _subscription: Effect,
}

struct DisplayArtwork {
    display: super::state::DisplayHandle,
    path: String,
    rows: Vec<(super::rows::RowHandle, String)>,
}

struct ArtworkPlan {
    requests: Vec<Lease>,
    path: String,
    key: ArtworkKey,
    changed: Instant,
    pending: bool,
    neighbors: Vec<String>,
    prefetch: VecDeque<ArtworkKey>,
    visible: Vec<ArtworkKey>,
    plan: Option<(usize, [u64; 4])>,
    displays: Vec<DisplayArtwork>,
    bindings_dirty: bool,
}

impl ArtworkPlan {
    fn source(&self, path: &str) -> Option<&Lease> {
        let key = if path == self.path {
            Some(&self.key)
        } else {
            self.visible.iter().find(|key| key.0 == path)
        };
        self.requests
            .iter()
            .find(|entry| key.map_or(entry.key.0 == path, |key| entry.key == *key))
    }

    fn loading(&self) -> usize {
        self.requests
            .iter()
            .filter(|entry| !entry.ready.get())
            .count()
    }

    fn request(&mut self, key: ArtworkKey, textures: &ResourceManager<Texture<'static>>) -> bool {
        if self.requests.iter().any(|entry| entry.key == key) {
            return true;
        }
        if self.loading() >= MAX_LOADING {
            return false;
        }
        let source = resources::token(&key);
        let resource = textures.load(source.clone());
        let signal = textures.subscribe(&source);
        let ready = Rc::new(Cell::new(false));
        let completed = ready.clone();
        let demand = resource.clone();
        let initialized = Cell::new(false);
        let subscription = Effect::new(move || {
            signal.get();
            let image = demand.get();
            let notified = initialized.replace(true);
            if image.cache_revision != 0 || notified {
                completed.set(true);
            }
        });
        self.requests.push(Lease {
            key,
            source,
            resource,
            ready,
            version_checked: false,
            _subscription: subscription,
        });
        self.bindings_dirty = true;
        true
    }

    fn refresh_sources(&mut self) {
        let mut changed = Vec::new();
        for entry in &mut self.requests {
            if !entry.version_checked && entry.ready.get() {
                entry.version_checked = true;
                let current = source_key(entry.key.0.clone());
                if current != entry.key {
                    changed.push((entry.key.clone(), current));
                }
            }
        }
        for (old, current) in changed {
            for key in self.visible.iter_mut().chain(self.prefetch.iter_mut()) {
                if *key == old {
                    *key = current.clone();
                }
            }
            if self.key == old {
                self.key = current;
                self.pending = true;
            }
            self.requests.retain(|entry| entry.key != old);
            self.bindings_dirty = true;
        }
    }

    fn retire(&mut self) {
        self.requests.retain(|entry| {
            entry.key == self.key
                || self.visible.contains(&entry.key)
                || self.neighbors.contains(&entry.key.0)
                || self
                    .displays
                    .iter()
                    .any(|display| display.path == entry.key.0)
        });
    }

    fn bind_targets(&mut self, textures: &ResourceManager<Texture<'static>>) {
        for display in &self.displays {
            let visible = ArtworkTarget {
                target: Target::Display(display.display.clone()),
                path: display.path.clone(),
            };
            let rows = display.rows.iter().map(|(row, path)| ArtworkTarget {
                target: Target::Row(row.clone()),
                path: path.clone(),
            });
            for target in std::iter::once(visible).chain(rows) {
                target.bind(self.source(&target.path), textures);
            }
        }
        self.bindings_dirty = false;
    }
}

pub(super) fn install<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
) -> Result<(), String> {
    app.add_plugin(
        ImageResourcesPlugin::empty()
            .with_values_budget(MaxSize::Bytes(TEXTURE_BUDGET))
            .with_loader(|images| {
                images.set_fallback(
                    Texture::owned(1, 1, ColorFormat::RGBA8888).expect("artwork fallback"),
                );
                images.set_fallback_meta(TextureMeta {
                    width: 1,
                    height: 1,
                    format: ColorFormat::RGBA8888,
                });
                images.add_probed_loader(resources::ArtworkDecoder::new());
            }),
    );
    app.add_resource(ArtworkPlan {
        requests: Vec::new(),
        path: String::new(),
        key: (String::new(), None),
        changed: Instant::now(),
        pending: false,
        neighbors: Vec::new(),
        prefetch: VecDeque::new(),
        visible: Vec::new(),
        plan: None,
        displays: Vec::new(),
        bindings_dirty: false,
    });
    let root = app.root.unwrap_or_else(|| app.spawn_root().id());
    let display = app.compose(root, super::state::display_state());
    app.add_system(update::system(display));
    Ok(())
}

#[system(order = Animation)]
fn update(
    state: model!(DisplayState),
    retained: res!(super::state::DisplayRuntime),
    loader: res!(mut ArtworkPlan),
    textures: res!(ResourceManager<Texture<'static>>),
) {
    let displays = [state, &retained.shelf, &retained.catalog, &retained.room];
    let count = if retained.deck.enabled() { 4 } else { 1 };
    let mut revisions = [0; 4];
    for (revision, display) in revisions.iter_mut().zip(&displays[..count]) {
        *revision = display.artwork_revision();
    }
    if loader.plan != Some((count, revisions)) {
        let mut visible = Vec::new();
        loader.displays = displays[..count]
            .iter()
            .map(|display| {
                let favorites = display.library_kind() == crate::model::LibraryKind::Favorites;
                let rows = display
                    .entries()
                    .entries()
                    .map(|(_, row)| {
                        let path = row.artwork();
                        if favorites && !path.is_empty() && !visible.contains(&path) {
                            visible.push(path.clone());
                        }
                        (row, path)
                    })
                    .collect();
                DisplayArtwork {
                    display: (*display).clone(),
                    path: display.artwork(),
                    rows,
                }
            })
            .collect();
        let path = state.artwork();
        let mut neighbors = state.artwork_neighbors();
        if retained.deck.enabled() {
            neighbors.insert(0, retained.shelf.artwork());
            neighbors.insert(1, retained.catalog.artwork());
        }
        if path != loader.path || neighbors != loader.neighbors {
            loader.prefetch = neighbors
                .iter()
                .filter(|path| !path.is_empty())
                .take(4)
                .cloned()
                .map(source_key)
                .collect();
            loader.neighbors = neighbors;
        }
        if path != loader.path || loader.visible.iter().map(|key| &key.0).ne(visible.iter()) {
            loader.visible = visible.into_iter().map(source_key).collect();
        }
        if path != loader.path {
            loader.path = path;
            loader.key = source_key(loader.path.clone());
            loader.changed = Instant::now();
            loader.pending = !loader.path.is_empty();
        }
        loader.retire();
        loader.bindings_dirty = true;
        loader.plan = Some((count, revisions));
    }
    loader.refresh_sources();
    let visible = std::mem::take(&mut loader.visible);
    for key in &visible {
        if !loader.request(key.clone(), &textures) {
            break;
        }
    }
    loader.visible = visible;
    if loader.pending
        && (loader.requests.iter().any(|entry| entry.key == loader.key)
            || loader.changed.elapsed() >= DEBOUNCE)
    {
        let key = loader.key.clone();
        if loader.request(key, &textures) {
            loader.pending = false;
        }
    }
    if !loader.pending && loader.loading() == 0 && loader.changed.elapsed() >= DEBOUNCE {
        while let Some(key) = loader.prefetch.front().cloned() {
            let existed = loader.requests.iter().any(|entry| entry.key == key);
            if loader.request(key, &textures) {
                loader.prefetch.pop_front();
            }
            if !existed {
                break;
            }
        }
    }
    if loader.bindings_dirty {
        loader.bind_targets(&textures);
    }
}

#[compose]
pub(super) fn preview(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            bg_color: PANEL,
            padding: Padding::all(2),
            justify: JustifyContent::Center,
            row_gap: metrics.gap
        ) {
            cover (
                state.cover_bounds(),
                ${ state.selected_platform() }
            )
        }
    }
}

#[compose]
pub(super) fn cover(
    bounds: model!(CoverBounds),
    platform: prop!(super::consoles::Platform),
) -> Entity {
    let fallback = bounds.clone();
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            min_width: 0,
            container: true,
            clip_children: true,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            bitmap (bounds)
            if ${ fallback.token().is_none() } {
                Column (grow: 1.0, min_height: 0, width: Dimension::percent(100)) {
                    walk ${ std::iter::once(platform.get()) } with kind by kind {
                        console (kind)
                    }
                }
            }
        } on SizeChanged { bounds.resize(event.new); }
    }
}

#[compose]
fn bitmap(bounds: model!(CoverBounds)) -> Entity {
    let visible = bounds.clone();
    let source = bounds.clone();
    let height = bounds.clone();
    let horizontal = bounds.clone();
    let vertical = bounds.clone();
    ui! {
        Image (
            src: ${ source.source() },
            visible: ${ visible.token().is_some() },
            position: Position::Absolute,
            shrink: 0.0,
            width: ${ bounds.size().width.max(bounds.size().height * bounds.aspect()) },
            height: ${ height.size().height.max(height.size().width / height.aspect()) },
            left: ${
                (horizontal.size().width
                    - horizontal.size().width.max(horizontal.size().height * horizontal.aspect()))
                    / 2
            },
            top: ${
                (vertical.size().height
                    - vertical.size().height.max(vertical.size().width / vertical.aspect())) / 2
            }
        )
    }
}
