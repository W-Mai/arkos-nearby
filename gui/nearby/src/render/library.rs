use mirui::prelude::*;
use mirui::ui::widgets::{Image, TextAlign::Start};

use super::keyboard::keycap as keyboard_keycap;
use super::{
    artwork::cover,
    cartridge::SlotLayout,
    consoles::console,
    lists::games,
    pixel::Scenery,
    reading::cartridge_title,
    rows::Row,
    selection::FocusTarget,
    state::DisplayState,
    style::*,
    widgets::{caption, hold_badge},
};
use crate::model::LibraryKind;

const KEYS: [&str; 40] = [
    "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "A", "S", "D", "F", "G", "H", "J", "K", "L",
    "退格", "Z", "X", "C", "V", "B", "N", "M", "空格", "清空", "结果", "1", "2", "3", "4", "5",
    "6", "7", "8", "9", "0",
];

const CARTRIDGE_EDGE: i32 = 2;
const CARTRIDGE_RAIL: i32 = 2;
const CARTRIDGE_CAP: i32 = 8;
const CARTRIDGE_CONTACTS: usize = super::cartridge::CONTACT_COUNT;

#[compose]
pub(super) fn library(kind: LibraryKind, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0) {
            match kind {
                LibraryKind :: Favorites => {
                    shelf (state, state.shelf(), metrics)
                }
                LibraryKind :: Search => {
                    search (state, metrics)
                }
                _ => {
                    games (state, metrics, kind == LibraryKind::Platforms)
                }
            }
        }
    }
}

#[compose]
fn shelf(
    state: model!(DisplayState),
    grid: model!(super::shelf::Shelf),
    metrics: Metrics,
) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            min_width: 0,
            bg_color: BACKGROUND,
            clip_children: true,
            padding: Padding::all(super::shelf::INSET),
            row_gap: 4
        ) [
            Scenery::Ridge,
        ] {
            if ${ state.entries().is_empty() } {
                console (super::consoles::Platform::Favorites)
            } else {
                View (
                    grow: 1.0,
                    min_width: 0,
                    min_height: 0,
                    container: true
                ) {
                    shelf_sheet (state, grid, metrics, false)
                    walk ${ (!grid.departing().is_empty()).then_some(()) } with _departing {
                        shelf_sheet (state, grid, metrics, true)
                    }
                }
            }
            Row (height: 4, shrink: 0.0, column_gap: 4, justify: JustifyContent::Center) [
                super::shelf::PageMarkers,
            ] {
                walk ${
                    let (first, end, _) = grid.pages();
                    first..end
                } with page by page {
                    page_marker (grid, page)
                }
            }
        } on SizeChanged { grid.set_size(event.new.width, event.new.height); }
    }
}

#[compose]
fn shelf_sheet(
    state: model!(DisplayState),
    grid: model!(super::shelf::Shelf),
    metrics: Metrics,
    departing: bool,
) -> Entity {
    ui! {
        Column (
            position: Position::Absolute,
            top: 0,
            left: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            transform: ${
                {
                    let offsets = grid.offsets();
                    mirui::types::Transform::translate(
                        if departing { offsets.1 } else { offsets.0 },
                        Fixed::ZERO,
                    )
                }
            },
            min_height: 0,
            min_width: 0,
            row_gap: super::shelf::GAP,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            walk ${ 0..grid.plan().grid.rows } with line {
                Row (
                    width: ${ grid.plan().width() },
                    height: ${ grid.plan().card_height },
                    shrink: 0.0,
                    min_height: 0,
                    column_gap: super::shelf::GAP
                ) {
                    walk ${
                        let rows = if departing { grid.departing() } else { state.entries() };
                        rows.entries()
                            .skip(line * grid.plan().grid.columns)
                            .take(grid.plan().grid.columns)
                            .collect::<Vec<_>>()
                    } with item by item.0 {
                        cartridge (state, item.1, item.0, grid, metrics, departing)
                    }
                }
            }
        }
    }
}

#[compose]
fn page_marker(grid: model!(super::shelf::Shelf), page: usize) -> Entity {
    ui! {
        View (
            width: ${ grid.marker_width(page) },
            bg_color: ${ if grid.pages().2 == page { ACCENT } else { DIVIDER } }
        )
    }
}

