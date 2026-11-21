use super::static_snapshot::{Picture, Tile, turn::Stamp};
use super::{cartridge, rows::Row};
use crate::model::{LibraryKind, Page, View};
use howff_menu::input::Generation;
use mirui::anim::{Spring, SpringConfig};
use mirui::core::model::{ModelList, OwnedModel};
use mirui::prelude::*;
use serde::Serialize;

pub(super) const WINDOW_BUDGET: usize = howff_menu::shelf::MAX_WINDOW_ITEMS;
pub(super) const INSET: i32 = 6;
pub(super) const GAP: i32 = 6;
pub(super) const CARD_INSET: i32 = 2;
pub(super) const LABEL_HEIGHT: i32 = 30;
pub(super) const SHADOW: i32 = 5;
const MIN_CARD_WIDTH: i32 = 160;
const MAX_CARD_WIDTH: i32 = 304;
const IMAGE_CHROME_WIDTH: i32 = 20;
const IMAGE_CHROME_HEIGHT: i32 = 52;
pub(super) const PAGE_MARK_HEIGHT: i32 = 4;
const PAGE_MARK_GAP: i32 = 4;
pub(super) const PAGE_MARKERS: usize = 7;
pub(super) const SHORT_MARK: Fixed = Fixed::from_int(6);
const LONG_MARK: Fixed = Fixed::from_int(22);
const FIRST_SLIDE_STEP_MS: u64 = 24;
const SPRING_STEP_MS: u16 = 16;
const PAGE_MOTION: SpringConfig = SpringConfig {
    duration_ms: 175,
    bounce: Fixed::from_ratio(1, 16),
};
const MARKER_MOTION: SpringConfig = SpringConfig {
    duration_ms: 250,
    bounce: Fixed::from_ratio(1, 8),
};

pub use howff_menu::shelf::Grid;

#[component]
pub(super) struct PageMarkers;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LayoutRequest {
    #[serde(skip)]
    pub generation: Generation,
    event: &'static str,
    launcher: bool,
    grid: Grid,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Plan {
    pub grid: Grid,
    pub card_width: i32,
    pub card_height: i32,
}

impl Plan {
    fn fit(width: i32, height: i32, total: usize) -> Self {
        let available_width = (width - INSET * 2).max(1);
        let available_height = (height - INSET * 2 - PAGE_MARK_HEIGHT - PAGE_MARK_GAP).max(1);
        let mut best = Self {
            grid: Grid {
                columns: 1,
                rows: 1,
            },
            card_width: available_width.min(MAX_CARD_WIDTH),
            card_height: available_height,
        };
        let mut score = 0;
        let minimum_height = (MIN_CARD_WIDTH - IMAGE_CHROME_WIDTH) * 2 / 3 + IMAGE_CHROME_HEIGHT;
        let row_limit = ((available_height + GAP) / (minimum_height + GAP)).max(1) as usize;
        let column_limit = ((available_width + GAP) / (MIN_CARD_WIDTH + GAP)).max(1) as usize;
        for rows in 1..=row_limit.min(WINDOW_BUDGET) {
            for columns in 1..=column_limit.min(WINDOW_BUDGET) {
                if rows * columns > WINDOW_BUDGET || rows > 1 && (rows - 1) * columns >= total {
                    continue;
                }
                let row_height = (available_height - GAP * (rows as i32 - 1)) / rows as i32;
                // Fill the row with landscape artwork in a 4:3 to 3:2 window.
                let height_limit = (row_height - IMAGE_CHROME_HEIGHT) * 3 / 2 + IMAGE_CHROME_WIDTH;
                let card_width = ((available_width - GAP * (columns as i32 - 1)) / columns as i32)
                    .min(MAX_CARD_WIDTH)
                    .min(height_limit);
                if card_width < MIN_CARD_WIDTH {
                    continue;
                }
                let card_height =
                    row_height.min((card_width - IMAGE_CHROME_WIDTH) * 3 / 4 + IMAGE_CHROME_HEIGHT);
                let candidate =
                    (rows * columns).min(total.max(1)) as i32 * 1000 + card_width - rows as i32;
                if candidate > score {
                    score = candidate;
                    best = Self {
                        grid: Grid { columns, rows },
                        card_width,
                        card_height,
                    };
                }
            }
        }
        best
    }

