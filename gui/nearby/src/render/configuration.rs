use super::{
    deck::{ActionAnchor, ActionLayout},
    style::*,
    widgets::{control_key, control_label, shadow, text_block},
};
use crate::launcher::configuration::Editor;
use mirui::input::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::prelude::*;
use mirui::ui::widgets::{Image, Text, TextAlign::Start};
use nearby_menu::configuration::Section;

#[component]
pub(super) struct ConfigScroll {
    state: model!(Editor),
}
#[system(order=Animation)]
fn scroll_config(nodes: query!(Entity, ConfigScroll, ScrollOffset), commands: commands!()) {
    for (entity, node, offset) in nodes {
        let y = node.state.offset();
        if node.state.active() && offset.y != y {
            commands.insert(entity, ScrollOffset { x: Fixed::ZERO, y });
        }
    }
}
#[view(component=ConfigScroll,systems=[scroll_config::system()])]
pub(super) fn config_scroll() {}

#[compose]
pub(super) fn panel(state: model!(Editor), metrics: Metrics) -> Entity {
    let identity = state.clone();
    let scope = state.clone();
    let status = state.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            Row (height: 14, min_width: 0, column_gap: metrics.gap, shrink: 0.0) {
                Text (
                    text: ${ identity.identity() },
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
                    text: ${ scope.scope().label().to_owned() },
                    shrink: 0.0,
                    font: font(BODY_SIZE),
                    font_size: BODY_SIZE,
                    text_color: ACCENT
                ) [
                    FONT_PAINT,
                ]
            }
            if ${ !state.status().is_empty() } {
                text_block (move || status.status(), || WARNING, BODY_SIZE, Start, 0, 2)
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
                ConfigScroll {
                    state: state.clone(),
                },
            ] {
                walk ${ state.window() } with index by index {
                    row (state, index, metrics)
                }
                if ${ state.section() == Section::Core && state.ready() && state.count() == 0 } {
                    text_block (
                        || "此核心未提供可编辑选项".into(),
                        || SECONDARY,
                        TITLE_SIZE,
                        Start,
                        0,
                        3
                    )
                }
            } on SizeChanged { state.viewport(event.new.height, metrics.gap, metrics.shadow_height()); }
        }
    }
}

