use crate::model::{Action, LauncherView, View};
use howff_menu::input::{Generation, Sourced};
use mirui::anim::{Tween, TweenConfig, ease};
use mirui::prelude::*;
use mirui::ui::{text_input::TextInputSession, widgets::TextDocument};

pub(crate) mod configuration;
pub(crate) mod management;
pub(crate) mod saves;
pub(crate) mod text_entry;
pub(crate) mod text_input;

const HOLD_MS: u64 = 700;
const FEEDBACK_MS: u64 = 130;
const MAX_FRAME_GAP_MS: u64 = 250;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Action(Action),
    Game {
        key: String,
        multiplayer: bool,
    },
    Wifi(howff_menu::wifi::Request),
    #[cfg(target_os = "linux")]
    Management(howff_menu::management::Request),
    #[cfg(target_os = "linux")]
    Configuration(howff_menu::configuration::Request),
    #[cfg(target_os = "linux")]
    Saves(howff_menu::saves::Request),
}

pub struct InvalidInput;

#[derive(Default)]
struct MenuContent {
    title: String,
    kind: String,
    items: Vec<crate::model::LauncherItem>,
}

#[derive(Default)]
enum Gesture {
    #[default]
    Idle,
    Pressed {
        key: String,
        at: u64,
        previous: u64,
    },
    Consumed,
}

#[model]
pub struct Launcher {
    input_generation: Generation,
    view: LauncherView,
    menu: MenuContent,
    pub(crate) configuration: model!(owned configuration::Editor),
    pub(crate) saves: model!(owned saves::Manager),
    pub(crate) management: model!(owned management::Manager),
    #[observe]
    overlay_revision: u64,
    #[observe]
    title_revision: u64,
    #[observe]
    notice_revision: u64,
    #[observe]
    detail_revision: u64,
    #[observe]
    wifi_view_revision: u64,
    #[observe]
    items_revision: u64,
    #[observe]
    pub progress: u16,
    #[observe]
    pub selection_age: u16,
    #[observe]
    pub level_positions: [Fixed; 2],
    #[observe]
    pub detail_offset: Fixed,
    detail_reading: Option<(howff_menu::details::Section, usize)>,
    detail_scroll_input: u32,
    detail_height: Fixed,
    detail_rows: Vec<Fixed>,
    detail_text_sizes: Vec<[Fixed; 2]>,
    detail_fact_sizes: Vec<Vec<[Fixed; 2]>>,
    wifi_editor: wifi::Editor,
    pub(crate) wifi_document: model!(owned TextDocument),
    input_session: Option<TextInputSession>,
    input_edits: std::collections::VecDeque<text_input::Edit>,
    invalid_input: bool,
    #[observe]
    wifi_revision: u64,
    #[observe]
    wifi_visible_rows: usize,
    #[observe]
    pub wifi_pulse: u16,
    #[observe]
    pub wifi_loading_start: Fixed,
    #[observe]
    pub wifi_loading_end: Fixed,
    #[observe]
    pub wifi_track: Fixed,
    #[observe]
    detail_geometry: u64,
    level_targets: [Option<u8>; 2],
    level_motion: [Option<Tween>; 2],
    level_started: [u64; 2],
    now: u64,
    selected_at: u64,
    gesture: Gesture,
    pending: Option<Intent>,
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            input_generation: Generation::default(),
            view: Default::default(),
            menu: Default::default(),
            configuration: configuration::Editor::default().into(),
            saves: saves::Manager::default().into(),
            management: management::Manager::default().into(),
            overlay_revision: Default::default(),
            title_revision: Default::default(),
            notice_revision: Default::default(),
            detail_revision: Default::default(),
            wifi_view_revision: Default::default(),
            items_revision: Default::default(),
            progress: Default::default(),
            selection_age: Default::default(),
            level_positions: Default::default(),
            detail_offset: Default::default(),
            detail_reading: Default::default(),
            detail_scroll_input: Default::default(),
            detail_height: Default::default(),
            detail_rows: Default::default(),
            detail_text_sizes: Default::default(),
            detail_fact_sizes: Default::default(),
            wifi_editor: Default::default(),
            wifi_document: TextDocument::new().into(),
            input_session: None,
            input_edits: std::collections::VecDeque::new(),
            invalid_input: false,
            wifi_revision: Default::default(),
            wifi_visible_rows: Default::default(),
            wifi_pulse: Default::default(),
            wifi_loading_start: Default::default(),
            wifi_loading_end: Default::default(),
            wifi_track: Default::default(),
            detail_geometry: Default::default(),
            level_targets: Default::default(),
            level_motion: Default::default(),
            level_started: Default::default(),
            now: 0,
            selected_at: 0,
            gesture: Default::default(),
            pending: None,
        }
    }
}

