use super::{
    deck::{ActionAnchor, ActionLayout},
    keyboard::keycap,
    style::*,
    widgets::{control_key, control_label, shadow, text_block},
};
use crate::launcher::Launcher;
use mirui::prelude::*;
use mirui::ui::widgets::TextAlign::{Center, Start};
use mirui::ui::widgets::{Image, Text, TextEditing};
use nearby_menu::wifi::Page;

#[component]
pub(super) struct LoadingTrack;

fn label(state: &crate::launcher::LauncherHandle, index: usize) -> String {
    match state.wifi_page() {
        Page::List => state
            .wifi_row(index)
            .map(|row| row.ssid)
            .unwrap_or_else(|| "添加网络".into()),
        Page::Off => "开启 Wi-Fi".into(),
        Page::Empty => if index == 0 { "刷新" } else { "添加网络" }.into(),
        Page::Security => if index == 0 {
            "密码网络"
        } else {
            "开放网络"
        }
        .into(),
        Page::Details => if index == 0 { "断开" } else { "忘记" }.into(),
        Page::Forget => if index == 0 { "取消" } else { "忘记" }.into(),
        Page::Failed => if index == 0 { "重试" } else { "返回" }.into(),
        Page::Success => "完成".into(),
        Page::Busy if state.wifi_retained() => "返回房间".into(),
        _ => "返回".into(),
    }
}
#[compose]
pub(super) fn panel(state: model!(Launcher), metrics: Metrics) -> Entity {
    let notice = state.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            if ${ !state.wifi_text(1).is_empty() } {
                text_block (${ notice.wifi_text(1) }, WARNING, BODY_SIZE, Start, 0, 2)
            }
            match ${ state.wifi_page() } {
                Page :: Password | Page :: Hidden => {
                    keyboard (state, metrics)
                }
                Page :: List => {
                    scan_label (state)
                    Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
                        walk ${ state.wifi_window() } with index by index {
                            row (state, index, true, metrics)
                        }
                    } on SizeChanged { state.wifi_height(event.new.height, metrics.wifi_row_extent(), metrics.gap); }
                }
                Page :: Connecting | Page :: Cancelling | Page :: Restoring => {
                    progress (state, metrics)
                }
                _ => {
                    options (state, metrics)
                }
            }
        }
    }
}
#[compose]
fn scan_label(state: model!(Launcher)) -> Entity {
    let status = state.clone();
    ui! {
        Row (height: 16, shrink: 0.0) {
            text_block (
                ${ if status.wifi_scanning() { "扫描中".into() } else { String::new() } },
                SECONDARY,
                BODY_SIZE,
                Start,
                0,
                1
            )
        }
    }
}
#[compose]
fn options(state: model!(Launcher), metrics: Metrics) -> Entity {
    let ssid = state.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            match ${ state.wifi_page() } {
                Page :: Info | Page :: Details | Page :: Success => {
                    facts (state, metrics)
                }
                Page :: Off => {
                    text_block (
                        "Wi-Fi 已关闭",
                        FOREGROUND,
                        TITLE_SIZE,
                        Start,
                        0,
                        2
                    )
                }
                Page :: Empty => {
                    text_block (
                        "还没发现网络",
                        FOREGROUND,
                        TITLE_SIZE,
                        Start,
                        0,
                        2
                    )
                }
                Page :: Busy => {
                    text_block (
                        "无线网络正在使用中",
                        FOREGROUND,
                        TITLE_SIZE,
                        Start,
                        0,
                        2
                    )
                }
                Page :: Forget => {
                    text_block (${ ssid.wifi_text(0) }, FOREGROUND, TITLE_SIZE, Start, 0, 2)
                    text_block (
                        "删除此网络保存的连接信息",
                        SECONDARY,
                        BODY_SIZE,
                        Start,
                        0,
                        2
                    )
                }
                _ => {
                }
            }
            View (grow: 1.0, min_height: 0)
            Row (
                height: metrics.wifi_row_height() + metrics.shadow_height(),
                shrink: 0.0,
                column_gap: metrics.gap,
                min_width: 0
            ) {
                row (state, 0, false, metrics)
                if ${
                    matches!(
                        state.wifi_page(), Page::Details | Page::Forget | Page::Security | Page::Empty |
                        Page::Failed
                    )
                } {
                    row (state, 1, false, metrics)
                }
            }
        }
    }
}

