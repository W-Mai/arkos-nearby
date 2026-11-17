use howff_menu::brand::NAME as PRODUCT_NAME;
use mirui::prelude::*;
use mirui::types::Transform;
use mirui::ui::widgets::TextAlign::Center;
use mirui::ui::widgets::{Image, Text};

pub const WIDTH: u16 = 1120;
pub const HEIGHT: u16 = 780;
pub const SCREEN_KEYS: [&str; 2] = ["demo-host", "demo-guest"];
const BODY_W: f32 = 400.0;
const BODY_H: f32 = 540.0;
const BUTTON_DETAIL: Color = Color::rgb(48, 62, 53);
const RIDGE: Color = Color::rgb(30, 48, 38);
const INK: Color = Color::rgb(20, 35, 30);
const PAPER: Color = Color::rgb(242, 237, 219);
const GOLD: Color = Color::rgb(220, 164, 78);
const QUIET: Color = Color::rgb(151, 172, 148);
const TITLE: FontToken = FontToken::Custom("nearby_title");
const SMALL: FontToken = FontToken::Custom("nearby_menu");

#[derive(Clone, Copy, PartialEq)]
pub struct Pose {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub angle: f32,
}

impl Pose {
    fn transform(self) -> Transform {
        Transform::translate(
            (self.x - BODY_W / 2.0).into(),
            (self.y - BODY_H / 2.0).into(),
        )
        .compose(&Transform::rotate_deg(self.angle.into()))
        .compose(&Transform::scale(self.scale.into(), self.scale.into()))
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Frame {
    pub time: f32,
    pub poses: [Pose; 2],
    pub presses: [f32; 2],
    pub title: &'static str,
    pub subtitle: &'static str,
    pub stage: usize,
    pub connected: bool,
}

impl Frame {
    fn active_key(self) -> &'static str {
        if self.stage == 6 { "Y" } else { "A" }
    }

    fn spark(self, index: i32) -> Transform {
        let age = (self.time - 10.0).clamp(0.0, 1.2);
        let angle = index as f32 * std::f32::consts::TAU / 12.0;
        let distance = 30.0 + 150.0 * (1.0 - (1.0 - age / 1.2).powi(3));
        Transform::translate(
            (angle.cos() * distance).into(),
            (angle.sin() * distance + age * age * 36.0).into(),
        )
        .compose(&Transform::rotate_deg(
            (age * 180.0 + index as f32 * 30.0).into(),
        ))
    }

    fn spark_color(self, index: i32) -> Color {
        let age = self.time - 10.0;
        let color = if index % 2 == 0 { GOLD } else { PAPER };
        let alpha = if (0.0..1.2).contains(&age) {
            (255.0 * (1.0 - age / 1.2)) as u8
        } else {
            0
        };
        Color::rgba(color.r, color.g, color.b, alpha)
    }
}

#[model]
pub struct Promo {
    pub frame_signal: Signal<Frame>,
}

#[model]
impl Promo {
    pub fn frame(&self) -> Frame {
        self.frame_signal.get()
    }
    pub fn present(&mut self, frame: Frame) {
        self.frame_signal.set(frame);
    }
}

fn centered() -> mirui::ui::widgets::ParagraphStyle {
    mirui::ui::widgets::ParagraphStyle {
        align: Center,
        vertical_align: mirui::ui::widgets::text::TextVerticalAlign::Center,
        ..Default::default()
    }
}

#[compose]
fn square(x: i32, y: i32, w: i32, h: i32, color: Color) -> Entity {
    ui! {
        View (position: Position::Absolute, left: x, top: y, width: w, height: h, bg_color: color)
    }
}

#[compose]
fn caption(text: &'static str, x: i32, y: i32, w: i32, color: Color) -> Entity {
    ui! {
        Text (
            text,
            position: Position::Absolute,
            left: x,
            top: y,
            width: w,
            height: 24,
            font: SMALL,
            font_size: 12,
            text_color: color,
            paragraph: centered()
        )
    }
}

#[compose]
fn handheld(state: model!(Promo), side: usize) -> Entity {
    let shell = if side == 0 {
        Color::rgb(224, 226, 209)
    } else {
        Color::rgb(111, 85, 143)
    };
    let edge = if side == 0 {
        Color::rgb(156, 171, 155)
    } else {
        Color::rgb(65, 50, 91)
    };
    let highlight = if side == 0 {
        Color::rgb(251, 247, 230)
    } else {
        Color::rgb(159, 124, 187)
    };
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: BODY_W as i32,
            height: BODY_H as i32,
            transform: ${ state.frame().poses[side].transform() },
            clip_children: false
        ) {
            View (
                position: Position::Absolute,
                left: 8,
                top: 16,
                width: 400,
                height: 540,
                border_radius: 28,
                bg_color: Color::rgba(0, 0, 0, 75)
            )
            View (
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 400,
                height: 540,
                border_radius: 26,
                bg_color: edge
            )
            View (
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 400,
                height: 532,
                border_radius: 26,
                bg_color: shell,
                border_width: 3,
                border_color: highlight
            )
            View (
                position: Position::Absolute,
                left: 13,
                top: 14,
                width: 374,
                height: 290,
                border_radius: 14,
                bg_color: INK,
                border_width: 3,
                border_color: edge
            )
            Image (
                SCREEN_KEYS[side],
                position: Position::Absolute,
                left: 20,
                top: 23,
                width: 360,
                height: 270
            )
            caption (if side == 0 { "PLAYER 01" } else { "PLAYER 02" }, 142, 307, 116, INK)
            View (
                position: Position::Absolute,
                left: 37,
                top: 331,
                width: 102,
                height: 102,
                border_radius: 51,
                bg_color: edge
            )
            square (71, 338, 34, 88, INK)
            square (44, 365, 88, 34, INK)
            square (79, 374, 18, 18, BUTTON_DETAIL)
            walk [
                ("X", 300, 334, Color::rgb(109, 184, 199)),
                ("Y", 266, 368, Color::rgb(176, 204, 108)),
                ("A", 334, 368, Color::rgb(243, 137, 96)),
                ("B", 300, 402, Color::rgb(241, 200, 96)),
            ]
                .into_iter() with button {
                View (
                    position: Position::Absolute,
                    left: button.1 - 3,
                    top: button.2 - 3,
                    width: 42,
                    height: 42,
                    border_radius: 21,
                    bg_color: edge
                )
                View (
                    position: Position::Absolute,
                    left: button.1,
                    top: button.2,
                    width: 36,
                    height: 36,
                    border_radius: 18,
                    bg_color: ${
                        if button.0 == state.frame().active_key() && state.frame().presses[side] > 0.1 {
                            GOLD
                        } else {
                            INK
                        }
                    },
                    transform: ${
                        Transform::translate(
                            Fixed::ZERO,
                            (if button.0 == state.frame().active_key() {
                                state.frame().presses[side] * 3.0
                            } else {
                                0.0
                            })
                                .into(),
                        )
                    }
                ) {
                    Text (
                        button.0,
                        width: 36,
                        height: 36,
                        font: TITLE,
                        font_size: 24,
                        text_color: ${
                            if button.0 == state.frame().active_key() && state.frame().presses[side] > 0.1 {
                                INK
                            } else {
                                button.3
                            }
                        },
                        paragraph: centered()
                    )
                }
            }
            walk [149, 173, 197, 221].into_iter() with x {
                View (
                    position: Position::Absolute,
                    left: x,
                    top: 351,
                    width: 14,
                    height: 4,
                    border_radius: 2,
                    bg_color: INK
                )
            }
            walk [(99, 453), (251, 453)].into_iter() with stick {
                View (
                    position: Position::Absolute,
                    left: stick.0 - 5,
                    top: stick.1 - 5,
                    width: 66,
                    height: 66,
                    border_radius: 33,
                    bg_color: edge
                )
                View (
                    position: Position::Absolute,
                    left: stick.0,
                    top: stick.1,
                    width: 56,
                    height: 56,
                    border_radius: 28,
                    bg_color: INK
                )
                View (
                    position: Position::Absolute,
                    left: stick.0 + 10,
                    top: stick.1 + 9,
                    width: 36,
                    height: 36,
                    border_radius: 18,
                    bg_color: Color::rgb(47, 56, 48)
                )
            }
            walk [171, 209].into_iter() with x {
                View (
                    position: Position::Absolute,
                    left: x,
                    top: 430,
                    width: 20,
                    height: 14,
                    border_radius: 7,
                    bg_color: INK
                )
            }
            caption ("SELECT  START", 150, 449, 104, INK)
            View (
                position: Position::Absolute,
                left: 196,
                top: 494,
                width: 8,
                height: 8,
                border_radius: 4,
                bg_color: Color::rgb(175, 224, 118)
            )
        }
    }
}

