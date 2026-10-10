use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::content::{confined, digest};
use crate::context::{Context, config};
use crate::handheld_link::LinkMode;
use crate::menu::OwnedChild;
use crate::protocol::{Room, VerifiedRoom};
use crate::session::{State, best, cmd, current, live_peer, task};
use crate::system::{Lock, mkdir, monotonic, private_write, read, write};
use crate::{Result, require};

const ES: &str = "emulationstation.service";
const ES_FILE: &str = "/etc/systemd/system/emulationstation.service";

fn account() -> Result<(u32, u32)> {
    let value = cmd(&["getent", "passwd", "ark"])?;
    let fields: Vec<_> = value.split(':').collect();
    require(
        fields.len() >= 4 && fields[0] == "ark",
        "Missing installed game account",
    )?;
    Ok((fields[2].parse()?, fields[3].parse()?))
}
fn chown(path: &Path, uid: u32, gid: u32) -> Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(path.as_os_str().as_bytes())?;
    require(
        unsafe { libc::chown(path.as_ptr(), uid, gid) } == 0,
        "Session ownership failed",
    )
}

fn controller_users(link_mode: Option<LinkMode>) -> usize {
    match link_mode {
        None | Some(LinkMode::GbDual) => 2,
        Some(_) => 1,
    }
}

pub fn configuration(path: &Path, link_mode: Option<LinkMode>) -> Result<String> {
    let path = path.to_str().ok_or("Invalid runtime configuration path")?;
    require(
        !path.contains(['"', '\n', '\r']),
        "Invalid configuration text",
    )?;
    let mut lines = Vec::new();
    for (key, folder) in [
        ("savefile_directory", "saves"),
        ("savestate_directory", "states"),
        ("cache_directory", "cache"),
    ] {
        lines.push(format!("{key} = \"{path}/{folder}\""));
    }
    lines.push(format!("core_options_path = \"{path}/core-options.cfg\""));
    for key in [
        "savestate_auto_save",
        "savestate_auto_load",
        "savefiles_in_content_dir",
        "savestates_in_content_dir",
        "sort_savefiles_enable",
        "sort_savestates_enable",
        "sort_savefiles_by_content_enable",
        "sort_savestates_by_content_enable",
        "config_save_on_exit",
        "remap_save_on_exit",
        "auto_overrides_enable",
        "auto_remaps_enable",
        "game_specific_options",
        "cheevos_enable",
        "netplay_public_announce",
        "netplay_use_mitm_server",
        "netplay_nat_traversal",
    ] {
        lines.push(format!("{key} = \"false\""));
    }
    lines.extend([
        "global_core_options = \"true\"".into(),
        "netplay_ip_port = \"55435\"".into(),
        // Match RetroArch's default periodic state check interval.
        "netplay_check_frames = \"600\"".into(),
    ]);
    let users = controller_users(link_mode);
    lines.push(format!("input_max_users = \"{users}\""));
    Ok(lines.join("\n") + "\n")
}

pub fn frontend_arguments(context: &Context, directory: &Path, host: bool) -> Vec<String> {
    let mut args = vec![
        context.frontend_path.to_string_lossy().into_owned(),
        "--verbose".into(),
        "-c".into(),
        directory.join("base.cfg").to_string_lossy().into_owned(),
        "--appendconfig".into(),
        directory.join("netplay.cfg").to_string_lossy().into_owned(),
        format!("--nick=ArkOS_{}", if host { "host" } else { "peer" }),
        "--sram-mode=noload-nosave".into(),
    ];
    // Device types in base.cfg are ignored by RetroArch. The CLI sets the
    // devices used by both ordinary initialization and the netplay handshake.
    let users = controller_users(context.link_mode);
    for port in 1..=16 {
        let device = u8::from(port <= users);
        args.push(format!("--device={port}:{device}"));
    }
    args.extend([
        "-L".into(),
        context.core_path.to_string_lossy().into_owned(),
        context.game_path.to_string_lossy().into_owned(),
        if host { "-H" } else { "--connect=192.168.49.1" }.into(),
        "--port=55435".into(),
    ]);
    args
}

