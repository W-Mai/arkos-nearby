use super::{deck::Layer, motion::timing::HOP_MS, state::DisplayRuntime};
use mirui::app::RendererFactory;
use mirui::app::plugin::Plugin;
use mirui::ecs::{Entity, World};
use mirui::prelude::{App, Fixed, Rect};
use mirui::surface::Surface;
use mirui::ui::{ComputedRect, Parent};

#[mirui::component]
pub(super) struct ActionsHost {
    pub primary: bool,
}

#[mirui::component]
pub(super) struct ActionsPanel;

#[derive(PartialEq, Eq)]
struct Motion {
    panel: Entity,
    title: String,
    kind: String,
    key: String,
    selected: usize,
    age: u16,
    levels: [Fixed; 2],
    rect: Rect,
}

/// Requests one coherent action-panel paint while its tiles move.
///
/// **Inserts**
/// - hooks: `pre_render`
#[derive(Default)]
pub(super) struct ActionsDamage {
    last: Option<Motion>,
}

impl ActionsDamage {
    pub(super) fn region(&mut self, world: &World) -> Option<Rect> {
        let runtime = world.resource::<DisplayRuntime>()?;
        if !runtime.deck.enabled() || runtime.deck.layer() != Layer::Actions {
            return self.last.take().map(|last| last.rect);
        }
        let launcher = runtime.state.launcher();
        let age = launcher.selection_age();
        let levels = launcher.level_positions();
        let levels_changed = self.last.as_ref().is_some_and(|last| last.levels != levels);
        if !levels_changed
            && age >= HOP_MS
            && self.last.as_ref().is_none_or(|last| last.age >= HOP_MS)
        {
            return None;
        }
        let view = launcher.view();
        if !view.enabled || view.overlay.is_empty() {
            return self.last.take().map(|last| last.rect);
        }
        let host = world
            .query::<ActionsHost>()
            .iter()
            .find_map(|(entity, host)| host.primary.then_some(entity))?;
        let panel = world
            .query::<ActionsPanel>()
            .iter()
            .find_map(|(entity, _)| {
                super::selection::contains(host, entity, |node| {
                    world.get::<Parent>(node).map(|parent| parent.0)
                })
                .then_some(entity)
            })?;
        let rect = world.get::<ComputedRect>(panel)?.0;
        if rect.w <= Fixed::ZERO || rect.h <= Fixed::ZERO {
            return None;
        }
        let motion = Motion {
            panel,
            title: view.overlay,
            kind: view.overlay_kind,
            key: view.key,
            selected: view.selected,
            age,
            levels,
            rect,
        };
        if self.last.as_ref() == Some(&motion) {
            return None;
        }
        let damage = self
            .last
            .as_ref()
            .map_or(rect, |last| last.rect.union(&rect));
        // Keep the settled bounds for a single cleanup when this panel closes.
        self.last = Some(motion);
        Some(damage)
    }
}

impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for ActionsDamage {
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn pre_render(&mut self, world: &mut World) {
        if let Some(rect) = self.region(world) {
            world.invalidate_rect(rect);
        }
    }
}
