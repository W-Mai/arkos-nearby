use super::{Painter, path, timing};
use crate::model::Phase;
use crate::render::{motion::Frame, style::*};
use mirui::{
    prelude::{ColorToken, Path},
    render::command::DrawCommand,
};

static OUTER: Path =
    path!(M 1 5 H 3 V 3 H 6 V 2 H 10 V 3 H 13 V 5 H 15 V 7 H 13 V 5 H 10 V 4 H 6 V 5 H 3 V 7 H 1 Z);
static MIDDLE: Path = path!(M 4 7 H 6 V 6 H 10 V 7 H 12 V 9 H 10 V 8 H 6 V 9 H 4 Z);
static INNER: Path = path!(M 6 9 H 10 V 11 H 6 Z);
static COMPACT_OUTER: Path = path!(M 0 2 H 2 V 1 H 6 V 2 H 8 V 3 H 6 V 2 H 2 V 3 H 0 Z);
static COMPACT_MIDDLE: Path = path!(M 2 3 H 6 V 4 H 2 Z);
static COMPACT_INNER: Path = path!(M 3 4 H 5 V 5 H 3 Z);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Link {
    Searching,
    Approval,
    Checking,
    Connected,
    Closing,
}

impl Link {
    fn phase(phase: Phase) -> Self {
        match phase {
            Phase::ApprovalPending | Phase::ApprovalPendingClient => Self::Approval,
            Phase::PeerPreparing
            | Phase::Checking
            | Phase::Notifying
            | Phase::NetworkConfirming => Self::Checking,
            Phase::NetworkConnected => Self::Connected,
            Phase::Leaving | Phase::Closed | Phase::Failed | Phase::RestoreFailed => Self::Closing,
            _ => Self::Searching,
        }
    }
}

