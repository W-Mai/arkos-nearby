use super::motion::anchors::SEAT_WIDTH_PERCENT;
use super::pixel::Stage::Session;
use super::selection::FocusTarget;
use super::{
    Section, StaticCache,
    pixel::{self, art},
    state::{DisplayState, TextField},
    style::*,
    widgets::{caption, control_key, guidance, label, live_label, paragraph, shadow, text_block},
};
use TextField::Headline;
use mirui::prelude::*;
use mirui::ui::OffscreenRender;
use mirui::ui::widgets::Text;
use mirui::ui::widgets::TextAlign::{Center, End, Start};

#[compose]
pub(super) fn game_room(state: model!(DisplayState), metrics: Metrics, host: bool) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) {
            Column (grow: 1.0, min_height: 0) {
                Column (
                    grow: 1.0,
                    min_height: 0,
                    padding: Padding::all(metrics.inset),
                    bg_color: PANEL,
                    row_gap: metrics.gap
                ) {
                    player_cards (state, metrics, host)
                }
                shadow (metrics)
            }
            Column (
                height: metrics.action_height(),
                shrink: 0.0,
                transform: ${ state.card_motion().card_transform(0) }
            ) {
                if host {
                    Column (
                        grow: 1.0,
                        bg_color: ${ if state.enabled() { ACCENT } else { PANEL } },
                        border_color: ACCENT,
                        border_width: ${ if state.enabled() { 2 } else { 0 } },
                        justify: JustifyContent::Center
                    ) [
                        Section::FocusHost,
                        FocusTarget::fixed(0),
                    ] {
                        Row (
                            width: Dimension::percent(100),
                            align: AlignItems::Center,
                            justify: JustifyContent::Center,
                            column_gap: metrics.gap
                        ) {
                            if ${ state.enabled() } {
                                control_key ("A", metrics, false)
                            }
                            Text (
                                text: ${ state.host_button() },
                                width: Dimension::Content,
                                min_width: 0,
                                font: font(TITLE_SIZE),
                                font_size: TITLE_SIZE,
                                text_color: ${ if state.enabled() { ON_ACCENT } else { FOREGROUND } },
                                paragraph: paragraph(Center, 1)
                            ) [
                                FONT_PAINT,
                            ]
                        }
                    }
                } else {
                    Column (grow: 1.0, bg_color: PANEL, justify: JustifyContent::Center) {
                        live_label (state, Headline, TITLE_SIZE, FOREGROUND, Center)
                    }
                }
                shadow (metrics)
            }
        }
    }
}

#[compose]
fn player_cards(state: model!(DisplayState), metrics: Metrics, host: bool) -> Entity {
    let plate_height = 2 * (metrics.gap + 2)
        + i32::from(BODY_SIZE)
        + LABEL_LEADING
        + 2
        + i32::from(TITLE_SIZE) * 2
        + LABEL_LEADING;
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            View (grow: 1.0, min_height: 0, min_width: 0, container: true, clip_children: true) [
                pixel::Scenery::Landscape,
                pixel::SessionSprite {
                    state: state.clone(),
                },
            ]
            Row (
                height: plate_height,
                shrink: 0.0,
                min_width: 0
            ) {
                player_plate (state, metrics, false, host)
                player_plate (state, metrics, true, false)
            }
        }
    }
}

#[compose]
fn player_plate(
    state: model!(DisplayState),
    metrics: Metrics,
    guest: bool,
    local_host: bool,
) -> Entity {
    let inset = metrics.gap + 2;
    ui! {
        Column (
            width: Dimension::percent(SEAT_WIDTH_PERCENT),
            height: Dimension::percent(100),
            padding: Padding {
                left: (metrics.gap / 2).into(),
                right: (metrics.gap / 2).into(),
                ..Default::default()
            }
        ) {
            Column (
                grow: 1.0,
                min_width: 0,
                padding: Padding::all(inset),
                row_gap: 2,
                justify: JustifyContent::Center,
                container: true,
                clip_children: true,
                bg_color: BACKGROUND
            ) {
                Row (
                    height: i32::from(BODY_SIZE) + LABEL_LEADING,
                    align: AlignItems::Center,
                    column_gap: inset,
                    justify: JustifyContent::Center
                ) {
                    seat_rule (state, guest, local_host)
                    Text (
                        if guest { "2P 玩家" } else { "1P 房主" },
                        width: Dimension::Content,
                        font: font(BODY_SIZE),
                        font_size: BODY_SIZE,
                        text_color: SECONDARY,
                        paragraph: paragraph(Center, 1)
                    ) [
                        FONT_PAINT,
                    ]
                    seat_rule (state, guest, local_host)
                }
                text_block (
                    ${ state.player(usize::from(guest)) },
                    FOREGROUND,
                    TITLE_SIZE,
                    Center,
                    LABEL_LEADING,
                    2
                )
            }
        }
    }
}

fn arrival_mark(age: u16, guest: bool) -> i32 {
    // The second seat answers the first. Only the ink inside the fixed slot expands.
    let stagger = if guest { 72 } else { 0 };
    let t = Fixed::from_ratio(i32::from(age.saturating_sub(stagger).min(300)), 300);
    let pulse = Fixed::from_int(4) * t * (Fixed::ONE - t);
    12 + (Fixed::from_int(8) * pulse).to_int()
}

