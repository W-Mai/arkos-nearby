use super::*;
use crate::model::{LauncherItem, LibraryKind, MenuAction};

fn game_view(kind: LibraryKind, artwork: bool) -> View {
    let mut games = view(Page::Games);
    games.library.kind = kind;
    games.selected = 1;
    games.library.row_selected = 1;
    games.library.position = 2;
    games.library.total = games.entries.len();
    games.launcher.enabled = true;
    games.launcher.tab = usize::from(kind == LibraryKind::Directory);
    games.launcher.key = games.entries[1].key.clone();
    games.launcher.netplay = true;
    games.status.clear();
    for (row, title) in games.entries.iter_mut().zip([
        "坦克大战",
        "超级炸弹人 5 特别版 / Super Bomberman 5 Gold Cartridge",
        "快打旋风",
        "俄罗斯方块",
    ]) {
        row.name = title.into();
    }
    if artwork {
        games.entries[1].favorite = true;
        games.entries[1].artwork = "/howff-fixture/bomberman5.png".into();
        games.artwork = games.entries[1].artwork.clone();
    }
    let mut child = games.clone();
    child.launcher = Default::default();
    if kind == LibraryKind::Directory {
        games.launcher_catalog = Some(Box::new(child.clone()));
    }
    child.library.kind = LibraryKind::Favorites;
    games.launcher_shelf = Some(Box::new(child));
    games
}

fn seed_art(app: &TestApp, path: &str) {
    use mirui::core::resource::ResourceManager;
    use mirui::render::texture::Texture;
    let image = std::env::var_os("HOWFF_TEST_CARTRIDGE_ART")
        .map(|dir| {
            image::open(std::path::PathBuf::from(dir).join("bomberman5.png"))
                .unwrap()
                .to_rgba8()
        })
        .unwrap_or_else(|| image::RgbaImage::from_pixel(16, 12, image::Rgba([31, 122, 239, 255])));
    let (w, h) = image.dimensions();
    let texture = Texture::from_vec(image.into_raw(), w as u16, h as u16, ColorFormat::RGBA8888)
        .unwrap()
        .with_cache_revision(71);
    let key = format!("nearby-artwork:{}", serde_json::json!([path, null]));
    app.world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap()
        .add_static(key, texture);
}

fn open_actions(games: &View) -> View {
    let mut open = games.clone();
    open.launcher.overlay_kind = "game".into();
    open.launcher.overlay = games.entries[games.selected].name.clone();
    open.launcher.items = [
        (MenuAction::Play, "开始游戏"),
        (MenuAction::Multiplayer, "联机"),
        (MenuAction::ToggleFavorite, "移出收藏"),
        (MenuAction::ShowGameDetails, "详情"),
    ]
    .into_iter()
    .map(|(action, title)| LauncherItem::available(action, title))
    .collect();
    open
}

fn assert_first_frame_near_origin(app: &mut TestApp, size: [u16; 2], origin: Rect, before: &[u8]) {
    let framebuffer = app.backend.framebuffer();
    let frame = framebuffer.read_pixels().unwrap();
    let width = usize::from(size[0]);
    let changed = before
        .chunks_exact(4)
        .zip(frame.bytes().chunks_exact(4))
        .enumerate()
        .filter(|(i, (old, new))| {
            let x = (i % width) as i32;
            let y = (i / width) as i32;
            let inside = (origin.x.to_int() - 4..=(origin.x + origin.w).to_int() + 4).contains(&x)
                && (origin.y.to_int() - 4..=(origin.y + origin.h).to_int() + 4).contains(&y);
            !inside
                && old[..3]
                    .iter()
                    .zip(&new[..3])
                    .any(|(a, b)| a.abs_diff(*b) > 48)
        })
        .count();
    assert!(
        // The background drops its cursor, focus outline and marquee on modal input.
        changed < width * usize::from(size[1]) / 20,
        "{changed} pixels flashed outside the cartridge at rest"
    );
}

fn assert_cartridge_repaint(app: &mut TestApp) {
    let partial = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    let framebuffer = app.backend.framebuffer();
    let full = framebuffer.read_pixels().unwrap();
    // A translucent transformed edge may round a color channel by one level.
    for (a, b) in partial.chunks_exact(4).zip(full.bytes().chunks_exact(4)) {
        assert_eq!(a[3], b[3]);
        assert!(
            a[..3].iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= 1),
            "{a:?} != {b:?}"
        );
    }
}

