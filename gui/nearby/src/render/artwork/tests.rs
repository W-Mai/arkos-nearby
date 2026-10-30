use super::*;
use crate::model::{GameRow, LibraryKind, Page, View};
use mirui::core::resource::{FormatMatch, LoadContext, LoadError, LoadInput, Loader};
use mirui::surface::framebuf::FramebufSurface;
use mirui::ui::widgets::image::ImageSource;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::task::{Poll, Waker};

type Queued = (String, Rc<RefCell<Pending>>);

#[derive(Default)]
struct Pending {
    output: Option<Result<Texture<'static>, LoadError>>,
    wake: Option<Waker>,
}

#[derive(Clone, Default)]
struct Control {
    pending: Rc<RefCell<Vec<Queued>>>,
    count: Rc<Cell<usize>>,
    revision: Rc<Cell<u64>>,
}

struct Deferred(Control);

impl Loader<Texture<'static>> for Deferred {
    type State = Option<Rc<RefCell<Pending>>>;

    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        match input {
            LoadInput::Token(token) if token.starts_with("nearby-artwork:") => FormatMatch::Match,
            _ => FormatMatch::NoMatch,
        }
    }

    fn poll_load(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<Texture<'static>, LoadError>> {
        let pending = state.get_or_insert_with(|| {
            self.0.count.set(self.0.count.get() + 1);
            let pending = Rc::new(RefCell::new(Pending::default()));
            self.0
                .pending
                .borrow_mut()
                .push((cx.token().to_owned(), pending.clone()));
            pending
        });
        let mut pending = pending.borrow_mut();
        pending.wake = Some(cx.waker().clone());
        pending.output.take().map_or(Poll::Pending, Poll::Ready)
    }
}

