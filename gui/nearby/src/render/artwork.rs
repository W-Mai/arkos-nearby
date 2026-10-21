use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

use mirui::app::RendererFactory;
use mirui::core::cache::MaxSize;
use mirui::core::resource::ResourceManager;
use mirui::prelude::*;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::surface::Surface;
use mirui::ui::widgets::Image;

use super::consoles::console;
use super::{state::DisplayState, style::*};

const PREVIEW_WIDTH: u32 = 256;
const PREVIEW_HEIGHT: u32 = 192;
const DEBOUNCE: Duration = Duration::from_millis(120);
const CACHE_ENTRIES: usize = 16;
const TEXTURE_BUDGET: usize = CACHE_ENTRIES * PREVIEW_WIDTH as usize * PREVIEW_HEIGHT as usize * 4;
type ArtworkKey = (String, Option<(u64, Option<std::time::SystemTime>)>);

#[model]
pub(super) struct CoverBounds {
    #[observe]
    pub size: LayoutSize,
}

impl Default for CoverBounds {
    fn default() -> Self {
        Self {
            size: LayoutSize {
                width: Fixed::ZERO,
                height: Fixed::ZERO,
            },
        }
    }
}

#[model]
impl CoverBounds {
    pub fn resize(&mut self, size: LayoutSize) {
        self.size = size;
    }
}

fn texture_token(id: u64) -> String {
    format!("nearby-cover-{id}")
}

fn source_key(path: String) -> ArtworkKey {
    let stamp = std::fs::metadata(&path)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()));
    (path, stamp)
}

#[model]
#[derive(Default)]
struct Artwork {
    #[observe]
    token: Option<u64>,
}

#[model]
impl Artwork {
    fn show(&mut self, token: Option<u64>) {
        self.token = token;
    }
}

#[derive(Clone)]
struct Pixels {
    bytes: Vec<u8>,
    width: u16,
    height: u16,
}

struct Loader {
    model: ArtworkHandle,
    send: SyncSender<ArtworkKey>,
    receive: Receiver<(ArtworkKey, Option<Pixels>)>,
    path: String,
    key: ArtworkKey,
    changed: Instant,
    pending: bool,
    serial: u64,
    cached: VecDeque<(ArtworkKey, Option<u64>)>,
    in_flight: Vec<ArtworkKey>,
    neighbors: Vec<String>,
    prefetch: VecDeque<ArtworkKey>,
    visible: Vec<ArtworkKey>,
}

impl Loader {
    fn token(&self, path: &str) -> Option<u64> {
        let key = if path == self.path {
            Some(&self.key)
        } else {
            self.visible.iter().find(|key| key.0 == path)
        };
        self.cached
            .iter()
            .rev()
            .find(|(cached, _)| key.map_or(cached.0 == path, |key| cached == key))
            .and_then(|(_, token)| *token)
    }

    // A resident resource is reusable without decoding, copying pixels or waiting.
    fn cached(
        &mut self,
        key: &ArtworkKey,
        textures: &ResourceManager<Texture<'static>>,
    ) -> Option<Option<u64>> {
        let index = self.cached.iter().position(|(old, _)| old == key)?;
        let entry = self.cached.remove(index)?;
        if entry
            .1
            .as_ref()
            .is_some_and(|token| textures.resolve(&texture_token(*token)).width() == 1)
        {
            if let Some(token) = &entry.1 {
                textures.remove_token(&texture_token(*token));
            }
            return None;
        }
        let token = entry.1;
        self.cached.push_back(entry);
        Some(token)
    }

    fn enqueue(&mut self, key: ArtworkKey) -> bool {
        if self.in_flight.contains(&key) {
            return true;
        }
        if self.in_flight.len() >= 2 || self.send.try_send(key.clone()).is_err() {
            return false;
        }
        self.in_flight.push(key);
        true
    }

