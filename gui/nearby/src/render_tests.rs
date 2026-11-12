use super::*;

#[test]
fn departing_platform_previews_keep_content_and_entities_until_the_slide_finishes() {
    use crate::model::{GameRow, LibraryKind};
    use mirui::ui::{Hidden, IdMap, Parent};

    for launcher in [false, true] {
        let mut app = app([640, 480]);
        let mut current = view(Page::Games);
        current.launcher.enabled = launcher;
        current.launcher.tab = usize::from(launcher);
        current.platforms = true;
        current.library.kind = LibraryKind::Platforms;
        current.selected = 1;
        current.entries = ["nes", "snes", "gba"]
            .into_iter()
            .enumerate()
            .map(|(index, icon)| GameRow {
                key: format!("platform-{index}"),
                name: icon.into(),
                icon: icon.into(),
                available: true,
                directory: true,
                ..Default::default()
            })
            .collect();
        let mut scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 400);
        let state = if launcher {
            &scene.catalog
        } else {
            &scene.state
        };
        let presentation = state.platform_previews();
        let preview = app
            .world
            .resource::<IdMap>()
            .unwrap()
            .get("platform_previews")
            .unwrap();
        let members = app
            .world
            .query::<Style>()
            .iter()
            .filter_map(|(entity, _)| {
                selection::contains(preview, entity, |node| {
                    app.world.get::<Parent>(node).map(|p| p.0)
                })
                .then_some(entity)
            })
            .collect::<Vec<_>>();
        let visible = app
            .world
            .get::<mirui::ui::Children>(preview)
            .unwrap()
            .0
            .iter()
            .copied()
            .filter(|entity| !app.world.has::<Hidden>(*entity))
            .collect::<Vec<_>>();
        assert!(!members.is_empty());
        assert_eq!(state.platform_preview_title(), "snes");

        current.library.kind = LibraryKind::Directory;
        current.platforms = false;
        current.selected = 0;
        current.input_generation.advance();
        current.entries = (0..8)
            .map(|index| GameRow {
                key: format!("game-{index}"),
                name: format!("Game {index}"),
                icon: "snes".into(),
                available: true,
                ..Default::default()
            })
            .collect();
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 416);
        let state = if launcher {
            &scene.catalog
        } else {
            &scene.state
        };
        assert_eq!(state.page_key().library, LibraryKind::Directory);
        assert_eq!(scene.state.snapshot(), current);
        assert!(state.platform_previews() == presentation);
        assert_eq!(state.platform_preview_selection(), 1);
        assert_eq!(state.platform_preview_title(), "snes");
        assert!(members.iter().all(|entity| app.world.is_alive(*entity)));
        assert_eq!(
            app.world
                .get::<mirui::ui::Children>(preview)
                .unwrap()
                .0
                .iter()
                .copied()
                .filter(|entity| !app.world.has::<Hidden>(*entity))
                .collect::<Vec<_>>(),
            visible
        );
        assert_eq!(
            state.entries().entries().next().unwrap().1.label(true),
            "Game 0"
        );
        paint_at(&mut app, &scene, 1000);
        assert!(!app.world.is_alive(preview));
    }
}

#[test]
fn directory_pages_do_not_construct_hidden_platform_preview_collections() {
    use crate::model::{GameRow, LibraryKind};
    use mirui::ui::IdMap;

    for launcher in [false, true] {
        let mut app = app([640, 480]);
        let mut current = view(Page::Games);
        current.launcher.enabled = launcher;
        current.launcher.tab = usize::from(launcher);
        current.library.kind = LibraryKind::Directory;
        current.entries = (0..8)
            .map(|index| GameRow {
                key: format!("game-{index}"),
                name: format!("Game {index}"),
                icon: "snes".into(),
                available: true,
                ..Default::default()
            })
            .collect();
        let mut scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 400);
        assert!(
            app.world
                .resource::<IdMap>()
                .unwrap()
                .get("platform_previews")
                .is_none()
        );

        current.library.kind = LibraryKind::Platforms;
        current.platforms = true;
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 800);
        paint_at(&mut app, &scene, 1200);
        assert!(
            app.world
                .resource::<IdMap>()
                .unwrap()
                .get("platform_previews")
                .is_some()
        );

        current.library.kind = LibraryKind::Directory;
        current.platforms = false;
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 1600);
        paint_at(&mut app, &scene, 2000);
        assert!(
            app.world
                .resource::<IdMap>()
                .unwrap()
                .get("platform_previews")
                .is_none()
        );
    }
}

#[test]
fn deferred_panels_mount_only_when_visible_and_reuse_their_editors() {
    use howff_menu::wifi::{Page as WifiPage, View as WifiView};
    use mirui::ui::Children;
    use mirui::ui::widgets::TextEditing;
    use mirui::ui::widgets::page_view::PageView;

    fn mounted(app: &TestApp, pane: deferred::Pane) -> Option<Entity> {
        app.world
            .query::<PageView<deferred::Pane>>()
            .iter()
            .find_map(|(entity, view)| {
                (view.page == Some(pane))
                    .then(|| app.world.get::<Children>(entity)?.0.first().copied())
                    .flatten()
            })
    }

    for launcher in [false, true] {
        let mut app = app([640, 480]);
        let mut current = view(if launcher { Page::Games } else { Page::Lobby });
        current.launcher.enabled = launcher;
        if launcher {
            current.library.kind = crate::model::LibraryKind::Favorites;
        }
        let mut scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 200);
        assert!(mounted(&app, deferred::Pane::Overlay).is_none());
        assert_eq!(app.world.query::<TextEditing>().iter().count(), 0);
        if !launcher {
            continue;
        }
        current.launcher.overlay = "系统设置".into();
        current.launcher.overlay_kind = "system".into();
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 400);
        let overlay = mounted(&app, deferred::Pane::Overlay).unwrap();
        assert!(mounted(&app, deferred::Pane::Wifi).is_none());
        assert!(mounted(&app, deferred::Pane::Management).is_none());
        assert!(mounted(&app, deferred::Pane::Saves).is_none());
        assert!(mounted(&app, deferred::Pane::Configuration).is_none());
        assert_eq!(app.world.query::<TextEditing>().iter().count(), 0);

        current.launcher.overlay = "Wi-Fi".into();
        current.launcher.overlay_kind = "wifi".into();
        current.launcher.wifi = Some(WifiView {
            epoch: 4,
            page: WifiPage::List,
            ..Default::default()
        });
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 600);
        let wifi = mounted(&app, deferred::Pane::Wifi).unwrap();
        assert!(mounted(&app, deferred::Pane::WifiKeyboard).is_none());
        assert_eq!(app.world.query::<TextEditing>().iter().count(), 0);

        current.launcher.wifi.as_mut().unwrap().page = WifiPage::Password;
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 800);
        let keyboard = mounted(&app, deferred::Pane::WifiKeyboard).unwrap();
        let editor = app.world.query::<TextEditing>().iter().next().unwrap().0;
        assert_eq!(app.world.query::<TextEditing>().iter().count(), 1);
        assert!(scene.state.launcher().input_session().is_some());

        current.launcher.overlay.clear();
        current.launcher.wifi = None;
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 1000);
        assert!(scene.state.launcher().input_session().is_none());
        assert_eq!(mounted(&app, deferred::Pane::Overlay), Some(overlay));
        assert_eq!(mounted(&app, deferred::Pane::Wifi), Some(wifi));
        assert_eq!(mounted(&app, deferred::Pane::WifiKeyboard), Some(keyboard));

        current.launcher.overlay = "Wi-Fi 密码".into();
        current.launcher.wifi = Some(WifiView {
            epoch: 5,
            page: WifiPage::Password,
            ..Default::default()
        });
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 1200);
        assert_eq!(mounted(&app, deferred::Pane::Overlay), Some(overlay));
        assert_eq!(mounted(&app, deferred::Pane::Wifi), Some(wifi));
        assert_eq!(mounted(&app, deferred::Pane::WifiKeyboard), Some(keyboard));
        assert!(app.world.has::<TextEditing>(editor));
        assert_eq!(app.world.query::<TextEditing>().iter().count(), 1);
        assert!(scene.state.launcher().input_session().is_some());
    }
}

#[test]
fn configuration_editor_keeps_active_focus_and_wrapped_paths_inside_its_panel() {
    use crate::model::Action;
    use howff_menu::configuration::Section as ConfigSection;
    let settle = std::env::var("NEARBY_CONFIG_SETTLE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "游戏设置".into();
        current.launcher.overlay_kind = "configuration".into();
        current.launcher.configuration = Some(crate::launcher::configuration::tests::fixture());
        let scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 800);
        let launcher = scene.state.launcher();
        let editor = launcher.configuration();
        let requests = Rc::new(std::cell::RefCell::new(Vec::new()));
        let output = requests.clone();
        editor
            .on_effect(
                move |request: &howff_menu::input::Sourced<howff_menu::configuration::Request>| {
                    let request = &request.payload;
                    output.borrow_mut().push(request.clone())
                },
            )
            .unwrap();
        for (step, actions) in [
            vec![],
            vec![Action::Down, Action::Confirm],
            vec![Action::Down; 20],
            vec![Action::Confirm],
            vec![Action::Down; 3],
            vec![Action::Back],
            vec![Action::Back],
        ]
        .into_iter()
        .enumerate()
        {
            for action in actions {
                launcher.configuration_input(action);
            }
            paint_at(
                &mut app,
                &scene,
                1000 + step as u64 * (200 + settle) + settle,
            );
            let selected = app
                .world
                .query::<deck::ActionAnchor>()
                .iter()
                .find_map(|(entity, anchor)| {
                    (anchor.1 == deck::ActionLayout::Configuration
                        && anchor.0 == editor.selected()
                        && !mirui::ui::branch::is_effectively_hidden(&app.world, entity))
                    .then(|| app.world.get::<ComputedRect>(entity).unwrap().0)
                })
                .expect("active settings anchor");
            assert_eq!(launcher.menu_selection(), Some(editor.selected()));
            assert_eq!(launcher.selection_age(), editor.age());
            assert!(
                selected.y >= Fixed::ZERO
                    && selected.y + selected.h <= Fixed::from_int(i32::from(size[1])),
                "{size:?} {step}: {selected:?}"
            );
            if let Ok(directory) = std::env::var("NEARBY_CONFIG_CAPTURES") {
                let directory = std::path::Path::new(&directory);
                std::fs::create_dir_all(directory).unwrap();
                let file = std::fs::File::create(
                    directory.join(format!("settings-{}-{step}.png", size[0])),
                )
                .unwrap();
                let mut encoder = png::Encoder::new(file, u32::from(size[0]), u32::from(size[1]));
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let frame = app.backend.framebuffer();
                let pixels = frame.read_pixels().unwrap();
                encoder
                    .write_header()
                    .unwrap()
                    .write_image_data(pixels.bytes())
                    .unwrap();
            }
        }
        assert_eq!(editor.section(), ConfigSection::Home);
        launcher.configuration_input(Action::Back);
        paint_at(&mut app, &scene, 2600 + settle * 8);
        assert!(
            requests
                .borrow()
                .iter()
                .any(|r| matches!(r, howff_menu::configuration::Request::Close { epoch: 7 }))
        );
    }
}

#[test]
fn wifi_input_method_orders_edits_and_submit_and_clears_retired_sessions() {
    use crate::model::Action;
    use howff_menu::wifi::{Page as WifiPage, View as WifiView};
    use mirui::ui::widgets::TextEditing;
    let mut app = app([640, 480]);
    let mut current = view(Page::Games);
    current.launcher.enabled = true;
    current.launcher.overlay = "Wi-Fi 密码".into();
    current.launcher.overlay_kind = "wifi".into();
    current.launcher.wifi = Some(WifiView {
        epoch: 4,
        page: WifiPage::Password,
        ..Default::default()
    });
    let mut scene = draw(&mut app, &current).unwrap();
    paint_at(&mut app, &scene, 1000);
    let launcher = scene.state.launcher();
    assert!(
        launcher.input_session().is_some(),
        "framework owns the active software session"
    );
    let sent = Rc::new(std::cell::RefCell::new(Vec::new()));
    let received = sent.clone();
    launcher
        .on_effect(
            move |intent: &howff_menu::input::Sourced<crate::launcher::Intent>| {
                let intent = &intent.payload;
                received.borrow_mut().push(intent.clone())
            },
        )
        .unwrap();
    // A whole burst and Submit in one frame must read the committed document.
    for _ in 0..8 {
        launcher.wifi_input(Action::Confirm);
    }
    launcher.wifi_input(Action::Refresh);
    launcher.wifi_input(Action::Confirm);
    launcher.wifi_input(Action::Start);
    paint_at(&mut app, &scene, 1600);
    assert_eq!(launcher.wifi_length(), 8);
    assert_eq!(launcher.wifi_editor(35), "********");
    assert!(
        app.world
            .query::<TextEditing>()
            .iter()
            .any(|(_, field)| field.read(|text| text == "qqqqqqqq"))
    );
    assert_eq!(sent.borrow().len(), 1);
    assert!(
        matches!(&sent.borrow()[0], crate::launcher::Intent::Wifi(request) if request.valid() && request.epoch() == 4)
    );
    assert!(
        !serde_json::to_string(&launcher.view())
            .unwrap()
            .contains("qqqqqqqq")
    );
    launcher.wifi_input(Action::Auxiliary);
    assert_eq!(launcher.wifi_editor(35), "qqqqqqqq");
    current.launcher.wifi.as_mut().unwrap().page = WifiPage::List;
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 1800);
    assert!(launcher.input_session().is_none());
    assert_eq!(launcher.wifi_length(), 0);
    assert!(launcher.input_edit().is_none());
}

#[test]
fn wifi_windows_and_keyboard_keep_focus_in_the_active_panel_scope() {
    use crate::model::Action;
    use howff_menu::wifi::{Network, Page as WifiPage, View as WifiView};
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "Wi-Fi 设置".into();
        current.launcher.overlay_kind = "wifi".into();
        current.launcher.selected = 25;
        current.launcher.wifi = Some(WifiView {
            selected: 25,
            networks: (0..30)
                .map(|index| Network {
                    key: index.to_string(),
                    ssid: format!("Network {index}"),
                    signal: 80,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
        let mut scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 1000);
        let rows: Vec<_> = app
            .world
            .query::<deck::ActionAnchor>()
            .iter()
            .filter_map(|(entity, anchor)| {
                Some((anchor.0, app.world.get::<ComputedRect>(entity)?.0))
            })
            .collect();
        assert!(
            (1..=8).contains(&rows.len()),
            "bounded visible window: {}",
            rows.len()
        );
        let selected = rows
            .iter()
            .find(|r| r.0 == 25)
            .expect("selected network visible");
        assert!(
            selected.1.y >= Fixed::ZERO
                && selected.1.y + selected.1.h <= Fixed::from_int(i32::from(size[1]))
        );
        for (step, phase) in [WifiPage::Password, WifiPage::Connecting, WifiPage::Password]
            .into_iter()
            .enumerate()
        {
            current.launcher.wifi.as_mut().unwrap().page = phase;
            current.launcher.wifi.as_mut().unwrap().epoch = 1;
            update(&mut scene, &current).unwrap();
            let phase_time = 1200 + step as u64 * 3000;
            paint_at(&mut app, &scene, phase_time);
            let target = app
                .world
                .query::<actor::ActorFrame>()
                .iter()
                .find_map(|(entity, _)| {
                    app.world
                        .get::<FollowBounds>(entity)
                        .map(|bounds| bounds.target)
                })
                .expect("actor remains visible");
            let anchor = app
                .world
                .get::<deck::ActionAnchor>(target)
                .expect("semantic actor target");
            assert_eq!(anchor.0, 0, "{phase:?}");
            assert_eq!(
                anchor.1,
                if phase == WifiPage::Password {
                    deck::ActionLayout::WifiKeyboard
                } else {
                    deck::ActionLayout::WifiProgress
                }
            );
            if phase == WifiPage::Connecting {
                for offset in [0, 160, 500, 999] {
                    paint_at(&mut app, &scene, phase_time + offset);
                    let track = app
                        .world
                        .query::<wifi::LoadingTrack>()
                        .iter()
                        .find_map(|(entity, _)| {
                            app.world.get::<ComputedRect>(entity).map(|rect| rect.0)
                        })
                        .unwrap();
                    let outputs = app
                        .world
                        .resource::<mirui::ui::Outputs<mirui::ui::LayoutGeometry>>()
                        .unwrap();
                    let geometry = outputs.get(target).unwrap();
                    let ink = Rect::bounding_quad(
                        &geometry
                            .transform()
                            .apply_rect(geometry.border_rect())
                            .unwrap(),
                    );
                    let expected = track.x + scene.state.launcher().wifi_loading_end() * track.w;
                    assert!(
                        (ink.x - expected).abs() <= Fixed::from_int(2),
                        "actor must follow the moving leading edge: {ink:?} {track:?}"
                    );
                    assert!(ink.x >= track.x && ink.x <= track.x + track.w + Fixed::ONE);
                }
            }
            if phase == WifiPage::Password {
                for index in 1..35 {
                    scene.state.launcher().wifi_input(Action::Right);
                    paint_at(&mut app, &scene, phase_time + 100 + index as u64 * 20);
                    let target = app
                        .world
                        .query::<actor::ActorFrame>()
                        .iter()
                        .find_map(|(entity, _)| {
                            app.world.get::<FollowBounds>(entity).map(|b| b.target)
                        })
                        .unwrap();
                    let anchor = app.world.get::<deck::ActionAnchor>(target).unwrap();
                    assert_eq!(
                        (anchor.0, anchor.1),
                        (index, deck::ActionLayout::WifiKeyboard)
                    );
                }
                // Preserve a retained keyboard's local key through a connection attempt.
                for _ in 0..34 {
                    scene.state.launcher().wifi_input(Action::Left);
                }
                paint_at(&mut app, &scene, phase_time + 1000);
            }
        }
        for (step, page) in [
            WifiPage::Info,
            WifiPage::List,
            WifiPage::Success,
            WifiPage::Details,
            WifiPage::List,
        ]
        .into_iter()
        .enumerate()
        {
            current.launcher.wifi.as_mut().unwrap().page = page;
            current.launcher.wifi.as_mut().unwrap().selected = 0;
            current.launcher.selected = 0;
            update(&mut scene, &current).unwrap();
            paint_at(&mut app, &scene, 10000 + step as u64 * 200);
            let target = app
                .world
                .query::<actor::ActorFrame>()
                .iter()
                .find_map(|(entity, _)| app.world.get::<FollowBounds>(entity).map(|v| v.target))
                .unwrap();
            let action = app
                .world
                .get::<deck::ActionAnchor>(target)
                .expect("active Wi-Fi row");
            assert_eq!(action.0, 0);
            let mut node = target;
            loop {
                assert!(
                    !mirui::ui::branch::is_effectively_hidden(&app.world, node),
                    "actor cannot follow a cached hidden branch: {page:?}"
                );
                let Some(parent) = app.world.get::<mirui::ui::Parent>(node) else {
                    break;
                };
                node = parent.0;
            }
            let target_rect = app.world.get::<ComputedRect>(target).unwrap().0;
            let minimum_y = Fixed::from_int(35);
            assert!(
                target_rect.y > minimum_y && target_rect.h >= Fixed::from_int(24),
                "active row must retain visible geometry: {page:?} {target_rect:?}"
            );
        }
    }
}

#[test]
fn details_keep_rows_separate_and_selected_content_visible_on_short_screens() {
    use howff_menu::details::{Document, Load, Section, View as DetailsView};
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "游戏详情".into();
        current.launcher.overlay_kind = "details".into();
        current.launcher.items = (0..3)
            .map(|index| {
                howff_menu::MenuItem::available(
                    howff_menu::MenuAction::ExpandDetail,
                    format!("详情条目 {index}"),
                )
                .with_value("来源与当前文件状态")
            })
            .collect();
        current.launcher.details = Some(DetailsView {
            section: Section::Core,
            load: Load::Ready {
                document: Document::default(),
            },
            ..DetailsView::default()
        });
        let scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 1000);
        let bounds = |app: &TestApp| {
            let mut rows: Vec<_> = app
                .world
                .query::<deck::ActionAnchor>()
                .iter()
                .filter_map(|(entity, anchor)| {
                    app.world
                        .get::<ComputedRect>(entity)
                        .map(|rect| (anchor.0, rect.0))
                })
                .collect();
            rows.sort_by_key(|row| row.0);
            rows
        };
        let rows = bounds(&app);
        assert_eq!(rows.len(), 3);
        for pair in rows.windows(2) {
            assert!(pair[0].1.h >= Fixed::from_int(40));
            assert!(pair[1].1.y >= pair[0].1.y + pair[0].1.h);
        }
        current.launcher.items.extend((3..12).map(|index| {
            howff_menu::MenuItem::available(
                howff_menu::MenuAction::ExpandDetail,
                format!("详情条目 {index}"),
            )
            .with_value("来源与当前文件状态")
        }));
        current.launcher.selected = 11;
        let mut scene = scene;
        update(&mut scene, &current).unwrap();
        for frame in [1100, 1200, 1300] {
            paint_at(&mut app, &scene, frame);
        }
        let (viewport, offset) = app
            .world
            .query::<launcher::DetailsScroll>()
            .iter()
            .find_map(|(entity, _)| {
                Some((
                    app.world.get::<ComputedRect>(entity)?.0,
                    app.world
                        .get::<mirui::input::event::scroll::ScrollOffset>(entity)?
                        .y,
                ))
            })
            .unwrap();
        let selected = bounds(&app).into_iter().find(|row| row.0 == 11).unwrap().1;
        assert!(offset > Fixed::ZERO);
        assert!(selected.y - offset >= viewport.y);
        assert!(selected.y - offset + selected.h <= viewport.y + viewport.h);

        let details = current.launcher.details.as_mut().unwrap();
        let Load::Ready { document } = &mut details.load else {
            unreachable!()
        };
        document.core = (0..12)
            .map(|_| howff_menu::details::Row {
                facts: vec![
                    howff_menu::details::Fact::new(
                        "目录",
                        "/home/ark/.config/retroarch/".repeat(12),
                    ),
                    howff_menu::details::Fact::new(
                        "配置文件",
                        "/home/ark/.config/retroarch/retroarch.cfg",
                    ),
                ],
                path: "/home/ark/.config/retroarch/".repeat(12),
                ..Default::default()
            })
            .collect();
        details.expanded = true;
        update(&mut scene, &current).unwrap();
        for frame in [1400, 1500, 1600] {
            paint_at(&mut app, &scene, frame);
        }
        let expanded = bounds(&app).into_iter().find(|row| row.0 == 11).unwrap().1;
        assert!(expanded.h > selected.h);
        let facts: Vec<_> = app
            .world
            .query::<Text>()
            .iter()
            .filter_map(|(entity, text)| {
                let rect = app.world.get::<ComputedRect>(entity)?.0;
                Some((text.resolve(app.world.resource()).to_string(), rect))
            })
            .collect();
        for label in ["目录", "配置文件"] {
            assert!(facts.iter().any(|(text, _)| text == label));
        }
        let wrapped = facts
            .iter()
            .find(|(text, _)| text == &"/home/ark/.config/retroarch/".repeat(12))
            .unwrap()
            .1;
        assert!(wrapped.h > Fixed::from_int(14));
        assert!(wrapped.x >= viewport.x && wrapped.x + wrapped.w <= viewport.x + viewport.w);
        let reading = app
            .world
            .query::<launcher::DetailsScroll>()
            .iter()
            .find_map(|(entity, _)| app.world.get::<ComputedRect>(entity).map(|_| entity))
            .unwrap();
        assert!(
            app.world
                .query::<actor::ActorFrame>()
                .iter()
                .any(|(entity, _)| app
                    .world
                    .get::<mirui::ui::widgets::FollowBounds>(entity)
                    .is_some_and(|follow| follow.target == reading))
        );
        current.launcher.details.as_mut().unwrap().scroll = 48;
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 1700);
        let scroll = app
            .world
            .query::<launcher::DetailsScroll>()
            .iter()
            .find_map(|(entity, _)| {
                app.world
                    .get::<mirui::input::event::scroll::ScrollOffset>(entity)
                    .map(|offset| offset.y)
            })
            .unwrap();
        assert!(scroll > offset);
    }
}

