use howff_platform::arkos;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::context::Context;
use crate::game::{Settings, settings};
use crate::session::{State, best, current, progress};
use crate::system::{command, mkdir, monotonic, private_write, write};
use crate::{Result, require};

fn system_directory(base: &str) -> Result<PathBuf> {
    let value = base
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| key.trim() == "system_directory")
        .map(|(_, value)| value.trim().trim_matches('"'))
        .next_back()
        .ok_or("Missing installed BIOS directory")?;
    require(
        Path::new(value).is_absolute(),
        "BIOS directory must be absolute",
    )?;
    let path = Path::new(value).canonicalize()?;
    require(path.is_dir(), "Installed BIOS directory is unavailable")?;
    Ok(path)
}

fn probe_report(output: &str) -> Result<Value> {
    let report = output
        .lines()
        .rev()
        .find_map(|line| {
            let value = serde_json::from_str::<Value>(line).ok()?;
            (value["schema"] == 1 && value["loaded"] == true).then_some(value)
        })
        .ok_or("Core did not confirm a loaded game")?;
    require(
        report["state_bytes"]
            .as_u64()
            .is_some_and(|size| (1..=64 * 1024 * 1024).contains(&size)),
        "Core did not provide a usable rollback state",
    )?;
    Ok(report)
}

pub fn stop(state: &State) -> Result<()> {
    stop_probe(&state.unit(), &state.root())
}

fn stop_probe(unit: &str, root: &Path) -> Result<()> {
    best(&["systemctl", "stop", &format!("{unit}-probe.service")]);
    let active = command(
        "systemctl",
        &[
            "show",
            "-p",
            "ActiveState",
            "--value",
            &format!("{unit}-probe.service"),
        ],
        8,
        false,
    )?;
    require(
        matches!(active.trim(), "" | "inactive" | "failed"),
        "Arcade load check is still stopping",
    )?;
    let directory = root.join("core-probe");
    if directory.exists() {
        fs::remove_dir_all(directory)?;
    }
    Ok(())
}

pub fn stop_launch(root: &Path, id: &str) -> Result<()> {
    stop_probe(&format!("howff-launch-{id}"), root)
}

struct Scope<'a> {
    state: Option<&'a State>,
    root: PathBuf,
    unit: String,
    id: String,
}

impl<'a> Scope<'a> {
    fn room(state: &'a State) -> Self {
        Self {
            state: Some(state),
            root: state.root(),
            unit: state.unit(),
            id: state.id.clone(),
        }
    }
    fn check(&self) -> Result<()> {
        if let Some(state) = self.state {
            current(&state.id)?;
        }
        Ok(())
    }

    fn settings(&self, context: &Context) -> Result<Settings> {
        if self.state.is_some() {
            settings(context)
        } else {
            crate::game::solo_settings(context)
        }
    }
}

pub fn requires_probe(context: &Context) -> bool {
    context.frontend == "retroarch"
        && crate::core_choice::arcade_family(&context.core_id)
        && context.core_elf_bits == 64
}

fn probe(scope: &Scope<'_>, context: &Context, settings: &Settings) -> Result<()> {
    require(
        requires_probe(context),
        "Arcade load check requires an installed ARM64 arcade core",
    )?;
    scope.check()?;
    let directory = scope.root.join("core-probe");
    mkdir(&directory)?;
    let saves = directory.join("saves");
    mkdir(&saves)?;
    let options = directory.join("core-options.cfg");
    private_write(&options, &settings.options)?;
    let system = system_directory(&settings.base)?;
    let mut arguments = vec![
        "--collect".into(),
        "--wait".into(),
        "--pipe".into(),
        "--quiet".into(),
        format!("--unit={}-probe", scope.unit),
        "--property=RuntimeMaxSec=45".into(),
        "--property=TimeoutStopSec=2".into(),
        "--property=MemoryMax=512M".into(),
        "--property=ProtectSystem=strict".into(),
        "--property=ProtectHome=read-only".into(),
        "--property=PrivateDevices=yes".into(),
        "--property=PrivateNetwork=yes".into(),
        "--property=ProtectKernelTunables=yes".into(),
        "--property=ProtectControlGroups=yes".into(),
        "--property=NoNewPrivileges=yes".into(),
        format!("--property=ReadWritePaths={}", directory.display()),
        format!("--property=WorkingDirectory={}", directory.display()),
        format!("--setenv=HOME={}", directory.display()),
    ];
    if scope.state.is_some() {
        arguments.push(format!("--property=BindsTo={}.service", scope.unit));
        arguments.push(format!("--property=After={}.service", scope.unit));
    }
    if let Some(runtime) = crate::core_runtime::directory(&context.core_id, context.core_elf_bits)?
    {
        arguments.push(format!("--setenv=LD_LIBRARY_PATH={runtime}"));
    }
    arguments.extend([
        arkos::application::CORE_PROBE.as_str().into(),
        context.core_path.to_string_lossy().into_owned(),
        context.game_path.to_string_lossy().into_owned(),
        system.to_string_lossy().into_owned(),
        saves.to_string_lossy().into_owned(),
        options.to_string_lossy().into_owned(),
    ]);
    let started = monotonic();
    let outcome = command(
        "systemd-run",
        &arguments.iter().map(String::as_str).collect::<Vec<_>>(),
        50,
        true,
    )
    .and_then(|text| probe_report(&text));
    stop_probe(&scope.unit, &scope.root)?;
    scope.check()?;
    let record = json!({"session_id":scope.id,"role":scope.state.map_or("single_player",|state| state.role.as_str()),"core_id":context.core_id,
        "core_sha256":context.core_sha256,"content_sha256":context.identity.content.sha256,
        "elapsed_ms":(monotonic()-started)*1000.0,"result":outcome.as_ref().ok(),
        "error":outcome.as_ref().err().map(|error| error.to_string().chars().take(4096).collect::<String>())});
    write(&scope.root.join("core-probe.json"), &record)?;
    mkdir(arkos::application::LOGS.path())?;
    write(
        &arkos::application::LOGS.path().join("last-core-probe.json"),
        &record,
    )?;
    outcome.map(|_| ())
}

