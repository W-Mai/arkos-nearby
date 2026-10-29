use mirui::prelude::*;
use mirui::ui::widgets::{Image, Text, TextAlign::Center};

use super::{
    artwork::cover,
    consoles::console,
    lists::{game_rows, games},
    pixel::Scenery,
    rows::Row,
    selection::FocusTarget,
    state::DisplayState,
    style::*,
    widgets::{caption, paragraph, shadow},
};
use crate::model::LibraryKind;

const KEYS: [&str; 40] = [
    "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "A", "S", "D", "F", "G", "H", "J", "K", "L",
    "退格", "Z", "X", "C", "V", "B", "N", "M", "空格", "清空", "结果", "1", "2", "3", "4", "5",
    "6", "7", "8", "9", "0",
];

const CARTRIDGE_EDGE: i32 = 2;
const CARTRIDGE_RAIL: i32 = 2;
const CARTRIDGE_CAP: i32 = 4;
const CARTRIDGE_CONTACTS: usize = 8;

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
                    games (state, metrics)
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
            bg_color: PANEL,
            padding: Padding::all(super::shelf::INSET),
            row_gap: 4
        ) [
            Scenery::Ridge,
        ] {
            if ${ state.entries().is_empty() } {
                console (super::consoles::Platform::Favorites)
            } else {
                Column (
                    grow: 1.0,
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
                                state
                                    .entries()
                                    .entries()
                                    .skip(line * grid.plan().grid.columns)
                                    .take(grid.plan().grid.columns)
                                    .collect::<Vec<_>>()
                            } with item by item.0 {
                                cartridge (state, item.1, item.0, grid, metrics)
                            }
                        }
                    }
                }
            }
            Row (height: 4, shrink: 0.0, column_gap: 4, justify: JustifyContent::Center) {
                walk ${
                    let (first, end, selected) = grid.pages();
                    (first..end).map(|i| (i, i == selected)).collect::<Vec<_>>()
                } with page by page.0 {
                    View (
                        width: if page.1 { 12 } else { 4 },
                        bg_color: if page.1 { ACCENT } else { DIVIDER }
                    )
                }
            }
        } on SizeChanged { grid.set_size(event.new.width, event.new.height); }
    }
}

#[compose]
fn cartridge(
    state: model!(DisplayState),
    row: model!(Row),
    member: mirui::core::model::ModelItemId,
    grid: model!(super::shelf::Shelf),
    metrics: Metrics,
) -> Entity {
    let movement = row.clone();
    let focus = row.clone();
    let directory = row.clone();
    let name = row.clone();
    let enabled = row.clone();
    let selected = row.clone();
    ui! {
        Column (
            width: ${ grid.plan().card_width },
            shrink: 0.0,
            min_width: 0,
            min_height: 0,
            transform: ${ state.card_motion().card_transform(movement.index()) }
        ) {
            Column (
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                padding: Padding::all(super::shelf::CARD_INSET),
                container: true,
                bg_color: ${ state.card_motion().card_color(focus.index()) }
            ) [
                FocusTarget::row(row.clone()),
            ] {
                if ${ directory.directory() } {
                    library_icon (metrics)
                } else {
                    cartridge_content (state.clone(), row, member, metrics)
                }
                Row (
                    height: super::shelf::LABEL_HEIGHT,
                    shrink: 0.0,
                    min_width: 0,
                    container: true,
                    padding: Padding {
                        right: super::shelf::LABEL_ACTOR_SPACE.into(),
                        ..Default::default()
                    }
                ) {
                    Text (
                        text: ${ name.label(true) },
                        grow: 1.0,
                        min_width: 0,
                        font: font(BODY_SIZE),
                        font_size: BODY_SIZE,
                        text_color: ${ if enabled.available() { FOREGROUND } else { DISABLED } },
                        paragraph: paragraph(mirui::ui::widgets::TextAlign::Start, 1)
                    ) [
                        FONT_PAINT,
                    ]
                    if ${ !state.launcher().view().enabled && state.selected() == Some(selected.index()) } {
                        View (
                            position: Position::Absolute,
                            left: Dimension::percent(100),
                            top: Dimension::percent(100),
                            width: 32,
                            height: 48,
                            transform: ${
                                mirui::types::Transform::translate(Fixed::from_int(-24), Fixed::from_int(-36))
                                    .compose(&mirui::types::Transform::scale(Fixed::HALF, Fixed::HALF))
                            }
                        ) [
                            super::pixel::ActorSprite {
                                state: state.clone(),
                            },
                        ]
                    }
                }
            }
            View (height: super::shelf::SHADOW, shrink: 0.0, bg_color: DIVIDER)
        }
    }
}

