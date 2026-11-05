use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::context::{self, Context};
use crate::{Result, require};

mod index;
pub(crate) mod library;
pub(crate) mod matching;
mod metadata;
pub(crate) mod shelf;
mod store;
pub use index::{Index, IndexJob, SearchJob, SearchResult};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    path: PathBuf,
    name: String,
    directory: bool,
    frontend: String,
    core: String,
    platform: String,
    count: usize,
    image: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    extensions: String,
}

#[cfg(test)]
mod library_tests {
    use super::*;

    #[test]
    fn pending_library_work_and_errors_are_distinct_from_game_preparation() {
        let mut browser = Browser::with_inventory(IndexJob::local().unwrap(), true).unwrap();
        browser.indexing = false;
        browser.status.clear();
        browser.levels.push(Level {
            kind: LibraryPage::Search,
            path: None,
            entries: vec![],
            selected: 0,
            offset: 0,
            total: 0,
        });
        browser.query = "mario".into();
        let load = |browser: &Browser| {
            serde_json::from_value::<nearby_menu::LibraryLoad>(
                browser.view()["library"]["load"].clone(),
            )
            .unwrap()
        };
        browser.search_running = true;
        assert!(!browser.view()["busy"].as_bool().unwrap());
        assert_eq!(load(&browser), nearby_menu::LibraryLoad::Loading);
        browser.search_running = false;
        assert_eq!(load(&browser), nearby_menu::LibraryLoad::Ready);
        browser.status = "无法读取目录，请返回后重试。".into();
        assert_eq!(
            load(&browser),
            nearby_menu::LibraryLoad::Failed {
                message: browser.status.clone()
            }
        );
        browser.query_due = Some(crate::system::monotonic());
        assert_eq!(load(&browser), nearby_menu::LibraryLoad::Loading);
    }