#[model]
impl Launcher {
    #[observe]
    pub fn enabled(&self) -> bool {
        self.view.enabled
    }
    #[observe]
    pub fn tab(&self) -> usize {
        self.view.tab
    }
    #[observe]
    pub fn overlay_visible(&self) -> bool {
        self.view.enabled && !self.view.overlay.is_empty()
    }
    fn snapshot_overlay_title(&self) -> String {
        if self.view.overlay.is_empty() {
            self.menu.title.clone()
        } else {
            self.view.overlay.clone()
        }
    }
    fn snapshot_overlay_kind(&self) -> String {
        self.menu.kind.clone()
    }
    #[observe]
    pub fn selected(&self) -> usize {
        self.view.selected
    }
    #[observe]
    pub fn item_count(&self) -> usize {
        self.menu.items.len()
    }
    fn snapshot_confirm_label(&self) -> String {
        match self.view.details.as_ref() {
            Some(details) if details.expanded => "收起",
            Some(details) if details.section == howff_menu::details::Section::Storage => "查看详情",
            Some(details) if details.section != howff_menu::details::Section::Overview => {
                "查看来源"
            }
            _ => "确认",
        }
        .into()
    }
    fn snapshot_notice(&self) -> String {
        self.view.notice.clone()
    }
    #[observe]
    pub fn menu_selection(&self) -> Option<usize> {
        if self.management.active() {
            return Some(self.management.focus());
        }
        if self.saves.active() {
            return Some(self.saves.selected());
        }
        if self.configuration.active() {
            return Some(self.configuration.selected());
        }
        let _ = self.wifi_revision;
        if let Some(view) = self.view.wifi.as_ref() {
            return Some(if view.page.keyboard() {
                self.wifi_editor.selected
            } else {
                view.selected
            });
        }
        (self.view.enabled && !self.view.overlay.is_empty()).then_some(self.view.selected)
    }
    #[observe]
    pub fn wifi_active(&self) -> bool {
        self.view.wifi.is_some()
    }
    #[observe]
    pub fn wifi_page(&self) -> howff_menu::wifi::Page {
        self.view.wifi.as_ref().map(|v| v.page).unwrap_or_default()
    }

    #[observe]
    pub fn wifi_retained(&self) -> bool {
        self.view.retained
    }
    #[observe]
    pub fn wifi_keyboard(&self) -> bool {
        self.view.wifi.as_ref().is_some_and(|v| v.page.keyboard())
    }
    #[observe]
    pub fn wifi_stage(&self) -> u8 {
        self.view.wifi.as_ref().map(|v| v.stage).unwrap_or(0).min(2)
    }
    #[observe]
    pub fn wifi_scanning(&self) -> bool {
        self.view.wifi.as_ref().is_some_and(|v| v.scanning)
    }
    #[observe]
    pub fn wifi_length(&self) -> usize {
        let _ = self.wifi_document.revision();
        self.wifi_document.len()
    }
    pub(crate) fn text_session(&mut self, session: Option<TextInputSession>) {
        self.input_session = session;
    }
    pub(crate) fn input_session(&self) -> Option<TextInputSession> {
        self.input_session
    }
    pub(crate) fn input_edit(&self) -> Option<text_input::Edit> {
        self.input_edits.front().cloned()
    }
    pub(crate) fn finish_input_edit(&mut self) {
        self.input_edits.pop_front();
    }
    pub fn wifi_track_width(&mut self, width: Fixed) {
        self.wifi_track = width;
    }
    pub fn wifi_height(&mut self, height: Fixed, extent: i32, gap: i32) {
        self.wifi_visible_rows =
            ((height.to_int() + gap).max(0) as usize / extent.max(1) as usize).clamp(1, 8);
    }
    fn snapshot_wifi_window(&self) -> std::ops::Range<usize> {
        let Some(view) = self.view.wifi.as_ref() else {
            return 0..0;
        };
        let count = view.networks.len() + 1;
        let rows = self.wifi_visible_rows.max(1).min(count);
        let first = view.selected.saturating_sub(rows - 1).min(count - rows);
        first..(first + rows)
    }
    fn snapshot_wifi_text(&self, index: usize) -> String {
        let Some(view) = self.view.wifi.as_ref() else {
            return String::new();
        };
        match index {
            0 => view.ssid.clone(),
            1 => {
                if self.wifi_editor.error.is_empty() {
                    view.notice.clone()
                } else {
                    self.wifi_editor.error.clone()
                }
            }
            _ => String::new(),
        }
    }

    #[observe]
    pub fn detail_active(&self) -> bool {
        self.view.overlay_kind == "details"
    }

    #[observe]
    pub fn detail_expanded(&self) -> bool {
        self.view
            .details
            .as_ref()
            .is_some_and(|details| details.expanded)
    }

    #[observe]
    pub fn detail_count(&self) -> usize {
        if self.view.overlay_kind == "details" {
            self.view.items.len()
        } else {
            0
        }
    }

    fn snapshot_detail_heading(&self) -> String {
        match self.view.details.as_ref().map(|details| &details.load) {
            Some(howff_menu::details::Load::Ready { document }) => document.title.clone(),
            Some(howff_menu::details::Load::Failed { message }) => {
                format!("读取未完成：{message}")
            }
            _ => {
                if self
                    .view
                    .details
                    .as_ref()
                    .is_some_and(|details| details.section == howff_menu::details::Section::Storage)
                {
                    "正在读取存储空间…".into()
                } else {
                    "正在读取游戏信息…".into()
                }
            }
        }
    }

