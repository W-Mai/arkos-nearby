use super::{StaticCache, pixel::Scenery, style::*};
use mirui::prelude::*;
use mirui::ui::OffscreenRender;
use mirui::ui::widgets::{Text, TextAlign::Start};

#[compose]
pub(crate) fn logo(size: i32) -> Entity {
    ui! {
        View (width: size, height: size, shrink: 0.0) [
            Scenery::Logo,
            StaticCache,
            OffscreenRender::default(),
        ]
    }
}

#[compose]
pub(crate) fn wordmark(color: ColorToken) -> Entity {
    ui! {
        Text (
            text: howff_menu::brand::NAME,
            width: Dimension::Content,
            height: Dimension::Content,
            min_height: i32::from(TITLE_SIZE) + LABEL_LEADING,
            shrink: 0.0,
            font: font(TITLE_SIZE),
            font_size: TITLE_SIZE,
            text_color: color,
            paragraph: super::widgets::paragraph(Start, 1)
        ) [
            FONT_PAINT,
            StaticCache,
            OffscreenRender::default(),
        ]
    }
}
