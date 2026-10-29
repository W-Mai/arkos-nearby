mod resources;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use mirui::app::{RendererFactory, plugins::ImageResourcesPlugin};
use mirui::core::cache::MaxSize;
use mirui::core::model::{ModelHandle, ModelMap};
use mirui::core::reactive::Effect;
use mirui::core::resource::ResourceManager;
use mirui::prelude::*;
use mirui::render::texture::{ColorFormat, Texture, TextureMeta};
use mirui::surface::Surface;
use mirui::ui::property::{Property, PropertyChange};
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
    pub aspect: Fixed,
    #[observe]
    pub token: Option<u64>,
    pub source: String,
}

impl Default for CoverBounds {
    fn default() -> Self {
        Self {
            size: LayoutSize {
                width: Fixed::ZERO,
                height: Fixed::ZERO,
            },
            aspect: Fixed::ONE,
            token: None,
            source: String::new(),
        }
    }
}

#[model]
impl CoverBounds {
    pub fn resize(&mut self, size: LayoutSize) {
        self.size = size;
    }

    pub fn set_source(&mut self, token: Option<u64>, aspect: Fixed, source: String) {
        self.token = token;
        self.aspect = aspect;
        self.source = source;
    }

    fn source_value(&self) -> String {
        self.source.clone()
    }
}

impl CoverBoundsHandle {
    fn source(&self) -> String {
        self.token();
        self.source_value()
    }
}

struct ArtworkSource;

impl Property for ArtworkSource {
    type Value = String;

    fn apply(world: &mut mirui::ecs::World, entity: Entity, value: String) -> PropertyChange {
        if world.get::<Image>(entity).is_some_and(|image| matches!(&image.src, mirui::ui::widgets::image::ImageSource::Texture(token) if token.as_ref()==value)) {
            return PropertyChange::Unchanged;
        }
        world.insert(entity, Image::new(value));
        PropertyChange::Visual
    }
}

fn source_key(path: String) -> ArtworkKey {
    let stamp = std::fs::metadata(&path)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()));
    (path, stamp)
}

#[model]
struct ReadyImage {
    #[observe]
    ready: bool,
    #[observe]
    token: Option<u64>,
    #[observe]
    aspect: Fixed,
    source: String,
    targets: Vec<ArtworkTarget>,
}

impl Default for ReadyImage {
    fn default() -> Self {
        Self {
            ready: false,
            token: None,
            aspect: Fixed::ONE,
            source: String::new(),
            targets: Vec::new(),
        }
    }
}

#[model]
impl ReadyImage {
    fn complete(&mut self, token: Option<u64>, aspect: Fixed) {
        self.ready = true;
        self.token = token;
        self.aspect = aspect;
        self.publish();
    }

    fn bind(&mut self, source: String, targets: Vec<ArtworkTarget>) {
        self.source = source;
        self.targets = targets;
        self.publish();
    }

    fn publish(&self) {
        let source = if self.token.is_some() {
            self.source.as_str()
        } else {
            ""
        };
        for target in &self.targets {
            target.publish(self.token, self.aspect, source);
        }
    }
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
    fn publish(&self, token: Option<u64>, aspect: Fixed, source: &str) {
        // A completion may outlive a collection member or its selected source.
        match &self.target {
            Target::Display(display)
                if display.callback_ready() && display.artwork() == self.path =>
            {
                display
                    .cover_bounds()
                    .set_source(token, aspect, source.into());
                display.set_artwork_token(token);
            }
            Target::Row(row) if row.callback_ready() && row.artwork() == self.path => {
                row.cover_bounds().set_source(token, aspect, source.into());
                row.set_artwork_token(token);
            }
            _ => {}
        }
    }
}

#[model]
struct Artwork {
    assets: model!(owned ModelMap<String, ReadyImage>),
}

impl Default for Artwork {
    fn default() -> Self {
        Self {
            assets: ModelMap::with_capacity(CACHE_ENTRIES).into(),
        }
    }
}

#[model]
impl Artwork {}

