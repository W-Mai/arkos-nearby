use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::context::{Context, INSTALL};
use crate::game::{Settings, settings};
use crate::session::{State, best, current};
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
    best(&[
        "systemctl",
        "stop",
        &format!("{}-probe.service", state.unit()),
    ]);
    let active = command(
        "systemctl",
        &[
            "show",
            "-p",
            "ActiveState",
            "--value",
            &format!("{}-probe.service", state.unit()),
        ],
        8,
        false,
    )?;
    require(
        matches!(active.trim(), "" | "inactive" | "failed"),
        "Arcade load check is still stopping",
    )?;
    let directory = state.root().join("core-probe");
    if directory.exists() {
        fs::remove_dir_all(directory)?;
    }
    Ok(())
}

fn probe(state: &State, context: &Context, settings: &Settings) -> Result<()> {
    require(
        context.frontend == "retroarch"
            && context.core_id == "fbneo"
            && context.core_elf_bits == 64,
        "Arcade load check requires the installed ARM64 FBNeo core",
    )?;
    current(&state.id)?;
    let directory = state.root().join("core-probe");
    mkdir(&directory)?;
    let saves = directory.join("saves");
    mkdir(&saves)?;
    let options = directory.join("core-options.cfg");
    private_write(&options, &settings.options)?;
    let system = system_directory(&settings.base)?;
    let arguments = vec![
        "--collect".into(),
        "--wait".into(),
        "--pipe".into(),
        "--quiet".into(),
        format!("--unit={}-probe", state.unit()),
        format!("--property=BindsTo={}.service", state.unit()),
        format!("--property=After={}.service", state.unit()),
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
        format!("{INSTALL}/core-probe64"),
        context.core_path.to_string_lossy().into_owned(),
        context.game_path.to_string_lossy().into_owned(),
        system.to_string_lossy().into_owned(),
        saves.to_string_lossy().into_owned(),
        options.to_string_lossy().into_owned(),
    ];
    let started = monotonic();
    let outcome = command(
        "systemd-run",
        &arguments.iter().map(String::as_str).collect::<Vec<_>>(),
        50,
        true,
    )
    .and_then(|text| probe_report(&text));
    stop(state)?;
    current(&state.id)?;
    let record = json!({"session_id":state.id,"role":state.role,"core_id":context.core_id,
        "core_sha256":context.core_sha256,"content_sha256":context.identity.content.sha256,
        "elapsed_ms":(monotonic()-started)*1000.0,"result":outcome.as_ref().ok(),
        "error":outcome.as_ref().err().map(|error| error.to_string().chars().take(4096).collect::<String>())});
    write(&state.root().join("core-probe.json"), &record)?;
    mkdir(&Path::new(INSTALL).join("logs"))?;
    write(
        &Path::new(INSTALL).join("logs/last-core-probe.json"),
        &record,
    )?;
    outcome.map(|_| ())
}

pub fn host(state: &State, original: &Context) -> Result<(Context, Settings)> {
    if let Some(choice) =
        crate::core_choice::arcade_candidate(&original.game_path, &original.core_id)
    {
        let selected = (|| -> Result<(Context, Settings)> {
            let context = original.with_core(choice)?;
            let snapshot = settings(&context)?;
            probe(state, &context, &snapshot)?;
            Ok((context, snapshot))
        })();
        match selected {
            Ok(value) => return Ok(value),
            Err(error) => {
                current(&state.id)?;
                eprintln!(
                    "Arcade candidate unavailable; retaining {}: {error}",
                    original.core_id
                );
            }
        }
    }
    let snapshot = client(state, original)?;
    Ok((original.clone(), snapshot))
}

pub fn client(state: &State, context: &Context) -> Result<Settings> {
    let snapshot = settings(context)?;
    if context.core_id == "fbneo" && context.core_elf_bits == 64 {
        probe(state, context, &snapshot)?;
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
