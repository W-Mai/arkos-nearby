use mirui::anim::{Spring, SpringConfig};
use mirui::prelude::*;
use mirui::render::command::DrawCommand;
use mirui::types::{Transform, TransformClass};

use super::{
    library::{cartridge_content, cartridge_source, departing_cartridge_content, library_icon},
    reading::cartridge_title,
    shelf::{CARD_INSET, LABEL_HEIGHT, WINDOW_BUDGET},
    style::*,
};
use mirui::ui::{Prop, theme::ThemedColor, widgets::Image};

const ARRIVAL_DELAY_MS: u64 = 86;
const PULL_OUT: SpringConfig = SpringConfig {
    duration_ms: 375,
    bounce: Fixed::from_ratio(1, 8),
};
const PUSH_IN: SpringConfig = SpringConfig {
    duration_ms: 305,
    bounce: Fixed::from_ratio(1, 20),
};
const REFLECTION_PERIOD_MS: u64 = 3800;
const REFLECTION_START_MS: u64 = 1350;
const REFLECTION_DURATION_MS: u64 = 640;
const CONTACT_HEIGHT: i32 = SHELL_FOOT_INSET + SHELL_LIFT;
pub(super) const CONTACT_COUNT: usize = 8;
const FRAME_GAP: i32 = 3;
const SHELL_LIFT_ROOM: i32 = 7;
const SHELL_LIFT: i32 = 4;
const TINY_SHELL_LIFT: i32 = 3;
const SHELL_SIDE_CLEARANCE: i32 = 2;
const ENGRAVING_WIDTH: i32 = 50;
const ENGRAVING_HEIGHT: i32 = 8;
const ENGRAVING_TOP: i32 = 1;
const ENGRAVING_CUT: i32 = 5;
const ENGRAVING_HIGHLIGHT: i32 = 12;
const SHELL_FOOT_INSET: i32 = 2;

pub(super) struct SlotLayout {
    pub frame_height: i32,
    shell: Rect,
}

impl SlotLayout {
    pub fn new(plan: super::shelf::Plan, stroke: i32) -> Self {
        let frame_height = plan.card_height - super::shelf::SHADOW;
        let inset = stroke + FRAME_GAP;
        let shell_left = stroke + SHELL_SIDE_CLEARANCE;
        let shell_top = inset + SHELL_LIFT_ROOM;
        let contacts_floor = frame_height - inset;
        Self {
            frame_height,
            shell: Rect::new(
                shell_left,
                shell_top,
                (plan.card_width - 2 * shell_left).max(0),
                (contacts_floor - SHELL_FOOT_INSET - shell_top).max(0),
            ),
        }
    }

    pub fn shell_rect(&self) -> Rect {
        self.shell
    }
}

#[component]
pub(super) struct Shell;

#[component]
pub(super) struct Seat {
    pub grid: model!(super::shelf::Shelf),
    pub row: model!(super::rows::Row),
    pub departing: bool,
    pub stroke: i32,
}

#[view(component = Seat, priority = 50, invalidation = Visual)]
pub(super) fn seat(
    seat: component!(),
    grid: model!(component.grid, watch(plan, poses, departing_poses)),
    row: model!(component.row, watch(index)),
    theme: res!(mirui::ui::theme::Theme),
    paint: paint!(),
) {
    let layout = SlotLayout::new(grid.plan, seat.stroke);
    let pose = grid.card_pose(row.index, seat.departing);
    let shell = layout.shell_rect();
    let rect = paint.rect();
    let transform = paint.transform();
    let complex = !matches!(
        transform.classify(),
        TransformClass::Identity | TransformClass::Translate
    );

    let mut fill = |area: Rect, color, radius| {
        paint.draw(&DrawCommand::Fill {
            area,
            transform,
            quad: complex.then(|| transform.apply_rect(area)),
            color,
            radius,
            opa: 255,
        });
    };
    let clearance = pose.clearance();
    let shadow = Rect::new(
        rect.x + shell.x + Fixed::from_int(7) - clearance * 2,
        rect.y + shell.y + shell.h - Fixed::ONE,
        (shell.w - Fixed::from_int(14) + clearance * 4).max(Fixed::ZERO),
        Fixed::from_int(3) + clearance * 3,
    );
    fill(shadow, theme.resolve(CARTRIDGE_PENUMBRA), 2.into());
    fill(
        Rect::new(
            shadow.x + Fixed::from_int(3),
            shadow.y + Fixed::ONE,
            (shadow.w - Fixed::from_int(6)).max(Fixed::ZERO),
            2,
        ),
        theme.resolve(CARTRIDGE_SHADOW),
        Fixed::ONE,
    );
}

