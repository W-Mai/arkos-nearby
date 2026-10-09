use std::rc::Rc;

use mirui::ecs::{Entity, World};
use mirui::prelude::*;
use mirui::render::command::DrawCommand;
use mirui::render::font::{Font, FontBackend, FontManager, FontToken, mirx::MirxFontProvider};
use mirui::render::renderer::Renderer;
use mirui::surface::FramebufferAccess;
use mirui::ui::layout::{LayoutStyle, Position};
use mirui::ui::view::ViewCtx;
use mirui::ui::widgets::{ParagraphStyle, Text, TextAlign, TextOverflow, TextWrap};
use mirui::ui::{Children, OffscreenRender, Parent};

use crate::model::{Page, Phase, View};

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

pub fn install_fonts<B: FramebufferAccess>(app: &mut App<B>) -> Result<(), String> {
    app.with_offscreen_pool_budget(2 * 1024 * 1024);
    app.with_widget(
        mirui::ui::view::View::new("nearby_focus", 90, paint_focus).with_filter::<FocusPaint>(),
    );
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
    app.world
        .resource::<FontManager>()
        .ok_or("The menu has no font manager")?
        .add_static(FONT.cache_key(), font);
    Ok(())
}

fn layout([x, y, width, height]: [i32; 4]) -> LayoutStyle {
    LayoutStyle {
        position: Position::Absolute,
        left: Dimension::px(x),
        top: Dimension::px(y),
        width: Dimension::px(width),
        height: Dimension::px(height),
        ..Default::default()
    }
}

fn attach(world: &mut World, parent: Entity, child: Entity) -> Entity {
    world.insert(child, Parent(parent));
    world
        .get_mut::<Children>(parent)
        .expect("Menu containers own their children")
        .0
        .push(child);
    child
}

fn panel(
    world: &mut World,
    parent: Entity,
    rect: [i32; 4],
    color: Color,
    border: Option<Color>,
) -> Entity {
    let builder = WidgetBuilder::new(world)
        .layout(layout(rect))
        .bg_color(color)
        .border_radius(0)
        .clip_children(true);
    let child = if let Some(color) = border {
        builder.border(color, 2).id()
    } else {
        builder.id()
    };
    attach(world, parent, child)
}

fn label(
    world: &mut World,
    parent: Entity,
    rect: [i32; 4],
    value: impl Into<String>,
    size: u16,
    color: Color,
    align: TextAlign,
) -> Entity {
    let paragraph = ParagraphStyle {
        wrap: TextWrap::Grapheme,
        align,
        vertical_align: mirui::ui::widgets::text::TextVerticalAlign::Center,
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(1),
        ..Default::default()
    };
    let child = WidgetBuilder::new(world)
        .layout(layout(rect))
        .text(Text::new(value.into()))
        .font(FONT)
        .font_size(size)
        .text_color(color)
        .paragraph(paragraph)
        .clip_children(true)
        .id();
    attach(world, parent, child)
}

fn text(
    world: &mut World,
    parent: Entity,
    rect: [i32; 4],
    value: impl Into<String>,
    size: u16,
    color: Color,
) -> Entity {
    label(world, parent, rect, value, size, color, TextAlign::Start)
}

fn centered(
    world: &mut World,
    parent: Entity,
    rect: [i32; 4],
    value: impl Into<String>,
    size: u16,
    color: Color,
) -> Entity {
    label(world, parent, rect, value, size, color, TextAlign::Center)
}