#[test]
fn list_windows_follow_available_space_and_keep_native_title_size() {
    for size in [[480, 320], [640, 480], [640, 900]] {
        for launcher in [false, true] {
            let mut app = app(size);
            let mut current = view(Page::Games);
            current.platforms = true;
            current.library.kind = crate::model::LibraryKind::Platforms;
            current.launcher.enabled = launcher;
            current.launcher.tab = usize::from(launcher);
            let mut home = current.clone();
            home.library.kind = crate::model::LibraryKind::Favorites;
            home.launcher.tab = 0;
            if launcher {
                current.launcher_shelf = Some(Box::new(home));
            }
            let requests = Rc::new(std::cell::RefCell::new(Vec::new()));
            let received = requests.clone();
            subscribe_list_layout(&app, move |request| {
                received
                    .borrow_mut()
                    .push(serde_json::to_value(request).unwrap());
            })
            .unwrap();
            let mut scene = draw(&mut app, &current).unwrap();
            paint_at(&mut app, &scene, 1000);
            let request = requests.borrow().last().cloned().unwrap();
            assert_eq!(request["launcher"], launcher);
            let count = request["rows"].as_u64().unwrap() as usize;
            assert!((1..=howff_menu::LIST_WINDOW_ITEMS).contains(&count));
            if size == [480, 320] {
                assert!(count < 4);
            }
            let entry = current.entries[0].clone();
            current.entries = (0..count)
                .map(|index| {
                    let mut entry = entry.clone();
                    entry.key = format!("platform-{index}");
                    entry
                })
                .collect();
            update(&mut scene, &current).unwrap();
            paint_at(&mut app, &scene, 1400);
            let display = if launcher {
                &scene.catalog
            } else {
                &scene.state
            };
            assert_eq!(display.entries().len(), count);
            let body = app
                .world
                .query::<selection::SelectionFrame>()
                .iter()
                .find(|(_, frame)| frame.page.library == crate::model::LibraryKind::Platforms)
                .unwrap()
                .1
                .body;
            let mut rows = 0;
            for (entity, _) in app.world.query::<selection::FocusTarget>().iter() {
                if !selection::contains(body, entity, |node| {
                    app.world
                        .get::<mirui::ui::Parent>(node)
                        .map(|parent| parent.0)
                }) {
                    continue;
                }
                let Some(bounds) = app.world.get::<ComputedRect>(entity).map(|rect| rect.0) else {
                    continue;
                };
                rows += 1;
                assert!(
                    bounds.y >= Fixed::ZERO && bounds.y + bounds.h <= i32::from(size[1]).into()
                );
                let has_title = app.world.query::<Text>().iter().any(|(text_entity, text)| {
                    selection::contains(entity, text_entity, |node| {
                        app.world
                            .get::<mirui::ui::Parent>(node)
                            .map(|parent| parent.0)
                    }) && text.resolve(app.world.resource()) == entry.name
                        && app
                            .world
                            .get::<mirui::ui::Style>(text_entity)
                            .is_some_and(|style| style.font_size == Some(24))
                });
                assert!(has_title);
            }
            assert_eq!(rows, count);
            let emitted = requests.borrow().len();
            paint_at(&mut app, &scene, 1600);
            assert_eq!(requests.borrow().len(), emitted);
        }
    }
}

#[test]
fn page_marker_entities_follow_the_active_page_without_remounting() {
    let mut app = app([640, 480]);
    let mut current = view(Page::Games);
    current.library.kind = crate::model::LibraryKind::Favorites;
    current.library.total = 24;
    current.library.position = 1;
    let mut scene = draw(&mut app, &current).unwrap();
    paint_at(&mut app, &scene, 1000);
    let markers = |app: &TestApp| {
        let parent = app
            .world
            .query::<shelf::PageMarkers>()
            .iter()
            .next()
            .unwrap()
            .0;
        let mut markers: Vec<_> = app
            .world
            .query::<mirui::ui::Parent>()
            .iter()
            .filter(|(_, value)| value.0 == parent)
            .map(|(entity, _)| (app.world.get::<ComputedRect>(entity).unwrap().0.x, entity))
            .collect();
        markers.sort_by_key(|(position, _)| *position);
        markers
            .into_iter()
            .enumerate()
            .map(|(index, (_, entity))| (index, entity))
            .collect::<Vec<_>>()
    };
    let initial = markers(&app);
    assert!(initial.len() >= 2);
    current.library.position = scene.state.shelf().plan().grid.capacity() + 1;
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 1130);
    let widths: Vec<_> = initial
        .iter()
        .take(2)
        .map(|(_, entity)| app.world.get::<ComputedRect>(*entity).unwrap().0.w)
        .collect();
    assert!(
        widths
            .iter()
            .all(|width| *width > 6.into() && *width < 22.into())
    );
    paint_at(&mut app, &scene, 1400);
    assert_eq!(markers(&app), initial);
    assert_eq!(
        app.world.get::<ComputedRect>(initial[0].1).unwrap().0.w,
        6.into()
    );
    assert_eq!(
        app.world.get::<ComputedRect>(initial[1].1).unwrap().0.w,
        22.into()
    );
}

#[test]
fn empty_library_actor_returns_to_live_results_and_keyboard_targets() {
    let mut app = app([480, 320]);
    let mut home = view(Page::Games);
    home.library.kind = crate::model::LibraryKind::Favorites;
    home.launcher.enabled = true;
    let mut current = home.clone();
    current.launcher.tab = 1;
    current.launcher_shelf = Some(Box::new(home));
    current.library.kind = crate::model::LibraryKind::Search;
    let results = current.entries.clone();
    current.entries.clear();
    let mut scene = draw(&mut app, &current).unwrap();
    paint_at(&mut app, &scene, 800);
    let target = |app: &TestApp| {
        let frame = app
            .world
            .query::<actor::ActorFrame>()
            .iter()
            .next()
            .unwrap()
            .0;
        assert!(!app.world.has::<mirui::ui::Hidden>(frame));
        app.world.get::<FollowBounds>(frame).unwrap().target
    };
    assert!(app.world.has::<deck::ActorAnchor>(target(&app)));
    current.entries = results;
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 1600);
    let selected = target(&app);
    assert!(app.world.has::<selection::FocusTarget>(selected));
    assert!(!app.world.has::<deck::ActorAnchor>(selected));
    current.library.keyboard = true;
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 2400);
    assert!(
        app.world
            .get::<selection::FocusTarget>(target(&app))
            .unwrap()
            .index()
            >= 100
    );
}

#[test]
fn library_loading_empty_and_failed_states_update_without_changing_routes() {
    let mut app = app([640, 480]);
    let mut current = view(Page::Games);
    current.library.kind = crate::model::LibraryKind::Search;
    current.library.query = "no-match".into();
    current.entries.clear();
    current.library.load = howff_menu::LibraryLoad::Loading;
    let mut scene = draw(&mut app, &current).unwrap();
    paint_at(&mut app, &scene, 400);
    let count = app.world.query::<mirui::ui::widgets::Text>().iter().count();
    assert!(!current.busy);
    assert_eq!(scene.state.library_placeholder().title(), "正在搜索");
    assert!(copy::loading(&current));
    current.library.load = howff_menu::LibraryLoad::Ready;
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 600);
    assert_eq!(scene.state.library_placeholder().title(), "没有匹配的游戏");
    current.library.load = howff_menu::LibraryLoad::Failed {
        message: "无法读取目录".into(),
    };
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 800);
    assert_eq!(scene.state.library_placeholder().title(), "暂时无法读取");
    assert_eq!(
        app.world.query::<mirui::ui::widgets::Text>().iter().count(),
        count
    );
}

#[test]
fn menu_values_observe_settings_without_replacing_the_backend_view() {
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.library.kind = crate::model::LibraryKind::Favorites;
        current.launcher.enabled = true;
        current.launcher.overlay = "启动器设置".into();
        current.launcher.overlay_kind = "system".into();
        current.launcher.items = vec![
            crate::model::LauncherItem::unavailable(
                crate::model::MenuAction::ManageNetwork,
                "管理直连网络",
                "尚未保留直连网络",
            ),
            crate::model::LauncherItem::available(
                crate::model::MenuAction::ToggleSound,
                "音效开关",
            ),
            crate::model::LauncherItem::available(crate::model::MenuAction::CycleTheme, "切换主题"),
            crate::model::LauncherItem::available(crate::model::MenuAction::Volume, "音量 +")
                .with_value("45%"),
            crate::model::LauncherItem::available(crate::model::MenuAction::Brightness, "亮度 +")
                .with_value("60%"),
        ];
        let mut scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 400);
        let labels = |world: &mirui::ecs::World| {
            world
                .query::<Text>()
                .iter()
                .filter(|(entity, _)| {
                    let mut node = *entity;
                    loop {
                        if mirui::ui::branch::is_effectively_hidden(world, node) {
                            return false;
                        }
                        let Some(parent) = world.get::<mirui::ui::Parent>(node) else {
                            return true;
                        };
                        node = parent.0;
                    }
                })
                .map(|(_, text)| text.resolve(world.resource()).to_string())
                .collect::<Vec<_>>()
        };
        for expected in ["尚未保留直连网络", "森林", "开", "45%", "60%"] {
            assert!(
                labels(&app.world).iter().any(|text| text == expected),
                "{size:?}: missing {expected}"
            );
        }
        let settings = scene.state.settings();
        current.launcher.selected = 3;
        current.launcher.notice = "未能完成系统工具，请重试或返回".into();
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 450);
        assert!(labels(&app.world).contains(&current.launcher.notice));
        assert_eq!(scene.state.launcher().notice(), current.launcher.notice);
        current.launcher.notice.clear();
        current.launcher.selected = 0;
        update(&mut scene, &current).unwrap();
        settings.cycle_theme();
        settings.set_sound(crate::settings::SoundPreferences {
            enabled: false,
            ..Default::default()
        });
        paint_at(&mut app, &scene, 500);
        for expected in ["落日", "关"] {
            assert!(
                labels(&app.world).iter().any(|text| text == expected),
                "{size:?}: missing {expected}"
            );
        }
        current.launcher.items[3].value = "50%".into();
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 700);
        assert!(labels(&app.world).iter().any(|label| label == "50%"));
        assert!(!labels(&app.world).iter().any(|label| label == "45%"));
        assert_full_frame_matches(&mut app, "Current menu values invalidate the active branch");
    }
}

#[test]
fn text_output_policy_is_shared_across_library_and_room_pages() {
    for page in [
        Page::Mode,
        Page::Choice,
        Page::Rooms,
        Page::Games,
        Page::Host,
        Page::Joined,
    ] {
        let mut app = app([640, 480]);
        let mut current = view(page);
        current.network_only = matches!(page, Page::Host | Page::Joined);
        let scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 800);
        let mut count = 0;
        for (entity, _) in app.world.query::<mirui::ui::widgets::Text>().iter() {
            assert!(!app.world.has::<cache::GpuLayerCache>(entity));
            count += 1;
        }
        assert!(count > 0);
    }
}

#[test]
fn labels_use_backend_text_retention_without_nested_offscreen_caches() {
    let mut app = app([640, 480]);
    let root = app.spawn_root().id();
    app.compose(
        root,
        widgets::caption(
            "Nearby",
            style::FOREGROUND,
            12,
            mirui::ui::widgets::TextAlign::Start,
            style::LABEL_LEADING,
        ),
    );
    super::paint_at(&mut app, 800).unwrap();
    let labels: Vec<_> = app
        .world
        .query::<mirui::ui::widgets::Text>()
        .iter()
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(labels.len(), 1);
    for label in labels {
        assert!(!app.world.has::<cache::GpuLayerCache>(label));
    }
}

#[test]
fn retained_pages_own_rows_while_the_primary_model_keeps_shared_context() {
    let mut app = app([640, 480]);
    let mut home = view(Page::Games);
    home.library.kind = crate::model::LibraryKind::Favorites;
    home.launcher.enabled = true;
    let mut scene = draw(&mut app, &home).unwrap();
    paint_at(&mut app, &scene, 800);
    assert!(scene.state.entries().is_empty());
    assert!(!scene.shelf.entries().is_empty());

    let mut catalog = view(Page::Games);
    catalog.launcher.enabled = true;
    catalog.launcher.tab = 1;
    catalog.launcher_shelf = Some(Box::new(home));
    catalog.artwork = "selected.png".into();
    update(&mut scene, &catalog).unwrap();
    paint_at(&mut app, &scene, 1600);
    assert!(scene.state.entries().is_empty());
    assert!(!scene.catalog.entries().is_empty());
    assert_eq!(scene.state.artwork(), catalog.artwork);
    assert!(
        scene.state.selected_platform()
            == consoles::Platform::from_id(&catalog.entries[catalog.selected].icon)
    );

    let mut room = view(Page::Rooms);
    room.return_to_launcher = true;
    update(&mut scene, &room).unwrap();
    paint_at(&mut app, &scene, 2400);
    assert!(scene.state.rooms().is_empty());
    assert!(!scene.room.rooms().is_empty());

    catalog.launcher.enabled = false;
    scene.state.present(&catalog);
    assert_eq!(scene.state.entries().len(), catalog.entries.len().min(4));
    room.return_to_launcher = false;
    scene.state.present(&room);
    assert_eq!(scene.state.rooms().len(), room.rooms.len().min(4));
}

#[test]
fn catalog_platform_and_directory_routes_reuse_display_slots_without_reusing_input_identity() {
    use crate::model::{GameRow, LibraryKind};
    let mut app = app([640, 480]);
    let mut catalog = view(Page::Games);
    catalog.launcher.enabled = true;
    catalog.launcher.tab = 1;
    catalog.library.kind = LibraryKind::Platforms;
    catalog.platforms = true;
    catalog.entries = ["nes", "snes", "gba", "psx"]
        .into_iter()
        .enumerate()
        .map(|(index, icon)| GameRow {
            key: format!("platform-{index}"),
            name: icon.into(),
            icon: icon.into(),
            directory: true,
            available: true,
            ..Default::default()
        })
        .collect();
    let platforms = catalog.entries.clone();
    let mut scene = draw(&mut app, &catalog).unwrap();
    paint_at(&mut app, &scene, 400);
    let ids = scene
        .catalog
        .entries()
        .entries()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let covers = scene
        .catalog
        .entries()
        .entries()
        .map(|(_, row)| row.cover_bounds())
        .collect::<Vec<_>>();
    for (step, kind) in [
        LibraryKind::Directory,
        LibraryKind::Platforms,
        LibraryKind::Directory,
        LibraryKind::Platforms,
    ]
    .into_iter()
    .enumerate()
    {
        catalog.library.kind = kind;
        catalog.platforms = kind == LibraryKind::Platforms;
        catalog.selected = step % platforms.len();
        catalog.input_generation.advance();
        catalog.entries = if catalog.platforms {
            platforms.clone()
        } else {
            (0..platforms.len())
                .map(|index| GameRow {
                    key: format!("game-{step}-{index}"),
                    name: format!("Game {step}-{index}"),
                    icon: "snes".into(),
                    available: true,
                    favorite: index % 2 == 0,
                    ..Default::default()
                })
                .collect()
        };
        update(&mut scene, &catalog).unwrap();
        assert_eq!(
            scene
                .catalog
                .entries()
                .entries()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(scene.state.snapshot(), catalog);
        for (((_, row), cover), expected) in scene
            .catalog
            .entries()
            .entries()
            .zip(&covers)
            .zip(&catalog.entries)
        {
            assert!(cover.callback_ready());
            let probe = LayoutSize {
                width: Fixed::from_int(31 + step as i32),
                height: Fixed::from_int(23),
            };
            cover.resize(probe);
            assert_eq!(row.cover_bounds().size(), probe);
            assert_eq!(row.label(true), expected.name);
            assert_eq!(row.directory(), expected.directory);
            assert_eq!(row.favorite(), expected.favorite);
        }
        paint_at(&mut app, &scene, 1200 + step as u64 * 800);
        assert!(focus_rect(&app).is_some());
        assert_full_frame_matches(&mut app, "Catalog route slot reuse");
    }
}

#[test]
fn catalog_slots_keep_bindings_while_game_content_and_window_change() {
    let mut app = app([640, 480]);
    let mut home = view(Page::Games);
    home.library.kind = crate::model::LibraryKind::Favorites;
    home.launcher.enabled = true;
    let mut scene = draw(&mut app, &home).unwrap();
    paint_at(&mut app, &scene, 800);
    let mut catalog = view(Page::Games);
    catalog.launcher.enabled = true;
    catalog.launcher.tab = 1;
    catalog.launcher_shelf = Some(Box::new(home));
    catalog.selected = 3;
    catalog.status.clear();
    let window = |start| {
        (start..start + 4)
            .map(|i| crate::model::GameRow {
                key: format!("game-{i}"),
                name: format!("Game {i}"),
                icon: "gba".into(),
                available: true,
                favorite: i % 2 == 0,
                ..Default::default()
            })
            .collect::<Vec<_>>()
    };
    catalog.entries = window(0);
    update(&mut scene, &catalog).unwrap();
    paint_at(&mut app, &scene, 1600);
    let members: Vec<_> = scene
        .catalog
        .entries()
        .entries()
        .map(|(id, _)| id)
        .collect();
    for (step, start) in [1, 2, 3, 2, 0, 7].into_iter().enumerate() {
        catalog.entries = window(start);
        update(&mut scene, &catalog).unwrap();
        paint_at(&mut app, &scene, 2400 + step as u64 * 800);
        assert_eq!(
            scene
                .catalog
                .entries()
                .entries()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            members
        );
        assert_eq!(scene.catalog.entries().len(), 4);
        for ((_, row), expected) in scene.catalog.entries().entries().zip(&catalog.entries) {
            assert_eq!(row.label(true), expected.name);
            assert_eq!(row.favorite(), expected.favorite);
        }
        assert_full_frame_matches(&mut app, "Catalog slot content");
    }
    catalog.entries.truncate(1);
    catalog.selected = 0;
    update(&mut scene, &catalog).unwrap();
    paint_at(&mut app, &scene, 8000);
    assert_eq!(scene.catalog.entries().len(), 1);
    catalog.entries.clear();
    update(&mut scene, &catalog).unwrap();
    paint_at(&mut app, &scene, 8800);
    assert!(scene.catalog.entries().is_empty());
}

#[test]
fn modal_input_and_geometry_followers_belong_to_the_active_layer() {
    let mut app = app([640, 480]);
    let mut home = view(Page::Games);
    home.library.kind = crate::model::LibraryKind::Favorites;
    home.launcher.enabled = true;
    let mut scene = draw(&mut app, &home).unwrap();
    paint_at(&mut app, &scene, 800);
    let shelf_frame = app
        .world
        .query::<selection::SelectionFrame>()
        .iter()
        .find(|(_, frame)| frame.page.library == crate::model::LibraryKind::Favorites)
        .unwrap()
        .0;
    let initial = app.world.get::<FollowBounds>(shelf_frame).unwrap().target;
    let mut modal = view(Page::Mode);
    modal.lobby = true;
    modal.return_to_launcher = true;
    update(&mut scene, &modal).unwrap();
    paint_at(&mut app, &scene, 1600);
    assert!(app.world.has::<mirui::ui::Hidden>(shelf_frame));
    assert!(!app.world.has::<FollowBounds>(shelf_frame));
    for (entity, _) in app.world.query::<selection::SelectionFrame>().iter() {
        if app.world.has::<mirui::ui::Hidden>(entity) {
            assert!(!app.world.has::<FollowBounds>(entity));
        }
    }
    let background = scene.shelf.card_motion();
    scene.press(1600);
    paint_at(&mut app, &scene, 1616);
    assert_eq!(scene.shelf.card_motion(), background);
    assert_eq!(scene.room.card_motion().press, 0);
    update(&mut scene, &home).unwrap();
    paint_at(&mut app, &scene, 2400);
    assert!(!app.world.has::<mirui::ui::Hidden>(shelf_frame));
    assert_eq!(
        app.world.get::<FollowBounds>(shelf_frame).unwrap().target,
        initial
    );
}

#[test]
fn launcher_feedback_retains_shelf_geometry_and_selected_action() {
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut home = view(Page::Games);
        home.status.clear();
        home.library.kind = crate::model::LibraryKind::Favorites;
        home.library.total = 6;
        home.launcher.enabled = true;
        home.launcher.key = home.entries[0].key.clone();
        home.launcher_shelf = Some(Box::new(home.clone()));
        let requests = Rc::new(std::cell::RefCell::new(Vec::new()));
        let received = requests.clone();
        subscribe_shelf_layout(&app, move |request| received.borrow_mut().push(*request)).unwrap();
        let mut scene = draw(&mut app, &home).unwrap();
        paint_at(&mut app, &scene, 320);
        let plan = scene.shelf.shelf().plan();
        let height = scene.deck.height();
        let emitted = requests.borrow().len();
        let gesture = super::launcher(&mut app);
        let intents = Rc::new(std::cell::RefCell::new(Vec::new()));
        let received = intents.clone();
        gesture
            .on_effect(
                move |intent: &howff_menu::input::Sourced<crate::launcher::Intent>| {
                    let intent = &intent.payload;
                    received.borrow_mut().push(intent.clone());
                },
            )
            .unwrap();

        for (start, cancel) in [(320, true), (1120, false)] {
            gesture.press(start);
            paint_at(&mut app, &scene, start + 240);
            assert!(gesture.progress() > 0);
            assert_eq!(
                scene.deck.height(),
                height,
                "Hold feedback cannot resize the deck"
            );
            assert_eq!(scene.shelf.shelf().plan(), plan);
            assert_eq!(scene.state.confirm(), "操作");
            if cancel {
                gesture.cancel();
            }
            paint_at(&mut app, &scene, start + 720);
            gesture.release(start + 720);
            assert_eq!(gesture.progress(), 0);
        }
        assert_eq!(
            *intents.borrow(),
            vec![crate::launcher::Intent::Game {
                key: home.launcher.key.clone(),
                multiplayer: true,
            }]
        );
        let mut waiting = home.clone();
        waiting.busy = true;
        waiting.status = "正在准备联机".into();
        waiting.launcher.key.clear();
        update(&mut scene, &waiting).unwrap();
        paint_at(&mut app, &scene, 2000);
        assert!(
            !scene.state.confirm_visible(),
            "Preparation has no library action"
        );
        assert_eq!(scene.state.hint(), waiting.status);
        assert_eq!(
            scene.deck.height(),
            height,
            "Status feedback cannot resize the deck"
        );
        assert_eq!(scene.shelf.shelf().plan(), plan);
        update(&mut scene, &home).unwrap();
        paint_at(&mut app, &scene, 2200);
        assert_eq!(scene.state.confirm(), "操作");
        assert_eq!(scene.shelf.shelf().plan(), plan);
        assert_eq!(
            requests.borrow().len(),
            emitted,
            "Feedback cannot renegotiate the grid"
        );
    }
}

