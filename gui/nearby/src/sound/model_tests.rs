use super::*;
use crate::model::RoomRow;
use mirui::app::plugins::AudioPlugin;
use mirui::audio::{AudioBank, AudioCommand, AudioSink};
use mirui::surface::FramebufferAccess;
use std::cell::RefCell;
use std::convert::Infallible;
use std::rc::Rc;

#[derive(Clone, Default)]
struct RecordingSink(Rc<RefCell<Vec<AudioCommand>>>);

impl AudioSink for RecordingSink {
    type Error = Infallible;

    fn start(&mut self, _: &'static AudioBank) -> Result<(), Self::Error> {
        Ok(())
    }

    fn submit(&mut self, command: AudioCommand) -> Result<(), Self::Error> {
        self.0.borrow_mut().push(command);
        Ok(())
    }
}

fn setup() -> (App<impl FramebufferAccess>, SoundStateHandle, RecordingSink) {
    let mut app = App::headless(16, 16);
    app.with_default_widgets().with_default_systems();
    app.spawn_root().id();
    let sink = RecordingSink::default();
    app.add_plugin(AudioPlugin::new(sink.clone(), &bank::BANK));
    let state = register(&mut app).unwrap();
    (app, state, sink)
}

fn view(page: Page, phase: Phase) -> View {
    View {
        page,
        phase,
        ..View::default()
    }
}

#[test]
fn wifi_editor_focus_and_validation_use_the_existing_model_audio_effects() {
    let (mut app, state, sink) = setup();
    app.audio().unwrap().set_muted(false);
    state.focus_changed(Some(0), Some(1), 1000);
    app.render().unwrap();
    state.focus_changed(Some(1), Some(1), 1100);
    app.render().unwrap();
    state.invalid_input();
    app.render().unwrap();
    let commands = sink.0.borrow();
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command,AudioCommand::Play{cue,..} if *cue==Cue::Focus.id()))
            .count(),
        1
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command,AudioCommand::Play{cue,..} if *cue==Cue::Error.id()))
            .count(),
        1
    );
}

enum Delivery {
    View { view: Rc<View>, at: u64 },
    Input { action: Action, at: u64 },
    ThemeChanged,
}

fn presentation(view: &View, at: u64) -> Delivery {
    Delivery::View {
        view: Rc::new(view.clone()),
        at,
    }
}

#[test]
fn model_effects_preserve_policy_commands_and_dispatch_in_the_same_frame() {
    let (mut app, state, sink) = setup();
    let audio = app.audio().unwrap();
    let mut policy = Feedback::default();
    let mut previous: Option<Rc<View>> = None;
    let mut expected = Vec::new();
    let mut at = 0;
    let mut deliver = |delivery: Delivery| {
        let cue = match &delivery {
            Delivery::View { view, at } => {
                let cue = if previous.as_deref() != Some(view) {
                    policy.observe(previous.as_deref(), view, *at)
                } else {
                    None
                };
                previous = Some(Rc::clone(view));
                cue
            }
            Delivery::Input { action, at } => previous
                .as_deref()
                .and_then(|view| policy.input(*action, view, *at)),
            Delivery::ThemeChanged => Some(Cue::Confirm),
        };
        if let Some(cue) = cue {
            expected.extend([
                AudioCommand::Stop {
                    cue: Cue::Focus.id(),
                },
                AudioCommand::Stop { cue: cue.id() },
                AudioCommand::Play {
                    cue: cue.id(),
                    gain: 255,
                },
            ]);
        }
        match delivery {
            Delivery::View { view, at } => state.present(&view, at),
            Delivery::Input { action, at } => state.input(action, at),
            Delivery::ThemeChanged => state.theme_changed(),
        }
        app.render().unwrap();
        assert_eq!(*sink.0.borrow(), expected);
        app.render().unwrap();
        assert_eq!(
            *sink.0.borrow(),
            expected,
            "rendering cannot replay a delivery"
        );
        assert_eq!(audio.state().unwrap().output, AudioOutputState::Ready);
    };
    let mut rooms = view(Page::Rooms, Phase::default());
    deliver(presentation(&rooms, at));
    for index in 0..3 {
        rooms.rooms.push(RoomRow {
            key: index.to_string(),
            available: true,
            ..RoomRow::default()
        });
        at += 150;
        deliver(presentation(&rooms, at));
    }
    rooms.rooms.reverse();
    rooms.selected = 2;
    deliver(presentation(&rooms, at + 1));
    deliver(Delivery::Input {
        action: Action::Down,
        at: at + 2,
    });
    rooms.selected = 1;
    deliver(presentation(&rooms, at + 3));
    for action in [
        Action::Confirm,
        Action::Confirm,
        Action::Refresh,
        Action::Back,
    ] {
        at += 150;
        deliver(Delivery::Input { action, at });
    }
    for (page, phase) in [
        (Page::Host, Phase::ApprovalPending),
        (Page::Host, Phase::HostReady),
        (Page::Joined, Phase::WaitingStart),
        (Page::Joined, Phase::NetworkConnected),
        (Page::Host, Phase::Starting),
        (Page::Leaving, Phase::Failed),
        (Page::Leaving, Phase::RestoreFailed),
        (Page::Choice, Phase::default()),
        (Page::Rooms, Phase::default()),
    ] {
        let mut next = view(page, phase);
        next.ready = true;
        next.connected = true;
        next.network_only = true;
        at += 150;
        deliver(presentation(&next, at));
        next.status = "Status refresh".into();
        deliver(presentation(&next, at + 1));
        deliver(Delivery::Input {
            action: Action::Confirm,
            at: at + 2,
        });
    }
    deliver(Delivery::ThemeChanged);
    deliver(Delivery::ThemeChanged);
}

