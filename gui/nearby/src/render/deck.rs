use crate::model::View;
use mirui::anim::{Tween, TweenConfig, ease};
use mirui::prelude::*;

pub(super) const FULL: Fixed = Fixed::from_ratio(13, 16);
const MINIATURE: Fixed = Fixed::from_ratio(36, 100);
const SLIDE: TweenConfig = TweenConfig::new(540, &ease::IN_OUT_CUBIC);
const MAX_ROOM_STEP_MS: u64 = 24;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Layer {
    #[default]
    Shelf,
    Catalog,
    Room,
    Actions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RoomEntry {
    Charge,
    Action(usize),
}

#[component]
pub(super) enum ActorScope {
    Reading,
    ManagementReading,
    Layer(Layer),
    Pane(crate::model::Page),
}

#[component]
pub(super) struct ActorAnchor(pub super::pixel::Stage);

#[component]
pub(super) struct ActionAnchor(pub usize, pub ActionLayout);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionLayout {
    Grid,
    Levels,
    Details,
    Configuration,
    Management,
    ManagementProgress,
    SavesList,
    SavesModal,
    SavesProgress,
    SaveCopy,
    WifiList,
    WifiOptions,
    WifiKeyboard,
    WifiProgress,
    SearchKeyboard,
}

struct Track {
    tween: Tween,
    at: u64,
    target: Fixed,
}
impl Track {
    fn new(config: TweenConfig) -> Self {
        Self {
            tween: Tween::preset(Fixed::ZERO, Fixed::ZERO, config),
            at: 0,
            target: Fixed::ZERO,
        }
    }
    fn value(&self, now: u64) -> Fixed {
        let mut tween = self.tween;
        tween.tick(
            now.saturating_sub(self.at)
                .min(u64::from(tween.duration_ms)) as u16,
        );
        tween.value()
    }
    fn set(&mut self, target: bool, now: u64, config: TweenConfig) {
        let target = if target { Fixed::ONE } else { Fixed::ZERO };
        if self.target != target {
            self.tween = Tween::preset(self.value(now), target, config);
            self.at = now;
            self.target = target;
        }
    }
}

#[model]
pub(super) struct Deck {
    #[observe]
    pub layer: Layer,
    #[observe]
    pub actor_stage: bool,
    #[observe]
    pub enabled: bool,
    #[observe]
    pub expansion: Fixed,
    #[observe]
    pub panel: Fixed,
    #[observe]
    pub panel_visible: bool,
    #[observe]
    pub room_entry: Option<RoomEntry>,
    room_origin: Option<Rect>,
    #[observe]
    pub height: Fixed,
    slide: Track,
    modal: crate::launcher::popup::Track,
    now: u64,
    room_now: u64,
    launcher_entry: RoomEntry,
    pause_background: bool,
    slide_clock: super::modal::AnimationClock,
    slide_now: u64,
}
impl Default for Deck {
    fn default() -> Self {
        Self {
            layer: Layer::Shelf,
            actor_stage: true,
            enabled: false,
            expansion: Fixed::ZERO,
            panel: Fixed::ZERO,
            panel_visible: false,
            room_entry: None,
            room_origin: None,
            height: Fixed::ZERO,
            slide: Track::new(SLIDE),
            modal: crate::launcher::popup::Track::visibility(),
            now: 0,
            room_now: 0,
            launcher_entry: RoomEntry::Charge,
            pause_background: false,
            slide_clock: super::modal::AnimationClock::default(),
            slide_now: 0,
        }
    }
}
#[model]
impl Deck {
    #[observe]
    pub fn viewport_settled(&self) -> bool {
        self.expansion == self.slide.target
    }

    pub fn set_height(&mut self, height: Fixed) {
        self.height = height;
    }
    pub fn present(&mut self, view: &View) {
        self.layer = if view.return_to_launcher || !self.enabled && !view.launcher.enabled {
            Layer::Room
        } else if !view.launcher.overlay.is_empty() {
            Layer::Actions
        } else if view.launcher.tab == 1 {
            Layer::Catalog
        } else {
            Layer::Shelf
        };
        if view.launcher.enabled {
            self.enabled = true;
            self.launcher_entry = if view.launcher.overlay_kind == "game" {
                view.launcher
                    .items
                    .iter()
                    .position(|item| item.action == crate::model::MenuAction::Multiplayer)
                    .map_or(RoomEntry::Charge, RoomEntry::Action)
            } else {
                RoomEntry::Charge
            };
            self.slide
                .set(view.launcher.tab == 1, self.slide_now, SLIDE);
        }
        if view.return_to_launcher && view.current_game && self.enabled && !self.panel_visible {
            self.room_entry = Some(self.launcher_entry);
            self.room_origin = None;
        }
        self.modal.show(view.return_to_launcher);
        self.panel_visible = self.modal.visible();
    }
    pub fn tick(&mut self, now: u64) {
        let elapsed = now.saturating_sub(self.now);
        self.now = now;
        if let Some(local) = self.slide_clock.sample(now, self.pause_background) {
            self.slide_now = local;
            self.expansion = self.slide.value(local);
        }
        self.room_now = if self.room_entry.is_some() {
            self.room_now + elapsed.min(MAX_ROOM_STEP_MS)
        } else {
            now
        };
        self.panel = self.modal.tick(self.room_now);
        self.panel_visible = self.modal.visible();
        if !self.panel_visible {
            self.room_entry = None;
            self.room_origin = None;
        }
    }
    pub fn pin_room_origin(&mut self, origin: Rect) {
        if self.room_entry.is_some() && self.room_origin.is_none() {
            self.room_origin = Some(origin);
        }
    }
    pub fn room_origin(&self) -> Option<Rect> {
        self.room_origin
    }
    pub fn pause_background(&mut self, paused: bool) {
        self.pause_background = paused;
    }
    pub fn stage(&mut self, value: bool) {
        self.actor_stage = value;
    }
}

pub(super) fn catalog_share(expansion: Fixed) -> Fixed {
    FULL * expansion
}
pub(super) fn shelf_share(expansion: Fixed) -> Fixed {
    Fixed::ONE - catalog_share(expansion)
}
pub(super) fn transform(expansion: Fixed, height: Fixed) -> mirui::types::Transform {
    let scale = MINIATURE + (Fixed::ONE - MINIATURE) * expansion;
    mirui::types::Transform::translate(Fixed::ZERO, (scale - Fixed::ONE) * height / 2)
        .compose(&mirui::types::Transform::scale(scale, scale))
}

pub(super) fn shelf_transform(expansion: Fixed, height: Fixed) -> mirui::types::Transform {
    let scale = shelf_share(expansion);
    mirui::types::Transform::translate(Fixed::ZERO, (scale - Fixed::ONE) * height / 2)
        .compose(&mirui::types::Transform::scale(scale, scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_reversal_preserves_position_and_modal_progress_is_independent() {
        let mut deck = Deck::default();
        let mut view = View::default();
        view.launcher.enabled = true;
        deck.present(&view);
        view.launcher.tab = 1;
        deck.present(&view);
        deck.tick(200);
        let middle = deck.expansion;
        assert!(middle > Fixed::ZERO && middle < Fixed::ONE);
        view.launcher.tab = 0;
        deck.present(&view);
        deck.tick(200);
        assert_eq!(deck.expansion, middle);
        view.return_to_launcher = true;
        view.launcher.enabled = false;
        deck.present(&view);
        deck.tick(200);
        deck.tick(800);
        assert_eq!(deck.expansion, Fixed::ZERO);
        assert_eq!(deck.panel, Fixed::ONE);
        assert!(deck.layer == Layer::Room);
        for step in 0..=16 {
            let expansion = Fixed::from_ratio(step, 16);
            assert_eq!(
                shelf_share(expansion) + catalog_share(expansion),
                Fixed::ONE
            );
        }
        assert_eq!(catalog_share(Fixed::ZERO), Fixed::ZERO);
        assert_eq!(shelf_share(Fixed::ZERO), Fixed::ONE);
        assert_eq!(shelf_share(Fixed::ONE), Fixed::from_ratio(3, 16));
    }
}
