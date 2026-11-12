use crate::{Result, require};
use howff_menu::management::{Package, Update};
use howff_platform::arkos;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

const ROOT: &str = arkos::application::UPDATES.as_str();
const UPDATE_PROTOCOL: u8 = 1;
const LIMIT: u64 = 128 * 1024 * 1024;
const EXECUTABLE: &str = "howff/howff";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    product: String,
    schema: u8,
    update_protocol: u8,
    version: String,
    profile: String,
    kernel_sha256: String,
    target: String,
    executable_sha256: String,
}
struct Contents {
    manifest: Manifest,
    executable: Vec<u8>,
}
fn inside(path: &Path, roots: &[PathBuf]) -> Result<PathBuf> {
    let real = path.canonicalize()?;
    require(
        roots.iter().any(|root| real.starts_with(root)),
        "请选择游戏卡内的安装包",
    )?;
    Ok(real)
}
fn roots() -> Vec<PathBuf> {
    arkos::ROM_ROOTS
        .iter()
        .filter_map(|p| Path::new(p).canonicalize().ok())
        .collect()
}
pub fn browse(directory: &str) -> Result<(String, Vec<Package>)> {
    let roots = roots();
    if directory.is_empty() {
        let entries = roots
            .iter()
            .map(|p| Package {
                key: p.to_string_lossy().into(),
                name: if p == Path::new(arkos::ROM_ROOTS[0]) {
                    "游戏卡"
                } else {
                    "第二张游戏卡"
                }
                .into(),
                directory: true,
                size: 0,
            })
            .collect();
        return Ok((String::new(), entries));
    }
    let directory = inside(Path::new(directory), &roots)?;
    let mut entries = Vec::new();
    for item in fs::read_dir(&directory)? {
        let item = item?;
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || inside(&path, &roots).is_err() {
            continue;
        }
        let meta = fs::metadata(&path)?;
        if meta.is_dir() || meta.is_file() && name.ends_with(".tar.gz") {
            entries.push(Package {
                key: path.to_string_lossy().into(),
                name,
                directory: meta.is_dir(),
                size: meta.len(),
            });
            require(
                entries.len() <= 4096,
                "目录内容过多，请把安装包放到单独文件夹",
            )?;
        }
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok((directory.to_string_lossy().into(), entries))
}
fn unpack(path: &Path) -> Result<Contents> {
    require(path.metadata()?.len() <= LIMIT, "安装包超过大小限制")?;
    let mut decoder = flate2::read::GzDecoder::new(File::open(path)?).take(LIMIT + 1);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;
    require(bytes.len() as u64 <= LIMIT, "安装包解压后超过大小限制")?;
    let mut archive = tar::Archive::new(bytes.as_slice());
    let mut names = BTreeSet::new();
    let mut executable = None;
    let mut manifest = None;
    let mut sums = None;
    for item in archive.entries()? {
        let mut item = item?;
        let path = item.path()?.into_owned();
        require(
            path.components().all(|p| matches!(p, Component::Normal(_))),
            "安装包路径无效",
        )?;
        let name = path.to_str().ok_or("安装包文件名无效")?;
        require(
            names.insert(name.to_string()) && item.header().entry_type().is_file(),
            "安装包含重复文件或非普通文件",
        )?;
        require(
            matches!(
                name,
                EXECUTABLE
                    | "howff/manifest.json"
                    | "howff/SHA256SUMS"
                    | "howff/README.md"
                    | "howff/Install Howff.sh"
            ),
            "安装包内容不匹配",
        )?;
        let mut value = Vec::new();
        if name != EXECUTABLE {
            require(item.size() <= 65536, "安装包元信息过大")?;
        }
        item.read_to_end(&mut value)?;
        match name {
            EXECUTABLE => executable = Some(value),
            "howff/manifest.json" => manifest = Some(serde_json::from_slice::<Manifest>(&value)?),
            "howff/SHA256SUMS" => sums = Some(String::from_utf8(value)?),
            _ => {}
        }
    }
    let executable = executable.ok_or("安装包缺少运行程序")?;
    let manifest = manifest.ok_or("安装包缺少版本信息")?;
    let sums = sums.ok_or("安装包缺少文件摘要")?;
    let digest = format!("{:x}", Sha256::digest(&executable));
    require(
        manifest.product == "howff"
            && manifest.schema == 1
            && manifest.update_protocol == UPDATE_PROTOCOL,
        "安装包更新协议不匹配",
    )?;
    require(
        manifest.executable_sha256 == digest
            && sums.lines().any(|line| line == format!("{digest}  howff")),
        "安装包文件摘要不匹配",
    )?;
    require(
        executable.len() >= 64
            && executable[..5] == *b"\x7fELF\x02"
            && u16::from_le_bytes([executable[18], executable[19]]) == 183,
        "安装包架构不匹配",
    )?;
    require(
        !manifest.version.is_empty() && manifest.version.len() <= 64,
        "安装包版本无效",
    )?;
    Ok(Contents {
        manifest,
        executable,
    })
}
fn compatible(manifest: &Manifest) -> bool {
    manifest.profile == crate::compat::PROFILE
        && manifest.kernel_sha256 == crate::compat::KERNEL_SHA
        && manifest.target == "aarch64-unknown-linux-musl"
}
pub fn inspect(path: &str) -> Result<Update> {
    let path = inside(Path::new(path), &roots())?;
    let before = crate::content::digest(&path)?;
    let contents = unpack(&path)?;
    require(
        crate::content::digest(&path)? == before,
        "安装包读取期间发生变化，请重新选择",
    )?;
    let compatible = compatible(&contents.manifest);
    Ok(Update {
        phase: howff_menu::management::UpdatePhase::Ready,
        path: path.to_string_lossy().into(),
        version: contents.manifest.version,
        digest: before,
        compatible,
        reason: if compatible {
            "适配当前设备，校验通过"
        } else {
            "设备配置或内核不匹配"
        }
        .into(),
    })
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Phase {
    Staging,
    Installing,
    Complete,
    Failed,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema: u8,
    pid: u32,
    started: u64,
    boot_id: String,
    foreground: crate::home::ForegroundOrigin,
    executable_sha256: String,
    phase: Phase,
}
impl Owner {
    fn matches_lifetime(&self, boot: &str, started: Option<u64>) -> bool {
        self.schema == UPDATE_PROTOCOL && self.boot_id == boot && started == Some(self.started)
    }
}
fn boot_id() -> Result<String> {
    Ok(fs::read_to_string("/proc/sys/kernel/random/boot_id")?
        .trim()
        .into())
}
fn started(pid: u32) -> Result<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let (_, tail) = stat.rsplit_once(')').ok_or("Missing process identity")?;
    Ok(tail
        .split_whitespace()
        .nth(19)
        .ok_or("Missing process start time")?
        .parse()?)
}
fn stage(token: &str) -> Result<PathBuf> {
    require(crate::protocol::hex(token, 32), "Invalid update identity")?;
    Ok(Path::new(ROOT).join(token))
}
fn owner(token: &str) -> Result<Owner> {
    let path = stage(token)?;
    let meta = fs::symlink_metadata(&path)?;
    require(
        meta.uid() == 0
            && meta.is_dir()
            && !meta.file_type().is_symlink()
            && meta.permissions().mode() & 0o077 == 0,
        "Invalid update directory ownership",
    )?;
    let owner: Owner = crate::system::read(&path.join("owner.json"))?;
    require(
        owner.matches_lifetime(&boot_id()?, started(owner.pid).ok()),
        "Update foreground has ended",
    )?;
    Ok(owner)
}
pub fn install_permission(token: &str) -> Result<u32> {
    let owner = owner(token)?;
    require(
        owner.phase == Phase::Installing && unsafe { libc::getppid() } as u32 == owner.pid,
        "Update installer has no active parent",
    )?;
    let group = fs::read_to_string(format!("/proc/{}/cgroup", owner.pid))?;
    require(
        group
            .lines()
            .any(|line| line.ends_with(&format!("/system.slice/{}", owner.foreground.unit()))),
        "Update parent does not own the foreground unit",
    )?;
    let expected = stage(token)?.join("howff");
    require(
        std::env::current_exe()?.canonicalize()? == expected
            && crate::content::digest(&expected)? == owner.executable_sha256,
        "Update installer identity differs",
    )?;
    require(
        crate::home::foreground_origin()? == owner.foreground,
        "Update foreground lease differs",
    )?;
    Ok(owner.pid)
}
pub fn apply(update: &Update) -> Result<String> {
    crate::session::require_idle()?;
    let path = inside(Path::new(&update.path), &roots())?;
    require(
        crate::content::digest(&path)? == update.digest,
        "安装包已改变，请重新选择",
    )?;
    let contents = unpack(&path)?;
    require(
        compatible(&contents.manifest) && crate::content::digest(&path)? == update.digest,
        "安装包适配或摘要已改变",
    )?;
    let token = crate::system::random_id()?;
    let root = stage(&token)?;
    crate::system::mkdir(&root)?;
    let executable = root.join("howff");
    let mut owner = Owner {
        schema: UPDATE_PROTOCOL,
        pid: std::process::id(),
        started: started(std::process::id())?,
        boot_id: boot_id()?,
        foreground: crate::home::foreground_origin()?,
        executable_sha256: contents.manifest.executable_sha256,
        phase: Phase::Staging,
    };
    crate::system::write(&root.join("owner.json"), &owner)?;
    let result = (|| -> Result<()> {
        crate::system::private_write(&executable, &contents.executable)?;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))?;
        owner.phase = Phase::Installing;
        crate::system::write(&root.join("owner.json"), &owner)?;
        let log = File::create(root.join("install.log"))?;
        let mut child = std::process::Command::new(&executable)
            .args(["update-install", &token])
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if let Some(status) = child.try_wait()? {
                require(status.success(), "安装未完成，已保留原版本，请查看诊断")?;
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err("安装超时，已保留原版本".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        require(
            crate::content::digest(Path::new(crate::session::BINARY))? == owner.executable_sha256,
            "安装后运行程序摘要不匹配",
        )?;
        crate::installer::installation_status()?;
        Ok(())
    })();
    owner.phase = if result.is_ok() {
        Phase::Complete
    } else {
        Phase::Failed
    };
    let recovery = if result.is_err() {
        crate::installer::recover_update()
    } else {
        Ok(())
    };
    crate::system::write(&root.join("owner.json"), &owner)?;
    let log_path = arkos::application::LOGS
        .path()
        .join(format!("update-{token}.log"));
    if root.join("install.log").is_file() {
        fs::copy(root.join("install.log"), log_path)?;
    }
    if let Err(error) = result {
        recovery?;
        fs::remove_dir_all(root)?;
        return Err(error);
    }
    Ok(token)
}
#[cfg(target_os = "linux")]
pub fn restart(token: &str) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let lease = owner(token)?;
    require(
        lease.phase == Phase::Complete && lease.pid == std::process::id(),
        "Update result is not owned by this Launcher",
    )?;
    let error = std::process::Command::new(crate::session::BINARY)
        .args(["launcher-continue", token])
        .exec();
    Err(error.into())
}
#[cfg(not(target_os = "linux"))]
pub fn restart(_: &str) -> Result<()> {
    Err("Updates require Linux".into())
}
pub fn consume(token: &str) -> Result<()> {
    let lease = owner(token)?;
    require(
        lease.phase == Phase::Complete
            && lease.pid == std::process::id()
            && crate::content::digest(&std::env::current_exe()?)? == lease.executable_sha256,
        "Update continuation differs",
    )?;
    fs::remove_dir_all(stage(token)?)?;
    Ok(())
}
pub fn recover_orphaned() -> Result<bool> {
    let path = Path::new(ROOT);
    if !path.try_exists()? {
        return Ok(false);
    }
    let mut recovered = false;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let token = entry.file_name().to_string_lossy().into_owned();
        if !crate::protocol::hex(&token, 32)
            || entry.file_type()?.is_symlink()
            || !entry.file_type()?.is_dir()
        {
            continue;
        }
        if !entry.path().join("owner.json").is_file() {
            continue;
        }
        let lease: Owner = crate::system::read(&entry.path().join("owner.json"))?;
        if lease.matches_lifetime(&boot_id()?, started(lease.pid).ok()) {
            continue;
        }
        crate::installer::recover_update()?;
        fs::remove_dir_all(entry.path())?;
        recovered = true;
    }
    Ok(recovered)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path, name: &str, version: &str, product: Option<&str>) -> PathBuf {
        let mut binary = vec![0; 64];
        binary[..5].copy_from_slice(b"\x7fELF\x02");
        binary[18..20].copy_from_slice(&183u16.to_le_bytes());
        let digest = format!("{:x}", Sha256::digest(&binary));
        let manifest = Manifest {
            product: "howff".into(),
            schema: 1,
            update_protocol: UPDATE_PROTOCOL,
            version: version.into(),
            profile: crate::compat::PROFILE.into(),
            kernel_sha256: crate::compat::KERNEL_SHA.into(),
            target: "aarch64-unknown-linux-musl".into(),
            executable_sha256: digest.clone(),
        };
        let mut manifest = serde_json::to_value(manifest).unwrap();
        if let Some(product) = product {
            manifest["product"] = product.into();
        } else {
            manifest.as_object_mut().unwrap().remove("product");
        }
        let path = root.join(name);
        let encoder = flate2::write::GzEncoder::new(
            File::create(&path).unwrap(),
            flate2::Compression::fast(),
        );
        let mut tar = tar::Builder::new(encoder);
        for (name, bytes) in [
            (EXECUTABLE, binary),
            (
                "howff/manifest.json",
                serde_json::to_vec(&manifest).unwrap(),
            ),
            (
                "howff/SHA256SUMS",
                format!("{digest}  howff\n").into_bytes(),
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, bytes.as_slice())
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
        path
    }
    #[test]
    fn persistent_update_owner_requires_the_same_boot_and_process_lifetime() {
        let lease = Owner {
            schema: UPDATE_PROTOCOL,
            pid: 123,
            started: 42,
            boot_id: "boot-a".into(),
            foreground: crate::home::ForegroundOrigin::OriginalWrapper,
            executable_sha256: "a".repeat(64),
            phase: Phase::Staging,
        };
        assert!(lease.matches_lifetime("boot-a", Some(42)));
        assert!(!lease.matches_lifetime("boot-b", Some(42)));
        assert!(!lease.matches_lifetime("boot-a", Some(43)));
        assert!(!lease.matches_lifetime("boot-a", None));
        assert!(Path::new(ROOT).starts_with(crate::context::INSTALL));
    }
    #[test]
    fn package_admission_reads_metadata_without_executing_and_keeps_real_card_boundaries() {
        let root = std::env::temp_dir().join(crate::system::random_id().unwrap());
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = fixture(&root, "update.tar.gz", "0.11.0", Some("howff"));
        let contents = unpack(&path).unwrap();
        assert!(compatible(&contents.manifest));
        assert_eq!(contents.manifest.version, "0.11.0");
        assert!(inside(&path, std::slice::from_ref(&root)).is_ok());
        let escape = root.join("escape");
        std::os::unix::fs::symlink("/", &escape).unwrap();
        assert!(inside(&escape, std::slice::from_ref(&root)).is_err());
        for product in [None, Some("external")] {
            let rejected = fixture(&root, "invalid.tar.gz", "0.11.0", product);
            assert!(unpack(&rejected).is_err());
        }
        let mut bytes = fs::read(&path).unwrap();
        bytes.truncate(bytes.len() / 2);
        fs::write(&path, bytes).unwrap();
        assert!(unpack(&path).is_err());
        assert!(stage("../else").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
