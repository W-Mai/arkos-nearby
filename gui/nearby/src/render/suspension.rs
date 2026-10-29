use super::{deck, state::DisplayRuntime};
use mirui::app::RendererFactory;
use mirui::app::plugin::Plugin;
use mirui::core::model::ModelItemId;
use mirui::core::resource::{ResourceHandle, ResourceManager};
use mirui::ecs::World;
use mirui::prelude::*;
use mirui::render::texture::Texture;
use mirui::surface::Surface;
use std::cell::RefCell;
use std::rc::Rc;

#[model]
#[derive(Default)]
pub(super) struct ShelfSnapshot {
    #[observe]
    pub hidden: bool,
    #[observe]
    pub paused: bool,
    #[observe]
    pub token: Option<u64>,
}

#[model]
impl ShelfSnapshot {
    pub fn publish(&mut self, hidden: bool, paused: bool, token: Option<u64>) {
        self.hidden = hidden;
        self.paused = paused;
        self.token = token;
    }
}

#[component]
pub(super) struct ShelfSource;

pub(super) struct Runtime {
    pub model: ShelfSnapshotHandle,
    controller: Rc<RefCell<Controller>>,
}

pub(super) fn texture_token(revision: u64) -> String {
    format!("nearby-shelf-snapshot-{revision}")
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RowStamp {
    member: ModelItemId,
    text: u64,
    artwork: Option<u64>,
    index: usize,
    available: bool,
    directory: bool,
    icon: u8,
    platform: super::consoles::Platform,
    favorite: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    theme: mirui::ui::theme::ThemeId,
    physical: (u16, u16),
    logical: (u16, u16),
    height: Fixed,
    plan: super::shelf::Plan,
    pages: (usize, usize, usize),
    cards: super::motion::Frame,
    selected: Option<usize>,
    rows: [Option<RowStamp>; super::shelf::MAX_VISIBLE],
}

struct Image {
    revision: u64,
    stamp: Stamp,
    _handle: ResourceHandle<Texture<'static>>,
}

struct Capture {
    source: Entity,
    stamp: Stamp,
    owned_request: bool,
}

struct Controller {
    model: ShelfSnapshotHandle,
    enabled: bool,
    capturing: Option<Capture>,
    attempted: Option<Stamp>,
    pending: Option<Image>,
    held: Option<Image>,
    retired: Vec<Image>,
    resuming: bool,
    revision: u64,
}

pub(super) fn install<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    let model = app.add_model(ShelfSnapshot::default()).finish();
    let controller = Rc::new(RefCell::new(Controller {
        model: model.clone(),
        enabled: std::env::var("NEARBY_SHELF_SNAPSHOT").as_deref() != Ok("live"),
        capturing: None,
        attempted: None,
        pending: None,
        held: None,
        retired: Vec::with_capacity(2),
        resuming: false,
        revision: 0,
    }));
    app.add_resource(Runtime {
        model,
        controller: controller.clone(),
    });
    app.add_plugin(RestorePaint { controller });
}

/// Repaints the restored representation when its viewport motion finishes.
///
/// **Inserts**
/// - hooks: `pre_render`
struct RestorePaint {
    controller: Rc<RefCell<Controller>>,
}

impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for RestorePaint {
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn pre_render(&mut self, world: &mut World) {
        if !self.controller.borrow().resuming {
            return;
        }
        let settled = world.resource::<DisplayRuntime>().is_some_and(|display| {
            display.deck.layer() == deck::Layer::Shelf
                && display.deck.expansion() == Fixed::ZERO
                && display.deck.viewport_settled()
        });
        if settled {
            let rect = world
                .find_by_id("shelf_snapshot_surface")
                .and_then(|entity| world.get::<mirui::ui::ComputedRect>(entity))
                .map(|rect| rect.0);
            if let Some(rect) = rect {
                world.invalidate_rect(rect);
                self.controller.borrow_mut().resuming = false;
            }
        }
    }
}

