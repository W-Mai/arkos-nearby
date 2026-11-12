use super::deferred::Pane;
use super::{
    deck::{ActionAnchor, ActionLayout},
    keyboard::keycap,
    style::*,
    widgets::{control_key, control_label, shadow, text_block},
};
use crate::launcher::management::{Manager, Page};
use crate::launcher::text_entry::{Mode, TextEntry};
use mirui::prelude::*;
use mirui::ui::widgets::TextAlign::{Center, Start};
use mirui::ui::widgets::page_view::{CreatePage, PageView};
use mirui::ui::widgets::{Image, Text, TextEditing};

#[compose]
pub(super) fn panel(state: model!(Manager), metrics: Metrics) -> Entity {
    ui! {
        PageView (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            page: Pane::Management
        ) on CreatePage(create_panel::callback(state, metrics))
    }
}

#[event]
fn create_panel(
    _event: event!(CreatePage<Pane>),
    state: model!(Manager),
    metrics: Metrics,
) -> Entity {
    ui! {
        panel_contents (state, metrics)
    }
}

#[compose]
pub(super) fn panel_contents(state: model!(Manager), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0) {
            if ${ state.progressing() } {
                progress (state, metrics)
            } else {
                management_body (state, metrics)
            }
        }
    }
}
#[compose]
fn management_body(state: model!(Manager), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0) {
            if ${ state.page() == Page::Detail } {
                detail (state, metrics)
            } else {
                regular_panel (state, metrics)
            }
        }
    }
}
#[compose]
fn regular_panel(state: model!(Manager), metrics: Metrics) -> Entity {
    let message = state.clone();
    let summary = state.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
            if ${ state.editing() } {
                name_editor (state, state.text(), metrics)
            } else {
                Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
                    if ${ !state.summary().is_empty() } {
                        Row (
                            height: Dimension::Content,
                            min_height: i32::from(TITLE_SIZE) + metrics.gap * 2,
                            shrink: 0.0,
                            min_width: 0,
                            column_gap: metrics.gap,
                            align: AlignItems::Center,
                            padding: Padding::all(metrics.gap),
                            bg_color: PANEL
                        ) {
                            Image (
                                width: 28,
                                height: 38,
                                shrink: 0.0,
                                src: path!(
                                    M 2 0 H 14 V 20 H 2 Z M 4 2 V 10 H 12 V 2 Z M 5 12 H 7 V 17 H 5 Z M 3 14 H 9 V 16
                                    H 3 Z M 11 13 H 13 V 15 H 11 Z
                                ),
                                color: ACCENT
                            )
                            Column (
                                grow: 1.0,
                                min_width: 0,
                                height: Dimension::Content,
                                row_gap: metrics.gap / 2
                            ) {
                                walk ${ summary.summary().lines().map(str::to_owned).collect::<Vec<_>>() } with line by line.clone() {
                                    text_block (
                                        ${ line.clone() },
                                        FOREGROUND,
                                        if metrics.compact { BODY_SIZE } else { TITLE_SIZE },
                                        Start,
                                        0,
                                        2
                                    )
                                }
                            }
                        }
                    }
                    if ${ state.page() == Page::Friend } {
                        Column (shrink: 0.0, min_width: 0, row_gap: metrics.gap / 2) {
                            walk 0_usize..2 with index {
                                friend_fact (state, index)
                            }
                        }
                    }
                    Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap) {
                        walk ${ state.window() } with index by index {
                            row (state, index, metrics)
                        }
                    } on SizeChanged { state.viewport(event.new.height, metrics.management_row_height(), metrics.gap); }
                }
            }
            if ${ !state.notice().is_empty() } {
                text_block (${ message.notice() }, WARNING, BODY_SIZE, Start, 0, 2)
            }
        }
    }
}

