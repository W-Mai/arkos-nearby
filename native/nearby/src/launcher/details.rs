use crate::catalog::Entry;
use crate::{context, core_choice, owned_core};
use howff_menu::details::{Document, Fact, FileStatus, Load, Requirement, Row};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::SystemTime;

type Reply = (u64, Document);
type Request = (u64, Query);

enum Query {
    Game(Entry),
    Storage,
}

#[derive(Default)]
pub(super) struct Service {
    epoch: u64,
    pending: Option<Request>,
    channels: Option<(SyncSender<Request>, Receiver<Reply>)>,
    pub load: Load,
}

impl Service {
    pub fn open(&mut self, entry: Entry) {
        self.query(Query::Game(entry));
    }

    pub fn storage(&mut self) {
        self.query(Query::Storage);
    }

    fn query(&mut self, query: Query) {
        self.cancel();
        self.load = Load::Loading;
        if self.channels.is_none() {
            let (requests, incoming) = mpsc::sync_channel::<Request>(1);
            let (outgoing, replies) = mpsc::sync_channel(1);
            match std::thread::Builder::new()
                .name("game-details".into())
                .spawn(move || {
                    let mut cache = MetadataCache::default();
                    while let Ok((epoch, query)) = incoming.recv() {
                        let document = match query {
                            Query::Game(entry) => read(&entry, &mut cache),
                            Query::Storage => super::storage::read(),
                        };
                        if outgoing.send((epoch, document)).is_err() {
                            break;
                        }
                    }
                }) {
                Ok(_) => self.channels = Some((requests, replies)),
                Err(error) => {
                    self.load = Load::Failed {
                        message: error.to_string(),
                    };
                    return;
                }
            }
        }
        self.pending = Some((self.epoch, query));
        self.poll();
    }

    pub fn cancel(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = None;
    }

    pub fn poll(&mut self) {
        let Some((requests, replies)) = &self.channels else {
            return;
        };
        loop {
            match replies.try_recv() {
                Ok((epoch, document)) if epoch == self.epoch => {
                    self.load = Load::Ready { document }
                }
                Ok(_) => {}
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.load = Load::Failed {
                        message: "详情读取任务已结束".into(),
                    };
                    self.pending = None;
                    self.channels = None;
                    return;
                }
            }
        }
        if let Some(request) = self.pending.take() {
            match requests.try_send(request) {
                Ok(()) => {}
                Err(TrySendError::Full(request)) => self.pending = Some(request),
                Err(TrySendError::Disconnected(_)) => {
                    self.load = Load::Failed {
                        message: "详情读取任务已结束".into(),
                    };
                }
            }
        }
    }
}

// Only parsed, revision-matched metadata is reused. File presence is checked on each open.
#[derive(Default)]
struct MetadataCache(VecDeque<CachedMetadata>);
struct CachedMetadata {
    path: PathBuf,
    revision: (u64, SystemTime),
    values: Arc<HashMap<String, String>>,
}
impl MetadataCache {
    fn flags(&mut self, path: &Path) -> Option<Arc<HashMap<String, String>>> {
        let metadata = fs::metadata(path).ok()?;
        if !metadata.is_file() || metadata.len() > 512 * 1024 {
            return None;
        }
        let revision = (metadata.len(), metadata.modified().ok()?);
        if let Some(index) = self
            .0
            .iter()
            .position(|entry| entry.path == path && entry.revision == revision)
        {
            let entry = self.0.remove(index)?;
            let result = entry.values.clone();
            self.0.push_back(entry);
            return Some(result);
        }
        self.0.retain(|entry| entry.path != path);
        let values = Arc::new(context::flags(path).ok()?);
        if self.0.len() == 8 {
            self.0.pop_front();
        }
        self.0.push_back(CachedMetadata {
            path: path.into(),
            revision,
            values: values.clone(),
        });
        Some(values)
    }
}

fn file_status(path: &Path) -> FileStatus {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => FileStatus::Found,
        Ok(_) => FileStatus::Unknown,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => FileStatus::Missing,
        Err(_) => FileStatus::Unknown,
    }
}

fn row(
    name: impl Into<String>,
    summary: impl Into<String>,
    source: impl Into<String>,
    path: &Path,
) -> Row {
    let source = source.into();
    let mut facts = Vec::new();
    if !path.as_os_str().is_empty() {
        facts.push(Fact::new("路径", path.to_string_lossy()));
    }
    if !source.is_empty() && source != path.to_string_lossy() {
        facts.push(Fact::new("来源", source));
    }
    Row {
        name: name.into(),
        summary: summary.into(),
        facts,
        path: path.to_string_lossy().into(),
        ..Row::default()
    }
}

fn firmware_path(system: &Path, value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    (!value.is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir)))
    .then(|| system.join(path))
}

