use crate::theme::Palette;
use mirui::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "linux", test))]
use std::path::Path;

#[cfg(any(target_os = "linux", test))]
pub const SOUND_FILE: &str = "/opt/arkos-nearby/ui-sound.json";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SoundPreferences {
    pub enabled: bool,
    pub gain: u8,
}

impl Default for SoundPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            gain: 96,
        }
    }
}

impl SoundPreferences {
    #[cfg(any(target_os = "linux", test))]
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(data) => serde_json::from_slice(&data).unwrap_or_else(|error| {
                eprintln!("Sound preferences: {error}");
                Self::default()
            }),
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("Sound preferences: {error}");
                }
                Self::default()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    #[cfg(any(target_os = "linux", test))]
    Theme(Palette),
    Sound(SoundPreferences),
}

#[model]
#[derive(Default)]
pub struct Settings {
    #[observe]
    pub palette: Palette,
    #[observe]
    pub sound: SoundPreferences,
    pending: Option<Change>,
}

#[model]
impl Settings {
    pub fn restore(&mut self, palette: Palette, sound: SoundPreferences) {
        self.palette = palette;
        self.set_sound(sound);
        self.pending = None;
    }

    #[cfg(any(target_os = "linux", test))]
    pub fn cycle_theme(&mut self) {
        self.palette = self.palette.next();
        self.pending = Some(Change::Theme(self.palette));
    }

    pub fn set_sound(&mut self, sound: SoundPreferences) {
        if self.sound != sound {
            self.sound = sound;
            self.pending = Some(Change::Sound(sound));
        }
    }

    #[effects]
    fn take_change(&mut self) -> Option<Change> {
        self.pending.take()
    }
}

impl SettingsHandle {
    pub fn menu_value(&self, action: nearby_menu::MenuAction) -> &'static str {
        match action {
            nearby_menu::MenuAction::CycleTheme => self.palette().label(),
            nearby_menu::MenuAction::ToggleSound => {
                if self.sound().enabled {
                    "开"
                } else {
                    "关"
                }
            }
            _ => "",
        }
    }
}

#[cfg(any(target_os = "linux", test))]
pub fn connect(
    settings: &SettingsHandle,
    audio: mirui::audio::AudioHandle,
    theme_path: std::path::PathBuf,
    sound_path: std::path::PathBuf,
    confirm_theme: impl Fn() + 'static,
) -> Result<(), String> {
    settings
        .on_effect(move |change: &Change| {
            let result = match *change {
                Change::Theme(palette) => {
                    confirm_theme();
                    palette.save(&theme_path)
                }
                Change::Sound(preferences) => {
                    audio.set_master_gain(preferences.gain);
                    audio.set_muted(!preferences.enabled);
                    save_sound(preferences, &sound_path)
                }
            };
            if let Err(error) = result {
                eprintln!("Interface preferences: {error}");
            }
        })
        .map_err(|error| format!("Settings effect registration: {error:?}"))
}

#[cfg(any(target_os = "linux", test))]
fn save_sound(preferences: SoundPreferences, path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temporary)?;
    serde_json::to_writer(&mut file, &preferences)?;
    writeln!(file)?;
    file.sync_all()?;
    std::fs::rename(temporary, path)?;
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}
