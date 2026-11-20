use super::state::{DisplayState, PageKey};
use mirui::prelude::*;
use mirui::ui::widgets::FollowBounds;
use mirui::ui::{Hidden, LayoutGeometry, Outputs, Parent};

#[component]
pub(super) struct FocusTarget {
    fixed: usize,
    scope: Option<crate::model::Page>,
    row: model!(optional super::rows::Row),
}

impl FocusTarget {
    pub fn fixed(index: usize) -> Self {
        Self {
            fixed: index,
            scope: None,
            row: None,
        }
    }
    pub fn row(row: super::rows::RowHandle) -> Self {
        Self {
            fixed: 0,
            scope: None,
            row: Some(row),
        }
    }
    pub fn scoped(mut self, page: crate::model::Page) -> Self {
        self.scope = Some(page);
        self
    }
    fn active(&self, state: &super::state::DisplayHandle) -> bool {
        self.scope.is_none_or(|scope| {
            scope == state.focus_page()
                || scope == crate::model::Page::Rooms
                    && state.focus_page() == crate::model::Page::Lobby
        })
    }
    pub fn index(&self) -> usize {
        self.row.as_ref().map_or(self.fixed, |row| row.index())
    }
}

#[component]
pub(super) struct SelectionFrame {
    pub body: Entity,
    pub page: PageKey,
    pub state: model!(DisplayState),
}

#[component]
#[derive(Clone, Copy)]
pub(super) struct Outline {
    #[depends_on(LayoutGeometry)]
    pub frame: Entity,
    // The sibling shares this coordinate space even when it is painted offscreen.
    #[depends_on(LayoutGeometry)]
    pub panel: Entity,
    pub width: Fixed,
}

#[event]
fn outline_panel(
    node: entity!(),
    outlines: query!(Outline),
    parents: query!(Parent),
    commands: commands!(),
) {
    if let Some((outline, parent)) = outlines.get(node).zip(parents.get(node)) {
        commands.insert(
            node,
            Outline {
                panel: parent.0,
                ..*outline
            },
        );
    }
}

#[view(component = Outline, attach = outline_panel::callback(), invalidation = Visual)]
pub(super) fn outline(
    outline: component!(),
    paint: paint!(),
    geometry: res!(Outputs<LayoutGeometry>),
    frames: query!(SelectionFrame),
    theme: res!(mirui::ui::theme::Theme),
) {
    let Some(((bounds, panel), frame)) = geometry
        .get(outline.frame)
        .zip(geometry.get(outline.panel))
        .zip(frames.get(outline.frame))
    else {
        return;
    };
    let Some(border) = outline_rect(bounds, panel, frame.state.motion().expansion()) else {
        return;
    };
    let width = outline.width.min(border.w / 2).min(border.h / 2);
    if width <= Fixed::ZERO {
        return;
    }
    let color = theme.resolve(super::style::ACCENT);
    let rect = paint.rect();
    let transform = paint
        .transform()
        .compose(&mirui::types::Transform::translate(rect.x, rect.y));
    for area in [
        Rect { h: width, ..border },
        Rect {
            y: border.y + border.h - width,
            h: width,
            ..border
        },
        Rect {
            y: border.y + width,
            w: width,
            h: border.h - width * 2,
            ..border
        },
        Rect {
            x: border.x + border.w - width,
            y: border.y + width,
            w: width,
            h: border.h - width * 2,
        },
    ] {
        paint.draw(&mirui::render::command::DrawCommand::Fill {
            area,
            color,
            transform,
            quad: Some(transform.apply_rect(area)),
            radius: Fixed::ZERO,
            opa: 255,
        });
    }
}

pub(super) fn outline_rect(
    bounds: mirui::ui::LayoutGeometryRead,
    panel: mirui::ui::LayoutGeometryRead,
    (x, y): (Fixed, Fixed),
) -> Option<Rect> {
    let to_panel = panel.transform().inverse()?;
    let percent = Fixed::from_int(100);
    let mut border = bounds.border_rect();
    let expand_x = border.w * x / percent;
    let expand_y = border.h * y / percent;
    border.x -= expand_x;
    border.y -= expand_y;
    border.w += expand_x * 2;
    border.h += expand_y * 2;
    let corners = to_panel.compose(&bounds.transform()).apply_rect(border)?;
    let mut border = Rect::bounding_quad(&corners);

    let clip = bounds.clip();
    if clip.w <= Fixed::ZERO || clip.h <= Fixed::ZERO {
        return None;
    }
    // Leave room for the moving outline beyond the row viewport. Clamp the
    // whole rectangle so no individual edge disappears into a scroll mask.
    let allowed = to_panel
        .apply_rect(clip.inflate(6.into()))
        .and_then(|clip| Rect::bounding_quad(&clip).intersect(&panel.border_rect()))?;
    border.w = border.w.min(allowed.w);
    border.h = border.h.min(allowed.h);
    border.x = border
        .x
        .max(allowed.x)
        .min(allowed.x + allowed.w - border.w);
    border.y = border
        .y
        .max(allowed.y)
        .min(allowed.y + allowed.h - border.h);
    Some(border)
}

