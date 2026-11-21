use super::{CoverBounds, CoverBoundsHandle, Handle, Texture};
use mirui::{
    anim::{INTERACTIVE, Spring},
    ecs::DeltaTimeMs,
    prelude::*,
    render::command::{CompositeMode, DrawCommand},
    types::TransformClass,
};

type Artwork = Handle<Texture<'static>>;

#[component]
#[derive(Clone)]
pub(super) struct LeavingArtwork {
    bounds: model!(CoverBounds),
    current: Option<Artwork>,
    leaving: Option<Artwork>,
    opacity: Spring,
}

impl LeavingArtwork {
    pub(super) fn new(bounds: CoverBoundsHandle) -> Self {
        Self {
            bounds,
            current: None,
            leaving: None,
            opacity: Spring::preset(Fixed::ZERO, Fixed::ZERO, INTERACTIVE),
        }
    }

    fn advance(&mut self, next: Option<Artwork>, dt: u16) {
        if revision(&next) != revision(&self.current) {
            if let Some(current) = self.current.take() {
                self.leaving = Some(current);
                self.opacity = Spring::preset(Fixed::ONE, Fixed::ZERO, INTERACTIVE);
            }
            self.current = next;
        }
        self.opacity.tick(dt);
        if self.opacity.is_settled() {
            self.leaving = None;
        }
    }
}

fn revision(image: &Option<Artwork>) -> Option<u64> {
    image.as_ref().map(|texture| texture.cache_revision)
}

#[system(order = Animation)]
fn animate(
    previews: query!(Entity, LeavingArtwork),
    delta: res!(DeltaTimeMs),
    commands: commands!(),
) {
    for (entity, preview) in previews {
        let next = preview.bounds.image();
        if revision(&next) == revision(&preview.current) && preview.leaving.is_none() {
            continue;
        }
        let mut updated = preview.clone();
        updated.advance(next, delta.0);
        commands.insert(entity, updated);
    }
}

#[view(component = LeavingArtwork, invalidation = Visual, systems = [animate::system()])]
pub(super) fn departure(art: component!(), paint: paint!()) {
    let Some(image) = &art.leaving else {
        return;
    };
    let alpha = art.opacity.value().clamp(Fixed::ZERO, Fixed::ONE);
    let rect = paint.rect();
    let aspect =
        Fixed::from_int(i32::from(image.width())) / Fixed::from_int(i32::from(image.height()));
    let width = rect.w.max(rect.h * aspect);
    let height = rect.h.max(rect.w / aspect);
    let offset = (Fixed::ONE - alpha) * 12;
    let cover = Rect::new(
        rect.x + (rect.w - width) / 2,
        rect.y + (rect.h - height) / 2 - offset,
        width,
        height,
    );
    let transform = paint.transform();
    let complex = !matches!(
        transform.classify(),
        TransformClass::Identity | TransformClass::Translate
    );
    paint.draw(&DrawCommand::Blit {
        pos: Point::new(cover.x, cover.y),
        size: Point::new(cover.w, cover.h),
        transform,
        quad: complex.then(|| transform.apply_rect(cover)),
        texture: image,
        opa: (alpha * 255).round().to_int() as u8,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirui::render::texture::ColorFormat;

    fn image(revision: u64) -> Artwork {
        Handle::new(
            Texture::owned(4, 4, ColorFormat::RGBA8888)
                .unwrap()
                .with_cache_revision(revision),
        )
    }

    #[test]
    fn game_preview_reuses_the_departing_texture_and_releases_it_at_rest() {
        let mut app = App::headless(64, 64);
        let bounds = app.add_model(CoverBounds::default()).finish();
        let mut preview = LeavingArtwork::new(bounds);
        preview.advance(Some(image(1)), 16);
        assert!(preview.leaving.is_none());
        preview.advance(None, 16);
        assert_eq!(revision(&preview.leaving), Some(1));
        let opacity = preview.opacity.value();
        preview.advance(Some(image(2)), 16);
        assert_eq!(revision(&preview.leaving), Some(1));
        assert!(preview.opacity.value() < opacity);
        for _ in 0..30 {
            preview.advance(Some(image(2)), 16);
        }
        assert!(preview.leaving.is_none());
        assert_eq!(revision(&preview.current), Some(2));
    }
}
