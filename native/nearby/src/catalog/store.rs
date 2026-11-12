use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::{Entry, Index, metadata};
use crate::platform::content;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Stamp {
    path: PathBuf,
    device: u64,
    inode: u64,
    modified: (i64, i64),
    bytes: u64,
}

impl Stamp {
    pub fn read(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        Some(Self {
            path: path.into(),
            device: meta.dev(),
            inode: meta.ino(),
            modified: (meta.mtime(), meta.mtime_nsec()),
            bytes: meta.len(),
        })
    }
    pub fn current(&self) -> bool {
        Self::read(&self.path).as_ref() == Some(self)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Directory {
    stamp: Stamp,
    list: Option<Stamp>,
    pub directories: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
    pub metadata: BTreeMap<PathBuf, metadata::Metadata>,
}

impl Directory {
    pub fn current(&self, path: &Path) -> bool {
        self.stamp.current() && self.list == Stamp::read(&content::metadata_path(path))
    }
    pub fn read(path: &Path, platform: &Entry, roots: &[PathBuf]) -> Option<Self> {
        let mut result = Self {
            stamp: Stamp::read(path)?,
            list: Stamp::read(&content::metadata_path(path)),
            directories: Vec::new(),
            files: Vec::new(),
            metadata: content::metadata(path, roots),
        };
        for child in fs::read_dir(path)
            .ok()?
            .flatten()
            .take(super::index::MAX_RECORDS)
        {
            let Ok(child) = child.path().canonicalize() else {
                continue;
            };
            if !child.starts_with(&platform.path) {
                continue;
            }
            if child.is_dir() {
                result.directories.push(child);
            } else if platform.accepts(&child) {
                result.files.push(child);
            }
        }
        result.directories.sort();
        result.directories.dedup();
        result.files.sort();
        result.files.dedup();
        // Never persist a directory snapshot spanning a concurrent change.
        result.current(path).then_some(result)
    }
}

struct Cached {
    path: PathBuf,
    stamp: Option<Stamp>,
    index: Arc<Index>,
}

// One bounded snapshot per menu process. Radio workers share the disk snapshot.
static CACHED: Mutex<Option<Cached>> = Mutex::new(None);

pub(super) fn load(path: &Path, roots: &[PathBuf], platforms: &[Entry]) -> Option<Arc<Index>> {
    let mut cache = CACHED.lock().unwrap_or_else(|error| error.into_inner());
    let stamp = Stamp::read(path);
    if let Some(value) = cache.as_ref().filter(|value| {
        value.path == path && value.stamp == stamp && value.index.matches(roots, platforms)
    }) {
        return Some(value.index.clone());
    }
    let index = Arc::new(Index::read(path, roots, platforms)?);
    *cache = Some(Cached {
        path: path.into(),
        stamp,
        index: index.clone(),
    });
    Some(index)
}

pub(super) fn publish(path: &Path, index: Arc<Index>) {
    *CACHED.lock().unwrap_or_else(|error| error.into_inner()) = Some(Cached {
        path: path.into(),
        stamp: Stamp::read(path),
        index,
    });
}
