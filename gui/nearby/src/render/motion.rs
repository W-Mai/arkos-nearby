use super::{
    copy,
    style::{FOCUS_OUTSET_X_PERCENT, FOCUS_OUTSET_Y_PERCENT},
};
use crate::model::{Page, Phase, View};
use mirui::anim::{Tween, TweenConfig, ease};
use mirui::prelude::*;
use mirui::types::Transform;

pub(super) fn smoothstep(progress: Fixed) -> Fixed {
    let t = progress.clamp(Fixed::ZERO, Fixed::ONE);
    t * t * (Fixed::from_int(3) - t * 2)
}

pub(super) fn tween_at(mut tween: Tween, elapsed: u64) -> Fixed {
    tween.elapsed_ms = elapsed.min(u64::from(tween.duration_ms)) as u16;
    tween.value()
}

pub(crate) mod timing {
    use super::*;

    pub const TRAVEL: TweenConfig = TweenConfig::new(240, &ease::OUT_CUBIC);
    pub const SETTLED_MS: u16 = 800;
    pub const HOP_MS: u16 = 300;
    pub const RECOIL_MS: u16 = 240;
    pub const FOCUS_PULSE_MS: u16 = 240;
    pub const PRESS_MS: u16 = 120;
    pub const PRESS_FADE_MS: u16 = 180;
    pub const FLAG_RISE_MS: u16 = 240;
    pub const WALK_FRAME_MS: u16 = 90;
    pub const WAVE_MS: u16 = 1200;
    pub const WAVE_STEP_MS: u16 = 32;
    pub const WAVE_COUNT: u16 = 3;
    pub const CHOICE_WAVE_MS: u16 = 600;
    pub const DUST_MS: u16 = 260;
    pub const STARS_MS: u16 = 420;
    pub const GUEST_TURN_MS: u16 = 56;
    pub const HOST_HOP_DELAY_MS: u16 = 70;
    pub const RELAY_LIFT_MS: u16 = 56;
    pub const RELAY_FLIGHT_MS: u16 = 208;
    pub const RELAY_CATCH_MS: u16 = 160;
    pub const RELAY_RELEASE_MS: u16 = RELAY_LIFT_MS + RELAY_FLIGHT_MS;
    pub const SIGNAL_STAGGER_MS: u16 = 55;
    pub const ACTIVITY_MS: u16 = 450;
    pub const ACTIVITY_STAGGER_MS: u16 = 105;

    pub fn progress(age: u16, duration: u16) -> f32 {
        f32::from(age.min(duration)) / f32::from(duration)
    }
}

pub(super) mod anchors {
    use super::Fixed;

    pub const CHOICE_LEFT: Fixed = Fixed::from_ratio(1, 4);
    pub const CHOICE_RIGHT: Fixed = Fixed::from_ratio(3, 4);
    pub const SEAT_WIDTH_PERCENT: i32 = 50;
    pub const HOST_HOME: Fixed = Fixed::from_ratio(SEAT_WIDTH_PERCENT, 200);
    pub const HOST_WELCOME: Fixed = Fixed::from_ratio(3, 10);
    pub const HOST_EXIT: Fixed = Fixed::from_ratio(1, 10);
    pub const HOST_START: Fixed = Fixed::from_ratio(73, 100);
    pub const GUEST_HOME: Fixed = Fixed::from_ratio(100 + SEAT_WIDTH_PERCENT, 200);
    pub const GUEST_GATE: Fixed = Fixed::from_ratio(98, 100);
    pub const GUEST_APPROACH: Fixed = Fixed::from_ratio(94, 100);
    pub const GUEST_CHECK: Fixed = Fixed::from_ratio(88, 100);
    pub const GUEST_EXIT: Fixed = Fixed::from_ratio(3, 10);
}

use timing::SETTLED_MS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Frame {
    pub external_actor: bool,
    pub page: Page,
    pub phase: Phase,
    pub selected: Option<usize>,
    pub ready: bool,
    pub connected: bool,
    pub network_only: bool,
    pub selection: u16,
    pub pulse: u16,
    pub phase_age: u16,
    pub press: u16,
    pub wave: Option<u16>,
    pub walker: Fixed,
    pub deck: Fixed,
    pub host: Fixed,
    pub guest: Fixed,
    pub walking: bool,
    pub host_walking: bool,
    pub host_presence: Fixed,
    pub host_facing_left: bool,
    pub host_ack_age: u16,
    pub relay_age: Option<u16>,
    pub guest_walking: bool,
    pub visitor: bool,
    pub guest_presence: Fixed,
    pub guest_facing_left: bool,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            external_actor: false,
            page: Page::Choice,
            phase: Phase::Idle,
            selected: None,
            ready: false,
            connected: false,
            network_only: false,
            selection: SETTLED_MS,
            pulse: SETTLED_MS,
            phase_age: SETTLED_MS,
            press: SETTLED_MS,
            wave: None,
            walker: anchors::CHOICE_LEFT,
            deck: Fixed::ZERO,
            host: anchors::HOST_HOME,
            guest: anchors::GUEST_HOME,
            walking: false,
            host_walking: false,
            host_presence: Fixed::ONE,
            host_facing_left: false,
            host_ack_age: SETTLED_MS,
            relay_age: None,
            guest_walking: false,
            visitor: false,
            guest_presence: Fixed::ZERO,
            guest_facing_left: true,
        }
    }
}

