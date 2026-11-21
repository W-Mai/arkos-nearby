use super::artwork::{preview, room_preview};
use super::choices::choice;
use super::consoles::{ConsolePreview, console};
use super::discovery::{self, caption as discovery_caption};
use super::focus_surface::{Cue, paint_shadow};
use super::library::search_icon;
use super::pixel::Stage::Search;
use super::reading::title_strip;
use super::selection::FocusTarget;
use super::widgets::hold_badge;
use super::{
    Section,
    pixel::{self, Scenery, art},
    rows::Row,
    state::DisplayState,
    style::*,
    widgets::{caption, shadow, text_block},
};
use mirui::anim::{SNAPPY, Spring};
use mirui::prelude::*;
use mirui::types::Transform;
use mirui::ui::widgets::Image;
use mirui::ui::widgets::TextAlign::{Center, Start};

// Sheet contents keep their layout coordinates while the viewports move and clip.
const DECK_PEEK: Fixed = Fixed::from_ratio(14, 100);
const DECK_FULL: Fixed = Fixed::from_ratio(86, 100);
const DECK_MIN_SCALE: Fixed = Fixed::from_ratio(44, 100);
const ROW_ACTOR_SHARE: i32 = 18;

fn sheet_scale(expansion: Fixed) -> Fixed {
    DECK_MIN_SCALE + (Fixed::ONE - DECK_MIN_SCALE) * expansion
}

fn sheet_transform(expansion: Fixed, height: Fixed) -> mirui::types::Transform {
    let scale = sheet_scale(expansion);
    // The widget transform pivots around its center; keep its painted top at zero.
    mirui::types::Transform::translate(Fixed::ZERO, (scale - Fixed::ONE) * height * DECK_FULL / 2)
        .compose(&mirui::types::Transform::scale(scale, scale))
}

fn sheet_share(expansion: Fixed) -> Fixed {
    DECK_PEEK + (DECK_FULL - DECK_PEEK) * expansion
}

#[compose]
pub(super) fn lobby(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (id: "discovery_deck", grow: 1.0, min_height: 0, container: true) on SizeChanged {
            state.set_deck_height(event.new.height);
        } {
            walk [false, true].into_iter() with actions {
                Column (
                    position: Position::Absolute,
                    width: Dimension::percent(100),
                    top: 0,
                    height: ${ Dimension::Percent(sheet_share(if actions { state.deck() } else { Fixed::ONE - state.deck() }) * Fixed::from_int(100)) },
                    transform: ${ mirui::types::Transform::translate(Fixed::ZERO, if actions { state.deck_height() * sheet_share(Fixed::ONE - state.deck()) } else { Fixed::ZERO }) },
                    shrink: 0.0,
                    container: true,
                    min_height: 0,
                    clip_children: true
                ) {
                    Column (
                        position: Position::Absolute,
                        width: Dimension::percent(100),
                        height: @(id("discovery_deck").height as deck_height) { deck_height * DECK_FULL },
                        top: 0,
                        transform: ${ sheet_transform(if actions { state.deck() } else { Fixed::ONE - state.deck() }, state.deck_height()) },
                        shrink: 0.0,
                        row_gap: metrics.gap,
                        padding: Padding::all(metrics.gap),
                    ) [
                        super::cache::SettledLayerCache { state: state.clone(), backdrop: false },
                        super::deck::ActorScope::Pane(if actions { crate::model::Page::Mode } else { crate::model::Page::Lobby }),
                    ] {
                        if actions {
                            choice (super::state::PageKey { page: crate::model::Page::Mode, network_only: false, library: crate::model::LibraryKind::Directory }, state, metrics)
                        } else {
                            discovery_status (state, metrics)
                            rooms (state, metrics)
                        }
                    }
                }
            }
        }
    }
}

#[compose]
fn row_signal(state: model!(DisplayState), row: model!(Row), metrics: Metrics) -> Entity {
    ui! {
        View (width: 30, height: if metrics.compact { 26 } else { 30 }, shrink: 0.0) {
            View (grow: 1.0, width: Dimension::percent(100)) [
                pixel::SignalSprite {
                    state: state.clone(),
                    row: row.clone(),
                },
            ]
        }
    }
}

