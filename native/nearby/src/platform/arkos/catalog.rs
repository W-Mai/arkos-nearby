use std::path::Path;

pub(crate) fn secondary_root(root: &Path) -> bool {
    root.ends_with("roms2")
}
