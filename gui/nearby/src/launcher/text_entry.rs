use super::text_input::Edit;
use crate::model::Action;
use mirui::prelude::*;
use mirui::ui::widgets::TextDocument;
use std::collections::VecDeque;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Pinyin,
    Latin,
    Symbols,
}
#[derive(Clone)]
struct Candidate {
    text: String,
    consumed: usize,
}

#[model]
pub struct TextEntry {
    pub document: model!(owned TextDocument),
    #[observe]
    pub mode: Mode,
    pub preedit: String,
    #[observe]
    revision: u64,
    #[observe]
    pub selected: usize,
    #[observe]
    pub candidate_page: usize,
    #[observe]
    pub candidate_count: usize,
    #[observe]
    pub page_size: usize,
    pub error: String,
    #[observe]
    pub prepared: bool,
    engine: Option<inputx_pinyin::PinyinEngine>,
    glyphs: Option<std::collections::BTreeSet<char>>,
    candidates: Vec<Candidate>,
    letter: usize,
    #[observe]
    uppercase: bool,
    edits: VecDeque<Edit>,
}
impl Default for TextEntry {
    fn default() -> Self {
        Self {
            document: TextDocument::new().into(),
            revision: 0,
            mode: Mode::Pinyin,
            preedit: String::new(),
            selected: 0,
            candidate_page: 0,
            candidate_count: 0,
            page_size: 4,
            error: String::new(),
            prepared: false,
            engine: None,
            glyphs: None,
            candidates: vec![],
            letter: 0,
            uppercase: false,
            edits: VecDeque::new(),
        }
    }
}

