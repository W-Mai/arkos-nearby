//! Opt-in input-to-submission diagnostics on the platform monotonic clock.

use super::{deck::Layer, state::DisplayRuntime};
#[cfg(target_os = "linux")]
use crate::model::Action;
use crate::model::{LibraryKind, Page, View};
use mirui::app::{App, RendererFactory, plugin::Plugin};
use mirui::ecs::{TimingOutcome, TimingSample, World};
use mirui::surface::Surface;
use mirui::types::Fixed;
use serde_json::{Value, json};
use std::sync::OnceLock;

const INTERACTION_FRAME_LIMIT: u32 = 20_000;

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("HOWFF_RESPONSE_PROFILE").as_deref() == Ok("1"))
}

fn interactions() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("HOWFF_INTERACTION_PROFILE").as_deref() == Ok("1"))
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct Target {
    generation: howff_menu::input::Generation,
    page: Page,
    library: LibraryKind,
    selected: usize,
    position: usize,
    keyboard: bool,
    tab: usize,
    key: String,
    overlay: String,
}

impl Target {
    pub(super) fn from_view(view: &View) -> Self {
        let key = if view.launcher.enabled && !view.launcher.key.is_empty() {
            view.launcher.key.clone()
        } else if matches!(view.page, Page::Lobby | Page::Rooms) {
            view.rooms
                .get(view.selected)
                .map(|row| row.key.clone())
                .unwrap_or_default()
        } else {
            view.entries
                .get(view.selected)
                .map(|row| row.key.clone())
                .unwrap_or_default()
        };
        Self {
            generation: view.input_generation,
            page: view.page,
            library: view.library.kind,
            selected: view.selected,
            position: view.library.position,
            keyboard: view.library.keyboard,
            tab: view.launcher.tab,
            key,
            overlay: view.launcher.overlay_kind.clone(),
        }
    }
}

fn monotonic_ns() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: clock_gettime initializes the valid timespec pointer.
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0 {
            return None;
        }
        u64::try_from(time.tv_sec)
            .ok()?
            .checked_mul(1_000_000_000)?
            .checked_add(u64::try_from(time.tv_nsec).ok()?)
    }
    #[cfg(not(target_os = "linux"))]
    None
}

fn record(event: &str, value: Value) {
    if let Some(at_ns) = monotonic_ns() {
        eprintln!(
            "{}",
            json!({"response_profile":true,"event":event,"at_ns":at_ns,"value":value})
        );
    }
}