pub fn isolated(original: &str, overrides: &str) -> String {
    let keys: std::collections::BTreeSet<_> = overrides
        .lines()
        .filter_map(|line| line.split_once('=').map(|(key, _)| key.trim()))
        .collect();
    original
        .lines()
        .filter(|line| !keys.contains(line.split('=').next().unwrap_or("").trim()))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
        + overrides
}

pub struct Settings {
    pub base: String,
    pub options: Vec<u8>,
}

pub fn settings(context: &Context) -> Result<Settings> {
    let configuration = config(&context.frontend)?;
    let source = confined(
        &configuration.join("retroarch.cfg"),
        std::slice::from_ref(&configuration),
    )?;
    let path = configuration.join("retroarch-core-options.cfg");
    let bytes = if path.is_file() {
        fs::read(confined(&path, std::slice::from_ref(&configuration))?)?
    } else {
        vec![]
    };
    let options = if let Some(mode) = context.link_mode {
        isolated(&String::from_utf8(bytes)?, mode.options()).into_bytes()
    } else {
        bytes
    };
    Ok(Settings {
        base: fs::read_to_string(source)?,
        options,
    })
}

pub fn prepare(
    state: &State,
    context: &Context,
    remote: Option<&VerifiedRoom>,
    settings: &Settings,
) -> Result<()> {
    context.revalidate()?;
    let room = if let Some(remote) = remote {
        require(
            remote.room().game().compatible(&context.identity),
            "Local client is incompatible",
        )?;
        remote.room().clone()
    } else {
        Room::new(
            state.id.clone(),
            state.self_mac.clone(),
            context.identity.clone(),
        )?
    };
    require(
        (state.role == "host" && remote.is_none()) || (state.role == "client" && remote.is_some()),
        "Prepared role lacks a verified room",
    )?;
    let owner = Path::new(crate::session::BASE).join("game-owner.lock");
    if !owner.exists() {
        private_write(&owner, b"")?;
    }
    let directory = state.root().join("game");
    require(!directory.exists(), "Game directory already exists")?;
    mkdir(&directory)?;
    let (uid, gid) = account()?;
    chown(&directory, uid, gid)?;
    for folder in ["saves", "states", "cache"] {
        let path = directory.join(folder);
        mkdir(&path)?;
        chown(&path, uid, gid)?;
    }
    let overrides = configuration(&directory, context.link_mode)?;
    private_write(
        &directory.join("base.cfg"),
        isolated(&settings.base, &overrides).as_bytes(),
    )?;
    private_write(&directory.join("netplay.cfg"), overrides.as_bytes())?;
    private_write(&directory.join("core-options.cfg"), &settings.options)?;
    for name in ["base.cfg", "netplay.cfg", "core-options.cfg"] {
        chown(&directory.join(name), uid, gid)?;
    }
    write(&directory.join("room.json"), &room)?;
    fs::set_permissions(
        directory.join("room.json"),
        fs::Permissions::from_mode(0o644),
    )?;
    write(
        &directory.join("game.json"),
        &json!({"phase":"prepared","session_id":state.id,"unit":format!("{}-game",state.unit())}),
    )?;
    Ok(())
}

pub fn start(state: &State) -> Result<()> {
    let context: Context = read(&state.root().join("context.json"))?;
    context.revalidate()?;
    if state.role == "host" {
        live_peer(state)?;
    } else {
        require(
            state.claim_deadline > monotonic() + 2.0,
            "Client readiness receipt expired",
        )?;
    }
    let directory = state.root().join("game");
    let mut game: Value = read(&directory.join("game.json"))?;
    require(game["phase"] == "prepared", "Game is already active")?;
    game["phase"] = "armed".into();
    write(&directory.join("game.json"), &game)?;
    let finalize = format!(
        "--property=ExecStopPost=/usr/bin/systemd-run --collect --unit={}-game-finalize {} game-recover {}",
        state.unit(),
        crate::session::BINARY,
        state.id
    );
    task(
        state,
        "-game",
        "game",
        &[
            "--property=RuntimeMaxSec=infinity".into(),
            "--property=ProtectSystem=strict".into(),
            "--property=ProtectHome=read-only".into(),
            "--property=PrivateTmp=yes".into(),
            "--property=KillMode=control-group".into(),
            "--property=TimeoutStopSec=5".into(),
            format!("--property=ReadWritePaths={}", directory.display()),
            finalize,
        ],
    )
}

