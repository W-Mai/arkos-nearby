use std::rc::Rc;

use mirui::ecs::Entity;
use mirui::prelude::draw::*;
use mirui::prelude::*;
use mirui::render::font::{Font, FontBackend, FontManager, FontToken, mirx::MirxFontProvider};
use mirui::surface::FramebufferAccess;
use mirui::ui::widgets::{ParagraphStyle, Text, TextAlign, TextOverflow, TextWrap};
use mirui::ui::{Children, ComputedRect, IdMap, OffscreenRender};

use crate::model::{Page, Phase, View};
use TextAlign::{Center, End, Start};
use TextField::{Headline, HostButton, Next, Peer};

const FONT: FontToken = FontToken::Custom("nearby_menu");
const FONT_BYTES: &[u8] = include_bytes!("../assets/nearby-menu-20.mirx");
const BACKGROUND: Color = Color::rgb(242, 235, 221);
const PANEL: Color = Color::rgb(231, 221, 200);
const ACCENT: Color = Color::rgb(166, 61, 50);
const ACCENT_DIM: Color = Color::rgb(234, 207, 192);
const FOREGROUND: Color = Color::rgb(41, 37, 32);
const SECONDARY: Color = Color::rgb(112, 98, 83);
const DISABLED: Color = Color::rgb(131, 117, 99);
const DIVIDER: Color = Color::rgb(204, 192, 169);
const WARNING: Color = Color::rgb(152, 106, 32);
const WARNING_PANEL: Color = Color::rgb(233, 218, 186);

trait TextSignal {
    fn set_if_changed(&self, next: &str);
}

impl TextSignal for Signal<String> {
    fn set_if_changed(&self, next: &str) {
        if self.get_untracked() != next {
            self.set(next.to_owned());
        }
    }
}

#[derive(Clone, Copy)]
enum TextField {
    Headline,
    Next,
    HostButton,
    Peer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct StepColors {
    background: Color,
    foreground: Color,
}

#[model]
struct DisplayState {
    status: Signal<String>,
    hint: Signal<String>,
    back: Signal<String>,
    headline: Signal<String>,
    next: Signal<String>,
    host_button: Signal<String>,
    peer: Signal<String>,
    confirm: Signal<String>,
    #[observe]
    confirm_visible: bool,
    #[observe]
    steps: [StepColors; 3],
    #[observe]
    stamp: Color,
    #[observe]
    activity: [Color; 3],
}

#[model]
impl DisplayState {
    fn present(&mut self, view: &View) {
        self.status.set_if_changed(status_text(view));
        self.hint.set_if_changed(hint_text(view));
        self.back.set_if_changed(if view.back_label.is_empty() {
            "返回"
        } else {
            &view.back_label
        });
        self.headline.set_if_changed(headline(view));
        self.next.set_if_changed(next_step(view));
        self.host_button.set_if_changed(button_text(view));
        self.peer.set_if_changed(peer_text(view));
        self.confirm.set_if_changed(confirm_text(view));
        self.confirm_visible = !confirm_text(view).is_empty();
        let active = match phase(view) {
            Phase::Connecting => Some(0),
            Phase::Checking | Phase::Notifying => Some(1),
            Phase::WaitingStart | Phase::Starting => Some(2),
            _ => None,
        };
        self.steps = std::array::from_fn(|index| {
            if active == Some(index) {
                StepColors {
                    background: ACCENT,
                    foreground: BACKGROUND,
                }
            } else if active.is_some_and(|step| index < step) {
                StepColors {
                    background: ACCENT_DIM,
                    foreground: ACCENT,
                }
            } else {
                StepColors {
                    background: PANEL,
                    foreground: DISABLED,
                }
            }
        });
    }

    fn status(&self) -> String {
        self.status.get()
    }
    fn hint(&self) -> String {
        self.hint.get()
    }
    fn back(&self) -> String {
        self.back.get()
    }
    fn headline(&self) -> String {
        self.headline.get()
    }
    fn next(&self) -> String {
        self.next.get()
    }
    fn host_button(&self) -> String {
        self.host_button.get()
    }
    fn peer(&self) -> String {
        self.peer.get()
    }
    fn confirm(&self) -> String {
        self.confirm.get()
    }

