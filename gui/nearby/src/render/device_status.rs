use super::{style::*, widgets::paragraph};
use howff_platform::device::{Battery, Status, Wireless};
use mirui::prelude::*;
use mirui::ui::widgets::{Image, Text, TextAlign::End};

const SIGNAL_WIDTH: i32 = 18;
const CLOCK_WIDTH: i32 = 36;
const BATTERY_WIDTH: i32 = 25;
const PERCENT_WIDTH: i32 = 28;
const STATUS_GAP: i32 = 8;
const CHARGE_GAP: i32 = 4;
const CHARGE_WIDTH: i32 = BATTERY_WIDTH + PERCENT_WIDTH + CHARGE_GAP;
const STATUS_WIDTH: i32 = SIGNAL_WIDTH + CLOCK_WIDTH + CHARGE_WIDTH + STATUS_GAP * 2;

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
            .map_or_else(|| "--%".into(), |percent| format!("{percent}%"))
    }

    #[observe]
    pub fn charge_width(&self) -> i32 {
        self.battery
            .and_then(|battery| battery.percent)
            .map_or(0, |percent| (i32::from(percent) * 16 + 50) / 100)
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
            Row (
                width: Dimension::Content,
                height: Dimension::Content,
                min_width: CHARGE_WIDTH,
                column_gap: CHARGE_GAP,
                align: AlignItems::Center
            ) {
                View (width: BATTERY_WIDTH, height: 18, shrink: 0.0) {
                    View (
                        position: Position::Absolute,
                        left: 0,
                        top: 2,
                        width: 22,
                        height: 14,
                        border_width: 2,
                        border_color: ${ if status.low_battery() { CLAY } else { color } }
                    )
                    View (
                        position: Position::Absolute,
                        left: 22,
                        top: 6,
                        width: 3,
                        height: 6,
                        bg_color: ${ if status.low_battery() { CLAY } else { color } }
                    )
                    View (
                        position: Position::Absolute,
                        left: 3,
                        top: 5,
                        height: 8,
                        width: ${ status.charge_width() },
                        bg_color: ${ if status.low_battery() { CLAY } else { color } }
                    )
                    Image (
                        position: Position::Absolute,
                        left: 7,
                        top: 0,
                        width: 10,
                        height: 18,
                        viewbox: 18,
                        visible: ${ status.charging() },
                        color: ACCENT,
                        src: path!(M 9 0 H 17 L 11 7 H 17 L 4 18 L 8 10 H 1 Z)
                    )
                }
                Text (
                    text: ${ status.battery_label() },
                    width: PERCENT_WIDTH,
                    height: Dimension::Content,
                    shrink: 0.0,
                    font: font(BODY_SIZE),
                    font_size: BODY_SIZE,
                    text_color: ${ if status.low_battery() { CLAY } else { color } },
                    paragraph: paragraph(End, 1)
                ) [
                    FONT_PAINT,
                ]
            }
        }
    }
}
