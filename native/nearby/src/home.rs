use crate::{Result, require};
use howff_platform::arkos;

pub const LAUNCHER: &str = arkos::firmware::ES_WRAPPER_RELATIVE;
pub const ORIGINAL_SHA: &str = "b277c8d1d5a8ed7f81734a55ba591a94a73c71789908782ccd439d2235baee1a";
const LINE: &str = "      \"$esdir/emulationstation\" \"$@\"\n      ret=$?";
static REPLACEMENT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    "      LD_PRELOAD=\"@INPUT_FILTER@${LD_PRELOAD:+:$LD_PRELOAD}\" \"$esdir/emulationstation\" \"$@\" &\n      nearby_es_pid=$!\n      sudo -n @HOWFF_BINARY@ home-listen \"$nearby_es_pid\" &\n      nearby_listener_pid=$!\n      wait \"$nearby_es_pid\"\n      ret=$?\n      kill \"$nearby_listener_pid\" 2>/dev/null || true".replace("@INPUT_FILTER@", arkos::application::INPUT_FILTER.as_str()).replace("@HOWFF_BINARY@", arkos::application::EXECUTABLE.as_str())
});

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultHome {
    #[default]
    Original,
    Howff,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub schema: u8,
    pub default_home: DefaultHome,
}

impl Default for Preference {
    fn default() -> Self {
        Self {
            schema: 1,
            default_home: DefaultHome::Original,
        }
    }
}

