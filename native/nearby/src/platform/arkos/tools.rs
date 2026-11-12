use crate::platform::system::Tool;
use crate::{Result, require};
use howff_platform::arkos;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const OWNER: &str = arkos::runtime::TOOL_OWNER.as_str();

impl Tool {
    fn path(self) -> &'static str {
        self.paths()
            .iter()
            .copied()
            .find(|path| Path::new(path).is_file())
            .unwrap_or(self.paths()[0])
    }

    fn paths(self) -> &'static [&'static str] {
        match self {
            Self::SystemInfo => {
                const {
                    &[
                        arkos::firmware::SYSTEM_INFO.as_str(),
                        arkos::firmware::SYSTEM_INFO_FALLBACK.as_str(),
                    ]
                }
            }
            Self::Files => const { &[arkos::firmware::FILE_MANAGER.as_str()] },
        }
    }
}

pub(crate) fn check_tool_runtime() -> std::result::Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("此工具需要 ArkOS 运行环境".into());
    }
    Ok(())
}

pub(crate) fn check_tool(tool: Tool) -> std::result::Result<(), String> {
    if !Path::new(tool.path()).is_file() {
        return Err("此系统未提供该工具".into());
    }
    if !fs::metadata(tool.path()).is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0) {
        return Err("此系统工具缺少执行权限".into());
    }
    for path in ["/bin/bash", "/usr/bin/systemd-run", "/dev/tty1"] {
        if !Path::new(path).exists() {
            return Err("系统工具的运行环境不完整".into());
        }
    }
    Ok(())
}

pub(crate) fn prepare_tool(tool: Tool) -> Result<Prepared> {
    require_external_controls_idle(Path::new("/proc"))?;
    let account = crate::session::cmd(&["getent", "passwd", "ark"])?;
    let uid = account
        .split(':')
        .nth(2)
        .ok_or("Missing system-tool account")?
        .parse::<u32>()?;
    Ok(Prepared {
        tool,
        uid,
        unit: format!("howff-tool-{}", crate::system::random_id()?),
        foreground: crate::home::foreground_origin()?,
    })
}

pub(crate) struct Prepared {
    tool: Tool,
    uid: u32,
    unit: String,
    foreground: crate::home::ForegroundOrigin,
}

