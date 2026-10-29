use super::rows::{self, Row};
use super::{copy::*, selection, style::*};
use crate::model::{Page, Phase, View};
use TextField::{Headline, HostButton, Next, Peer};
use mirui::app::RendererFactory;
use mirui::core::model::ModelList;
use mirui::prelude::*;
use mirui::surface::Surface;
use mirui::ui::widgets::navigation::{PageDirection, PageTransition};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub(super) enum TextField {
    Headline,
    Next,
    HostButton,
    Peer,
    RoomName,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct StepColors {
    pub(super) background: ColorToken,
    pub(super) foreground: ColorToken,
}

#[derive(Default)]
struct LibraryPresentation {
    entries: Vec<crate::model::GameRow>,
    detail: crate::model::LibraryView,
    selected: usize,
    platforms: bool,
    artwork: String,
    artwork_neighbors: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DisplayRole {
    Primary,
    Shelf,
    Catalog,
    Room,
}

fn renders_rows(role: DisplayRole, view: &View) -> bool {
    role != DisplayRole::Primary || !(view.launcher.enabled || view.return_to_launcher)
}

#[derive(Clone, Copy, Default)]
pub(super) struct Changes(u16);

impl Changes {
    const NONE: Self = Self(0);
    const ROOM_NAME: Self = Self(1 << 0);
    const HEADING: Self = Self(1 << 1);
    const SUBTITLE: Self = Self(1 << 2);
    const STATUS: Self = Self(1 << 3);
    const HINT: Self = Self(1 << 4);
    const BACK: Self = Self(1 << 5);
    const HEADLINE: Self = Self(1 << 6);
    const NEXT: Self = Self(1 << 7);
    const HOST_BUTTON: Self = Self(1 << 8);
    const PEER: Self = Self(1 << 9);
    const CONFIRM: Self = Self(1 << 10);
    const DEVICES: Self = Self(1 << 11);
    const QUERY: Self = Self(1 << 12);
    fn contains(self, mask: Self) -> bool {
        self.0 & mask.0 == mask.0
    }
    fn update<T: PartialEq>(&mut self, target: &mut T, next: T, mask: Self) {
        if *target != next {
            *target = next;
            self.0 |= mask.0;
        }
    }
}

#[model(
    change = Changes,
    watch(
        room_name = Changes::ROOM_NAME,
        heading = Changes::HEADING,
        subtitle = Changes::SUBTITLE,
        status = Changes::STATUS,
        hint = Changes::HINT,
        back = Changes::BACK,
        headline = Changes::HEADLINE,
        next = Changes::NEXT,
        host_button = Changes::HOST_BUTTON,
        peer = Changes::PEER,
        confirm = Changes::CONFIRM,
        devices = Changes::DEVICES,
        query = Changes::QUERY,
    )
)]
pub(super) struct DisplayState {
    role: DisplayRole,
    #[observe]
    pub(super) artwork_token: Option<u64>,
    pub(super) cover_bounds: model!(owned super::artwork::CoverBounds),
    pub(super) shelf: model!(owned super::shelf::Shelf),
    #[observe]
    pub(super) deck_height: Fixed,
    view: View,
    library: LibraryPresentation,
    artwork_revision: u64,
    pub(crate) launcher: model!(owned crate::launcher::Launcher),
    pub(crate) settings: model!(owned crate::settings::Settings),
    direction: PageDirection,
    pub(super) rooms: model!(owned ModelList<Row>),
    pub(super) entries: model!(owned ModelList<Row>),
    row_revision: u64,
    devices: Vec<crate::model::NetworkPeer>,
    query: String,
    room_name: String,
    heading: String,
    subtitle: String,
    status: String,
    hint: String,
    back: String,
    headline: String,
    next: String,
    host_button: String,
    peer: String,
    confirm: String,
    #[observe]
    pub(super) confirm_visible: bool,
    #[observe]
    pub(super) steps: [StepColors; 3],
    #[observe]
    pub(super) stamp: Color,
    #[observe]
    pub(super) activity: [Color; 3],
    timeline: super::motion::Timeline,
    feedback: crate::feedback::Feedback,
    #[observe]
    pub(super) motion: super::motion::Frame,
    animation_gate: Option<Rc<super::modal::AnimationGate>>,
}

