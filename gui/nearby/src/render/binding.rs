use super::style::Metrics;
use crate::model::{LibraryKind, Page, View};
use mirui::anim::{Tween, TweenConfig, ease};
use mirui::prelude::*;
use mirui::types::Transform;

const LABEL: TweenConfig = TweenConfig::new(280, &ease::IN_OUT_CUBIC);
const ATTACK: TweenConfig = TweenConfig::new(82, &ease::OUT_CUBIC);
const SETTLE: TweenConfig = TweenConfig::new(185, &ease::OUT_CUBIC);

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Pose {
    seal: Fixed,
    lean: Fixed,
}
impl Pose {
    pub(super) fn mounted(self) -> bool {
        self.seal > Fixed::ZERO
    }
    pub(super) fn tilt(self) -> Transform {
        Transform::rotate_deg(self.lean * 4)
    }
    fn lift(self) -> Fixed {
        self.seal * (Fixed::ONE - self.seal) * 4
    }
    pub(super) fn label(self, metrics: Metrics) -> Transform {
        let lift = self.lift();
        Transform {
            ty: -lift * Fixed::from_int(metrics.gap + 2),
            tx: lift * Fixed::from_int(metrics.gap),
            ..Transform::rotate_deg(lift * -3).compose(&Transform::scale(
                Fixed::ONE - self.seal * Fixed::from_ratio(8, 100),
                Fixed::ONE - self.seal * Fixed::from_ratio(10, 100),
            ))
        }
    }
    pub(super) fn folded(self) -> bool {
        self.lift() > Fixed::ZERO
    }
    pub(super) fn fold(self) -> Transform {
        let lift = self.lift();
        Transform {
            m00: lift,
            m11: lift,
            ..Transform::IDENTITY
        }
    }
}

pub(super) struct Sleeve {
    key: String,
    position: usize,
    kind: Option<LibraryKind>,
    seal: Tween,
    lean: Tween,
    attacking: bool,
    last: Option<u64>,
}
impl Default for Sleeve {
    fn default() -> Self {
        Self {
            key: String::new(),
            position: 0,
            kind: None,
            seal: Tween::preset(Fixed::ZERO, Fixed::ZERO, LABEL),
            lean: Tween::preset(Fixed::ZERO, Fixed::ZERO, SETTLE),
            attacking: false,
            last: None,
        }
    }
}
impl Sleeve {
    pub(super) fn pose(&self) -> Pose {
        Pose {
            seal: self.seal.value(),
            lean: self.lean.value(),
        }
    }
    pub(super) fn present(&mut self, v: &View) {
        let selection = (v.page == Page::Games
            && matches!(v.library.kind, LibraryKind::Directory | LibraryKind::Search)
            && !v.library.keyboard
            && !v.launcher.navigation
            && v.launcher.overlay.is_empty())
        .then(|| v.entries.get(v.selected))
        .flatten()
        .filter(|r| !r.directory && !r.key.is_empty());
        let Some(row) = selection else {
            if self.kind.is_some() {
                *self = Self::default();
            }
            return;
        };
        let target = if row.favorite {
            Fixed::ONE
        } else {
            Fixed::ZERO
        };
        if self.key == row.key && self.kind == Some(v.library.kind) {
            if self.seal.to != target {
                self.seal.retarget(target, LABEL);
            }
            return;
        }
        let position = if v.library.position > 0 {
            v.library.position
        } else {
            v.selected + 1
        };
        let direction = if self.kind == Some(v.library.kind) {
            match position.cmp(&self.position) {
                std::cmp::Ordering::Greater => Fixed::ONE,
                std::cmp::Ordering::Less => -Fixed::ONE,
                _ => Fixed::ZERO,
            }
        } else {
            Fixed::ZERO
        };
        self.attacking = direction != Fixed::ZERO;
        self.lean
            .retarget(direction, if self.attacking { ATTACK } else { SETTLE });
        self.seal = Tween::preset(target, target, LABEL);
        self.kind = Some(v.library.kind);
        self.position = position;
        row.key.clone_into(&mut self.key);
    }
    fn idle(&self) -> bool {
        !self.attacking && self.seal.is_finished() && self.lean.is_finished()
    }
    pub(super) fn tick(&mut self, now: u64) -> bool {
        let Some(last) = self.last.replace(now) else {
            return false;
        };
        if self.idle() {
            return false;
        }
        let dt = now.saturating_sub(last).min(u64::from(u16::MAX)) as u16;
        let before = self.pose();
        self.seal.tick(dt);
        let remaining = self.lean.duration_ms - self.lean.elapsed_ms;
        self.lean.tick(dt);
        if self.attacking && self.lean.is_finished() {
            self.attacking = false;
            self.lean.retarget(Fixed::ZERO, SETTLE);
            self.lean.tick(dt.saturating_sub(remaining));
        }
        self.pose() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GameRow;
    fn catalog() -> View {
        View {
            page: Page::Games,
            entries: ["a", "b", "c"]
                .map(|key| GameRow {
                    key: key.to_owned(),
                    ..GameRow::default()
                })
                .into(),
            ..View::default()
        }
    }
    #[test]
    fn reversal_uses_displayed_pose_and_only_selected_persistent_state() {
        let mut view = catalog();
        let mut sleeve = Sleeve::default();
        sleeve.present(&view);
        assert!(sleeve.idle());
        view.entries[0].favorite = true;
        sleeve.present(&view);
        sleeve.tick(5);
        sleeve.tick(145);
        let middle = sleeve.pose();
        assert!(middle.folded());
        sleeve.present(&view);
        assert_eq!(middle, sleeve.pose());
        view.entries[0].favorite = false;
        sleeve.present(&view);
        assert_eq!(middle, sleeve.pose());
        sleeve.tick(185);
        assert!(sleeve.pose().seal < middle.seal);
        view.selected = 1;
        sleeve.present(&view);
        assert_eq!(sleeve.pose(), Pose::default());
        sleeve.tick(215);
        let turn = sleeve.pose();
        assert!(turn.lean > Fixed::ZERO);
        view.selected = 0;
        sleeve.present(&view);
        assert_eq!(sleeve.pose(), turn);
        sleeve.tick(297);
        assert_eq!(sleeve.pose().lean, -Fixed::ONE);
        sleeve.tick(500);
        assert!(sleeve.idle());
        assert_eq!(sleeve.pose(), Pose::default());
        assert!(!sleeve.tick(516));
    }
    #[test]
    fn keyboard_overlay_and_directory_clear_departed_motion() {
        let mut v = catalog();
        v.library.kind = LibraryKind::Search;
        v.entries[0].favorite = true;
        let mut s = Sleeve::default();
        s.present(&v);
        assert_eq!(s.pose().seal, Fixed::ONE);
        assert!(s.idle());
        v.library.keyboard = true;
        s.present(&v);
        assert_eq!(s.pose(), Pose::default());
        v.library.keyboard = false;
        s.present(&v);
        assert_eq!(s.pose().seal, Fixed::ONE);
        v.launcher.navigation = true;
        s.present(&v);
        assert_eq!(s.pose(), Pose::default());
        v.launcher.navigation = false;
        v.entries[0].directory = true;
        s.present(&v);
        assert!(s.idle());
        assert_eq!(s.pose(), Pose::default());
    }
}