impl<F: FnMut(DrawCommand<'_>)> Painter<'_, F> {
    pub(super) fn network_link(&mut self, y: i32, frame: Frame) {
        let step = self.unit;
        let middle = self.width / 2;
        let port = self.width / 4 + 10;
        let left = port - port % step;
        let right = self.width - left;
        let link = Link::phase(frame.phase);
        let cycle = i32::from(timing::WAVE_MS);
        let time = i32::from(frame.wave.unwrap_or(frame.phase_age)) % cycle;
        let from_left = left + (middle - left) * time / cycle;
        let from_right = self.width - from_left;
        let badge = match link {
            Link::Searching => {
                self.fill(left, y, middle - left, step, DIVIDER);
                self.network_packet(from_left, y, ACCENT);
                (1 + 3 * time / cycle, SHELL, SHELL_TEXT)
            }
            Link::Approval => {
                let gap = 9 * step;
                self.fill(left, y, middle - gap - left, step, WARNING);
                self.fill(middle + gap, y, right - middle - gap, step, WARNING);
                self.network_packet(middle - gap, y, WARNING);
                self.network_packet(middle + gap, y, WARNING);
                (2, SHELL, WARNING)
            }
            Link::Checking => {
                self.fill(left, y, right - left, step, DIVIDER);
                self.network_packet(from_left, y, ACCENT);
                self.network_packet(from_right, y, ACCENT);
                (2 + 2 * time / cycle, SHELL, SHELL_TEXT)
            }
            Link::Connected => {
                let age = i32::from(frame.phase_age).min(240);
                let reach = (middle - left) * age / 240;
                self.fill(middle - reach, y, reach * 2, 2 * step, ACCENT);
                self.network_packet(middle - reach, y, ACCENT);
                self.network_packet(middle + reach, y, ACCENT);
                (3, ACCENT, ON_ACCENT)
            }
            Link::Closing => {
                let gap = 10 * step;
                self.fill(left, y, (middle - gap - left) / 2, step, CLAY);
                self.fill(
                    right - (right - middle - gap) / 2,
                    y,
                    (right - middle - gap) / 2,
                    step,
                    CLAY,
                );
                (0, SHELL, DIVIDER)
            }
        };
        self.wireless_badge(middle, y, badge);
    }

    fn network_packet(&mut self, x: i32, y: i32, color: ColorToken) {
        let step = self.unit;
        self.fill(x - step, y - step, 3 * step, 3 * step, color);
    }

    fn wireless_badge(
        &mut self,
        middle: i32,
        y: i32,
        (arcs, shell, ink): (i32, ColorToken, ColorToken),
    ) {
        let step = self.unit;
        let scale = if y >= 36 * step { 2 * step } else { step };
        let (width, height, dot_y, paths) = if y >= 17 * step {
            (16, 14, 12, [&INNER, &MIDDLE, &OUTER])
        } else {
            (8, 7, 6, [&COMPACT_INNER, &COMPACT_MIDDLE, &COMPACT_OUTER])
        };
        if self.width < width * scale + 2 * step || self.height < height * scale + step {
            return;
        }
        let x = middle - width * scale / 2;
        let y = (y - height * scale - 3 * step).max(0);
        let width = width * scale + 2 * step;
        let height = height * scale;
        self.fill(x - step, y + step, width, height, DIVIDER);
        self.fill(x - step, y, width, height, shell);
        for (level, path) in paths.into_iter().enumerate() {
            self.contour(
                path,
                x,
                y,
                scale,
                if arcs > level as i32 { ink } else { DIVIDER },
            );
        }
        self.fill(middle - scale, y + dot_y * scale, 2 * scale, scale, ink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Page;
    use mirui::prelude::{Fixed, Rect};

    #[test]
    fn network_marks_share_the_same_geometry_at_every_peer_phase() {
        use Phase::*;
        let theme = crate::theme::Palette::Forest.theme();
        for (width, height, unit) in [(480, 96, 1), (440, 63, 1), (480, 64, 2), (2592, 66, 6)] {
            let sample = |frame| {
                let mut ink = Vec::new();
                Painter {
                    theme: &theme,
                    width,
                    height,
                    unit,
                    emit: |cmd: DrawCommand<'_>| match cmd {
                        DrawCommand::Fill { area, color, .. } => {
                            assert!(area.x >= Fixed::ZERO && area.y >= Fixed::ZERO);
                            assert!(area.x + area.w <= Fixed::from_int(width / unit));
                            assert!(area.y + area.h <= Fixed::from_int(height / unit));
                            ink.push((area, color));
                        }
                        DrawCommand::FillPath {
                            transform, paint, ..
                        } => {
                            assert!(transform.tx >= Fixed::ZERO && transform.ty >= Fixed::ZERO);
                            let mirui::render::scene::Paint::Color(color) = paint else {
                                panic!("Unexpected glyph paint")
                            };
                            ink.push((
                                Rect::new(transform.tx, transform.ty, transform.m00, Fixed::ZERO),
                                (*color).into(),
                            ));
                        }
                        _ => unreachable!(),
                    },
                }
                .network_link(height - 24, frame);
                ink
            };
            for (a, b) in [
                (WaitingPeer, Connecting),
                (ApprovalPending, ApprovalPendingClient),
                (PeerPreparing, NetworkConfirming),
                (NetworkConnected, NetworkConnected),
                (NetworkReconnecting, NetworkReconnecting),
            ] {
                for age in [0, 64, 192, 640, 960] {
                    let base = Frame {
                        phase: a,
                        phase_age: age,
                        wave: Some(age),
                        network_only: true,
                        ..Frame::default()
                    };
                    let host = sample(Frame {
                        page: Page::Host,
                        phase: a,
                        host: Fixed::from_ratio(1, 4),
                        guest: Fixed::from_ratio(9, 10),
                        ..base
                    });
                    let guest = sample(Frame {
                        page: Page::Joined,
                        phase: b,
                        host: Fixed::from_ratio(1, 3),
                        guest: Fixed::from_ratio(3, 4),
                        ..base
                    });
                    assert_eq!(host, guest);
                    let paths: Vec<_> = host
                        .iter()
                        .filter(|(area, _)| area.h == Fixed::ZERO)
                        .collect();
                    assert_eq!(paths.len(), 3);
                    let dot = host.last().unwrap().0;
                    assert_eq!(dot.x + dot.w / 2, Fixed::from_int(width / (2 * unit)));
                }
            }
        }
    }
}