impl Control {
    fn finish(&self, request: &Rc<RefCell<Pending>>, failed: bool) {
        let value = if failed {
            Err(LoadError::Failed("test image failure"))
        } else {
            self.revision.set(self.revision.get() + 1);
            Ok(
                Texture::from_vec([201, 3, 223, 255].repeat(16), 4, 4, ColorFormat::RGBA8888)
                    .unwrap()
                    .with_cache_revision(self.revision.get()),
            )
        };
        let wake = {
            let mut state = request.borrow_mut();
            state.output = Some(value);
            state.wake.take()
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }

    fn finish_all(&self) {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        for (_, request) in pending {
            self.finish(&request, false);
        }
    }
}

type TestApp = App<FramebufSurface<fn(&[u8], mirui::types::PhysicalRect)>>;

fn app() -> (TestApp, Control) {
    let mut app = App::new(FramebufSurface::with_format(
        640,
        480,
        ColorFormat::RGBA8888,
        (|_, _| {}) as fn(&[u8], mirui::types::PhysicalRect),
    ));
    app.with_default_widgets().with_default_systems();
    crate::render::install(&mut app).unwrap();
    let control = Control::default();
    let images = ResourceManager::new(
        MaxSize::Bytes(TEXTURE_BUDGET),
        Texture::owned(1, 1, ColorFormat::RGBA8888).unwrap(),
    )
    .with_probes(
        MaxSize::Count(32),
        TextureMeta {
            width: 1,
            height: 1,
            format: ColorFormat::RGBA8888,
        },
    );
    images.add_loader(Deferred(control.clone()));
    app.add_resource(images);
    (app, control)
}

fn home() -> View {
    let mut view = View {
        page: Page::Games,
        entries: (0..6)
            .map(|i| GameRow {
                key: format!("game-{i}"),
                name: format!("Game {i}"),
                artwork: format!("cover-{i}"),
                icon: "nes".into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    view.library.kind = LibraryKind::Favorites;
    view.launcher.enabled = true;
    view
}

fn pump(app: &mut TestApp, control: &Control, begin: u64) {
    for frame in 0..12 {
        crate::render::paint_at(app, begin + frame * 17).unwrap();
        control.finish_all();
    }
    crate::render::paint_at(app, begin + 12 * 17).unwrap();
}

#[test]
fn visible_images_share_framework_values_and_survive_reorder_and_page_return() {
    let (mut app, control) = app();
    let mut view = home();
    let mut scene = crate::render::draw(&mut app, &view).unwrap();
    pump(&mut app, &control, 17);
    let tokens = |display: &super::super::state::DisplayHandle| {
        display
            .entries()
            .entries()
            .map(|(_, r)| {
                (
                    r.artwork(),
                    r.artwork_token().unwrap(),
                    r.cover_bounds().source(),
                )
            })
            .collect::<Vec<_>>()
    };
    let original = tokens(&scene.shelf);
    assert_eq!(original.len(), 6);
    assert_eq!(
        control.count.get(),
        6,
        "each visible image decoded once without debounce"
    );
    let image = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap()
        .resolve(&original[0].2);
    view.selected = 1;
    view.entries.swap(0, 2);
    crate::render::update(&mut scene, &view).unwrap();
    pump(&mut app, &control, 300);
    let mut reordered = original.clone();
    reordered.swap(0, 2);
    assert_eq!(tokens(&scene.shelf), reordered);
    let shelf = view.clone();
    view.launcher_shelf = Some(Box::new(shelf.clone()));
    view.launcher.tab = 1;
    view.library.kind = LibraryKind::Directory;
    view.entries.clear();
    crate::render::update(&mut scene, &view).unwrap();
    pump(&mut app, &control, 600);
    crate::render::update(&mut scene, &shelf).unwrap();
    pump(&mut app, &control, 900);
    assert_eq!(control.count.get(), 6);
    assert!(Rc::ptr_eq(
        &image,
        &app.world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap()
            .resolve(&original[0].2)
    ));
    assert_eq!(
        app.world.resource::<ArtworkPlan>().unwrap().requests.len(),
        6
    );
}

#[test]
fn debounce_prefetch_and_late_results_keep_current_selection() {
    let (mut app, control) = app();
    let mut view = View {
        page: Page::Games,
        artwork: "first".into(),
        artwork_neighbors: vec!["neighbor".into()],
        ..Default::default()
    };
    let mut scene = crate::render::draw(&mut app, &view).unwrap();
    crate::render::paint_at(&mut app, 17).unwrap();
    assert!(control.pending.borrow().is_empty());
    app.world.resource_mut::<ArtworkPlan>().unwrap().changed -= DEBOUNCE;
    crate::render::paint_at(&mut app, 34).unwrap();
    let first = control.pending.borrow_mut().pop().unwrap();
    assert!(first.0.contains("first"));
    view.artwork = "second".into();
    crate::render::update(&mut scene, &view).unwrap();
    crate::render::paint_at(&mut app, 51).unwrap();
    assert!(scene.state.cover_bounds().token().is_none());
    control.finish(&first.1, false);
    crate::render::paint_at(&mut app, 68).unwrap();
    assert!(scene.state.cover_bounds().token().is_none());
    app.world.resource_mut::<ArtworkPlan>().unwrap().changed -= DEBOUNCE;
    pump(&mut app, &control, 85);
    assert!(scene.state.cover_bounds().token().is_some());
    assert!(scene.state.cover_bounds().source().contains("second"));
    assert!(
        app.world
            .resource::<ArtworkPlan>()
            .unwrap()
            .requests
            .iter()
            .any(|entry| entry.key.0 == "neighbor" && entry.ready.get())
    );
    let before = control.count.get();
    view.artwork = "neighbor".into();
    view.artwork_neighbors.clear();
    crate::render::update(&mut scene, &view).unwrap();
    crate::render::paint_at(&mut app, 400).unwrap();
    assert!(scene.state.cover_bounds().token().is_some());
    assert_eq!(
        control.count.get(),
        before,
        "prefetched selection is immediately resident"
    );
}

#[test]
fn image_property_retains_entity_and_reacts_to_resource_replacement() {
    let (mut app, control) = app();
    let mut view = View {
        page: Page::Games,
        artwork: "first".into(),
        ..Default::default()
    };
    let mut scene = crate::render::draw(&mut app, &view).unwrap();
    crate::render::paint_at(&mut app, 1).unwrap();
    app.world.resource_mut::<ArtworkPlan>().unwrap().changed -= DEBOUNCE;
    pump(&mut app, &control, 17);
    assert!(scene.state.cover_bounds().token().is_some());
    let source = scene.state.cover_bounds().source();
    let entity = app
        .world
        .query::<Image>()
        .iter()
        .find(
            |(_, image)| matches!(&image.src,ImageSource::Texture(token) if token.as_ref()==source),
        )
        .unwrap()
        .0;
    let first = scene.state.cover_bounds().token();
    app.world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap()
        .remove_token(&source);
    pump(&mut app, &control, 300);
    assert!(scene.state.cover_bounds().token().is_some());
    assert_ne!(first, scene.state.cover_bounds().token());
    view.artwork = "second".into();
    crate::render::update(&mut scene, &view).unwrap();
    crate::render::paint_at(&mut app, 580).unwrap();
    app.world.resource_mut::<ArtworkPlan>().unwrap().changed -= DEBOUNCE;
    pump(&mut app, &control, 600);
    let next = scene.state.cover_bounds().source();
    assert!(
        matches!(&app.world.get::<Image>(entity).unwrap().src,ImageSource::Texture(token) if token.as_ref()==next)
    );
}

#[test]
fn failures_release_admission_and_visible_sources_remain_bounded() {
    let (mut app, control) = app();
    let mut scene = crate::render::draw(&mut app, &home()).unwrap();
    for frame in 0..10 {
        crate::render::paint_at(&mut app, 17 + frame * 17).unwrap();
        let pending = std::mem::take(&mut *control.pending.borrow_mut());
        for (_, request) in pending {
            control.finish(&request, true);
        }
    }
    assert_eq!(control.count.get(), 6);
    assert_eq!(app.world.resource::<ArtworkPlan>().unwrap().loading(), 0);
    assert!(
        scene
            .shelf
            .entries()
            .entries()
            .all(|(_, row)| row.artwork_token().is_none())
    );
    let mut next = home();
    next.entries.clear();
    crate::render::update(&mut scene, &next).unwrap();
    crate::render::paint_at(&mut app, 300).unwrap();
    assert_eq!(
        app.world.resource::<ArtworkPlan>().unwrap().requests.len(),
        0
    );
}

#[test]
fn resource_completion_replaces_a_frozen_cartridge_and_keeps_cover_crop() {
    let (mut app, control) = app();
    let mut view = home();
    for row in &mut view.entries {
        row.artwork = "late.png".into();
    }
    view.launcher_shelf = Some(Box::new(view.clone()));
    let scene = crate::render::draw(&mut app, &view).unwrap();
    for frame in 0..100 {
        crate::render::paint_at(&mut app, frame * 17).unwrap();
    }
    let row = scene.shelf.entries().entries().next().unwrap().1;
    let picture = row.static_picture();
    let first = picture.token().unwrap();
    assert!(picture.hidden());
    pump(&mut app, &control, 1700);
    assert!(row.artwork_token().is_some());
    assert_ne!(picture.token(), Some(first));
    assert!(picture.hidden());
    let bounds = row.cover_bounds();
    let size = bounds.size();
    let expected = size.width.max(size.height * bounds.aspect());
    assert!(expected >= size.width && expected / bounds.aspect() >= size.height);
}

#[test]
fn resource_publication_reaches_bound_models_in_the_same_tick() {
    let (mut app, control) = app();
    let scene = crate::render::draw(&mut app, &home()).unwrap();
    crate::render::paint_at(&mut app, 17).unwrap();
    control.finish_all();
    crate::render::paint_at(&mut app, 34).unwrap();
    let row = scene.shelf.entries().entries().next().unwrap().1;
    assert!(row.artwork_token().is_some());
}

#[test]
fn completion_after_row_retirement_does_not_touch_expired_models() {
    let (mut app, control) = app();
    let mut view = home();
    let mut scene = crate::render::draw(&mut app, &view).unwrap();
    crate::render::paint_at(&mut app, 17).unwrap();
    view.entries.clear();
    crate::render::update(&mut scene, &view).unwrap();
    control.finish_all();
    crate::render::paint_at(&mut app, 34).unwrap();
    assert_eq!(
        app.world.resource::<ArtworkPlan>().unwrap().requests.len(),
        0
    );
}