    fn feedback(&mut self, stamp: Color, activity: [Color; 3]) {
        self.stamp = stamp;
        self.activity = activity;
    }
}

type DisplayHandle = <DisplayState as mirui::core::model::Model>::Handle;

impl DisplayStateHandle {
    fn text(&self, field: TextField) -> String {
        match field {
            Headline => self.headline(),
            Next => self.next(),
            HostButton => self.host_button(),
            Peer => self.peer(),
        }
    }
}

struct DisplayRuntime {
    state: DisplayHandle,
}

pub fn install_fonts<B: FramebufferAccess>(app: &mut App<B>) -> Result<(), String> {
    app.with_offscreen_pool_budget(2 * 1024 * 1024);
    app.with_widget(paint_focus::view());
    let limits = mirx::reader::PayloadLimits::HOST;
    let options = mirx::reader::ReadOptions::new().with_payload_limits(limits);
    let reader = mirx::Reader::open_with(FONT_BYTES, &options)
        .map_err(|error| format!("Menu font container: {error:?}"))?;
    let mut faces = reader
        .chunks()
        .filter(|chunk| chunk.chunk_type() == mirx::ChunkType::FONT);
    let face = faces.next().ok_or("The menu asset contains no font")?;
    if faces.next().is_some() {
        return Err("The menu asset contains multiple fonts".into());
    }
    let provider = MirxFontProvider::from_payload(face.payload(), &limits)
        .map_err(|error| format!("Menu font face: {error:?}"))?;
    let font = Font {
        family: "Nearby Menu",
        size: 20,
        backend: FontBackend::Custom(Rc::new(provider)),
    };
    let fonts = FontManager::new(mirui::core::cache::MaxSize::Unbound, Font::bitmap_8x8());
    fonts.add_static(FONT.cache_key(), font);
    app.with_fonts(fonts);
    let state = app.add_model(DisplayState {
        status: Signal::new(String::new()),
        hint: Signal::new(String::new()),
        back: Signal::new(String::new()),
        headline: Signal::new(String::new()),
        next: Signal::new(String::new()),
        host_button: Signal::new(String::new()),
        peer: Signal::new(String::new()),
        confirm: Signal::new(String::new()),
        confirm_visible: false,
        steps: [StepColors {
            background: PANEL,
            foreground: DISABLED,
        }; 3],
        stamp: ACCENT_DIM,
        activity: [BACKGROUND; 3],
    });
    app.add_resource(DisplayRuntime { state });
    app.add_resource(FocusGeometry(None));
    Ok(())
}

#[derive(Clone, Copy)]
struct Metrics {
    compact: bool,
    title: u16,
    body: u16,
    small: u16,
    row_title: u16,
    row_detail: u16,
    gap: i32,
    inset: i32,
}

impl Metrics {
    fn for_height(height: u16) -> Self {
        let compact = height < 400;
        Self {
            compact,
            title: if compact { 18 } else { 25 },
            body: if compact { 14 } else { 17 },
            small: if compact { 10 } else { 15 },
            row_title: if compact { 14 } else { 18 },
            row_detail: if compact { 9 } else { 12 },
            gap: if compact { 3 } else { 8 },
            inset: if compact { 8 } else { 18 },
        }
    }
}

#[component]
struct StaticCache;

#[component]
struct FocusTarget;

fn paragraph(align: TextAlign, lines: u16) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::Word,
        align,
        vertical_align: mirui::ui::widgets::text::TextVerticalAlign::Center,
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(lines),
        ..Default::default()
    }
}

#[compose]
fn label(value: &str, size: u16, color: Color, align: TextAlign) -> Entity {
    ui! {
        Text (
            value.to_owned(),
            width: Dimension::percent(100),
            height: Dimension::Content,
            min_width: 0,
            min_height: i32::from(size) + 6,
            font: FONT,
            font_size: size,
            text_color: color,
            paragraph: paragraph(align, 1),
            clip_children: true
        )
    }
}

#[compose]
fn live_label(
    state: model!(DisplayState),
    field: TextField,
    size: u16,
    color: Color,
    align: TextAlign,
) -> Entity {
    ui! {
        Text (
            text: ${ state.text(field) },
            width: Dimension::percent(100),
            height: Dimension::Content,
            min_width: 0,
            min_height: i32::from(size) + 6,
            font: FONT,
            font_size: size,
            text_color: color,
            paragraph: paragraph(align, 1),
            clip_children: true
        )
    }
}

#[compose]
fn header(view: &View, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let subtitle = if view.page == Page::Games {
        view.core.clone()
    } else {
        format!("当前游戏  {}", view.title)
    };
    let step = if view.page == Page::Games {
        "选择游戏".into()
    } else {
        format!("步骤 {} / 2", if view.page == Page::Choice { 1 } else { 2 })
    };
    ui! {
        Row (
            id: "nearby_header",
            height: if metrics.compact { 44 } else { 68 },
            shrink: 0.0,
            column_gap: metrics.gap + 4,
            align: AlignItems::Center
        ) {
            Row (
                width: if metrics.compact { 30 } else { 42 },
                height: if metrics.compact { 30 } else { 42 },
                shrink: 0.0,
                bg_color: ${ state.stamp() },
                justify: JustifyContent::Center,
                align: AlignItems::Center,
                border_radius: 0
            ) {
                View (
                    width: 7,
                    height: 18,
                    bg_color: ACCENT
                )
                View (
                    width: 7,
                    height: 6,
                    bg_color: ACCENT
                )
                View (
                    width: 7,
                    height: 18,
                    bg_color: ACCENT
                )
            }
            Column (
                grow: 1.0,
                min_width: 0,
                justify: JustifyContent::Center,
                clip_children: true
            ) [
                StaticCache,
                OffscreenRender::default(),
            ] {
                label (
                    "附近联机",
                    if metrics.compact { 20 } else { 27 },
                    FOREGROUND,
                    Start
                )
                label (
                    &subtitle,
                    metrics.small,
                    SECONDARY,
                    Start
                )
            }
            Text (
                step,
                width: if metrics.compact { 76 } else { 96 },
                height: if metrics.compact { 26 } else { 34 },
                shrink: 0.0,
                bg_color: PANEL,
                font: FONT,
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(Center, 1)
            )
        }
    }
}