    fn snapshot_detail_platform(&self) -> String {
        match self.view.details.as_ref().map(|details| &details.load) {
            Some(howff_menu::details::Load::Ready { document }) => document.platform.clone(),
            _ => String::new(),
        }
    }

    fn snapshot_detail_fact(&self, index: usize, field: usize) -> howff_menu::details::Fact {
        self.detail_row_data(index)
            .and_then(|row| row.facts.get(field))
            .cloned()
            .unwrap_or_default()
    }

    fn snapshot_detail_fact_count(&self, index: usize) -> usize {
        self.detail_row_data(index).map_or(0, |row| row.facts.len())
    }

    fn snapshot(&self) -> LauncherView {
        self.view.clone()
    }
    fn snapshot_item(&self, index: usize) -> Option<crate::model::LauncherItem> {
        self.menu.items.get(index).cloned()
    }
    fn snapshot_level(&self, index: usize) -> Option<howff_menu::Level> {
        self.menu.items.get(index).and_then(|item| item.level)
    }
    pub fn present(&mut self, view: &View) {
        if self.input_generation != view.input_generation {
            self.cancel();
            self.pending = None;
            self.input_edits.clear();
            self.input_generation = view.input_generation;
        }
        self.management.bind_input(view.input_generation);
        self.configuration.bind_input(view.input_generation);
        self.saves.bind_input(view.input_generation);
        self.management.sync(view.launcher.management.as_ref());
        self.configuration
            .sync(view.launcher.configuration.as_ref());
        self.saves.sync(view.launcher.saves.as_ref());
        if self.wifi_document.capacity() != 64 {
            let _ = self.wifi_document.set_capacity(64);
        }
        if self.wifi_editor.sync(view.launcher.wifi.as_ref()) {
            let _ = self.wifi_document.set_text("");
            self.input_edits.clear();
        }
        if !view
            .launcher
            .wifi
            .as_ref()
            .is_some_and(|wifi| wifi.page.keyboard())
        {
            self.input_edits.clear();
        }
        if self.view.details.as_ref().map(|details| details.section)
            != view
                .launcher
                .details
                .as_ref()
                .map(|details| details.section)
        {
            self.detail_rows.clear();
            self.detail_text_sizes.clear();
            self.detail_fact_sizes.clear();
            self.detail_geometry += 1;
            self.detail_offset = Fixed::ZERO;
        }
        for index in 0..2 {
            let target = view
                .launcher
                .items
                .get(index)
                .and_then(|item| item.level)
                .and_then(|level| level.value);
            if self.level_targets[index] != target {
                let value = self.level_value(index, self.now);
                self.level_targets[index] = target;
                self.level_started[index] = self.now;
                self.level_motion[index] = target.map(|target| {
                    Tween::preset(
                        value,
                        Fixed::from_int(i32::from(target)),
                        TweenConfig::new(150, &ease::OUT_CUBIC),
                    )
                });
            }
        }
        if self.view.overlay != view.launcher.overlay
            || self.view.selected != view.launcher.selected
        {
            self.selected_at = self.now;
        }
        if self.view.key != view.launcher.key
            || self.view.navigation != view.launcher.navigation
            || self.view.overlay != view.launcher.overlay
            || self.view.enabled != view.launcher.enabled
        {
            self.cancel();
        }
        if self.view != view.launcher {
            let previous_title = self.snapshot_overlay_title();
            let next = &view.launcher;
            // Hidden menus and child editors do not replace the retained menu payload.
            // Input dispatch still reads the current authoritative View below.
            if next.enabled
                && !next.overlay.is_empty()
                && next.configuration.is_none()
                && next.saves.is_none()
                && next.management.is_none()
                && next.wifi.is_none()
            {
                if self.menu.items != next.items {
                    self.menu.items.clone_from(&next.items);
                    self.items_revision += 1;
                }
                self.menu.title.clone_from(&next.overlay);
                if self.menu.kind != next.overlay_kind {
                    self.menu.kind.clone_from(&next.overlay_kind);
                    self.overlay_revision += 1;
                }
            }
            if self.view.notice != view.launcher.notice {
                self.notice_revision += 1;
            }
            if self.view.details != view.launcher.details {
                self.detail_revision += 1;
            }
            if self.view.wifi != view.launcher.wifi {
                self.wifi_view_revision += 1;
            }
            self.view = view.launcher.clone();
            if self.snapshot_overlay_title() != previous_title {
                self.title_revision += 1;
            }
            self.update_detail_offset();
        }
    }

    pub fn detail_viewport(&mut self, height: Fixed) {
        self.detail_height = height;
        self.update_detail_offset();
    }

    pub fn detail_measure(&mut self, index: usize, height: Fixed) {
        if index >= self.view.items.len() {
            return;
        }
        self.detail_rows.resize(self.view.items.len(), Fixed::ZERO);
        self.detail_rows[index] = height;
        self.update_detail_offset();
    }

