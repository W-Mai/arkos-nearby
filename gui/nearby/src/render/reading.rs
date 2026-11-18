use super::{style::*, widgets::paragraph};
use mirui::prelude::*;
use mirui::types::Transform;
use mirui::ui::widgets::{ParagraphStyle, Text, TextAlign::Start, WidgetTransform};
use mirui::ui::{ComputedRect, Parent};

const LEAD_MS: u64 = 500;
const TAIL_MS: u64 = 720;
const PIXELS_PER_SECOND: u64 = 55;

#[component]
pub(super) struct Reading {
    state: super::state::DisplayHandle,
    last_index: Option<usize>,
    title: String,
    extent: i32,
    since: u64,
    pose: Option<(i32, bool)>,
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
    commands: commands!(),
) {
    let Some(clock) = clock else { return };
    let now = clock.now_ns() / 1_000_000;
    for (entity, reader, ink, parent, text) in readers {
        let Some(viewport) = bounds.get(parent.0) else {
            continue;
        };
        if viewport.0.w <= Fixed::ZERO {
            continue;
        }
        let extent = (ink.0.w - viewport.0.w).to_int().max(0);
        let index = reader.state.library_selection();
        let title = text.resolve(None);
        if extent != reader.extent || reader.last_index != Some(index) || reader.title != title {
            reader.last_index = Some(index);
            if reader.title != title {
                reader.title.clear();
                reader.title.push_str(&title);
            }
            reader.since = now;
            reader.extent = extent;
        }
        let scroll = offset(extent, now.saturating_sub(reader.since));
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
                Reading {
                    state: state.clone(),
                    last_index: None,
                    title: String::new(),
                    extent: 0,
                    since: 0,
                    pose: None,
                },
                FONT_PAINT,
            ]
        }
    }
}