pub fn host(state: &State, original: &Context) -> Result<(Context, Settings)> {
    select(&Scope::room(state), original)
}

pub fn single_player(root: &Path, id: &str, original: &Context) -> Result<(Context, Settings)> {
    select(
        &Scope {
            state: None,
            root: root.into(),
            unit: format!("howff-launch-{id}"),
            id: id.into(),
        },
        original,
    )
}

fn select(scope: &Scope<'_>, original: &Context) -> Result<(Context, Settings)> {
    let choices = crate::core_choice::arcade_order(&original.game_path);
    if choices.is_empty() || original.frontend != "retroarch" {
        let snapshot = scope.settings(original)?;
        if requires_probe(original) && scope.state.is_some() {
            probe(scope, original, &snapshot)?;
        }
        return Ok((original.clone(), snapshot));
    }
    let mut attempts = Vec::new();
    for choice in choices {
        scope.check()?;
        let name = match choice.id {
            "fbneo" => "FBNeo",
            "fbneo_plus" => "FBNeo Plus",
            _ => "MAME",
        };
        if let Some(state) = scope.state {
            progress(
                &state.id,
                None,
                "creating",
                &format!("正在检查 {name} 游戏兼容性..."),
            )?;
        }
        let selected = (|| -> Result<(Context, Settings)> {
            let context = if original.core_id == choice.id {
                original.clone()
            } else {
                original.with_core(*choice)?
            };
            let snapshot = scope.settings(&context)?;
            probe(scope, &context, &snapshot)?;
            Ok((context, snapshot))
        })();
        scope.check()?;
        attempts.push(json!({"core_id":choice.id,"loaded":selected.is_ok(),
            "error":selected.as_ref().err().map(|error|error.to_string().chars().take(4096).collect::<String>())}));
        let record = json!({"session_id":scope.id,"attempts":attempts,"origin":selected.as_ref().ok().map(|(context,_)|context.origin())});
        write(&scope.root.join("core-selection.json"), &record)?;
        mkdir(arkos::application::LOGS.path())?;
        write(
            &arkos::application::LOGS
                .path()
                .join("last-core-selection.json"),
            &record,
        )?;
        match selected {
            Ok(value) => return Ok(value),
            Err(error) => {
                eprintln!("Arcade core {} unavailable: {error}", choice.id);
            }
        }
    }
    Err("本机街机核心均无法加载这份 ROM，请检查游戏文件与 BIOS".into())
}

pub fn client(state: &State, context: &Context) -> Result<Settings> {
    let snapshot = settings(context)?;
    if requires_probe(context) {
        probe(&Scope::room(state), context, &snapshot)?;
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_error_screens_and_unbounded_states_cannot_pass_preflight() {
        for text in [
            r#"{"schema":1,"loaded":true,"state_bytes":0}"#,
            r#"{"schema":1,"loaded":false,"state_bytes":1024}"#,
            r#"{"schema":1,"loaded":true,"state_bytes":67108865}"#,
            r#"{"schema":1,"loaded":true,"state_bytes":-1}"#,
            "core log without a result",
        ] {
            assert!(probe_report(text).is_err());
        }
        assert!(
            probe_report("core log\n{\"schema\":1,\"loaded\":true,\"state_bytes\":1024}\n").is_ok()
        );
    }
}