#[derive(PartialEq, Eq)]
struct SceneKey {
    page: Page,
    phase: Phase,
    ready: bool,
    connected: bool,
    network_only: bool,
    room: Option<String>,
}

impl SceneKey {
    fn is_session(&self) -> bool {
        matches!(self.page, Page::Host | Page::Joined | Page::Leaving)
    }
}

#[derive(Default)]
pub(super) struct Timeline {
    key: Option<SceneKey>,
    selected: Option<usize>,
    catalog: Option<(String, Vec<crate::model::GameRow>)>,
    selection_at: Option<u64>,
    pulse_at: Option<u64>,
    phase_at: Option<u64>,
    network_wave_at: u64,
    press_at: Option<u64>,
    press_sequence: u64,
    walker: Travel,
    deck: Travel,
    host: Travel,
    host_presence: Travel,
    host_ack_at: Option<u64>,
    relay_session: String,
    relay_at: Option<u64>,
    guest: Travel,
    guest_presence: Travel,
    guest_present: bool,
    guest_turnaround: Option<Fixed>,
}

struct Travel {
    tween: Tween,
    at: u64,
}

impl Default for Travel {
    fn default() -> Self {
        Self {
            tween: Tween::preset(Fixed::ZERO, Fixed::ZERO, timing::TRAVEL),
            at: 0,
        }
    }
}

impl Travel {
    fn sample(&self, now: u64) -> Tween {
        let mut tween = self.tween;
        tween.tick(
            now.saturating_sub(self.at)
                .min(u64::from(tween.duration_ms)) as u16,
        );
        tween
    }
    fn moving(&self, now: u64) -> bool {
        !self.sample(now).is_finished()
    }
    fn value(&self, now: u64) -> Fixed {
        self.sample(now).value()
    }
    fn target(&mut self, target: Fixed, now: u64, snap: bool) {
        self.target_after(target, now, snap, 0);
    }

    fn target_after(&mut self, target: Fixed, now: u64, snap: bool, delay_ms: u16) {
        if snap {
            self.tween = Tween::preset(target, target, timing::TRAVEL);
            self.at = now;
        } else if self.tween.to != target {
            self.tween = self.sample(now);
            self.tween.retarget(target, timing::TRAVEL);
            self.at = now.saturating_add(u64::from(delay_ms));
        }
    }
}

fn age(now: u64, start: Option<u64>) -> u16 {
    start.map_or(SETTLED_MS, |start| {
        now.saturating_sub(start).min(u64::from(SETTLED_MS)) as u16
    })
}

