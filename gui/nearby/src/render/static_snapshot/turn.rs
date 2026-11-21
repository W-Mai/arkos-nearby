use mirui::app::RendererFactory;
use mirui::core::model::{ModelHandle, ModelItemId, ModelList, OwnedModel};
use mirui::prelude::*;
use mirui::surface::Surface;
use mirui::ui::{ComputedRect, OffscreenRender, Outputs, WidgetTexture};
use std::{cell::RefCell, rc::Rc};

use super::{Tile, pixel_aligned};
use crate::render::{cartridge, deck::Layer, shelf, state::DisplayState};

#[component]
pub(in crate::render) struct PageSource {
    pub state: model!(DisplayState),
    pub departing: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(in crate::render) struct Stamp {
    plan: shelf::Plan,
    position: usize,
    rows: Vec<(
        ModelItemId,
        crate::render::rows::CartridgeStamp,
        cartridge::Pose,
    )>,
}

impl Stamp {
    pub fn new(
        plan: shelf::Plan,
        position: usize,
        poses: &[cartridge::Pose],
        rows: &ModelList<crate::render::rows::Row>,
        visible: usize,
    ) -> Self {
        Self {
            plan,
            position,
            rows: rows
                .entries()
                .zip(poses)
                .take(visible)
                .map(|((member, row), pose)| (member, row.read(|row| row.cartridge_stamp()), *pose))
                .collect(),
        }
    }
}

#[derive(Default)]
struct Capture {
    attempted: Option<(Entity, mirui::ui::theme::ThemeId, Stamp)>,
    requested: Option<(Entity, PageSource, Stamp)>,
    restore_direct_paint: Option<Entity>,
    revision: u64,
}

struct Runtime(Rc<RefCell<Capture>>);

pub(super) fn install<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    app.add_resource(Runtime(Rc::new(RefCell::new(Capture::default()))));
}

fn allowed<B: Surface, F: RendererFactory<B>>(app: &App<B, F>, entity: Entity) -> bool {
    app.world
        .resource::<crate::render::state::DisplayRuntime>()
        .is_some_and(|display| {
            display.deck.enabled()
                && display.deck.layer() == Layer::Shelf
                && display.deck.viewport_settled()
                && display.deck.expansion() == Fixed::ZERO
                && display.shelf.card_motion().cards_settled()
                && display.shelf.shelf().can_prepare_turn()
                && !mirui::ui::branch::is_effectively_hidden(&app.world, entity)
        })
}

fn state_stamp(source: &PageSource) -> Stamp {
    let shelf = source.state.shelf();
    let visible = source.state.populated_rows();
    source
        .state
        .entries()
        .read(|rows| shelf.read(|shelf| shelf.turn_stamp(rows, visible)))
}

pub(super) fn before_tick<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    if !cfg!(any(feature = "gpu", test)) {
        return;
    }
    let runtime = app.world.resource::<Runtime>().unwrap().0.clone();
    let mut capture = runtime.borrow_mut();
    if let Some(entity) = capture.restore_direct_paint.take()
        && app.world.is_alive(entity)
    {
        app.world.remove::<OffscreenRender>(entity);
        app.world.mark_subtree_dirty(entity);
    }
    let source = app
        .world
        .query::<PageSource>()
        .iter()
        .find_map(|(entity, source)| {
            (!source.departing && allowed(app, entity)).then(|| {
                (
                    entity,
                    PageSource {
                        state: source.state.clone(),
                        departing: false,
                    },
                )
            })
        });
    let Some((entity, source)) = source else {
        return;
    };
    let Some(rect) = app.world.get::<ComputedRect>(entity).map(|rect| rect.0) else {
        return;
    };
    if app.world.has::<OffscreenRender>(entity)
        || !pixel_aligned(
            rect,
            Rect::new(0, 0, rect.w, rect.h),
            app.backend.viewport().scale(),
        )
    {
        return;
    }
    if source.state.shelf().has_prepared_turn() {
        return;
    }
    let stamp = state_stamp(&source);
    let key = (entity, app.theme().id(), stamp.clone());
    if capture.attempted.as_ref() == Some(&key) {
        return;
    }
    capture.attempted = Some(key);
    app.world.insert(entity, OffscreenRender::default());
    app.world.mark_subtree_dirty(entity);
    capture.requested = Some((entity, source, stamp));
}

pub(super) fn after_tick<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    if !cfg!(any(feature = "gpu", test)) {
        return;
    }
    let runtime = app.world.resource::<Runtime>().unwrap().0.clone();
    let mut capture = runtime.borrow_mut();
    let Some((entity, source, stamp)) = capture.requested.take() else {
        return;
    };
    let bounds = app
        .world
        .resource::<Outputs<WidgetTexture>>()
        .and_then(|outputs| outputs.get(entity).map(|output| output.local_bounds()));
    let texture = if allowed(app, entity) && stamp == state_stamp(&source) {
        bounds.and_then(|bounds| {
            let rect = app.world.get::<ComputedRect>(entity)?.0;
            if !pixel_aligned(rect, bounds, app.backend.viewport().scale()) {
                return None;
            }
            crate::render::snapshot::take(app, entity).map(|texture| (bounds, texture))
        })
    } else {
        capture.attempted = None;
        None
    };
    // Keep the submitted layout outputs available until the next tick.
    capture.restore_direct_paint = Some(entity);
    let Some((bounds, texture)) = texture else {
        return;
    };
    capture.revision += 1;
    let revision = capture.revision | (1_u64 << 61);
    source.state.shelf().prepare_turn(
        stamp,
        Tile {
            revision,
            bounds,
            texture: mirui::core::cache::Handle::new(texture.with_cache_revision(revision)),
        },
    );
}