#[compose]
pub(super) fn rooms(state: model!(DisplayState), metrics: Metrics) -> Entity {
    let slots = Signal::new(super::state::ROOM_WINDOW);
    let measured_slots = slots.clone();
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, container: true, clip_children: true) [
            Scenery::Ridge,
        ] {
            Column (
                position: Position::Absolute,
                top: 0,
                left: 0,
                height: Dimension::percent(100),
                width: Dimension::percent(100),
                min_height: 0,
                bg_color: PANEL
            ) [
                discovery::Layer::Beacon,
            ] {
                View (height: Dimension::percent(50), shrink: 0.0)
                Column (
                    grow: 1.0,
                    min_height: 0,
                    padding: Padding {
                        left: metrics.inset.into(),
                        right: metrics.inset.into(),
                        ..Default::default()
                    },
                    row_gap: metrics.gap
                ) {
                    art (state, Search, if metrics.compact { 40 } else { 72 })
                    caption (
                        "请好友在另一台掌机上创建房间",
                        SECONDARY,
                        metrics.small,
                        Center,
                        LABEL_LEADING
                    )
                    View (grow: 1.0, min_height: 0)
                }
                shadow (metrics)
            }
            Column (
                grow: 1.0,
                min_width: 0,
                min_height: 0
            ) [
                discovery::Layer::Results,
            ] {
                View (height: discovery::HEADER_HEIGHT, shrink: 0.0)
                Row (grow: 1.0, min_height: 0, column_gap: metrics.gap) {
                    Column (grow: 3.0, min_width: 0, min_height: 0, row_gap: metrics.gap) [
                        Section::FocusList,
                    ] {
                        walk ${
                            let rooms = state.rooms();
                            let limit = slots.get();
                            let first = rooms.get(0).map_or(0, |row| row.index());
                            let cursor = state.room_cursor().saturating_sub(first);
                            let offset = cursor
                                .saturating_add(1)
                                .saturating_sub(limit)
                                .min(rooms.len().saturating_sub(limit));
                            rooms.entries().skip(offset).take(limit)
                        } with item by item.0 {
                            list_row (state, item.1, metrics, true)
                        }
                    } on SizeChanged {
                        let capacity = ((event.new.height.to_int() + metrics.gap)
                            / (metrics.row_min_height() + metrics.gap))
                            .clamp(1, super::state::ROOM_WINDOW as i32) as usize;
                        if measured_slots.get_untracked() != capacity {
                            measured_slots.set(capacity);
                        }
                    }
                    if ${ state.room_art() } {
                        Column (grow: 2.0, min_width: 0, min_height: 0) {
                            room_preview (state)
                            text_block (
                                ${ state.room_game() },
                                FOREGROUND,
                                metrics.title,
                                Center,
                                LABEL_LEADING,
                                2
                            )
                            shadow (metrics)
                        }
                    }
                }
            }
            discovery_caption (state, metrics)
        }
    }
}

#[compose]
pub(super) fn games(state: model!(DisplayState), metrics: Metrics, platforms: bool) -> Entity {
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, row_gap: metrics.gap) {
            Row (grow: 1.0, min_height: 0, column_gap: metrics.gap) [
                Scenery::Ridge,
            ] {
                Column (grow: 59.0, min_width: 0, min_height: 0) {
                    game_rows (state, metrics)
                }
                Column (
                    grow: 41.0,
                    min_width: 0,
                    min_height: 0,
                    row_gap: metrics.gap
                ) {
                    if platforms {
                        Column (
                            id: "platform_previews",
                            grow: 1.0,
                            min_height: 0,
                            bg_color: PANEL,
                            clip_children: true
                        ) {
                            View (grow: 1.0, min_height: 0, min_width: 0) [
                                ConsolePreview { state: state.clone() },
                            ]
                        }
                    } else {
                        preview (state, metrics)
                    }
                }
            }
            Row (
                height: metrics.preview_title_height(),
                align: AlignItems::Center,
                shrink: 0.0,
                column_gap: metrics.gap,
                bg_color: BACKGROUND
            ) {
                View (width: 4, height: 18, shrink: 0.0, bg_color: ACCENT)
                title_strip (state, platforms)
                if ${ state.shows_netplay_hint() && !platforms } {
                    hold_badge (false)
                }
            }
        }
    }
}

