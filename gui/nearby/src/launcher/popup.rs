use mirui::anim::{Tween, TweenConfig, ease};
use mirui::prelude::*;

pub(crate) const ENTER: TweenConfig = TweenConfig::new(320, &ease::IN_OUT_CUBIC);
pub(crate) const EXIT: TweenConfig = TweenConfig::new(200, &ease::IN_OUT_CUBIC);
const CONTENT: TweenConfig = TweenConfig::new(420, &ease::IN_OUT_CUBIC);

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
    pub arrival: Fixed,
    route: Option<Route>,
    kind: String,
    travel: Track,
    content: Track,
}

impl Default for Presence {
    fn default() -> Self {
        Self {
            mounted: false,
            progress: Fixed::ZERO,
            arrival: Fixed::ONE,
            route: None,
            kind: String::new(),
            travel: Track::visibility(),
            content: Track::new(Fixed::ONE, CONTENT),
        }
    }
}

#[model]
impl Presence {
    pub(super) fn present(&mut self, kind: &str, route: Option<Route>) {
        let shown = route.is_some();
        self.travel.show(shown);
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

    pub fn tick(&mut self, now: u64) {
        self.progress = self.travel.tick(now);
        self.arrival = self.content.tick(now);
        if self.route.is_none() && !self.travel.visible() {
            self.mounted = false;
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
    fn presence_retains_exit_and_reverses_without_jumping() {
        let mut motion = Presence::default();
        motion.present("settings", Some(Route::default()));
        motion.tick(0);
        motion.tick(120);
        let open = motion.progress;
        assert!(open > Fixed::ZERO && open < Fixed::ONE);
        motion.present("", None);
        motion.tick(120);
        assert_eq!(motion.progress, open);
        assert!(motion.mounted);
        motion.tick(180);
        let closing = motion.progress;
        motion.present("settings", Some(Route::default()));
        motion.tick(180);
        assert_eq!(motion.progress, closing);
        motion.tick(800);
        assert_eq!(motion.progress, Fixed::ONE);
        motion.present("", None);
        motion.tick(800);
        motion.tick(1300);
        assert!(!motion.mounted);
    }

    #[test]
    fn nested_route_changes_preserve_an_active_content_transition() {
        let mut motion = Presence::default();
        let route = Some(Route::default());
        motion.present("settings", route);
        motion.tick(0);
        motion.tick(600);
        motion.present("details", route);
        motion.tick(600);
        motion.tick(740);
        let arrival = motion.arrival;
        motion.present("settings", route);
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
        motion.present("settings", Some(route));
        motion.tick(0);
        motion.tick(600);
        motion.present("settings", Some(route));
        assert_eq!(motion.arrival, Fixed::ONE);
        motion.present("details", Some(route));
        assert_eq!(motion.arrival, Fixed::ZERO);
    }
}
