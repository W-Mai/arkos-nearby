use super::actor::actor;
use super::artwork::preview as super_preview;
use super::deck::{ActionAnchor, ActorScope, Layer};
use super::{
    chrome::{footer, header},
    composition::{focus_frame, room_pages},
    deck::{Deck, FULL, catalog_share, shelf_share, transform},
    library::library,
    pixel::{Scenery::Margins, backdrop},
    state::{DisplayState, PageKey},
};
use super::{
    style::*,
    widgets::{control_key, control_label, shadow, text_block},
};
use crate::launcher::Launcher;
use crate::model::{LibraryKind, Page};
use mirui::prelude::*;
use mirui::ui::widgets::Image;
use mirui::ui::widgets::TextAlign::{Center, Start};
use mirui::ui::widgets::page_view::{CreatePage, PageView};

#[event]
fn create_library(
    event: event!(CreatePage<PageKey>),
    state: model!(DisplayState),
    metrics: Metrics,
) -> Entity {
    let route = *event.route();
    ui! {
        library_page (route, state, metrics)
    }
}

#[compose]
fn library_page(route: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0) {
            library_contents (route, state, metrics)
        }
    }
}

#[compose]
fn library_contents(route: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let body = if route.library == LibraryKind::Favorites {
        ui! {
            Column (grow: 1.0, min_width: 0, min_height: 0) [
                super::cache::SettledLayerCache {
                    state: state.clone(),
                    backdrop: true,
                },
            ] {
                library (route.library, state, metrics)
            }
        }
    } else {
        ui! {
            library (route.library, state, metrics)
        }
    };
    ui! {
        focus_frame (body, route, state)
    }
}

#[compose]
pub(super) fn deck(
    state: model!(DisplayState),
    shelf: model!(DisplayState),
    catalog: model!(DisplayState),
    room: model!(DisplayState),
    deck: model!(Deck),
    metrics: Metrics,
) -> Entity {
    let panel_metrics = Metrics {
        compact: true,
        gap: metrics.gap / 2,
        ..metrics
    };
    let route = PageKey {
        page: Page::Games,
        library: LibraryKind::Favorites,
        network_only: false,
    };
    let modal = res!(super::modal::Runtime).with(|runtime| runtime.model.clone());
    let snapshot = modal.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, bg_color: BACKGROUND) {
            Column (
                id: "launcher_background",
                grow: 1.0,
                min_height: 0,
                visible: ${ !modal.hidden() },
                bg_color: BACKGROUND
            ) [
                super::modal::BackgroundRoot,
            ] {
                header (route, shelf, metrics)
                View (height: metrics.rule_height(), shrink: 0.0, bg_color: WARNING)
                Column (
                    grow: 1.0,
                    min_height: 0,
                    padding: metrics.page_padding(),
                    row_gap: metrics.gap
                ) {
                    backdrop (Margins)
                    Column (id: "launcher_deck", grow: 1.0, min_height: 0, container: true) {
                        shelf_viewport (shelf, deck, metrics)
                        catalog_viewport (catalog, deck, metrics)
                    } on SizeChanged { deck.set_height(event.new.height); }
                    footer (route, state, metrics)
                }
            }
            View (
                id: "launcher_background_snapshot_layer",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100)
            ) [
                mirui::ui::IgnoreHitTest,
            ] {
                walk ${ snapshot.token().into_iter() } with revision by revision {
                    Image (
                        position: Position::Absolute,
                        left: 0,
                        top: 0,
                        width: Dimension::percent(100),
                        height: Dimension::percent(100),
                        src: super::modal::texture_token(revision)
                    ) [
                        mirui::ui::IgnoreHitTest,
                    ]
                }
            }
            Column (
                visible: ${ deck.panel() > Fixed::ZERO },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                bg_color: ${ Color::rgba(7, 16, 12, (deck.panel() * Fixed::from_int(185)).to_int() as u8) },
                padding: metrics.modal_padding(),
                justify: JustifyContent::Center,
                align: AlignItems::Stretch
            ) {
                Column (
                    grow: 1.0,
                    min_height: 0,
                    border_width: 2,
                    border_color: DIVIDER,
                    bg_color: BACKGROUND,
                    transform: ${
                        mirui::types::Transform::translate(
                                Fixed::ZERO,
                                (Fixed::ONE - deck.panel()) * Fixed::from_int(80),
                            )
                            .compose(
                                &mirui::types::Transform::scale(
                                    Fixed::from_ratio(15, 16) + deck.panel() * Fixed::from_ratio(1, 16),
                                    Fixed::from_ratio(15, 16) + deck.panel() * Fixed::from_ratio(1, 16),
                                ),
                            )
                    }
                ) {
                    Column (grow: 1.0, min_height: 0) [
                        ActorScope::Layer(Layer::Room),
                    ] {
                        room_pages (room, panel_metrics)
                    }
                }
                shadow (metrics)
            }
            primary_overlay (state.clone(), metrics)
            actor (state, deck)
        }
    }
}