fn stamp<B: Surface, F: RendererFactory<B>>(app: &App<B, F>) -> Option<Stamp> {
    let display = app.world.resource::<DisplayRuntime>()?;
    let shelf = &display.shelf;
    let viewport = app.backend.viewport();
    let mut rows = [None; super::shelf::MAX_VISIBLE];
    for (slot, (member, row)) in rows.iter_mut().zip(shelf.entries().entries()) {
        *slot = Some(RowStamp {
            member,
            text: row.text_revision(),
            artwork: row.artwork_token(),
            index: row.index(),
            available: row.available(),
            directory: row.directory(),
            icon: row.icon(),
            platform: row.platform(),
            favorite: row.favorite(),
        });
    }
    Some(Stamp {
        theme: app.theme().id(),
        physical: viewport.physical_size(),
        logical: viewport.logical_size(),
        height: display.deck.height(),
        plan: shelf.shelf().plan(),
        pages: shelf.shelf().pages(),
        cards: shelf.card_motion(),
        selected: shelf.selected(),
        rows,
    })
}

fn wanted<B: Surface, F: RendererFactory<B>>(app: &App<B, F>) -> bool {
    app.world
        .resource::<DisplayRuntime>()
        .is_some_and(|display| {
            display.deck.enabled()
                && display.deck.layer() != deck::Layer::Shelf
                && display.deck.expansion() == Fixed::ONE
                && display.deck.viewport_settled()
                && display.shelf.card_motion().cards_settled()
        })
}

fn capture_allowed<B: Surface, F: RendererFactory<B>>(app: &App<B, F>) -> bool {
    app.world
        .resource::<super::modal::Runtime>()
        .is_none_or(|modal| !modal.model.hidden() && !modal.model.paused())
}

impl Controller {
    fn publish(&self, hidden: bool, token: Option<u64>) {
        if (self.model.hidden(), self.model.paused(), self.model.token()) != (hidden, hidden, token)
        {
            self.model.publish(hidden, hidden, token);
        }
    }

    fn restore(&mut self) {
        self.publish(false, None);
        if let Some(image) = self.held.take() {
            self.retired.push(image);
        }
        if let Some(image) = self.pending.take() {
            self.retired.push(image);
        }
    }

    fn before<B: Surface, F: RendererFactory<B>>(&mut self, app: &mut App<B, F>) {
        if !self.enabled {
            return;
        }
        let Some(source) = app.world.find_by_id("shelf_snapshot_source") else {
            return;
        };
        let current = stamp(app);
        if !wanted(app) || current.is_none() {
            self.resuming |= self.model.hidden();
            self.restore();
            if let Some(capture) = self.capturing.take() {
                capture.release(app);
            }
            self.attempted = None;
            return;
        }
        self.resuming = false;
        let current = current.unwrap();
        if let Some(image) = self.pending.take() {
            if image.stamp == current {
                self.held = Some(image);
                self.publish(true, self.held.as_ref().map(|image| image.revision));
            } else {
                self.retired.push(image);
                self.attempted = None;
            }
        }
        if self
            .held
            .as_ref()
            .is_some_and(|image| image.stamp == current)
        {
            return;
        }
        self.restore();
        if capture_allowed(app) && self.attempted != Some(current) {
            self.attempted = Some(current);
            let owned_request = !app.world.has::<mirui::ui::OffscreenRender>(source);
            if owned_request {
                app.world
                    .insert(source, mirui::ui::OffscreenRender::default());
            }
            self.capturing = Some(Capture {
                source,
                stamp: current,
                owned_request,
            });
            app.world.mark_subtree_dirty(source);
        }
    }

    fn after<B: Surface, F: RendererFactory<B>>(&mut self, app: &mut App<B, F>) {
        if let Some(textures) = app.world.resource::<ResourceManager<Texture<'static>>>() {
            for image in self.retired.drain(..) {
                textures.remove_token(&texture_token(image.revision));
            }
        }
        let Some(captured) = self.capturing.take() else {
            return;
        };
        if !wanted(app) || stamp(app) != Some(captured.stamp) || !capture_allowed(app) {
            captured.release(app);
            self.attempted = None;
            return;
        }
        // Read only the completed output; a fallback render would reenter
        // RestorePaint while this controller is mutably borrowed.
        let prepared = app
            .world
            .resource::<mirui::ui::Outputs<mirui::ui::WidgetTexture>>()
            .is_some_and(|outputs| outputs.get(captured.source).is_some());
        if !prepared {
            captured.release(app);
            return;
        }
        let snapshot = app.snapshot_widget(captured.source);
        captured.release(app);
        let Ok(Some(texture)) = snapshot else {
            return;
        };
        if texture.width() == 0 || texture.height() == 0 {
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        let revision = self.revision | (1 << 63);
        let dimensions = (texture.width(), texture.height());
        let texture = texture.with_cache_revision(revision);
        let Some(textures) = app.world.resource::<ResourceManager<Texture<'static>>>() else {
            return;
        };
        let token = texture_token(revision);
        textures.add_factory(token.clone(), move || Some(texture));
        let handle = textures.load(token.clone());
        let admitted = handle.get();
        if (admitted.width(), admitted.height()) != dimensions {
            textures.remove_token(&token);
            return;
        }
        self.pending = Some(Image {
            revision,
            stamp: captured.stamp,
            _handle: handle,
        });
    }
}

