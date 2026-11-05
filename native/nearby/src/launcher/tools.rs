use crate::{Result, require};
use nearby_menu::{MenuAction, MenuItem};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const OWNER: &str = "/run/arkos-nearby-rust/tool-owner.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    SystemInfo,
    Files,
}

impl Tool {
    const ALL: [Self; 2] = [Self::Files, Self::SystemInfo];

    pub(super) fn action(self) -> MenuAction {
        match self {
            Self::SystemInfo => MenuAction::SystemInfo,
            Self::Files => MenuAction::FileManager,
        }
    }

    pub(super) fn from_action(action: MenuAction) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.action() == action)
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::SystemInfo => "系统信息",
            Self::Files => "文件管理",
        }
    }

    fn path(self) -> &'static str {
        self.paths()
            .iter()
            .copied()
            .find(|path| Path::new(path).is_file())
            .unwrap_or(self.paths()[0])
    }

    fn paths(self) -> &'static [&'static str] {
        match self {
            Self::SystemInfo => &[
                "/opt/system/System Info.sh",
                "/opt/system/Tools/System Info.sh",
            ],
            Self::Files => &["/opt/system/351Files.sh"],
        }
    }

    fn available(self) -> std::result::Result<(), String> {
        if !cfg!(target_os = "linux") {
            return Err("此工具需要 ArkOS 运行环境".into());
        }
        crate::session::require_idle().map_err(|_| "请先结束房间并完成网络恢复".to_string())?;
        if !Path::new(self.path()).is_file() {
            return Err("此系统未提供该工具".into());
        }
        if !fs::metadata(self.path()).is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0) {
            return Err("此系统工具缺少执行权限".into());
        }
        for path in ["/bin/bash", "/usr/bin/systemd-run", "/dev/tty1"] {
            if !Path::new(path).exists() {
                return Err("系统工具的运行环境不完整".into());
            }
        }
        Ok(())
    }

    pub(super) fn prepare(self) -> Result<Prepared> {
        self.available()?;
        recover_pending()?;
        require_external_controls_idle(Path::new("/proc"))?;
        let account = crate::session::cmd(&["getent", "passwd", "ark"])?;
        let uid = account
            .split(':')
            .nth(2)
            .ok_or("Missing system-tool account")?
            .parse::<u32>()?;
        Ok(Prepared {
            tool: self,
            uid,
            unit: format!("arkos-nearby-tool-{}", crate::system::random_id()?),
        })
    }
}

#[derive(Default)]
pub(super) struct Tools {
    items: Vec<MenuItem>,
}

impl Tools {
    pub(super) fn refresh(&mut self) {
        self.items = [
            MenuItem::available(MenuAction::OpenWifi, "Wi-Fi 设置"),
            MenuItem::available(MenuAction::NetworkInfo, "网络信息"),
            MenuItem::available(MenuAction::ShowStorage, "存储空间"),
        ]
        .into_iter()
        .chain(Tool::ALL.into_iter().map(|tool| match tool.available() {
            Ok(()) => MenuItem::available(tool.action(), tool.name()),
            Err(reason) => MenuItem::unavailable(tool.action(), tool.name(), reason),
        }))
        .chain([MenuItem::available(MenuAction::Back, "返回")])
        .collect();
    }

    pub(super) fn items(&self) -> Vec<MenuItem> {
        self.items.clone()
    }
}

pub(super) struct Prepared {
    tool: Tool,
    uid: u32,
    unit: String,
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
                "--property=WorkingDirectory=/home/ark",
                "--property=StandardInput=tty",
                "--property=StandardOutput=tty",
                "--property=StandardError=tty",
                "--property=TTYPath=/dev/tty1",
                "--property=TTYReset=yes",
                "--property=KillMode=control-group",
                "--property=TimeoutStopSec=10",
                "--property=BindsTo=arkos-nearby-launcher.service",
                "--property=After=arkos-nearby-launcher.service",
                "--setenv=HOME=/home/ark",
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

    pub(super) fn run(&self, stopping: &AtomicBool) -> Result<()> {
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
    name.strip_prefix("arkos-nearby-tool-")
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
                unit: format!("arkos-nearby-tool-{}", "a".repeat(32)),
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
                "--property=BindsTo=arkos-nearby-launcher.service",
                "--property=After=arkos-nearby-launcher.service",
                "--property=KillMode=control-group",
                "--property=StandardInput=tty",
                "--property=StandardOutput=tty",
                "--property=StandardError=tty",
                "--property=TTYPath=/dev/tty1",
                "--setenv=HOME=/home/ark",
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
        let name = format!("arkos-nearby-tool-{}", "b".repeat(32));
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
        assert!(!valid_unit("arkos-nearby-tool-../other"));
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

    #[test]
    fn tools_preserve_selection_and_return_through_the_existing_menu_hierarchy() {
        use super::super::{Controller, Overlay, controls};
        for back in ["back", "confirm"] {
            let mut controller = Controller {
                browsers: [
                    crate::catalog::Browser::new().unwrap(),
                    crate::catalog::Browser::new().unwrap(),
                ],
                tab: 1,
                overlay: Some(Overlay::Tools),
                selection: 0,
                entry: None,
                controls: controls::Controls::default(),
                tools: Tools {
                    items: Tool::ALL
                        .into_iter()
                        .map(|tool| MenuItem::available(tool.action(), tool.name()))
                        .chain([MenuItem::available(MenuAction::Back, "返回")])
                        .collect(),
                },
                details: Default::default(),
                wifi: Default::default(),
                wifi_return: MenuAction::OpenWifi,
                configuration: Default::default(),
                saves: Default::default(),
                saves_from_configuration: false,
                detail_expanded: false,
                detail_scroll: 0,
                changed_tab: false,
                status: String::new(),
            };
            let browser = controller.browser().view();
            for (index, tool) in Tool::ALL.into_iter().enumerate() {
                controller.selection = index;
                assert!(
                    matches!(controller.input("confirm").unwrap(), Some(super::super::Intent::Tool(value)) if value == tool)
                );
                assert!(controller.overlay == Some(Overlay::Tools));
                assert_eq!(controller.selection, index);
                assert_eq!(controller.browser().view(), browser);
            }
            controller.tools.items[0] = MenuItem::unavailable(
                MenuAction::OpenWifi,
                "Wi-Fi 设置",
                "请先结束房间并完成网络恢复",
            );
            controller.selection = 0;
            assert!(controller.input("confirm").unwrap().is_none());
            assert!(controller.overlay == Some(Overlay::Tools));
            assert!(!controller.status.is_empty());
            if back == "confirm" {
                controller.selection = controller.options().len() - 1;
            }
            assert!(controller.input(back).unwrap().is_none());
            assert!(controller.overlay == Some(Overlay::Controls));
            assert_eq!(
                controller.options()[controller.selection].action,
                MenuAction::ShowTools
            );
            assert_eq!(controller.tab, 1);
            controller.input("back").unwrap();
            assert!(controller.overlay == Some(Overlay::System));
            assert_eq!(
                controller.options()[controller.selection].action,
                MenuAction::ShowControls
            );
            controller.input("back").unwrap();
            assert!(controller.overlay.is_none());
            assert_eq!(controller.browser().view(), browser);
        }
    }
}