#[compose]
fn cartridge(
    state: model!(DisplayState),
    row: model!(Row),
    member: mirui::core::model::ModelItemId,
    grid: model!(super::shelf::Shelf),
    metrics: Metrics,
    departing: bool,
) -> Entity {
    let geometry = grid.clone();
    let frame = grid.clone();
    let shell_top = grid.clone();
    let shell_left = grid.clone();
    let shell_width = grid.clone();
    let shell_height = grid.clone();
    let label_state = state.clone();
    let stroke = metrics.focus_border();
    let origin = if departing {
        super::material::CardOrigin::Retiring
    } else {
        super::material::CardOrigin::Shelf(row.clone())
    };
    let motion = row.clone();
    let cover = row.clone();
    let directory = row.clone();
    let pins = row.clone();
    let label = row.clone();
    let badge = row.clone();
    ui! {
        Column (
            width: ${ geometry.plan().card_width },
            shrink: 0.0,
            min_width: 0,
            min_height: 0,
            container: true,
            bg_color: SHELL_EDGE
        ) [
            origin,
        ] {
            if !departing {
                View (
                    position: Position::Absolute,
                    top: 0, left: 0, width: Dimension::percent(100),
                    height: ${ SlotLayout::new(frame.plan(), stroke).frame_height }
                ) [
                    FocusTarget::row(row.clone()),
                    mirui::ui::IgnoreHitTest,
                ]
            }
            if ${ !pins.directory() } {
                exposed_contacts (grid.clone(), pins.clone(), departing, stroke)
            }
            Column (
                position: Position::Absolute,
                top: ${ SlotLayout::new(shell_top.plan(), stroke).shell_top },
                left: ${ SlotLayout::new(shell_left.plan(), stroke).shell_left },
                width: ${ SlotLayout::new(shell_width.plan(), stroke).shell_width },
                height: ${ SlotLayout::new(shell_height.plan(), stroke).shell_height },
                min_width: 0,
                min_height: 0,
                padding: Padding::all(super::shelf::CARD_INSET),
                container: true,
                bg_color: SHELL,
                border_width: 2,
                border_color: SHELL_EDGE,
                transform: ${ grid.pose(motion.index(), departing).shell(metrics.tiny) }
            ) [
                super::cartridge::Shell,
            ] {
                View (position: Position::Absolute, width: 0, height: 0) [
                    super::material::MeasureCard(true),
                    mirui::ui::IgnoreHitTest,
                ]
                if ${ directory.directory() } {
                    library_icon (metrics)
                } else {
                    View (grow: 1.0, min_width: 0, min_height: 0, clip_children: true) {
                        Column (
                            grow: 1.0, min_width: 0, min_height: 0,
                            transform: ${ grid.pose(cover.index(), departing).cover() }
                        ) {
                            if departing {
                                cartridge_source (row.cover_bounds(), ${ row.platform() })
                            } else {
                                cartridge_content (state.clone(), row, member)
                            }
                        }
                    }
                }
                if ${
                    !departing && state.shows_netplay_hint() && state.selected() == Some(badge.index())
                        && !badge.directory()
                } {
                    Row (position: Position::Absolute, top: 4, left: 4) {
                        hold_badge ()
                    }
                }
                cartridge_label (label_state, label.clone(), ${ grid.pose(label.index(), departing).paper() }, metrics, departing)
            }
        }
    }
}

#[compose]
fn exposed_contacts(
    grid: model!(super::shelf::Shelf),
    row: model!(Row),
    departing: bool,
    stroke: i32,
) -> Entity {
    let motion = grid.clone();
    let position = row.clone();
    ui! {
        Row (
            position: Position::Absolute,
            left: Dimension::percent(18),
            top: ${ SlotLayout::new(grid.plan(), stroke).contacts_top },
            width: Dimension::percent(64),
            height: super::cartridge::CONTACT_HEIGHT,
            clip_children: true,
            bg_color: SHELL_EDGE
        ) {
            Row (
                position: Position::Absolute, left: 0, top: 0,
                width: Dimension::percent(100), height: super::cartridge::CONTACT_HEIGHT,
                column_gap: CARTRIDGE_EDGE,
                transform: ${ motion.pose(position.index(), departing).contact_offset() }
            ) [
                super::cartridge::Contacts,
            ] {
                walk 0..CARTRIDGE_CONTACTS with index {
                    View (
                        grow: 1.0,
                        bg_color: ${
                            if grid.pose(row.index(), departing).reflection().is_some_and(|phase| {
                                usize::from(phase) == index
                            }) { CARTRIDGE_REFLECTION } else { CARTRIDGE_BRASS }
                        }
                    )
                }
            }
        }
    }
}