fn header(world: &mut World, root: Entity, overlay: Entity, view: &View) -> Entity {
    let badge = panel(world, overlay, [28, 26, 42, 42], ACCENT_DIM, None);
    panel(world, badge, [11, 12, 8, 18], ACCENT, None);
    panel(world, badge, [23, 12, 8, 18], ACCENT, None);
    panel(world, badge, [15, 18, 12, 5], ACCENT, None);
    text(world, root, [84, 8, 392, 43], "附近联机", 27, FOREGROUND);
    text(
        world,
        root,
        [85, 53, 380, 28],
        if view.page == Page::Games {
            view.core.clone()
        } else {
            format!("当前游戏  {}", view.title)
        },
        17,
        SECONDARY,
    );
    let step = if view.page == Page::Choice { 1 } else { 2 };
    let progress = panel(world, root, [500, 28, 112, 36], PANEL, None);
    centered(
        world,
        progress,
        [0, 0, 112, 36],
        if view.page == Page::Games {
            "选择游戏".into()
        } else {
            format!("步骤 {step} / 2")
        },
        15,
        SECONDARY,
    );
    panel(world, root, [28, 94, 584, 1], DIVIDER, None);
    badge
}

fn footer(world: &mut World, root: Entity, page: &Page) {
    panel(world, root, [28, 434, 584, 1], DIVIDER, None);
    let key = panel(world, root, [138, 447, 25, 25], PANEL, None);
    centered(world, key, [0, 0, 25, 25], "B", 14, SECONDARY);
    if *page == Page::Rooms {
        let key = panel(world, root, [248, 447, 25, 25], PANEL, None);
        centered(world, key, [0, 0, 25, 25], "X", 14, SECONDARY);
        text(world, root, [282, 447, 68, 25], "刷新", 15, SECONDARY);
    }
}

fn choice(world: &mut World, root: Entity) {
    text(
        world,
        root,
        [30, 112, 560, 38],
        "和好友一起，开始游戏",
        24,
        FOREGROUND,
    );
    for (index, title, detail, bottom) in [
        (0, "创建房间", "邀请好友加入你的房间", "你是房主"),
        (1, "加入房间", "寻找附近好友的房间", "一起开始游戏"),
    ] {
        let card = panel(world, root, [28 + index * 300, 197, 284, 201], PANEL, None);
        let icon = panel(world, card, [23, 22, 49, 49], ACCENT_DIM, None);
        if index == 0 {
            panel(world, icon, [21, 12, 7, 25], ACCENT, None);
            panel(world, icon, [12, 21, 25, 7], ACCENT, None);
        } else {
            panel(world, icon, [9, 19, 12, 12], ACCENT, None);
            panel(world, icon, [28, 19, 12, 12], ACCENT, None);
            panel(world, icon, [19, 23, 12, 4], ACCENT, None);
        }
        text(world, card, [23, 87, 239, 37], title, 27, FOREGROUND);
        text(world, card, [23, 129, 239, 25], detail, 16, SECONDARY);
        text(world, card, [23, 168, 239, 22], bottom, 14, SECONDARY);
    }
}

fn avatar(world: &mut World, parent: Entity, x: i32, number: &str, name: &str, ready: bool) {
    let icon = panel(
        world,
        parent,
        [x, 24, 61, 61],
        if ready { ACCENT_DIM } else { PANEL },
        None,
    );
    centered(
        world,
        icon,
        [0, 0, 61, 61],
        number,
        25,
        if ready { ACCENT } else { DISABLED },
    );
    centered(
        world,
        parent,
        [x - 32, 91, 125, 27],
        name,
        17,
        if ready { FOREGROUND } else { SECONDARY },
    );
}

fn host_action_enabled(view: &View) -> bool {
    (view.ready || view.phase == Phase::ApprovalPending) && !view.busy
}

fn host(world: &mut World, root: Entity, view: &View) {
    text(
        world,
        root,
        [30, 110, 560, 35],
        if view.room_name.is_empty() {
            "正在创建房间"
        } else {
            view.room_name.as_str()
        },
        25,
        FOREGROUND,
    );
    let lobby = panel(world, root, [28, 193, 584, 153], PANEL, None);
    avatar(world, lobby, 143, "1", "你 · 房主", true);
    avatar(world, lobby, 375, "2", "", view.ready);
    for index in 0..3 {
        panel(
            world,
            lobby,
            [277 + index * 15, 49, 5, 5],
            if view.ready { ACCENT } else { DISABLED },
            None,
        );
    }
    let enabled = host_action_enabled(view);
    panel(
        world,
        root,
        [28, 365, 584, 50],
        if enabled { ACCENT } else { PANEL },
        enabled.then_some(ACCENT),
    );
}

