use mirui::prelude::*;
pub(super) const FONT_PAINT: mirui::ui::dirty::PaintInflate =
    mirui::ui::dirty::PaintInflate::uniform(Fixed::ONE);

pub(super) const FONT: FontToken = FontToken::Custom("nearby_menu");
pub(super) const TITLE_FONT: FontToken = FontToken::Custom("nearby_title");
pub(super) const TITLE_SIZE: u16 = 24;
pub(super) const BODY_SIZE: u16 = 12;
pub(super) const ROW_LEADING: i32 = 2;
pub(super) const LABEL_LEADING: i32 = 6;
pub(super) const FOCUS_BORDER: i32 = 2;
// Outsets are percentages of the followed widget, independent of its dimensions.
pub(super) const FOCUS_OUTSET_X_PERCENT: f32 = 3.25;
pub(super) const FOCUS_OUTSET_Y_PERCENT: f32 = 3.5;

pub(super) fn font(size: u16) -> FontToken {
    match size {
        BODY_SIZE => FONT,
        TITLE_SIZE => TITLE_FONT,
        _ => unreachable!("Pixel text requires a native font size"),
    }
}
pub(super) const BACKGROUND: ColorToken = ColorToken::Surface;
pub(super) const FOREGROUND: ColorToken = ColorToken::OnSurface;
pub(super) const ACCENT: ColorToken = ColorToken::Primary;
pub(super) const ON_ACCENT: ColorToken = ColorToken::OnPrimary;
pub(super) const PANEL: ColorToken = ColorToken::SurfaceVariant;
pub(super) const SECONDARY: ColorToken = ColorToken::OnSurfaceVariant;
pub(super) const DISABLED: ColorToken = SECONDARY;
pub(super) const DIVIDER: ColorToken = ColorToken::Outline;
pub(super) const ACCENT_DIM: ColorToken = ColorToken::Custom("sky");
pub(super) const WARNING: ColorToken = ColorToken::Custom("gold");
pub(super) const WARNING_PANEL: ColorToken = PANEL;
pub(super) const CLAY: ColorToken = ColorToken::Custom("clay");
pub(super) const SKY: ColorToken = ColorToken::Custom("sky");
pub(super) const HILL: ColorToken = ColorToken::Custom("hill");
pub(super) const DEEP: ColorToken = ColorToken::Custom("deep");
pub(super) const HEADER_TEXT: ColorToken = ColorToken::Custom("header-text");
pub(super) const HEADER_DETAIL: ColorToken = ColorToken::Custom("header-detail");
#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub(super) compact: bool,
    pub(super) tiny: bool,
    pub(super) title: u16,
    pub(super) body: u16,
    pub(super) small: u16,
    pub(super) row_title: u16,
    pub(super) row_detail: u16,
    pub(super) gap: i32,
    pub(super) inset: i32,
}

impl Metrics {
    pub fn wifi_row_height(self) -> i32 {
        i32::from(TITLE_SIZE) + self.gap * 2 + 8
    }
    pub fn wifi_row_extent(self) -> i32 {
        self.wifi_row_height() + self.shadow_height() + self.gap
    }
    fn sized(self, tiny: i32, compact: i32, regular: i32) -> i32 {
        if self.tiny {
            tiny
        } else if self.compact {
            compact
        } else {
            regular
        }
    }

    pub fn header_height(self) -> i32 {
        self.sized(44, 52, 72)
    }
    pub fn launcher_header_height(self) -> i32 {
        self.sized(44, 60, 64)
    }
    pub fn logo_size(self) -> i32 {
        self.sized(30, 30, 48)
    }
    pub fn keycap_size(self) -> i32 {
        i32::from(self.small) + 6
    }
    pub fn footer_height(self) -> i32 {
        self.keycap_size() + 4
    }
    pub fn control_gap(self) -> i32 {
        4
    }
    pub fn shortcut_gap(self) -> i32 {
        self.sized(6, 6, 12)
    }
    pub fn shadow_height(self) -> i32 {
        self.sized(3, 3, 4)
    }
    pub fn row_height(self) -> i32 {
        self.sized(40, 44, 68)
    }
    pub fn library_row_height(self) -> i32 {
        self.sized(40, 48, 80)
    }
    pub fn library_row_min_height(self) -> i32 {
        i32::from(TITLE_SIZE + self.row_detail)
            + ROW_LEADING * 2
            + self.sized(6, 8, 12)
            + self.shadow_height()
    }
    pub fn row_min_height(self) -> i32 {
        i32::from(self.row_title + self.row_detail)
            + ROW_LEADING * 2
            + self.sized(6, 6, 12)
            + self.shadow_height()
    }
    pub fn row_padding(self) -> Padding {
        Padding::all(self.sized(3, 4, 6))
    }
    pub fn preview_title_height(self) -> i32 {
        i32::from(self.title) * 2 + LABEL_LEADING * 2
    }
    pub fn action_height(self) -> i32 {
        self.sized(32, 37, 54)
    }
    pub fn device_row_height(self) -> i32 {
        self.sized(22, 28, 40)
    }
    pub fn rail_width(self) -> i32 {
        self.sized(2, 2, 3)
    }
    pub fn rule_height(self) -> i32 {
        self.sized(2, 2, 3)
    }
    pub fn choice_art_height(self) -> i32 {
        self.sized(28, 40, 80)
    }
    pub fn walker_height(self) -> i32 {
        self.sized(22, 38, 46)
    }
    pub fn choice_gap(self) -> i32 {
        self.gap + 14
    }
    pub fn card_padding(self) -> Padding {
        Padding {
            top: FOCUS_BORDER.into(),
            bottom: self.sized(4, 4, 8).into(),
            left: FOCUS_BORDER.into(),
            right: FOCUS_BORDER.into(),
        }
    }
    pub fn header_padding(self) -> Padding {
        Padding {
            top: 4.into(),
            bottom: 4.into(),
            left: self.sized(16, 16, 24).into(),
            right: self.sized(16, 16, 24).into(),
        }
    }
    pub fn modal_padding(self) -> Padding {
        let horizontal = Dimension::Percent(if self.compact {
            Fixed::from_ratio(5, 2)
        } else {
            Fixed::from_ratio(15, 4)
        });
        let vertical = Dimension::Percent(if self.compact {
            Fixed::from_ratio(5, 2)
        } else {
            Fixed::from_ratio(20, 3)
        });
        Padding {
            top: vertical,
            bottom: vertical,
            left: horizontal,
            right: horizontal,
        }
    }

    pub fn page_padding(self) -> Padding {
        Padding {
            top: self.sized(8, 8, 12).into(),
            bottom: 4.into(),
            left: self.sized(16, 16, 24).into(),
            right: self.sized(16, 16, 24).into(),
        }
    }

    pub(super) fn for_height(height: u16) -> Self {
        let compact = height < 400;
        Self {
            compact,
            tiny: height < 300,
            title: if compact { BODY_SIZE } else { TITLE_SIZE },
            body: BODY_SIZE,
            small: BODY_SIZE,
            row_title: if compact { BODY_SIZE } else { TITLE_SIZE },
            row_detail: BODY_SIZE,
            gap: if compact { 3 } else { 8 },
            inset: if compact { 8 } else { 18 },
        }
    }
}
