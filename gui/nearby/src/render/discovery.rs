use super::{state::DisplayState, style::*, widgets::paragraph};
use mirui::anim::Spring;
use mirui::ecs::{DeltaTimeMs, World};
use mirui::prelude::*;
use mirui::render::path::{PathCmd, PathStore};
use mirui::text::TextPath;
use mirui::ui::property::{Property, PropertyChange, prop};
use mirui::ui::widgets::{Text, TextAlign::Center};
use mirui::ui::{Children, ComputedRect, OffscreenRender, Parent};
use std::{cell::RefCell, rc::Rc};

const LETTERS: [&str; 4] = ["附", "近", "房", "间"];
const COUNT: usize = LETTERS.len();
const EM: i32 = TITLE_SIZE as i32;
const GLYPH_WINDOW: i32 = EM + 2;
const BEAT_MS: u16 = 23;
pub(super) const HEADER_HEIGHT: i32 = 36;

#[component]
#[derive(Clone, Copy)]
pub(super) enum Layer {
    Beacon,
    Results,
}

#[component]
#[derive(Clone, Copy)]
pub(super) struct DiscoveryGlyph(pub(super) usize);

#[derive(Default, Clone)]
struct RetiredPaths(Rc<RefCell<Vec<PathId>>>);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Destination {
    Beacon,
    Offline,
    Online,
}

impl Destination {
    fn from_display(state: &super::state::DisplayHandle) -> Self {
        if state.has_online_room() {
            Self::Online
        } else if state.rooms().is_empty() {
            Self::Beacon
        } else {
            Self::Offline
        }
    }

    fn coordinates(self) -> (Fixed, Fixed) {
        match self {
            Self::Beacon => (Fixed::ZERO, Fixed::ZERO),
            Self::Offline => (Fixed::ONE, Fixed::ZERO),
            Self::Online => (Fixed::ONE, Fixed::ONE),
        }
    }
}

struct Knot {
    dock: Spring,
    online: Spring,
    delay: u16,
}

impl Knot {
    fn new(at: Destination) -> Self {
        let (dock, online) = at.coordinates();
        Self {
            dock: Spring::new(dock, dock, 515, Fixed::from_ratio(1, 9)),
            online: Spring::new(online, online, 555, Fixed::from_ratio(1, 9)),
            delay: 0,
        }
    }

    fn advance(&mut self, destination: Destination, dt: u16) {
        // A reversal keeps each letter's current velocity, including while
        // the change in direction travels along the word.
        let hold = dt.min(self.delay);
        self.delay -= hold;
        let (dock, online) = destination.coordinates();
        for (spring, target) in [(&mut self.dock, dock), (&mut self.online, online)] {
            if hold != 0 && !spring.is_settled() {
                spring.tick(hold);
            }
            if self.delay == 0 {
                spring.retarget(target, None);
                if dt > hold && !spring.is_settled() {
                    spring.tick(dt - hold);
                }
            }
        }
    }
}

#[component]
struct DiscoveryCaption {
    state: model!(DisplayState),
    paths: [PathId; COUNT],
    retired: RetiredPaths,
    inset: Fixed,
    destination: Destination,
    knots: [Knot; COUNT],
    settled_size: Option<(Fixed, Fixed)>,
    last_centers: Option<[Point; COUNT]>,
    last_opacities: Option<[u8; 2]>,
}

impl Drop for DiscoveryCaption {
    fn drop(&mut self) {
        self.retired.0.borrow_mut().extend(self.paths);
    }
}

fn mix(a: Fixed, b: Fixed, t: Fixed) -> Fixed {
    a + (b - a) * t
}
impl DiscoveryCaption {
    fn advance(&mut self, next: Destination, dt: u16) {
        let changed = next != self.destination;
        if changed {
            let left_leads = next > self.destination;
            for (i, knot) in self.knots.iter_mut().enumerate() {
                let order = if left_leads { i } else { COUNT - 1 - i };
                knot.delay = BEAT_MS * order as u16;
            }
            self.destination = next;
        }
        // Time preceding a discovery notification belongs to the old state.
        // Keep the wave legible when a slow scan straddles the first frame.
        let dt = if changed { 0 } else { dt.min(32) };
        for knot in &mut self.knots {
            knot.advance(self.destination, dt);
        }
    }