#[model]
impl TextEntry {
    pub fn begin(&mut self, name: &str) {
        self.prepared = false;
        self.revision += 1;
        let _ = self.document.set_capacity(64);
        let _ = self.document.set_text(name);
        self.mode = Mode::Pinyin;
        self.selected = 0;
        self.letter = 0;
        self.uppercase = false;
        self.preedit.clear();
        self.candidates.clear();
        self.candidate_count = 0;
        self.candidate_page = 0;
        self.edits.clear();
        self.error.clear();
    }
    pub fn retire(&mut self) {
        if !self.prepared
            && self.preedit.is_empty()
            && self.edits.is_empty()
            && self.candidates.is_empty()
        {
            return;
        }
        self.prepared = false;
        self.revision += 1;
        self.preedit.clear();
        self.edits.clear();
        self.candidates.clear();
        self.candidate_count = 0;
    }
    pub fn prepare_editor(&mut self) {
        self.prepared = true;
    }
    pub fn layout_width(&mut self, width: Fixed) {
        let count = if width.to_int() >= 480 { 4 } else { 3 };
        if self.page_size != count {
            self.page_size = count;
            self.candidate_page = 0;
            if self.selected >= 100 {
                self.selected = 100;
            }
        }
    }
    pub fn edit(&self) -> Option<Edit> {
        self.edits.front().cloned()
    }
    pub fn finish_edit(&mut self) {
        self.edits.pop_front();
    }
    pub fn snapshot_preedit(&self) -> String {
        self.preedit.clone()
    }
    pub fn snapshot_error(&self) -> String {
        self.error.clone()
    }
    pub fn snapshot_value(&self) -> String {
        let _ = self.document.revision();
        self.document.read(str::to_owned)
    }
    pub fn snapshot_key_label(&self, index: usize) -> String {
        if index >= 27 {
            return match index {
                27 => "空格",
                28 => {
                    if self.mode == Mode::Symbols {
                        "ABC"
                    } else {
                        "123"
                    }
                }
                29 => "aA",
                30 => {
                    if self.mode == Mode::Pinyin {
                        "中 → 英"
                    } else {
                        "英 → 中"
                    }
                }
                31 => "删除",
                _ => "保存",
            }
            .into();
        }
        if self.mode == Mode::Symbols {
            return "1234567890'\"-_!?@#$%&*().,/"
                .chars()
                .nth(index)
                .unwrap_or(' ')
                .to_string();
        }
        let ch = "qwertyuiopasdfghjkl'zxcvbnm"
            .chars()
            .nth(index)
            .unwrap_or(' ');
        if self.mode == Mode::Latin && self.uppercase {
            ch.to_ascii_uppercase()
        } else {
            ch
        }
        .to_string()
    }
    pub fn snapshot_candidate_label(&self, index: usize) -> String {
        let _ = self.candidate_count;
        self.candidates
            .get(self.candidate_page * self.page_size + index)
            .map(|v| v.text.clone())
            .unwrap_or_default()
    }
    fn composing(&mut self) {
        self.candidate_page = 0;
        self.candidates.clear();
        if !self.preedit.is_empty() {
            let glyphs = self.glyphs.get_or_insert_with(|| {
                include_str!("../../assets/nearby-menu-charset.txt")
                    .chars()
                    .collect()
            });
            let engine = self
                .engine
                .get_or_insert_with(inputx_pinyin::PinyinEngine::new);
            for end in (1..=self.preedit.len()).rev() {
                let raw = &self.preedit[..end];
                if raw.ends_with('\'') {
                    continue;
                }
                let input = raw.replace('\'', "");
                let mut session = inputx_pinyin::Session::new(engine);
                for ch in input.chars() {
                    session.input_char(ch);
                }
                for word in session.candidates() {
                    if word.chars().all(|ch| glyphs.contains(&ch))
                        && !self.candidates.iter().any(|old| old.text == *word)
                        && separated(word, raw)
                    {
                        self.candidates.push(Candidate {
                            text: word.clone(),
                            consumed: end,
                        });
                    }
                }
            }
        }
        self.candidate_count = self.candidates.len();
        if self.selected >= 100 {
            self.selected = if self.candidate_count == 0 {
                self.letter
            } else {
                100 + (self.selected - 100).min(self.candidate_count.min(self.page_size) - 1)
            };
        }
        self.edits.push_back(if self.preedit.is_empty() {
            Edit::Cancel
        } else {
            Edit::Preedit(self.preedit.clone())
        });
    }
    #[model(local)]
    fn insert(&mut self, text: String) -> bool {
        if self.snapshot_value().len() + text.len() > 64 {
            self.error = "名称超过 64 字节".into();
            return false;
        }
        self.edits.push_back(Edit::Insert(text));
        true
    }
    fn commit(&mut self, index: usize) {
        let Some(word) = self.candidates.get(index).cloned() else {
            return;
        };
        if !self.insert(word.text) {
            return;
        }
        self.preedit = self.preedit[word.consumed..]
            .trim_start_matches('\'')
            .into();
        self.selected = self.letter;
        self.composing();
    }
    fn delete(&mut self) {
        if !self.preedit.is_empty() {
            self.preedit.pop();
            self.composing();
        } else {
            self.edits.push_back(Edit::Backspace);
        }
    }
    pub fn choose_candidates(&mut self) {
        if self.candidate_count > 0 {
            self.selected = 100;
        } else {
            self.error = "未找到对应文字".into();
        }
        self.revision += 1;
    }
    pub fn reject(&mut self, message: &str) {
        self.error = message.into();
        self.revision += 1;
    }
    fn request_submit(&mut self) {
        if self.preedit.is_empty() {
            self.edits.push_back(Edit::Submit);
        } else {
            self.choose_candidates();
        }
    }
    pub fn input(&mut self, action: Action) {
        self.revision += 1;
        self.error.clear();
        match action {
            Action::Back if !self.preedit.is_empty() => {
                self.preedit.clear();
                self.selected = self.letter;
                self.composing();
            }
            Action::Back => {}
            Action::Up if self.selected < 10 && self.candidate_count > 0 => {
                self.letter = self.selected;
                self.selected = 100;
            }
            Action::Up => {
                self.selected = if self.selected >= 100 {
                    self.selected
                } else if self.selected >= 30 {
                    20 + (self.selected - 30) * 3
                } else {
                    self.selected.saturating_sub(10)
                };
            }
            Action::Down if self.selected >= 100 => self.selected = self.letter,
            Action::Down => {
                self.selected = if self.selected >= 30 {
                    self.selected
                } else if self.selected >= 20 {
                    30 + (self.selected - 20) / 4
                } else {
                    self.selected + 10
                }
            }
            Action::Left if self.selected >= 100 => {
                self.selected = self.selected.saturating_sub(1).max(100)
            }
            Action::Right if self.selected >= 100 => {
                self.selected = (self.selected + 1).min(
                    100 + self
                        .page_size
                        .min(
                            self.candidate_count
                                .saturating_sub(self.candidate_page * self.page_size),
                        )
                        .saturating_sub(1),
                )
            }
            Action::Left => self.selected = self.selected.saturating_sub(1),
            Action::Right => self.selected = (self.selected + 1).min(32),
            Action::PageUp | Action::PageDown => {
                let pages = self.candidate_count.div_ceil(self.page_size);
                if pages > 0 {
                    self.candidate_page = if action == Action::PageDown {
                        (self.candidate_page + 1) % pages
                    } else {
                        (self.candidate_page + pages - 1) % pages
                    };
                    self.selected = 100;
                }
            }
            Action::Auxiliary => self.delete(),
            Action::Refresh => self.change_mode(),
            Action::Start => self.request_submit(),
            Action::Confirm => {
                let index = self.selected;
                if index >= 100 {
                    self.commit(self.candidate_page * self.page_size + index - 100);
                } else if index < 27 {
                    self.letter = index;
                    let text = self.snapshot_key_label(index);
                    if self.mode == Mode::Pinyin
                        && text.chars().all(|c| c.is_ascii_alphabetic() || c == '\'')
                    {
                        if self.preedit.len() < 32
                            && !(text == "'"
                                && (self.preedit.is_empty() || self.preedit.ends_with('\'')))
                        {
                            self.preedit.push_str(&text);
                            self.composing();
                        }
                    } else {
                        self.insert(text);
                    }
                } else {
                    match index {
                        27 if !self.preedit.is_empty() => {
                            self.commit(self.candidate_page * self.page_size)
                        }
                        27 => {
                            self.insert(" ".into());
                        }
                        28 if self.preedit.is_empty() => {
                            self.mode = if self.mode == Mode::Symbols {
                                Mode::Latin
                            } else {
                                Mode::Symbols
                            }
                        }
                        28 => self.error = "先选词或按 B 取消拼音".into(),
                        29 => self.uppercase = !self.uppercase,
                        30 => self.change_mode(),
                        31 => self.delete(),
                        _ => self.request_submit(),
                    }
                }
            }
            _ => {}
        }
    }
    fn change_mode(&mut self) {
        if !self.preedit.is_empty() {
            self.error = "先选词或按 B 取消拼音".into();
            return;
        }
        self.mode = if self.mode == Mode::Pinyin {
            Mode::Latin
        } else {
            Mode::Pinyin
        };
    }
}