fn signal(world: &mut World, parent: Entity, strength: u8, available: bool) {
    for index in 0..4 {
        let height = 6 + index * 4;
        panel(
            world,
            parent,
            [522 + index * 8, 25 - height, 5, height],
            if available && strength as i32 > index * 25 {
                ACCENT
            } else {
                DIVIDER
            },
            None,
        );
    }
}

fn rooms(world: &mut World, root: Entity, view: &View) {
    text(
        world,
        root,
        [30, 109, 560, 36],
        "选择好友的房间",
        25,
        FOREGROUND,
    );
    if view.rooms.is_empty() {
        let empty = panel(world, root, [28, 198, 584, 194], PANEL, None);
        centered(
            world,
            empty,
            [24, 53, 536, 40],
            "正在搜索附近的房间",
            24,
            FOREGROUND,
        );
        centered(
            world,
            empty,
            [24, 104, 536, 33],
            "请好友先创建房间，并把两台掌机放近一些",
            16,
            SECONDARY,
        );
        return;
    }
    let first = view
        .selected
        .saturating_sub(3)
        .min(view.rooms.len().saturating_sub(4));
    for (row, room) in view.rooms.iter().enumerate().skip(first).take(4) {
        let card = panel(
            world,
            root,
            [28, 178 + (row - first) as i32 * 62, 584, 55],
            PANEL,
            None,
        );
        text(
            world,
            card,
            [17, 4, 480, 28],
            &room.name,
            20,
            if room.available { FOREGROUND } else { DISABLED },
        );
        text(
            world,
            card,
            [18, 30, 470, 21],
            if room.available {
                room.detail.as_str()
            } else {
                "房间已离线，等待重新出现"
            },
            13,
            if room.available { SECONDARY } else { DISABLED },
        );
        signal(world, card, room.signal, room.available);
    }
}

fn games(world: &mut World, root: Entity, view: &View) {
    text(world, root, [30, 109, 560, 36], &view.title, 25, FOREGROUND);
    if view.entries.is_empty() {
        let empty = panel(world, root, [28, 198, 584, 194], PANEL, None);
        centered(
            world,
            empty,
            [24, 61, 536, 40],
            "没有可选游戏",
            24,
            FOREGROUND,
        );
        return;
    }
    for (row, entry) in view.entries.iter().take(4).enumerate() {
        let card = panel(
            world,
            root,
            [28, 178 + row as i32 * 62, 584, 55],
            PANEL,
            None,
        );
        centered(
            world,
            card,
            [12, 9, 32, 32],
            if entry.directory { ">" } else { "·" },
            24,
            ACCENT,
        );
        text(world, card, [53, 4, 507, 28], &entry.name, 20, FOREGROUND);
        text(world, card, [54, 30, 498, 21], &entry.detail, 13, SECONDARY);
    }
}

fn joined(world: &mut World, root: Entity, view: &View) {
    text(world, root, [30, 110, 560, 35], "加入房间", 25, FOREGROUND);
    let lobby = panel(world, root, [28, 193, 584, 217], PANEL, None);
    for (index, title) in [(0, "连接房间"), (1, "核对游戏"), (2, "等待开局")] {
        text(
            world,
            lobby,
            [36 + index * 178, 57, 150, 28],
            title,
            17,
            SECONDARY,
        );
    }
    if !view.room_name.is_empty() {
        centered(
            world,
            lobby,
            [24, 184, 536, 27],
            &view.room_name,
            16,
            SECONDARY,
        );
    }
}

fn leaving(world: &mut World, root: Entity) {
    text(world, root, [30, 110, 560, 35], "退出联机", 25, FOREGROUND);
    let card = panel(world, root, [28, 193, 584, 217], PANEL, None);
    let badge = panel(world, card, [264, 23, 56, 40], ACCENT_DIM, None);
    centered(world, badge, [0, 0, 56, 40], "B", 24, ACCENT);
}