#[compose]
fn row(state: model!(Manager), index: usize, metrics: Metrics) -> Entity {
    let title = state.clone();
    let detail_color = state.clone();
    let detail = state.clone();
    let fill = state.clone();
    let color = state.clone();
    let motion = state.clone();
    ui! {
        Column (
            height: metrics.management_row_height(),
            shrink: 0.0,
            min_width: 0,
            transform: ${
                mirui::types::Transform::translate(
                    Fixed::ZERO,
                    Fixed::from_int(
                        if motion.selected() == index {
                            super::motion::hop(motion.age(), 2.0)
                        } else {
                            0
                        },
                    ),
                )
            }
        ) {
            Row (
                grow: 1.0,
                min_height: 0,
                min_width: 0,
                column_gap: metrics.gap,
                padding: Padding {
                    left: metrics.gap.into(),
                    right: 20.into(),
                    ..Default::default()
                },
                align: AlignItems::Center,
                bg_color: ${
                    if fill.selected() == index && fill.row(index).is_some_and(|r| r.enabled) {
                        ACCENT
                    } else {
                        PANEL
                    }
                },
                border_width: 1,
                border_color: ${ if state.selected() == index { FOREGROUND } else { DIVIDER } }
            ) [
                ActionAnchor(index, ActionLayout::Management),
            ] {
                View (width: 3, height: Dimension::percent(70), shrink: 0.0, bg_color: WARNING)
                Column (width: 20, height: 24, shrink: 0.0) {
                    walk ${
                        std::iter::once((
                            icon_kind(&color, index),
                            color.selected() == index && color.row(index).is_some_and(|r| r.enabled),
                        ))
                    } with icon by icon {
                        Image (
                            width: 20,
                            height: 24,
                            shrink: 0.0,
                            src: match icon.0 {
                                Icon::Pen => path!(M 2 11 L 11 2 L 15 6 L 6 15 H 2 Z M 1 16 H 15 V 18 H 1 Z),
                                Icon::People => {
                                    path!(
                                        M 2 1 H 7 V 7 H 2 Z M 0 9 H 9 V 16 H 0 Z M 10 4 H 15 V 10 H 10 Z M 11 12
                                        H 16 V 16 H 11 Z
                                    )
                                }
                                Icon::Info => {
                                    path!(M 1 1 H 15 V 15 H 1 Z M 7 4 V 6 H 9 V 4 Z M 7 8 V 12 H 9 V 8 Z)
                                }
                                Icon::Search => {
                                    path!(
                                        M 1 1 H 11 V 11 H 1 Z M 3 3 V 9 H 9 V 3 Z M 10 10 H 13 V 13 H 16 V 16 H
                                        13 V 13 H 10 Z
                                    )
                                }
                                Icon::Package => {
                                    path!(
                                        M 1 4 L 8 0 L 15 4 V 13 L 8 17 L 1 13 Z M 3 5 V 12 L 7 14 V 7 Z M 9 7 V
                                        14 L 13 12 V 5 Z
                                    )
                                }
                                Icon::Home => {
                                    path!(M 0 8 L 8 0 L 16 8 H 13 V 16 H 3 V 8 Z M 7 10 V 16 H 10 V 10 Z)
                                }
                                Icon::Back => path!(M 7 1 L 0 8 L 7 15 V 10 H 16 V 6 H 7 Z),
                            },
                            color: if icon.1 { ON_ACCENT } else { ACCENT }
                        )
                    }
                }
                Column (grow: 1.0, min_height: 0, min_width: 0, justify: JustifyContent::Center) {
                    text_block (
                        ${ title.row(index).map(|r| r.title).unwrap_or_default() },
                        ${
                            {
                                if state.selected() == index && state.row(index).is_some_and(|r| r.enabled) {
                                    ON_ACCENT
                                } else {
                                    FOREGROUND
                                }
                            }
                        },
                        TITLE_SIZE,
                        Start,
                        0,
                        1
                    )
                    text_block (
                        ${ detail.row(index).map(|r| r.detail).unwrap_or_default() },
                        ${
                            {
                                if detail_color.selected() == index
                                    && detail_color.row(index).is_some_and(|r| r.enabled)
                                {
                                    ON_ACCENT
                                } else {
                                    SECONDARY
                                }
                            }
                        },
                        BODY_SIZE,
                        Start,
                        0,
                        2
                    )
                }
            }
            shadow (metrics)
        }
    }
}

#[compose]
fn name_editor(manager: model!(Manager), entry: model!(TextEntry), metrics: Metrics) -> Entity {
    ui! {
        PageView (
            grow: 1.0,
            min_width: 0,
            min_height: 0,
            page: Pane::NameEditor
        ) on CreatePage(create_name_editor::callback(manager, entry, metrics))
    }
}

#[event]
fn create_name_editor(
    _event: event!(CreatePage<Pane>),
    manager: model!(Manager),
    entry: model!(TextEntry),
    metrics: Metrics,
) -> Entity {
    ui! {
        name_editor_contents (manager, entry, metrics)
    }
}