    pub fn detail_text_size(&mut self, index: usize, part: usize, height: Fixed) {
        if index >= self.view.items.len() || part >= 2 {
            return;
        }
        self.detail_text_sizes
            .resize(self.view.items.len(), [Fixed::ZERO; 2]);
        if self.detail_text_sizes[index][part] != height {
            self.detail_text_sizes[index][part] = height;
            self.detail_geometry += 1;
        }
    }

    pub fn detail_fact_measure(&mut self, index: usize, field: usize, value: bool, height: Fixed) {
        let count = self.snapshot_detail_fact_count(index);
        if index >= self.view.items.len() || field >= count {
            return;
        }
        self.detail_fact_sizes
            .resize_with(self.view.items.len(), Vec::new);
        self.detail_fact_sizes[index].resize(count, [Fixed::ZERO; 2]);
        let part = usize::from(value);
        if self.detail_fact_sizes[index][field][part] != height {
            self.detail_fact_sizes[index][field][part] = height;
            self.detail_geometry += 1;
        }
    }

    fn snapshot_detail_fact_height(&self, index: usize, field: usize) -> Fixed {
        self.detail_fact_sizes
            .get(index)
            .and_then(|sizes| sizes.get(field))
            .map_or(Fixed::from_int(14), |sizes| {
                sizes[0].max(sizes[1]).max(Fixed::from_int(14))
            })
    }

    fn snapshot_detail_facts_size(&self, index: usize, gap: i32) -> Fixed {
        let count = self.snapshot_detail_fact_count(index);
        (0..count).fold(Fixed::ZERO, |total, field| {
            total + self.snapshot_detail_fact_height(index, field)
        }) + Fixed::from_int(count as i32 + gap * count.saturating_sub(1) as i32)
    }

    fn snapshot_detail_size(&self, index: usize, gap: i32) -> Fixed {
        let sizes = self
            .detail_text_sizes
            .get(index)
            .copied()
            .unwrap_or_default();
        let expanded = self.view.selected == index
            && self
                .view
                .details
                .as_ref()
                .is_some_and(|details| details.expanded);
        sizes[0].max(Fixed::from_int(26))
            + sizes[1].max(Fixed::from_int(14))
            + if expanded {
                self.snapshot_detail_facts_size(index, gap) + Fixed::from_int(9)
            } else {
                Fixed::ZERO
            }
            + Fixed::from_int(gap * 2 + 8)
    }

    fn update_detail_offset(&mut self) {
        let Some(details) = self.view.details.as_ref() else {
            self.detail_reading = None;
            self.detail_scroll_input = 0;
            return;
        };
        let selected = self.view.selected.min(self.detail_rows.len());
        let top = self
            .detail_rows
            .iter()
            .take(selected)
            .fold(Fixed::ZERO, |total, height| total + *height);
        let height = self.detail_rows.get(selected).copied().unwrap_or_default();
        let total = self
            .detail_rows
            .iter()
            .fold(Fixed::ZERO, |total, height| total + *height);
        let maximum = (total - self.detail_height).max(Fixed::ZERO);
        let offset = if details.expanded {
            let reading = Some((details.section, selected));
            let offset = if self.detail_reading == reading {
                self.detail_offset
                    + Fixed::from_int(details.scroll as i32 - self.detail_scroll_input as i32)
            } else {
                top + Fixed::from_int(details.scroll as i32)
            };
            self.detail_reading = reading;
            self.detail_scroll_input = details.scroll;
            offset.max(top.min(maximum))
        } else if top < self.detail_offset {
            top
        } else if top + height > self.detail_offset + self.detail_height {
            top + height - self.detail_height
        } else {
            self.detail_offset
        };
        if !details.expanded {
            self.detail_reading = None;
            self.detail_scroll_input = 0;
        }
        self.detail_offset = offset.clamp(Fixed::ZERO, maximum);
    }

    pub fn blocked(&self) -> bool {
        self.view.enabled && (self.view.navigation || !self.view.overlay.is_empty())
    }

    pub fn cancel(&mut self) {
        if !matches!(self.gesture, Gesture::Idle) {
            self.gesture = Gesture::Consumed;
        }
        self.progress = 0;
    }

    pub fn press(&mut self, at: u64) {
        if !matches!(self.gesture, Gesture::Idle) {
            return;
        }
        if self.management.active() {
            self.pending = Some(Intent::Action(Action::Refresh));
            self.gesture = Gesture::Consumed;
            return;
        }
        if self.saves.active() {
            self.saves.input(Action::Refresh);
            self.gesture = Gesture::Consumed;
            return;
        }
        if self.view.configuration.is_some() {
            self.configuration.input(Action::Refresh);
            self.gesture = Gesture::Consumed;
            return;
        }
        if self.view.wifi.is_some() {
            self.pending = Some(Intent::Action(Action::Refresh));
            self.gesture = Gesture::Consumed;
            return;
        }
        if !self.view.enabled {
            self.pending = Some(Intent::Action(Action::Refresh));
            self.gesture = Gesture::Consumed;
        } else if !self.blocked() && !self.view.key.is_empty() {
            self.gesture = Gesture::Pressed {
                key: self.view.key.clone(),
                at,
                previous: at,
            };
        } else {
            self.gesture = Gesture::Consumed;
        }
    }

