use super::*;

#[test]
fn frontend_and_scope_combinations_keep_cfg_option_and_remap_precedence_separate() {
    let mut cases = 0;
    for frontend in ["retroarch", "retroarch32"] {
        for mask in 0..64 {
            for controller_known in [false, true] {
                let f = Fixture::new();
                let present = |bit| mask & (1 << bit) != 0;
                let overrides = present(3);
                let per_core = present(4);
                let game_options = present(5);
                let prefix = format!("{frontend}/");
                let root = f.root.join(frontend);
                let put = |path: &str, value: &str| f.write(&(prefix.clone() + path), value);
                put(
                    "selected.cfg",
                    &format!(
                        "auto_overrides_enable = \"{overrides}\"\nglobal_core_options = \"{}\"\ngame_specific_options = \"{game_options}\"\nauto_remaps_enable = \"{overrides}\"\ninput_remap_sort_by_controller_enable = \"true\"\n",
                        !per_core
                    ),
                );
                put("append-first.cfg", "video_scale_integer = \"true\"\n");
                put("append-last.cfg", "video_scale_integer = \"false\"\n");
                put("retroarch-core-options.cfg", "snes9x_speed = \"normal\"\n");
                let remaps = if controller_known {
                    "config/remaps/Snes9x/Pad_A"
                } else {
                    "config/remaps/Snes9x"
                };
                let layers = [
                    ("Snes9x", true, "fast", 0),
                    ("snes", false, "folder", 1),
                    ("game", true, "game", 2),
                ];
                for (index, (name, integer, option, button)) in layers.iter().enumerate() {
                    if present(index) {
                        put(
                            &format!("config/Snes9x/{name}.cfg"),
                            &format!("video_scale_integer = \"{integer}\"\n"),
                        );
                        put(
                            &format!("config/Snes9x/{name}.opt"),
                            &format!("snes9x_speed = \"{option}\"\n"),
                        );
                        put(
                            &format!("{remaps}/{name}.rmp"),
                            &format!("input_player1_btn_a = \"{button}\"\n"),
                        );
                    }
                }
                let snapshot = Snapshot::load(
                    Environment {
                access: None,
                        home: f.root.clone(),
                        config_home: f.root.join(".config"),
                        cwd: f.root.clone(),
                        root: root.clone(),
                        core: "Snes9x".into(),
                        game: f.root.join("roms/snes/game.zip"),
                        argv: vec![
                            "--config".into(),
                            format!("{frontend}/selected.cfg"),
                            format!("--appendconfig={frontend}/append-first.cfg|{frontend}/append-last.cfg"),
                        ],
                        controller: controller_known.then(|| "Pad/A".into()),
                    },
                    Document::default(),
                    vec![("snes9x_speed".into(), "Speed; normal|fast|folder|game".into())],
                )
                .unwrap();
                let active_cfg = (0..3).rev().find(|&index| overrides && present(index));
                let integer = field(&snapshot, Id::IntegerScale);
                assert_eq!(
                    integer.values[0],
                    Value::Bool(active_cfg.is_some_and(|index| layers[index].1)),
                    "{frontend} {mask} {controller_known}"
                );
                let cfg_origin = active_cfg
                    .map(|index| root.join(format!("config/Snes9x/{}.cfg", layers[index].0)))
                    .unwrap_or_else(|| root.join("append-last.cfg"));
                assert_eq!(integer.origins[0], cfg_origin.to_string_lossy());
                let core_option = per_core.then_some(0).filter(|&index| present(index));
                let active_option = (0..3)
                    .rev()
                    .find(|&index| game_options && index > 0 && present(index))
                    .or(core_option);
                let option = field(&snapshot, Id::Option("snes9x_speed".into()));
                assert_eq!(
                    option.values[0],
                    Value::Text(
                        active_option
                            .map_or("normal", |index| layers[index].2)
                            .into()
                    )
                );
                let option_origin = active_option
                    .map(|index| root.join(format!("config/Snes9x/{}.opt", layers[index].0)))
                    .unwrap_or_else(|| root.join("retroarch-core-options.cfg"));
                assert_eq!(option.origins[0], option_origin.to_string_lossy());
                assert_eq!(
                    option.values[1],
                    Value::Text(core_option.map_or("normal", |index| layers[index].2).into())
                );
                assert_eq!(option.targets[0].is_empty(), !game_options);
                let button = field(&snapshot, Id::Button(8));
                assert_eq!(
                    button.values[0],
                    Value::Number(active_cfg.map_or(8, |index| layers[index].3))
                );
                assert_eq!(
                    button.targets[0].is_empty(),
                    !overrides || !controller_known
                );
                assert!(button.targets[2].is_empty());
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 256);
}
