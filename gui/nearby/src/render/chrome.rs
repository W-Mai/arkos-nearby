use super::state::PageKey;
use super::{
    Section,
    branding::{logo as brand_logo, wordmark as brand_wordmark},
    cache::GpuLayerCache,
    device_status::indicators,
    state::DisplayState,
    style::*,
    widgets::{control_key, control_label, paragraph},
};
use crate::model::Page;
use mirui::prelude::*;
use mirui::ui::widgets::TextAlign::{End, Start};
use mirui::ui::widgets::{Image, Text};

#[compose]
pub(super) fn header(view: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: metrics.header_height(),
            shrink: 0.0,
            padding: metrics.header_padding(),
            bg_color: ${ if state.embedded() { SHELL } else { BACKGROUND } }
        ) [
            Section::Header,
        ] {
            if ${ state.embedded() } {
                embedded_header (view, state, metrics)
            } else {
                product_header (view, state, metrics)
            }
        }
    }
}

#[compose]
fn product_header(view: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Row (
            width: Dimension::percent(100),
            grow: 1.0,
            min_width: 0,
            column_gap: metrics.gap + 4,
            align: AlignItems::Center
        ) {
            brand_logo (metrics.logo_size())
            brand_wordmark (HEADER_TEXT)
            header_context (state, metrics, false)
            if ${ state.network_connected() } {
                Image (
                    width: 20,
                    height: 20,
                    shrink: 0.0,
                    viewbox: 32,
                    color: HEADER_DETAIL,
                    src: path!(
                        M 12 5 H 4 V 27 H 12 V 23 H 8 V 9 H 12 Z M 20 5 H 28 V 27 H 20 V 23 H 24 V 9 H 20
                        Z M 10 14 H 22 V 18 H 10 Z
                    )
                )
            }
            header_badge (view, state, metrics, HEADER_DETAIL)
            if ${
                state.launcher().enabled() || matches!(view.page, Page::Lobby | Page::Mode)
                    || (view.page == Page::Choice && !view.network_only)
            } {
                indicators (HEADER_DETAIL)
            }
        }
    }
}

#[compose]
fn embedded_header(view: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Row (
            width: Dimension::percent(100),
            grow: 1.0,
            min_width: 0,
            column_gap: metrics.gap + 4,
            align: AlignItems::Center
        ) {
            Image (
                width: 24,
                height: 24,
                shrink: 0.0,
                viewbox: 32,
                color: ACCENT,
                src: if view.network_only {
                    path!(
                        M 12 5 H 4 V 27 H 12 V 23 H 8 V 9 H 12 Z M 20 5 H 28 V 27 H 20 V 23 H 24 V 9
                        H 20 Z M 10 14 H 22 V 18 H 10 Z
                    )
                } else if matches!(view.page, Page::Lobby | Page::Mode) {
                    path!(
                        M 5 3 H 13 V 13 H 5 Z M 3 17 H 15 V 29 H 3 Z M 20 3 H 28 V 13 H 20 Z M 18 17
                        H 30 V 29 H 18 Z
                    )
                } else {
                    path!(
                        M 5 9 H 27 V 13 H 30 V 27 H 20 V 22 H 12 V 27 H 2 V 13 H 5 Z M 7 14 H 10 V 17
                        H 13 V 20 H 10 V 23 H 7 V 20 H 4 V 17 H 7 Z M 22 14 H 25 V 17 H 22 Z M 25 19
                        H 28 V 22 H 25 Z
                    )
                }
            )
            header_context (state, metrics, true)
            header_badge (view, state, metrics, SHELL_TEXT)
        }
    }
}

#[compose]
fn header_context(state: model!(DisplayState), metrics: Metrics, embedded: bool) -> Entity {
    let size = if embedded { TITLE_SIZE } else { metrics.small };
    ui! {
        Text (
            text: ${ state.subtitle() },
            grow: 1.0,
            min_width: 0,
            height: Dimension::Content,
            font: font(size),
            font_size: size,
            text_color: if embedded { SHELL_TEXT } else { HEADER_DETAIL },
            paragraph: paragraph(if embedded { Start } else { End }, 1),
            clip_children: true
        ) [
            FONT_PAINT,
        ]
    }
}

#[compose]
fn header_badge(
    view: PageKey,
    state: model!(DisplayState),
    metrics: Metrics,
    color: ColorToken,
) -> Entity {
    ui! {
        Text (
            text: ${
                if matches!(view.page, Page::Host | Page::Joined) {
                    state.room_name()
                } else if view.page == Page::Games {
                    state
                        .library_counter()
                        .map(|(position, total)| format!("{position} / {total}"))
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            },
            width: Dimension::Content,
            height: Dimension::Content,
            shrink: 0.0,
            font: font(metrics.small),
            font_size: metrics.small,
            text_color: color,
            paragraph: paragraph(End, 1),
            clip_children: true
        ) [
            FONT_PAINT,
        ]
    }
}

#[compose]
pub(super) fn footer(view: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: metrics.footer_height() + metrics.control_gap() + 1,
            shrink: 0.0,
            row_gap: metrics.control_gap()
        ) [
            Section::Footer,
            GpuLayerCache,
        ] {
            View (height: 1, shrink: 0.0, bg_color: DIVIDER)
            footer_controls (view, state, metrics)
        }
    }
}

#[compose]
fn footer_controls(view: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let confirm = state.clone();
    let back = state.clone();
    ui! {
        Row (
            height: metrics.footer_height(),
            shrink: 0.0,
            column_gap: metrics.control_gap(),
            align: AlignItems::Center
        ) {
            if ${
                state.confirm_visible()
                    && !(view.page == Page::Host && !view.network_only && state.enabled())
            } {
                control_key ("A", metrics, true)
                control_label (${ confirm.confirm() }, metrics)
            }
            if ${ !state.launcher().enabled() || !state.busy() } {
                control_key ("B", metrics, false)
                control_label (
                    ${
                        if state.launcher().enabled() && state.launcher().tab() == 0 {
                            "首页".into()
                        } else {
                            back.back()
                        }
                    },
                    metrics
                )
            }
            if matches!(view.page, Page::Rooms | Page::Lobby) {
                control_key ("X", metrics, false)
                control_label ("刷新", metrics)
            }
            if ${ state.can_switch_theme() } {
                control_key ("Y", metrics, false)
                control_label ("主题", metrics)
            }
            Text (
                text: ${ if state.launcher().enabled() && state.busy() { String::new() } else { state.hint() } },
                grow: 1.0,
                min_width: 0,
                height: Dimension::Content,
                font: font(metrics.small),
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(End, 1),
                clip_children: true
            ) [
                FONT_PAINT,
            ]
            if ${ state.launcher().enabled() && !state.busy() } {
                control_key ("SELECT", metrics, false)
                control_label ("联机", metrics)
            }
            if ${
                state.launcher().enabled() && !state.busy()
                    && state.library_kind() != crate::model::LibraryKind::Search
            } {
                control_key ("START", metrics, false)
                control_label ("设置", metrics)
            }
        }
    }
}