#[compose]
pub fn stage(state: model!(Promo)) -> Entity {
    ui! {
        View (width: WIDTH as i32, height: HEIGHT as i32, bg_color: INK, clip_children: true) {
            walk 0..32 with i {
                View (
                    position: Position::Absolute,
                    left: 30 + (i * 137) % 1060,
                    top: 144 + (i * 73) % 550,
                    width: if i % 4 == 0 { 5 } else { 2 },
                    height: if i % 4 == 0 { 5 } else { 2 },
                    bg_color: Color::rgb(58, 77, 57),
                    transform: ${
                        Transform::translate(
                            Fixed::ZERO,
                            ((state.frame().time * 1.256637 + i as f32).sin() * 7.0).into(),
                        )
                    }
                )
            }
            walk 0..14 with i {
                square (i * 80, 720 - (i % 4) * 12, 80, 90, RIDGE)
            }
            Text (
                text: ${ state.frame().title },
                position: Position::Absolute,
                left: 60,
                top: 16,
                width: 1000,
                height: 64,
                font: TITLE,
                font_size: 48,
                text_color: PAPER,
                paragraph: centered()
            )
            Text (
                text: ${ state.frame().subtitle },
                position: Position::Absolute,
                left: 60,
                top: 84,
                width: 1000,
                height: 24,
                font: SMALL,
                font_size: 12,
                text_color: GOLD,
                paragraph: centered()
            )
            walk 0..7 with i {
                View (
                    position: Position::Absolute,
                    left: 528 + (i % 3) * 25,
                    top: 350 + (i / 3) * 27,
                    width: 7,
                    height: 7,
                    bg_color: ${
                        if state.frame().connected || ((state.frame().time * 7.0) as i32 % 7) == i {
                            GOLD
                        } else {
                            Color::rgb(61, 83, 66)
                        }
                    },
                    transform: ${ Transform::rotate_deg((state.frame().time * 90.0).into()) }
                )
            }
            handheld (state.clone(), 0)
            handheld (state.clone(), 1)
            walk 0..12 with i {
                View (
                    position: Position::Absolute,
                    left: 556,
                    top: 500,
                    width: 8,
                    height: 8,
                    visible: ${ (10.0..11.2).contains(&state.frame().time) },
                    bg_color: ${ state.frame().spark_color(i) },
                    transform: ${ state.frame().spark(i) }
                )
            }
            walk 0..7 with i {
                View (
                    position: Position::Absolute,
                    left: 468 + i * 28,
                    top: 728,
                    width: 20,
                    height: 4,
                    bg_color: ${ if state.frame().stage == i as usize { GOLD } else { Color::rgb(66, 87, 66) } }
                )
            }
            caption (PRODUCT_NAME, 36, 724, 180, PAPER)
            caption ("R36S", 886, 724, 198, QUIET)
        }
    }
}
