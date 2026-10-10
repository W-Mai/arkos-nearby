use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::content::{confined, digest, fingerprint_with_header};
use crate::handheld_link::LinkMode;
use crate::protocol::{CoreIdentity, GameIdentity, Profile, VerifiedRoom};
use crate::system::{command, mkdir, read, write};
use crate::{Result, require};

pub const INSTALL: &str = "/opt/arkos-nearby";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub frontend: String,
    pub frontend_path: PathBuf,
    pub frontend_sha256: String,
    pub core_id: String,
    pub core_path: PathBuf,
    pub game_path: PathBuf,
    pub title: String,
    pub identity: GameIdentity,
    pub core_sha256: String,
    pub core_elf_bits: u8,
    pub block_extract: bool,
    pub supported: bool,
    #[serde(default)]
    pub link_mode: Option<LinkMode>,
    pub argv: Vec<String>,
}

pub struct Core {
    pub identity: CoreIdentity,
    pub path: PathBuf,
    pub sha256: String,
    pub bits: u8,
    pub extensions: Vec<String>,
    pub block_extract: bool,
    pub supported: bool,
}

pub fn config(frontend: &str) -> Result<PathBuf> {
    require(
        matches!(frontend, "retroarch" | "retroarch32"),
        "Unknown installed frontend",
    )?;
    Ok(Path::new("/home/ark/.config").join(frontend))
}

pub fn flags(path: &Path) -> Result<std::collections::HashMap<String, String>> {
    let mut result = std::collections::HashMap::new();
    if !path.is_file() {
        return Ok(result);
    }
    for line in fs::read_to_string(path)?.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if key.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                && value.starts_with('"')
                && value.ends_with('"')
                && value.len() >= 2
            {
                result.insert(key.into(), value[1..value.len() - 1].into());
            }
        }
    }
    Ok(result)
}

pub fn inspect(path: &Path, frontend: &str) -> Result<Core> {
    let owned = crate::owned_core::by_path(path, frontend);
    let root = if owned.is_some() {
        PathBuf::from(crate::owned_core::ROOT).canonicalize()?
    } else {
        config(frontend)?.join("cores").canonicalize()?
    };
    let path = confined(path, std::slice::from_ref(&root))?;
    require(
        path.parent() == Some(root.as_path()),
        "Core is outside the installed registry",
    )?;
    let filename = path
        .file_name()
        .ok_or("Core has no filename")?
        .to_string_lossy();
    let id = filename
        .strip_suffix("_libretro.so")
        .ok_or("Unknown local core filename")?
        .to_owned();
    require(
        crate::owned_core::entry(&id).is_none() || owned.is_some(),
        "Owned core identifier is outside its fixed registry path",
    )?;
    let sha256 = digest(&path)?;
    if let Some(entry) = owned {
        require(
            path.file_name().is_some_and(|name| name == entry.filename) && sha256 == entry.sha256,
            "Owned core artifact differs",
        )?;
    }
    let mut header = [0; 6];
    fs::File::open(&path)?.read_exact(&mut header)?;
    require(
        header.len() > 5 && header[..4] == *b"\x7fELF" && matches!(header[4], 1 | 2),
        "Core is not an installed ELF library",
    )?;
    let bits = if header[4] == 1 { 32 } else { 64 };
    if let Some(entry) = owned {
        require(bits == entry.bits, "Owned core ABI differs")?;
    }
    let helper = format!("{INSTALL}/core-inspect{bits}");
    let output = if let Some(runtime) = crate::core_runtime::directory(&id, bits)? {
        command(
            "env",
            &[
                &format!("LD_LIBRARY_PATH={runtime}"),
                &helper,
                path.to_str().ok_or("Invalid core path")?,
            ],
            10,
            true,
        )?
    } else {
        command(
            &helper,
            &[path.to_str().ok_or("Invalid core path")?],
            10,
            true,
        )?
    };
    let metadata: Value = output
        .lines()
        .rev()
        .find_map(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .filter(|v| v.is_object())
        })
        .ok_or("Core returned no native metadata")?;
    require(
        metadata["pointer_bits"].as_u64() == Some(bits as u64)
            && metadata["need_fullpath"].is_boolean()
            && metadata["block_extract"].is_boolean(),
        "Core returned invalid ABI metadata",
    )?;
    if let Some(entry) = owned {
        entry.validate_metadata(&metadata)?;
    }
    let name = metadata["library_name"]
        .as_str()
        .ok_or("Core returned no name")?
        .into();
    let version = metadata["library_version"]
        .as_str()
        .ok_or("Core returned no version")?
        .into();
    let identity = CoreIdentity { id, name, version };
    identity.validate()?;
    require(
        metadata["valid_extensions"].is_null() || metadata["valid_extensions"].is_string(),
        "Core returned invalid extensions",
    )?;
    let extensions = metadata["valid_extensions"]
        .as_str()
        .unwrap_or("")
        .split('|')
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect();
    let supported = if owned.is_some() {
        true
    } else {
        let info_path = path.with_extension("info");
        if info_path.is_file() {
            require(
                info_path.canonicalize()?.parent() == Some(root.as_path()),
                "Core information is outside registry",
            )?;
        }
        let values = flags(&info_path)?;
        values.get("savestate").is_some_and(|v| v == "true")
            && values
                .get("savestate_features")
                .is_some_and(|v| v.split('|').any(|v| v == "deterministic"))
    };
    Ok(Core {
        identity,
        sha256,
        path,
        bits,
        extensions,
        block_extract: metadata["block_extract"].as_bool().unwrap(),
        supported,
    })
}