    #[test]
    fn returning_between_browsers_retains_one_compatible_index() {
        let directory = std::env::temp_dir().join(format!(
            "nearby-shared-index-{}",
            crate::system::random_id().unwrap()
        ));
        fs::create_dir_all(&directory).unwrap();
        let mut job = IndexJob::local().unwrap();
        job.cache = directory.join("index.json");
        let mut first = Browser::with_inventory(job.clone(), true).unwrap();
        let mut second = Browser::with_inventory(job.clone(), true).unwrap();
        let index = job.run();
        first.accept_index(index.clone());
        second.accept_index(index.clone());
        for _ in 0..32 {
            for browser in [&mut first, &mut second] {
                browser.resume();
                assert!(!browser.indexing);
                assert!(Arc::ptr_eq(browser.index.as_ref().unwrap(), &index));
            }
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn visible_favorites_export_each_games_local_artwork() {
        let mut browser = Browser::with_inventory(IndexJob::local().unwrap(), true).unwrap();
        browser.levels.push(Level {
            kind: LibraryPage::Favorites,
            path: None,
            entries: (0..5)
                .map(|i| Entry {
                    path: PathBuf::from(format!("/roms/nes/{i}/game.nes")),
                    image: (i != 4).then(|| PathBuf::from(format!("/roms/nes/{i}/image.png"))),
                    name: format!("Game {i}"),
                    directory: false,
                    frontend: "retroarch".into(),
                    core: "fceumm".into(),
                    platform: "nes".into(),
                    count: 0,
                    extensions: String::new(),
                })
                .collect(),
            selected: 0,
            offset: 0,
            total: 5,
        });
        let first = browser.view();
        let rows = first["entries"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row["artwork"], format!("/roms/nes/{i}/image.png"));
        }
        browser.levels.last_mut().unwrap().selected = 4;
        let last = browser.view();
        let rows = last["entries"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["artwork"], "/roms/nes/3/image.png");
        assert_eq!(rows[1]["artwork"], "");
        let selected = browser.current().unwrap().key();
        for (columns, rows) in [(3, 2), (2, 1), (4, 2), (3, 1), (3, 3), (2, 4)] {
            let grid = serde_json::from_value(json!({"columns":columns,"rows":rows})).unwrap();
            browser.shelf_layout(grid);
            assert_eq!(browser.current().unwrap().key(), selected);
            let view = browser.view();
            let visible = view["entries"].as_array().unwrap();
            let slot = view["selected"].as_u64().unwrap() as usize;
            assert_eq!(visible[slot]["key"], selected);
            assert!(visible.len() <= columns * rows);
            browser.dispatch("up").unwrap();
            browser.dispatch("down").unwrap();
            assert_eq!(browser.current().unwrap().key(), selected);
        }
        browser.dispatch("shelf_home").unwrap();
        assert_eq!(browser.current().unwrap().name(), "Game 0");
        assert!(!browser.exit);
    }

    #[test]
    fn list_window_resizes_without_changing_selection_or_replacing_the_index() {
        let mut browser = Browser::with_inventory(IndexJob::local().unwrap(), true).unwrap();
        let index = browser.index_job().run();
        browser.accept_index(index.clone());
        browser.levels.push(Level {
            kind: LibraryPage::Platforms,
            path: None,
            entries: (0..20)
                .map(|index| Entry {
                    path: PathBuf::from(format!("/roms/{index}")),
                    name: format!("Platform {index}"),
                    directory: true,
                    frontend: String::new(),
                    core: String::new(),
                    platform: "nes".into(),
                    count: 1,
                    image: None,
                    extensions: String::new(),
                })
                .collect(),
            selected: 9,
            offset: 0,
            total: 20,
        });
        let key = browser.current().unwrap().key();
        let epoch = browser.input_epoch;
        for rows in [2, 1, nearby_menu::LIST_WINDOW_ITEMS, 3] {
            browser.list_layout(rows);
            let view = browser.view();
            let entries = view["entries"].as_array().unwrap();
            assert_eq!(entries.len(), rows);
            assert_eq!(
                entries[view["selected"].as_u64().unwrap() as usize]["key"],
                key
            );
            assert_eq!(browser.input_epoch, epoch);
            assert!(Arc::ptr_eq(browser.index.as_ref().unwrap(), &index));
            browser.dispatch("page_down").unwrap();
            assert_eq!(browser.levels.last().unwrap().selected, 9 + rows);
            browser.dispatch("page_up").unwrap();
            assert_eq!(browser.current().unwrap().key(), key);
        }
        for rows in [0, nearby_menu::LIST_WINDOW_ITEMS + 1, usize::MAX] {
            browser.list_layout(rows);
            assert_eq!(browser.list_rows, 3);
        }
    }

    #[test]
    fn launcher_library_link_is_stable_and_never_a_game() {
        let mut browser = Browser::with_inventory(IndexJob::local().unwrap(), true).unwrap();
        browser.levels.push(Level {
            kind: LibraryPage::Favorites,
            path: None,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
            total: 0,
        });
        browser.personal.favorites.clear();
        browser.favorites();
        let link = browser.current().unwrap();
        assert!(link.library_link());
        assert!(!link.netplay());
        assert!(browser.game().is_none());
        assert!(browser.dispatch("confirm").unwrap().is_none());
        let mut game = link.clone();
        game.path = PathBuf::from("/roms/nes/local.nes");
        game.directory = false;
        game.platform = "nes".into();
        browser.personal.favorites.insert(game.key(), game);
        browser.favorites();
        assert_eq!(browser.current().unwrap().key(), link.key());
        assert!(browser.game().is_none());
        browser.personal.favorites.clear();
        browser.favorites();
        assert_eq!(browser.current().unwrap().key(), link.key());
    }

    #[test]
    fn query_cache_follows_snapshot_identity_and_rejects_replaced_snapshots() {
        let mut browser = Browser::new().unwrap();
        let index = browser.index_job().run();
        browser.indexed(index.clone());
        browser.dispatch("auxiliary").unwrap();
        browser.query = "bomber".into();
        browser.changed_query();
        let result = browser.search_job().unwrap().run();
        browser.searched(result);
        browser.indexed(index.clone());
        assert!(browser.queries.get("bomber").is_some());
        browser.query = "tank".into();
        browser.changed_query();
        let stale = browser.search_job().unwrap().run();
        let replacement = Arc::new((*index).clone());
        browser.indexed(replacement);
        assert!(browser.queries.get("bomber").is_none());
        browser.searched(stale);
        assert!(browser.query_due.is_some());
        let result = browser.search_job().unwrap().run();
        browser.searched(result);
        assert!(browser.query_due.is_none());
        assert!(browser.queries.get("tank").is_some());
    }

    #[test]
    fn library_navigation_is_bounded_and_rejects_obsolete_search_results() {
        let mut browser = Browser::new().unwrap();
        browser.dispatch("auxiliary").unwrap();
        assert!(browser.kind() == LibraryPage::Search && browser.keyboard);
        for _ in 0..50 {
            browser.dispatch("left").unwrap();
            browser.dispatch("up").unwrap();
        }
        assert!(browser.key_selected < KEYS.len());
        browser.query = "bomber".into();
        browser.changed_query();
        let epoch = browser.query_epoch;
        browser.dispatch("back").unwrap();
        assert!(!browser.keyboard);
        browser.dispatch("back").unwrap();
        assert!(browser.kind() == LibraryPage::Platforms);
        browser.searched(SearchResult {
            index: browser.index_job().run(),
            ids: Arc::new(Vec::new()),
            query: "bomber".into(),
            epoch,
        });
        assert!(browser.kind() == LibraryPage::Platforms);
        let entry = browser.levels[0].entries[0].clone();
        for kind in [
            LibraryPage::Platforms,
            LibraryPage::Directory,
            LibraryPage::Favorites,
            LibraryPage::Search,
        ] {
            browser.levels = vec![Level {
                kind,
                path: None,
                entries: vec![entry.clone(); 100],
                selected: 99,
                offset: 0,
                total: 100,
            }];
            let view = browser.view();
            let entries = view["entries"].as_array().unwrap();
            assert!(entries.len() <= 4);
            assert!(view["selected"].as_u64().unwrap() < entries.len() as u64);
        }
    }

    #[test]
    fn large_directory_keeps_a_window_while_navigation_preserves_position() {
        let root = std::env::temp_dir().join(format!(
            "nearby-window-{}",
            crate::system::random_id().unwrap()
        ));
        let platform_path = root.join("nes");
        std::fs::create_dir_all(&platform_path).unwrap();
        for index in 0..40 {
            std::fs::write(platform_path.join(format!("game-{index:02}.nes")), b"game").unwrap();
        }
        let root = root.canonicalize().unwrap();
        let platform_path = root.join("nes");
        let platform = Entry {
            path: platform_path.clone(),
            name: "NES".into(),
            directory: true,
            frontend: "retroarch".into(),
            core: "fceumm".into(),
            platform: "nes".into(),
            count: 0,
            image: None,
            extensions: String::new(),
        };
        let job = IndexJob {
            roots: vec![root.clone()],
            platforms: vec![platform],
            cache: root.join("catalog-index.json"),
            seed: None,
        };
        let mut browser = Browser::with_inventory(job.clone(), true).unwrap();
        browser.indexed(job.run());
        browser.dispatch("confirm").unwrap();
        assert_eq!(browser.view()["library"]["total"], 40);
        assert_eq!(
            browser.levels.last().unwrap().entries.len(),
            DIRECTORY_WINDOW
        );
        for _ in 0..20 {
            browser.dispatch("down").unwrap();
        }
        assert_eq!(browser.view()["library"]["position"], 21);
        assert_eq!(browser.current().unwrap().name(), "game-20.nes");
        assert_eq!(
            browser.levels.last().unwrap().entries.len(),
            DIRECTORY_WINDOW
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
impl Entry {
    pub fn library_link(&self) -> bool {
        self.platform == "library" && self.directory
    }
    pub fn key(&self) -> String {
        library::key(&self.path)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn frontend(&self) -> &str {
        &self.frontend
    }
    pub fn core(&self) -> &str {
        &self.core
    }
    pub fn platform_name(&self) -> &str {
        PLATFORMS
            .iter()
            .find_map(|(id, name)| (*id == self.platform).then_some(*name))
            .unwrap_or(&self.platform)
    }
    pub fn directory(&self) -> bool {
        self.directory
    }
    pub fn netplay(&self) -> bool {
        !self.core.is_empty()
    }
    fn accepts(&self, path: &Path) -> bool {
        if self.extensions.is_empty() {
            return game_file(path);
        }
        path.extension().is_some_and(|ext| {
            self.extensions.split_whitespace().any(|e| {
                e.trim_start_matches('.')
                    .eq_ignore_ascii_case(&ext.to_string_lossy())
            })
        })
    }
    pub fn prepare(&self) -> Result<Context> {
        let library = context::resolve_core(&self.frontend, &self.core)?;
        context::parse(
            &self.frontend,
            &[
                "-L".into(),
                library.to_string_lossy().into(),
                self.path.to_string_lossy().into(),
            ],
        )
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LibraryPage {
    Platforms,
    #[default]
    Directory,
    Favorites,
    Search,
}

const KEYS: [&str; 40] = [
    "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "A", "S", "D", "F", "G", "H", "J", "K", "L",
    "退格", "Z", "X", "C", "V", "B", "N", "M", "空格", "清空", "结果", "1", "2", "3", "4", "5",
    "6", "7", "8", "9", "0",
];

struct Level {
    kind: LibraryPage,
    path: Option<Entry>,
    entries: Vec<Entry>,
    selected: usize,
    offset: usize,
    total: usize,
}

pub struct Browser {
    shelf: shelf::Grid,
    list_rows: usize,
    launcher: bool,
    roots: Vec<PathBuf>,
    levels: Vec<Level>,
    pub busy: bool,
    pub input_epoch: f64,
    canceled: bool,
    pub exit: bool,
    pub selected_context: Option<Context>,
    status: String,
    inventory: IndexJob,
    pub indexing: bool,
    index: Option<Arc<Index>>,
    personal: library::Library,
    queries: index::Queries,
    query: String,
    keyboard: bool,
    key_selected: usize,
    query_epoch: u64,
    query_due: Option<f64>,
    search_running: bool,
}

const PLATFORMS: &[(&str, &str)] = &[
    ("nes", "NES / FC"),
    ("fds", "FDS"),
    ("snes", "SFC"),
    ("megadrive", "MD"),
    ("mastersystem", "SMS"),
    ("segacd", "Sega CD"),
    ("sega32x", "32X"),
    ("pcengine", "PC Engine"),
    ("arcade", "街机"),
    ("neogeo", "Neo Geo"),
    ("cps1", "CPS1"),
    ("cps2", "CPS2"),
    ("cps3", "CPS3"),
    ("psx", "PlayStation"),
    ("gb", "GB"),
    ("gbc", "GBC"),
    ("gba", "GBA"),
];

const DIRECTORY_WINDOW: usize = 16;

fn game_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        matches!(
            extension.to_string_lossy().to_ascii_lowercase().as_str(),
            "zip"
                | "7z"
                | "nes"
                | "fds"
                | "unf"
                | "unif"
                | "sfc"
                | "smc"
                | "gb"
                | "gbc"
                | "gba"
                | "bin"
                | "md"
                | "gen"
                | "sms"
                | "pce"
                | "32x"
                | "chd"
                | "pbp"
                | "iso"
        )
    })
}

impl Browser {
    pub fn list_layout(&mut self, rows: usize) {
        if (1..=nearby_menu::LIST_WINDOW_ITEMS).contains(&rows) {
            self.list_rows = rows;
        }
    }
    pub fn shelf_layout(&mut self, grid: shelf::Grid) {
        if grid.valid() {
            self.shelf = grid;
        }
    }
    pub fn reopen(&mut self) {
        self.resume();
        self.indexing = true;
        if let Ok(personal) = library::Library::load(Path::new(context::INSTALL)) {
            self.personal = personal;
        }
        if self.kind() == LibraryPage::Favorites {
            self.favorites();
        }
        self.refresh_platforms();
        self.refresh_recent();
    }

    pub fn resume(&mut self) {
        self.input_epoch = crate::system::monotonic();
        self.search_running = false;
        self.exit = false;
        if self.kind() == LibraryPage::Search {
            self.changed_query();
        }
    }

    pub(crate) fn accept_index(&mut self, index: Arc<Index>) {
        if index.matches(&self.roots, &self.inventory.platforms) {
            self.indexed(index);
        }
    }

    pub(crate) fn refresh_personal(&mut self) {
        if let Ok(personal) = library::Library::load(Path::new(context::INSTALL)) {
            self.personal = personal;
            if self.kind() == LibraryPage::Favorites {
                self.favorites();
            }
            self.refresh_recent();
        }
    }
    pub fn new() -> Result<Self> {
        Self::with_inventory(IndexJob::local()?, false)
    }
    pub fn launcher(favorites: bool) -> Result<Self> {
        Self::launcher_with_inventory(IndexJob::launcher()?, favorites)
    }
    pub(crate) fn launcher_with_inventory(inventory: IndexJob, favorites: bool) -> Result<Self> {
        let mut browser = Self::with_inventory(inventory, true)?;
        if favorites {
            browser.levels.push(Level {
                kind: LibraryPage::Favorites,
                path: None,
                entries: Vec::new(),
                selected: 0,
                offset: 0,
                total: 0,
            });
            browser.favorites();
        }
        Ok(browser)
    }
    pub fn current(&self) -> Option<Entry> {
        self.levels
            .last()?
            .entries
            .get(self.levels.last()?.selected)
            .cloned()
    }
    pub fn game(&self) -> Option<Entry> {
        self.current().filter(|e| {
            !e.directory
                && !(self.kind() == LibraryPage::Search
                    && (self.keyboard || self.search_running || self.query_due.is_some()))
        })
    }
    pub fn root(&self) -> bool {
        self.levels.len() == 1
    }
    pub fn editing(&self) -> bool {
        self.kind() == LibraryPage::Search && self.keyboard
    }
    pub fn searching(&self) -> bool {
        self.kind() == LibraryPage::Search
    }
    pub fn is_favorite(&self) -> bool {
        self.game()
            .is_some_and(|entry| self.personal.favorites.contains_key(&entry.key()))
    }
    pub fn message(&mut self, value: impl Into<String>) {
        self.status = value.into();
    }
    pub fn select_key(&mut self, key: &str) {
        if let Some(level) = self.levels.last_mut() {
            if let Some(index) = level.entries.iter().position(|entry| entry.key() == key) {
                level.selected = index;
            }
        }
    }
    fn with_inventory(inventory: IndexJob, launcher: bool) -> Result<Self> {
        let roots = inventory.roots.clone();
        let mut entries = Vec::new();
        let personal = library::Library::load(Path::new(context::INSTALL));
        let status = if personal.is_err() {
            "收藏记录无法读取，原文件已保留。".into()
        } else {
            String::new()
        };
        if !launcher {
            entries.push(Entry {
                path: PathBuf::new(),
                name: "收藏夹".into(),
                directory: true,
                frontend: String::new(),
                core: String::new(),
                platform: "favorites".into(),
                count: 0,
                image: None,
                extensions: String::new(),
            });
        }
        // Platform visibility is derived from the index, including on a cold start.
        let entry_total = entries.len();
        let mut browser = Self {
            shelf: shelf::Grid::default(),
            list_rows: 4,
            launcher,
            roots,
            levels: vec![Level {
                kind: LibraryPage::Platforms,
                path: None,
                entries,
                selected: 0,
                offset: 0,
                total: entry_total,
            }],
            busy: false,
            input_epoch: crate::system::monotonic(),
            canceled: false,
            exit: false,
            selected_context: None,
            status,
            inventory,
            indexing: true,
            index: None,
            personal: personal.unwrap_or_default(),
            queries: index::Queries::default(),
            query: String::new(),
            keyboard: false,
            key_selected: 0,
            query_epoch: 0,
            query_due: None,
            search_running: false,
        };
        let job = browser.index_job();
        if let Some(index) = Index::cached(&job.cache, &job.roots, &job.platforms) {
            browser.indexed(index);
            browser.indexing = true;
        }
        Ok(browser)
    }

    pub fn index_job(&self) -> IndexJob {
        let mut job = self.inventory.clone();
        job.seed = self.index.clone();
        job
    }

    pub fn indexed(&mut self, index: Arc<Index>) {
        self.indexing = false;
        if self
            .index
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &index))
        {
            return;
        }
        self.index = Some(index.clone());
        self.queries = index::Queries::default();
        self.refresh_platforms();
        for level_index in 0..self.levels.len() {
            let Some(level) = self.levels.get(level_index) else {
                continue;
            };
            if level.kind == LibraryPage::Directory {
                let selected = level.offset + level.selected;
                let _ = self.reload_directory_window(level_index, selected);
            }
            let Some(level) = self.levels.get_mut(level_index) else {
                continue;
            };
            for entry in &mut level.entries {
                if let Some(count) = index.counts.get(&entry.path) {
                    entry.count = *count;
                }
                if let Some(metadata) = index.metadata.get(&entry.path) {
                    if !metadata.name.is_empty() {
                        entry.name = metadata.name.clone();
                    }
                    entry.image = metadata.image.clone();
                }
            }
        }
        if self.kind() == LibraryPage::Favorites {
            self.favorites();
        }
        self.refresh_recent();
        if self.kind() == LibraryPage::Search {
            self.changed_query();
        }
    }

    fn refresh_platforms(&mut self) {
        let Some(index) = &self.index else { return };
        let Some(root) = self.levels.first_mut() else {
            return;
        };
        let selected = root.entries.get(root.selected).map(Entry::key);
        root.entries.retain(|entry| entry.platform == "favorites");
        let recent_count = if self.launcher {
            index.recent(&self.personal.recent).len()
        } else {
            0
        };
        if recent_count > 0 {
            root.entries.push(Entry {
                path: PathBuf::new(),
                name: "最近游玩".into(),
                directory: true,
                frontend: String::new(),
                core: String::new(),
                platform: "recent".into(),
                count: recent_count,
                image: None,
                extensions: String::new(),
            });
        }
        root.entries.extend(
            self.inventory
                .platforms
                .iter()
                .filter(|entry| {
                    (self.launcher || !entry.core.is_empty())
                        && index.counts.get(&entry.path).copied().unwrap_or(0) > 0
                })
                .cloned()
                .map(|mut entry| {
                    entry.count = index.counts[&entry.path];
                    entry
                }),
        );
        root.selected = selected
            .and_then(|key| root.entries.iter().position(|entry| entry.key() == key))
            .unwrap_or(root.selected.min(root.entries.len().saturating_sub(1)));
        root.offset = 0;
        root.total = root.entries.len();
    }

    fn kind(&self) -> LibraryPage {
        self.levels.last().expect("Browser root").kind
    }

    fn changed_query(&mut self) {
        self.query_epoch = self.query_epoch.wrapping_add(1);
        let query = self.query.trim().to_lowercase();
        self.query_due = None;
        if self.kind() != LibraryPage::Search {
            return;
        }
        if query.is_empty() {
            let level = self.levels.last_mut().expect("Browser root");
            level.entries.clear();
            level.selected = 0;
            level.offset = 0;
            level.total = 0;
            return;
        }
        if let Some(ids) = self.queries.get(&query) {
            let entries = self.index.as_ref().expect("Query index").materialize(&ids);
            let level = self.levels.last_mut().expect("Browser root");
            level.entries = entries;
            level.selected = 0;
            level.offset = 0;
            level.total = level.entries.len();
        } else {
            self.query_due = Some(crate::system::monotonic());
        }
    }

    pub fn search_job(&mut self) -> Option<SearchJob> {
        if self.search_running
            || self
                .query_due
                .is_none_or(|time| crate::system::monotonic() < time)
            || self.kind() != LibraryPage::Search
        {
            return None;
        }
        let index = self.index.clone()?;
        self.query_due = None;
        self.search_running = true;
        Some(SearchJob {
            index,
            query: self.query.trim().to_lowercase(),
            epoch: self.query_epoch,
            candidates: self.queries.candidates(&self.query.trim().to_lowercase()),
        })
    }

    pub fn searched(&mut self, result: SearchResult) {
        self.search_running = false;
        if result.epoch != self.query_epoch
            || self.kind() != LibraryPage::Search
            || self
                .index
                .as_ref()
                .is_none_or(|index| !Arc::ptr_eq(index, &result.index))
        {
            return;
        }
        let entries = result.index.materialize(&result.ids);
        self.queries.put(result.query, result.ids);
        let level = self.levels.last_mut().expect("Browser root");
        level.entries = entries;
        level.selected = 0;
        level.offset = 0;
        level.total = level.entries.len();
    }

    fn favorites(&mut self) {
        let level = self.levels.last_mut().expect("Browser root");
        let selected = level
            .entries
            .get(level.selected)
            .map(|entry| library::key(&entry.path));
        let mut entries: Vec<_> = self
            .personal
            .favorites
            .values()
            .filter(|entry| self.launcher || entry.netplay())
            .cloned()
            .collect();
        if let Some(index) = &self.index {
            for entry in &mut entries {
                if let Some(current) = index.entry(&entry.path) {
                    *entry = current.clone();
                }
            }
        }
        entries.sort_by_key(|entry| {
            (
                if self.personal.favorite_sort == library::Sort::Recent {
                    std::cmp::Reverse(
                        self.personal
                            .recent
                            .get(&library::key(&entry.path))
                            .copied()
                            .unwrap_or(0),
                    )
                } else {
                    std::cmp::Reverse(0)
                },
                entry.name.to_lowercase(),
                entry.path.clone(),
            )
        });
        if self.launcher {
            entries.push(Entry {
                path: PathBuf::from("nearby:library"),
                name: "游戏库".into(),
                directory: true,
                frontend: String::new(),
                core: String::new(),
                platform: "library".into(),
                count: 0,
                image: None,
                extensions: String::new(),
            });
        }
        level.selected = selected
            .and_then(|key| {
                entries
                    .iter()
                    .position(|entry| library::key(&entry.path) == key)
            })
            .unwrap_or(level.selected.min(entries.len().saturating_sub(1)));
        level.entries = entries;
        level.offset = 0;
        level.total = level.entries.len();
    }

    fn refresh_recent(&mut self) {
        let Some(index) = &self.index else { return };
        for level in &mut self.levels {
            if level
                .path
                .as_ref()
                .is_some_and(|entry| entry.platform == "recent")
            {
                let selected = level.entries.get(level.selected).map(Entry::key);
                level.entries = index.recent(&self.personal.recent);
                level.offset = 0;
                level.total = level.entries.len();
                level.selected = selected
                    .and_then(|key| level.entries.iter().position(|entry| entry.key() == key))
                    .unwrap_or(0);
            }
        }
    }

    fn available(&self, entry: &Entry) -> bool {
        entry.path.canonicalize().is_ok_and(|path| {
            path == entry.path
                && self.roots.iter().any(|root| path.starts_with(root))
                && path.exists()
        })
    }

    fn child_entry(&self, directory: &Entry, entry: fs::DirEntry) -> Option<Entry> {
        let path = entry.path().canonicalize().ok()?;
        if !self.roots.iter().any(|root| path.starts_with(root))
            || !(path.is_dir() || directory.accepts(&path))
            || path.is_dir()
                && self
                    .index
                    .as_ref()
                    .and_then(|index| index.counts.get(&path))
                    .copied()
                    .unwrap_or(0)
                    == 0
        {
            return None;
        }
        let metadata = self
            .index
            .as_ref()
            .and_then(|index| index.metadata.get(&path));
        let name = metadata
            .filter(|value| !value.name.is_empty())
            .map(|value| value.name.clone())
            .unwrap_or_else(|| crate::flow::label(&entry.file_name().to_string_lossy()));
        if name.starts_with('.') {
            return None;
        }
        let count = self
            .index
            .as_ref()
            .and_then(|index| index.counts.get(&path))
            .copied()
            .unwrap_or(0);
        Some(Entry {
            directory: path.is_dir(),
            path,
            name,
            frontend: directory.frontend.clone(),
            core: directory.core.clone(),
            platform: directory.platform.clone(),
            count,
            image: metadata.and_then(|value| value.image.clone()),
            extensions: directory.extensions.clone(),
        })
    }

    fn children_window(
        &self,
        directory: &Entry,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Entry>, usize)> {
        if let Some(window) = self
            .index
            .as_ref()
            .and_then(|index| index.children_window(&directory.path, offset, limit))
        {
            return Ok(window);
        }
        let capacity = offset.saturating_add(limit).max(1);
        let mut retained = BTreeMap::new();
        let mut total = 0;
        for entry in fs::read_dir(&directory.path)?.filter_map(|entry| entry.ok()) {
            let Some(child) = self.child_entry(directory, entry) else {
                continue;
            };
            total += 1;
            let key = (
                !child.directory,
                child.name.to_lowercase(),
                child.path.clone(),
            );
            retained.insert(key, child);
            if retained.len() > capacity {
                if let Some(last) = retained.keys().next_back().cloned() {
                    retained.remove(&last);
                }
            }
        }
        let window = retained
            .into_values()
            .skip(offset.min(total))
            .take(limit)
            .collect();
        Ok((window, total))
    }

    fn reload_directory_window(&mut self, level_index: usize, selected: usize) -> Result<()> {
        let directory = self
            .levels
            .get(level_index)
            .filter(|level| level.kind == LibraryPage::Directory)
            .and_then(|level| level.path.clone())
            .ok_or("Missing directory level")?;
        let offset = if let Some(index) = &self.index {
            let total = index
                .children_window(&directory.path, 0, 0)
                .map(|(_, total)| total)
                .unwrap_or(0);
            selected
                .saturating_sub(DIRECTORY_WINDOW / 2)
                .min(total.saturating_sub(DIRECTORY_WINDOW))
        } else {
            self.count_children(&directory)
                .map(|total| {
                    selected
                        .saturating_sub(DIRECTORY_WINDOW / 2)
                        .min(total.saturating_sub(DIRECTORY_WINDOW))
                })
                .unwrap_or(0)
        };
        let (entries, total) = self.children_window(&directory, offset, DIRECTORY_WINDOW)?;
        let level = self
            .levels
            .get_mut(level_index)
            .ok_or("Missing directory level")?;
        level.offset = offset.min(total);
        level.total = total;
        level.entries = entries;
        level.selected = selected
            .saturating_sub(level.offset)
            .min(level.entries.len().saturating_sub(1));
        Ok(())
    }

    fn count_children(&self, directory: &Entry) -> Result<usize> {
        Ok(fs::read_dir(&directory.path)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| self.child_entry(directory, entry))
            .count())
    }

    pub fn dispatch(&mut self, action: &str) -> Result<Option<Entry>> {
        let action = if action == "start" && self.kind() != LibraryPage::Search {
            "confirm"
        } else {
            action
        };
        if self.busy {
            if action == "back" {
                self.canceled = true;
                self.status = "正在取消游戏核对...".into();
            }
            return Ok(None);
        }
        self.status.clear();
        if action == "auxiliary"
            && (self.kind() == LibraryPage::Platforms
                || self.launcher && self.kind() == LibraryPage::Directory && self.game().is_none())
        {
            self.levels.push(Level {
                kind: LibraryPage::Search,
                path: None,
                entries: Vec::new(),
                selected: 0,
                offset: 0,
                total: 0,
            });
            self.keyboard = true;
            self.changed_query();
            self.input_epoch = crate::system::monotonic();
            return Ok(None);
        }
        if action == "auxiliary" && self.kind() == LibraryPage::Favorites {
            match library::Library::sort(Path::new(context::INSTALL)) {
                Ok(value) => {
                    self.personal = value;
                    self.favorites();
                }
                Err(_) => self.status = "无法保存排序，请重试。".into(),
            }
            return Ok(None);
        }
        if self.kind() == LibraryPage::Search {
            if action == "start" {
                self.keyboard = !self.keyboard;
                return Ok(None);
            }
            if action == "auxiliary" {
                if self.keyboard {
                    self.query.pop();
                    self.changed_query();
                } else {
                    self.keyboard = true;
                }
                return Ok(None);
            }
            if self.keyboard {
                match action {
                    "back" => self.keyboard = false,
                    "left" => {
                        self.key_selected =
                            self.key_selected / 10 * 10 + (self.key_selected + 9) % 10
                    }
                    "right" => {
                        self.key_selected =
                            self.key_selected / 10 * 10 + (self.key_selected + 1) % 10
                    }
                    "up" => self.key_selected = (self.key_selected + 30) % 40,
                    "down" => self.key_selected = (self.key_selected + 10) % 40,
                    "confirm" => {
                        match KEYS[self.key_selected] {
                            "退格" => {
                                self.query.pop();
                            }
                            "清空" => self.query.clear(),
                            "结果" => {
                                self.keyboard = false;
                                return Ok(None);
                            }
                            text if self.query.chars().count() < 64 => {
                                self.query.push_str(if text == "空格" { " " } else { text })
                            }
                            _ => {}
                        }
                        self.changed_query();
                    }
                    _ => {}
                }
                return Ok(None);
            }
        }
        if self.kind() == LibraryPage::Search
            && self.query_due.is_some()
            && matches!(action, "confirm" | "refresh")
        {
            return Ok(None);
        }
        if self.kind() == LibraryPage::Search
            && self.search_running
            && matches!(action, "confirm" | "refresh")
        {
            return Ok(None);
        }
        if action == "refresh" && self.kind() != LibraryPage::Platforms {
            if let Some(entry) = self
                .levels
                .last()
                .and_then(|level| level.entries.get(level.selected))
                .filter(|entry| !entry.directory)
                .cloned()
            {
                match library::Library::toggle(Path::new(context::INSTALL), &entry) {
                    Ok(value) => {
                        self.personal = value;
                        if self.kind() == LibraryPage::Favorites {
                            self.favorites();
                        }
                    }
                    Err(_) => self.status = "无法保存收藏，请重试。".into(),
                }
            }
            return Ok(None);
        }
        if action == "back" {
            self.input_epoch = crate::system::monotonic();
            self.query_epoch = self.query_epoch.wrapping_add(1);
            self.query_due = None;
            if self.levels.len() > 1 {
                self.levels.pop();
            } else {
                self.exit = true;
            }
            return Ok(None);
        }
        let favorites = self.kind() == LibraryPage::Favorites;
        if favorites && action == "shelf_home" {
            self.input_epoch = crate::system::monotonic();
            if let Some(level) = self.levels.last_mut() {
                level.selected = 0;
            }
            return Ok(None);
        }
        let level_index = self.levels.len().saturating_sub(1);
        let is_directory = self
            .levels
            .get(level_index)
            .is_some_and(|level| level.kind == LibraryPage::Directory);
        if favorites {
            let level = self
                .levels
                .get_mut(level_index)
                .ok_or("Missing game browser level")?;
            level.selected = self
                .shelf
                .navigate(level.selected, level.entries.len(), action);
        }
        let delta = match action {
            _ if favorites => 0,
            "up" => -1,
            "down" => 1,
            "left" | "page_up" => -(self.list_rows as isize),
            "right" | "page_down" => self.list_rows as isize,
            _ => 0,
        };
        if delta != 0 {
            let (current, offset, total) = {
                let level = self
                    .levels
                    .get(level_index)
                    .ok_or("Missing game browser level")?;
                (level.selected, level.offset, level.total)
            };
            let length = total;
            let global = offset + current;
            let target =
                (global as isize + delta).clamp(0, length.saturating_sub(1) as isize) as usize;
            if is_directory {
                let window_offset = target
                    .saturating_sub(DIRECTORY_WINDOW / 2)
                    .min(length.saturating_sub(DIRECTORY_WINDOW));
                if window_offset != offset
                    || target >= offset + self.levels[level_index].entries.len()
                {
                    self.reload_directory_window(level_index, target)?;
                } else if let Some(level) = self.levels.get_mut(level_index) {
                    level.selected = target - offset;
                }
            } else if let Some(level) = self.levels.get_mut(level_index) {
                level.selected = target;
            }
        }
        let level = self
            .levels
            .get_mut(level_index)
            .ok_or("Missing game browser level")?;
        if action != "confirm" || level.entries.is_empty() {
            return Ok(None);
        }
        let selected = level.entries[level.selected].clone();
        if selected.library_link() {
            return Ok(None);
        }
        if selected.platform == "recent" {
            self.levels.push(Level {
                kind: LibraryPage::Directory,
                path: Some(selected),
                entries: Vec::new(),
                selected: 0,
                offset: 0,
                total: 0,
            });
            self.refresh_recent();
            self.input_epoch = crate::system::monotonic();
            return Ok(None);
        }
        if selected.platform == "favorites" {
            self.levels.push(Level {
                kind: LibraryPage::Favorites,
                path: None,
                entries: Vec::new(),
                selected: 0,
                offset: 0,
                total: 0,
            });
            if let Ok(personal) = library::Library::load(Path::new(context::INSTALL)) {
                self.personal = personal;
            }
            self.favorites();
            self.input_epoch = crate::system::monotonic();
            return Ok(None);
        }
        if !self.available(&selected) {
            self.status = "游戏文件暂不可用，请检查存储卡。".into();
            return Ok(None);
        }

        require(
            self.roots
                .iter()
                .any(|root| selected.path.starts_with(root)),
            "Game browser escaped its local roots",
        )?;
        if selected.directory {
            let (entries, total) = match self.children_window(&selected, 0, DIRECTORY_WINDOW) {
                Ok(window) => window,
                Err(_) => {
                    self.status = "无法读取目录，请返回后重试。".into();
                    return Ok(None);
                }
            };
            self.levels.push(Level {
                kind: LibraryPage::Directory,
                path: Some(selected),
                entries,
                selected: 0,
                offset: 0,
                total,
            });
            self.input_epoch = crate::system::monotonic();
            Ok(None)
        } else {
            self.busy = true;
            self.canceled = false;
            self.status = "正在核对游戏...".into();
            Ok(Some(selected))
        }
    }

    pub fn prepared(&mut self, result: std::result::Result<Context, String>) {
        self.input_epoch = crate::system::monotonic();
        self.busy = false;
        if self.canceled {
            self.status.clear();
            self.canceled = false;
            return;
        }
        match result {
            Ok(context) => {
                self.status.clear();
                self.selected_context = Some(context);
            }
            Err(_) => self.status = "无法核对这款游戏，请选择其他游戏。".into(),
        }
    }

    pub fn view(&self) -> Value {
        let level = self.levels.last().expect("Browser always has its root");
        let visible = if self.kind() == LibraryPage::Favorites {
            self.shelf.capacity()
        } else {
            self.list_rows
        };
        let first = if self.kind() == LibraryPage::Favorites {
            self.shelf.first(level.selected)
        } else {
            level
                .selected
                .saturating_sub(visible - 1)
                .min(level.entries.len().saturating_sub(visible))
        };
        let entries: Vec<_> = level.entries.iter().skip(first).take(visible).map(|entry| {
            let detail = if entry.platform=="recent" {format!("{} 款游戏",entry.count)} else if entry.platform=="favorites" { format!("{} 款游戏",self.personal.favorites.len()) } else if entry.directory {
                if self.indexing && self.index.as_ref().is_none_or(|index| !index.counts.contains_key(&entry.path)) {
                    format!("{} · 正在统计", entry.core)
                } else if entry.core.is_empty() {
                    format!("{} 款游戏",entry.count)
                } else {
                    format!("{} · {} 款", entry.core, entry.count)
                }
            } else {
                entry.core.clone()
            };
            json!({
                "key":if entry.platform=="favorites" {"favorites".into()} else {library::key(&entry.path)},
                "favorite":self.personal.favorites.contains_key(&library::key(&entry.path)),
                "artwork":entry.image.as_ref().map(|path|path.to_string_lossy().into_owned()).unwrap_or_default(),
                "available":!(self.kind()==LibraryPage::Search && (self.query_due.is_some() || self.search_running)) && (matches!(entry.platform.as_str(),"favorites"|"recent"|"library") || self.available(entry)),
                "name":entry.name,"detail":detail,"icon":entry.platform,"directory":entry.directory
            })
        }).collect();
        let status = if self.kind() == LibraryPage::Search
            && (self.query_due.is_some() || self.search_running)
        {
            "正在搜索本机游戏".into()
        } else if self.kind() == LibraryPage::Search && self.query.trim().is_empty() {
            "名称 / 文件名 / 拼音首字母".into()
        } else if self.kind() == LibraryPage::Favorites && level.entries.is_empty() {
            if self.launcher {
                String::new()
            } else {
                "在游戏列表按 X，把喜欢的游戏放在这里".into()
            }
        } else if !self.status.is_empty() {
            self.status.clone()
        } else if self.indexing && self.index.is_none() {
            "正在整理本机游戏".into()
        } else {
            String::new()
        };
        let artwork_neighbors: Vec<_> =
            [level.selected.checked_sub(1), level.selected.checked_add(1)]
                .into_iter()
                .flatten()
                .filter_map(|i| level.entries.get(i))
                .filter_map(|entry| entry.image.as_ref())
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
        let load = if self.busy
            || self.indexing && self.index.is_none()
            || self.kind() == LibraryPage::Search
                && (self.query_due.is_some() || self.search_running)
        {
            nearby_menu::LibraryLoad::Loading
        } else if !self.status.is_empty() {
            nearby_menu::LibraryLoad::Failed {
                message: self.status.clone(),
            }
        } else {
            nearby_menu::LibraryLoad::Ready
        };
        json!({
            "platforms":self.kind()==LibraryPage::Platforms,
            "library":{"kind":level.kind,"load":load,"query":self.query,"keyboard":self.keyboard && self.kind()==LibraryPage::Search,"key_selected":self.key_selected,"row_selected":level.selected.saturating_sub(first),"sort":self.personal.favorite_sort,"position":if level.entries.is_empty(){0}else{level.offset+level.selected+1},"total":level.total},
            "artwork":level.entries.get(level.selected).and_then(|entry|entry.image.as_ref()).map(|path|path.to_string_lossy().into_owned()).unwrap_or_default(),
            "artwork_neighbors":artwork_neighbors,
            "page":"games","title":match self.kind() {LibraryPage::Platforms=>"选择游戏平台",LibraryPage::Favorites=>"收藏夹",LibraryPage::Search=>"搜索游戏",LibraryPage::Directory=>"选择游戏"},
            "core":level.path.as_ref().map(|entry|entry.name.clone()).unwrap_or_else(||match self.kind() {LibraryPage::Favorites=>"喜欢的游戏，随时出发",LibraryPage::Search=>"查找本机游戏",_=>"本机游戏"}.into()),
            "selected":if self.keyboard && self.kind()==LibraryPage::Search {100+self.key_selected} else {level.selected.saturating_sub(first)},"entries":entries,"rooms":[],"room_name":"",
            "ready":false,"busy":self.busy,"refreshing":self.indexing,"status":status,"phase":"idle",
            "back_label":if self.busy {"取消"} else if self.levels.len()==1 {"返回"} else {"上级"}
        })
    }
}

fn local_platforms() -> Result<(Vec<PathBuf>, Vec<Entry>)> {
    let roots: Vec<_> = ["/roms", "/roms2"]
        .iter()
        .filter_map(|root| Path::new(root).canonicalize().ok())
        .collect();
    let mut entries = Vec::new();
    for root in &roots {
        for (platform, name) in PLATFORMS {
            let directory = root.join(platform);
            let Ok(path) = directory.canonicalize() else {
                continue;
            };
            if !path.is_dir() || !roots.iter().any(|root| path.starts_with(root)) {
                continue;
            }
            let choice =
                if let Some(core) = crate::handheld_link::preferred(&directory.join("game.zip")) {
                    Some(("retroarch", core))
                } else {
                    crate::core_choice::core_order(&directory.join("game.zip"))
                        .iter()
                        .find(|choice| {
                            context::resolve_core(choice.frontend, choice.id)
                                .is_ok_and(|path| path.is_file())
                        })
                        .map(|choice| (choice.frontend, choice.id))
                };
            let (frontend, core) = choice
                .filter(|(frontend, core)| {
                    context::resolve_core(frontend, core).is_ok_and(|path| path.is_file())
                })
                .unwrap_or(("", ""));
            entries.push(Entry {
                path,
                name: format!(
                    "{}{}",
                    name,
                    if root.ends_with("roms2") {
                        " · 卡 2"
                    } else {
                        ""
                    }
                ),
                directory: true,
                frontend: frontend.into(),
                core: core.into(),
                platform: (*platform).into(),
                count: 0,
                image: None,
                extensions: String::new(),
            });
        }
    }
    Ok((roots, entries))
}