    fn accept(
        &mut self,
        key: ArtworkKey,
        pixels: Option<Pixels>,
        textures: &ResourceManager<Texture<'static>>,
    ) {
        self.in_flight.retain(|old| old != &key);
        // A result for another selection can populate the cache, but cannot change the view.
        let current = source_key(key.0.clone());
        if current != key {
            for visible in &mut self.visible {
                if *visible == key {
                    *visible = current.clone();
                }
            }
            if self.key == key {
                self.key = current;
                self.pending = true;
            }
            return;
        }
        if let Some(index) = self.cached.iter().position(|(old, _)| old == &key) {
            if let Some(token) = self.cached.remove(index).and_then(|entry| entry.1) {
                textures.remove_token(&texture_token(token));
            }
        }
        let token = pixels.and_then(|pixels| {
            let texture = Texture::from_vec(
                pixels.bytes,
                pixels.width,
                pixels.height,
                ColorFormat::RGBA8888,
            )
            .ok()?;
            self.serial += 1;
            let token = self.serial;
            // Published artwork is immutable. A new admission gets a new
            // revision even if its allocation reuses a previous buffer address.
            let texture = texture.with_cache_revision(token);
            textures.add_factory(texture_token(token), move || Some(texture));
            textures.prefetch(&texture_token(token));
            Some(token)
        });
        while self.cached.len() >= CACHE_ENTRIES {
            let evict = self
                .cached
                .iter()
                .position(|(key, _)| key != &self.key && !self.visible.contains(key))
                .unwrap_or(0);
            if let Some((_, Some(token))) = self.cached.remove(evict) {
                textures.remove_token(&texture_token(token));
            }
        }
        self.cached.push_back((key.clone(), token));
        if key == self.key {
            self.model.show(token);
        }
    }
}

fn decode(path: &str) -> Option<Pixels> {
    let path = Path::new(path).canonicalize().ok()?;
    if !["/roms", "/roms2"]
        .iter()
        .filter_map(|root| Path::new(root).canonicalize().ok())
        .any(|root| path.starts_with(root))
        || path.metadata().ok()?.len() > 4 * 1024 * 1024
    {
        return None;
    }
    let mut reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(24 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .ok()?
        .thumbnail(PREVIEW_WIDTH, PREVIEW_HEIGHT)
        .to_rgba8();
    Some(Pixels {
        width: image.width() as u16,
        height: image.height() as u16,
        bytes: image.into_raw(),
    })
}

pub(super) fn install<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
) -> Result<(), String> {
    let (send, jobs) = mpsc::sync_channel::<ArtworkKey>(1);
    let (results, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut cache: VecDeque<(ArtworkKey, Option<Pixels>)> = VecDeque::new();
        while let Ok(key) = jobs.recv() {
            let pixels = if let Some(index) = cache.iter().position(|(old, _)| old == &key) {
                let item = cache.remove(index).expect("Located cached artwork");
                let pixels = item.1.clone();
                cache.push_back(item);
                pixels
            } else {
                let pixels = decode(&key.0);
                if cache.len() == CACHE_ENTRIES {
                    cache.pop_front();
                }
                cache.push_back((key.clone(), pixels.clone()));
                pixels
            };
            if results.send((key, pixels)).is_err() {
                break;
            }
        }
    });
    let model = app.add_model(Artwork::default()).finish();
    app.add_resource(Loader {
        model,
        send,
        receive,
        path: String::new(),
        key: (String::new(), None),
        changed: Instant::now(),
        pending: false,
        serial: 0,
        cached: VecDeque::new(),
        in_flight: Vec::new(),
        neighbors: Vec::new(),
        prefetch: VecDeque::new(),
        visible: Vec::new(),
    });
    if app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .is_none()
    {
        app.add_resource(ResourceManager::new(
            MaxSize::Bytes(TEXTURE_BUDGET),
            Texture::owned(1, 1, ColorFormat::RGBA8888)
                .map_err(|error| format!("Artwork texture: {error:?}"))?,
        ));
    }
    let root = app.root.unwrap_or_else(|| app.spawn_root().id());
    let display = app.compose(root, super::state::display_state());
    app.add_system(update::system(display));
    Ok(())
}