#[model]
impl DisplayState {
    pub(super) fn set_artwork_token(&mut self, token: Option<u64>) -> Changes {
        self.artwork_token = token;
        Changes::NONE
    }
    pub(super) fn set_deck_height(&mut self, height: Fixed) -> Changes {
        self.deck_height = height;
        Changes::NONE
    }

    pub(super) fn present(&mut self, view: &View) -> Changes {
        let bind_rows = renders_rows(self.role, view);
        let binding_activated = bind_rows && !renders_rows(self.role, &self.view);
        self.launcher.present(view);
        self.shelf.present(view);
        let mut changes = Changes::NONE;
        if view.page == Page::Games {
            changes.update(&mut self.query, view.library.query.clone(), Changes::QUERY);
        }
        changes.update(&mut self.status, status_text(view).into(), Changes::STATUS);
        changes.update(&mut self.hint, hint_text(view).into(), Changes::HINT);
        changes.update(
            &mut self.back,
            if view.back_label.is_empty() {
                "返回"
            } else {
                &view.back_label
            }
            .into(),
            Changes::BACK,
        );
        changes.update(&mut self.headline, headline(view).into(), Changes::HEADLINE);
        changes.update(&mut self.next, next_step(view).into(), Changes::NEXT);
        changes.update(
            &mut self.host_button,
            button_text(view).into(),
            Changes::HOST_BUTTON,
        );
        changes.update(&mut self.peer, peer_text(view).into(), Changes::PEER);
        changes.update(
            &mut self.confirm,
            confirm_text(view).into(),
            Changes::CONFIRM,
        );
        self.confirm_visible = !confirm_text(view).is_empty();
        let active = match phase(view) {
            Phase::Connecting => Some(0),
            Phase::ResolvingGame | Phase::SelectingGame | Phase::Checking | Phase::Notifying => {
                Some(1)
            }
            Phase::WaitingStart | Phase::Starting => Some(2),
            _ => None,
        };
        self.steps = std::array::from_fn(|index| {
            if active == Some(index) {
                StepColors {
                    background: ACCENT,
                    foreground: ON_ACCENT,
                }
            } else if active.is_some_and(|step| index < step) {
                StepColors {
                    background: ACCENT_DIM,
                    foreground: ACCENT,
                }
            } else {
                StepColors {
                    background: PANEL,
                    foreground: DISABLED,
                }
            }
        });
        let first = view
            .room_selected
            .unwrap_or(view.selected)
            .saturating_sub(3)
            .min(view.rooms.len().saturating_sub(4));
        let previous_first = self
            .view
            .room_selected
            .unwrap_or(self.view.selected)
            .saturating_sub(3)
            .min(self.view.rooms.len().saturating_sub(4));
        let rooms_changed = previous_first != first
            || self.view.rooms.iter().skip(previous_first).take(4).ne(view
                .rooms
                .iter()
                .skip(first)
                .take(4));
        if bind_rows && (rooms_changed || binding_activated) {
            let rows: Vec<_> = view
                .rooms
                .iter()
                .enumerate()
                .skip(first)
                .take(4)
                .map(|(i, row)| Row::room(i, row))
                .collect();
            self.row_revision = self.row_revision.wrapping_add(1);
            rows::reconcile(&mut self.rooms, rows, self.row_revision);
        }
        // Both game-row routes own this collection. Retain it only when leaving
        // for another route whose empty defaults do not describe game rows.
        if matches!(view.page, Page::Games | Page::Candidates) {
            let limit = if view.library.kind == crate::model::LibraryKind::Favorites {
                super::shelf::MAX_VISIBLE
            } else {
                4
            };
            if bind_rows
                && (binding_activated
                    || self
                        .library
                        .entries
                        .iter()
                        .take(limit)
                        .ne(view.entries.iter().take(limit)))
            {
                self.artwork_revision = self.artwork_revision.wrapping_add(1);
                self.row_revision = self.row_revision.wrapping_add(1);
                let entries = view
                    .entries
                    .iter()
                    .take(limit)
                    .enumerate()
                    .map(|(i, row)| Row::game(i, row))
                    .collect();
                if self.role == DisplayRole::Catalog
                    && view.page == Page::Games
                    && !view.platforms
                    && view.library.kind == crate::model::LibraryKind::Directory
                {
                    rows::reconcile_slots(&mut self.entries, entries, self.row_revision);
                } else {
                    rows::reconcile(&mut self.entries, entries, self.row_revision);
                }
            }
            if self.library.artwork != view.artwork
                || self
                    .library
                    .artwork_neighbors
                    .iter()
                    .ne(view.artwork_neighbors.iter().take(2))
                || self.library.detail.kind != view.library.kind
            {
                self.artwork_revision = self.artwork_revision.wrapping_add(1);
            }
            self.library = LibraryPresentation {
                entries: view.entries.iter().take(limit).cloned().collect(),
                detail: view.library.clone(),
                selected: view.selected,
                platforms: view.platforms,
                artwork: view.artwork.clone(),
                artwork_neighbors: view.artwork_neighbors.iter().take(2).cloned().collect(),
            };
        }
        let mut devices = vec![crate::model::NetworkPeer {
            name: "本机".into(),
            address: if view.local_address.is_empty() {
                "等待分配地址".into()
            } else {
                view.local_address.clone()
            },
        }];
        devices.extend(view.peers.iter().cloned());
        changes.update(&mut self.devices, devices, Changes::DEVICES);
        changes.update(
            &mut self.room_name,
            view.room_name.clone(),
            Changes::ROOM_NAME,
        );
        changes.update(
            &mut self.heading,
            page_heading(view).into(),
            Changes::HEADING,
        );
        changes.update(
            &mut self.subtitle,
            match view.page {
                Page::Mode | Page::Lobby if !view.current_game => "连接附近的掌机".into(),
                _ if view.network_only => "仅组网 · 局域网连接".into(),
                Page::Games => view.core.clone(),
                _ => format!("当前游戏  {}", view.title),
            },
            Changes::SUBTITLE,
        );
        self.view.clone_from(view);
        changes
    }