    pub fn width(self) -> i32 {
        self.card_width * self.grid.columns as i32
            + GAP * self.grid.columns.saturating_sub(1) as i32
    }
}

#[model]
pub(super) struct Shelf {
    input_generation: Generation,
    launcher: bool,
    active: bool,
    size: (i32, i32),
    layout_pending: bool,
    total: usize,
    position: usize,
    #[observe]
    pub plan: Plan,
    sent: Option<Grid>,
    pending: Option<LayoutRequest>,
    #[observe]
    pub pages: (usize, usize, usize),
    #[observe]
    pub marker_lengths: [Fixed; PAGE_MARKERS],
    pub departing: model!(owned ModelList<Row>),
    #[observe]
    pub departing_count: usize,
    pub departure_image: model!(owned Picture),
    prepared_turn: Option<(Stamp, Tile)>,
    #[observe]
    pub offsets: (Fixed, Fixed),
    slide: Option<Slide>,
    markers: [MarkerMotion; PAGE_MARKERS],
    last_tick: u64,
    cartridges: cartridge::Cartridges,
    #[observe]
    pub poses: [cartridge::Pose; WINDOW_BUDGET],
    #[observe]
    pub departing_poses: [cartridge::Pose; WINDOW_BUDGET],
}

#[derive(Clone, Copy)]
struct Slide {
    spring: Spring,
    distance: Fixed,
    origin: usize,
    start: SlideStart,
}

#[derive(Clone, Copy)]
enum SlideStart {
    AwaitingPaint,
    FirstStep,
    Running,
}

impl Slide {
    fn advance(&mut self, delta: u64) -> u64 {
        match self.start {
            // Discard time spent before the transition's first paint and limit
            // the next step so a cold mount cannot skip the crossing.
            SlideStart::AwaitingPaint => {
                self.start = SlideStart::FirstStep;
                0
            }
            SlideStart::FirstStep if delta > 0 => {
                self.start = SlideStart::Running;
                delta.min(FIRST_SLIDE_STEP_MS)
            }
            _ => delta,
        }
    }
}

fn advance_spring(spring: &mut Spring, mut delta: u16) {
    // A late frame still consumes its elapsed time through the same small steps.
    while delta > 0 && !spring.is_settled() {
        let step = delta.min(SPRING_STEP_MS);
        spring.tick(step);
        delta -= step;
    }
}

fn window(view: &View) -> Option<usize> {
    (view.page == Page::Games
        && view.library.kind == LibraryKind::Favorites
        && !view.busy
        && !view.entries.is_empty()
        && !view.launcher.navigation
        && view.launcher.tab == 0
        && view.launcher.overlay.is_empty())
    .then(|| {
        view.library
            .position
            .saturating_sub(view.selected.saturating_add(1))
    })
}

#[derive(Clone, Copy)]
struct MarkerMotion {
    page: usize,
    spring: Spring,
}

impl MarkerMotion {
    fn new(page: usize) -> Self {
        Self {
            page,
            spring: Spring::preset(SHORT_MARK, SHORT_MARK, MARKER_MOTION),
        }
    }
}

impl Shelf {
    pub fn new(launcher: bool) -> Self {
        Self {
            input_generation: Generation::default(),
            launcher,
            active: false,
            size: (0, 0),
            layout_pending: false,
            total: 0,
            position: 0,
            plan: Plan::default(),
            sent: None,
            pending: None,
            pages: (0, 0, 0),
            marker_lengths: [SHORT_MARK; PAGE_MARKERS],
            departing: ModelList::new().into(),
            departing_count: 0,
            departure_image: Picture::default().into(),
            prepared_turn: None,
            offsets: (Fixed::ZERO, Fixed::ZERO),
            slide: None,
            markers: [MarkerMotion::new(usize::MAX); PAGE_MARKERS],
            last_tick: 0,
            cartridges: Default::default(),
            poses: [cartridge::Pose::default(); WINDOW_BUDGET],
            departing_poses: [cartridge::Pose::default(); WINDOW_BUDGET],
        }
    }

    pub fn card_pose(&self, index: usize, departing: bool) -> cartridge::Pose {
        let poses = if departing {
            &self.departing_poses
        } else {
            &self.poses
        };
        pose_at(poses, index, departing)
    }

