//! Serialize input with the provenance captured by its Model.

use crate::model::{Action, LibraryKind, Page, View};
use howff_menu::input::{Generation, Sourced};
use serde::Serialize;
use std::io::{self, Write};

pub(crate) fn send<T: Serialize>(input: Sourced<T>) -> Result<(), String> {
    write(&mut io::stdout().lock(), input)
}

fn write<T: Serialize>(output: &mut impl Write, input: Sourced<T>) -> Result<(), String> {
    serde_json::to_writer(&mut *output, &input).map_err(|error| error.to_string())?;
    writeln!(output).map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}

/// A may already be on the wire while the input owner still displays the card.
#[derive(Default)]
pub(crate) struct GameMenuOpening(Option<Opening>);

struct Opening {
    origin: Generation,
    key: String,
    tab: usize,
    library: LibraryKind,
    back: bool,
}

impl Opening {
    fn same_game(&self, view: &View) -> bool {
        view.page == Page::Games
            && view.launcher.enabled
            && view.launcher.tab == self.tab
            && view.library.kind == self.library
            && view.launcher.key == self.key
    }
}

impl GameMenuOpening {
    fn card(view: &View) -> bool {
        view.page == Page::Games
            && view.launcher.enabled
            && !view.launcher.key.is_empty()
            && view.launcher.overlay.is_empty()
            && !view.launcher.navigation
            && !view.library.keyboard
            && !view.busy
    }

    /// Keep normal input on its own generation. If the confirm retires that
    /// generation, the same game's newly presented menu can consume the return.
    pub(crate) fn observe(&mut self, view: &View, action: Action) {
        if action == Action::Back
            && let Some(opening) = &mut self.0
            && opening.origin == view.input_generation
            && opening.same_game(view)
            && Self::card(view)
        {
            opening.back = true;
            return;
        }
        self.0 = (action == Action::Confirm && Self::card(view)).then(|| Opening {
            origin: view.input_generation,
            key: view.launcher.key.clone(),
            tab: view.launcher.tab,
            library: view.library.kind,
            back: false,
        });
    }

    pub(crate) fn presented(&mut self, view: &View) -> Option<Sourced<Action>> {
        let opening = self.0.as_ref()?;
        if view.input_generation == opening.origin && opening.same_game(view) && Self::card(view) {
            return None;
        }
        let opening = self.0.take()?;
        let mut next = opening.origin;
        next.advance();
        (opening.back
            && next == view.input_generation
            && opening.same_game(view)
            && view.launcher.overlay_kind == "game"
            && !view.launcher.overlay.is_empty())
        .then(|| next.bind(Action::Back))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn early_back_reverses_only_the_requested_game_menu_once() {
        let mut card = View {
            page: Page::Games,
            input_generation: Generation::FIRST,
            ..Default::default()
        };
        card.launcher.enabled = true;
        card.launcher.key = "rom".into();
        let mut menu = card.clone();
        menu.input_generation.advance();
        menu.launcher.overlay_kind = "game".into();
        menu.launcher.overlay = "操作".into();
        let mut input = GameMenuOpening::default();

        input.observe(&card, Action::Back);
        for mismatch in [
            "same",
            "key",
            "menu",
            "generation",
            "tab",
            "library",
            "other-key",
            "retry",
        ] {
            input.observe(&card, Action::Confirm);
            input.observe(&card, Action::Back);
            input.observe(&card, Action::Back);
            assert!(input.presented(&card).is_none());
            let mut arrived = menu.clone();
            match mismatch {
                "key" => arrived.launcher.key = "another".into(),
                "menu" => arrived.launcher.overlay_kind = "system".into(),
                "generation" => arrived.input_generation.advance(),
                "tab" => arrived.launcher.tab += 1,
                "library" => arrived.library.kind = LibraryKind::Search,
                "other-key" => input.observe(&card, Action::Down),
                "retry" => input.observe(&card, Action::Confirm),
                _ => {}
            }
            assert_eq!(
                input.presented(&arrived),
                (mismatch == "same").then(|| menu.input_generation.bind(Action::Back))
            );
            assert!(input.presented(&menu).is_none());
            input.observe(&menu, Action::Back);
        }
        input.observe(&card, Action::Confirm);
        let mut other = card.clone();
        other.launcher.key = "another".into();
        assert!(input.presented(&other).is_none());
        input.observe(&card, Action::Back);
    }

    #[test]
    fn serialized_actions_keep_the_model_origin_and_one_message_boundary() {
        let mut output = Vec::new();
        let input = Generation::FIRST.bind(crate::model::Action::Start);
        write(&mut output, input).unwrap();
        let decoded: Sourced<crate::model::Action> = serde_json::from_slice(&output).unwrap();
        assert_eq!(decoded.generation, Generation::FIRST);
        assert_eq!(decoded.payload, crate::model::Action::Start);
        assert_eq!(output.iter().filter(|byte| **byte == b'\n').count(), 1);
    }
}
