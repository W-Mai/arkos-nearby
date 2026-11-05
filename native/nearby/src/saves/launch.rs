use crate::{Result, require};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const BASE: &str = "/run/arkos-nearby-save-launch";
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Intent {
    Configured,
    State { slot: i32, revision: String },
    Sram,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Ticket {
    identity: super::Identity,
    game: std::path::PathBuf,
    paths: crate::configuration::ResolvedSavePaths,
    intent: Intent,
    expires: f64,
}

pub fn issue(
    context: &crate::context::Context,
    paths: &crate::configuration::ResolvedSavePaths,
    intent: Intent,
) -> Result<String> {
    crate::system::mkdir(Path::new(BASE))?;
    let id = crate::system::random_id()?;
    crate::system::write(
        &Path::new(BASE).join(&id),
        &Ticket {
            identity: super::Identity::from_context(context),
            game: context.game_path.clone(),
            paths: paths.clone(),
            intent,
            expires: crate::system::monotonic() + 120.0,
        },
    )?;
    Ok(id)
}
pub fn discard(id: &str) {
    if crate::protocol::hex(id, 32) {
        let _ = fs::remove_file(Path::new(BASE).join(id));
    }
}

fn strip(args: &mut Vec<String>, short: &str, long: &str) {
    let mut retained = Vec::new();
    let mut values = std::mem::take(args).into_iter();
    while let Some(value) = values.next() {
        if value == short || value == long {
            values.next();
        } else if !(value.starts_with(&format!("{long}="))
            || short.len() == 2 && value.starts_with(short) && value.len() > 2)
        {
            retained.push(value);
        }
    }
    *args = retained;
}

pub fn take(args: &mut Vec<String>) -> Result<Option<String>> {
    let mut id = None;
    let mut clean = Vec::new();
    for arg in std::mem::take(args) {
        if let Some(value) = arg.strip_prefix("--nearby-save-ticket=") {
            require(
                id.is_none() && crate::protocol::hex(value, 32),
                "存档启动凭据无效",
            )?;
            id = Some(value.to_owned());
        } else {
            clean.push(arg);
        }
    }
    *args = clean;
    Ok(id)
}

pub fn apply(
    id: &str,
    context: &mut crate::context::Context,
    settings: &crate::game::Settings,
    directory: &Path,
) -> Result<()> {
    require(crate::protocol::hex(id, 32), "存档启动凭据无效")?;
    let path = Path::new(BASE).join(id);
    let ticket: Ticket = super::files::read_manifest(&path)?;
    require(
        ticket.expires > crate::system::monotonic()
            && ticket.game == context.game_path
            && ticket.identity == super::Identity::from_context(context),
        "存档启动来源已变化",
    )?;
    let snapshot = crate::configuration::Snapshot::load(
        crate::configuration::Snapshot::environment(context)?,
        Default::default(),
        vec![],
    )?;
    require(snapshot.saves == ticket.paths, "存档位置已变化，请重新读取")?;
    if let Intent::State { slot, revision } = &ticket.intent {
        require(
            (-1..=999).contains(slot)
                && super::files::stamp(&ticket.paths.slot_path(*slot))?.revision == *revision,
            "存档已变化，请重新读取",
        )?;
    }
    if let Intent::State { slot, .. } = ticket.intent
        && slot >= 0
    {
        let entry = entry_path(&ticket.paths, slot);
        require(
            !entry.exists()
                || crate::content::digest(&entry)?
                    == crate::content::digest(&ticket.paths.slot_path(slot))?,
            "同槽位的启动存档与所选文件不同，请查看存档位置",
        )?;
    }
    fs::remove_file(path)?;
    if ticket.intent == Intent::Configured {
        return Ok(());
    }
    for (short, long) in [
        ("-e", "--entryslot"),
        ("-s", "--save"),
        ("-S", "--savestate"),
    ] {
        strip(&mut context.argv, short, long);
    }
    let mut controls = String::from(
        "config_save_on_exit = \"false\"\nauto_overrides_enable = \"false\"\nsort_savefiles_enable = \"false\"\nsort_savestates_enable = \"false\"\nsort_savefiles_by_content_enable = \"false\"\nsort_savestates_by_content_enable = \"false\"\nsavefiles_in_content_dir = \"false\"\nsavestates_in_content_dir = \"false\"\n",
    );
    let game_name = context
        .game_path
        .file_stem()
        .ok_or("游戏名称缺失")?
        .to_string_lossy();
    for (arg, path, extension, key) in [
        ("-s", &ticket.paths.sram, "srm", "savefile_directory"),
        ("-S", &ticket.paths.state, "state", "savestate_directory"),
    ] {
        let parent = path.parent().ok_or("存档目录缺失")?;
        require(
            !parent.to_string_lossy().contains(['"', '\n', '\r']),
            "存档目录包含无效字符",
        )?;
        context.argv.extend([
            arg.to_owned(),
            if path.file_name().is_some_and(|name| {
                name == std::ffi::OsStr::new(&format!("{game_name}.{extension}"))
            }) {
                parent.to_string_lossy().into_owned()
            } else {
                path.to_string_lossy().into_owned()
            },
        ]);
        controls.push_str(&format!("{key} = \"{}\"\n", parent.display()));
    }
    if let Intent::State { slot, .. } = ticket.intent {
        if slot >= 0 {
            context
                .argv
                .extend(["--entryslot".into(), slot.to_string()]);
        }
        controls.push_str(&format!(
            "savestate_auto_load = \"{}\"\nstate_slot = \"{slot}\"\n",
            slot < 0
        ));
    } else {
        controls.push_str("savestate_auto_load = \"false\"\n");
    }
    let configuration = directory.join("save-launch.cfg");
    crate::system::private_write(
        &configuration,
        crate::game::isolated(&settings.base, &controls).as_bytes(),
    )?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&configuration, fs::Permissions::from_mode(0o644))?;
    crate::launch::append_configuration(&mut context.argv, &configuration)?;
    let actual = crate::configuration::Snapshot::load(
        crate::configuration::Snapshot::environment(context)?,
        Default::default(),
        vec![],
    )?;
    require(
        actual.saves.sram == ticket.paths.sram && actual.saves.state == ticket.paths.state,
        "单次启动未能保留存档路径",
    )?;
    context.argv.push("--verbose".into());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_launch_arguments_are_consumed_and_old_slot_forms_are_removed() {
        let mut args = vec![
            "--entryslot=4".into(),
            "-e5".into(),
            "-e".into(),
            "6".into(),
            "/roms/a'$(x).zip".into(),
            format!("--nearby-save-ticket={}", "a".repeat(32)),
        ];
        assert_eq!(take(&mut args).unwrap(), Some("a".repeat(32)));
        strip(&mut args, "-e", "--entryslot");
        assert_eq!(args, ["/roms/a'$(x).zip"]);
        args.push("--nearby-save-ticket=bad".into());
        assert!(take(&mut args).is_err());
    }
}