    fn spine(&self, width: Fixed, height: Fixed) -> [Point; COUNT] {
        let em = Fixed::from_int(EM);
        let centered = (width - em * COUNT as i32) / 2;
        let header_y = em + Fixed::from_int(4);
        let idle_y = (height / 2 - Fixed::from_int(18)).max(header_y);
        let mut centers = std::array::from_fn(|i| {
            let knot = &self.knots[i];
            let dock = knot.dock.value();
            let online = knot.online.value();
            let arch = mix(Fixed::from_int(14), Fixed::from_int(3), dock) * (Fixed::ONE - online);
            let axis = Fixed::from_ratio(2 * i as i32 + 1, COUNT as i32) - Fixed::ONE;
            // The leading edge lifts before sliding out; the final approach
            // skims the header horizontally. Both directions share this track.
            let travel = Fixed::ONE - (Fixed::ONE - dock) * (Fixed::ONE - dock);
            let x = mix(centered, self.inset, super::motion::smoothstep(online))
                + em * i as i32
                + em / 2;
            // The retained offline row stays in place. The word turns inside
            // the header even when both endpoints share the same height.
            let turn = Fixed::sin_deg(online.clamp(Fixed::ZERO, Fixed::ONE) * 360) * 8;
            let y = mix(idle_y, header_y, travel) + arch * axis * axis + turn * dock;
            Point { x, y }
        });
        // A fixed reading-order constraint gives the same geometry at the
        // instant of a reversal, even with a fully extended pair of letters.
        for i in 1..COUNT {
            let a: Point = centers[i - 1];
            let b = &mut centers[i];
            b.x = b.x.clamp(a.x + em, a.x + em * Fixed::from_ratio(8, 5));
            b.y = b.y.clamp(a.y - em, a.y + em);
        }
        centers
    }

    fn opacities(&self) -> [u8; 2] {
        let dock = self
            .knots
            .iter()
            .fold(Fixed::ZERO, |total, knot| total + knot.dock.value())
            / COUNT as i32;
        let beacon = Fixed::ONE - super::motion::smoothstep(dock * 2);
        let results = super::motion::smoothstep(
            (dock - Fixed::from_ratio(60, 100)) / Fixed::from_ratio(40, 100),
        );
        [
            (beacon * 255).to_int() as u8,
            (results * 255).to_int() as u8,
        ]
    }
}

// Each native text window samples the same spine at one character. Its
// controls share the local tangent while leaving typesetting to Mirui.
fn glyph_segment(centers: &[Point; COUNT], i: usize) -> [PathCmd; 2] {
    let center = centers[i];
    let prev = centers[i.saturating_sub(1)];
    let next = centers[(i + 1).min(COUNT - 1)];
    let angle = Fixed::from_ratio(2, 5);
    let secant =
        |a: Point, b: Point| ((b.y - a.y) / (b.x - a.x).max(Fixed::ONE)).clamp(-angle, angle);
    let slope = secant(prev, next);
    let bend = if i == 0 || i + 1 == COUNT {
        Fixed::ZERO
    } else {
        (secant(center, next) - secant(prev, center)) / 4
    };
    let half = Fixed::from_int(GLYPH_WINDOW) / 2;
    let point = |x, lift| Point {
        x: center.x + x,
        y: center.y + slope * x + lift,
    };
    let lift = bend * half;
    [
        PathCmd::MoveTo(point(-half, lift)),
        PathCmd::CubicTo {
            ctrl1: point(-half / 3, -lift / 3),
            ctrl2: point(half / 3, -lift / 3),
            end: point(half, lift),
        },
    ]
}

