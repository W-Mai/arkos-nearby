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

// Game-code protocol assignments from gpSP 74db5e5 gba_over.h.
pub fn gba_serial(header: &[u8]) -> Option<&'static str> {
    let code = header.get(0xac..0xb0)?;
    match code {
        b"B2WE" | b"B3AE" | b"B4UE" | b"B4UP" | b"B85A" | b"B85P" | b"BDGE" | b"BDGP" | b"BG3E"
        | b"BKRJ" | b"BMGD" | b"BMGE" | b"BMGF" | b"BMGI" | b"BMGJ" | b"BMGP" | b"BMGS"
        | b"BMGU" | b"BPED" | b"BPEE" | b"BPEF" | b"BPEI" | b"BPEJ" | b"BPES" | b"BPGD"
        | b"BPGE" | b"BPGF" | b"BPGI" | b"BPGJ" | b"BPGS" | b"BPRD" | b"BPRE" | b"BPRF"
        | b"BPRI" | b"BPRJ" | b"BPRS" | b"BR5E" | b"BR6E" | b"BRBE" | b"BRKE" | b"BTME"
        | b"BTMJ" | b"BTMP" => Some("rfu"),
        b"AXPD" | b"AXPE" | b"AXPF" | b"AXPI" | b"AXPJ" | b"AXPS" | b"AXVD" | b"AXVE" | b"AXVF"
        | b"AXVI" | b"AXVJ" | b"AXVS" => Some("mul_poke"),
        b"AWRE" | b"AWRP" => Some("mul_aw1"),
        b"AW2E" | b"AW2P" => Some("mul_aw2"),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkMode {
    GbDual,
    Rfu,
    PokemonCable,
    AdvanceWarsCable,
    AdvanceWars2Cable,
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
            Self::Rfu => "gpsp_serial = \"rfu\"\n",
            Self::PokemonCable => "gpsp_serial = \"mul_poke\"\n",
            Self::AdvanceWarsCable => "gpsp_serial = \"mul_aw1\"\n",
            Self::AdvanceWars2Cable => "gpsp_serial = \"mul_aw2\"\n",
        }
    }
}

pub fn select(core: &str, header: &[u8]) -> Option<LinkMode> {
    match core {
        "tgbdual" => Some(LinkMode::GbDual),
        "gpsp" => match gba_serial(header)? {
            "rfu" => Some(LinkMode::Rfu),
            "mul_poke" => Some(LinkMode::PokemonCable),
            "mul_aw1" => Some(LinkMode::AdvanceWarsCable),
            "mul_aw2" => Some(LinkMode::AdvanceWars2Cable),
            _ => None,
        },
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
    fn gba_modes_require_a_recognized_local_game_header() {
        for (code, expected) in [
            (b"BPRE", "rfu"),
            (b"AXVE", "mul_poke"),
            (b"AWRE", "mul_aw1"),
            (b"AW2E", "mul_aw2"),
        ] {
            let mut header = [0; 192];
            header[0xac..0xb0].copy_from_slice(code);
            assert_eq!(gba_serial(&header), Some(expected));
        }
        assert_eq!(gba_serial(&[0; 192]), None);
        assert_eq!(gba_serial(b"BPRE"), None);
    }

    #[test]
    fn link_mode_depends_on_wire_bound_core_and_content_only() {
        let mut header = [0; 192];
        header[0xac..0xb0].copy_from_slice(b"BPRE");
        assert_eq!(select("gpsp", &header), Some(LinkMode::Rfu));
        assert_eq!(select("mgba", &header), None);
        assert_eq!(select("tgbdual", &header), Some(LinkMode::GbDual));
        assert_eq!(select("gpsp", &[0; 192]), None);
    }
}