// Active demand and presentation only; decoded pixels reside in ResourceManager.
struct Lease {
    key: ArtworkKey,
    source: String,
    state: ReadyImageHandle,
    version_checked: bool,
    _subscription: Effect,
}

struct DisplayArtwork {
    display: super::state::DisplayHandle,
    path: String,
    rows: Vec<(super::rows::RowHandle, String)>,
}

struct ArtworkPlan {
    model: ArtworkHandle,
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
            .filter(|entry| !entry.state.ready())
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
        let assets = self.model.assets();
        if assets.len() == assets.capacity() {
            assets
                .reserve(CACHE_ENTRIES)
                .expect("artwork demand capacity");
        }
        assets
            .try_insert(source.clone(), ReadyImage::default())
            .unwrap_or_else(|(error, _, _)| panic!("artwork model insertion: {error:?}"));
        let state = assets
            .get(source.as_str())
            .expect("registered artwork state");
        let resource = textures.load(source.clone());
        let signal = textures.subscribe(&source);
        let state_changed = state.clone();
        let expected = key.clone();
        let initialized = std::cell::Cell::new(false);
        let subscription = Effect::new(move || {
            signal.get();
            let image = resource.get();
            let notified = initialized.replace(true);
            if image.cache_revision != 0 || notified {
                let token = (image.cache_revision != 0
                    && source_key(expected.0.clone()) == expected)
                    .then_some(image.cache_revision);
                let aspect = if token.is_some() {
                    Fixed::from_int(i32::from(image.width()))
                        / Fixed::from_int(i32::from(image.height()))
                } else {
                    Fixed::ONE
                };
                state_changed.complete(token, aspect);
            }
        });
        self.requests.push(Lease {
            key,
            source,
            state,
            version_checked: false,
            _subscription: subscription,
        });
        self.bindings_dirty = true;
        true
    }

    fn refresh_sources(&mut self) {
        let mut changed = Vec::new();
        for entry in &mut self.requests {
            if !entry.version_checked && entry.state.ready() {
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
            let mut retired = None;
            self.requests.retain(|entry| {
                if entry.key == old {
                    retired = Some(entry.source.clone());
                    false
                } else {
                    true
                }
            });
            if let Some(token) = retired {
                self.model
                    .assets()
                    .remove(token.as_str())
                    .expect("retire changed artwork");
            }
            self.bindings_dirty = true;
        }
    }

    fn retire(&mut self) {
        let mut retired = Vec::new();
        self.requests.retain(|entry| {
            let keep = entry.key == self.key
                || self.visible.contains(&entry.key)
                || self.neighbors.contains(&entry.key.0)
                || self
                    .displays
                    .iter()
                    .any(|display| display.path == entry.key.0);
            if !keep {
                retired.push(entry.source.clone());
            }
            keep
        });
        let assets = self.model.assets();
        for token in retired {
            assets.remove(token.as_str()).expect("retire artwork model");
        }
    }

    fn bind_targets(&mut self) {
        let mut targets: Vec<Vec<ArtworkTarget>> =
            (0..self.requests.len()).map(|_| Vec::new()).collect();
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
                if let Some(source) = self.source(&target.path) {
                    let index = self
                        .requests
                        .iter()
                        .position(|entry| entry.key == source.key)
                        .expect("active artwork demand");
                    targets[index].push(target);
                } else {
                    target.publish(None, Fixed::ONE, "");
                }
            }
        }
        for (entry, targets) in self.requests.iter().zip(targets) {
            entry.state.bind(entry.source.clone(), targets);
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
    let model = app.add_model(Artwork::default()).finish();
    app.add_resource(ArtworkPlan {
        model,
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
        loader.bind_targets();
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
                move || state.selected_platform()
            )
        }
    }
}

#[compose]
pub(super) fn cover<P>(bounds: model!(CoverBounds), platform: P) -> Entity
where
    P: Fn() -> super::consoles::Platform + 'static,
{
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
                    walk ${ std::iter::once(platform()) } with kind by kind {
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
    let image = ui! {
        Image (
            src: "",
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
    };
    cx.bind_property::<ArtworkSource>(image, move || source.source());
    image
}