fn pause(state: &State, game: &mut Value) -> Result<()> {
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
            cmd(&["systemctl", "show", ES, "-p", "FragmentPath", "--value"])? == ES_FILE,
            "Unexpected menu service definition",
        )?;
        game["frontend_service"] =
            json!({"unit":ES,"definition_sha256":digest(Path::new(ES_FILE))?});
        write(&state.root().join("game/game.json"), game)?;
        cmd(&["systemctl", "stop", ES])?;
    }
    Ok(())
}

fn resume(state: &State) -> Result<()> {
    let path = state.root().join("game/game.json");
    if !path.exists() {
        return Ok(());
    }
    let mut game: Value = read(&path)?;
    if game["frontend_service"].is_object() {
        require(
            game["frontend_service"]["unit"] == ES
                && game["frontend_service"]["definition_sha256"] == digest(Path::new(ES_FILE))?,
            "Menu ownership changed during game handoff",
        )?;
        if best(&["systemctl", "is-active", ES]) != "active" {
            cmd(&["systemctl", "start", ES])?;
        }
        game.as_object_mut().unwrap().remove("frontend_service");
        write(&path, &game)?;
    }
    Ok(())
}

pub fn run(id: &str) -> Result<()> {
    crate::session::identity(false)?;
    let state = current(id)?;
    let directory = state.root().join("game");
    // A separate lock prevents two native game workers from taking the display.
    let _owner = Lock::existing(&Path::new(crate::session::BASE).join("game-owner.lock"))?;
    let result = (|| -> Result<()> {
        for name in ["retroarch", "retroarch32"] {
            require(
                best(&["pgrep", "-x", name]).is_empty(),
                "Another game is already running",
            )?;
        }
        let context: Context = read(&state.root().join("context.json"))?;
        context.revalidate()?;
        let room = Room::decode(&fs::read(directory.join("room.json"))?)?;
        require(
            room.game().compatible(&context.identity),
            "Prepared game identity changed",
        )?;
        if state.role == "client" {
            VerifiedRoom::observe(room, &state.room.bssid, &state.room.token)?;
        }
        let mut game: Value = read(&directory.join("game.json"))?;
        require(game["session_id"] == id, "Game ownership changed")?;
        pause(&state, &mut game)?;
        let latest = current(id)?;
        if state.role == "host" {
            live_peer(&latest)?;
        } else {
            require(
                latest.claim_deadline > monotonic() + 2.0,
                "Client receipt expired after display handoff",
            )?;
        }
        let (uid, _) = account()?;
        let mut args = vec![
            "-n".into(),
            "-u".into(),
            "ark".into(),
            "env".into(),
            format!("XDG_RUNTIME_DIR=/run/user/{uid}"),
            "TERM=linux".into(),
        ];
        args.extend(frontend_arguments(
            &context,
            &directory,
            state.role == "host",
        ));
        if let Some(runtime) =
            crate::core_runtime::directory(&context.core_id, context.core_elf_bits)?
        {
            args.insert(4, format!("LD_LIBRARY_PATH={runtime}"));
        }
        if let Some(compression) = crate::netplay_compression::environment(&context)? {
            args.insert(4, compression);
        }
        let log = File::create(directory.join("game.log"))?;
        game["phase"] = "running".into();
        write(&directory.join("game.json"), &game)?;
        let mut child = OwnedChild(
            Command::new("sudo")
                .args(args)
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log))
                .spawn()?,
        );
        let status = child.0.wait()?;
        game["phase"] = if status.success() { "exited" } else { "failed" }.into();
        game["returncode"] = json!(status.code());
        write(&directory.join("game.json"), &game)?;
        Ok(())
    })();
    if let Err(error) = &result {
        let mut game = read::<Value>(&directory.join("game.json")).unwrap_or_else(|_| json!({}));
        game["phase"] = "failed".into();
        game["error"] = error.to_string().into();
        let _ = write(&directory.join("game.json"), &game);
    }
    result
}

