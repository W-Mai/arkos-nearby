use mirui::anim::{Spring, SpringConfig, Tween, TweenConfig, ease};
use mirui::prelude::*;

pub(crate) const ENTER: TweenConfig = TweenConfig::new(320, &ease::IN_OUT_CUBIC);
pub(crate) const EXIT: TweenConfig = TweenConfig::new(200, &ease::IN_OUT_CUBIC);
const CONTENT: TweenConfig = TweenConfig::new(420, &ease::IN_OUT_CUBIC);
const CARD_LIFT: SpringConfig = SpringConfig {
    duration_ms: 380,
    bounce: Fixed::from_ratio(1, 2),
};
const CARD_RETURN: SpringConfig = SpringConfig {
    duration_ms: 250,
    bounce: Fixed::from_ratio(2, 5),
};

/// A transition starts on its first animation frame, independent of idle time.
pub(crate) struct Track {
    tween: Tween,
    started: Option<u64>,
}

impl Track {
    fn new(value: Fixed, config: TweenConfig) -> Self {
        Self {
            tween: Tween::preset(value, value, config),
            started: None,
        }
    }

    pub(crate) fn visibility() -> Self {
        Self::new(Fixed::ZERO, ENTER)
    }

    pub(crate) fn show(&mut self, shown: bool) {
        let target = if shown { Fixed::ONE } else { Fixed::ZERO };
        if self.tween.to != target {
            // Redirect from the displayed value, never from unrendered wall time.
            self.tween
                .retarget(target, if shown { ENTER } else { EXIT });
            self.started = None;
        }
    }

    pub(crate) fn tick(&mut self, now: u64) -> Fixed {
        if self.tween.is_finished() {
            return self.tween.value();
        }
        let started = *self.started.get_or_insert(now);
        self.tween.elapsed_ms = now
            .saturating_sub(started)
            .min(u64::from(self.tween.duration_ms)) as u16;
        self.tween.value()
    }

    pub(crate) fn visible(&self) -> bool {
        self.tween.to != Fixed::ZERO || !self.tween.is_finished()
    }