#[compose]
fn cartridge_label(
    state: model!(DisplayState),
    row: model!(Row),
    paper: prop!(Fixed),
    metrics: Metrics,
    departing: bool,
) -> Entity {
    let enabled = row.clone();
    let visibility = paper.clone();
    let paper_title = row.clone();
    let paper_ink = mirui::ui::theme::ThemedColor::from(CARTRIDGE_INK);
    let paper_backdrop = mirui::ui::theme::ThemedColor::from(CARTRIDGE_PAPER);
    let shell_backdrop = mirui::ui::theme::ThemedColor::from(SHELL);
    let selected = row.clone();
    let scroll = mirui::ui::Prop::new(move || {
        !departing && !selected.directory() && state.selected() == Some(selected.index())
    });
    let inset = metrics.cartridge_title_padding();
    ui! {
        Row (
            height: super::shelf::LABEL_HEIGHT, shrink: 0.0, min_width: 0,
            container: true, padding: inset
        ) {
            cartridge_title (
                ${ row.label(true) },
                scroll.clone(),
                ${ mirui::ui::theme::ThemedColor::from(if enabled.available() { SHELL_TEXT } else { DIVIDER }) },
                shell_backdrop
            )
            View (
                position: Position::Absolute, top: 0, left: 0,
                width: Dimension::percent(100), height: Dimension::percent(100),
                visible: ${ visibility.get() > Fixed::ZERO }
            ) {
                Row (
                    position: Position::Absolute, top: 0, left: 0,
                    width: ${ Dimension::Percent(paper.get() * 100) },
                    height: Dimension::percent(100), clip_children: true
                ) {
                    Row (
                        position: Position::Absolute, top: 0, left: 0,
                        width: @(width as full) { full },
                        height: Dimension::percent(100),
                        padding: inset, bg_color: CARTRIDGE_PAPER
                    ) {
                        cartridge_title (${ paper_title.label(true) }, scroll, paper_ink, paper_backdrop)
                        Image (
                            src: path!(M 0 0 H 4 V 4 Z), viewbox: 4,
                            position: Position::Absolute, top: 0,
                            left: @(width as full) { full - Fixed::from_int(4) },
                            width: 4, height: 4, color: SHELL
                        ) [mirui::ui::IgnoreHitTest]
                    }
                }
            }
        }
    }
}

#[compose]
fn search(state: model!(DisplayState), metrics: Metrics) -> Entity {
    let query_state = state.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Row (
                height: i32::from(metrics.body) + LABEL_LEADING + metrics.gap * 2 + 4,
                shrink: 0.0,
                min_width: 0,
                align: AlignItems::Center,
                column_gap: metrics.gap,
                padding: Padding::all(metrics.gap),
                bg_color: PANEL,
                border_width: ${ if state.keyboard() { 2 } else { 1 } },
                border_color: ${ if state.keyboard() { ACCENT } else { DIVIDER } }
            ) {
                search_icon (16)
                Column (grow: 1.0, min_width: 0) {
                    caption (
                        ${
                            {
                                let query = query_state.query();
                                if query.is_empty() { "输入游戏名或文件名".into() } else { query }
                            }
                        },
                        FOREGROUND,
                        metrics.body,
                        Start,
                        LABEL_LEADING
                    )
                }
            }
            if ${ state.keyboard() } {
                Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
                    walk 0..4 with line {
                        Row (grow: 1.0, min_height: 0, column_gap: metrics.gap) {
                            walk 0..10 with column {
                                keycap (state, line * 10 + column, metrics)
                            }
                        }
                    }
                }
            } else {
                games (state, metrics, false)
            }
        }
    }
}

#[compose]
pub(super) fn search_icon(size: i32) -> Entity {
    ui! {
        Image (
            src: path!(
                M 3 0 H 9 V 2 H 11 V 4 H 13 V 9 H 11 V 11 H 9 V 13 H 4 V 11 H 2 V 9 H 0 V 4 H 2 V
                2 H 3 Z M 4 3 H 3 V 9 H 4 V 10 H 9 V 9 H 10 V 4 H 9 V 3 Z M 11 10 H 13 V 12 H 15
                V 14 H 16 V 16 H 14 V 15 H 12 V 13 H 10 V 11 Z
            ),
            viewbox: 16,
            width: size,
            height: size,
            shrink: 0.0,
            color: ACCENT
        )
    }
}

#[compose]
fn keycap(state: model!(DisplayState), index: usize, metrics: Metrics) -> Entity {
    let selected = state.clone();
    let motion = state.clone();
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            transform: ${ motion.card_motion().card_transform(100 + index) }
        ) [
            FocusTarget::fixed(100 + index),
        ] {
            keyboard_keycap (
                ${ KEYS[index].to_owned() },
                ${ selected.selected() == Some(100 + index) },
                ${ mirui::types::Transform::IDENTITY },
                super::keyboard::search_anchor(index),
                if KEYS[index].len() == 1 { metrics.title } else { metrics.body },
                metrics
            )
        }
    }
}