#[compose]
fn keycap(value: &str, metrics: Metrics) -> Entity {
    ui! {
        Text (
            value.to_owned(),
            width: if metrics.compact { 20 } else { 25 },
            height: if metrics.compact { 20 } else { 25 },
            shrink: 0.0,
            bg_color: PANEL,
            font: FONT,
            font_size: metrics.small,
            text_color: SECONDARY,
            paragraph: paragraph(Center, 1)
        )
    }
}

#[compose]
fn footer(view: &View, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Row (
            id: "nearby_footer",
            height: if metrics.compact { 20 } else { 32 },
            shrink: 0.0,
            column_gap: metrics.gap,
            align: AlignItems::Center
        ) {
            Text (
                text: ${ if state.confirm_visible() { "A" } else { "" } },
                width: if metrics.compact { 20 } else { 25 },
                height: if metrics.compact { 20 } else { 25 },
                shrink: 0.0,
                bg_color: ${ if state.confirm_visible() { PANEL } else { BACKGROUND } },
                font: FONT,
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(Center, 1)
            )
            Text (
                text: ${ state.confirm() },
                width: if metrics.compact { 30 } else { 42 },
                shrink: 0.0,
                height: Dimension::Content,
                font: FONT,
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(Start, 1)
            )
            keycap ("B", metrics)
            Text (
                text: ${ state.back() },
                max_width: if metrics.compact { 76 } else { 112 },
                min_width: 0,
                height: Dimension::Content,
                font: FONT,
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(Start, 1)
            )
            if view.page == Page::Rooms {
                keycap ("X", metrics)
                Text (
                    "刷新",
                    height: Dimension::Content,
                    font: FONT,
                    font_size: metrics.small,
                    text_color: SECONDARY,
                    paragraph: paragraph(Start, 1)
                )
            }
            Text (
                text: ${ state.hint() },
                grow: 1.0,
                min_width: 0,
                height: Dimension::Content,
                font: FONT,
                font_size: metrics.small,
                text_color: SECONDARY,
                paragraph: paragraph(End, 1),
                clip_children: true
            )
        }
    }
}

#[compose]
fn choice(metrics: Metrics) -> Entity {
    ui! {
        Row (
            id: "focus_list",
            grow: 1.0,
            min_height: 0,
            column_gap: metrics.gap + 6
        ) [
            StaticCache,
            OffscreenRender::default(),
        ] {
            walk [
                (0, "创建房间", "邀请好友加入你的房间", "你是房主"),
                (1, "加入房间", "寻找附近好友的房间", "一起开始游戏"),
            ]
                .into_iter() with card {
                Column (
                    grow: 1.0,
                    min_width: 0,
                    padding: Padding::all(metrics.inset),
                    row_gap: metrics.gap,
                    bg_color: PANEL,
                    border_radius: 0,
                    clip_children: true
                ) [
                    FocusTarget,
                ] {
                    Row (
                        height: if metrics.compact { 28 } else { 44 },
                        align: AlignItems::Center
                    ) {
                        Text (
                            if card.0 == 0 { "+" } else { "2" },
                            width: if metrics.compact { 28 } else { 44 },
                            height: Dimension::percent(100),
                            bg_color: ACCENT_DIM,
                            font: FONT,
                            font_size: metrics.title,
                            text_color: ACCENT,
                            paragraph: paragraph(Center, 1)
                        )
                    }
                    View (
                        grow: 1.0,
                        min_height: 0
                    )
                    label (
                        card.1,
                        metrics.title,
                        FOREGROUND,
                        Start
                    )
                    Text (
                        card.2,
                        width: Dimension::percent(100),
                        min_width: 0,
                        height: Dimension::Content,
                        font: FONT,
                        font_size: metrics.small,
                        text_color: SECONDARY,
                        paragraph: paragraph(Start, 2)
                    )
                    if !metrics.compact {
                        label (
                            card.3,
                            metrics.small,
                            SECONDARY,
                            Start
                        )
                    }
                }
            }
        }
    }
}

fn host_action_enabled(view: &View) -> bool {
    (view.ready || view.phase == Phase::ApprovalPending) && !view.busy
}

#[compose]
fn avatar(number: &str, ready: bool, metrics: Metrics) -> Entity {
    ui! {
        Text (
            number.to_owned(),
            width: if metrics.compact { 36 } else { 60 },
            height: if metrics.compact { 36 } else { 60 },
            bg_color: if ready { ACCENT_DIM } else { PANEL },
            font: FONT,
            font_size: metrics.title,
            text_color: if ready { ACCENT } else { DISABLED },
            paragraph: paragraph(Center, 1)
        ) [
            StaticCache,
            OffscreenRender::default(),
        ]
    }
}