    fn arrive(&mut self) {
        if self.tween.is_finished() {
            self.tween = Tween::preset(Fixed::ZERO, Fixed::ONE, CONTENT);
            self.started = None;
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Route {
    pub wifi: Option<howff_menu::wifi::Page>,
    pub management: Option<super::management::Page>,
    pub saves: Option<super::saves::Page>,
    pub configuration: Option<howff_menu::configuration::Section>,
    pub details: Option<(howff_menu::details::Section, bool)>,
}

#[model]
pub struct Presence {
    #[observe]
    pub mounted: bool,
    #[observe]
    pub progress: Fixed,
    #[observe]
    pub card_travel: Fixed,
    #[observe]
    pub arrival: Fixed,
    #[observe]
    pub cartridge: bool,
    #[observe]
    pub card_epoch: u64,
    #[observe]
    pub detached_at: Option<Fixed>,
    card_key: String,
    route: Option<Route>,
    kind: String,
    travel: Track,
    card_spring: Spring,
    card_tick: Option<u64>,
    content: Track,
}

impl Default for Presence {
    fn default() -> Self {
        Self {
            mounted: false,
            progress: Fixed::ZERO,
            card_travel: Fixed::ZERO,
            arrival: Fixed::ONE,
            cartridge: false,
            card_epoch: 0,
            detached_at: None,
            card_key: String::new(),
            route: None,
            kind: String::new(),
            travel: Track::visibility(),
            card_spring: Spring::preset(Fixed::ZERO, Fixed::ZERO, CARD_LIFT),
            card_tick: None,
            content: Track::new(Fixed::ONE, CONTENT),
        }
    }
}

#[model]
impl Presence {
    pub(super) fn present(&mut self, kind: &str, route: Option<Route>, key: &str) {
        let shown = route.is_some();
        if shown && self.route.is_none() {
            let cartridge = matches!(kind, "game" | "details");
            if cartridge && (!self.cartridge || self.card_key != key || self.detached_at.is_some())
            {
                if self.cartridge {
                    self.travel = Track::visibility();
                    self.progress = Fixed::ZERO;
                }
                self.card_epoch = self.card_epoch.wrapping_add(1);
                self.card_spring = Spring::preset(Fixed::ZERO, Fixed::ZERO, CARD_LIFT);
                self.card_travel = Fixed::ZERO;
                self.card_tick = None;
                self.card_key.clear();
                self.card_key.push_str(key);
            }
            self.cartridge = cartridge;
            self.detached_at = None;
        } else if !shown && self.cartridge && !self.card_matches(key) {
            self.detach();
        }
        self.travel.show(shown);
        if self.cartridge {
            let target = if shown { Fixed::ONE } else { Fixed::ZERO };
            if self.card_spring.target != target {
                self.card_spring
                    .retarget(target, Some(if shown { CARD_LIFT } else { CARD_RETURN }));
                self.card_tick = None;
            }
        }
        self.mounted |= shown;
        if shown && (route != self.route || kind != self.kind) {
            if self.route.is_some() {
                self.content.arrive();
                self.arrival = self.content.tween.value();
            }
            self.kind.clear();
            self.kind.push_str(kind);
        }
        self.route = route;
    }

    pub(crate) fn card_matches(&self, key: &str) -> bool {
        self.card_key.is_empty() || self.card_key == key
    }

    pub(crate) fn closing(&self) -> bool {
        self.route.is_none()
    }

    pub(crate) fn detach(&mut self) {
        self.detached_at.get_or_insert(self.progress);
    }

    #[observe]
    pub fn shows_front(&self) -> bool {
        self.detached_at.unwrap_or(self.progress) < Fixed::HALF
    }

    pub fn tick(&mut self, now: u64) {
        self.progress = self.travel.tick(now);
        if self.cartridge && self.detached_at.is_none() && !self.card_spring.is_settled() {
            if let Some(last) = self.card_tick.replace(now) {
                self.card_spring
                    .tick(now.saturating_sub(last).min(u64::from(u16::MAX)) as u16);
            }
            self.card_travel = self.card_spring.value();
        }
        self.arrival = self.content.tick(now);
        if self.route.is_none() && !self.travel.visible() {
            self.mounted = false;
            self.cartridge = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_starts_at_its_displayed_endpoint_after_idle_or_mount_delays() {
        let mut track = Track::visibility();
        assert_eq!(track.tick(100), Fixed::ZERO);
        assert!(!track.visible());
        track.show(true);
        assert!(track.visible());
        assert_eq!(track.tick(60_000), Fixed::ZERO);
        let first = track.tick(60_016);
        assert!(first < Fixed::from_ratio(1, 10));
        let mid = track.tick(60_240);
        track.show(false);
        assert_eq!(track.tick(70_000), mid);
        let closing = track.tick(70_100);
        assert!(closing < mid);
        track.show(true);
        assert_eq!(track.tick(71_000), closing);
        assert_eq!(track.tick(71_600), Fixed::ONE);
        track.show(false);
        assert_eq!(track.tick(80_000), Fixed::ONE);
        assert_eq!(track.tick(80_500), Fixed::ZERO);
        assert!(!track.visible());
    }

    #[test]
    fn cartridge_spring_overshoots_and_keeps_momentum_on_reversal() {
        let mut motion = Presence::default();
        motion.present("game", Some(Route::default()), "game-a");
        for time in (0..=96).step_by(16) {
            motion.tick(time);
        }
        let start = motion.card_travel;
        motion.present("", None, "game-a");
        motion.tick(112);
        assert_eq!(motion.card_travel, start);
        motion.tick(120);
        assert!(
            motion.card_travel > start,
            "the lifted card retains its outgoing velocity"
        );
        motion.present("game", Some(Route::default()), "game-a");
        let mut peak = Fixed::ZERO;
        for time in (120..=800).step_by(8) {
            motion.tick(time);
            peak = peak.max(motion.card_travel);
        }
        assert!(peak > Fixed::ONE);
        assert_eq!(motion.card_travel, Fixed::ONE);
        assert_eq!(motion.progress, Fixed::ONE);
    }

    #[test]
    fn presence_retains_exit_and_reverses_without_jumping() {
        let mut motion = Presence::default();
        motion.present("settings", Some(Route::default()), "");
        motion.tick(0);
        motion.tick(120);
        let open = motion.progress;
        assert!(open > Fixed::ZERO && open < Fixed::ONE);
        motion.present("", None, "");
        motion.tick(120);
        assert_eq!(motion.progress, open);
        assert!(motion.mounted);
        motion.tick(180);
        let closing = motion.progress;
        motion.present("settings", Some(Route::default()), "");
        motion.tick(180);
        assert_eq!(motion.progress, closing);
        motion.tick(800);
        assert_eq!(motion.progress, Fixed::ONE);
        motion.present("", None, "");
        motion.tick(800);
        motion.tick(1300);
        assert!(!motion.mounted);
    }

    #[test]
    fn nested_route_changes_preserve_an_active_content_transition() {
        let mut motion = Presence::default();
        let route = Some(Route::default());
        motion.present("settings", route, "");
        motion.tick(0);
        motion.tick(600);
        motion.present("details", route, "");
        motion.tick(600);
        motion.tick(740);
        let arrival = motion.arrival;
        motion.present("settings", route, "");
        assert_eq!(motion.arrival, arrival);
        motion.tick(740);
        assert_eq!(motion.arrival, arrival);
        motion.tick(1100);
        assert_eq!(motion.arrival, Fixed::ONE);
    }

    #[test]
    fn identical_routes_do_not_restart_content_motion() {
        let mut motion = Presence::default();
        let route = Route::default();
        motion.present("settings", Some(route), "");
        motion.tick(0);
        motion.tick(600);
        motion.present("settings", Some(route), "");
        assert_eq!(motion.arrival, Fixed::ONE);
        motion.present("details", Some(route), "");
        assert_eq!(motion.arrival, Fixed::ZERO);
    }
}
