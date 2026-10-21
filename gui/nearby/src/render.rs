use crate::model::View;
use copy::loading;
use mirui::app::RendererFactory;
use mirui::prelude::*;
use mirui::surface::Surface;
use mirui::ui::widgets::page_view;
use state::{DisplayHandle, PageKey};
use style::{ACCENT_DIM, BACKGROUND, Metrics};

mod actor;
mod artwork;
mod cache;
mod choices;
mod chrome;
mod composition;
mod consoles;
mod copy;
mod deck;
mod font;
mod launcher;
mod library;
mod lists;
pub(crate) mod motion;
mod pages;
mod pixel;
mod rows;
mod selection;
mod sessions;
mod shelf;
mod state;
mod style;
mod widgets;

#[component]
struct StaticCache;

#[component]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Shell,
    Header,
    Content,
    Footer,
    Status,
    FocusList,
    FocusHost,
}

pub fn install<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) -> Result<(), String> {
    app.with_theme(crate::theme::Palette::default().theme());
    app.with_offscreen_pool_budget(8 * 1024 * 1024);
    app.with_widget(page_view::view::<PageKey>());
    app.with_widget(selection::target::view());
    app.with_widget(selection::frame::view());
    app.with_widget(pixel::choice_sprite::view());
    app.with_widget(pixel::walker_sprite::view());
    app.with_widget(pixel::actor_sprite::view());
    app.with_widget(pixel::session_sprite::view());
    app.with_widget(pixel::signal_sprite::view());
    app.with_widget(pixel::search_sprite::view());
    app.with_widget(pixel::scenery::view());
    app.with_widget(cache::gpu_layer::view());
    font::install(app)?;
    state::install(app);
    artwork::install(app)?;
    Ok(())
}

pub struct Scene {
    state: DisplayHandle,
    shelf: DisplayHandle,
    catalog: DisplayHandle,
    room: DisplayHandle,
    deck: deck::DeckHandle,
}

impl Scene {
    #[cfg(any(target_os = "linux", test))]
    pub fn navigation_intent(&self, backward: bool) {
        if backward {
            self.state.navigate_back();
            self.catalog.navigate_back();
            self.room.navigate_back();
        } else {
            self.state.navigate_forward();
            self.catalog.navigate_forward();
            self.room.navigate_forward();
        }
    }

    pub fn press(&self, now: u64) {
        self.state.press(now);
        if self.deck.enabled() {
            let active = match self.deck.layer() {
                deck::Layer::Shelf => Some(&self.shelf),
                deck::Layer::Catalog => Some(&self.catalog),
                deck::Layer::Room => Some(&self.room),
                deck::Layer::Actions => None,
            };
            if let Some(active) = active {
                active.press(now);
            }
        }
    }
}

pub fn update(scene: &mut Scene, view: &View) -> Result<(), String> {
    scene.deck.present(view);
    if view.launcher.enabled {
        for (snapshot, state, tab) in [
            (&view.launcher_shelf, &scene.shelf, 0),
            (&view.launcher_catalog, &scene.catalog, 1),
        ] {
            if let Some(snapshot) = snapshot {
                let mut snapshot = (**snapshot).clone();
                snapshot.launcher.enabled = true;
                snapshot.launcher.tab = tab;
                snapshot.launcher.navigation =
                    view.launcher.tab != tab || !view.launcher.overlay.is_empty();
                state.present(&snapshot);
            } else if view.launcher.tab == tab {
                state.present(view);
            }
        }
    } else if view.return_to_launcher {
        scene.room.present(view);
    }
    scene.state.present(view);

    Ok(())
}

#[cfg(any(target_os = "linux", test))]
pub fn cycle_theme<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    scene: &mut Scene,
) -> Result<Option<crate::theme::Palette>, String> {
    if !scene.state.can_switch_theme() {
        return Ok(None);
    }
    let settings = scene.state.settings();
    settings.cycle_theme();
    let palette = settings.palette();
    app.set_theme(palette.theme())
        .map_err(|error| format!("Theme: {error:?}"))?;
    Ok(Some(palette))
}

