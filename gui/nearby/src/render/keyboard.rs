use super::{
    deck::{ActionAnchor, ActionLayout},
    style::*,
    widgets::{caption, shadow},
};
use mirui::prelude::*;
use mirui::types::Transform;
use mirui::ui::widgets::TextAlign::Center;

/// One key surface shared by search and private network input.
#[compose]
pub(super) fn keycap<T, S, M>(
    text: T,
    selected: S,
    motion: M,
    anchor: ActionAnchor,
    size: u16,
    metrics: Metrics,
) -> Entity
where
    T: Fn() -> String + 'static,
    S: Fn() -> bool + Clone + 'static,
    M: Fn() -> Transform + 'static,
{
    let fill = selected.clone();
    let border = selected.clone();
    ui! {
        Column (grow: 1.0, min_width: 0, min_height: 0, transform: ${ motion() }) {
            Column (
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                justify: JustifyContent::Center,
                padding: Padding {
                    right: 12.into(),
                    ..Default::default()
                },
                border_width: 1,
                border_color: ${ if border() { FOREGROUND } else { DIVIDER } },
                bg_color: ${ if fill() { ACCENT } else { PANEL } }
            ) [
                anchor,
            ] {
                caption (
                    text,
                    move || if selected() { ON_ACCENT } else { FOREGROUND },
                    size,
                    Center,
                    LABEL_LEADING
                )
            }
            shadow (metrics)
        }
    }
}

pub(super) fn search_anchor(index: usize) -> ActionAnchor {
    ActionAnchor(index, ActionLayout::SearchKeyboard)
}
