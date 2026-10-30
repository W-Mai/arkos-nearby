use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    pub frontend: &'static str,
    pub id: &'static str,
}

const fn core(id: &'static str) -> Choice {
    Choice {
        frontend: "retroarch",
        id,
    }
}

pub const ARCADE_CORE: Choice = core("fbneo");

pub fn arcade_order(game: &Path) -> &'static [Choice] {
    const ARCADE: &[Choice] = &[ARCADE_CORE, core("fbneo_plus"), core("mame")];
    match platform(game) {
        Some("arcade" | "neogeo" | "cps1" | "cps2" | "cps3") => ARCADE,
        _ => &[],
    }
}

pub fn arcade_family(core_id: &str) -> bool {
    matches!(core_id, "fbneo" | "fbneo_plus" | "mame")
}

pub fn arcade_options(game: &Path, core_id: &str) -> Option<&'static str> {
    (core_id == "mame" && !arcade_order(game).is_empty())
        .then_some("mame_softlists_enable = \"disabled\"\nmame_media_type = \"rom\"\n")
}

fn platform(game: &Path) -> Option<&str> {
    let mut parts = game.components();
    if parts.next() != Some(std::path::Component::RootDir)
        || !parts
            .next()
            .is_some_and(|part| matches!(part.as_os_str().to_str(), Some("roms" | "roms2")))
    {
        return None;
    }
    parts.next()?.as_os_str().to_str()
}

pub fn core_order(game: &Path) -> &'static [Choice] {
    const GB: &[Choice] = &[core("tgbdual")];
    const GBA: &[Choice] = &[core("gpsp")];
    const NES: &[Choice] = &[core("nestopia_nearby"), core("nestopia"), core("fceumm")];
    const SNES: &[Choice] = &[
        core("snes9x"),
        core("snes9x2010"),
        core("chimerasnes"),
        core("snes9x2005"),
    ];
    const SEGA: &[Choice] = &[core("genesis_plus_gx"), core("picodrive")];
    const SEGA32X: &[Choice] = &[core("picodrive")];
    const PCE: &[Choice] = &[core("mednafen_pce_fast"), core("mednafen_pce")];
    match platform(game) {
        Some("gb" | "gbc") => GB,
        Some("gba") => GBA,
        Some("nes" | "fds") => NES,
        Some("snes") => SNES,
        Some("megadrive" | "mastersystem" | "segacd") => SEGA,
        Some("sega32x") => SEGA32X,
        Some("pcengine") => PCE,
        Some("arcade" | "neogeo" | "cps1" | "cps2" | "cps3") => arcade_order(game),
        Some("psx") => &[
            Choice {
                frontend: "retroarch32",
                id: "pcsx_rearmed",
            },
            Choice {
                frontend: "retroarch32",
                id: "pcsx_rearmed_unai",
            },
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_are_fixed_installed_registry_identifiers() {
        for platform in [
            "nes",
            "fds",
            "snes",
            "megadrive",
            "mastersystem",
            "segacd",
            "sega32x",
            "pcengine",
            "arcade",
            "neogeo",
            "cps1",
            "cps2",
            "cps3",
            "psx",
        ] {
            let choices = core_order(&Path::new("/roms").join(platform).join("game.zip"));
            assert!(!choices.is_empty());
            for choice in choices {
                assert!(matches!(choice.frontend, "retroarch" | "retroarch32"));
                assert!(choice.id.bytes().all(|byte| byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || byte == b'_'));
            }
        }
    }

    #[test]
    fn unsupported_protocols_and_foreign_paths_have_no_fallback() {
        for path in [
            "/tmp/nes/game.zip",
            "roms/nes/game.zip",
            "/roms/nds/game.zip",
            "/roms/psp/game.iso",
            "/roms/dreamcast/game.chd",
        ] {
            assert!(core_order(Path::new(path)).is_empty());
        }
        assert_eq!(
            core_order(Path::new("/roms2/psx/game.pbp"))[0].frontend,
            "retroarch32"
        );
    }

    #[test]
    fn every_entry_uses_the_same_platform_order() {
        for root in ["/roms", "/roms2"] {
            for platform in ["arcade", "neogeo", "cps1", "cps2", "cps3"] {
                let path = Path::new(root).join(platform).join("any-local-game.zip");
                let choices = arcade_order(&path);
                assert_eq!(
                    choices.iter().map(|choice| choice.id).collect::<Vec<_>>(),
                    ["fbneo", "fbneo_plus", "mame"]
                );
                assert_eq!(choices, core_order(&path));
                for id in ["fbneo", "fbneo_plus", "mame"] {
                    assert_eq!(arcade_options(&path, id).is_some(), id == "mame");
                }
            }
        }
        assert_eq!(
            core_order(Path::new("/roms/nes/game.zip"))[0].id,
            "nestopia_nearby"
        );
        assert_eq!(core_order(Path::new("/roms/snes/game.zip"))[0].id, "snes9x");
        assert!(arcade_order(Path::new("/roms/nes/game.zip")).is_empty());
        assert!(arcade_options(Path::new("/roms/nes/game.zip"), "mame").is_none());
    }
}
