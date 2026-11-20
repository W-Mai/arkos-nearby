use super::{style::*, widgets::paragraph};
use mirui::core::cache::{Cache, HasSize, MaxSize};
use mirui::prelude::*;
use mirui::render::{
    command::DrawCommand,
    path::Path,
    raster::FillRule,
    scene::{GradientStop, GradientUnits, LinearGradient, Paint, SpreadMode},
};
use mirui::types::Transform;
use mirui::ui::theme::{Theme, ThemedColor};
use mirui::ui::widgets::{ParagraphStyle, Text, TextAlign::Start, WidgetTransform};
use mirui::ui::{ComputedRect, Parent};

const LEAD_MS: u64 = 500;
const TAIL_MS: u64 = 720;
const PIXELS_PER_SECOND: u64 = 55;
const FADE_WIDTH: i32 = 48;

pub(super) struct TitleGradient(Paint);

impl HasSize for TitleGradient {
    fn cache_size(&self) -> usize {
        size_of::<Self>() + 4 * size_of::<GradientStop>()
    }
}

pub(super) type TitleGradients = std::cell::RefCell<Cache<[u8; 3], TitleGradient>>;

pub(super) fn title_gradients() -> TitleGradients {
    Cache::new(MaxSize::Count(crate::theme::Palette::ALL.len() + 1)).into()
}

static FADE_RECT: Path = path!(M 0 0 H 1 V 1 H 0 Z);

#[component]
pub(super) struct FoldTitle(ThemedColor);

#[view(component = FoldTitle, priority = 90)]
pub(super) fn fold_title(
    fold: component!(),
    node: entity!(),
    labels: query!(Parent, Reading),
    bounds: query!(ComputedRect),
    paint: paint!(),
    theme: res!(Theme),
    gradients: res!(TitleGradients),
) {
    let Some((parent, reader)) = labels.get(node) else {
        return;
    };
    let Some(slot) = bounds.get(parent.0) else {
        return;
    };
    let text = paint.rect();
    if reader.enabled() || text.w <= slot.0.w || slot.0.w <= Fixed::ZERO {
        return;
    }
    let width = Fixed::from_int(FADE_WIDTH).min(slot.0.w);
    let color = fold.0.resolve(&theme);
    let gradient = gradients
        .borrow_mut()
        .entry([color.r, color.g, color.b])
        .or_insert_with(|| {
            let stops =
                [(0, 0), (50, 64), (82, 179), (100, 255)].map(|(offset, alpha)| GradientStop {
                    offset: Fixed::from_ratio(offset, 100).into(),
                    color: Color::rgba(color.r, color.g, color.b, alpha).into(),
                });
            TitleGradient(Paint::LinearGradient(LinearGradient {
                start: Point::ZERO.into(),
                end: Point::new(Fixed::ONE, Fixed::ZERO).into(),
                stops: stops.to_vec().into(),
                spread: SpreadMode::Pad,
                units: GradientUnits::ObjectBoundingBox,
                transform: Transform::IDENTITY.into(),
            }))
        });
    paint.draw(&DrawCommand::FillPath {
        path: &FADE_RECT,
        transform: paint
            .transform()
            .compose(&Transform::translate(text.x + slot.0.w - width, text.y))
            .compose(&Transform::scale(width, text.h)),
        paint: &gradient.0,
        opa: 255,
        fill_rule: FillRule::NonZero,
    });
}

#[component]
pub(super) struct Reading {
    library: Option<super::state::DisplayHandle>,
    enabled: mirui::ui::Prop<bool>,
    last_index: Option<usize>,
    title: String,
    extent: i32,
    since: u64,
    pose: Option<(i32, bool)>,
}

