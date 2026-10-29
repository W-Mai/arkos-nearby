use super::{
    deck::{ActionAnchor, ActorAnchor, ActorScope, Layer},
    pixel::ActorSprite,
    selection::{FocusTarget, contains},
    state::{DisplayRuntime, DisplayState},
};
use mirui::prelude::*;
use mirui::ui::widgets::FollowBounds;
use mirui::ui::{ComputedRect, Hidden, Parent};

#[component]
pub(super) struct ActorFrame;

#[system(order = Animation)]
pub(super) fn follow(
    runtime: res!(DisplayRuntime),
    frames: query!(Entity, ActorFrame, optional Hidden, optional FollowBounds),
    scopes: query!(Entity, ActorScope),
    anchors: query!(Entity, ActorAnchor, ComputedRect),
    actions: query!(Entity, ActionAnchor, ComputedRect),
    targets: query!(Entity, FocusTarget, ComputedRect),
    pages: query!(super::selection::SelectionFrame),
    parents: query!(Parent),
    commands: commands!(),
) {
    let layer = runtime.deck.layer();
    let state = match layer {
        Layer::Shelf => &runtime.shelf,
        Layer::Catalog => &runtime.catalog,
        Layer::Room => &runtime.room,
        Layer::Actions => &runtime.state,
    };
    let scopes: Vec<_> = scopes.into_iter().collect();
    let Some(root) = scopes.iter().copied().find_map(|(entity, scope)| {
        matches!(scope, ActorScope::Layer(value) if *value == layer).then_some(entity)
    }) else {
        return;
    };
    let root = pages
        .into_iter()
        .find_map(|page| {
            (page.page == page.state.page_key()
                && contains(root, page.body, |e| parents.get(e).map(|p| p.0)))
            .then_some(page.body)
        })
        .unwrap_or(root);
    let root = scopes
        .iter()
        .copied()
        .find_map(|(entity, pane)| {
            (matches!(pane, ActorScope::Pane(page) if *page == state.focus_page())
                && contains(root, entity, |e| parents.get(e).map(|p| p.0)))
            .then_some(entity)
        })
        .unwrap_or(root);
    let inside = |node| contains(root, node, |e| parents.get(e).map(|p| p.0));
    let anchor = anchors.into_iter().find_map(|(entity, anchor, _)| {
        // Reactive branches retain their stage even after rooms appear.
        let active = !matches!(anchor.0, super::pixel::Stage::Search) || state.rooms().is_empty();
        (active && inside(entity)).then_some(entity)
    });
    let selection = match layer {
        Layer::Actions => Some(runtime.state.launcher().view().selected),
        _ => state.selected(),
    };
    let target = anchor.or_else(|| {
        if layer == Layer::Actions {
            actions.into_iter().find_map(|(entity, item, _)| {
                (inside(entity) && Some(item.0) == selection).then_some(entity)
            })
        } else {
            targets.into_iter().find_map(|(entity, item, _)| {
                (inside(entity) && Some(item.index()) == selection).then_some(entity)
            })
        }
    });
    if runtime.deck.actor_stage() != anchor.is_some() {
        runtime.deck.stage(anchor.is_some());
    }
    for (entity, _, hidden, bounds) in frames {
        if let Some(target) = target {
            if bounds.is_none_or(|old| old.target != target) {
                commands.insert(
                    entity,
                    FollowBounds::new(target).with_transition(mirui::anim::SNAPPY),
                );
            }
            if hidden.is_some() {
                commands.remove::<Hidden>(entity);
            }
        } else if hidden.is_none() {
            commands.insert(entity, Hidden);
        }
    }
}

#[compose]
pub(super) fn actor(state: model!(DisplayState), deck: model!(super::deck::Deck)) -> Entity {
    ui! {
        View (position: Position::Absolute, width: 0, height: 0) [
            ActorFrame,
            mirui::ui::IgnoreHitTest,
        ] {
            View (
                position: Position::Absolute,
                left: ${
                    Dimension::Percent(
                        if deck.actor_stage() {
                            state.actor_position() * Fixed::from_int(100)
                        } else if deck.layer() == Layer::Shelf {
                            Fixed::from_int(100)
                        } else {
                            Fixed::from_int(86)
                        },
                    )
                },
                top: ${
                    Dimension::percent(
                        if deck.actor_stage() || deck.layer() == Layer::Shelf {
                            100
                        } else if deck.layer() == Layer::Actions {
                            50
                        } else {
                            85
                        },
                    )
                },
                width: 32,
                height: 48,
                transform: ${
                    mirui::types::Transform::translate(
                            Fixed::from_int(if deck.layer() == Layer::Shelf { -24 } else { -16 }),
                            Fixed::from_int(
                                if matches!(deck.layer(), Layer::Actions | Layer::Shelf) {
                                    -36
                                } else {
                                    -48
                                },
                            ),
                        )
                        .compose(
                            &mirui::types::Transform::scale(
                                if matches!(deck.layer(), Layer::Actions | Layer::Shelf) {
                                    Fixed::HALF
                                } else {
                                    Fixed::ONE
                                },
                                if matches!(deck.layer(), Layer::Actions | Layer::Shelf) {
                                    Fixed::HALF
                                } else {
                                    Fixed::ONE
                                },
                            ),
                        )
                }
            ) [
                ActorSprite {
                    state: state.clone(),
                },
            ]
        }
    }
}