    fn finish_slide(&mut self) {
        if self.slide.is_none() {
            return;
        }
        self.slide = None;
        self.offsets = (Fixed::ZERO, Fixed::ZERO);
        self.departing_count = 0;
        self.departure_image.update(Picture::clear);
    }

    fn update(&mut self, fit: bool) {
        if !self.active || self.size.0 <= 0 || self.size.1 <= 0 {
            return;
        }
        if fit {
            self.plan = Plan::fit(self.size.0, self.size.1, self.total);
            // Reserve only the visible leaving page, once layout has measured its size.
            let capacity = self.plan.grid.capacity();
            if self.total > capacity && capacity > self.departing.capacity() {
                let additional = capacity - self.departing.len();
                let _ = self.departing.reserve(additional);
            }
        }
        if self.sent != Some(self.plan.grid) {
            self.sent = Some(self.plan.grid);
            self.pending = Some(LayoutRequest {
                generation: self.input_generation,
                event: "shelf_layout",
                launcher: self.launcher,
                grid: self.plan.grid,
            });
        }
        let capacity = self.plan.grid.columns * self.plan.grid.rows;
        let page = self.position.saturating_sub(1) / capacity;
        let count = self.total.div_ceil(capacity);
        let first = page.saturating_sub(3).min(count.saturating_sub(7));
        let pages = if count > 1 {
            (first, count.min(first + 7), page)
        } else {
            (0, 0, 0)
        };
        if self.pages != pages {
            let previous = self.markers;
            self.pages = pages;
            for (slot, motion) in self.markers.iter_mut().enumerate() {
                let page = pages.0 + slot;
                let target = if page == pages.2 {
                    LONG_MARK
                } else {
                    SHORT_MARK
                };
                *motion = previous
                    .iter()
                    .find(|old| old.page == page)
                    .copied()
                    .unwrap_or_else(|| MarkerMotion::new(page));
                motion.spring.retarget(target, None);
            }
        }
    }
}

#[model]
impl Shelf {
    pub fn change_window(&mut self, old: &View, new: &View, rows: &ModelList<Row>) {
        if old.library.sort != new.library.sort {
            self.cartridges = Default::default();
        }
        let Some((from, to)) = window(old).zip(window(new)) else {
            self.finish_slide();
            return;
        };
        let capacity = self.plan.grid.capacity();
        if !self.active
            || self.size.0 <= 0
            || self.layout_pending
            || capacity == 0
            || old.library.sort != new.library.sort
            || old.library.total != new.library.total
            || old.launcher.enabled != new.launcher.enabled
            || rows.is_empty()
            || old.entries.len() > self.departing.capacity()
            || from % capacity != 0
            || to % capacity != 0
        {
            self.finish_slide();
            return;
        }
        if from == to || old.entries == new.entries {
            return;
        }
        let stride = Fixed::from_int((self.size.0 - 2 * INSET).max(1));
        let distance = if to > from { stride } else { -stride };
        let (start, velocity) = match self.slide {
            Some(slide) if slide.origin == to => (self.offsets.1, slide.spring.velocity),
            _ => (distance, Fixed::ZERO),
        };
        if start == Fixed::ZERO && velocity == Fixed::ZERO {
            self.finish_slide();
            return;
        }
        let prepared = self.prepared_turn.take().and_then(|(stamp, tile)| {
            (self.slide.is_none() && stamp == self.turn_stamp(rows, old.entries.len()))
                .then_some(tile)
        });
        #[cfg(any(target_os = "linux", test))]
        super::response::stage(if prepared.is_some() {
            "shelf_turn_texture"
        } else {
            "shelf_turn_live"
        });
        if let Some(tile) = prepared {
            self.departing_count = 0;
            self.departure_image
                .update(|picture| picture.replace(Some(tile)));
        } else {
            self.departure_image.update(Picture::clear);
            let departing: Vec<_> = rows
                .entries()
                .zip(&old.entries)
                .map(|((_, row), data)| row.read(|row| row.departing(data)))
                .collect();
            self.departing_count = departing.len();
            super::rows::reconcile_shelf(&mut self.departing, departing, 0, true);
            self.departing_poses = self.poses;
        }
        self.slide = Some(Slide {
            origin: from,
            start: SlideStart::AwaitingPaint,
            distance,
            spring: Spring::preset(start, Fixed::ZERO, PAGE_MOTION).with_velocity(velocity),
        });
        self.offsets = (start, start - distance);
    }