impl Timeline {
    pub(super) fn sample(
        &mut self,
        view: &View,
        selected: Option<usize>,
        now: u64,
        press: u64,
    ) -> Frame {
        let mut key = SceneKey {
            page: view.scene_page(),
            phase: copy::phase(view),
            ready: view.ready,
            connected: view.connected,
            network_only: view.network_only,
            // The backend supplies either an empty label, the unnamed room,
            // or a label containing the advertised room code.
            room: match view.room_name.as_str() {
                "" | "房间" => None,
                label => Some(label.to_owned()),
            },
        };
        if let Some(previous) = self.key.as_mut()
            && previous.page == key.page
            && previous.network_only == key.network_only
        {
            match (&previous.room, &key.room) {
                (None, Some(_)) => previous.room.clone_from(&key.room),
                (Some(_), None) => key.room.clone_from(&previous.room),
                _ => {}
            }
        }
        let page_changed = self
            .key
            .as_ref()
            .is_none_or(|old| old.page != key.page || old.room != key.room);
        let catalog_changed = if matches!(view.page, Page::Games | Page::Candidates) {
            self.catalog
                .as_ref()
                .is_none_or(|(path, entries)| path != &view.core || entries != &view.entries)
        } else {
            self.catalog.is_some()
        };
        let deck_target = if view.lobby && view.page == Page::Mode {
            Fixed::ONE
        } else {
            Fixed::ZERO
        };
        let pane_changed = self.deck.tween.to != deck_target;
        self.deck.target(deck_target, now, page_changed);
        let entered = pane_changed
            || page_changed
            || catalog_changed
            || self
                .key
                .as_ref()
                .is_some_and(|old| old.network_only != key.network_only);
        if catalog_changed {
            self.catalog = (matches!(view.page, Page::Games | Page::Candidates))
                .then(|| (view.core.clone(), view.entries.clone()));
        }
        let continues_session = self.key.as_ref().is_some_and(|old| {
            // Room labels can arrive after the pairing route is already visible.
            old.is_session()
                && key.is_session()
                && (key.page == Page::Leaving
                    || old.page == key.page
                        && old.network_only == key.network_only
                        && old.room == key.room)
        });
        let was_present = self.guest_present;
        if !matches!(
            key.phase,
            Phase::Leaving | Phase::Failed | Phase::RestoreFailed | Phase::NetworkReconnecting
        ) || !continues_session
        {
            self.guest_present = view.ready
                || view.connected
                || view.page == Page::Joined
                || matches!(key.phase, Phase::ApprovalPending | Phase::PeerPreparing)
                || (key.phase == Phase::NetworkConfirming && continues_session && was_present)
                || !view.network_only
                    && matches!(
                        key.phase,
                        Phase::Checking
                            | Phase::Notifying
                            | Phase::HostReady
                            | Phase::WaitingStart
                            | Phase::Starting
                    );
        }
        if !continues_session || !self.guest_present {
            self.guest_turnaround = None;
        } else if !was_present && self.guest_presence.value(now) > Fixed::ZERO {
            // A returning guest is still visible on the way out of this room.
            self.guest_turnaround = Some(self.guest.value(now));
        }
        let entering = self.key.is_some()
            && !continues_session
            && matches!(key.phase, Phase::Creating | Phase::Connecting);
        let creating_host = entering && key.page == Page::Host && key.phase == Phase::Creating;
        if !continues_session
            || !self.guest_present
            || matches!(
                key.phase,
                Phase::Leaving | Phase::Closed | Phase::Failed | Phase::RestoreFailed
            )
        {
            self.host_ack_at = None;
        } else if self.key.as_ref().is_some_and(|old| {
            use Phase::*;
            matches!(
                (key.page, old.phase, key.phase),
                (
                    Page::Host,
                    ApprovalPending,
                    PeerPreparing | HostReady | NetworkConnected
                ) | (
                    Page::Joined,
                    ApprovalPendingClient,
                    Connecting
                        | Checking
                        | Notifying
                        | WaitingStart
                        | NetworkConfirming
                        | NetworkConnected
                )
            )
        }) {
            // Both peers observe approval at their own confirmed transition.
            self.host_ack_at = Some(now);
        }
        if self.key.as_ref() != Some(&key) {
            if key.network_only && key.is_session() {
                self.network_wave_at = now;
            }
            // Opening an existing state does not replay its readiness celebration.
            self.phase_at = self
                .key
                .as_ref()
                .filter(|old| {
                    old.page == key.page
                        || matches!(
                            key.phase,
                            Phase::Creating | Phase::Connecting | Phase::Leaving
                        )
                })
                .map(|_| now);
            self.key = Some(key);
        }
        if entered || self.selected != selected {
            self.selection_at = Some(now);
            if entered || age(now, self.pulse_at) >= timing::FOCUS_PULSE_MS {
                self.pulse_at = Some(now);
            }
            self.selected = selected;
        }
        if press != self.press_sequence {
            self.press_sequence = press;
            self.press_at = Some(now);
        }
        self.walker.target(
            if selected == Some(1) {
                anchors::CHOICE_RIGHT
            } else {
                anchors::CHOICE_LEFT
            },
            now,
            page_changed,
        );
        let phase = copy::phase(view);
        if phase == Phase::Failed {
            self.relay_session.clear();
        }
        let relay_age = view.relay_session().map(|session| {
            if self.relay_session != session {
                self.relay_session = session.to_owned();
                self.relay_at = Some(now);
            }
            age(now, self.relay_at)
        });
        if relay_age.is_none() {
            self.relay_at = None;
        }
        let guest = match phase {
            _ if !self.guest_present => anchors::GUEST_GATE,
            Phase::Connecting if self.host_ack_at.is_some() => anchors::GUEST_CHECK,
            Phase::Connecting | Phase::ApprovalPending | Phase::ApprovalPendingClient => {
                anchors::GUEST_GATE
            }
            Phase::NetworkReconnecting => anchors::GUEST_APPROACH,
            Phase::Checking
            | Phase::Notifying
            | Phase::PeerPreparing
            | Phase::NetworkConfirming => anchors::GUEST_CHECK,
            Phase::Starting
                if relay_age.is_some()
                    && (view.page == Page::Host
                        || relay_age.is_some_and(|age| age < timing::RELAY_CATCH_MS)) =>
            {
                anchors::GUEST_HOME
            }
            Phase::Starting => anchors::GUEST_APPROACH,
            Phase::Leaving | Phase::Closed => anchors::GUEST_EXIT,
            Phase::Failed | Phase::RestoreFailed if continues_session => self.guest.value(now),
            _ => anchors::GUEST_HOME,
        };
        let guest = match (self.guest_turnaround, phase) {
            (
                Some(turn),
                Phase::ApprovalPending | Phase::ApprovalPendingClient | Phase::PeerPreparing,
            ) => guest.min(turn),
            _ => {
                self.guest_turnaround = None;
                guest
            }
        };
        let (host, host_presence) = match phase {
            Phase::Starting
                if relay_age.is_some_and(|age| age < timing::RELAY_RELEASE_MS)
                    || (relay_age.is_some() && view.page == Page::Joined) =>
            {
                (anchors::HOST_HOME, Fixed::ONE)
            }
            Phase::Starting => (anchors::HOST_START, Fixed::ONE),
            Phase::Leaving | Phase::Closed => (anchors::HOST_EXIT, Fixed::ZERO),
            Phase::Failed | Phase::RestoreFailed if continues_session => {
                (self.host.value(now), self.host_presence.value(now))
            }
            Phase::HostReady | Phase::WaitingStart | Phase::NetworkConnected => {
                (anchors::HOST_HOME, Fixed::ONE)
            }
            _ if self.guest_present => (anchors::HOST_WELCOME, Fixed::ONE),
            _ => (anchors::HOST_HOME, Fixed::ONE),
        };
        if creating_host {
            self.host.target(anchors::HOST_EXIT, now, true);
            self.host_presence.target(Fixed::ZERO, now, true);
        }
        let snap_host = !continues_session && !creating_host;
        self.host.target(host, now, snap_host);
        self.host_presence.target(host_presence, now, snap_host);
        let host_pose = self.host.sample(now);
        let host_gate = self.host_presence.sample(now);
        let elapsed = age(now, self.host_ack_at);
        let host_ack_age = if elapsed < timing::HOP_MS {
            elapsed
        } else {
            SETTLED_MS
        };
        let host_facing_left = if host_ack_age < timing::HOP_MS {
            // Step back to the seat without turning the greeting away from the guest.
            false
        } else if !host_gate.is_finished() {
            host_gate.to < host_gate.from
        } else if !host_pose.is_finished() {
            host_pose.to < host_pose.from
        } else {
            matches!(phase, Phase::Leaving | Phase::Closed)
        };
        if entering {
            self.guest.target(anchors::GUEST_GATE, now, true);
            self.guest_presence.target(Fixed::ZERO, now, true);
        }
        let snap = !continues_session && !entering;
        // Turn on the spot before walking out. A new arrival redirects both
        // tracks from their current positions, even during the turn.
        let turn_ms = if continues_session && was_present && !self.guest_present {
            timing::GUEST_TURN_MS
        } else {
            0
        };
        let presence_target = if self.guest_present {
            Fixed::ONE
        } else {
            Fixed::ZERO
        };
        self.guest_presence
            .target_after(presence_target, now, snap, turn_ms);
        self.guest.target_after(guest, now, snap, turn_ms);
        let presence = self.guest_presence.sample(now);
        let guest_pose = self.guest.sample(now);
        let walking_to_seat = !guest_pose.is_finished() && now >= self.guest.at;
        let crossing_gate = !presence.is_finished() && now >= self.guest_presence.at;
        let guest_facing_left = self.guest_present
            && if crossing_gate {
                presence.to > presence.from
            } else if walking_to_seat {
                guest_pose.to < guest_pose.from
            } else {
                phase != Phase::Starting
            };
        let wave_clock = if view.network_only
            && matches!(view.page, Page::Host | Page::Joined | Page::Leaving)
        {
            now.saturating_sub(self.network_wave_at)
        } else {
            now
        };
        Frame {
            external_actor: false,
            page: view.page,
            phase: copy::phase(view),
            selected,
            ready: view.ready,
            connected: view.connected,
            network_only: view.network_only,
            selection: age(now, self.selection_at),
            pulse: age(now, self.pulse_at),
            phase_age: age(now, self.phase_at),
            press: age(now, self.press_at),
            wave: copy::loading(view).then_some(
                (wave_clock % u64::from(timing::WAVE_MS) / u64::from(timing::WAVE_STEP_MS)
                    * u64::from(timing::WAVE_STEP_MS)) as u16,
            ),
            walker: self.walker.value(now),
            deck: self.deck.value(now),
            host: host_pose.value(),
            guest: guest_pose.value(),
            walking: self.walker.moving(now),
            host_walking: !host_pose.is_finished() || !host_gate.is_finished(),
            host_presence: host_gate.value(),
            host_facing_left,
            host_ack_age,
            relay_age,
            guest_walking: walking_to_seat || crossing_gate,
            visitor: self.guest_present || presence.value() > Fixed::ZERO,
            guest_presence: presence.value(),
            guest_facing_left,
        }
    }
}

