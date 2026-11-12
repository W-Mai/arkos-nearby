use crate::model::{LibraryKind, View};
use howff_menu::input::Generation;
use mirui::anim::{Tween, TweenConfig, ease};
use mirui::prelude::*;
use serde::Serialize;

pub(super) const WINDOW_BUDGET: usize = howff_menu::shelf::MAX_WINDOW_ITEMS;
pub(super) const INSET: i32 = 12;
pub(super) const GAP: i32 = 12;
pub(super) const CARD_INSET: i32 = 2;
pub(super) const LABEL_HEIGHT: i32 = 30;
pub(super) const LABEL_ACTOR_SPACE: i32 = 24;
pub(super) const SHADOW: i32 = 2;
const MIN_CARD_WIDTH: i32 = 160;
const MAX_CARD_WIDTH: i32 = 280;
const IMAGE_CHROME_WIDTH: i32 = 28;
const IMAGE_CHROME_HEIGHT: i32 = 52;
const PAGE_MARK_HEIGHT: i32 = 4;
const PAGE_MARK_GAP: i32 = 4;
pub(super) const PAGE_MARKERS: usize = 7;
const SHORT_MARK: Fixed = Fixed::from_int(6);
const LONG_MARK: Fixed = Fixed::from_int(22);
const PAGE_MOTION: TweenConfig = TweenConfig::new(260, &ease::OUT_CUBIC);

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
        let minimum_height = (MIN_CARD_WIDTH - IMAGE_CHROME_WIDTH) * 3 / 4 + IMAGE_CHROME_HEIGHT;
        let row_limit = ((available_height + GAP) / (minimum_height + GAP)).max(1) as usize;
        let column_limit = ((available_width + GAP) / (MIN_CARD_WIDTH + GAP)).max(1) as usize;
        for rows in 1..=row_limit.min(WINDOW_BUDGET) {
            for columns in 1..=column_limit.min(WINDOW_BUDGET) {
                if rows * columns > WINDOW_BUDGET || rows > 1 && (rows - 1) * columns >= total {
                    continue;
                }
                let row_height = (available_height - GAP * (rows as i32 - 1)) / rows as i32;
                let height_limit = (row_height - IMAGE_CHROME_HEIGHT) * 4 / 3 + IMAGE_CHROME_WIDTH;
                let card_width = ((available_width - GAP * (columns as i32 - 1)) / columns as i32)
                    .min(MAX_CARD_WIDTH)
                    .min(height_limit);
                if card_width < MIN_CARD_WIDTH {
                    continue;
                }
                let card_height = (card_width - IMAGE_CHROME_WIDTH) * 3 / 4 + IMAGE_CHROME_HEIGHT;
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
    markers: [MarkerMotion; PAGE_MARKERS],
    now: u64,
}

#[derive(Clone, Copy)]
struct MarkerMotion {
    page: usize,
    tween: Tween,
    started: u64,
}

impl MarkerMotion {
    fn value(self, now: u64) -> Fixed {
        let mut tween = self.tween;
        tween.tick(
            now.saturating_sub(self.started)
                .min(u64::from(tween.duration_ms)) as u16,
        );
        tween.value()
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
            markers: [MarkerMotion {
                page: usize::MAX,
                tween: Tween::preset(SHORT_MARK, SHORT_MARK, PAGE_MOTION),
                started: 0,
            }; PAGE_MARKERS],
            now: 0,
        }
    }

    fn update(&mut self) {
        if !self.active || self.size.0 <= 0 || self.size.1 <= 0 {
            return;
        }
        self.plan = Plan::fit(self.size.0, self.size.1, self.total);
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
                    .unwrap_or(MarkerMotion {
                        page,
                        tween: Tween::preset(SHORT_MARK, SHORT_MARK, PAGE_MOTION),
                        started: self.now,
                    });
                if motion.tween.to != target {
                    motion.tween = Tween::preset(motion.value(self.now), target, PAGE_MOTION);
                    motion.started = self.now;
                }
            }
        }
    }
}

#[model]
impl Shelf {
    pub fn tick(&mut self, now: u64) {
        self.now = now;
        self.marker_lengths = self.markers.map(|marker| marker.value(now));
    }

    pub fn markers_settled(&self) -> bool {
        self.markers.iter().all(|marker| {
            self.now.saturating_sub(marker.started) >= u64::from(marker.tween.duration_ms)
        })
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
        self.active = active;
        self.total = view.library.total.max(view.entries.len());
        self.position = view
            .library
            .position
            .max(view.selected.saturating_add(1))
            .min(self.total);
        self.update();
    }

    pub fn set_size(&mut self, width: Fixed, height: Fixed) {
        let size = (width.to_int(), height.to_int());
        if self.size != size {
            self.size = size;
            self.layout_pending = true;
        }
    }

    pub fn layout(&mut self) {
        if self.layout_pending {
            self.layout_pending = false;
            self.update();
        }
    }

    #[effects]
    fn take_layout(&mut self) -> Option<LayoutRequest> {
        self.pending.take()
    }
}

impl ShelfHandle {
    pub fn marker_width(&self, page: usize) -> Fixed {
        self.marker_lengths()
            .get(page.saturating_sub(self.pages().0))
            .copied()
            .unwrap_or(SHORT_MARK)
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
                        assert!(
                            ((plan.card_width - IMAGE_CHROME_WIDTH) * 3
                                - (plan.card_height - IMAGE_CHROME_HEIGHT) * 4)
                                .abs()
                                < 4
                        );
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
        shelf.update();
        shelf.tick(300);
        assert_eq!(shelf.marker_lengths[0], LONG_MARK);
        shelf.position = shelf.plan.grid.capacity() + 1;
        shelf.update();
        shelf.tick(430);
        let previous = shelf.marker_lengths;
        assert!(previous[0] > SHORT_MARK && previous[0] < LONG_MARK);
        assert!(previous[1] > SHORT_MARK && previous[1] < LONG_MARK);
        shelf.position = 1;
        shelf.update();
        shelf.tick(430);
        assert_eq!(shelf.marker_lengths, previous);
        shelf.tick(800);
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
