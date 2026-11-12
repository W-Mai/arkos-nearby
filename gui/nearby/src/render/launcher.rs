use super::actor::actor;
use super::configuration::{hints as configuration_hints, panel as configuration_panel};
use super::deck::{ActionAnchor, ActionLayout, ActorScope, Layer};
use super::deferred::Pane;
use super::management::{hints as management_hints, panel as management_panel};
use super::saves::{hints as saves_hints, panel as saves_panel};
use super::wifi::{heading as wifi_heading, hints as wifi_hints, panel as wifi_panel};
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
use mirui::ui::widgets::TextAlign::{Center, Start};
use mirui::ui::widgets::page_view::{CreatePage, PageView};
use mirui::ui::widgets::{Image, Text};

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
                Image (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    src: ${ snapshot.source() }
                ) [
                    mirui::ui::IgnoreHitTest,
                ]
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
            actor (state, deck, shelf, catalog, room)
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
            if ${ state.overlay_visible() } {
                PageView (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    min_width: 0,
                    min_height: 0,
                    page: Pane::Overlay
                ) on CreatePage(create_overlay::callback(display.clone(), state.clone(), metrics))
            }
        }
    }
}

#[event]
fn create_overlay(
    _event: event!(CreatePage<Pane>),
    display: model!(DisplayState),
    state: model!(Launcher),
    metrics: Metrics,
) -> Entity {
    ui! {
        overlay_contents (display, state, metrics)
    }
}

