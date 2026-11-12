//! Content descriptors and the active platform's recipe source.

use crate::Result;
use std::path::{Path, PathBuf};

pub(crate) use super::arkos::metadata::{path as metadata_path, read as metadata};
pub(crate) use howff_platform::content::CatalogKind;

pub(crate) use super::arkos::catalog::secondary_root;
pub use super::arkos::recipes::{finish, prepare};
pub(crate) use super::arkos::recipes::{recipe, settings};

/// Platform directory metadata consumed by the shared catalog.
pub(crate) struct CatalogSystem {
    pub name: String,
    pub title: String,
    pub path: PathBuf,
    pub extensions: String,
}

/// A local recipe consumed by the existing game lifecycle.
pub struct Launch {
    pub path: PathBuf,
    pub command: String,
}

pub(crate) fn systems() -> Result<impl Iterator<Item = CatalogSystem>> {
    Ok(super::arkos::recipes::load()?
        .into_iter()
        .map(|system| CatalogSystem {
            name: system.name,
            title: system.title,
            path: system.path,
            extensions: system.extensions,
        }))
}

pub(crate) fn roots() -> Vec<PathBuf> {
    howff_platform::content::paths()
        .rom_roots()
        .iter()
        .filter_map(|root| Path::new(root).canonicalize().ok())
        .collect()
}

pub(crate) fn index_path(kind: CatalogKind) -> PathBuf {
    howff_platform::content::paths()
        .index(kind)
        .path()
        .to_path_buf()
}