fn firmware(
    info: &HashMap<String, String>,
    source: &Path,
    system: Option<&Path>,
    name: &str,
) -> Vec<Row> {
    let mut result = Vec::new();
    for index in 0..64 {
        let Some(filename) = info.get(&format!("firmware{index}_path")) else {
            continue;
        };
        let requirement = match info
            .get(&format!("firmware{index}_opt"))
            .map(String::as_str)
        {
            Some("true") => Requirement::Optional,
            Some("false") => Requirement::Required,
            _ => Requirement::Unspecified,
        };
        let path = system.and_then(|system| firmware_path(system, filename));
        let status = path.as_deref().map(file_status).unwrap_or_default();
        let presence = match status {
            FileStatus::Found => "已找到文件",
            FileStatus::Missing => "未找到文件",
            FileStatus::Unknown => "位置待确认",
        };
        let declaration = match requirement {
            Requirement::Required => "必需",
            Requirement::Optional => "可选",
            Requirement::Unspecified => "未声明",
        };
        let mut item = row(
            filename,
            presence,
            "",
            path.as_deref().unwrap_or_else(|| Path::new("")),
        );
        item.facts.extend([
            Fact::new("要求", declaration),
            Fact::new("核心", name),
            Fact::new("声明文件", source.to_string_lossy()),
        ]);
        if let Some(description) = info
            .get(&format!("firmware{index}_desc"))
            .filter(|text| !text.is_empty())
        {
            item.facts.push(Fact::new("说明", description));
        }
        item.status = status;
        item.requirement = Some(requirement);
        result.push(item);
    }
    result
}

fn saves(frontend: &str, cache: &mut MetadataCache) -> Vec<Row> {
    let Ok(files) = crate::game::configuration_sources(frontend) else {
        return vec![row(
            "启动配置",
            "由原系统启动脚本提供",
            "原系统启动脚本",
            Path::new(""),
        )];
    };
    let path = files.base;
    let values = cache.flags(&path);
    let mut rows = vec![row(
        "单机配置",
        if values.is_some() {
            "原前端配置"
        } else {
            "读取失败"
        },
        frontend,
        &path,
    )];
    for (name, key) in [
        ("SRAM 目录", "savefile_directory"),
        ("即时存档目录", "savestate_directory"),
    ] {
        let value = values.as_ref().and_then(|values| values.get(key));
        let directory = value.filter(|value| !value.is_empty() && value.as_str() != "default");
        rows.push(row(
            name,
            if directory.is_some() {
                "配置目录"
            } else if values.is_none() {
                "读取失败"
            } else {
                "前端默认目录"
            },
            path.to_string_lossy(),
            Path::new(directory.map_or("", String::as_str)),
        ));
    }
    rows.push(row(
        "核心选项",
        "原前端选项文件",
        "原前端核心选项",
        &files.options,
    ));
    rows.push(row(
        "联机存档",
        "创建会话时分配私有目录",
        "联机会话目录",
        Path::new(crate::session::BASE),
    ));
    rows
}

