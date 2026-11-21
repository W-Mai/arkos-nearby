use mirui::prelude::*;
use mirui::types::Transform;
use mirui::ui::ComputedRect;

use super::{artwork::CoverBounds, consoles::Platform, state::DisplayRuntime};

#[component]
pub(super) enum CardOrigin {
    Shelf(super::rows::RowHandle),
    Preview(super::state::DisplayHandle),
    Retiring,
}

#[component]
pub(super) struct CartridgeFront;

#[component]
pub(super) struct RoomChargeOrigin;

pub(super) fn retain_room_origin(world: &mirui::ecs::World) {
    let Some(display) = world.resource::<DisplayRuntime>() else {
        return;
    };
    let deck = &display.deck;
    if !deck.game_room() || deck.room_origin().is_some() {
        return;
    }
    let origin = world
        .query::<RoomChargeOrigin>()
        .iter()
        .find_map(|(node, _)| {
            if mirui::ui::branch::is_effectively_hidden(world, node) {
                return None;
            }
            world
                .get::<ComputedRect>(node)
                .map(|rect| rect.0)
                .filter(|rect| rect.w > Fixed::ZERO && rect.h > Fixed::ZERO)
        });
    if let Some(origin) = origin {
        deck.pin_room_origin(origin);
    }
}

#[component]
pub(super) struct CardPlacement {
    #[depends_on(mirui::ui::LayoutGeometry)]
    source: Entity,
}

#[component]
pub(super) struct MeasureCard(pub bool);

#[event]
fn measure_card(
    node: entity!(),
    enabled: query!(MeasureCard),
    parents: query!(mirui::ui::Parent),
    commands: commands!(),
) {
    if enabled.get(node).is_some_and(|measure| measure.0)
        && let Some(parent) = parents.get(node)
    {
        commands.insert(node, CardPlacement { source: parent.0 });
    }
}

#[view(component = MeasureCard, attach = measure_card::callback())]
pub(super) fn measure_source() {}

#[view(component = CardPlacement)]
pub(super) fn card_placement() {}

#[model]
pub(super) struct CardFace {
    #[observe]
    pub epoch: u64,
    #[observe]
    pub origin: Option<Rect>,
    #[observe]
    pub panel: Option<Rect>,
    #[observe]
    pub shelf: bool,
    #[observe]
    pub platform: Platform,
    #[observe]
    pub title: String,
    pub cover: model!(owned CoverBounds),
    sleeve: super::binding::Pose,
    cartridge: super::cartridge::Pose,
    source: Option<Entity>,
    lookup: Option<(u64, u8)>,
}

impl Default for CardFace {
    fn default() -> Self {
        Self {
            origin: None,
            epoch: 0,
            panel: None,
            shelf: false,
            platform: Platform::Unknown,
            title: String::new(),
            cover: CoverBounds::default().into(),
            sleeve: Default::default(),
            cartridge: Default::default(),
            source: None,
            lookup: None,
        }
    }
}

#[model]
impl CardFace {
    pub fn sleeve_pose(&self) -> super::binding::Pose {
        self.sleeve
    }
    pub fn cartridge_pose(&self) -> super::cartridge::Pose {
        self.cartridge
    }
    fn capture_cartridge(&mut self, pose: super::cartridge::Pose) {
        self.cartridge = pose;
    }
    fn capture_pose(&mut self, pose: super::binding::Pose) {
        self.sleeve = pose;
    }
    fn lookup_count(&self, epoch: u64) -> u8 {
        self.lookup
            .filter(|(last, _)| *last == epoch)
            .map_or(0, |(_, count)| count)
    }

    fn record_lookup(&mut self, epoch: u64) {
        self.lookup = Some((epoch, self.lookup_count(epoch) + 1));
    }

    fn pin(
        &mut self,
        origin: Rect,
        title: String,
        platform: Platform,
        source: Entity,
        shelf: bool,
        epoch: u64,
    ) {
        self.title = title;
        self.platform = platform;
        self.origin = Some(origin);
        self.source = Some(source);
        self.shelf = shelf;
        self.epoch = epoch;
    }