#[compose]
fn overlay_contents(
    display: model!(DisplayState),
    state: model!(Launcher),
    metrics: Metrics,
) -> Entity {
    let hint = state.clone();
    ui! {
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
                if ${ state.wifi_active() } {
                    wifi_heading (state, metrics)
                } else {
                    overlay_label (state, None)
                }
                View (height: metrics.rule_height(), bg_color: WARNING, shrink: 0.0)
                Row (grow: 1.0, min_height: 0, column_gap: metrics.gap * 2) {
                    if ${
                        state.management().active() || state.saves().active()
                            || state.configuration().active()
                    } {
                        configuration_or_saves_panel (state, metrics)
                    } else {
                        Column (grow: 1.0, min_height: 0, min_width: 0) {
                            if ${ state.wifi_active() } {
                                wifi_panel (state, metrics)
                            } else {
                                Column (grow: 1.0, min_height: 0, min_width: 0) {
                                    if ${ state.overlay_kind() == "controls" } {
                                        level_panel (state, display.settings(), metrics)
                                    } else {
                                        Column (grow: 1.0, min_width: 0, min_height: 0) {
                                            if ${ state.overlay_kind() == "details" } {
                                                details_panel (state, metrics)
                                            } else {
                                                Column (
                                                    grow: 1.0,
                                                    min_width: 0,
                                                    min_height: 0,
                                                    row_gap: metrics.gap
                                                ) {
                                                    walk ${ 0_usize..state.item_count().div_ceil(2) } with line {
                                                        Row (
                                                            grow: 1.0,
                                                            min_height: 0,
                                                            column_gap: metrics.gap
                                                        ) {
                                                            walk 0_usize..2 with column {
                                                                if ${ line * 2 + column < state.item_count() } {
                                                                    action_tile (
                                                                        state,
                                                                        display.settings(),
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
                                    }
                                }
                            }
                        }
                    }
                }
                Row (
                    height: metrics.footer_height(),
                    shrink: 0.0,
                    column_gap: metrics.gap * 2,
                    align: AlignItems::Center
                ) {
                    if ${ state.wifi_active() } {
                        wifi_hints (state, metrics)
                    } else {
                        Row (grow: 1.0, min_width: 0) {
                            if ${
                                state.management().active() || state.saves().active()
                                    || state.configuration().active()
                            } {
                                configuration_or_saves_hints (state, metrics)
                            } else {
                                Row (
                                    grow: 1.0,
                                    min_width: 0,
                                    column_gap: metrics.gap * 2,
                                    align: AlignItems::Center
                                ) {
                                    if ${ state.overlay_kind() == "controls" && state.selected() < 2 } {
                                        control_key ("左右", metrics, true)
                                        control_label ("调节", metrics)
                                    } else {
                                        control_key ("A", metrics, true)
                                        control_label (
                                            ${ hint.confirm_label() },
                                            metrics
                                        )
                                    }
                                    if ${ state.detail_expanded() } {
                                        control_key ("上下", metrics, false)
                                        control_label ("滚动", metrics)
                                    }
                                    control_key ("B", metrics, false)
                                    control_label ("返回", metrics)
                                    Column (grow: 1.0, min_width: 0) {
                                        action_reason (state, metrics)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            shadow (metrics)
        }
    }
}

#[component]
pub(super) struct DetailsScroll {
    state: model!(Launcher),
}

#[system(order = Animation)]
fn scroll_details(
    nodes: query!(
        Entity,
        DetailsScroll,
        mirui::input::event::scroll::ScrollOffset
    ),
    commands: commands!(),
) {
    for (entity, component, offset) in nodes {
        if !component.state.detail_active() {
            continue;
        }
        let y = component.state.detail_offset();
        if offset.y != y {
            commands.set::<super::scroll::Position>(entity, y);
        }
    }
}

#[view(component = DetailsScroll, systems = [scroll_details::system()])]
pub(super) fn details_scroll() {}

#[compose]
fn details_panel(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        PageView (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            page: Pane::Details
        ) on CreatePage(create_details_panel::callback(state, metrics))
    }
}

#[event]
fn create_details_panel(
    _event: event!(CreatePage<Pane>),
    state: model!(Launcher),
    metrics: Metrics,
) -> Entity {
    ui! {
        details_panel_contents (state, metrics)
    }
}

#[compose]
fn details_panel_contents(state: model!(Launcher), metrics: Metrics) -> Entity {
    use mirui::input::event::scroll::{ScrollConfig, ScrollOffset};
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            Row (
                height: i32::from(BODY_SIZE) + LABEL_LEADING,
                shrink: 0.0,
                min_width: 0,
                column_gap: metrics.gap
            ) {
                Text (
                    text: ${ state.detail_heading() },
                    grow: 1.0,
                    min_width: 0,
                    font: font(BODY_SIZE),
                    font_size: BODY_SIZE,
                    text_color: SECONDARY,
                    paragraph: super::widgets::paragraph(Start, 1)
                ) [
                    FONT_PAINT,
                ]
                Text (
                    text: ${ state.detail_platform() },
                    width: Dimension::Content,
                    shrink: 0.0,
                    font: font(BODY_SIZE),
                    font_size: BODY_SIZE,
                    text_color: SECONDARY
                ) [
                    FONT_PAINT,
                ]
            }
            Column (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                clip_children: true
            ) [
                ScrollOffset::default(),
                ScrollConfig {
                    elastic: false,
                    ..Default::default()
                },
                DetailsScroll {
                    state: state.clone(),
                },
                ActorScope::Reading,
            ] {
                walk ${ 0..state.detail_count() } with index by index {
                    detail_row (state, index, metrics)
                }
            } on SizeChanged { state.detail_viewport(event.new.height); }
        }
    }
}

#[compose]
fn detail_row(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: ${
                state.detail_size(index, metrics.gap)
                    + Fixed::from_int(metrics.shadow_height() + metrics.gap)
            },
            shrink: 0.0,
            min_width: 0,
            padding: Padding {
                bottom: Dimension::px(metrics.gap),
                ..Default::default()
            },
            transform: ${
                mirui::types::Transform::translate(
                    Fixed::ZERO,
                    Fixed::from_int(
                        if state.menu_selection() == Some(index) {
                            super::motion::hop(state.selection_age(), 2.0)
                        } else {
                            0
                        },
                    ),
                )
            }
        ) {
            Row (
                height: ${ state.detail_size(index, metrics.gap) },
                shrink: 0.0,
                min_width: 0,
                padding: Padding::all(metrics.gap),
                column_gap: metrics.gap,
                bg_color: ${ if state.menu_selection() == Some(index) { ACCENT_DIM } else { PANEL } },
                border_width: 2,
                border_color: ${ if state.menu_selection() == Some(index) { ACCENT } else { DIVIDER } }
            ) [
                ActionAnchor(index, ActionLayout::Details),
            ] {
                View (width: 3, height: 40, shrink: 0.0, bg_color: ACCENT)
                Column (grow: 1.0, min_width: 0, height: Dimension::percent(100), row_gap: 4) {
                    detail_text (state, index, 0, TITLE_SIZE)
                    detail_text (state, index, 1, BODY_SIZE)
                    if ${ state.menu_selection() == Some(index) && state.detail_expanded() } {
                        View (height: 1, bg_color: DIVIDER)
                        Column (
                            height: ${ state.detail_facts_size(index, metrics.gap) },
                            shrink: 0.0,
                            min_width: 0,
                            row_gap: metrics.gap
                        ) {
                            walk ${ 0..state.detail_fact_count(index) } with field by field {
                                detail_fact (state, index, field, metrics)
                            }
                        }
                    }
                }
                Column (width: 24, height: 24, shrink: 0.0) {
                    action_icon (state, index)
                }
            }
            shadow (metrics)
        } on SizeChanged { state.detail_measure(index, event.new.height); }
    }
}

#[compose]
fn detail_text(state: model!(Launcher), index: usize, part: usize, size: u16) -> Entity {
    ui! {
        Text (
            text: ${
                match part {
                    0 => state.item(index).map(|item| item.name).unwrap_or_default(),
                    1 => state.item(index).map(|item| item.value).unwrap_or_default(),
                    _ => String::new(),
                }
            },
            height: Dimension::Content,
            width: Dimension::percent(100),
            min_height: i32::from(size) + ROW_LEADING,
            min_width: 0,
            shrink: 0.0,
            font: font(size),
            font_size: size,
            text_color: if part == 1 { SECONDARY } else { FOREGROUND },
            paragraph: mirui::ui::widgets::ParagraphStyle {
                wrap: mirui::ui::widgets::TextWrap::Grapheme,
                max_lines: None,
                ..Default::default()
            }
        ) [
            FONT_PAINT,
        ] on SizeChanged { state.detail_text_size(index, part, event.new.height); }
    }
}

#[compose]
fn detail_fact(state: model!(Launcher), index: usize, field: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: ${ state.detail_fact_height(index, field) + Fixed::ONE },
            shrink: 0.0,
            min_width: 0
        ) {
            Row (
                height: ${ state.detail_fact_height(index, field) },
                shrink: 0.0,
                min_width: 0,
                column_gap: metrics.gap
            ) {
                Column (width: Dimension::percent(20), min_width: 56, shrink: 0.0) {
                    detail_fact_text (state, index, field, false)
                }
                Column (grow: 1.0, min_width: 0) {
                    detail_fact_text (state, index, field, true)
                }
            }
            View (height: 1, shrink: 0.0, bg_color: DIVIDER)
        }
    }
}

#[compose]
fn detail_fact_text(state: model!(Launcher), index: usize, field: usize, value: bool) -> Entity {
    ui! {
        Text (
            text: ${
                {
                    let fact = state.detail_fact(index, field);
                    if value { fact.value } else { fact.label }
                }
            },
            width: Dimension::percent(100),
            height: Dimension::Content,
            min_height: i32::from(BODY_SIZE) + ROW_LEADING,
            min_width: 0,
            shrink: 0.0,
            font: font(BODY_SIZE),
            font_size: BODY_SIZE,
            text_color: if value { FOREGROUND } else { SECONDARY },
            paragraph: mirui::ui::widgets::ParagraphStyle {
                wrap: mirui::ui::widgets::TextWrap::Grapheme,
                max_lines: None,
                ..Default::default()
            }
        ) [
            FONT_PAINT,
        ] on SizeChanged { state.detail_fact_measure(index, field, value, event.new.height); }
    }
}

#[compose]
fn level_panel(
    state: model!(Launcher),
    settings: model!(crate::settings::Settings),
    metrics: Metrics,
) -> Entity {
    ui! {
        PageView (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            page: Pane::Levels
        ) on CreatePage(create_level_panel::callback(state, settings, metrics))
    }
}

#[event]
fn create_level_panel(
    _event: event!(CreatePage<Pane>),
    state: model!(Launcher),
    settings: model!(crate::settings::Settings),
    metrics: Metrics,
) -> Entity {
    ui! {
        level_panel_contents (state, settings, metrics)
    }
}

#[compose]
fn level_panel_contents(
    state: model!(Launcher),
    settings: model!(crate::settings::Settings),
    metrics: Metrics,
) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
            level_row (state, 0, metrics)
            level_row (state, 1, metrics)
            Row (
                height: metrics.action_height() + 4,
                shrink: 0.0,
                column_gap: metrics.gap,
                min_height: 0
            ) {
                action_tile (state, settings, 2, metrics)
                action_tile (state, settings, 3, metrics)
            }
        }
    }
}

#[compose]
fn level_row(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: if metrics.compact { 60 } else { 76 }, min_width: 0) {
            Row (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                column_gap: metrics.gap,
                padding: Padding::all(metrics.gap),
                align: AlignItems::Center,
                bg_color: ${ if state.selected() == index { ACCENT_DIM } else { PANEL } },
                border_width: 2,
                border_color: ${ if state.selected() == index { ACCENT } else { DIVIDER } },
                transform: ${
                    mirui::types::Transform::translate(
                        Fixed::ZERO,
                        Fixed::from_int(
                            if state.selected() == index {
                                super::motion::hop(state.selection_age(), 3.0)
                            } else {
                                0
                            },
                        ),
                    )
                }
            ) [
                ActionAnchor(index, ActionLayout::Levels),
            ] {
                View (
                    width: 3,
                    height: Dimension::percent(100),
                    shrink: 0.0,
                    bg_color: ${ if state.selected() == index { ACCENT } else { DIVIDER } }
                )
                Column (width: 32, height: 32, shrink: 0.0) {
                    action_icon (state, index)
                }
                Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: 4) {
                    Row (
                        height: 24,
                        min_width: 0,
                        shrink: 0.0,
                        align: AlignItems::Center,
                        column_gap: metrics.gap
                    ) {
                        Text (
                            text: ${ state.item(index).map(|item| item.name).unwrap_or_default() },
                            grow: 1.0,
                            min_width: 0,
                            font: font(TITLE_SIZE),
                            font_size: TITLE_SIZE,
                            text_color: ${ action_foreground(&state, index) },
                            paragraph: super::widgets::paragraph(Start, 1)
                        ) [
                            FONT_PAINT,
                        ]
                        View (
                            width: 4,
                            height: 4,
                            bg_color: ${ if state.level(index).is_some_and(|level| level.pending) { WARNING } else { PANEL } }
                        )
                        Text (
                            text: ${
                                state
                                    .level(index)
                                    .and_then(|level| level.value)
                                    .map_or_else(|| "--".into(), |value| format!("{value}%"))
                            },
                            width: 60,
                            shrink: 0.0,
                            font: font(TITLE_SIZE),
                            font_size: TITLE_SIZE,
                            text_color: ${ action_foreground(&state, index) },
                            paragraph: super::widgets::paragraph(mirui::ui::widgets::TextAlign::End, 1)
                        ) [
                            FONT_PAINT,
                        ]
                    }
                    View (
                        height: 14,
                        width: Dimension::percent(100),
                        min_width: 0,
                        shrink: 0.0,
                        container: true,
                        bg_color: BACKGROUND,
                        border_width: 1,
                        border_color: SECONDARY
                    ) {
                        View (
                            position: Position::Absolute,
                            left: 0,
                            top: 0,
                            height: Dimension::percent(100),
                            width: ${ Dimension::Percent(state.level_positions()[index]) },
                            bg_color: ACCENT
                        )
                        walk 1..20 with segment {
                            View (
                                position: Position::Absolute,
                                left: Dimension::percent(segment * 5),
                                top: 0,
                                height: Dimension::percent(100),
                                width: 2,
                                bg_color: BACKGROUND
                            )
                        }
                        if ${ state.level(index).is_some_and(|level| level.value.is_some()) } {
                            View (
                                position: Position::Absolute,
                                left: ${ Dimension::Percent(state.level_positions()[index]) },
                                top: -4,
                                width: 10,
                                height: 22,
                                bg_color: WARNING,
                                border_width: 2,
                                border_color: FOREGROUND
                            ) [
                                mirui::ui::widgets::WidgetTransform(
                                    mirui::types::Transform::translate((-5).into(), Fixed::ZERO),
                                ),
                            ]
                        }
                    }
                }
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn action_tile(
    state: model!(Launcher),
    settings: model!(crate::settings::Settings),
    index: usize,
    metrics: Metrics,
) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            transform: ${
                mirui::types::Transform::translate(
                    Fixed::ZERO,
                    Fixed::from_int(
                        if state.selected() == index {
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
                    if state.selected() == index && state.item(index).is_some_and(|item| item.enabled())
                    {
                        ACCENT
                    } else {
                        PANEL
                    }
                },
                border_width: 2,
                border_color: ${ if state.selected() == index { WARNING } else { DIVIDER } }
            ) [
                ActionAnchor(index, ActionLayout::Grid),
            ] {
                Row (
                    grow: 1.0,
                    min_height: 0,
                    width: Dimension::percent(100),
                    column_gap: metrics.gap,
                    align: AlignItems::Center
                ) {
                    Column (width: 32, height: 32, shrink: 0.0) {
                        action_icon (state, index)
                    }
                    Column (
                        grow: 1.0,
                        min_width: 0,
                        row_gap: metrics.gap / 2,
                        justify: JustifyContent::Center
                    ) {
                        overlay_label (state, Some(index))
                        action_value (settings, state, index, metrics)
                    }
                }
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn action_value(
    settings: model!(crate::settings::Settings),
    state: model!(Launcher),
    index: usize,
    metrics: Metrics,
) -> Entity {
    let foreground = state.clone();
    ui! {
        Column (
            visible: ${
                state
                    .item(index)
                    .is_some_and(|item| {
                        !settings.menu_value(item.action).is_empty() || !item.value.is_empty()
                    })
            },
            shrink: 0.0
        ) {
            text_block (
                ${
                    {
                        state
                            .item(index)
                            .map(|item| {
                                let local = settings.menu_value(item.action);
                                if local.is_empty() { item.value } else { local.into() }
                            })
                            .unwrap_or_default()
                    }
                },
                ${ action_foreground(&foreground, index) },
                metrics.small,
                Center,
                LABEL_LEADING,
                1
            )
        }
    }
}

#[compose]
fn action_reason(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        text_block (
            ${
                {
                    let reason = state
                        .menu_selection()
                        .and_then(|selected| state.item(selected))
                        .map(|item| item.reason().to_owned())
                        .unwrap_or_default();
                    if reason.is_empty() { state.notice() } else { reason }
                }
            },
            SECONDARY,
            metrics.small,
            Start,
            LABEL_LEADING,
            1
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ActionGlyph {
    Play,
    Library,
    Signal,
    Favorite,
    Sound,
    Brightness,
    Settings,
    Power,
    Restart,
    Exit,
    Back,
    Toolbox,
    Info,
    Wifi,
    Network,
    Muted,
    Core,
    Device,
    Package,
    Firmware,
    Storage,
}

fn action_glyph(action: crate::model::MenuAction) -> ActionGlyph {
    use crate::model::MenuAction::*;
    match action {
        Play => ActionGlyph::Play,
        Multiplayer | ManageNetwork | RestorePlay => ActionGlyph::Signal,
        OpenWifi => ActionGlyph::Wifi,
        NetworkInfo => ActionGlyph::Network,
        ToggleFavorite => ActionGlyph::Favorite,
        ToggleSound | Volume => ActionGlyph::Sound,
        Brightness => ActionGlyph::Brightness,
        ShowControls | ShowGameSettings => ActionGlyph::Settings,
        ShowTools => ActionGlyph::Toolbox,
        ShowDevice => ActionGlyph::Device,
        ShowApplication => ActionGlyph::Package,
        SystemInfo | ShowGameDetails | ExpandDetail => ActionGlyph::Info,
        ShowCoreDetails => ActionGlyph::Core,
        ShowFirmwareDetails => ActionGlyph::Firmware,
        ShowSaveDetails | ShowSaveManager | ShowStorage => ActionGlyph::Storage,
        FileManager => ActionGlyph::Library,
        ShowPower | Poweroff => ActionGlyph::Power,
        Reboot => ActionGlyph::Restart,
        ShowExit | Exit | KeepNetworkExit => ActionGlyph::Exit,
        CycleTheme | Back => ActionGlyph::Back,
    }
}

fn item_glyph(item: Option<crate::model::LauncherItem>) -> ActionGlyph {
    match item {
        Some(item)
            if item.action == crate::model::MenuAction::Volume
                && item
                    .level
                    .is_some_and(|level| level.muted || level.value == Some(0)) =>
        {
            ActionGlyph::Muted
        }
        Some(item) => action_glyph(item.action),
        None => ActionGlyph::Back,
    }
}

fn action_foreground(state: &crate::launcher::LauncherHandle, index: usize) -> ColorToken {
    if !state.item(index).is_some_and(|item| item.enabled()) {
        SECONDARY
    } else if state.overlay_kind() == "details" || state.overlay_kind() == "controls" && index < 2 {
        FOREGROUND
    } else if state.selected() == index {
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
            if ${
                state
                    .item(index)
                    .is_some_and(|item| item.action == crate::model::MenuAction::CycleTheme)
            } {
                Row (
                    grow: 1.0,
                    width: Dimension::percent(70),
                    column_gap: 3,
                    padding: Padding::all(2),
                    bg_color: PANEL,
                    border_width: 1,
                    border_color: ${ action_foreground(&state, index) }
                ) {
                    walk [ACCENT, WARNING, CLAY, SKY].into_iter() with color {
                        View (grow: 1.0, height: Dimension::percent(100), bg_color: color)
                    }
                }
            } else {
                Column (width: 32, grow: 1.0, min_height: 0) {
                    walk ${ std::iter::once((item_glyph(state.item(index)), action_foreground(&state, index))) } with icon by icon {
                        Image (
                            src: match icon.0 {
                                ActionGlyph::Device => {
                                    path!(
                                        M 2 1 H 7 V 7 H 2 Z M 0 9 H 9 V 16 H 0 Z M 10 4 H 15 V 10 H 10 Z M 11 12
                                        H 16 V 16 H 11 Z
                                    )
                                }
                                ActionGlyph::Package => {
                                    path!(
                                        M 1 4 L 8 0 L 15 4 V 13 L 8 17 L 1 13 Z M 3 5 V 12 L 7 14 V 7 Z M 9 7 V
                                        14 L 13 12 V 5 Z
                                    )
                                }
                                ActionGlyph::Core => {
                                    path!(
                                        M 4 4 H 12 V 12 H 4 Z M 5 1 H 7 V 4 H 5 Z M 9 1 H 11 V 4 H 9 Z M 5 12 H 7
                                        V 15 H 5 Z M 9 12 H 11 V 15 H 9 Z M 1 5 H 4 V 7 H 1 Z M 1 9 H 4 V 11 H 1
                                        Z M 12 5 H 15 V 7 H 12 Z M 12 9 H 15 V 11 H 12 Z
                                    )
                                }
                                ActionGlyph::Firmware => {
                                    path!(
                                        M 3 2 H 6 V 4 H 10 V 2 H 13 V 14 H 3 Z M 1 4 H 3 V 6 H 1 Z M 1 8 H 3 V 10
                                        H 1 Z M 1 12 H 3 V 14 H 1 Z M 13 4 H 15 V 6 H 13 Z M 13 8 H 15 V 10 H 13
                                        Z M 13 12 H 15 V 14 H 13 Z
                                    )
                                }
                                ActionGlyph::Storage => {
                                    path!(
                                        M 2 1 H 12 L 15 4 V 15 H 1 V 1 Z M 4 2 V 6 H 11 V 2 Z M 4 9 V 14 H 12 V 9
                                        Z
                                    )
                                }
                                ActionGlyph::Play => {
                                    path!(M 3 2 H 6 V 4 H 10 V 7 H 14 V 10 H 10 V 13 H 6 V 15 H 3 Z)
                                }
                                ActionGlyph::Library => {
                                    path!(M 1 3 H 7 V 5 H 15 V 14 H 1 Z M 3 7 V 12 H 13 V 7 Z)
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
                                ActionGlyph::Restart => {
                                    path!(
                                        M 4 1 H 11 V 3 H 13 V 5 H 15 V 1 H 16 V 8 H 9 V 6 H 12 V 4 H 10 V 3 H 5 V
                                        4 H 3 V 11 H 5 V 13 H 11 V 11 H 13 V 9 H 15 V 12 H 13 V 14 H 11 V 15 H 4
                                        V 13 H 2 V 11 H 1 V 5 H 2 V 3 H 4 Z
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
                                ActionGlyph::Brightness => {
                                    path!(
                                        M 6 4 H 10 V 6 H 12 V 10 H 10 V 12 H 6 V 10 H 4 V 6 H 6 Z M 7 0 H 9 V 2 H
                                        7 Z M 7 14 H 9 V 16 H 7 Z M 0 7 H 2 V 9 H 0 Z M 14 7 H 16 V 9 H 14 Z M 2
                                        2 H 4 V 4 H 2 Z M 12 2 H 14 V 4 H 12 Z M 2 12 H 4 V 14 H 2 Z M 12 12 H 14
                                        V 14 H 12 Z
                                    )
                                }
                                ActionGlyph::Settings => {
                                    path!(
                                        M 5 1 H 11 V 3 H 13 V 5 H 15 V 11 H 13 V 13 H 11 V 15 H 5 V 13 H 3 V 11 H
                                        1 V 5 H 3 V 3 H 5 Z M 6 5 V 6 H 5 V 10 H 6 V 11 H 10 V 10 H 11 V 6 H 10 V
                                        5 Z
                                    )
                                }
                                ActionGlyph::Toolbox => {
                                    path!(
                                        M 5 1 H 11 V 5 H 15 V 15 H 1 V 5 H 5 Z M 7 3 V 5 H 9 V 3 Z M 2 8 V 10 H 7
                                        V 12 H 9 V 10 H 14 V 8 H 9 V 7 H 7 V 8 Z
                                    )
                                }
                                ActionGlyph::Info => {
                                    path!(
                                        M 3 0 H 13 V 2 H 15 V 14 H 13 V 16 H 3 V 14 H 1 V 2 H 3 Z M 7 3 V 5 H 9 V
                                        3 Z M 7 7 V 13 H 9 V 7 Z
                                    )
                                }
                                ActionGlyph::Wifi => {
                                    path!(
                                        M 1 2 H 15 V 4 H 1 Z M 3 6 H 13 V 8 H 3 Z M 5 10 H 11 V 12 H 5 Z M 7 14 H
                                        9 V 16 H 7 Z
                                    )
                                }
                                ActionGlyph::Network => {
                                    path!(
                                        M 5 0 H 11 V 5 H 9 V 8 H 14 V 11 H 16 V 16 H 10 V 11 H 12 V 10 H 4 V 11 H
                                        6 V 16 H 0 V 11 H 2 V 8 H 7 V 5 H 5 Z
                                    )
                                }
                                ActionGlyph::Muted => {
                                    path!(
                                        M 1 6 H 4 V 4 H 6 V 2 H 8 V 14 H 6 V 12 H 4 V 10 H 1 Z M 10 4 H 12 V 6 H
                                        10 Z M 12 6 H 14 V 10 H 12 Z M 14 4 H 16 V 6 H 14 Z M 10 10 H 12 V 12 H
                                        10 Z M 14 10 H 16 V 12 H 14 Z
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
fn overlay_label(state: model!(Launcher), index: Option<usize>) -> Entity {
    let text = state.clone();
    ui! {
        text_block (
            ${
                {
                    index
                        .map_or_else(
                            || {
                                if text.management().active() {
                                    text.management().title()
                                } else if text.saves().active() {
                                    text.saves().heading()
                                } else if text.configuration().active() {
                                    text.configuration().heading()
                                } else {
                                    text.overlay_title()
                                }
                            },
                            |index| {
                                text.item(index).map(|item| item.name.clone()).unwrap_or_default()
                            },
                        )
                }
            },
            ${ index.map_or(FOREGROUND, |index| action_foreground(&state, index)) },
            TITLE_SIZE,
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

#[compose]
fn configuration_or_saves_panel(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0) {
            if ${ state.management().active() } {
                management_panel (state.management(), metrics)
            } else {
                Column (grow: 1.0, min_width: 0, min_height: 0) {
                    if ${ state.saves().active() } {
                        saves_panel (state.saves(), metrics)
                    } else {
                        configuration_panel (state.configuration(), metrics)
                    }
                }
            }
        }
    }
}
#[compose]
fn configuration_or_saves_hints(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Row (grow: 1.0, min_width: 0, min_height: 0) {
            if ${ state.management().active() } {
                management_hints (state.management(), metrics)
            } else {
                Row (grow: 1.0, min_width: 0, min_height: 0) {
                    if ${ state.saves().active() } {
                        saves_hints (state.saves(), metrics)
                    } else {
                        configuration_hints (state.configuration(), metrics)
                    }
                }
            }
        }
    }
}
