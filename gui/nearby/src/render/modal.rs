use super::{deck::Layer, state::DisplayRuntime};
use mirui::app::RendererFactory;
use mirui::core::resource::{ResourceHandle, ResourceManager};
use mirui::prelude::*;
use mirui::render::texture::Texture;
use mirui::surface::Surface;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

const PROFILE_LIMIT: u32 = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Phase {
    #[default]
    Live,
    Capturing,
    Ready,
    Suspended,
    Restoring,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Live,
    Pause,
    Freeze,
}

#[model]
#[derive(Default)]
pub(super) struct Modal {
    #[observe]
    pub hidden: bool,
    #[observe]
    pub paused: bool,
    #[observe]
    pub token: Option<u64>,
    #[observe]
    pub phase: Phase,
}

#[model]
impl Modal {
    fn publish(&mut self, phase: Phase, hidden: bool, paused: bool, token: Option<u64>) {
        self.phase = phase;
        self.hidden = hidden;
        self.paused = paused;
        self.token = token;
    }
}

#[component]
pub(super) struct BackgroundRoot;

#[derive(Default)]
pub(super) struct AnimationClock {
    paused_at: Option<u64>,
    offset: u64,
    last: Option<u64>,
}

impl AnimationClock {
    pub(super) fn sample(&mut self, now: u64, paused: bool) -> Option<u64> {
        if paused {
            self.paused_at.get_or_insert(self.last.unwrap_or(now));
            return None;
        }
        if let Some(start) = self.paused_at.take() {
            self.offset = self.offset.saturating_add(now.saturating_sub(start));
        }
        self.last = Some(now);
        Some(self.local(now))
    }

    pub(super) fn local(&self, now: u64) -> u64 {
        self.paused_at.unwrap_or(now).saturating_sub(self.offset)
    }
}

#[derive(Default)]
pub(super) struct AnimationGate {
    clock: RefCell<AnimationClock>,
    counts: Cell<(u64, u64)>,
}

impl AnimationGate {
    pub(super) fn sample(&self, now: u64, paused: bool) -> Option<u64> {
        let sampled = self.clock.borrow_mut().sample(now, paused);
        let (ticks, skips) = self.counts.get();
        self.counts.set(if sampled.is_some() {
            (ticks + 1, skips)
        } else {
            (ticks, skips + 1)
        });
        sampled
    }

    pub(super) fn local(&self, now: u64) -> u64 {
        self.clock.borrow().local(now)
    }

    pub(super) fn counts(&self) -> (u64, u64) {
        self.counts.get()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    theme: String,
    physical: (u16, u16),
    logical: (u16, u16),
}

pub(super) struct Runtime {
    pub model: ModalHandle,
    pub gates: [Rc<AnimationGate>; 2],
    controller: Rc<RefCell<Controller>>,
}

struct Controller {
    model: ModalHandle,
    mode: Mode,
    requested: bool,
    attempted: bool,
    generation: u64,
    revision: u64,
    stamp: Option<Stamp>,
    capture_generation: u64,
    capture_calls: u64,
    captures: u64,
    held: Option<Held>,
    pending: Option<Pending>,
    retired: Vec<String>,
    profile: bool,
    profile_events: u32,
    frames: u64,
    tick_started: Option<Instant>,
    opening_tick: bool,
    committing: bool,
    normal_tick_ms: Option<f64>,
    snapshot_ms: Option<f64>,
    admit_ms: Option<f64>,
    commit_ms: Option<f64>,
    paused: bool,
}

struct Held {
    revision: u64,
    token: String,
    _handle: ResourceHandle<Texture<'static>>,
}

struct Pending {
    image: Held,
    generation: u64,
    stamp: Stamp,
}

pub(super) fn install<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    let model = app.add_model(Modal::default()).finish();
    let mode = match std::env::var("NEARBY_MODAL_PATCH").as_deref() {
        Ok("live") => Mode::Live,
        Ok("pause") => Mode::Pause,
        _ => Mode::Freeze,
    };
    let controller = Controller {
        model: model.clone(),
        mode,
        requested: false,
        attempted: false,
        generation: 0,
        revision: 0,
        stamp: None,
        capture_generation: 0,
        capture_calls: 0,
        captures: 0,
        held: None,
        pending: None,
        retired: Vec::new(),
        profile: std::env::var("NEARBY_MODAL_PROFILE").as_deref() == Ok("1"),
        profile_events: 0,
        frames: 0,
        tick_started: None,
        opening_tick: false,
        committing: false,
        normal_tick_ms: None,
        snapshot_ms: None,
        admit_ms: None,
        commit_ms: None,
        paused: false,
    };
    app.add_resource(Runtime {
        model,
        gates: std::array::from_fn(|_| Rc::new(AnimationGate::default())),
        controller: Rc::new(RefCell::new(controller)),
    });
}