#[test]
fn launcher_library_hint_requires_an_available_library_entry() {
    let mut current = view(Page::Games);
    current.launcher.enabled = true;
    current.library.kind = crate::model::LibraryKind::Favorites;
    assert_ne!(copy::confirm_text(&current), "游戏库");
    current.entries[0].directory = true;
    current.entries[0].icon = "library".into();
    assert_eq!(copy::confirm_text(&current), "游戏库");
    current.busy = true;
    assert_eq!(copy::confirm_text(&current), "");
    current.busy = false;
    current.entries[0].available = false;
    assert_ne!(copy::confirm_text(&current), "游戏库");
    current.entries.clear();
    assert_eq!(copy::confirm_text(&current), "");
}

#[test]
fn adaptive_shelf_retains_grid_selection_and_models_through_library_expansion() {
    for (size, rows) in [([480, 320], 1), ([640, 480], 2), ([800, 480], 2)] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.status.clear();
        current.library.kind = crate::model::LibraryKind::Favorites;
        current.library.total = 9;
        current.launcher.enabled = true;
        current.entries = (0..8)
            .map(|i| crate::model::GameRow {
                key: format!("cartridge-{i}"),
                name: format!("游戏 {i}"),
                icon: "nes".into(),
                available: true,
                ..Default::default()
            })
            .collect();
        let requests = Rc::new(std::cell::RefCell::new(Vec::new()));
        let received = requests.clone();
        subscribe_shelf_layout(&app, move |request| received.borrow_mut().push(*request)).unwrap();
        let mut scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 320);
        let plan = scene.shelf.shelf().plan();
        assert_eq!(plan.grid.rows, rows, "{size:?}");
        // Apply the same bounded visible window returned by the native browser.
        let capacity = plan.grid.columns * plan.grid.rows;
        current.entries.truncate(capacity);
        current.selected = capacity - 1;
        current.library.position = capacity;
        current.launcher.key = current.entries[current.selected].key.clone();
        current.launcher_shelf = Some(Box::new(current.clone()));
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 640);
        let ids: Vec<_> = scene.shelf.entries().entries().map(|(id, _)| id).collect();
        let actor = app
            .world
            .query::<actor::ActorFrame>()
            .iter()
            .next()
            .unwrap()
            .0;
        let target = app.world.get::<FollowBounds>(actor).unwrap().target;
        assert_eq!(
            app.world.get::<FocusTarget>(target).unwrap().index(),
            current.selected
        );
        let original = app.world.get::<ComputedRect>(target).unwrap().0;
        assert!(
            original.w > original.h,
            "{size:?}: cartridge must remain horizontal"
        );
        let emitted = requests.borrow().len();
        assert_eq!(emitted, 1, "Only the active shelf publishes geometry");
        let home = current.clone();
        current.launcher.tab = 1;
        current.library.kind = crate::model::LibraryKind::Platforms;
        current.platforms = true;
        current.selected = 0;
        current.launcher.key.clear();
        current.launcher_catalog = Some(Box::new(current.clone()));
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 1600);
        assert_eq!(scene.deck.expansion(), Fixed::ONE);
        assert_eq!(
            scene.shelf.shelf().plan(),
            plan,
            "Compression retains logical grid geometry"
        );
        assert_eq!(
            scene
                .shelf
                .entries()
                .entries()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            ids
        );
        update(&mut scene, &home).unwrap();
        paint_at(&mut app, &scene, 2600);
        assert_eq!(scene.deck.expansion(), Fixed::ZERO);
        assert_eq!(app.world.get::<FollowBounds>(actor).unwrap().target, target);
        assert_eq!(app.world.get::<ComputedRect>(target).unwrap().0, original);
        assert_eq!(
            requests.borrow().len(),
            emitted,
            "Motion and return cannot renegotiate the grid"
        );
        assert_full_frame_matches(&mut app, "Retained landscape shelf return");
    }
}