    fn room_name_value(&self) -> String {
        self.room_name.clone()
    }
    fn heading_value(&self) -> String {
        self.heading.clone()
    }
    fn subtitle_value(&self) -> String {
        self.subtitle.clone()
    }
    fn status_value(&self) -> String {
        self.status.clone()
    }
    fn hint_value(&self) -> String {
        self.hint.clone()
    }
    fn back_value(&self) -> String {
        self.back.clone()
    }
    fn headline_value(&self) -> String {
        self.headline.clone()
    }
    fn next_value(&self) -> String {
        self.next.clone()
    }
    fn host_button_value(&self) -> String {
        self.host_button.clone()
    }
    fn peer_value(&self) -> String {
        self.peer.clone()
    }
    fn confirm_value(&self) -> String {
        self.confirm.clone()
    }
    fn devices_value(&self) -> Vec<crate::model::NetworkPeer> {
        self.devices.clone()
    }

    #[cfg(test)]
    pub(super) fn snapshot(&self) -> View {
        self.view.clone()
    }

    #[observe]
    pub(super) fn page_key(&self) -> PageKey {
        PageKey {
            library: if self.view.page == Page::Games {
                self.view.library.kind
            } else {
                crate::model::LibraryKind::Directory
            },
            page: self.view.scene_page(),
            network_only: self.view.network_only
                && matches!(
                    self.view.page,
                    Page::Choice | Page::Rooms | Page::Host | Page::Joined | Page::Leaving
                ),
        }
    }

    #[observe]
    pub(super) fn transition(&self) -> PageTransition {
        PageTransition::slide(mirui::anim::SNAPPY).with_direction(self.direction)
    }

    #[cfg(any(target_os = "linux", test))]
    pub(super) fn navigate_back(&mut self) -> Changes {
        self.direction = PageDirection::Backward;
        Changes::NONE
    }

    #[cfg(any(target_os = "linux", test))]
    pub(super) fn navigate_forward(&mut self) -> Changes {
        self.direction = PageDirection::Forward;
        Changes::NONE
    }