#[test]
fn cartridge_action_flip_reverses_and_restores_its_game_and_launch_gestures() {
    use super::material::{CardOrigin, CartridgeFront};
    use crate::launcher::Intent;
    for size in [[480, 320], [640, 480]] {
        for (kind, artwork) in [LibraryKind::Favorites, LibraryKind::Directory]
            .into_iter()
            .flat_map(|kind| [false, true].map(|artwork| (kind, artwork)))
        {
            let mut app = app(size);
            let games = game_view(kind, artwork);
            if artwork {
                seed_art(&app, &games.artwork);
            }
            let actions = open_actions(&games);
            let mut scene = draw(&mut app, &games).unwrap();
            let launcher = super::launcher(&mut app);
            let presence = launcher.popup();
            let display = if kind == LibraryKind::Favorites {
                scene.shelf.clone()
            } else {
                scene.catalog.clone()
            };
            let identities = || {
                display
                    .entries()
                    .entries()
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>()
            };
            let saved = identities();
            let selection = display.selected();
            assert_eq!(selection, Some(1));
            paint_at(&mut app, &scene, 480);
            let (source_entity, origin, source_cover) = app
                .world
                .query::<CardOrigin>()
                .iter()
                .find_map(|(e, source)| {
                    if !visible_in_tree(&app.world, e) {
                        return None;
                    }
                    let cover = match source {
                        CardOrigin::Shelf(row)
                            if kind == LibraryKind::Favorites && Some(row.index()) == selection =>
                        {
                            row.cover_bounds()
                        }
                        CardOrigin::Preview(owner) if kind == LibraryKind::Directory => {
                            owner.cover_bounds()
                        }
                        _ => return None,
                    };
                    let rect = if kind == LibraryKind::Favorites {
                        let shell = app
                            .world
                            .get::<mirui::ui::Children>(e)?
                            .0
                            .iter()
                            .copied()
                            .find(|shell| {
                                app.world.has::<super::super::cartridge::Shell>(*shell)
                            })?;
                        let geometry = app
                            .world
                            .resource::<mirui::ui::Outputs<mirui::ui::LayoutGeometry>>()?
                            .get(shell)?;
                        let mut border = geometry.border_rect();
                        border.h += Fixed::from_int(super::super::shelf::SHADOW);
                        Rect::bounding_quad(&geometry.transform().apply_rect(border)?)
                    } else {
                        let geometry = app
                            .world
                            .resource::<mirui::ui::Outputs<mirui::ui::LayoutGeometry>>()?
                            .get(e)?;
                        Rect::bounding_quad(
                            &geometry.transform().apply_rect(geometry.border_rect())?,
                        )
                    };
                    Some((e, rect, cover))
                })
                .expect("active game cartridge");
            let face = app
                .world
                .resource::<state::DisplayRuntime>()
                .unwrap()
                .card_face
                .clone();
            let mut time = 480;
            if artwork {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                while source_cover.token() != Some(71) {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "selected game artwork"
                    );
                    time += 16;
                    paint_at(&mut app, &scene, time);
                }
            }
            let base = time - 480;
            let source_opacity = |app: &TestApp| {
                app.world
                    .get::<mirui::ui::OffscreenRender>(source_entity)
                    .map(|component| component.opacity)
            };
            let original_opacity = source_opacity(&app);
            let before = app
                .backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes()
                .to_vec();
            let record = |app: &mut TestApp, phase: &str, elapsed: u64| {
                if artwork {
                    capture_text_audit(app, &format!("a-{kind:?}-{phase}-{elapsed:03}"), size);
                }
            };
            record(&mut app, "rest", 0);
            update(&mut scene, &actions).unwrap();
            paint_at(&mut app, &scene, time + 16);
            assert_first_frame_near_origin(&mut app, size, origin, &before);
            if kind == LibraryKind::Directory && artwork {
                let framebuffer = app.backend.framebuffer();
                let frame = framebuffer.read_pixels().unwrap();
                let x = (origin.x.to_int() + 2) as usize;
                let y = (origin.y + origin.h / 2).to_int() as usize;
                let sample = 4 * (usize::from(size[0]) * y + x);
                assert_eq!(
                    &frame.bytes()[sample..sample + 3],
                    &before[sample..sample + 3],
                    "preview shell changed color at the start of its flip"
                );
            }
            let front = app
                .world
                .query::<CartridgeFront>()
                .iter()
                .find(|(e, _)| visible_in_tree(&app.world, *e))
                .expect("front face on first frame")
                .0;
            let first = app.world.get::<ComputedRect>(front).unwrap().0;
            for (actual, expected) in [
                (first.x, origin.x),
                (first.y, origin.y),
                (first.w, origin.w),
                (first.h, origin.h),
            ] {
                assert!(
                    (actual - expected).abs() < Fixed::from_int(2),
                    "front {first:?} opened away from {origin:?}"
                );
            }
            for elapsed in (16..=144).step_by(16) {
                paint_at(&mut app, &scene, time + elapsed);
                record(&mut app, "open", elapsed);
                assert_cartridge_repaint(&mut app);
            }
            assert_eq!(face.origin(), Some(origin));
            assert_eq!(source_opacity(&app), original_opacity);
            assert_eq!(face.cover().token(), artwork.then_some(71));
            assert_eq!(face.title(), games.entries[1].name);
            let has_front = |app: &TestApp| {
                app.world
                    .query::<CartridgeFront>()
                    .iter()
                    .any(|(entity, _)| visible_in_tree(&app.world, entity))
            };
            assert!(has_front(&app));
            let during = presence.progress();
            assert!(during > Fixed::ZERO && during < Fixed::ONE);
            update(&mut scene, &games).unwrap();
            assert_eq!(presence.progress(), during);
            for elapsed in (16..=80).step_by(16) {
                paint_at(&mut app, &scene, base + 624 + elapsed);
                record(&mut app, "reverse", elapsed);
                assert_cartridge_repaint(&mut app);
            }
            assert!(presence.progress() > Fixed::ZERO && presence.progress() < during);
            update(&mut scene, &actions).unwrap();
            for elapsed in (16..=400).step_by(16) {
                paint_at(&mut app, &scene, base + 704 + elapsed);
                record(&mut app, "reopen", elapsed);
            }
            assert_eq!(presence.progress(), Fixed::ONE);
            assert!(launcher.blocked());
            assert!(!has_front(&app));
            let mut details = actions.clone();
            details.launcher.overlay = "游戏详情".into();
            details.launcher.overlay_kind = "details".into();
            details.launcher.details = Some(Default::default());
            details.launcher.items =
                vec![LauncherItem::available(MenuAction::ExpandDetail, "核心")];
            update(&mut scene, &details).unwrap();
            paint_at(&mut app, &scene, base + 1168);
            assert_eq!(presence.progress(), Fixed::ONE);
            let mut previous = actions.clone();
            previous.launcher.selected = 3;
            update(&mut scene, &previous).unwrap();
            paint_at(&mut app, &scene, base + 1232);
            assert_eq!(presence.progress(), Fixed::ONE);
            assert_eq!(
                scene.state.snapshot().launcher.selected_action(),
                Some(MenuAction::ShowGameDetails)
            );
            update(&mut scene, &games).unwrap();
            for elapsed in (16..=272).step_by(16) {
                paint_at(&mut app, &scene, base + 1232 + elapsed);
                record(&mut app, "close", elapsed);
                assert_cartridge_repaint(&mut app);
                assert_eq!(source_opacity(&app), original_opacity);
            }
            assert!(!presence.mounted());
            assert_eq!(face.origin(), None);
            assert_eq!(source_opacity(&app), original_opacity);
            assert_eq!(display.selected(), selection);
            assert_eq!(identities(), saved);
            let intents = Rc::new(std::cell::RefCell::new(Vec::new()));
            let received = intents.clone();
            launcher
                .on_effect(move |i: &howff_menu::input::Sourced<Intent>| {
                    received.borrow_mut().push(i.payload.clone());
                })
                .unwrap();
            launcher.press(base + 1504);
            launcher.release(base + 1584);
            paint_at(&mut app, &scene, base + 1600);
            launcher.press(base + 1600);
            paint_at(&mut app, &scene, base + 2336);
            launcher.release(base + 2336);
            assert_eq!(
                *intents.borrow(),
                [false, true].map(|multiplayer| Intent::Game {
                    key: games.launcher.key.clone(),
                    multiplayer
                })
            );
        }
    }
}

