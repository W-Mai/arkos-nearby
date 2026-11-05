use crate::{Result, require};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

pub(super) struct Stamp {
    pub bytes: u64,
    pub modified: u64,
    pub revision: String,
}
pub(super) fn solo(path: &Path) -> Result<()> {
    let mut actual = path.to_path_buf();
    while !actual.exists() {
        actual = actual.parent().ok_or("存档位置不可用")?.into();
    }
    let actual = actual.canonicalize()?;
    require(
        !path.starts_with(crate::session::BASE) && !actual.starts_with(crate::session::BASE),
        "联机会话存档属于独立来源",
    )
}
pub(super) fn stamp(path: &Path) -> Result<Stamp> {
    solo(path)?;
    let meta = fs::metadata(path)?;
    require(meta.is_file(), "存档文件大小或类型无效")?;
    let bytes = serde_json::to_vec(&(
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
        meta.mode(),
        meta.uid(),
        meta.gid(),
    ))?;
    Ok(Stamp {
        bytes: meta.len(),
        modified: meta.mtime().max(0) as u64,
        revision: format!("{:x}", Sha256::digest(bytes)),
    })
}
pub(super) fn directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
pub(super) fn read_manifest<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let meta = fs::symlink_metadata(path)?;
    require(
        meta.is_file()
            && meta.len() < 256 * 1024
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.mode() & 0o077 == 0,
        "存档记录不可用",
    )?;
    crate::system::read(path)
}
pub(super) fn capture(source: &Path, target: &Path, revision: &str) -> Result<String> {
    require(
        stamp(source)?.bytes <= 256 * 1024 * 1024,
        "存档超过备份大小限制",
    )?;
    require(
        stamp(source)?.revision == revision,
        "存档已变化，请重新读取",
    )?;
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)?;
    let mut hash = Sha256::new();
    let mut block = [0u8; 65536];
    loop {
        let count = input.read(&mut block)?;
        if count == 0 {
            break;
        }
        hash.update(&block[..count]);
        output.write_all(&block[..count])?;
    }
    output.sync_all()?;
    if stamp(source)?.revision != revision {
        fs::remove_file(target)?;
        return Err("存档已变化，请重新读取".into());
    }
    File::open(target.parent().ok_or("备份目录缺失")?)?.sync_all()?;
    Ok(format!("{:x}", hash.finalize()))
}
pub(super) fn replace(source: &Path, target: &Path, digest: &str) -> Result<()> {
    solo(target)?;
    require(
        !target
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink() && !target.exists()),
        "存档链接目标当前不可用",
    )?;
    require(crate::content::digest(source)? == digest, "备份内容已变化")?;
    let original_target = target.to_path_buf();
    let expected = if target.exists() {
        Some(stamp(target)?.revision)
    } else {
        None
    };
    let target = if target.exists() {
        target.canonicalize()?
    } else {
        target.into()
    };
    let parent = target.parent().ok_or("目标目录缺失")?;
    fs::create_dir_all(parent)?;
    let meta = if target.exists() {
        fs::metadata(&target)?
    } else {
        fs::metadata(parent)?
    };
    let mode = if target.exists() {
        meta.mode() & 0o777
    } else {
        0o644
    };
    let temporary = parent.join(format!(".nearby-save-{}", crate::system::random_id()?));
    let result = (|| -> Result<()> {
        require(
            capture(source, &temporary, &stamp(source)?.revision)? == digest,
            "备份内容已变化",
        )?;
        let file = File::options().write(true).open(&temporary)?;
        let name = std::ffi::CString::new(temporary.as_os_str().as_encoded_bytes())?;
        require(
            unsafe { libc::chown(name.as_ptr(), meta.uid(), meta.gid()) } == 0,
            "存档属主恢复失败",
        )?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        require(
            if let Some(expected) = &expected {
                original_target.canonicalize()? == target
                    && stamp(&original_target)?.revision == *expected
            } else {
                !original_target.exists()
            },
            "目标存档已变化",
        )?;
        fs::rename(&temporary, &target)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}