    #[observe]
    pub(super) fn selected(&self) -> Option<usize> {
        match self.view.page {
            Page::Games if self.view.library.keyboard => {
                Some(100 + self.view.library.key_selected.min(39))
            }
            Page::Mode => Some(
                self.view
                    .selected
                    .min(if self.view.current_game { 2 } else { 1 }),
            ),
            Page::Choice => Some(self.view.selected.min(1)),
            Page::Rooms | Page::Lobby if !self.view.rooms.is_empty() => {
                Some(self.view.selected.min(self.view.rooms.len() - 1))
            }
            Page::Games | Page::Candidates if !self.view.entries.is_empty() => {
                Some(self.view.selected.min(self.entries.len().saturating_sub(1)))
            }
            Page::Host if host_action_enabled(&self.view) => Some(0),
            Page::Joined if self.view.network_only && host_action_enabled(&self.view) => Some(0),
            _ => None,
        }
    }

    #[observe]
    pub(super) fn focus_page(&self) -> Page {
        self.view.page
    }

    #[observe]
    pub(super) fn deck(&self) -> Fixed {
        self.motion.deck
    }

    #[observe]
    pub(super) fn current_game(&self) -> bool {
        self.view.current_game
    }

    pub(super) fn artwork(&self) -> String {
        self.library.artwork.clone()
    }

    pub(super) fn artwork_revision(&self) -> u64 {
        self.artwork_revision
    }

    pub(super) fn artwork_neighbors(&self) -> Vec<String> {
        self.library.artwork_neighbors.clone()
    }

    #[observe]
    pub(super) fn platforms(&self) -> bool {
        self.library.platforms
    }

    #[observe]
    pub(super) fn library_selection(&self) -> usize {
        if self.library.detail.keyboard {
            self.library.detail.row_selected
        } else {
            self.library.selected
        }
    }

    #[observe]
    pub(super) fn selected_platform(&self) -> super::consoles::Platform {
        super::consoles::Platform::from_id(
            self.library
                .entries
                .get(self.library_selection())
                .map(|row| row.icon.as_str())
                .unwrap_or_default(),
        )
    }

    #[observe]
    pub(super) fn library_kind(&self) -> crate::model::LibraryKind {
        self.library.detail.kind
    }

    #[observe]
    pub(super) fn keyboard(&self) -> bool {
        self.library.detail.keyboard
    }

    #[observe]
    pub(super) fn library_counter(&self) -> Option<(usize, usize)> {
        let detail = &self.library.detail;
        if detail.total == 0 || detail.kind == crate::model::LibraryKind::Favorites {
            None
        } else {
            Some((detail.position, detail.total))
        }
    }

    #[observe]
    pub(super) fn recent_sort(&self) -> bool {
        self.library.detail.sort == "recent"
    }

    fn query_value(&self) -> String {
        self.query.clone()
    }

    #[observe]
    pub(super) fn ready(&self) -> bool {
        self.view.ready
    }

    #[observe]
    pub(super) fn busy(&self) -> bool {
        self.view.busy
    }

    #[observe]
    pub(super) fn enabled(&self) -> bool {
        host_action_enabled(&self.view)
    }

    pub(super) fn feedback(&mut self, stamp: Color, activity: [Color; 3]) -> Changes {
        self.stamp = stamp;
        self.activity = activity;
        Changes::NONE
    }

    #[observe]
    pub(super) fn card_motion(&self) -> super::motion::Frame {
        self.motion.cards()
    }

    #[observe]
    pub(super) fn choice_motion(&self) -> super::motion::Frame {
        self.motion.choice()
    }

    #[observe]
    pub(super) fn walker_motion(&self) -> super::motion::Frame {
        self.motion.walker()
    }

    #[observe]
    pub(super) fn actor_position(&self) -> Fixed {
        if matches!(self.view.page, Page::Host | Page::Joined | Page::Leaving) {
            self.motion.host
        } else {
            self.motion.walker
        }
    }

    #[observe]
    pub(super) fn actor_motion(&self) -> super::motion::Frame {
        if matches!(self.view.page, Page::Host | Page::Joined | Page::Leaving) {
            super::motion::Frame {
                page: self.view.page,
                ..self.motion.session()
            }
        } else {
            self.motion.walker()
        }
    }

    #[observe]
    pub(super) fn session_motion(&self) -> super::motion::Frame {
        self.motion.session()
    }