    pub fn release(&mut self, at: u64) {
        self.tick(at);
        if let Gesture::Pressed { key, .. } = std::mem::take(&mut self.gesture) {
            self.pending = Some(Intent::Game {
                key,
                multiplayer: false,
            });
        }
        self.progress = 0;
    }

    pub fn tick(&mut self, now: u64) {
        self.now = now;
        self.configuration.tick(now);
        self.saves.tick(now);
        self.management.tick(now);
        if self.wifi_page().pending() {
            let (start, end) = wifi::loading_span(now);
            self.wifi_loading_start = start;
            self.wifi_loading_end = end;
        }
        self.wifi_pulse = if self.wifi_scanning() {
            ((now / 125) % 4) as u16
        } else {
            0
        };
        self.level_positions = std::array::from_fn(|index| self.level_value(index, now));
        self.selection_age = if self.saves.active() {
            self.saves.age()
        } else if self.configuration.active() {
            self.configuration.age()
        } else {
            now.saturating_sub(self.selected_at).min(800) as u16
        };
        let Gesture::Pressed { key, at, previous } = &mut self.gesture else {
            return;
        };
        if now.saturating_sub(*previous) > MAX_FRAME_GAP_MS {
            self.cancel();
            return;
        }
        *previous = now;
        let elapsed = now.saturating_sub(*at);
        if elapsed >= HOLD_MS {
            self.pending = Some(Intent::Game {
                key: key.clone(),
                multiplayer: true,
            });
            self.gesture = Gesture::Consumed;
            self.progress = 0;
        } else if elapsed >= FEEDBACK_MS {
            self.progress = (elapsed * 1000 / HOLD_MS) as u16;
        }
    }

    #[effects]
    fn take_input(&mut self) -> Option<Sourced<Intent>> {
        self.take_intent()
            .map(|intent| self.input_generation.bind(intent))
    }

    #[cfg(any(target_os = "linux", test))]
    pub fn saves_input(&mut self, action: Action) {
        self.saves.input(action);
    }
    pub fn management_input(&mut self, action: Action) {
        self.management.input(action);
        self.selected_at = self.now;
    }
    pub fn configuration_input(&mut self, action: Action) {
        self.configuration.input(action);
    }
    pub fn wifi_input(&mut self, action: Action) {
        let Some(view) = self.view.wifi.as_ref() else {
            return;
        };
        match self.wifi_editor.input(view, action) {
            wifi::Edit::Forward => {}
            result => {
                if let wifi::Edit::Send(request) = result {
                    self.pending = Some(Intent::Wifi(request));
                } else if let wifi::Edit::Text(edit) = result
                    && self.input_edits.len() < 64
                {
                    self.input_edits.push_back(edit);
                }
                self.wifi_revision += 1;
                self.selected_at = self.now;
            }
        }
    }
    pub fn wifi_submit(&mut self) {
        let Some(view) = self.view.wifi.as_ref().filter(|view| view.page.keyboard()) else {
            return;
        };
        let text = self.wifi_document.read(str::to_owned);
        let previous_error = self.wifi_editor.error.clone();
        if let wifi::Edit::Send(request) = self.wifi_editor.submit(view, &text) {
            self.pending = Some(Intent::Wifi(request));
        }
        self.invalid_input =
            !self.wifi_editor.error.is_empty() && self.wifi_editor.error != previous_error;
        self.wifi_revision += 1;
    }

    #[effects]
    fn take_invalid_input(&mut self) -> Option<InvalidInput> {
        std::mem::take(&mut self.invalid_input).then_some(InvalidInput)
    }
    fn snapshot_wifi_editor(&self, index: usize) -> String {
        let hidden = self
            .view
            .wifi
            .as_ref()
            .is_some_and(|v| v.page == howff_menu::wifi::Page::Hidden);
        if index == 35 {
            let _ = self.wifi_document.revision();
            self.wifi_document
                .read(|text| self.wifi_editor.text(hidden, text))
        } else if index == 36 {
            self.wifi_editor.error.clone()
        } else {
            self.wifi_editor.label(index, hidden)
        }
    }
    fn snapshot_wifi_row(&self, index: usize) -> Option<howff_menu::wifi::Network> {
        self.view.wifi.as_ref()?.networks.get(index).cloned()
    }
    fn snapshot_wifi_info(&self) -> Option<howff_menu::wifi::Info> {
        self.view.wifi.as_ref()?.info.clone()
    }
    fn snapshot_wifi_echo(&self) -> String {
        self.wifi_editor.echo_label().into()
    }
}

impl Launcher {
    fn level_value(&self, index: usize, now: u64) -> Fixed {
        self.level_motion[index].map_or(Fixed::ZERO, |mut tween| {
            tween.tick(
                now.saturating_sub(self.level_started[index])
                    .min(u64::from(tween.duration_ms)) as u16,
            );
            tween.value()
        })
    }
}