#[compose]
pub(super) fn game_rows(state: model!(DisplayState), metrics: Metrics) -> Entity {
    let empty = state.clone();
    ui! {
        Column (
            grow: 1.0,
            min_height: 0,
            row_gap: metrics.gap
        ) [
            Section::FocusList,
        ] {
            if ${ state.entries().is_empty() } {
                Column (grow: 1.0, min_height: 0) {
                    Column (
                        grow: 1.0,
                        bg_color: PANEL,
                        justify: JustifyContent::Center,
                        align: AlignItems::Center,
                        row_gap: metrics.gap * 2,
                        padding: Padding::all(metrics.inset)
                    ) {
                        Row (
                            height: 48,
                            width: 96,
                            shrink: 0.0,
                            column_gap: metrics.gap,
                            align: AlignItems::Center,
                            justify: JustifyContent::Center
                        ) {
                            search_icon (32)
                            View (width: 32, height: 48, shrink: 0.0) [
                                super::deck::ActorAnchor(Search),
                            ]
                        }
                        caption (
                            ${ empty.library_placeholder().title().to_owned() },
                            SECONDARY,
                            metrics.body,
                            Center,
                            LABEL_LEADING
                        )
                        Row (height: 4, column_gap: metrics.gap, justify: JustifyContent::Center) {
                            walk 0..3 with index {
                                View (
                                    width: 4,
                                    height: 4,
                                    bg_color: ${
                                        if matches!(
                                            state.library_placeholder(), super::state::LibraryPlaceholder::Searching |
                                            super::state::LibraryPlaceholder::Loading |
                                            super::state::LibraryPlaceholder::Preparing
                                        ) {
                                            state.activity()[index]
                                        } else {
                                            Color::rgba(0, 0, 0, 0)
                                        }
                                    }
                                )
                            }
                        }
                    }
                    shadow (metrics)
                }
            } else {
                Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
                    walk ${
                        let rows = state.list_layout().rows();
                        let first = state
                            .library_selection()
                            .saturating_sub(rows - 1)
                            .min(state.entries().len().saturating_sub(rows));
                        state.entries().entries().skip(first).take(rows).collect::<Vec<_>>()
                    } with item by item.0 {
                        list_row (state, item.1, metrics, false)
                    }
                }
            }
        } on SizeChanged {
            state
                .list_layout()
                .resize(event.new.height, metrics.library_row_min_height(), metrics.gap);
        }
    }
}

#[compose]
fn row_label(row: model!(Row), size: u16, title: bool, lines: u16) -> Entity {
    let content = row.clone();
    let colors = row;
    ui! {
        super::widgets::text_block (
            ${ content.label(title) },
            ${ {
                if !colors.available() {
                    DISABLED
                } else if title {
                    FOREGROUND
                } else {
                    SECONDARY
                }
            } },
            size,
            Start,
            ROW_LEADING,
            lines
        )
    }
}

#[compose]
fn platform_icon(row: model!(Row)) -> Entity {
    ui! {
        View (width: 24, height: 24, shrink: 0.0) [
            super::cache::GpuLayerCache,
        ] {
            walk ${ std::iter::once(row.platform()) } with platform by platform {
                console (platform)
            }
        }
    }
}