    #[observe]
    pub(super) fn signal_motion(&self) -> super::motion::Frame {
        self.motion.signal()
    }

    #[observe]
    pub(super) fn search_motion(&self) -> super::motion::Frame {
        self.motion.search()
    }

    pub(super) fn press(&mut self, now: u64) -> Changes {
        let now = self
            .animation_gate
            .as_ref()
            .map_or(now, |gate| gate.local(now));
        self.feedback.press(now);
        Changes::NONE
    }

    pub(super) fn animation_counts(&self) -> (u64, u64) {
        self.animation_gate
            .as_ref()
            .map_or((0, 0), |gate| gate.counts())
    }

    pub(super) fn advance_frame(&mut self, now: u64, theme: &mirui::ui::theme::Theme) -> Changes {
        self.frame(now, theme);
        self.launcher.tick(now);
        Changes::NONE
    }

    pub(super) fn frame(&mut self, now: u64, theme: &mirui::ui::theme::Theme) -> Changes {
        self.animate(now, self.feedback.press_sequence);
        let (stamp, activity) = self.feedback.sample(now, loading(&self.view), theme);
        self.feedback(stamp, activity);
        Changes::NONE
    }

    #[observe]
    pub(super) fn can_switch_theme(&self) -> bool {
        self.view.can_switch_theme()
    }

    pub(super) fn animate(&mut self, now: u64, press_sequence: u64) -> Changes {
        let selected = self.selected();
        self.motion = self
            .timeline
            .sample(&self.view, selected, now, press_sequence);
        Changes::NONE
    }
}

pub(super) type DisplayHandle = <DisplayState as mirui::core::model::Model>::Handle;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct PageKey {
    pub(super) library: crate::model::LibraryKind,
    pub(super) page: Page,
    pub(super) network_only: bool,
}

impl DisplayStateHandle {
    pub(super) fn query(&self) -> String {
        self.query_revision();
        self.query_value()
    }
    pub(super) fn room_name(&self) -> String {
        self.room_name_revision();
        self.room_name_value()
    }
    pub(super) fn heading(&self) -> String {
        self.heading_revision();
        self.heading_value()
    }
    pub(super) fn subtitle(&self) -> String {
        self.subtitle_revision();
        self.subtitle_value()
    }
    pub(super) fn status(&self) -> String {
        self.status_revision();
        self.status_value()
    }
    pub(super) fn hint(&self) -> String {
        self.hint_revision();
        self.hint_value()
    }
    pub(super) fn back(&self) -> String {
        self.back_revision();
        self.back_value()
    }
    pub(super) fn headline(&self) -> String {
        self.headline_revision();
        self.headline_value()
    }
    pub(super) fn next(&self) -> String {
        self.next_revision();
        self.next_value()
    }
    pub(super) fn host_button(&self) -> String {
        self.host_button_revision();
        self.host_button_value()
    }
    pub(super) fn peer(&self) -> String {
        self.peer_revision();
        self.peer_value()
    }
    pub(super) fn confirm(&self) -> String {
        self.confirm_revision();
        self.confirm_value()
    }
    pub(super) fn devices(&self) -> Vec<crate::model::NetworkPeer> {
        self.devices_revision();
        self.devices_value()
    }

    pub(super) fn text(&self, field: TextField) -> String {
        match field {
            Headline => self.headline(),
            Next => self.next(),
            HostButton => self.host_button(),
            Peer => self.peer(),
            TextField::RoomName => self.room_name(),
        }
    }
}

pub(super) struct DisplayRuntime {
    pub state: DisplayHandle,
    pub shelf: DisplayHandle,
    pub catalog: DisplayHandle,
    pub room: DisplayHandle,
    pub deck: super::deck::DeckHandle,
    animation_gates: [Rc<super::modal::AnimationGate>; 3],
}