impl Prepared {
    fn command(&self) -> Command {
        let mut command = Command::new("systemd-run");
        command
            .args([
                "--collect",
                "--wait",
                &format!("--unit={}", self.unit),
                "--property=User=ark",
                &format!("--property=WorkingDirectory={}", arkos::home::ROOT),
                "--property=StandardInput=tty",
                "--property=StandardOutput=tty",
                "--property=StandardError=tty",
                "--property=TTYPath=/dev/tty1",
                "--property=TTYReset=yes",
                "--property=KillMode=control-group",
                "--property=TimeoutStopSec=10",
                &format!("--property=BindsTo={}", self.foreground.unit()),
                &format!("--property=After={}", self.foreground.unit()),
                &format!("--setenv=HOME={}", arkos::home::ROOT),
                &format!("--setenv=XDG_RUNTIME_DIR=/run/user/{}", self.uid),
                "--setenv=TERM=linux",
                "--setenv=LANG=C.UTF-8",
                "--setenv=LC_ALL=C.UTF-8",
                self.tool.path(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    pub(crate) fn run(&self, stopping: &AtomicBool) -> Result<()> {
        require(
            !stopping.load(Ordering::Relaxed),
            "System tool was cancelled",
        )?;
        crate::system::write(Path::new(OWNER), &serde_json::json!({"unit":self.unit}))?;
        let mut unit = Unit {
            name: &self.unit,
            finished: false,
        };
        let mut child = crate::menu::OwnedChild(self.command().spawn()?);
        let status = wait(&mut child.0, stopping);
        // Reap the launch client before checking the unit so cancellation
        // cannot race a still-running client submitting its start request.
        drop(child);
        let cleanup = finish_unit(&self.unit, Path::new(OWNER), systemctl);
        unit.finished = cleanup.is_ok();
        cleanup?;
        require(status?.success(), "System tool exited with an error")
    }
}

struct Unit<'a> {
    name: &'a str,
    finished: bool,
}

impl Drop for Unit<'_> {
    fn drop(&mut self) {
        // The unit owns helpers started in the background by the original tool.
        if !self.finished {
            let _ = finish_unit(self.name, Path::new(OWNER), systemctl);
        }
    }
}

pub(crate) fn recover_pending() -> Result<()> {
    if !Path::new(OWNER).try_exists()? {
        return Ok(());
    }
    let owner: serde_json::Value = crate::system::read(Path::new(OWNER))?;
    let name = owner["unit"]
        .as_str()
        .ok_or("Missing system tool ownership")?;
    require(valid_unit(name), "Invalid system tool ownership")?;
    finish_unit(name, Path::new(OWNER), systemctl)
}

fn systemctl(action: &str, unit: &str) -> Result<String> {
    crate::system::command("systemctl", &[action, unit], 15, false)
}

fn finish_unit(
    name: &str,
    owner: &Path,
    mut control: impl FnMut(&str, &str) -> Result<String>,
) -> Result<()> {
    require(valid_unit(name), "Invalid system tool ownership")?;
    let _ = control("stop", name);
    let state = control("is-active", name)?;
    require(
        matches!(state.as_str(), "inactive" | "failed" | "unknown"),
        "System tool still owns the display",
    )?;
    if owner.try_exists()? {
        let record: serde_json::Value = crate::system::read(owner)?;
        require(record["unit"] == name, "System tool ownership changed")?;
        fs::remove_file(owner)?;
    }
    Ok(())
}

fn valid_unit(name: &str) -> bool {
    name.strip_prefix("howff-tool-")
        .is_some_and(|id| crate::protocol::hex(id, 32))
}

fn wait(
    child: &mut std::process::Child,
    stopping: &AtomicBool,
) -> Result<std::process::ExitStatus> {
    loop {
        require(
            !stopping.load(Ordering::Relaxed),
            "System tool was cancelled",
        )?;
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn require_external_controls_idle(processes: &Path) -> Result<()> {
    for entry in fs::read_dir(processes)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let arguments = match fs::read(entry.path().join("cmdline")) {
            Ok(value) => value,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(libc::ESRCH) =>
            {
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        // The original tools use pgrep -f, including helper names inside
        // wrapper arguments and architecture-specific executable names.
        let busy = [&b"gptokeyb"[..], &b"osk.py"[..]]
            .into_iter()
            .any(|pattern| {
                arguments
                    .windows(pattern.len())
                    .any(|value| value == pattern)
            })
            || arguments.split(|b| *b == 0).any(|argument| {
                let name = argument
                    .rsplit(|byte| *byte == b'/')
                    .next()
                    .unwrap_or_default();
                name == b"osk"
            });
        require(!busy, "请先退出正在使用手柄映射或键盘的系统工具 / Ports")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use howff_menu::MenuAction;

    #[test]
    fn tools_share_a_typed_allowlist_and_foreground_unit_ownership() {
        let mut actions = Vec::new();
        for tool in Tool::ALL {
            assert!(!actions.contains(&tool.action()));
            actions.push(tool.action());
            assert_eq!(Tool::from_action(tool.action()), Some(tool));
            let prepared = Prepared {
                tool,
                uid: 1000,
                unit: format!("howff-tool-{}", "a".repeat(32)),
                foreground: crate::home::ForegroundOrigin::BorrowedEs,
            };
            let command = prepared.command();
            let args: Vec<_> = command
                .get_args()
                .map(|value| value.to_str().unwrap())
                .collect();
            assert_eq!(command.get_program(), "systemd-run");
            for expected in [
                "--wait",
                "--collect",
                "--property=User=ark",
                "--property=BindsTo=howff-launcher.service",
                "--property=After=howff-launcher.service",
                "--property=KillMode=control-group",
                "--property=StandardInput=tty",
                "--property=StandardOutput=tty",
                "--property=StandardError=tty",
                "--property=TTYPath=/dev/tty1",
                &format!("--setenv=HOME={}", arkos::home::ROOT),
                "--setenv=XDG_RUNTIME_DIR=/run/user/1000",
            ] {
                assert!(args.contains(&expected), "Missing {expected}");
            }
            assert_eq!(args.last(), Some(&tool.path()));
            assert!(!args.contains(&"-c"));
            assert!(!args.contains(&"/bin/bash"));
        }
        assert_eq!(Tool::from_action(MenuAction::ManageNetwork), None);
        assert_eq!(Tool::from_action(MenuAction::Play), None);
    }

    #[test]
    fn cleanup_requires_a_released_owned_unit_and_retains_unresolved_journals() {
        let directory = std::env::temp_dir().join(crate::system::random_id().unwrap());
        fs::create_dir(&directory).unwrap();
        let owner = directory.join("owner.json");
        let name = format!("howff-tool-{}", "b".repeat(32));
        for state in [
            "active",
            "activating",
            "deactivating",
            "",
            "inactive",
            "failed",
            "unknown",
        ] {
            crate::system::write(&owner, &serde_json::json!({"unit":name})).unwrap();
            let mut calls = Vec::new();
            let result = finish_unit(&name, &owner, |action, unit| {
                assert_eq!(unit, name);
                calls.push(action.to_owned());
                Ok(if action == "is-active" { state } else { "" }.into())
            });
            let released = matches!(state, "inactive" | "failed" | "unknown");
            assert_eq!(result.is_ok(), released, "{state}");
            assert_eq!(owner.exists(), !released);
            assert_eq!(calls, ["stop", "is-active"]);
        }
        crate::system::write(&owner, &serde_json::json!({"unit":"another.service"})).unwrap();
        assert!(finish_unit(&name, &owner, |_, _| Ok("inactive".into())).is_err());
        assert!(owner.exists());
        assert!(
            finish_unit("emulationstation.service", &owner, |_, _| panic!(
                "Unowned unit"
            ))
            .is_err()
        );
        assert!(!valid_unit(&format!("{name}.service")));
        assert!(!valid_unit("howff-tool-../other"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn independent_ports_controls_are_never_taken_over_by_system_tools() {
        let root = std::env::temp_dir().join(crate::system::random_id().unwrap());
        fs::create_dir_all(root.join("123")).unwrap();
        for args in [
            &b"/opt/inttools/gptokeyb\0-1\0game\0"[..],
            &b"/usr/bin/python3\0-u\0/usr/bin/osk.py\0"[..],
            &b"/opt/ports/gptokeyb2\0"[..],
            &b"/opt/ports/gptokeyb.aarch64\0"[..],
            &b"/bin/sh\0-c\0/opt/inttools/gptokeyb -1 game\0"[..],
        ] {
            fs::write(root.join("123/cmdline"), args).unwrap();
            assert!(require_external_controls_idle(&root).is_err());
        }
        fs::write(root.join("123/cmdline"), b"nearby\0--launcher\0").unwrap();
        assert!(require_external_controls_idle(&root).is_ok());
        fs::remove_file(root.join("123/cmdline")).unwrap();
        assert!(require_external_controls_idle(&root).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tool_waiting_preserves_errors_and_responds_to_cancellation() {
        let stopping = AtomicBool::new(false);
        for code in [0, 7] {
            let mut child = crate::menu::OwnedChild(
                Command::new("sh")
                    .args(["-c", &format!("exit {code}")])
                    .spawn()
                    .unwrap(),
            );
            assert_eq!(wait(&mut child.0, &stopping).unwrap().code(), Some(code));
        }
        let mut child = crate::menu::OwnedChild(Command::new("sleep").arg("5").spawn().unwrap());
        let pid = child.0.id();
        stopping.store(true, Ordering::Relaxed);
        assert!(wait(&mut child.0, &stopping).is_err());
        drop(child);
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    }
}