impl Reading {
    fn new(library: Option<super::state::DisplayHandle>, enabled: mirui::ui::Prop<bool>) -> Self {
        Self {
            library,
            enabled,
            last_index: None,
            title: String::new(),
            extent: 0,
            since: 0,
            pose: None,
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled.get()
    }
}

fn cycle(overflow: i32, elapsed: u64) -> i32 {
    if overflow <= 0 {
        return 0;
    }
    let travel = (overflow as u64 * 1000).div_ceil(PIXELS_PER_SECOND);
    let elapsed = elapsed % (LEAD_MS + TAIL_MS + 2 * travel);
    let outgoing = elapsed.saturating_sub(LEAD_MS).min(travel);
    let returning = elapsed.saturating_sub(LEAD_MS + travel + TAIL_MS);
    let progress = outgoing.saturating_sub(returning);
    (progress * PIXELS_PER_SECOND / 1000).min(overflow as u64) as i32
}

fn offset(overflow: i32, elapsed: u64) -> Option<i32> {
    if overflow <= 0 || elapsed < LEAD_MS {
        return None;
    }
    let travel = (overflow as u64 * 1000).div_ceil(PIXELS_PER_SECOND);
    let elapsed = elapsed - LEAD_MS;
    (elapsed < travel + TAIL_MS)
        .then(|| ((elapsed.min(travel) * PIXELS_PER_SECOND / 1000) as i32).min(overflow))
}

#[system(order = Animation)]
fn advance(
    readers: query!(Entity, mut Reading, ComputedRect, Parent, Text),
    bounds: query!(ComputedRect),
    clock: res!(optional mirui::ecs::MonoClock),
    runtime: res!(super::state::DisplayRuntime),
    commands: commands!(),
) {
    let Some(clock) = clock else { return };
    let now = clock.now_ns() / 1_000_000;
    for (entity, reader, ink, parent, text) in readers {
        if !reader.enabled() {
            if reader.pose.take().is_some() {
                reader.title.clear();
                commands.remove::<WidgetTransform>(entity);
            }
            continue;
        }
        let Some(viewport) = bounds.get(parent.0) else {
            continue;
        };
        if viewport.0.w <= Fixed::ZERO {
            continue;
        }
        let extent = (ink.0.w - viewport.0.w).to_int().max(0);
        let index = reader
            .library
            .as_ref()
            .map(|state| state.library_selection());
        let title = text.resolve(None);
        if extent != reader.extent || reader.last_index != index || reader.title != title {
            reader.last_index = index;
            if reader.title != title {
                reader.title.clear();
                reader.title.push_str(&title);
            }
            reader.since = now;
            reader.extent = extent;
        }
        let elapsed = now.saturating_sub(reader.since);
        let scroll = if reader.library.is_some() {
            offset(extent, elapsed)
        } else if (!runtime.deck.enabled() || runtime.deck.layer() == super::deck::Layer::Shelf)
            && !runtime.state.launcher().blocked()
        {
            Some(cycle(extent, elapsed))
        } else {
            reader.since = now;
            continue;
        };
        // Keep the natural text measurable while the fitted label is visible.
        let pose = (scroll.unwrap_or_default(), scroll.is_some());
        if reader.pose != Some(pose) {
            commands.insert(
                entity,
                WidgetTransform(Transform::translate(
                    Fixed::from_int(-pose.0),
                    if pose.1 { Fixed::ZERO } else { viewport.0.h },
                )),
            );
            reader.pose = Some(pose);
        }
    }
}

#[view(component = Reading, systems = [advance::system()])]
pub(super) fn marquee() {}

#[compose]
pub(super) fn title_strip(state: model!(super::state::DisplayState), platforms: bool) -> Entity {
    ui! {
        Row (
            grow: 1.0,
            min_width: 0,
            height: Dimension::percent(100),
            container: true,
            clip_children: true
        ) {
            Text (
                text: ${ state.reading_title(platforms) },
                grow: 1.0,
                min_width: 0,
                height: Dimension::percent(100),
                font: font(TITLE_SIZE),
                font_size: TITLE_SIZE,
                text_color: FOREGROUND,
                paragraph: paragraph(Start, 1)
            ) [
                FONT_PAINT,
            ]
            Text (
                text: ${ state.reading_title(platforms) },
                position: Position::Absolute,
                top: 0,
                left: 0,
                width: Dimension::Content,
                shrink: 0.0,
                height: Dimension::percent(100),
                font: font(TITLE_SIZE),
                font_size: TITLE_SIZE,
                bg_color: BACKGROUND,
                text_color: FOREGROUND,
                paragraph: ParagraphStyle::label().with_align(Start)
            ) [
                Reading::new(Some(state.clone()), true.into()),
                FONT_PAINT,
            ]
        }
    }
}

#[compose]
pub(super) fn cartridge_title(
    title: prop!(String),
    scroll: prop!(bool),
    ink: prop!(ThemedColor),
    backdrop: ThemedColor,
) -> Entity {
    ui! {
        Row (
            grow: 1.0, min_width: 0, height: Dimension::percent(100),
            justify: JustifyContent::Center, container: true, clip_children: true
        ) {
            Text (
                text: ${ title.get() },
                width: Dimension::Content, shrink: 0.0, height: Dimension::percent(100),
                font: font(TITLE_SIZE), font_size: TITLE_SIZE,
                text_color: ${ ink.get() },
                paragraph: ParagraphStyle::label().with_align(Start)
            ) [
                Reading::new(None, scroll),
                FoldTitle(backdrop),
                FONT_PAINT,
            ]
        }
    }
}