impl LauncherHandle {
    pub fn overlay_title(&self) -> String {
        self.title_revision();
        self.snapshot_overlay_title()
    }
    pub fn overlay_kind(&self) -> String {
        self.overlay_revision();
        self.snapshot_overlay_kind()
    }
    pub fn confirm_label(&self) -> String {
        self.detail_revision();
        self.snapshot_confirm_label()
    }
    pub fn notice(&self) -> String {
        self.notice_revision();
        self.snapshot_notice()
    }
    pub fn wifi_echo(&self) -> String {
        self.wifi_revision();
        self.snapshot_wifi_echo()
    }
    pub fn wifi_window(&self) -> std::ops::Range<usize> {
        self.wifi_view_revision();
        self.wifi_visible_rows();
        self.snapshot_wifi_window()
    }
    pub fn wifi_text(&self, index: usize) -> String {
        self.wifi_view_revision();
        self.wifi_revision();
        self.snapshot_wifi_text(index)
    }
    pub fn wifi_editor(&self, index: usize) -> String {
        self.wifi_view_revision();
        self.wifi_revision();
        self.snapshot_wifi_editor(index)
    }
    pub fn wifi_row(&self, index: usize) -> Option<howff_menu::wifi::Network> {
        self.wifi_view_revision();
        self.snapshot_wifi_row(index)
    }
    pub fn wifi_info(&self) -> Option<howff_menu::wifi::Info> {
        self.wifi_view_revision();
        self.snapshot_wifi_info()
    }
    pub fn level(&self, index: usize) -> Option<howff_menu::Level> {
        self.items_revision();
        self.snapshot_level(index)
    }

    pub fn item(&self, index: usize) -> Option<crate::model::LauncherItem> {
        self.items_revision();
        self.snapshot_item(index)
    }
    // Full snapshots are only for imperative consumers; bindings use observed selectors.
    pub fn view(&self) -> LauncherView {
        self.snapshot()
    }

    pub fn detail_heading(&self) -> String {
        self.detail_revision();
        self.snapshot_detail_heading()
    }

    pub fn detail_platform(&self) -> String {
        self.detail_revision();
        self.snapshot_detail_platform()
    }
    pub fn detail_fact_count(&self, index: usize) -> usize {
        self.detail_revision();
        self.snapshot_detail_fact_count(index)
    }
    pub fn detail_fact(&self, index: usize, field: usize) -> howff_menu::details::Fact {
        self.detail_revision();
        self.snapshot_detail_fact(index, field)
    }
    pub fn detail_fact_height(&self, index: usize, field: usize) -> Fixed {
        self.detail_geometry();
        self.snapshot_detail_fact_height(index, field)
    }
    pub fn detail_facts_size(&self, index: usize, gap: i32) -> Fixed {
        self.detail_revision();
        self.detail_geometry();
        self.snapshot_detail_facts_size(index, gap)
    }

    pub fn detail_size(&self, index: usize, gap: i32) -> Fixed {
        self.detail_revision();
        self.detail_geometry();
        self.selected();
        self.snapshot_detail_size(index, gap)
    }
}

impl Launcher {
    fn detail_row_data(&self, index: usize) -> Option<&howff_menu::details::Row> {
        self.view
            .details
            .as_ref()
            .and_then(|details| match &details.load {
                howff_menu::details::Load::Ready { document } => {
                    document.rows(details.section).get(index)
                }
                _ => None,
            })
    }
}
mod wifi;

#[cfg(target_os = "linux")]
pub mod input;