#[compose]
pub(super) fn overlay(display: model!(DisplayState), metrics: Metrics) -> Entity {
    let state = display.launcher();
    ui! {
        graphical_overlay (display, state, metrics, false)
    }
}

#[compose]
fn primary_overlay(display: model!(DisplayState), metrics: Metrics) -> Entity {
    let state = display.launcher();
    ui! {
        graphical_overlay (display, state, metrics, true)
    }
}

#[compose]
fn graphical_overlay(
    display: model!(DisplayState),
    state: model!(Launcher),
    metrics: Metrics,
    primary: bool,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100)
        ) [
            mirui::ui::IgnoreHitTest,
            super::actions_damage::ActionsHost {
                primary,
            },
        ] {
            if ${ state.view().enabled && !state.view().overlay.is_empty() } {
                Column (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    padding: metrics.modal_padding(),
                    justify: JustifyContent::Center,
                    bg_color: Color::rgba(7, 16, 12, 185)
                ) {
                    Column (
                        grow: 1.0,
                        min_height: 0,
                        padding: Padding::all(metrics.gap * 2),
                        row_gap: metrics.gap,
                        bg_color: BACKGROUND,
                        border_width: 2,
                        border_color: DIVIDER
                    ) [
                        ActorScope::Layer(Layer::Actions),
                        super::actions_damage::ActionsPanel,
                    ] {
                        backdrop (Margins)
                        overlay_label (state, None, metrics)
                        View (height: metrics.rule_height(), bg_color: WARNING, shrink: 0.0)
                        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap * 2) {
                            if ${ state.view().overlay_kind == "game" } {
                                Column (width: Dimension::percent(38), shrink: 0.0, min_height: 0) {
                                    super_preview (display, metrics)
                                    shadow (metrics)
                                }
                            }
                            Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
                                walk 0_usize..3 with line {
                                    if ${ line * 2 < state.view().items.len() } {
                                        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap) {
                                            walk 0_usize..2 with column {
                                                if ${ line * 2 + column < state.view().items.len() } {
                                                    action_tile (
                                                        state,
                                                        line * 2 + column,
                                                        metrics
                                                    )
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Row (
                            height: metrics.footer_height(),
                            column_gap: metrics.gap * 2,
                            align: AlignItems::Center
                        ) {
                            control_key ("A", metrics, true)
                            control_label (|| "确认".into(), metrics)
                            control_key ("B", metrics, false)
                            control_label (|| "返回".into(), metrics)
                        }
                    }
                    shadow (metrics)
                }
            }
        }
    }
}

#[compose]
fn action_tile(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            transform: ${
                mirui::types::Transform::translate(
                    Fixed::ZERO,
                    Fixed::from_int(
                        if state.view().selected == index {
                            super::motion::hop(state.selection_age(), 4.0)
                        } else {
                            0
                        },
                    ),
                )
            }
        ) {
            Column (
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                padding: Padding::all(metrics.gap),
                row_gap: metrics.gap / 2,
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                bg_color: ${
                    if state.view().selected == index
                        && state.view().items.get(index).is_some_and(|item| item.enabled)
                    {
                        ACCENT
                    } else {
                        PANEL
                    }
                },
                border_width: 2,
                border_color: ${ if state.view().selected == index { WARNING } else { DIVIDER } }
            ) [
                ActionAnchor(index),
            ] {
                action_icon (state, index)
                overlay_label (state, Some(index), metrics)
            }
            shadow (metrics)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ActionGlyph {
    Play,
    Signal,
    Favorite,
    Sound,
    Power,
    Exit,
    Back,
}

fn action_glyph(kind: &str, index: usize) -> ActionGlyph {
    match (kind, index) {
        ("game", 0) | ("network", 0) => ActionGlyph::Play,
        ("game", 1) | ("system", 2) | ("network", 1) => ActionGlyph::Signal,
        ("game", 2) => ActionGlyph::Favorite,
        ("system", 1) => ActionGlyph::Sound,
        ("system", 4) | ("power", 1) => ActionGlyph::Power,
        ("system", 3) | ("exit", 1) => ActionGlyph::Exit,
        _ => ActionGlyph::Back,
    }
}

fn action_foreground(view: &crate::model::LauncherView, index: usize) -> ColorToken {
    if !view.items.get(index).is_some_and(|item| item.enabled) {
        SECONDARY
    } else if view.selected == index {
        ON_ACCENT
    } else {
        FOREGROUND
    }
}

#[compose]
fn action_icon(state: model!(Launcher), index: usize) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 16,
            max_height: 32,
            width: Dimension::percent(100),
            align: AlignItems::Center
        ) {
            if ${ state.view().overlay_kind == "system" && index == 0 } {
                Row (
                    grow: 1.0,
                    width: Dimension::percent(70),
                    column_gap: 3,
                    padding: Padding::all(2),
                    bg_color: PANEL,
                    border_width: 1,
                    border_color: ${ action_foreground(&state.view(), index) }
                ) {
                    walk [ACCENT, WARNING, CLAY, SKY].into_iter() with color {
                        View (grow: 1.0, height: Dimension::percent(100), bg_color: color)
                    }
                }
            } else {
                Column (width: 32, grow: 1.0, min_height: 0) {
                    walk ${
                        std::iter::once((
                            action_glyph(&state.view().overlay_kind, index),
                            action_foreground(&state.view(), index),
                        ))
                    } with icon by icon {
                        Image (
                            src: match icon.0 {
                                ActionGlyph::Play => {
                                    path!(M 3 2 H 6 V 4 H 10 V 7 H 14 V 10 H 10 V 13 H 6 V 15 H 3 Z)
                                }
                                ActionGlyph::Signal => {
                                    path!(
                                        M 7 1 H 9 V 10 H 7 Z M 4 4 H 6 V 12 H 4 Z M 10 4 H 12 V 12 H 10 Z M 1 7 H
                                        3 V 14 H 1 Z M 13 7 H 15 V 14 H 13 Z
                                    )
                                }
                                ActionGlyph::Favorite => {
                                    path!(
                                        M 6 0 H 10 V 4 H 16 V 7 H 12 V 11 H 14 V 16 H 10 V 13 H 6 V 16 H 2 V 11 H
                                        4 V 7 H 0 V 4 H 6 Z
                                    )
                                }
                                ActionGlyph::Sound => {
                                    path!(
                                        M 1 6 H 4 V 4 H 6 V 2 H 8 V 14 H 6 V 12 H 4 V 10 H 1 Z M 10 5 H 12 V 11 H
                                        10 Z M 13 2 H 15 V 14 H 13 Z
                                    )
                                }
                                ActionGlyph::Power => {
                                    path!(
                                        M 7 0 H 9 V 8 H 7 Z M 3 3 H 5 V 5 H 3 V 11 H 5 V 13 H 11 V 11 H 13 V 5 H
                                        11 V 3 H 13 V 5 H 15 V 11 H 13 V 13 H 11 V 15 H 5 V 13 H 3 V 11 H 1 V 5 H
                                        3 Z
                                    )
                                }
                                ActionGlyph::Exit => {
                                    path!(
                                        M 1 1 H 8 V 3 H 3 V 13 H 8 V 15 H 1 Z M 11 3 H 13 V 5 H 15 V 7 H 16 V 9 H
                                        15 V 11 H 13 V 13 H 11 V 10 H 5 V 6 H 11 Z
                                    )
                                }
                                ActionGlyph::Back => {
                                    path!(
                                        M 6 2 H 8 V 6 H 15 V 10 H 8 V 14 H 6 V 12 H 4 V 10 H 2 V 6 H 4 V 4 H 6 Z
                                    )
                                }
                            },
                            viewbox: 16,
                            width: 32,
                            height: Dimension::percent(100),
                            color: icon.1
                        )
                    }
                }
            }
        }
    }
}

