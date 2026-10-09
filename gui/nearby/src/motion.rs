use crate::model::Page;
use crate::render::Scene;
use mirui::prelude::*;
use mirui::surface::FramebufferAccess;
use mirui::ui::property::prop;

const MOVE_MS: u64 = 220;
const PRESS_MS: u64 = 180;

#[derive(Default)]
pub struct Motion {
    page: Option<Page>,
    from: [i32; 2],
    to: [i32; 2],
    moved_at: u64,
    pressed_at: Option<u64>,
    focused: bool,
}

impl Motion {
    fn position(&self, now: u64) -> [i32; 2] {
        let t = (now.saturating_sub(self.moved_at) as f32 / MOVE_MS as f32).min(1.0);
        let eased = if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        };
        [0, 1].map(|index| {
            (self.from[index] as f32 + (self.to[index] - self.from[index]) as f32 * eased).round()
                as i32
        })
    }

    pub fn retarget(&mut self, page: &Page, rect: Option<[i32; 4]>, now: u64) {
        if let Some(rect) = rect {
            let target = [rect[0], rect[1]];
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
        if let Some(entity) = scene.focus {
            let position = self.position(now);
            crate::render::move_focus(&mut app.world, entity, position);
        }
        let press = self.pressed_at.map_or(0.0, |start| {
            (1.0 - now.saturating_sub(start) as f32 / PRESS_MS as f32).max(0.0)
        });
        let stamp = Color::rgb(
            (234.0 - 16.0 * press) as u8,
            (207.0 - 37.0 * press) as u8,
            (192.0 - 60.0 * press) as u8,
        );
        app.world
            .widget_mut(scene.stamp)
            .ok_or("The confirmation widget is missing")?
            .set::<prop::BackgroundColor>(stamp.into());
        for (index, entity) in scene.activity.iter().enumerate() {
            let color = if scene.loading {
                let phase = (now + index as u64 * 210) % 900;
                let triangle = 1.0 - (phase as f32 / 450.0 - 1.0).abs();
                Color::rgba(166, 61, 50, (48.0 + triangle * 207.0) as u8)
            } else {
                Color::rgb(242, 235, 221)
            };
            app.world
                .widget_mut(*entity)
                .ok_or("The activity widget is missing")?
                .set::<prop::BackgroundColor>(color.into());
        }
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
