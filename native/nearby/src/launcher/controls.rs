mod worker;

use crate::{Result, require};
use nearby_menu::SystemLevel;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
pub(super) use worker::Controls;
use worker::{Driver, Reading};

struct Hardware;

impl Driver for Hardware {
    fn read(&mut self, control: SystemLevel) -> std::result::Result<Reading, String> {
        let result = (|| -> Result<Reading> {
            Ok(match control {
                SystemLevel::Volume => {
                    let mixer = Mixer::load()?;
                    Reading {
                        value: mixer.percent,
                        muted: mixer.muted,
                    }
                }
                SystemLevel::Brightness => {
                    let light = Backlight::discover(Path::new("/sys/class/backlight"))?;
                    Reading {
                        value: (u64::from(light.current) * 100)
                            .div_ceil(u64::from(light.maximum))
                            .min(100) as u8,
                        muted: false,
                    }
                }
            })
        })();
        result.map_err(|error| format!("读取{}失败：{error}", control.label()))
    }

    fn write(&mut self, control: SystemLevel, target: u8) -> std::result::Result<Reading, String> {
        let result = (|| -> Result<()> {
            match control {
                SystemLevel::Volume => {
                    let mixer = Mixer::load()?;
                    let delta = i16::from(target) - i16::from(mixer.percent);
                    if delta != 0 {
                        let amount = format!(
                            "{}%{}",
                            delta.unsigned_abs(),
                            if delta > 0 { "+" } else { "-" }
                        );
                        amixer(&mixer.card, &["sset", &mixer.selector, "playback", &amount])?;
                    }
                }
                SystemLevel::Brightness => {
                    Backlight::discover(Path::new("/sys/class/backlight"))?.adjust(target)?
                }
            }
            Ok(())
        })();
        result.map_err(|error| format!("调整{}失败：{error}", control.label()))?;
        self.read(control)
    }
}

fn amixer(card: &str, arguments: &[&str]) -> Result<String> {
    // Keep reads and relative writes on the hardware's linear volume scale.
    // Some mixer drivers expose invalid dB ranges to mapped-volume conversion.
    let mut args = vec!["LC_ALL=C", "amixer", "-R", "-D", card];
    args.extend_from_slice(arguments);
    crate::system::command("/usr/bin/env", &args, 3, true)
}

fn mixer_settings() -> (String, Option<String>) {
    let mut values = [
        "/home/ark/.emulationstation/es_settings.cfg",
        "/etc/emulationstation/es_settings.cfg",
    ]
    .into_iter()
    .find_map(|path| {
        let text = fs::read_to_string(path).ok()?;
        let values = super::recipes::settings(&text);
        (values.contains_key("AudioDevice") || values.contains_key("AudioCard")).then_some(values)
    })
    .unwrap_or_default();
    (
        values
            .remove("AudioCard")
            .filter(|card| !card.is_empty())
            .unwrap_or_else(|| "default".into()),
        values.remove("AudioDevice"),
    )
}

struct Mixer {
    card: String,
    name: String,
    selector: String,
    percent: u8,
    muted: bool,
}

impl Mixer {
    fn load() -> Result<Self> {
        let (card, preferred) = mixer_settings();
        let text = amixer(&card, &["scontents"])?;
        let mut mixer = Self::select(&text, preferred.as_deref())?;
        mixer.card = card;
        Ok(mixer)
    }
    fn select(text: &str, preferred: Option<&str>) -> Result<Self> {
        let mut mixers: Vec<_> = text
            .split("Simple mixer control '")
            .skip(1)
            .filter_map(Self::parse)
            .collect();
        let index = preferred
            .and_then(|name| mixers.iter().position(|m| m.name == name))
            .or_else(|| {
                ["Master", "Playback", "PCM", "Speaker", "Headphone"]
                    .into_iter()
                    .find_map(|name| mixers.iter().position(|m| m.name == name))
            })
            .or_else(|| (mixers.len() == 1).then_some(0));
        Ok(mixers.remove(index.ok_or("No unambiguous playback volume control")?))
    }

    fn parse(block: &str) -> Option<Self> {
        let (name, rest) = block.split_once("',")?;
        let (index, content) = rest.split_once('\n')?;
        let index = index.trim().parse::<u32>().ok()?;
        let capabilities = content
            .lines()
            .find_map(|line| line.trim().strip_prefix("Capabilities:"))?;
        if !capabilities
            .split_whitespace()
            .any(|cap| matches!(cap, "pvolume" | "volume"))
        {
            return None;
        }
        let channels = content
            .lines()
            .find_map(|line| line.trim().strip_prefix("Playback channels:"))?;
        let mut levels = Vec::new();
        let mut muted = true;
        for line in content.lines().filter(|line| {
            channels
                .split(" - ")
                .any(|channel| line.trim().starts_with(&format!("{}:", channel.trim())))
                && line.contains("%]")
        }) {
            let percent = line
                .split('[')
                .find_map(|tail| tail.split_once("%]").map(|v| v.0))?
                .parse::<u8>()
                .ok()?;
            if percent > 100 {
                return None;
            }
            levels.push(u32::from(percent));
            muted &= line.contains("[off]");
        }
        if levels.is_empty() {
            return None;
        }
        Some(Self {
            card: "default".into(),
            name: name.into(),
            selector: format!("{name},{index}"),
            percent: (levels.iter().sum::<u32>() / levels.len() as u32) as u8,
            muted,
        })
    }

