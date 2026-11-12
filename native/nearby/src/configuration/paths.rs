use super::files::{self, Graph, Values};
use super::resolve::{Environment, argument, get, yes};
use super::{StartupSlot, StateSlots};
use crate::Result;
use howff_menu::details::Fact;
use std::path::{Path, PathBuf};

struct Directories<'a> {
    graph: &'a mut Graph,
    planned: Vec<PathBuf>,
}
impl Directories<'_> {
    fn exists(&mut self, path: &Path) -> Result<bool> {
        Ok(self.planned.iter().any(|created| created.starts_with(path))
            || self.graph.directory(path)?)
    }
    fn create(&mut self, path: &Path) -> Result<bool> {
        if self.exists(path)? || self.graph.can_create_directory(path)? {
            self.planned.push(path.into());
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn path(
    env: &Environment,
    values: &Values,
    overrides_loaded: bool,
    directories: &mut Directories<'_>,
    state: bool,
) -> Result<PathBuf> {
    let (key, content, sort, folder, short, long, ext, default) = if state {
        (
            "savestate_directory",
            "savestates_in_content_dir",
            "sort_savestates_enable",
            "sort_savestates_by_content_enable",
            "-S",
            "--savestate",
            "state",
            "states",
        )
    } else {
        (
            "savefile_directory",
            "savefiles_in_content_dir",
            "sort_savefiles_enable",
            "sort_savefiles_by_content_enable",
            "-s",
            "--save",
            "srm",
            "saves",
        )
    };
    let defaults = env.config_home.join("retroarch").join(default);
    let explicit =
        argument(&env.argv, short, long).map(|value| files::expand(&value, &env.home, &env.cwd));
    let game_dir = env.game.parent().unwrap_or(&env.cwd);
    let name = format!(
        "{}.{}",
        env.game.file_stem().unwrap_or_default().to_string_lossy(),
        ext
    );
    let initial = explicit.clone().unwrap_or_else(|| game_dir.join(&name));
    let mut intermediate = defaults.clone();
    if let Some(explicit) = &explicit
        && directories.exists(explicit)?
    {
        intermediate = explicit.clone();
    }
    if (explicit.is_none() || overrides_loaded)
        && let Some(value) = get(values, key).filter(|value| !value.is_empty())
    {
        let configured = if value == "default" {
            defaults
        } else {
            files::expand(value, &env.home, &env.cwd)
        };
        if directories.exists(&configured)? {
            intermediate = configured;
        }
    }
    if yes(values, content, false) {
        intermediate = game_dir.into();
    }
    let mut sorted = intermediate.clone();
    if yes(values, folder, false)
        && let Some(name) = game_dir.file_name()
    {
        sorted.push(name);
    }
    if yes(values, sort, false) {
        sorted.push(&env.core);
    }
    let directory = if sorted != intermediate && directories.create(&sorted)? {
        sorted
    } else {
        intermediate
    };
    if directories.exists(&directory)? {
        Ok(directory.join(name))
    } else if directories.exists(&initial)? {
        Ok(initial.join(name))
    } else {
        Ok(initial)
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct ResolvedSavePaths {
    pub sram: PathBuf,
    pub state: PathBuf,
    pub slots: StateSlots,
    pub content_directory: bool,
}
impl ResolvedSavePaths {
    pub fn slot_path(&self, slot: i32) -> PathBuf {
        match slot {
            i32::MIN..=-1 => format!("{}.auto", self.state.display()).into(),
            0 => self.state.clone(),
            _ => format!("{}{slot}", self.state.display()).into(),
        }
    }
    pub fn facts(&self) -> Vec<Fact> {
        vec![
            Fact::new("SRAM 文件", self.sram.to_string_lossy()),
            Fact::new(
                "SRAM 状态",
                if self.sram.is_file() {
                    "已找到"
                } else {
                    "尚未生成"
                },
            ),
            Fact::new("配置槽位", format!("槽位 {}", self.slots.configured)),
            Fact::new(
                "显式载入槽位",
                self.slots
                    .entry
                    .map(|slot| format!("槽位 {slot}"))
                    .unwrap_or_else(|| "—".into()),
            ),
            Fact::new(
                "启动后槽位",
                match self.slots.startup() {
                    StartupSlot::Fixed(slot) => format!("槽位 {slot}"),
                    StartupSlot::AutoIndexed => "前端自动编号".into(),
                },
            ),
            Fact::new(
                "即时存档",
                match self.slots.startup() {
                    StartupSlot::Fixed(slot) => self.slot_path(slot).to_string_lossy().into_owned(),
                    StartupSlot::AutoIndexed => "前端自动编号".into(),
                },
            ),
            Fact::new(
                "即时存档目录",
                self.state
                    .parent()
                    .unwrap_or(Path::new("."))
                    .to_string_lossy(),
            ),
            Fact::new("自动存档", self.slot_path(-1).to_string_lossy()),
            Fact::new(
                "目录规则",
                if self.content_directory {
                    "游戏目录"
                } else {
                    "配置目录"
                },
            ),
            Fact::new("联机存档", "联机会话内独立保存"),
        ]
    }
}

pub(super) fn resolve(
    env: &Environment,
    values: &Values,
    overrides_loaded: bool,
    graph: &mut Graph,
) -> Result<ResolvedSavePaths> {
    let mut directories = Directories {
        graph,
        planned: Vec::new(),
    };
    // Frontend startup creates both default directories before reading configuration.
    for name in ["saves", "states"] {
        directories.create(&env.config_home.join("retroarch").join(name))?;
    }
    let sram = path(env, values, overrides_loaded, &mut directories, false)?;
    let state = path(env, values, overrides_loaded, &mut directories, true)?;
    let slots = StateSlots::resolve(
        get(values, "state_slot")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(0),
        &env.argv,
        yes(values, "savestate_auto_index", false),
    );
    Ok(ResolvedSavePaths {
        sram,
        state,
        slots,
        content_directory: yes(values, "savefiles_in_content_dir", false),
    })
}
