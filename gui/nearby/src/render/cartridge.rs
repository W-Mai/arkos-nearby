use mirui::anim::{Spring, SpringConfig};
use mirui::prelude::*;
use mirui::types::Transform;

use super::shelf::WINDOW_BUDGET;

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
pub(super) const CONTACT_HEIGHT: i32 = 6;
pub(super) const CONTACT_COUNT: usize = 8;
const FRAME_GAP: i32 = 3;
const SHELL_LIFT_ROOM: i32 = 8;
const SHELL_SIDE_ROOM: i32 = 3;
const SHELL_FOOT_INSET: i32 = 2;

pub(super) struct SlotLayout {
    pub frame_height: i32,
    pub shell_left: i32,
    pub shell_top: i32,
    pub shell_width: i32,
    pub shell_height: i32,
    pub contacts_top: i32,
}

impl SlotLayout {
    pub fn new(plan: super::shelf::Plan, stroke: i32) -> Self {
        let frame_height = plan.card_height - super::shelf::SHADOW;
        let inset = stroke + FRAME_GAP;
        let shell_left = inset + SHELL_SIDE_ROOM;
        let shell_top = inset + SHELL_LIFT_ROOM;
        let contacts_floor = frame_height - inset;
        Self {
            frame_height,
            shell_left,
            shell_top,
            shell_width: (plan.card_width - 2 * shell_left).max(0),
            shell_height: (contacts_floor - SHELL_FOOT_INSET - shell_top).max(0),
            contacts_top: (contacts_floor - CONTACT_HEIGHT).max(0),
        }
    }
}

#[component]
pub(super) struct Shell;

#[component]
pub(super) struct Contacts;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Pose {
    pull: Fixed,
    side: i32,
    shimmer: Option<u8>,
}

fn ease(from: i32, to: i32, position: Fixed) -> Fixed {
    let from = Fixed::from_ratio(from, 100);
    let to = Fixed::from_ratio(to, 100);
    let t = ((position - from) / (to - from))
        .max(Fixed::ZERO)
        .min(Fixed::ONE);
    t * t * (Fixed::from_int(3) - t * 2)
}

impl Pose {
    pub fn shell(self, small: bool) -> Transform {
        let press = ease(0, 16, self.pull) * (Fixed::ONE - ease(16, 43, self.pull));
        let lift = self.clearance() * if small { 3 } else { 4 };
        let swing = self.swing();
        let height = lift + swing * 2 - press * Fixed::from_ratio(4, 5);
        let bank = (swing * Fixed::from_ratio(9, 10) + press / 3) * self.side;
        Transform::translate(swing * self.side / 2, -height).compose(&Transform::rotate_deg(bank))
    }

    pub fn cover(self) -> Transform {
        let clearance = self.clearance();
        let scale = Fixed::ONE + clearance / 40;
        Transform::translate((clearance + self.swing() / 2) * self.side, Fixed::ZERO)
            .compose(&Transform::scale(scale, scale))
    }

    pub fn paper(self) -> Fixed {
        ease(33, 78, self.pull)
    }

    pub fn contact_offset(self) -> Transform {
        Transform::translate(
            Fixed::ZERO,
            (Fixed::ONE - ease(25, 88, self.pull)) * CONTACT_HEIGHT,
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
        assert_eq!(settled[0].contact_offset(), Transform::IDENTITY);
    }
}