impl Preference {
    pub fn load_at(path: &std::path::Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let value: Self = serde_json::from_slice(&bytes)?;
                require(value.schema == 1, "Unsupported home preference")?;
                Ok(value)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn load() -> Result<Self> {
        Self::load_at(arkos::application::HOME_PREFERENCE.path())
    }

    pub fn save(home: DefaultHome) -> Result<()> {
        crate::session::while_idle(|| {
            crate::system::write(
                arkos::application::HOME_PREFERENCE.path(),
                &Self {
                    schema: 1,
                    default_home: home,
                },
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForegroundOrigin {
    #[default]
    BorrowedEs,
    StandaloneUnit,
    OriginalWrapper,
}

impl ForegroundOrigin {
    pub fn unit(self) -> &'static str {
        crate::platform::system::foreground_unit(self)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ForegroundLease {
    origin: ForegroundOrigin,
    definition_sha256: String,
    #[serde(default)]
    keep_network: bool,
}

pub fn foreground_origin() -> Result<ForegroundOrigin> {
    let path = arkos::runtime::HOME_OWNER.path();
    if !path.try_exists()? {
        return Ok(ForegroundOrigin::BorrowedEs);
    }
    Ok(crate::system::read::<ForegroundLease>(path)?.origin)
}

pub fn patched(original: &[u8]) -> Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    require(
        format!("{:x}", Sha256::digest(original)) == ORIGINAL_SHA,
        "Unsupported home launcher",
    )?;
    let text = std::str::from_utf8(original)?;
    require(text.matches(LINE).count() == 1, "Home dispatcher differs")?;
    let (first, rest) = text.split_once('\n').ok_or("Missing launcher header")?;
    require(first.starts_with("#!"), "Missing launcher interpreter")?;
    let dispatch = format!(
        "sudo -n {} home-dispatch || true\n",
        arkos::application::EXECUTABLE.as_str()
    );
    Ok(format!(
        "{first}\n{dispatch}{}",
        rest.replacen(LINE, &REPLACEMENT, 1)
    )
    .into_bytes())
}

#[cfg(target_os = "linux")]
mod device {
    use super::*;
    use crate::platform::system as platform_system;
    use crate::session;
    use crate::system::{read, write};
    use serde_json::json;
    use std::fs;
    use std::path::Path;
    use std::thread;
    use std::time::{Duration, Instant};

    const OWNER: &str = arkos::runtime::HOME_OWNER.as_str();
    const RETURN_ORIGINAL: &str = arkos::runtime::HOME_RETURN.as_str();

    pub fn dispatch() -> Result<()> {
        if crate::updates::recover_orphaned()? {
            resume()?;
        }
        if Path::new(RETURN_ORIGINAL).try_exists()? {
            fs::remove_file(RETURN_ORIGINAL)?;
            return Ok(());
        }
        if Preference::load()?.default_home != DefaultHome::Howff {
            return Ok(());
        }
        session::identity(false)?;
        require(
            !Path::new(OWNER).exists(),
            "Home interface already owns the display",
        )?;
        write(
            Path::new(OWNER),
            &ForegroundLease {
                origin: ForegroundOrigin::OriginalWrapper,
                definition_sha256: platform_system::definition()?,
                keep_network: false,
            },
        )?;
        let result = run_launcher(true);
        let recovery = resume();
        result.and(recovery)
    }

    pub fn standalone() -> Result<()> {
        session::identity(false)?;
        require(
            !platform_system::frontend_active(),
            "Original frontend still owns the display",
        )?;
        require(
            !Path::new(OWNER).exists(),
            "Home interface already owns the display",
        )?;
        write(
            Path::new(OWNER),
            &ForegroundLease {
                origin: ForegroundOrigin::StandaloneUnit,
                definition_sha256: platform_system::definition()?,
                keep_network: false,
            },
        )?;
        let result = run_launcher(true);
        let recovery = resume();
        result.and(recovery)
    }

    pub fn open_launcher(pid: u32) -> Result<()> {
        // The Options caller must return before its frontend service is stopped.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !platform_system::foreground(pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        open_mode(pid, true)
    }

    pub fn open(pid: u32) -> Result<()> {
        open_mode(pid, false)
    }

    fn open_mode(pid: u32, launcher: bool) -> Result<()> {
        session::identity(false)?;
        platform_system::validate_frontend(pid)?;
        require(
            !Path::new(OWNER).exists(),
            "Home interface already owns the display",
        )?;
        write(
            Path::new(OWNER),
            &ForegroundLease {
                origin: ForegroundOrigin::BorrowedEs,
                definition_sha256: platform_system::definition()?,
                keep_network: false,
            },
        )?;
        platform_system::stop_frontend()?;
        let result = run_launcher(launcher);
        let recovery = resume();
        result.and(recovery)
    }

    pub fn continued() -> Result<()> {
        let result = run_launcher_page(true, true);
        let recovery = resume();
        result.and(recovery)
    }
    fn run_launcher(launcher: bool) -> Result<()> {
        run_launcher_page(launcher, false)
    }
    fn run_launcher_page(launcher: bool, application: bool) -> Result<()> {
        let result = if application {
            crate::launcher::run_application()
        } else if launcher {
            crate::launcher::run()
        } else {
            crate::manual::run_manual()
        };
        if result.is_ok() {
            while session::state().is_ok_and(|state| {
                state.purpose == crate::purpose::SessionPurpose::Game && !state.closed()
            }) {
                thread::sleep(Duration::from_millis(250));
            }
            if session::state().is_ok_and(|state| {
                state.purpose == crate::purpose::SessionPurpose::NetworkOnly && state.established()
            }) {
                let mut owner: serde_json::Value = read(Path::new(OWNER))?;
                owner["keep_network"] = true.into();
                write(Path::new(OWNER), &owner)?;
            }
        }
        result.map(|_| ())
    }

    pub fn resume() -> Result<()> {
        crate::updates::recover_orphaned()?;
        if !Path::new(OWNER).is_file() {
            return Ok(());
        }
        let owner: serde_json::Value = read(Path::new(OWNER))?;
        require(
            owner["definition_sha256"] == platform_system::definition()?,
            "Frontend definition changed during shortcut session",
        )?;
        crate::launcher::tools::recover_pending()?;
        if let Ok(state) = session::state() {
            if (state.purpose == crate::purpose::SessionPurpose::Game
                || owner["keep_network"] != true)
                && (!state.closed() || !state.network_restored)
            {
                session::recover(&state.id, "home_exit")?;
            }
        }
        let origin = foreground_origin()?;
        if origin != ForegroundOrigin::OriginalWrapper {
            write(Path::new(RETURN_ORIGINAL), &json!({"return":"original"}))?;
            platform_system::start_frontend()?;
        }
        fs::remove_file(OWNER)?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use crate::platform::system::{activate, launch, launch_standalone, listen};
#[cfg(target_os = "linux")]
pub use device::{continued, dispatch, open, open_launcher, resume, standalone};

#[cfg(not(target_os = "linux"))]
pub fn continued() -> Result<()> {
    Err("Launcher requires Linux".into())
}

#[cfg(not(target_os = "linux"))]
pub fn dispatch() -> Result<()> {
    Err("Home dispatch requires Linux".into())
}
#[cfg(not(target_os = "linux"))]
pub fn standalone() -> Result<()> {
    Err("Launcher requires Linux".into())
}
#[cfg(not(target_os = "linux"))]
pub fn launch_standalone() -> Result<()> {
    Err("Launcher requires Linux".into())
}

#[cfg(not(target_os = "linux"))]
pub fn activate() -> Result<bool> {
    Ok(false)
}

#[cfg(not(target_os = "linux"))]
pub fn listen(_: u32) -> Result<()> {
    Err("Home shortcut requires Linux".into())
}
#[cfg(not(target_os = "linux"))]
pub fn open(_: u32) -> Result<()> {
    Err("Home shortcut requires Linux".into())
}
#[cfg(not(target_os = "linux"))]
pub fn launch() -> Result<()> {
    Err("Launcher requires Linux".into())
}
#[cfg(not(target_os = "linux"))]
pub fn open_launcher(_: u32) -> Result<()> {
    Err("Launcher requires Linux".into())
}
#[cfg(not(target_os = "linux"))]
pub fn resume() -> Result<()> {
    Err("Home shortcut requires Linux".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn foreground_leases_require_explicit_unit_ownership() {
        let old: ForegroundLease = serde_json::from_value(
            serde_json::json!({"origin":"borrowed_es","definition_sha256":"a","keep_network":true}),
        )
        .unwrap();
        assert_eq!(old.origin, ForegroundOrigin::BorrowedEs);
        assert_eq!(old.origin.unit(), "howff-launcher.service");
        assert_eq!(ForegroundOrigin::StandaloneUnit.unit(), old.origin.unit());
        assert_eq!(
            ForegroundOrigin::OriginalWrapper.unit(),
            "emulationstation.service"
        );
        assert!(serde_json::from_str::<ForegroundOrigin>("\"other.service\"").is_err());
    }

    #[test]
    fn default_home_requires_a_versioned_explicit_preference() {
        let root = std::env::temp_dir().join(crate::system::random_id().unwrap());
        std::fs::create_dir(&root).unwrap();
        let file = root.join("launcher-home.json");
        assert_eq!(
            Preference::load_at(&file).unwrap().default_home,
            DefaultHome::Original
        );
        for home in [DefaultHome::Original, DefaultHome::Howff] {
            crate::system::write(
                &file,
                &Preference {
                    schema: 1,
                    default_home: home,
                },
            )
            .unwrap();
            assert_eq!(Preference::load_at(&file).unwrap().default_home, home);
        }
        for bytes in [
            "{}",
            "{\"schema\":2,\"default_home\":\"nearby\"}",
            "{\"schema\":1,\"default_home\":\"command\"}",
        ] {
            std::fs::write(&file, bytes).unwrap();
            assert!(Preference::load_at(&file).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod path_contract {
    #[test]
    fn generated_home_integration_uses_howff_owned_resources() {
        let source = &*super::REPLACEMENT;
        assert!(source.contains(howff_platform::arkos::application::EXECUTABLE.as_str()));
        assert!(source.contains(howff_platform::arkos::application::INPUT_FILTER.as_str()));
        assert!(!source.contains("@HOWFF_BINARY@"));
        assert!(!source.contains("@INPUT_FILTER@"));
        assert!(source.contains("home-listen"));
        assert!(source.contains("wait \"$nearby_es_pid\""));
        assert!(source.contains("kill \"$nearby_listener_pid\""));
    }
}