pub fn recover(state: &State) -> Result<()> {
    let directory = state.root().join("game");
    if !directory.is_dir() {
        return Ok(());
    }
    best(&[
        "systemctl",
        "stop",
        &format!("{}-game.service", state.unit()),
    ]);
    Ok(())
}

pub fn restore_menu(state: &State) -> Result<()> {
    resume(state)
}

fn owns_listener(table: &str, owned: &std::collections::BTreeSet<String>) -> bool {
    table.lines().any(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        fields.len() >= 10
            && fields[3] == "0A"
            && fields[1]
                .split_once(':')
                .and_then(|(_, port)| u16::from_str_radix(port, 16).ok())
                == Some(55435)
            && owned.contains(fields[9])
    })
}

pub fn listening(state: &State) -> bool {
    let directory = state.root().join("game");
    let Ok(game) = read::<Value>(&directory.join("game.json")) else {
        return false;
    };
    if state.role != "host" || game["session_id"] != state.id || game["phase"] != "running" {
        return false;
    }
    let Ok(context) = read::<Context>(&state.root().join("context.json")) else {
        return false;
    };
    let Ok(frontend) = fs::canonicalize(&context.frontend_path) else {
        return false;
    };
    let group = format!("/system.slice/{}-game.service", state.unit());
    let Ok(processes) = fs::read_dir("/proc") else {
        return false;
    };
    let mut sockets = std::collections::BTreeSet::new();
    for process in processes.flatten() {
        if !process
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|c| c.is_ascii_digit())
        {
            continue;
        }
        let root = process.path();
        if fs::read_link(root.join("exe")).ok().as_ref() != Some(&frontend)
            || !fs::read_to_string(root.join("cgroup")).is_ok_and(|text| {
                text.lines()
                    .any(|line| line.rsplit(':').next() == Some(group.as_str()))
            })
        {
            continue;
        }
        if let Ok(files) = fs::read_dir(root.join("fd")) {
            for file in files.flatten() {
                if let Ok(link) = fs::read_link(file.path()) {
                    if let Some(inode) = link
                        .to_str()
                        .and_then(|s| s.strip_prefix("socket:["))
                        .and_then(|s| s.strip_suffix(']'))
                    {
                        sockets.insert(inode.into());
                    }
                }
            }
        }
    }
    ["/proc/net/tcp", "/proc/net/tcp6"]
        .iter()
        .any(|path| fs::read_to_string(path).is_ok_and(|table| owns_listener(&table, &sockets)))
}
pub fn detached_recover(id: &str) -> Result<()> {
    let state = crate::session::state()?;
    if state.id == id {
        require(
            best(&[
                "systemctl",
                "is-active",
                &format!("{}-game.service", state.unit()),
            ]) != "active",
            "A running native game still owns the display",
        )?;
        let path = state.root().join("game/game.json");
        if path.is_file() {
            let mut game: Value = read(&path)?;
            if matches!(game["phase"].as_str(), Some("armed" | "running")) {
                game["phase"] = "exited".into();
                game["end_reason"] = "detached_worker_exit".into();
                write(&path, &game)?;
            }
        }
        crate::session::recover(id, "native_game_exit")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_start_notification_requires_the_owned_listening_socket() {
        let table = "0: 00000000:D88B 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1002 0 2345\n";
        let owned = std::collections::BTreeSet::from(["2345".to_owned()]);
        assert!(owns_listener(table, &owned));
        assert!(!owns_listener(&table.replace("0A", "01"), &owned));
        assert!(!owns_listener(&table.replace("D88B", "2222"), &owned));
        assert!(!owns_listener(
            table,
            &std::collections::BTreeSet::from(["9999".to_owned()])
        ));
    }
}
