use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::context::{self, Context};
use crate::{Result, require};

#[derive(Clone)]
pub struct Entry {
    path: PathBuf,
    name: String,
    directory: bool,
    frontend: String,
    core: String,
}
impl Entry {
    pub fn prepare(&self) -> Result<Context> {
        let library = context::config(&self.frontend)?
            .join("cores")
            .join(format!("{}_libretro.so", self.core));
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

struct Level {
    path: Option<Entry>,
    entries: Vec<Entry>,
    selected: usize,
}

pub struct Browser {
    roots: Vec<PathBuf>,
    levels: Vec<Level>,
    pub busy: bool,
    pub input_epoch: f64,
    canceled: bool,
    pub exit: bool,
    pub selected_context: Option<Context>,
    status: String,
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
    pub fn new() -> Result<Self> {
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
                let choice = if let Some(core) =
                    crate::handheld_link::preferred(&directory.join("game.zip"))
                {
                    Some(("retroarch", core))
                } else {
                    crate::core_choice::candidates(&directory.join("game.zip"))
                        .iter()
                        .find(|choice| {
                            context::config(choice.frontend).is_ok_and(|root| {
                                root.join("cores")
                                    .join(format!("{}_libretro.so", choice.id))
                                    .is_file()
                            })
                        })
                        .map(|choice| (choice.frontend, choice.id))
                };
                if let Some((frontend, core)) = choice {
                    if !context::config(frontend)?
                        .join("cores")
                        .join(format!("{core}_libretro.so"))
                        .is_file()
                    {
                        continue;
                    }
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
                    });
                }
            }
        }
        Ok(Self {
            roots,
            levels: vec![Level {
                path: None,
                entries,
                selected: 0,
            }],
            busy: false,
            input_epoch: crate::system::monotonic(),
            canceled: false,
            exit: false,
            selected_context: None,
            status: String::new(),
        })
    }

    fn children(&self, directory: &Entry) -> Result<Vec<Entry>> {
        let mut entries: Vec<_> = fs::read_dir(&directory.path)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path().canonicalize().ok()?;
                if !self.roots.iter().any(|root| path.starts_with(root))
                    || !(path.is_dir() || game_file(&path))
                {
                    return None;
                }
                let name = crate::flow::label(&entry.file_name().to_string_lossy());
                if name.starts_with('.') {
                    return None;
                }
                Some(Entry {
                    directory: path.is_dir(),
                    path,
                    name,
                    frontend: directory.frontend.clone(),
                    core: directory.core.clone(),
                })
            })
            .collect();
        entries.sort_by_key(|entry| (!entry.directory, entry.name.to_lowercase()));
        Ok(entries)
    }

    pub fn dispatch(&mut self, action: &str) -> Result<Option<Entry>> {
        if self.busy {
            if action == "back" {
                self.canceled = true;
                self.status = "正在取消游戏核对...".into();
            }
            return Ok(None);
        }
        self.status.clear();
        if action == "back" {
            self.input_epoch = crate::system::monotonic();
            if self.levels.len() > 1 {
                self.levels.pop();
            } else {
                self.exit = true;
            }
            return Ok(None);
        }
        let level = self.levels.last_mut().ok_or("Missing game browser level")?;
        let delta = match action {
            "up" => -1,
            "down" => 1,
            "left" | "page_up" => -4,
            "right" | "page_down" => 4,
            _ => 0,
        };
        if delta != 0 {
            level.selected = (level.selected as isize + delta)
                .clamp(0, level.entries.len().saturating_sub(1) as isize)
                as usize;
        }
        if action != "confirm" || level.entries.is_empty() {
            return Ok(None);
        }
        let selected = level.entries[level.selected].clone();
        require(
            self.roots
                .iter()
                .any(|root| selected.path.starts_with(root)),
            "Game browser escaped its local roots",
        )?;
        if selected.directory {
            let entries = match self.children(&selected) {
                Ok(entries) => entries,
                Err(_) => {
                    self.status = "无法读取目录，请返回后重试。".into();
                    return Ok(None);
                }
            };
            self.levels.push(Level {
                path: Some(selected),
                entries,
                selected: 0,
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
        let first = level
            .selected
            .saturating_sub(3)
            .min(level.entries.len().saturating_sub(4));
        let entries: Vec<_> = level.entries.iter().skip(first).take(4).map(|entry| json!({"name":entry.name,"detail":if entry.directory { "打开列表" } else { "选择游戏" },"directory":entry.directory})).collect();
        json!({"page":"games","title":if self.levels.len()==1 { "选择游戏平台" } else { "选择游戏" },"core":level.path.as_ref().map(|entry|entry.name.clone()).unwrap_or_else(||"本机游戏".into()),"selected":level.selected.saturating_sub(first),"entries":entries,"rooms":[],"room_name":"","ready":false,"busy":self.busy,"refreshing":false,"status":if self.status.is_empty() { format!("{} / {}",if level.entries.is_empty(){0}else{level.selected+1},level.entries.len()) } else {self.status.clone()},"phase":"idle","back_label":if self.busy {"取消"} else if self.levels.len()==1 {"退出"} else {"上级"}})
    }
}
