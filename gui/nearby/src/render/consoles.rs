use mirui::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Platform {
    Unknown,
    Favorites,
    Nes,
    Fds,
    Snes,
    Megadrive,
    Mastersystem,
    Segacd,
    Sega32X,
    Pcengine,
    Arcade,
    Neogeo,
    Cps1,
    Cps2,
    Cps3,
    Psx,
    Gb,
    Gbc,
    Gba,
    Psp,
    Gamegear,
    Lynx,
    Wonderswan,
    Ngp,
    Nds,
    Dreamcast,
    N64,
    Saturn,
    Virtualboy,
    Computer,
    Phone,
    Engine,
    Ports,
}
impl Platform {
    pub(super) fn from_id(id: &str) -> Self {
        match id.to_ascii_lowercase().as_str() {
            "favorites" => Self::Favorites,
            "nes" => Self::Nes,
            "fds" => Self::Fds,
            "snes" => Self::Snes,
            "megadrive" => Self::Megadrive,
            "mastersystem" => Self::Mastersystem,
            "segacd" => Self::Segacd,
            "sega32x" => Self::Sega32X,
            "pcengine" => Self::Pcengine,
            "arcade" => Self::Arcade,
            "neogeo" => Self::Neogeo,
            "cps1" => Self::Cps1,
            "cps2" => Self::Cps2,
            "cps3" => Self::Cps3,
            "psx" => Self::Psx,
            "gb" => Self::Gb,
            "gbc" => Self::Gbc,
            "gba" => Self::Gba,
            "psp" | "ppsspp" | "pspminis" => Self::Psp,
            "gamegear" => Self::Gamegear,
            "atarilynx" => Self::Lynx,
            "wonderswan" | "wonderswancolor" => Self::Wonderswan,
            "ngp" | "ngpc" => Self::Ngp,
            "nds" => Self::Nds,
            "dreamcast" => Self::Dreamcast,
            "n64" | "n64dd" => Self::N64,
            "saturn" => Self::Saturn,
            "virtualboy" => Self::Virtualboy,
            "msx" | "msx2" | "dos" | "msdos" | "amiga" | "amstradcpc" | "apple2" | "bbcmicro"
            | "macintosh" | "vmac" | "atari800" | "atarist" | "c64" | "c16" | "c128" | "vic20"
            | "pc98" | "x68000" | "x1" | "zxspectrum" | "zx81" | "ti99" => Self::Computer,
            "j2me" => Self::Phone,
            "openbor" | "easyrpg" | "pymo" | "pico-8" | "tic-80" | "tic80" | "love2d" | "doom"
            | "wolf" | "scummvm" | "onscripter" | "solarus" | "cavestory" => Self::Engine,
            "ports" => Self::Ports,
            "mame" | "mame2003" | "mame2010" | "hbmame" | "fbneo" | "fbn" | "daphne" | "alg"
            | "atomiswave" | "sega atomiswave" | "naomi" => Self::Arcade,
            "famicom" => Self::Nes,
            "sfc" | "sufami" | "satellaview" | "snesmsu" | "snesmsu1" | "snes-hacks" => Self::Snes,
            "pcenginecd" | "turbografx" | "turbografx16" | "turbografxcd" | "supergrafx" => {
                Self::Pcengine
            }
            "genesis" | "megadrivemsu" | "msumd" => Self::Megadrive,
            "neogeocd" => Self::Neogeo,
            "sgb" => Self::Gb,
            "recent" => Self::Favorites,
            _ => Self::Unknown,
        }
    }
}

mod vectors;

use mirui::{
    anim::{INTERACTIVE, SMOOTH, Spring},
    render::{canvas::Paint, command::DrawCommand, raster::FillRule},
    types::Transform,
    ui::view::PaintCtx,
};

