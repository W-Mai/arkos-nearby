//! ArkOS single-player foreground resources and power operations.

use crate::Result;
use crate::platform::system::PowerAction;
use howff_platform::arkos;
use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};

impl PowerAction {
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Poweroff => "poweroff",
            Self::Reboot => "reboot",
        }
    }
}

pub(crate) fn power(action: PowerAction) -> Result<()> {
    crate::system::command("systemctl", &[action.command()], 10, true)?;
    Ok(())
}

pub(crate) struct Solo {
    unit: String,
    uid: u32,
}

impl Drop for Solo {
    fn drop(&mut self) {
        let _ = crate::session::cmd(&["systemctl", "stop", &self.unit]);
    }
}

impl Solo {
    pub(crate) fn acquire() -> Result<Self> {
        let account = crate::session::cmd(&["getent", "passwd", "ark"])?;
        let uid = account
            .split(':')
            .nth(2)
            .ok_or("Missing game account")?
            .parse::<u32>()?;
        let unit = format!("howff-solo-{}", crate::system::random_id()?);
        Ok(Self { unit, uid })
    }

    pub(crate) fn command(
        &self,
        launch: &str,
        ticket: Option<&str>,
        log_path: &Path,
        log: File,
    ) -> Result<Command> {
        let mut command = Command::new("systemd-run");
        command
            .args([
                "--collect",
                "--wait",
                &format!("--unit={}", self.unit),
                "--property=User=ark",
                &format!("--property=WorkingDirectory={}", arkos::home::ROOT),
                "--property=StandardInput=tty",
                "--property=TTYPath=/dev/tty1",
                &format!("--property=StandardOutput=append:{}", log_path.display()),
                "--property=StandardError=inherit",
                "--property=KillMode=control-group",
                "--property=TimeoutStopSec=10",
                "--property=BindsTo=howff-launcher.service",
                "--property=After=howff-launcher.service",
                &format!(
                    "--property=ExecStopPost={} launcher-game-end",
                    arkos::application::EXECUTABLE.as_str()
                ),
                &format!("--setenv=HOME={}", arkos::home::ROOT),
                &format!("--setenv=XDG_RUNTIME_DIR=/run/user/{}", self.uid),
                "--setenv=TERM=linux",
                &format!("--setenv=HOWFF_SAVE_TICKET={}", ticket.unwrap_or_default()),
                "/bin/bash",
                "-c",
                launch,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log));
        Ok(command)
    }

    pub(crate) fn running(&self, game: &Path) -> bool {
        running_frontend(&self.unit, game)
    }
}

fn running_frontend(unit: &str, game: &Path) -> bool {
    std::fs::read_dir("/proc").is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let path = entry.path();
            std::fs::read_to_string(path.join("cgroup")).is_ok_and(|text| {
                text.lines()
                    .any(|line| line.ends_with(&format!("/{unit}.service")))
            }) && std::fs::read_link(path.join("exe")).is_ok_and(|exe| {
                !matches!(
                    exe.file_name().and_then(|name| name.to_str()),
                    Some("bash" | "sh" | "dash" | "sudo" | "env" | "nice" | "ffplay")
                ) && (std::fs::read(path.join("cmdline")).is_ok_and(|args| {
                    args.split(|byte| *byte == 0)
                        .any(|arg| arg == game.as_os_str().as_encoded_bytes())
                }) || game.extension().is_some_and(|ext| ext == "sh")
                    && game.parent().is_some_and(|parent| exe.starts_with(parent)))
            })
        })
    })
}