#[test]
fn changing_games_during_exit_replaces_the_pinned_title_and_artwork() {
    for kind in [LibraryKind::Favorites, LibraryKind::Directory] {
        for replacement in [false, true] {
            let mut app = app([480, 320]);
            let home = game_view(kind, true);
            seed_art(&app, &home.artwork);
            let mut scene = draw(&mut app, &home).unwrap();
            let face = app
                .world
                .resource::<state::DisplayRuntime>()
                .unwrap()
                .card_face
                .clone();
            paint_at(&mut app, &scene, 480);
            update(&mut scene, &open_actions(&home)).unwrap();
            paint_at(&mut app, &scene, 704);
            assert_eq!(face.title(), home.entries[1].name);
            assert_eq!(face.cover().token(), Some(71));
            update(&mut scene, &home).unwrap();
            paint_at(&mut app, &scene, 736);
            let mut next = home.clone();
            if replacement {
                next.entries.remove(1);
                next.library.total = next.entries.len();
            } else {
                next.selected = 0;
                next.library.row_selected = 0;
                next.library.position = 1;
            }
            let selected = &next.entries[next.selected];
            let next_title = selected.name.clone();
            next.launcher.key = selected.key.clone();
            next.artwork = selected.artwork.clone();
            for layer in [&mut next.launcher_shelf, &mut next.launcher_catalog]
                .into_iter()
                .flatten()
            {
                layer.entries = next.entries.clone();
                layer.selected = next.selected;
                layer.library.row_selected = next.library.row_selected;
                layer.library.position = next.library.position;
                layer.library.total = next.library.total;
                layer.artwork = next.artwork.clone();
            }
            update(&mut scene, &next).unwrap();
            paint_at(&mut app, &scene, 752);
            update(&mut scene, &open_actions(&next)).unwrap();
            paint_at(&mut app, &scene, 768);
            assert_eq!(
                face.title(),
                next_title,
                "{kind:?} replacement={replacement}"
            );
            assert_eq!(face.cover().token(), None);
        }
    }
}