#[component]
struct Contacts {
    grid: model!(super::shelf::Shelf),
    row: model!(super::rows::Row),
    departing: bool,
    small: bool,
}

#[view(component = Contacts, invalidation = Visual)]
pub(super) fn contacts(
    contact: component!(),
    grid: model!(component.grid, watch(plan, poses, departing_poses)),
    row: model!(component.row, watch(index)),
    theme: res!(mirui::ui::theme::Theme),
    paint: paint!(),
) {
    let pose = grid.card_pose(row.index, contact.departing);
    let height = pose.exposed_contacts(contact.small);
    const GAP: Fixed = Fixed::from_int(2);
    const COUNT: i32 = CONTACT_COUNT as i32;
    let shell = paint.rect();
    let width = Fixed::from_int(grid.plan.card_width) * 64 / 100;
    let area = Rect::new(shell.x + (shell.w - width) / 2, shell.y, width, height);
    let metal_width = area.w - GAP * (COUNT - 1);
    if height <= Fixed::ZERO || metal_width <= Fixed::ZERO {
        return;
    }

    let transform = paint.transform();
    let complex = !matches!(
        transform.classify(),
        TransformClass::Identity | TransformClass::Translate
    );
    let mut fill = |area: Rect, color| {
        paint.draw(&DrawCommand::Fill {
            area,
            transform,
            quad: complex.then(|| transform.apply_rect(area)),
            color,
            radius: Fixed::ZERO,
            opa: 255,
        });
    };
    fill(area, theme.resolve(CARTRIDGE_RECESS));
    for index in 0..COUNT {
        let start = metal_width * index / COUNT;
        let end = metal_width * (index + 1) / COUNT;
        let color = if pose.reflection() == Some(index as u8) {
            CARTRIDGE_REFLECTION
        } else {
            CARTRIDGE_BRASS
        };
        fill(
            Rect::new(area.x + start + GAP * index, area.y, end - start, height),
            color,
        );
    }
}

#[derive(Clone)]
pub(super) struct Label {
    title: Prop<String>,
    paper: Prop<Fixed>,
    scroll: Prop<bool>,
    ink: Prop<ThemedColor>,
}

impl Label {
    pub fn new(
        row: super::rows::RowHandle,
        state: super::state::DisplayHandle,
        grid: super::shelf::ShelfHandle,
        departing: bool,
    ) -> Self {
        let title = row.clone();
        let paper = row.clone();
        let ink = row.clone();
        Self {
            title: Prop::new(move || title.label(true)),
            paper: Prop::new(move || grid.pose(paper.index(), departing).paper()),
            scroll: Prop::new(move || {
                !departing && !row.directory() && state.selected() == Some(row.index())
            }),
            ink: Prop::new(move || {
                ThemedColor::from(if ink.available() { SHELL_TEXT } else { DIVIDER })
            }),
        }
    }

    pub fn captured(title: String, paper: Fixed) -> Self {
        Self {
            title: title.into(),
            paper: paper.into(),
            scroll: false.into(),
            ink: ThemedColor::from(SHELL_TEXT).into(),
        }
    }
}

pub(super) enum CartridgeFace {
    Live(
        super::state::DisplayHandle,
        super::rows::RowHandle,
        mirui::core::model::ModelItemId,
    ),
    Departing(super::state::DisplayHandle, super::rows::RowHandle),
    Flip,
}