#[cfg(any(target_os = "linux", test))]
mod controls;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_menu_content_survives_child_editors_without_retaining_input_authority() {
        let mut model = Launcher::default();
        let mut current = View::default();
        current.launcher.enabled = true;
        current.launcher.overlay = "Game".into();
        current.launcher.overlay_kind = "game".into();
        current.launcher.items = vec![crate::model::LauncherItem::available(
            crate::model::MenuAction::Play,
            "Play",
        )];
        model.present(&current);
        let menu = current.clone();
        let revisions = (model.items_revision, model.overlay_revision);
        current.launcher.overlay.clear();
        current.launcher.overlay_kind.clear();
        current.launcher.items.clear();
        current.input_generation.advance();
        model.present(&current);
        assert_eq!(model.snapshot(), current.launcher);
        assert!(!model.overlay_visible());
        assert!(model.menu_selection().is_none());
        assert!(model.view.selected_action().is_none());
        assert_eq!(model.item_count(), 1);
        assert_eq!(model.snapshot_item(0), menu.launcher.items.first().cloned());
        assert_eq!((model.items_revision, model.overlay_revision), revisions);
        current.launcher.overlay = "Configuration".into();
        current.launcher.configuration = Some(Default::default());
        model.present(&current);
        assert_eq!(model.snapshot(), current.launcher);
        assert_eq!(model.snapshot_overlay_title(), "Configuration");
        assert_eq!((model.items_revision, model.overlay_revision), revisions);
        current = menu.clone();
        model.present(&current);
        assert_eq!((model.items_revision, model.overlay_revision), revisions);
        current.launcher.overlay = "Empty menu".into();
        current.launcher.overlay_kind = "tools".into();
        current.launcher.items.clear();
        model.present(&current);
        assert_eq!(
            model.item_count(),
            0,
            "An active empty menu replaces old content"
        );
        assert_eq!(model.items_revision, revisions.0 + 1);
    }

    #[test]
    fn focus_and_gestures_do_not_publish_menu_content_or_structure_changes() {
        use mirui::core::reactive::{Effect, flush_signal_dirty};
        use std::{cell::Cell, rc::Rc};

        let mut app = App::headless(64, 64);
        let model = app.add_model(Launcher::default()).finish();
        let mut view = View::default();
        view.launcher.enabled = true;
        view.launcher.overlay = "Settings".into();
        view.launcher.overlay_kind = "system".into();
        view.launcher.items = vec![
            crate::model::LauncherItem::available(crate::model::MenuAction::CycleTheme, "Theme"),
            crate::model::LauncherItem::available(crate::model::MenuAction::ToggleSound, "Sound"),
        ];
        model.present(&view);
        flush_signal_dirty(&mut app.world);
        let counts = Rc::new(Cell::new([0; 4]));
        let _effects: Vec<_> = (0..4)
            .map(|index| {
                let model = model.clone();
                let counts = counts.clone();
                Effect::new(move || {
                    match index {
                        0 => {
                            let _ = model.item(0);
                        }
                        1 => {
                            let _ = (
                                model.overlay_visible(),
                                model.overlay_kind(),
                                model.item_count(),
                            );
                        }
                        2 => {
                            let _ = model.selected();
                        }
                        _ => {
                            let _ = model.notice();
                        }
                    }
                    let mut next = counts.get();
                    next[index] += 1;
                    counts.set(next);
                })
            })
            .collect();

        view.launcher.selected = 1;
        view.launcher.key = "different-game".into();
        model.present(&view);
        model.tick(100);
        flush_signal_dirty(&mut app.world);
        assert_eq!(counts.get(), [1, 1, 2, 1]);
        view.launcher.notice = "Updated".into();
        model.present(&view);
        flush_signal_dirty(&mut app.world);
        assert_eq!(counts.get(), [1, 1, 2, 2]);
        view.launcher.items[0].name = "Changed theme".into();
        model.present(&view);
        flush_signal_dirty(&mut app.world);
        assert_eq!(counts.get(), [2, 1, 2, 2]);
        view.launcher.items.pop();
        model.present(&view);
        flush_signal_dirty(&mut app.world);
        assert_eq!(counts.get(), [3, 2, 2, 2]);
        view.launcher.overlay.clear();
        model.present(&view);
        flush_signal_dirty(&mut app.world);
        assert_eq!(counts.get(), [3, 3, 2, 2]);
    }

    #[test]
    fn page_binding_cancels_old_gestures_and_preserves_already_emitted_origins() {
        let mut model = Launcher::default();
        let mut view = View {
            input_generation: Generation::FIRST,
            ..Default::default()
        };
        view.launcher.enabled = true;
        view.launcher.key = "a".repeat(64);
        model.present(&view);
        model.press(100);
        model.release(200);
        let queued = model.take_input().unwrap();
        model.press(300);
        model.pending = Some(Intent::Action(Action::Confirm));
        view.input_generation.advance();
        model.present(&view);
        model.release(400);
        assert!(model.take_input().is_none());
        assert_eq!(queued.generation, Generation::FIRST);
        assert!(matches!(
            queued.payload,
            Intent::Game {
                multiplayer: false,
                ..
            }
        ));
        model.press(500);
        model.release(600);
        assert_eq!(
            model.take_input().unwrap().generation,
            view.input_generation
        );
    }
    #[test]
    fn wifi_keyboard_emits_one_private_model_effect_and_x_never_launches_a_game() {
        let mut model = Launcher::default();
        let mut view = View::default();
        view.launcher.enabled = true;
        view.launcher.wifi = Some(howff_menu::wifi::View {
            epoch: 4,
            page: howff_menu::wifi::Page::Password,
            ..Default::default()
        });
        model.present(&view);
        for _ in 0..8 {
            model.wifi_input(Action::Confirm);
        }
        assert_eq!(model.input_edits.len(), 8);
        assert_eq!(model.wifi_length(), 0);
        model.wifi_document.set_text("qqqqqqqq").unwrap();
        model.wifi_submit();
        let Some(Intent::Wifi(request)) = model.take_intent() else {
            panic!("Missing private credential effect")
        };
        assert!(request.valid());
        assert_eq!(request.epoch(), 4);
        assert!(!format!("{request:?}").contains("qqqqqqqq"));
        assert!(
            !serde_json::to_string(&model.view)
                .unwrap()
                .contains("qqqqqqqq")
        );
        model.press(1000);
        assert_eq!(model.take_intent(), Some(Intent::Action(Action::Refresh)));
        model.release(1200);
        assert!(model.take_intent().is_none());
    }

    #[test]
    fn detail_scroll_reverses_immediately_after_repeated_input_at_the_limit() {
        let mut model = Launcher::default();
        model.view.overlay_kind = "details".into();
        model.view.items = vec![crate::model::LauncherItem::default(); 5];
        model.view.details = Some(howff_menu::details::View {
            section: howff_menu::details::Section::Saves,
            expanded: true,
            ..Default::default()
        });
        model.detail_rows = vec![Fixed::from_int(80); 5];
        model.detail_height = Fixed::from_int(100);
        model.update_detail_offset();
        model.view.details.as_mut().unwrap().scroll = 8192;
        model.update_detail_offset();
        assert_eq!(model.detail_offset, Fixed::from_int(300));
        model.view.details.as_mut().unwrap().scroll = 8168;
        model.update_detail_offset();
        assert_eq!(model.detail_offset, Fixed::from_int(276));
        model.update_detail_offset();
        assert_eq!(model.detail_offset, Fixed::from_int(276));
        model.view.details.as_mut().unwrap().expanded = false;
        model.update_detail_offset();
        model.view.details.as_mut().unwrap().expanded = true;
        model.view.details.as_mut().unwrap().scroll = 0;
        model.update_detail_offset();
        assert_eq!(model.detail_offset, Fixed::ZERO);
    }
    #[test]
    fn typed_levels_follow_readback_and_retarget_without_resetting_menu_selection() {
        let mut model = Launcher::default();
        let mut view = View::default();
        view.launcher.enabled = true;
        view.launcher.overlay = "系统控制".into();
        view.launcher.overlay_kind = "controls".into();
        view.launcher.items = howff_menu::SystemLevel::ALL
            .into_iter()
            .map(|control| {
                howff_menu::MenuItem::available(control.action(), control.label())
                    .with_level(howff_menu::Level::new(control, None, false, false))
            })
            .collect();
        model.present(&view);
        model.tick(1000);
        assert_eq!(model.level_positions, [Fixed::ZERO; 2]);
        view.launcher.items[0].level.as_mut().unwrap().value = Some(50);
        model.present(&view);
        model.tick(1150);
        assert_eq!(model.level_positions[0], 50.into());
        let selection_age = model.selection_age;
        view.launcher.items[0].level.as_mut().unwrap().pending = true;
        model.present(&view);
        assert_eq!(model.selection_age, selection_age);
        view.launcher.items[0].level.as_mut().unwrap().value = Some(80);
        model.present(&view);
        model.tick(1200);
        let intermediate = model.level_positions[0];
        assert!(intermediate > 50.into() && intermediate < 80.into());
        view.launcher.items[0].level.as_mut().unwrap().value = Some(30);
        model.present(&view);
        assert_eq!(model.level_value(0, 1200), intermediate);
        model.tick(1350);
        assert_eq!(model.level_positions[0], 30.into());
        assert_eq!(model.snapshot_level(0).unwrap().value, Some(30));
        assert!(model.pending.is_none());
        assert_eq!(model.view.selected, 0);
    }
    #[test]
    fn release_hold_cancel_and_stale_selection_are_exclusive() {
        let mut model = Launcher {
            view: LauncherView {
                enabled: true,
                key: "game".into(),
                netplay: true,
                ..Default::default()
            },
            ..Default::default()
        };
        model.press(0);
        model.release(80);
        assert_eq!(
            model.pending.take(),
            Some(Intent::Game {
                key: "game".into(),
                multiplayer: false
            })
        );
        model.press(100);
        for t in (200..=800).step_by(100) {
            model.tick(t);
        }
        assert_eq!(
            model.pending.take(),
            Some(Intent::Game {
                key: "game".into(),
                multiplayer: true
            })
        );
        model.press(810);
        model.release(820);
        assert!(model.pending.is_none());
        model.press(900);
        model.cancel();
        model.release(950);
        assert!(model.pending.is_none());
        model.press(1000);
        model.tick(1600);
        model.release(1601);
        assert!(model.pending.is_none());
        model.press(1700);
        model.present(&View::default());
        model.release(1800);
        assert!(model.pending.is_none());
    }
}