#[compose]
fn row(state: model!(Launcher), index: usize, list: bool, metrics: Metrics) -> Entity {
    let name = state.clone();
    let badge = state.clone();
    ui! {
        Column (
            grow: if list { 0.0 } else { 1.0 },
            height: metrics.wifi_row_height() + metrics.shadow_height(),
            shrink: 0.0,
            min_width: 0
        ) {
            Row (
                height: metrics.wifi_row_height(),
                min_width: 0,
                align: AlignItems::Center,
                padding: Padding::all(metrics.gap),
                column_gap: metrics.gap,
                bg_color: ${ if state.menu_selection() == Some(index) { ACCENT_DIM } else { PANEL } },
                border_width: 2,
                border_color: ${ if state.menu_selection() == Some(index) { ACCENT } else { DIVIDER } },
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
            ) [
                ActionAnchor(
                    index,
                    if list { ActionLayout::WifiList } else { ActionLayout::WifiOptions },
                ),
            ] {
                View (width: 3, height: 24, shrink: 0.0, bg_color: ACCENT)
                if list {
                    network_icon (state, index)
                }
                Text (
                    text: ${ label(&name, index) },
                    font: font(TITLE_SIZE),
                    font_size: TITLE_SIZE,
                    text_color: FOREGROUND,
                    grow: 1.0,
                    min_width: 0,
                    paragraph: super::widgets::paragraph(Start, 1)
                ) [
                    FONT_PAINT,
                ]
                if list {
                    Column (width: 36, shrink: 0.0, min_height: 12) {
                        text_block (
                            ${
                                {
                                    badge
                                        .wifi_row(index)
                                        .map(|r| {
                                            if r.connected {
                                                "已连接"
                                            } else if r.saved {
                                                "已保存"
                                            } else if r.security == nearby_menu::wifi::Security::Open {
                                                "开放"
                                            } else {
                                                ""
                                            }
                                                .to_owned()
                                        })
                                        .unwrap_or_default()
                                }
                            },
                            SECONDARY,
                            BODY_SIZE,
                            Start,
                            0,
                            1
                        )
                    }
                    if ${ state.wifi_row(index).is_some() } {
                        signal (state, index)
                    }
                }
            }
            shadow (metrics)
        }
    }
}
#[compose]
fn signal(state: model!(Launcher), index: usize) -> Entity {
    ui! {
        Row (width: 24, height: 20, align: AlignItems::FlexEnd, column_gap: 2, shrink: 0.0) {
            walk 0_usize..4 with bar {
                View (
                    width: 4,
                    height: (bar as i32 + 1) * 5,
                    bg_color: ${
                        if state.wifi_row(index).is_some_and(|r| usize::from(r.signal) > bar * 24) {
                            ACCENT
                        } else {
                            DIVIDER
                        }
                    },
                    transform: ${
                        if state.wifi_scanning() {
                            let scale = Fixed::from_ratio(
                                i32::from((state.wifi_pulse() + bar as u16) % 4) + 3,
                                6,
                            );
                            mirui::types::Transform::translate(
                                    Fixed::ZERO,
                                    Fixed::from_int((bar as i32 + 1) * 5) * (Fixed::ONE - scale),
                                )
                                .compose(&mirui::types::Transform::scale(Fixed::ONE, scale))
                        } else {
                            mirui::types::Transform::IDENTITY
                        }
                    }
                )
            }
        }
    }
}