fn separated(word: &str, raw: &str) -> bool {
    if !raw.contains('\'') {
        return true;
    }
    let key = raw.replace('\'', "");
    let separators: Vec<_> = raw
        .match_indices('\'')
        .enumerate()
        .map(|(n, (index, _))| index - n)
        .collect();
    let mut paths = vec![(0, Vec::<usize>::new())];
    for ch in word.chars() {
        let mut next = vec![];
        for (offset, bounds) in paths {
            for reading in inputx_pinyin::char_to_pinyin(ch) {
                if key[offset..].starts_with(&reading) {
                    let end = offset + reading.len();
                    let mut bounds = bounds.clone();
                    bounds.push(end);
                    next.push((end, bounds));
                }
            }
        }
        paths = next;
    }
    paths
        .into_iter()
        .any(|(end, bounds)| end == key.len() && separators.iter().all(|i| bounds.contains(i)))
}

impl TextEntryHandle {
    pub fn value(&self) -> String {
        self.document().revision();
        self.snapshot_value()
    }
    pub fn key_label(&self, index: usize) -> String {
        self.mode();
        self.uppercase();
        self.snapshot_key_label(index)
    }
    pub fn candidate_label(&self, index: usize) -> String {
        self.revision();
        self.snapshot_candidate_label(index)
    }
    pub fn preedit(&self) -> String {
        self.revision();
        self.snapshot_preedit()
    }
    pub fn error(&self) -> String {
        self.revision();
        self.snapshot_error()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submit_waits_for_composition_from_every_keyboard_row_and_save_key() {
        for selected in [0, 15, 26, 32] {
            let mut entry = TextEntry::default();
            entry.begin("");
            entry.preedit = "wo".into();
            entry.composing();
            entry.edits.clear();
            entry.selected = selected;
            entry.input(if selected == 32 {
                Action::Confirm
            } else {
                Action::Start
            });
            assert_eq!(entry.selected, 100);
            assert_eq!(entry.preedit, "wo");
            assert!(!entry.edits.iter().any(|e| matches!(e, Edit::Submit)));
            entry.input(Action::Confirm);
            assert!(entry.preedit.is_empty());
            entry.input(Action::Start);
            assert!(matches!(entry.edits.back(), Some(Edit::Submit)));
        }
    }
}
