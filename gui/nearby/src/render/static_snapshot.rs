use super::{rows::RowHandle, state::DisplayHandle};
use mirui::app::RendererFactory;
use mirui::core::resource::{ResourceHandle, ResourceManager};
use mirui::prelude::*;
use mirui::render::texture::Texture;
use mirui::surface::Surface;
use std::{cell::RefCell, rc::Rc};

#[model]
pub(super) struct Picture {
    #[observe]
    pub hidden: bool,
    #[observe]
    pub token: Option<u64>,
    #[observe]
    pub bounds: Rect,
    size: (Fixed, Fixed),
}

impl Default for Picture {
    fn default() -> Self {
        Self {
            hidden: false,
            token: None,
            bounds: Rect::new(0, 0, 0, 0),
            size: (Fixed::ZERO, Fixed::ZERO),
        }
    }
}

#[model]
impl Picture {
    pub fn resize(&mut self, size: LayoutSize) {
        self.size = (size.width, size.height);
    }

    fn dimensions(&self) -> (Fixed, Fixed) {
        self.size
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    pub(super) fn publish(&mut self, hidden: bool, token: Option<u64>) {
        self.hidden = hidden;
        self.token = token;
    }
}

#[component]
pub(super) struct Source {
    pub member: mirui::core::model::ModelItemId,
    pub row: model!(super::rows::Row),
    pub state: model!(super::state::DisplayState),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    theme: mirui::ui::theme::ThemeId,
    physical: (u16, u16),
    logical: (u16, u16),
    size: (Fixed, Fixed),
    platform: super::consoles::Platform,
    artwork: Option<u64>,
}

struct Image {
    revision: u64,
    stamp: Stamp,
    bounds: Rect,
    _handle: ResourceHandle<Texture<'static>>,
}

struct Entry {
    entity: Entity,
    member: mirui::core::model::ModelItemId,
    state: DisplayHandle,
    picture: PictureHandle,
    attempted: Option<Stamp>,
    pending: Option<Image>,
    held: Option<Image>,
}

struct Capture {
    entity: Entity,
    row: RowHandle,
    state: DisplayHandle,
    stamp: Stamp,
    owned_request: bool,
}

fn source_live(world: &mirui::ecs::World, entity: Entity) -> bool {
    world
        .get::<Source>(entity)
        .is_some_and(|source| source.state.entries().get_by_id(source.member).is_some())
}

fn pixel_aligned(layout: Rect, local: Rect, scale: Fixed) -> bool {
    let left = layout.x + local.x;
    let top = layout.y + local.y;
    let scale = mirui::types::Fixed64::from_fixed(scale);
    [left, top, left + local.w, top + local.h]
        .into_iter()
        .all(|edge| {
            let edge = mirui::types::Fixed64::from_fixed(edge).mul_wide(scale);
            edge == mirui::types::Fixed64::from_int(edge.to_int())
        })
}

#[derive(Default)]
struct Controller {
    entries: Vec<Entry>,
    retired: Vec<Image>,
    capture: Option<Capture>,
    revision: u64,
    #[cfg(test)]
    captures: u64,
}

struct Runtime(Rc<RefCell<Controller>>);

pub(super) fn install<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    app.add_resource(Runtime(Rc::new(RefCell::new(Controller::default()))));
}

pub(super) fn texture_token(revision: u64) -> String {
    format!("nearby-static-picture-{revision}")
}

fn publish(picture: &PictureHandle, hidden: bool, token: Option<u64>) {
    if picture.hidden() != hidden || picture.token() != token {
        picture.publish(hidden, token);
    }
}

fn stamp<B: Surface, F: RendererFactory<B>>(app: &App<B, F>, row: &RowHandle) -> Option<Stamp> {
    let size = row.static_picture().dimensions();
    if size.0 <= Fixed::ZERO || size.1 <= Fixed::ZERO {
        return None;
    }
    Some(Stamp {
        theme: app.theme().id(),
        physical: app.backend.viewport().physical_size(),
        logical: app.backend.viewport().logical_size(),
        size,
        platform: row.platform(),
        artwork: row.artwork_token(),
    })
}

fn allowed<B: Surface, F: RendererFactory<B>>(
    app: &App<B, F>,
    entity: Entity,
    state: &DisplayHandle,
) -> bool {
    if !state.card_motion().cards_settled()
        || app
            .world
            .resource::<super::state::DisplayRuntime>()
            .is_some_and(|display| !display.deck.viewport_settled())
        || app
            .world
            .resource::<super::modal::Runtime>()
            .is_some_and(|modal| modal.model.paused() || modal.model.hidden())
        || app
            .world
            .resource::<super::suspension::Runtime>()
            .is_some_and(|shelf| shelf.model.hidden())
    {
        return false;
    }
    // The source's own Hidden flag is the representation being replaced.
    // Every ancestor must remain visible before requesting fresh pixels.
    let mut current = app.world.get::<mirui::ui::Parent>(entity).map(|p| p.0);
    while let Some(parent) = current {
        if app.world.has::<mirui::ui::Hidden>(parent) {
            return false;
        }
        current = app.world.get::<mirui::ui::Parent>(parent).map(|p| p.0);
    }
    true
}

