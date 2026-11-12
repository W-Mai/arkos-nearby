mod bank;
#[cfg(test)]
mod model_tests;

use crate::model::{Action, Page, Phase, View};
use mirui::app::{App, RendererFactory};
use mirui::audio::{AudioHandle, AudioOutputState, CueId};
use mirui::prelude::model;
use mirui::surface::Surface;
use std::collections::BTreeSet;
use std::path::Path;

const NAVIGATION_INTERVAL_MS: u64 = 65;
const ACTION_INTERVAL_MS: u64 = 120;
const INPUT_WINDOW_MS: u64 = 750;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Cue {
    Focus,
    Confirm,
    Back,
    Refresh,
    RoomFound,
    JoinRequest,
    Ready,
    Start,
    Error,
    Restored,
}

impl Cue {
    const fn id(self) -> CueId {
        CueId::new(self as u16)
    }
}

use crate::settings::{SOUND_FILE, SoundPreferences};

pub fn install<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) -> SoundPreferences {
    use mirui::app::plugins::AudioPlugin;
    #[cfg(feature = "sound")]
    let sink = mirui::audio::SdlAudioSink::new();
    #[cfg(not(feature = "sound"))]
    let sink = mirui::audio::SilentAudioSink::default();
    app.add_plugin(AudioPlugin::new(sink, &bank::BANK));
    let preferences = SoundPreferences::load(Path::new(SOUND_FILE));
    if let Some(audio) = app.audio() {
        audio.set_master_gain(preferences.gain);
        audio.set_muted(!preferences.enabled);
        eprintln!(
            "{}",
            serde_json::json!({"ui_audio":format!("{:?}", audio.state()),"sdl_output":cfg!(feature="sound")})
        );
    }
    preferences
}

pub(crate) fn play(audio: &AudioHandle, cue: Option<Cue>) {
    if let Some(cue) = cue {
        if !audio
            .state()
            .is_some_and(|state| state.output == AudioOutputState::Ready && !state.muted)
        {
            return;
        }
        audio.stop(Cue::Focus.id());
        audio.stop(cue.id());
        audio.play(cue.id());
    }
}

#[model]
#[derive(Default)]
pub struct SoundState {
    feedback: Feedback,
    previous: Option<View>,
    pending: Option<Cue>,
}

#[model]
impl SoundState {
    pub fn present(&mut self, view: &View, at: u64) {
        if self.previous.as_ref() == Some(view) {
            return;
        }
        self.pending = self.feedback.observe(self.previous.as_ref(), view, at);
        self.previous = Some(view.clone());
    }

    pub fn input(&mut self, action: Action, at: u64) {
        self.pending = self
            .previous
            .as_ref()
            .and_then(|view| self.feedback.input(action, view, at));
    }
    pub fn focus_changed(&mut self, previous: Option<usize>, next: Option<usize>, at: u64) {
        if previous != next && elapsed(self.feedback.last_focus, at, NAVIGATION_INTERVAL_MS) {
            self.feedback.last_focus = Some(at);
            self.pending = Some(Cue::Focus);
        }
    }
    pub fn invalid_input(&mut self) {
        self.pending = Some(Cue::Error);
    }

    #[cfg(test)]
    pub fn theme_changed(&mut self) {
        self.pending = Some(Cue::Confirm);
    }

    #[effects]
    fn take_sound(&mut self) -> Option<Cue> {
        self.pending.take()
    }
}

pub fn register<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
) -> Result<SoundStateHandle, String> {
    let audio = app.audio().ok_or("Audio plugin is unavailable")?;
    let state = app.add_model(SoundState::default()).finish();
    state
        .on_effect(move |cue: &Cue| play(&audio, Some(*cue)))
        .map_err(|error| format!("Sound effect registration: {error:?}"))?;
    Ok(state)
}

#[cfg(all(target_os = "linux", feature = "sound"))]
pub fn probe() -> Result<(), String> {
    let mut app = App::headless(1, 1);
    app.with_default_widgets().with_default_systems();
    app.spawn_root().id();
    install(&mut app);
    let audio = app.audio().ok_or("Audio plugin is unavailable")?;
    let state = audio.state().ok_or("Audio state is unavailable")?;
    if state.output != AudioOutputState::Ready {
        return Err(format!("Audio output: {:?}", state.output));
    }
    play(&audio, Some(Cue::Confirm));
    app.render()
        .map_err(|error| format!("Audio dispatch: {error:?}"))?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    println!(
        "{}",
        serde_json::json!({"audio_output":"ready","muted":state.muted,"gain":state.master_gain,"cue":"confirm"})
    );
    Ok(())
}

