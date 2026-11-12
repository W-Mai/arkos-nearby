use howff_platform::arkos;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::{Result, require};

pub const ROOT: &str = arkos::application::CORES.as_str();
pub const STOCK_NESTOPIA_SHA: &str =
    "dcf01334577a81d5a1f4331b2d4aac9fba2a3308a5442ec0818d7599e624c086";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Optimized,
    Device,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Optimized => "工具优化",
            Self::Device => "设备原有",
        }
    }
}

pub struct Entry {
    pub id: &'static str,
    pub frontend: &'static str,
    pub filename: &'static str,
    pub info_filename: &'static str,
    pub frontend_info: &'static str,
    pub sha256: &'static str,
    pub bits: u8,
    pub name: &'static str,
    pub version: &'static str,
}

pub const NESTOPIA: Entry = Entry {
    id: "nestopia_nearby",
    frontend: "retroarch",
    filename: "nestopia_nearby_libretro.so",
    info_filename: "nestopia_libretro.info",
    frontend_info: include_str!("../../owned_cores/nestopia/nestopia_libretro.info"),
    sha256: "2c276c3338e7df8105213f7c2008a8e2a0cd307a89f40f19fbde05fc2c84f430",
    bits: 64,
    name: "Nestopia",
    version: "1.53.27dfdc25-footer1",
};

impl Entry {
    pub fn frontend_configuration(&self, root: &Path) -> Result<String> {
        use std::os::unix::fs::PermissionsExt;

        let directory = root.join("core-info");
        crate::system::mkdir(&directory)?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))?;
        let path = directory.join(Path::new(self.filename).with_extension("info"));
        if std::fs::read(&path).ok().as_deref() != Some(self.frontend_info.as_bytes()) {
            crate::system::private_write(&path, self.frontend_info.as_bytes())?;
        }
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
        Ok(format!(
            "libretro_directory = \"{ROOT}\"\nlibretro_info_path = \"{}\"\n",
            directory.display()
        ))
    }

    pub fn path(&self) -> PathBuf {
        Path::new(ROOT).join(self.filename)
    }

    pub fn validate_metadata(&self, metadata: &Value) -> Result<()> {
        require(
            metadata["pointer_bits"].as_u64() == Some(u64::from(self.bits))
                && metadata["library_name"].as_str() == Some(self.name)
                && metadata["library_version"].as_str() == Some(self.version)
                && metadata["valid_extensions"].as_str() == Some("nes|fds|unf|unif")
                && metadata["need_fullpath"].as_bool() == Some(false)
                && metadata["block_extract"].as_bool() == Some(false),
            "Owned core metadata differs from the installed artifact",
        )
    }
}

pub fn entry(id: &str) -> Option<&'static Entry> {
    (id == NESTOPIA.id).then_some(&NESTOPIA)
}

pub fn by_path(path: &Path, frontend: &str) -> Option<&'static Entry> {
    (frontend == NESTOPIA.frontend && path == NESTOPIA.path()).then_some(&NESTOPIA)
}

pub fn replacement(
    frontend: &str,
    core_id: &str,
    sha256: &str,
    bits: u8,
    game: &Path,
) -> Option<&'static Entry> {
    let mut parts = game.components();
    let nes = parts.next() == Some(Component::RootDir)
        && parts
            .next()
            .is_some_and(|part| matches!(part.as_os_str().to_str(), Some("roms" | "roms2")))
        && parts.next().is_some_and(|part| part.as_os_str() == "nes");
    (frontend == "retroarch"
        && core_id == "nestopia"
        && sha256 == STOCK_NESTOPIA_SHA
        && bits == 64
        && nes)
        .then_some(&NESTOPIA)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn owned_registry_binds_one_local_path_and_exact_metadata() {
        assert!(crate::protocol::hex(NESTOPIA.sha256, 64));
        assert!(crate::protocol::hex(STOCK_NESTOPIA_SHA, 64));
        assert_eq!(NESTOPIA.path().parent(), Some(Path::new(ROOT)));
        assert_eq!(entry(NESTOPIA.id).unwrap().sha256, NESTOPIA.sha256);
        assert!(entry("nestopia").is_none());
        assert!(by_path(&NESTOPIA.path(), "retroarch").is_some());
        assert!(by_path(&NESTOPIA.path(), "retroarch32").is_none());
        assert!(by_path(Path::new("/tmp/nestopia_nearby_libretro.so"), "retroarch").is_none());
        assert_eq!(
            crate::context::resolve_core("retroarch", NESTOPIA.id).unwrap(),
            NESTOPIA.path()
        );
        assert!(crate::context::resolve_core("retroarch32", NESTOPIA.id).is_err());
        for id in ["", "../nestopia_nearby", "/tmp/core", "nestopia/extra"] {
            assert!(crate::context::resolve_core("retroarch", id).is_err());
        }
        let valid = json!({"pointer_bits":64,"library_name":NESTOPIA.name,
            "library_version":NESTOPIA.version,"valid_extensions":"nes|fds|unf|unif",
            "need_fullpath":false,"block_extract":false});
        NESTOPIA.validate_metadata(&valid).unwrap();
        for key in [
            "pointer_bits",
            "library_name",
            "library_version",
            "valid_extensions",
            "need_fullpath",
            "block_extract",
        ] {
            let mut changed = valid.clone();
            changed[key] = Value::Null;
            assert!(NESTOPIA.validate_metadata(&changed).is_err());
        }
    }

    #[test]
    fn registry_identity_matches_the_packaged_source_record() {
        use sha2::{Digest, Sha256};
        let source: Value =
            serde_json::from_str(include_str!("../../owned_cores/nestopia/source.json")).unwrap();
        assert_eq!(source["id"], NESTOPIA.id);
        assert_eq!(source["artifact_sha256"], NESTOPIA.sha256);
        assert_eq!(source["info_filename"], NESTOPIA.info_filename);
        assert_eq!(
            source["info_sha256"],
            format!("{:x}", Sha256::digest(NESTOPIA.frontend_info.as_bytes()))
        );
        NESTOPIA.validate_metadata(&source["metadata"]).unwrap();
    }

    #[test]
    fn replacement_is_limited_to_the_verified_nes_core() {
        for game in ["/roms/nes/game.zip", "/roms2/nes/game.nes"] {
            assert!(
                replacement(
                    "retroarch",
                    "nestopia",
                    STOCK_NESTOPIA_SHA,
                    64,
                    Path::new(game)
                )
                .is_some()
            );
        }
        for (frontend, id, hash, bits, game) in [
            (
                "retroarch32",
                "nestopia",
                STOCK_NESTOPIA_SHA,
                64,
                "/roms/nes/a.nes",
            ),
            (
                "retroarch",
                "fceumm",
                STOCK_NESTOPIA_SHA,
                64,
                "/roms/nes/a.nes",
            ),
            (
                "retroarch",
                "nestopia",
                NESTOPIA.sha256,
                64,
                "/roms/nes/a.nes",
            ),
            (
                "retroarch",
                "nestopia",
                STOCK_NESTOPIA_SHA,
                32,
                "/roms/nes/a.nes",
            ),
            (
                "retroarch",
                "nestopia",
                STOCK_NESTOPIA_SHA,
                64,
                "/roms/fds/a.fds",
            ),
            (
                "retroarch",
                "nestopia",
                STOCK_NESTOPIA_SHA,
                64,
                "/tmp/nes/a.nes",
            ),
        ] {
            assert!(replacement(frontend, id, hash, bits, Path::new(game)).is_none());
        }
    }
}