pub fn build<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    view: &View,
) -> Result<Scene, String> {
    let root = match app.root {
        Some(root) => root,
        None => app.spawn_root().bg_color(BACKGROUND).id(),
    };
    let state = app.compose(root, state::display_state());
    let preferences = state.settings();
    preferences.restore(
        crate::theme::Palette::parse(app.theme().id().as_str()).unwrap_or_default(),
        preferences.sound(),
    );
    let (shelf, catalog, room, deck) = app.compose(root, retained_states());
    let mut scene = Scene {
        state: state.clone(),
        shelf,
        catalog,
        room,
        deck,
    };
    update(&mut scene, view)?;
    let at = app
        .world
        .resource::<mirui::ecs::MonoClock>()
        .map_or(0, |clock| clock.now_ns() / 1_000_000);
    state.animate(at, 0);
    state.feedback(
        app.theme().resolve(ACCENT_DIM),
        [if loading(view) {
            app.theme().resolve(ACCENT_DIM)
        } else {
            app.theme().resolve(BACKGROUND)
        }; 3],
    );
    if !app.compose(root, has_pages()) {
        let metrics = Metrics::for_height(app.backend.viewport().logical_size().1);
        app.compose(
            root,
            composition::shell(state.clone(), metrics, view.launcher.enabled),
        );
    }
    Ok(scene)
}

#[compose]
fn retained_states() -> (
    DisplayHandle,
    DisplayHandle,
    DisplayHandle,
    deck::DeckHandle,
) {
    res!(state::DisplayRuntime).with(|runtime| {
        (
            runtime.shelf.clone(),
            runtime.catalog.clone(),
            runtime.room.clone(),
            runtime.deck.clone(),
        )
    })
}

#[compose]
fn has_pages() -> bool {
    res!(mirui::ui::IdMap).with(|ids| ids.get("nearby_pages").is_some())
}

pub fn draw<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    view: &View,
) -> Result<Scene, String> {
    let scene = build(app, view)?;
    app.render()
        .map_err(|error| format!("Menu frame: {error:?}"))?;
    Ok(scene)
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;

// Deterministic capture uses the same scheduler and clock contract as the device.
thread_local! { static CAPTURE_TIME_NS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

fn capture_time_ns() -> u64 {
    CAPTURE_TIME_NS.with(std::cell::Cell::get)
}

pub fn paint_at<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    now: u64,
) -> Result<(), String> {
    CAPTURE_TIME_NS.with(|time| time.set(now * 1_000_000));
    if let Some(clock) = app.world.resource_mut::<mirui::ecs::MonoClock>() {
        clock.clock = capture_time_ns;
    } else {
        app.add_resource(mirui::ecs::MonoClock {
            clock: capture_time_ns,
            last_ns: 0,
        });
    }
    tick(app)
}

pub fn tick<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) -> Result<(), String> {
    if app.tick() {
        Err("The display requested shutdown".into())
    } else {
        Ok(())
    }
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn settings<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
) -> crate::settings::SettingsHandle {
    let root = app.root.unwrap_or_else(|| app.spawn_root().id());
    app.compose(root, state::display_state()).settings()
}

#[cfg(target_os = "linux")]
pub fn now<B: Surface, F: RendererFactory<B>>(app: &App<B, F>) -> u64 {
    app.world
        .resource::<mirui::ecs::MonoClock>()
        .expect("Live UI clock is installed")
        .now_ns()
        / 1_000_000
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn launcher<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
) -> crate::launcher::LauncherHandle {
    let root = app.root.unwrap_or_else(|| app.spawn_root().id());
    app.compose(root, state::display_state()).launcher()
}

#[cfg(any(target_os = "linux", test))]
pub fn subscribe_shelf_layout<B: Surface, F: RendererFactory<B>>(
    app: &App<B, F>,
    listener: impl Fn(&shelf::LayoutRequest) + 'static,
) -> Result<(), String> {
    let runtime = app
        .world
        .resource::<state::DisplayRuntime>()
        .ok_or("Display runtime is missing")?;
    let listener = std::rc::Rc::new(listener);
    for state in [&runtime.state, &runtime.shelf] {
        let listener = listener.clone();
        state
            .shelf()
            .on_effect(move |request: &shelf::LayoutRequest| listener(request))
            .map_err(|e| format!("Shelf effects: {e:?}"))?;
    }
    Ok(())
}
