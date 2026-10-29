use super::artwork::preview;
use super::choices::choice;
use super::consoles::console;
use super::pixel::Stage::Search;
use super::selection::FocusTarget;
use super::{
    Section,
    pixel::{self, Scenery, art},
    rows::Row,
    state::DisplayState,
    style::*,
    widgets::{caption, label, paragraph, shadow},
};
use mirui::prelude::*;
use mirui::ui::widgets::TextAlign::{Center, Start};
use mirui::ui::widgets::{Image, Text};

// Sheet contents keep their layout coordinates while the viewports move and clip.
const DECK_PEEK: Fixed = Fixed::from_ratio(14, 100);
const DECK_FULL: Fixed = Fixed::from_ratio(86, 100);
const DECK_MIN_SCALE: Fixed = Fixed::from_ratio(44, 100);

fn sheet_scale(expansion: Fixed) -> Fixed {
    DECK_MIN_SCALE + (Fixed::ONE - DECK_MIN_SCALE) * expansion
}

fn sheet_transform(expansion: Fixed, height: Fixed) -> mirui::types::Transform {
    let scale = sheet_scale(expansion);
    // The widget transform pivots around its center; keep its painted top at zero.
    mirui::types::Transform::translate(Fixed::ZERO, (scale - Fixed::ONE) * height * DECK_FULL / 2)
        .compose(&mirui::types::Transform::scale(scale, scale))
}

fn sheet_share(expansion: Fixed) -> Fixed {
    DECK_PEEK + (DECK_FULL - DECK_PEEK) * expansion
}

#[compose]
pub(super) fn lobby(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (id: "discovery_deck", grow: 1.0, min_height: 0, container: true) on SizeChanged {
            state.set_deck_height(event.new.height);
        } {
            walk [false, true].into_iter() with actions {
                Column (
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    top: 0,
                    height: ${ Dimension::Percent(sheet_share(if actions { state.deck() } else { Fixed::ONE - state.deck() }) * Fixed::from_int(100)) },
                    transform: ${ mirui::types::Transform::translate(Fixed::ZERO, if actions { state.deck_height() * sheet_share(Fixed::ONE - state.deck()) } else { Fixed::ZERO }) },
                    shrink: 0.0,
                    container: true,
                    min_height: 0,
                    clip_children: true
                ) {
                    Column (
                        position: Position::Absolute,
                        width: Dimension::percent(100),
                        height: @(id("discovery_deck").height as deck_height) { deck_height * DECK_FULL },
                        top: 0,
                        transform: ${ sheet_transform(if actions { state.deck() } else { Fixed::ONE - state.deck() }, state.deck_height()) },
                        shrink: 0.0,
                        row_gap: metrics.gap,
                        padding: Padding::all(metrics.gap),
                    ) [
                        super::cache::SettledLayerCache { state: state.clone(), backdrop: false },
                        super::deck::ActorScope::Pane(if actions { crate::model::Page::Mode } else { crate::model::Page::Lobby }),
                    ] {
                        if actions {
                            label ("仅组网 / 选择游戏", metrics.body, SECONDARY, Start)
                            choice (super::state::PageKey { page: crate::model::Page::Mode, network_only: false, library: crate::model::LibraryKind::Directory }, state, metrics)
                        } else {
                            label ("附近正在玩的伙伴", metrics.title, FOREGROUND, Start)
                            discovery_status (state, metrics)
                            rooms (state, metrics)
                        }
                    }
                }
            }
        }
    }
}

#[compose]
fn row_signal(state: model!(DisplayState), row: model!(Row), metrics: Metrics) -> Entity {
    ui! {
        View (width: 30, height: if metrics.compact { 26 } else { 30 }, shrink: 0.0) {
            View (grow: 1.0, width: Dimension::percent(100)) [
                pixel::SignalSprite {
                    state: state.clone(),
                    row: row.clone(),
                },
            ]
        }
    }
}

#[compose]
pub(super) fn rooms(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0) [
            Scenery::Ridge,
        ] {
            room_rows (state, metrics)
        }
    }
}

#[compose]
pub(super) fn room_rows(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) [
            Section::FocusList,
        ] {
            if ${ state.rooms().is_empty() } {
                Column (grow: 1.0, min_height: 0) {
                    Column (
                        grow: 1.0,
                        bg_color: PANEL,
                        padding: Padding::all(metrics.inset),
                        justify: JustifyContent::Center,
                        row_gap: metrics.gap
                    ) {
                        art (state, Search, if metrics.compact { 48 } else { 88 })
                        label (
                            "正在搜索附近的房间",
                            metrics.title,
                            FOREGROUND,
                            Center
                        )
                        Text (
                            "请好友先创建房间，并把两台掌机放近一些",
                            width: Dimension::percent(100),
                            height: Dimension::Content,
                            font: font(metrics.small),
                            font_size: metrics.small,
                            text_color: SECONDARY,
                            paragraph: paragraph(Center, 2)
                        ) [
                            FONT_PAINT,
                        ]
                    }
                    shadow (metrics)
                }
            } else {
                Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
                    walk ${ state.rooms().entries() } with item by item.0 {
                        list_row (state, item.1, metrics, true)
                    }
                }
            }
        }
    }
}