    #[cfg(test)]
    fn label(&self) -> String {
        format!(
            "{}%{}",
            self.percent,
            if self.muted { " · 静音" } else { "" }
        )
    }
}

struct Backlight {
    path: PathBuf,
    current: u32,
    maximum: u32,
}

impl Backlight {
    fn discover(root: &Path) -> Result<Self> {
        let mut devices = Vec::new();
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.join("brightness").is_file() && path.join("max_brightness").is_file() {
                devices.push(path);
            }
        }
        require(devices.len() == 1, "Expected one display backlight")?;
        let light = Self::read(&devices[0])?;
        OpenOptions::new()
            .write(true)
            .open(light.path.join("brightness"))?;
        Ok(light)
    }

    fn read(path: &Path) -> Result<Self> {
        let current = fs::read_to_string(path.join("brightness"))?
            .trim()
            .parse()?;
        let maximum = fs::read_to_string(path.join("max_brightness"))?
            .trim()
            .parse()?;
        require(maximum > 0 && current <= maximum, "Invalid backlight range")?;
        Ok(Self {
            path: path.into(),
            current,
            maximum,
        })
    }

    fn adjust(&self, percent: u8) -> Result<()> {
        let current = Self::read(&self.path)?;
        let value = (u64::from(current.maximum) * u64::from(percent.min(100)))
            .div_ceil(100)
            .max(1) as u32;
        fs::write(current.path.join("brightness"), format!("{value}\n"))?;
        Self::read(&current.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIXERS: &str = "Simple mixer control 'Capture',0\n  Capabilities: cvolume cswitch\n  Capture channels: Mono\n  Limits: Capture 0 - 100\n  Mono: Capture 90 [90%] [on]\nSimple mixer control 'Playback',0\n  Capabilities: pvolume pswitch\n  Playback channels: Front Left - Front Right\n  Limits: Playback 0 - 255\n  Front Left: Playback 40 [40%] [-20.00dB] [off]\n  Front Right: Playback 60 [60%] [-12.00dB] [off]\n";

    #[test]
    fn shared_volume_controls_require_playback_channels() {
        let shared = "Simple mixer control 'Playback',0\n  Capabilities: volume\n  Playback channels: Front Left - Front Right\n  Capture channels: Front Left - Front Right\n  Limits: 0 - 255\n  Front Left: 0 [0%] [-72.00dB]\n  Front Right: 0 [0%] [-72.00dB]\nSimple mixer control 'Record',0\n  Capabilities: volume\n  Playback channels: Front Left - Front Right\n  Capture channels: Front Left - Front Right\n  Limits: 0 - 255\n  Front Left: 255 [100%]\n  Front Right: 255 [100%]\n";
        let mixer = Mixer::select(shared, None).unwrap();
        assert_eq!(mixer.selector, "Playback,0");
        assert_eq!(mixer.label(), "0%");
        assert!(
            Mixer::select(
                &shared.replace("Playback channels:", "Other channels:"),
                None
            )
            .is_err()
        );
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "nearby-controls-{}",
                crate::system::random_id().unwrap()
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn light(&self, name: &str, maximum: u32, current: u32) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("max_brightness"), maximum.to_string()).unwrap();
            fs::write(path.join("brightness"), current.to_string()).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn playback_selection_and_menu_values_exclude_capture_controls() {
        let mixer = Mixer::select(MIXERS, Some("Capture")).unwrap();
        assert_eq!(mixer.selector, "Playback,0");
        assert_eq!(mixer.label(), "50% · 静音");
        assert!(
            Mixer::select(
                MIXERS
                    .split("Simple mixer control 'Playback'")
                    .next()
                    .unwrap(),
                None
            )
            .is_err()
        );
        let item =
            nearby_menu::MenuItem::available(SystemLevel::Volume.action(), "音量").with_level(
                nearby_menu::Level::new(SystemLevel::Volume, Some(50), true, false),
            );
        let encoded = serde_json::to_vec(&item).unwrap();
        assert_eq!(item, serde_json::from_slice(&encoded).unwrap());
        assert!(item.value.is_empty());
    }

    #[test]
    fn backlight_discovery_and_adjustment_preserve_hardware_bounds() {
        let fixture = Fixture::new();
        for maximum in [1, 2, 15, 100, 255, 4095, u32::MAX] {
            let path = fixture.light("panel", maximum, maximum / 2);
            let light = Backlight::discover(&fixture.0).unwrap();
            for initial in [0, 1, maximum.saturating_sub(1), maximum] {
                for target in [0, 1, 50, 100] {
                    // Adjustment reads hardware again after an external update.
                    fs::write(path.join("brightness"), initial.to_string()).unwrap();
                    light.adjust(target).unwrap();
                    let observed = Backlight::read(&path).unwrap();
                    assert!((1..=maximum).contains(&observed.current));
                }
            }
        }
        fixture.light("other", 100, 50);
        let before = fs::read(fixture.0.join("panel/brightness")).unwrap();
        assert!(Backlight::discover(&fixture.0).is_err());
        assert_eq!(
            fs::read(fixture.0.join("panel/brightness")).unwrap(),
            before
        );
    }
}