pub(super) fn hop(age: u16, height: f32) -> i32 {
    let p = timing::progress(age, timing::HOP_MS);
    -(p * std::f32::consts::PI)
        .sin()
        .mul_add(height, 0.0)
        .round() as i32
}

pub(super) fn recoil(age: u16) -> i32 {
    let p = timing::progress(age, timing::RECOIL_MS);
    ((p * std::f32::consts::PI * 5.0).sin() * 3.0 * (1.0 - p).powi(2)).round() as i32
}

impl Frame {
    pub(super) fn settled(self) -> bool {
        self.selection >= SETTLED_MS
            && self.phase_age >= SETTLED_MS
            && self.relay_age.is_none_or(|age| age >= SETTLED_MS)
            && self.pulse >= timing::FOCUS_PULSE_MS
            && self.press >= timing::PRESS_FADE_MS
            && self.wave.is_none()
            && !self.walking
            && self.host_ack_age >= timing::HOP_MS
            && !self.host_walking
            && !self.guest_walking
    }

    pub(super) fn expansion(self) -> (Fixed, Fixed) {
        let p = timing::progress(self.pulse, timing::FOCUS_PULSE_MS);
        let amount = 16.0 * p.powi(2) * (1.0 - p).powi(2);
        (
            Fixed::from_f32(amount * FOCUS_OUTSET_X_PERCENT),
            Fixed::from_f32(amount * FOCUS_OUTSET_Y_PERCENT),
        )
    }

    pub(super) fn card_transform(self, index: usize) -> Transform {
        if self.selected != Some(index) {
            return Transform::IDENTITY;
        }
        let pressed = if self.press < timing::PRESS_MS { 2 } else { 0 };
        let age = if matches!(self.page, Page::Host | Page::Joined) {
            self.phase_age.min(self.selection)
        } else {
            self.selection
        };
        Transform::translate(recoil(age).into(), (hop(age, 3.0) + pressed).into())
    }
}

impl Frame {
    pub(super) fn cards(self) -> Self {
        Self {
            page: self.page,
            selected: self.selected,
            selection: self.selection.min(timing::HOP_MS),
            phase_age: self.phase_age.min(timing::HOP_MS),
            press: self.press.min(timing::PRESS_MS),
            ..Self::default()
        }
    }

    pub(super) fn cards_settled(self) -> bool {
        self.selected.is_none()
            || (self.selection >= timing::HOP_MS
                && (!matches!(self.page, Page::Host | Page::Joined)
                    || self.phase_age >= timing::HOP_MS)
                && self.press >= timing::PRESS_MS)
    }

