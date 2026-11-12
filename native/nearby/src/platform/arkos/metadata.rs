use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_XML_BYTES: u64 = 32 * 1024 * 1024;
const MAX_RECORDS: usize = 100_000;

use crate::catalog::metadata::Metadata;

fn local_path(base: &Path, value: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let path = base.join(value).canonicalize().ok()?;
    (path.is_file() && roots.iter().any(|root| path.starts_with(root))).then_some(path)
}

pub(crate) fn path(directory: &Path) -> PathBuf {
    directory.join("gamelist.xml")
}

pub(crate) fn read(directory: &Path, roots: &[PathBuf]) -> BTreeMap<PathBuf, Metadata> {
    let path = path(directory);
    let result = (|| {
        if fs::metadata(&path).ok()?.len() > MAX_XML_BYTES {
            return None;
        }
        let xml = fs::read_to_string(path).ok()?;
        let document = roxmltree::Document::parse(&xml).ok()?;
        let mut entries = BTreeMap::new();
        for game in document
            .descendants()
            .filter(|node| node.has_tag_name("game"))
            .take(MAX_RECORDS)
        {
            let field = |name| {
                game.children()
                    .find(|node| node.has_tag_name(name))
                    .and_then(|node| node.text())
            };
            let Some(path) = field("path").and_then(|value| local_path(directory, value, roots))
            else {
                continue;
            };
            let name = field("name")
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(crate::flow::label)
                .unwrap_or_default();
            let image = ["image", "thumbnail"]
                .into_iter()
                .find_map(|name| field(name).and_then(|value| local_path(directory, value, roots)));
            entries.insert(path, Metadata { name, image });
        }
        Some(entries)
    })();
    result.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_paths_retain_local_confinement() {
        let root = std::env::temp_dir().join(format!("nearby-catalog-{}", std::process::id()));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested/game.nes"), b"test").unwrap();
        fs::write(root.join("picture.png"), b"test").unwrap();
        fs::write(root.join("gamelist.xml"), "<gameList><game><path>./nested/game.nes</path><name>Game &amp; Friend</name><image>./picture.png</image></game><game><path>/etc/passwd</path><name>Outside</name></game></gameList>").unwrap();
        let root = root.canonicalize().unwrap();
        let roots = [root.clone()];
        let entries = read(&root, &roots);
        assert_eq!(entries.len(), 1);
        let metadata = entries.get(&root.join("nested/game.nes")).unwrap();
        assert_eq!(metadata.name, "Game & Friend");
        assert_eq!(metadata.image, Some(root.join("picture.png")));
        fs::remove_dir_all(root).unwrap();
    }
}