#[compose]
fn host(view: &View, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let enabled = host_action_enabled(view);
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) {
            Row (
                grow: 1.0,
                min_height: 0,
                padding: Padding::all(metrics.inset),
                bg_color: PANEL,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {
                Column (
                    grow: 1.0,
                    min_width: 0,
                    align: AlignItems::Center,
                    justify: JustifyContent::Center,
                    row_gap: metrics.gap
                ) {
                    avatar (
                        "1",
                        true,
                        metrics
                    )
                    label (
                        "你 · 房主",
                        metrics.body,
                        FOREGROUND,
                        Center
                    )
                }
                Row (
                    width: if metrics.compact { 40 } else { 70 },
                    justify: JustifyContent::SpaceEvenly,
                    align: AlignItems::Center
                ) {
                    walk 0..3 with _index {
                        View (
                            width: 5,
                            height: 5,
                            bg_color: if view.ready { ACCENT } else { DISABLED }
                        )
                    }
                }
                Column (
                    grow: 1.0,
                    min_width: 0,
                    align: AlignItems::Center,
                    justify: JustifyContent::Center,
                    row_gap: metrics.gap
                ) {
                    avatar (
                        "2",
                        view.ready,
                        metrics
                    )
                    live_label (
                        state,
                        Peer,
                        metrics.body,
                        if view.ready { FOREGROUND } else { SECONDARY },
                        Center
                    )
                }
            }
            Column (
                id: "focus_host",
                height: if metrics.compact { 34 } else { 50 },
                shrink: 0.0,
                bg_color: if enabled { ACCENT } else { PANEL },
                border_color: ACCENT,
                border_width: if enabled { 2 } else { 0 },
                justify: JustifyContent::Center
            ) [
                FocusTarget,
            ] {
                live_label (
                    state,
                    HostButton,
                    metrics.title,
                    if enabled { BACKGROUND } else { DISABLED },
                    Center
                )
            }
        }
    }
}

#[compose]
fn room_signal(strength: u8, available: bool) -> Entity {
    ui! {
        Row (
            width: 30,
            height: 24,
            shrink: 0.0,
            column_gap: 3,
            align: AlignItems::FlexEnd
        ) {
            walk 0..4 with index {
                View (
                    width: 4,
                    height: 6 + index * 4,
                    bg_color: if available && strength as i32 > index * 25 { ACCENT } else { DIVIDER }
                )
            }
        }
    }
}

#[compose]
fn rooms(view: &View, metrics: Metrics) -> Entity {
    let first = view
        .selected
        .saturating_sub(3)
        .min(view.rooms.len().saturating_sub(4));
    ui! {
        Column (
            id: "focus_list",
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) [
            StaticCache,
            OffscreenRender::default(),
        ] {
            if view.rooms.is_empty() {
                Column (
                    grow: 1.0,
                    bg_color: PANEL,
                    padding: Padding::all(metrics.inset),
                    justify: JustifyContent::Center,
                    row_gap: metrics.gap
                ) {
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
                        font: FONT,
                        font_size: metrics.small,
                        text_color: SECONDARY,
                        paragraph: paragraph(Center, 2)
                    )
                }
            } else {
                walk view.rooms.iter().enumerate().skip(first).take(4) with item {
                    Row (
                        grow: 1.0,
                        min_height: i32::from(metrics.row_title + metrics.row_detail) + 17,
                        max_height: if metrics.compact { 42 } else { 70 },
                        padding: Padding::all(2),
                        column_gap: metrics.gap,
                        bg_color: PANEL,
                        align: AlignItems::Center,
                        clip_children: true
                    ) [
                        FocusTarget,
                    ] {
                        Column (
                            grow: 1.0,
                            min_width: 0,
                            justify: JustifyContent::Center
                        ) {
                            label (
                                &item.1.name,
                                metrics.row_title,
                                if item.1.available { FOREGROUND } else { DISABLED },
                                Start
                            )
                            label (
                                if item.1.available {
                                    &item.1.detail
                                } else {
                                    "房间已离线，等待重新出现"
                                },
                                metrics.row_detail,
                                if item.1.available { SECONDARY } else { DISABLED },
                                Start
                            )
                        }
                        room_signal (
                            item.1.signal,
                            item.1.available
                        )
                    }
                }
                View (
                    grow: 1.0,
                    min_height: 0
                )
            }
        }
    }
}

