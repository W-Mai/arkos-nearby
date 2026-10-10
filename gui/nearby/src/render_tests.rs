use super::*;
use crate::model::{GameRow, RoomRow};
use crate::motion::Motion;
use mirui::core::model::ModelHandle;
use mirui::render::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::PhysicalRect;
use mirui::ui::OffscreenGeneration;
use mirui::ui::render_system::LastDirtyRegions;
use std::cell::Cell;

type TestApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn app(size: [u16; 2]) -> TestApp {
    let mut app = App::headless(size[0], size[1]);
    app.with_default_widgets().with_default_systems();
    install_fonts(&mut app).unwrap();
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
                name: format!("房间 {i}"),
                detail: "同一游戏，可以加入".into(),
                signal: 70,
                available: true,
            })
            .collect(),
        entries: (0..4)
            .map(|i| GameRow {
                name: format!("游戏 {i} / Multiplayer"),
                detail: "NES / FC".into(),
                directory: false,
            })
            .collect(),
        status: "请选择".into(),
        ..View::default()
    }
}

fn rect(app: &TestApp, id: &'static str) -> Rect {
    let entity = app.world.resource::<IdMap>().unwrap().get(id).unwrap();
    app.world.get::<ComputedRect>(entity).unwrap().0
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
    let incremental = app.backend.framebuffer().buf.as_slice().to_vec();
    app.render().unwrap();
    let full_frame = app.backend.framebuffer();
    let full = full_frame.buf.as_slice();
    assert_eq!(
        incremental.iter().zip(full).filter(|(a, b)| a != b).count(),
        0,
        "{context}"
    );
}