fn background<B: Surface, F: RendererFactory<B>>(app: &App<B, F>) -> Option<Entity> {
    app.world
        .query::<BackgroundRoot>()
        .iter()
        .next()
        .map(|(id, _)| id)
}

fn requested<B: Surface, F: RendererFactory<B>>(app: &App<B, F>) -> bool {
    app.world
        .resource::<DisplayRuntime>()
        .is_some_and(|runtime| {
            runtime.deck.enabled() && matches!(runtime.deck.layer(), Layer::Room | Layer::Actions)
        })
}

fn stamp<B: Surface, F: RendererFactory<B>>(app: &App<B, F>) -> Stamp {
    let viewport = app.backend.viewport();
    Stamp {
        theme: app.theme().id().as_str().to_owned(),
        physical: viewport.physical_size(),
        logical: viewport.logical_size(),
    }
}

impl Controller {
    fn sync_background<B: Surface, F: RendererFactory<B>>(&mut self, app: &App<B, F>) {
        let paused = self.model.paused();
        if paused != self.paused
            && let Some(display) = app.world.resource::<DisplayRuntime>()
        {
            display.deck.pause_background(paused);
            self.paused = paused;
        }
    }

    fn publish(&self, phase: Phase, hidden: bool, paused: bool) {
        self.model.publish(
            phase,
            hidden,
            paused,
            self.held.as_ref().map(|image| image.revision),
        );
    }

    fn log<B: Surface, F: RendererFactory<B>>(
        &mut self,
        app: &App<B, F>,
        event: &str,
        capture_ms: Option<f64>,
    ) {
        if !self.profile || self.profile_events >= PROFILE_LIMIT {
            return;
        }
        self.profile_events += 1;
        let counters = app.world.resource::<DisplayRuntime>().map(|display| {
            [
                display.shelf.animation_counts(),
                display.catalog.animation_counts(),
            ]
        });
        println!(
            "{}",
            serde_json::json!({
                "modal_profile": true,
                "event": event,
                "mode": format!("{:?}", self.mode),
                "phase": format!("{:?}", self.model.phase()),
                "generation": self.generation,
                "capture_calls": self.capture_calls,
                "captures": self.captures,
                "capture_ms": capture_ms,
                "snapshot_ms": self.snapshot_ms,
                "admit_ms": self.admit_ms,
                "commit_ms": self.commit_ms,
                "normal_tick_ms": self.normal_tick_ms,
                "cache_revision": self.revision,
                "hidden": self.model.hidden(),
                "paused": self.model.paused(),
                "background_counts": counters,
            })
        );
    }

