use crate::{Result, require};

pub const LAUNCHER: &str = "usr/bin/emulationstation/emulationstation.sh";
pub const ORIGINAL_SHA: &str = "b277c8d1d5a8ed7f81734a55ba591a94a73c71789908782ccd439d2235baee1a";
const LINE: &str = "      \"$esdir/emulationstation\" \"$@\"\n      ret=$?";
const REPLACEMENT: &str = "      LD_PRELOAD=\"/opt/arkos-nearby/assets/home-input.so${LD_PRELOAD:+:$LD_PRELOAD}\" \"$esdir/emulationstation\" \"$@\" &\n      nearby_es_pid=$!\n      sudo -n /opt/arkos-nearby/arkos-nearby home-listen \"$nearby_es_pid\" &\n      nearby_listener_pid=$!\n      wait \"$nearby_es_pid\"\n      ret=$?\n      kill \"$nearby_listener_pid\" 2>/dev/null || true";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultHome {
    #[default]
    Original,
    Nearby,
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
        Self::load_at(&std::path::Path::new(crate::context::INSTALL).join("launcher-home.json"))
    }

    pub fn save(home: DefaultHome) -> Result<()> {
        crate::session::while_idle(|| {
            crate::system::write(
                &std::path::Path::new(crate::context::INSTALL).join("launcher-home.json"),
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
        match self {
            Self::OriginalWrapper => "emulationstation.service",
            Self::BorrowedEs | Self::StandaloneUnit => "arkos-nearby-launcher.service",
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ForegroundLease {
    #[serde(default)]
    origin: ForegroundOrigin,
    definition_sha256: String,
    #[serde(default)]
    keep_network: bool,
}

pub fn foreground_origin() -> Result<ForegroundOrigin> {
    let path = std::path::Path::new("/run/arkos-nearby-rust/home-owner.json");
    if !path.try_exists()? {
        return Ok(ForegroundOrigin::BorrowedEs);
    }
    Ok(crate::system::read::<ForegroundLease>(path)?.origin)
}

#[cfg(any(target_os = "linux", test))]
fn parent_pid(stat: &[u8]) -> Option<u32> {
    std::str::from_utf8(&stat[stat.iter().rposition(|byte| *byte == b')')? + 1..])
        .ok()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
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
    let dispatch = "sudo -n /opt/arkos-nearby/arkos-nearby home-dispatch || true\n";
    Ok(format!("{first}\n{dispatch}{}", rest.replacen(LINE, REPLACEMENT, 1)).into_bytes())
}

#[derive(Default)]
#[cfg(any(target_os = "linux", test))]
struct Chord {
    function: bool,
    x: bool,
    latched: bool,
}
#[cfg(any(target_os = "linux", test))]
impl Chord {
    fn input(&mut self, code: u16, value: i32) -> bool {
        if value == 2 {
            return false;
        }
        match code {
            708 => self.function = value == 1,
            307 => self.x = value == 1,
            _ => return false,
        }
        let pressed = self.function && self.x;
        let trigger = pressed && !self.latched;
        self.latched = pressed;
        trigger
    }
}

#[cfg(target_os = "linux")]
mod device {
    use super::*;
    use crate::session::{self, best, cmd};
    use crate::system::{Lock, mkdir, read, write};
    use serde_json::json;
    use std::fs::{self, OpenOptions};
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;
    use std::thread;
    use std::time::{Duration, Instant};

    const ES: &str = "emulationstation.service";
    const UNIT: &str = "/etc/systemd/system/emulationstation.service";
    const OWNER: &str = "/run/arkos-nearby-rust/home-owner.json";
    const RETURN_ORIGINAL: &str = "/run/arkos-nearby-rust/home-return-original.json";

    pub fn dispatch() -> Result<()> {
        if crate::updates::recover_orphaned()? {
            resume()?;
        }
        if Path::new(RETURN_ORIGINAL).try_exists()? {
            fs::remove_file(RETURN_ORIGINAL)?;
            return Ok(());
        }
        if Preference::load()?.default_home != DefaultHome::Nearby {
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
                definition_sha256: crate::content::digest(Path::new(UNIT))?,
                keep_network: false,
            },
        )?;
        let result = run_launcher(true);
        let recovery = resume();
        result.and(recovery)
    }

    pub fn launch_standalone() -> Result<()> {
        session::identity(false)?;
        if best(&["systemctl", "is-active", ES]) == "active" {
            return launch();
        }
        cmd(&[
            "systemd-run",
            "--collect",
            "--unit=arkos-nearby-launcher",
            "--property=ExecStopPost=/opt/arkos-nearby/arkos-nearby home-resume",
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=10",
            session::BINARY,
            "launcher-standalone",
        ])?;
        Ok(())
    }

    pub fn standalone() -> Result<()> {
        session::identity(false)?;
        require(
            best(&["systemctl", "is-active", ES]) != "active",
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
                definition_sha256: crate::content::digest(Path::new(UNIT))?,
                keep_network: false,
            },
        )?;
        let result = run_launcher(true);
        let recovery = resume();
        result.and(recovery)
    }

    pub fn activate() -> Result<bool> {
        if best(&["systemctl", "is-active", ES]) != "active" {
            return Ok(false);
        }
        for entry in fs::read_dir("/proc")?.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let path = entry.path();
            if !fs::read_link(path.join("exe"))
                .is_ok_and(|exe| exe == Path::new("/usr/bin/emulationstation/emulationstation"))
                || !fs::read_to_string(path.join("cgroup")).is_ok_and(|group| {
                    group
                        .lines()
                        .any(|line| line.ends_with("/system.slice/emulationstation.service"))
                })
            {
                continue;
            }
            let unit = format!("arkos-nearby-home-listen-{pid}");
            if best(&["systemctl", "is-active", &unit]) != "active" {
                cmd(&[
                    "systemd-run",
                    "--collect",
                    &format!("--unit={unit}"),
                    session::BINARY,
                    "home-listen",
                    &pid.to_string(),
                ])?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn foreground(pid: u32) -> bool {
        let root = Path::new("/proc").join(pid.to_string());
        fs::read_link(root.join("exe"))
            .is_ok_and(|path| path == Path::new("/usr/bin/emulationstation/emulationstation"))
            && fs::read_dir("/proc").is_ok_and(|processes| {
                processes.flatten().all(|process| {
                    if process
                        .file_name()
                        .to_string_lossy()
                        .parse::<u32>()
                        .is_err()
                    {
                        return true;
                    }
                    match fs::read(process.path().join("stat")) {
                        Ok(stat) => parent_pid(&stat).is_some_and(|parent| parent != pid),
                        Err(error) => {
                            error.kind() == std::io::ErrorKind::NotFound
                                || error.raw_os_error() == Some(libc::ESRCH)
                        }
                    }
                })
            })
    }

    fn settled_foreground(pid: u32) -> bool {
        // Let short-lived frontend helpers finish without queuing a game-time shortcut.
        let deadline = Instant::now() + Duration::from_millis(250);
        loop {
            if foreground(pid) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn listen(pid: u32) -> Result<()> {
        session::identity(false)?;
        mkdir(Path::new(session::BASE))?;
        let _lock =
            Lock::try_acquire(&Path::new(session::BASE).join(format!("home-listen-{pid}.lock")))?;
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/input/by-path/platform-odroidgo3-joypad-event-joystick")?;
        let mut chord = Chord::default();
        let mut event = [0u8; 24];
        let process = Path::new("/proc").join(pid.to_string());
        let identity = fs::read_to_string(process.join("stat"))?
            .split_whitespace()
            .nth(21)
            .ok_or("Missing frontend start time")?
            .to_string();
        while fs::read_to_string(process.join("stat"))
            .is_ok_and(|stat| stat.split_whitespace().nth(21) == Some(identity.as_str()))
        {
            match file.read(&mut event) {
                Ok(24) if u16::from_ne_bytes([event[16], event[17]]) == 1 => {
                    let code = u16::from_ne_bytes([event[18], event[19]]);
                    let value = i32::from_ne_bytes(event[20..24].try_into().unwrap());
                    if chord.input(code, value) && settled_foreground(pid) {
                        let _ = cmd(&[
                            "systemd-run",
                            "--collect",
                            "--unit=arkos-nearby-home",
                            "--property=ExecStopPost=/opt/arkos-nearby/arkos-nearby home-resume",
                            session::BINARY,
                            "home-open",
                            &pid.to_string(),
                        ]);
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20))
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub fn launch() -> Result<()> {
        session::identity(false)?;
        let pid = fs::read_dir("/proc")?
            .flatten()
            .find_map(|entry| {
                fs::read_link(entry.path().join("exe"))
                    .ok()
                    .filter(|exe| exe == Path::new("/usr/bin/emulationstation/emulationstation"))
                    .and_then(|_| entry.file_name().to_string_lossy().parse::<u32>().ok())
            })
            .ok_or("Original frontend is unavailable")?;
        cmd(&[
            "systemd-run",
            "--collect",
            "--unit=arkos-nearby-launcher",
            "--property=ExecStopPost=/opt/arkos-nearby/arkos-nearby home-resume",
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=10",
            session::BINARY,
            "launcher-open",
            &pid.to_string(),
        ])?;
        Ok(())
    }

    pub fn open_launcher(pid: u32) -> Result<()> {
        // The Options caller must return before its frontend service is stopped.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !foreground(pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        open_mode(pid, true)
    }

    pub fn open(pid: u32) -> Result<()> {
        open_mode(pid, false)
    }

    fn open_mode(pid: u32, launcher: bool) -> Result<()> {
        session::identity(false)?;
        require(
            settled_foreground(pid),
            "Home shortcut requires the foreground menu",
        )?;
        require(
            best(&["systemctl", "is-active", ES]) == "active"
                && cmd(&["systemctl", "show", ES, "-p", "FragmentPath", "--value"])? == UNIT,
            "Unknown frontend owner",
        )?;
        let group = fs::read_to_string(format!("/proc/{pid}/cgroup"))?;
        require(
            group
                .lines()
                .any(|line| line.ends_with("/system.slice/emulationstation.service")),
            "Unowned home frontend",
        )?;
        require(
            !Path::new(OWNER).exists(),
            "Home interface already owns the display",
        )?;
        write(
            Path::new(OWNER),
            &ForegroundLease {
                origin: ForegroundOrigin::BorrowedEs,
                definition_sha256: crate::content::digest(Path::new(UNIT))?,
                keep_network: false,
            },
        )?;
        cmd(&["systemctl", "stop", ES])?;
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
            owner["definition_sha256"] == crate::content::digest(Path::new(UNIT))?,
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
            cmd(&["systemctl", "start", ES])?;
        }
        fs::remove_file(OWNER)?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use device::{
    activate, continued, dispatch, launch, launch_standalone, listen, open, open_launcher, resume,
    standalone,
};

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
    fn foreground_origins_and_legacy_leases_have_closed_unit_ownership() {
        let old: ForegroundLease = serde_json::from_value(
            serde_json::json!({"definition_sha256":"a","keep_network":true}),
        )
        .unwrap();
        assert_eq!(old.origin, ForegroundOrigin::BorrowedEs);
        assert_eq!(old.origin.unit(), "arkos-nearby-launcher.service");
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
        for home in [DefaultHome::Original, DefaultHome::Nearby] {
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
    #[test]
    fn process_identity_does_not_depend_on_kernel_children_support() {
        assert_eq!(parent_pid(b"1020 (emulationstatio) S 761 761"), Some(761));
        assert_eq!(
            parent_pid(b"1050 (name with ) spaces) R 1020 761"),
            Some(1020)
        );
        assert_eq!(parent_pid(b"118 (driver\xc8) S 2 0"), Some(2));
        assert_eq!(parent_pid(b"1050 (invalid)"), None);
    }
    #[test]
    fn shortcut_requires_both_keys_and_release_before_repeating() {
        let mut chord = Chord::default();
        assert!(!chord.input(307, 1));
        assert!(chord.input(708, 1));
        assert!(!chord.input(307, 2));
        assert!(!chord.input(708, 1));
        assert!(!chord.input(307, 0));
        assert!(chord.input(307, 1));
        assert!(!chord.input(708, 0));
        assert!(!chord.input(307, 1));
    }
}
