//! Content and artwork locations for the selected platform.

use crate::Location;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The two catalog snapshots consumed by the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogKind {
    Multiplayer,
    Launcher,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkKind {
    Rom,
    Metadata,
    SavePreview,
    FrontendConfig,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtworkSource {
    pub id: &'static str,
    pub kind: ArtworkKind,
    pub root: &'static str,
}

impl ArtworkSource {
    pub const fn new(id: &'static str, kind: ArtworkKind, root: &'static str) -> Self {
        Self { id, kind, root }
    }
}

/// Immutable content locations, separate from resource loading and indexing.
pub struct ContentPaths {
    pub(crate) rom_roots: &'static [&'static str],
    pub(crate) artwork_sources: &'static [ArtworkSource],
    pub(crate) catalog_index: Location,
    pub(crate) launcher_index: Location,
}

impl ContentPaths {
    pub const fn rom_roots(&self) -> &'static [&'static str] {
        self.rom_roots
    }

    pub const fn artwork_sources(&self) -> &'static [ArtworkSource] {
        self.artwork_sources
    }

    pub fn roots_for(&self, kind: ArtworkKind) -> Vec<PathBuf> {
        self.artwork_sources
            .iter()
            .filter(|source| source.kind == kind)
            .map(|source| PathBuf::from(source.root))
            .collect()
    }

    /// All registered artwork roots, retained for the existing decoder boundary.
    pub fn artwork_roots(&self) -> Vec<PathBuf> {
        self.artwork_sources
            .iter()
            .map(|source| PathBuf::from(source.root))
            .collect()
    }

    pub const fn index(&self, kind: CatalogKind) -> Location {
        match kind {
            CatalogKind::Multiplayer => self.catalog_index,
            CatalogKind::Launcher => self.launcher_index,
        }
    }
}

pub const fn paths() -> &'static ContentPaths {
    &crate::arkos::CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Storage, arkos};

    #[test]
    fn content_permissions_and_cache_roles_preserve_the_platform_layout() {
        let paths = paths();
        assert_eq!(paths.rom_roots(), arkos::PATHS.rom_roots);
        assert_eq!(
            paths.artwork_roots(),
            [
                "/roms",
                "/roms2",
                "/opt/howff/saves/backups",
                "/home/ark/.config/retroarch",
                "/home/ark/.config/retroarch32",
            ]
            .map(PathBuf::from)
        );
        assert_eq!(
            paths.roots_for(ArtworkKind::Rom),
            ["/roms", "/roms2"].map(PathBuf::from)
        );
        assert!(paths.roots_for(ArtworkKind::Metadata).is_empty());
        assert_eq!(
            paths.roots_for(ArtworkKind::SavePreview),
            ["/opt/howff/saves/backups"].map(PathBuf::from)
        );
        assert!(
            paths
                .artwork_sources()
                .iter()
                .all(|source| !source.root.is_empty() && !source.id.is_empty())
        );
        assert_eq!(
            paths.index(CatalogKind::Launcher),
            arkos::application::LAUNCHER_INDEX
        );
        assert_eq!(
            paths.index(CatalogKind::Multiplayer),
            arkos::application::CATALOG_INDEX
        );
        for kind in [CatalogKind::Launcher, CatalogKind::Multiplayer] {
            assert_eq!(paths.index(kind).storage(), Storage::Cache);
        }
    }
}
