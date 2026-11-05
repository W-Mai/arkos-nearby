use super::{
    deck::{ActionAnchor, ActionLayout, ActorAnchor, ActorScope, Layer},
    pixel::ActorSprite,
    selection::{FocusTarget, contains},
    state::{DisplayRuntime, DisplayState},
};
use mirui::prelude::*;
use mirui::ui::widgets::FollowBounds;
use mirui::ui::{ComputedRect, Hidden, Parent};

#[component]
pub(super) struct ActorFrame;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Source {
    Primary,
    Shelf,
    Catalog,
    Room,
    Actions,
}

fn source(deck: &super::deck::DeckHandle) -> Source {
    if !deck.enabled() {
        return Source::Primary;
    }
    match deck.layer() {
        Layer::Shelf => Source::Shelf,
        Layer::Catalog => Source::Catalog,
        Layer::Room => Source::Room,
        Layer::Actions => Source::Actions,
    }
}

fn session_owns_actor(page: crate::model::Page) -> bool {
    matches!(
        page,
        crate::model::Page::Host | crate::model::Page::Joined | crate::model::Page::Leaving
    )
}

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
    let state = match source(&runtime.deck) {
        Source::Primary | Source::Actions => &runtime.state,
        Source::Shelf => &runtime.shelf,
        Source::Catalog => &runtime.catalog,
        Source::Room => &runtime.room,
    };
    if session_owns_actor(state.page_key().page) {
        for (entity, _, hidden, _) in frames {
            if hidden.is_none() {
                commands.insert(entity, Hidden);
            }
        }
        return;
    }
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
        let empty = if matches!(
            state.page_key().page,
            crate::model::Page::Games | crate::model::Page::Candidates
        ) {
            state.entries().is_empty()
        } else {
            state.rooms().is_empty()
        };
        let active = !matches!(anchor.0, super::pixel::Stage::Search) || empty;
        (active && inside(entity)).then_some(entity)
    });
    let (selection, action_layout) = if layer == Layer::Actions {
        let menu = runtime.state.launcher().view();
        let layout = if let Some(wifi) = &menu.wifi {
            if wifi.page.keyboard() {
                ActionLayout::WifiKeyboard
            } else if wifi.page.pending() {
                ActionLayout::WifiProgress
            } else if wifi.page == nearby_menu::wifi::Page::List {
                ActionLayout::WifiList
            } else {
                ActionLayout::WifiOptions
            }
        } else if menu.saves.is_some() {
            if runtime.state.launcher().saves().busy() {
                ActionLayout::SavesProgress
            } else if runtime.state.launcher().saves().modal() {
                ActionLayout::SavesModal
            } else {
                ActionLayout::SavesList
            }
        } else if menu.configuration.is_some() {
            ActionLayout::Configuration
        } else if menu.overlay_kind == "controls"
            && menu
                .items
                .get(menu.selected)
                .is_some_and(|item| item.level.is_some())
        {
            ActionLayout::Levels
        } else if menu.overlay_kind == "details" {
            ActionLayout::Details
        } else {
            ActionLayout::Grid
        };
        (
            if matches!(
                layout,
                ActionLayout::WifiProgress | ActionLayout::SavesProgress
            ) {
                Some(0)
            } else {
                runtime.state.launcher().menu_selection()
            },
            layout,
        )
    } else {
        (state.selected(), ActionLayout::Grid)
    };
    let target = if layer == Layer::Actions && runtime.state.launcher().detail_expanded() {
        scopes.iter().find_map(|(entity, scope)| {
            let active = matches!(scope, ActorScope::Reading);
            (active && inside(*entity)).then_some(*entity)
        })
    } else {
        anchor.or_else(|| {
            if layer == Layer::Actions {
                actions.into_iter().find_map(|(entity, item, _)| {
                    (item.1 == action_layout && inside(entity) && Some(item.0) == selection)
                        .then_some(entity)
                })
            } else {
                targets.into_iter().find_map(|(entity, item, _)| {
                    (inside(entity) && Some(item.index()) == selection).then_some(entity)
                })
            }
        })
    };
    if runtime.deck.actor_stage() != anchor.is_some() {
        runtime.deck.stage(anchor.is_some());
    }
    for (entity, _, hidden, bounds) in frames {
        if let Some(target) = target {
            if bounds.is_none_or(|old| old.target != target) {
                commands.insert(
                    entity,
                    if matches!(
                        action_layout,
                        ActionLayout::WifiProgress | ActionLayout::SavesProgress
                    ) {
                        FollowBounds::new(target)
                    } else {
                        FollowBounds::new(target).with_transition(mirui::anim::SNAPPY)
                    },
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
pub(super) fn actor(
    state: model!(DisplayState),
    deck: model!(super::deck::Deck),
    shelf: model!(DisplayState),
    catalog: model!(DisplayState),
    room: model!(DisplayState),
) -> Entity {
    ui! {
        View (position: Position::Absolute, width: 0, height: 0) [
            ActorFrame,
            mirui::ui::IgnoreHitTest,
        ] {
            walk ${
                let active = source(&deck);
                std::iter::once((
                    active,
                    match active {
                        Source::Primary | Source::Actions => state.clone(),
                        Source::Shelf => shelf.clone(),
                        Source::Catalog => catalog.clone(),
                        Source::Room => room.clone(),
                    },
                ))
            } with owner by owner.0 {
                actor_body (owner.1, deck)
            }
        }
    }
}

#[compose]
fn actor_body(state: model!(DisplayState), deck: model!(super::deck::Deck)) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: ${
                Dimension::Percent(
                    if deck.layer() == Layer::Actions
                        && (state.launcher().wifi_page().pending()
                            || state.launcher().saves().active() && state.launcher().saves().busy())
                    {
                        Fixed::ZERO
                    } else if deck.actor_stage() {
                        state.actor_position() * Fixed::from_int(100)
                    } else if compact_actor(&state, &deck) {
                        Fixed::from_int(100)
                    } else {
                        Fixed::from_int(86)
                    },
                )
            },
            top: ${
                Dimension::percent(
                    if deck.layer() == Layer::Actions
                        && (state.launcher().wifi_page().pending()
                            || state.launcher().saves().active() && state.launcher().saves().busy())
                    {
                        0
                    } else if deck.actor_stage() || compact_actor(&state, &deck) {
                        100
                    } else {
                        85
                    },
                )
            },
            width: 32,
            height: 48,
            transform: ${
                mirui::types::Transform::translate(
                        Fixed::from_int(
                            if deck.layer() == Layer::Actions
                                && (state.launcher().wifi_page().pending()
                                    || state.launcher().saves().active()
                                        && state.launcher().saves().busy())
                            {
                                -16
                            } else if compact_actor(&state, &deck) {
                                -24
                            } else {
                                -16
                            },
                        ),
                        Fixed::from_int(if compact_actor(&state, &deck) { -36 } else { -48 }),
                    )
                    .compose(
                        &mirui::types::Transform::scale(
                            if compact_actor(&state, &deck) { Fixed::HALF } else { Fixed::ONE },
                            if compact_actor(&state, &deck) { Fixed::HALF } else { Fixed::ONE },
                        ),
                    )
            }
        ) [
            ActorSprite {
                state: state.clone(),
                actions: state.launcher(),
            },
        ]
    }
}

fn shelf_actor(state: &super::state::DisplayHandle, deck: &super::deck::DeckHandle) -> bool {
    deck.layer() == Layer::Shelf
        || state.page_key().page == crate::model::Page::Games
            && state.page_key().library == crate::model::LibraryKind::Favorites
}

fn keyboard_actor(state: &super::state::DisplayHandle) -> bool {
    state.page_key().page == crate::model::Page::Games && state.keyboard()
}

fn compact_actor(state: &super::state::DisplayHandle, deck: &super::deck::DeckHandle) -> bool {
    deck.layer() == Layer::Actions || shelf_actor(state, deck) || keyboard_actor(state)
}
