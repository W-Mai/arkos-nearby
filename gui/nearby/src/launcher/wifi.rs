use crate::model::Action;
use howff_menu::wifi::{Page, Request, Secret, View, valid_psk, valid_ssid};
use mirui::{
    anim::{Tween, TweenConfig, ease},
    prelude::Fixed,
};

const TRAVEL_MS: u64 = 1500;
const STRETCH_MS: u16 = 350;

/// A continuous, reversing indeterminate span; it never represents a percentage.
pub(super) fn loading_span(now: u64) -> (Fixed, Fixed) {
    let elapsed = (now % TRAVEL_MS) as u16;
    let sample = |offset: u16| {
        let mut tween = Tween::preset(
            Fixed::ZERO,
            Fixed::ONE,
            TweenConfig::new(TRAVEL_MS as u16 - STRETCH_MS, &ease::IN_OUT_CUBIC),
        );
        tween.tick(elapsed.saturating_sub(offset));
        tween.value()
    };
    let tail = sample(STRETCH_MS) * Fixed::from_ratio(4, 5);
    let head = sample(0) * Fixed::from_ratio(4, 5) + Fixed::from_ratio(1, 5);
    if (now / TRAVEL_MS).is_multiple_of(2) {
        (tail, head)
    } else {
        (Fixed::ONE - head, Fixed::ONE - tail)
    }
}

