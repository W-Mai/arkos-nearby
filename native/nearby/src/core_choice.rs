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

pub fn arcade_candidate(game: &Path, core_id: &str) -> Option<Choice> {
    (matches!(core_id, "mame" | "fbneo_plus") && candidates(game) == [ARCADE_CORE])
        .then_some(ARCADE_CORE)
}

pub fn candidates(game: &Path) -> &'static [Choice] {
    const NES: &[Choice] = &[core("fceumm"), core("nestopia")];
    const SNES: &[Choice] = &[core("snes9x2005"), core("snes9x"), core("chimerasnes")];
    const SEGA: &[Choice] = &[core("genesis_plus_gx"), core("picodrive")];
    const SEGA32X: &[Choice] = &[core("picodrive")];
    const PCE: &[Choice] = &[core("mednafen_pce_fast"), core("mednafen_pce")];
    const ARCADE: &[Choice] = &[core("fbneo")];
    let mut parts = game.components();
    if parts.next() != Some(std::path::Component::RootDir)
        || !parts
            .next()
            .is_some_and(|part| matches!(part.as_os_str().to_str(), Some("roms" | "roms2")))
    {
        return &[];
    }
    match parts.next().and_then(|part| part.as_os_str().to_str()) {
        Some("nes" | "fds") => NES,
        Some("snes") => SNES,
        Some("megadrive" | "mastersystem" | "segacd") => SEGA,
        Some("sega32x") => SEGA32X,
        Some("pcengine") => PCE,
        Some("arcade" | "neogeo" | "cps1" | "cps2" | "cps3") => ARCADE,
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
            let choices = candidates(&Path::new("/roms").join(platform).join("game.zip"));
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
            "/roms/gb/game.zip",
            "/roms/gba/game.zip",
            "/roms/nds/game.zip",
            "/roms/psp/game.iso",
            "/roms/dreamcast/game.chd",
        ] {
            assert!(candidates(Path::new(path)).is_empty());
        }
        assert_eq!(
            candidates(Path::new("/roms2/psx/game.pbp"))[0].frontend,
            "retroarch32"
        );
    }

    #[test]
    fn arcade_selection_depends_on_platform_and_core_family() {
        for root in ["/roms", "/roms2"] {
            for platform in ["arcade", "neogeo", "cps1", "cps2", "cps3"] {
                let path = Path::new(root).join(platform).join("any-local-game.zip");
                for id in ["mame", "fbneo_plus"] {
                    assert_eq!(arcade_candidate(&path, id), Some(ARCADE_CORE));
                }
                for id in ["fbneo", "mame2003_plus", "fceumm"] {
                    assert_eq!(arcade_candidate(&path, id), None);
                }
            }
        }
        assert!(arcade_candidate(Path::new("/roms/nes/game.zip"), "mame").is_none());
        assert!(arcade_candidate(Path::new("/tmp/arcade/game.zip"), "mame").is_none());
    }
}