pub fn resolve_core(frontend: &str, id: &str) -> Result<PathBuf> {
    if let Some(entry) = crate::owned_core::entry(id) {
        require(frontend == entry.frontend, "Owned core frontend differs")?;
        return Ok(entry.path());
    }
    require(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
        "Invalid local core identifier",
    )?;
    Ok(config(frontend)?
        .join("cores")
        .join(format!("{id}_libretro.so")))
}

pub fn library_argument(args: &[String]) -> Result<String> {
    let mut paths = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        if matches!(arg.as_str(), "-L" | "--libretro") {
            paths.push(
                args.get(index + 1)
                    .ok_or("Launch has no core argument")?
                    .clone(),
            );
        } else if let Some(path) = arg.strip_prefix("--libretro=") {
            paths.push(path.into());
        }
    }
    require(paths.len() == 1, "Launch must select one installed core")?;
    Ok(paths.remove(0))
}

pub fn frontend_version(frontend: &str, executable: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let info = fs::metadata(executable)?;
    let key = json!([
        executable,
        fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim(),
        info.dev(),
        info.ino(),
        info.size(),
        info.mtime(),
        info.mtime_nsec(),
        info.ctime(),
        info.ctime_nsec()
    ]);
    let root = Path::new("/run/arkos-nearby-x/frontend-versions");
    let path = root.join(format!("{frontend}-native.json"));
    if let Ok(info) = path.metadata() {
        if info.uid() == 0 && info.mode() & 0o022 == 0 {
            if let Ok(saved) = read::<Value>(&path) {
                if saved["key"] == key {
                    if let Some(version) = saved["version"].as_str() {
                        return Ok(version.into());
                    }
                }
            }
        }
    }
    let output = command(
        executable.to_str().ok_or("Invalid frontend path")?,
        &["--version"],
        5,
        true,
    )?;
    let version = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("Version:").map(str::trim))
        .and_then(|line| line.split_whitespace().next())
        .ok_or("Frontend returned no version")?;
    require(
        version.split('.').count() == 3
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())),
        "Invalid frontend version",
    )?;
    if unsafe { libc::geteuid() } == 0 {
        mkdir(root)?;
        write(&path, &json!({"key": key, "version": version}))?;
    }
    Ok(version.into())
}

pub fn parse(frontend: &str, args: &[String]) -> Result<Context> {
    parse_selected(frontend, args, true)
}

fn replace_core(args: &[String], path: &Path) -> Vec<String> {
    let mut args = args.to_vec();
    for index in 0..args.len() {
        if index > 0 && matches!(args[index - 1].as_str(), "-L" | "--libretro") {
            args[index] = path.to_string_lossy().into();
        } else if args[index].starts_with("--libretro=") {
            args[index] = format!("--libretro={}", path.display());
        }
    }
    args
}

