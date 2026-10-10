use std::path::Path;

use serde::{Deserialize, Serialize};

pub fn preferred(game: &Path) -> Option<&'static str> {
    if !matches!(
        game.components().nth(1)?.as_os_str().to_str()?,
        "roms" | "roms2"
    ) {
        return None;
    }
    let platform = game.components().nth(2)?.as_os_str().to_str()?;
    match platform {
        "gb" | "gbc" => Some("tgbdual"),
        "gba" => Some("gpsp"),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkMode {
    GbDual,
    #[serde(
        alias = "rfu",
        alias = "pokemon_cable",
        alias = "advance_wars_cable",
        alias = "advance_wars2_cable"
    )]
    GbaAuto,
}

impl LinkMode {
    pub fn options(self) -> &'static str {
        match self {
            Self::GbDual => concat!(
                "tgbdual_gblink_enable = \"enabled\"\n",
                "tgbdual_screen_placement = \"left-right\"\n",
                "tgbdual_switch_screens = \"normal\"\n",
                "tgbdual_single_screen_mp = \"both players\"\n",
                "tgbdual_audio_output = \"Game Boy #1\"\n",
            ),
            Self::GbaAuto => "gpsp_serial = \"auto\"\n",
        }
    }
}

pub fn select(core: &str) -> Option<LinkMode> {
    match core {
        "tgbdual" => Some(LinkMode::GbDual),
        "gpsp" => Some(LinkMode::GbaAuto),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linked_platforms_have_fixed_local_core_choices() {
        assert_eq!(preferred(Path::new("/roms/gb/test.zip")), Some("tgbdual"));
        assert_eq!(preferred(Path::new("/roms2/gbc/test.zip")), Some("tgbdual"));
        assert_eq!(preferred(Path::new("/roms/gba/test.zip")), Some("gpsp"));
        assert_eq!(preferred(Path::new("/roms/nes/test.zip")), None);
    }
    #[test]
    fn gba_protocol_selection_is_owned_by_the_core() {
        assert_eq!(select("gpsp"), Some(LinkMode::GbaAuto));
        assert_eq!(LinkMode::GbaAuto.options(), "gpsp_serial = \"auto\"\n");
        for persisted in [
            "rfu",
            "pokemon_cable",
            "advance_wars_cable",
            "advance_wars2_cable",
            "gba_auto",
        ] {
            let restored: LinkMode = serde_json::from_value(persisted.into()).unwrap();
            assert_eq!(restored, LinkMode::GbaAuto);
            assert_eq!(serde_json::to_value(restored).unwrap(), "gba_auto");
        }
    }

    #[test]
    fn link_mode_depends_on_the_selected_core() {
        assert_eq!(select("gpsp"), Some(LinkMode::GbaAuto));
        assert_eq!(select("tgbdual"), Some(LinkMode::GbDual));
        for core in ["mgba", "gambatte", "snes9x", "fbneo", ""] {
            assert_eq!(select(core), None);
        }
    }
}
