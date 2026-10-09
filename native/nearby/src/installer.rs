use std::collections::BTreeMap;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::bundle::{ALLOWED, Bundle};
use crate::compat::Device;
use crate::content::digest;
use crate::system::{mkdir, private_write, random_id, read, write};
use crate::{Result, require};

pub const ORIGINAL_SHA: &str = "4d36f0cbab754991eb002b47ba719b9f0986f6ab0f9e66d6830ff8a2e159ab44";
const START: &[u8] = b"if [ \"$?\" -eq \"10\" ]; then\n";
const END: &[u8] = b"if [ -f \"/boot/rk3566.dtb\" ]";
pub const BLOCK:&[u8]=b"if [ \"$?\" -eq \"10\" ]; then\n  # ARKOS_NEARBY_X_BEGIN\n  sudo -n /opt/arkos-nearby/arkos-nearby menu --frontend \"$emulator\" -- \"$@\"\n  result=$?\n  if [ \"$result\" -ne \"230\" ]; then\n    exit \"$result\"\n  fi\n  # ARKOS_NEARBY_X_END\nfi\n\n";
pub const ENTRY: &[u8] = b"#!/bin/sh\nexec sudo -n /opt/arkos-nearby/arkos-nearby manual\n";
const ENTRY_PATH: &str = "opt/system/Nearby Multiplayer.sh";
const ROOT: &str = "opt/arkos-nearby";

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn find(bytes: &[u8], pattern: &[u8]) -> Result<usize> {
    bytes
        .windows(pattern.len())
        .position(|value| value == pattern)
        .ok_or_else(|| "Unrecognized launcher structure".into())
}
fn parts(bytes: &[u8]) -> Result<(usize, usize)> {
    let test = find(bytes, b"Test_Button_X\n")?;
    let start = test + find(&bytes[test..], START)?;
    let end = start + find(&bytes[start..], END)?;
    Ok((start, end))
}
pub fn patched(original: &[u8]) -> Result<Vec<u8>> {
    require(
        hash(original) == ORIGINAL_SHA,
        "No supported profile for this launcher",
    )?;
    let (start, end) = parts(original)?;
    let mut bytes = original[..start].to_vec();
    bytes.extend_from_slice(BLOCK);
    bytes.extend_from_slice(&original[end..]);
    Ok(bytes)
}
pub fn owned_launcher(original: &[u8], current: &[u8]) -> Result<()> {
    require(
        hash(original) == ORIGINAL_SHA,
        "Original launcher backup differs",
    )?;
    if current == original {
        return Ok(());
    }
    let (start, end) = parts(original)?;
    let (cur_start, cur_end) = parts(current)?;
    require(
        original[..start] == current[..cur_start]
            && original[end..] == current[cur_end..]
            && current[cur_start..cur_end] == *BLOCK,
        "Launcher contains unrelated modifications",
    )
}