fn unavailable(world: &mut World, root: Entity) {
    let card = panel(world, root, [28, 132, 584, 266], PANEL, None);
    let icon = panel(world, card, [260, 22, 64, 64], WARNING_PANEL, None);
    centered(world, icon, [0, 0, 64, 64], "!", 29, WARNING);
    centered(
        world,
        card,
        [22, 99, 540, 43],
        "暂时无法进行联机",
        25,
        FOREGROUND,
    );
    centered(
        world,
        card,
        [22, 204, 540, 33],
        "按 B 返回，仍然可以单机游玩",
        17,
        ACCENT,
    );
}

pub struct Scene {
    pub focus: Option<Entity>,
    pub focus_rect: Option<[i32; 4]>,
    pub stamp: Entity,
    pub activity: [Entity; 3],
    pub loading: bool,
    pub status: Entity,
    pub hint: Entity,
    pub back_label: Entity,
    pub headline: Option<Entity>,
    pub next_step: Option<Entity>,
    pub host_button: Option<Entity>,
    pub peer_label: Option<Entity>,
    pub join_steps: Vec<(Entity, Entity)>,
    pub confirm_key: Entity,
    pub confirm_digit: Entity,
    pub confirm_label: Entity,
}

struct FocusPaint([i32; 4]);

fn paint_focus(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    _rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let [x, y, width, height] = world
        .get::<FocusPaint>(entity)
        .expect("The focus view owns its coordinates")
        .0;
    for [left, top, w, h] in [
        [x, y, width, 2],
        [x, y + height - 2, width, 2],
        [x, y + 2, 2, height - 4],
        [x + width - 2, y + 2, 2, height - 4],
    ] {
        let command = DrawCommand::Fill {
            area: Rect::new(left, top, w, h),
            transform: ctx.transform,
            quad: None,
            color: ACCENT,
            radius: Fixed::ZERO,
            opa: 255,
        };
        let clip = *ctx.clip;
        ctx.draw(renderer, &command, &clip);
    }
}

pub fn move_focus(world: &mut World, entity: Entity, position: [i32; 2]) {
    let focus = world
        .get_mut::<FocusPaint>(entity)
        .expect("The scene owns its focus overlay");
    let old = focus.0;
    let mut next = old;
    next[0] = position[0];
    next[1] = position[1];
    if old == next {
        return;
    }
    focus.0 = next;
    let dx = (next[0] - old[0]).abs();
    let dy = (next[1] - old[1]).abs();
    let x = old[0].min(next[0]);
    let y = old[1].min(next[1]);
    let width = old[2] + dx;
    let height = old[3] + dy;
    let regions = if dy == 0 && dx < old[2] - 4 {
        let band = dx + 2;
        [
            [x, y, band, height],
            [x + width - band, y, band, height],
            [x + band, y, width - 2 * band, 2],
            [x + band, y + height - 2, width - 2 * band, 2],
        ]
    } else if dx == 0 && dy < old[3] - 4 {
        let band = dy + 2;
        [
            [x, y, width, band],
            [x, y + height - band, width, band],
            [x, y + band, 2, height - 2 * band],
            [x + width - 2, y + band, 2, height - 2 * band],
        ]
    } else {
        [[x, y, width, height]; 4]
    };
    for [left, top, w, h] in regions {
        world.invalidate_rect(Rect::new(left, top, w, h));
    }
}