fn parse_selected(frontend: &str, args: &[String], adapt: bool) -> Result<Context> {
    let mut frontend = frontend.to_owned();
    let mut args = args.to_vec();
    let mut core = inspect(Path::new(&library_argument(&args)?), &frontend)?;
    let roots: Vec<_> = ["/roms", "/roms2"]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect();
    let games: Vec<_> = args
        .iter()
        .filter(|arg| !arg.starts_with('-'))
        .filter_map(|arg| {
            let path = Path::new(arg);
            let extension = path.extension()?.to_str()?;
            if !extension.eq_ignore_ascii_case("zip")
                && !core
                    .extensions
                    .iter()
                    .any(|v| v.eq_ignore_ascii_case(extension))
            {
                return None;
            }
            confined(path, &roots).ok()
        })
        .collect();
    require(games.len() == 1, "Launch must select one local game")?;
    let game_path = games[0].clone();
    if adapt
        && let Some(id) = crate::handheld_link::preferred(&game_path)
        && (core.identity.id != id || frontend != "retroarch")
    {
        let path = config("retroarch")?
            .join("cores")
            .join(format!("{id}_libretro.so"));
        core = inspect(&path, "retroarch")?;
        frontend = "retroarch".into();
        args = replace_core(&args, &path);
    }
    if adapt && !core.supported {
        for choice in crate::core_choice::candidates(&game_path) {
            let path = config(choice.frontend)?
                .join("cores")
                .join(format!("{}_libretro.so", choice.id));
            if let Ok(candidate) = inspect(&path, choice.frontend)
                && candidate.supported
            {
                core = candidate;
                frontend = choice.frontend.into();
                args = replace_core(&args, &path);
                break;
            }
        }
    }
    if adapt
        && let Some(entry) = crate::owned_core::replacement(
            &frontend,
            &core.identity.id,
            &core.sha256,
            core.bits,
            &game_path,
        )
    {
        let path = entry.path();
        core = inspect(&path, entry.frontend)?;
        args = replace_core(&args, &path);
    }
    let (mut content, mut header) =
        fingerprint_with_header(&game_path, &core.extensions, &roots, core.block_extract)?;
    if adapt
        && let Some(rule) = crate::core_choice::performance_candidate(
            &frontend,
            &core.identity.id,
            &core.sha256,
            core.bits,
            &content.sha256,
        )
    {
        let path = config(rule.target.frontend)?
            .join("cores")
            .join(format!("{}_libretro.so", rule.target.id));
        if let Ok(candidate) = inspect(&path, rule.target.frontend)
            && candidate.supported
            && rule.matches_target(
                rule.target.frontend,
                &candidate.identity.id,
                &candidate.sha256,
                candidate.bits,
            )
        {
            let (candidate_content, candidate_header) = fingerprint_with_header(
                &game_path,
                &candidate.extensions,
                &roots,
                candidate.block_extract,
            )?;
            require(
                candidate_content.same_bytes(&content),
                "Performance candidate interprets different content",
            )?;
            core = candidate;
            frontend = rule.target.frontend.into();
            args = replace_core(&args, &path);
            content = candidate_content;
            header = candidate_header;
        }
    }
    let configuration = config(&frontend)?;
    let frontend_root = PathBuf::from("/opt/retroarch/bin").canonicalize()?;
    let frontend_path = confined(
        &frontend_root.join(&frontend),
        std::slice::from_ref(&frontend_root),
    )?;
    require(
        frontend_path.parent() == Some(frontend_root.as_path()),
        "Frontend is outside the installed registry",
    )?;
    let version = frontend_version(&frontend, &frontend_path)?;
    let link_mode = crate::handheld_link::select(&core.identity.id, &header);
    let supported = core.supported
        && (!(crate::handheld_link::preferred(&game_path).is_some()
            || matches!(core.identity.id.as_str(), "gpsp" | "tgbdual"))
            || link_mode.is_some());
    let identity = GameIdentity {
        frontend_version: version,
        core: core.identity.clone(),
        content,
    };
    identity.validate()?;
    require(configuration.is_dir(), "Missing installed configuration")?;
    Ok(Context {
        frontend,
        frontend_sha256: digest(&frontend_path)?,
        frontend_path,
        core_id: core.identity.id,
        core_path: core.path,
        game_path,
        title: identity.content.title.clone(),
        identity,
        core_sha256: core.sha256,
        core_elf_bits: core.bits,
        block_extract: core.block_extract,
        supported,
        link_mode,
        argv: args,
    })
}

impl Context {
    pub fn key(&self) -> Value {
        json!([
            self.identity.frontend_version,
            self.core_id,
            self.identity.core.version,
            self.identity.content.sha256
        ])
    }
    pub fn revalidate(&self) -> Result<()> {
        let current = parse_selected(&self.frontend, &self.argv, false)?;
        require(
            current == *self && self.supported,
            "Prepared local identities changed or are unsupported",
        )
    }
    pub fn profile(&self, session_id: &str) -> Profile {
        Profile {
            session_id: session_id.into(),
            frontend: self.frontend.clone(),
            frontend_sha256: self.frontend_sha256.clone(),
            core_id: self.core_id.clone(),
            core_sha256: self.core_sha256.clone(),
            elf_bits: self.core_elf_bits,
        }
    }
    pub fn matching(&self, room: &VerifiedRoom, profile: &Profile) -> Result<Self> {
        profile.validate(room)?;
        room.room().core.validate()?;
        let path = resolve_core(&profile.frontend, &profile.core_id)?;
        let args = replace_core(&self.argv, &path);
        let matched = parse_selected(&profile.frontend, &args, false)?;
        require(
            matched.supported
                && matched.game_path == self.game_path
                && matched.core_elf_bits == profile.elf_bits
                && matched.core_sha256 == profile.core_sha256
                && matched.frontend_sha256 == profile.frontend_sha256
                && matched.identity.compatible(&room.room().game()),
            "Corresponding installed game, core or frontend differs",
        )?;
        Ok(matched)
    }
}