#[test]
fn repeated_inputs_keep_delivery_order_and_theme_confirmation_keeps_its_policy() {
    let (mut app, state, sink) = setup();
    state.present(&view(Page::Choice, Phase::default()), 0);
    state.input(Action::Confirm, 200);
    state.input(Action::Confirm, 201);
    state.input(Action::Back, 400);
    state.theme_changed();
    state.theme_changed();
    app.render().unwrap();
    let played: Vec<_> = sink
        .0
        .borrow()
        .iter()
        .filter_map(|command| match command {
            AudioCommand::Play { cue, .. } => Some(*cue),
            _ => None,
        })
        .collect();
    assert_eq!(
        played,
        [
            Cue::Confirm.id(),
            Cue::Back.id(),
            Cue::Confirm.id(),
            Cue::Confirm.id()
        ]
    );
}

#[test]
fn audio_controls_and_repeated_state_cannot_replay_an_input() {
    let (mut app, state, sink) = setup();
    state.present(&view(Page::Choice, Phase::default()), 0);
    let audio = app.audio().unwrap();
    audio.set_muted(true);
    state.input(Action::Confirm, 200);
    app.render().unwrap();
    audio.set_muted(false);
    audio.set_master_gain(80);
    app.render().unwrap();
    assert!(
        !sink
            .0
            .borrow()
            .iter()
            .any(|command| matches!(command, AudioCommand::Play { .. }))
    );
    state.input(Action::Confirm, 400);
    app.render().unwrap();
    let commands = sink.0.borrow().len();
    assert!(state.on_effect(|_: &Cue| {}).is_err());
    state.present(&view(Page::Choice, Phase::default()), 500);
    app.render().unwrap();
    assert_eq!(sink.0.borrow().len(), commands);
    drop(app);
    assert!(audio.state().is_none());
}

#[test]
fn settings_effects_preserve_confirmation_and_existing_preference_files() {
    use crate::settings::SoundPreferences;
    use crate::theme::Palette;
    let (mut app, _sound, sink) = setup();
    crate::render::install(&mut app).unwrap();
    let settings = crate::render::settings(&mut app);
    let directory = std::env::temp_dir().join(format!("nearby-settings-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let theme_path = directory.join("ui-theme");
    let sound_path = directory.join("ui-sound.json");
    std::fs::write(&theme_path, "sunset\n").unwrap();
    std::fs::write(&sound_path, r#"{"enabled":true,"gain":81}"#).unwrap();
    let preferences = SoundPreferences::load(&sound_path);
    settings.restore(Palette::load(&theme_path), preferences);
    let confirmation = app.audio().unwrap();
    crate::settings::connect(
        &settings,
        app.audio().unwrap(),
        theme_path.clone(),
        sound_path.clone(),
        move || play(&confirmation, Some(Cue::Confirm)),
    )
    .unwrap();
    sink.0.borrow_mut().clear();
    settings.restore(Palette::Sunset, preferences);
    assert!(sink.0.borrow().is_empty());
    settings.cycle_theme();
    app.render().unwrap();
    assert_eq!(Palette::load(&theme_path), Palette::Night);
    assert!(
        sink.0
            .borrow()
            .iter()
            .any(|command| matches!(command, AudioCommand::Play { .. }))
    );
    sink.0.borrow_mut().clear();
    let muted = SoundPreferences {
        enabled: false,
        gain: 73,
    };
    settings.set_sound(muted);
    app.render().unwrap();
    assert_eq!(SoundPreferences::load(&sound_path), muted);
    assert_eq!(app.audio().unwrap().state().unwrap().master_gain, 73);
    assert!(app.audio().unwrap().state().unwrap().muted);
    sink.0.borrow_mut().clear();
    settings.set_sound(muted);
    app.render().unwrap();
    assert!(
        !sink
            .0
            .borrow()
            .iter()
            .any(|command| matches!(command, AudioCommand::Play { .. }))
    );
    std::fs::remove_dir_all(directory).unwrap();
}