#[compose]
fn seat_rule(state: model!(DisplayState), guest: bool, local_host: bool) -> Entity {
    ui! {
        Row (width: 20, height: 12, align: AlignItems::Center, justify: JustifyContent::Center) {
            View (
                width: ${ arrival_mark(state.session_motion().phase_age, guest) },
                height: 2,
                bg_color: ${ if local_host && state.enabled() { crate::theme::actor::HOST } else { DIVIDER } }
            )
        }
    }
}

#[compose]
pub(super) fn network_panel(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Column (grow: 1.0, min_height: 0) {
                Column (
                    grow: 1.0,
                    min_height: 0,
                    padding: Padding::all(metrics.gap),
                    bg_color: PANEL,
                    justify: JustifyContent::Center,
                    row_gap: metrics.gap
                ) {
                    Row (
                        height: i32::from(TITLE_SIZE) + LABEL_LEADING,
                        shrink: 0.0,
                        align: AlignItems::Center,
                        column_gap: metrics.gap
                    ) {
                        Column (grow: 1.0, min_width: 0) {
                            live_label (state, Headline, TITLE_SIZE, FOREGROUND, Start)
                        }
                    }
                    View (
                        height: if metrics.compact { 48 } else { 84 },
                        min_height: i32::from(metrics.small) + 4,
                        shrink: 1.0,
                        grow: 1.0,
                        width: Dimension::percent(100),
                        clip_children: true
                    ) [
                        pixel::SessionSprite {
                            state: state.clone(),
                        },
                    ]
                    network_devices (state, metrics)
                    guidance (state, metrics)
                }
                shadow (metrics)
            }
            Row (height: metrics.action_height(), shrink: 0.0, column_gap: metrics.gap) {
                network_action (state, metrics, 0)
                if ${ state.manages_network() } {
                    network_action (state, metrics, 1)
                }
            }
        }
    }
}

#[compose]
fn network_action(state: model!(DisplayState), metrics: Metrics, index: usize) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            transform: ${ state.card_motion().card_transform(index) }
        ) {
            Column (
                grow: 1.0,
                min_width: 0,
                justify: JustifyContent::Center,
                bg_color: ${ if state.selected() == Some(index) { ACCENT } else { PANEL } }
            ) [
                FocusTarget::fixed(index),
            ] {
                caption (
                    ${
                        if index == 0 {
                            state.host_button()
                        } else if state.confirming_disconnect() {
                            "确认断开".into()
                        } else {
                            "断开组网".into()
                        }
                    },
                    ${ if state.selected() == Some(index) { ON_ACCENT } else { FOREGROUND } },
                    TITLE_SIZE,
                    Center,
                    LABEL_LEADING
                )
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn network_devices(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            height: ${
                if state.network_connected() {
                    metrics.device_list_height(state.devices().len())
                } else {
                    0
                }
            },
            visible: ${ state.network_connected() },
            shrink: 0.0,
            justify: JustifyContent::Center,
            row_gap: metrics.device_row_gap()
        ) {
            walk ${ state.devices() } with device {
                Row (
                    height: metrics.device_row_height(),
                    shrink: 1.0,
                    min_height: i32::from(metrics.body) + LABEL_LEADING,
                    bg_color: BACKGROUND,
                    align: AlignItems::Center,
                    column_gap: metrics.gap
                ) {
                    View (width: metrics.rail_width(), height: 12, shrink: 0.0, bg_color: ACCENT)
                    Text (
                        device.name.clone(),
                        grow: 1.0,
                        min_width: 0,
                        height: 18,
                        font: font(metrics.body),
                        font_size: metrics.body,
                        text_color: FOREGROUND,
                        paragraph: paragraph(Start, 1),
                        clip_children: true
                    ) [
                        FONT_PAINT,
                    ]
                    Text (
                        device.address.clone(),
                        width: Dimension::Content,
                        shrink: 0.0,
                        height: 18,
                        font: font(metrics.body),
                        font_size: metrics.body,
                        text_color: SECONDARY,
                        paragraph: paragraph(End, 1)
                    ) [
                        FONT_PAINT,
                    ]
                }
            }
        }
    }
}

#[compose]
pub(super) fn leaving(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0) {
            Column (
                grow: 1.0,
                min_height: 0,
                bg_color: PANEL,
                padding: Padding::all(metrics.inset),
                row_gap: metrics.gap,
                justify: JustifyContent::Center,
                align: AlignItems::Center
            ) {
                art (state, Session, if metrics.compact { 56 } else { 120 })
                live_label (
                    state,
                    Headline,
                    TITLE_SIZE,
                    FOREGROUND,
                    Center
                )
                guidance (state, metrics)
            }
            shadow (metrics)
        }
    }
}

#[compose]
pub(super) fn unavailable(metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0) {
            Column (
                grow: 1.0,
                min_height: 0,
                bg_color: PANEL,
                padding: Padding::all(metrics.inset),
                row_gap: metrics.gap,
                justify: JustifyContent::Center,
                align: AlignItems::Center
            ) [
                StaticCache,
                OffscreenRender::default(),
            ] {
                Text (
                    "!",
                    width: 44,
                    height: 40,
                    bg_color: WARNING_PANEL,
                    font: font(TITLE_SIZE),
                    font_size: TITLE_SIZE,
                    text_color: WARNING,
                    paragraph: paragraph(Center, 1)
                ) [
                    FONT_PAINT,
                ]
                label (
                    "暂时无法进行联机",
                    TITLE_SIZE,
                    FOREGROUND,
                    Center
                )
                label (
                    "按 B 返回，仍然可以单机游玩",
                    metrics.small,
                    ACCENT,
                    Center
                )
            }
            shadow (metrics)
        }
    }
}
