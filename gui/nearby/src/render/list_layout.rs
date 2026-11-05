use crate::model::{LibraryKind, Page, View};
use mirui::prelude::*;
use serde::Serialize;

pub(super) const WINDOW_BUDGET: usize = nearby_menu::LIST_WINDOW_ITEMS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LayoutRequest {
    event: &'static str,
    launcher: bool,
    rows: usize,
}

#[model]
pub(super) struct ListLayout {
    launcher: bool,
    active: bool,
    #[observe]
    pub rows: usize,
    sent: Option<usize>,
    pending: Option<LayoutRequest>,
}

impl ListLayout {
    pub fn new(launcher: bool) -> Self {
        Self {
            launcher,
            active: false,
            rows: 4,
            sent: None,
            pending: None,
        }
    }
    fn update(&mut self) {
        if self.active && Some(self.rows) != self.sent {
            let rows = self.rows;
            self.sent = Some(rows);
            self.pending = Some(LayoutRequest {
                event: "list_layout",
                launcher: self.launcher,
                rows,
            });
        }
    }
}

#[model]
impl ListLayout {
    pub fn present(&mut self, view: &View) {
        self.active = view.page == Page::Games
            && view.library.kind != LibraryKind::Favorites
            && view.launcher.enabled == self.launcher;
        if !self.active {
            self.pending = None;
            self.sent = None;
        }
        self.update();
    }
    pub fn resize(&mut self, height: Fixed, minimum: i32, gap: i32) {
        if height <= Fixed::ZERO {
            return;
        }
        self.rows =
            ((height.to_int() + gap) / (minimum + gap)).clamp(1, WINDOW_BUDGET as i32) as usize;
        self.update();
    }
    #[effects]
    fn take_layout(&mut self) -> Option<LayoutRequest> {
        self.pending.take()
    }
}
