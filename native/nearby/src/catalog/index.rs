use super::{
    Entry, metadata,
    store::{self, Directory, Stamp},
};
use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_CACHE_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const MAX_RECORDS: usize = 100_000;
const MAX_QUERIES: usize = 32;
const MAX_QUERY_IDS: usize = MAX_RECORDS * 4;
const VISIBLE_RESULTS: usize = 4096;

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    entry: Entry,
    search: String,
    #[serde(default)]
    hidden: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Index {
    schema: u32,
    #[serde(default)]
    complete: bool,
    roots: Vec<PathBuf>,
    platforms: Vec<Entry>,
    stamps: Vec<Stamp>,
    #[serde(default)]
    directories: BTreeMap<PathBuf, Directory>,
    #[serde(default, skip_serializing)]
    pub(super) metadata: BTreeMap<PathBuf, metadata::Metadata>,
    pub(super) counts: BTreeMap<PathBuf, usize>,
    records: Vec<Record>,
    #[serde(skip)]
    children: BTreeMap<PathBuf, Vec<Entry>>,
    #[serde(skip)]
    by_path: BTreeMap<PathBuf, usize>,
}

impl Index {
    pub(super) fn recent(&self, times: &BTreeMap<String, u64>) -> Vec<Entry> {
        let mut entries: Vec<_> = self
            .records
            .iter()
            .filter(|record| {
                !record.hidden && !record.entry.directory && times.contains_key(&record.entry.key())
            })
            .map(|record| record.entry.clone())
            .collect();
        entries.sort_by_key(|entry| {
            (
                std::cmp::Reverse(times.get(&entry.key()).copied().unwrap_or(0)),
                entry.name.clone(),
            )
        });
        entries.truncate(100);
        entries
    }
    pub(super) fn cached(path: &Path, roots: &[PathBuf], platforms: &[Entry]) -> Option<Arc<Self>> {
        store::load(path, roots, platforms)
    }
    pub(super) fn matches(&self, roots: &[PathBuf], platforms: &[Entry]) -> bool {
        self.roots == roots && self.platforms == platforms
    }
    pub(super) fn read(path: &Path, roots: &[PathBuf], platforms: &[Entry]) -> Option<Self> {
        if fs::metadata(path).ok()?.len() > MAX_CACHE_BYTES {
            return None;
        }
        let mut value: Self = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
        if !matches!(value.schema, 1..=3)
            || !value.matches(roots, platforms)
            || value.records.len() > MAX_RECORDS
            || value.directories.len() > MAX_RECORDS
            || value.stamps.len() > MAX_RECORDS * 2
        {
            return None;
        }
        value.derive();
        Some(value)
    }
    fn derive(&mut self) {
        self.by_path = self
            .records
            .iter()
            .enumerate()
            .map(|(id, row)| (row.entry.path.clone(), id))
            .collect();
        if self.schema >= 2 {
            self.metadata = self
                .directories
                .values()
                .flat_map(|dir| dir.metadata.clone())
                .collect();
        }
        self.children.clear();
        for (path, dir) in &self.directories {
            let Some(platform) = self
                .platforms
                .iter()
                .find(|platform| path.starts_with(&platform.path))
            else {
                continue;
            };
            let mut children: Vec<_> = dir
                .files
                .iter()
                .filter(|path| !hidden(path, &platform.path))
                .filter_map(|path| self.entry(path).cloned())
                .collect();
            children.extend(
                dir.directories
                    .iter()
                    .filter(|path| {
                        !hidden(path, &platform.path)
                            && self.counts.get(*path).copied().unwrap_or(0) > 0
                    })
                    .map(|path| {
                        let mut entry = platform.clone();
                        entry.path = path.clone();
                        entry.name = self
                            .metadata
                            .get(path)
                            .filter(|meta| !meta.name.is_empty())
                            .map(|meta| meta.name.clone())
                            .unwrap_or_else(|| {
                                crate::flow::label(
                                    &path.file_name().unwrap_or_default().to_string_lossy(),
                                )
                            });
                        entry.count = self.counts.get(path).copied().unwrap_or(0);
                        entry.image = self.metadata.get(path).and_then(|meta| meta.image.clone());
                        entry
                    }),
            );
            children.sort_by_cached_key(|entry| {
                (
                    !entry.directory,
                    entry.name.to_lowercase(),
                    entry.path.clone(),
                )
            });
            self.children.insert(path.clone(), children);
        }
    }
    pub(super) fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.records.iter().map(|record| &record.entry)
    }
    pub(super) fn entry(&self, path: &Path) -> Option<&Entry> {
        self.by_path.get(path).map(|id| &self.records[*id].entry)
    }
    #[cfg(test)]
    pub(super) fn children(&self, path: &Path) -> Option<Vec<Entry>> {
        self.children.get(path).cloned()
    }
    pub(super) fn children_window(
        &self,
        path: &Path,
        offset: usize,
        limit: usize,
    ) -> Option<(Vec<Entry>, usize)> {
        let children = self.children.get(path)?;
        let total = children.len();
        let entries = children
            .iter()
            .skip(offset.min(total))
            .take(limit)
            .cloned()
            .collect();
        Some((entries, total))
    }
    fn search(&self, query: &str, candidates: Option<&[usize]>) -> Vec<usize> {
        if query.is_empty() {
            return Vec::new();
        }
        let terms: Vec<_> = query.split_whitespace().collect();
        let matches = |id: &usize| {
            !self.records[*id].hidden
                && (!self.records[*id].entry.core.is_empty()
                    || !self.records[*id].entry.extensions.is_empty())
                && terms
                    .iter()
                    .all(|term| self.records[*id].search.contains(term))
        };
        match candidates {
            Some(ids) => ids.iter().copied().filter(matches).collect(),
            None => (0..self.records.len()).filter(matches).collect(),
        }
    }
    pub(super) fn materialize(&self, ids: &[usize]) -> Vec<Entry> {
        ids.iter()
            .take(VISIBLE_RESULTS)
            .map(|id| self.records[*id].entry.clone())
            .collect()
    }
}

