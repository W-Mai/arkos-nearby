use super::files::{self, Graph, Values};
use crate::{Result, require};
use howff_menu::configuration::{Change, Choice, Document, Field, Id, Scope, Section, Value};
use howff_menu::details::Fact;
use howff_platform::arkos;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Environment {
    pub home: PathBuf,
    pub cwd: PathBuf,
    pub root: PathBuf,
    pub config_home: PathBuf,
    pub access: Option<files::Access>,
    pub core: String,
    pub game: PathBuf,
    pub argv: Vec<String>,
    pub controller: Option<String>,
}

#[derive(Clone)]
pub struct Snapshot {
    pub document: Document,
    pub saves: super::ResolvedSavePaths,
    pub graph: Graph,
    env: Environment,
    configs: [Values; 3],
    options: [Values; 3],
    remaps: [Values; 3],
    inherited_configs: [Values; 3],
    inherited_options: [Values; 3],
    inherited_remaps: [Values; 3],
    config_targets: [PathBuf; 3],
    option_targets: [PathBuf; 3],
    remap_targets: [PathBuf; 3],
    policy: Option<String>,
}

pub(super) fn get<'a>(values: &'a Values, key: &str) -> Option<&'a str> {
    values.get(key).map(|v| v.value.as_str())
}
pub(super) fn yes(values: &Values, key: &str, default: bool) -> bool {
    get(values, key).map(|s| s == "true").unwrap_or(default)
}
fn overlay(target: &mut Values, additional: Values) {
    target.extend(additional);
}
fn scoped_name(name: &str) -> Result<&str> {
    require(
        !name.is_empty() && !matches!(name, "." | "..") && !name.contains(['/', '\\', '\0']),
        "核心或内容名称不可用",
    )?;
    Ok(name)
}
pub(super) fn argument(args: &[String], short: &str, long: &str) -> Option<String> {
    args.iter()
        .enumerate()
        .filter_map(|(index, arg)| {
            if arg == short || arg == long {
                args.get(index + 1).cloned()
            } else {
                arg.strip_prefix(&format!("{long}="))
                    .or_else(|| {
                        (short.len() == 2)
                            .then(|| arg.strip_prefix(short))
                            .flatten()
                    })
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            }
        })
        .next_back()
}
fn config_path(value: Option<&str>, fallback: &Path, env: &Environment) -> PathBuf {
    value
        .filter(|s| !s.is_empty() && *s != "default")
        .map(|s| files::expand(s, &env.home, &env.cwd))
        .unwrap_or_else(|| fallback.into())
}