#[compose]
fn games(view: &View, metrics: Metrics) -> Entity {
    ui! {
        Column (
            id: "focus_list",
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) [
            StaticCache,
            OffscreenRender::default(),
        ] {
            if view.entries.is_empty() {
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
            } else {
                walk view.entries.iter().take(4).enumerate() with item {
                    Row (
                        grow: 1.0,
                        min_height: i32::from(metrics.row_title + metrics.row_detail) + 17,
                        max_height: if metrics.compact { 42 } else { 70 },
                        padding: Padding::all(2),
                        column_gap: metrics.gap,
                        bg_color: PANEL,
                        align: AlignItems::Center,
                        clip_children: true
                    ) [
                        FocusTarget,
                    ] {
                        Text (
                            if item.1.directory { ">" } else { "·" },
                            width: if metrics.compact { 20 } else { 32 },
                            shrink: 0.0,
                            height: Dimension::Content,
                            font: FONT,
                            font_size: metrics.title,
                            text_color: ACCENT,
                            paragraph: paragraph(Center, 1)
                        )
                        Column (
                            grow: 1.0,
                            min_width: 0,
                            justify: JustifyContent::Center
                        ) {
                            label (
                                &item.1.name,
                                metrics.row_title,
                                FOREGROUND,
                                Start
                            )
                            label (
                                &item.1.detail,
                                metrics.row_detail,
                                SECONDARY,
                                Start
                            )
                        }
                    }
                }
                View (
                    grow: 1.0,
                    min_height: 0
                )
            }
        }
    }
}

#[compose]
fn joined(view: &View, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            bg_color: PANEL,
            padding: Padding::all(metrics.inset),
            row_gap: metrics.gap,
            justify: JustifyContent::Center,
            clip_children: true
        ) {
            Row (
                height: if metrics.compact { 44 } else { 60 },
                shrink: 0.0,
                column_gap: metrics.gap
            ) {
                walk [(0, "连接房间"), (1, "核对游戏"), (2, "等待开局")].into_iter() with step {
                    Column (
                        grow: 1.0,
                        min_width: 0,
                        align: AlignItems::Center,
                        row_gap: metrics.gap
                    ) {
                        Text (
                            (step.0 + 1).to_string(),
                            width: if metrics.compact { 22 } else { 28 },
                            height: if metrics.compact { 22 } else { 28 },
                            bg_color: ${ state.steps()[step.0].background },
                            font: FONT,
                            font_size: metrics.small,
                            text_color: ${ state.steps()[step.0].foreground },
                            paragraph: paragraph(Center, 1)
                        )
                        label (
                            step.1,
                            metrics.small,
                            SECONDARY,
                            Center
                        )
                    }
                }
            }
            View (
                grow: 1.0,
                min_height: 0,
                max_height: 24
            )
            live_label (
                state,
                Headline,
                metrics.title,
                FOREGROUND,
                Center
            )
            live_label (
                state,
                Next,
                metrics.small,
                SECONDARY,
                Center
            )
            if !view.room_name.is_empty() {
                label (
                    &view.room_name,
                    metrics.small,
                    SECONDARY,
                    Center
                )
            }
        }
    }
}

#[compose]
fn leaving(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            bg_color: PANEL,
            padding: Padding::all(metrics.inset),
            row_gap: metrics.gap,
            justify: JustifyContent::Center,
            align: AlignItems::Center
        ) {
            Text (
                "B",
                width: 44,
                height: 36,
                bg_color: ACCENT_DIM,
                font: FONT,
                font_size: metrics.title,
                text_color: ACCENT,
                paragraph: paragraph(Center, 1)
            )
            live_label (
                state,
                Headline,
                metrics.title,
                FOREGROUND,
                Center
            )
            live_label (
                state,
                Next,
                metrics.small,
                SECONDARY,
                Center
            )
        }
    }
}

#[compose]
fn unavailable(metrics: Metrics) -> Entity {
    ui! {
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
                font: FONT,
                font_size: metrics.title,
                text_color: WARNING,
                paragraph: paragraph(Center, 1)
            )
            label (
                "暂时无法进行联机",
                metrics.title,
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
    }
}

#[compose]
fn page(view: &View, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let heading = match view.page {
        Page::Choice => "和好友一起，开始游戏",
        Page::Host => {
            if view.room_name.is_empty() {
                "正在创建房间"
            } else {
                &view.room_name
            }
        }
        Page::Rooms => "选择好友的房间",
        Page::Games => &view.title,
        Page::Joined => "加入房间",
        Page::Leaving => "退出联机",
        Page::Unavailable => "附近联机",
    };
    ui! {
        Column (
            id: "nearby_shell",
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            bg_color: BACKGROUND,
            padding: @(id(nearby_shell).height) { Padding::all(if nearby_shell.height < Fixed::from_int(400) { 8 } else { 24 }) },
            row_gap: metrics.gap,
        ) {
            header (
                view,
                state,
                metrics,
            )
            View (
                height: 1,
                shrink: 0.0,
                bg_color: DIVIDER,
            )
            Column (
                id: "nearby_content",
                grow: 1.0,
                min_height: 0,
                row_gap: metrics.gap,
            ) {
                Row (
                    height: i32::from(metrics.title) + 8,
                    shrink: 0.0,
                    align: AlignItems::Center,
                    column_gap: metrics.gap,
                ) {
                    Text (
                        heading.to_owned(),
                        grow: 1.0,
                        min_width: 0,
                        height: Dimension::Content,
                        min_height: i32::from(metrics.title) + 8,
                        font: FONT,
                        font_size: metrics.title,
                        text_color: FOREGROUND,
                        paragraph: paragraph(Start, 1),
                    )
                    Row (
                        width: 36,
                        shrink: 0.0,
                        align: AlignItems::Center,
                        justify: JustifyContent::SpaceBetween,
                    ) {
                        walk 0_usize..3 with index {
                            View (
                                width: 6,
                                height: 6,
                                bg_color: ${state.activity()[index]},
                            )
                        }
                    }
                }
                Text (
                    id: "nearby_status",
                    text: ${ state.status() },
                    height: Dimension::Content,
                    min_height: i32::from(metrics.small) + 6,
                    shrink: 0.0,
                    width: Dimension::percent(100),
                    min_width: 0,
                    font: FONT,
                    font_size: metrics.small,
                    text_color: SECONDARY,
                    paragraph: paragraph(Start, 2),
                    clip_children: true,
                )
                match view.page {
                    Page::Choice => { choice(metrics) }
                    Page::Host => { host(view, state, metrics) }
                    Page::Rooms => { rooms(view, metrics) }
                    Page::Joined => { joined(view, state, metrics) }
                    Page::Leaving => { leaving(state, metrics) }
                    Page::Unavailable => { unavailable(metrics) }
                    Page::Games => { games(view, metrics) }
                }
            }
            View (
                height: 1,
                shrink: 0.0,
                bg_color: DIVIDER,
            )
            footer (
                view,
                state,
                metrics,
            )
        }
    }
}

