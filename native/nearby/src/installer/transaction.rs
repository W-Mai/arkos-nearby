use super::{ROOT, allowed_target, hash};
use crate::system::{mkdir, private_write, random_id, read};
use crate::{Result, require};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

const JOURNAL: &str = "install-journal.json";
const MAX_CHANGES: usize = crate::bundle::ALLOWED.len() + 16;

#[derive(Clone)]
pub(super) struct Layout {
    pub(super) prefix: PathBuf,
}
impl Layout {
    pub(super) fn system() -> Self {
        Self { prefix: "/".into() }
    }
    pub(super) fn path(&self, name: &str) -> Result<PathBuf> {
        let relative = Path::new(name);
        require(
            !name.is_empty()
                && relative
                    .components()
                    .all(|p| matches!(p, Component::Normal(_))),
            "Invalid installation path",
        )?;
        let mut path = self.prefix.clone();
        let root = fs::symlink_metadata(&path)?;
        require(root.is_dir(), "Installation root is not a directory")?;
        let mut components = relative.components().peekable();
        while let Some(component) = components.next() {
            path.push(component);
            match fs::symlink_metadata(&path) {
                Ok(meta) => require(
                    if components.peek().is_some() {
                        meta.is_dir()
                    } else {
                        meta.is_dir() || meta.is_file()
                    },
                    "Installation path contains a link or unrelated file type",
                )?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(path)
    }
    pub(super) fn file(&self, name: &str) -> Result<PathBuf> {
        self.path(&format!("{ROOT}/{name}"))
    }
}

pub(super) struct Change {
    pub(super) target: String,
    pub(super) data: Option<Vec<u8>>,
    pub(super) mode: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Image {
    sha256: String,
    mode: u32,
    uid: u32,
    gid: u32,
}
impl Image {
    fn capture(path: &Path) -> Result<(Option<Self>, Vec<u8>)> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((None, Vec::new()));
            }
            Err(error) => return Err(error.into()),
        };
        require(metadata.is_file(), "Install target is not a regular file")?;
        let bytes = fs::read(path)?;
        Ok((
            Some(Self {
                sha256: hash(&bytes),
                mode: metadata.mode() & 0o7777,
                uid: metadata.uid(),
                gid: metadata.gid(),
            }),
            bytes,
        ))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Undo {
    target: String,
    backup: String,
    before: Option<Image>,
    after: Option<Image>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Phase {
    Prepared,
    Committed,
    Restored,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u8,
    directory: String,
    phase: Phase,
    undo: Vec<Undo>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    BackedUp(usize),
    Published,
    Applied(usize),
    ReplacementCreated(usize),
    ReplacementWritten(usize),
    Replaced(usize),
    Committed,
    Restored(usize),
}

fn sync(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}
fn directory(path: &Path) -> Result<()> {
    if path.try_exists()? {
        require(path.is_dir(), "Install parent is not a directory")?;
        return Ok(());
    }
    let parent = path.parent().ok_or("Install directory has no parent")?;
    directory(parent)?;
    fs::create_dir(path)?;
    sync(parent)
}
fn replacement(layout: &Layout, journal: &Journal, index: usize) -> Result<PathBuf> {
    let path = layout.path(&journal.undo[index].target)?;
    Ok(path
        .parent()
        .ok_or("Install target has no parent")?
        .join(format!(
            ".howff-{}-{index}.install",
            journal
                .directory
                .strip_prefix(".transaction-")
                .ok_or("Invalid install stage")?
        )))
}
fn validate_replacement(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => require(
            meta.is_file() && meta.uid() == unsafe { libc::geteuid() },
            "Installation replacement has unrelated ownership",
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
fn assign(
    path: &Path,
    temporary: &Path,
    bytes: &[u8],
    image: &Image,
    index: usize,
    observe: &mut impl FnMut(Step) -> Result<()>,
) -> Result<()> {
    let parent = path.parent().ok_or("Install target has no parent")?;
    directory(parent)?;
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(temporary)?;
        observe(Step::ReplacementCreated(index))?;
        file.write_all(bytes)?;
        observe(Step::ReplacementWritten(index))?;
        // SAFETY: the descriptor is owned here and IDs come from the captured target.
        if unsafe { libc::fchown(file.as_raw_fd(), image.uid, image.gid) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        file.set_permissions(fs::Permissions::from_mode(image.mode))?;
        file.sync_all()?;
        fs::rename(temporary, path)?;
        observe(Step::Replaced(index))?;
        sync(parent)
    })();
    if temporary.exists() {
        fs::remove_file(temporary)?;
    }
    result
}
fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => sync(path.parent().ok_or("Install target has no parent")?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
fn cleanup(layout: &Layout, journal: &Journal) -> Result<()> {
    for index in 0..journal.undo.len() {
        let path = replacement(layout, journal, index)?;
        validate_replacement(&path)?;
        remove(&path)?;
    }
    let directory = layout.file(&journal.directory)?;
    if directory.try_exists()? {
        fs::remove_dir_all(directory)?;
        sync(&layout.path(ROOT)?)?;
    }
    remove(&layout.file(JOURNAL)?)
}
fn publish(layout: &Layout, journal: &Journal) -> Result<()> {
    let temporary = layout.file(&format!("{}/journal.next", journal.directory))?;
    private_write(&temporary, &serde_json::to_vec(journal)?)?;
    fs::rename(temporary, layout.file(JOURNAL)?)?;
    sync(&layout.path(ROOT)?)
}
fn valid_directory(name: &str) -> bool {
    name.strip_prefix(".transaction-")
        .is_some_and(|suffix| crate::protocol::hex(suffix, 32))
}
fn clean_unpublished(layout: &Layout) -> Result<()> {
    let root = layout.path(ROOT)?;
    if !root.try_exists()? {
        return Ok(());
    }
    for item in fs::read_dir(&root)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        if !valid_directory(&name) {
            continue;
        }
        let path = layout.file(&name)?;
        let meta = fs::symlink_metadata(&path)?;
        require(
            meta.is_dir() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o077 == 0,
            "Unpublished installation stage has unrelated ownership",
        )?;
        for backup in fs::read_dir(&path)? {
            let backup = backup?;
            let name = backup.file_name();
            let text = name.to_string_lossy();
            let meta = backup.metadata()?;
            require(
                (text == "journal.next" || text.parse::<usize>().is_ok_and(|i| i < MAX_CHANGES))
                    && !backup.file_type()?.is_symlink()
                    && meta.is_file()
                    && meta.uid() == unsafe { libc::geteuid() }
                    && meta.mode() & 0o077 == 0,
                "Unpublished installation stage contains unrelated files",
            )?;
        }
        fs::remove_dir_all(path)?;
        sync(&root)?;
    }
    Ok(())
}
pub(super) fn recover_transaction(layout: &Layout) -> Result<()> {
    recover_observed(layout, &mut |_| Ok(()))
}
fn recover_observed(layout: &Layout, observe: &mut impl FnMut(Step) -> Result<()>) -> Result<()> {
    let file = layout.file(JOURNAL)?;
    if !file.try_exists()? {
        return clean_unpublished(layout);
    }
    let meta = fs::symlink_metadata(&file)?;
    require(
        meta.is_file()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.mode() & 0o077 == 0
            && meta.len() <= 1024 * 1024,
        "Invalid installation journal ownership",
    )?;
    let mut journal: Journal = read(&file)?;
    require(
        journal.schema == 1
            && valid_directory(&journal.directory)
            && !journal.undo.is_empty()
            && journal.undo.len() <= MAX_CHANGES,
        "Invalid installation transaction",
    )?;
    let mut targets = BTreeSet::new();
    let mut backups = Vec::new();
    for (index, undo) in journal.undo.iter().enumerate() {
        require(
            allowed_target(&undo.target)
                && targets.insert(&undo.target)
                && undo.backup == index.to_string()
                && [&undo.before, &undo.after].iter().all(|image| {
                    image.as_ref().is_none_or(|image| {
                        crate::protocol::hex(&image.sha256, 64) && image.mode & !0o7777 == 0
                    })
                }),
            "Invalid installation rollback entry",
        )?;
        if journal.phase != Phase::Prepared {
            continue;
        }
        let target = layout.path(&undo.target)?;
        validate_replacement(&replacement(layout, &journal, index)?)?;
        let current = Image::capture(&target)?.0;
        require(
            current == undo.before || current == undo.after,
            "Installation recovery found an external modification",
        )?;
        let bytes = if let Some(before) = &undo.before {
            let backup = layout.file(&format!("{}/{}", journal.directory, undo.backup))?;
            let (image, bytes) = Image::capture(&backup)?;
            require(
                image.is_some_and(|image| {
                    image.uid == unsafe { libc::geteuid() } && image.mode & 0o077 == 0
                }) && hash(&bytes) == before.sha256,
                "Installation rollback checksum differs",
            )?;
            bytes
        } else {
            Vec::new()
        };
        backups.push((target, bytes));
    }
    if journal.phase == Phase::Prepared {
        for (index, (undo, (target, bytes))) in journal.undo.iter().zip(backups).enumerate().rev() {
            let temporary = replacement(layout, &journal, index)?;
            remove(&temporary)?;
            if Image::capture(&target)?.0 != undo.before {
                if let Some(before) = &undo.before {
                    assign(&target, &temporary, &bytes, before, index, observe)?;
                } else {
                    remove(&target)?;
                }
            }
            observe(Step::Restored(index))?;
        }
        journal.phase = Phase::Restored;
        publish(layout, &journal)?;
    }
    cleanup(layout, &journal)?;
    clean_unpublished(layout)
}

pub(super) fn transact(layout: &Layout, changes: Vec<Change>) -> Result<()> {
    transact_observed(layout, changes, &mut |_| Ok(()))
}
pub(super) fn transact_observed(
    layout: &Layout,
    changes: Vec<Change>,
    observe: &mut impl FnMut(Step) -> Result<()>,
) -> Result<()> {
    recover_transaction(layout)?;
    require(
        changes.len() <= MAX_CHANGES,
        "Installation plan is oversized",
    )?;
    if changes.is_empty() {
        return Ok(());
    }
    let mut targets = BTreeSet::new();
    let mut undo = Vec::new();
    let mut backups = Vec::new();
    for (index, change) in changes.iter().enumerate() {
        require(
            allowed_target(&change.target)
                && targets.insert(&change.target)
                && change.mode & !0o7777 == 0,
            "Invalid or repeated installation target",
        )?;
        let path = layout.path(&change.target)?;
        let (before, bytes) = Image::capture(&path)?;
        let after = change.data.as_ref().map(|data| Image {
            sha256: hash(data),
            mode: change.mode,
            uid: before
                .as_ref()
                .map_or_else(|| unsafe { libc::geteuid() }, |image| image.uid),
            gid: before
                .as_ref()
                .map_or_else(|| unsafe { libc::getegid() }, |image| image.gid),
        });
        undo.push(Undo {
            target: change.target.clone(),
            backup: index.to_string(),
            before,
            after,
        });
        backups.push(bytes);
    }
    directory(&layout.path(ROOT)?)?;
    let mut journal = Journal {
        schema: 1,
        directory: format!(".transaction-{}", random_id()?),
        phase: Phase::Prepared,
        undo,
    };
    let directory = layout.file(&journal.directory)?;
    mkdir(&directory)?;
    let file = layout.file(JOURNAL)?;
    let prepared = (|| -> Result<()> {
        for (index, (undo, bytes)) in journal.undo.iter().zip(backups).enumerate() {
            if undo.before.is_some() {
                private_write(&directory.join(&undo.backup), &bytes)?;
            }
            observe(Step::BackedUp(index))?;
        }
        sync(&directory)?;
        publish(layout, &journal)
    })();
    if let Err(error) = prepared {
        if !file.try_exists()? {
            fs::remove_dir_all(directory)?;
            sync(&layout.path(ROOT)?)?;
        }
        return Err(error);
    }
    let result = (|| -> Result<()> {
        observe(Step::Published)?;
        for (index, (change, undo)) in changes.into_iter().zip(&journal.undo).enumerate() {
            let path = layout.path(&change.target)?;
            require(
                Image::capture(&path)?.0 == undo.before,
                "Install target changed during preparation",
            )?;
            if let (Some(bytes), Some(after)) = (change.data, &undo.after) {
                assign(
                    &path,
                    &replacement(layout, &journal, index)?,
                    &bytes,
                    after,
                    index,
                    observe,
                )?;
            } else {
                remove(&path)?;
            }
            observe(Step::Applied(index))?;
        }
        journal.phase = Phase::Committed;
        publish(layout, &journal)?;
        observe(Step::Committed)?;
        cleanup(layout, &journal)
    })();
    if result.is_err() {
        let committed = if file.try_exists()? {
            read::<Journal>(&file)?.phase == Phase::Committed
        } else {
            journal.phase == Phase::Committed
        };
        recover_transaction(layout)?;
        if committed {
            for undo in &journal.undo {
                require(
                    Image::capture(&layout.path(&undo.target)?)?.0 == undo.after,
                    "Committed installation changed before cleanup completed",
                )?;
            }
            return Ok(());
        }
    }
    result
}

#[cfg(test)]
mod tests;
