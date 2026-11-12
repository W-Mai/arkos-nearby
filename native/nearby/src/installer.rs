use howff_platform::arkos;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::bundle::{ALLOWED, Asset, Bundle};
use crate::compat::Device;
use crate::content::digest;
use crate::system::read;
use crate::{Result, require};

mod transaction;
use transaction::{Change, Layout, recover_transaction, transact};

pub const ORIGINAL_SHA: &str = "4d36f0cbab754991eb002b47ba719b9f0986f6ab0f9e66d6830ff8a2e159ab44";
const START: &[u8] = b"if [ \"$?\" -eq \"10\" ]; then\n";
const END: &[u8] = b"if [ -f \"/boot/rk3566.dtb\" ]";
pub static BLOCK: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
    include_str!("launch-wrapper.sh")
        .replace("@HOWFF_BINARY@", arkos::application::EXECUTABLE.as_str())
        .replace("@FRONTEND_DIRECTORY@", arkos::firmware::FRONTENDS.as_str())
        .into_bytes()
});
pub static ENTRY: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| entry("manual"));
const ENTRY_PATH: &str = arkos::firmware::MULTIPLAYER_ENTRY.relative();
const LAUNCHER_PATH: &str = arkos::firmware::LAUNCHER_ENTRY.relative();
static LAUNCHER_ENTRY: std::sync::LazyLock<Vec<u8>> =
    std::sync::LazyLock::new(|| entry("launcher"));
const ROOT: &str = arkos::application::RELATIVE_ROOT;

