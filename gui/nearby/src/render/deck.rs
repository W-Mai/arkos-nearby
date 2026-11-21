use crate::model::View;
use mirui::anim::{Tween, TweenConfig, ease};
use mirui::prelude::*;

pub(super) const FULL: Fixed = Fixed::from_ratio(13, 16);
const MINIATURE: Fixed = Fixed::from_ratio(36, 100);
const SLIDE: TweenConfig = TweenConfig::new(540, &ease::IN_OUT_CUBIC);
const MAX_ROOM_STEP_MS: u64 = 24;
const MIN_PREPARATION_MS: u64 = 180;
const PREPARATION_SWEEP_OFFSET_MS: u64 = 350;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Layer {
    #[default]
    Shelf,
    Catalog,
    Room,
    Actions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoomPreparation {
    ClosingMenu,
    Loading(u64),
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
        super::motion::tween_at(self.tween, now.saturating_sub(self.at))
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
    pub game_room: bool,
    #[observe]
    preparation: Option<RoomPreparation>,
    room_origin: Option<Rect>,
    room_requested: bool,
    game_menu: bool,
    #[observe]
    pub height: Fixed,
    slide: Track,
    modal: crate::launcher::popup::Track,
    now: u64,
    room_now: u64,
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
            game_room: false,
            preparation: None,
            room_origin: None,
            room_requested: false,
            game_menu: false,
            height: Fixed::ZERO,
            slide: Track::new(SLIDE),
            modal: crate::launcher::popup::Track::visibility(),
            now: 0,
            room_now: 0,
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
            self.slide
                .set(view.launcher.tab == 1, self.slide_now, SLIDE);
        }
        let opening = view.return_to_launcher
            && view.current_game
            && self.enabled
            && !self.room_requested
            && !self.panel_visible;
        if self.game_menu && (view.preparing_netplay || opening) && self.preparation.is_none() {
            self.preparation = Some(RoomPreparation::ClosingMenu);
        } else if !view.preparing_netplay && !view.return_to_launcher {
            self.preparation = None;
        }
        self.game_menu = view.launcher.enabled && view.launcher.overlay_kind == "game";
        if opening {
            self.game_room = true;
            self.room_origin = None;
        }
        self.room_requested = view.return_to_launcher;
        self.modal
            .show(self.room_requested && self.preparation.is_none());
        self.panel_visible = self.modal.visible();
    }
    pub fn tick(&mut self, now: u64, menu_mounted: bool) {
        let elapsed = now.saturating_sub(self.now);
        self.now = now;
        if let Some(local) = self.slide_clock.sample(now, self.pause_background) {
            self.slide_now = local;
            self.expansion = self.slide.value(local);
        }
        let room_step = elapsed.min(MAX_ROOM_STEP_MS);
        match self.preparation {
            Some(RoomPreparation::ClosingMenu) if !menu_mounted => {
                self.preparation = Some(RoomPreparation::Loading(0));
            }
            Some(RoomPreparation::Loading(shown)) => {
                let shown = shown + room_step;
                if self.room_requested && shown >= MIN_PREPARATION_MS {
                    self.preparation = None;
                    self.modal.show(true);
                } else {
                    self.preparation = Some(RoomPreparation::Loading(shown));
                }
            }
            _ => {}
        }

        // Both room entrances keep their opening frames across a cold mount.
        self.room_now += room_step;
        self.panel = self.modal.tick(self.room_now);
        self.panel_visible = self.modal.visible();
        if !self.panel_visible && !self.room_requested {
            self.game_room = false;
            self.room_origin = None;
        }
    }

    #[observe]
    pub(super) fn closing_game_menu(&self) -> bool {
        self.preparation == Some(RoomPreparation::ClosingMenu)
    }

    #[observe]
    pub(super) fn room_loading_span(&self) -> Option<(Fixed, Fixed)> {
        match self.preparation {
            Some(RoomPreparation::Loading(shown)) => Some(crate::launcher::loading_span(
                shown + PREPARATION_SWEEP_OFFSET_MS,
            )),
            _ => None,
        }
    }

    #[observe]
    pub(super) fn preparing_room(&self) -> bool {
        self.preparation.is_some()
    }

    pub fn pin_room_origin(&mut self, origin: Rect) {
        if self.game_room && self.room_origin.is_none() {
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
        deck.tick(200, false);
        let middle = deck.expansion;
        assert!(middle > Fixed::ZERO && middle < Fixed::ONE);
        view.launcher.tab = 0;
        deck.present(&view);
        deck.tick(200, false);
        assert_eq!(deck.expansion, middle);
        view.return_to_launcher = true;
        view.launcher.enabled = false;
        deck.present(&view);
        deck.tick(200, false);
        deck.tick(800, false);
        assert_eq!(deck.expansion, Fixed::ZERO);
        assert!(deck.panel < Fixed::from_ratio(1, 10));
        for now in (816..=1152).step_by(16) {
            deck.tick(now, false);
        }
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
