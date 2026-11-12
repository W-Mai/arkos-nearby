use howff_platform::arkos;
mod files;
pub mod launch;

use crate::configuration::ResolvedSavePaths;
use crate::{Result, require};
use howff_menu::saves::{Document, Kind, Operation, Record, SourceMatch};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const ROOT: &str = arkos::application::SAVE_BACKUPS.as_str();
const MAX_RECORDS: usize = 4096;

fn time_label(seconds: u64) -> String {
    let seconds = seconds as _;
    let mut date = unsafe { std::mem::zeroed::<libc::tm>() };
    let mut output = [0u8; 32];
    if unsafe { libc::localtime_r(&seconds, &mut date) }.is_null() {
        return String::new();
    }
    let count = unsafe {
        libc::strftime(
            output.as_mut_ptr().cast(),
            output.len(),
            c"%m-%d %H:%M".as_ptr(),
            &date,
        )
    };
    String::from_utf8_lossy(&output[..count]).into_owned()
}

type Thumbnail = (
    String,
    (u32, u32),
    Option<(u64, Option<std::time::SystemTime>)>,
);
fn thumbnail(path: &Path) -> Thumbnail {
    use std::io::Read;
    let result = (|| -> Result<_> {
        files::solo(path)?;
        let meta = fs::metadata(path)?;
        require(meta.len() <= 4 * 1024 * 1024, "截图过大")?;
        let mut header = [0u8; 24];
        std::fs::File::open(path)?.read_exact(&mut header)?;
        require(&header[..8] == b"\x89PNG\r\n\x1a\n", "截图格式无效")?;
        let width = u32::from_be_bytes(header[16..20].try_into()?);
        let height = u32::from_be_bytes(header[20..24].try_into()?);
        require(
            width > 0 && height > 0 && width <= 4096 && height <= 4096,
            "截图尺寸无效",
        )?;
        Ok((
            path.to_string_lossy().into_owned(),
            (width, height),
            Some((meta.len(), meta.modified().ok())),
        ))
    })();
    result.unwrap_or_default()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Identity {
    pub content: String,
    pub core: String,
    pub bits: u8,
    pub name: String,
}
impl Identity {
    pub fn from_context(context: &crate::context::Context) -> Self {
        Self {
            content: context.identity.content.sha256.clone(),
            core: context.core_sha256.clone(),
            bits: context.core_elf_bits,
            name: format!(
                "{} {}",
                context.identity.core.name, context.identity.core.version
            ),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    schema: u8,
    id: String,
    identity: Identity,
    original: Record,
    digest: String,
    thumbnail_digest: Option<String>,
    created: u64,
}

pub struct Store {
    identity: Identity,
    paths: ResolvedSavePaths,
    root: PathBuf,
    title: String,
    records: BTreeMap<String, Record>,
}

fn hash(value: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(value.as_ref()))
}

impl Store {
    pub fn new(
        root: &Path,
        identity: Identity,
        paths: ResolvedSavePaths,
        title: String,
    ) -> Result<Self> {
        require(
            crate::protocol::hex(&identity.content, 64) && crate::protocol::hex(&identity.core, 64),
            "存档来源不可用",
        )?;
        for path in [&paths.sram, &paths.state] {
            files::solo(path)?;
        }
        let root = root.join(&identity.content).join(&identity.core);
        Ok(Self {
            identity,
            paths,
            root,
            title,
            records: BTreeMap::new(),
        })
    }
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
    pub fn paths(&self) -> &ResolvedSavePaths {
        &self.paths
    }
    fn backup_path(&self, id: &str, extension: &str) -> Result<PathBuf> {
        require(crate::protocol::hex(id, 32), "备份身份无效")?;
        Ok(self.root.join(format!("{id}.{extension}")))
    }
    fn record(&self, path: &Path, kind: Kind, slot: Option<i32>, title: String) -> Result<Record> {
        files::solo(path)?;
        let stamp = files::stamp(path)?;
        let key = hash(path.as_os_str().as_encoded_bytes());
        let image = PathBuf::from(format!("{}.png", path.display()));
        let (thumbnail, thumbnail_size, thumbnail_revision) = if kind == Kind::State {
            thumbnail(&image)
        } else {
            Default::default()
        };
        Ok(Record {
            key,
            kind,
            slot,
            title,
            path: path.to_string_lossy().into_owned(),
            bytes: stamp.bytes,
            modified: stamp.modified,
            time_label: time_label(stamp.modified),
            revision: stamp.revision,
            thumbnail,
            thumbnail_size,
            thumbnail_revision,
            source: SourceMatch::Unrecorded,
            core: if kind == Kind::Sram {
                self.identity.name.clone()
            } else {
                "未记录".into()
            },
        })
    }
    pub fn scan(&mut self) -> Result<Document> {
        require(
            !self.root.join("transaction.json").exists(),
            "存档恢复尚未完成",
        )?;
        let mut records = Vec::new();
        if self.paths.sram.is_file() {
            records.push(self.record(&self.paths.sram, Kind::Sram, None, "当前进度".into())?);
        }
        let parent = self.paths.state.parent().ok_or("存档目录缺失")?;
        let stem = self
            .paths
            .state
            .file_name()
            .ok_or("存档名称缺失")?
            .to_string_lossy();
        if parent.is_dir() {
            for entry in fs::read_dir(parent)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let slot = if name == stem {
                    Some(0)
                } else if let Some(suffix) = name.strip_prefix(stem.as_ref()) {
                    if suffix == ".auto" {
                        Some(-1)
                    } else if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                        suffix.parse::<i32>().ok().filter(|v| (0..=999).contains(v))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(slot) = slot
                    && entry.path().is_file()
                {
                    if entry.path() != self.paths.slot_path(slot) {
                        continue;
                    }
                    require(records.len() < MAX_RECORDS, "存档条目过多")?;
                    records.push(self.record(
                        &entry.path(),
                        Kind::State,
                        Some(slot),
                        if slot < 0 {
                            "自动存档".into()
                        } else {
                            format!("槽位 {slot}")
                        },
                    )?);
                }
            }
        }
        if self.root.is_dir() {
            for entry in fs::read_dir(&self.root)? {
                let path = entry?.path();
                if path.extension().is_none_or(|v| v != "json")
                    || path
                        .file_stem()
                        .is_none_or(|v| !crate::protocol::hex(&v.to_string_lossy(), 32))
                {
                    continue;
                }
                let backup: Backup = files::read_manifest(&path)?;
                require(
                    backup.schema == 1 && backup.identity == self.identity,
                    "备份来源已变化",
                )?;
                let data = self.backup_path(&backup.id, "data")?;
                if !data.is_file() {
                    continue;
                }
                let mut record = self.record(
                    &data,
                    Kind::Backup,
                    backup.original.slot,
                    format!("{} 备份", backup.original.title),
                )?;
                record.key = backup.id.clone();
                record.modified = backup.created;
                record.time_label = time_label(backup.created);
                record.source = backup.original.source;
                record.core = backup.original.core;
                if backup.thumbnail_digest.is_some() {
                    (
                        record.thumbnail,
                        record.thumbnail_size,
                        record.thumbnail_revision,
                    ) = thumbnail(&self.backup_path(&backup.id, "png")?);
                }
                require(records.len() < MAX_RECORDS, "存档条目过多")?;
                records.push(record);
            }
        }
        records.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.key.cmp(&b.key)));
        self.records = records.iter().map(|r| (r.key.clone(), r.clone())).collect();
        let revision = hash(serde_json::to_vec(&records)?);
        Ok(Document {
            title: self.title.clone(),
            core: self.identity.name.clone(),
            revision,
            records,
            ..Default::default()
        })
    }
    pub fn selected(&self, key: &str, revision: &str, operation: Operation) -> Result<Record> {
        let record = self.records.get(key).ok_or("存档已变化，请重新读取")?;
        require(record.can(operation), "此存档不支持该操作")?;
        require(
            record.revision == revision
                && files::stamp(Path::new(&record.path))?.revision == revision,
            "存档已变化，请重新读取",
        )?;
        Ok(record.clone())
    }
    fn capture(&self, record: &Record) -> Result<String> {
        let id = crate::system::random_id()?;
        files::directory(&self.root)?;
        let target = self.backup_path(&id, "data")?;
        let digest = files::capture(Path::new(&record.path), &target, &record.revision)?;
        let paired = PathBuf::from(format!("{}.png", record.path));
        let thumbnail_digest = if record.kind == Kind::State && paired.is_file() {
            let path = &paired;
            Some(files::capture(
                path,
                &self.backup_path(&id, "png")?,
                &files::stamp(path)?.revision,
            )?)
        } else {
            None
        };
        let backup = Backup {
            schema: 1,
            id: id.clone(),
            identity: self.identity.clone(),
            original: record.clone(),
            digest,
            thumbnail_digest,
            created: crate::system::wall_time() as u64,
        };
        crate::system::write(&self.backup_path(&id, "json")?, &backup)?;
        Ok(id)
    }
    pub fn backup_progress(&self) -> Result<()> {
        if self.paths.sram.is_file() {
            self.capture(&self.record(&self.paths.sram, Kind::Sram, None, "原进度".into())?)?;
        }
        Ok(())
    }
    pub fn operate(
        &mut self,
        key: &str,
        revision: &str,
        operation: Operation,
    ) -> Result<Option<launch::Intent>> {
        files::directory(self.root.parent().ok_or("备份目录缺失")?)?;
        let _lock = crate::system::Lock::try_acquire(&self.root.with_extension("lock"))?;
        self.recover()?;
        let record = self.selected(key, revision, operation)?;
        match operation {
            Operation::Backup => {
                self.capture(&record)?;
            }
            Operation::Load => {
                self.backup_progress()?;
                return Ok(Some(launch::Intent::State {
                    slot: record.slot.ok_or("存档槽缺失")?,
                    revision: record.revision,
                }));
            }
            Operation::Start => return Ok(Some(launch::Intent::Configured)),
            Operation::Delete => {
                if record.kind != Kind::Backup {
                    self.capture(&record)?;
                }
                require(
                    files::stamp(Path::new(&record.path))?.revision == record.revision,
                    "存档已变化，请重新读取",
                )?;
                if record.kind == Kind::Backup {
                    fs::remove_file(self.backup_path(&record.key, "json")?)?;
                    fs::remove_file(&record.path)?;
                    let paired = self.backup_path(&record.key, "png")?;
                    if paired.is_file() {
                        fs::remove_file(paired)?;
                    }
                } else {
                    fs::remove_file(&record.path)?;
                }
                if record.kind == Kind::State {
                    let paired = PathBuf::from(format!("{}.png", record.path));
                    if paired.is_file() {
                        fs::remove_file(paired)?;
                    }
                }
            }
            Operation::Restore => {
                let backup: Backup = files::read_manifest(&self.backup_path(&record.key, "json")?)?;
                require(
                    backup.identity == self.identity
                        && crate::content::digest(Path::new(&record.path))? == backup.digest,
                    "备份内容已变化",
                )?;
                let target = record
                    .slot
                    .map(|slot| self.paths.slot_path(slot))
                    .unwrap_or_else(|| self.paths.sram.clone());
                let previous = if target.is_file() {
                    Some(self.capture(&self.record(
                        &target,
                        if record.slot.is_some() {
                            Kind::State
                        } else {
                            Kind::Sram
                        },
                        record.slot,
                        "原存档".into(),
                    )?)?)
                } else {
                    None
                };
                let journal = Journal {
                    target: target.clone(),
                    before: previous,
                    source: record.key.clone(),
                    slot: record.slot,
                };
                if let Some(id) = &journal.before {
                    let original: Backup = files::read_manifest(&self.backup_path(id, "json")?)?;
                    require(
                        files::stamp(&target)?.revision == original.original.revision,
                        "目标存档已变化",
                    )?;
                }
                crate::system::write(&self.root.join("transaction.json"), &journal)?;
                let result = self.replace_backup(&journal.source, &target);
                if result.is_err() {
                    self.recover()?;
                    result?;
                }
                fs::remove_file(self.root.join("transaction.json"))?;
                if record.slot.is_none() {
                    return Ok(Some(launch::Intent::Sram));
                }
            }
        }
        Ok(None)
    }
    fn replace_backup(&self, id: &str, target: &Path) -> Result<()> {
        let backup: Backup = files::read_manifest(&self.backup_path(id, "json")?)?;
        require(backup.identity == self.identity, "备份来源不匹配")?;
        files::replace(&self.backup_path(id, "data")?, target, &backup.digest)?;
        let thumbnail = PathBuf::from(format!("{}.png", target.display()));
        if let Some(digest) = backup.thumbnail_digest {
            files::replace(&self.backup_path(id, "png")?, &thumbnail, &digest)?;
        } else if thumbnail.is_file() && backup.original.slot.is_some() {
            fs::remove_file(thumbnail)?;
        }
        Ok(())
    }
    pub fn recover(&self) -> Result<()> {
        let path = self.root.join("transaction.json");
        if !path.is_file() {
            return Ok(());
        }
        let journal: Journal = files::read_manifest(&path)?;
        let target = journal
            .slot
            .map(|slot| self.paths.slot_path(slot))
            .unwrap_or_else(|| self.paths.sram.clone());
        require(journal.target == target, "存档恢复目标已变化")?;
        if target.is_file() {
            let source: Backup = files::read_manifest(&self.backup_path(&journal.source, "json")?)?;
            let before = journal
                .before
                .as_ref()
                .map(|id| files::read_manifest::<Backup>(&self.backup_path(id, "json")?))
                .transpose()?;
            let digest = crate::content::digest(&target)?;
            require(
                digest == source.digest || before.as_ref().is_some_and(|b| b.digest == digest),
                "存档恢复遇到外部修改，请保留备份",
            )?;
        }
        if let Some(id) = journal.before {
            self.replace_backup(&id, &target)?;
        } else if target.is_file() {
            fs::remove_file(target)?;
        }
        fs::remove_file(path)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    target: PathBuf,
    before: Option<String>,
    source: String,
    slot: Option<i32>,
}

#[cfg(test)]
mod tests;