impl Controller {
    fn before<B: Surface, F: RendererFactory<B>>(&mut self, app: &mut App<B, F>) {
        for index in (0..self.entries.len()).rev() {
            if !source_live(&app.world, self.entries[index].entity) {
                let entry = self.entries.remove(index);
                if entry.state.entries().get_by_id(entry.member).is_some() {
                    publish(&entry.picture, false, None);
                }
                self.retired.extend(entry.held);
                self.retired.extend(entry.pending);
            }
        }
        let sources: Vec<_> = app
            .world
            .query::<Source>()
            .iter()
            .filter(|(entity, _)| source_live(&app.world, *entity))
            .map(|(entity, source)| {
                (
                    entity,
                    source.member,
                    source.row.clone(),
                    source.state.clone(),
                )
            })
            .collect();
        for (entity, member, row, state) in sources {
            let existing = self.entries.iter().position(|entry| entry.entity == entity);
            let index = match existing {
                Some(index) => index,
                None if self.entries.len() < super::shelf::WINDOW_BUDGET
                    && allowed(app, entity, &state) =>
                {
                    self.entries.push(Entry {
                        entity,
                        member,
                        state: state.clone(),
                        picture: row.static_picture(),
                        attempted: None,
                        pending: None,
                        held: None,
                    });
                    self.entries.len() - 1
                }
                None => continue,
            };
            let entry = &mut self.entries[index];
            let current = stamp(app, &row);
            if let Some(pending) = entry.pending.take() {
                if Some(pending.stamp) == current {
                    entry.held = Some(pending);
                    entry
                        .picture
                        .set_bounds(entry.held.as_ref().unwrap().bounds);
                    publish(
                        &entry.picture,
                        true,
                        entry.held.as_ref().map(|image| image.revision),
                    );
                } else {
                    self.retired.push(pending);
                    entry.attempted = None;
                }
            }
            if entry
                .held
                .as_ref()
                .is_some_and(|image| Some(image.stamp) == current)
            {
                continue;
            }
            if let Some(previous) = entry.held.take() {
                self.retired.push(previous);
                publish(&entry.picture, false, None);
            }
            let Some(current) = current else { continue };
            if self.capture.is_none()
                && entry.attempted != Some(current)
                && allowed(app, entity, &state)
            {
                let Some(rect) = app
                    .world
                    .get::<mirui::ui::ComputedRect>(entity)
                    .map(|rect| rect.0)
                else {
                    continue;
                };
                entry.attempted = Some(current);
                if !pixel_aligned(
                    rect,
                    Rect::new(0, 0, rect.w, rect.h),
                    app.backend.viewport().scale(),
                ) {
                    continue;
                }
                let owned_request = !app.world.has::<mirui::ui::OffscreenRender>(entity);
                if owned_request {
                    app.world
                        .insert(entity, mirui::ui::OffscreenRender::default());
                }
                app.world.mark_subtree_dirty(entity);
                self.capture = Some(Capture {
                    entity,
                    row,
                    state,
                    stamp: current,
                    owned_request,
                });
            }
        }
    }

    fn after<B: Surface, F: RendererFactory<B>>(&mut self, app: &mut App<B, F>) {
        if let Some(textures) = app.world.resource::<ResourceManager<Texture<'static>>>() {
            for image in self.retired.drain(..) {
                textures.remove_token(&texture_token(image.revision));
            }
        }
        let Some(capture) = self.capture.take() else {
            return;
        };
        let valid = source_live(&app.world, capture.entity)
            && stamp(app, &capture.row) == Some(capture.stamp)
            && allowed(app, capture.entity, &capture.state);
        let bounds = app
            .world
            .resource::<mirui::ui::Outputs<mirui::ui::WidgetTexture>>()
            .and_then(|outputs| {
                outputs
                    .get(capture.entity)
                    .map(|output| output.local_bounds())
            });
        // The standard Image translate path requires integral physical edges
        // to preserve the prepared output's pixel placement.
        let aligned = bounds.is_some_and(|bounds| {
            app.world
                .get::<mirui::ui::ComputedRect>(capture.entity)
                .is_some_and(|rect| pixel_aligned(rect.0, bounds, app.backend.viewport().scale()))
        });
        // Reading a prepared widget output does not add another present.
        let texture = if valid && aligned {
            app.take_widget_output(capture.entity)
                .ok()
                .flatten()
                .map(|owned| owned.into_texture())
        } else {
            None
        };
        if capture.owned_request {
            app.world
                .remove::<mirui::ui::OffscreenRender>(capture.entity);
            if app.world.is_alive(capture.entity) {
                app.world.mark_subtree_dirty(capture.entity);
            }
        }
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.entity == capture.entity)
        else {
            return;
        };
        if !valid {
            entry.attempted = None;
        }
        let Some(texture) = texture else { return };
        if texture.width() == 0 || texture.height() == 0 {
            return;
        }
        let Some(textures) = app.world.resource::<ResourceManager<Texture<'static>>>() else {
            return;
        };
        self.revision += 1;
        let revision = self.revision | (1_u64 << 62);
        let texture = texture.with_cache_revision(revision);
        let dimensions = (texture.width(), texture.height());
        let token = texture_token(revision);
        textures.add_factory(token.clone(), move || Some(texture));
        let handle = textures.load(token.clone());
        if (handle.get().width(), handle.get().height()) != dimensions {
            textures.remove_token(&token);
            return;
        }
        #[cfg(test)]
        {
            self.captures += 1;
        }
        entry.pending = Some(Image {
            revision,
            stamp: capture.stamp,
            bounds: bounds.unwrap(),
            _handle: handle,
        });
    }
}