pub fn focus_rect(view: &View) -> Option<[i32; 4]> {
    match view.page {
        Page::Choice => Some([28 + view.selected.min(1) as i32 * 300, 197, 284, 201]),
        Page::Rooms if !view.rooms.is_empty() => {
            let first = view
                .selected
                .saturating_sub(3)
                .min(view.rooms.len().saturating_sub(4));
            Some([
                28,
                178 + view.selected.saturating_sub(first).min(3) as i32 * 62,
                584,
                55,
            ])
        }
        Page::Host if host_action_enabled(view) => Some([28, 365, 584, 50]),
        Page::Games if !view.entries.is_empty() => {
            Some([28, 178 + view.selected.min(3) as i32 * 62, 584, 55])
        }
        _ => None,
    }
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

fn update_steps<B: FramebufferAccess>(app: &mut App<B>, scene: &Scene, view: &View) {
    use mirui::ui::property::prop;
    let active = match phase(view) {
        Phase::Connecting => Some(0),
        Phase::Checking | Phase::Notifying => Some(1),
        Phase::WaitingStart | Phase::Starting => Some(2),
        _ => None,
    };
    for (index, (square, digit)) in scene.join_steps.iter().enumerate() {
        let current = active == Some(index);
        let complete = active.is_some_and(|step| index < step);
        app.world
            .widget_mut(*square)
            .unwrap()
            .set::<prop::BackgroundColor>(
                (if current {
                    ACCENT
                } else if complete {
                    ACCENT_DIM
                } else {
                    PANEL
                })
                .into(),
            );
        app.world
            .widget_mut(*digit)
            .unwrap()
            .set::<prop::TextColor>(
                (if current {
                    BACKGROUND
                } else if complete {
                    ACCENT
                } else {
                    DISABLED
                })
                .into(),
            );
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
        Page::Joined => "等房主开始，就可以一起玩了",
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
            key.ready = view.ready || view.phase == Phase::ApprovalPending;
            key.busy = view.busy && key.ready;
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

pub fn update<B: FramebufferAccess>(app: &mut App<B>, scene: &mut Scene, view: &View) {
    use mirui::ui::property::prop;
    app.world
        .widget_mut(scene.status)
        .expect("The scene owns its status")
        .set::<prop::TextContent>(status_text(view).into());
    app.world
        .widget_mut(scene.hint)
        .expect("The scene owns its footer hint")
        .set::<prop::TextContent>(hint_text(view).into());
    scene.loading = loading(view);
    app.world
        .widget_mut(scene.back_label)
        .unwrap()
        .set::<prop::TextContent>(
            (if view.back_label.is_empty() {
                "返回"
            } else {
                &view.back_label
            })
            .into(),
        );
    for (entity, value) in [
        (scene.headline, headline(view)),
        (scene.next_step, next_step(view)),
        (scene.host_button, button_text(view)),
        (scene.peer_label, peer_text(view)),
    ] {
        if let Some(entity) = entity {
            app.world
                .widget_mut(entity)
                .unwrap()
                .set::<prop::TextContent>(value.into());
        }
    }
    update_steps(app, scene, view);
    let confirm = confirm_text(view);
    app.world
        .widget_mut(scene.confirm_key)
        .unwrap()
        .set::<prop::BackgroundColor>(
            (if confirm.is_empty() {
                BACKGROUND
            } else {
                PANEL
            })
            .into(),
        );
    app.world
        .widget_mut(scene.confirm_digit)
        .unwrap()
        .set::<prop::TextContent>((if confirm.is_empty() { "" } else { "A" }).into());
    app.world
        .widget_mut(scene.confirm_label)
        .unwrap()
        .set::<prop::TextContent>(confirm.into());
}

pub fn build<B: FramebufferAccess>(app: &mut App<B>, view: &View) -> Result<Scene, String> {
    if let Some(root) = app.root.take() {
        mirui::ui::despawn_subtree(&mut app.world, root);
    }
    let root = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::px(640),
            height: Dimension::px(480),
            ..Default::default()
        })
        .bg_color(BACKGROUND)
        .font(FONT)
        .id();
    let base = panel(&mut app.world, root, [0, 0, 640, 480], BACKGROUND, None);
    app.world.insert(base, OffscreenRender::default());
    let stamp = header(&mut app.world, base, root, view);
    match view.page {
        Page::Choice => choice(&mut app.world, base),
        Page::Host => host(&mut app.world, base, view),
        Page::Rooms => rooms(&mut app.world, base, view),
        Page::Joined => joined(&mut app.world, base, view),
        Page::Leaving => leaving(&mut app.world, base),
        Page::Unavailable => unavailable(&mut app.world, base),
        Page::Games => games(&mut app.world, base, view),
    }
    footer(&mut app.world, base, &view.page);
    let status = if view.page == Page::Unavailable {
        centered(
            &mut app.world,
            root,
            [50, 283, 540, 36],
            status_text(view),
            17,
            SECONDARY,
        )
    } else {
        let rect = match view.page {
            Page::Choice => [30, 151, 560, 27],
            Page::Rooms => [30, 148, 560, 24],
            _ => [30, 149, 560, 27],
        };
        text(&mut app.world, root, rect, status_text(view), 16, SECONDARY)
    };
    let hint = label(
        &mut app.world,
        root,
        [305, 446, 307, 27],
        hint_text(view),
        15,
        SECONDARY,
        TextAlign::End,
    );
    let back_label = text(
        &mut app.world,
        root,
        [172, 447, 95, 25],
        if view.back_label.is_empty() {
            "返回"
        } else {
            &view.back_label
        },
        15,
        SECONDARY,
    );
    let confirm = confirm_text(view);
    let confirm_key = panel(
        &mut app.world,
        root,
        [28, 447, 25, 25],
        if confirm.is_empty() {
            BACKGROUND
        } else {
            PANEL
        },
        None,
    );
    let confirm_digit = centered(
        &mut app.world,
        confirm_key,
        [0, 0, 25, 25],
        if confirm.is_empty() { "" } else { "A" },
        14,
        SECONDARY,
    );
    let confirm_label = text(
        &mut app.world,
        root,
        [62, 447, 68, 25],
        confirm,
        15,
        SECONDARY,
    );
    let headline = matches!(view.page, Page::Joined | Page::Leaving).then(|| {
        centered(
            &mut app.world,
            root,
            [52, 299, 536, 40],
            headline(view),
            25,
            FOREGROUND,
        )
    });
    let next_step = matches!(view.page, Page::Joined | Page::Leaving).then(|| {
        centered(
            &mut app.world,
            root,
            [52, 348, 536, 27],
            next_step(view),
            17,
            SECONDARY,
        )
    });
    let host_button = (view.page == Page::Host).then(|| {
        centered(
            &mut app.world,
            root,
            [28, 365, 584, 50],
            button_text(view),
            21,
            if host_action_enabled(view) {
                BACKGROUND
            } else {
                DISABLED
            },
        )
    });
    let peer_label = (view.page == Page::Host).then(|| {
        centered(
            &mut app.world,
            root,
            [371, 284, 125, 27],
            peer_text(view),
            17,
            if view.ready { FOREGROUND } else { SECONDARY },
        )
    });
    let join_steps = if view.page == Page::Joined {
        (0..3)
            .map(|index| {
                let square = panel(
                    &mut app.world,
                    root,
                    [64 + index * 178, 215, 28, 28],
                    PANEL,
                    None,
                );
                let digit = centered(
                    &mut app.world,
                    square,
                    [0, 0, 28, 28],
                    (index + 1).to_string(),
                    16,
                    DISABLED,
                );
                (square, digit)
            })
            .collect()
    } else {
        Vec::new()
    };
    let focus_rect = focus_rect(view);
    let focus = focus_rect.map(|rect| {
        let overlay = WidgetBuilder::new(&mut app.world)
            .layout(layout([0, 0, 640, 480]))
            .id();
        app.world.insert(overlay, FocusPaint(rect));
        attach(&mut app.world, root, overlay)
    });
    let loading = loading(view);
    let activity = [0, 1, 2].map(|index| {
        panel(
            &mut app.world,
            root,
            [568 + index * 12, 119, 6, 6],
            if loading { ACCENT_DIM } else { BACKGROUND },
            None,
        )
    });
    app.set_root(root);
    app.prepare_text_layout()
        .map_err(|error| format!("Menu text: {error:?}"))?;
    app.systems.run_all(&mut app.world);
    let scene = Scene {
        focus,
        focus_rect,
        stamp,
        activity,
        loading,
        status,
        hint,
        back_label,
        headline,
        next_step,
        host_button,
        peer_label,
        join_steps,
        confirm_key,
        confirm_digit,
        confirm_label,
    };
    update_steps(app, &scene, view);
    Ok(scene)
}

pub fn draw<B: FramebufferAccess>(app: &mut App<B>, view: &View) -> Result<Scene, String> {
    let scene = build(app, view)?;
    app.render()
        .map_err(|error| format!("Menu frame: {error:?}"))?;
    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::Motion;
    use mirui::ui::OffscreenGeneration;

    #[test]
    fn focus_damage_stays_disjoint_and_excludes_unchanged_interiors() {
        for page in [Page::Choice, Page::Rooms] {
            let mut view = View {
                page: page.clone(),
                ..View::default()
            };
            if page == Page::Rooms {
                view.rooms.push(crate::model::RoomRow::default());
            }
            let mut app = App::headless(640, 480);
            app.with_default_widgets().with_default_systems();
            install_fonts(&mut app).unwrap();
            let scene = draw(&mut app, &view).unwrap();
            let rect = scene.focus_rect.unwrap();
            let position = if page == Page::Choice {
                [rect[0] + 1, rect[1]]
            } else {
                [rect[0], rect[1] + 1]
            };
            move_focus(&mut app.world, scene.focus.unwrap(), position);
            app.render_dirty().unwrap();
            let plan = &app
                .world
                .resource::<mirui::ui::render_system::LastDirtyRegions>()
                .unwrap()
                .0;
            assert!(plan.rects.len() <= 4);
            let area: i32 = plan
                .rects
                .iter()
                .map(|rect| rect.w.to_int() * rect.h.to_int())
                .sum();
            assert!(area < (rect[2] + rect[3]) * 8);
            for (index, left) in plan.rects.iter().enumerate() {
                for right in &plan.rects[index + 1..] {
                    assert!(
                        left.x >= right.x + right.w
                            || right.x >= left.x + left.w
                            || left.y >= right.y + right.h
                            || right.y >= left.y + left.h
                    );
                }
            }
        }
    }

    #[test]
    fn dynamic_updates_preserve_the_static_page_cache() {
        for page in [
            Page::Choice,
            Page::Host,
            Page::Rooms,
            Page::Joined,
            Page::Unavailable,
        ] {
            let initial = View {
                page: page.clone(),
                ..View::default()
            };
            let mut actual = App::headless(640, 480);
            actual.with_default_widgets().with_default_systems();
            install_fonts(&mut actual).unwrap();
            let mut scene = draw(&mut actual, &initial).unwrap();
            let root = actual.root.unwrap();
            let base = actual.world.get::<Children>(root).unwrap().0[0];
            let generation = actual
                .world
                .get::<OffscreenGeneration>(base)
                .map_or(0, |value| value.0);
            let mut next = initial.clone();
            next.status = "选择房间后按 A 加入。".into();
            if page == Page::Choice {
                next.selected = 1;
            }
            assert!(can_update(&initial, &next));
            update(&mut actual, &mut scene, &next);
            let mut motion = Motion::default();
            motion.retarget(&initial.page, scene.focus_rect, 0);
            motion.retarget(&next.page, focus_rect(&next), 0);
            motion.paint(&mut actual, &scene, 220).unwrap();
            assert_eq!(actual.root, Some(root));
            assert_eq!(
                actual
                    .world
                    .get::<OffscreenGeneration>(base)
                    .map_or(0, |value| value.0),
                generation
            );
        }
    }

    #[test]
    fn host_actions_share_enabled_state_for_confirmation_and_start() {
        let mut view = View {
            page: Page::Host,
            phase: Phase::ApprovalPending,
            ..View::default()
        };
        assert!(host_action_enabled(&view));
        assert_eq!(button_text(&view), "确认加入");
        view.phase = Phase::HostReady;
        view.ready = true;
        assert!(host_action_enabled(&view));
        assert_eq!(button_text(&view), "开始游戏");
        view.busy = true;
        assert!(!host_action_enabled(&view));
        view.busy = false;
        view.ready = false;
        view.phase = Phase::PeerPreparing;
        assert!(!host_action_enabled(&view));
    }
}
