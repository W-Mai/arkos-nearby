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

#[derive(Clone, Copy, Debug)]
pub struct PerformanceRule {
    source: Choice,
    source_sha256: &'static str,
    pub target: Choice,
    target_sha256: &'static str,
    elf_bits: u8,
    content_sha256: &'static str,
}

impl PerformanceRule {
    fn matches_source(
        &self,
        frontend: &str,
        core_id: &str,
        core_sha256: &str,
        elf_bits: u8,
        content_sha256: &str,
    ) -> bool {
        self.source.frontend == frontend
            && self.source.id == core_id
            && self.source_sha256 == core_sha256
            && self.elf_bits == elf_bits
            && self.content_sha256 == content_sha256
    }

    pub fn matches_target(
        &self,
        frontend: &str,
        core_id: &str,
        core_sha256: &str,
        elf_bits: u8,
    ) -> bool {
        self.target.frontend == frontend
            && self.target.id == core_id
            && self.target_sha256 == core_sha256
            && self.elf_bits == elf_bits
    }
}

const FFIGHT: PerformanceRule = PerformanceRule {
    source: core("mame"),
    source_sha256: "adbb2488b7d170432f8acca2f4a665fec760adcf361f363c2c3cfc2aa6cfe1a6",
    target: core("fbneo"),
    target_sha256: "514a62b76a6eb6ca33db7beec0966949fe9711190f9719e9e650c771055f5dea",
    elf_bits: 64,
    content_sha256: "a7cc8894488ee126851ee1f45eccb99e6da96f63b5bbd31689adaf08f7a37eab",
};

const PERFORMANCE_RULES: &[PerformanceRule] = &[FFIGHT];

pub fn performance_candidate(
    frontend: &str,
    core_id: &str,
    core_sha256: &str,
    elf_bits: u8,
    content_sha256: &str,
) -> Option<&'static PerformanceRule> {
    PERFORMANCE_RULES
        .iter()
        .find(|rule| rule.matches_source(frontend, core_id, core_sha256, elf_bits, content_sha256))
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
    fn performance_selection_requires_the_tested_content_source_and_target() {
        assert_eq!(PERFORMANCE_RULES.len(), 1);
        for tested in PERFORMANCE_RULES {
            let rule = performance_candidate(
                tested.source.frontend,
                tested.source.id,
                tested.source_sha256,
                tested.elf_bits,
                tested.content_sha256,
            )
            .unwrap();
            assert_eq!(rule.target, tested.target);
            for (frontend, id, digest, bits, content) in [
                (
                    "retroarch32",
                    tested.source.id,
                    tested.source_sha256,
                    64,
                    tested.content_sha256,
                ),
                (
                    "retroarch",
                    tested.target.id,
                    tested.source_sha256,
                    64,
                    tested.content_sha256,
                ),
                (
                    "retroarch",
                    tested.source.id,
                    tested.target_sha256,
                    64,
                    tested.content_sha256,
                ),
                (
                    "retroarch",
                    tested.source.id,
                    tested.source_sha256,
                    32,
                    tested.content_sha256,
                ),
                (
                    "retroarch",
                    tested.source.id,
                    tested.source_sha256,
                    64,
                    tested.source_sha256,
                ),
            ] {
                assert!(performance_candidate(frontend, id, digest, bits, content).is_none());
            }
            assert!(rule.matches_target("retroarch", tested.target.id, tested.target_sha256, 64));
            assert!(!rule.matches_target(
                "retroarch32",
                tested.target.id,
                tested.target_sha256,
                64
            ));
            assert!(!rule.matches_target("retroarch", tested.source.id, tested.target_sha256, 64));
            assert!(!rule.matches_target("retroarch", tested.target.id, tested.source_sha256, 64));
            assert!(!rule.matches_target("retroarch", tested.target.id, tested.target_sha256, 32));
            assert!(crate::protocol::hex(tested.source_sha256, 64));
            assert!(crate::protocol::hex(tested.target_sha256, 64));
            assert!(crate::protocol::hex(tested.content_sha256, 64));
        }
    }
}