#[compose]
pub(super) fn games(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap) [
            Scenery::Ridge,
        ] {
            Column (grow: 46.0, min_width: 0, min_height: 0) {
                game_rows (state, metrics)
            }
            Column (
                grow: 54.0,
                min_width: 0,
                min_height: 0,
                row_gap: metrics.gap
            ) {
                if ${ state.platforms() } {
                    Column (grow: 1.0, min_height: 0, bg_color: PANEL, container: true) {
                        walk ${ state.entries().entries() } with item by item.0 {
                            platform_preview_layer (state, item.1)
                        }
                    }
                } else {
                    preview (state, metrics)
                }
                preview_label (state, metrics)
            }
        }
    }
}

#[compose]
fn platform_preview_layer(state: model!(DisplayState), row: model!(Row)) -> Entity {
    let visible = row.clone();
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            visible: ${ visible.index() == state.library_selection() }
        ) {
            walk ${ std::iter::once(row.platform()) } with platform by platform {
                console (platform)
            }
        }
    }
}

#[compose]
fn preview_label(state: model!(DisplayState), metrics: Metrics) -> Entity {
    let title = move || {
        let selected = state.library_selection();
        state
            .entries()
            .entries()
            .into_iter()
            .find(|(_, row)| row.index() == selected)
            .map(|(_, row)| row.label(true))
            .unwrap_or_default()
    };
    ui! {
        Text (
            text: ${ title() },
            width: Dimension::percent(100),
            height: metrics.preview_title_height(),
            min_width: 0,
            shrink: 0.0,
            font: font(metrics.title),
            font_size: metrics.title,
            text_color: FOREGROUND,
            bg_color: BACKGROUND,
            paragraph: paragraph(Center, 2)
        ) [
            FONT_PAINT,
        ]
    }
}

#[compose]
pub(super) fn game_rows(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) [
            Section::FocusList,
        ] {
            if ${ state.entries().is_empty() } {
                Column (grow: 1.0, min_height: 0) {
                    Column (
                        grow: 1.0,
                        bg_color: PANEL,
                        justify: JustifyContent::Center,
                        padding: Padding::all(metrics.inset)
                    ) {
                        label (
                            "没有可选游戏",
                            metrics.title,
                            FOREGROUND,
                            Center
                        )
                    }
                    shadow (metrics)
                }
            } else {
                Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
                    walk ${ state.entries().entries() } with item by item.0 {
                        list_row (state, item.1, metrics, false)
                    }
                }
            }
        }
    }
}

#[compose]
fn row_label(row: model!(Row), size: u16, title: bool, lines: u16) -> Entity {
    let content = row.clone();
    let colors = row;
    ui! {
        super::widgets::text_block (
            move || content.label(title),
            move || {
                if !colors.available() {
                    DISABLED
                } else if title {
                    FOREGROUND
                } else {
                    SECONDARY
                }
            },
            size,
            Start,
            ROW_LEADING,
            lines
        )
    }
}

#[compose]
fn platform_icon(row: model!(Row)) -> Entity {
    ui! {
        View (width: 24, height: 24, shrink: 0.0) [
            super::cache::GpuLayerCache,
        ] {
            walk ${ std::iter::once(row.platform()) } with platform by platform {
                console (platform)
            }
        }
    }
}

#[compose]
fn list_row(
    state: model!(DisplayState),
    item: model!(Row),
    metrics: Metrics,
    has_signal: bool,
) -> Entity {
    let color_index = item.clone();
    let transform_index = item.clone();
    let accent_index = item.clone();
    let available = item.clone();
    let marker = item.clone();
    let favorite = item.clone();
    ui! {
        Column (
            grow: 1.0,
            min_height: metrics.row_min_height(),
            max_height: if has_signal { metrics.row_height() } else { metrics.library_row_height() },
            min_width: 0,
            transform: ${ state.card_motion().card_transform(transform_index.index()) }
        ) {
            Row (
                grow: 1.0,
                min_height: 0,
                padding: metrics.row_padding(),
                column_gap: metrics.gap,
                bg_color: ${ state.card_motion().card_color(color_index.index()) },
                align: AlignItems::Center
            ) [
                if has_signal {
                    FocusTarget::row(item.clone()).scoped(crate::model::Page::Rooms)
                } else {
                    FocusTarget::row(item.clone())
                },
            ] {
                View (
                    width: metrics.rail_width(),
                    height: Dimension::percent(100),
                    shrink: 0.0,
                    bg_color: ${
                        if available.available() && state.selected() == Some(accent_index.index()) {
                            ACCENT
                        } else {
                            DIVIDER
                        }
                    }
                )
                if !has_signal {
                    platform_icon (marker)
                }
                Column (grow: 1.0, min_width: 0, justify: JustifyContent::Center) {
                    row_label (
                        item.clone(),
                        metrics.row_title,
                        true,
                        if has_signal { 1 } else { 2 }
                    )
                    if ${
                        has_signal || state.platforms() || item.directory() || !item.available()
                            || state.page_key().page == crate::model::Page::Candidates
                            || state.page_key().library == crate::model::LibraryKind::Search
                    } {
                        row_label (item.clone(), metrics.row_detail, false, 1)
                    }
                }
                if ${ !has_signal && favorite.favorite() } {
                    Image (
                        src: path!(
                            M 8 0 H 12 V 4 H 20 V 8 H 16 V 12 H 18 V 20 H 12 V 16 H 8 V 20 H 2 V 12 H 4 V 8 H
                            0 V 4 H 8 Z
                        ),
                        viewbox: 20,
                        width: 12,
                        height: 12,
                        shrink: 0.0,
                        color: WARNING
                    )
                }
                if has_signal {
                    row_signal (state, item, metrics)
                }
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn discovery_status(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        caption (move || state.status(), || SECONDARY, metrics.small, Start, LABEL_LEADING)
    }
}
