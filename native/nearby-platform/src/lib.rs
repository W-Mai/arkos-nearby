//! Platform-owned locations shared by the runtime and interface.

pub mod arkos;
pub mod content;

use std::path::Path;

/// Storage ownership, independent of directories shared on a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    Program,
    Configuration,
    Personal,
    Cache,
    Maintenance,
    Runtime,
    Firmware,
    Content,
}

/// An immutable platform location with an explicit storage role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    storage: Storage,
    absolute: &'static str,
    relative: &'static str,
    local: &'static str,
}

impl Location {
    pub const fn storage(self) -> Storage {
        self.storage
    }

    pub const fn as_str(self) -> &'static str {
        self.absolute
    }

    /// The same location beneath an installer-owned filesystem root.
    pub const fn relative(self) -> &'static str {
        self.relative
    }

    /// The location relative to its declaring platform root.
    pub const fn local(self) -> &'static str {
        self.local
    }

    pub fn path(&self) -> &Path {
        Path::new(self.absolute)
    }
}

impl AsRef<Path> for Location {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

/// Physical roots used by the selected firmware layout.
#[derive(Clone, Copy, Debug)]
pub struct PlatformPaths {
    pub program: &'static str,
    pub configuration: &'static str,
    pub personal: &'static str,
    pub cache: &'static str,
    pub runtime: &'static str,
    pub rom_roots: &'static [&'static str],
}

impl PlatformPaths {
    pub fn root(self, storage: Storage) -> Option<&'static Path> {
        Some(Path::new(match storage {
            Storage::Program | Storage::Maintenance => self.program,
            Storage::Configuration => self.configuration,
            Storage::Personal => self.personal,
            Storage::Cache => self.cache,
            Storage::Runtime => self.runtime,
            Storage::Firmware | Storage::Content => return None,
        }))
    }
}
