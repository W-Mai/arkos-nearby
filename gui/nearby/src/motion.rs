use crate::model::Page;
use crate::render::Scene;
use mirui::prelude::*;
use mirui::surface::FramebufferAccess;

const MOVE_MS: u64 = 220;
const PRESS_MS: u64 = 180;

#[derive(Default)]
pub struct Motion {
    page: Option<Page>,
    from: [i32; 4],
    to: [i32; 4],
    moved_at: u64,
    pressed_at: Option<u64>,
    focused: bool,
}

impl Motion {
    fn position(&self, now: u64) -> [i32; 4] {
        let t = (now.saturating_sub(self.moved_at) as f32 / MOVE_MS as f32).min(1.0);
        let eased = if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        };
        [0, 1, 2, 3].map(|index| {
            (self.from[index] as f32 + (self.to[index] - self.from[index]) as f32 * eased).round()
                as i32
        })
    }

    pub fn retarget(&mut self, page: &Page, rect: Option<[i32; 4]>, now: u64) {
        if let Some(rect) = rect {
            let target = rect;
            if self.focused && self.page.as_ref() == Some(page) && self.to == target {
                return;
            }
            let previous = if self.focused && self.page.as_ref() == Some(page) {
                self.position(now)
            } else {
                target
            };
            self.from = previous;
            self.to = target;
            self.moved_at = now;
            self.focused = true;
        } else {
            self.focused = false;
        }
        self.page = Some(page.clone());
    }

    pub fn press(&mut self, now: u64) {
        self.pressed_at = Some(now);
    }

    pub fn active(&self, scene: &Scene, now: u64) -> bool {
        scene.loading
            || (self.focused
                && self.from != self.to
                && now.saturating_sub(self.moved_at) <= MOVE_MS)
            || self
                .pressed_at
                .is_some_and(|start| now.saturating_sub(start) <= PRESS_MS)
    }

    pub fn apply<B: FramebufferAccess>(
        &self,
        app: &mut App<B>,
        scene: &Scene,
        now: u64,
    ) -> Result<(), String> {
        crate::render::move_focus(app, self.focused.then(|| self.position(now)));
        let press = self.pressed_at.map_or(0.0, |start| {
            (1.0 - now.saturating_sub(start) as f32 / PRESS_MS as f32).max(0.0)
        });
        let stamp = Color::rgb(
            (234.0 - 16.0 * press) as u8,
            (207.0 - 37.0 * press) as u8,
            (192.0 - 60.0 * press) as u8,
        );
        let activity = std::array::from_fn(|index| {
            if scene.loading {
                let phase = (now + index as u64 * 210) % 900;
                let triangle = 1.0 - (phase as f32 / 450.0 - 1.0).abs();
                Color::rgba(166, 61, 50, (48.0 + triangle * 207.0) as u8)
            } else {
                Color::rgb(242, 235, 221)
            }
        });
        scene.feedback(stamp, activity);
        Ok(())
    }

    pub fn paint<B: FramebufferAccess>(
        &self,
        app: &mut App<B>,
        scene: &Scene,
        now: u64,
    ) -> Result<(), String> {
        self.apply(app, scene, now)?;
        app.render_dirty()
            .map_err(|error| format!("Menu animation: {error:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retarget_preserves_midcurve_geometry_and_same_target_keeps_its_clock() {
        let mut motion = Motion::default();
        let first = [12, 24, 180, 140];
        let second = [240, 40, 210, 120];
        motion.retarget(&Page::Choice, Some(first), 0);
        motion.retarget(&Page::Choice, Some(second), 10);
        let middle = motion.position(100);
        assert!(middle[0] > first[0] && middle[0] < second[0]);
        assert!(middle[2] > first[2] && middle[2] < second[2]);
        motion.retarget(&Page::Choice, Some(second), 100);
        assert_eq!(motion.moved_at, 10);
        assert_eq!(motion.position(100), middle);
        motion.retarget(&Page::Choice, Some(first), 100);
        assert_eq!(motion.position(100), middle);
        assert_eq!(motion.position(320), first);
        motion.retarget(&Page::Rooms, Some(second), 400);
        assert_eq!(motion.position(400), second);
        motion.retarget(&Page::Rooms, None, 500);
        assert!(!motion.focused);
    }
}