#[test]
fn launcher_shortcuts_and_catalog_fit_after_expanding_the_favorite_shelf() {
    for size in [[480, 320], [640, 480], [800, 480]] {
        let mut app = app(size);
        let mut shelf = view(Page::Games);
        shelf.status.clear();
        shelf.library.kind = crate::model::LibraryKind::Favorites;
        shelf.entries.truncate(3);
        shelf.launcher.enabled = true;
        shelf.launcher.key = "row-0".into();
        shelf.launcher_shelf = Some(Box::new(shelf.clone()));
        let mut scene = draw(&mut app, &shelf).unwrap();
        let mut time = 0;
        for kind in [
            crate::model::LibraryKind::Favorites,
            crate::model::LibraryKind::Platforms,
            crate::model::LibraryKind::Directory,
            crate::model::LibraryKind::Search,
            crate::model::LibraryKind::Favorites,
        ] {
            let mut current = shelf.clone();
            if kind != crate::model::LibraryKind::Favorites {
                current.library.kind = kind;
                current.library.keyboard = kind == crate::model::LibraryKind::Search;
                current.platforms = kind == crate::model::LibraryKind::Platforms;
                current.launcher.tab = 1;
                if kind != crate::model::LibraryKind::Directory {
                    current.launcher.key.clear();
                }
                current.launcher_catalog = Some(Box::new(current.clone()));
            }
            update(&mut scene, &current).unwrap();
            for _ in 0..13 {
                time += 64;
                TIME.with(|clock| clock.set(time * 1_000_000));
                super::paint_at(&mut app, time).unwrap();
            }
            let visible = |mut entity| {
                loop {
                    if mirui::ui::branch::is_effectively_hidden(&app.world, entity) {
                        return false;
                    }
                    let Some(parent) = app.world.get::<mirui::ui::Parent>(entity) else {
                        return true;
                    };
                    entity = parent.0;
                }
            };
            let (footer, bounds) = app
                .world
                .query::<Section>()
                .iter()
                .find_map(|(entity, section)| {
                    (*section == Section::Footer)
                        .then(|| (entity, app.world.get::<ComputedRect>(entity).unwrap().0))
                })
                .unwrap();
            assert!(bounds.h <= Fixed::from_int(28));
            assert!(bounds.y + bounds.h <= Fixed::from_int(i32::from(size[1])));
            for (entity, text) in app.world.query::<Text>().iter() {
                if selection::contains(footer, entity, |node| {
                    app.world
                        .get::<mirui::ui::Parent>(node)
                        .map(|parent| parent.0)
                }) {
                    if !visible(entity) {
                        continue;
                    }
                    let area = app.world.get::<ComputedRect>(entity).unwrap().0;
                    assert!(
                        area.x >= bounds.x
                            && area.x + area.w <= bounds.x + bounds.w
                            && area.y >= bounds.y
                            && area.y + area.h <= bounds.y + bounds.h,
                        "{size:?} {kind:?}: {} escapes shortcuts",
                        text.resolve(app.world.resource())
                    );
                }
            }
            if kind != crate::model::LibraryKind::Favorites {
                assert_eq!(scene.deck.expansion(), Fixed::ONE);
                let (catalog, area) = app
                    .world
                    .query::<deck::ActorScope>()
                    .iter()
                    .find_map(|(entity, scope)| {
                        matches!(scope, deck::ActorScope::Layer(deck::Layer::Catalog))
                            .then(|| (entity, app.world.get::<ComputedRect>(entity).unwrap().0))
                    })
                    .unwrap();
                assert_eq!(area.h, scene.deck.height() * deck::FULL);
                for (entity, _) in app.world.query::<FocusTarget>().iter() {
                    if visible(entity)
                        && selection::contains(catalog, entity, |node| {
                            app.world
                                .get::<mirui::ui::Parent>(node)
                                .map(|parent| parent.0)
                        })
                    {
                        let target = app.world.get::<ComputedRect>(entity).unwrap().0;
                        assert!(
                            target.y >= area.y && target.y + target.h <= area.y + area.h,
                            "{size:?} {kind:?}: row exceeds the catalog's reduced height"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn action_panels_keep_labels_inside_their_cards() {
    for size in [[480, 320], [640, 480], [800, 480]] {
        for (kind, labels) in [
            ("game", vec!["开始游戏", "附近联机", "移出收藏", "返回"]),
            (
                "tools",
                vec![
                    "Wi-Fi 设置",
                    "网络信息",
                    "存储空间",
                    "文件管理",
                    "系统信息",
                    "返回",
                ],
            ),
            (
                "system",
                vec![
                    "切换主题",
                    "音效开关",
                    "系统控制",
                    "管理直连网络",
                    "返回原系统",
                    "电源",
                ],
            ),
            ("controls", vec!["音量", "亮度", "系统工具", "更多设置"]),
            (
                "network",
                vec!["保留直连，开始游戏", "恢复 Wi-Fi 后开始", "返回"],
            ),
            ("exit", vec!["返回", "退出并恢复 Wi-Fi"]),
            ("power", vec!["返回", "确认关机", "确认重启"]),
        ] {
            let mut app = app(size);
            let mut current = view(Page::Games);
            current.library.kind = crate::model::LibraryKind::Favorites;
            current.launcher.enabled = true;
            current.launcher.overlay = "超级马里奥世界 / Super Mario World".into();
            current.launcher.overlay_kind = kind.into();
            current.launcher.items = labels
                .iter()
                .map(|label| {
                    let mut item = crate::model::LauncherItem {
                        name: (*label).into(),
                        ..Default::default()
                    };
                    if kind == "controls"
                        && (label.starts_with("音量") || label.starts_with("亮度"))
                    {
                        let control = if label.starts_with("音量") {
                            howff_menu::SystemLevel::Volume
                        } else {
                            howff_menu::SystemLevel::Brightness
                        };
                        item.action = control.action();
                        item.level = Some(howff_menu::Level::new(control, Some(100), false, false));
                    } else if kind == "controls" && *label == "系统工具" {
                        item.action = crate::model::MenuAction::ShowTools;
                    } else if kind == "tools" {
                        item.action = match *label {
                            "Wi-Fi 设置" => crate::model::MenuAction::OpenWifi,
                            "网络信息" => crate::model::MenuAction::NetworkInfo,
                            "文件管理" => crate::model::MenuAction::FileManager,
                            "系统信息" => crate::model::MenuAction::SystemInfo,
                            "存储空间" => crate::model::MenuAction::ShowStorage,
                            _ => crate::model::MenuAction::Back,
                        };
                    }
                    item
                })
                .collect();
            let scene = draw(&mut app, &current).unwrap();
            paint_at(&mut app, &scene, 400);
            let anchors: Vec<_> = app
                .world
                .query::<deck::ActionAnchor>()
                .iter()
                .filter_map(|(entity, _)| {
                    app.world
                        .get::<ComputedRect>(entity)
                        .map(|rect| (entity, rect.0))
                })
                .collect();
            assert_eq!(anchors.len(), labels.len());
            for (_, bounds) in &anchors {
                assert!(bounds.x >= Fixed::ZERO && bounds.y >= Fixed::ZERO);
                assert!(bounds.x + bounds.w <= Fixed::from_int(i32::from(size[0])));
                assert!(bounds.y + bounds.h <= Fixed::from_int(i32::from(size[1])));
            }
            for (entity, text) in app.world.query::<Text>().iter() {
                let Some(bounds) = app.world.get::<ComputedRect>(entity).map(|rect| rect.0) else {
                    continue;
                };
                for &(card, area) in &anchors {
                    if selection::contains(card, entity, |node| {
                        app.world
                            .get::<mirui::ui::Parent>(node)
                            .map(|parent| parent.0)
                    }) {
                        assert!(
                            bounds.x >= area.x
                                && bounds.x + bounds.w <= area.x + area.w
                                && bounds.y >= area.y
                                && bounds.y + bounds.h <= area.y + area.h,
                            "{size:?} {kind}: {} escapes card",
                            text.resolve(app.world.resource())
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn settled_content_preserves_pixels_and_survives_viewport_motion() {
    use mirui::ui::OffscreenRender;
    let mut app = app([640, 480]);
    let mut current = view(Page::Games);
    current.library.kind = crate::model::LibraryKind::Favorites;
    current.launcher.enabled = true;
    let mut scene = draw(&mut app, &current).unwrap();
    paint_at(&mut app, &scene, 1000);
    let shelf_frame = app
        .world
        .query::<selection::SelectionFrame>()
        .iter()
        .find(|(_, frame)| frame.page.library == crate::model::LibraryKind::Favorites)
        .unwrap()
        .0;
    let original_target = app.world.get::<FollowBounds>(shelf_frame).unwrap().target;
    let original_bounds = app.world.get::<ComputedRect>(original_target).unwrap().0;
    let mut panel = view(Page::Mode);
    panel.return_to_launcher = true;
    update(&mut scene, &panel).unwrap();
    paint_at(&mut app, &scene, 1400);
    let layers: Vec<_> = app
        .world
        .query::<cache::SettledLayerCache>()
        .iter()
        .map(|(entity, _)| entity)
        .collect();
    // The hidden catalog has no mounted page or content output yet.
    assert_eq!(layers.len(), 1);
    let frames: Vec<_> = app
        .world
        .query::<selection::SelectionFrame>()
        .iter()
        .map(|(entity, _)| entity)
        .collect();
    for &layer in &layers {
        assert!(
            frames
                .iter()
                .all(|&frame| !selection::contains(layer, frame, |node| app
                    .world
                    .get::<mirui::ui::Parent>(node)
                    .map(|parent| parent.0)))
        );
    }
    assert!(
        layers
            .iter()
            .all(|&entity| app.world.has::<OffscreenRender>(entity))
    );
    for palette in [crate::theme::Palette::Forest, crate::theme::Palette::Sunset] {
        app.set_theme(palette.theme()).unwrap();
        super::paint_at(&mut app, 1400).unwrap();
        let cached = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        for &entity in &layers {
            app.world.remove::<OffscreenRender>(entity);
        }
        app.render().unwrap();
        assert_eq!(
            cached,
            app.backend.framebuffer().read_pixels().unwrap().bytes()
        );
    }
    scene.press(1400);
    paint_at(&mut app, &scene, 1416);
    assert!(
        layers
            .iter()
            .all(|&entity| app.world.has::<OffscreenRender>(entity))
    );
    paint_at(&mut app, &scene, 2000);
    assert!(
        layers
            .iter()
            .all(|&entity| app.world.has::<OffscreenRender>(entity))
    );
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 2400);
    let shelf = app
        .world
        .query::<cache::SettledLayerCache>()
        .iter()
        .find(|(_, layer)| layer.state.launcher().view().tab == 0)
        .unwrap()
        .0;
    assert!(!app.world.has::<OffscreenRender>(shelf));
    assert!(
        app.world
            .resource::<mirui::ui::Outputs<mirui::ui::WidgetTexture>>()
            .unwrap()
            .get(shelf)
            .is_none()
    );
    assert_eq!(
        app.world.get::<FollowBounds>(shelf_frame).unwrap().target,
        original_target
    );
    assert_full_frame_matches(&mut app, "Active full shelf after the room panel closes");
    let home = current.clone();
    let mut catalog = view(Page::Games);
    catalog.library.kind = crate::model::LibraryKind::Platforms;
    catalog.platforms = true;
    current.launcher_shelf = Some(Box::new(home.clone()));
    current.launcher_catalog = Some(Box::new(catalog));
    current.launcher.tab = 1;
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 2416);
    assert!(scene.deck.layer() == deck::Layer::Catalog);
    assert!(!scene.deck.viewport_settled());
    assert!(!app.world.has::<OffscreenRender>(shelf));
    let frozen = app
        .world
        .resource::<suspension::Runtime>()
        .unwrap()
        .model
        .clone();
    assert!(!frozen.hidden());
    paint_at(&mut app, &scene, 2500);
    assert!(scene.deck.expansion() > Fixed::ZERO && scene.deck.expansion() < Fixed::ONE);
    assert!(!app.world.has::<OffscreenRender>(shelf));
    assert!(!frozen.hidden());
    paint_at(&mut app, &scene, 3100);
    assert_eq!(scene.deck.expansion(), Fixed::ONE);
    assert!(scene.deck.viewport_settled());
    assert!(frozen.hidden() && frozen.paused());
    let token = frozen
        .token()
        .expect("The compressed shelf owns its prepared snapshot");
    let textures = app.world.resource::<mirui::core::resource::ResourceManager<mirui::render::texture::Texture<'static>>>().unwrap();
    let picture = textures.resolve(&suspension::texture_token(token));
    assert!(picture.width() > 0 && picture.height() > 0);
    assert_full_frame_matches(&mut app, "Compressed shelf snapshot and catalog");
    update(&mut scene, &home).unwrap();
    paint_at(&mut app, &scene, 3116);
    assert!(!frozen.hidden() && frozen.token().is_none());
    paint_at(&mut app, &scene, 3800);
    assert_eq!(scene.deck.expansion(), Fixed::ZERO);
    assert!(!app.world.has::<OffscreenRender>(shelf));
    assert!(!app.world.has::<mirui::ui::Hidden>(shelf_frame));
    assert_eq!(
        app.world.get::<FollowBounds>(shelf_frame).unwrap().target,
        original_target
    );
    assert_eq!(
        app.world.get::<ComputedRect>(original_target).unwrap().0,
        original_bounds
    );
    assert_full_frame_matches(&mut app, "Restored full shelf and original focus");
}

#[test]
fn shelf_output_reuses_pixels_during_catalog_loading_and_refreshes_content() {
    use mirui::core::resource::ResourceManager;
    use mirui::render::texture::Texture;
    let mut app = app([640, 480]);
    let mut shelf = view(Page::Games);
    shelf.library.kind = crate::model::LibraryKind::Favorites;
    shelf.refreshing = true;
    let mut current = view(Page::Games);
    current.library.kind = crate::model::LibraryKind::Platforms;
    current.platforms = true;
    current.launcher.enabled = true;
    current.launcher.tab = 1;
    current.launcher_shelf = Some(Box::new(shelf));
    current.launcher_catalog = Some(Box::new(current.clone()));
    let mut scene = draw(&mut app, &current).unwrap();
    paint_at(&mut app, &scene, 1000);
    let model = app
        .world
        .resource::<suspension::Runtime>()
        .unwrap()
        .model
        .clone();
    assert!(model.hidden());
    let token = model.token().unwrap();
    let original = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap()
        .resolve(&suspension::texture_token(token))
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    for (step, selected) in [1, 0, 1].into_iter().enumerate() {
        current.selected = selected;
        current.launcher_catalog.as_mut().unwrap().selected = selected;
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 1100 + step as u64 * 100);
        assert!(model.hidden());
        assert_eq!(model.token(), Some(token));
    }
    current.launcher_shelf.as_mut().unwrap().entries[0].name = "Changed title".into();
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 1400);
    assert!(!model.hidden());
    paint_at(&mut app, &scene, 2000);
    assert!(model.hidden());
    let updated = model.token().unwrap();
    assert_ne!(updated, token);
    let textures = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    let output = textures.resolve(&suspension::texture_token(updated));
    assert!(
        original != output.read_pixels().unwrap().bytes(),
        "Changed content must replace cached pixels"
    );
}

#[test]
fn launcher_sheets_and_room_overlay_retain_their_own_rows() {
    let mut app = app([640, 480]);
    let mut shelf = view(Page::Games);
    shelf.library.kind = crate::model::LibraryKind::Favorites;
    let mut catalog = view(Page::Games);
    catalog.library.kind = crate::model::LibraryKind::Platforms;
    catalog.platforms = true;
    let mut initial = shelf.clone();
    initial.launcher.enabled = true;
    initial.launcher_shelf = Some(Box::new(shelf));
    initial.launcher_catalog = Some(Box::new(catalog));
    let mut scene = draw(&mut app, &initial).unwrap();
    let shelf_ids: Vec<_> = scene.shelf.entries().entries().map(|(id, _)| id).collect();
    let catalog_ids: Vec<_> = scene
        .catalog
        .entries()
        .entries()
        .map(|(id, _)| id)
        .collect();
    for (step, tab) in [1, 0, 1, 0].into_iter().enumerate() {
        initial.launcher.tab = tab;
        update(&mut scene, &initial).unwrap();
        paint_at(&mut app, &scene, (step as u64 + 1) * 160);
        assert_eq!(
            scene
                .shelf
                .entries()
                .entries()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            shelf_ids
        );
        assert_eq!(
            scene
                .catalog
                .entries()
                .entries()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            catalog_ids
        );
    }
    let mut room = view(Page::Choice);
    room.return_to_launcher = true;
    update(&mut scene, &room).unwrap();
    paint_at(&mut app, &scene, 1100);
    assert!(scene.deck.layer() == deck::Layer::Room);
    room.page = Page::Rooms;
    update(&mut scene, &room).unwrap();
    paint_at(&mut app, &scene, 1500);
    assert_eq!(scene.room.rooms().len(), room.rooms.len());
    assert_eq!(
        scene
            .shelf
            .entries()
            .entries()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        shelf_ids
    );
    update(&mut scene, &initial).unwrap();
    paint_at(&mut app, &scene, 2000);
    assert_eq!(scene.deck.panel(), Fixed::ZERO);
    assert_eq!(
        scene
            .catalog
            .entries()
            .entries()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        catalog_ids
    );
}

#[test]
fn launcher_overlays_keep_library_identity_and_return_to_the_same_selection() {
    let mut app = app([640, 480]);
    let mut initial = view(Page::Games);
    initial.library.kind = crate::model::LibraryKind::Favorites;
    initial.launcher = crate::model::LauncherView {
        enabled: true,
        key: "local-game".into(),
        ..Default::default()
    };
    let mut scene = draw(&mut app, &initial).unwrap();
    let launcher = super::launcher(&mut app);
    let ids: Vec<_> = scene.state.entries().entries().map(|(id, _)| id).collect();
    for (index, title) in ["游戏操作", "启动器设置", ""].into_iter().enumerate() {
        let mut next = initial.clone();
        next.launcher.overlay = title.into();
        next.launcher.items = vec![crate::model::LauncherItem {
            name: "开始游戏".into(),
            ..Default::default()
        }];
        update(&mut scene, &next).unwrap();
        paint_at(&mut app, &scene, 1000 + index as u64 * 1000);
        assert_eq!(
            scene
                .state
                .entries()
                .entries()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(launcher.blocked(), !title.is_empty());
    }
    assert!(focus_rect(&app).is_some());
}

#[test]
fn game_row_routes_replace_contents_and_retain_them_only_on_departure() {
    for initial_page in [Page::Lobby, Page::Games] {
        let mut app = app([640, 480]);
        let mut scene = draw(&mut app, &view(initial_page)).unwrap();
        for page in [Page::Candidates, Page::Games, Page::Candidates] {
            let mut current = view(page);
            current.entries.clear();
            update(&mut scene, &current).unwrap();
            assert!(
                scene.state.entries().is_empty(),
                "Empty {page:?} clears old rows"
            );

            current.entries = view(page).entries;
            current.entries.truncate(2);
            current.entries[0].name = format!("{page:?} candidate");
            update(&mut scene, &current).unwrap();
            let labels = || {
                scene
                    .state
                    .entries()
                    .entries()
                    .map(|(_, row)| row.label(true))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                labels(),
                current
                    .entries
                    .iter()
                    .map(|row| row.name.clone())
                    .collect::<Vec<_>>()
            );
            let identities: Vec<_> = scene.state.entries().entries().map(|(id, _)| id).collect();
            current.entries[0].detail = "Matching completed".into();
            current.entries[0].available = true;
            current.selected = 1;
            update(&mut scene, &current).unwrap();
            assert_eq!(
                scene
                    .state
                    .entries()
                    .entries()
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>(),
                identities,
                "Background updates preserve row identities"
            );
            assert_eq!(scene.state.selected(), Some(1));
            assert_eq!(
                scene
                    .state
                    .entries()
                    .entries()
                    .next()
                    .unwrap()
                    .1
                    .label(false),
                "Matching completed"
            );
            paint_at(&mut app, &scene, now_ns() / 1_000_000 + 1000);
            assert!(
                focus_rect(&app).is_some(),
                "Rows have a focus target in {page:?}"
            );

            update(
                &mut scene,
                &View {
                    page: Page::Rooms,
                    ..View::default()
                },
            )
            .unwrap();
            assert_eq!(
                scene.state.entries().entries().len(),
                2,
                "Departure retains rows"
            );
        }
    }
}

#[test]
fn library_content_is_owned_by_its_route_during_page_departure() {
    for kind in [
        crate::model::LibraryKind::Platforms,
        crate::model::LibraryKind::Directory,
        crate::model::LibraryKind::Favorites,
        crate::model::LibraryKind::Search,
    ] {
        let mut app = app([640, 480]);
        let mut current = view(Page::Games);
        current.library.kind = kind;
        current.platforms = kind == crate::model::LibraryKind::Platforms;
        current.selected = 1;
        current.artwork = "retained-cover".into();
        let mut scene = draw(&mut app, &current).unwrap();
        let count = scene.state.entries().entries().len();
        scene.navigation_intent(true);
        update(
            &mut scene,
            &View {
                page: Page::Lobby,
                ..View::default()
            },
        )
        .unwrap();
        for at in [16, 32, 64, 128, 256, 1000] {
            paint_at(&mut app, &scene, at);
            assert_eq!(scene.state.entries().entries().len(), count);
            assert_eq!(scene.state.artwork(), "retained-cover");
            assert_eq!(scene.state.library_selection(), 1);
            assert_eq!(scene.state.platforms(), current.platforms);
            assert!(!app.world.query::<Text>().iter().any(|(entity, text)| {
                if text.resolve(None) != "没有可选游戏" {
                    return false;
                }
                let mut node = Some(entity);
                while let Some(entity) = node {
                    if mirui::ui::branch::is_effectively_hidden(&app.world, entity) {
                        return false;
                    }
                    node = app.world.get::<mirui::ui::Parent>(entity).map(|p| p.0);
                }
                true
            }));
        }
        current.entries.clear();
        current.artwork.clear();
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 2000);
        assert!(
            scene.state.entries().is_empty(),
            "A real empty library still updates"
        );
        assert!(scene.state.artwork().is_empty());
    }
}

#[test]
fn library_routes_keep_focus_inside_their_cards_and_keyboard() {
    use crate::model::LibraryKind;
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.library.kind = LibraryKind::Favorites;
        current.entries.truncate(2);
        current.library.total = current.entries.len();
        let mut scene = draw(&mut app, &current).unwrap();
        for index in 0..current.entries.len() {
            current.selected = index;
            update(&mut scene, &current).unwrap();
            paint_at(&mut app, &scene, 1000 + index as u64 * 1000);
            let [x, y, w, h] = focus_rect(&app).expect("Favorite has a focus target");
            assert!(
                x >= 0
                    && y >= 0
                    && w > 0
                    && h > 0
                    && x + w <= i32::from(size[0])
                    && y + h <= i32::from(size[1])
            );
            assert_full_frame_matches(&mut app, "Favorite focus and card motion");
        }
        current.library.kind = LibraryKind::Search;
        current.library.keyboard = true;
        current.library.query = "ZDR".into();
        for index in [0, 9, 19, 29, 39] {
            current.selected = 100 + index;
            current.library.key_selected = index;
            update(&mut scene, &current).unwrap();
            for at in [5000, 5500, 6000] {
                paint_at(&mut app, &scene, at + index as u64 * 1000);
            }
            let [x, y, w, h] = focus_rect(&app).expect("Keyboard has a focus target");
            assert!(
                x >= 0
                    && y >= 0
                    && w > 0
                    && h > 0
                    && x + w <= i32::from(size[0])
                    && y + h <= i32::from(size[1])
            );
        }
        current.library.keyboard = false;
        current.selected = 0;
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 50000);
        assert!(focus_rect(&app).is_some());
    }
}
use super::{copy::*, selection::FocusTarget};
use crate::model::{GameRow, Page, Phase, RoomRow};
use mirui::core::model::ModelHandle;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::PhysicalRect;
use mirui::ui::ComputedRect;
use mirui::ui::OffscreenGeneration;
use mirui::ui::render_system::LastDirtyRegions;
use mirui::ui::widgets::FollowBounds;
use mirui::ui::widgets::Text;
use std::cell::Cell;
use std::rc::Rc;

type TestApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

#[test]
fn sprite_observers_ignore_unrelated_motion_and_track_selection_and_scan() {
    use mirui::core::reactive::{Effect, flush_signal_dirty};
    let mut app = app([640, 480]);
    let mut current = view(Page::Rooms);
    let scene = draw(&mut app, &current).unwrap();
    scene.state.animate(0, 0);
    scene.state.animate(1000, 0);
    flush_signal_dirty(&mut app.world);
    let selectors: [fn(&DisplayHandle) -> motion::Frame; 5] = [
        DisplayHandle::choice_motion,
        DisplayHandle::walker_motion,
        DisplayHandle::session_motion,
        DisplayHandle::signal_motion,
        DisplayHandle::search_motion,
    ];
    let counts = Rc::new(Cell::new([0; 5]));
    let _effects: Vec<_> = selectors
        .into_iter()
        .enumerate()
        .map(|(index, read)| {
            let state = scene.state.clone();
            let counts = counts.clone();
            Effect::new(move || {
                let _ = read(&state);
                let mut next = counts.get();
                next[index] += 1;
                counts.set(next);
            })
        })
        .collect();
    assert_eq!(counts.get(), [1; 5]);
    scene.state.animate(1000, 1);
    flush_signal_dirty(&mut app.world);
    assert_eq!(counts.get(), [1; 5], "Button press does not redraw artwork");
    current.selected = 1;
    scene.state.present(&current);
    scene.state.animate(1000, 1);
    flush_signal_dirty(&mut app.world);
    assert_eq!(counts.get(), [2, 2, 1, 2, 1]);
    current.refreshing = true;
    scene.state.present(&current);
    scene.state.animate(1000, 1);
    flush_signal_dirty(&mut app.world);
    assert_eq!(counts.get(), [2, 2, 2, 3, 2]);
}

#[test]
fn retained_lists_start_below_status_at_every_row_count() {
    for size in [[480, 320], [640, 480]] {
        for page in [Page::Rooms, Page::Games] {
            let mut app = app(size);
            let mut current = view(page);
            current.rooms.truncate(1);
            current.entries.truncate(1);
            let mut scene = draw(&mut app, &current).unwrap();
            for (step, count) in [1, 4, 2, 0, 1].into_iter().enumerate() {
                current = view(page);
                current.rooms.truncate(count);
                current.entries.truncate(count);
                update(&mut scene, &current).unwrap();
                paint_at(&mut app, &scene, (step as u64 + 1) * 1000);
                if page == Page::Rooms {
                    assert_eq!(confirm_text(&current), if count == 0 { "" } else { "加入" });
                }
                if count > 0 {
                    let top = focus_rect(&app).unwrap()[1];
                    let status = rect(&app, "nearby_status");
                    let gap = top - (status.y + status.h).to_int();
                    assert!(
                        (0..=style::Metrics::for_height(size[1]).gap * 2).contains(&gap),
                        "{size:?} {page:?} count={count} gap={gap}"
                    );
                }
                assert_full_frame_matches(
                    &mut app,
                    "Retained list decoration and empty transition",
                );
            }
        }
    }
}

#[test]
fn theme_changes_preserve_scene_state_and_refresh_cached_pixels() {
    for size in [[320, 240], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Choice);
        current.selected = 1;
        let mut scene = draw(&mut app, &current).unwrap();
        let root = app.root;
        let page = body(&app);
        paint_at(&mut app, &scene, 1000);
        let selection = focus_rect(&app);
        for index in 1..=3 {
            assert!(cycle_theme(&mut app, &mut scene).unwrap().is_some());
            paint_at(&mut app, &scene, 1000 + index * 100);
            assert_eq!(app.root, root);
            assert_eq!(body(&app), page);
            assert_eq!(focus_rect(&app), selection);
            assert_eq!(scene.state.snapshot(), current);
            assert_full_frame_matches(&mut app, "theme switch invalidates all cached colors");
        }
        current.busy = true;
        update(&mut scene, &current).unwrap();
        assert!(cycle_theme(&mut app, &mut scene).unwrap().is_none());
        current.busy = false;
        current.page = Page::Host;
        update(&mut scene, &current).unwrap();
        assert!(cycle_theme(&mut app, &mut scene).unwrap().is_none());
    }
}

thread_local! { static TIME: Cell<u64> = const { Cell::new(0) }; }

fn now_ns() -> u64 {
    TIME.with(Cell::get)
}

fn body(app: &TestApp) -> Entity {
    app.world
        .query::<selection::SelectionFrame>()
        .iter()
        .find(|(_, frame)| frame.page == frame.state.page_key())
        .unwrap()
        .1
        .body
}

fn focus_rect(app: &TestApp) -> Option<[i32; 4]> {
    let (entity, _) = app
        .world
        .query::<selection::SelectionFrame>()
        .iter()
        .find(|(_, frame)| frame.page == frame.state.page_key())?;
    let target = app.world.get::<FollowBounds>(entity)?.target;
    let rect = app.world.get::<ComputedRect>(target)?.0;
    Some([
        rect.x.to_int(),
        rect.y.to_int(),
        rect.w.to_int(),
        rect.h.to_int(),
    ])
}

fn paint_at<B: FramebufferAccess>(app: &mut App<B>, _scene: &Scene, time: u64) {
    let mut current = now_ns() / 1_000_000;
    while current < time {
        current = (current + 16).min(time);
        TIME.with(|clock| clock.set(current * 1_000_000));
        super::paint_at(app, current).unwrap();
    }
}

fn offset_focus(app: &mut TestApp, axis: usize) {
    let node = app
        .world
        .query::<selection::SelectionFrame>()
        .iter()
        .next()
        .unwrap()
        .0;
    let bounds = app.world.get_mut::<FollowBounds>(node).unwrap();
    bounds.transition = None;
    if axis == 0 {
        bounds.offset.x += Fixed::ONE;
    } else {
        bounds.offset.y += Fixed::ONE;
    }
}

fn app(size: [u16; 2]) -> TestApp {
    app_with_format(size, ColorFormat::RGBA8888)
}

fn app_with_format(size: [u16; 2], format: ColorFormat) -> TestApp {
    let surface = FramebufSurface::with_format(
        size[0],
        size[1],
        format,
        (|_, _| {}) as fn(&[u8], PhysicalRect),
    );
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    install(&mut app).unwrap();
    TIME.with(|clock| clock.set(0));
    app.add_resource(mirui::ecs::MonoClock::new(now_ns));
    app
}

fn view(page: Page) -> View {
    View {
        page,
        title: "游戏标题 / Multiplayer".into(),
        core: "NES / FC".into(),
        room_name: "房间 A216".into(),
        rooms: (0..4)
            .map(|i| RoomRow {
                key: format!("row-{i}"),
                name: format!("房间 {i}"),
                detail: "同一游戏，可以加入".into(),
                signal: 70,
                available: true,
            })
            .collect(),
        entries: (0..4)
            .map(|i| GameRow {
                artwork: String::new(),
                favorite: false,
                key: format!("row-{i}"),
                name: format!("游戏 {i} / Multiplayer"),
                detail: "NES / FC".into(),
                directory: false,
                icon: "nes".into(),
                available: true,
            })
            .collect(),
        status: "请选择".into(),
        ..View::default()
    }
}

fn section(app: &TestApp, kind: Section) -> Entity {
    let root = body(app);
    app.world
        .query::<Section>()
        .iter()
        .find(|(entity, section)| {
            **section == kind
                && selection::contains(root, *entity, |node| {
                    app.world
                        .get::<mirui::ui::Parent>(node)
                        .map(|parent| parent.0)
                })
        })
        .unwrap()
        .0
}

fn rect(app: &TestApp, id: &'static str) -> Rect {
    let kind = match id {
        "nearby_header" => Section::Header,
        "nearby_content" => Section::Content,
        "nearby_footer" => Section::Footer,
        "nearby_status" => Section::Status,
        _ => panic!("unknown section"),
    };
    app.world.get::<ComputedRect>(section(app, kind)).unwrap().0
}

fn cache_generations(app: &TestApp) -> Vec<(Entity, u32)> {
    app.world
        .query::<StaticCache>()
        .iter()
        .map(|(entity, _)| {
            (
                entity,
                app.world
                    .get::<OffscreenGeneration>(entity)
                    .map_or(0, |v| v.0),
            )
        })
        .collect()
}

fn assert_full_frame_matches(app: &mut TestApp, context: &str) {
    let incremental = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    let full_frame = app.backend.framebuffer();
    let full_pixels = full_frame.read_pixels().unwrap();
    let full = full_pixels.bytes();
    assert_eq!(
        incremental.iter().zip(full).filter(|(a, b)| a != b).count(),
        0,
        "{context}"
    );
}

#[test]
fn dynamic_updates_preserve_static_caches_and_one_model_registration() {
    for page in [
        Page::Mode,
        Page::Choice,
        Page::Rooms,
        Page::Lobby,
        Page::Candidates,
        Page::Games,
        Page::Host,
        Page::Joined,
        Page::Leaving,
        Page::Unavailable,
    ] {
        let mut app = app([640, 480]);
        let initial = view(page);
        let mut scene = draw(&mut app, &initial).unwrap();
        let registered = scene.state.clone();
        let identity = registered.read(|model| std::ptr::from_ref(model) as usize);
        let generations = cache_generations(&app);
        assert!(!generations.is_empty());
        let mut next = initial.clone();
        next.status = "正在准备".into();
        update(&mut scene, &next).unwrap();

        paint_at(&mut app, &scene, 70);
        assert_eq!(cache_generations(&app), generations, "{page:?}");
        let previous_root = app.root.unwrap();
        let previous_page = body(&app);
        let next_scene = draw(
            &mut app,
            &view(if page == Page::Choice {
                Page::Mode
            } else {
                Page::Choice
            }),
        )
        .unwrap();
        paint_at(&mut app, &next_scene, 1500);
        assert_eq!(app.root, Some(previous_root));
        assert!(app.world.is_alive(previous_root));
        assert!(!app.world.is_alive(previous_page));
        assert_eq!(
            identity,
            next_scene
                .state
                .read(|model| std::ptr::from_ref(model) as usize)
        );
        drop(scene);
        drop(next_scene);
        drop(app);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| registered.status())).is_err(),
            "Model lifetime must end with its App"
        );
    }
}

#[test]
fn text_fields_invalidate_their_own_bindings_and_old_page_bindings_are_released() {
    let mut app = app([640, 480]);
    let initial = view(Page::Choice);
    let mut scene = draw(&mut app, &initial).unwrap();
    let texts: Vec<_> = app.world.query::<Text>().iter().map(|(e, _)| e).collect();
    let status_entity = section(&app, Section::Status);
    let mut next = initial.clone();
    next.status = "准备完成".into();
    update(&mut scene, &next).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(
        app.world
            .get::<Text>(status_entity)
            .unwrap()
            .resolve(app.world.resource())
            .as_ref(),
        "准备完成"
    );
    scene.navigation_intent(false);
    scene.state.present(&view(Page::Unavailable));
    paint_at(&mut app, &scene, 1500);
    assert!(texts.iter().all(|e| !app.world.is_alive(*e)));
    let next_texts: Vec<_> = app
        .world
        .query::<Text>()
        .iter()
        .map(|(entity, _)| entity)
        .collect();
    assert!(!next_texts.is_empty());
    scene.state.present(&view(Page::Unavailable));
    app.render_dirty().unwrap();
    assert!(next_texts.iter().all(|e| app.world.is_alive(*e)));
}

#[test]
fn responsive_layout_keeps_focus_and_text_sections_inside_the_viewport() {
    for size in [[480, 320], [640, 480], [800, 480]] {
        for page in [
            Page::Mode,
            Page::Choice,
            Page::Rooms,
            Page::Lobby,
            Page::Candidates,
            Page::Games,
            Page::Host,
            Page::Joined,
            Page::Leaving,
            Page::Unavailable,
        ] {
            let mut app = app(size);
            let mut view = view(page);
            view.ready = true;
            view.selected = 3;
            view.status = "正在核对两台设备的游戏和模拟核心，请稍候。Preparing the selected game and checking the connection before starting multiplayer.".into();
            view.title = "较长的游戏标题 / A long multiplayer game title that should fit its available space".into();
            let _scene = draw(&mut app, &view).unwrap();
            let header = rect(&app, "nearby_header");
            assert_eq!(header.x, Fixed::ZERO, "Header must span the viewport");
            assert_eq!(header.y, Fixed::ZERO, "Header must start at the top edge");
            assert_eq!(header.w, Fixed::from_int(i32::from(size[0])));
            let body = rect(&app, "nearby_content");
            let footer = rect(&app, "nearby_footer");
            let status = rect(&app, "nearby_status");
            assert!(header.y + header.h <= body.y, "{size:?} {page:?}");
            assert!(body.y + body.h <= footer.y, "{size:?} {page:?}");
            assert!(status.y + status.h <= body.y + body.h, "{size:?} {page:?}");
            assert!(footer.y + footer.h <= Fixed::from_int(i32::from(size[1])));
            for (entity, _) in app.world.query::<Text>().iter() {
                let style = app.world.get::<mirui::ui::Style>(entity).unwrap();
                let size = style.font_size.expect("Every label has a native font size");
                assert!(matches!(size, style::BODY_SIZE | style::TITLE_SIZE));
                assert_eq!(style.font_stack.iter().next(), Some(&style::font(size)));
                let mut node = entity;
                let mut hidden = false;
                loop {
                    hidden |= mirui::ui::branch::is_effectively_hidden(&app.world, node);
                    let Some(parent) = app.world.get::<mirui::ui::Parent>(node) else {
                        break;
                    };
                    node = parent.0;
                }
                if hidden {
                    continue;
                }
                let text = app
                    .world
                    .get::<ComputedRect>(entity)
                    .expect("Visible text must have computed layout")
                    .0;
                let mut ancestor = entity;
                while let Some(parent) = app.world.get::<mirui::ui::Parent>(ancestor) {
                    ancestor = parent.0;
                    if app.world.get::<FocusTarget>(ancestor).is_some() {
                        let row = app.world.get::<ComputedRect>(ancestor).unwrap().0;
                        assert!(
                            text.y >= row.y && text.y + text.h <= row.y + row.h,
                            "{size:?} {page:?}: text {text:?} escapes row {row:?}"
                        );
                        break;
                    }
                }
            }
            if let Some([x, y, w, h]) = focus_rect(&app) {
                assert!(w > 40 && h >= 26, "{size:?} {page:?}: {w}×{h}");
                assert!(
                    x >= 0 && y >= 0 && x + w <= i32::from(size[0]) && y + h <= footer.y.to_int(),
                    "{size:?} {page:?}: focus {:?} escapes footer {footer:?}",
                    [x, y, w, h]
                );
            }
        }
    }
}

#[test]
fn network_guidance_reserves_space_across_session_transitions() {
    for size in [[480, 320], [640, 480]] {
        for page in [Page::Host, Page::Joined] {
            let mut app = app(size);
            let mut current = view(page);
            current.network_only = true;
            current.phase = Phase::Creating;
            let mut scene = draw(&mut app, &current).unwrap();
            let mut time = 0;
            for phase in [
                Phase::Creating,
                Phase::WaitingPeer,
                Phase::ApprovalPending,
                Phase::Connecting,
                Phase::ApprovalPendingClient,
                Phase::NetworkConfirming,
                Phase::NetworkConnected,
            ] {
                current.phase = phase;
                for duplicate in [false, true, false] {
                    current.status = if duplicate {
                        next_step(&current).into()
                    } else {
                        "设备提供的当前状态".into()
                    };
                    update(&mut scene, &current).unwrap();
                    time += 100;
                    paint_at(&mut app, &scene, time);
                    if duplicate {
                        continue;
                    }
                    let hint = app
                        .world
                        .query::<Text>()
                        .iter()
                        .find(|(_, text)| {
                            text.resolve(app.world.resource()).as_ref() == next_step(&current)
                        })
                        .unwrap()
                        .0;
                    let hint_rect = app.world.get::<ComputedRect>(hint).unwrap().0;
                    let mut node = hint;
                    while let Some(parent) = app.world.get::<mirui::ui::Parent>(node) {
                        node = parent.0;
                        if let Some(bounds) = app.world.get::<ComputedRect>(node) {
                            assert!(
                                hint_rect.y >= bounds.0.y
                                    && hint_rect.y + hint_rect.h <= bounds.0.y + bounds.0.h,
                                "{size:?} {page:?} {phase:?}: hint {hint_rect:?} escapes ancestor {:?}",
                                bounds.0
                            );
                        }
                    }
                    assert_full_frame_matches(&mut app, "Guidance visibility transition");
                }
            }
        }
    }
}

#[test]
fn network_status_keeps_connection_actions_inside_the_viewport() {
    for size in [[480, 320], [640, 480], [800, 480]] {
        for page in [Page::Host, Page::Joined] {
            let mut app = app(size);
            let mut view = view(page);
            view.network_only = true;
            view.connected = true;
            view.phase = Phase::NetworkConnected;
            view.local_address = "192.168.49.123".into();
            view.back_label = "断开并恢复 Wi-Fi".into();
            view.peers = (0..3)
                .map(|index| crate::model::NetworkPeer {
                    name: format!("好友 {index} / Handheld"),
                    address: format!("192.168.49.{}", index + 2),
                })
                .collect();
            let _scene = draw(&mut app, &view).unwrap();
            let footer = rect(&app, "nearby_footer");
            let [x, y, w, h] = focus_rect(&app).unwrap();
            assert!(x >= 0 && y >= 0 && w > 40 && h >= 26);
            assert!(x + w <= i32::from(size[0]) && y + h <= footer.y.to_int());
            assert!(footer.y + footer.h <= Fixed::from_int(i32::from(size[1])));
            assert_eq!(confirm_text(&view), "返回系统");
            let text_rect = |value: &str| {
                let entity = app
                    .world
                    .query::<Text>()
                    .iter()
                    .find(|(_, text)| text.resolve(app.world.resource()).as_ref() == value)
                    .unwrap_or_else(|| panic!("Missing device text: {value}"))
                    .0;
                app.world.get::<ComputedRect>(entity).unwrap().0
            };
            let local = text_rect(&view.local_address);
            let hint =
                (next_step(&view) != status_text(&view)).then(|| text_rect(next_step(&view)));
            let mut previous_bottom = Fixed::ZERO;
            for (name, address) in std::iter::once(("本机", view.local_address.as_str())).chain(
                view.peers
                    .iter()
                    .map(|peer| (peer.name.as_str(), peer.address.as_str())),
            ) {
                let name = text_rect(name);
                let address = text_rect(address);
                assert_eq!(name.y, address.y, "Device name and address share a row");
                assert_eq!(
                    local.x + local.w,
                    address.x + address.w,
                    "Addresses align right"
                );
                assert!(name.x + name.w < address.x, "Columns remain separate");
                assert!(address.y >= previous_bottom, "Device rows never overlap");
                assert!(
                    address.y + address.h < Fixed::from_int(y),
                    "Addresses stay above actions: {size:?} {page:?} {address:?} action_y={y}"
                );
                assert!(
                    hint.is_none_or(|hint| address.y + address.h <= hint.y),
                    "Addresses stay above the connection hint"
                );
                previous_bottom = address.y + address.h;
            }
        }
    }
}

#[test]
fn reflow_and_reverse_retarget_match_complete_frames_at_every_viewport() {
    for size in [[480, 320], [640, 480], [800, 480]] {
        for page in [Page::Choice, Page::Rooms, Page::Games] {
            let mut app = app(size);
            let mut view = view(page);
            let mut scene = draw(&mut app, &view).unwrap();
            let original = focus_rect(&app).unwrap();

            for (time, selected, status) in [
                (
                    10,
                    1,
                    "正在核对两台设备的游戏和模拟核心，请稍候。Preparing the selected game and checking the connection before starting multiplayer.",
                ),
                (70, 0, "Ready"),
                (140, 1, "连接准备完成"),
                (400, 1, "连接准备完成"),
            ] {
                view.selected = selected;
                view.status = status.into();
                update(&mut scene, &view).unwrap();
                paint_at(&mut app, &scene, time + 16);
                if time == 10 {
                    assert_ne!(
                        focus_rect(&app).unwrap()[1],
                        original[1],
                        "Text must reflow before retarget"
                    );
                }

                assert_full_frame_matches(&mut app, &format!("{size:?} {page:?} at {time}"));
            }
            paint_at(&mut app, &scene, 1500);
            let frame = app
                .world
                .query::<selection::SelectionFrame>()
                .iter()
                .next()
                .unwrap()
                .0;
            let target = focus_rect(&app).unwrap();
            let current = app.world.get::<ComputedRect>(frame).unwrap().0;
            assert_eq!(
                [
                    current.x.to_int(),
                    current.y.to_int(),
                    current.w.to_int(),
                    current.h.to_int()
                ],
                target
            );
        }
    }
}

#[test]
fn semantic_phase_transitions_match_complete_frames() {
    for page in [Page::Host, Page::Joined, Page::Leaving] {
        let mut app = app([640, 480]);
        let mut previous = view(page);
        let mut scene = draw(&mut app, &previous).unwrap();
        for (i, phase) in [
            Phase::ApprovalPending,
            Phase::PeerPreparing,
            Phase::HostReady,
            Phase::Connecting,
            Phase::Checking,
            Phase::Notifying,
            Phase::WaitingStart,
            Phase::Starting,
            Phase::Leaving,
            Phase::RestoreFailed,
        ]
        .into_iter()
        .enumerate()
        {
            let mut next = previous.clone();
            next.phase = phase;
            next.ready = phase == Phase::HostReady;
            next.back_label = if phase == Phase::Leaving {
                "正在退出"
            } else {
                "返回"
            }
            .into();
            update(&mut scene, &next).unwrap();

            paint_at(&mut app, &scene, i as u64 * 300 + 250);
            assert_full_frame_matches(&mut app, &format!("{page:?} {phase:?}"));
            previous = next;
        }
    }
}

#[test]
fn focus_damage_stays_disjoint_and_avoids_unchanged_interiors() {
    for page in [Page::Choice, Page::Rooms] {
        let mut app = app([640, 480]);
        let _scene = draw(&mut app, &view(page)).unwrap();
        let mut target = focus_rect(&app).unwrap();
        target[usize::from(page == Page::Rooms)] += 1;
        offset_focus(&mut app, usize::from(page == Page::Rooms));
        app.render_dirty().unwrap();
        let plan = &app.world.resource::<LastDirtyRegions>().unwrap().0;
        assert!(plan.rects.len() <= 4);
        let area: i32 = plan.rects.iter().map(|r| r.w.to_int() * r.h.to_int()).sum();
        assert!(area < (target[2] + target[3]) * 8);
        for (i, a) in plan.rects.iter().enumerate() {
            for b in &plan.rects[i + 1..] {
                assert!(
                    a.x >= b.x + b.w || b.x >= a.x + a.w || a.y >= b.y + b.h || b.y >= a.y + a.h
                );
            }
        }
        assert_full_frame_matches(&mut app, "narrow focus damage");
    }
}

#[test]
fn unchanged_idle_frames_do_not_submit_pixels() {
    let pixels = Rc::new(Cell::new(0_u32));
    let output = pixels.clone();
    let surface = FramebufSurface::with_format(640, 480, ColorFormat::RGBA8888, move |_, area| {
        output.set(output.get() + u32::from(area.width()) * u32::from(area.height()))
    });
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    install(&mut app).unwrap();
    let view = view(Page::Choice);
    let mut scene = draw(&mut app, &view).unwrap();
    TIME.with(|clock| clock.set(0));
    app.add_resource(mirui::ecs::MonoClock::new(now_ns));

    paint_at(&mut app, &scene, 1500);
    pixels.set(0);
    update(&mut scene, &view).unwrap();
    paint_at(&mut app, &scene, 1800);

    assert_eq!(pixels.get(), 0);
}

#[test]
fn readiness_is_the_only_source_of_waiting_for_host_copy() {
    let mut view = view(Page::Joined);
    view.status.clear();
    for phase in [
        Phase::Connecting,
        Phase::Checking,
        Phase::Notifying,
        Phase::WaitingStart,
        Phase::Starting,
        Phase::RestoreFailed,
    ] {
        view.phase = phase;
        assert_eq!(status_text(&view), next_step(&view));
        if matches!(
            phase,
            Phase::Connecting | Phase::Checking | Phase::Notifying
        ) {
            assert_ne!(headline(&view), "已就绪，等待房主开局");
            assert_ne!(status_text(&view), "请房主按 A 开始游戏");
        }
    }
    view.status = "设备提供的当前状态".into();
    assert_eq!(status_text(&view), view.status);
    view.page = Page::Host;
    view.phase = Phase::ApprovalPending;
    assert!(host_action_enabled(&view));
    let approval = view.clone();
    view.phase = Phase::HostReady;
    view.ready = true;
    assert!(host_action_enabled(&view));
    let mut app = app([640, 480]);
    let mut scene = draw(&mut app, &approval).unwrap();
    let page = body(&app);
    update(&mut scene, &view).unwrap();
    app.tick();
    assert_eq!(body(&app), page);
    view.busy = true;
    assert!(!host_action_enabled(&view));
}

fn page_frames(app: &TestApp) -> Vec<(Entity, Page)> {
    app.world
        .query::<selection::SelectionFrame>()
        .iter()
        .map(|(_, frame)| (frame.body, frame.page.page))
        .collect()
}

fn translation(app: &TestApp, mut node: Entity) -> Fixed {
    loop {
        if let Some(transform) = app.world.get::<mirui::ui::widgets::WidgetTransform>(node) {
            return transform.0.apply_point(mirui::types::Point::ZERO).x;
        }
        node = app.world.get::<mirui::ui::Parent>(node).unwrap().0;
    }
}

fn advance_checked(app: &mut TestApp, scene: &Scene, until: u64) {
    while now_ns() / 1_000_000 < until {
        let next = (now_ns() / 1_000_000 + 16).min(until);
        paint_at(app, scene, next);
        assert_full_frame_matches(app, "interrupted page transition");
    }
}

#[test]
fn rapid_return_reuses_the_page_and_interrupted_navigation_keeps_the_latest_target() {
    let mut app = app([480, 320]);
    let mut scene = draw(&mut app, &view(Page::Mode)).unwrap();
    let root = app.root;
    let original = body(&app);
    scene.navigation_intent(false);
    update(&mut scene, &view(Page::Choice)).unwrap();
    advance_checked(&mut app, &scene, 16);
    let incoming = body(&app);
    let entered_from = translation(&app, incoming);
    assert!(entered_from > Fixed::ZERO);
    advance_checked(&mut app, &scene, 64);
    assert!(translation(&app, incoming) < entered_from);

    scene.navigation_intent(true);
    update(&mut scene, &view(Page::Mode)).unwrap();
    advance_checked(&mut app, &scene, 80);
    assert_eq!(body(&app), original);
    advance_checked(&mut app, &scene, 1200);
    assert_eq!(page_frames(&app), vec![(original, Page::Mode)]);
    assert!(!app.world.is_alive(incoming));
    assert_eq!(app.root, root);

    scene.navigation_intent(false);
    update(&mut scene, &view(Page::Choice)).unwrap();
    advance_checked(&mut app, &scene, 1216);
    let retired = page_frames(&app);
    update(&mut scene, &view(Page::Host)).unwrap();
    advance_checked(&mut app, &scene, 1232);
    update(&mut scene, &view(Page::Rooms)).unwrap();
    advance_checked(&mut app, &scene, 3000);
    assert_eq!(page_frames(&app).len(), 1);
    assert_eq!(page_frames(&app)[0].1, Page::Rooms);
    assert!(retired.iter().all(|(node, _)| !app.world.is_alive(*node)));
    assert_eq!(app.root, root);
    assert!(focus_rect(&app).is_some());
}

#[test]
fn refreshed_room_rows_keep_the_page_and_rebind_selection_to_live_entities() {
    let mut app = app([480, 320]);
    let mut next = view(Page::Rooms);
    let mut scene = draw(&mut app, &next).unwrap();
    let page = body(&app);
    let old_targets: Vec<_> = app
        .world
        .query::<FocusTarget>()
        .iter()
        .map(|(node, _)| node)
        .collect();
    next.rooms.clear();
    next.selected = usize::MAX;
    update(&mut scene, &next).unwrap();
    advance_checked(&mut app, &scene, 64);
    assert_eq!(body(&app), page);
    assert!(old_targets.iter().all(|node| !app.world.is_alive(*node)));
    next.rooms = view(Page::Rooms).rooms;
    next.selected = 2;
    update(&mut scene, &next).unwrap();
    advance_checked(&mut app, &scene, 1200);
    assert_eq!(body(&app), page);
    let frame = app
        .world
        .query::<selection::SelectionFrame>()
        .iter()
        .next()
        .unwrap()
        .0;
    let selected = app.world.get::<FollowBounds>(frame).unwrap().target;
    assert!(app.world.is_alive(selected));
    assert_eq!(app.world.get::<FocusTarget>(selected).unwrap().index(), 2);
    assert!(old_targets.iter().all(|node| *node != selected));
}

#[test]
fn retained_rows_keep_entities_and_update_contents_indices_and_focus() {
    for page in [Page::Rooms, Page::Games] {
        let mut app = app([640, 480]);
        let mut next = view(page);
        let mut scene = draw(&mut app, &next).unwrap();
        paint_at(&mut app, &scene, 1000);
        let targets: Vec<_> = app
            .world
            .query::<FocusTarget>()
            .iter()
            .map(|(node, marker)| (marker.index(), node))
            .collect();
        let first = targets.iter().find(|(index, _)| *index == 0).unwrap().1;
        let original_text: Vec<_> = app
            .world
            .query::<Text>()
            .iter()
            .map(|(node, _)| node)
            .collect();
        if page == Page::Rooms {
            next.rooms[0].name = "更新后的房间".into();
            next.rooms[0].signal = 12;
            next.rooms[0].available = false;
            next.rooms.swap(0, 2);
        } else {
            next.entries[0].name = "更新后的游戏".into();
            next.entries[0].detail = "新的说明".into();
            next.entries.swap(0, 2);
        }
        next.selected = 2;
        update(&mut scene, &next).unwrap();
        advance_checked(&mut app, &scene, 1600);
        assert!(targets.iter().all(|(_, node)| app.world.is_alive(*node)));
        assert!(original_text.iter().all(|node| app.world.is_alive(*node)));
        assert_eq!(app.world.get::<FocusTarget>(first).unwrap().index(), 2);
        let focus = app
            .world
            .query::<selection::SelectionFrame>()
            .iter()
            .next()
            .unwrap()
            .0;
        assert_eq!(app.world.get::<FollowBounds>(focus).unwrap().target, first);
        let labels: Vec<_> = app
            .world
            .query::<Text>()
            .iter()
            .filter(|(node, _)| {
                selection::contains(first, *node, |node| {
                    app.world.get::<mirui::ui::Parent>(node).map(|p| p.0)
                })
            })
            .map(|(_, text)| text.resolve(app.world.resource()).to_string())
            .collect();
        assert!(
            labels.contains(
                &if page == Page::Rooms {
                    "更新后的房间"
                } else {
                    "更新后的游戏"
                }
                .to_string()
            )
        );
        if page == Page::Rooms {
            assert!(labels.contains(&"房间已离线，等待重新出现".to_string()));
        }
        let mut expected = self::app([640, 480]);
        let expected_scene = draw(&mut expected, &next).unwrap();
        paint_at(&mut expected, &expected_scene, 1600);
        assert_eq!(
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            expected
                .backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes(),
            "Retained rows must render the same live values as fresh rows"
        );
        if page == Page::Games {
            // Regular game details are hidden. Reveal the retained description
            // and verify its latest value without replacing the row or focus.
            next.entries[2].directory = true;
            update(&mut scene, &next).unwrap();
            advance_checked(&mut app, &scene, 1800);
            assert!(targets.iter().all(|(_, node)| app.world.is_alive(*node)));
            assert!(original_text.iter().all(|node| app.world.is_alive(*node)));
            assert_eq!(app.world.get::<FollowBounds>(focus).unwrap().target, first);
            assert!(app.world.query::<Text>().iter().any(|(node, text)| {
                selection::contains(first, node, |node| {
                    app.world.get::<mirui::ui::Parent>(node).map(|p| p.0)
                }) && text.resolve(app.world.resource()) == "新的说明"
            }));
        }
    }
}

#[test]
fn menu_handoffs_animate_without_blank_frames_in_panel_formats() {
    for format in [ColorFormat::BGRA8888, ColorFormat::RGB565] {
        let mut app = app_with_format([640, 480], format);
        let mut scene = draw(&mut app, &view(Page::Mode)).unwrap();
        let root = app.root;
        for (page, network_only, backward) in [
            (Page::Games, false, false),
            (Page::Choice, false, false),
            (Page::Host, false, false),
            (Page::Choice, false, true),
            (Page::Mode, false, true),
            (Page::Choice, true, false),
            (Page::Rooms, true, false),
            (Page::Mode, false, true),
        ] {
            let mut next = view(page);
            next.network_only = network_only;
            scene.navigation_intent(backward);
            update(&mut scene, &next).unwrap();
            let start = now_ns() / 1_000_000;
            paint_at(&mut app, &scene, start + 16);
            assert_eq!(app.root, root);
            assert_eq!(
                page_frames(&app).len(),
                2,
                "{page:?}: outgoing page must stay mounted"
            );
            let offset = translation(&app, body(&app));
            assert!(if backward {
                offset < Fixed::ZERO
            } else {
                offset > Fixed::ZERO
            });
            while now_ns() / 1_000_000 < start + 480 {
                let time = now_ns() / 1_000_000 + 16;
                paint_at(&mut app, &scene, time);
                for y in (0..480).step_by(16) {
                    for x in (0..640).step_by(16) {
                        let color = app
                            .backend
                            .framebuffer()
                            .read_pixels()
                            .unwrap()
                            .get_pixel(x, y);
                        assert!(
                            color.r != 0 || color.g != 0 || color.b != 0,
                            "{format:?} {page:?}: black pixel at {x},{y}"
                        );
                    }
                }
                assert_full_frame_matches(&mut app, "continuous menu handoff");
            }
            assert_eq!(page_frames(&app).len(), 1);
        }
    }
}

#[test]
fn palettes_define_all_roles_and_legible_action_text() {
    use super::style::*;
    use crate::theme::Palette;
    fn luminance(color: Color) -> f64 {
        [color.r, color.g, color.b]
            .into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(value, weight)| {
                let value = f64::from(value) / 255.0;
                weight
                    * if value <= 0.04045 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    }
            })
            .sum()
    }
    for palette in Palette::ALL {
        let theme = palette.theme();
        for token in [
            BACKGROUND,
            FOREGROUND,
            ACCENT,
            ON_ACCENT,
            PANEL,
            SECONDARY,
            DISABLED,
            DIVIDER,
            ACCENT_DIM,
            WARNING,
            WARNING_PANEL,
            CLAY,
            SKY,
            HILL,
            DEEP,
            HEADER_TEXT,
            HEADER_DETAIL,
        ] {
            assert_ne!(theme.resolve(token), Color::rgb(255, 0, 255));
        }
        for (foreground, background) in [
            (FOREGROUND, BACKGROUND),
            (FOREGROUND, PANEL),
            (ON_ACCENT, ACCENT),
            (HEADER_TEXT, DEEP),
        ] {
            let a = luminance(theme.resolve(foreground));
            let b = luminance(theme.resolve(background));
            assert!(
                (a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5,
                "{palette:?}: {foreground:?}/{background:?}"
            );
        }
    }
}

#[test]
fn model_rows_preserve_member_and_sprite_identity_for_live_updates() {
    let mut app = app([640, 480]);
    let mut current = view(Page::Rooms);
    let mut scene = draw(&mut app, &current).unwrap();
    paint_at(&mut app, &scene, 1600);
    let before: Vec<_> = scene.state.rooms().entries().collect();
    let sprites: Vec<_> = app
        .world
        .query::<pixel::SignalSprite>()
        .iter()
        .map(|(entity, _)| entity)
        .collect();
    current.rooms.swap(0, 1);
    current.rooms[1].signal = 12;
    current.rooms[1].available = false;
    update(&mut scene, &current).unwrap();
    paint_at(&mut app, &scene, 1700);
    let after: Vec<_> = scene.state.rooms().entries().collect();
    assert_eq!(after[0].0, before[1].0);
    assert_eq!(after[1].0, before[0].0);
    assert_eq!(before[0].1.index(), 1);
    assert_eq!(before[0].1.strength(), 12);
    assert!(!before[0].1.available());
    assert_eq!(
        app.world
            .query::<pixel::SignalSprite>()
            .iter()
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        sprites
    );
    current.rooms.clear();
    update(&mut scene, &current).unwrap();
    current = view(Page::Rooms);
    update(&mut scene, &current).unwrap();
    assert_ne!(scene.state.rooms().entries().next().unwrap().0, before[0].0);
}

#[test]
fn ambiguous_row_keys_keep_unchanged_snapshots_and_retire_changed_rows() {
    for key in ["", "duplicate"] {
        let mut app = app([640, 480]);
        let mut current = view(Page::Rooms);
        current.rooms.truncate(2);
        for row in &mut current.rooms {
            row.key = key.into();
        }
        let mut scene = draw(&mut app, &current).unwrap();
        let before: Vec<_> = scene.state.rooms().entries().map(|(id, _)| id).collect();
        current.status = "扫描完成".into();
        update(&mut scene, &current).unwrap();
        assert_eq!(
            scene
                .state
                .rooms()
                .entries()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            before
        );
        current.rooms[0].signal = 12;
        update(&mut scene, &current).unwrap();
        assert!(
            scene
                .state
                .rooms()
                .entries()
                .all(|(id, _)| !before.contains(&id))
        );
    }
}

#[test]
fn entry_motion_starts_at_the_installed_framework_clock() {
    let mut app = app([640, 480]);
    TIME.with(|time| time.set(5_000_000_000));
    let scene = draw(&mut app, &view(Page::Choice)).unwrap();
    assert_eq!(scene.state.motion().selection, 0);
    paint_at(&mut app, &scene, 5080);
    assert_eq!(scene.state.motion().selection, 80);
}

#[test]
fn lobby_sheets_retain_entities_and_scope_focus_through_reversal() {
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Lobby);
        current.lobby = true;
        current.selected = 3;
        current.room_selected = Some(3);
        let mut scene = draw(&mut app, &current).unwrap();
        let retained = body(&app);
        let target = |app: &TestApp| {
            let frame = app
                .world
                .query::<selection::SelectionFrame>()
                .iter()
                .find(|(_, frame)| frame.page == frame.state.page_key())
                .unwrap()
                .0;
            app.world.get::<FollowBounds>(frame).unwrap().target
        };
        paint_at(&mut app, &scene, 1000);
        let room_target = target(&app);
        let deck = app.world.find_by_id("discovery_deck").unwrap();
        let text_layout: Vec<_> = app
            .world
            .query::<Text>()
            .iter()
            .filter(|(entity, _)| {
                selection::contains(deck, *entity, |node| {
                    app.world.get::<mirui::ui::Parent>(node).map(|p| p.0)
                })
            })
            .filter_map(|(entity, _)| {
                app.world
                    .get::<ComputedRect>(entity)
                    .map(|rect| (entity, rect.0))
            })
            .collect();
        assert!(!text_layout.is_empty());
        for (time, page) in [
            (1100, Page::Mode),
            (1180, Page::Lobby),
            (1260, Page::Mode),
            (1800, Page::Lobby),
        ] {
            let before = scene.state.deck();
            current.page = page;
            current.selected = if page == Page::Mode { 0 } else { 3 };
            update(&mut scene, &current).unwrap();
            scene.state.animate(now_ns() / 1_000_000, 0);
            assert_eq!(
                scene.state.deck(),
                before,
                "Retarget starts at the current position"
            );
            paint_at(&mut app, &scene, time);
            assert_eq!(body(&app), retained);
            assert_eq!(scene.state.rooms().len(), 4);
            for (entity, rect) in &text_layout {
                assert_eq!(
                    app.world.get::<ComputedRect>(*entity).unwrap().0,
                    *rect,
                    "Deck motion must preserve local text layout and cache coordinates"
                );
            }
            if page == Page::Mode {
                assert_ne!(target(&app), room_target);
            } else {
                assert_eq!(target(&app), room_target);
            }
            assert_full_frame_matches(&mut app, "Deck compression and rapid reversal");
        }
        paint_at(&mut app, &scene, 2400);
        assert_eq!(scene.state.deck(), Fixed::ZERO);
    }
}

#[test]
fn action_cursor_observes_the_menu_model_without_a_library_selection_change() {
    let mut app = app([640, 480]);
    let mut home = view(Page::Games);
    home.library.kind = crate::model::LibraryKind::Favorites;
    home.launcher.enabled = true;
    let mut scene = draw(&mut app, &home).unwrap();
    paint_at(&mut app, &scene, 800);
    home.launcher.overlay = "启动器设置".into();
    home.launcher.items =
        vec![crate::model::LauncherItem::available(crate::model::MenuAction::Back, "返回"); 4];
    update(&mut scene, &home).unwrap();
    paint_at(&mut app, &scene, 1600);
    let actions = scene.state.launcher();
    home.launcher.selected = 3;
    // A standard Model mutation is sufficient; the library View stays unchanged.
    actions.present(&home);
    paint_at(&mut app, &scene, 1648);
    let pose = pixel::cursor_frame(
        scene.state.actor_motion(),
        actions.menu_selection(),
        actions.selection_age(),
    );
    assert_eq!(scene.state.selected(), Some(0));
    assert_eq!(pose.selected, Some(3));
    assert_eq!(pose.selection, actions.selection_age());
    assert!(pose.walking);
    assert_ne!(pose.selection, scene.state.walker_motion().selection);
    paint_at(&mut app, &scene, 2400);
    assert!(
        !pixel::cursor_frame(
            scene.state.actor_motion(),
            actions.menu_selection(),
            actions.selection_age()
        )
        .walking
    );
    assert_full_frame_matches(&mut app, "Menu cursor follows its own animation clock");
    home.launcher.overlay.clear();
    actions.present(&home);
    assert_eq!(actions.menu_selection(), None);
    assert_eq!(
        pixel::cursor_frame(
            scene.state.actor_motion(),
            actions.menu_selection(),
            actions.selection_age()
        ),
        scene.state.actor_motion()
    );
}

#[test]
fn action_actor_placement_and_return_do_not_inherit_the_background_page() {
    use crate::model::LibraryKind;
    for size in [[480, 320], [640, 480]] {
        let mut reference = [None; 4];
        for (kind, empty) in [
            (LibraryKind::Favorites, false),
            (LibraryKind::Platforms, false),
            (LibraryKind::Search, false),
            (LibraryKind::Search, true),
        ] {
            let mut app = app(size);
            let mut current = view(Page::Games);
            current.launcher.enabled = true;
            current.launcher.tab = usize::from(kind != LibraryKind::Favorites);
            current.library.kind = kind;
            current.library.keyboard = kind == LibraryKind::Search && !empty;
            if empty {
                current.entries.clear();
            }
            let mut scene = draw(&mut app, &current).unwrap();
            paint_at(&mut app, &scene, 1000);
            let frame = app
                .world
                .query::<actor::ActorFrame>()
                .iter()
                .next()
                .unwrap()
                .0;
            let original = app.world.get::<FollowBounds>(frame).unwrap().target;
            current.launcher.overlay = "系统控制".into();
            current.launcher.overlay_kind = "controls".into();
            current.launcher.items = howff_menu::SystemLevel::ALL
                .into_iter()
                .map(|control| {
                    howff_menu::MenuItem::available(control.action(), control.label())
                        .with_level(howff_menu::Level::new(control, Some(50), false, false))
                })
                .chain([
                    howff_menu::MenuItem::available(howff_menu::MenuAction::ShowTools, "系统工具"),
                    howff_menu::MenuItem::available(howff_menu::MenuAction::Back, "更多设置"),
                ])
                .collect();
            for (step, selected) in [0, 1, 2, 3].into_iter().enumerate() {
                current.launcher.selected = selected;
                update(&mut scene, &current).unwrap();
                paint_at(&mut app, &scene, 2000 + step as u64 * 1000);
                let target = app.world.get::<FollowBounds>(frame).unwrap().target;
                assert_eq!(
                    app.world.get::<deck::ActionAnchor>(target).unwrap().0,
                    selected
                );
                let body = app
                    .world
                    .query::<pixel::ActorSprite>()
                    .iter()
                    .next()
                    .unwrap()
                    .0;
                let rect = app.world.get::<ComputedRect>(body).unwrap().0;
                let anchor = app.world.get::<ComputedRect>(target).unwrap().0;
                let offset = (rect.x - anchor.x, rect.y - anchor.y, rect.w, rect.h);
                if let Some(expected) = reference[selected] {
                    assert_eq!(offset, expected, "{size:?} {kind:?}/{selected}");
                } else {
                    reference[selected] = Some(offset);
                }
            }
            current.launcher.overlay.clear();
            current.launcher.overlay_kind.clear();
            current.launcher.items.clear();
            update(&mut scene, &current).unwrap();
            paint_at(&mut app, &scene, 6500);
            assert_eq!(
                app.world.get::<FollowBounds>(frame).unwrap().target,
                original
            );
            assert!(!app.world.has::<mirui::ui::Hidden>(frame));
        }
    }
}

#[test]
fn room_actor_ownership_and_membership_are_independent_of_entry() {
    fn actors(app: &mut TestApp) -> (usize, bool, bool) {
        let rgb = |role| {
            let color = app.theme().resolve(role);
            [color.r, color.g, color.b]
        };
        let skin = rgb(crate::theme::actor::SKIN);
        let host = rgb(crate::theme::actor::HOST);
        let guest = rgb(crate::theme::actor::GUEST);
        let frame = app.backend.framebuffer();
        let pixels = frame.read_pixels().unwrap();
        let bytes = pixels.bytes();
        let width = 640;
        let head = |x: usize, y: usize| bytes[(y * width + x) * 4..][..3] == skin;
        let mut count = 0;
        for y in 0..480 {
            let mut x = 0;
            while x < width {
                if !head(x, y) {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < width && head(x, y) {
                    x += 1;
                }
                if x - start >= 2 && (y == 0 || !(start..x).any(|x| head(x, y - 1))) {
                    count += 1;
                }
            }
        }
        (
            count,
            bytes.chunks_exact(4).any(|p| p[..3] == host),
            bytes.chunks_exact(4).any(|p| p[..3] == guest),
        )
    }
    for (launcher, current_game) in [(false, true), (true, false), (true, true)] {
        let mut app = app([640, 480]);
        let mut initial = view(if launcher { Page::Games } else { Page::Choice });
        initial.launcher.enabled = launcher;
        initial.library.kind = crate::model::LibraryKind::Favorites;
        let mut scene = draw(&mut app, &initial).unwrap();
        paint_at(&mut app, &scene, 800);
        let mut current = view(Page::Host);
        current.return_to_launcher = launcher;
        current.current_game = current_game;
        for (index, (page, phase, ready, connected, visitor)) in [
            (Page::Host, Phase::Creating, false, false, false),
            (Page::Host, Phase::WaitingPeer, false, false, false),
            (Page::Host, Phase::ApprovalPending, false, false, true),
            (Page::Host, Phase::PeerPreparing, false, false, true),
            (Page::Host, Phase::HostReady, true, true, true),
            (Page::Host, Phase::WaitingPeer, false, false, false),
            (Page::Joined, Phase::Connecting, false, false, true),
            (Page::Joined, Phase::Checking, false, true, true),
            (Page::Joined, Phase::Notifying, false, true, true),
            (Page::Joined, Phase::WaitingStart, true, true, true),
            (Page::Joined, Phase::Starting, true, true, true),
            (Page::Leaving, Phase::Leaving, false, false, true),
            (Page::Leaving, Phase::RestoreFailed, false, false, true),
            (Page::Host, Phase::NetworkConfirming, false, false, false),
            (Page::Host, Phase::NetworkConnected, false, true, true),
            (Page::Host, Phase::NetworkReconnecting, false, false, true),
            (Page::Joined, Phase::NetworkConnected, false, true, true),
        ]
        .into_iter()
        .enumerate()
        {
            current.page = page;
            current.phase = phase;
            current.network_only = matches!(
                phase,
                Phase::NetworkConfirming | Phase::NetworkConnected | Phase::NetworkReconnecting
            );
            current.ready = ready;
            current.connected = connected;
            update(&mut scene, &current).unwrap();
            paint_at(&mut app, &scene, 1600 + index as u64 * 480);
            let active = if launcher { &scene.room } else { &scene.state };
            assert_eq!(
                active.session_motion().visitor,
                visitor,
                "{launcher}/{phase:?}"
            );
            let cursor = app
                .world
                .query::<actor::ActorFrame>()
                .iter()
                .next()
                .unwrap()
                .0;
            assert!(app.world.get::<mirui::ui::Hidden>(cursor).is_some());
            // Leaving actors travel outside the stage; membership survives restoration errors.
            if page != Page::Leaving {
                assert_eq!(
                    actors(&mut app),
                    (1 + usize::from(visitor), true, visitor),
                    "{launcher}/{phase:?}"
                );
            }
        }
        current.page = Page::Rooms;
        current.phase = Phase::Idle;
        current.selected = 1;
        current.network_only = false;
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, 10500);
        let cursor = app
            .world
            .query::<actor::ActorFrame>()
            .iter()
            .next()
            .unwrap()
            .0;
        assert!(app.world.get::<mirui::ui::Hidden>(cursor).is_none());
        let target = app.world.get::<FollowBounds>(cursor).unwrap().target;
        assert_eq!(app.world.get::<FocusTarget>(target).unwrap().index(), 1);
        assert_eq!(actors(&mut app), (1, true, false));
        let sprites = app.world.query::<pixel::ActorSprite>();
        assert_eq!(sprites.iter().count(), 1);
        assert_eq!(
            sprites.iter().next().unwrap().1.state.page_key().page,
            Page::Rooms
        );
        assert_full_frame_matches(&mut app, "Room return restores exactly one active cursor");
    }
}

#[test]
fn retained_actor_follows_only_the_active_layer_page_and_pane() {
    let mut app = app([640, 480]);
    let mut initial = view(Page::Games);
    initial.library.kind = crate::model::LibraryKind::Favorites;
    initial.launcher.enabled = true;
    let mut scene = draw(&mut app, &initial).unwrap();
    let mut current = view(Page::Lobby);
    current.lobby = true;
    current.return_to_launcher = true;
    let mut time = 0;
    for populated in [false, true] {
        current.rooms = if populated {
            view(Page::Rooms).rooms
        } else {
            Vec::new()
        };
        for (page, selected) in [
            (Page::Lobby, 0),
            (Page::Mode, 0),
            (Page::Mode, 1),
            (Page::Lobby, 0),
        ] {
            current.page = page;
            current.selected = selected;
            update(&mut scene, &current).unwrap();
            time += 800;
            paint_at(&mut app, &scene, time);
            let scopes = app.world.query::<deck::ActorScope>();
            let active_layer = scopes
                .iter()
                .find_map(|(entity, scope)| {
                    matches!(scope, deck::ActorScope::Layer(deck::Layer::Room)).then_some(entity)
                })
                .unwrap();
            let inside = |root, entity| {
                selection::contains(root, entity, |node| {
                    app.world
                        .get::<mirui::ui::Parent>(node)
                        .map(|parent| parent.0)
                })
            };
            let active_pane = scopes
                .iter()
                .find_map(|(entity, scope)| {
                    (matches!(scope, deck::ActorScope::Pane(value) if *value == page)
                        && inside(active_layer, entity))
                    .then_some(entity)
                })
                .unwrap();
            let actors: Vec<_> = app
                .world
                .query::<actor::ActorFrame>()
                .iter()
                .map(|(node, _)| node)
                .collect();
            assert_eq!(actors.len(), 1);
            let actor = actors[0];
            assert!(app.world.get::<mirui::ui::Hidden>(actor).is_none());
            let target = app.world.get::<FollowBounds>(actor).unwrap().target;
            assert!(
                inside(active_pane, target),
                "{page:?}: actor must leave the compressed pane"
            );
            if page == Page::Mode || !populated {
                assert!(app.world.get::<deck::ActorAnchor>(target).is_some());
            } else {
                assert_eq!(
                    app.world.get::<FocusTarget>(target).unwrap().index(),
                    selected
                );
            }
        }
    }
    update(&mut scene, &initial).unwrap();
    paint_at(&mut app, &scene, time + 800);
    let frame = app
        .world
        .query::<actor::ActorFrame>()
        .iter()
        .next()
        .unwrap()
        .0;
    let target = app.world.get::<FollowBounds>(frame).unwrap().target;
    let shelf = app
        .world
        .query::<deck::ActorScope>()
        .iter()
        .find_map(|(entity, scope)| {
            matches!(scope, deck::ActorScope::Layer(deck::Layer::Shelf)).then_some(entity)
        })
        .unwrap();
    assert!(selection::contains(shelf, target, |node| app
        .world
        .get::<mirui::ui::Parent>(node)
        .map(|parent| parent.0)));
}

#[test]
fn modal_snapshot_stays_below_foreground_panel_and_actor() {
    use mirui::ui::widgets::image::ImageSource;
    use mirui::ui::{Children, Parent};
    let mut app = app([640, 480]);
    let mut home = view(Page::Games);
    home.library.kind = crate::model::LibraryKind::Favorites;
    home.launcher.enabled = true;
    let mut scene = draw(&mut app, &home).unwrap();
    super::paint_at(&mut app, 1000).unwrap();
    let original_layer = app
        .world
        .find_by_id("launcher_background_snapshot_layer")
        .unwrap();
    let shell = app.world.get::<Parent>(original_layer).unwrap().0;
    let original_order = app.world.get::<Children>(shell).unwrap().0.clone();
    let check_order = |app: &TestApp| {
        assert_eq!(
            app.world.find_by_id("launcher_background_snapshot_layer"),
            Some(original_layer)
        );
        assert_eq!(app.world.get::<Children>(shell).unwrap().0, original_order);
        for (node, image) in app.world.query::<mirui::ui::widgets::Image>().iter() {
            if matches!(&image.src, ImageSource::Handle(_)) {
                assert_eq!(app.world.get::<Parent>(node).unwrap().0, original_layer);
            }
        }
    };
    check_order(&app);
    let mut panel = view(Page::Lobby);
    panel.lobby = true;
    panel.return_to_launcher = true;
    update(&mut scene, &panel).unwrap();
    super::paint_at(&mut app, 1016).unwrap();
    super::paint_at(&mut app, 1500).unwrap();
    check_order(&app);
    let layer = app
        .world
        .find_by_id("launcher_background_snapshot_layer")
        .unwrap();
    let parent = app.world.get::<Parent>(layer).unwrap().0;
    let children = &app.world.get::<Children>(parent).unwrap().0;
    let layer_index = children.iter().position(|node| *node == layer).unwrap();
    let actor = app
        .world
        .query::<actor::ActorFrame>()
        .iter()
        .next()
        .unwrap()
        .0;
    let actor_index = children.iter().position(|node| *node == actor).unwrap();
    let mut panel_root = app
        .world
        .query::<deck::ActorScope>()
        .iter()
        .find_map(|(node, scope)| {
            matches!(scope, deck::ActorScope::Layer(deck::Layer::Room)).then_some(node)
        })
        .unwrap();
    while app.world.get::<Parent>(panel_root).unwrap().0 != parent {
        panel_root = app.world.get::<Parent>(panel_root).unwrap().0;
    }
    let panel_index = children
        .iter()
        .position(|node| *node == panel_root)
        .unwrap();
    assert!(layer_index < panel_index && panel_index < actor_index);
    let image = app
        .world
        .query::<mirui::ui::widgets::Image>()
        .iter()
        .find_map(|(node, image)| matches!(&image.src, ImageSource::Handle(_)).then_some(node))
        .unwrap();
    assert_eq!(app.world.get::<Parent>(image).unwrap().0, layer);
    let token = app
        .world
        .resource::<modal::Runtime>()
        .unwrap()
        .model
        .token()
        .unwrap();
    let source = app
        .world
        .resource::<modal::Runtime>()
        .unwrap()
        .model
        .source();
    let ImageSource::Handle(source) = source else {
        panic!("modal snapshots must be direct texture sources");
    };
    let frozen = source.read_pixels().unwrap();
    let presented = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    assert_ne!(
        &presented[..4],
        &frozen.bytes()[..4],
        "The foreground dimmer must cover the frozen image"
    );
    assert!(presented[0] < frozen.bytes()[0]);
    let changed = presented
        .chunks_exact(4)
        .zip(frozen.bytes().chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 640 * 480 / 2,
        "The foreground panel must remain visible over the snapshot"
    );
    let bright = presented
        .chunks_exact(4)
        .filter(|pixel| pixel[..3].iter().all(|channel| *channel > 160))
        .count();
    assert!(
        bright > 640 * 480 / 4,
        "The foreground content must remain visible inside the dimmer"
    );
    app.set_theme(crate::theme::Palette::Sunset.theme())
        .unwrap();
    super::paint_at(&mut app, 1516).unwrap();
    super::paint_at(&mut app, 1532).unwrap();
    check_order(&app);
    let next_token = app
        .world
        .resource::<modal::Runtime>()
        .unwrap()
        .model
        .token()
        .unwrap();
    assert_ne!(token, next_token);
    update(&mut scene, &home).unwrap();
    super::paint_at(&mut app, 1548).unwrap();
    super::paint_at(&mut app, 1900).unwrap();
    check_order(&app);
    assert!(
        app.world
            .resource::<modal::Runtime>()
            .unwrap()
            .model
            .token()
            .is_none()
    );
    update(&mut scene, &panel).unwrap();
    super::paint_at(&mut app, 1916).unwrap();
    super::paint_at(&mut app, 2400).unwrap();
    check_order(&app);
    assert!(
        app.world
            .resource::<modal::Runtime>()
            .unwrap()
            .model
            .token()
            .unwrap()
            > next_token
    );
}

#[test]
fn launcher_overlays_share_one_background_snapshot_until_the_last_overlay_closes() {
    use mirui::ui::widgets::image::ImageSource;
    use mirui::ui::{Children, Parent};

    let mut app = app([640, 480]);
    let mut home = view(Page::Games);
    home.library.kind = crate::model::LibraryKind::Favorites;
    home.launcher.enabled = true;
    let mut scene = draw(&mut app, &home).unwrap();
    super::paint_at(&mut app, 1000).unwrap();
    let background = app.world.find_by_id("launcher_background").unwrap();
    let image_layer = app
        .world
        .find_by_id("launcher_background_snapshot_layer")
        .unwrap();
    let shell = app.world.get::<Parent>(image_layer).unwrap().0;
    let order = app.world.get::<Children>(shell).unwrap().0.clone();
    let modal = app
        .world
        .resource::<modal::Runtime>()
        .unwrap()
        .model
        .clone();
    let mut actions = home.clone();
    actions.launcher.overlay = "系统设置".into();
    actions.launcher.overlay_kind = "system".into();
    actions.launcher.items = ["切换主题", "音效开关", "管理直连网络", "返回原系统", "关机"]
        .into_iter()
        .map(|name| crate::model::LauncherItem {
            name: name.into(),
            ..Default::default()
        })
        .collect();
    update(&mut scene, &actions).unwrap();
    super::paint_at(&mut app, 1016).unwrap();
    super::paint_at(&mut app, 1032).unwrap();
    assert_eq!(modal.phase(), modal::Phase::Suspended);
    assert!(modal.hidden() && modal.paused());
    assert!(app.world.has::<mirui::ui::Hidden>(background));
    let token = modal.token().unwrap();
    let counts = scene.shelf.animation_counts();
    for now in [1048, 1064, 1080] {
        super::paint_at(&mut app, now).unwrap();
        assert_eq!(modal.token(), Some(token));
    }
    assert_eq!(scene.shelf.animation_counts().0, counts.0);

    let source = modal.source();
    let ImageSource::Handle(source) = source else {
        panic!("modal snapshots must be direct texture sources");
    };
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    let frozen = source.read_pixels().unwrap();
    assert!(pixels[0] < frozen.bytes()[0]);
    let bright = pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[..3].iter().all(|channel| *channel > 160))
        .count();
    assert!(
        bright > 640 * 480 / 4,
        "The action panel must remain visible"
    );
    let action_root = app
        .world
        .query::<deck::ActorScope>()
        .iter()
        .find_map(|(node, scope)| {
            matches!(scope, deck::ActorScope::Layer(deck::Layer::Actions)).then_some(node)
        })
        .unwrap();
    let mut action_layer = action_root;
    while app.world.get::<Parent>(action_layer).unwrap().0 != shell {
        action_layer = app.world.get::<Parent>(action_layer).unwrap().0;
    }
    let position = |node| order.iter().position(|value| *value == node).unwrap();
    let actor = app
        .world
        .query::<actor::ActorFrame>()
        .iter()
        .next()
        .unwrap()
        .0;
    assert!(position(image_layer) < position(action_layer));
    assert!(position(action_layer) < position(actor));

    let mut room = view(Page::Lobby);
    room.lobby = true;
    room.return_to_launcher = true;
    update(&mut scene, &room).unwrap();
    super::paint_at(&mut app, 1096).unwrap();
    super::paint_at(&mut app, 1500).unwrap();
    assert_eq!(modal.token(), Some(token));
    assert!(modal.hidden() && modal.paused());
    actions.launcher.overlay_kind = "game".into();
    actions.launcher.overlay = "超级马里奥世界".into();
    update(&mut scene, &actions).unwrap();
    super::paint_at(&mut app, 1516).unwrap();
    assert_eq!(modal.token(), Some(token));
    app.set_theme(crate::theme::Palette::Sunset.theme())
        .unwrap();
    super::paint_at(&mut app, 1532).unwrap();
    super::paint_at(&mut app, 1548).unwrap();
    assert!(modal.token().unwrap() > token);
    assert_eq!(app.world.get::<Children>(shell).unwrap().0, order);
    update(&mut scene, &home).unwrap();
    super::paint_at(&mut app, 1564).unwrap();
    super::paint_at(&mut app, 2000).unwrap();
    assert_eq!(modal.phase(), modal::Phase::Live);
    assert!(!modal.hidden() && !modal.paused());
    assert!(modal.token().is_none());
    assert!(!app.world.has::<mirui::ui::Hidden>(background));
    assert_eq!(app.world.get::<Children>(shell).unwrap().0, order);
}