#[compose]
#[allow(clippy::too_many_arguments)]
pub(super) fn front(
    placement: Option<Prop<Rect>>,
    shell: prop!(Transform),
    pose: prop!(Pose),
    bounds: model!(super::artwork::CoverBounds),
    platform: prop!(super::consoles::Platform),
    face: CartridgeFace,
    directory: prop!(bool),
    metrics: Metrics,
    label: Label,
) -> Entity {
    let on_shelf = placement.is_some();
    let measure = matches!(&face, CartridgeFace::Live(..));
    let contact = match &face {
        CartridgeFace::Live(state, row, _) | CartridgeFace::Departing(state, row) => {
            Some(Contacts {
                grid: state.shelf(),
                row: row.clone(),
                departing: matches!(&face, CartridgeFace::Departing(..)),
                small: metrics.tiny,
            })
        }
        CartridgeFace::Flip => None,
    };
    let cover = pose.clone();
    let placement = placement.unwrap_or_else(|| Rect::ZERO.into());
    let left = placement.clone();
    let top = placement.clone();
    let width = placement.clone();
    ui! {
        Column (
            position: if on_shelf { Position::Absolute } else { Position::Flex },
            grow: if on_shelf { 0.0 } else { 1.0 },
            top: ${ top.get().y }, left: ${ left.get().x },
            width: ${ if on_shelf { Dimension::Px(width.get().w) } else { Dimension::Auto } },
            height: ${ if on_shelf { Dimension::Px(placement.get().h) } else { Dimension::Auto } },
            min_width: 0, min_height: 0, container: true,
            padding: Padding::all(CARD_INSET),
            bg_color: CARTRIDGE_BODY, border_width: 2, border_color: CARTRIDGE_RIM,
            transform: ${ shell.get() }
        ) [Shell] {
            if measure {
                View (position: Position::Absolute, width: 0, height: 0) [
                    super::material::MeasureCard(true),
                    mirui::ui::IgnoreHitTest,
                ]
            }
            if ${ directory.get() } {
                library_icon (metrics)
            } else {
                View (grow: 1.0, min_width: 0, min_height: 0, clip_children: true) {
                    Column (grow: 1.0, min_width: 0, min_height: 0, transform: ${ cover.get().cover() }) {
                        match face {
                            CartridgeFace :: Live(state, row, member) => {
                                cartridge_content (state, row, member)
                            }
                            CartridgeFace :: Departing(_state, row) => {
                                departing_cartridge_content (row)
                            }
                            CartridgeFace :: Flip => {
                                cartridge_source (bounds, platform)
                            }
                        }
                    }
                }
                engraving (pose, on_shelf, metrics.tiny)
                walk contact.into_iter() with contact {
                    View (
                        position: Position::Absolute,
                        left: 0, top: Dimension::percent(100),
                        width: Dimension::percent(100), height: CONTACT_HEIGHT
                    ) [contact, mirui::ui::IgnoreHitTest]
                }
            }
            cartridge_label (label, metrics)
        }
    }
}

#[compose]
fn engraving(pose: prop!(Pose), on_shelf: bool, small: bool) -> Entity {
    let sheen = pose.clone();
    let glint = pose.clone();
    let rim = pose.clone();
    let recess = pose.clone();
    ui! {
        Row (
            position: Position::Absolute, left: 0, top: ENGRAVING_TOP,
            width: Dimension::percent(100), height: ENGRAVING_HEIGHT,
            justify: JustifyContent::FlexEnd,
            padding: Padding { right: 4.into(), ..Default::default() }
        ) [mirui::ui::IgnoreHitTest] {
            View (
                width: ENGRAVING_WIDTH, height: ENGRAVING_HEIGHT,
                container: true
            ) [mirui::ui::IgnoreHitTest] {
                Image (
                    position: Position::Absolute, top: 0, left: 0,
                    width: ENGRAVING_WIDTH, height: ENGRAVING_WIDTH, viewbox: ENGRAVING_WIDTH,
                    color: CARTRIDGE_RIM,
                    transform: ${ rim.get().engraving_backing(on_shelf, small) },
                    src: path!(M 0 4 L 4 0 H 46 L 50 4 V 8 H 0 Z)
                ) [mirui::ui::IgnoreHitTest]
                Image (
                    position: Position::Absolute, top: 0, left: 0,
                    width: ENGRAVING_WIDTH, height: ENGRAVING_WIDTH, viewbox: ENGRAVING_WIDTH,
                    color: CARTRIDGE_RECESS,
                    transform: ${ recess.get().engraving_backing(on_shelf, small) },
                    src: path!(M 1 4.5 L 4.5 1 H 45.5 L 49 4.5 V 7 H 1 Z)
                ) [mirui::ui::IgnoreHitTest]
                Image (
                    position: Position::Absolute, top: 0, left: 0,
                    width: ENGRAVING_WIDTH, height: ENGRAVING_WIDTH, viewbox: ENGRAVING_WIDTH,
                    color: CARTRIDGE_TRIM,
                    transform: ${ pose.get().engraving(on_shelf, small) },
                    src: path!(
                        M 3 2 H 16 L 13 6 H 0 Z M 20 2 H 29 L 26 6 H 17 Z
                        M 33 2 H 39 L 36 6 H 30 Z
                    )
                )
                Image (
                    width: ENGRAVING_WIDTH, height: ENGRAVING_WIDTH, viewbox: ENGRAVING_WIDTH,
                    position: Position::Absolute, top: 0, left: 0, color: CARTRIDGE_REFLECTION,
                    visible: ${ sheen.get().reflection().is_some() },
                    transform: ${ glint.get().engraving_glint(on_shelf, small) },
                    src: path!(M 3 2 H 11 L 8 6 H 0 Z)
                )
            }
        }
    }
}