    pub fn tick(&mut self, now: u64, reflect: bool) {
        let delta = now.saturating_sub(std::mem::replace(&mut self.last_tick, now));
        self.poses = self
            .cartridges
            .step(now, delta.min(48) as u16, reflect && self.active);
        let step = self
            .slide
            .as_mut()
            .map_or(delta, |slide| slide.advance(delta));
        let step = step.min(u64::from(u16::MAX)) as u16;
        if let Some(slide) = self.slide.as_mut() {
            advance_spring(&mut slide.spring, step);
            if slide.spring.is_settled() {
                self.finish_slide();
            } else {
                let x = slide.spring.value().round();
                self.offsets = (x, x - slide.distance);
            }
        }
        for marker in &mut self.markers {
            advance_spring(&mut marker.spring, step);
        }
        self.marker_lengths = self.markers.map(|marker| marker.spring.value());
    }

    pub fn sliding(&self) -> bool {
        self.slide.is_some()
    }

    pub fn markers_settled(&self) -> bool {
        self.slide.is_none() && self.markers.iter().all(|marker| marker.spring.is_settled())
    }
    pub fn present(&mut self, view: &View) {
        if self.input_generation != view.input_generation {
            self.sent = None;
            self.pending = None;
            self.input_generation = view.input_generation;
        }
        let active =
            view.library.kind == LibraryKind::Favorites && view.launcher.enabled == self.launcher;
        if self.active != active {
            self.sent = None;
            self.pending = None;
        }
        let total = view.library.total.max(view.entries.len());
        let fit = active != self.active || self.total != total;
        self.active = active;
        self.total = total;
        let position = view
            .library
            .position
            .max(view.selected.saturating_add(1))
            .min(self.total);
        if position != self.position {
            self.clear_prepared_turn();
        }
        self.position = position;
        if let Some(start) = window(view) {
            self.cartridges.set_count(view.entries.len());
            if self.cartridges.select(
                start,
                view.selected.min(view.entries.len() - 1),
                self.last_tick,
            ) {
                self.poses = self.cartridges.step(self.last_tick, 0, false);
            }
        }
        self.update(fit);
    }

    pub fn can_prepare_turn(&self) -> bool {
        self.active
            && !self.layout_pending
            && self.total > self.plan.grid.capacity()
            && self.slide.is_none()
            && self.markers_settled()
            && self.cartridges.settled()
            && self.poses.iter().all(|pose| pose.reflection().is_none())
    }

    pub fn turn_stamp(&self, rows: &ModelList<Row>, visible: usize) -> Stamp {
        Stamp::new(self.plan, self.position, &self.poses, rows, visible)
    }

    pub fn has_prepared_turn(&self) -> bool {
        self.prepared_turn.is_some()
    }

    pub fn prepare_turn(&mut self, stamp: Stamp, tile: Tile) {
        self.prepared_turn = Some((stamp, tile));
    }

    pub fn clear_prepared_turn(&mut self) {
        self.prepared_turn = None;
    }

    pub fn set_size(&mut self, width: Fixed, height: Fixed) {
        let size = (width.to_int(), height.to_int());
        if self.size != size {
            self.finish_slide();
            self.clear_prepared_turn();
            self.size = size;
            self.layout_pending = true;
        }
    }

    pub fn layout(&mut self) {
        if self.layout_pending {
            self.layout_pending = false;
            self.update(true);
        }
    }

    #[effects]
    fn take_layout(&mut self) -> Option<LayoutRequest> {
        self.pending.take()
    }
}

fn pose_at(
    poses: &[cartridge::Pose; WINDOW_BUDGET],
    index: usize,
    departing: bool,
) -> cartridge::Pose {
    poses
        .get(index)
        .copied()
        .unwrap_or_default()
        .without_reflection(departing)
}

impl ShelfHandle {
    pub fn pose(&self, index: usize, departing: bool) -> cartridge::Pose {
        let poses = if departing {
            self.departing_poses()
        } else {
            self.poses()
        };
        pose_at(&poses, index, departing)
    }