#[compose]
fn row(state: model!(Editor), index: usize, metrics: Metrics) -> Entity {
    let label = state.clone();
    let value = state.clone();
    ui! {
        Column (
            height: ${
                state.extent(index, metrics.gap)
                    + Fixed::from_int(metrics.shadow_height() + metrics.gap)
            },
            min_width: 0,
            shrink: 0.0,
            padding: Padding {
                bottom: Dimension::px(metrics.gap),
                ..Default::default()
            }
        ) {
            Row (
                height: ${ state.extent(index, metrics.gap) },
                min_height: metrics.wifi_row_height(),
                min_width: 0,
                padding: Padding::all(metrics.gap),
                column_gap: metrics.gap,
                align: AlignItems::Center,
                bg_color: ${ if state.selected() == index { ACCENT_DIM } else { PANEL } },
                border_width: 2,
                border_color: ${ if state.selected() == index { ACCENT } else { DIVIDER } },
                transform: ${
                    mirui::types::Transform::translate(
                        Fixed::ZERO,
                        Fixed::from_int(
                            if state.selected() == index {
                                super::motion::hop(state.age(), 2.0)
                            } else {
                                0
                            },
                        ),
                    )
                }
            ) [
                ActionAnchor(index, ActionLayout::Configuration),
            ] {
                View (width: 3, height: 24, shrink: 0.0, bg_color: ACCENT)
                if ${ state.section() == Section::Home } {
                    Image (
                        src: match index {
                            0 => {
                                path!(
                                    M 1 2 H 15 V 12 H 9 V 14 H 12 V 16 H 4 V 14 H 7 V 12 H 1 Z M 3 4 V 10 H
                                    13 V 4 Z
                                )
                            }
                            1 => {
                                path!(
                                    M 4 4 H 12 V 12 H 4 Z M 5 1 H 7 V 4 H 5 Z M 9 1 H 11 V 4 H 9 Z M 5 12 H 7
                                    V 15 H 5 Z M 9 12 H 11 V 15 H 9 Z M 1 5 H 4 V 7 H 1 Z M 1 9 H 4 V 11 H 1
                                    Z M 12 5 H 15 V 7 H 12 Z M 12 9 H 15 V 11 H 12 Z
                                )
                            }
                            2 => {
                                path!(
                                    M 2 4 H 14 V 6 H 16 V 13 H 12 V 11 H 4 V 13 H 0 V 6 H 2 Z M 3 6 V 8 H 1 V
                                    10 H 3 V 12 H 5 V 10 H 7 V 8 H 5 V 6 Z M 11 6 V 8 H 13 V 6 Z M 13 9 V 11
                                    H 15 V 9 Z
                                )
                            }
                            _ => {
                                path!(
                                    M 2 1 H 12 L 15 4 V 15 H 1 V 1 Z M 4 2 V 6 H 11 V 2 Z M 4 9 V 14 H 12 V 9
                                    Z
                                )
                            }
                        },
                        viewbox: 16,
                        width: 24,
                        height: 24,
                        shrink: 0.0,
                        color: ACCENT
                    )
                }
                Column (
                    height: ${ state.extent(index, metrics.gap) - Fixed::from_int(metrics.gap * 2 + 4) },
                    grow: 1.0,
                    min_width: 0,
                    row_gap: 4
                ) {
                    Text (
                        text: ${ label.row(index).label },
                        width: Dimension::percent(100),
                        height: Dimension::Content,
                        min_height: 26,
                        min_width: 0,
                        font: font(TITLE_SIZE),
                        font_size: TITLE_SIZE,
                        text_color: FOREGROUND,
                        paragraph: super::widgets::paragraph(Start, 2)
                    ) [
                        FONT_PAINT,
                    ] on SizeChanged { state.text_size(index, 0, event.new.height); }
                    if ${ !state.row(index).value.is_empty() } {
                        Text (
                            text: ${ value.row(index).value },
                            width: Dimension::percent(100),
                            height: Dimension::Content,
                            min_width: 0,
                            min_height: 14,
                            font: font(BODY_SIZE),
                            font_size: BODY_SIZE,
                            text_color: SECONDARY,
                            paragraph: mirui::ui::widgets::ParagraphStyle {
                                wrap: mirui::ui::widgets::TextWrap::Grapheme,
                                max_lines: None,
                                ..Default::default()
                            }
                        ) [
                            FONT_PAINT,
                        ] on SizeChanged { state.text_size(index, 1, event.new.height); }
                    }
                }
                if ${ state.row(index).changed } {
                    View (width: 6, height: 6, bg_color: WARNING, shrink: 0.0)
                }
                if ${ state.row(index).editable && state.section() != Section::Sources } {
                    Text (
                        text: ${
                            if matches!(state.section(), Section::Home | Section::Confirm | Section::Conflict) {
                                ">"
                            } else {
                                "< >"
                            }
                                .to_owned()
                        },
                        font: font(BODY_SIZE),
                        font_size: BODY_SIZE,
                        text_color: ACCENT,
                        shrink: 0.0
                    ) [
                        FONT_PAINT,
                    ]
                }
            }
            shadow (metrics)
        } on SizeChanged { state.measure(index, event.new.height); }
    }
}

#[compose]
pub(super) fn hints(state: model!(Editor), metrics: Metrics) -> Entity {
    let confirm = state.clone();
    ui! {
        Row (grow: 1.0, min_width: 0, column_gap: metrics.gap, align: AlignItems::Center) {
            if ${ state.confirmable() } {
                control_key ("A", metrics, true)
                control_label (move || confirm.confirm_label(), metrics)
            }
            control_key ("B", metrics, false)
            control_label (|| "返回".into(), metrics)
            View (grow: 1.0, min_width: 0)
            if ${
                (state.dirty() || !state.ready() && !state.loading())
                    && !matches!(state.section(), Section::Confirm | Section::Conflict)
            } {
                control_key ("X", metrics, false)
                retry_label (state, metrics)
            }
            if ${
                state.ready()
                    && !matches!(
                        state.section(), Section::Sources | Section::Confirm | Section::Conflict
                    )
            } {
                control_key ("Y", metrics, false)
                control_label (|| "范围".into(), metrics)
            }
        }
    }
}

#[compose]
fn retry_label(state: model!(Editor), metrics: Metrics) -> Entity {
    ui! {
        control_label (move || if state.ready() { "保存" } else { "重试" }.into(), metrics)
    }
}