#[compose]
fn overlay_label(state: model!(Launcher), index: Option<usize>, metrics: Metrics) -> Entity {
    let text = state.clone();
    ui! {
        text_block (
            move || {
                index
                    .map_or_else(
                        || text.view().overlay,
                        |index| {
                            text.view()
                                .items
                                .get(index)
                                .map(|item| item.name.clone())
                                .unwrap_or_default()
                        },
                    )
            },
            move || { index.map_or(FOREGROUND, |index| action_foreground(&state.view(), index)) },
            metrics.title,
            if index.is_none() { Start } else { Center },
            LABEL_LEADING,
            2
        )
    }
}

#[compose]
fn shelf_viewport(state: model!(DisplayState), deck: model!(Deck), metrics: Metrics) -> Entity {
    let route = PageKey {
        page: Page::Games,
        library: LibraryKind::Favorites,
        network_only: false,
    };
    ui! {
        Column (position: Position::Absolute, width: Dimension::percent(100), top: 0, height: ${ Dimension::Percent(shelf_share(deck.expansion()) * Fixed::from_int(100)) }, clip_children: true, min_height: 0, container: true) {
            Column (position: Position::Absolute, top: 0, width: Dimension::percent(100), height: @(id("launcher_deck").height as height) { height }, transform: ${ super::deck::shelf_transform(deck.expansion(), deck.height()) }, padding: Padding::all(metrics.gap), min_height: 0) [ActorScope::Layer(Layer::Shelf)] {
                shelf_snapshot_page (route, state, metrics)
            }
        }
    }
}