#[compose]
fn search(state: model!(DisplayState), metrics: Metrics) -> Entity {
    let query_state = state.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Column (
                height: i32::from(metrics.title) + LABEL_LEADING + metrics.gap * 2,
                shrink: 0.0,
                padding: Padding::all(metrics.gap),
                bg_color: PANEL,
                border_width: 1,
                border_color: ACCENT
            ) {
                caption (
                    move || {
                        let query = query_state.query();
                        if query.is_empty() { "输入游戏名或文件名".into() } else { query }
                    },
                    || FOREGROUND,
                    metrics.title,
                    Center,
                    LABEL_LEADING
                )
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
                game_rows (state, metrics)
            }
        }
    }
}

#[compose]
fn keycap(state: model!(DisplayState), index: usize, metrics: Metrics) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            transform: ${ state.card_motion().card_transform(100 + index) }
        ) {
            Column (
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                justify: JustifyContent::Center,
                bg_color: ${ if state.selected() == Some(100 + index) { ACCENT } else { PANEL } }
            ) [
                FocusTarget::fixed(100 + index),
            ] {
                caption (
                    move || KEYS[index].into(),
                    move || if state.selected() == Some(100 + index) { ON_ACCENT } else { FOREGROUND },
                    if KEYS[index].len() == 1 { metrics.title } else { metrics.body },
                    Center,
                    LABEL_LEADING
                )
            }
            shadow (metrics)
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
    metrics: Metrics,
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
                cartridge_source (row, metrics)
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
fn cartridge_source(row: model!(Row), _metrics: Metrics) -> Entity {
    let platform = row.clone();
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: 0) {
            Column (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                bg_color: DIVIDER,
                padding: Padding {
                    left: CARTRIDGE_RAIL.into(),
                    right: CARTRIDGE_RAIL.into(),
                    ..Default::default()
                },
                border_width: CARTRIDGE_EDGE,
                border_color: DEEP
            ) {
                Row (
                    height: CARTRIDGE_CAP,
                    shrink: 0.0,
                    align: AlignItems::Center,
                    column_gap: CARTRIDGE_RAIL
                ) {
                    cartridge_rivet ()
                    Column (grow: 1.0, row_gap: 2) {
                        View (height: 1, bg_color: PANEL)
                        View (height: 1, bg_color: SECONDARY)
                    }
                    cartridge_rivet ()
                }
                Row (grow: 1.0, min_height: 0, min_width: 0, column_gap: CARTRIDGE_RAIL * 2) {
                    cartridge_grip ()
                    Column (
                        grow: 1.0,
                        min_height: 0,
                        min_width: 0,
                        bg_color: DEEP,
                        padding: Padding::all(CARTRIDGE_EDGE)
                    ) {
                        cover (
                            row.cover_bounds(),
                            move || platform.platform()
                        )
                    }
                    cartridge_grip ()
                }
                Row (
                    height: CARTRIDGE_CAP,
                    shrink: 0.0,
                    justify: JustifyContent::Center,
                    align: AlignItems::FlexEnd
                ) {
                    Row (
                        width: Dimension::percent(64),
                        height: 4,
                        padding: Padding {
                            left: CARTRIDGE_RAIL.into(),
                            right: CARTRIDGE_RAIL.into(),
                            ..Default::default()
                        },
                        column_gap: CARTRIDGE_EDGE,
                        bg_color: DEEP
                    ) {
                        walk 0..CARTRIDGE_CONTACTS with _index {
                            Column (grow: 1.0, min_width: 0, bg_color: WARNING) {
                                View (height: CARTRIDGE_EDGE, bg_color: CLAY)
                            }
                        }
                    }
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
                Column (height: 3, shrink: 0.0, bg_color: SECONDARY) {
                    View (height: 1, bg_color: PANEL)
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
            bg_color: SECONDARY,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            View (width: 3, height: 1, bg_color: PANEL)
        }
    }
}
