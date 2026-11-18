use mirui::prelude::*;
use mirui::types::Transform;

// Orthographic card lift stays on SDL's native textured-quad route. A
// projective request uses the backend's CPU destination-edit path instead.
pub(super) fn panel_pose(progress: Fixed, arrival: Fixed) -> Transform {
    let depth = Fixed::ONE - progress;
    let turn = Fixed::from_int(4) * arrival * (Fixed::ONE - arrival);
    let scale = Fixed::ONE - depth * Fixed::from_ratio(14, 100) - turn * Fixed::from_ratio(1, 100);
    let roll = -depth * Fixed::from_ratio(4, 100) + turn * Fixed::from_ratio(2, 100);
    Transform {
        m00: scale,
        m01: -roll,
        m10: roll,
        m11: scale * (Fixed::ONE - depth * Fixed::from_ratio(5, 100)),
        ty: depth * Fixed::from_int(36) - turn * Fixed::from_int(4),
        ..Transform::IDENTITY
    }
}

#[derive(Clone)]
enum Source {
    Popup(crate::launcher::popup::PresenceHandle),
    Room(super::deck::DeckHandle),
}

#[component]
pub(super) struct PanelMotion {
    source: Source,
    previous: Option<Transform>,
}

impl PanelMotion {
    pub(super) fn popup(model: crate::launcher::popup::PresenceHandle) -> Self {
        Self {
            source: Source::Popup(model),
            previous: None,
        }
    }
    pub(super) fn room(model: super::deck::DeckHandle) -> Self {
        Self {
            source: Source::Room(model),
            previous: None,
        }
    }
}

#[system(order = Normal)]
fn tilt_panels(
    nodes: query!(Entity, mut PanelMotion, optional mirui::ui::OffscreenRender),
    commands: commands!(),
) {
    for (entity, motion, output) in nodes {
        let (progress, arrival) = match &motion.source {
            Source::Popup(model) => (model.progress(), model.arrival()),
            Source::Room(model) => (model.panel(), Fixed::ONE),
        };
        let opacity = (progress * Fixed::from_int(3)).min(Fixed::ONE);
        let opacity = (opacity * Fixed::from_int(255)).to_int() as u8;
        if output.is_none_or(|output| output.opacity != opacity) {
            commands.insert(
                entity,
                mirui::ui::OffscreenRender::default().with_opacity(opacity),
            );
        }
        let pose = panel_pose(progress, arrival);
        if motion.previous != Some(pose) {
            motion.previous = Some(pose);
            if pose.is_identity() {
                commands.remove::<mirui::ui::widgets::WidgetTransform>(entity);
            } else {
                commands.insert(entity, mirui::ui::widgets::WidgetTransform(pose));
            }
        }
    }
}

#[view(component = PanelMotion, systems = [tilt_panels::system()])]
pub(super) fn panel_motion() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_motion_keeps_continuous_endpoints_and_native_affine_geometry() {
        let mut translations = Vec::new();
        for step in 0..=256 {
            let pose = panel_pose(Fixed::from_ratio(step, 256), Fixed::ONE);
            if translations.last() != Some(&pose.ty) {
                translations.push(pose.ty);
            }
            assert!(pose.m00 > Fixed::ZERO && pose.m11 > Fixed::ZERO);
        }
        assert_eq!(translations.len(), 257);
        assert_eq!(panel_pose(Fixed::ONE, Fixed::ZERO), Transform::IDENTITY);
        assert_eq!(panel_pose(Fixed::ONE, Fixed::ONE), Transform::IDENTITY);
        assert_ne!(panel_pose(Fixed::ONE, Fixed::HALF), Transform::IDENTITY);
    }
}