impl Snapshot {
    pub fn environment(context: &crate::context::Context) -> Result<Environment> {
        let home = PathBuf::from(arkos::home::ROOT);
        let controller = std::fs::canonicalize(howff_platform::arkos::input::JOYSTICK.as_str())
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| Path::new("/sys/class/input").join(name).join("device/name"))
            })
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| text.trim().to_string());
        Ok(Environment {
            access: Some(files::Access {
                uid: crate::session::cmd(&["id", "-u", "ark"])?.trim().parse()?,
                groups: crate::session::cmd(&["id", "-G", "ark"])?
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()?,
            }),
            config_home: std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config")),
            home: home.clone(),
            cwd: home,
            root: crate::context::config(&context.frontend)?,
            core: context.identity.core.name.clone(),
            game: context.game_path.clone(),
            argv: context.argv.clone(),
            controller,
        })
    }
    pub fn settings(&self) -> crate::game::Settings {
        let text = |values: &Values| {
            values
                .iter()
                .map(|(key, value)| format!("{key} = \"{}\"\n", value.value))
                .collect::<String>()
        };
        crate::game::Settings {
            base: text(&self.configs[0]),
            options: text(&self.options[0]).into_bytes(),
            remap: text(
                &self.remaps[0]
                    .iter()
                    .filter(|(key, _)| {
                        key.strip_prefix("input_player")
                            .and_then(|s| s.split_once("_btn_"))
                            .is_some_and(|(port, _)| {
                                port.parse::<usize>()
                                    .is_ok_and(|port| (1..=2).contains(&port))
                            })
                    })
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        }
    }
    pub fn load(
        env: Environment,
        document: Document,
        definitions: Vec<(String, String)>,
    ) -> Result<Self> {
        Self::load_graph(env, document, definitions, Graph::default())
    }
    fn load_graph(
        env: Environment,
        mut document: Document,
        definitions: Vec<(String, String)>,
        mut graph: Graph,
    ) -> Result<Self> {
        graph.set_access(env.access.clone());
        scoped_name(&env.core)?;
        let title = env
            .game
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("内容文件名不可用")?;
        scoped_name(title)?;
        let folder = env
            .game
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .ok_or("内容目录不可用")?;
        scoped_name(folder)?;
        let base = config_path(
            argument(&env.argv, "-c", "--config").as_deref(),
            &env.root.join("retroarch.cfg"),
            &env,
        );
        let mut frontend = graph.load(&base, &env.home)?;
        if let Some(append) = argument(&env.argv, "--appendconfig", "--appendconfig") {
            for path in append.split('|').filter(|s| !s.is_empty()) {
                overlay(
                    &mut frontend,
                    graph.load(&files::expand(path, &env.home, &env.cwd), &env.home)?,
                );
            }
        }
        let directory = config_path(
            get(&frontend, "rgui_config_directory"),
            &env.root.join("config"),
            &env,
        );
        let core_cfg = directory.join(&env.core).join(format!("{}.cfg", env.core));
        let folder_cfg = directory.join(&env.core).join(format!("{folder}.cfg"));
        let game_cfg = directory.join(&env.core).join(format!("{title}.cfg"));
        let frontend_inherited = graph.included(&base, &env.home)?;
        let mut core = frontend.clone();
        let mut game = frontend.clone();
        let mut core_inherited = frontend.clone();
        let mut game_inherited = frontend.clone();
        let overrides = yes(&frontend, "auto_overrides_enable", true);
        if overrides {
            overlay(&mut core_inherited, graph.included(&core_cfg, &env.home)?);
            overlay(&mut core, graph.load(&core_cfg, &env.home)?);
            game = core.clone();
            overlay(&mut game, graph.load(&folder_cfg, &env.home)?);
            game_inherited = game.clone();
            overlay(&mut game_inherited, graph.included(&game_cfg, &env.home)?);
            overlay(&mut game, graph.load(&game_cfg, &env.home)?);
        }
        let inherited_configs = [game_inherited, core_inherited, frontend_inherited];
        let overrides_loaded = overrides
            && [&core_cfg, &folder_cfg, &game_cfg]
                .iter()
                .any(|path| graph.exists(path));
        let configs = [game, core, frontend];
        let global_opt = config_path(
            get(&configs[0], "core_options_path"),
            &base.with_file_name("retroarch-core-options.cfg"),
            &env,
        );
        let core_opt = directory.join(&env.core).join(format!("{}.opt", env.core));
        let folder_opt = directory.join(&env.core).join(format!("{folder}.opt"));
        let game_opt = directory.join(&env.core).join(format!("{title}.opt"));
        let global = graph.load(&global_opt, &env.home)?;
        let per_core = !yes(&configs[0], "global_core_options", true);
        let core_options = if per_core {
            let values = graph.load(&core_opt, &env.home)?;
            if graph.exists(&core_opt) {
                values
            } else {
                global.clone()
            }
        } else {
            global.clone()
        };
        let mut game_options = core_options.clone();
        let mut game_option_inherited = core_options.clone();
        if yes(&configs[0], "game_specific_options", true) {
            let game_values = graph.load(&game_opt, &env.home)?;
            let folder_values = graph.load(&folder_opt, &env.home)?;
            if graph.exists(&folder_opt) {
                game_option_inherited = folder_values.clone();
            }
            if graph.exists(&game_opt) {
                game_options = game_values;
            } else if graph.exists(&folder_opt) {
                game_options = folder_values;
            }
        }
        let options = [game_options, core_options, global];
        let inherited_options = [game_option_inherited, Values::new(), Values::new()];
        let remap_root = config_path(
            get(&configs[0], "input_remapping_directory"),
            &directory.join("remaps"),
            &env,
        );
        let mut remap_directory = remap_root.join(&env.core);
        let controller_sorted = yes(&configs[0], "input_remap_sort_by_controller_enable", false);
        if controller_sorted && let Some(controller) = &env.controller {
            let name: String = controller
                .chars()
                .map(|c| {
                    if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                        '_'
                    } else {
                        c
                    }
                })
                .collect();
            remap_directory.push(name);
        }
        let core_remap = remap_directory.join(format!("{}.rmp", env.core));
        let folder_remap = remap_directory.join(format!("{folder}.rmp"));
        let game_remap = remap_directory.join(format!("{title}.rmp"));
        let mut remaps = [Values::new(), Values::new(), Values::new()];
        let mut inherited_remaps = [Values::new(), Values::new(), Values::new()];
        if yes(&configs[0], "auto_remaps_enable", true) {
            remaps[1] = graph.load(&core_remap, &env.home)?;
            remaps[0] = remaps[1].clone();
            let folder = graph.load(&folder_remap, &env.home)?;
            let game = graph.load(&game_remap, &env.home)?;
            if graph.exists(&folder_remap) {
                remaps[0] = folder;
            }
            inherited_remaps[0] = remaps[0].clone();
            if graph.exists(&game_remap) {
                remaps[0] = game;
            }
        }
        let config_targets = [game_cfg, core_cfg, base];
        let option_targets = [
            game_opt,
            if per_core {
                core_opt
            } else {
                global_opt.clone()
            },
            global_opt,
        ];
        // Core joypad mappings are distinct from frontend hardware/autoconfiguration binds.
        let remap_targets = [game_remap, core_remap, config_targets[2].clone()];
        for path in config_targets
            .iter()
            .chain(option_targets.iter())
            .chain(remap_targets.iter())
        {
            graph.observe(path)?;
        }
        document.core = env.core.clone();
        let saves = super::paths::resolve(&env, &configs[0], overrides_loaded, &mut graph)?;
        let mut result = Self {
            saves,
            document,
            graph,
            env,
            configs,
            options,
            remaps,
            inherited_configs,
            inherited_options,
            inherited_remaps,
            config_targets,
            option_targets,
            remap_targets,
            policy: None,
        };
        result.fields(definitions, overrides)?;
        for field in &mut result.document.fields {
            if matches!(field.id, Id::Button(_)) {
                field.targets[2].clear();
                if !yes(&result.configs[0], "auto_remaps_enable", true)
                    || (controller_sorted && result.env.controller.is_none())
                {
                    field.targets[0].clear();
                    field.targets[1].clear();
                }
            }
            if matches!(field.id, Id::Option(_))
                && !yes(&result.configs[0], "game_specific_options", true)
            {
                field.targets[0].clear();
            }
        }
        result.document.saves = result.saves.facts();
        result.document.sources = result
            .graph
            .inputs
            .values()
            .filter(|input| input.bytes.is_some())
            .map(|input| Fact::new("读取文件", input.path.to_string_lossy()))
            .collect();
        result.document.revision = result.graph.revision();
        Ok(result)
    }

    fn field(
        &mut self,
        id: Id,
        section: Section,
        label: &str,
        key: &str,
        selection: (Value, Vec<Choice>),
        kind: u8,
    ) {
        let (default, choices) = selection;
        let (sets, targets) = match kind {
            1 => (&self.options, &self.option_targets),
            2 => (&self.remaps, &self.remap_targets),
            _ => (&self.configs, &self.config_targets),
        };
        let read = |index: usize| {
            sets[index]
                .get(key)
                .map(|v| match &default {
                    Value::Bool(_) => Value::Bool(v.value == "true"),
                    Value::Number(fallback) => Value::Number(v.value.parse().unwrap_or(*fallback)),
                    Value::Text(_) => Value::Text(v.value.clone()),
                })
                .unwrap_or_else(|| default.clone())
        };
        let values = std::array::from_fn(read);
        let inherited_sets = match kind {
            1 => &self.inherited_options,
            2 => &self.inherited_remaps,
            _ => &self.inherited_configs,
        };
        let inherited = std::array::from_fn(|index| {
            inherited_sets[index]
                .get(key)
                .map(|entry| match &default {
                    Value::Bool(_) => Value::Bool(entry.value == "true"),
                    Value::Number(v) => Value::Number(entry.value.parse().unwrap_or(*v)),
                    Value::Text(_) => Value::Text(entry.value.clone()),
                })
                .unwrap_or_else(|| default.clone())
        });
        let origins = std::array::from_fn(|index| {
            sets[index]
                .get(key)
                .map(|v| v.source.to_string_lossy().into_owned())
                .unwrap_or_else(|| "默认值".into())
        });
        self.document.fields.push(Field {
            id,
            section,
            label: label.into(),
            choices,
            values,
            origins,
            targets: std::array::from_fn(|i| targets[i].to_string_lossy().into_owned()),
            inherited,
        });
    }
    fn fields(&mut self, definitions: Vec<(String, String)>, overrides: bool) -> Result<()> {
        let bools = || {
            vec![
                Choice {
                    value: Value::Bool(false),
                    label: "关闭".into(),
                },
                Choice {
                    value: Value::Bool(true),
                    label: "开启".into(),
                },
            ]
        };
        self.field(
            Id::IntegerScale,
            Section::Video,
            "整数缩放",
            "video_scale_integer",
            (Value::Bool(false), bools()),
            0,
        );
        self.field(
            Id::Aspect,
            Section::Video,
            "画面比例",
            "aspect_ratio_index",
            (
                Value::Number(22),
                vec![
                    (22, "原始"),
                    (21, "方形像素"),
                    (0, "4:3"),
                    (1, "16:9"),
                    (24, "全屏"),
                ]
                .into_iter()
                .map(|(v, label)| Choice {
                    value: Value::Number(v),
                    label: label.into(),
                })
                .collect(),
            ),
            0,
        );
        self.field(
            Id::AutoLoad,
            Section::Saves,
            "自动载入即时存档",
            "savestate_auto_load",
            (Value::Bool(false), bools()),
            0,
        );
        self.field(
            Id::Slot,
            Section::Saves,
            "配置槽位",
            "state_slot",
            (
                Value::Number(0),
                (0..10)
                    .map(|v| Choice {
                        value: Value::Number(v),
                        label: format!("槽位 {v}"),
                    })
                    .collect(),
            ),
            0,
        );
        for (id, label, key) in [
            (Id::SaveByCore, "SRAM 按核心分类", "sort_savefiles_enable"),
            (
                Id::StateByCore,
                "即时存档按核心分类",
                "sort_savestates_enable",
            ),
            (
                Id::SaveInContent,
                "SRAM 存到游戏目录",
                "savefiles_in_content_dir",
            ),
            (
                Id::StateInContent,
                "即时存档存到游戏目录",
                "savestates_in_content_dir",
            ),
        ] {
            self.field(
                id,
                Section::Saves,
                label,
                key,
                (Value::Bool(false), bools()),
                0,
            );
        }
        for (key, definition) in definitions {
            if !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            {
                continue;
            }
            let Some((label, values)) = definition.split_once(';') else {
                continue;
            };
            let choices: Vec<_> = values
                .trim()
                .split('|')
                .filter(|v| !v.is_empty())
                .map(|v| Choice {
                    value: Value::Text(v.into()),
                    label: v.into(),
                })
                .collect();
            if let Some(first) = choices.first() {
                self.field(
                    Id::Option(key.clone()),
                    Section::Core,
                    label,
                    &key,
                    (first.value.clone(), choices),
                    1,
                );
            }
        }
        for (id, label) in [
            (0, "B"),
            (1, "Y"),
            (2, "SELECT"),
            (3, "START"),
            (4, "↑"),
            (5, "↓"),
            (6, "←"),
            (7, "→"),
            (8, "A"),
            (9, "X"),
            (10, "L"),
            (11, "R"),
            (12, "L2"),
            (13, "R2"),
            (14, "L3"),
            (15, "R3"),
        ] {
            let key = Self::button_key(id);
            let choices = [
                (0, "B"),
                (1, "Y"),
                (2, "SELECT"),
                (3, "START"),
                (4, "↑"),
                (5, "↓"),
                (6, "←"),
                (7, "→"),
                (8, "A"),
                (9, "X"),
                (10, "L"),
                (11, "R"),
                (12, "L2"),
                (13, "R2"),
                (14, "L3"),
                (15, "R3"),
                (-1, "关闭"),
            ]
            .into_iter()
            .map(|(v, label)| Choice {
                value: Value::Number(v),
                label: label.into(),
            })
            .collect();
            self.field(
                Id::Button(id),
                Section::Input,
                label,
                &key,
                (Value::Number(i32::from(id)), choices),
                2,
            );
        }
        if !overrides {
            for field in &mut self.document.fields {
                if !matches!(field.id, Id::Option(_) | Id::Button(_)) {
                    field.targets[0].clear();
                    field.targets[1].clear();
                }
            }
        }
        Ok(())
    }

    fn key(id: &Id) -> String {
        match id {
            Id::IntegerScale => "video_scale_integer".into(),
            Id::Aspect => "aspect_ratio_index".into(),
            Id::AutoLoad => "savestate_auto_load".into(),
            Id::Slot => "state_slot".into(),
            Id::SaveByCore => "sort_savefiles_enable".into(),
            Id::StateByCore => "sort_savestates_enable".into(),
            Id::SaveInContent => "savefiles_in_content_dir".into(),
            Id::StateInContent => "savestates_in_content_dir".into(),
            Id::Option(key) => key.clone(),
            Id::Button(id) => Self::button_key(*id),
        }
    }
    fn button_key(id: u8) -> String {
        let names = [
            "b", "y", "select", "start", "up", "down", "left", "right", "a", "x", "l", "r", "l2",
            "r2", "l3", "r3",
        ];
        format!(
            "input_player1_btn_{}",
            names.get(usize::from(id)).copied().unwrap_or("b")
        )
    }

    fn changes(
        &self,
        scope: Scope,
        changes: &[Change],
    ) -> Result<BTreeMap<PathBuf, BTreeMap<String, Option<String>>>> {
        let mut files = BTreeMap::<PathBuf, BTreeMap<String, Option<String>>>::new();
        for change in changes {
            let field = self
                .document
                .fields
                .iter()
                .find(|field| field.id == change.id)
                .ok_or("设置项已变化，请重新读取")?;
            if let Some(value) = &change.value {
                require(field.accepts(value), "设置值不在核心提供的范围内")?;
            }
            let target = &field.targets[scope.index()];
            require(!target.is_empty(), "当前范围未启用配置覆盖")?;
            if matches!(change.id, Id::Button(_)) {
                require(scope != Scope::Frontend, "按键映射使用游戏或核心范围")?;
            }
            let values = files.entry(PathBuf::from(target)).or_default();
            if matches!(change.id, Id::Option(_) | Id::Button(_)) && !Path::new(target).is_file() {
                let kind_is_option = matches!(change.id, Id::Option(_));
                for field in &self.document.fields {
                    if matches!(field.id, Id::Option(_)) == kind_is_option
                        && matches!(field.id, Id::Option(_) | Id::Button(_))
                    {
                        values
                            .entry(Self::key(&field.id))
                            .or_insert(Some(field.values[scope.index()].raw()));
                    }
                }
                let source = if kind_is_option {
                    &self.options[scope.index()]
                } else {
                    &self.remaps[scope.index()]
                };
                for (key, entry) in source {
                    if !kind_is_option
                        || self.option_targets[0]
                            .parent()
                            .is_some_and(|folder| entry.source.starts_with(folder))
                    {
                        values
                            .entry(key.clone())
                            .or_insert(Some(entry.value.clone()));
                    }
                }
            }
            values.insert(Self::key(&change.id), change.value.as_ref().map(Value::raw));
        }
        Ok(files)
    }
    fn write_plan(
        &self,
        scope: Scope,
        changes: &[Change],
    ) -> Result<Vec<(PathBuf, Option<Vec<u8>>)>> {
        let mut writes = Vec::new();
        for change in changes {
            let field = self
                .document
                .fields
                .iter()
                .find(|field| field.id == change.id)
                .ok_or("设置项已变化，请重新读取")?;
            require(
                !field.targets[scope.index()].is_empty(),
                "当前范围未启用配置覆盖",
            )?;
            require(
                change.value.as_ref().is_none_or(|v| field.accepts(v)),
                "设置值不在核心提供的范围内",
            )?;
        }
        let inherit_options = scope == Scope::Game
            && changes
                .iter()
                .any(|change| matches!(change.id, Id::Option(_)) && change.value.is_none());
        let inherit_remap = changes
            .iter()
            .any(|change| matches!(change.id, Id::Button(_)) && change.value.is_none());
        if inherit_options {
            writes.push((self.option_targets[scope.index()].clone(), None));
        }
        if inherit_remap {
            require(scope != Scope::Frontend, "请选择游戏或核心范围")?;
            writes.push((self.remap_targets[scope.index()].clone(), None));
        }
        let changes: Vec<_> = changes
            .iter()
            .filter(|change| {
                !(inherit_options && matches!(change.id, Id::Option(_))
                    || inherit_remap && matches!(change.id, Id::Button(_)))
            })
            .cloned()
            .collect();
        for (path, changes) in self.changes(scope, &changes)? {
            let input = self.graph.inputs.get(&path).ok_or("设置目标没有修订")?;
            writes.push((
                path,
                Some(files::rewrite(
                    input.bytes.as_deref().unwrap_or_default(),
                    &changes,
                )?),
            ));
        }
        Ok(writes)
    }
    pub fn save(&self, scope: Scope, changes: &[Change], journal: &Path) -> Result<()> {
        self.graph.unchanged()?;
        let writes = self.write_plan(scope, changes)?;
        super::transaction::commit(&self.graph, writes, journal)
    }
    pub fn preview(&self, scope: Scope, changes: &[Change]) -> Result<Vec<Fact>> {
        let graph = Graph::with_preview(self.write_plan(scope, changes)?.into_iter().collect());
        let projected = Self::load_graph(
            self.env.clone(),
            self.clean_document(),
            self.definitions(),
            graph,
        )?;
        Ok(projected.document.saves)
    }
    fn definitions(&self) -> Vec<(String, String)> {
        self.document
            .fields
            .iter()
            .filter_map(|field| {
                if let Id::Option(key) = &field.id {
                    Some((
                        key.clone(),
                        format!(
                            "{}; {}",
                            field.label,
                            field
                                .choices
                                .iter()
                                .map(|choice| choice.value.raw())
                                .collect::<Vec<_>>()
                                .join("|")
                        ),
                    ))
                } else {
                    None
                }
            })
            .collect()
    }
    fn clean_document(&self) -> Document {
        let mut document = self.document.clone();
        document.fields.clear();
        document
    }
    pub fn reload(&self) -> Result<Self> {
        let mut snapshot = Self::load(self.env.clone(), self.clean_document(), self.definitions())?;
        if let Some(policy) = &self.policy {
            snapshot.lock_options(policy);
        }
        Ok(snapshot)
    }
    pub fn lock_options(&mut self, policy: &str) {
        self.policy = Some(policy.into());
        for (key, value) in policy.lines().filter_map(files::assignment) {
            for values in &mut self.options {
                values.insert(
                    key.into(),
                    files::Entry {
                        value: value.clone(),
                        source: "单机运行模式".into(),
                    },
                );
            }
            if let Some(field) = self
                .document
                .fields
                .iter_mut()
                .find(|f| f.id == Id::Option(key.into()))
            {
                field.values = std::array::from_fn(|_| Value::Text(value.clone()));
                field.inherited = field.values.clone();
                field.origins = std::array::from_fn(|_| "单机运行模式".into());
                field.targets = Default::default();
            }
        }
    }
}
