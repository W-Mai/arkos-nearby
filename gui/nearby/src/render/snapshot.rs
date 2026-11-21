use mirui::app::RendererFactory;
use mirui::core::resource::{ResourceHandle, ResourceManager};
use mirui::ecs::World;
use mirui::prelude::*;
use mirui::render::texture::Texture;
use mirui::surface::Surface;
use mirui::ui::{Outputs, WidgetTexture};

/// Takes the completed output without another render or pixel copy.
pub(super) fn take<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    entity: Entity,
) -> Option<Texture<'static>> {
    app.world
        .resource::<Outputs<WidgetTexture>>()?
        .get(entity)?;
    let texture = app.take_widget_output(entity).ok()??.into_texture();
    (texture.width() > 0 && texture.height() > 0).then_some(texture)
}

pub(super) fn register(
    world: &World,
    token: String,
    texture: Texture<'static>,
    revision: u64,
) -> Option<ResourceHandle<Texture<'static>>> {
    let textures = world.resource::<ResourceManager<Texture<'static>>>()?;
    let dimensions = (texture.width(), texture.height());
    let texture = texture.with_cache_revision(revision);
    textures.add_factory(token.clone(), move || Some(texture));
    let handle = textures.load(token.clone());
    let admitted = handle.get();
    if (admitted.width(), admitted.height()) != dimensions {
        textures.remove_token(&token);
        return None;
    }
    Some(handle)
}