pub struct Scene {
    pub focus_rect: Option<[i32; 4]>,
    pub loading: bool,
    state: DisplayHandle,
}

impl Scene {
    pub fn feedback(&self, stamp: Color, activity: [Color; 3]) {
        self.state.feedback(stamp, activity);
    }
}

#[component]
struct FocusOverlay;

struct FocusGeometry(Option<[i32; 4]>);

#[view(component = FocusOverlay, name = "nearby_focus", priority = 90)]
fn paint_focus(geometry: res!(FocusGeometry), paint: paint!()) {
    let Some([x, y, width, height]) = geometry.0 else {
        return;
    };
    for [left, top, w, h] in [
        [x, y, width, 2],
        [x, y + height - 2, width, 2],
        [x, y + 2, 2, height - 4],
        [x + width - 2, y + 2, 2, height - 4],
    ] {
        paint.draw(&DrawCommand::Fill {
            area: Rect::new(left, top, w, h),
            transform: paint.transform(),
            quad: None,
            color: ACCENT,
            radius: Fixed::ZERO,
            opa: 255,
        });
    }
}

#[compose]
fn focus_overlay() -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100)
        ) [
            FocusOverlay,
        ]
    }
}

#[compose]
fn replace_focus(next: Option<[i32; 4]>) -> Option<[i32; 4]> {
    res!(mut FocusGeometry).with(|geometry| std::mem::replace(&mut geometry.0, next))
}

pub fn move_focus<B: FramebufferAccess>(app: &mut App<B>, next: Option<[i32; 4]>) {
    let Some(root) = app.root else { return };
    let previous = app.compose(root, replace_focus(next));
    if previous == next {
        return;
    }
    let (Some(old), Some(next)) = (previous, next) else {
        if let Some(rect) = previous.or(next) {
            for [x, y, w, h] in border_bands(rect) {
                app.invalidate_rect(Rect::new(x, y, w, h));
            }
        }
        return;
    };
    let dx = (next[0] - old[0]).abs();
    let dy = (next[1] - old[1]).abs();
    let x = old[0].min(next[0]);
    let y = old[1].min(next[1]);
    let width = old[2] + dx;
    let height = old[3] + dy;
    let same_size = old[2..] == next[2..];
    let regions = if same_size && dy == 0 && dx < old[2] - 4 {
        let band = dx + 2;
        Some([
            [x, y, band, height],
            [x + width - band, y, band, height],
            [x + band, y, width - 2 * band, 2],
            [x + band, y + height - 2, width - 2 * band, 2],
        ])
    } else if same_size && dx == 0 && dy < old[3] - 4 {
        let band = dy + 2;
        Some([
            [x, y, width, band],
            [x, y + height - band, width, band],
            [x, y + band, 2, height - 2 * band],
            [x + width - 2, y + band, 2, height - 2 * band],
        ])
    } else {
        None
    };
    if let Some(regions) = regions {
        for [left, top, w, h] in regions {
            app.invalidate_rect(Rect::new(left, top, w, h));
        }
    } else {
        for [left, top, w, h] in border_bands(old).into_iter().chain(border_bands(next)) {
            if w > 0 && h > 0 {
                app.invalidate_rect(Rect::new(left, top, w, h));
            }
        }
    }
}

fn border_bands([x, y, width, height]: [i32; 4]) -> [[i32; 4]; 4] {
    [
        [x, y, width, 2],
        [x, y + height - 2, width, 2],
        [x, y + 2, 2, height - 4],
        [x + width - 2, y + 2, 2, height - 4],
    ]
}