#[derive(Clone)]
pub struct IndexJob {
    pub(super) roots: Vec<PathBuf>,
    pub(super) platforms: Vec<Entry>,
    pub(super) cache: PathBuf,
    pub(super) seed: Option<Arc<Index>>,
}

fn hidden(path: &Path, platform: &Path) -> bool {
    path.strip_prefix(platform).is_ok_and(|path| {
        path.components()
            .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
    })
}

fn aliases(entry: &Entry) -> String {
    let mut full = String::new();
    let mut initials = String::new();
    for ch in entry.name.chars() {
        if let Some(pinyin) = ch.to_pinyin() {
            full.push_str(pinyin.plain());
            if let Some(first) = pinyin.plain().chars().next() {
                initials.push(first);
            }
        } else {
            full.push(ch);
            initials.push(ch);
        }
    }
    format!(
        "{} {} {full} {initials}",
        entry.name,
        entry.path.file_name().unwrap_or_default().to_string_lossy()
    )
    .to_lowercase()
}

impl IndexJob {
    pub fn launcher() -> crate::Result<Self> {
        let (roots, multiplayer) = super::local_platforms()?;
        let platforms = crate::launcher::recipes::load()?
            .into_iter()
            .filter_map(|system| {
                let path = system.path.canonicalize().ok()?;
                if !roots.iter().any(|root| path.starts_with(root)) || !path.is_dir() {
                    return None;
                }
                let compatible = multiplayer.iter().find(|entry| entry.path == path);
                Some(Entry {
                    path,
                    name: system.title,
                    directory: true,
                    frontend: compatible.map(|e| e.frontend.clone()).unwrap_or_default(),
                    core: compatible.map(|e| e.core.clone()).unwrap_or_default(),
                    platform: compatible
                        .map(|entry| entry.platform.clone())
                        .unwrap_or(system.name),
                    count: 0,
                    image: None,
                    extensions: system.extensions,
                })
            })
            .collect();
        Ok(Self {
            roots,
            platforms,
            cache: Path::new(crate::context::INSTALL).join("launcher-index.json"),
            seed: None,
        })
    }

