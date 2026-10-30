use crate::model::Action;

const REPEAT_DELAY_MS: u64 = 350;
const REPEAT_INTERVAL_MS: u64 = 100;

#[derive(Default)]
pub(super) struct Controls {
    held: Vec<(u16, Option<Action>, u64)>,
}

impl Controls {
    pub fn reset(&mut self, pressed: impl Iterator<Item = u16>) {
        self.held = pressed.map(|code| (code, None, 0)).collect();
    }

    pub fn update(&mut self, source: u16, action: Option<Action>, at: u64) -> Option<Action> {
        if let Some(index) = self.held.iter().position(|entry| entry.0 == source) {
            if action.is_some() && (self.held[index].1 == action || self.held[index].1.is_none()) {
                return None;
            }
            self.held.remove(index);
        }
        if action.is_some() {
            self.held.push((source, action, at + REPEAT_DELAY_MS));
        }
        action
    }

    pub fn repeat(&mut self, at: u64) -> Vec<Action> {
        self.held
            .iter_mut()
            .filter_map(|(_, action, deadline)| {
                if at >= *deadline
                    && matches!(
                        action,
                        Some(
                            Action::Up
                                | Action::Down
                                | Action::Left
                                | Action::Right
                                | Action::PageUp
                                | Action::PageDown
                        )
                    )
                {
                    *deadline = at + REPEAT_INTERVAL_MS;
                    *action
                } else {
                    None
                }
            })
            .collect()
    }
}

pub(super) fn button(code: u16) -> Option<Action> {
    Some(match code {
        544 => Action::Up,
        545 => Action::Down,
        546 => Action::Left,
        547 => Action::Right,
        305 => Action::Confirm,
        304 => Action::Back,
        308 => Action::Theme,
        310 | 312 => Action::PageUp,
        311 | 313 => Action::PageDown,
        704 => Action::LauncherHome,
        705 => Action::Start,
        _ => return None,
    })
}

pub(super) fn button_for(code: u16, launcher: bool) -> Option<Action> {
    button(code).map(|action| {
        if !launcher && action == Action::LauncherHome {
            Action::Back
        } else {
            action
        }
    })
}

pub(super) fn axis(code: u16, value: i32, min: i32, max: i32) -> Option<Action> {
    let center = (i64::from(min) + i64::from(max)) / 2;
    let deadzone = (i64::from(max) - i64::from(min)) / 4;
    let offset = i64::from(value) - center;
    if max <= min || offset.abs() <= deadzone {
        return None;
    }
    Some(match (code, offset < 0) {
        (0 | 3, true) => Action::Left,
        (0 | 3, false) => Action::Right,
        (1 | 4, true) => Action::Up,
        (1 | 4, false) => Action::Down,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_edges_repeat_only_navigation_and_resync_waits_for_release() {
        let mut controls = Controls::default();
        controls.reset([547].into_iter());
        assert_eq!(controls.update(547, button(547), 0), None);
        assert!(controls.repeat(1000).is_empty());
        controls.update(547, None, 1001);
        assert_eq!(controls.update(547, button(547), 1010), Some(Action::Right));
        assert_eq!(controls.update(547, button(547), 1020), None);
        assert!(controls.repeat(1359).is_empty());
        assert_eq!(controls.repeat(1360), vec![Action::Right]);
        controls.update(547, None, 1361);
        assert_eq!(
            controls.update(305, button(305), 1400),
            Some(Action::Confirm)
        );
        assert!(controls.repeat(3000).is_empty());
        assert_eq!(axis(0, 2048, 0, 4095), None);
        assert_eq!(axis(0, 4095, 0, 4095), Some(Action::Right));
        assert_eq!(axis(1, -32768, -32768, 32767), Some(Action::Up));
        assert_eq!(axis(3, 0, 0, 4095), Some(Action::Left));
        assert_eq!(button(704), Some(Action::LauncherHome));
        assert_eq!(button(308), Some(Action::Theme));
    }

    #[test]
    fn room_and_launcher_share_physical_controls_with_entry_scoped_select() {
        for code in 0..768 {
            if code == 704 {
                assert_eq!(button_for(code, false), Some(Action::Back));
                assert_eq!(button_for(code, true), Some(Action::LauncherHome));
            } else {
                assert_eq!(button_for(code, false), button_for(code, true));
            }
        }
    }
}