    fn layout_panel(&mut self, panel: Rect) {
        self.panel = Some(panel);
    }
    fn pinned_source(&self) -> Option<Entity> {
        self.source
    }

    fn unpin(&mut self) {
        self.source = None;
    }

    fn release(&mut self) {
        self.origin = None;
        self.panel = None;
        self.title.clear();
        self.source = None;
    }
}

pub(super) fn retain_card_face(world: &mut mirui::ecs::World) {
    let Some(runtime) = world.resource::<DisplayRuntime>() else {
        return;
    };
    let launcher = runtime.state.launcher();
    let shelf = runtime.shelf.clone();
    let face = runtime.card_face.clone();
    let popup = launcher.popup();
    if face.origin().is_some() {
        if popup.cartridge() && face.epoch() == popup.card_epoch() {
            if let Some(source) = face.pinned_source()
                && popup.closing()
            {
                if !world.is_alive(source) {
                    popup.detach();
                }
                if popup.detached_at().is_some() {
                    face.unpin();
                }
            }
            return;
        }
        face.release();
        face.cover().clear_artwork();
    }
    // A freshly mounted source publishes geometry after its first layout.
    // A second lookup consumes that publication; later frames use the normal popup.
    let epoch = popup.card_epoch();
    if !popup.cartridge()
        || popup.closing()
        || popup.progress() >= Fixed::HALF
        || face.lookup_count(epoch) >= 2
    {
        return;
    }
    face.record_lookup(epoch);
    let tab = launcher.tab();
    let selected = shelf.selected();
    let shelf_matches = tab == 0
        && shelf
            .selected_game_key()
            .is_some_and(|key| popup.card_matches(&key));
    let mut retained = None;
    for (entity, source) in world.query::<CardOrigin>().iter() {
        let (title, platform, cover) = match source {
            CardOrigin::Shelf(row)
                if shelf_matches && selected == Some(row.index()) && !row.directory() =>
            {
                (row.label(true), row.platform(), row.cover_bounds())
            }
            CardOrigin::Preview(display)
                if tab == 1
                    && display
                        .selected_game_key()
                        .is_some_and(|key| popup.card_matches(&key)) =>
            {
                (
                    display.reading_title(false),
                    display.selected_platform(),
                    display.cover_bounds(),
                )
            }
            _ => continue,
        };
        // Capture the shell at the displayed pose. The slot's focus bounds stay put.
        let (geometry_source, shadow) = if matches!(source, CardOrigin::Shelf(_)) {
            let Some(target) = world
                .get::<mirui::ui::Children>(entity)
                .and_then(|children| {
                    children
                        .0
                        .iter()
                        .copied()
                        .find(|node| world.has::<super::cartridge::Shell>(*node))
                })
            else {
                continue;
            };
            face.capture_cartridge(shelf.shelf().pose(selected.unwrap_or_default(), false));
            (target, Fixed::from_int(super::shelf::SHADOW))
        } else {
            (entity, Fixed::ZERO)
        };
        let Some(rect) = world
            .resource::<mirui::ui::Outputs<mirui::ui::LayoutGeometry>>()
            .and_then(|outputs| outputs.get(geometry_source))
            .and_then(|layout| {
                let mut border = layout.border_rect();
                border.h += shadow;
                layout.transform().apply_rect(border)
            })
            .map(|corners| Rect::bounding_quad(&corners))
        else {
            continue;
        };
        if rect.w <= Fixed::ZERO
            || rect.h <= Fixed::ZERO
            || (mirui::ui::branch::is_effectively_hidden(world, entity)
                && !world
                    .resource::<super::modal::Runtime>()
                    .is_some_and(|runtime| runtime.model.hidden()))
        {
            continue;
        }
        if let CardOrigin::Preview(display) = source {
            face.capture_pose(display.source_sleeve());
        }
        retained = Some((entity, rect, title, platform, cover));
        break;
    }
    if let Some((entity, rect, title, platform, cover)) = retained {
        // The modal snapshot must retain the real card through the handoff back to live UI.
        face.cover().share_artwork(&cover);
        face.pin(rect, title, platform, entity, tab == 0, popup.card_epoch());
    }
}