#[compose]
pub(super) fn caption(state: model!(DisplayState), metrics: Metrics) -> Entity {
    let destination = Destination::from_display(&state);
    let retired = res!(RetiredPaths).with(|paths| RetiredPaths(paths.0.clone()));
    let mut store = cx.paths();
    for id in retired.0.borrow_mut().drain(..) {
        let _ = store.remove(id);
    }
    let hidden = std::array::from_fn(|i| Point::new((i as i32 + 1) * EM, -EM * 2));
    let paths: [PathId; COUNT] = std::array::from_fn(|i| {
        store
            .insert(Path::from_owned(glyph_segment(&hidden, i).into()))
            .expect("a mounted discovery caption owns four text paths")
    });
    let motion = DiscoveryCaption {
        state: state.clone(),
        paths,
        retired,
        destination,
        inset: Fixed::from_int(metrics.inset),
        knots: std::array::from_fn(|_| Knot::new(destination)),
        settled_size: None,
        last_centers: None,
        last_opacities: None,
    };
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100)
        ) [
            motion,
        ] {
            walk paths.into_iter().enumerate().map(|(i, path)| (DiscoveryGlyph(i), path)) with glyph {
                Text (
                    text: LETTERS[glyph.0.0],
                    path: TextPath::new(glyph.1).with_range(Fixed::ZERO..Fixed::from_int(GLYPH_WINDOW)),
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    font: font(TITLE_SIZE),
                    font_size: TITLE_SIZE,
                    text_color: FOREGROUND,
                    paragraph: paragraph(Center, 1)
                ) [
                    glyph.0,
                    FONT_PAINT,
                ]
            }
        }
    }
}

struct LayerOpacity;

impl Property for LayerOpacity {
    type Value = u8;

    fn apply(world: &mut World, entity: Entity, opacity: u8) -> PropertyChange {
        mirui::ui::property::apply_to_world::<prop::Visible>(world, entity, opacity != 0);
        if opacity == 0 || opacity == 255 {
            world.remove::<OffscreenRender>(entity);
        } else {
            world.insert(entity, OffscreenRender::default().with_opacity(opacity));
        }
        PropertyChange::Visual
    }
}

#[system(order = Animation)]
fn animate(
    delta: res!(DeltaTimeMs),
    store: res!(mut PathStore),
    retired: res!(RetiredPaths),
    captions: query!(mut DiscoveryCaption, Parent),
    stages: query!(ComputedRect, Children),
    layers: query!(Layer),
    commands: commands!(),
) {
    for path in retired.0.borrow_mut().drain(..) {
        let _ = store.remove(path);
    }
    for (caption, parent) in captions {
        let Some((stage, children)) = stages.get(parent.0) else {
            continue;
        };
        if stage.0.w <= Fixed::ZERO || stage.0.h <= Fixed::ZERO {
            continue;
        }
        let size = (stage.0.w, stage.0.h);
        let destination = Destination::from_display(&caption.state);
        if caption.settled_size == Some(size) && caption.destination == destination {
            continue;
        }
        caption.advance(destination, delta.0);
        caption.settled_size = caption
            .knots
            .iter()
            .all(|k| k.delay == 0 && k.dock.is_settled() && k.online.is_settled())
            .then_some(size);
        let centers = caption.spine(stage.0.w, stage.0.h);
        if caption.last_centers != Some(centers) {
            let prev = caption.last_centers.replace(centers);
            for (i, &id) in caption.paths.iter().enumerate() {
                let commands = glyph_segment(&centers, i);
                if prev
                    .as_ref()
                    .is_some_and(|old| glyph_segment(old, i) == commands)
                {
                    continue;
                }
                store
                    .edit(id, |path| {
                        for (index, command) in commands.into_iter().enumerate() {
                            path.set_command(index, command)
                                .expect("two-command discovery baseline");
                        }
                    })
                    .expect("mounted discovery glyph path");
            }
        }
        let opacities = caption.opacities();
        if caption.last_opacities == Some(opacities) {
            continue;
        }
        let last = caption.last_opacities.replace(opacities);
        for &child in &children.0 {
            let index = match layers.get(child) {
                Some(Layer::Beacon) => 0,
                Some(Layer::Results) => 1,
                None => continue,
            };
            if last.is_none_or(|prior| prior[index] != opacities[index]) {
                commands.set::<LayerOpacity>(child, opacities[index]);
            }
        }
    }
}

pub(super) fn install<B: mirui::surface::Surface, F: mirui::app::RendererFactory<B>>(
    app: &mut App<B, F>,
) {
    app.add_resource(RetiredPaths::default());
    app.add_system(animate::system());
}
