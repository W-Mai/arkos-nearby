//! ArkOS frontend ownership, input listener, and display launch environment.

use crate::home::ForegroundOrigin;
use crate::session::{best, cmd};
use crate::system::{read, write};
use crate::{Result, require};
use howff_platform::arkos;
use serde_json::{Value, json};
use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Stdio};

const ES: &str = "emulationstation.service";
const UNIT: &str = arkos::firmware::ES_UNIT;

pub(crate) fn foreground_unit(origin: ForegroundOrigin) -> &'static str {
    match origin {
        ForegroundOrigin::OriginalWrapper => ES,
        ForegroundOrigin::BorrowedEs | ForegroundOrigin::StandaloneUnit => "howff-launcher.service",
    }
}

pub(crate) fn gui_command(launcher: bool, log: File) -> Command {
    let mut command = Command::new(arkos::application::GUI.path());
    command
        .env("HOWFF_LAUNCHER_INPUT", if launcher { "1" } else { "0" })
        .env("SDL_INPUT_LINUX_KEEP_KBD", "1")
        .env("SDL_RENDER_BATCHING", "1")
        .env("TERM", "linux")
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log));
    command
}

pub(crate) fn pause_game(path: &Path, game: &mut Value) -> Result<()> {
    let pids = best(&["pgrep", "-x", "emulationstatio"]);
    let active = best(&["systemctl", "is-active", ES]) == "active";
    require(
        pids.is_empty() || active,
        "An unrelated menu owns the display",
    )?;
    for pid in pids.lines() {
        let groups = fs::read_to_string(Path::new("/proc").join(pid).join("cgroup"))?;
        require(
            groups.lines().any(|line| {
                line.rsplit(':').next() == Some("/system.slice/emulationstation.service")
            }),
            "Display process is not owned by the installed menu service",
        )?;
    }
    if active {
        require(
            cmd(&["systemctl", "show", ES, "-p", "FragmentPath", "--value"])? == UNIT,
            "Unexpected menu service definition",
        )?;
        game["frontend_service"] =
            json!({"unit":ES,"definition_sha256":crate::content::digest(Path::new(UNIT))?});
        write(path, game)?;
        cmd(&["systemctl", "stop", ES])?;
    }
    Ok(())
}

