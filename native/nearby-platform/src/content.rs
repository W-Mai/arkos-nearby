//! Content and artwork locations for the selected platform.

use crate::Location;
use std::path::PathBuf;

/// The two catalog snapshots consumed by the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogKind {
    Multiplayer,
    Launcher,
}

/// Immutable content locations, separate from resource loading and indexing.
pub struct ContentPaths {
    pub(crate) rom_roots: &'static [&'static str],
    pub(crate) artwork_extra: &'static [Location],
    pub(crate) catalog_index: Location,
    pub(crate) launcher_index: Location,
}

impl ContentPaths {
    pub const fn rom_roots(&self) -> &'static [&'static str] {
        self.rom_roots
    }

    /// Artwork also includes save previews and frontend configuration images.
    pub fn artwork_roots(&self) -> Vec<PathBuf> {
        self.rom_roots
            .iter()
            .map(PathBuf::from)
            .chain(
                self.artwork_extra
                    .iter()
                    .map(|location| location.path().to_path_buf()),
            )
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
        assert!(
            paths
                .artwork_extra
                .iter()
                .all(|location| !paths.rom_roots().contains(&location.as_str()))
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