    fn release<B: Surface, F: RendererFactory<B>>(&mut self, app: &App<B, F>) {
        if let Some(image) = self.held.take() {
            self.retired.push(image.token);
        }
        if let Some(textures) = app.world.resource::<ResourceManager<Texture<'static>>>() {
            for token in self.retired.drain(..) {
                textures.remove_token(&token);
            }
        }
    }

    fn before<B: Surface, F: RendererFactory<B>>(&mut self, app: &mut App<B, F>) {
        self.tick_started = Some(Instant::now());
        self.normal_tick_ms = None;
        self.commit_ms = None;
        let Some(root) = background(app) else { return };
        let wanted = requested(app);
        let current = stamp(app);
        if wanted != self.requested {
            self.requested = wanted;
            self.generation = self.generation.wrapping_add(1);
            self.attempted = false;
            self.opening_tick = wanted;
            self.log(app, if wanted { "open" } else { "close" }, None);
        }
        if self.mode == Mode::Live {
            return;
        }
        if let Some(pending) = self.pending.take() {
            if wanted && pending.generation == self.generation && pending.stamp == current {
                if let Some(previous) = self.held.replace(pending.image) {
                    self.retired.push(previous.token);
                }
                self.publish(Phase::Suspended, true, true);
                self.committing = true;
            } else {
                self.retired.push(pending.image.token);
                self.attempted = false;
                self.log(app, "capture_discarded", None);
            }
        }
        if self.mode == Mode::Pause {
            self.publish(
                if wanted {
                    Phase::Suspended
                } else {
                    Phase::Live
                },
                false,
                wanted,
            );
        } else if !wanted {
            if self.model.phase() == Phase::Capturing {
                app.world.remove::<mirui::ui::OffscreenRender>(root);
            }
            if self.held.is_some() {
                self.publish(Phase::Restoring, false, false);
                app.world.mark_subtree_dirty(root);
            } else {
                self.publish(Phase::Live, false, false);
            }
        } else if self.held.is_some() && self.stamp.as_ref() != Some(&current) {
            self.capture_generation = self.generation;
            self.stamp = Some(current);
            self.publish(Phase::Capturing, false, true);
            app.world
                .insert(root, mirui::ui::OffscreenRender::default());
            app.world.mark_subtree_dirty(root);
            self.log(app, "recapture", None);
        } else if !self.attempted {
            self.attempted = true;
            self.capture_generation = self.generation;
            self.stamp = Some(current);
            self.publish(Phase::Capturing, false, false);
            app.world
                .insert(root, mirui::ui::OffscreenRender::default());
            app.world.mark_subtree_dirty(root);
        }
        self.sync_background(app);
    }

    fn after<B: Surface, F: RendererFactory<B>>(
        &mut self,
        app: &mut App<B, F>,
    ) -> Result<(), String> {
        self.normal_tick_ms = self
            .tick_started
            .take()
            .map(|start| start.elapsed().as_secs_f64() * 1000.0);
        if self.opening_tick {
            self.opening_tick = false;
            self.log(app, "opening_tick", None);
        }
        if self.committing {
            self.committing = false;
            self.commit_ms = self.normal_tick_ms;
            self.log(app, "capture_committed", None);
        }
        let Some(root) = background(app) else {
            return Ok(());
        };
        if self.mode == Mode::Freeze && self.model.phase() == Phase::Capturing {
            self.capture_calls += 1;
            let start = Instant::now();
            let captured = app.snapshot_widget(root);
            self.snapshot_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
            app.world.remove::<mirui::ui::OffscreenRender>(root);
            let texture = match captured {
                Ok(Some(texture))
                    if requested(app)
                        && self.capture_generation == self.generation
                        && texture.width() > 0
                        && texture.height() > 0 =>
                {
                    Some(texture)
                }
                _ => None,
            };
            let admit = Instant::now();
            let event = if let Some(texture) = texture {
                self.revision += 1;
                let dimensions = (texture.width(), texture.height());
                let texture = texture.with_cache_revision(self.revision);
                let token = texture_token(self.revision);
                let textures = app
                    .world
                    .resource::<ResourceManager<Texture<'static>>>()
                    .ok_or("Modal texture manager is missing")?;
                textures.add_factory(token.clone(), move || Some(texture));
                let handle = textures.load(token.clone());
                let admitted = handle.get();
                if (admitted.width(), admitted.height()) == dimensions {
                    self.pending = Some(Pending {
                        image: Held {
                            revision: self.revision,
                            token,
                            _handle: handle,
                        },
                        generation: self.capture_generation,
                        stamp: stamp(app),
                    });
                    self.captures += 1;
                    self.publish(Phase::Ready, self.model.hidden(), true);
                    "capture_ready"
                } else {
                    textures.remove_token(&token);
                    self.release(app);
                    self.publish(Phase::Live, false, false);
                    "capture_rejected"
                }
            } else {
                self.release(app);
                self.publish(Phase::Live, false, false);
                "capture_failed"
            };
            self.admit_ms = Some(admit.elapsed().as_secs_f64() * 1000.0);
            self.log(app, event, Some(start.elapsed().as_secs_f64() * 1000.0));
        } else if self.model.phase() == Phase::Restoring {
            // Validate restored geometry before retiring the covering image.
            app.dirty_regions()
                .map_err(|error| format!("Modal restore: {error:?}"))?;
            self.release(app);
            self.publish(Phase::Live, false, false);
            app.render_dirty()
                .map_err(|error| format!("Modal restore commit: {error:?}"))?;
            self.log(app, "restored", None);
        } else if let Some(textures) = app.world.resource::<ResourceManager<Texture<'static>>>() {
            for token in self.retired.drain(..) {
                textures.remove_token(&token);
            }
        }
        self.sync_background(app);
        self.frames += 1;
        if self.frames.is_multiple_of(120) {
            self.log(app, "sample", None);
        }
        Ok(())
    }
}

pub(super) fn texture_token(revision: u64) -> String {
    format!("nearby-modal-{revision}")
}

pub(super) fn before_tick<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    let controller = app
        .world
        .resource::<Runtime>()
        .map(|runtime| runtime.controller.clone());
    if let Some(controller) = controller {
        controller.borrow_mut().before(app);
    }
}

pub(super) fn after_tick<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
) -> Result<(), String> {
    let controller = app
        .world
        .resource::<Runtime>()
        .map(|runtime| runtime.controller.clone());
    controller.map_or(Ok(()), |controller| controller.borrow_mut().after(app))
}