#[compose]
fn network_icon(state: model!(Launcher), index: usize) -> Entity {
    ui! {
        Column (width: 22, height: 22, shrink: 0.0) {
            walk ${ std::iter::once(network_glyph(&state, index)) } with kind by kind {
                Image (
                    width: 22,
                    height: 22,
                    viewbox: 16,
                    color: SECONDARY,
                    src: match kind {
                        NetworkGlyph::Add => {
                            path!(M 6 1 H 10 V 6 H 15 V 10 H 10 V 15 H 6 V 10 H 1 V 6 H 6 Z)
                        }
                        NetworkGlyph::Connected => {
                            path!(
                                M 1 7 H 5 V 11 H 8 V 7 H 11 V 3 H 15 V 8 H 12 V 12 H 9 V 15 H 5 V 12 H 1
                                Z
                            )
                        }
                        NetworkGlyph::Open => {
                            path!(
                                M 4 1 H 12 V 4 H 15 V 12 H 12 V 15 H 4 V 12 H 1 V 4 H 4 Z M 5 4 V 7 H 11
                                V 4 Z M 5 10 V 12 H 11 V 10 Z
                            )
                        }
                        NetworkGlyph::Locked => {
                            path!(
                                M 4 7 V 3 H 6 V 1 H 10 V 3 H 12 V 7 H 14 V 15 H 2 V 7 Z M 6 7 H 10 V 4 H
                                6 Z M 7 10 V 13 H 9 V 10 Z
                            )
                        }
                    }
                )
            }
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum NetworkGlyph {
    Add,
    Connected,
    Open,
    Locked,
}
fn network_glyph(state: &crate::launcher::LauncherHandle, index: usize) -> NetworkGlyph {
    match state.wifi_row(index) {
        None => NetworkGlyph::Add,
        Some(row) if row.connected => NetworkGlyph::Connected,
        Some(row) if row.security == nearby_menu::wifi::Security::Open => NetworkGlyph::Open,
        _ => NetworkGlyph::Locked,
    }
}
#[compose]
fn keyboard(state: model!(Launcher), metrics: Metrics) -> Entity {
    let text = state.clone();
    let editor = state.wifi_document();
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
            Row (
                height: 36,
                shrink: 0.0,
                padding: Padding::all(4),
                bg_color: PANEL,
                border_width: 2,
                border_color: ACCENT,
                min_width: 0
            ) [
                crate::launcher::text_input::InputEditor,
                mirui::ui::widgets::TextEditing::new(editor),
            ] {
                Text (
                    text: ${ text.wifi_editor(35) },
                    font: font(TITLE_SIZE),
                    font_size: TITLE_SIZE,
                    text_color: FOREGROUND,
                    grow: 1.0,
                    min_width: 0,
                    paragraph: super::widgets::paragraph(Start, 1)
                ) [
                    FONT_PAINT,
                ]
                Text (
                    text: ${ state.wifi_length().to_string() },
                    font: font(BODY_SIZE),
                    font_size: BODY_SIZE,
                    text_color: SECONDARY,
                    width: 28,
                    shrink: 0.0
                ) [
                    FONT_PAINT,
                ]
            } on TextEditing::Submitted { state.wifi_submit(); }
            Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: 3) {
                walk 0_usize..4 with line {
                    Row (grow: 1.0, min_height: 24, min_width: 0, column_gap: 3) {
                        walk 0_usize..(if line == 3 { 5 } else { 10 }) with column {
                            key (
                                state,
                                if line == 3 { 30 + column } else { line * 10 + column },
                                metrics
                            )
                        }
                    }
                }
            }
        }
    }
}
#[compose]
fn key(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    let value = state.clone();
    let selected = state.clone();
    ui! {
        keycap (
            ${ value.wifi_editor(index) },
            ${ selected.menu_selection() == Some(index) },
            ${
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
            },
            ActionAnchor(index, ActionLayout::WifiKeyboard),
            if index >= 30 { BODY_SIZE } else { TITLE_SIZE },
            metrics
        )
    }
}
#[compose]
fn progress(state: model!(Launcher), metrics: Metrics) -> Entity {
    let start = state.clone();
    let end = state.clone();
    let status = state.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Column (
                grow: 1.0,
                min_height: 50,
                justify: JustifyContent::Center,
                row_gap: metrics.gap * 3,
                bg_color: PANEL,
                padding: Padding::all(metrics.gap * 2)
            ) {
                text_block (
                    ${
                        match status.wifi_page() {
                            Page::Cancelling => "正在取消连接".to_owned(),
                            Page::Restoring => "恢复原来的网络".into(),
                            _ => {
                                [
                                    "连接网络",
                                    "获取 IP 地址",
                                    "连接完成",
                                ][usize::from(status.wifi_stage())]
                                    .into()
                            }
                        }
                    },
                    FOREGROUND,
                    TITLE_SIZE,
                    Center,
                    0,
                    1
                )
                View (
                    height: 12,
                    min_width: 0,
                    bg_color: DIVIDER,
                    clip_children: false
                ) [
                    LoadingTrack,
                ] {
                    View (
                        position: Position::Absolute,
                        width: Dimension::percent(100),
                        height: 12,
                        bg_color: ACCENT,
                        transform: ${
                            mirui::types::Transform::translate(
                                    (start.wifi_loading_start()
                                        - (Fixed::ONE - (end.wifi_loading_end() - end.wifi_loading_start()))
                                            * Fixed::HALF) * start.wifi_track(),
                                    Fixed::ZERO,
                                )
                                .compose(
                                    &mirui::types::Transform::scale(
                                        end.wifi_loading_end() - end.wifi_loading_start(),
                                        Fixed::ONE,
                                    ),
                                )
                        }
                    ) {
                        View (
                            position: Position::Absolute,
                            left: Dimension::percent(100),
                            width: 0,
                            height: 0
                        ) [
                            ActionAnchor(0, ActionLayout::WifiProgress),
                        ]
                    }
                } on SizeChanged { state.wifi_track_width(event.new.width); }
            }
            Row (shrink: 0.0, min_height: i32::from(BODY_SIZE), column_gap: metrics.gap) {
                walk 0_usize..3 with stage {
                    Column (grow: 1.0, min_width: 0, min_height: i32::from(BODY_SIZE)) {
                        stage_label (state, stage)
                    }
                }
            }
        }
    }
}
#[compose]
fn facts(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Column (
            shrink: 0.0,
            min_width: 0,
            row_gap: 4,
            padding: Padding::all(metrics.gap),
            bg_color: PANEL
        ) {
            walk 0_usize..5 with index {
                fact (state, index, metrics)
                View (height: 1, shrink: 0.0, bg_color: DIVIDER)
            }
        }
    }
}