#[compose]
pub(super) fn library_icon(metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            min_width: 0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(metrics.gap),
            bg_color: SKY
        ) {
            View (grow: 1.0, min_height: 0, width: Dimension::percent(100)) {
                Image (
                    src: path!(M 22 20 H 138 V 111 H 136 V 116 H 128 V 111 H 32 V 116 H 24 V 111 H 22 Z),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: FOREGROUND
                )
                Image (
                    src: path!(M 27 25 H 133 V 106 H 27 Z),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: PANEL
                )
                Image (
                    src: path!(
                        M 31 61 H 129 V 66 H 31 Z M 31 96 H 129 V 101 H 31 Z M 87 34 H 118 V 57 H 87 Z M
                        35 73 H 63 V 92 H 35 Z
                    ),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: ACCENT
                )
                Image (
                    src: path!(M 35 32 H 56 V 57 H 35 Z M 69 71 H 89 V 92 H 69 Z),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: CLAY
                )
                Image (
                    src: path!(M 61 31 H 82 V 57 H 61 Z M 95 71 H 118 V 92 H 95 Z),
                    viewbox: 160,
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    color: WARNING
                )
            }
            Image (
                src: path!(M 0 0 L 9 8 L 18 0 V 5 L 9 14 L 0 5 Z),
                viewbox: 18,
                width: 18,
                height: 14,
                shrink: 0.0,
                color: ACCENT
            )
        }
    }
}

#[compose]
fn cartridge_content(
    state: model!(DisplayState),
    row: model!(Row),
    member: mirui::core::model::ModelItemId,
) -> Entity {
    let picture = row.static_picture();
    let image = picture.clone();
    let dimensions = picture.clone();
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, container: true) {
            Column (
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                min_width: 0,
                min_height: 0,
                visible: ${ !picture.hidden() }
            ) [
                super::static_snapshot::Source {
                    row: row.clone(),
                    state,
                    member,
                },
            ] {
                cartridge_source (row.cover_bounds(), ${ row.platform() })
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
                walk ${ image.token().into_iter() } with token by token {
                    static_cartridge_image (image.clone(), token)
                }
            }
        } on SizeChanged { dimensions.resize(event.new); }
    }
}

#[compose]
fn static_cartridge_image(picture: model!(super::static_snapshot::Picture), token: u64) -> Entity {
    let top = picture.clone();
    let width = picture.clone();
    let height = picture.clone();
    ui! {
        Image (
            src: super::static_snapshot::texture_token(token),
            position: Position::Absolute,
            left: ${ picture.bounds().x },
            top: ${ top.bounds().y },
            width: ${ width.bounds().w },
            height: ${ height.bounds().h }
        ) [
            mirui::ui::IgnoreHitTest,
        ]
    }
}

#[compose]
pub(super) fn cartridge_source(
    bounds: model!(super::artwork::CoverBounds),
    platform: prop!(super::consoles::Platform),
) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: 0) {
            Column (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                bg_color: SHELL,
                padding: Padding {
                    left: CARTRIDGE_RAIL.into(),
                    right: CARTRIDGE_RAIL.into(),
                    ..Default::default()
                },
                border_width: CARTRIDGE_EDGE,
                border_color: SHELL_EDGE
            ) {
                Row (
                    height: CARTRIDGE_CAP,
                    shrink: 0.0,
                    align: AlignItems::Center,
                    column_gap: CARTRIDGE_RAIL
                ) {
                    cartridge_rivet ()
                    Column (grow: 1.0, row_gap: 2) {
                        View (height: 1, bg_color: SHELL_FACE)
                        View (height: 1, bg_color: SHELL_EDGE)
                    }
                    cartridge_rivet ()
                }
                Row (grow: 1.0, min_height: 0, min_width: 0, column_gap: CARTRIDGE_RAIL * 2) {
                    cartridge_grip ()
                    Column (
                        grow: 1.0,
                        min_height: 0,
                        min_width: 0,
                        bg_color: SHELL_EDGE,
                        padding: Padding::all(CARTRIDGE_EDGE)
                    ) {
                        cover (bounds, platform)
                    }
                    cartridge_grip ()
                }
                Column (height: CARTRIDGE_CAP, shrink: 0.0, justify: JustifyContent::Center) {
                    View (height: 1, bg_color: SHELL_FACE)
                    View (height: 1, bg_color: SHELL_EDGE)
                }
            }
        }
    }
}

#[compose]
fn cartridge_grip() -> Entity {
    ui! {
        Column (
            width: CARTRIDGE_RAIL,
            shrink: 0.0,
            justify: JustifyContent::Center,
            row_gap: CARTRIDGE_RAIL
        ) {
            walk 0..3 with _index {
                Column (height: 3, shrink: 0.0, bg_color: SHELL_EDGE) {
                    View (height: 1, bg_color: SHELL_FACE)
                }
            }
        }
    }
}

#[compose]
fn cartridge_rivet() -> Entity {
    ui! {
        Column (
            width: 3,
            height: 3,
            shrink: 0.0,
            bg_color: SHELL_EDGE,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            View (width: 3, height: 1, bg_color: SHELL_FACE)
        }
    }
}
