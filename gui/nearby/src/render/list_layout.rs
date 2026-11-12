use crate::model::{LibraryKind, Page, View};
use howff_menu::input::Generation;
use mirui::prelude::*;
use serde::Serialize;

pub(super) const WINDOW_BUDGET: usize = howff_menu::LIST_WINDOW_ITEMS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LayoutRequest {
    #[serde(skip)]
    pub generation: Generation,
    event: &'static str,
    launcher: bool,
    rows: usize,
}

#[model]
pub(super) struct ListLayout {
    input_generation: Generation,
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
            input_generation: Generation::default(),
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
                generation: self.input_generation,
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
        if self.input_generation != view.input_generation {
            self.sent = None;
            self.pending = None;
            self.input_generation = view.input_generation;
        }
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

#[cfg(test)]
mod input_contract {
    use super::*;

    #[test]
    fn layout_measurements_are_reissued_for_the_new_input_owner() {
        let mut model = ListLayout::new(false);
        let mut view = View {
            input_generation: Generation::FIRST,
            ..Default::default()
        };
        view.page = Page::Games;
        view.library.kind = LibraryKind::Directory;
        model.present(&view);
        let queued = model.take_layout().unwrap();
        view.input_generation.advance();
        model.present(&view);
        let current = model.take_layout().unwrap();
        assert_eq!(queued.generation, Generation::FIRST);
        assert_eq!(current.generation, view.input_generation);
    }
}