pub fn expected_state(id: &str) -> Result<Option<std::path::PathBuf>> {
    require(crate::protocol::hex(id, 32), "存档启动凭据无效")?;
    let ticket: Ticket = super::files::read_manifest(&Path::new(BASE).join(id))?;
    Ok(if let Intent::State { slot, .. } = ticket.intent {
        Some(if slot >= 0 && entry_path(&ticket.paths, slot).exists() {
            entry_path(&ticket.paths, slot)
        } else {
            ticket.paths.slot_path(slot)
        })
    } else {
        None
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum StateResult {
    Pending,
    Queued,
    Failed,
}
pub fn state_result(log: &[u8], path: &Path) -> StateResult {
    let log = String::from_utf8_lossy(log);
    let path = format!("\"{}\"", path.display());
    let mut result = StateResult::Pending;
    for line in log
        .lines()
        .filter(|line| line.contains("[State]") && line.contains(&path))
    {
        if line.contains("[ERROR]") || line.ends_with(" failed.") {
            return StateResult::Failed;
        }
        if line.ends_with(" succeeded.") {
            result = StateResult::Queued;
        }
    }
    result
}

fn entry_path(paths: &crate::configuration::ResolvedSavePaths, slot: i32) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}{slot}.entry", paths.state.display()))
}
#[cfg(test)]
mod result_tests {
    use super::*;
    #[test]
    fn state_acknowledgement_and_async_failure_are_scoped_to_the_selected_file() {
        let path = Path::new("/roms/snes/游戏.state1");
        assert_eq!(
            state_result(
                "[INFO] [State] Loading \"/roms/snes/other.state1\" succeeded.".as_bytes(),
                path
            ),
            StateResult::Pending
        );
        assert_eq!(
            state_result(
                "[INFO] [State] Loading \"/roms/snes/游戏.state1\" succeeded.".as_bytes(),
                path
            ),
            StateResult::Queued
        );
        assert_eq!(
            state_result(
                "[INFO] [State] Loading \"/roms/snes/游戏.state1\" succeeded.\n[ERROR] [State] 无法载入状态 \"/roms/snes/游戏.state1\".".as_bytes(),
                path
            ),
            StateResult::Failed
        );
    }

    #[test]
    fn foreign_log_encoding_cannot_hide_the_requested_state_result() {
        let path = Path::new("/roms/snes/游戏.state1");
        let mut bytes = b"[Content] archive.zip#\xff\x81.sfc\n".to_vec();
        bytes.extend_from_slice(
            "[INFO] [State] Loading \"/roms/snes/游戏.state1\" succeeded.\n".as_bytes(),
        );
        assert_eq!(state_result(&bytes, path), StateResult::Queued);
        bytes.extend_from_slice(b"[Core] \xff\xfe\n");
        bytes.extend_from_slice(
            "[ERROR] [State] Could not load \"/roms/snes/游戏.state1\".\n".as_bytes(),
        );
        assert_eq!(state_result(&bytes, path), StateResult::Failed);
        assert_eq!(
            state_result(&bytes, Path::new("/roms/snes/other.state1")),
            StateResult::Pending
        );
    }
}