#[test]
fn opening_and_closing_show_one_face_and_leave_the_source_slot_empty() {
    use super::material::CartridgeFront;
    use mirui::ui::OffscreenRender;
    for size in [[480, 320], [640, 480], [800, 480]] {
        for kind in [LibraryKind::Favorites, LibraryKind::Directory] {
            let home = game_view(kind, true);
            let actions = open_actions(&home);
            let mut app = app(size);
            seed_art(&app, &home.artwork);
            let mut scene = draw(&mut app, &home).unwrap();
            let presence = scene.state.launcher().popup();
            let face = app
                .world
                .resource::<state::DisplayRuntime>()
                .unwrap()
                .card_face
                .clone();
            let mut now = 480;
            paint_at(&mut app, &scene, now);
            for (phase, target, duration) in [("open", &actions, 320), ("close", &home, 200)] {
                update(&mut scene, target).unwrap();
                now += 16;
                let mut samples: Vec<_> = (0..duration).step_by(16).collect();
                samples.extend([duration / 2, duration]);
                samples.sort_unstable();
                samples.dedup();
                for elapsed in samples {
                    paint_at(&mut app, &scene, now + elapsed);
                    capture_text_audit(
                        &mut app,
                        &format!("turn-{kind:?}-{phase}-{elapsed:03}"),
                        size,
                    );
                    if !presence.mounted() {
                        assert_eq!((phase, elapsed), ("close", duration));
                        continue;
                    }
                    let front = app.world.query::<CartridgeFront>().iter().any(|(e, _)| {
                        visible_in_tree(&app.world, e)
                            && app
                                .world
                                .get::<OffscreenRender>(e)
                                .is_some_and(|r| r.opacity == 255)
                    });
                    let panel = app
                        .world
                        .query::<actions_damage::ActionsPanel>()
                        .iter()
                        .find(|(e, _)| visible_in_tree(&app.world, *e))
                        .unwrap()
                        .0;
                    let back = app.world.get::<mirui::ui::Parent>(panel).unwrap().0;
                    let back_alpha = app.world.get::<OffscreenRender>(back).unwrap().opacity;
                    let p = presence.progress();
                    if let (Some(origin), Some(dest)) = (face.origin(), face.panel()) {
                        let local = if front { origin } else { dest };
                        let pose = super::material::cartridge_pose(
                            p,
                            presence.card_travel(),
                            origin,
                            dest,
                            local,
                        );
                        let center_y = local.y + local.h / 2 + pose.ty;
                        let radius = (pose.m10.abs() * local.w + pose.m11.abs() * local.h) / 2;
                        assert!(
                            center_y - radius >= -Fixed::ONE
                                && center_y + radius
                                    <= Fixed::from_int(i32::from(size[1])) + Fixed::ONE,
                            "{kind:?} {size:?} {phase} {elapsed}: card clipped vertically"
                        );
                    }
                    assert_eq!(
                        (front, back_alpha),
                        if p < Fixed::HALF {
                            (true, 0)
                        } else {
                            (false, 255)
                        }
                    );
                    if elapsed != duration / 2 {
                        continue;
                    }
                    assert_eq!(p, Fixed::HALF);
                    assert!(
                        app.world
                            .query::<super::actor::ActorFrame>()
                            .iter()
                            .all(|(e, _)| !visible_in_tree(&app.world, e))
                    );
                    let origin = face.origin().unwrap();
                    let dest = face.panel().unwrap();
                    let pose = app
                        .world
                        .get::<mirui::ui::widgets::WidgetTransform>(back)
                        .unwrap()
                        .0;
                    let edge = dest.x + dest.w / 2 + pose.tx;
                    let half_width = (pose.m00.abs() * dest.w + pose.m01.abs() * dest.h) / 2;
                    let framebuffer = app.backend.framebuffer();
                    let frame = framebuffer.read_pixels().unwrap();
                    let mut color = None;
                    for y in
                        (origin.y + origin.h / 4).to_int()..(origin.y + origin.h * 3 / 4).to_int()
                    {
                        for x in (origin.x + origin.w / 4).to_int()
                            ..(origin.x + origin.w * 3 / 4).to_int()
                        {
                            if (x - edge.to_int()).abs() <= half_width.to_int() + 2 {
                                continue;
                            }
                            let offset = 4 * (y as usize * usize::from(size[0]) + x as usize);
                            let pixel = &frame.bytes()[offset..offset + 3];
                            assert_eq!(
                                *color.get_or_insert(pixel),
                                pixel,
                                "duplicate cover in {kind:?} {phase} slot"
                            );
                        }
                    }
                    assert!(color.is_some());
                }
                now += duration;
            }
        }
    }
}

