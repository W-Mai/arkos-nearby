use mirui::prelude::*;
use mirui::ui::theme::{Theme, ThemeInfo};
#[cfg(any(target_os = "linux", test))]
use std::path::Path;

#[cfg(target_os = "linux")]
pub const SETTINGS: &str = howff_platform::arkos::application::THEME.as_str();

pub(crate) mod actor {
    use mirui::prelude::ColorToken;

    pub const HOST: ColorToken = ColorToken::Custom("actor-host");
    pub const HOST_LIGHT: ColorToken = ColorToken::Custom("actor-host-light");
    pub const GUEST: ColorToken = ColorToken::Custom("actor-guest");
    pub const GUEST_LIGHT: ColorToken = ColorToken::Custom("actor-guest-light");
    pub const SKIN: ColorToken = ColorToken::Custom("actor-skin");
    pub const BOOTS: ColorToken = ColorToken::Custom("actor-boots");
    pub const PACK: ColorToken = ColorToken::Custom("actor-pack");

    // Player identity colors stay consistent when the campsite palette changes.
    pub const COLORS: [(ColorToken, u32); 7] = [
        (HOST, 0xdb603d),
        (HOST_LIGHT, 0xffad68),
        (GUEST, 0x6477cf),
        (GUEST_LIGHT, 0xb5c7ff),
        (SKIN, 0xffe0aa),
        (BOOTS, 0x302b38),
        (PACK, 0xf4c85f),
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Palette {
    #[default]
    Forest,
    Sunset,
    Night,
}

impl Palette {
    pub fn label(self) -> &'static str {
        match self {
            Self::Forest => "森林",
            Self::Sunset => "落日",
            Self::Night => "夜色",
        }
    }
    pub const ALL: [Self; 3] = [Self::Forest, Self::Sunset, Self::Night];

    pub fn id(self) -> &'static str {
        match self {
            Self::Forest => "forest",
            Self::Sunset => "sunset",
            Self::Night => "night",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|theme| theme.id() == value.trim())
    }

    #[cfg(any(target_os = "linux", test))]
    pub fn next(self) -> Self {
        match self {
            Self::Forest => Self::Sunset,
            Self::Sunset => Self::Night,
            Self::Night => Self::Forest,
        }
    }

    #[cfg(any(target_os = "linux", test))]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|value| Self::parse(&value))
            .unwrap_or_default()
    }

    #[cfg(any(target_os = "linux", test))]
    pub fn save(self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        let temporary = path.with_extension("tmp");
        let mut file = std::fs::File::create(&temporary)?;
        writeln!(file, "{}", self.id())?;
        file.sync_all()?;
        std::fs::rename(temporary, path)?;
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    }

    pub fn theme(self) -> Theme {
        use ColorToken::*;
        let values = match self {
            Self::Forest => [
                0xf2eddb, 0x293a31, 0x42664c, 0xe6e7d3, 0xbdc6a9, 0x63705b, 0xc68c3d, 0xad573b,
                0xd6dfc7, 0xaabda0, 0x243f33, 0xf2eddb,
            ],
            Self::Sunset => [
                0xf5e9d4, 0x482f29, 0xa74e37, 0xecddc4, 0xcebaa0, 0x82614f, 0xc18a32, 0x70835c,
                0xe8cdb0, 0xc49e81, 0x623b30, 0xf5e9d4,
            ],
            Self::Night => [
                0x283933, 0xf2e8c9, 0xd2aa65, 0x344b40, 0x58705b, 0xb5c6a4, 0xdcaa56, 0xde8760,
                0x3c5547, 0x577058, 0x1c2e26, 0x1c2e26,
            ],
        };
        let tokens = [
            Surface,
            OnSurface,
            Primary,
            SurfaceVariant,
            Outline,
            OnSurfaceVariant,
            Custom("gold"),
            Custom("clay"),
            Custom("sky"),
            Custom("hill"),
            Custom("deep"),
            OnPrimary,
        ];
        let mut theme = Theme::light().with_info(ThemeInfo::new(self.id(), self.id(), ""));
        for (token, rgb) in tokens.into_iter().zip(values).chain(actor::COLORS) {
            theme.set(
                token,
                Color::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8),
            );
        }
        theme.set(Custom("header-text"), Color::rgb(243, 236, 216));
        theme.set(Custom("header-detail"), Color::rgb(199, 208, 181));
        theme
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_foregrounds_have_readable_contrast_in_every_palette() {
        fn luminance(color: Color) -> f64 {
            let linear = |channel: u8| {
                let value = f64::from(channel) / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            linear(color.r) * 0.2126 + linear(color.g) * 0.7152 + linear(color.b) * 0.0722
        }
        for palette in Palette::ALL {
            let theme = palette.theme();
            for (foreground, background) in [
                (ColorToken::OnPrimary, ColorToken::Primary),
                (ColorToken::OnSurface, ColorToken::SurfaceVariant),
            ] {
                let a = luminance(theme.resolve(foreground));
                let b = luminance(theme.resolve(background));
                let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(
                    ratio >= 4.5,
                    "{palette:?}: {foreground:?} on {background:?} has contrast {ratio}"
                );
            }
        }
    }

    #[test]
    fn preferences_round_trip_and_unknown_values_use_default() {
        let path = std::env::temp_dir().join(format!("nearby-theme-{}", std::process::id()));
        assert_eq!(Palette::load(&path), Palette::Forest);
        for palette in Palette::ALL {
            palette.save(&path).unwrap();
            assert_eq!(Palette::load(&path), palette);
        }
        std::fs::write(&path, "unknown").unwrap();
        assert_eq!(Palette::load(&path), Palette::Forest);
        std::fs::remove_file(path).unwrap();
    }
}