pub(super) fn install<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
    let gates = app
        .world
        .resource::<super::modal::Runtime>()
        .unwrap()
        .gates
        .clone();
    let gates = [
        gates[0].clone(),
        gates[1].clone(),
        Rc::new(super::modal::AnimationGate::default()),
    ];
    let state = register(app, DisplayRole::Primary, None);
    let shelf = register(app, DisplayRole::Shelf, Some(gates[0].clone()));
    let catalog = register(app, DisplayRole::Catalog, Some(gates[1].clone()));
    let room = register(app, DisplayRole::Room, Some(gates[2].clone()));
    app.add_system(advance::system());
    let deck = app.add_model(super::deck::Deck::default()).finish();
    app.add_system(advance_deck::system(deck.clone()));
    app.add_system(super::cache::settled_layer::system());
    app.add_system(selection::update::system());
    app.add_system(super::actor::follow::system());
    app.add_system(super::shelf::layout_shelves::system());
    app.add_resource(DisplayRuntime {
        state,
        shelf,
        catalog,
        room,
        deck,
        animation_gates: gates,
    });
}

fn register<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    role: DisplayRole,
    animation_gate: Option<Rc<super::modal::AnimationGate>>,
) -> DisplayHandle {
    app.add_model(DisplayState {
        role,
        artwork_token: None,
        cover_bounds: super::artwork::CoverBounds::default().into(),
        shelf: super::shelf::Shelf::new(role == DisplayRole::Shelf).into(),
        deck_height: Fixed::ZERO,
        view: View::default(),
        library: LibraryPresentation::default(),
        artwork_revision: 0,
        query: String::new(),
        launcher: crate::launcher::Launcher::default().into(),
        settings: crate::settings::Settings::default().into(),
        direction: PageDirection::Forward,
        rooms: ModelList::bounded(4).into(),
        entries: ModelList::bounded(super::shelf::MAX_VISIBLE).into(),
        row_revision: 0,
        devices: Vec::new(),
        room_name: String::new(),
        heading: String::new(),
        subtitle: String::new(),
        status: String::new(),
        hint: String::new(),
        back: String::new(),
        headline: String::new(),
        next: String::new(),
        host_button: String::new(),
        peer: String::new(),
        confirm: String::new(),
        confirm_visible: false,
        steps: [StepColors {
            background: PANEL,
            foreground: DISABLED,
        }; 3],
        stamp: app.theme().resolve(ACCENT_DIM),
        activity: [app.theme().resolve(BACKGROUND); 3],
        timeline: super::motion::Timeline::default(),
        feedback: crate::feedback::Feedback::default(),
        motion: super::motion::Frame::default(),
        animation_gate,
    })
    .finish()
}

#[system(order = Animation)]
fn advance_deck(deck: model!(super::deck::Deck), clock: res!(optional mirui::ecs::MonoClock)) {
    if let Some(clock) = clock {
        deck.tick(clock.now_ns() / 1_000_000);
    }
}

#[compose]
pub(super) fn display_state() -> DisplayHandle {
    res!(DisplayRuntime).with(|runtime| runtime.state.clone())
}

#[system(order = Animation)]
fn advance(
    runtime: res!(DisplayRuntime),
    clock: res!(optional mirui::ecs::MonoClock),
    theme: res!(mirui::ui::theme::Theme),
    modal: res!(super::modal::Runtime),
    suspension: res!(super::suspension::Runtime),
) {
    if let Some(clock) = clock {
        let now = clock.now_ns() / 1_000_000;
        // The primary model owns the live gesture and shared actor even when
        // retained display layers are hidden or represented by a frozen image.
        runtime.state.advance_frame(now, &theme);
        let mut active = display_activity(&runtime.deck, modal.model.paused());
        active[0] &= !suspension.model.paused();
        for ((state, gate), active) in [
            (&runtime.shelf, &runtime.animation_gates[0]),
            (&runtime.catalog, &runtime.animation_gates[1]),
            (&runtime.room, &runtime.animation_gates[2]),
        ]
        .into_iter()
        .zip(active)
        {
            if let Some(now) = gate.sample(now, !active) {
                state.advance_frame(now, &theme);
            }
        }
    }
}

fn display_activity(deck: &super::deck::DeckHandle, background_paused: bool) -> [bool; 3] {
    use super::deck::Layer;
    if !deck.enabled() {
        return [false; 3];
    }
    [
        !background_paused,
        !background_paused && (deck.expansion() > Fixed::ZERO || deck.layer() == Layer::Catalog),
        deck.panel() > Fixed::ZERO || deck.layer() == Layer::Room,
    ]
}