#[system(order = Animation)]
fn update(
    state: model!(DisplayState),
    retained: res!(super::state::DisplayRuntime),
    loader: res!(mut Loader),
    textures: res!(ResourceManager<Texture<'static>>),
) {
    let mut displays = vec![state];
    if retained.deck.enabled() {
        displays.extend([&retained.shelf, &retained.catalog, &retained.room]);
    }
    let mut visible = Vec::new();
    for display in &displays {
        if display.library_kind() == crate::model::LibraryKind::Favorites {
            for (_, row) in display.entries().entries() {
                let path = row.artwork();
                if !path.is_empty() && !visible.contains(&path) {
                    visible.push(path);
                }
            }
        }
    }
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
        let key = loader.key.clone();
        if let Some(token) = loader.cached(&key, &textures) {
            loader.model.show(token);
            loader.pending = false;
        } else {
            loader.model.show(None);
        }
    }
    while let Ok((key, pixels)) = loader.receive.try_recv() {
        loader.accept(key, pixels, &textures);
    }
    // Visible cards take priority over the selected-preview debounce and neighbor prefetch.
    for key in loader.visible.clone() {
        if loader.cached(&key, &textures).is_none() && !loader.enqueue(key) {
            break;
        }
    }
    let selected_key = loader.key.clone();
    if loader.pending && loader.cached(&selected_key, &textures).is_some() {
        loader.pending = false;
    }
    if loader.pending && loader.changed.elapsed() >= DEBOUNCE {
        let key = loader.key.clone();
        if loader.enqueue(key) {
            loader.pending = false;
        }
    }
    if !loader.pending && loader.in_flight.is_empty() && loader.changed.elapsed() >= DEBOUNCE {
        while let Some(key) = loader.prefetch.front().cloned() {
            if loader.cached(&key, &textures).is_none() {
                if loader.enqueue(key) {
                    loader.prefetch.pop_front();
                }
                break;
            }
            loader.prefetch.pop_front();
        }
    }
    for display in displays {
        let path = display.artwork();
        let token = loader.token(&path);
        display.set_artwork_token(token);
        for (_, row) in display.entries().entries() {
            let token = loader.token(&row.artwork());
            if row.artwork_token() != token {
                row.set_artwork_token(token);
            }
        }
    }
}

#[compose]
pub(super) fn preview(state: model!(DisplayState), metrics: Metrics) -> Entity {
    let selected = state.clone();
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
                move || selected.artwork_token(),
                move || state.selected_platform()
            )
        }
    }
}

