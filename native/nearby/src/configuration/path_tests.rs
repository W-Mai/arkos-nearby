use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn configured_slot_is_not_the_content_startup_slot() {
    let f = Fixture::new();
    f.write(
        "retroarch.cfg",
        "state_slot = \"3\"\nsavestates_in_content_dir = \"true\"\n",
    );
    let snapshot = f.snapshot();
    let facts = &snapshot.document.saves;
    let state = facts.iter().find(|fact| fact.label == "即时存档").unwrap();
    assert_eq!(
        state.value,
        f.root.join("roms/snes/game.state").to_string_lossy()
    );
    assert_eq!(field(&snapshot, Id::Slot).values[0], Value::Number(3));
}

#[test]
fn entry_slot_and_automatic_index_do_not_overwrite_the_configured_selection() {
    let f = Fixture::new();
    for configured in [-1, 0, 3, 999] {
        for automatic in [false, true] {
            f.write(
                "retroarch.cfg",
                &format!("state_slot = \"{configured}\"\nsavestate_auto_index = \"{automatic}\"\n"),
            );
            let snapshot = f.snapshot();
            assert_eq!(snapshot.saves.slots.configured, configured);
            assert_eq!(snapshot.saves.slots.entry, None);
            assert_eq!(
                snapshot.saves.slots.startup(),
                if automatic {
                    StartupSlot::AutoIndexed
                } else {
                    StartupSlot::Fixed(0)
                }
            );
            for entry in [0, 3, 999] {
                let snapshot = f.snapshot_args(vec![format!("--entryslot={entry}")]);
                assert_eq!(snapshot.saves.slots.configured, configured);
                assert_eq!(snapshot.saves.slots.entry, Some(entry));
                assert_eq!(
                    snapshot.saves.slots.startup(),
                    StartupSlot::Fixed(i32::from(entry))
                );
                assert_eq!(
                    field(&snapshot, Id::Slot).values[0],
                    Value::Number(configured)
                );
            }
        }
    }
}

#[test]
fn explicit_entry_uses_frontend_numeric_grammar_and_ignores_invalid_replacements() {
    let f = Fixture::new();
    let cases: &[(&[&str], Option<u16>)] = &[
        (&["-e", "3"], Some(3)),
        (&["-e0xA"], Some(10)),
        (&["--entryslot=010"], Some(8)),
        (&["--entryslot=+3"], Some(3)),
        (&["--entryslot=-0"], Some(0)),
        (&["--entryslot=08"], None),
        (&["--entryslot=-1"], None),
        (&["--entryslot=1000"], None),
        (&["--entryslot=65536"], None),
        (&["--entryslot=3 "], None),
        (&["--entryslot=++3"], None),
        (&["--entryslot=0x+3"], None),
        (&["-e3", "--entryslot=bad"], Some(3)),
        (&["-e3", "--entryslot=4"], Some(4)),
        (&["--", "-e3"], None),
    ];
    for (args, expected) in cases {
        let snapshot = f.snapshot_args(args.iter().map(|s| s.to_string()).collect());
        assert_eq!(snapshot.saves.slots.entry, *expected, "{args:?}");
    }
}

#[test]
fn slot_drafts_project_only_configuration_and_keep_entry_and_startup_identity() {
    let f = Fixture::new();
    f.write("retroarch.cfg", "state_slot = \"3\"\n");
    let change = Change {
        id: Id::Slot,
        value: Some(Value::Number(5)),
    };
    for argv in [vec![], vec!["-e7".into()]] {
        let snapshot = f.snapshot_args(argv.clone());
        let baseline = snapshot.document.saves.clone();
        let preview = snapshot
            .preview(Scope::Game, std::slice::from_ref(&change))
            .unwrap();
        for label in ["显式载入槽位", "启动后槽位", "即时存档"] {
            assert_eq!(
                preview.iter().find(|f| f.label == label),
                baseline.iter().find(|f| f.label == label)
            );
        }
        assert_eq!(
            preview
                .iter()
                .find(|f| f.label == "配置槽位")
                .unwrap()
                .value,
            "槽位 5"
        );
        assert!(!f.root.join("config/Snes9x/game.cfg").exists());
        snapshot
            .save(Scope::Game, std::slice::from_ref(&change), &f.journal())
            .unwrap();
        assert_eq!(preview, f.snapshot_args(argv).document.saves);
        fs::remove_file(f.root.join("config/Snes9x/game.cfg")).unwrap();
    }
}