#[compose]
fn fact(state: model!(Launcher), index: usize, metrics: Metrics) -> Entity {
    let info = state.clone();
    ui! {
        Row (
            min_width: 0,
            min_height: i32::from(BODY_SIZE) + 4,
            shrink: 0.0,
            column_gap: metrics.gap
        ) {
            Column (width: 64, min_height: i32::from(BODY_SIZE), shrink: 0.0) {
                text_block (
                    ${ ["网络", "IP 地址", "网关", "信号", "安全"][index].to_owned() },
                    SECONDARY,
                    BODY_SIZE,
                    Start,
                    0,
                    1
                )
            }
            Column (grow: 1.0, min_width: 0, min_height: i32::from(BODY_SIZE)) {
                text_block (
                    ${
                        {
                            info.wifi_info()
                                .map(|value| match index {
                                    0 => value.ssid,
                                    1 => value.address,
                                    2 => value.gateway,
                                    3 => format!("{}%", value.signal),
                                    _ => value.security.label().into(),
                                })
                                .unwrap_or_else(|| {
                                    if index == 0 { "尚未连接".into() } else { String::new() }
                                })
                        }
                    },
                    FOREGROUND,
                    BODY_SIZE,
                    Start,
                    0,
                    2
                )
            }
        }
    }
}
#[compose]
fn stage_label(state: model!(Launcher), stage: usize) -> Entity {
    let current = state.clone();
    ui! {
        text_block (
            ${ ["连接网络", "获取地址", "已连接"][stage].to_owned() },
            ${ if usize::from(current.wifi_stage()) == stage { FOREGROUND } else { SECONDARY } },
            BODY_SIZE,
            Start,
            0,
            1
        )
    }
}
#[compose]
pub(super) fn heading(state: model!(Launcher), metrics: Metrics) -> Entity {
    ui! {
        Row (
            height: 30,
            shrink: 0.0,
            min_width: 0,
            column_gap: metrics.gap,
            align: AlignItems::Center
        ) {
            Image (
                src: path!(
                    M 1 2 H 15 V 4 H 1 Z M 3 6 H 13 V 8 H 3 Z M 5 10 H 11 V 12 H 5 Z M 7 14 H 9 V 16
                    H 7 Z
                ),
                viewbox: 16,
                width: 24,
                height: 24,
                color: ACCENT
            )
            Text (
                text: ${ state.view().overlay },
                font: font(TITLE_SIZE),
                font_size: TITLE_SIZE,
                text_color: FOREGROUND,
                grow: 1.0,
                min_width: 0,
                paragraph: super::widgets::paragraph(Start, 1)
            ) [
                FONT_PAINT,
            ]
        }
    }
}

#[compose]
pub(super) fn hints(state: model!(Launcher), metrics: Metrics) -> Entity {
    let label = state.clone();
    let cancel = state.clone();
    let echo = state.clone();
    let submit = state.clone();
    ui! {
        Row (min_width: 0, column_gap: metrics.gap, align: AlignItems::Center) {
            if ${ state.wifi_page().pending() } {
                control_key ("B", metrics, true)
                control_label (
                    ${
                        if cancel.wifi_page() == Page::Connecting { "取消" } else { "恢复中" }
                            .to_owned()
                    },
                    metrics
                )
            } else {
                Row (min_width: 0, column_gap: metrics.gap, align: AlignItems::Center) {
                    control_key ("A", metrics, true)
                    control_label (
                        ${
                            {
                                if label.wifi_keyboard() {
                                    "输入"
                                } else if label.wifi_page() == Page::List {
                                    "选择"
                                } else {
                                    "确认"
                                }
                                    .to_owned()
                            }
                        },
                        metrics
                    )
                    control_key ("B", metrics, false)
                    control_label ("返回", metrics)
                    match ${ state.wifi_page() } {
                        Page :: List => {
                            control_key ("X", metrics, false)
                            control_label ("刷新", metrics)
                            control_key ("Y", metrics, false)
                            control_label ("网络信息", metrics)
                        }
                        Page :: Password | Page :: Hidden => {
                            control_key ("X", metrics, false)
                            control_label ("删除", metrics)
                            control_key ("Y", metrics, false)
                            control_label (${ echo.wifi_echo() }, metrics)
                            control_key ("START", metrics, false)
                            control_label (
                                ${ if submit.wifi_page() == Page::Hidden { "下一步" } else { "连接" }.to_owned() },
                                metrics
                            )
                        }
                        _ => {
                        }
                    }
                }
            }
        }
    }
}