    pub fn marker_transform(&self, page: usize) -> mirui::types::Transform {
        use mirui::types::Transform;

        let (first, end, _) = self.pages();
        let lengths = self.marker_lengths();
        let visible = &lengths[..(end - first).min(PAGE_MARKERS)];
        let Some(before) = page.checked_sub(first) else {
            return Transform::IDENTITY;
        };
        let Some(width) = visible.get(before).copied() else {
            return Transform::IDENTITY;
        };
        let gap = Fixed::from_int(PAGE_MARK_GAP);
        let strip_width = visible
            .iter()
            .copied()
            .fold(Fixed::ZERO, |sum, length| sum + length)
            + gap * visible.len().saturating_sub(1) as i32;
        let start = visible[..before]
            .iter()
            .copied()
            .fold(Fixed::ZERO, |sum, length| sum + length)
            + gap * before as i32;
        let x = start - strip_width / 2 + (width - SHORT_MARK) / 2;
        Transform::translate(x, Fixed::ZERO)
            .compose(&Transform::scale(width / SHORT_MARK, Fixed::ONE))
    }
}

#[system(order = Animation)]
pub(super) fn layout_shelves(runtime: res!(super::state::DisplayRuntime)) {
    // Mount cards before layout events so their size observers can register normally.
    runtime.state.shelf().layout();
    runtime.shelf.shelf().layout();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grid_fits_available_bounds_and_retains_landscape_artwork() {
        for width in [424, 560, 720] {
            for height in (190..=1040).step_by(17) {
                for total in 1..=17 {
                    let plan = Plan::fit(width, height, total);
                    assert!(plan.grid.valid());
                    assert!(plan.width() <= width - INSET * 2);
                    assert!(
                        plan.card_height * plan.grid.rows as i32
                            + GAP * (plan.grid.rows as i32 - 1)
                            <= height - INSET * 2 - PAGE_MARK_HEIGHT - PAGE_MARK_GAP
                    );
                    if plan.card_width >= MIN_CARD_WIDTH {
                        let width = plan.card_width - IMAGE_CHROME_WIDTH;
                        let height = plan.card_height - IMAGE_CHROME_HEIGHT;
                        assert!(width * 3 >= height * 4 && width * 2 <= height * 3 + 2);
                    }
                }
            }
        }
        assert_eq!(
            Plan::fit(560, 370, 6).grid,
            Grid {
                columns: 3,
                rows: 2
            }
        );
        assert_eq!(Plan::fit(424, 205, 6).grid.rows, 1);
        assert!(Plan::fit(600, 650, 20).grid.rows >= 3);
        assert!(Plan::fit(424, 850, 20).grid.rows >= 4);
    }

    #[test]
    fn page_marker_motion_retargets_from_its_current_length() {
        let mut shelf = Shelf::new(false);
        shelf.active = true;
        shelf.size = (600, 370);
        shelf.total = 24;
        shelf.position = 1;
        shelf.update(true);
        shelf.tick(300, false);
        assert_eq!(shelf.marker_lengths[0], LONG_MARK);
        shelf.position = shelf.plan.grid.capacity() + 1;
        shelf.update(true);
        shelf.tick(430, false);
        let previous = shelf.marker_lengths;
        assert!(previous[0] > SHORT_MARK && previous[0] < LONG_MARK);
        assert!(previous[1] > SHORT_MARK && previous[1] < LONG_MARK);
        shelf.position = 1;
        shelf.update(true);
        shelf.tick(430, false);
        assert_eq!(shelf.marker_lengths, previous);
        shelf.tick(800, false);
        assert_eq!(shelf.marker_lengths[0], LONG_MARK);
        assert_eq!(shelf.marker_lengths[1], SHORT_MARK);
        assert!(shelf.markers_settled());
    }
}

#[cfg(test)]
mod input_contract {
    use super::*;

    #[test]
    fn layout_measurements_are_reissued_for_the_new_input_owner() {
        let mut model = Shelf::new(false);
        let mut view = View {
            input_generation: Generation::FIRST,
            ..Default::default()
        };
        view.library.kind = LibraryKind::Favorites;
        view.library.total = 8;
        model.set_size(Fixed::from_int(640), Fixed::from_int(320));
        model.present(&view);
        let queued = model.take_layout().unwrap();
        view.input_generation.advance();
        model.present(&view);
        let current = model.take_layout().unwrap();
        assert_eq!(queued.generation, Generation::FIRST);
        assert_eq!(current.generation, view.input_generation);
    }
}
