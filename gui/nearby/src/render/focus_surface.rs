use super::style::{ACCENT, DIVIDER, Metrics, SHELL_EDGE, WARNING};
use mirui::{
    anim::{INTERACTIVE, SNAPPY, Spring},
    core::reactive::effect_with_widget,
    ecs::DeltaTimeMs,
    prelude::*,
    render::command::DrawCommand,
    types::TransformClass,
    ui::{theme::Theme, view::PaintCtx},
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy)]
pub(super) enum Cue {
    Card,
    Row,
    Key,
    Dial,
    Depth,
}

#[component]
#[derive(Clone)]
struct FocusEdge {
    selected: Rc<Cell<bool>>,
    motion: Spring,
    cue: Cue,
}

#[compose]
pub(super) fn focus_shadow(selected: prop!(bool), cue: Cue, metrics: Metrics) -> Entity {
    let target = Rc::new(Cell::new(false));
    let edge = FocusEdge {
        selected: target.clone(),
        motion: Spring::preset(Fixed::ZERO, Fixed::ZERO, SNAPPY),
        cue,
    };
    let node = ui! {
        View (height: metrics.shadow_height(), min_width: 0, shrink: 0.0) [edge]
    };
    effect_with_widget(node, move || target.set(selected.get()));
    node
}

#[system(order = Animation)]
fn advance(edges: query!(Entity, FocusEdge), delta: res!(DeltaTimeMs), commands: commands!()) {
    for (entity, edge) in edges {
        let selected = edge.selected.get();
        let target = if selected { Fixed::ONE } else { Fixed::ZERO };
        if edge.motion.target == target
            && edge.motion.position == target
            && edge.motion.velocity == Fixed::ZERO
        {
            continue;
        }
        let mut next = edge.clone();
        if edge.motion.target != target {
            let spring = if matches!(edge.cue, Cue::Key | Cue::Dial) || !selected {
                INTERACTIVE
            } else {
                SNAPPY
            };
            next.motion.retarget(target, Some(spring));
        }
        next.motion.tick(delta.0.min(32));
        commands.insert(entity, next);
    }
}

#[view(component = FocusEdge, invalidation = Visual, systems = [advance::system()])]
pub(super) fn edge(edge: component!(), theme: res!(Theme), paint: paint!()) {
    paint_shadow(paint, &theme, edge.cue, edge.motion.position);
}

pub(super) fn paint_shadow(paint: &mut PaintCtx<'_>, theme: &Theme, cue: Cue, amount: Fixed) {
    let rect = paint.rect();
    if rect.w <= Fixed::ZERO || rect.h <= Fixed::ZERO {
        return;
    }
    let mut band = |area, color, opa| fill(paint, area, color, opa);
    band(rect, theme.resolve(DIVIDER), 255);
    let amount = amount.clamp(Fixed::ZERO, Fixed::ONE);
    if amount == Fixed::ZERO {
        return;
    }

    let opacity = (amount * 125).round().to_int() as u8;
    let shade = theme.resolve(SHELL_EDGE);
    let inset = ((Fixed::ONE - amount) * 4).min(rect.w / 2);
    band(
        Rect::new(
            rect.x + inset,
            rect.y + Fixed::ONE,
            rect.w - inset * 2,
            rect.h - Fixed::ONE,
        ),
        shade,
        opacity / 2,
    );
    let core = (inset + Fixed::from_int(3)).min(rect.w / 2);
    band(
        Rect::new(
            rect.x + core,
            rect.y,
            rect.w - core * 2,
            rect.h - Fixed::ONE,
        ),
        shade,
        opacity,
    );

    let (length, centered, color) = match cue {
        Cue::Card => (Fixed::from_ratio(3, 5), true, ACCENT),
        Cue::Row => (Fixed::from_ratio(1, 4), false, ACCENT),
        Cue::Key => (Fixed::HALF, true, ACCENT),
        Cue::Dial => (Fixed::from_ratio(1, 3), false, WARNING),
        Cue::Depth => return,
    };
    let length = (rect.w * length * amount).round().max(Fixed::ONE);
    let x = if centered {
        rect.x + (rect.w - length) / 2
    } else {
        rect.x + core
    };
    let height = if matches!(cue, Cue::Card) {
        rect.h.min(Fixed::from_int(2))
    } else {
        rect.h.min(Fixed::ONE)
    };
    let color = theme.resolve(color);
    let ink = (amount * 235).round().to_int() as u8;
    band(Rect::new(x, rect.y, length, height), color, ink);

    if matches!(cue, Cue::Dial) {
        let step = Fixed::from_int(4);
        let tick_width = Fixed::from_int(2);
        for index in 1..=3 {
            let tick = x + length + step * index;
            if tick + tick_width <= rect.x + rect.w {
                band(Rect::new(tick, rect.y, tick_width, height), color, ink);
            }
        }
    }
}

#[component]
struct ButtonShadow {
    state: model!(super::state::DisplayState),
    index: usize,
}

pub(super) fn button_transform(
    state: &super::state::DisplayStateHandle,
    index: usize,
    metrics: Metrics,
) -> mirui::types::Transform {
    use super::motion::timing::PRESS_MS;
    let height = if metrics.compact { 2 } else { 3 };
    let lift = state.button_motion().at(index) * height;
    let press = state.card_motion();
    let depth = if press.selected == Some(index) {
        Fixed::from_ratio(
            i32::from(PRESS_MS.saturating_sub(press.press)),
            i32::from(PRESS_MS),
        ) * 2
    } else {
        Fixed::ZERO
    };
    mirui::types::Transform::translate(Fixed::ZERO, depth - lift)
}

#[compose]
pub(super) fn button_shadow(
    state: model!(super::state::DisplayState),
    index: usize,
    metrics: Metrics,
) -> Entity {
    ui! {
        View (height: metrics.shadow_height(), min_width: 0, shrink: 0.0) [
            ButtonShadow { state, index },
        ]
    }
}

#[view(component = ButtonShadow, invalidation = Visual)]
pub(super) fn button_bottom(
    shadow: component!(),
    state: model!(component.state, watch(button_motion)),
    theme: res!(Theme),
    paint: paint!(),
) {
    paint_shadow(
        paint,
        &theme,
        Cue::Depth,
        state.button_motion().at(shadow.index),
    );
}

fn fill(paint: &mut PaintCtx<'_>, area: Rect, color: Color, opa: u8) {
    if area.w <= Fixed::ZERO || area.h <= Fixed::ZERO || opa == 0 {
        return;
    }
    let transform = paint.transform();
    let complex = !matches!(
        transform.classify(),
        TransformClass::Identity | TransformClass::Translate
    );
    paint.draw(&DrawCommand::Fill {
        area,
        transform,
        quad: complex.then(|| transform.apply_rect(area)),
        color,
        radius: Fixed::ZERO,
        opa,
    });
}