pub(super) fn contains(
    root: Entity,
    mut node: Entity,
    parent: impl Fn(Entity) -> Option<Entity>,
) -> bool {
    loop {
        if node == root {
            return true;
        }
        let Some(next) = parent(node) else {
            return false;
        };
        node = next;
    }
}

fn resolve<'a>(
    frame: &SelectionFrame,
    targets: impl IntoIterator<Item = (Entity, &'a FocusTarget)>,
    parent: impl Fn(Entity) -> Option<Entity>,
) -> Option<Entity> {
    if frame.state.launcher().blocked() {
        return None;
    }
    let selected = frame.state.selected();
    targets.into_iter().find_map(|(node, target)| {
        (target.active(&frame.state)
            && selected == Some(target.index())
            && contains(frame.body, node, &parent))
        .then_some(node)
    })
}

enum FocusChange {
    Bounds(FollowBounds),
    Show,
    Hide,
}

pub(super) fn track(
    target: Entity,
    current: Option<&FollowBounds>,
    direct: bool,
) -> Option<FollowBounds> {
    if current.is_some_and(|old| old.target == target && (!direct || old.transition.is_none())) {
        return None;
    }
    let mut next = current
        .copied()
        .unwrap_or_else(|| FollowBounds::new(target));
    next.target = target;
    next.transition = (!direct).then(|| mirui::anim::SNAPPY.into());
    Some(next)
}

fn follow(
    node: Option<Entity>,
    current: Option<&FollowBounds>,
    hidden: bool,
    direct: bool,
    mut apply: impl FnMut(FocusChange),
) {
    if let Some(target) = node {
        if let Some(next) = track(target, current, direct) {
            apply(FocusChange::Bounds(next));
        }
        if hidden {
            apply(FocusChange::Show);
        }
    } else if !hidden || current.is_some() {
        apply(FocusChange::Hide);
    }
}

#[event]
fn initialize(
    node: entity!(),
    frames: query!(SelectionFrame),
    targets: query!(Entity, FocusTarget),
    parents: query!(Parent),
    commands: commands!(),
) {
    let Some(frame) = frames.get(node) else {
        return;
    };
    let target = resolve(frame, targets, |e| parents.get(e).map(|p| p.0));
    follow(
        target,
        None,
        false,
        frame.state.shelf().sliding(),
        |change| match change {
            FocusChange::Bounds(bounds) => commands.insert(node, bounds),
            FocusChange::Show => commands.remove::<Hidden>(node),
            FocusChange::Hide => {
                commands.insert(node, Hidden);
                commands.remove::<FollowBounds>(node);
            }
        },
    );
}

#[view(component = SelectionFrame, attach = initialize::callback())]
pub(super) fn frame() {}

#[event]
fn initialize_target(
    node: entity!(),
    targets: query!(FocusTarget),
    frames: query!(Entity, SelectionFrame, optional Hidden),
    followers: query!(FollowBounds),
    parents: query!(Parent),
    commands: commands!(),
) {
    let Some(target) = targets.get(node) else {
        return;
    };
    for (entity, frame, hidden) in frames {
        if target.active(&frame.state)
            && frame.state.selected() == Some(target.index())
            && contains(frame.body, node, |e| parents.get(e).map(|p| p.0))
        {
            follow(
                Some(node),
                followers.get(entity),
                hidden.is_some(),
                frame.state.shelf().sliding(),
                |change| match change {
                    FocusChange::Bounds(bounds) => commands.insert(entity, bounds),
                    FocusChange::Show => commands.remove::<Hidden>(entity),
                    FocusChange::Hide => {
                        commands.insert(entity, Hidden);
                        commands.remove::<FollowBounds>(entity);
                    }
                },
            );
        }
    }
}

#[view(component = FocusTarget, attach = initialize_target::callback())]
pub(super) fn target() {}

#[system(order = Animation)]
pub(super) fn update(
    runtime: res!(super::state::DisplayRuntime),
    frames: query!(Entity, SelectionFrame, optional Hidden),
    scopes: query!(super::deck::ActorScope),
    followers: query!(FollowBounds),
    targets: query!(Entity, FocusTarget),
    parents: query!(Parent),
    commands: commands!(),
) {
    let targets: Vec<_> = targets.into_iter().collect();
    for (entity, frame, hidden) in frames {
        let mut ancestor = Some(frame.body);
        let mut active = true;
        while let Some(node) = ancestor {
            if let Some(super::deck::ActorScope::Layer(layer)) = scopes.get(node) {
                active = *layer == runtime.deck.layer();
                break;
            }
            ancestor = parents.get(node).map(|parent| parent.0);
        }
        let target = if active && frame.page == frame.state.page_key() {
            resolve(frame, targets.iter().copied(), |e| {
                parents.get(e).map(|p| p.0)
            })
        } else {
            None
        };
        follow(
            target,
            followers.get(entity),
            hidden.is_some(),
            frame.state.shelf().sliding(),
            |change| match change {
                FocusChange::Bounds(bounds) => commands.insert(entity, bounds),
                FocusChange::Show => commands.remove::<Hidden>(entity),
                FocusChange::Hide => {
                    commands.insert(entity, Hidden);
                    commands.remove::<FollowBounds>(entity);
                }
            },
        );
    }
}