fn loading(view: &View) -> bool {
    if matches!(
        phase(view),
        Phase::ApprovalPending
            | Phase::Closed
            | Phase::Failed
            | Phase::RestoreFailed
            | Phase::RoomActive
    ) {
        return false;
    }
    view.busy
        || view.refreshing
        || (view.page == Page::Rooms && view.rooms.is_empty())
        || (view.page == Page::Host && !view.ready)
        || matches!(view.page, Page::Joined | Page::Leaving)
}

fn phase(view: &View) -> Phase {
    if view.phase != Phase::Idle {
        return view.phase;
    }
    match view.page {
        Page::Joined => {
            if view.ready {
                Phase::WaitingStart
            } else {
                Phase::Connecting
            }
        }
        Page::Host => {
            if view.ready {
                Phase::HostReady
            } else {
                Phase::WaitingPeer
            }
        }
        Page::Leaving => Phase::Leaving,
        _ => Phase::Idle,
    }
}

fn headline(view: &View) -> &str {
    match phase(view) {
        Phase::Creating => "正在创建房间",
        Phase::WaitingPeer => "等待好友加入",
        Phase::PeerPreparing => "好友正在准备",
        Phase::ApprovalPending => "好友加入",
        Phase::HostReady => "可以开始游戏",
        Phase::Connecting => "正在连接房间",
        Phase::Checking => "正在核对游戏",
        Phase::Notifying => "正在确认准备完成",
        Phase::WaitingStart => "已就绪，等待房主开局",
        Phase::Starting => "正在启动游戏",
        Phase::Leaving => "正在退出联机",
        Phase::Closed => "本次联机已结束",
        Phase::Failed => "未能完成本次连接",
        Phase::RestoreFailed => "网络尚未恢复",
        Phase::RoomActive => "这台掌机已有房间",
        Phase::Idle => "请选择创建或加入",
    }
}

fn next_step(view: &View) -> &str {
    match phase(view) {
        Phase::Creating => "房间准备好后，好友才能加入",
        Phase::WaitingPeer => "请好友选择这个房间加入",
        Phase::PeerPreparing => "准备完成后，可以开始游戏",
        Phase::ApprovalPending => "按 A 确认，B 关闭房间",
        Phase::HostReady => "按 A 开始，双方一起进入游戏",
        Phase::Connecting => "连接完成后，会核对两台的游戏",
        Phase::Checking => "正在确认两台的游戏一致",
        Phase::Notifying => "房主确认就绪后，才能开始游戏",
        Phase::WaitingStart => "请房主按 A 开始游戏",
        Phase::Starting => "正在准备双方的联机画面",
        Phase::Leaving => "关闭本次连接，恢复原网络",
        Phase::Closed | Phase::Failed => "按 B 返回，重新创建或选择房间",
        Phase::RestoreFailed => "按 A 重试，完成后再返回",
        Phase::RoomActive => "按 A 关闭上一场后继续",
        Phase::Idle => "方向键选择，按 A 确认",
    }
}

fn button_text(view: &View) -> &str {
    match phase(view) {
        Phase::HostReady => {
            if view.busy {
                "正在开局"
            } else {
                "开始游戏"
            }
        }
        Phase::Creating => "房间准备中",
        Phase::PeerPreparing => "等待好友准备",
        Phase::ApprovalPending => "确认加入",
        Phase::Starting => "正在开局",
        Phase::Closed | Phase::Failed => "按 B 返回",
        Phase::Leaving | Phase::RestoreFailed => "正在退出房间",
        _ => "等待好友加入",
    }
}

fn peer_text(view: &View) -> &str {
    match phase(view) {
        Phase::HostReady | Phase::Starting => "好友已就绪",
        Phase::PeerPreparing => "正在核对",
        Phase::ApprovalPending => "等待确认",
        Phase::Closed | Phase::Failed | Phase::Leaving => "联机已结束",
        _ => "等待好友",
    }
}

fn status_text(view: &View) -> &str {
    if !view.status.is_empty() {
        return &view.status;
    }
    match view.page {
        Page::Choice => "两台掌机选择同一款游戏，再创建或加入房间",
        Page::Host => "请好友选择同一款游戏，加入这个房间",
        Page::Rooms => "选择房间后确认加入，列表会自动刷新",
        Page::Joined => next_step(view),
        Page::Leaving => "正在结束本次联机...",
        Page::Unavailable => "这款游戏暂时不支持附近联机",
        Page::Games => "选择游戏后创建或加入房间",
    }
}

fn hint_text(view: &View) -> &str {
    if view.page == Page::Games {
        return if view.busy {
            "正在核对游戏"
        } else {
            "L/R 翻页"
        };
    }
    match phase(view) {
        Phase::RestoreFailed => "按 A 重试退出",
        Phase::RoomActive => "A 关闭后继续",
        Phase::Leaving => "等待网络恢复",
        Phase::WaitingStart => "等待房主开局",
        Phase::HostReady => "A 开始游戏",
        Phase::ApprovalPending => "按 A 确认",
        Phase::Creating | Phase::WaitingPeer | Phase::PeerPreparing => "B 关闭房间",
        Phase::Connecting | Phase::Checking | Phase::Notifying => "B 取消加入",
        Phase::Starting => "正在进入游戏",
        Phase::Closed | Phase::Failed => "B 返回后重试",
        _ => {
            if view.page == Page::Rooms {
                "自动搜索"
            } else {
                "方向键选择"
            }
        }
    }
}