#[compose]
fn name_editor_contents(
    manager: model!(Manager),
    entry: model!(TextEntry),
    metrics: Metrics,
) -> Entity {
    let value = entry.clone();
    let mode = entry.clone();
    let composing = entry.clone();
    let feedback = entry.clone();
    let error_color = entry.clone();
    let document = entry.document();
    let editor = TextEditing::new(document);
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: metrics.gap / 2) {
            Row (
                height: 32,
                shrink: 0.0,
                min_width: 0,
                padding: Padding::all(3),
                bg_color: PANEL,
                border_width: 2,
                border_color: ACCENT
            ) [
                crate::launcher::text_input::InputEditor,
                editor,
            ] {
                Text (
                    text: ${ value.value() },
                    grow: 1.0,
                    min_width: 0,
                    font: font(TITLE_SIZE),
                    font_size: TITLE_SIZE,
                    text_color: FOREGROUND,
                    paragraph: super::widgets::paragraph(Start, 1)
                ) [
                    FONT_PAINT,
                ]
            } on TextEditing::Submitted { manager.submit_name(); }
            Row (height: 16, shrink: 0.0, min_width: 0, column_gap: metrics.gap) {
                text_block (
                    ${
                        {
                            match mode.mode() {
                                Mode::Pinyin => "拼音",
                                Mode::Latin => "英文",
                                Mode::Symbols => "符号",
                            }
                                .to_owned()
                        }
                    },
                    ACCENT,
                    BODY_SIZE,
                    Start,
                    0,
                    1
                )
                text_block (
                    ${
                        {
                            if !feedback.error().is_empty() {
                                feedback.error()
                            } else if composing.preedit().is_empty() {
                                "↑ 选词 · L/R 翻页".into()
                            } else {
                                composing.preedit()
                            }
                        }
                    },
                    ${ if error_color.error().is_empty() { SECONDARY } else { WARNING } },
                    BODY_SIZE,
                    Start,
                    0,
                    1
                )
            }
            Row (height: 30, shrink: 0.0, column_gap: metrics.gap / 2, min_width: 0) {
                walk ${ 0..entry.page_size().min(entry.candidate_count()) } with index by index {
                    candidate (entry, index, manager, metrics)
                }
            }
            Column (grow: 1.0, min_height: 0, min_width: 0, row_gap: metrics.gap / 2) {
                walk 0_usize..3 with line {
                    Row (grow: 1.0, min_height: 0, min_width: 0, column_gap: metrics.gap / 2) {
                        walk 0_usize..10 with column {
                            name_key (entry, manager, line * 10 + column, metrics)
                        }
                    }
                }
                Row (grow: 1.0, min_height: 0, min_width: 0, column_gap: metrics.gap / 2) {
                    walk 30_usize..33 with index {
                        name_key (entry, manager, index, metrics)
                    }
                }
            }
        } on SizeChanged { entry.layout_width(event.new.width); }
    }
}
#[compose]
fn name_key(
    entry: model!(TextEntry),
    manager: model!(Manager),
    index: usize,
    metrics: Metrics,
) -> Entity {
    let label = entry.clone();
    let focus = entry.clone();
    let motion = entry.clone();
    ui! {
        keycap (
            ${ label.key_label(index) },
            ${ focus.selected() == index },
            ${
                mirui::types::Transform::translate(
                    Fixed::ZERO,
                    Fixed::from_int(
                        if motion.selected() == index {
                            super::motion::hop(manager.age(), 2.0)
                        } else {
                            0
                        },
                    ),
                )
            },
            ActionAnchor(index, ActionLayout::Management),
            if metrics.compact || index >= 27 { BODY_SIZE } else { TITLE_SIZE },
            metrics
        )
    }
}
#[compose]
fn candidate(
    entry: model!(TextEntry),
    index: usize,
    manager: model!(Manager),
    metrics: Metrics,
) -> Entity {
    let label = entry.clone();
    let focus = entry.clone();
    let motion = entry.clone();
    ui! {
        keycap (
            ${ label.candidate_label(index) },
            ${ focus.selected() == 100 + index },
            ${
                mirui::types::Transform::translate(
                    Fixed::ZERO,
                    Fixed::from_int(
                        if motion.selected() == 100 + index {
                            super::motion::hop(manager.age(), 2.0)
                        } else {
                            0
                        },
                    ),
                )
            },
            ActionAnchor(100 + index, ActionLayout::Management),
            TITLE_SIZE,
            metrics
        )
    }
}
#[compose]
pub(super) fn hints(state: model!(Manager), metrics: Metrics) -> Entity {
    ui! {
        Row (grow: 1.0, min_width: 0, column_gap: metrics.gap, align: AlignItems::Center) {
            if ${ !(state.page() == Page::Progress && state.busy()) } {
                Row (column_gap: metrics.gap, align: AlignItems::Center) {
                    if ${ !state.busy() } {
                        Row (column_gap: metrics.gap, align: AlignItems::Center) {
                            control_key ("A", metrics, true)
                            control_label ("确认", metrics)
                        }
                    }
                    control_key ("B", metrics, false)
                    control_label ("返回", metrics)
                    if ${ state.page() == Page::Rename } {
                        Row (column_gap: metrics.gap, align: AlignItems::Center) {
                            control_key ("Y", metrics, false)
                            control_label ("删除", metrics)
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Icon {
    Pen,
    People,
    Info,
    Search,
    Package,
    Home,
    Back,
}
fn icon_kind(state: &crate::launcher::management::ManagerHandle, index: usize) -> Icon {
    let Some(row) = state.row(index) else {
        return Icon::Info;
    };
    match row.key.as_str() {
        "rename" => Icon::Pen,
        "version" => Icon::Info,
        "diagnostics" => Icon::Search,
        "packages" | "install" => Icon::Package,
        "startup" | "home" | "original" | "nearby" | "standalone" => Icon::Home,
        "cancel" => Icon::Back,
        "forget" => Icon::People,
        _ if row.key.starts_with("fact-") => Icon::Info,
        _ => Icon::People,
    }
}
#[compose]
fn friend_fact(state: model!(Manager), index: usize) -> Entity {
    let label = state.clone();
    let value = state.clone();
    ui! {
        Row (shrink: 0.0, min_width: 0, column_gap: 8, align: AlignItems::FlexStart) {
            Column (width: 60, shrink: 0.0) {
                text_block (
                    ${ label.friend_field(index).map(|f| f.label).unwrap_or_default() },
                    SECONDARY,
                    BODY_SIZE,
                    Start,
                    0,
                    1
                )
            }
            Column (grow: 1.0, min_width: 0) {
                text_block (
                    ${ value.friend_field(index).map(|f| f.value).unwrap_or_default() },
                    FOREGROUND,
                    BODY_SIZE,
                    Start,
                    0,
                    3
                )
            }
        }
    }
}

#[compose]
fn progress(state: model!(Manager), metrics: Metrics) -> Entity {
    let label = state.clone();
    let message = state.clone();
    let start = state.clone();
    let end = state.clone();
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            justify: JustifyContent::Center,
            row_gap: metrics.gap * 3,
            bg_color: PANEL,
            padding: Padding::all(metrics.gap * 2)
        ) {
            text_block (${ label.progress_label() }, FOREGROUND, TITLE_SIZE, Center, 0, 2)
            if ${ !state.notice().is_empty() } {
                text_block (${ message.notice() }, WARNING, BODY_SIZE, Center, 0, 3)
            }
            if ${ state.busy() } {
                View (height: 12, min_width: 0, bg_color: DIVIDER, clip_children: false) {
                    View (
                        position: Position::Absolute,
                        width: Dimension::percent(100),
                        height: 12,
                        bg_color: ACCENT,
                        transform: ${
                            mirui::types::Transform::translate(
                                    (start.loading_start()
                                        - (Fixed::ONE - (end.loading_end() - end.loading_start())) * Fixed::HALF)
                                        * start.track(),
                                    Fixed::ZERO,
                                )
                                .compose(
                                    &mirui::types::Transform::scale(
                                        end.loading_end() - end.loading_start(),
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
                            ActionAnchor(0, ActionLayout::ManagementProgress),
                        ]
                    }
                } on SizeChanged { state.track_width(event.new.width); }
            } else {
                row (state, 0, metrics)
            }
        }
    }
}
#[component]
struct ManagementScroll {
    state: model!(Manager),
}
#[system(order=Animation)]
fn scroll_detail(
    nodes: query!(
        Entity,
        ManagementScroll,
        mirui::input::event::scroll::ScrollOffset,
        mirui::input::event::scroll::ScrollBounds
    ),
    commands: commands!(),
) {
    for (entity, model, offset, bounds) in nodes {
        if !model.state.active() || model.state.page() != Page::Detail {
            continue;
        }
        model.state.detail_bounds(bounds.max_y);
        let y = model.state.detail_scroll();
        if offset.y != y {
            commands.set::<super::scroll::Position>(entity, y);
        }
    }
}
#[view(component=ManagementScroll,systems=[scroll_detail::system()])]
pub(super) fn management_scroll() {}

#[compose]
fn detail(state: model!(Manager), metrics: Metrics) -> Entity {
    let title = state.clone();
    let value = state.clone();
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            text_block (${ title.summary() }, ACCENT, TITLE_SIZE, Start, 0, 2)
            Column (grow: 1.0, min_height: 0, clip_children: true) [
                super::deck::ActorScope::ManagementReading,
                mirui::input::event::scroll::ScrollConfig {
                    elastic: false,
                    ..Default::default()
                },
                mirui::input::event::scroll::ScrollOffset::default(),
                ManagementScroll {
                    state: state.clone(),
                },
            ] {
                Column (height: Dimension::Content, min_width: 0, shrink: 0.0) {
                    text_block (
                        ${ value.detail_value() },
                        FOREGROUND,
                        TITLE_SIZE,
                        Start,
                        0,
                        0
                    )
                }
            }
        }
    }
}