pub(crate) fn resume_game(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut game: Value = read(path)?;
    if game["frontend_service"].is_object() {
        require(
            game["frontend_service"]["unit"] == ES
                && game["frontend_service"]["definition_sha256"]
                    == crate::content::digest(Path::new(UNIT))?,
            "Menu ownership changed during game handoff",
        )?;
        if best(&["systemctl", "is-active", ES]) != "active" {
            cmd(&["systemctl", "start", ES])?;
        }
        game.as_object_mut().unwrap().remove("frontend_service");
        write(path, &game)?;
    }
    Ok(())
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

#[cfg(any(target_os = "linux", test))]
fn listener_for_frontend(bytes: &[u8], pid: u32) -> bool {
    let mut args = bytes.split(|byte| *byte == 0);
    args.next() == Some(crate::session::BINARY.as_bytes())
        && args.next() == Some(b"home-listen".as_slice())
        && args.next() == Some(pid.to_string().as_bytes())
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
    use crate::session;
    use crate::system::{Lock, mkdir};
    use std::fs::OpenOptions;
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    use std::thread;
    use std::time::{Duration, Instant};

    pub(crate) fn definition() -> Result<String> {
        crate::content::digest(Path::new(UNIT))
    }

    pub(crate) fn frontend_active() -> bool {
        best(&["systemctl", "is-active", ES]) == "active"
    }

    pub(crate) fn stop_frontend() -> Result<()> {
        cmd(&["systemctl", "stop", ES])?;
        Ok(())
    }

    pub(crate) fn start_frontend() -> Result<()> {
        cmd(&["systemctl", "start", ES])?;
        Ok(())
    }

    pub(crate) fn validate_frontend(pid: u32) -> Result<()> {
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
        Ok(())
    }

    pub fn launch_standalone() -> Result<()> {
        session::identity(false)?;
        if best(&["systemctl", "is-active", ES]) == "active" {
            return launch();
        }
        cmd(&[
            "systemd-run",
            "--collect",
            "--unit=howff-launcher",
            &format!(
                "--property=ExecStopPost={} home-resume",
                arkos::application::EXECUTABLE.as_str()
            ),
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=10",
            session::BINARY,
            "launcher-standalone",
        ])?;
        Ok(())
    }

    fn service_frontend() -> Result<Option<u32>> {
        if best(&["systemctl", "is-active", ES]) != "active" {
            return Ok(None);
        }
        for entry in fs::read_dir("/proc")?.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let path = entry.path();
            if !fs::read_link(path.join("exe"))
                .is_ok_and(|exe| exe == Path::new(arkos::firmware::ES_EXECUTABLE))
                || !fs::read_to_string(path.join("cgroup")).is_ok_and(|group| {
                    group
                        .lines()
                        .any(|line| line.ends_with("/system.slice/emulationstation.service"))
                })
            {
                continue;
            }
            return Ok(Some(pid));
        }
        Ok(None)
    }

    pub fn activate() -> Result<bool> {
        let Some(pid) = service_frontend()? else {
            return Ok(false);
        };
        let unit = format!("howff-home-listen-{pid}");
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
        Ok(true)
    }

    fn has_listener(pid: u32) -> Result<bool> {
        Ok(fs::read_dir("/proc")?.flatten().any(|entry| {
            fs::read(entry.path().join("cmdline"))
                .is_ok_and(|bytes| listener_for_frontend(&bytes, pid))
        }))
    }

    pub(crate) fn release_uninstalled_frontend() -> Result<bool> {
        let Some(pid) = service_frontend()? else {
            return Ok(false);
        };
        let maps = fs::read(format!("/proc/{pid}/maps"))?;
        let filter = arkos::application::INPUT_FILTER.as_str().as_bytes();
        if !maps.windows(filter.len()).any(|part| part == filter) && !has_listener(pid)? {
            return Ok(false);
        }
        validate_frontend(pid)?;
        cmd(&["systemctl", "restart", ES])?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while has_listener(pid)? {
            require(
                Instant::now() < deadline,
                "Home shortcut listener is still stopping",
            )?;
            thread::sleep(Duration::from_millis(20));
        }
        Ok(true)
    }

    pub(crate) fn foreground(pid: u32) -> bool {
        let root = Path::new("/proc").join(pid.to_string());
        fs::read_link(root.join("exe"))
            .is_ok_and(|path| path == Path::new(arkos::firmware::ES_EXECUTABLE))
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

    pub(crate) fn settled_foreground(pid: u32) -> bool {
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
            .open(howff_platform::arkos::input::JOYSTICK.as_str())?;
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
                            "--unit=howff-home",
                            &format!(
                                "--property=ExecStopPost={} home-resume",
                                arkos::application::EXECUTABLE.as_str()
                            ),
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
                    .filter(|exe| exe == Path::new(arkos::firmware::ES_EXECUTABLE))
                    .and_then(|_| entry.file_name().to_string_lossy().parse::<u32>().ok())
            })
            .ok_or("Original frontend is unavailable")?;
        cmd(&[
            "systemd-run",
            "--collect",
            "--unit=howff-launcher",
            &format!(
                "--property=ExecStopPost={} home-resume",
                arkos::application::EXECUTABLE.as_str()
            ),
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=10",
            session::BINARY,
            "launcher-open",
            &pid.to_string(),
        ])?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use device::{activate, launch, launch_standalone, listen};
#[cfg(target_os = "linux")]
pub(crate) use device::{
    definition, foreground, frontend_active, release_uninstalled_frontend, start_frontend,
    stop_frontend, validate_frontend,
};

#[cfg(not(target_os = "linux"))]
pub(crate) fn release_uninstalled_frontend() -> Result<bool> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_listener_ownership_binds_the_command_and_frontend() {
        assert!(listener_for_frontend(
            b"/opt/howff/howff\0home-listen\x00123\0",
            123
        ));
        for bytes in [
            b"/opt/howff/howff\0home-listen\x00124\0".as_slice(),
            b"/opt/howff/howff\0launcher-open\x00123\0",
            b"/opt/else/howff\0home-listen\x00123\0",
            b"/opt/howff/howff\0home-listen\0",
        ] {
            assert!(!listener_for_frontend(bytes, 123));
        }
    }

    #[test]
    fn gui_modes_share_the_same_environment_and_distinct_input_selection() {
        for launcher in [false, true] {
            let command = gui_command(launcher, File::open("/dev/null").unwrap());
            assert_eq!(command.get_program(), arkos::application::GUI.path());
            assert_eq!(command.get_args().len(), 0);
            let environment: std::collections::BTreeMap<_, _> = command
                .get_envs()
                .map(|(key, value)| (key.to_str().unwrap(), value.unwrap().to_str().unwrap()))
                .collect();
            assert_eq!(
                environment,
                std::collections::BTreeMap::from([
                    ("HOWFF_LAUNCHER_INPUT", if launcher { "1" } else { "0" }),
                    ("SDL_INPUT_LINUX_KEEP_KBD", "1"),
                    ("SDL_RENDER_BATCHING", "1"),
                    ("TERM", "linux"),
                    ("LANG", "C.UTF-8"),
                    ("LC_ALL", "C.UTF-8"),
                ])
            );
        }
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