#[test]
fn compressed_shelf_snapshot_preserves_pixels_and_reopens_current_contents() {
    fn equivalent(app: &mut TestApp) {
        let model = app
            .world
            .resource::<suspension::Runtime>()
            .unwrap()
            .model
            .clone();
        let token = model.token().unwrap();
        let frozen = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        model.publish(false, false, None);
        app.dirty_regions().unwrap();
        app.render_dirty().unwrap();
        let live = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        let changed = frozen
            .chunks_exact(4)
            .zip(live.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            changed, 0,
            "Compressed shelf pixels must match their retained source"
        );
        model.publish(true, true, Some(token));
        app.dirty_regions().unwrap();
        app.render_dirty().unwrap();
    }

    for size in [[480, 320], [640, 480], [800, 480]] {
        let mut app = app(size);
        let mut home = view(Page::Games);
        home.library.kind = crate::model::LibraryKind::Favorites;
        home.launcher.enabled = true;
        home.launcher_shelf = Some(Box::new(home.clone()));
        let mut scene = draw(&mut app, &home).unwrap();
        super::paint_at(&mut app, 1000).unwrap();
        let mut library = home.clone();
        library.launcher.tab = 1;
        library.library.kind = crate::model::LibraryKind::Platforms;
        update(&mut scene, &library).unwrap();
        for now in [1016, 2000, 2016, 2032] {
            super::paint_at(&mut app, now).unwrap();
        }
        let model = app
            .world
            .resource::<suspension::Runtime>()
            .unwrap()
            .model
            .clone();
        assert!(model.hidden() && model.paused());
        let mut token = model.token().unwrap();
        equivalent(&mut app);
        super::paint_at(&mut app, 2048).unwrap();
        assert_eq!(model.token(), Some(token));
        let source = app.world.find_by_id("shelf_snapshot_source").unwrap();
        assert!(app.world.has::<mirui::ui::Hidden>(source));

        let old_cards = scene.shelf.card_motion();
        library.launcher_shelf.as_mut().unwrap().selected = 1;
        update(&mut scene, &library).unwrap();
        assert_eq!(scene.shelf.card_motion(), old_cards);
        suspension::before_tick(&mut app);
        assert!(
            !model.hidden(),
            "A changed selected row must release a frozen source before the clock advances"
        );
        for now in [2050, 2500, 2516, 2532] {
            super::paint_at(&mut app, now).unwrap();
        }
        assert!(model.hidden());
        assert_ne!(model.token(), Some(token));
        token = model.token().unwrap();
        equivalent(&mut app);

        let mut overlay = library.clone();
        overlay.launcher.overlay = "Settings".into();
        overlay.launcher.overlay_kind = "system".into();
        overlay.launcher.items = vec![crate::model::LauncherItem {
            name: "Back".into(),
            ..Default::default()
        }];
        update(&mut scene, &overlay).unwrap();
        super::paint_at(&mut app, 2548).unwrap();
        super::paint_at(&mut app, 2564).unwrap();
        assert_eq!(model.token(), Some(token));
        update(&mut scene, &library).unwrap();
        super::paint_at(&mut app, 2580).unwrap();
        assert_eq!(model.token(), Some(token));

        library.launcher_shelf.as_mut().unwrap().entries[0].name = "Updated favorite".into();
        library.launcher_shelf.as_mut().unwrap().entries[0].favorite = true;
        update(&mut scene, &library).unwrap();
        super::paint_at(&mut app, 2596).unwrap();
        assert!(!model.hidden());
        super::paint_at(&mut app, 3284).unwrap();
        super::paint_at(&mut app, 3300).unwrap();
        super::paint_at(&mut app, 3316).unwrap();
        let updated = model.token().unwrap();
        assert_ne!(updated, token);
        equivalent(&mut app);
        app.set_theme(crate::theme::Palette::Sunset.theme())
            .unwrap();
        super::paint_at(&mut app, 3332).unwrap();
        assert!(!model.hidden());
        super::paint_at(&mut app, 3348).unwrap();
        assert!(model.token().unwrap() > updated);
        equivalent(&mut app);

        update(&mut scene, &home).unwrap();
        super::paint_at(&mut app, 3364).unwrap();
        assert!(!model.hidden() && !model.paused());
        assert!(model.token().is_none());
        assert!(!app.world.has::<mirui::ui::Hidden>(source));
        update(&mut scene, &library).unwrap();
        super::paint_at(&mut app, 3380).unwrap();
        update(&mut scene, &home).unwrap();
        super::paint_at(&mut app, 3396).unwrap();
        super::paint_at(&mut app, 4084).unwrap();
        assert!(model.token().is_none());
        assert!(!model.hidden() && !model.paused());
    }
}

