use mirui::prelude::*;
use mirui::{app::RendererFactory, app::plugin::Plugin, surface::Surface};

#[derive(Default)]
pub(super) struct CacheBoundaryPaint {
    previous: Vec<(Entity, bool, Rect)>,
}

impl CacheBoundaryPaint {
    fn regions(&mut self, world: &mirui::ecs::World) -> Vec<Rect> {
        self.previous.retain(|(entity, _, _)| {
            world.is_alive(*entity) && world.has::<SettledLayerCache>(*entity)
        });
        let mut damage = Vec::new();
        for (entity, _) in world.query::<SettledLayerCache>().iter() {
            let Some(rect) = world
                .get::<mirui::ui::ComputedRect>(entity)
                .map(|rect| rect.0)
            else {
                continue;
            };
            let cached = world.has::<mirui::ui::OffscreenRender>(entity);
            if let Some(previous) = self
                .previous
                .iter_mut()
                .find(|previous| previous.0 == entity)
            {
                if previous.1 && !cached {
                    // Direct text and cached composition can round channels
                    // differently. Replace the whole previous representation once.
                    damage.push(previous.2.union(&rect));
                }
                *previous = (entity, cached, rect);
            } else {
                self.previous.push((entity, cached, rect));
            }
        }
        damage
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[compose]
    fn cache_policy_scene(state: model!(super::super::state::DisplayState)) -> Entity {
        ui! {
            Column (grow: 1.0) {
                View (
                    id: "policy_shelf",
                    width: 100,
                    height: 60,
                    bg_color: super::super::style::BACKGROUND
                ) [
                    SettledLayerCache {
                        state: state.clone(),
                        backdrop: true,
                    },
                ]
                View (id: "policy_header", width: 100, height: 10) [
                    SettledLayerCache {
                        state,
                        backdrop: false,
                    },
                ]
                View (id: "policy_card", width: 100, height: 10) [
                    mirui::ui::OffscreenRender::default(),
                ]
            }
        }
    }

    #[test]
    fn live_full_shelf_preserves_component_and_compressed_cache_ownership() {
        let mut app = App::headless(640, 480);
        app.with_default_widgets().with_default_systems();
        crate::render::install(&mut app).unwrap();
        let (state, deck) = {
            let display = app
                .world
                .resource::<super::super::state::DisplayRuntime>()
                .unwrap();
            (display.shelf.clone(), display.deck.clone())
        };
        let mut view = crate::model::View {
            page: crate::model::Page::Games,
            ..crate::model::View::default()
        };
        view.library.kind = crate::model::LibraryKind::Favorites;
        view.launcher.enabled = true;
        state.present(&view);
        let root = app.spawn_root().id();
        app.compose(root, cache_policy_scene(state));
        let source = app.world.find_by_id("policy_shelf").unwrap();
        let header = app.world.find_by_id("policy_header").unwrap();
        let card = app.world.find_by_id("policy_card").unwrap();
        deck.present(&view);
        app.world
            .insert(source, mirui::ui::OffscreenRender::default());
        crate::render::paint_at(&mut app, 0).unwrap();
        crate::render::paint_at(&mut app, 1600).unwrap();
        assert!(!app.world.has::<mirui::ui::OffscreenRender>(source));
        assert!(app.world.has::<mirui::ui::OffscreenRender>(header));
        assert!(app.world.has::<mirui::ui::OffscreenRender>(card));
        view.launcher.tab = 1;
        deck.present(&view);
        crate::render::paint_at(&mut app, 2400).unwrap();
        crate::render::paint_at(&mut app, 2417).unwrap();
        assert_eq!(deck.expansion(), Fixed::ONE);
        assert!(app.world.has::<mirui::ui::OffscreenRender>(source));
        view.launcher.tab = 0;
        deck.present(&view);
        crate::render::paint_at(&mut app, 2600).unwrap();
        crate::render::paint_at(&mut app, 3200).unwrap();
        assert_eq!(deck.expansion(), Fixed::ZERO);
        assert!(!app.world.has::<mirui::ui::OffscreenRender>(source));
        assert!(app.world.has::<mirui::ui::OffscreenRender>(header));
        assert!(app.world.has::<mirui::ui::OffscreenRender>(card));
    }

    #[test]
    fn cached_to_direct_replaces_the_complete_representation_once() {
        let mut app = App::headless(64, 48);
        app.with_default_widgets().with_default_systems();
        crate::render::install(&mut app).unwrap();
        let state = app
            .world
            .resource::<super::super::state::DisplayRuntime>()
            .unwrap()
            .shelf
            .clone();
        let node = app.world.spawn_empty();
        app.world.insert(
            node,
            SettledLayerCache {
                state,
                backdrop: true,
            },
        );
        let before = Rect::new(4, 8, 40, 30);
        let after = Rect::new(8, 6, 40, 30);
        app.world.insert(node, mirui::ui::ComputedRect(before));
        app.world
            .insert(node, mirui::ui::OffscreenRender::default());
        let mut paint = CacheBoundaryPaint::default();
        assert!(paint.regions(&app.world).is_empty());
        app.world.remove::<mirui::ui::OffscreenRender>(node);
        app.world.insert(node, mirui::ui::ComputedRect(after));
        assert_eq!(paint.regions(&app.world), vec![before.union(&after)]);
        assert!(paint.regions(&app.world).is_empty());
        app.world
            .insert(node, mirui::ui::OffscreenRender::default());
        assert!(paint.regions(&app.world).is_empty());
        app.world.remove::<mirui::ui::OffscreenRender>(node);
        assert_eq!(paint.regions(&app.world), vec![after]);
        assert!(paint.regions(&app.world).is_empty());
        app.world.remove::<SettledLayerCache>(node);
        assert!(paint.regions(&app.world).is_empty());
        assert!(paint.previous.is_empty());
    }
}

impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for CacheBoundaryPaint {
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn pre_render(&mut self, world: &mut mirui::ecs::World) {
        for rect in self.regions(world) {
            world.invalidate_rect(rect);
        }
    }
}

#[component]
pub(super) struct GpuLayerCache;

#[event]
fn attach(node: entity!(), commands: commands!()) {
    if cfg!(feature = "gpu") {
        commands.insert(node, mirui::ui::OffscreenRender::default());
    }
}

#[view(component = GpuLayerCache, attach = attach::callback())]
pub(super) fn gpu_layer() {}

#[component]
pub(super) struct SettledLayerCache {
    pub state: model!(super::state::DisplayState),
    pub backdrop: bool,
}

#[system(order = Animation)]
pub(super) fn settled_layer(
    runtime: res!(super::state::DisplayRuntime),
    outputs: res!(mirui::ui::Outputs<mirui::ui::WidgetTexture>),
    nodes: query!(Entity, SettledLayerCache, optional mirui::ui::OffscreenRender),
    commands: commands!(),
) {
    let gpu = cfg!(any(feature = "gpu", test));
    let viewport_settled = runtime.deck.viewport_settled();
    for (entity, layer, output) in nodes {
        // Content and focus animate independently. A completed content output
        // can survive its parent's transform; new captures wait for it to settle.
        let reusable = output.is_some() && outputs.get(entity).is_some();
        // The retained shelf contains card motion; loading and actor timelines
        // belong to separate layers and cannot keep its pixels uncached.
        let content_settled = if layer.backdrop {
            layer.state.card_motion().cards_settled()
        } else {
            layer.state.motion().settled()
        };
        let active_full_shelf = layer.backdrop
            && runtime.deck.enabled()
            && runtime.deck.layer() == super::deck::Layer::Shelf
            && runtime.deck.expansion() == Fixed::ZERO;
        let cached = gpu
            && !active_full_shelf
            && (!layer.backdrop || viewport_settled || reusable)
            && content_settled;
        if cached && output.is_none() {
            commands.insert(entity, mirui::ui::OffscreenRender::default());
        } else if !cached && output.is_some() {
            commands.remove::<mirui::ui::OffscreenRender>(entity);
        }
    }
}