const VIEWBOX: i32 = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MachinePose {
    pub(super) platform: Platform,
    progress: Fixed,
    side: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PreviewFrame {
    pub(super) arriving: Option<MachinePose>,
    pub(super) leaving: Option<MachinePose>,
}

#[derive(Clone, Copy)]
struct Assembly {
    platform: Platform,
    progress: Spring,
    side: i32,
}

impl Assembly {
    fn pose(self) -> MachinePose {
        MachinePose {
            platform: self.platform,
            progress: self.progress.value(),
            side: self.side,
        }
    }
}

#[derive(Default)]
pub(super) struct PreviewMotion {
    arriving: Option<Assembly>,
    leaving: Option<Assembly>,
    index: usize,
}

impl PreviewMotion {
    pub(super) fn select(&mut self, platform: Option<Platform>, index: usize) {
        let side = if index >= self.index { 1 } else { -1 };
        self.index = index;
        if self.arriving.map(|assembly| assembly.platform) == platform {
            return;
        }
        if platform.is_some() && self.leaving.map(|assembly| assembly.platform) == platform {
            std::mem::swap(&mut self.arriving, &mut self.leaving);
            if let Some(assembly) = &mut self.arriving {
                assembly.progress.retarget(Fixed::ONE, Some(SMOOTH));
            }
        } else {
            if let Some(mut old) = self.arriving.take()
                && self
                    .leaving
                    .is_none_or(|leaving| leaving.progress.value() <= old.progress.value())
            {
                if old.progress.is_settled() {
                    old.side = -side;
                }
                self.leaving = Some(old);
            }
            self.arriving = platform.map(|platform| Assembly {
                platform,
                progress: Spring::preset(Fixed::from_ratio(1, 8), Fixed::ONE, SMOOTH),
                side,
            });
        }
        if let Some(leaving) = &mut self.leaving {
            leaving.progress.retarget(Fixed::ZERO, Some(INTERACTIVE));
        }
    }

    pub(super) fn advance(&mut self, delta: u16) {
        if let Some(assembly) = &mut self.arriving {
            assembly.progress.tick(delta);
        }
        if let Some(assembly) = &mut self.leaving {
            assembly.progress.tick(delta);
            if assembly.progress.is_settled() {
                self.leaving = None;
            }
        }
    }

    pub(super) fn frame(&self) -> PreviewFrame {
        PreviewFrame {
            arriving: self.arriving.map(Assembly::pose),
            leaving: self.leaving.map(Assembly::pose),
        }
    }
}

#[component]
struct ConsoleIcon(Platform);

#[compose]
pub(super) fn console(platform: Platform) -> Entity {
    ui! {
        View (grow: 1.0, min_height: 0, min_width: 0) [ConsoleIcon(platform)]
    }
}

#[view(component = ConsoleIcon, invalidation = Visual)]
pub(super) fn console_icon(icon: component!(), paint: paint!()) {
    draw_machine(
        paint,
        MachinePose {
            platform: icon.0,
            progress: Fixed::ONE,
            side: 0,
        },
        false,
    );
}

#[component]
pub(super) struct ConsolePreview {
    pub(super) state: model!(super::state::DisplayState),
}

#[view(component = ConsolePreview, invalidation = Visual)]
pub(super) fn console_preview(
    preview: model!(component.state, watch(platform_preview_frame)),
    paint: paint!(),
) {
    let frame = preview.platform_preview_frame();
    for pose in [frame.leaving, frame.arriving].into_iter().flatten() {
        draw_machine(paint, pose, true);
    }
}

fn draw_machine(paint: &mut PaintCtx<'_>, pose: MachinePose, staged: bool) {
    let rect = paint.rect();
    let stage = if staged {
        rect.w.min(rect.h) * 9 / 10
    } else {
        rect.w.min(rect.h)
    };
    let overall = pose.progress.clamp(Fixed::ZERO, Fixed::ONE);
    let distance = Fixed::ONE - overall;
    let size = stage * (Fixed::ONE - distance / 6);
    let group_offset = if staged {
        distance * (pose.side * 58)
    } else {
        Fixed::ZERO
    };
    let transform = paint
        .transform()
        .compose(&Transform::translate(
            rect.x + (rect.w - size) / 2 + group_offset * stage / VIEWBOX,
            rect.y + (rect.h - size) / 2,
        ))
        .compose(&Transform::scale(size / VIEWBOX, size / VIEWBOX));
    for (index, part) in vectors::machine(pose.platform).iter().enumerate() {
        // The chassis leads. Small parts move across its direction of travel
        // so each reads as a piece rather than a copy of the whole machine.
        let delay = Fixed::from_ratio(index.min(5) as i32, 18);
        let progress = ((overall - delay) / (Fixed::ONE - delay)).clamp(Fixed::ZERO, Fixed::ONE);
        if progress == Fixed::ZERO {
            continue;
        }
        let offset = Fixed::ONE - progress;
        let (x, y) = match index {
            0 => (0, 14),
            1 => (-8, -25),
            _ if index % 2 == 0 => (-23, -38),
            _ => (29, 29),
        };
        let position = transform.compose(&Transform::translate(offset * x * pose.side, offset * y));
        paint.draw(&DrawCommand::FillPath {
            path: &part.path,
            transform: position,
            paint: &Paint::Color(part.color.into()),
            opa: ((progress * 3).min(Fixed::ONE) * 255).round().to_int() as u8,
            fill_rule: FillRule::EvenOdd,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversing_machine_assembly_preserves_the_displayed_poses() {
        let mut preview = PreviewMotion::default();
        preview.select(Some(Platform::Nes), 0);
        for _ in 0..25 {
            preview.advance(16);
        }
        preview.select(Some(Platform::Snes), 1);
        preview.advance(64);
        let before = preview.frame();
        preview.select(Some(Platform::Nes), 0);
        assert_eq!(before.arriving, preview.frame().leaving);
        assert_eq!(before.leaving, preview.frame().arriving);
        for _ in 0..64 {
            preview.advance(16);
        }
        assert!(preview.frame().leaving.is_none());
        assert_eq!(preview.frame().arriving.unwrap().progress, Fixed::ONE);
    }
}