#[test]
fn inactive_retained_displays_skip_transactions_and_keep_local_time() {
    let mut direct_app = app([480, 320]);
    let direct = view(Page::Choice);
    let direct_scene = draw(&mut direct_app, &direct).unwrap();
    for step in 1..=8 {
        super::paint_at(&mut direct_app, step * 16).unwrap();
    }
    for state in [
        &direct_scene.shelf,
        &direct_scene.catalog,
        &direct_scene.room,
    ] {
        assert_eq!(state.animation_counts(), (0, 8));
    }

    let mut app = app([480, 320]);
    let mut home = view(Page::Games);
    home.library.kind = crate::model::LibraryKind::Favorites;
    home.launcher.enabled = true;
    let mut scene = draw(&mut app, &home).unwrap();
    for step in 1..=8 {
        super::paint_at(&mut app, step * 16).unwrap();
    }
    assert_eq!(scene.shelf.animation_counts(), (8, 0));
    assert_eq!(scene.catalog.animation_counts(), (0, 8));
    assert_eq!(scene.room.animation_counts(), (0, 8));
    let mut catalog = home.clone();
    catalog.launcher.tab = 1;
    update(&mut scene, &catalog).unwrap();
    super::paint_at(&mut app, 144).unwrap();
    assert_eq!(scene.catalog.animation_counts(), (1, 8));
    assert_eq!(scene.catalog.card_motion().selection, 0);
    super::paint_at(&mut app, 160).unwrap();
    assert_eq!(scene.catalog.card_motion().selection, 16);

    let mut room = view(Page::Mode);
    room.return_to_launcher = true;
    update(&mut scene, &room).unwrap();
    super::paint_at(&mut app, 176).unwrap();
    assert_eq!(scene.room.animation_counts(), (1, 10));
    assert_eq!(scene.room.card_motion().selection, 0);
    scene.press(176);
    super::paint_at(&mut app, 192).unwrap();
    assert_eq!(scene.room.card_motion().press, 0);
    super::paint_at(&mut app, 208).unwrap();
    assert_eq!(scene.room.card_motion().press, 16);
    let global_launcher = super::launcher(&mut app);
    assert_eq!(global_launcher.view(), room.launcher);
}

