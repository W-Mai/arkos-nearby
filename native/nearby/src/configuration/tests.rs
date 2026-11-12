use super::*;
use howff_menu::configuration::{Change, Document, Id, Scope, Value};
use std::fs;
use std::path::PathBuf;

#[path = "matrix_tests.rs"]
mod matrix;

#[path = "path_tests.rs"]
mod paths;

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nearby-configuration-{}",
            crate::system::random_id().unwrap()
        ));
        fs::create_dir_all(root.join("roms/snes")).unwrap();
        fs::create_dir_all(root.join("config/Snes9x")).unwrap();
        fs::write(root.join("roms/snes/game.zip"), b"content").unwrap();
        Self { root }
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn snapshot(&self) -> Snapshot {
        self.snapshot_args(vec![])
    }
    fn snapshot_args(&self, argv: Vec<String>) -> Snapshot {
        Snapshot::load(
            Environment {
                access: None,
                home: self.root.clone(),
                config_home: self.root.join(".config"),
                cwd: self.root.clone(),
                root: self.root.clone(),
                core: "Snes9x".into(),
                game: self.root.join("roms/snes/game.zip"),
                argv,
                controller: None,
            },
            Document {
                title: "游戏名".into(),
                key: "identity".into(),
                ..Default::default()
            },
            vec![
                ("snes9x_speed".into(), "速度; normal|fast".into()),
                ("snes9x_color".into(), "色彩; default|vivid".into()),
            ],
        )
        .unwrap()
    }
    fn journal(&self) -> PathBuf {
        self.root.join("journal.json")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn field(snapshot: &Snapshot, id: Id) -> &howff_menu::configuration::Field {
    snapshot
        .document
        .fields
        .iter()
        .find(|f| f.id == id)
        .unwrap()
}

#[test]
fn save_paths_and_dependencies_do_not_require_editor_option_descriptors() {
    let f = Fixture::new();
    f.write("retroarch.cfg", "state_slot = \"3\"\nsavefiles_in_content_dir = \"true\"\nsavestates_in_content_dir = \"true\"\n");
    f.write("retroarch-core-options.cfg", "snes9x_speed = \"fast\"\n");
    for argv in [vec![], vec!["--entryslot=7".into()]] {
        let full = f.snapshot_args(argv.clone());
        let paths = Snapshot::load(
            Environment {
                access: None,
                home: f.root.clone(),
                config_home: f.root.join(".config"),
                cwd: f.root.clone(),
                root: f.root.clone(),
                core: "Snes9x".into(),
                game: f.root.join("roms/snes/game.zip"),
                argv,
                controller: None,
            },
            full.document.clone(),
            vec![],
        )
        .unwrap();
        assert_eq!(full.saves, paths.saves);
        assert_eq!(full.document.saves, paths.document.saves);
        assert_eq!(full.graph.revision(), paths.graph.revision());
    }
}

#[test]
fn first_definition_include_order_and_atomic_rewrites_match_configuration_rules() {
    let f = Fixture::new();
    f.write("defaults.cfg", "value = \"included\"\n");
    f.write(
        "order.cfg",
        "#include \"defaults.cfg\"\nvalue = \"local\"\n",
    );
    let mut graph = files::Graph::default();
    let path = f.root.join("order.cfg");
    assert_eq!(
        graph.load(&path, &f.root).unwrap()["value"].value,
        "included"
    );
    let changes = std::collections::BTreeMap::from([("value".into(), Some("edited".into()))]);
    let bytes = files::rewrite(&fs::read(&path).unwrap(), &changes).unwrap();
    fs::write(&path, bytes).unwrap();
    assert!(graph.unchanged().is_err());
    let mut reloaded = files::Graph::default();
    assert_eq!(
        reloaded.load(&path, &f.root).unwrap()["value"].value,
        "edited"
    );
    f.write("cycle.cfg", "#include \"cycle.cfg\"\n");
    assert!(graph.load(&f.root.join("cycle.cfg"), &f.root).is_err());
}

#[test]
fn configuration_scopes_preserve_folder_inheritance_comments_and_unknown_values() {
    let f = Fixture::new();
    f.write("retroarch.cfg", "video_scale_integer = \"false\"\n");
    f.write(
        "config/Snes9x/Snes9x.cfg",
        "video_scale_integer = \"true\"\n",
    );
    f.write(
        "config/Snes9x/snes.cfg",
        "video_scale_integer = \"false\"\n",
    );
    f.write(
        "config/Snes9x/game.cfg",
        "# personal\nvideo_scale_integer = \"true\"\nunknown = \"keep\" # user\n",
    );
    let snapshot = f.snapshot();
    let values = &field(&snapshot, Id::IntegerScale).values;
    assert_eq!(
        values,
        [Value::Bool(true), Value::Bool(true), Value::Bool(false)].as_slice()
    );
    assert_eq!(
        field(&snapshot, Id::IntegerScale).inherited[0],
        Value::Bool(false)
    );
    snapshot
        .save(
            Scope::Game,
            &[Change {
                id: Id::IntegerScale,
                value: None,
            }],
            &f.journal(),
        )
        .unwrap();
    let text = fs::read_to_string(f.root.join("config/Snes9x/game.cfg")).unwrap();
    assert!(text.contains("# personal") && text.contains("unknown = \"keep\" # user"));
    assert_eq!(
        field(&f.snapshot(), Id::IntegerScale).values[0],
        Value::Bool(false)
    );
}

#[test]
fn option_files_capture_complete_core_values_without_copying_other_cores() {
    let f = Fixture::new();
    f.write(
        "retroarch-core-options.cfg",
        "snes9x_speed = \"fast\"\nsnes9x_color = \"default\"\nother_core_option = \"keep\"\n",
    );
    let snapshot = f.snapshot();
    snapshot
        .save(
            Scope::Game,
            &[Change {
                id: Id::Option("snes9x_color".into()),
                value: Some(Value::Text("vivid".into())),
            }],
            &f.journal(),
        )
        .unwrap();
    let target = f.root.join("config/Snes9x/game.opt");
    let text = fs::read_to_string(&target).unwrap();
    assert!(text.contains("snes9x_speed = \"fast\"") && text.contains("snes9x_color = \"vivid\""));
    assert!(!text.contains("other_core"));
    let snapshot = f.snapshot();
    snapshot
        .save(
            Scope::Game,
            &[Change {
                id: Id::Option("snes9x_color".into()),
                value: None,
            }],
            &f.journal(),
        )
        .unwrap();
    assert!(!target.exists());
    assert_eq!(
        field(&f.snapshot(), Id::Option("snes9x_color".into())).values[0],
        Value::Text("default".into())
    );
    assert!(
        fs::read_to_string(f.root.join("retroarch-core-options.cfg"))
            .unwrap()
            .contains("other_core_option")
    );
}

#[test]
fn remaps_select_one_file_and_restore_the_actual_parent_file() {
    let f = Fixture::new();
    f.write(
        "config/remaps/Snes9x/Snes9x.rmp",
        "input_player1_btn_a = \"0\"\n",
    );
    f.write(
        "config/remaps/Snes9x/game.rmp",
        "input_player1_btn_b = \"8\"\n",
    );
    let snapshot = f.snapshot();
    assert_eq!(field(&snapshot, Id::Button(8)).values[0], Value::Number(8));
    assert_eq!(field(&snapshot, Id::Button(8)).values[1], Value::Number(0));
    snapshot
        .save(
            Scope::Game,
            &[Change {
                id: Id::Button(0),
                value: None,
            }],
            &f.journal(),
        )
        .unwrap();
    assert_eq!(
        field(&f.snapshot(), Id::Button(8)).values[0],
        Value::Number(0)
    );
}

#[test]
fn preview_and_committed_paths_agree_without_writing_or_moving_sram() {
    let f = Fixture::new();
    f.write("retroarch.cfg","savefiles_in_content_dir = \"true\"\nsavestates_in_content_dir = \"true\"\nsort_savefiles_enable = \"true\"\nsort_savestates_enable = \"true\"\nstate_slot = \"3\"\n");
    f.write("roms/snes/Snes9x/game.srm", "personal sram");
    let snapshot = f.snapshot();
    assert!(
        snapshot
            .document
            .saves
            .iter()
            .any(|fact| fact.value.ends_with("/snes/Snes9x/game.srm"))
    );
    assert!(
        snapshot
            .document
            .saves
            .iter()
            .any(|fact| fact.label == "即时存档" && fact.value.ends_with("game.state"))
    );
    let change = Change {
        id: Id::SaveByCore,
        value: Some(Value::Bool(false)),
    };
    let predicted = snapshot
        .preview(Scope::Game, std::slice::from_ref(&change))
        .unwrap();
    assert!(!f.root.join("config/Snes9x/game.cfg").exists());
    assert!(
        predicted
            .iter()
            .any(|fact| fact.value.ends_with("/snes/game.srm"))
    );
    snapshot.save(Scope::Game, &[change], &f.journal()).unwrap();
    assert_eq!(predicted, f.snapshot().document.saves);
    assert_eq!(
        fs::read(f.root.join("roms/snes/Snes9x/game.srm")).unwrap(),
        b"personal sram"
    );
}

#[test]
fn malformed_or_revised_settings_cannot_redirect_a_write() {
    let f = Fixture::new();
    let snapshot = f.snapshot();
    f.write("retroarch.cfg", "new_external_value = \"true\"\n");
    assert!(
        snapshot
            .save(
                Scope::Game,
                &[Change {
                    id: Id::IntegerScale,
                    value: Some(Value::Bool(true))
                }],
                &f.journal()
            )
            .is_err()
    );
    assert!(!f.root.join("config/Snes9x/game.cfg").exists());
    assert!(
        snapshot
            .preview(
                Scope::Game,
                &[Change {
                    id: Id::Option("../../outside".into()),
                    value: Some(Value::Text("x".into()))
                }]
            )
            .is_err()
    );
}

#[test]
fn one_dependency_graph_keeps_observed_bytes_and_absence_until_explicit_reload() {
    let f = Fixture::new();
    let present = f.root.join("observed.cfg");
    let missing = f.root.join("appeared.cfg");
    f.write("observed.cfg", "value = \"original\"\n");
    let mut graph = files::Graph::default();
    assert_eq!(
        graph.load(&present, &f.root).unwrap()["value"].value,
        "original"
    );
    assert!(graph.load(&missing, &f.root).unwrap().is_empty());
    let revision = graph.revision();
    f.write("observed.cfg", "value = \"external\"\n");
    f.write("appeared.cfg", "value = \"new file\"\n");
    assert!(!graph.exists(&missing));
    assert_eq!(
        graph.load(&present, &f.root).unwrap()["value"].value,
        "original"
    );
    assert!(graph.load(&missing, &f.root).unwrap().is_empty());
    assert_eq!(revision, graph.revision());
    assert!(graph.unchanged().is_err());
    let mut reloaded = files::Graph::default();
    assert_eq!(
        reloaded.load(&present, &f.root).unwrap()["value"].value,
        "external"
    );
    assert_eq!(
        reloaded.load(&missing, &f.root).unwrap()["value"].value,
        "new file"
    );
    fs::remove_file(&present).unwrap();
    assert!(graph.exists(&present));
}

#[test]
fn inheritance_is_validated_before_whole_file_removal_and_multiplayer_keeps_pad_mapping_only() {
    let f = Fixture::new();
    f.write("config/Snes9x/game.opt", "snes9x_speed = \"fast\"\n");
    f.write("config/remaps/Snes9x/game.rmp","input_player1_btn_a = \"0\"\ninput_libretro_device_p1 = \"257\"\ninput_player3_btn_a = \"9\"\n");
    let snapshot = f.snapshot();
    assert!(
        snapshot
            .save(
                Scope::Game,
                &[Change {
                    id: Id::Option("unknown".into()),
                    value: None
                }],
                &f.journal()
            )
            .is_err()
    );
    assert!(f.root.join("config/Snes9x/game.opt").is_file());
    let settings = snapshot.settings();
    assert!(settings.remap.contains("input_player1_btn_a"));
    assert!(
        !settings.remap.contains("input_libretro_device")
            && !settings.remap.contains("input_player3")
    );
    assert_eq!(snapshot.reload().unwrap().document, snapshot.document);
}

#[test]
fn launch_controlled_core_options_report_effective_values_and_reject_edits() {
    let f = Fixture::new();
    f.write("retroarch-core-options.cfg", "snes9x_speed = \"fast\"\n");
    let mut snapshot = f.snapshot();
    snapshot.lock_options("snes9x_speed = \"normal\"\n");
    let setting = field(&snapshot, Id::Option("snes9x_speed".into()));
    assert!(
        setting
            .values
            .iter()
            .all(|v| v == &Value::Text("normal".into()))
    );
    assert!(setting.targets.iter().all(String::is_empty));
    assert!(
        snapshot
            .save(
                Scope::Core,
                &[Change {
                    id: setting.id.clone(),
                    value: Some(Value::Text("fast".into()))
                }],
                &f.journal()
            )
            .is_err()
    );
    assert_eq!(snapshot.reload().unwrap().document, snapshot.document);
    assert!(
        String::from_utf8(snapshot.settings().options)
            .unwrap()
            .contains("snes9x_speed = \"normal\"")
    );
    assert_eq!(
        fs::read_to_string(f.root.join("retroarch-core-options.cfg")).unwrap(),
        "snes9x_speed = \"fast\"\n"
    );
}