#[compose]
fn shelf_snapshot_page(route: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let frozen = res!(super::suspension::Runtime).with(|runtime| runtime.model.clone());
    let image = frozen.clone();
    let body = ui! {
        Column (
            id: "shelf_snapshot_surface",
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            container: true
        ) {
            Column (
                id: "shelf_snapshot_source",
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                visible: ${ !frozen.hidden() }
            ) [
                super::suspension::ShelfSource,
                super::cache::SettledLayerCache {
                    state: state.clone(),
                    backdrop: true,
                },
            ] {
                library (route.library, state, metrics)
            }
            View (
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100)
            ) [
                mirui::ui::IgnoreHitTest,
            ] {
                walk ${ image.token().into_iter() } with revision by revision {
                    Image (
                        width: Dimension::percent(100),
                        height: Dimension::percent(100),
                        src: super::suspension::texture_token(revision)
                    ) [
                        mirui::ui::IgnoreHitTest,
                    ]
                }
            }
        }
    };
    ui! {
        focus_frame (body, route, state)
    }
}

#[compose]
fn catalog_viewport(state: model!(DisplayState), deck: model!(Deck), metrics: Metrics) -> Entity {
    ui! {
        Column (visible: ${ deck.expansion() > Fixed::ZERO }, position: Position::Absolute, width: Dimension::percent(100), top: 0, height: ${ Dimension::Percent(catalog_share(deck.expansion()) * Fixed::from_int(100)) }, transform: ${ mirui::types::Transform::translate(Fixed::ZERO, deck.height() * shelf_share(deck.expansion())) }, clip_children: true, min_height: 0, container: true) {
            Column (position: Position::Absolute, top: 0, width: Dimension::percent(100), height: @(id("launcher_deck").height as height) { height * FULL }, transform: ${ transform(deck.expansion(), deck.height() * FULL) }, padding: Padding::all(metrics.gap), min_height: 0) [ActorScope::Layer(Layer::Catalog)] {
                PageView (grow: 1.0, min_height: 0, page: ${ state.page_key() }, transition: ${ state.transition() }) on CreatePage(create_library::callback(state.clone(), metrics))
            }
        }
    }
}
