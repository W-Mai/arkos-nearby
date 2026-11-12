use howff_platform::arkos;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::Entry;
use crate::context::Context;
use crate::system::{Lock, read, wall_time, write};
use crate::{Result, require};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FAVORITES: usize = 4096;
const MAX_RECENT: usize = 100_000;

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Sort {
    #[default]
    Name,
    Recent,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Library {
    schema: u32,
    pub favorites: BTreeMap<String, Entry>,
    pub recent: BTreeMap<String, u64>,
    pub favorite_sort: Sort,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            schema: 1,
            favorites: BTreeMap::new(),
            recent: BTreeMap::new(),
            favorite_sort: Sort::Name,
        }
    }
}

pub(super) fn key(path: &Path) -> String {
    format!("{:x}", Sha256::digest(path.as_os_str().as_encoded_bytes()))
}

impl Library {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(arkos::application::LIBRARY.local());
        if !path.exists() {
            return Ok(Self::default());
        }
        require(
            path.metadata()?.len() <= MAX_BYTES,
            "Library record is too large",
        )?;
        let value: Self = read(&path)?;
        require(
            value.schema == 1
                && value.favorites.len() <= MAX_FAVORITES
                && value.recent.len() <= MAX_RECENT,
            "Unsupported library record",
        )?;
        Ok(value)
    }

    fn change(root: &Path, change: impl FnOnce(&mut Self) -> Result<()>) -> Result<Self> {
        let _lock = Lock::try_acquire(&root.join("library.lock"))?;
        let mut value = Self::load(root)?;
        change(&mut value)?;
        require(
            serde_json::to_vec(&value)?.len() as u64 <= MAX_BYTES,
            "Library record is full",
        )?;
        write(&root.join(arkos::application::LIBRARY.local()), &value)?;
        Ok(value)
    }

    pub fn toggle(root: &Path, entry: &Entry) -> Result<Self> {
        require(!entry.directory, "Only games can be collected")?;
        Self::change(root, |value| {
            let key = key(&entry.path);
            if value.favorites.remove(&key).is_none() {
                require(value.favorites.len() < MAX_FAVORITES, "Collection is full")?;
                value.favorites.insert(key, entry.clone());
            }
            Ok(())
        })
    }

    pub fn sort(root: &Path) -> Result<Self> {
        Self::change(root, |value| {
            value.favorite_sort = match value.favorite_sort {
                Sort::Name => Sort::Recent,
                Sort::Recent => Sort::Name,
            };
            Ok(())
        })
    }

    fn used(root: &Path, path: &Path) -> Result<Self> {
        Self::change(root, |value| {
            value.recent.insert(key(path), wall_time() as u64);
            if value.recent.len() > MAX_RECENT {
                let oldest = value
                    .recent
                    .iter()
                    .min_by_key(|(key, time)| (**time, *key))
                    .map(|(key, _)| key.clone());
                if let Some(key) = oldest {
                    value.recent.remove(&key);
                }
            }
            Ok(())
        })
    }
}

pub(crate) fn record_play(context: &Context) {
    record_path(&context.game_path);
}

pub(crate) fn record_path(path: &Path) {
    if let Err(error) = Library::used(&PathBuf::from(arkos::PATHS.personal), path) {
        eprintln!("Could not record library use: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_merge_and_preserve_unreadable_records() {
        let root = std::env::temp_dir().join(format!(
            "nearby-library-{}",
            crate::system::random_id().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        let entry = Entry {
            path: root.join("game.nes"),
            name: "Game".into(),
            directory: false,
            frontend: "retroarch".into(),
            core: "fceumm".into(),
            platform: "nes".into(),
            count: 0,
            image: None,
            extensions: String::new(),
        };
        Library::toggle(&root, &entry).unwrap();
        Library::used(&root, &entry.path).unwrap();
        let value = Library::sort(&root).unwrap();
        assert_eq!(value.favorites.len(), 1);
        assert_eq!(value.recent.len(), 1);
        assert!(value.favorite_sort == Sort::Recent);
        std::fs::write(root.join("library.json"), b"unreadable").unwrap();
        assert!(Library::toggle(&root, &entry).is_err());
        assert_eq!(
            std::fs::read(root.join("library.json")).unwrap(),
            b"unreadable"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
