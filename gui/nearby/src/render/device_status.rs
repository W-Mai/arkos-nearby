use super::{style::*, widgets::paragraph};
use howff_platform::device::{Battery, Status, Wireless};
use mirui::prelude::*;
use mirui::ui::widgets::{
    Image, Text,
    TextAlign::{Center, End},
};

const SIGNAL_WIDTH: i32 = 18;
const CLOCK_WIDTH: i32 = 36;
const BATTERY_BODY: i32 = 31;
const BATTERY_TERMINAL: i32 = 3;
const BATTERY_WIDTH: i32 = BATTERY_BODY + BATTERY_TERMINAL;
const BATTERY_HEIGHT: i32 = 18;
const CHARGE_INSET: i32 = 2;
const BATTERY_CORNER: i32 = 3;
const CHARGE_WIDTH: i32 = BATTERY_BODY - 2 * CHARGE_INSET;
const STATUS_GAP: i32 = 8;
const STATUS_WIDTH: i32 = SIGNAL_WIDTH + CLOCK_WIDTH + BATTERY_WIDTH + STATUS_GAP * 2;

#[model]
pub(crate) struct DeviceStatus {
    #[observe]
    pub clock: String,
    #[observe]
    pub battery: Option<Battery>,
    #[observe]
    pub wireless: Wireless,
}

impl Default for DeviceStatus {
    fn default() -> Self {
        Self {
            clock: "--:--".into(),
            battery: None,
            wireless: Wireless::Unknown,
        }
    }
}

#[model]
impl DeviceStatus {
    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    pub fn present(&mut self, clock: String, status: Status) {
        if self.clock != clock {
            self.clock = clock;
        }
        if self.battery != status.battery {
            self.battery = status.battery;
        }
        if self.wireless != status.wireless {
            self.wireless = status.wireless;
        }
    }

    #[observe]
    pub fn battery_label(&self) -> String {
        self.battery
            .and_then(|battery| battery.percent)
            .map_or_else(|| "--".into(), |percent| percent.to_string())
    }

    #[observe]
    pub fn charge_width(&self) -> i32 {
        self.battery
            .and_then(|battery| battery.percent)
            .map_or(0, |percent| (i32::from(percent) * CHARGE_WIDTH + 50) / 100)
    }

    #[observe]
    pub fn charging(&self) -> bool {
        self.battery.is_some_and(|battery| battery.charging)
    }

    #[observe]
    pub fn low_battery(&self) -> bool {
        self.battery
            .is_some_and(|battery| !battery.charging && battery.percent.is_some_and(|p| p <= 15))
    }
}

pub(crate) struct Runtime(pub DeviceStatusHandle);

#[compose]
pub(super) fn indicators(color: ColorToken) -> Entity {
    let status = res!(Runtime).with(|runtime| runtime.0.clone());
    ui! {
        status_contents (status, color)
    }
}

#[compose]
fn status_contents(status: model!(DeviceStatus), color: ColorToken) -> Entity {
    ui! {
        Row (
            width: Dimension::Content,
            min_width: STATUS_WIDTH,
            height: Dimension::Content,
            shrink: 0.0,
            column_gap: STATUS_GAP,
            align: AlignItems::Center
        ) {
            View (width: SIGNAL_WIDTH, height: SIGNAL_WIDTH, shrink: 0.0) {
                Image (
                    width: 18,
                    height: 18,
                    viewbox: 24,
                    color: color,
                    src: path!(
                        M 3 5 H 21 V 8 H 18 V 7 H 6 V 8 H 3 Z M 7 10 H 17 V 13 H 15 V 12 H 9 V 13 H 7 Z M
                        10 15 H 14 V 19 H 10 Z
                    )
                )
                Image (
                    position: Position::Absolute,
                    width: 18,
                    height: 18,
                    viewbox: 24,
                    color: color,
                    visible: ${ status.wireless() != Wireless::Connected },
                    src: path!(M 3 17 L 17 3 L 20 6 L 6 20 Z)
                )
            }
            Text (
                text: ${ status.clock() },
                width: CLOCK_WIDTH,
                height: Dimension::Content,
                shrink: 0.0,
                font: font(BODY_SIZE),
                font_size: BODY_SIZE,
                text_color: color,
                paragraph: paragraph(End, 1)
            ) [
                FONT_PAINT,
            ]
            battery (status, color)
        }
    }
}

#[compose]
fn battery(status: model!(DeviceStatus), color: ColorToken) -> Entity {
    let digits = status.clone();
    let outline = status.clone();
    ui! {
        Row (width: BATTERY_WIDTH, height: BATTERY_HEIGHT, shrink: 0.0, align: AlignItems::Center) {
            View (
                width: BATTERY_BODY, height: BATTERY_HEIGHT, shrink: 0.0,
                border_width: 1, border_radius: BATTERY_CORNER,
                border_color: ${ if outline.low_battery() { CLAY } else { color } },
                container: true
            ) {
                battery_digits (digits, color, 0, 0)
                View (
                    position: Position::Absolute, left: CHARGE_INSET, top: CHARGE_INSET,
                    width: ${ status.charge_width() }, height: BATTERY_HEIGHT - 2 * CHARGE_INSET,
                    border_radius: 1, container: true, clip_children: true,
                    bg_color: ${ if status.low_battery() { CLAY } else { color } }
                ) {
                    battery_digits (status.clone(), BACKGROUND, -CHARGE_INSET, -CHARGE_INSET)
                }
            }
            View (
                width: BATTERY_TERMINAL, height: 6, border_radius: 1,
                bg_color: ${ if status.charging() { ACCENT } else if status.low_battery() { CLAY } else { color } }
            )
        }
    }
}

#[compose]
fn battery_digits(status: model!(DeviceStatus), color: ColorToken, left: i32, top: i32) -> Entity {
    ui! {
        Text (
            position: Position::Absolute, left: left, top: top + 2,
            width: BATTERY_BODY,
            height: Dimension::Content,
            text: ${ status.battery_label() },
            font: font(BODY_SIZE),
            font_size: BODY_SIZE,
            text_color: color,
            paragraph: paragraph(Center, 1)
        ) [FONT_PAINT]
    }
}