#[derive(Default)]
struct Feedback {
    navigation_at: Option<u64>,
    last_focus: Option<u64>,
    last_action: Option<u64>,
    seen_rooms: BTreeSet<String>,
}

fn selection(view: &View) -> Option<String> {
    if view.launcher.enabled {
        if !view.launcher.overlay.is_empty() {
            return Some(format!(
                "launcher-menu:{}:{}",
                view.launcher.overlay, view.launcher.selected
            ));
        }
        if view.launcher.navigation {
            return Some(format!("launcher-tab:{}", view.launcher.tab));
        }
    }
    match view.page {
        Page::Games if view.library.keyboard => {
            Some(format!("keyboard:{}", view.library.key_selected))
        }
        Page::Mode | Page::Choice => Some(view.selected.to_string()),
        Page::Rooms | Page::Lobby => view.rooms.get(view.selected).map(|row| row.key.clone()),
        Page::Games | Page::Candidates => {
            view.entries.get(view.selected).map(|row| row.key.clone())
        }
        _ => None,
    }
    .filter(|key| !key.is_empty())
}

fn elapsed(last: Option<u64>, now: u64, interval: u64) -> bool {
    last.is_none_or(|last| now.saturating_sub(last) >= interval)
}

impl Feedback {
    pub fn input(&mut self, action: Action, view: &View, now: u64) -> Option<Cue> {
        if matches!(
            action,
            Action::Up
                | Action::Down
                | Action::Left
                | Action::Right
                | Action::PageUp
                | Action::PageDown
        ) {
            if !view.busy {
                self.navigation_at = Some(now);
            }
            return None;
        }
        let cue = match action {
            Action::Confirm
                if view
                    .launcher
                    .wifi
                    .as_ref()
                    .is_some_and(|wifi| !wifi.page.pending()) =>
            {
                Some(Cue::Confirm)
            }
            Action::Refresh
                if view.launcher.wifi.as_ref().is_some_and(|wifi| {
                    matches!(
                        wifi.page,
                        howff_menu::wifi::Page::List | howff_menu::wifi::Page::Empty
                    )
                }) =>
            {
                Some(Cue::Refresh)
            }
            Action::Confirm if view.launcher.enabled && !view.launcher.overlay.is_empty() => view
                .launcher
                .items
                .get(view.launcher.selected)
                .filter(|item| item.enabled() && !view.busy)
                .filter(|item| item.action != crate::model::MenuAction::CycleTheme)
                .map(|_| Cue::Confirm),
            Action::Auxiliary
            | Action::Start
            | Action::Refresh
            | Action::Favorite
            | Action::LauncherHome
                if view.page == Page::Games && !view.busy =>
            {
                Some(Cue::Confirm)
            }
            Action::Confirm if view.page == Page::Games && view.library.keyboard && !view.busy => {
                Some(Cue::Confirm)
            }
            Action::Back if view.page != Page::Leaving => Some(Cue::Back),
            Action::Theme if view.can_switch_theme() => Some(Cue::Confirm),
            Action::Refresh
                if matches!(view.scene_page(), Page::Rooms | Page::Lobby)
                    && !view.busy
                    && !view.refreshing =>
            {
                Some(Cue::Refresh)
            }
            Action::Confirm if !view.busy => match view.page {
                Page::Mode | Page::Choice => Some(Cue::Confirm),
                Page::Games | Page::Candidates
                    if view
                        .entries
                        .get(view.selected)
                        .is_some_and(|row| row.available) =>
                {
                    Some(Cue::Confirm)
                }
                Page::Rooms | Page::Lobby
                    if view
                        .rooms
                        .get(view.selected)
                        .is_some_and(|room| room.available) =>
                {
                    Some(Cue::Confirm)
                }
                Page::Host if view.host_action_enabled() => Some(Cue::Confirm),
                Page::Joined if view.network_only && view.host_action_enabled() => {
                    Some(Cue::Confirm)
                }
                Page::Leaving if matches!(view.phase, Phase::RestoreFailed | Phase::RoomActive) => {
                    Some(Cue::Confirm)
                }
                _ => None,
            },
            _ => None,
        };
        if cue.is_some() && elapsed(self.last_action, now, ACTION_INTERVAL_MS) {
            self.last_action = Some(now);
            cue
        } else {
            None
        }
    }

