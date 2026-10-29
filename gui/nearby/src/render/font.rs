use super::style::{FONT, TITLE_FONT};
use mirui::app::RendererFactory;
use mirui::prelude::*;
use mirui::render::font::{Font, FontManager, mirx::MirxFontProvider};
use mirui::surface::Surface;
use std::rc::Rc;
const MENU_BYTES: &[u8] = include_bytes!("../../assets/nearby-menu-12.mirx");
const TITLE_BYTES: &[u8] = include_bytes!("../../assets/nearby-title-24.mirx");

fn load(bytes: &'static [u8], size: u16) -> Result<Font, String> {
    let limits = mirx::reader::PayloadLimits::HOST;
    let options = mirx::reader::ReadOptions::new().with_payload_limits(limits);
    let reader = mirx::Reader::open_with(bytes, &options)
        .map_err(|error| format!("Menu font container: {error:?}"))?;
    let mut faces = reader
        .chunks()
        .filter(|chunk| chunk.chunk_type() == mirx::ChunkType::FONT);
    let face = faces.next().ok_or("The menu asset contains no font")?;
    if faces.next().is_some() {
        return Err("The menu asset contains multiple fonts".into());
    }
    let provider = MirxFontProvider::from_payload(face.payload(), &limits)
        .map_err(|error| format!("Menu font face: {error:?}"))?;
    Ok(Font {
        family: "Nearby Pixel",
        size,
        provider: Rc::new(provider),
    })
}

pub(super) fn install<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
) -> Result<(), String> {
    let menu = load(MENU_BYTES, 12)?;
    let title = load(TITLE_BYTES, 24)?;
    let fonts = FontManager::new(mirui::core::cache::MaxSize::Unbound, menu.clone());
    fonts.add_static(FONT.cache_key(), menu);
    fonts.add_static(TITLE_FONT.cache_key(), title);
    app.with_fonts(fonts);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::style::{Metrics, font};

    #[test]
    fn pixel_faces_cover_the_catalog_charset_at_native_sizes() {
        for (bytes, size) in [(MENU_BYTES, 12), (TITLE_BYTES, 24)] {
            let face = load(bytes, size).unwrap();
            for character in include_str!("../../assets/nearby-menu-charset.txt")
                .trim_end_matches('\n')
                .chars()
            {
                assert!(face.map_char(character).is_some(), "Missing {character}");
            }
        }
        for height in [320, 480, 720] {
            let metrics = Metrics::for_height(height);
            for size in [
                metrics.title,
                metrics.body,
                metrics.small,
                metrics.row_title,
                metrics.row_detail,
            ] {
                assert!(matches!(size, 12 | 24));
                assert_eq!(font(size), if size == 24 { TITLE_FONT } else { FONT });
            }
        }
    }
}