#[cfg(test)]
mod management_contracts {
    use super::*;
    use howff_menu::management::{Device, Entry, View as ManagementView};

    #[test]
    fn management_routes_keep_their_parent_focus_and_use_the_launcher_input_path() {
        for entry in [Entry::Device, Entry::Application] {
            let mut launcher = Launcher::default();
            let mut view = View::default();
            view.launcher.enabled = true;
            view.launcher.overlay = "设备与好友".into();
            view.launcher.management = Some(ManagementView {
                epoch: 1,
                revision: 1,
                entry,
                friends: vec![Device {
                    id: "b".repeat(64),
                    name: "小林".into(),
                    address: String::new(),
                }],
                device: Device {
                    id: "a".repeat(64),
                    name: "我的掌机".into(),
                    address: String::new(),
                },
                ..Default::default()
            });
            launcher.present(&view);
            assert!(launcher.management.active());
            let title = launcher.management.snapshot_title();
            launcher.management_input(Action::Down);
            launcher.management_input(Action::Confirm);
            launcher.management_input(Action::Back);
            assert_eq!(launcher.management.snapshot_title(), title);
            assert_eq!(launcher.management.selected(), 1);
        }
    }
}

impl Launcher {
    fn take_intent(&mut self) -> Option<Intent> {
        self.pending.take()
    }
}