    pub(super) fn choice(self) -> Self {
        Self {
            selected: self.selected,
            selection: self.selection,
            ..Self::default()
        }
    }
    pub(super) fn walker(self) -> Self {
        Self {
            selected: self.selected,
            selection: self.selection,
            walker: self.walker,
            walking: self.walking,
            ..Self::default()
        }
    }
    pub(super) fn session(self) -> Self {
        Self {
            page: self.page,
            phase: self.phase,
            phase_age: self.phase_age,
            relay_age: self.relay_age,
            ready: self.ready,
            connected: self.connected,
            network_only: self.network_only,
            host: self.host,
            guest: self.guest,
            host_walking: self.host_walking,
            host_presence: self.host_presence,
            host_facing_left: self.host_facing_left,
            host_ack_age: self.host_ack_age,
            guest_walking: self.guest_walking,
            visitor: self.visitor,
            guest_presence: self.guest_presence,
            guest_facing_left: self.guest_facing_left,
            wave: self.wave,
            ..Self::default()
        }
    }
    pub(super) fn signal(self) -> Self {
        Self {
            selected: self.selected,
            selection: self.selection,
            wave: self.wave,
            ..Self::default()
        }
    }
    pub(super) fn search(self) -> Self {
        Self {
            wave: self.wave,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_projection_preserves_pixels_and_excludes_unrelated_activity() {
        for page in [Page::Games, Page::Choice, Page::Host, Page::Joined] {
            for age in 0..=SETTLED_MS {
                let frame = Frame {
                    page,
                    selected: Some(0),
                    selection: age,
                    phase_age: age / 2,
                    press: age / 3,
                    wave: Some(age),
                    walking: true,
                    ..Frame::default()
                };
                let cards = frame.cards();
                for index in [0, 1] {
                    assert_eq!(cards.card_transform(index), frame.card_transform(index));
                }
                if cards.cards_settled() {
                    assert_eq!(cards.card_transform(0), Transform::IDENTITY);
                }
            }
        }
        let settled = Frame {
            selected: Some(0),
            ..Frame::default()
        };
        let loading = Frame {
            wave: Some(32),
            walking: true,
            ..settled
        };
        assert_eq!(settled.cards(), loading.cards());
        assert!(loading.cards_settled());
        assert!(!loading.settled());
    }

    #[test]
    fn travel_retargets_continuously_and_finishes_inside_normalized_bounds() {
        let mut travel = Travel::default();
        travel.target(anchors::CHOICE_LEFT, 0, true);
        travel.target(anchors::CHOICE_RIGHT, 0, false);
        for now in 0..u64::from(timing::TRAVEL.duration_ms) {
            let current = travel.value(now);
            assert!((Fixed::ZERO..=Fixed::ONE).contains(&current));
            let next = if now % 2 == 0 {
                anchors::CHOICE_LEFT
            } else {
                anchors::CHOICE_RIGHT
            };
            travel.target(next, now, false);
            assert_eq!(travel.value(now), current);
        }
        let start = u64::from(timing::TRAVEL.duration_ms);
        travel.target(anchors::GUEST_HOME, start, false);
        // Repeated observations of the same target must not extend the movement.
        for now in start..=start + u64::from(timing::TRAVEL.duration_ms) {
            travel.target(anchors::GUEST_HOME, now, false);
        }
        assert_eq!(travel.value(start * 2), anchors::GUEST_HOME);
        assert!(!travel.moving(start * 2));
    }

    #[test]
    fn entry_and_catalog_navigation_animate_without_poll_replays() {
        let mut timeline = Timeline::default();
        let mut view = View::default();
        for page in [Page::Mode, Page::Choice, Page::Mode, Page::Games] {
            view.page = page;
            assert_eq!(timeline.sample(&view, Some(0), 1000, 0).selection, 0);
            view.status = "Status update".into();
            assert_eq!(timeline.sample(&view, Some(0), 1120, 0).selection, 120);
            assert_eq!(
                timeline.sample(&view, Some(0), 2000, 0).selection,
                SETTLED_MS
            );
        }
        view.core = "NES / FC".into();
        view.entries.push(crate::model::GameRow {
            key: String::new(),
            name: "Game".into(),
            ..Default::default()
        });
        assert_eq!(timeline.sample(&view, Some(0), 2100, 0).selection, 0);
        let moving = timeline.sample(&view, Some(0), 2160, 0);
        assert_ne!(moving.card_transform(0), Transform::IDENTITY);
        assert_eq!(moving.card_transform(1), Transform::IDENTITY);
        assert_eq!(
            timeline.sample(&view, Some(0), 3000, 0).selection,
            SETTLED_MS
        );
    }

    #[test]
    fn snapshots_do_not_replay_motion_and_bursts_cannot_accumulate_scale() {
        let mut timeline = Timeline::default();
        let view = View::default();
        timeline.sample(&view, Some(0), 0, 0);
        timeline.sample(&view, Some(1), 10, 0);
        assert_eq!(
            timeline.sample(&view, Some(1), 120, 0).expansion(),
            (
                Fixed::from_f32(FOCUS_OUTSET_X_PERCENT),
                Fixed::from_f32(FOCUS_OUTSET_Y_PERCENT)
            )
        );
        for i in 0..100 {
            let frame = timeline.sample(&view, Some(i % 2), 140 + i as u64 * 17, 0);
            let (x, y) = frame.expansion();
            assert!(
                (Fixed::ZERO..=Fixed::from_f32(FOCUS_OUTSET_X_PERCENT)).contains(&x)
                    && (Fixed::ZERO..=Fixed::from_f32(FOCUS_OUTSET_Y_PERCENT)).contains(&y)
            );
        }
        let mut next = view.clone();
        next.status = "Updated status".into();
        let a = timeline.sample(&next, Some(1), 5000, 0);
        let b = timeline.sample(&next, Some(1), 6000, 0);
        assert_eq!(a, b);
        assert_eq!(b.expansion(), (Fixed::ZERO, Fixed::ZERO));
    }

    #[test]
    fn readiness_and_reopening_follow_backend_state() {
        let mut timeline = Timeline::default();
        let mut view = View {
            page: Page::Host,
            phase: Phase::WaitingPeer,
            ..View::default()
        };
        timeline.sample(&view, None, 0, 0);
        view.phase = Phase::PeerPreparing;
        assert!(!timeline.sample(&view, None, 10, 0).ready);
        view.phase = Phase::HostReady;
        view.ready = true;
        assert_eq!(timeline.sample(&view, Some(0), 20, 0).phase_age, 0);
        assert_eq!(
            Timeline::default().sample(&view, Some(0), 20, 0).phase_age,
            SETTLED_MS
        );
    }

    #[test]
    fn only_creating_a_room_brings_the_host_through_the_left_gate() {
        let mut timeline = Timeline::default();
        let choice = View::default();
        timeline.sample(&choice, Some(0), 0, 0);
        let mut room = View {
            page: Page::Host,
            phase: Phase::Creating,
            ..View::default()
        };
        let entrance = timeline.sample(&room, Some(0), 100, 0);
        assert_eq!(
            (entrance.host, entrance.host_presence),
            (anchors::HOST_EXIT, Fixed::ZERO)
        );
        assert!(entrance.host_walking && !entrance.host_facing_left && !entrance.visitor);
        let step = timeline.sample(&room, Some(0), 160, 0);
        assert!(step.host_presence > Fixed::ZERO && step.host_presence < Fixed::ONE);
        room.phase = Phase::WaitingPeer;
        room.room_name = "房间 A216".into();
        let named = timeline.sample(&room, None, 160, 0);
        assert_eq!(
            (named.host, named.host_presence),
            (step.host, step.host_presence)
        );
        let seated = timeline.sample(&room, None, 500, 0);
        assert_eq!(
            (seated.host, seated.host_presence),
            (anchors::HOST_HOME, Fixed::ONE)
        );
        assert!(!seated.host_walking);

        timeline.sample(&choice, Some(1), 600, 0);
        room.page = Page::Joined;
        room.phase = Phase::Connecting;
        let joining = timeline.sample(&room, None, 700, 0);
        assert_eq!(joining.host_presence, Fixed::ONE);
        assert!(!joining.host_walking && joining.guest_walking);
        assert_eq!(joining.host_ack_age, SETTLED_MS);
        for page in [Page::Host, Page::Joined] {
            room.page = page;
            room.phase = if page == Page::Host {
                Phase::HostReady
            } else {
                Phase::WaitingStart
            };
            room.ready = true;
            let recovered = Timeline::default().sample(&room, None, 900, 0);
            assert_eq!(
                (recovered.host, recovered.host_presence),
                (anchors::HOST_HOME, Fixed::ONE)
            );
            assert!(!recovered.host_walking && !recovered.host_facing_left);
            assert_eq!(recovered.host_ack_age, SETTLED_MS);
        }
    }

    #[test]
    fn host_welcomes_approves_and_turns_back_without_restarting_a_stride() {
        let mut timeline = Timeline::default();
        let mut room = View {
            page: Page::Host,
            phase: Phase::WaitingPeer,
            ..View::default()
        };
        timeline.sample(&room, None, 0, 0);
        room.phase = Phase::ApprovalPending;
        let invitation = timeline.sample(&room, Some(0), 1000, 0);
        assert_eq!(invitation.host, anchors::HOST_HOME);
        assert!(!invitation.ready && invitation.host_walking && !invitation.host_facing_left);
        let welcome = timeline.sample(&room, Some(0), 1080, 1);
        assert!(welcome.host > anchors::HOST_HOME && welcome.host < anchors::HOST_WELCOME);
        assert_eq!(welcome.host_ack_age, SETTLED_MS);
        room.phase = Phase::PeerPreparing;
        let approved = timeline.sample(&room, Some(0), 1080, 1);
        assert_eq!((approved.host, approved.host_ack_age), (welcome.host, 0));
        assert!(!approved.ready);
        room.phase = Phase::HostReady;
        room.ready = true;
        let ready = timeline.sample(&room, Some(0), 1110, 1);
        assert_eq!(ready.host_ack_age, 30);
        assert!(ready.ready);
        let seated = timeline.sample(&room, Some(0), 1500, 1);
        assert_eq!(
            (seated.host, seated.guest),
            (anchors::HOST_HOME, anchors::GUEST_HOME)
        );
        assert_eq!(seated.host_ack_age, SETTLED_MS);
        room.ready = false;
        room.phase = Phase::ApprovalPending;
        timeline.sample(&room, None, 1500, 1);
        let greeting = timeline.sample(&room, None, 1560, 1);
        room.phase = Phase::WaitingPeer;
        let turn = timeline.sample(&room, None, 1560, 1);
        assert_eq!(turn.host, greeting.host);
        assert!(turn.host_walking && turn.host_facing_left);
        let retreat = timeline.sample(&room, None, 1600, 1);
        assert!(retreat.host < turn.host && retreat.host > anchors::HOST_HOME);
        room.phase = Phase::ApprovalPending;
        let redirect = timeline.sample(&room, None, 1600, 1);
        assert_eq!(redirect.host, retreat.host);
        assert!(redirect.host_walking && !redirect.host_facing_left);
        let next = timeline.sample(&room, None, 1640, 1);
        assert!(next.host > redirect.host && next.host < anchors::HOST_WELCOME);
        room.phase = Phase::WaitingPeer;
        timeline.sample(&room, None, 1640, 1);
        let alone = timeline.sample(&room, None, 2000, 1);
        assert_eq!(
            (alone.host, alone.host_presence),
            (anchors::HOST_HOME, Fixed::ONE)
        );
        assert!(!alone.host_walking && !alone.host_facing_left);
    }

    #[test]
    fn approval_and_seat_positions_follow_each_roles_observed_phase() {
        use Phase::*;
        for (page, pending, approved, ready, network_only) in [
            (Page::Host, ApprovalPending, PeerPreparing, HostReady, false),
            (
                Page::Joined,
                ApprovalPendingClient,
                Checking,
                WaitingStart,
                false,
            ),
            (
                Page::Joined,
                ApprovalPendingClient,
                Connecting,
                WaitingStart,
                false,
            ),
            (
                Page::Joined,
                ApprovalPendingClient,
                WaitingStart,
                WaitingStart,
                false,
            ),
            (
                Page::Host,
                ApprovalPending,
                PeerPreparing,
                NetworkConnected,
                true,
            ),
            (
                Page::Joined,
                ApprovalPendingClient,
                NetworkConfirming,
                NetworkConnected,
                true,
            ),
        ] {
            let mut room = View {
                page,
                phase: pending,
                network_only,
                ..View::default()
            };
            let mut timeline = Timeline::default();
            timeline.sample(&room, None, 0, 0);
            room.phase = approved;
            assert_eq!(timeline.sample(&room, None, 100, 0).host_ack_age, 0);
            assert_eq!(timeline.sample(&room, None, 130, 0).host_ack_age, 30);
            room.phase = ready;
            room.ready = !network_only;
            room.connected = network_only;
            let greeting = timeline.sample(&room, None, 160, 0);
            assert_eq!(greeting.host_ack_age, 60);
            assert!(!greeting.host_facing_left);
            let seated = timeline.sample(&room, None, 960, 0);
            assert_eq!(
                (seated.host, seated.guest),
                (anchors::HOST_HOME, anchors::GUEST_HOME)
            );
            assert_eq!(seated.host_ack_age, SETTLED_MS);
        }
        for outcome in [
            Leaving,
            Closed,
            Failed,
            RestoreFailed,
            NetworkReconnecting,
            Starting,
        ] {
            let mut room = View {
                page: Page::Joined,
                phase: ApprovalPendingClient,
                ..View::default()
            };
            let mut timeline = Timeline::default();
            timeline.sample(&room, None, 0, 0);
            room.phase = outcome;
            assert_eq!(
                timeline.sample(&room, None, 100, 0).host_ack_age,
                SETTLED_MS
            );
        }
    }

    #[test]
    fn first_join_approval_settles_at_the_same_door_on_either_screen() {
        use Phase::*;
        for (page, before, pending) in [
            (Page::Host, WaitingPeer, ApprovalPending),
            (Page::Joined, Connecting, ApprovalPendingClient),
            (Page::Joined, Checking, ApprovalPendingClient),
        ] {
            let mut room = View {
                page,
                phase: before,
                ..View::default()
            };
            let mut timeline = Timeline::default();
            timeline.sample(&View::default(), None, 0, 0);
            timeline.sample(&room, None, 100, 0);
            let arrived = timeline.sample(&room, None, 800, 0);
            room.phase = pending;
            let invite = timeline.sample(&room, None, 800, 0);
            assert_eq!(invite.guest, arrived.guest);
            let settled = timeline.sample(&room, None, 1400, 0);
            assert_eq!(
                (settled.guest, settled.guest_presence),
                (anchors::GUEST_GATE, Fixed::ONE)
            );
            assert_eq!(settled.host, anchors::HOST_WELCOME);
            assert!(settled.guest_facing_left && !settled.guest_walking);
        }
    }

    #[test]
    fn guest_approaches_from_the_gate_and_can_rejoin_during_departure() {
        let mut host = View {
            page: Page::Host,
            phase: Phase::WaitingPeer,
            room_name: "Room".into(),
            ..View::default()
        };
        let mut timeline = Timeline::default();
        let empty = timeline.sample(&host, None, 0, 0);
        assert!(!empty.visitor);
        assert_eq!(empty.guest, anchors::GUEST_GATE);
        host.phase = Phase::ApprovalPending;
        let arrival = timeline.sample(&host, None, 1000, 0);
        assert!(arrival.visitor && arrival.guest_facing_left && !arrival.ready);
        assert_eq!(
            (arrival.guest, arrival.guest_presence),
            (empty.guest, Fixed::ZERO)
        );
        let entry = timeline.sample(&host, None, 1080, 0);
        assert!(entry.guest_presence > Fixed::ZERO && entry.guest_presence < Fixed::ONE);
        host.phase = Phase::HostReady;
        host.ready = true;
        let confirming = timeline.sample(&host, None, 1080, 0);
        assert!(confirming.ready);
        assert_eq!(confirming.guest_presence, entry.guest_presence);
        let seated = timeline.sample(&host, None, 1500, 0);
        assert_eq!(
            (seated.guest, seated.guest_presence),
            (anchors::GUEST_HOME, Fixed::ONE)
        );
        host.phase = Phase::WaitingPeer;
        host.ready = false;
        let turn = timeline.sample(&host, None, 1500, 0);
        assert!(turn.visitor && !turn.ready && !turn.guest_facing_left && !turn.guest_walking);
        assert_eq!(turn.guest, seated.guest);
        let pause = timeline.sample(&host, None, 1540, 0);
        assert_eq!(
            (pause.guest, pause.guest_presence),
            (turn.guest, turn.guest_presence)
        );
        let exit = timeline.sample(&host, None, 1640, 0);
        assert!(exit.guest > seated.guest && exit.guest_presence < Fixed::ONE);
        host.phase = Phase::ApprovalPending;
        let rejoin = timeline.sample(&host, None, 1640, 0);
        assert!(rejoin.guest_facing_left);
        assert_eq!(
            (rejoin.guest, rejoin.guest_presence),
            (exit.guest, exit.guest_presence)
        );
        let next = timeline.sample(&host, None, 1700, 0);
        assert!(next.guest <= rejoin.guest && next.guest_presence > rejoin.guest_presence);
        host.phase = Phase::WaitingPeer;
        timeline.sample(&host, None, 1710, 0);
        let empty = timeline.sample(&host, None, 2100, 0);
        assert!(!empty.visitor && !empty.guest_walking);
        assert_eq!(
            (empty.guest, empty.guest_presence),
            (anchors::GUEST_GATE, Fixed::ZERO)
        );
    }

    #[test]
    fn advertised_room_label_does_not_restart_an_arrival() {
        let mut timeline = Timeline::default();
        timeline.sample(&View::default(), None, 0, 0);
        let mut joined = View {
            page: Page::Joined,
            phase: Phase::Connecting,
            ..View::default()
        };
        timeline.sample(&joined, None, 100, 0);
        let moving = timeline.sample(&joined, None, 180, 0);
        assert!(moving.guest_presence > Fixed::ZERO && moving.guest_presence < Fixed::ONE);
        joined.room_name = "房间".into();
        assert_eq!(timeline.sample(&joined, None, 180, 0), moving);
        joined.room_name = "房间 A216".into();
        assert_eq!(timeline.sample(&joined, None, 180, 0), moving);
        joined.room_name.clear();
        assert_eq!(timeline.sample(&joined, None, 180, 0), moving);
        joined.room_name = "房间 B001".into();
        assert_eq!(
            timeline.sample(&joined, None, 180, 0).guest_presence,
            Fixed::ZERO
        );
    }

    #[test]
    fn joined_and_network_only_rooms_reopen_at_their_current_occupancy() {
        for (page, network_only, phase, ready, connected, occupied) in [
            (Page::Host, false, Phase::HostReady, true, false, true),
            (Page::Joined, false, Phase::Checking, false, false, true),
            (
                Page::Host,
                true,
                Phase::NetworkConfirming,
                false,
                false,
                false,
            ),
            (Page::Host, true, Phase::NetworkConnected, false, true, true),
            (
                Page::Joined,
                true,
                Phase::NetworkConfirming,
                false,
                false,
                true,
            ),
        ] {
            let view = View {
                page,
                network_only,
                phase,
                ready,
                connected,
                ..View::default()
            };
            let frame = Timeline::default().sample(&view, None, 5000, 0);
            assert_eq!(frame.visitor, occupied);
            assert_eq!(
                frame.guest_presence,
                if occupied { Fixed::ONE } else { Fixed::ZERO }
            );
            assert_eq!(frame.ready, ready);
            assert!(!frame.guest_walking);
        }
    }

    #[test]
    fn session_routes_continue_from_current_positions_and_preserve_membership() {
        let mut timeline = Timeline::default();
        let mut view = View {
            page: Page::Host,
            phase: Phase::HostReady,
            ready: true,
            room_name: "Room".into(),
            ..View::default()
        };
        let ready = timeline.sample(&view, Some(0), 0, 0);
        view.phase = Phase::Starting;
        let start = timeline.sample(&view, Some(0), 10, 0);
        assert_eq!((ready.host, ready.guest), (start.host, start.guest));
        let moving = timeline.sample(&view, Some(0), 130, 0);
        assert!(moving.host > start.host && moving.guest > start.guest);
        view.page = Page::Leaving;
        view.phase = Phase::Leaving;
        view.room_name.clear();
        view.ready = false;
        let leaving = timeline.sample(&view, None, 130, 0);
        assert_eq!((moving.host, moving.guest), (leaving.host, leaving.guest));
        assert!(leaving.visitor);
        let settled = timeline.sample(&view, None, 1000, 0);
        assert!(settled.host < leaving.host && settled.guest < leaving.guest);
        assert!(moving.host_walking && !moving.host_facing_left);
        assert!(leaving.host_walking && leaving.host_facing_left);
        assert_eq!(settled.host_presence, Fixed::ZERO);
        assert!(!settled.host_walking && !settled.guest_walking);

        let mut alone = Timeline::default();
        view.page = Page::Host;
        view.phase = Phase::WaitingPeer;
        assert!(!alone.sample(&view, None, 0, 0).visitor);
        view.page = Page::Leaving;
        view.phase = Phase::Leaving;
        assert!(!alone.sample(&view, None, 10, 0).visitor);
    }
}