    pub fn local() -> crate::Result<Self> {
        let (roots, platforms) = super::local_platforms()?;
        Ok(Self {
            roots,
            platforms,
            cache: Path::new(crate::context::INSTALL).join("catalog-index.json"),
            seed: None,
        })
    }
    pub fn cached(&self) -> Option<Arc<Index>> {
        self.seed
            .clone()
            .or_else(|| Index::cached(&self.cache, &self.roots, &self.platforms))
    }
    pub fn run(self) -> Arc<Index> {
        let previous = self.cached();
        if let Some(index) = &previous {
            if index.schema == 3
                && index.complete
                && index.stamps.iter().all(Stamp::current)
                && index
                    .directories
                    .iter()
                    .all(|(path, dir)| dir.current(path))
            {
                return index.clone();
            }
        }
        let mut index = Index {
            schema: 3,
            complete: true,
            roots: self.roots.clone(),
            platforms: self.platforms.clone(),
            stamps: self
                .roots
                .iter()
                .filter_map(|root| Stamp::read(root))
                .collect(),
            directories: BTreeMap::new(),
            metadata: BTreeMap::new(),
            counts: BTreeMap::new(),
            records: Vec::new(),
            children: BTreeMap::new(),
            by_path: BTreeMap::new(),
        };
        let mut watched = BTreeSet::new();
        for platform in &self.platforms {
            let mut pending = vec![platform.path.clone()];
            let mut files = BTreeSet::new();
            while let Some(directory) = pending.pop() {
                let Ok(directory) = directory.canonicalize() else {
                    continue;
                };
                if !directory.starts_with(&platform.path)
                    || !watched.insert(directory.clone())
                    || watched.len() + index.records.len() >= MAX_RECORDS
                {
                    continue;
                }
                let cached = previous
                    .as_ref()
                    .filter(|index| index.schema == 3)
                    .and_then(|index| index.directories.get(&directory))
                    .filter(|dir| dir.current(&directory));
                let Some(dir) = cached
                    .cloned()
                    .or_else(|| Directory::read(&directory, platform, &self.roots))
                else {
                    index.complete = false;
                    continue;
                };
                pending.extend(dir.directories.iter().cloned());
                files.extend(
                    dir.files
                        .iter()
                        .take(MAX_RECORDS.saturating_sub(files.len() + index.records.len()))
                        .cloned(),
                );
                index.metadata.extend(dir.metadata.clone());
                index.directories.insert(directory, dir);
            }
            index.counts.insert(
                platform.path.clone(),
                files
                    .iter()
                    .filter(|path| !hidden(path, &platform.path))
                    .count(),
            );
            for path in files {
                if index.records.len() >= MAX_RECORDS {
                    break;
                }
                let hidden = hidden(&path, &platform.path);
                for parent in path.ancestors().skip(1).take_while(|parent| {
                    !hidden && *parent != platform.path && parent.starts_with(&platform.path)
                }) {
                    *index.counts.entry(parent.into()).or_default() += 1;
                }
                let info = index.metadata.get(&path);
                let mut entry = platform.clone();
                entry.name = info
                    .filter(|info| !info.name.is_empty())
                    .map(|info| info.name.clone())
                    .unwrap_or_else(|| {
                        crate::flow::label(&path.file_name().unwrap_or_default().to_string_lossy())
                    });
                entry.image = info.and_then(|info| info.image.clone());
                entry.path = path;
                entry.directory = false;
                entry.count = 0;
                let search = previous
                    .as_ref()
                    .and_then(|index| index.by_path.get(&entry.path).map(|id| &index.records[*id]))
                    .filter(|record| record.entry == entry)
                    .map(|record| record.search.clone())
                    .unwrap_or_else(|| aliases(&entry));
                index.records.push(Record {
                    entry,
                    search,
                    hidden,
                });
            }
        }
        index.records.sort_by_cached_key(|record| {
            (record.entry.name.to_lowercase(), record.entry.path.clone())
        });
        index.derive();
        if index.complete
            && serde_json::to_vec(&index).is_ok_and(|data| data.len() as u64 <= MAX_CACHE_BYTES)
        {
            let _ = crate::system::write(&self.cache, &index);
        }
        let index = Arc::new(index);
        store::publish(&self.cache, index.clone());
        index
    }
}

#[derive(Default)]
pub(super) struct Queries {
    values: VecDeque<(String, Arc<Vec<usize>>)>,
}
impl Queries {
    pub fn get(&mut self, query: &str) -> Option<Arc<Vec<usize>>> {
        let index = self.values.iter().position(|(key, _)| key == query)?;
        let value = self.values.remove(index)?;
        let ids = value.1.clone();
        self.values.push_back(value);
        Some(ids)
    }
    pub fn candidates(&self, query: &str) -> Option<Arc<Vec<usize>>> {
        self.values
            .iter()
            .filter(|(prefix, _)| !prefix.is_empty() && query.starts_with(prefix))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, ids)| ids.clone())
    }
    pub fn put(&mut self, query: String, ids: Arc<Vec<usize>>) {
        self.values.retain(|(key, _)| key != &query);
        while !self.values.is_empty()
            && (self.values.len() == MAX_QUERIES
                || self.values.iter().map(|(_, ids)| ids.len()).sum::<usize>() + ids.len()
                    > MAX_QUERY_IDS)
        {
            self.values.pop_front();
        }
        self.values.push_back((query, ids));
    }
}

pub struct SearchJob {
    pub(super) index: Arc<Index>,
    pub(super) query: String,
    pub(super) epoch: u64,
    pub(super) candidates: Option<Arc<Vec<usize>>>,
}
pub struct SearchResult {
    pub(super) index: Arc<Index>,
    pub(super) ids: Arc<Vec<usize>>,
    pub(super) query: String,
    pub(super) epoch: u64,
}
impl SearchJob {
    pub fn run(self) -> SearchResult {
        let ids = Arc::new(
            self.index
                .search(&self.query, self.candidates.as_deref().map(Vec::as_slice)),
        );
        SearchResult {
            index: self.index,
            ids,
            query: self.query,
            epoch: self.epoch,
        }
    }
}

#[cfg(test)]
mod tests;