fn entry(command: &str) -> Vec<u8> {
    format!(
        "#!/bin/sh\nexec sudo -n {} {command}\n",
        arkos::application::EXECUTABLE.as_str()
    )
    .into_bytes()
}

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
    bytes.extend_from_slice(&BLOCK);
    bytes.extend_from_slice(
        String::from_utf8(original[end..].to_vec())?
            .replace(
                &format!("{}/${{emulator}}", arkos::firmware::FRONTENDS.as_str()),
                "howff_launch_frontend",
            )
            .as_bytes(),
    );
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
    require(
        current == patched(original)?,
        "Launcher contains unrelated modifications",
    )
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    product: String,
    schema: u8,
    version: String,
    device: Device,
    assets: Vec<Asset>,
    files: BTreeMap<String, String>,
    wrappers: BTreeMap<String, Wrapper>,
    entry_sha256: String,
    launcher_sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wrapper {
    sha256: String,
    mode: u32,
    uid: u32,
    gid: u32,
}
impl Wrapper {
    fn capture(path: &Path, bytes: &[u8]) -> Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        require(metadata.is_file(), "Launcher is not a regular file")?;
        Ok(Self {
            sha256: hash(bytes),
            mode: metadata.mode() & 0o7777,
            uid: metadata.uid(),
            gid: metadata.gid(),
        })
    }
}
fn allowed_target(name: &str) -> bool {
    name == ENTRY_PATH
        || name == LAUNCHER_PATH
        || name == crate::home::LAUNCHER
        || matches!(
            name,
            "usr/local/bin/retroarch" | "usr/local/bin/retroarch32"
        )
        || name.strip_prefix(&format!("{ROOT}/")).is_some_and(|file| {
            ALLOWED.contains(&file)
                || matches!(
                    file,
                    "howff"
                        | "device.json"
                        | "pair.json"
                        | "install.json"
                        | "identity.pk8"
                        | "identity.json"
                        | "original-launch/retroarch"
                        | "original-launch/retroarch32"
                        | "original-launch/emulationstation"
                )
        })
}
fn verify_record(record: &Record, layout: &Layout) -> Result<()> {
    require(
        record.product == "howff" && record.schema == 1,
        "Unsupported installation record",
    )?;
    let installed: Device = read(&layout.file("device.json")?)?;
    record.device.validate(&installed.mac)?;
    installed.validate(&record.device.mac)?;
    let mut expected = BTreeMap::new();
    for asset in &record.assets {
        require(
            ALLOWED.contains(&asset.name.as_str())
                && expected
                    .insert(asset.name.clone(), asset.sha256.clone())
                    .is_none()
                && matches!(asset.mode, 0o644 | 0o755)
                && asset.size > 0,
            "Invalid recorded release asset",
        )?;
        let path = layout.file(&asset.name)?;
        let metadata = fs::symlink_metadata(path)?;
        require(
            metadata.is_file()
                && metadata.len() == asset.size
                && metadata.mode() & 0o7777 == asset.mode,
            "Installed release asset metadata differs",
        )?;
    }
    expected.insert(
        "howff".into(),
        record
            .files
            .get("howff")
            .ok_or("Missing recorded executable")?
            .clone(),
    );
    require(
        !record.assets.is_empty() && expected == record.files,
        "Installation record does not match its release asset inventory",
    )?;
    require(
        record.wrappers.contains_key("retroarch")
            && record.wrappers.contains_key("retroarch32")
            && record.wrappers.contains_key("emulationstation")
                == layout.path(crate::home::LAUNCHER)?.is_file(),
        "Installation record has an incomplete launcher inventory",
    )?;
    for (name, expected) in &record.files {
        require(
            ALLOWED.contains(&name.as_str()) || name == "howff",
            "Installation record contains an unrelated file",
        )?;
        let path = layout.file(name)?;
        require(
            path.is_file()
                && digest(&path)? == *expected
                && (name != "howff" || fs::metadata(&path)?.mode() & 0o7777 == 0o755),
            &format!("Installed file was changed externally: {name}"),
        )?;
    }
    for (name, expected) in &record.wrappers {
        require(
            matches!(
                name.as_str(),
                "retroarch" | "retroarch32" | "emulationstation"
            ),
            "Invalid recorded launcher",
        )?;
        let path = layout.path(&wrapper_path(name))?;
        let actual = Wrapper::capture(&path, &fs::read(&path)?)?;
        require(
            actual.sha256 == expected.sha256
                && actual.mode == expected.mode
                && actual.uid == expected.uid
                && actual.gid == expected.gid,
            "Launcher was changed externally",
        )?;
    }
    require(
        digest(&layout.path(LAUNCHER_PATH)?)? == record.launcher_sha256
            && fs::metadata(layout.path(LAUNCHER_PATH)?)?.mode() & 0o7777 == 0o755,
        "Launcher entry was changed externally",
    )?;
    require(
        digest(&layout.path(ENTRY_PATH)?)? == record.entry_sha256
            && fs::metadata(layout.path(ENTRY_PATH)?)?.mode() & 0o7777 == 0o755,
        "Options entry was changed externally",
    )
}
fn installed_record(layout: &Layout) -> Result<Record> {
    let path = layout.file("install.json")?;
    let metadata = fs::symlink_metadata(&path)?;
    require(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0
            && metadata.len() <= 1024 * 1024,
        "Invalid installation record ownership",
    )?;
    read(&path)
}
fn admit_existing(path: &Path, expected: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    require(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && digest(path)? == expected,
        "Existing application file has unrelated contents or ownership",
    )
}
fn wrapper_path(name: &str) -> String {
    if name == "emulationstation" {
        crate::home::LAUNCHER.into()
    } else {
        format!("usr/local/bin/{name}")
    }
}
fn check_idle_ui(update_parent: Option<u32>) -> Result<()> {
    for process in fs::read_dir("/proc")?.filter_map(|entry| entry.ok()) {
        if !process
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        if process.file_name().to_string_lossy().parse::<u32>().ok() == update_parent {
            continue;
        }
        let Ok(bytes) = fs::read(process.path().join("cmdline")) else {
            continue;
        };
        let args: Vec<_> = bytes.split(|byte| *byte == 0).collect();
        let owned = args
            .first()
            .is_some_and(|arg| *arg == crate::session::BINARY.as_bytes());
        require(
            !(owned
                && args.iter().any(|arg| {
                    matches!(
                        *arg,
                        b"menu"
                            | b"manual"
                            | b"worker"
                            | b"game"
                            | b"serve"
                            | b"launcher-open"
                            | b"launcher-continue"
                            | b"launcher-standalone"
                            | b"home-dispatch"
                    )
                })),
            "Exit the room menu before installing or uninstalling",
        )?;
    }
    Ok(())
}