    pub fn observe(&mut self, previous: Option<&View>, next: &View, now: u64) -> Option<Cue> {
        if previous.is_none_or(|old| old.scene_page() != next.scene_page()) {
            self.seen_rooms.clear();
            self.navigation_at = None;
        }
        let mut found = false;
        if matches!(next.scene_page(), Page::Rooms | Page::Lobby) {
            for room in next
                .rooms
                .iter()
                .filter(|room| room.available && !room.key.is_empty())
            {
                found |= self.seen_rooms.insert(room.key.clone());
            }
        }
        let old = previous?;
        if let Some(wifi) = next.launcher.wifi.as_ref() {
            let before = old.launcher.wifi.as_ref();
            if wifi.page == howff_menu::wifi::Page::Success
                && before.is_none_or(|v| v.page != wifi.page)
            {
                return Some(Cue::Ready);
            }
            if matches!(
                wifi.page,
                howff_menu::wifi::Page::Failed | howff_menu::wifi::Page::Password
            ) && !wifi.notice.is_empty()
                && before.is_none_or(|v| v.notice != wifi.notice)
            {
                return Some(Cue::Error);
            }
            if wifi.page == howff_menu::wifi::Page::List
                && before.is_some_and(|v| {
                    matches!(
                        v.page,
                        howff_menu::wifi::Page::Restoring | howff_menu::wifi::Page::Cancelling
                    )
                })
            {
                return Some(Cue::Restored);
            }
        }
        if old.phase != next.phase {
            let cue = match next.phase {
                Phase::Failed | Phase::RestoreFailed => Some(Cue::Error),
                Phase::ApprovalPending => Some(Cue::JoinRequest),
                Phase::HostReady | Phase::WaitingStart if next.ready => Some(Cue::Ready),
                Phase::NetworkConnected if next.connected => Some(Cue::Ready),
                Phase::Starting => Some(Cue::Start),
                _ => None,
            };
            if cue.is_some() {
                return cue;
            }
        }
        if old.page == Page::Leaving
            && matches!(next.page, Page::Choice | Page::Rooms | Page::Lobby)
            && !next.busy
        {
            return Some(Cue::Restored);
        }
        if found {
            return Some(Cue::RoomFound);
        }
        if old.scene_page() == next.scene_page()
            && (old.page != next.page || selection(old) != selection(next))
            && selection(next).is_some()
            && self
                .navigation_at
                .is_some_and(|at| now.saturating_sub(at) <= INPUT_WINDOW_MS)
            && elapsed(self.last_focus, now, NAVIGATION_INTERVAL_MS)
        {
            self.last_focus = Some(now);
            self.navigation_at = None;
            return Some(Cue::Focus);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RoomRow;
    use mirui::audio::{AudioCommand, AudioMixer};

    #[test]
    fn plugin_controls_expire_with_the_menu() {
        use mirui::surface::framebuf::FramebufSurface;
        let mut app = App::new(FramebufSurface::new(16, 16, |_, _| {}));
        install(&mut app);
        let audio = app.audio().unwrap();
        assert_eq!(audio.state().unwrap().master_gain, 96);
        play(&audio, Some(Cue::Confirm));
        drop(app);
        assert!(audio.state().is_none());
    }

    #[test]
    fn all_cues_are_finite_nonzero_and_unclipped() {
        assert_eq!(bank::BANK.cues().len(), Cue::Restored as usize + 1);
        for cue in bank::BANK.cues() {
            let mut mixer = AudioMixer::<4>::new(&bank::BANK, 48_000).unwrap();
            mixer
                .apply(AudioCommand::Play {
                    cue: cue.id,
                    gain: 255,
                })
                .unwrap();
            let mut pcm = vec![0; 24_000];
            mixer.render_mono(&mut pcm);
            assert!(pcm.iter().any(|sample| *sample != 0));
            assert!(pcm.iter().all(|sample| sample.unsigned_abs() < 30_000));
            assert!(pcm[16_800..].iter().all(|sample| *sample == 0));
            assert_eq!(mixer.active_voice_count(), 0);
        }
    }

    #[test]
    fn observation_is_quiet_until_real_edges_and_readiness() {
        let mut feedback = Feedback::default();
        let mut view = View {
            page: Page::Host,
            ..View::default()
        };
        assert_eq!(feedback.observe(None, &view, 0), None);
        let old = view.clone();
        view.phase = Phase::HostReady;
        assert_eq!(feedback.observe(Some(&old), &view, 1), None);
        view.ready = true;
        assert_eq!(feedback.observe(Some(&old), &view, 2), Some(Cue::Ready));
        assert_eq!(feedback.observe(Some(&view), &view, 3), None);
        let old = view.clone();
        view.status = "Updated status".into();
        assert_eq!(feedback.observe(Some(&old), &view, 4), None);
    }

    #[test]
    fn discovery_and_reordering_do_not_replay_focus_or_discovery() {
        let mut feedback = Feedback::default();
        let old = View {
            page: Page::Rooms,
            ..View::default()
        };
        feedback.observe(None, &old, 0);
        let mut view = old.clone();
        view.rooms = vec![
            RoomRow {
                key: "one".into(),
                available: true,
                ..RoomRow::default()
            },
            RoomRow {
                key: "two".into(),
                available: true,
                ..RoomRow::default()
            },
        ];
        assert_eq!(feedback.observe(Some(&old), &view, 1), Some(Cue::RoomFound));
        let old = view.clone();
        view.rooms.reverse();
        view.selected = 1;
        view.rooms[0].signal = 50;
        assert_eq!(feedback.observe(Some(&old), &view, 2), None);
        feedback.input(Action::Up, &view, 100);
        let old = view.clone();
        view.selected = 0;
        assert_eq!(feedback.observe(Some(&old), &view, 101), Some(Cue::Focus));
        assert_eq!(feedback.observe(Some(&view), &view, 102), None);
    }

    #[test]
    fn lobby_sheet_focus_preserves_discovery_visit_and_navigation_cues() {
        let mut feedback = Feedback::default();
        let mut current = View {
            page: Page::Lobby,
            lobby: true,
            rooms: vec![RoomRow {
                key: "room".into(),
                available: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(feedback.observe(None, &current, 0), None);
        for (at, page, action) in [
            (100, Page::Mode, Action::Down),
            (300, Page::Lobby, Action::Up),
            (500, Page::Mode, Action::Down),
        ] {
            feedback.input(action, &current, at);
            let old = current.clone();
            current.page = page;
            assert_eq!(
                feedback.observe(Some(&old), &current, at + 1),
                Some(Cue::Focus)
            );
            assert_eq!(feedback.observe(Some(&current), &current, at + 2), None);
        }
        assert_eq!(
            feedback.input(Action::Refresh, &current, 800),
            Some(Cue::Refresh)
        );
    }

    #[test]
    fn disabled_actions_and_repeated_cleanup_stay_quiet() {
        let mut feedback = Feedback::default();
        assert_eq!(
            feedback.input(Action::Theme, &View::default(), 0),
            Some(Cue::Confirm)
        );
        feedback.last_action = None;
        let mut view = View {
            page: Page::Host,
            ..View::default()
        };
        assert_eq!(feedback.input(Action::Confirm, &view, 0), None);
        view.ready = true;
        assert_eq!(
            feedback.input(Action::Confirm, &view, 1),
            Some(Cue::Confirm)
        );
        assert_eq!(feedback.input(Action::Confirm, &view, 2), None);
        view.page = Page::Leaving;
        view.busy = true;
        assert_eq!(feedback.input(Action::Back, &view, 500), None);
        view.phase = Phase::RestoreFailed;
        view.busy = false;
        assert_eq!(
            feedback.input(Action::Confirm, &view, 501),
            Some(Cue::Confirm)
        );
        view.page = Page::Games;
        view.launcher.enabled = true;
        view.launcher.overlay = "Settings".into();
        view.launcher.overlay_kind = "system".into();
        view.launcher.items = vec![crate::model::LauncherItem {
            action: crate::model::MenuAction::CycleTheme,
            name: "Theme".into(),
            ..Default::default()
        }];
        assert_eq!(feedback.input(Action::Confirm, &view, 1000), None);
        view.launcher.overlay_kind.clear();
        view.launcher.items[0].action = crate::model::MenuAction::Back;
        view.launcher.items[0].availability = howff_menu::Availability::Unavailable {
            reason: "Unavailable".into(),
        };
        assert_eq!(feedback.input(Action::Confirm, &view, 1001), None);
        view.launcher.items[0].availability = howff_menu::Availability::Available;
        assert_eq!(
            feedback.input(Action::Confirm, &view, 1002),
            Some(Cue::Confirm)
        );
    }
}