#[test]
fn cartridge_turns_edge_on_and_both_faces_share_the_axis() {
    let origin = Rect::new(187, 88, 103, 75);
    let destination = Rect::new(16, 12, 448, 296);
    let pose = |progress, face| {
        super::material::cartridge_pose(progress, progress, origin, destination, face)
    };
    assert_eq!(pose(Fixed::ZERO, origin), mirui::types::Transform::IDENTITY);
    assert_eq!(
        pose(Fixed::ONE, destination),
        mirui::types::Transform::IDENTITY
    );
    let width = |p| pose(p, origin).m00 * origin.w;
    assert!(width(Fixed::from_ratio(3, 8)) > width(Fixed::HALF));
    assert!(width(Fixed::from_ratio(5, 8)) > width(Fixed::HALF));
    assert!((width(Fixed::HALF) - Fixed::from_int(3)).abs() < Fixed::ONE);
    for step in 0..=64 {
        let progress = Fixed::from_ratio(step, 64);
        let front = pose(progress, origin);
        let back = pose(progress, destination);
        assert!(front.m00 > Fixed::ZERO && back.m00 > Fixed::ZERO);
        let bounds = |local: Rect, transform: mirui::types::Transform| Rect {
            x: local.x + local.w / 2 + transform.tx,
            y: local.y + local.h / 2 + transform.ty,
            w: local.w * transform.m00,
            h: local.h * transform.m11,
        };
        let front = bounds(origin, front);
        let back = bounds(destination, back);
        for (a, b) in [
            (front.x, back.x),
            (front.y, back.y),
            (front.w, back.w),
            (front.h, back.h),
        ] {
            assert!((a - b).abs() < Fixed::from_int(2), "{step}: {a:?} vs {b:?}");
        }
    }
}