fn install_at(
    layout: &Layout,
    device: Device,
    bundle: &Bundle,
    executable: &Path,
) -> Result<Value> {
    recover_transaction(layout)?;
    let record_path = layout.file("install.json")?;
    let previous = if record_path.exists() {
        let record = installed_record(layout)?;
        verify_record(&record, layout)?;
        record.device.validate(&device.mac)?;
        Some(record)
    } else {
        None
    };
    let mut changes = Vec::new();
    let mut wrappers = BTreeMap::new();
    for name in ["retroarch", "retroarch32"] {
        let path = layout.path(&format!("usr/local/bin/{name}"))?;
        let current = fs::read(&path)?;
        let backup = layout.file(&format!("original-launch/{name}"))?;
        let original = if backup.is_file() {
            fs::read(&backup)?
        } else {
            current.clone()
        };
        if previous.as_ref().is_some_and(|record| {
            record
                .wrappers
                .get(name)
                .is_some_and(|wrapper| wrapper.sha256 == hash(&current))
        }) {
            require(
                hash(&original) == ORIGINAL_SHA,
                "Original launcher backup differs",
            )?;
        } else {
            owned_launcher(&original, &current)?;
        }
        if !backup.exists() {
            changes.push(Change {
                target: format!("{ROOT}/original-launch/{name}"),
                data: Some(original.clone()),
                mode: 0o600,
            });
        }
        let updated = patched(&original)?;
        let wrapper = Wrapper::capture(&path, &updated)?;
        let mode = wrapper.mode;
        wrappers.insert(name.into(), wrapper);
        changes.push(Change {
            target: format!("usr/local/bin/{name}"),
            data: Some(updated),
            mode,
        });
    }
    let entry = layout.path(ENTRY_PATH)?;
    let launcher = layout.path(LAUNCHER_PATH)?;
    if previous.is_none() && launcher.exists() {
        admit_existing(&launcher, &hash(&LAUNCHER_ENTRY))?;
    }
    let home = layout.path(crate::home::LAUNCHER)?;
    if home.is_file() {
        let backup_name = "original-launch/emulationstation";
        let backup = layout.file(backup_name)?;
        let current = fs::read(&home)?;
        let original = if backup.is_file() {
            fs::read(&backup)?
        } else {
            current.clone()
        };
        let updated = crate::home::patched(&original)?;
        require(
            current == original
                || current == updated
                || previous.as_ref().is_some_and(|record| {
                    record
                        .wrappers
                        .get("emulationstation")
                        .is_some_and(|wrapper| wrapper.sha256 == hash(&current))
                }),
            "Home launcher contains unrelated edits",
        )?;
        if !backup.exists() {
            changes.push(Change {
                target: format!("{ROOT}/{backup_name}"),
                data: Some(original),
                mode: 0o600,
            });
        }
        let wrapper = Wrapper::capture(&home, &updated)?;
        let mode = wrapper.mode;
        wrappers.insert("emulationstation".into(), wrapper);
        changes.push(Change {
            target: crate::home::LAUNCHER.into(),
            data: Some(updated),
            mode,
        });
    }
    if previous.is_none() && entry.exists() {
        admit_existing(&entry, &hash(&ENTRY))?;
    }
    let mut files = BTreeMap::new();
    for asset in &bundle.manifest.files {
        let path = layout.file(&asset.name)?;
        if previous.is_none() && path.exists() {
            admit_existing(&path, &asset.sha256)?;
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
    let installed_binary = layout.file("howff")?;
    if previous.is_none() && installed_binary.try_exists()? {
        admit_existing(&installed_binary, &hash(&binary))?;
    }
    files.insert("howff".into(), hash(&binary));
    changes.push(Change {
        target: format!("{ROOT}/howff"),
        data: Some(binary),
        mode: 0o755,
    });
    let bytes = serde_json::to_vec(&device)?;
    changes.push(Change {
        target: format!("{ROOT}/device.json"),
        data: Some(bytes),
        mode: 0o600,
    });
    if !layout.file("pair.json")?.exists() {
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
        crate::pairing::ensure_at(&layout.file("pair.json")?)?;
    }
    let record = Record {
        product: "howff".into(),
        schema: 1,
        version: bundle.manifest.version.clone(),
        device,
        assets: bundle.manifest.files.clone(),
        files,
        wrappers,
        entry_sha256: hash(&ENTRY),
        launcher_sha256: hash(&LAUNCHER_ENTRY),
    };
    if let Some((key, metadata)) =
        crate::identity::prepare_at(&layout.path(ROOT)?, &record.device.mac)?
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
        target: LAUNCHER_PATH.into(),
        data: Some(LAUNCHER_ENTRY.to_vec()),
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

pub fn recover_update() -> Result<()> {
    crate::session::while_idle(|| recover_transaction(&Layout::system()))
}
pub fn install_for_update(token: &str) -> Result<Value> {
    install_with_parent(Some(crate::updates::install_permission(token)?))
}
pub fn install() -> Result<Value> {
    install_with_parent(None)
}
fn install_with_parent(update_parent: Option<u32>) -> Result<Value> {
    require(
        unsafe { libc::geteuid() } == 0,
        "Run the installer through sudo or the SD-card launcher",
    )?;
    let facts = crate::compat::detect();
    let device = facts.device()?;
    let bundle = Bundle::embedded()?;
    check_idle_ui(update_parent)?;
    let mut result = crate::session::while_idle(|| {
        install_at(
            &Layout::system(),
            device,
            &bundle,
            &std::env::current_exe()?,
        )
    })?;
    match crate::home::activate() {
        Ok(active) => result["home_shortcut_active"] = active.into(),
        Err(error) => result["home_shortcut_error"] = error.to_string().into(),
    }
    Ok(result)
}

fn uninstall_at(layout: &Layout) -> Result<Value> {
    recover_transaction(layout)?;
    if !layout.file("install.json")?.try_exists()? {
        for name in ["retroarch", "retroarch32", "emulationstation"] {
            let path = layout.path(&wrapper_path(name))?;
            if name == "emulationstation" && !path.try_exists()? {
                continue;
            }
            require(
                digest(&path)?
                    == if name == "emulationstation" {
                        crate::home::ORIGINAL_SHA
                    } else {
                        ORIGINAL_SHA
                    },
                "An unrecorded launcher still requires restoration",
            )?;
        }
        for name in [ENTRY_PATH, LAUNCHER_PATH] {
            require(
                !layout.path(name)?.try_exists()?,
                "An unrecorded Options entry remains",
            )?;
        }
        for name in ALLOWED.iter().copied().chain(std::iter::once("howff")) {
            require(
                !layout.file(name)?.try_exists()?,
                "Unrecorded application files remain",
            )?;
        }
        return Ok(
            json!({"action":"uninstall","installed":false,"launchers_restored":true,"network_changed":false}),
        );
    }
    let record = installed_record(layout)?;
    verify_record(&record, layout)?;
    let mut changes = Vec::new();
    for (name, wrapper) in &record.wrappers {
        let original = fs::read(layout.file(&format!("original-launch/{name}"))?)?;
        require(
            hash(&original)
                == if name == "emulationstation" {
                    crate::home::ORIGINAL_SHA
                } else {
                    ORIGINAL_SHA
                },
            "Original launcher backup differs",
        )?;
        changes.push(Change {
            target: wrapper_path(name),
            data: Some(original),
            mode: wrapper.mode,
        });
    }
    changes.push(Change {
        target: ENTRY_PATH.into(),
        data: None,
        mode: 0o755,
    });
    changes.push(Change {
        target: LAUNCHER_PATH.into(),
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
        json!({"action":"uninstall","launchers_restored":true,"retained":["device.json","pair.json","identity.json","identity.pk8","friends.json","logs/","original-launch/","saves/"],"network_changed":false}),
    )
}
pub fn uninstall() -> Result<Value> {
    crate::session::identity(false)?;
    check_idle_ui(None)?;
    crate::session::while_idle(|| {
        let mut result = uninstall_at(&Layout::system())?;
        match crate::platform::system::release_uninstalled_frontend() {
            Ok(restarted) => result["frontend_restarted"] = restarted.into(),
            Err(error) => result["frontend_restart_error"] = error.to_string().into(),
        }
        Ok(result)
    })
}
pub fn installation_status() -> Result<Value> {
    let layout = Layout::system();
    let record = installed_record(&layout)?;
    verify_record(&record, &layout)?;
    Ok(json!({"files":record.files.len(),"wrappers":record.wrappers.len()}))
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
    manifest.validate_profile()?;
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
            "opt/howff/../else",
            "opt/howff/pair.json/child",
        ] {
            assert!(!allowed_target(path));
        }
        assert!(allowed_target(ENTRY_PATH));
        assert!(allowed_target("opt/howff/assets/8188eu.ko"));
    }

    #[test]
    fn installation_inventory_binds_files_wrappers_and_device_as_one_record() {
        use crate::system::{random_id, write};
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("howff-record-{}", random_id().unwrap()));
        fs::create_dir(&root).unwrap();
        let layout = Layout {
            prefix: root.canonicalize().unwrap(),
        };
        let put = |name: &str, bytes: &[u8], mode| {
            let path = layout.path(name).unwrap();
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            path
        };
        let device = Device {
            schema: 1,
            mac: "02:00:00:00:00:01".into(),
            profile: crate::compat::PROFILE.into(),
            kernel_sha256: crate::compat::KERNEL_SHA.into(),
        };
        put(
            &format!("{ROOT}/device.json"),
            &serde_json::to_vec(&device).unwrap(),
            0o600,
        );
        put(&format!("{ROOT}/howff"), b"runtime", 0o755);
        put(&format!("{ROOT}/howff-gui"), b"gui", 0o755);
        let wrappers = ["retroarch", "retroarch32", "emulationstation"]
            .into_iter()
            .map(|name| {
                let path = put(&wrapper_path(name), b"wrapper", 0o750);
                (name.into(), Wrapper::capture(&path, b"wrapper").unwrap())
            })
            .collect();
        put(ENTRY_PATH, &ENTRY, 0o755);
        put(LAUNCHER_PATH, &LAUNCHER_ENTRY, 0o755);
        let record = Record {
            product: "howff".into(),
            schema: 1,
            version: "0.13.0".into(),
            device,
            assets: vec![Asset {
                name: "howff-gui".into(),
                sha256: hash(b"gui"),
                size: 3,
                offset: 0,
                mode: 0o755,
            }],
            files: BTreeMap::from([
                ("howff".into(), hash(b"runtime")),
                ("howff-gui".into(), hash(b"gui")),
            ]),
            wrappers,
            entry_sha256: hash(&ENTRY),
            launcher_sha256: hash(&LAUNCHER_ENTRY),
        };
        verify_record(&record, &layout).unwrap();
        for fault in 0..6 {
            let mut invalid = record.clone();
            match fault {
                0 => {
                    invalid.files.remove("howff-gui");
                }
                1 => {
                    invalid.wrappers.remove("retroarch");
                }
                2 => {
                    invalid.wrappers.remove("emulationstation");
                }
                3 => {
                    invalid.device.mac = "02:00:00:00:00:02".into();
                }
                4 => {
                    invalid.assets.push(invalid.assets[0].clone());
                }
                5 => {
                    invalid.wrappers.get_mut("retroarch").unwrap().mode = 0o755;
                }
                _ => unreachable!(),
            }
            assert!(verify_record(&invalid, &layout).is_err());
        }
        let record_path = layout.file("install.json").unwrap();
        write(&record_path, &record).unwrap();
        installed_record(&layout).unwrap();
        fs::set_permissions(record_path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(installed_record(&layout).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod path_contract {
    #[test]
    fn generated_entries_and_game_wrapper_use_one_howff_executable() {
        let executable = howff_platform::arkos::application::EXECUTABLE.as_str();
        for (bytes, command) in [
            (&*super::ENTRY, "manual"),
            (&*super::LAUNCHER_ENTRY, "launcher"),
        ] {
            assert_eq!(
                bytes,
                format!("#!/bin/sh\nexec sudo -n {executable} {command}\n").as_bytes()
            );
        }
        let block = std::str::from_utf8(&super::BLOCK).unwrap();
        assert!(block.contains(executable));
        assert!(!block.contains("@HOWFF_BINARY@"));
        assert!(!block.contains("@FRONTEND_DIRECTORY@"));
        assert!(block.contains("howff_launch_frontend"));
    }
}
