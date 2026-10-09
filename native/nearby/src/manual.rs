use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::context::Context;
use crate::menu::{OwnedChild, run_with_fallback};
use crate::session::{self, NativeBackend};
use crate::system::{mkdir, private_write, read};
use crate::{Result, require};

struct Picker {
    tty: fs::File,
    mapper: OwnedChild,
}
impl Picker {
    fn new() -> Result<Self> {
        let root = Path::new(session::BASE).join("manual");
        mkdir(&root)?;
        let keys = root.join("keys.gptk");
        private_write(&keys,b"a = enter\nb = esc\nback = esc\nstart = enter\nup = up\ndown = down\nleft = left\nright = right\nleft_analog_up = up\nleft_analog_down = down\n")?;
        let mut tty = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty1")?;
        tty.write_all(b"\x1bc")?;
        let mapper = OwnedChild(
            Command::new("/opt/inttools/gptokeyb")
                .args(["-c", keys.to_str().unwrap()])
                .env(
                    "SDL_GAMECONTROLLERCONFIG_FILE",
                    "/opt/inttools/gamecontrollerdb.txt",
                )
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        Ok(Self { tty, mapper })
    }
    fn choose(&mut self, title: &str, rows: &[(String, String)]) -> Result<Option<String>> {
        require(
            self.mapper.0.try_wait()?.is_none(),
            "Owned manual mapper stopped",
        )?;
        let mut args = vec![
            "--no-shadow".into(),
            "--clear".into(),
            "--title".into(),
            "Nearby Multiplayer".into(),
            "--cancel-label".into(),
            "Back".into(),
            "--menu".into(),
            title.into(),
            "20".into(),
            "65".into(),
            "12".into(),
        ];
        for (key, label) in rows {
            args.push(key.clone());
            args.push(label.chars().filter(|c| !c.is_control()).take(64).collect());
        }
        let result = Command::new("dialog")
            .args(args)
            .env("TERM", "linux")
            .env("LANG", "C.UTF-8")
            .env("LC_ALL", "C.UTF-8")
            .stdin(Stdio::from(self.tty.try_clone()?))
            .stdout(Stdio::from(self.tty.try_clone()?))
            .stderr(Stdio::piped())
            .output()?;
        if matches!(result.status.code(), Some(1 | 255)) {
            return Ok(None);
        }
        require(result.status.success(), "Manual dialog failed")?;
        Ok(Some(String::from_utf8(result.stderr)?.trim().into()))
    }
}
impl Drop for Picker {
    fn drop(&mut self) {
        let _ = self.tty.write_all(b"\x1bc");
    }
}

pub fn install_result(result: &serde_json::Value) -> Result<()> {
    let mut picker = Picker::new()?;
    let title = format!(
        "Installed {}\nOpen Nearby Multiplayer in Options.\nChoose a room; the owner confirms first pairing.",
        result["version"].as_str().unwrap_or("")
    );
    let _ = picker.choose(&title, &[("ok".into(), "Back".into())])?;
    Ok(())
}
pub fn run_manual() -> Result<i32> {
    session::identity(false)?;
    if let Ok(state) = session::state() {
        if !state.closed() || !state.network_restored || !state.recovery_errors.is_empty() {
            let context: Context = read(&state.root().join("context.json"))?;
            run_with_fallback(
                Some(context.clone()),
                Some(NativeBackend::shared(context)),
                false,
            )?;
            if session::state().is_ok_and(|state| !state.closed() || !state.network_restored) {
                return Ok(0);
            }
        }
    }
    let mut browser = crate::catalog::Browser::new()?;
    while let Some(context) = crate::menu::choose_game(&mut browser)? {
        run_with_fallback(
            Some(context.clone()),
            Some(NativeBackend::shared(context)),
            false,
        )?;
        if session::state().is_ok_and(|state| !state.closed() || !state.network_restored) {
            return Ok(0);
        }
    }
    Ok(0)
}