pub(super) fn before_tick<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    if let Some(runtime) = app
        .world
        .resource::<Runtime>()
        .map(|runtime| runtime.0.clone())
    {
        runtime.borrow_mut().before(app);
    }
}

pub(super) fn after_tick<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    if let Some(runtime) = app
        .world
        .resource::<Runtime>()
        .map(|runtime| runtime.0.clone())
    {
        runtime.borrow_mut().after(app);
    }
}

#[cfg(test)]
pub(super) fn counts(world: &mirui::ecs::World) -> (usize, u64) {
    let runtime = world.resource::<Runtime>().unwrap();
    let controller = runtime.0.borrow();
    (controller.entries.len(), controller.captures)
}

#[cfg(test)]
mod precision_tests {
    use super::*;

    #[test]
    fn image_admission_requires_exact_physical_edges() {
        assert!(pixel_aligned(
            Rect::new(40, 100, 160, 120),
            Rect::new(-1, -1, 162, 122),
            Fixed::ONE
        ));
        let centered = Rect::new(182, Fixed::from_ratio(287, 2), 276, 205);
        let bounds = Rect::new(-1, -1, 278, 207);
        assert!(!pixel_aligned(centered, bounds, Fixed::ONE));
        assert!(pixel_aligned(centered, bounds, Fixed::from_int(2)));
        assert!(!pixel_aligned(
            Rect::new(Fixed::from_ratio(1, 256), 0, 10, 10),
            Rect::new(0, 0, 10, 10),
            Fixed::HALF
        ));
        assert!(!pixel_aligned(
            Rect::new(0, 0, 10, 10),
            Rect::new(0, 0, Fixed::from_ratio(21, 2), 10),
            Fixed::ONE
        ));
        assert!(pixel_aligned(
            Rect::new(Fixed::HALF, Fixed::HALF, 10, 10),
            Rect::new(Fixed::HALF, Fixed::HALF, 10, 10),
            Fixed::ONE
        ));
        assert!(!pixel_aligned(
            Rect::new(-Fixed::HALF, 0, 10, 10),
            Rect::new(0, 0, 10, 10),
            Fixed::ONE
        ));
    }

    #[test]
    fn fractional_cartridge_remains_live_without_repeated_attempts() {
        let mut app = App::headless(640, 480);
        app.with_default_widgets().with_default_systems();
        crate::render::install(&mut app).unwrap();
        let mut view = crate::model::View {
            page: crate::model::Page::Games,
            entries: vec![crate::model::GameRow {
                key: "center".into(),
                name: "Centered".into(),
                icon: "nes".into(),
                ..crate::model::GameRow::default()
            }],
            ..crate::model::View::default()
        };
        view.library.kind = crate::model::LibraryKind::Favorites;
        view.launcher.enabled = true;
        view.launcher_shelf = Some(Box::new(view.clone()));
        let _scene = crate::render::draw(&mut app, &view).unwrap();
        for frame in 0..100 {
            crate::render::paint_at(&mut app, frame * 17).unwrap();
        }
        let runtime = app.world.resource::<Runtime>().unwrap().0.clone();
        let attempted = {
            let controller = runtime.borrow();
            assert_eq!(controller.entries.len(), 1);
            assert_eq!(controller.captures, 0);
            let entry = &controller.entries[0];
            assert!(entry.attempted.is_some());
            assert!(entry.held.is_none() && entry.pending.is_none());
            assert!(!entry.picture.hidden() && entry.picture.token().is_none());
            entry.attempted
        };
        let source = {
            let mut controller = runtime.borrow_mut();
            controller.entries[0].attempted = None;
            controller.entries[0].entity
        };
        before_tick(&mut app);
        assert!(runtime.borrow().capture.is_none());
        assert!(!app.world.has::<mirui::ui::OffscreenRender>(source));
        for frame in 100..140 {
            crate::render::paint_at(&mut app, frame * 17).unwrap();
        }
        let controller = runtime.borrow();
        assert!(controller.entries[0].attempted == attempted);
        assert_eq!(controller.captures, 0);
        assert!(!controller.entries[0].picture.hidden());
    }
}