#[compose]
pub(super) fn cover<T, P>(bounds: model!(CoverBounds), token: T, platform: P) -> Entity
where
    T: Fn() -> Option<u64> + Clone + 'static,
    P: Fn() -> super::consoles::Platform + 'static,
{
    let fallback = token.clone();
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
            walk ${ token().into_iter() } with id by id {
                bitmap (bounds, id)
            }
            if ${ fallback().is_none() } {
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
fn bitmap(bounds: model!(CoverBounds), token: u64) -> Entity {
    let aspect = res!(ResourceManager<Texture<'static>>).with(|textures| {
        let image = textures.resolve(&texture_token(token));
        Fixed::from_int(i32::from(image.width())) / Fixed::from_int(i32::from(image.height()))
    });
    let height = bounds.clone();
    let horizontal = bounds.clone();
    let vertical = bounds.clone();
    ui! {
        Image (
            src: texture_token(token),
            position: Position::Absolute,
            shrink: 0.0,
            width: ${ bounds.size().width.max(bounds.size().height * aspect) },
            height: ${ height.size().height.max(height.size().width / aspect) },
            left: ${
                (horizontal.size().width
                    - horizontal.size().width.max(horizontal.size().height * aspect)) / 2
            },
            top: ${
                (vertical.size().height - vertical.size().height.max(vertical.size().width / aspect))
                    / 2
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Page, View};
    use mirui::surface::framebuf::FramebufSurface;
    use mirui::ui::ComputedRect;

    #[test]
    fn visible_cards_share_images_across_selection_reorder_and_retained_pages() {
        use crate::model::{GameRow, LibraryKind};
        let surface = FramebufSurface::with_format(640, 480, ColorFormat::RGBA8888, |_, _| {});
        let mut app = App::new(surface);
        app.with_default_widgets().with_default_systems();
        super::super::install(&mut app).unwrap();
        let (jobs, queued) = mpsc::sync_channel(8);
        let (results, receive) = mpsc::sync_channel(8);
        {
            let loader = app.world.resource_mut::<Loader>().unwrap();
            loader.send = jobs;
            loader.receive = receive;
        }
        let mut view = View {
            page: Page::Games,
            entries: (0..6)
                .map(|i| GameRow {
                    key: format!("game-{i}"),
                    artwork: format!("cover-{i}"),
                    icon: "nes".into(),
                    name: format!("Game {i}"),
                    ..GameRow::default()
                })
                .collect(),
            ..View::default()
        };
        view.library.kind = LibraryKind::Favorites;
        view.launcher.enabled = true;
        let mut scene = super::super::draw(&mut app, &view).unwrap();
        // Cold visible cards start loading without waiting for selection or debounce.
        for time in [10, 20, 30, 40] {
            super::super::paint_at(&mut app, time).unwrap();
            for (i, key) in queued.try_iter().enumerate() {
                results
                    .send((
                        key,
                        Some(Pixels {
                            bytes: [i as u8 * 90, 100, 220, 255].repeat(4),
                            width: 2,
                            height: 2,
                        }),
                    ))
                    .unwrap();
            }
        }
        let tokens = |display: &super::super::state::DisplayHandle| {
            display
                .entries()
                .entries()
                .map(|(_, row)| {
                    (
                        row.artwork(),
                        row.artwork_token()
                            .expect("Every visible card has its own image"),
                    )
                })
                .collect::<Vec<_>>()
        };
        let original = tokens(&scene.shelf);
        assert_eq!(original.len(), 6);
        assert!(original.iter().enumerate().all(|(i, (_, token))| {
            original.iter().skip(i + 1).all(|(_, other)| token != other)
        }));
        let textures = app
            .world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap();
        let image = textures.resolve(&texture_token(original[0].1));
        let rendered = app.world.query::<Image>().iter().filter(|(entity, image)| {
            app.world.get::<ComputedRect>(*entity).is_some()
                && matches!(&image.src, mirui::ui::widgets::image::ImageSource::Texture(token) if original.iter().any(|(_, id)| *token == texture_token(*id)))
        }).count();
        assert_eq!(rendered, 6);
        for (entity, image) in app.world.query::<Image>().iter() {
            if matches!(&image.src, mirui::ui::widgets::image::ImageSource::Texture(token) if original.iter().any(|(_, id)| *token == texture_token(*id)))
            {
                let area = app.world.get::<ComputedRect>(entity).unwrap().0;
                assert!(area.w > Fixed::ZERO && area.h > Fixed::ZERO);
                let parent = app.world.get::<mirui::ui::Parent>(entity).unwrap().0;
                let viewport = app.world.get::<ComputedRect>(parent).unwrap().0;
                assert!(area.w >= viewport.w && area.h >= viewport.h);
            }
        }

        view.selected = 1;
        view.entries.swap(0, 2);
        super::super::update(&mut scene, &view).unwrap();
        super::super::paint_at(&mut app, 50).unwrap();
        let mut reordered = original.clone();
        reordered.swap(0, 2);
        assert_eq!(tokens(&scene.shelf), reordered);
        assert!(queued.try_recv().is_err());

        let shelf = view.clone();
        view.launcher_shelf = Some(Box::new(shelf.clone()));
        view.launcher.tab = 1;
        view.library.kind = LibraryKind::Directory;
        view.entries.clear();
        super::super::update(&mut scene, &view).unwrap();
        super::super::paint_at(&mut app, 60).unwrap();
        assert_eq!(tokens(&scene.shelf).len(), 6);
        super::super::update(&mut scene, &shelf).unwrap();
        super::super::paint_at(&mut app, 70).unwrap();
        assert!(queued.try_recv().is_err());
        let reused = app
            .world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap()
            .resolve(&texture_token(original[0].1));
        assert!(std::rc::Rc::ptr_eq(&image, &reused));

        // Reusing a row identity with another image clears only that card.
        let mut replacement = shelf;
        replacement.entries[0].artwork = "replacement".into();
        replacement.entries[1].artwork.clear();
        super::super::update(&mut scene, &replacement).unwrap();
        let rows: Vec<_> = scene.shelf.entries().entries().collect();
        assert!(rows[0].1.artwork_token().is_none());
        assert!(rows[1].1.artwork_token().is_none());
        assert_eq!(rows[2].1.artwork_token(), Some(original[0].1));
        results
            .send((
                source_key(original[2].0.clone()),
                Some(Pixels {
                    bytes: vec![255; 16],
                    width: 2,
                    height: 2,
                }),
            ))
            .unwrap();
        super::super::paint_at(&mut app, 80).unwrap();
        assert!(rows[0].1.artwork_token().is_none());
        assert!(rows[1].1.artwork_token().is_none());
        assert_eq!(rows[2].1.artwork_token(), Some(original[0].1));
    }

    #[test]
    fn selected_artwork_rejects_late_results_and_covers_its_container() {
        for (width, height, source_width, source_height) in [
            (480, 320, 2_u16, 2_u16),
            (640, 480, 4, 3),
            (640, 480, 3, 4),
            (640, 480, 5, 2),
        ] {
            let surface =
                FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
            let mut app = App::new(surface);
            app.with_default_widgets().with_default_systems();
            super::super::install(&mut app).unwrap();
            let mut view = View {
                page: Page::Games,
                artwork: "first".into(),
                ..View::default()
            };
            let mut scene = super::super::draw(&mut app, &view).unwrap();
            super::super::paint_at(&mut app, 10).unwrap();
            let (send, receive) = mpsc::sync_channel(2);
            app.world.resource_mut::<Loader>().unwrap().receive = receive;
            let old_key = app.world.resource::<Loader>().unwrap().key.clone();
            view.artwork = "second".into();
            super::super::update(&mut scene, &view).unwrap();
            super::super::paint_at(&mut app, 20).unwrap();
            send.send((
                old_key,
                Some(Pixels {
                    bytes: vec![255; 16],
                    width: 2,
                    height: 2,
                }),
            ))
            .unwrap();
            super::super::paint_at(&mut app, 30).unwrap();
            let loader = app.world.resource::<Loader>().unwrap();
            assert!(loader.model.token().is_none());
            let current_key = loader.key.clone();
            send.send((
                current_key,
                Some(Pixels {
                    bytes: vec![255; usize::from(source_width) * usize::from(source_height) * 4],
                    width: source_width,
                    height: source_height,
                }),
            ))
            .unwrap();
            for time in [40, 50, 60] {
                super::super::paint_at(&mut app, time).unwrap();
            }
            let key = app
                .world
                .resource::<Loader>()
                .unwrap()
                .model
                .token()
                .unwrap();
            let entity = app.world.query::<Image>().iter().find_map(|(entity, image)| {
                (app.world.get::<ComputedRect>(entity).is_some() && matches!(&image.src, mirui::ui::widgets::image::ImageSource::Texture(token) if token == &texture_token(key))).then_some(entity)
            }).unwrap_or_else(|| panic!("Missing {key}; images {:?}", app.world.query::<Image>().iter().map(|(_, image)| format!("{:?}", image.src)).collect::<Vec<_>>()));
            let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
            assert!(rect.w > Fixed::ZERO && rect.h > Fixed::ZERO);
            assert!(
                (rect.w * i32::from(source_height) - rect.h * i32::from(source_width)).abs()
                    < Fixed::from_int(5)
            );
            let parent = app.world.get::<mirui::ui::Parent>(entity).unwrap().0;
            let viewport = app.world.get::<ComputedRect>(parent).unwrap().0;
            assert!(
                app.world
                    .get::<mirui::ui::Style>(parent)
                    .unwrap()
                    .clip_children
            );
            assert!(rect.x <= viewport.x && rect.y <= viewport.y);
            assert!(rect.x + rect.w >= viewport.x + viewport.w - Fixed::ONE);
            assert!(rect.y + rect.h >= viewport.y + viewport.h - Fixed::ONE);
            assert!((rect.x * 2 + rect.w - viewport.x * 2 - viewport.w).abs() < Fixed::from_int(2));
            assert!((rect.y * 2 + rect.h - viewport.y * 2 - viewport.h).abs() < Fixed::from_int(2));

            let texture = app
                .world
                .resource::<ResourceManager<Texture<'static>>>()
                .unwrap()
                .resolve(&texture_token(key));
            view.artwork = "first".into();
            super::super::update(&mut scene, &view).unwrap();
            super::super::paint_at(&mut app, 70).unwrap();
            assert!(
                app.world
                    .resource::<Loader>()
                    .unwrap()
                    .model
                    .token()
                    .is_some()
            );
            assert!(!app.world.resource::<Loader>().unwrap().pending);
            view.artwork = "second".into();
            view.artwork_neighbors = vec!["third".into()];
            super::super::update(&mut scene, &view).unwrap();
            super::super::paint_at(&mut app, 80).unwrap();
            assert_eq!(
                app.world.resource::<Loader>().unwrap().model.token(),
                Some(key)
            );
            let reused = app
                .world
                .resource::<ResourceManager<Texture<'static>>>()
                .unwrap()
                .resolve(&texture_token(key));
            assert!(std::rc::Rc::ptr_eq(&texture, &reused));

            let (jobs, queued) = mpsc::sync_channel(1);
            {
                let loader = app.world.resource_mut::<Loader>().unwrap();
                loader.send = jobs;
                loader.changed = Instant::now() - DEBOUNCE;
            }
            super::super::paint_at(&mut app, 90).unwrap();
            let neighbor = queued.try_recv().unwrap();
            assert_eq!(neighbor.0, "third");
            send.send((
                neighbor,
                Some(Pixels {
                    bytes: vec![255; 16],
                    width: 2,
                    height: 2,
                }),
            ))
            .unwrap();
            super::super::paint_at(&mut app, 100).unwrap();
            assert_eq!(
                app.world.resource::<Loader>().unwrap().model.token(),
                Some(key)
            );
            view.artwork = "third".into();
            view.artwork_neighbors.clear();
            super::super::update(&mut scene, &view).unwrap();
            super::super::paint_at(&mut app, 110).unwrap();
            assert!(
                app.world
                    .resource::<Loader>()
                    .unwrap()
                    .model
                    .token()
                    .is_some()
            );
            assert!(!app.world.resource::<Loader>().unwrap().pending);
            assert!(queued.try_recv().is_err());
        }
    }

    #[test]
    fn resource_cache_is_bounded_and_source_changes_retire_results() {
        let surface = FramebufSurface::with_format(480, 320, ColorFormat::RGBA8888, |_, _| {});
        let mut app = App::new(surface);
        app.with_default_widgets().with_default_systems();
        super::super::install(&mut app).unwrap();
        let textures = ResourceManager::new(
            MaxSize::Bytes(32),
            Texture::owned(1, 1, ColorFormat::RGBA8888).unwrap(),
        );
        let loader = app.world.resource_mut::<Loader>().unwrap();
        let pixels = || {
            Some(Pixels {
                bytes: vec![255; 16],
                width: 2,
                height: 2,
            })
        };
        for i in 0..CACHE_ENTRIES * 2 {
            loader.accept(source_key(format!("missing-{i}")), pixels(), &textures);
            let texture = textures.resolve(&texture_token(loader.serial));
            assert!(!texture.transient, "Immutable artwork retains GPU uploads");
            assert_eq!(texture.cache_revision, loader.serial);
        }
        assert_eq!(loader.cached.len(), CACHE_ENTRIES);
        assert!(
            loader
                .cached(&source_key("missing-0".into()), &textures)
                .is_none()
        );
        assert!(
            loader
                .cached(&source_key(format!("missing-{CACHE_ENTRIES}")), &textures)
                .is_none()
        );
        assert!(
            loader
                .cached(
                    &source_key(format!("missing-{}", CACHE_ENTRIES * 2 - 1)),
                    &textures
                )
                .unwrap()
                .is_some()
        );

        let path = std::env::temp_dir().join(format!("nearby-artwork-{}", std::process::id()));
        std::fs::write(&path, b"before").unwrap();
        let key = source_key(path.to_string_lossy().into_owned());
        loader.key = key.clone();
        std::fs::write(&path, b"changed bytes").unwrap();
        loader.accept(key.clone(), pixels(), &textures);
        assert!(loader.model.token().is_none());
        assert!(loader.pending);
        assert_ne!(loader.key, key);
        std::fs::remove_file(path).unwrap();
    }
}
