use super::files::{Graph, Input};
use crate::{Result, require};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
    mode: u32,
    uid: u32,
    gid: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    committed: bool,
    edits: Vec<Edit>,
}

fn bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    Ok(Input::read(path)?.bytes)
}
fn parent(path: &Path) -> Result<&Path> {
    path.parent().ok_or_else(|| "配置路径缺少父目录".into())
}
fn directory(path: &Path, uid: u32, gid: u32) -> Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    directory(parent(path)?, uid, gid)?;
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    chown(path, uid, gid)?;
    File::open(parent(path)?)?.sync_all()?;
    Ok(())
}
fn chown(path: &Path, uid: u32, gid: u32) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    // SAFETY: the path is NUL-terminated and ownership comes from the original file/parent.
    if unsafe { libc::chown(name.as_ptr(), uid, gid) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
fn assign(edit: &Edit, data: &Option<Vec<u8>>) -> Result<()> {
    if let Some(data) = data {
        let temporary = parent(&edit.path)?.join(format!(
            ".nearby-config-{}.tmp",
            crate::system::random_id()?
        ));
        let result = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(data)?;
            chown(&temporary, edit.uid, edit.gid)?;
            file.set_permissions(fs::Permissions::from_mode(edit.mode))?;
            file.sync_all()?;
            fs::rename(&temporary, &edit.path)?;
            File::open(parent(&edit.path)?)?.sync_all()?;
            Ok(())
        })();
        if temporary.exists() {
            let _ = fs::remove_file(temporary);
        }
        result
    } else {
        if edit.path.exists() {
            fs::remove_file(&edit.path)?;
            File::open(parent(&edit.path)?)?.sync_all()?;
        }
        Ok(())
    }
}

fn recover_locked(journal: &Path) -> Result<()> {
    if !journal.exists() {
        return Ok(());
    }
    let meta = journal.symlink_metadata()?;
    require(
        meta.is_file()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.mode() & 0o077 == 0
            && meta.len() <= 16 * 1024 * 1024,
        "配置事务记录无效",
    )?;
    let record: Journal = crate::system::read(journal)?;
    if !record.committed {
        // Validate every target before rolling any target back.
        for edit in &record.edits {
            let current = bytes(&edit.path)?;
            require(
                current == edit.before || current == edit.after,
                "配置恢复遇到外部修改，请保留事务记录",
            )?;
        }
        for edit in record.edits.iter().rev() {
            if bytes(&edit.path)? != edit.before {
                assign(edit, &edit.before)?;
            }
        }
    }
    fs::remove_file(journal)?;
    File::open(parent(journal)?)?.sync_all()?;
    Ok(())
}

pub fn recover(journal: &Path) -> Result<()> {
    if !journal.exists() {
        return Ok(());
    }
    let _lock = crate::system::Lock::try_acquire(&journal.with_extension("lock"))?;
    recover_locked(journal)
}

pub fn commit(
    graph: &Graph,
    changes: Vec<(PathBuf, Option<Vec<u8>>)>,
    journal: &Path,
) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(parent(journal)?)?;
    let _lock = crate::system::Lock::try_acquire(&journal.with_extension("lock"))?;
    recover_locked(journal)?;
    graph.unchanged()?;
    let mut edits = Vec::new();
    for (requested, after) in changes {
        require(
            matches!(
                requested.extension().and_then(|s| s.to_str()),
                Some("cfg" | "opt" | "rmp")
            ),
            "设置写入路径类型不符",
        )?;
        let original = graph.inputs.get(&requested).ok_or("设置目标未记录修订")?;
        if original.bytes == after {
            continue;
        }
        let mut existing = parent(&requested)?;
        while !existing.exists() {
            existing = parent(existing)?;
        }
        let meta = fs::metadata(existing)?;
        directory(parent(&requested)?, meta.uid(), meta.gid())?;
        // Replacing a symlink would break the firmware's shared configuration layout.
        let path = if requested.exists() {
            requested.canonicalize()?
        } else {
            parent(&requested)?
                .canonicalize()?
                .join(requested.file_name().ok_or("设置目标无文件名")?)
        };
        require(
            !edits.iter().any(|edit: &Edit| edit.path == path),
            "设置目标重复指向同一文件",
        )?;
        edits.push(Edit {
            path,
            before: original.bytes.clone(),
            after,
            mode: original.mode,
            uid: if original.bytes.is_some() {
                original.uid
            } else {
                meta.uid()
            },
            gid: if original.bytes.is_some() {
                original.gid
            } else {
                meta.gid()
            },
        });
    }
    if edits.is_empty() {
        return Ok(());
    }
    let mut record = Journal {
        committed: false,
        edits,
    };
    crate::system::write(journal, &record)?;
    let result = (|| -> Result<()> {
        for edit in &record.edits {
            require(
                bytes(&edit.path)? == edit.before,
                "配置文件已变更，请重新读取",
            )?;
            assign(edit, &edit.after)?;
        }
        record.committed = true;
        crate::system::write(journal, &record)?;
        fs::remove_file(journal)?;
        File::open(parent(journal)?)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        recover_locked(journal)?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transactions_preserve_links_metadata_and_detect_external_edits() {
        let root = std::env::temp_dir().join(format!(
            "configuration-{}",
            crate::system::random_id().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        let target = root.join("original.cfg");
        let link = root.join("alias.cfg");
        fs::write(&target, b"key = \"old\"\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut graph = Graph::default();
        graph.observe(&link).unwrap();
        let journal = root.join("transaction.json");
        commit(
            &graph,
            vec![(link.clone(), Some(b"key = \"new\"\n".to_vec()))],
            &journal,
        )
        .unwrap();
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&target).unwrap(), b"key = \"new\"\n");
        assert_eq!(fs::metadata(&target).unwrap().mode() & 0o777, 0o640);
        assert!(commit(&graph, vec![(link, Some(b"bad".to_vec()))], &journal).is_err());
        let input = Input::read(&target).unwrap();
        let edit = Edit {
            path: target.clone(),
            before: input.bytes,
            after: Some(b"pending".to_vec()),
            mode: input.mode,
            uid: input.uid,
            gid: input.gid,
        };
        crate::system::write(
            &journal,
            &Journal {
                committed: false,
                edits: vec![edit.clone()],
            },
        )
        .unwrap();
        assign(&edit, &edit.after).unwrap();
        recover(&journal).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"key = \"new\"\n");
        fs::remove_dir_all(root).unwrap();
    }
}