#[component]
pub(super) struct PaperCorner;

#[compose]
fn cartridge_label(label: Label, metrics: Metrics) -> Entity {
    let Label {
        title,
        paper,
        scroll,
        ink,
    } = label;
    let paper_title = title.clone();
    let visibility = paper.clone();
    let inset = metrics.cartridge_title_padding();
    let shell = ThemedColor::from(CARTRIDGE_BODY);
    let paper_color = ThemedColor::from(CARTRIDGE_PAPER);
    let paper_ink = ThemedColor::from(CARTRIDGE_INK);
    ui! {
        Row (
            height: LABEL_HEIGHT, shrink: 0.0, min_width: 0,
            container: true, padding: inset
        ) {
            cartridge_title (title, scroll.clone(), ink, shell)
            View (
                position: Position::Absolute, top: 0, left: 0,
                width: Dimension::percent(100), height: Dimension::percent(100),
                visible: ${ visibility.get() > Fixed::ZERO }
            ) {
                Row (
                    position: Position::Absolute, top: 0, left: 0,
                    width: ${ Dimension::Percent(paper.get() * 100) },
                    height: Dimension::percent(100), clip_children: true
                ) {
                    Row (
                        position: Position::Absolute, top: 0, left: 0,
                        width: @(width as full) { full },
                        height: Dimension::percent(100),
                        padding: inset, bg_color: CARTRIDGE_PAPER
                    ) {
                        cartridge_title (paper_title, scroll, paper_ink, paper_color)
                        Image (
                            src: path!(M 0 0 H 4 V 4 Z), viewbox: 4,
                            position: Position::Absolute, top: 0,
                            left: @(width as full) { full - Fixed::from_int(4) },
                            width: 4, height: 4, color: CARTRIDGE_BODY
                        ) [mirui::ui::IgnoreHitTest, PaperCorner]
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Pose {
    pull: Fixed,
    side: i32,
    shimmer: Option<u8>,
}

fn settled_lift(small: bool) -> i32 {
    if small { TINY_SHELL_LIFT } else { SHELL_LIFT }
}

fn ease(from: i32, to: i32, position: Fixed) -> Fixed {
    let from = Fixed::from_ratio(from, 100);
    let to = Fixed::from_ratio(to, 100);
    super::motion::smoothstep((position - from) / (to - from))
}

impl Pose {
    pub fn shell(self, small: bool) -> Transform {
        let swing = self.swing();
        let bank = (swing * Fixed::from_ratio(9, 10) + self.press() / 3) * self.side;
        Transform::translate(swing * self.side / 2, -self.lift(small))
            .compose(&Transform::rotate_deg(bank))
    }

    pub fn cover(self) -> Transform {
        let clearance = self.clearance();
        let scale = Fixed::ONE + clearance / 40;
        Transform::translate((clearance + self.swing() / 2) * self.side, Fixed::ZERO)
            .compose(&Transform::scale(scale, scale))
    }

    fn engraving_rise(self, on_shelf: bool, small: bool) -> Fixed {
        if on_shelf {
            self.clearance() * (ENGRAVING_TOP + SHELL_LIFT_ROOM - settled_lift(small))
        } else {
            Fixed::ZERO
        }
    }

    fn engraving_backing(self, on_shelf: bool, small: bool) -> Transform {
        let rise = self.engraving_rise(on_shelf, small);
        let scale = rise / ENGRAVING_HEIGHT;
        // Mirui scales the square vector image about its center. Keep the
        // visible plate's lower edge in place while its beveled top rises.
        Transform::translate(Fixed::ZERO, scale * (ENGRAVING_WIDTH / 2) - rise)
            .compose(&Transform::scale(Fixed::ONE, Fixed::ONE + scale))
    }

    fn engraving(self, on_shelf: bool, small: bool) -> Transform {
        Transform::translate(
            self.clearance() * ENGRAVING_CUT,
            -self.engraving_rise(on_shelf, small) / 2,
        )
    }

    fn engraving_glint(self, on_shelf: bool, small: bool) -> Transform {
        let phase = i32::from(self.shimmer.unwrap_or_default());
        let span = ENGRAVING_WIDTH - ENGRAVING_HIGHLIGHT;
        Transform::translate(
            Fixed::from_ratio(phase * span, CONTACT_COUNT as i32 - 1),
            -self.engraving_rise(on_shelf, small) / 2,
        )
    }

    pub fn paper(self) -> Fixed {
        ease(33, 78, self.pull)
    }

    fn press(self) -> Fixed {
        ease(0, 16, self.pull) * (Fixed::ONE - ease(16, 43, self.pull))
    }

    fn lift(self, small: bool) -> Fixed {
        self.clearance() * settled_lift(small) + self.swing() * 2
            - self.press() * Fixed::from_ratio(4, 5)
    }

    fn exposed_contacts(self, small: bool) -> Fixed {
        (self.lift(small) + self.clearance() * SHELL_FOOT_INSET).clamp(
            Fixed::ZERO,
            Fixed::from_int(SHELL_FOOT_INSET + settled_lift(small)),
        )
    }

    pub fn without_reflection(mut self, departing: bool) -> Self {
        if departing {
            self.shimmer = None;
        }
        self
    }

    pub fn reflection(self) -> Option<u8> {
        self.shimmer
    }

    fn clearance(self) -> Fixed {
        ease(15, 92, self.pull)
    }

    fn swing(self) -> Fixed {
        ease(14, 60, self.pull) * (Fixed::ONE - ease(62, 100, self.pull))
    }
}

#[derive(Clone, Copy)]
struct Travel {
    spring: Spring,
    side: i32,
    moving: bool,
}

impl Travel {
    fn seated() -> Self {
        Self {
            spring: Spring::preset(Fixed::ZERO, Fixed::ZERO, PUSH_IN),
            side: 1,
            moving: false,
        }
    }

    fn aim(&mut self, target: Fixed, config: SpringConfig) {
        if self.spring.target != target {
            self.spring.retarget(target, Some(config));
            self.moving = true;
        }
    }
}

pub(super) struct Cartridges {
    slots: [Travel; WINDOW_BUDGET],
    selection: Option<(usize, usize)>,
    count: usize,
    engage_at: u64,
    held_since: Option<u64>,
}

impl Default for Cartridges {
    fn default() -> Self {
        Self {
            slots: [Travel::seated(); WINDOW_BUDGET],
            selection: None,
            count: 0,
            engage_at: 0,
            held_since: None,
        }
    }
}

impl Cartridges {
    pub fn settled(&self) -> bool {
        self.selection.is_some_and(|(_, selected)| {
            self.slots[selected].spring.target == Fixed::ONE
                && self.slots[..self.count].iter().all(|slot| !slot.moving)
        })
    }

    pub fn set_count(&mut self, count: usize) {
        let count = count.min(self.slots.len());
        if count < self.count {
            self.slots[count..self.count].fill(Travel::seated());
            if self
                .selection
                .is_some_and(|(_, selected)| selected >= count)
            {
                self.selection = None;
            }
        }
        self.count = count;
    }

    pub fn select(&mut self, page: usize, index: usize, now: u64) -> bool {
        if index >= self.slots.len() || self.selection == Some((page, index)) {
            return false;
        }
        let next = &mut self.slots[index];
        self.held_since = None;
        self.engage_at = now;
        if let Some((_, old_index)) = self.selection.filter(|(old_page, _)| *old_page == page) {
            if !next.moving {
                self.engage_at += ARRIVAL_DELAY_MS;
                next.side = if index < old_index { -1 } else { 1 };
            }
            self.slots[old_index].aim(Fixed::ZERO, PUSH_IN);
        } else {
            self.slots[..self.count].fill(Travel::seated());
            self.slots[index].spring = Spring::preset(Fixed::ONE, Fixed::ONE, PULL_OUT);
        }
        self.selection = Some((page, index));
        true
    }

    pub fn step(&mut self, now: u64, dt: u16, reflect: bool) -> [Pose; WINDOW_BUDGET] {
        let mut poses = [Pose::default(); WINDOW_BUDGET];
        let Some((_, selected)) = self.selection else {
            return poses;
        };
        if now >= self.engage_at {
            self.slots[selected].aim(Fixed::ONE, PULL_OUT);
        }
        for (i, (slot, pose)) in self.slots[..self.count]
            .iter_mut()
            .zip(&mut poses)
            .enumerate()
        {
            if slot.moving {
                slot.spring.tick(dt);
                slot.moving = !slot.spring.is_settled();
                if !slot.moving {
                    let endpoint = slot.spring.target;
                    slot.spring = Spring::preset(endpoint, endpoint, PUSH_IN);
                }
            }
            let pull = if slot.moving {
                slot.spring.position
            } else {
                slot.spring.target
            };
            if pull > Fixed::ZERO {
                *pose = Pose {
                    pull,
                    side: slot.side,
                    shimmer: None,
                };
            }
            if i == selected && reflect && slot.spring.target == Fixed::ONE && !slot.moving {
                let elapsed = (now - *self.held_since.get_or_insert(now)) % REFLECTION_PERIOD_MS;
                if (REFLECTION_START_MS..REFLECTION_START_MS + REFLECTION_DURATION_MS)
                    .contains(&elapsed)
                {
                    let phase = ((elapsed - REFLECTION_START_MS) * CONTACT_COUNT as u64
                        / REFLECTION_DURATION_MS) as u8;
                    pose.shimmer = Some(phase);
                }
            } else if i == selected {
                self.held_since = None;
            }
        }
        poses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seated_hardware_retracts_and_focused_hardware_shares_one_frame_inset() {
        for small in [false, true] {
            let stroke = if small { 3 } else { 4 };
            let plan = super::super::shelf::Plan {
                card_width: 186,
                card_height: 170,
                ..Default::default()
            };
            let slot = SlotLayout::new(plan, stroke);
            let shell = slot.shell_rect();
            let parked = Pose::default();
            assert_eq!(parked.engraving_rise(true, small), Fixed::ZERO);
            assert_eq!(parked.exposed_contacts(small), Fixed::ZERO);

            let selected = Pose {
                pull: Fixed::ONE,
                ..parked
            };
            let shift = selected.shell(small).ty;
            let leading_edge = shell.y + Fixed::from_int(ENGRAVING_TOP) + shift
                - selected.engraving_rise(true, small);
            let trailing_edge = shell.y + shell.h + shift + selected.exposed_contacts(small);
            let inset = Fixed::from_int(stroke + FRAME_GAP);
            assert_eq!(leading_edge, inset);
            assert_eq!(Fixed::from_int(slot.frame_height) - trailing_edge, inset);
        }
    }

    #[test]
    fn reversing_a_cartridge_keeps_the_spring_velocity_and_skips_intermediate_cards() {
        let mut travel = Cartridges::default();
        travel.set_count(2);
        assert!(travel.select(0, 0, 0));
        assert!(travel.select(0, 1, 16));
        let before = travel.step(64, 48, false);
        let speed = travel.slots[0].spring.velocity;
        assert!(before[0].pull < Fixed::ONE && speed < Fixed::ZERO);
        assert!(travel.select(0, 0, 64));
        let redirected = travel.step(64, 0, false);
        assert!(before[0] == redirected[0]);
        assert_eq!(travel.slots[0].spring.velocity, speed);
        assert!(travel.step(68, 4, false)[0].pull < before[0].pull);
        let mut settled = before;
        for ms in (72..=880).step_by(16) {
            settled = travel.step(ms, 16, false);
            assert!(settled[1] == Pose::default());
        }
        assert_eq!(settled[0].pull, Fixed::ONE);
        assert_eq!(
            settled[0].exposed_contacts(false),
            Fixed::from_int(CONTACT_HEIGHT)
        );
    }
}
