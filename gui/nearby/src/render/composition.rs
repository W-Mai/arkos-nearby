use super::actor::actor;
use super::launcher::deck as launcher_deck;
use super::launcher::overlay;
use super::{
    pages::page,
    selection,
    state::{DisplayState, PageKey},
    style::*,
};
use mirui::prelude::*;
use mirui::ui::widgets::page_view::{CreatePage, PageView};

#[event]
fn create_page(
    event: event!(CreatePage<PageKey>),
    state: model!(DisplayState),
    metrics: Metrics,
) -> Entity {
    let route = *event.route();
    ui! {
        mounted_page (route, state, metrics)
    }
}

#[compose]
fn mounted_page(view: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        Column (grow: 1.0, min_height: 0, bg_color: BACKGROUND, clip_children: true) {
            page_contents (view, state.clone(), metrics)
            overlay (state.clone(), metrics)
        }
    }
}

#[compose]
fn page_contents(view: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let body = ui! {
        page (view, state, metrics)
    };
    ui! {
        focus_frame (body, view, state)
    }
}

#[compose]
pub(super) fn focus_frame(body: Entity, view: PageKey, state: model!(DisplayState)) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            width: 0,
            height: 0,
            border_width: 0
        ) [
            mirui::ui::IgnoreHitTest,
            selection::SelectionFrame {
                body,
                page: view,
                state: state.clone(),
            },
        ] {
            View (
                position: Position::Absolute,
                left: ${ Dimension::Percent(-state.motion().expansion().0) },
                width: ${
                    Dimension::Percent(
                        Fixed::from_int(100) + state.motion().expansion().0 * Fixed::from_int(2),
                    )
                },
                top: ${ Dimension::Percent(-state.motion().expansion().1) },
                height: ${
                    Dimension::Percent(
                        Fixed::from_int(100) + state.motion().expansion().1 * Fixed::from_int(2),
                    )
                },
                border_color: ACCENT,
                border_width: FOCUS_BORDER
            ) [
                mirui::ui::IgnoreHitTest,
            ]
        }
    }
}

#[compose]
pub(super) fn shell(state: model!(DisplayState), metrics: Metrics, launcher: bool) -> Entity {
    let (shelf, catalog, room, deck) = res!(super::state::DisplayRuntime).with(|runtime| {
        (
            runtime.shelf.clone(),
            runtime.catalog.clone(),
            runtime.room.clone(),
            runtime.deck.clone(),
        )
    });
    if launcher {
        ui! {
            Column (id: "nearby_pages", grow: 1.0, min_height: 0, clip_children: true) {
                launcher_deck (state, shelf, catalog, room, deck, metrics)
            }
        }
    } else {
        ui! {
            Column (grow: 1.0, min_height: 0) [
                super::deck::ActorScope::Layer(super::deck::Layer::Room),
            ] {
                PageView (
                    id: "nearby_pages",
                    grow: 1.0,
                    min_height: 0,
                    bg_color: BACKGROUND,
                    page: ${ state.page_key() },
                    transition: ${ state.transition() }
                ) on CreatePage(create_page::callback(state.clone(), metrics))
                actor (state, deck, shelf, catalog, room)
            }
        }
    }
}

#[compose]
pub(super) fn room_pages(state: model!(DisplayState), metrics: Metrics) -> Entity {
    ui! {
        PageView (
            grow: 1.0,
            min_height: 0,
            page: ${ state.page_key() },
            transition: ${ state.transition() }
        ) on CreatePage(create_page::callback(state.clone(), metrics))
    }
}