pub(crate) fn stage(name: &str) {
    if enabled() {
        record("stage", json!({"name":name}));
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn observed(action: Action, code: u16) {
    if (enabled() && matches!(action, Action::LauncherHome | Action::Start | Action::Back))
        || (interactions() && !matches!(action, Action::Theme))
    {
        record("input_observed", json!({"action":action,"code":code}));
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn dispatched(action: Action) {
    if (enabled() && matches!(action, Action::LauncherHome | Action::Start | Action::Back))
        || (interactions() && !matches!(action, Action::Theme))
    {
        record("input_dispatched", json!({"action":action}));
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn received(view: &View) {
    if enabled() || interactions() {
        record(
            "view_received",
            json!({"page":view.page,"return_to_launcher":view.return_to_launcher,"generation":view.input_generation,"target":Target::from_view(view)}),
        );
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Frame {
    layer: Layer,
    panel: Fixed,
    page: Page,
    selected: Option<usize>,
    actions_ready: bool,
}

/// Reports successful submissions without changing input or rendering.
///
/// **Inserts**
/// - hooks: `on_timing`
#[derive(Default)]
struct Response {
    previous: Option<Frame>,
    interaction_frames: u32,
}

impl Response {
    fn accept(&mut self, accepted: u64, outcome: TimingOutcome, frame: Frame) -> bool {
        if accepted == 0 || outcome != TimingOutcome::Completed || self.previous == Some(frame) {
            return false;
        }
        self.previous = Some(frame);
        true
    }
}

impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for Response {
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_timing(&mut self, world: &mut World, sample: &TimingSample<'_>) {
        if sample.accepted_submissions == 0 || sample.outcome != TimingOutcome::Completed {
            return;
        }
        let Some(display) = world.resource::<DisplayRuntime>() else {
            return;
        };
        let frame = Frame {
            layer: display.deck.layer(),
            panel: display.deck.panel(),
            page: display.room.focus_page(),
            selected: display.room.selected(),
            actions_ready: world
                .query::<super::actions_damage::ActionsPanel>()
                .iter()
                .any(|(entity, _)| {
                    !mirui::ui::branch::is_effectively_hidden(world, entity)
                        && world
                            .get::<mirui::ui::ComputedRect>(entity)
                            .is_some_and(|rect| rect.0.w > Fixed::ZERO && rect.0.h > Fixed::ZERO)
                }),
        };
        if interactions() && self.interaction_frames < INTERACTION_FRAME_LIMIT {
            self.interaction_frames += 1;
            let launcher = display.state.launcher();
            let editor = if launcher.management().active() {
                format!("management/{:?}", launcher.management().page())
            } else if launcher.saves().active() {
                format!(
                    "saves/{:?}",
                    std::mem::discriminant(&launcher.saves().page())
                )
            } else if launcher.configuration().active() {
                format!("configuration/{:?}", launcher.configuration().section())
            } else if launcher.wifi_active() {
                format!("wifi/{:?}", launcher.wifi_page())
            } else {
                String::new()
            };
            let focus: Vec<_> =
                world
                    .query::<super::selection::SelectionFrame>()
                    .iter()
                    .filter(|(entity, _)| !mirui::ui::branch::is_effectively_hidden(world, *entity))
                    .filter_map(|(entity, _)| {
                        let rect = world.get::<mirui::ui::ComputedRect>(entity)?.0;
                        (rect.w > Fixed::ZERO && rect.h > Fixed::ZERO).then(|| json!({
                        "entity":format!("{entity:?}"),
                        "rect":[rect.x.to_f32(),rect.y.to_f32(),rect.w.to_f32(),rect.h.to_f32()]
                    }))
                    })
                    .collect();
            record(
                "interaction_frame",
                json!({
                    "target":display.state.response_target(),
                    "menu":launcher.menu_selection(),"editor":editor,
                    "grid":display.shelf.shelf().plan().grid,
                    "scroll":if launcher.management().active() { launcher.management().detail_scroll().to_f32() }
                        else if launcher.configuration().active() { launcher.configuration().offset().to_f32() }
                        else if launcher.saves().active() { launcher.saves().reading_offset().to_f32() }
                        else { 0.0 },
                    "layer":format!("{:?}",frame.layer),"panel":frame.panel.to_f32(),
                    "focus":focus,"wall_ns":sample.wall_nanos,"cpu_ns":sample.thread_cpu_nanos,
                    "accepted":sample.accepted_submissions,"instrumented":sample.instrumented,
                    "phases_ns":{
                        "input":sample.phases.input_nanos(),"systems":sample.phases.systems_nanos(),
                        "layout":sample.phases.layout_nanos(),"render":sample.phases.render_nanos(),
                        "flush":sample.phases.flush_nanos(),"seed":sample.phases.seed_prev_nanos()
                    }
                }),
            );
        }
        if self.accept(sample.accepted_submissions, sample.outcome, frame) {
            let layer = match frame.layer {
                Layer::Shelf => "Shelf",
                Layer::Catalog => "Catalog",
                Layer::Room => "Room",
                Layer::Actions => "Actions",
            };
            record(
                "frame_submitted",
                json!({"layer":layer,"panel":frame.panel.to_f32(),"page":frame.page,"selected":frame.selected,"actions_ready":frame.actions_ready}),
            );
        }
    }
}

pub(crate) fn install<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    if enabled() || interactions() {
        app.add_plugin(
            mirui::app::plugins::FpsSummaryPlugin::new(3600)
                .with_sink(|_| {})
                .with_thread_cpu_time(interactions()),
        );
        app.add_plugin(Response::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interaction_targets_track_routes_and_focus_without_private_content() {
        let mut view = View {
            page: Page::Games,
            library: crate::model::LibraryView {
                kind: LibraryKind::Favorites,
                ..Default::default()
            },
            launcher: crate::model::LauncherView {
                enabled: true,
                key: "game-key".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let first = Target::from_view(&view);
        view.status = "private-status".into();
        view.launcher.notice = "private-notice".into();
        assert_eq!(first, Target::from_view(&view));
        view.launcher.key = "second-key".into();
        assert_ne!(first, Target::from_view(&view));
        let second = Target::from_view(&view);
        view.launcher.overlay_kind = "system".into();
        assert_ne!(second, Target::from_view(&view));
        let menu = Target::from_view(&view);
        view.input_generation.advance();
        assert_ne!(menu, Target::from_view(&view));
        let text = serde_json::to_string(&Target::from_view(&view)).unwrap();
        assert!(!text.contains("private-status"));
        assert!(!text.contains("private-notice"));
    }

    #[test]
    fn state_changes_without_successful_submission_do_not_become_visual_feedback() {
        let mut response = Response::default();
        let frame = Frame {
            layer: Layer::Room,
            panel: Fixed::ONE,
            page: Page::Lobby,
            selected: None,
            actions_ready: false,
        };
        assert!(!response.accept(0, TimingOutcome::Completed, frame));
        assert!(!response.accept(1, TimingOutcome::Failed, frame));
        assert!(response.accept(1, TimingOutcome::Completed, frame));
        assert!(!response.accept(1, TimingOutcome::Completed, frame));
        let closed = Frame {
            layer: Layer::Shelf,
            panel: Fixed::ZERO,
            ..frame
        };
        assert!(!response.accept(0, TimingOutcome::Idle, closed));
        assert!(response.accept(1, TimingOutcome::Completed, closed));
        assert!(response.accept(1, TimingOutcome::Completed, frame));
    }
}