#[test]
fn dynamic_updates_preserve_static_caches_and_one_model_registration() {
    for page in [
        Page::Choice,
        Page::Rooms,
        Page::Games,
        Page::Host,
        Page::Joined,
        Page::Leaving,
        Page::Unavailable,
    ] {
        let mut app = app([640, 480]);
        let initial = view(page.clone());
        let mut scene = draw(&mut app, &initial).unwrap();
        let registered = scene.state.access().clone();
        let generations = cache_generations(&app);
        assert!(!generations.is_empty());
        let mut next = initial.clone();
        next.status = "正在准备".into();
        assert!(can_update(&initial, &next));
        update(&mut app, &mut scene, &next).unwrap();
        let mut motion = Motion::default();
        motion.retarget(&page, scene.focus_rect, 0);
        motion.paint(&mut app, &scene, 70).unwrap();
        assert_eq!(cache_generations(&app), generations, "{page:?}");
        let previous_root = app.root.unwrap();
        let next_scene = draw(&mut app, &view(Page::Choice)).unwrap();
        assert!(!app.world.is_alive(previous_root));
        assert!(std::rc::Weak::ptr_eq(
            &registered,
            next_scene.state.access()
        ));
        drop(scene);
        drop(next_scene);
        drop(app);
        assert!(
            registered.upgrade().is_none(),
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
    let status_entity = app
        .world
        .resource::<IdMap>()
        .unwrap()
        .get("nearby_status")
        .unwrap();
    let mut next = initial.clone();
    next.status = "准备完成".into();
    update(&mut app, &mut scene, &next).unwrap();
    assert_eq!(
        app.world
            .get::<Text>(status_entity)
            .unwrap()
            .resolve(&app.world)
            .as_ref(),
        "准备完成"
    );
    app.clear_root();
    assert!(texts.iter().all(|e| !app.world.is_alive(*e)));
    scene.state.present(&initial);
    app.prepare_layout().unwrap();
    assert!(app.world.query::<Text>().iter().next().is_none());
}

#[test]
fn responsive_layout_keeps_focus_and_text_sections_inside_the_viewport() {
    for size in [[480, 320], [640, 480], [800, 480]] {
        for page in [
            Page::Choice,
            Page::Rooms,
            Page::Games,
            Page::Host,
            Page::Joined,
            Page::Leaving,
            Page::Unavailable,
        ] {
            let mut app = app(size);
            let mut view = view(page.clone());
            view.ready = true;
            view.selected = 3;
            view.status = "正在核对两台设备的游戏和模拟核心，请稍候。Preparing the selected game and checking the connection before starting multiplayer.".into();
            view.title = "较长的游戏标题 / A long multiplayer game title that should fit its available space".into();
            let scene = draw(&mut app, &view).unwrap();
            let header = rect(&app, "nearby_header");
            let body = rect(&app, "nearby_content");
            let footer = rect(&app, "nearby_footer");
            let status = rect(&app, "nearby_status");
            assert!(header.y + header.h <= body.y, "{size:?} {page:?}");
            assert!(body.y + body.h <= footer.y, "{size:?} {page:?}");
            assert!(status.y + status.h <= body.y + body.h, "{size:?} {page:?}");
            assert!(footer.y + footer.h <= Fixed::from_int(i32::from(size[1])));
            for (entity, _) in app.world.query::<Text>().iter() {
                let text = app.world.get::<ComputedRect>(entity).unwrap().0;
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
            if let Some([x, y, w, h]) = scene.focus_rect {
                assert!(w > 40 && h >= 26, "{size:?} {page:?}: {w}×{h}");
                assert!(
                    x >= 0 && y >= 0 && x + w <= i32::from(size[0]) && y + h <= footer.y.to_int()
                );
            }
        }
    }
}

#[test]
fn reflow_and_reverse_retarget_match_complete_frames_at_every_viewport() {
    for size in [[480, 320], [640, 480], [800, 480]] {
        for page in [Page::Choice, Page::Rooms, Page::Games] {
            let mut app = app(size);
            let mut view = view(page.clone());
            let mut scene = draw(&mut app, &view).unwrap();
            let original = scene.focus_rect.unwrap();
            let mut motion = Motion::default();
            motion.retarget(&page, scene.focus_rect, 0);
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
                update(&mut app, &mut scene, &view).unwrap();
                if time == 10 {
                    assert_ne!(
                        scene.focus_rect.unwrap()[1],
                        original[1],
                        "Text must reflow before retarget"
                    );
                }
                motion.retarget(&page, scene.focus_rect, time);
                motion.paint(&mut app, &scene, time + 16).unwrap();
                assert_full_frame_matches(&mut app, &format!("{size:?} {page:?} at {time}"));
            }
            motion.paint(&mut app, &scene, 700).unwrap();
            assert_eq!(
                app.world.resource::<FocusGeometry>().unwrap().0,
                scene.focus_rect
            );
            assert!(!motion.active(&scene, 700));
        }
    }
}

#[test]
fn semantic_phase_transitions_match_complete_frames() {
    for page in [Page::Host, Page::Joined, Page::Leaving] {
        let mut app = app([640, 480]);
        let mut previous = view(page.clone());
        let mut scene = draw(&mut app, &previous).unwrap();
        let mut motion = Motion::default();
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
            if can_update(&previous, &next) {
                update(&mut app, &mut scene, &next).unwrap();
            } else {
                scene = build(&mut app, &next).unwrap();
            }
            motion.retarget(&page, scene.focus_rect, i as u64 * 300);
            motion
                .paint(&mut app, &scene, i as u64 * 300 + 250)
                .unwrap();
            assert_full_frame_matches(&mut app, &format!("{page:?} {phase:?}"));
            previous = next;
        }
    }
}

#[test]
fn focus_damage_stays_disjoint_and_avoids_unchanged_interiors() {
    for page in [Page::Choice, Page::Rooms] {
        let mut app = app([640, 480]);
        let scene = draw(&mut app, &view(page.clone())).unwrap();
        let mut target = scene.focus_rect.unwrap();
        target[usize::from(page == Page::Rooms)] += 1;
        move_focus(&mut app, Some(target));
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
    install_fonts(&mut app).unwrap();
    let view = view(Page::Choice);
    let mut scene = draw(&mut app, &view).unwrap();
    let mut motion = Motion::default();
    motion.retarget(&view.page, scene.focus_rect, 0);
    motion.paint(&mut app, &scene, 300).unwrap();
    pixels.set(0);
    update(&mut app, &mut scene, &view).unwrap();
    motion.paint(&mut app, &scene, 600).unwrap();
    assert!(!motion.active(&scene, 600));
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
    assert!(
        !can_update(&approval, &view),
        "Readiness changes cached avatar colors"
    );
    view.busy = true;
    assert!(!host_action_enabled(&view));
}
