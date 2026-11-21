use super::focus_surface::{button_shadow, button_transform};
use super::pixel::Stage::Walker;
use super::state::PageKey;
use super::{
    Section,
    pixel::{self, Scenery, art},
    selection::FocusTarget,
    state::DisplayState,
    style::*,
    widgets::{label, paragraph},
};
use crate::model::Page;
use mirui::prelude::*;
use mirui::ui::widgets::Text;
use mirui::ui::widgets::TextAlign::Center;

#[compose]
pub(super) fn choice(view: PageKey, state: model!(DisplayState), metrics: Metrics) -> Entity {
    let cards = if view.page == Page::Mode {
        [
            (0, "仅组网", "连接附近的掌机"),
            (1, "选择游戏", "从游戏库选择"),
        ]
    } else if view.network_only {
        [
            (0, "创建房间", "邀请好友连接你的掌机"),
            (1, "加入房间", "寻找附近好友的房间"),
        ]
    } else {
        [
            (0, "创建房间", "邀请好友加入你的房间"),
            (1, "加入房间", "寻找附近好友的房间"),
        ]
    };
    ui! {
        Column (grow: 1.0, min_height: 0, row_gap: metrics.gap) {
            Row (grow: 1.0, min_height: 0, column_gap: metrics.choice_gap()) [
                Section::FocusList,
            ] {
                walk cards.into_iter() with card {
                    Column (
                        grow: 1.0,
                        min_width: 0,
                        min_height: 0
                    ) {
                        Column (
                            grow: 1.0,
                            min_width: 0,
                            min_height: 0,
                            padding: metrics.card_padding(),
                            row_gap: metrics.gap,
                            transform: ${ button_transform(&state, card.0, metrics) },
                            bg_color: ${ if state.button_motion().at(card.0) >= Fixed::HALF { ACCENT_DIM } else { PANEL } },
                            border_width: 1,
                            border_color: DIVIDER
                        ) [
                            FocusTarget::fixed(card.0).scoped(view.page),
                        ] {
                            View (
                                grow: 1.0,
                                min_height: metrics.choice_art_height(),
                                clip_children: true
                            ) [
                                Scenery::Landscape,
                                pixel::ChoiceSprite {
                                    state: state.clone(),
                                    index: card.0,
                                    mode: view.page == Page::Mode,
                                },
                            ] {
                                Text (
                                    if card.0 == 0 { "+" } else { "2" },
                                    position: Position::Absolute,
                                    left: 6,
                                    top: 6,
                                    width: 24,
                                    height: 20,
                                    font: font(metrics.small),
                                    font_size: metrics.small,
                                    bg_color: BACKGROUND,
                                    text_color: SECONDARY,
                                    paragraph: paragraph(Center, 1)
                                ) [
                                    FONT_PAINT,
                                ]
                            }
                            label (card.1, TITLE_SIZE, FOREGROUND, Center)
                            if !metrics.tiny {
                                Text (
                                    card.2,
                                    width: Dimension::percent(100),
                                    min_width: 0,
                                    height: Dimension::Content,
                                    font: font(metrics.small),
                                    font_size: metrics.small,
                                    text_color: SECONDARY,
                                    paragraph: paragraph(Center, 2)
                                ) [
                                    FONT_PAINT,
                                ]
                            }
                        }
                        button_shadow (state, card.0, metrics)
                    }
                }
            }
            if ${ view.page == Page::Mode && state.current_game() } {
                Column (height: 36, shrink: 0.0) {
                    Row (
                        grow: 1.0,
                        min_height: 0,
                        bg_color: ${ if state.button_motion().at(2) >= Fixed::HALF { ACCENT_DIM } else { PANEL } },
                        transform: ${ button_transform(&state, 2, metrics) },
                        padding: Padding { left: 6.into(), right: 6.into(), ..Default::default() },
                        align: AlignItems::Center
                    ) [
                        FocusTarget::fixed(2).scoped(view.page),
                    ] {
                        label ("+ 创建当前游戏的房间", metrics.body, FOREGROUND, Center)
                    }
                    button_shadow (state, 2, metrics)
                }
            }
            art (state, Walker, metrics.walker_height())
        }
    }
}