#[derive(Default)]
pub(super) struct Editor {
    epoch: u64,
    pub selected: usize,
    mode: u8,
    visible: bool,
    pub error: String,
}
pub(super) enum Edit {
    Consumed,
    Forward,
    Send(Request),
    Text(super::text_input::Edit),
}
impl Editor {
    pub fn echo_label(&self) -> &str {
        if self.visible { "隐藏" } else { "显示" }
    }
    pub fn sync(&mut self, view: Option<&View>) -> bool {
        let Some(view) = view else {
            *self = Self::default();
            return true;
        };
        let reset = self.epoch != view.epoch;
        if self.epoch != view.epoch {
            *self = Self {
                epoch: view.epoch,
                ..Default::default()
            };
        }
        if !view.page.keyboard() && !view.page.pending() {
            self.visible = false;
            self.error.clear();
            return true;
        }
        reset
    }
    pub fn label(&self, index: usize, hidden: bool) -> String {
        if index < 30 {
            return self.character(index).to_string();
        }
        match index {
            30 => {
                if self.mode == 1 {
                    "小写"
                } else {
                    "大写"
                }
            }
            31 => match self.mode {
                2 => "#+=",
                3 => "ABC",
                _ => "123",
            },
            32 => "空格",
            33 => "删除",
            _ => {
                if hidden {
                    "下一步"
                } else {
                    "连接"
                }
            }
        }
        .into()
    }
    fn character(&self, index: usize) -> char {
        let text = match self.mode {
            2 => "1234567890!@#$%^&*()_+-=.:,/?~",
            3 => "[]{}\\\";<>|`!@#$%^&*()_+-=.:,/?~",
            _ => "qwertyuiopasdfghjkl'zxcvbnm.-_",
        };
        let ch = text.chars().nth(index).unwrap_or(' ');
        if self.mode == 1 {
            ch.to_ascii_uppercase()
        } else {
            ch
        }
    }
    pub fn text(&self, hidden: bool, value: &str) -> String {
        if value.is_empty() {
            return if hidden {
                "输入网络名称"
            } else {
                "输入 Wi-Fi 密码"
            }
            .into();
        }
        if self.visible || hidden {
            value.into()
        } else {
            "*".repeat(value.chars().count())
        }
    }
    pub fn submit(&mut self, view: &View, value: &str) -> Edit {
        if view.page == Page::Hidden {
            if !valid_ssid(value) {
                self.error = "网络名称需为 1–32 字节".into();
                return Edit::Consumed;
            }
            Edit::Send(Request::Hidden {
                epoch: view.epoch,
                ssid: value.into(),
            })
        } else {
            if !valid_psk(value) {
                self.error = "密码需 8–63 字节或 64 位十六进制".into();
                return Edit::Consumed;
            }
            Edit::Send(Request::Connect {
                epoch: view.epoch,
                secret: Secret::new(value.into()),
            })
        }
    }
    pub fn input(&mut self, view: &View, action: Action) -> Edit {
        if !view.page.keyboard() {
            return Edit::Forward;
        }
        match action {
            Action::Up => {
                self.selected = if self.selected >= 30 {
                    20 + (self.selected - 30) * 2
                } else {
                    self.selected.saturating_sub(10)
                }
            }
            Action::Down => {
                self.selected = if (20..30).contains(&self.selected) {
                    30 + (self.selected - 20) / 2
                } else if self.selected >= 30 {
                    self.selected
                } else {
                    (self.selected + 10).min(29)
                }
            }
            Action::Left => self.selected = self.selected.saturating_sub(1),
            Action::Right => self.selected = (self.selected + 1).min(34),
            Action::Refresh => {
                self.error.clear();
                return Edit::Text(super::text_input::Edit::Backspace);
            }
            Action::Auxiliary => self.visible = !self.visible,
            Action::Start => return Edit::Text(super::text_input::Edit::Submit),
            Action::Confirm => match self.selected {
                0..30 => {
                    self.error.clear();
                    return Edit::Text(super::text_input::Edit::Insert(
                        self.character(self.selected).to_string(),
                    ));
                }
                30 => self.mode = if self.mode == 1 { 0 } else { 1 },
                31 => {
                    self.mode = match self.mode {
                        2 => 3,
                        3 => 0,
                        _ => 2,
                    }
                }
                32 => {
                    self.error.clear();
                    return Edit::Text(super::text_input::Edit::Insert(" ".into()));
                }
                33 => {
                    self.error.clear();
                    return Edit::Text(super::text_input::Edit::Backspace);
                }
                _ => return Edit::Text(super::text_input::Edit::Submit),
            },
            _ => return Edit::Forward,
        }
        Edit::Consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loading_span_is_bounded_and_continuous_at_direction_changes() {
        let mut previous = loading_span(0);
        for now in (16..=6000).step_by(16) {
            let span = loading_span(now);
            assert!(span.0 >= Fixed::ZERO && span.1 <= Fixed::ONE);
            assert!(span.1 - span.0 >= Fixed::from_ratio(19, 100));
            assert!((span.0 - previous.0).abs() < Fixed::from_ratio(1, 20));
            assert!((span.1 - previous.1).abs() < Fixed::from_ratio(1, 20));
            previous = span;
        }
    }
    #[test]
    fn secret_stays_local_while_keys_modes_and_visibility_round_trip() {
        let view = View {
            epoch: 4,
            page: Page::Password,
            ..Default::default()
        };
        let mut editor = Editor::default();
        let mut characters = std::collections::BTreeSet::from([' ']);
        for mode in 0..4 {
            editor.mode = mode;
            characters.extend((0..30).map(|index| editor.character(index)));
        }
        assert!((b' '..=b'~').all(|byte| characters.contains(&char::from(byte))));
        editor.mode = 0;
        editor.sync(Some(&view));
        editor.selected = 19;
        assert!(
            matches!(editor.input(&view, Action::Confirm), Edit::Text(super::super::text_input::Edit::Insert(text)) if text == "'")
        );
        editor.input(&view, Action::Auxiliary);
        assert_eq!(editor.text(false, "'"), "'");
        assert!(matches!(
            editor.input(&view, Action::Refresh),
            Edit::Text(super::super::text_input::Edit::Backspace)
        ));
        let Edit::Send(request) = editor.submit(&view, "'quoted'") else {
            panic!("missing credential request")
        };
        assert!(request.valid());
        assert!(!format!("{request:?}").contains("quoted"));
        assert!(!editor.sync(Some(&View {
            page: Page::Connecting,
            ..view.clone()
        })));
        assert!(!editor.sync(Some(&view)));
        assert!(editor.sync(Some(&View {
            page: Page::List,
            ..view
        })));
    }
}