// Both faces turn around the same moving vertical axis. Draw the visible
// face upright on each half-turn so the actions read normally on the back.
pub(super) fn cartridge_pose(
    progress: Fixed,
    travel: Fixed,
    origin: Rect,
    panel: Rect,
    local: Rect,
) -> Transform {
    if local.w <= Fixed::ZERO || local.h <= Fixed::ZERO {
        return Transform::IDENTITY;
    }
    // Rotation governs which face is visible. The spring carries the mass of
    // the card, with the lift and counter-tilt measured from its own size.
    let mix = |a: Fixed, b: Fixed, t: Fixed| a + (b - a) * t;
    let envelope = (progress.min(Fixed::ONE - progress) * 16).min(Fixed::ONE);
    let lead = (travel - progress) * envelope;
    let carriage = progress + lead * Fixed::from_ratio(4, 5);
    let rise = Fixed::sin_deg(progress * 180);
    let swing = Fixed::sin_deg(progress * 360);
    let side = if origin.x + origin.w / 2 < panel.x + panel.w / 2 {
        -Fixed::ONE
    } else {
        Fixed::ONE
    };
    let arc = origin.h.min(panel.h) / 5;
    let flex = lead * Fixed::from_ratio(1, 10);
    let width = mix(origin.w, panel.w, progress + lead / 2) * (Fixed::ONE + flex);
    let height = mix(origin.h, panel.h, progress + lead / 2) * (Fixed::ONE - flex);
    // The travel tween is fastest at its midpoint. Ease the angle through
    // that point so the narrow edge remains legible between frames.
    let angle = progress * 180 + swing * 16;
    let thickness = Fixed::from_int(3);
    let projected = width * Fixed::cos_deg(angle).abs() + thickness * Fixed::sin_deg(angle);
    // Round Q24.8 scale to the nearest step so a wide menu keeps its thin edge.
    let scale = |size: Fixed, extent: Fixed| (size + extent / 512) / extent;
    let bank = side * (swing * 8 + rise * 3 + lead * 6);
    let rotation = Transform::rotate_deg(bank);
    let mut pose = rotation.compose(&Transform::scale(
        scale(projected, local.w),
        scale(height, local.h),
    ));
    pose.tx = mix(origin.x + origin.w / 2, panel.x + panel.w / 2, carriage) + side * rise * arc / 2
        - (local.x + local.w / 2);
    let center_y = mix(origin.y + origin.h / 2, panel.y + panel.h / 2, carriage);
    let half_height = (rotation.m10.abs() * projected + rotation.m11.abs() * height) / 2;
    let headroom = (center_y - half_height - origin.y.min(panel.y) / 2).max(Fixed::ZERO);
    let lift = rise * arc;
    let lift = if lift + headroom > Fixed::ZERO {
        lift * headroom / (lift + headroom)
    } else {
        Fixed::ZERO
    };
    pose.ty = center_y - lift - (local.y + local.h / 2);
    pose
}

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

fn room_pose(from: Rect, panel: Rect, progress: Fixed) -> Transform {
    let rest = Fixed::ONE - progress;
    let width = panel.w + (from.w - panel.w) * rest;
    let height = panel.h + (from.h - panel.h) * rest;
    let arc = Fixed::sin_deg(progress * 180);
    let x = (from.x + from.w / 2 - panel.x - panel.w / 2) * rest;
    let y = (from.y + from.h / 2 - panel.y - panel.h / 2) * rest - arc * 12;
    Transform::translate(x, y).compose(&Transform::scale(width / panel.w, height / panel.h))
}

pub(super) fn charge_opacity(deck: &super::deck::DeckHandle) -> Fixed {
    if deck.game_room() && deck.layer() == super::deck::Layer::Room {
        return (Fixed::ONE - deck.panel() * 2).max(Fixed::ZERO);
    }
    Fixed::ONE
}