#[test]
fn directory_dependencies_are_stable_sensitive_to_identity_and_ignore_unrelated_children() {
    let f = Fixture::new();
    let parent = f.root.join("saves");
    fs::create_dir(&parent).unwrap();
    let child = parent.join("Snes9x");
    let mut graph = files::Graph::default();
    assert!(graph.can_create_directory(&child).unwrap());
    let revision = graph.revision();
    f.write("saves/another.srm", "unrelated");
    graph.unchanged().unwrap();
    fs::write(&child, "blocked").unwrap();
    assert!(graph.can_create_directory(&child).unwrap());
    assert_eq!(revision, graph.revision());
    assert!(graph.unchanged().is_err());
    let mut fresh = files::Graph::default();
    assert!(!fresh.can_create_directory(&child).unwrap());
    fs::remove_file(&child).unwrap();
    fs::create_dir(&child).unwrap();
    let mut fresh = files::Graph::default();
    assert!(fresh.directory(&child).unwrap());
    fs::set_permissions(&child, fs::Permissions::from_mode(0o500)).unwrap();
    assert!(fresh.unchanged().is_err());
}

#[test]
fn directory_creation_uses_the_game_account_instead_of_the_privileged_editor_account() {
    let f = Fixture::new();
    fs::set_permissions(&f.root, fs::Permissions::from_mode(0o755)).unwrap();
    let mut graph = files::Graph::default();
    graph.set_access(Some(files::Access {
        uid: u32::MAX,
        groups: vec![],
    }));
    assert!(!graph.can_create_directory(&f.root.join("child")).unwrap());
    fs::set_permissions(&f.root, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(graph.unchanged().is_err());
    let mut graph = files::Graph::default();
    graph.set_access(Some(files::Access {
        uid: u32::MAX,
        groups: vec![],
    }));
    assert!(graph.can_create_directory(&f.root.join("child")).unwrap());
}

#[test]
fn frontend_defaults_and_attached_arguments_follow_directory_redirection_and_override_reload() {
    let f = Fixture::new();
    fs::create_dir_all(f.root.join("explicit")).unwrap();
    f.write("selected.cfg", "state_slot = \"3\"\n");
    let load = |profile: &str, extra: Vec<String>| {
        let mut argv = vec![format!("-c{}", f.root.join("selected.cfg").display())];
        argv.extend(extra);
        Snapshot::load(
            Environment {
                access: None,
                home: f.root.clone(),
                cwd: f.root.clone(),
                root: f.root.join(profile),
                config_home: f.root.join("xdg"),
                core: "Snes9x".into(),
                game: f.root.join("roms/snes/game.zip"),
                argv,
                controller: None,
            },
            Document::default(),
            vec![],
        )
        .unwrap()
    };
    let save = |snapshot: &Snapshot| snapshot.document.saves[0].value.clone();
    for profile in ["retroarch", "retroarch32"] {
        let file = load(
            profile,
            vec![format!(
                "-s{}",
                f.root.join("explicit/custom.srm").display()
            )],
        );
        assert_eq!(
            save(&file),
            f.root
                .join("xdg/retroarch/saves/game.srm")
                .to_string_lossy()
        );
        let directory = load(
            profile,
            vec![
                "-s".into(),
                f.root.join("explicit").to_string_lossy().into(),
            ],
        );
        assert_eq!(
            save(&directory),
            f.root.join("explicit/game.srm").to_string_lossy()
        );
        f.write(
            &format!("{profile}/config/Snes9x/game.cfg"),
            &format!(
                "savefile_directory = \"{}\"\n",
                f.root.join("roms/snes").display()
            ),
        );
        let overridden = load(profile, vec!["--save=explicit/custom.srm".into()]);
        assert_eq!(
            save(&overridden),
            f.root.join("roms/snes/game.srm").to_string_lossy()
        );
    }
    assert!(!f.root.join("xdg").exists());
}

#[test]
fn preview_is_read_only_and_detects_directory_changes_before_configuration_commit() {
    let f = Fixture::new();
    f.write(
        "retroarch.cfg",
        "savefiles_in_content_dir = \"true\"\nsort_savefiles_enable = \"true\"\n",
    );
    let snapshot = f.snapshot();
    let change = Change {
        id: Id::SaveByCore,
        value: Some(Value::Bool(false)),
    };
    snapshot
        .preview(Scope::Game, std::slice::from_ref(&change))
        .unwrap();
    assert!(!f.root.join("roms/snes/Snes9x").exists());
    assert!(!f.root.join(".config").exists());
    f.write("roms/snes/Snes9x", "blocks classification");
    assert!(snapshot.save(Scope::Game, &[change], &f.journal()).is_err());
    assert!(!f.root.join("config/Snes9x/game.cfg").exists());
    let reloaded = f.snapshot();
    assert_eq!(
        reloaded.document.saves[0].value,
        f.root.join("roms/snes/game.srm").to_string_lossy()
    );
}
