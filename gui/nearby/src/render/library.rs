use mirui::prelude::*;
use mirui::ui::widgets::{Image, TextAlign::Start};

use super::keyboard::keycap as keyboard_keycap;
use super::{
    artwork::cover,
    cartridge::{
        CartridgeFace, Label as CartridgeLabel, Seat, SlotLayout, front as cartridge_front,
    },
    consoles::console,
    lists::games,
    pixel::Scenery,
    rows::Row,
    selection::FocusTarget,
    state::DisplayState,
    style::*,
    widgets::caption,
};
use crate::model::LibraryKind;

const KEYS: [&str; 40] = [
    "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "A", "S", "D", "F", "G", "H", "J", "K", "L",
    "退格", "Z", "X", "C", "V", "B", "N", "M", "空格", "清空", "结果", "1", "2", "3", "4", "5",
    "6", "7", "8", "9", "0",
];

const CARTRIDGE_EDGE: i32 = 2;
const CARTRIDGE_RAIL: i32 = 3;
const CARTRIDGE_CAP: i32 = 8;
const CARTRIDGE_GRIP_HEIGHT: i32 = 23;

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
            if ${ state.populated_rows() == 0 } {
                console (super::consoles::Platform::Favorites)
            } else {
                View (
                    grow: 1.0,
                    min_width: 0,
                    min_height: 0,
                    container: true
                ) {
                    shelf_sheet (state, grid, metrics, false)
                    departing_page_image (grid)
                    walk ${ (!grid.departing().is_empty()).then_some(()) } with _departing {
                        shelf_sheet (state, grid, metrics, true)
                    }
                }
            }
            View (width: Dimension::percent(100), height: super::shelf::PAGE_MARK_HEIGHT, shrink: 0.0, container: true) [
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
            visible: ${ !departing || grid.departing_count() > 0 },
            min_height: 0,
            min_width: 0,
            row_gap: super::shelf::GAP,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) [
            super::static_snapshot::turn::PageSource { state: state.clone(), departing },
        ] {
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
fn departing_page_image(grid: model!(super::shelf::Shelf)) -> Entity {
    let picture = grid.departure_image();
    ui! {
        View (
            position: Position::Absolute, top: 0, left: 0,
            width: Dimension::percent(100), height: Dimension::percent(100),
            visible: ${ grid.departure_image().hidden() },
            transform: ${ mirui::types::Transform::translate(grid.offsets().1, Fixed::ZERO) }
        ) [mirui::ui::IgnoreHitTest] {
            static_cartridge_image (picture)
        }
    }
}

#[compose]
fn page_marker(grid: model!(super::shelf::Shelf), page: usize) -> Entity {
    ui! {
        View (
            position: Position::Absolute, left: Dimension::percent(50), top: 0,
            width: super::shelf::SHORT_MARK, height: Dimension::percent(100),
            transform: ${ grid.marker_transform(page) },
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
    let stroke = metrics.focus_border();
    let origin = if departing {
        super::material::CardOrigin::Retiring
    } else {
        super::material::CardOrigin::Shelf(row.clone())
    };
    let position = grid.clone();
    let shell_grid = grid.clone();
    let face_grid = grid.clone();
    let shell_row = row.clone();
    let face_row = row.clone();
    let label = CartridgeLabel::new(row.clone(), state.clone(), grid.clone(), departing);
    let placement =
        mirui::ui::Prop::new(move || SlotLayout::new(position.plan(), stroke).shell_rect());
    let seat = Seat {
        grid: grid.clone(),
        row: row.clone(),
        departing,
        stroke,
    };
    let face = if departing {
        CartridgeFace::Departing(state.clone(), row.clone())
    } else {
        CartridgeFace::Live(state.clone(), row.clone(), member)
    };
    let visible_index = row.clone();
    let visible_grid = grid.clone();
    ui! {
        Column (
            width: ${ geometry.plan().card_width },
            shrink: 0.0,
            min_width: 0,
            min_height: 0,
            container: true,
            visible: ${ visible_index.index() < if departing { visible_grid.departing_count() } else { state.populated_rows() } },
            bg_color: CARTRIDGE_TRAY
        ) [
            origin,
            seat,
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
            cartridge_front (
                Some(placement),
                ${ shell_grid.pose(shell_row.index(), departing).shell(metrics.tiny) },
                ${ face_grid.pose(face_row.index(), departing) },
                row.cover_bounds(),
                ${ row.platform() },
                face,
                ${ row.directory() },
                metrics,
                label
            )
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
pub(super) fn cartridge_content(
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
            static_cartridge_image (image)
        } on SizeChanged { dimensions.resize(event.new); }
    }
}

#[compose]
pub(super) fn departing_cartridge_content(row: model!(Row)) -> Entity {
    let picture = row.static_picture();
    let image = picture.clone();
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, container: true) {
            Column (grow: 1.0, min_width: 0, min_height: 0, visible: ${ !picture.hidden() }) {
                cartridge_source (row.cover_bounds(), ${ row.platform() })
            }
            static_cartridge_image (image)
        }
    }
}

#[compose]
fn static_cartridge_image(picture: model!(super::static_snapshot::Picture)) -> Entity {
    let image = picture.clone();
    let visible = picture.clone();
    let top = picture.clone();
    let width = picture.clone();
    let height = picture.clone();
    ui! {
        Image (
            src: ${ image.image() },
            visible: ${ visible.hidden() },
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
                bg_color: CARTRIDGE_BODY,
                padding: Padding {
                    left: CARTRIDGE_RAIL.into(),
                    right: CARTRIDGE_RAIL.into(),
                    ..Default::default()
                },
                border_width: CARTRIDGE_EDGE,
                border_color: CARTRIDGE_RECESS
            ) {
                Row (
                    height: CARTRIDGE_CAP,
                    shrink: 0.0,
                    align: AlignItems::Center,
                    column_gap: CARTRIDGE_RAIL
                ) {
                    cartridge_rivet ()
                    Column (grow: 1.0, row_gap: 2) {
                        View (height: 1, bg_color: CARTRIDGE_RIM)
                        View (height: 1, bg_color: CARTRIDGE_RECESS)
                    }
                    cartridge_rivet ()
                }
                Row (grow: 1.0, min_height: 0, min_width: 0, column_gap: CARTRIDGE_RAIL * 2) {
                    cartridge_grip ()
                    Column (
                        grow: 1.0,
                        min_height: 0,
                        min_width: 0,
                        bg_color: CARTRIDGE_RECESS,
                        padding: Padding::all(CARTRIDGE_EDGE)
                    ) {
                        cover (bounds, platform)
                    }
                    cartridge_grip ()
                }
                Column (height: CARTRIDGE_CAP, shrink: 0.0, justify: JustifyContent::Center) {
                    View (height: 1, bg_color: CARTRIDGE_RIM)
                    View (height: 1, bg_color: CARTRIDGE_RECESS)
                }
            }
        }
    }
}

#[compose]
fn cartridge_grip() -> Entity {
    ui! {
        Column (width: CARTRIDGE_RAIL, shrink: 0.0, justify: JustifyContent::Center) {
            Image (
                width: CARTRIDGE_GRIP_HEIGHT, height: CARTRIDGE_GRIP_HEIGHT,
                viewbox: CARTRIDGE_GRIP_HEIGHT, shrink: 0.0, color: CARTRIDGE_TRIM,
                src: path!(
                    M 0 0 H 3 V 3 H 0 Z M 0 6 H 3 V 9 H 0 Z M 0 12 H 3 V 15 H 0 Z
                    M 0 18 H 3 V 21 H 0 Z
                )
            )
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
            bg_color: CARTRIDGE_RECESS,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            View (width: 3, height: 1, bg_color: CARTRIDGE_RIM)
        }
    }
}