fn read(entry: &Entry, cache: &mut MetadataCache) -> Document {
    let mut document = Document {
        key: entry.key(),
        title: entry.name().into(),
        platform: entry.platform_name().into(),
        ..Document::default()
    };
    let policy = core_choice::core_order(entry.path());
    let mut choices: Vec<(&str, &str)> = policy
        .iter()
        .map(|choice| (choice.frontend, choice.id))
        .collect();
    if !entry.core().is_empty() && !choices.contains(&(entry.frontend(), entry.core())) {
        choices.push((entry.frontend(), entry.core()));
    }
    let arcade = !core_choice::arcade_order(entry.path()).is_empty();
    document.core.push(row(
        if choices.is_empty() {
            "启动方式"
        } else {
            "选择规则"
        },
        if choices.is_empty() {
            "由原系统启动脚本提供"
        } else if arcade {
            "按 ROM 内容核验后选择"
        } else {
            "按共享顺序核验可用核心"
        },
        if choices.is_empty() {
            "游戏目录条目"
        } else {
            "共享核心规则"
        },
        entry.path(),
    ));
    for (frontend, id) in choices {
        let Ok(path) = context::resolve_core(frontend, id) else {
            continue;
        };
        let status = file_status(&path);
        let owned = owned_core::entry(id);
        let optimized = status == FileStatus::Found
            && owned.is_some_and(|owned| {
                crate::content::digest(&path).is_ok_and(|hash| hash == owned.sha256)
            });
        let summary = match (status, owned) {
            (FileStatus::Found, Some(_)) if optimized => "工具优化",
            (FileStatus::Found, Some(_)) => "构建摘要异常",
            (FileStatus::Found, None) => "设备原有",
            (FileStatus::Missing, _) => "未安装",
            (FileStatus::Unknown, _) => "读取状态未知",
        };
        let mut core = row(id, summary, "", &path);
        core.facts.push(Fact::new("前端", frontend));
        if optimized {
            if let Some(owned) = owned {
                core.facts.extend([
                    Fact::new("核心", owned.name),
                    Fact::new("版本", owned.version),
                ]);
            }
        }
        core.status = status;
        document.core.push(core);
        if status != FileStatus::Found {
            continue;
        }
        let Ok(config) = context::config(frontend) else {
            continue;
        };
        let info_path = config.join("cores").join(owned.map_or_else(
            || format!("{id}_libretro.info"),
            |entry| entry.info_filename.into(),
        ));
        let base = cache.flags(&config.join("retroarch.cfg"));
        let system = base
            .as_ref()
            .and_then(|values| values.get("system_directory"))
            .filter(|value| Path::new(value).is_absolute())
            .map(PathBuf::from);
        if let Some(info) = cache.flags(&info_path) {
            let rows = firmware(&info, &info_path, system.as_deref(), id);
            if rows.is_empty() {
                document.firmware.push(row(
                    id,
                    "固件列表为空",
                    info_path.to_string_lossy(),
                    &info_path,
                ));
            } else {
                document.firmware.extend(rows);
            }
        } else {
            document
                .firmware
                .push(row(id, "暂无核心信息", "核心 info 文件", &info_path));
        }
    }
    if document.firmware.is_empty() {
        document.firmware.push(row(
            "BIOS 文件",
            "暂无核心信息",
            "核心 info 文件",
            Path::new(""),
        ));
    }
    let mut frontends: Vec<&str> = policy.iter().map(|choice| choice.frontend).collect();
    frontends.push(entry.frontend());
    frontends.sort_unstable();
    frontends.dedup();
    let multiple_frontends = frontends.len() > 1;
    for frontend in frontends {
        let mut rows = saves(frontend, cache);
        if multiple_frontends {
            for row in &mut rows {
                row.name = format!("{} ({frontend})", row.name);
            }
        }
        document.saves.extend(rows);
    }
    if let Ok(session) = crate::session::state() {
        if !session.closed() {
            if let Ok(context) =
                crate::system::read::<context::Context>(&session.root().join("context.json"))
            {
                if context.game_path == entry.path() {
                    let mut current = row(
                        "当前会话核心",
                        &context.identity.core.name,
                        "当前联机会话",
                        &context.core_path,
                    );
                    current
                        .facts
                        .push(Fact::new("版本", &context.identity.core.version));
                    document.core.insert(0, current);
                    let source = session.root().join("game/netplay.cfg");
                    if let Some(values) = cache.flags(&source) {
                        for (name, key) in [
                            ("当前联机 SRAM", "savefile_directory"),
                            ("当前联机即时存档", "savestate_directory"),
                        ] {
                            if let Some(path) = values.get(key) {
                                document.saves.push(row(
                                    name,
                                    "当前会话私有目录",
                                    source.to_string_lossy(),
                                    Path::new(path),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_reuse_is_bounded_revision_checked_and_reads_numbered_keys() {
        let directory = std::env::temp_dir().join(format!(
            "nearby-details-{}",
            crate::system::random_id().unwrap()
        ));
        fs::create_dir_all(&directory).unwrap();
        let mut cache = MetadataCache::default();
        for index in 0..12 {
            let path = directory.join(format!("{index}.info"));
            fs::write(&path, "firmware0_path = \"bios.bin\"\n").unwrap();
            assert_eq!(cache.flags(&path).unwrap()["firmware0_path"], "bios.bin");
        }
        assert_eq!(cache.0.len(), 8);
        let path = directory.join("11.info");
        fs::write(&path, "firmware0_path = \"updated-bios.bin\"\n").unwrap();
        assert_eq!(
            cache.flags(&path).unwrap()["firmware0_path"],
            "updated-bios.bin"
        );
        assert_eq!(cache.0.len(), 8);
        fs::remove_file(&path).unwrap();
        assert!(cache.flags(&path).is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn declarations_never_imply_presence_or_content_verification() {
        let info = HashMap::from([
            ("firmware0_path".into(), "missing.bin".into()),
            ("firmware0_opt".into(), "false".into()),
            ("firmware1_path".into(), "../escape.bin".into()),
            ("firmware1_opt".into(), "true".into()),
            ("firmware2_path".into(), "unknown-requirement.bin".into()),
        ]);
        let unknown = firmware(&info, Path::new("test.info"), None, "test");
        assert!(unknown.iter().all(|row| row.status == FileStatus::Unknown));
        assert_eq!(unknown[0].requirement, Some(Requirement::Required));
        assert_eq!(unknown[1].requirement, Some(Requirement::Optional));
        assert_eq!(unknown[2].requirement, Some(Requirement::Unspecified));
        let missing = firmware(
            &info,
            Path::new("test.info"),
            Some(Path::new("/nonexistent-nearby-test")),
            "test",
        );
        assert_eq!(missing[0].status, FileStatus::Missing);
        assert_eq!(missing[1].status, FileStatus::Unknown);
    }

    #[test]
    fn stale_results_cannot_replace_the_current_document() {
        let (requests, _) = mpsc::sync_channel(1);
        let (outgoing, replies) = mpsc::sync_channel(1);
        let mut service = Service {
            epoch: 3,
            channels: Some((requests, replies)),
            ..Service::default()
        };
        outgoing.send((2, Document::default())).unwrap();
        service.poll();
        assert_eq!(service.load, Load::Loading);
        outgoing.send((3, Document::default())).unwrap();
        service.poll();
        assert!(matches!(service.load, Load::Ready { .. }));
        service.cancel();
        outgoing.send((3, Document::default())).unwrap();
        service.load = Load::Loading;
        service.poll();
        assert_eq!(service.load, Load::Loading);
    }
}
