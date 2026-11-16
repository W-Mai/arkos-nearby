//! Content descriptors and the active platform's recipe source.

use crate::{Result, require};
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
    Ok(recipe_systems()?.into_iter().map(|system| CatalogSystem {
        name: system.name,
        title: system.title,
        path: system.path,
        extensions: system.extensions,
    }))
}

pub(crate) fn recipe_systems() -> Result<Vec<super::arkos::recipes::System>> {
    let snapshot = super::plugin::require_capability("recipes")?;
    let roots = snapshot
        .rom_roots
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let path = snapshot
        .recipes
        .systems_paths
        .iter()
        .find(|path| Path::new(path).is_file())
        .ok_or("Missing system catalog")?;
    require(
        std::fs::metadata(path)?.len() <= 4 * 1024 * 1024,
        "System catalog is too large",
    )?;
    super::arkos::recipes::parse_with_roots(&std::fs::read_to_string(path)?, &roots)
}

pub(crate) fn roots() -> Result<Vec<PathBuf>> {
    let roots = super::plugin::require_capability("paths")?
        .rom_roots
        .iter()
        .filter_map(|root| Path::new(root).canonicalize().ok())
        .collect();
    Ok(roots)
}

pub(crate) fn index_path(kind: CatalogKind) -> Result<PathBuf> {
    let snapshot = super::plugin::require_capability("paths")?;
    Ok(PathBuf::from(match kind {
        CatalogKind::Multiplayer => snapshot.catalog.multiplayer_index,
        CatalogKind::Launcher => snapshot.catalog.launcher_index,
    }))
}