#[derive(Clone)]
struct Layout {
    prefix: PathBuf,
}
impl Layout {
    fn system() -> Self {
        Self { prefix: "/".into() }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.prefix.join(name)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.path(ROOT).join(name)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u8,
    version: String,
    device: Device,
    files: BTreeMap<String, String>,
    wrappers: BTreeMap<String, String>,
    entry_sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Undo {
    target: String,
    backup: String,
    existed: bool,
    mode: u32,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u8,
    directory: String,
    undo: Vec<Undo>,
}
struct Change {
    target: String,
    data: Option<Vec<u8>>,
    mode: u32,
}

fn allowed_target(name: &str) -> bool {
    name == ENTRY_PATH
        || matches!(
            name,
            "usr/local/bin/retroarch" | "usr/local/bin/retroarch32"
        )
        || name.strip_prefix(&format!("{ROOT}/")).is_some_and(|file| {
            ALLOWED.contains(&file)
                || matches!(
                    file,
                    "arkos-nearby"
                        | "device.json"
                        | "pair.json"
                        | "install.json"
                        | "identity.pk8"
                        | "identity.json"
                        | "original-launch/retroarch"
                        | "original-launch/retroarch32"
                )
        })
}
fn recover_transaction(layout: &Layout) -> Result<()> {
    let file = layout.file("install-journal.json");
    if !file.exists() {
        return Ok(());
    }
    let journal: Journal = read(&file)?;
    require(
        journal.schema == 1
            && journal.directory.starts_with(".transaction-")
            && crate::protocol::hex(&journal.directory[13..], 32),
        "Invalid installation transaction",
    )?;
    let directory = layout.file(&journal.directory);
    for undo in journal.undo.iter().rev() {
        require(
            allowed_target(&undo.target) && undo.backup.bytes().all(|byte| byte.is_ascii_digit()),
            "Invalid installation rollback path",
        )?;
        let target = layout.path(&undo.target);
        if undo.existed {
            let bytes = fs::read(directory.join(&undo.backup))?;
            require(
                hash(&bytes) == undo.sha256,
                "Installation rollback checksum differs",
            )?;
            atomic(&target, &bytes, undo.mode)?;
        } else if target.exists() {
            require(!target.is_symlink(), "Unexpected rollback link")?;
            fs::remove_file(target)?;
        }
    }
    fs::remove_file(file)?;
    fs::remove_dir_all(directory)?;
    Ok(())
}
fn atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    require(!path.is_symlink(), "Refusing an unrelated symbolic link")?;
    fs::create_dir_all(path.parent().ok_or("Install target has no parent")?)?;
    let temporary = path.with_extension(format!("{}.install", random_id()?));
    private_write(&temporary, bytes)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))?;
    fs::rename(temporary, path)?;
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
fn transact(layout: &Layout, changes: Vec<Change>) -> Result<()> {
    let directory = format!(".transaction-{}", random_id()?);
    mkdir(&layout.file(&directory))?;
    let mut undo = Vec::new();
    for (index, change) in changes.iter().enumerate() {
        require(allowed_target(&change.target), "Invalid install target")?;
        let path = layout.path(&change.target);
        require(!path.is_symlink(), "Install target is an unrelated link")?;
        let existed = path.is_file();
        let (bytes, mode) = if existed {
            (
                fs::read(&path)?,
                path.metadata()?.permissions().mode() & 0o777,
            )
        } else {
            (vec![], 0o644)
        };
        let backup = index.to_string();
        private_write(&layout.file(&directory).join(&backup), &bytes)?;
        undo.push(Undo {
            target: change.target.clone(),
            backup,
            existed,
            mode,
            sha256: hash(&bytes),
        });
    }
    let journal = Journal {
        schema: 1,
        directory: directory.clone(),
        undo,
    };
    write(&layout.file("install-journal.json"), &journal)?;
    let result = (|| -> Result<()> {
        for change in changes {
            let path = layout.path(&change.target);
            if let Some(bytes) = change.data {
                atomic(&path, &bytes, change.mode)?;
            } else if path.is_file() {
                fs::remove_file(&path)?;
                File::open(path.parent().unwrap())?.sync_all()?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        recover_transaction(layout)?;
        return Err(error);
    }
    fs::remove_file(layout.file("install-journal.json"))?;
    File::open(layout.path(ROOT))?.sync_all()?;
    fs::remove_dir_all(layout.file(&directory))?;
    Ok(())
}
fn verify_record(record: &Record, layout: &Layout) -> Result<()> {
    require(record.schema == 1, "Unsupported installation record")?;
    for (name, expected) in &record.files {
        require(
            ALLOWED.contains(&name.as_str())
                || matches!(name.as_str(), "arkos-nearby" | "device.json"),
            "Installation record contains an unrelated file",
        )?;
        let path = layout.file(name);
        require(
            path.is_file() && digest(&path)? == *expected,
            &format!("Installed file was changed externally: {name}"),
        )?;
    }
    for (name, expected) in &record.wrappers {
        require(
            matches!(name.as_str(), "retroarch" | "retroarch32"),
            "Invalid recorded launcher",
        )?;
        require(
            digest(&layout.path(&format!("usr/local/bin/{name}")))? == *expected,
            "Launcher was changed externally",
        )?;
    }
    require(
        digest(&layout.path(ENTRY_PATH))? == record.entry_sha256,
        "Options entry was changed externally",
    )
}
fn check_idle_ui() -> Result<()> {
    for process in fs::read_dir("/proc")?.filter_map(|entry| entry.ok()) {
        if !process
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let Ok(bytes) = fs::read(process.path().join("cmdline")) else {
            continue;
        };
        let args: Vec<_> = bytes.split(|byte| *byte == 0).collect();
        let owned = args
            .iter()
            .any(|arg| *arg == crate::session::BINARY.as_bytes());
        require(
            !(owned
                && args.iter().any(|arg| {
                    matches!(*arg, b"menu" | b"manual" | b"worker" | b"game" | b"serve")
                })),
            "Exit the room menu before installing or uninstalling",
        )?;
    }
    let old = Path::new("/run/arkos-nearby-x/session.json");
    if old.is_file() {
        let value: Value = read(old)?;
        require(
            matches!(value["phase"].as_str(), Some("closed" | "failed"))
                && value["network_restored"] == true,
            "Close the previous room before installing",
        )?;
    }
    require(
        !Path::new("/etc/systemd/system/arkos-nearby.service").exists(),
        "Remove the old persistent multiplayer service before installing",
    )
}

fn install_at(
    layout: &Layout,
    device: Device,
    bundle: &Bundle,
    executable: &Path,
) -> Result<Value> {
    fs::create_dir_all(layout.path(ROOT))?;
    recover_transaction(layout)?;
    let record_path = layout.file("install.json");
    let previous = if record_path.exists() {
        let record: Record = read(&record_path)?;
        verify_record(&record, layout)?;
        Some(record)
    } else {
        None
    };
    let mut changes = Vec::new();
    let mut wrappers = BTreeMap::new();
    for name in ["retroarch", "retroarch32"] {
        let path = layout.path(&format!("usr/local/bin/{name}"));
        let current = fs::read(&path)?;
        let backup = layout.file(&format!("original-launch/{name}"));
        let original = if backup.is_file() {
            fs::read(&backup)?
        } else {
            current.clone()
        };
        owned_launcher(&original, &current)?;
        if !backup.exists() {
            changes.push(Change {
                target: format!("{ROOT}/original-launch/{name}"),
                data: Some(original.clone()),
                mode: 0o755,
            });
        }
        let updated = patched(&original)?;
        wrappers.insert(name.into(), hash(&updated));
        changes.push(Change {
            target: format!("usr/local/bin/{name}"),
            data: Some(updated),
            mode: 0o755,
        });
    }
    let entry = layout.path(ENTRY_PATH);
    if previous.is_none() && entry.exists() {
        let current = fs::read(&entry)?;
        require(current==ENTRY || current==b"#!/bin/sh\nexec sudo -n /usr/bin/python3 /opt/arkos-nearby/nearby_manual.py\n","Options entry has unrelated contents")?;
    }
    let mut files = BTreeMap::new();
    for asset in &bundle.manifest.files {
        let path = layout.file(&asset.name);
        if previous.is_none() && path.exists() {
            let legacy_gui = asset.name == "arkos-nearby-gui"
                && digest(&path)?
                    == "4f96c8b47671b325633648368a94568cb4e431195bca3bbb5fbaa64649c0e371";
            let legacy_report = asset.name == "nearby-gui-build.json"
                && read::<Value>(&path).is_ok_and(|report| {
                    report["sha256"]
                        == "4f96c8b47671b325633648368a94568cb4e431195bca3bbb5fbaa64649c0e371"
                });
            require(
                digest(&path)? == asset.sha256 || legacy_gui || legacy_report,
                "Existing native asset differs from this release",
            )?;
        }
        files.insert(asset.name.clone(), asset.sha256.clone());
        changes.push(Change {
            target: format!("{ROOT}/{}", asset.name),
            data: Some(bundle.bytes(asset).to_vec()),
            mode: asset.mode,
        });
    }
    let binary = fs::read(executable)?;
    require(
        binary.len() > 64
            && binary[..5] == *b"\x7fELF\x02"
            && u16::from_le_bytes(binary[18..20].try_into().unwrap()) == 183,
        "Installer must be the ARM64 release executable",
    )?;
    files.insert("arkos-nearby".into(), hash(&binary));
    changes.push(Change {
        target: format!("{ROOT}/arkos-nearby"),
        data: Some(binary),
        mode: 0o755,
    });
    let bytes = serde_json::to_vec(&device)?;
    changes.push(Change {
        target: format!("{ROOT}/device.json"),
        data: Some(bytes),
        mode: 0o600,
    });
    if !layout.file("pair.json").exists() {
        let code = crate::pairing::generate()?;
        let (psk, group) = crate::pairing::credential(&code)?;
        changes.push(Change {
            target: format!("{ROOT}/pair.json"),
            data: Some(serde_json::to_vec(
                &json!({"schema":2,"psk":psk,"pairing_code":code,"group_id":group}),
            )?),
            mode: 0o600,
        });
    } else {
        crate::pairing::ensure_at(&layout.file("pair.json"))?;
    }
    let record = Record {
        schema: 1,
        version: bundle.manifest.version.clone(),
        device,
        files,
        wrappers,
        entry_sha256: hash(ENTRY),
    };
    if let Some((key, metadata)) =
        crate::identity::prepare_at(&layout.path(ROOT), &record.device.mac)?
    {
        changes.push(Change {
            target: format!("{ROOT}/identity.pk8"),
            data: Some(key),
            mode: 0o600,
        });
        changes.push(Change {
            target: format!("{ROOT}/identity.json"),
            data: Some(metadata),
            mode: 0o600,
        });
    }
    changes.push(Change {
        target: ENTRY_PATH.into(),
        data: Some(ENTRY.to_vec()),
        mode: 0o755,
    });
    changes.push(Change {
        target: format!("{ROOT}/install.json"),
        data: Some(serde_json::to_vec(&record)?),
        mode: 0o600,
    });
    transact(layout, changes)?;
    Ok(
        json!({"action":"install","version":record.version,"profile":record.device.profile,"installed":true,"network_changed":false,"game_started":false,"files":record.files.keys().collect::<Vec<_>>()}),
    )
}

pub fn install() -> Result<Value> {
    require(
        unsafe { libc::geteuid() } == 0,
        "Run the installer through sudo or the SD-card launcher",
    )?;
    let facts = crate::compat::detect();
    let device = facts.device()?;
    let bundle = Bundle::embedded()?;
    check_idle_ui()?;
    crate::session::while_idle(|| {
        install_at(
            &Layout::system(),
            device,
            &bundle,
            &std::env::current_exe()?,
        )
    })
}

fn uninstall_at(layout: &Layout) -> Result<Value> {
    recover_transaction(layout)?;
    let record: Record = read(&layout.file("install.json"))?;
    verify_record(&record, layout)?;
    let mut changes = Vec::new();
    for name in record.wrappers.keys() {
        let original = fs::read(layout.file(&format!("original-launch/{name}")))?;
        require(
            hash(&original) == ORIGINAL_SHA,
            "Original launcher backup differs",
        )?;
        changes.push(Change {
            target: format!("usr/local/bin/{name}"),
            data: Some(original),
            mode: 0o755,
        });
    }
    changes.push(Change {
        target: ENTRY_PATH.into(),
        data: None,
        mode: 0o755,
    });
    for name in record.files.keys() {
        changes.push(Change {
            target: format!("{ROOT}/{name}"),
            data: None,
            mode: 0o644,
        });
    }
    changes.push(Change {
        target: format!("{ROOT}/install.json"),
        data: None,
        mode: 0o600,
    });
    transact(layout, changes)?;
    Ok(
        json!({"action":"uninstall","launchers_restored":true,"retained":["device.json","pair.json","identity.json","identity.pk8","friends.json","logs/","original-launch/","python-migration-backup/"],"network_changed":false}),
    )
}
pub fn uninstall() -> Result<Value> {
    crate::session::identity(false)?;
    check_idle_ui()?;
    crate::session::while_idle(|| uninstall_at(&Layout::system()))
}
pub fn release_info() -> Result<Value> {
    let bundle = Bundle::embedded()?;
    Ok(serde_json::to_value(bundle.manifest)?)
}
pub fn update(path: &Path) -> Result<Value> {
    require(
        unsafe { libc::geteuid() } == 0,
        "Updating requires the installed root wrapper",
    )?;
    let binary = path.canonicalize()?;
    require(binary.is_file(), "Choose the downloaded release executable")?;
    let text = crate::system::command(
        binary.to_str().ok_or("Invalid release path")?,
        &["release-info"],
        10,
        true,
    )?;
    let manifest: crate::bundle::Manifest = serde_json::from_str(&text)?;
    require(
        manifest.profile == crate::compat::PROFILE
            && manifest.kernel_sha256 == crate::compat::KERNEL_SHA,
        "Downloaded release has no profile for this firmware",
    )?;
    let output = crate::system::command(binary.to_str().unwrap(), &["install"], 60, true)?;
    Ok(serde_json::from_str(&output)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rollback_targets_are_limited_to_owned_files_and_launcher_entries() {
        for path in [
            "etc/passwd",
            "boot/Image",
            "usr/local/bin/netplay.sh",
            "opt/arkos-nearby/../else",
            "opt/arkos-nearby/pair.json/child",
        ] {
            assert!(!allowed_target(path));
        }
        assert!(allowed_target(ENTRY_PATH));
        assert!(allowed_target("opt/arkos-nearby/assets/8188eu.ko"));
    }
    #[test]
    fn interrupted_owned_transaction_restores_the_previous_bytes() {
        let root = std::env::temp_dir().join(format!("nearby-installer-{}", random_id().unwrap()));
        let layout = Layout {
            prefix: root.clone(),
        };
        mkdir(&layout.path(ROOT)).unwrap();
        let path = layout.path(ENTRY_PATH);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"old").unwrap();
        let directory = format!(".transaction-{}", random_id().unwrap());
        mkdir(&layout.file(&directory)).unwrap();
        private_write(&layout.file(&directory).join("0"), b"old").unwrap();
        write(
            &layout.file("install-journal.json"),
            &Journal {
                schema: 1,
                directory,
                undo: vec![Undo {
                    target: ENTRY_PATH.into(),
                    backup: "0".into(),
                    existed: true,
                    mode: 0o755,
                    sha256: hash(b"old"),
                }],
            },
        )
        .unwrap();
        fs::write(&path, b"new").unwrap();
        recover_transaction(&layout).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert!(!layout.file("install-journal.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