impl Capture {
    fn release<B: Surface, F: RendererFactory<B>>(&self, app: &mut App<B, F>) {
        if self.owned_request {
            app.world.remove::<mirui::ui::OffscreenRender>(self.source);
        }
    }
}

pub(super) fn before_tick<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    if let Some(controller) = app
        .world
        .resource::<Runtime>()
        .map(|runtime| runtime.controller.clone())
    {
        controller.borrow_mut().before(app);
    }
}

pub(super) fn after_tick<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    if let Some(controller) = app
        .world
        .resource::<Runtime>()
        .map(|runtime| runtime.controller.clone())
    {
        controller.borrow_mut().after(app);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GameRow, LibraryKind, Page, View};
    use mirui::render::texture::ColorFormat;
    use mirui::surface::framebuf::FramebufSurface;
    use std::cell::Cell;

    struct RenderCount(Rc<Cell<usize>>);

    impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for RenderCount {
        fn build(&mut self, _app: &mut App<B, F>) {}

        fn pre_render(&mut self, _world: &mut World) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn unavailable_prepared_outputs_keep_the_shelf_live_without_render_reentry() {
        for missing in [true, false] {
            let surface = FramebufSurface::with_format(640, 480, ColorFormat::RGBA8888, |_, _| {});
            let mut app = App::new(surface);
            app.with_default_widgets().with_default_systems();
            super::super::install(&mut app).unwrap();
            let renders = Rc::new(Cell::new(0));
            app.add_plugin(RenderCount(renders.clone()));
            let mut home = View {
                page: Page::Games,
                entries: (0..4)
                    .map(|index| GameRow {
                        key: format!("game-{index}"),
                        name: format!("Game {index}"),
                        icon: "nes".into(),
                        available: true,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            };
            home.library.kind = LibraryKind::Favorites;
            home.launcher.enabled = true;
            home.launcher_shelf = Some(Box::new(home.clone()));
            let mut scene = super::super::draw(&mut app, &home).unwrap();
            super::super::paint_at(&mut app, 1000).unwrap();
            let mut library = home;
            library.launcher.tab = 1;
            library.library.kind = LibraryKind::Platforms;
            super::super::update(&mut scene, &library).unwrap();
            for now in [1016, 2000] {
                super::super::paint_at(&mut app, now).unwrap();
            }
            assert!(wanted(&app));
            before_tick(&mut app);
            let controller = app.world.resource::<Runtime>().unwrap().controller.clone();
            let source = controller.borrow().capturing.as_ref().unwrap().source;
            let owned = controller
                .borrow()
                .capturing
                .as_ref()
                .unwrap()
                .owned_request;
            assert!(app.world.has::<mirui::ui::OffscreenRender>(source));
            if missing {
                app.world
                    .remove_resource::<mirui::ui::Outputs<mirui::ui::WidgetTexture>>();
            } else {
                app.add_resource(mirui::ui::Outputs::<mirui::ui::WidgetTexture>::with_budget(
                    0,
                ));
            }
            let before = renders.get();
            after_tick(&mut app);
            assert_eq!(renders.get(), before, "Capture completion must not render");
            let runtime = app.world.resource::<Runtime>().unwrap();
            assert!(!runtime.model.hidden() && !runtime.model.paused());
            assert!(runtime.model.token().is_none());
            let state = controller.borrow();
            assert!(state.capturing.is_none());
            assert!(state.pending.is_none() && state.held.is_none());
            assert_eq!(
                app.world.has::<mirui::ui::OffscreenRender>(source),
                !owned,
                "Capture completion must release only its own output request"
            );
        }
    }
}