#[derive(Clone)]
enum Source {
    Popup(crate::launcher::popup::PresenceHandle, CardFaceHandle),
    Front(crate::launcher::popup::PresenceHandle, CardFaceHandle),
    Room(super::deck::DeckHandle),
}

#[component]
pub(super) struct PanelMotion {
    source: Source,
    previous: Option<Transform>,
}

impl PanelMotion {
    #[cfg(test)]
    pub(super) fn is_room(&self) -> bool {
        matches!(&self.source, Source::Room(_))
    }

    pub(super) fn popup(
        model: crate::launcher::popup::PresenceHandle,
        face: CardFaceHandle,
    ) -> Self {
        Self {
            source: Source::Popup(model, face),
            previous: None,
        }
    }
    pub(super) fn room(model: super::deck::DeckHandle) -> Self {
        Self {
            source: Source::Room(model),
            previous: None,
        }
    }
    pub(super) fn front(
        model: crate::launcher::popup::PresenceHandle,
        face: CardFaceHandle,
    ) -> Self {
        Self {
            source: Source::Front(model, face),
            previous: None,
        }
    }
}

fn cartridge_appearance(
    model: &crate::launcher::popup::PresenceHandle,
    face: &CardFaceHandle,
    local: Option<Rect>,
    front: bool,
) -> Option<(Transform, u8)> {
    let origin = face.origin()?;
    if !model.cartridge() {
        return None;
    }
    if !front
        && let Some(panel) = local
        && panel.w > Fixed::ZERO
        && panel.h > Fixed::ZERO
        && face.panel() != Some(panel)
    {
        face.layout_panel(panel);
    }
    let progress = model.progress();
    let stop = model.detached_at();
    let angle = stop.unwrap_or(progress);
    let opacity = if model.shows_front() == front && (front || face.panel().is_some()) {
        Fixed::ONE
    } else {
        Fixed::ZERO
    };
    let alpha = match stop {
        Some(value) if value > Fixed::ZERO => opacity * (progress / value).min(Fixed::ONE),
        Some(_) => Fixed::ZERO,
        None => opacity,
    };
    let pose = match face.panel() {
        Some(panel) if angle < Fixed::ONE => cartridge_pose(
            angle,
            model.card_travel(),
            origin,
            panel,
            if front { origin } else { panel },
        ),
        Some(_) if !front => panel_pose(Fixed::ONE, model.arrival()),
        _ => Transform::IDENTITY,
    };
    Some((pose, (alpha * Fixed::from_int(255)).to_int() as u8))
}

fn popup_appearance(progress: Fixed, arrival: Fixed) -> (Transform, u8) {
    let alpha = (progress * Fixed::from_int(3)).min(Fixed::ONE);
    (
        panel_pose(progress, arrival),
        (alpha * Fixed::from_int(255)).to_int() as u8,
    )
}

#[system(order = Normal)]
fn tilt_panels(
    nodes: query!(Entity, mut PanelMotion, optional mirui::ui::OffscreenRender, optional ComputedRect),
    commands: commands!(),
) {
    for (entity, motion, output, rect) in nodes {
        let (pose, opacity) = match &motion.source {
            Source::Popup(model, face) => {
                cartridge_appearance(model, face, rect.map(|rect| rect.0), false)
                    .unwrap_or_else(|| popup_appearance(model.progress(), model.arrival()))
            }
            Source::Front(model, face) => {
                cartridge_appearance(model, face, None, true).unwrap_or((Transform::IDENTITY, 0))
            }
            Source::Room(model) => {
                let progress = model.panel();
                match (model.room_origin(), rect.map(|rect| rect.0)) {
                    (Some(from), Some(panel)) if panel.w > Fixed::ZERO && panel.h > Fixed::ZERO => {
                        (
                            room_pose(from, panel, progress),
                            ((progress * 3).min(Fixed::ONE) * 255).to_int() as u8,
                        )
                    }
                    _ => popup_appearance(progress, Fixed::ONE),
                }
            }
        };
        if output.is_none_or(|output| output.opacity != opacity) {
            commands.insert(
                entity,
                mirui::ui::OffscreenRender::default().with_opacity(opacity),
            );
        }
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