#[compose]
fn list_row(
    state: model!(DisplayState),
    item: model!(Row),
    metrics: Metrics,
    has_signal: bool,
) -> Entity {
    let color_index = item.clone();
    let face_state = state.clone();
    let face_index = item.clone();
    let shadow_state = state.clone();
    let shadow_item = item.clone();
    let accent_index = item.clone();
    let available = item.clone();
    let marker = item.clone();
    let favorite = item.clone();
    ui! {
        Column (
            grow: 1.0,
            min_height: if has_signal { metrics.row_min_height() } else { metrics.library_row_min_height() },
            max_height: if has_signal {
                metrics.row_height()
            } else {
                metrics.library_row_height().max(metrics.library_row_min_height())
            },
            min_width: 0
        ) {
            Row (
                grow: 1.0,
                min_height: 0,
                padding: metrics.row_padding(),
                column_gap: metrics.gap,
                transform: ${ if has_signal {
                    Transform::IDENTITY
                } else {
                    Transform::translate(Fixed::ZERO, -face_state.surface_focus().at(face_index.index()) * (if metrics.compact { 2 } else { 3 }))
                } },
                bg_color: ${ if state.selected() == Some(color_index.index()) {
                    ACCENT_DIM
                } else {
                    PANEL
                } },
                align: AlignItems::Center
            ) [
                if has_signal {
                    FocusTarget::row(item.clone()).scoped(crate::model::Page::Rooms)
                } else {
                    FocusTarget::row(item.clone())
                },
            ] {
                View (
                    width: metrics.rail_width(),
                    height: Dimension::percent(100),
                    shrink: 0.0,
                    bg_color: ${
                        if available.available() && state.selected() == Some(accent_index.index()) {
                            ACCENT
                        } else {
                            DIVIDER
                        }
                    }
                )
                if !has_signal {
                    platform_icon (marker)
                }
                Column (grow: 1.0, min_width: 0, justify: JustifyContent::Center) {
                    row_label (
                        item.clone(),
                        if has_signal { metrics.row_title } else { TITLE_SIZE },
                        true,
                        1
                    )
                    if ${
                        has_signal || state.platforms() || item.directory() || !item.available()
                            || state.page_key().page == crate::model::Page::Candidates
                            || state.page_key().library == crate::model::LibraryKind::Search
                    } {
                        row_label (item.clone(), metrics.row_detail, false, 1)
                    }
                }
                if ${ !has_signal && favorite.favorite() } {
                    Image (
                        src: path!(
                            M 8 0 H 12 V 4 H 20 V 8 H 16 V 12 H 18 V 20 H 12 V 16 H 8 V 20 H 2 V 12 H 4 V 8 H
                            0 V 4 H 8 Z
                        ),
                        viewbox: 20,
                        width: 12,
                        height: 12,
                        shrink: 0.0,
                        color: WARNING
                    )
                }
                View (width: Dimension::percent(ROW_ACTOR_SHARE), min_width: 32, shrink: 0.0)
                if has_signal {
                    row_signal (state, item, metrics)
                }
            }
            View (height: metrics.shadow_height(), shrink: 0.0) [
                ListingShadow { state: shadow_state, row: shadow_item, room: has_signal },
            ]
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RiseFrame {
    current: Option<(usize, Fixed)>,
    previous: Option<(usize, Fixed)>,
}

impl RiseFrame {
    pub(super) fn at(self, index: usize) -> Fixed {
        [self.current, self.previous]
            .into_iter()
            .flatten()
            .find_map(|(row, rise)| (row == index).then_some(rise))
            .unwrap_or_default()
            .clamp(Fixed::ZERO, Fixed::ONE)
    }
}

#[derive(Default)]
pub(super) struct RowRise {
    current: Option<(usize, Spring)>,
    previous: Option<(usize, Spring)>,
}

impl RowRise {
    pub(super) fn select(&mut self, index: Option<usize>, raised: bool) {
        let target = if raised { Fixed::ONE } else { Fixed::ZERO };
        if self.current.map(|(row, _)| row) == index {
            if let Some((_, motion)) = &mut self.current {
                motion.retarget(target, None);
            }
            return;
        }
        if index.is_some() && self.previous.map(|(row, _)| row) == index {
            std::mem::swap(&mut self.current, &mut self.previous);
            if let Some((_, motion)) = &mut self.current {
                motion.retarget(target, None);
            }
        } else {
            let initial = if self.current.is_none() && self.previous.is_none() {
                target
            } else {
                Fixed::ZERO
            };
            self.previous = self.current.take();
            self.current = index.map(|row| (row, Spring::preset(initial, target, SNAPPY)));
        }
        if let Some((_, motion)) = &mut self.previous {
            motion.retarget(Fixed::ZERO, None);
        }
    }

    pub(super) fn advance(&mut self, delta: u16) {
        if let Some((_, motion)) = &mut self.current {
            motion.tick(delta);
        }
        if let Some((_, motion)) = &mut self.previous {
            motion.tick(delta);
            if motion.is_settled() {
                self.previous = None;
            }
        }
    }

    pub(super) fn frame(&self) -> RiseFrame {
        RiseFrame {
            current: self.current.map(|(row, motion)| (row, motion.value())),
            previous: self.previous.map(|(row, motion)| (row, motion.value())),
        }
    }
}

#[component]
struct ListingShadow {
    state: model!(DisplayState),
    row: model!(Row),
    room: bool,
}

#[view(component = ListingShadow, invalidation = Visual)]
pub(super) fn listing_shadow(
    listing: component!(),
    state: model!(component.state, watch(surface_focus)),
    row: model!(component.row, watch(index, available)),
    theme: res!(mirui::ui::theme::Theme),
    paint: paint!(),
) {
    let cue = if listing.room { Cue::Row } else { Cue::Depth };
    let rise = if !listing.room || row.available {
        state.surface_focus().at(row.index)
    } else {
        Fixed::ZERO
    };
    paint_shadow(paint, &theme, cue, rise);
}

#[compose]
fn discovery_status(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        caption (${ state.status() }, SECONDARY, metrics.small, Start, LABEL_LEADING)
    }
}