#[test]
fn action_motion_damage_uses_only_the_panel_and_stops_when_settled() {
    let mut app = app([640, 480]);
    let mut current = view(Page::Games);
    current.library.kind = crate::model::LibraryKind::Favorites;
    current.launcher.enabled = true;
    current.launcher.overlay = "系统设置".into();
    current.launcher.overlay_kind = "system".into();
    current.launcher.key = "owned-game".into();
    current.launcher.items = (0..6)
        .map(|index| crate::model::LauncherItem {
            name: format!("操作 {index}"),
            ..Default::default()
        })
        .collect();
    let mut scene = draw(&mut app, &current).unwrap();
    super::paint_at(&mut app, 1000).unwrap();
    current.launcher.selected = 1;
    update(&mut scene, &current).unwrap();
    let launcher = scene.state.launcher();
    launcher.tick(1017);
    let panel = app
        .world
        .query::<actions_damage::ActionsPanel>()
        .iter()
        .next()
        .unwrap()
        .0;
    let region = app.world.get::<ComputedRect>(panel).unwrap().0;
    assert!(region.w < 640.into() && region.h < 480.into());
    let mut damage = actions_damage::ActionsDamage::default();
    assert_eq!(damage.region(&app.world), Some(region));
    assert_eq!(damage.region(&app.world), None);
    launcher.tick(1034);
    assert_eq!(damage.region(&app.world), Some(region));
    assert_eq!(damage.region(&app.world), None);
    current.launcher.key = "changed-game".into();
    update(&mut scene, &current).unwrap();
    assert_eq!(damage.region(&app.world), Some(region));
    assert_eq!(damage.region(&app.world), None);
    let shifted = Rect {
        x: region.x + Fixed::ONE,
        ..region
    };
    app.world.get_mut::<ComputedRect>(panel).unwrap().0 = shifted;
    launcher.tick(1051);
    assert_eq!(damage.region(&app.world), Some(region.union(&shifted)));
    app.world.get_mut::<ComputedRect>(panel).unwrap().0 = region;
    launcher.tick(1068);
    assert_eq!(damage.region(&app.world), Some(region.union(&shifted)));
    launcher.tick(1400);
    assert_eq!(damage.region(&app.world), Some(region));
    assert_eq!(damage.region(&app.world), None);
    launcher.tick(1500);
    assert_eq!(damage.region(&app.world), None);
    current.launcher.selected = 2;
    update(&mut scene, &current).unwrap();
    launcher.tick(1517);
    assert_eq!(damage.region(&app.world), Some(region));
    current.launcher.overlay.clear();
    update(&mut scene, &current).unwrap();
    assert_eq!(damage.region(&app.world), Some(region));
    assert_eq!(damage.region(&app.world), None);
}

#[test]
fn static_cartridge_snapshots_follow_content_geometry_and_row_lifetime() {
    let mut app = app([640, 480]);
    let mut home = view(Page::Games);
    home.library.kind = crate::model::LibraryKind::Favorites;
    home.library.total = 6;
    home.launcher.enabled = true;
    home.entries = (0..6)
        .map(|index| crate::model::GameRow {
            key: format!("static-{index}"),
            name: format!("Game {index}"),
            icon: "nes".into(),
            ..crate::model::GameRow::default()
        })
        .collect();
    home.selected = 0;
    home.launcher_shelf = Some(Box::new(home.clone()));
    let mut scene = draw(&mut app, &home).unwrap();
    let mut previous = 0;
    for frame in 0..100 {
        super::paint_at(&mut app, frame * 17).unwrap();
        let (count, captures) = static_snapshot::counts(&app.world);
        assert!(count <= shelf::WINDOW_BUDGET);
        assert!(
            captures <= previous + 1,
            "At most one static snapshot per normal tick"
        );
        previous = captures;
    }
    let pictures: Vec<_> = app
        .world
        .query::<static_snapshot::Source>()
        .iter()
        .map(|(_, source)| source.row.static_picture())
        .collect();
    assert_eq!(pictures.len(), 6);
    assert!(
        pictures
            .iter()
            .all(|picture| picture.hidden() && picture.token().is_some())
    );
    let captured = static_snapshot::counts(&app.world).1;
    let source = app
        .world
        .query::<static_snapshot::Source>()
        .iter()
        .next()
        .unwrap()
        .0;
    let wrapper = app.world.get::<mirui::ui::Parent>(source).unwrap().0;
    let original = app.world.get::<ComputedRect>(wrapper).unwrap().0;
    assert!(
        original.w > Fixed::ZERO && original.h > Fixed::ZERO,
        "Frozen content keeps its layout allocation"
    );
    home.selected = 1;
    home.launcher_shelf.as_mut().unwrap().selected = 1;
    update(&mut scene, &home).unwrap();
    for frame in 100..160 {
        super::paint_at(&mut app, frame * 17).unwrap();
    }
    assert_eq!(
        static_snapshot::counts(&app.world).1,
        captured,
        "Selection motion does not recapture static content"
    );
    assert_eq!(app.world.get::<ComputedRect>(wrapper).unwrap().0, original);
    app.set_theme(crate::theme::Palette::Sunset.theme())
        .unwrap();
    for frame in 160..220 {
        super::paint_at(&mut app, frame * 17).unwrap();
    }
    assert_eq!(static_snapshot::counts(&app.world).1, captured + 6);
    app.world
        .get_mut::<mirui::ui::Style>(wrapper)
        .unwrap()
        .layout
        .max_height = Dimension::px(original.h.to_int() - 4);
    app.world.invalidate(wrapper);
    for frame in 220..280 {
        super::paint_at(&mut app, frame * 17).unwrap();
    }
    let resized = app.world.get::<ComputedRect>(wrapper).unwrap().0;
    assert!(resized.h > Fixed::ZERO && resized.h < original.h);
    assert_eq!(
        static_snapshot::counts(&app.world).1,
        captured + 7,
        "Only the resized picture is recaptured"
    );
    home.entries.truncate(1);
    home.entries[0].key = "replacement".into();
    home.library.total = 1;
    home.selected = 0;
    home.launcher_shelf = Some(Box::new({
        let mut shelf = home.clone();
        shelf.launcher_shelf = None;
        shelf
    }));
    update(&mut scene, &home).unwrap();
    for frame in 280..360 {
        super::paint_at(&mut app, frame * 17).unwrap();
    }
    assert_eq!(
        static_snapshot::counts(&app.world).0,
        1,
        "Retired members release their snapshots before another member is read"
    );
}

#[test]
fn platform_preview_retains_console_entities_across_selection_changes() {
    use mirui::ui::{Hidden, Parent};
    let mut app = app([640, 480]);
    let mut current = view(Page::Games);
    current.platforms = true;
    current.library.kind = crate::model::LibraryKind::Platforms;
    current.library.total = 4;
    current.entries = ["nes", "snes", "gba", "psx"]
        .into_iter()
        .enumerate()
        .map(|(index, platform)| crate::model::GameRow {
            key: format!("retained-{index}"),
            name: platform.into(),
            icon: platform.into(),
            available: true,
            ..Default::default()
        })
        .collect();
    let mut scene = draw(&mut app, &current).unwrap();
    super::paint_at(&mut app, 1000).unwrap();
    let preview = app
        .world
        .query::<Style>()
        .iter()
        .find_map(|(entity, style)| {
            let rect = app.world.get::<ComputedRect>(entity)?.0;
            (rect.w > Fixed::from_int(100)
                && rect.h > Fixed::from_int(100)
                && style.bg_color == Some(style::PANEL.into()))
            .then_some(entity)
        })
        .unwrap();
    let descendants = |app: &TestApp| {
        app.world
            .query::<Style>()
            .iter()
            .filter_map(|(entity, _)| {
                selection::contains(preview, entity, |node| {
                    app.world.get::<Parent>(node).map(|p| p.0)
                })
                .then_some(entity)
            })
            .collect::<Vec<_>>()
    };
    let members = descendants(&app);
    let layers: Vec<_> = members
        .iter()
        .copied()
        .filter(|entity| {
            app.world.get::<Style>(*entity).unwrap().layout.position == Position::Absolute
                && app.world.get::<Parent>(*entity).map(|p| p.0) == Some(preview)
        })
        .collect();
    assert_eq!(layers.len(), 4);
    for (step, selected) in [1, 2, 3, 0].into_iter().enumerate() {
        current.selected = selected;
        update(&mut scene, &current).unwrap();
        super::paint_at(&mut app, 1016 + step as u64 * 16).unwrap();
        assert_eq!(descendants(&app), members);
        assert_eq!(
            layers
                .iter()
                .filter(|node| !app.world.has::<Hidden>(**node))
                .count(),
            1
        );
    }
}

#[test]
fn menu_ownership_stays_bounded_after_leaving_save_manager() {
    use crate::model::{Action, LauncherItem};
    fn frame(app: &mut TestApp, time: &mut u64) {
        *time += 300;
        super::paint_at(app, *time).unwrap();
        *time += 16;
        super::paint_at(app, *time).unwrap();
    }
    fn visible(world: &mirui::ecs::World, mut entity: mirui::ecs::Entity) -> bool {
        loop {
            if mirui::ui::branch::is_effectively_hidden(world, entity) {
                return false;
            }
            let Some(parent) = world.get::<mirui::ui::Parent>(entity) else {
                return true;
            };
            entity = parent.0;
        }
    }
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut time = 0;
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "游戏操作".into();
        current.launcher.overlay_kind = "game".into();
        current.launcher.items = (0..7)
            .map(|index| {
                LauncherItem::available(howff_menu::MenuAction::Back, format!("Action {index}"))
            })
            .collect();
        let mut scene = draw(&mut app, &current).unwrap();
        frame(&mut app, &mut time);
        current.launcher.overlay = "存档".into();
        current.launcher.overlay_kind = "saves".into();
        current.launcher.items.clear();
        current.launcher.saves = Some(crate::launcher::saves::tests::fixture());
        update(&mut scene, &current).unwrap();
        frame(&mut app, &mut time);
        scene.state.launcher().saves_input(Action::Confirm);
        frame(&mut app, &mut time);
        scene.state.launcher().saves_input(Action::Back);
        frame(&mut app, &mut time);
        current.launcher.saves = None;
        current.launcher.overlay.clear();
        update(&mut scene, &current).unwrap();
        frame(&mut app, &mut time);
        for (kind, items) in [("system", 6), ("game", 7), ("system", 6), ("exit", 2)] {
            current.launcher.overlay = kind.into();
            current.launcher.overlay_kind = kind.into();
            current.launcher.selected = 0;
            current.launcher.items = (0..items)
                .map(|index| {
                    LauncherItem::available(howff_menu::MenuAction::Back, format!("{kind} {index}"))
                })
                .collect();
            update(&mut scene, &current).unwrap();
            frame(&mut app, &mut time);
            let count = app.world.query::<Style>().iter().count();
            for step in 0..items * 3 {
                current.launcher.selected = step % items;
                update(&mut scene, &current).unwrap();
                frame(&mut app, &mut time);
                let panels = app
                    .world
                    .query::<actions_damage::ActionsPanel>()
                    .iter()
                    .count();
                assert_eq!(panels, 1, "{size:?} {kind} step={step}");
                let mut anchors: Vec<_> = app
                    .world
                    .query::<deck::ActionAnchor>()
                    .iter()
                    .filter(|(entity, anchor)| {
                        anchor.1 == deck::ActionLayout::Grid && visible(&app.world, *entity)
                    })
                    .map(|(_, anchor)| anchor.0)
                    .collect();
                anchors.sort_unstable();
                assert_eq!(
                    anchors,
                    (0..items).collect::<Vec<_>>(),
                    "Each action has exactly one visible node: {size:?} {kind} step={step}"
                );
                assert_eq!(
                    app.world.query::<Style>().iter().count(),
                    count,
                    "Menu navigation must preserve its mounted subtree: {size:?} {kind} step={step}"
                );
            }
        }
    }
}

#[test]
fn save_manager_keeps_the_current_actor_and_actions_within_each_viewport() {
    use crate::launcher::saves::Page as SavePage;
    use crate::model::Action;
    for size in [[480, 320], [640, 480]] {
        for palette in crate::theme::Palette::ALL {
            let mut app = app(size);
            app.set_theme(palette.theme()).unwrap();
            let mut current = view(Page::Games);
            current.launcher.enabled = true;
            current.launcher.overlay = "存档".into();
            current.launcher.overlay_kind = "saves".into();
            current.launcher.saves = Some(crate::launcher::saves::tests::fixture());
            let scene = draw(&mut app, &current).unwrap();
            paint_at(&mut app, &scene, 800);
            let launcher = scene.state.launcher();
            let saves = launcher.saves();
            for (step, actions) in [
                vec![],
                vec![Action::Down; 9],
                vec![Action::Confirm],
                vec![Action::Back],
                vec![Action::Refresh],
                vec![Action::Back, Action::Right],
                vec![Action::Right],
            ]
            .into_iter()
            .enumerate()
            {
                for action in actions {
                    launcher.saves_input(action);
                }
                paint_at(&mut app, &scene, 1200 + step as u64 * 1000);
                if saves.tab() != howff_menu::saves::Tab::Session {
                    let layout = if saves.modal() {
                        deck::ActionLayout::SavesModal
                    } else {
                        deck::ActionLayout::SavesList
                    };
                    let rect = app
                        .world
                        .query::<deck::ActionAnchor>()
                        .iter()
                        .find_map(|(entity, anchor)| {
                            (anchor.1 == layout
                                && anchor.0 == saves.selected()
                                && !mirui::ui::branch::is_effectively_hidden(&app.world, entity))
                            .then(|| app.world.get::<ComputedRect>(entity).map(|r| r.0))
                            .flatten()
                        })
                        .expect("active save action");
                    assert!(
                        rect.y >= Fixed::ZERO
                            && rect.y + rect.h <= Fixed::from_int(i32::from(size[1])),
                        "{size:?} {step} {rect:?}"
                    );
                    assert_eq!(launcher.menu_selection(), Some(saves.selected()));
                    let target = app
                        .world
                        .query::<actor::ActorFrame>()
                        .iter()
                        .find_map(|(e, _)| {
                            (!app.world.has::<mirui::ui::Hidden>(e))
                                .then(|| app.world.get::<FollowBounds>(e).map(|b| b.target))
                                .flatten()
                        })
                        .expect("visible save actor");
                    let anchor = app.world.get::<deck::ActionAnchor>(target).unwrap();
                    assert_eq!(
                        (anchor.0, anchor.1),
                        (saves.selected(), layout),
                        "{size:?} {step}"
                    );
                }
                assert_eq!(saves.page() == SavePage::Browse, step != 2 && step != 4);
                if let Ok(directory) = std::env::var("NEARBY_SAVE_CAPTURES") {
                    let directory = std::path::Path::new(&directory);
                    std::fs::create_dir_all(directory).unwrap();
                    let file = std::fs::File::create(directory.join(format!(
                        "saves-{}-{}-{step}.png",
                        palette.id(),
                        size[0]
                    )))
                    .unwrap();
                    let mut encoder =
                        png::Encoder::new(file, u32::from(size[0]), u32::from(size[1]));
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let frame = app.backend.framebuffer();
                    encoder
                        .write_header()
                        .unwrap()
                        .write_image_data(frame.read_pixels().unwrap().bytes())
                        .unwrap();
                }
            }
        }
    }
}

#[test]
fn save_operation_progress_has_one_exact_actor_target_and_close_is_deferred() {
    let mut app = app([480, 320]);
    let mut current = view(Page::Games);
    current.launcher.enabled = true;
    current.launcher.overlay = "存档".into();
    current.launcher.overlay_kind = "saves".into();
    let mut document = crate::launcher::saves::tests::fixture();
    document.busy = true;
    current.launcher.saves = Some(document);
    let scene = draw(&mut app, &current).unwrap();
    for time in [800, 960, 1400, 1800] {
        paint_at(&mut app, &scene, time);
        let target = app
            .world
            .query::<actor::ActorFrame>()
            .iter()
            .find_map(|(entity, _)| app.world.get::<FollowBounds>(entity))
            .unwrap();
        assert!(target.transition.is_none());
        let anchor = app.world.get::<deck::ActionAnchor>(target.target).unwrap();
        assert_eq!((anchor.0, anchor.1), (0, deck::ActionLayout::SavesProgress));
    }
    scene
        .state
        .launcher()
        .saves_input(crate::model::Action::Back);
    assert!(scene.state.launcher().saves().busy());
}

#[test]
fn management_name_uses_framework_composition_and_retires_its_input_session() {
    use crate::model::Action;
    use howff_menu::management::{Device, Operation, View as ManagementView};
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "设备与好友".into();
        current.launcher.overlay_kind = "management".into();
        current.launcher.management = Some(ManagementView {
            epoch: 10,
            revision: 1,
            device: Device {
                id: "a".repeat(64),
                name: "我的掌机".into(),
                address: String::new(),
            },
            ..Default::default()
        });
        let mut scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 800);
        let launcher = scene.state.launcher();
        let manager = launcher.management();
        let sent = Rc::new(std::cell::RefCell::new(Vec::new()));
        let output = sent.clone();
        manager
            .on_effect(
                move |request: &howff_menu::input::Sourced<howff_menu::management::Request>| {
                    let request = &request.payload;
                    output.borrow_mut().push(request.clone())
                },
            )
            .unwrap();
        launcher.management_input(Action::Confirm);
        paint_at(&mut app, &scene, 1000);
        assert!(manager.editing());
        assert!(launcher.input_session().is_some());
        for _ in 0..4 {
            launcher.management_input(Action::Auxiliary);
        }
        paint_at(&mut app, &scene, 1200);
        assert!(
            manager.text().value().is_empty(),
            "caret starts at the existing name's end: value={:?}, queued={}, selections={:?}",
            manager.text().value(),
            manager.text().edit().is_some(),
            app.world
                .query::<mirui::ui::widgets::TextEditing>()
                .iter()
                .map(|(_, e)| e.selection())
                .collect::<Vec<_>>()
        );
        let mut clock = 1200;
        for (input, word) in [("wo", "我"), ("de", "的"), ("zhang", "掌"), ("ji", "机")] {
            let text = manager.text();
            for ch in input.chars() {
                let target = "qwertyuiopasdfghjkl'zxcvbnm"
                    .chars()
                    .position(|v| v == ch)
                    .unwrap();
                while text.selected() < target {
                    launcher.management_input(Action::Right);
                }
                while text.selected() > target {
                    launcher.management_input(Action::Left);
                }
                launcher.management_input(Action::Confirm);
            }
            while text.selected() > 0 {
                launcher.management_input(Action::Left);
            }
            launcher.management_input(Action::Up);
            let pages = text.candidate_count().div_ceil(text.page_size());
            let mut matched = false;
            for _ in 0..pages {
                if let Some(index) =
                    (0..text.page_size()).find(|i| text.candidate_label(*i) == word)
                {
                    for _ in 0..index {
                        launcher.management_input(Action::Right);
                    }
                    launcher.management_input(Action::Confirm);
                    matched = true;
                    break;
                }
                launcher.management_input(Action::PageDown);
            }
            assert!(matched, "missing {word}");
            clock += 250;
            paint_at(&mut app, &scene, clock);
        }
        assert_eq!(manager.text().value(), "我的掌机");
        launcher.management_input(Action::Start);
        paint_at(&mut app, &scene, clock + 250);
        assert_eq!(sent.borrow().len(), 1);
        assert!(matches!(&sent.borrow()[0].operation,Operation::Rename{name} if name=="我的掌机"));
        assert_eq!(sent.borrow()[0].epoch, 10);
        current.launcher.management = None;
        current.launcher.overlay.clear();
        update(&mut scene, &current).unwrap();
        paint_at(&mut app, &scene, clock + 500);
        assert!(launcher.input_session().is_none());
        assert!(manager.text().edit().is_none());
    }
}