fn confirm_text(view: &View) -> &str {
    match phase(view) {
        Phase::RestoreFailed => "重试",
        Phase::RoomActive => "关闭",
        Phase::Closed | Phase::Failed => "返回",
        Phase::HostReady if !view.busy => "开始",
        Phase::ApprovalPending if !view.busy => "确认",
        Phase::Idle => match view.page {
            Page::Choice => "确认",
            Page::Rooms => "加入",
            Page::Games if !view.busy && !view.entries.is_empty() => "选择",
            _ => "",
        },
        _ => "",
    }
}

fn scene_key(view: &View) -> View {
    let mut key = view.clone();
    key.status.clear();
    key.refreshing = false;
    key.phase = Phase::Idle;
    key.back_label.clear();
    match view.page {
        Page::Choice => {
            key.selected = 0;
            key.busy = false;
            key.room_name.clear();
        }
        Page::Rooms => {
            key.selected = view
                .selected
                .saturating_sub(3)
                .min(view.rooms.len().saturating_sub(4));
            key.busy = false;
            key.room_name.clear();
            for room in &mut key.rooms {
                room.signal = [0, 25, 50, 75]
                    .into_iter()
                    .filter(|threshold| room.signal > *threshold)
                    .count() as u8;
            }
        }
        Page::Games => {
            key.selected = 0;
            key.busy = false;
        }
        Page::Host => {
            key.ready = view.ready;
            key.busy = host_action_enabled(view);
        }
        Page::Joined | Page::Leaving => {
            key.busy = false;
            key.ready = false;
        }
        _ => {}
    }
    key
}

pub fn can_update(previous: &View, next: &View) -> bool {
    scene_key(previous) == scene_key(next)
}

#[compose]
fn focus_rect(view: &View) -> Option<[i32; 4]> {
    let slot = match view.page {
        Page::Choice => view.selected.min(1),
        Page::Rooms if !view.rooms.is_empty() => view
            .selected
            .saturating_sub(
                view.selected
                    .saturating_sub(3)
                    .min(view.rooms.len().saturating_sub(4)),
            )
            .min(3),
        Page::Games if !view.entries.is_empty() => view.selected.min(3),
        Page::Host if host_action_enabled(view) => 0,
        _ => return None,
    };
    let parent = res!(IdMap).with(|ids| {
        ids.get(if view.page == Page::Host {
            "focus_host"
        } else {
            "focus_list"
        })
    })?;
    let entity = if view.page == Page::Host {
        parent
    } else {
        let children = com!(parent, Children)?.0.clone();
        children
            .into_iter()
            .filter(|entity| com!(*entity, FocusTarget).is_some())
            .nth(slot)?
    };
    let rect = com!(entity, ComputedRect)?.0;
    Some([
        rect.x.to_int(),
        rect.y.to_int(),
        rect.w.to_int(),
        rect.h.to_int(),
    ])
}

#[compose]
fn display_state() -> DisplayHandle {
    res!(DisplayRuntime).with(|runtime| runtime.state.clone())
}

pub fn update<B: FramebufferAccess>(
    app: &mut App<B>,
    scene: &mut Scene,
    view: &View,
) -> Result<(), String> {
    scene.state.present(view);
    scene.loading = loading(view);
    app.prepare_layout()
        .map_err(|error| format!("Menu layout: {error:?}"))?;
    scene.focus_rect = app
        .root
        .and_then(|root| app.compose(root, focus_rect(view)));
    Ok(())
}

pub fn build<B: FramebufferAccess>(app: &mut App<B>, view: &View) -> Result<Scene, String> {
    app.clear_root();
    let root = app.spawn_root().bg_color(BACKGROUND).id();
    let state = app.compose(root, display_state());
    state.present(view);
    state.feedback(
        ACCENT_DIM,
        [if loading(view) {
            ACCENT_DIM
        } else {
            BACKGROUND
        }; 3],
    );
    let metrics = Metrics::for_height(app.backend.viewport().logical_size().1);
    app.compose(root, page(view, state.clone(), metrics));
    app.compose(root, focus_overlay());
    app.prepare_layout()
        .map_err(|error| format!("Menu layout: {error:?}"))?;
    let scene = Scene {
        focus_rect: app.compose(root, focus_rect(view)),
        loading: loading(view),
        state,
    };
    app.compose(root, replace_focus(scene.focus_rect));
    Ok(scene)
}

pub fn draw<B: FramebufferAccess>(app: &mut App<B>, view: &View) -> Result<Scene, String> {
    let scene = build(app, view)?;
    app.render()
        .map_err(|error| format!("Menu frame: {error:?}"))?;
    Ok(scene)
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