#[test]
fn management_navigation_and_update_progress_keep_framework_anchors() {
    use crate::launcher::management::Page as ManagementPage;
    use crate::model::Action;
    use howff_menu::management::{
        Device, Entry, Fact, Operation, Package, Update, UpdatePhase, View as ManagementView,
    };
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "应用管理".into();
        current.launcher.overlay_kind = "management".into();
        current.launcher.management = Some(ManagementView {
            epoch: 20,
            revision: 1,
            entry: Entry::Application,
            version: "0.11.0".into(),
            profile: "ArkOS".into(),
            device: Device {
                id: "a".repeat(64),
                name: "我的掌机".into(),
                address: String::new(),
            },
            facts: vec![Fact {
                label: "安装位置".into(),
                value: "/opt/howff".into(),
            }],
            ..Default::default()
        });
        let mut scene = draw(&mut app, &current).unwrap();
        let launcher = scene.state.launcher();
        let manager = launcher.management();
        let sent = Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = sent.clone();
        manager
            .on_effect(
                move |r: &howff_menu::input::Sourced<howff_menu::management::Request>| {
                    let r = &r.payload;
                    sink.borrow_mut().push(r.clone())
                },
            )
            .unwrap();
        let mut clock = 800;
        let mut capture = |app: &mut TestApp, scene: &Scene, name: &str| {
            clock += 250;
            paint_at(app, scene, clock);
            if let Ok(folder) = std::env::var("NEARBY_MANAGEMENT_CAPTURES") {
                let folder = std::path::Path::new(&folder);
                std::fs::create_dir_all(folder).unwrap();
                let file =
                    std::fs::File::create(folder.join(format!("{name}-{}.png", size[0]))).unwrap();
                let mut encoder = png::Encoder::new(file, u32::from(size[0]), u32::from(size[1]));
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .unwrap()
                    .write_image_data(app.backend.framebuffer().read_pixels().unwrap().bytes())
                    .unwrap();
            }
        };
        capture(&mut app, &scene, "application");
        let first = app
            .world
            .query::<deck::ActionAnchor>()
            .iter()
            .find_map(|(e, a)| {
                (a.1 == deck::ActionLayout::Management
                    && a.0 == 0
                    && !mirui::ui::branch::is_effectively_hidden(&app.world, e))
                .then(|| app.world.get::<ComputedRect>(e).map(|r| r.0))
                .flatten()
            })
            .unwrap();
        let summary = app
            .world
            .query::<Text>()
            .iter()
            .find_map(|(e, t)| {
                (t.resolve(None).contains("Nearby 0.11.0")
                    && !mirui::ui::branch::is_effectively_hidden(&app.world, e))
                .then(|| app.world.get::<ComputedRect>(e).map(|r| r.0))
                .flatten()
            })
            .unwrap();
        assert!(
            summary.y + summary.h <= first.y,
            "summary overlaps first action: {summary:?} {first:?}"
        );
        launcher.management_input(Action::Confirm);
        capture(&mut app, &scene, "version");
        launcher.management_input(Action::Confirm);
        capture(&mut app, &scene, "fact");
        assert_eq!(manager.page(), ManagementPage::Detail);
        launcher.management_input(Action::Back);
        launcher.management_input(Action::Back);
        assert_eq!(manager.selected(), 0);
        for _ in 0..3 {
            launcher.management_input(Action::Down);
        }
        launcher.management_input(Action::Confirm);
        capture(&mut app, &scene, "package-loading");
        assert!(matches!(
            sent.borrow().last().unwrap().operation,
            Operation::Browse { .. }
        ));
        let state = current.launcher.management.as_mut().unwrap();
        state.revision = sent.borrow().last().unwrap().sequence;
        state.packages = vec![Package {
            key: "/roms".into(),
            name: "游戏卡".into(),
            directory: true,
            size: 0,
        }];
        update(&mut scene, &current).unwrap();
        launcher.management_input(Action::Confirm);
        capture(&mut app, &scene, "card-loading");
        let state = current.launcher.management.as_mut().unwrap();
        state.revision = sent.borrow().last().unwrap().sequence;
        state.directory = "/roms".into();
        state.packages = vec![Package {
            key: "/roms/nearby.tar.gz".into(),
            name: "nearby.tar.gz".into(),
            directory: false,
            size: 16_000_000,
        }];
        update(&mut scene, &current).unwrap();
        capture(&mut app, &scene, "packages");
        launcher.management_input(Action::Confirm);
        capture(&mut app, &scene, "package-checking");
        let state = current.launcher.management.as_mut().unwrap();
        state.revision = sent.borrow().last().unwrap().sequence;
        state.update = Some(Update {
            path: "/roms/nearby.tar.gz".into(),
            version: "0.11.0".into(),
            digest: "a".repeat(64),
            compatible: true,
            reason: "适配当前设备，校验通过".into(),
            phase: UpdatePhase::Ready,
        });
        update(&mut scene, &current).unwrap();
        capture(&mut app, &scene, "package-ready");
        assert_eq!(manager.page(), ManagementPage::Update);
        launcher.management_input(Action::Confirm);
        capture(&mut app, &scene, "install-confirm");
        assert_eq!(manager.selected(), 0);
        launcher.management_input(Action::Down);
        launcher.management_input(Action::Confirm);
        capture(&mut app, &scene, "installing");
        assert!(manager.busy());
        for action in [Action::Back, Action::Confirm, Action::Down, Action::Refresh] {
            launcher.management_input(action);
        }
        assert_eq!(manager.page(), ManagementPage::Progress);
        let target = app
            .world
            .query::<actor::ActorFrame>()
            .iter()
            .find_map(|(e, _)| app.world.get::<FollowBounds>(e))
            .unwrap();
        let anchor = app.world.get::<deck::ActionAnchor>(target.target).unwrap();
        assert_eq!(anchor.1, deck::ActionLayout::ManagementProgress);
        assert!(target.transition.is_none());
        let state = current.launcher.management.as_mut().unwrap();
        state.revision = sent.borrow().last().unwrap().sequence;
        state.update.as_mut().unwrap().phase = UpdatePhase::Complete;
        update(&mut scene, &current).unwrap();
        capture(&mut app, &scene, "installed");
        assert!(!manager.busy());
        launcher.management_input(Action::Confirm);
        capture(&mut app, &scene, "reopening");
        assert!(matches!(
            sent.borrow().last().unwrap().operation,
            Operation::Close
        ));
    }
}

#[test]
fn management_rows_keep_icon_slots_and_visual_order_during_navigation() {
    use crate::model::Action;
    use howff_menu::management::{Entry, View as ManagementView};
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "应用管理".into();
        current.launcher.overlay_kind = "management".into();
        current.launcher.management = Some(ManagementView {
            epoch: 20,
            revision: 1,
            entry: Entry::Application,
            version: "0.11.0".into(),
            profile: "ArkOS".into(),
            ..Default::default()
        });
        let scene = draw(&mut app, &current).unwrap();
        let launcher = scene.state.launcher();
        let manager = launcher.management();
        let mut clock = 800;
        paint_at(&mut app, &scene, clock);
        for action in [
            Action::Down,
            Action::Down,
            Action::Down,
            Action::Up,
            Action::Up,
            Action::Up,
        ] {
            launcher.management_input(action);
            clock += 300;
            paint_at(&mut app, &scene, clock);
            let mut rows = vec![];
            for (entity, anchor) in app.world.query::<deck::ActionAnchor>().iter() {
                if anchor.1 != deck::ActionLayout::Management
                    || mirui::ui::branch::is_effectively_hidden(&app.world, entity)
                {
                    continue;
                }
                let Some(bounds) = app.world.get::<ComputedRect>(entity) else {
                    continue;
                };
                let Some(row) = manager.row(anchor.0) else {
                    continue;
                };
                let descendant = |node| {
                    selection::contains(entity, node, |e| {
                        app.world.get::<mirui::ui::Parent>(e).map(|p| p.0)
                    })
                };
                let title = app
                    .world
                    .query::<Text>()
                    .iter()
                    .find_map(|(e, t)| {
                        (descendant(e) && t.resolve(None) == row.title)
                            .then(|| app.world.get::<ComputedRect>(e).map(|r| r.0))
                            .flatten()
                    })
                    .expect("row title");
                let icon = app
                    .world
                    .query::<mirui::ui::widgets::Image>()
                    .iter()
                    .find_map(|(e, _)| {
                        descendant(e)
                            .then(|| app.world.get::<ComputedRect>(e).map(|r| r.0))
                            .flatten()
                    })
                    .expect("row icon");
                assert!(
                    icon.x + icon.w <= title.x,
                    "{size:?} selected={} row={} icon moved after title: {icon:?} {title:?}",
                    manager.selected(),
                    anchor.0
                );
                let slot = app.world.get::<mirui::ui::Parent>(entity).unwrap().0;
                let list = app.world.get::<mirui::ui::Parent>(slot).unwrap().0;
                let slot_rect = app.world.get::<ComputedRect>(slot).unwrap().0;
                let list_rect = app.world.get::<ComputedRect>(list).unwrap().0;
                assert!(
                    slot_rect.y >= list_rect.y
                        && slot_rect.y + slot_rect.h <= list_rect.y + list_rect.h,
                    "{size:?} rows={} row={} clipped {slot_rect:?} in {list_rect:?}",
                    manager.visible_rows(),
                    anchor.0
                );
                rows.push((anchor.0, bounds.0.y));
            }
            rows.sort_by_key(|r| r.0);
            assert!(
                rows.windows(2).all(|pair| pair[0].1 < pair[1].1),
                "row order changed: {rows:?}"
            );
        }
    }
}

#[test]
fn management_reading_preserves_natural_text_and_clamps_to_measured_content() {
    use crate::model::Action;
    use howff_menu::management::{Entry, View as ManagementView};
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "应用管理".into();
        current.launcher.overlay_kind = "management".into();
        let contents = (0..40)
            .map(|i| format!("路径 {i:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        current.launcher.management = Some(ManagementView {
            epoch: 20,
            revision: 1,
            entry: Entry::Application,
            version: "0.11.0".into(),
            profile: contents.clone(),
            ..Default::default()
        });
        let scene = draw(&mut app, &current).unwrap();
        let launcher = scene.state.launcher();
        let manager = launcher.management();
        paint_at(&mut app, &scene, 800);
        for action in [Action::Confirm, Action::Down, Action::Down, Action::Confirm] {
            launcher.management_input(action);
        }
        paint_at(&mut app, &scene, 1200);
        let (text_entity, _) = app
            .world
            .query::<Text>()
            .iter()
            .find(|(e, t)| {
                t.resolve(None) == contents
                    && !mirui::ui::branch::is_effectively_hidden(&app.world, *e)
                    && app.world.get::<ComputedRect>(*e).is_some()
            })
            .expect("detail body");
        let text = app.world.get::<ComputedRect>(text_entity).unwrap().0;
        let (scroll_entity, _) = app
            .world
            .query::<mirui::input::event::scroll::ScrollOffset>()
            .iter()
            .find(|(e, _)| {
                !mirui::ui::branch::is_effectively_hidden(&app.world, *e)
                    && selection::contains(*e, text_entity, |node| {
                        app.world.get::<mirui::ui::Parent>(node).map(|p| p.0)
                    })
            })
            .unwrap();
        let viewport = app.world.get::<ComputedRect>(scroll_entity).unwrap().0;
        app.with_widget(reading_geometry_probe::view());
        app.world.insert(
            scroll_entity,
            ReadingGeometryProbe {
                target: text_entity,
            },
        );
        assert!(
            text.h >= Fixed::from_int(40 * i32::from(style::TITLE_SIZE)),
            "{size:?}: paragraph compressed: {text:?}; viewport={viewport:?}"
        );
        for _ in 0..300 {
            launcher.management_input(Action::Down);
        }
        paint_at(&mut app, &scene, 1600);
        let offset = app
            .world
            .get::<mirui::input::event::scroll::ScrollOffset>(scroll_entity)
            .unwrap();
        assert_eq!(
            offset.y,
            (text.h - viewport.h).max(Fixed::ZERO),
            "bottom must expose final line without scrolling past it"
        );
        assert_reading_bottom(&app, text_entity, 0);
        for _ in 0..300 {
            launcher.management_input(Action::Up);
        }
        paint_at(&mut app, &scene, 2000);
        assert_eq!(manager.detail_scroll(), Fixed::ZERO);
    }
}

#[test]
fn configuration_rows_keep_wrapped_text_inside_stable_ordered_slots() {
    use crate::model::Action;
    use howff_menu::configuration::{Load, Section};
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "游戏设置".into();
        current.launcher.overlay_kind = "configuration".into();
        let mut config = crate::launcher::configuration::tests::fixture();
        if let Load::Ready { document } = &mut config.load {
            for field in &mut document.fields {
                if field.section == Section::Core {
                    field.label = format!("{} 画面与声音同步的配置选项", field.label);
                }
            }
        }
        current.launcher.configuration = Some(config);
        let scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 800);
        let launcher = scene.state.launcher();
        launcher.configuration_input(Action::Down);
        launcher.configuration_input(Action::Confirm);
        let mut clock = 1000;
        for action in
            std::iter::repeat_n(Action::Down, 24).chain(std::iter::repeat_n(Action::Up, 24))
        {
            launcher.configuration_input(action);
            clock += 160;
            paint_at(&mut app, &scene, clock);
            let mut rows = vec![];
            for (e, a) in app.world.query::<deck::ActionAnchor>().iter() {
                if a.1 != deck::ActionLayout::Configuration
                    || mirui::ui::branch::is_effectively_hidden(&app.world, e)
                {
                    continue;
                }
                let Some(bounds) = app.world.get::<ComputedRect>(e).map(|r| r.0) else {
                    continue;
                };
                for (child, _) in app.world.query::<Text>().iter() {
                    if selection::contains(e, child, |n| {
                        app.world.get::<mirui::ui::Parent>(n).map(|p| p.0)
                    }) && let Some(text) = app.world.get::<ComputedRect>(child).map(|r| r.0)
                    {
                        assert!(
                            text.y >= bounds.y && text.y + text.h <= bounds.y + bounds.h,
                            "{size:?} row={} text exceeds row: {text:?} {bounds:?}",
                            a.0
                        );
                    }
                }
                rows.push((a.0, bounds.y));
            }
            rows.sort_by_key(|r| r.0);
            assert!(
                rows.windows(2).all(|p| p[0].1 < p[1].1),
                "{size:?} row order: {rows:?}"
            );
        }
    }
}

#[test]
fn save_list_slots_include_inter_row_spacing_and_shadow_in_the_viewport_budget() {
    use crate::model::Action;
    for size in [
        [480, 320],
        [640, 360],
        [640, 380],
        [640, 440],
        [640, 460],
        [640, 520],
    ] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "存档".into();
        current.launcher.overlay_kind = "saves".into();
        current.launcher.saves = Some(crate::launcher::saves::tests::fixture());
        let scene = draw(&mut app, &current).unwrap();
        let launcher = scene.state.launcher();
        paint_at(&mut app, &scene, 800);
        for _ in 0..20 {
            launcher.saves_input(Action::Down);
        }
        paint_at(&mut app, &scene, 1200);
        for (e, a) in app.world.query::<deck::ActionAnchor>().iter() {
            if a.1 != deck::ActionLayout::SavesList
                || mirui::ui::branch::is_effectively_hidden(&app.world, e)
            {
                continue;
            }
            let Some(bounds) = app.world.get::<ComputedRect>(e).map(|r| r.0) else {
                continue;
            };
            let row = app.world.get::<mirui::ui::Parent>(e).unwrap().0;
            let parent = app.world.get::<mirui::ui::Parent>(row).unwrap().0;
            let viewport = app.world.get::<ComputedRect>(parent).unwrap().0;
            let full = app.world.get::<ComputedRect>(row).unwrap().0;
            assert!(
                full.y >= viewport.y && full.y + full.h <= viewport.y + viewport.h,
                "{size:?} row={} clipped: {full:?}, viewport {viewport:?}, action {bounds:?}",
                a.0
            );
        }
    }
}

#[test]
fn save_reading_keeps_complete_paragraphs_and_uses_framework_scroll_bounds() {
    use crate::model::Action;
    use howff_menu::details::Fact;
    for size in [[480, 320], [640, 480]] {
        let mut app = app(size);
        let mut current = view(Page::Games);
        current.launcher.enabled = true;
        current.launcher.overlay = "存档".into();
        current.launcher.overlay_kind = "saves".into();
        let content = (0..20)
            .map(|i| format!("/private/session/path/{i:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut saves = crate::launcher::saves::tests::fixture();
        saves.document.as_mut().unwrap().session = vec![Fact::new("文件", &content)];
        current.launcher.saves = Some(saves);
        let scene = draw(&mut app, &current).unwrap();
        paint_at(&mut app, &scene, 800);
        let launcher = scene.state.launcher();
        launcher.saves_input(Action::PageUp);
        paint_at(&mut app, &scene, 1200);
        let (text_entity, _) = app
            .world
            .query::<Text>()
            .iter()
            .find(|(e, t)| {
                t.resolve(None) == content
                    && !mirui::ui::branch::is_effectively_hidden(&app.world, *e)
                    && app.world.get::<ComputedRect>(*e).is_some()
            })
            .unwrap();
        let text = app.world.get::<ComputedRect>(text_entity).unwrap().0;
        assert!(
            text.h >= Fixed::from_int(20 * i32::from(style::BODY_SIZE)),
            "{size:?}: paragraph truncated {text:?}"
        );
        let bounds_query = app
            .world
            .query::<mirui::input::event::scroll::ScrollBounds>();
        let (scroll_entity, bounds) = bounds_query
            .iter()
            .find(|(e, _)| {
                selection::contains(*e, text_entity, |n| {
                    app.world.get::<mirui::ui::Parent>(n).map(|p| p.0)
                })
            })
            .unwrap();
        let maximum = bounds.max_y;
        app.with_widget(reading_geometry_probe::view());
        app.world.insert(
            scroll_entity,
            ReadingGeometryProbe {
                target: text_entity,
            },
        );
        for _ in 0..100 {
            launcher.saves_input(Action::Down);
        }
        paint_at(&mut app, &scene, 1600);
        let offset = app
            .world
            .get::<mirui::input::event::scroll::ScrollOffset>(scroll_entity)
            .unwrap();
        assert_eq!(offset.y, maximum, "last paragraph line must be reachable");
        assert_reading_bottom(&app, text_entity, 4);
        launcher.saves_input(Action::Up);
        paint_at(&mut app, &scene, 1800);
        assert_eq!(
            app.world
                .get::<mirui::input::event::scroll::ScrollOffset>(scroll_entity)
                .unwrap()
                .y,
            (maximum - Fixed::from_int(24)).max(Fixed::ZERO)
        );
    }
}

#[component]
struct ReadingGeometryProbe {
    #[depends_on(mirui::ui::LayoutGeometry)]
    target: Entity,
}

fn assert_reading_bottom(app: &TestApp, text: Entity, trailing: i32) {
    let output = app
        .world
        .resource::<mirui::ui::Outputs<mirui::ui::LayoutGeometry>>()
        .expect("layout geometry outputs");
    let geometry = output.get(text).expect("prepared text geometry");
    let corners = geometry
        .transform()
        .apply_rect(geometry.border_rect())
        .unwrap();
    let bottom = corners[2].y;
    let clip = geometry.clip();
    let slack = clip.y + clip.h - bottom;
    assert!(
        slack >= Fixed::ZERO && slack <= Fixed::from_int(trailing + 1),
        "final text line is outside the painted clip: bottom={bottom:?} clip={clip:?}"
    );
}

#[view(component = ReadingGeometryProbe)]
fn reading_geometry_probe(
    reader: component!(),
    outputs: res!(mirui::ui::Outputs<mirui::ui::LayoutGeometry>),
) {
    let _ = outputs.get(reader.target);
}

#[test]
fn retained_reading_scroll_updates_pixels_without_a_geometry_subscriber() {
    use crate::model::Action;
    use howff_menu::details::Fact;
    let mut app = app([640, 480]);
    let mut current = view(Page::Games);
    current.launcher_shelf = Some(Box::new(current.clone()));
    current.launcher.enabled = true;
    current.launcher.overlay = "存档".into();
    current.launcher.overlay_kind = "saves".into();
    let mut saved = crate::launcher::saves::tests::fixture();
    let text = "/run/howff-tests/a2-layout-8c7747bf6459/".to_owned()
        + &(0..100)
            .map(|i| format!("save-directory-{i:03}"))
            .collect::<Vec<_>>()
            .join("/")
        + "/超级炸弹人5.state0";
    saved.document.as_mut().unwrap().session = vec![Fact::new("文件", &text)];
    current.launcher.saves = Some(saved);
    let target = current.clone();
    current.launcher.saves = None;
    current.launcher.management = Some(howff_menu::management::View {
        epoch: 9001,
        revision: 1,
        entry: howff_menu::management::Entry::Application,
        profile: "布局项目\n".repeat(50),
        ..Default::default()
    });
    current.launcher.overlay_kind = "management".into();
    let mut scene = draw(&mut app, &current).unwrap();
    paint_at(&mut app, &scene, 100);
    for a in [Action::Confirm, Action::Down, Action::Down, Action::Confirm] {
        scene.state.launcher().management_input(a);
    }
    paint_at(&mut app, &scene, 800);
    update(&mut scene, &target).unwrap();
    paint_at(&mut app, &scene, 1000);
    let launcher = scene.state.launcher();
    launcher.saves_input(Action::Right);
    launcher.saves_input(Action::Right);
    paint_at(&mut app, &scene, 1800);
    let before = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    for _ in 0..100 {
        launcher.saves_input(Action::Down);
    }
    paint_at(&mut app, &scene, 2400);
    assert!(launcher.saves().reading_offset() > Fixed::ZERO);
    assert!(
        before != app.backend.framebuffer().read_pixels().unwrap().bytes(),
        "scroll changed offset but not painted pixels"
    );
}
