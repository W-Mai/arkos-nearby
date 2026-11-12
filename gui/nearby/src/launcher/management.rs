use super::text_entry::TextEntry;
use crate::model::Action;
use howff_menu::input::{Generation, Sourced};
use howff_menu::management::{Entry, Fact, Home, Operation, Request, UpdatePhase, View};
use mirui::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Device,
    Friend,
    Rename,
    Application,
    Version,
    Startup,
    Home,
    Confirm,
    Diagnostics,
    Packages,
    Update,
    Progress,
    Detail,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub key: String,
    pub title: String,
    pub detail: String,
    pub enabled: bool,
}
fn row(
    key: impl Into<String>,
    title: impl Into<String>,
    detail: impl Into<String>,
    enabled: bool,
) -> Row {
    Row {
        key: key.into(),
        title: title.into(),
        detail: detail.into(),
        enabled,
    }
}

#[model]
pub struct Manager {
    input_generation: Generation,
    pub text: model!(owned TextEntry),
    #[observe]
    pub page: Page,
    #[observe]
    pub selected: usize,
    #[observe]
    pub age: u16,
    #[observe]
    pub visible_rows: usize,
    #[observe]
    revision: u64,
    #[observe]
    shown: bool,
    #[observe]
    pub loading_start: Fixed,
    #[observe]
    pub loading_end: Fixed,
    #[observe]
    pub track: Fixed,
    #[observe]
    pub detail_scroll: Fixed,
    detail_limit: Fixed,
    detail: Option<Fact>,
    directories: Vec<(String, usize)>,
    directory_focus: Option<usize>,
    view: Option<View>,
    rows_cache: Vec<Row>,
    parents: Vec<(Page, usize)>,
    friend: String,
    confirmation: Option<Operation>,
    pending: Option<Request>,
    awaiting: Option<Operation>,
    sequence: u64,
    changed_at: u64,
    now: u64,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            input_generation: Generation::default(),
            text: TextEntry::default().into(),
            page: Page::Device,
            selected: 0,
            age: 0,
            visible_rows: 4,
            revision: 0,
            shown: false,
            loading_start: Fixed::ZERO,
            loading_end: Fixed::HALF,
            track: Fixed::ZERO,
            detail_scroll: Fixed::ZERO,
            detail_limit: Fixed::ZERO,
            detail: None,
            directories: vec![],
            directory_focus: None,
            view: None,
            rows_cache: vec![],
            parents: vec![],
            friend: String::new(),
            confirmation: None,
            pending: None,
            awaiting: None,
            sequence: 1,
            changed_at: 0,
            now: 0,
        }
    }
}
#[model]
impl Manager {
    pub fn bind_input(&mut self, generation: Generation) {
        if self.input_generation != generation {
            self.pending = None;
            self.input_generation = generation;
        }
    }

    pub fn sync(&mut self, view: Option<&View>) {
        self.sync_state(view);
        self.refresh_rows();
    }
    fn sync_state(&mut self, view: Option<&View>) {
        let fresh = !self.shown || self.view.as_ref().map(|v| v.epoch) != view.map(|v| v.epoch);
        self.shown = view.is_some();
        let Some(view) = view else {
            self.text.retire();
            self.pending = None;
            return;
        };
        if fresh {
            self.parents.clear();
            self.directories.clear();
            self.directory_focus = None;
            self.confirmation = None;
            self.awaiting = None;
            self.page = if view.entry == Entry::Device {
                Page::Device
            } else {
                Page::Application
            };
            self.selected = 0;
            self.sequence = view.revision.max(1);
            self.changed_at = self.now;
        }
        if self.view.as_ref() != Some(view) {
            self.view = Some(view.clone());
            self.revision += 1;
        }
        if !view.busy
            && view.revision >= self.sequence
            && let Some(operation) = self.awaiting.take()
            && !view.error
        {
            match operation {
                Operation::Rename { .. } => {
                    self.text.retire();
                    self.page = Page::Device;
                    self.parents.clear();
                    self.selected = 0;
                }
                Operation::Forget { .. } => {
                    let index = self
                        .parents
                        .iter()
                        .find(|(page, _)| *page == Page::Device)
                        .map(|(_, i)| *i)
                        .unwrap_or(0);
                    self.page = Page::Device;
                    self.parents.clear();
                    self.selected = index.min(view.friends.len());
                }
                Operation::SetHome { .. } if self.page == Page::Confirm => self.back(),
                Operation::Inspect { .. } if self.page == Page::Packages => self.open(Page::Update),
                Operation::Browse { .. } if self.page == Page::Packages => {
                    self.selected = self
                        .directory_focus
                        .take()
                        .unwrap_or(0)
                        .min(view.packages.len().saturating_sub(1));
                }
                Operation::Install { .. } => {
                    self.page = Page::Progress;
                    self.selected = 0;
                }

                _ => {}
            }
        }
    }
    #[observe]
    pub fn focus(&self) -> usize {
        if self.page == Page::Rename {
            self.text.selected()
        } else {
            self.selected
        }
    }
    #[observe]
    pub fn active(&self) -> bool {
        self.shown
    }
    #[observe]
    pub fn editing(&self) -> bool {
        self.shown && self.page == Page::Rename
    }
    #[observe]
    pub fn busy(&self) -> bool {
        let _ = self.revision;
        self.view.as_ref().is_some_and(|v| v.busy) || self.awaiting.is_some()
    }
    #[observe]
    pub fn room_busy(&self) -> bool {
        let _ = self.revision;
        self.view.as_ref().is_some_and(|v| v.room_busy)
    }
    pub fn snapshot_notice(&self) -> String {
        let _ = self.revision;
        self.view
            .as_ref()
            .map(|v| v.notice.clone())
            .unwrap_or_default()
    }
    pub fn snapshot_name(&self) -> String {
        let _ = self.revision;
        self.view
            .as_ref()
            .map(|v| v.device.name.clone())
            .unwrap_or_default()
    }

    pub fn snapshot_title(&self) -> String {
        let _ = self.revision;
        match self.page {
            Page::Device => "设备与好友",
            Page::Friend => "好友详情",
            Page::Rename => "设备名称",
            Page::Application => "应用管理",
            Page::Version => "版本信息",
            Page::Startup => "启动方式",
            Page::Home => "开机首页",
            Page::Confirm => "确认",
            Page::Diagnostics => "设备诊断",
            Page::Packages => "选择安装包",
            Page::Update | Page::Progress => "安装更新",
            Page::Detail => "详细信息",
        }
        .into()
    }
    pub fn snapshot_summary(&self) -> String {
        let _ = self.revision;
        let Some(view) = &self.view else {
            return String::new();
        };
        match self.page {
            Page::Device => format!(
                "{}\n本机 {}",
                view.device.name,
                view.device.id.get(..8).unwrap_or(&view.device.id)
            ),
            Page::Friend => view
                .friends
                .iter()
                .find(|f| f.id == self.friend)
                .map(|f| f.name.clone())
                .unwrap_or_default(),
            Page::Application => format!("Nearby {}", view.version),
            Page::Detail => self
                .detail
                .as_ref()
                .map(|v| v.label.clone())
                .unwrap_or_default(),
            Page::Startup => view.origin.clone(),
            Page::Confirm => match self.confirmation {
                Some(Operation::Forget { .. }) => "移除后，下次加入时重新确认".into(),
                Some(Operation::SetHome { home }) => format!(
                    "开机进入{}",
                    if home == Home::Howff {
                        "Nearby 收藏首页"
                    } else {
                        "原系统游戏列表"
                    }
                ),
                Some(Operation::Install { .. }) => "安装完成后重新打开 Nearby".into(),
                _ => String::new(),
            },
            Page::Packages => view.directory.clone(),
            Page::Update => view
                .update
                .as_ref()
                .map(|p| format!("{} → {}\n{}", view.version, p.version, p.reason))
                .unwrap_or_default(),
            _ => String::new(),
        }
    }
    fn build_rows(&self) -> Vec<Row> {
        let _ = self.revision;
        let Some(view) = &self.view else {
            return vec![];
        };
        let idle = !view.room_busy && !view.busy;
        match self.page {
            Page::Device => std::iter::once(row(
                "rename",
                "修改设备名称",
                if view.room_busy {
                    "结束当前房间后可修改"
                } else {
                    "中文 / English"
                },
                idle,
            ))
            .chain(
                view.friends
                    .iter()
                    .map(|f| row(&f.id, &f.name, "已配对", true)),
            )
            .collect(),
            Page::Friend => vec![row(
                "forget",
                "移除好友",
                if view.room_busy {
                    "结束当前房间后可移除"
                } else {
                    "下次加入时重新确认"
                },
                idle,
            )],
            Page::Application => vec![
                row("version", "版本信息", &view.version, true),
                row("startup", "启动方式", &view.origin, true),
                row("diagnostics", "设备诊断", "安装、显示与系统状态", true),
                row("packages", "安装更新", "从游戏卡选择安装包", idle),
            ],
            Page::Version => vec![
                row("fact-version", "应用版本", &view.version, true),
                row(
                    "fact-path",
                    "安装位置",
                    howff_platform::arkos::PATHS.program,
                    true,
                ),
                row("fact-profile", "设备配置", &view.profile, true),
            ],
            Page::Startup => vec![
                row("standalone", "独立打开", "进入收藏首页", idle),
                row(
                    "home",
                    "开机首页",
                    if view.home == Home::Howff {
                        "Nearby"
                    } else {
                        "原系统"
                    },
                    true,
                ),
            ],
            Page::Home => vec![
                row("original", "原系统", "游戏列表", idle),
                row("nearby", "Nearby", "收藏首页", idle),
            ],
            Page::Confirm => vec![
                row("cancel", "返回", "", true),
                row("accept", "确认", "", idle),
            ],
            Page::Diagnostics => view
                .facts
                .iter()
                .enumerate()
                .map(|(i, Fact { label, value })| row(format!("fact-{i}"), label, value, true))
                .collect(),
            Page::Packages => view
                .packages
                .iter()
                .map(|p| {
                    row(
                        &p.key,
                        &p.name,
                        if p.directory {
                            "文件夹".into()
                        } else {
                            format!("{} KB", p.size / 1024)
                        },
                        true,
                    )
                })
                .collect(),
            Page::Update => vec![row(
                "install",
                "安装此版本",
                view.update
                    .as_ref()
                    .map(|p| p.reason.clone())
                    .unwrap_or_default(),
                idle && view.update.as_ref().is_some_and(|p| p.compatible),
            )],
            Page::Progress => vec![row("finish-update", "返回应用管理", "", !self.busy())],
            Page::Detail | Page::Rename => vec![],
        }
    }
    #[observe]
    pub fn reading(&self) -> bool {
        self.page == Page::Detail
    }
    #[observe]
    pub fn progressing(&self) -> bool {
        self.busy() || self.page == Page::Progress
    }
    pub fn snapshot_progress_label(&self) -> String {
        if self.busy() {
            if matches!(self.awaiting, Some(Operation::Install { .. })) {
                "正在安装"
            } else {
                "正在读取"
            }
        } else if self.view.as_ref().is_some_and(|v| v.error) {
            "安装未完成"
        } else {
            "安装完成"
        }
        .into()
    }
    pub fn snapshot_detail_value(&self) -> String {
        self.detail
            .as_ref()
            .map(|v| v.value.clone())
            .unwrap_or_default()
    }
    pub fn snapshot_friend_field(&self, index: usize) -> Option<Fact> {
        let friend = self
            .view
            .as_ref()?
            .friends
            .iter()
            .find(|f| f.id == self.friend)?;
        Some(match index {
            0 => Fact {
                label: "设备标识".into(),
                value: friend.id.clone(),
            },
            1 => Fact {
                label: "记录地址".into(),
                value: friend.address.clone(),
            },
            _ => return None,
        })
    }
    pub fn snapshot_rows(&self) -> Vec<Row> {
        self.rows_cache.clone()
    }
    pub fn snapshot_row(&self, index: usize) -> Option<Row> {
        self.rows_cache.get(index).cloned()
    }
    fn refresh_rows(&mut self) {
        let rows = self.build_rows();
        if rows != self.rows_cache {
            self.rows_cache = rows;
            self.revision += 1;
        }
    }
    pub fn snapshot_window(&self) -> std::ops::Range<usize> {
        let count = self.rows_cache.len();
        let rows = self.visible_rows.max(1).min(count);
        let first = self
            .selected
            .saturating_sub(rows.saturating_sub(1))
            .min(count.saturating_sub(rows));
        first..first + rows
    }
    pub fn track_width(&mut self, width: Fixed) {
        self.track = width;
    }
    pub fn viewport(&mut self, height: Fixed, extent: i32, gap: i32) {
        self.visible_rows =
            ((height.to_int() + gap).max(0) as usize / (extent + gap).max(1) as usize).clamp(1, 8);
    }
    fn clamp_detail_scroll(&mut self) {
        self.detail_scroll = self.detail_scroll.clamp(Fixed::ZERO, self.detail_limit);
    }
    pub fn detail_bounds(&mut self, max_y: Fixed) {
        self.detail_limit = max_y.max(Fixed::ZERO);
        self.clamp_detail_scroll();
    }
    fn open(&mut self, page: Page) {
        self.parents.push((self.page, self.selected));
        self.page = page;
        self.selected = 0;
        self.changed_at = self.now;
    }
    fn back(&mut self) {
        if self.page == Page::Progress {
            if self.busy() {
                return;
            }
            if self
                .view
                .as_ref()
                .and_then(|v| v.update.as_ref())
                .is_some_and(|u| u.phase == UpdatePhase::Complete)
            {
                self.send(Operation::Close);
            } else {
                self.page = Page::Application;
                self.parents.clear();
                self.selected = 3;
            }
            return;
        }
        if self.page == Page::Packages
            && !self.busy()
            && let Some((directory, index)) = self.directories.pop()
        {
            self.directory_focus = Some(index);
            self.send(Operation::Browse { directory });
            return;
        }
        if matches!(
            self.awaiting,
            Some(
                Operation::Refresh
                    | Operation::Diagnose
                    | Operation::Browse { .. }
                    | Operation::Inspect { .. }
            )
        ) {
            self.awaiting = None;
            self.pending = None;
        }
        if self.page == Page::Rename {
            self.text.retire();
        }
        if let Some((page, index)) = self.parents.pop() {
            self.page = page;
            self.selected = index;
            self.changed_at = self.now;
        } else {
            self.send(Operation::Close);
        }
    }
    fn send(&mut self, operation: Operation) {
        if self.busy() && !matches!(operation, Operation::Close) {
            return;
        }
        let Some(view) = &self.view else {
            return;
        };
        self.sequence = self.sequence.max(view.revision) + 1;
        if matches!(operation, Operation::Install { .. }) {
            self.page = Page::Progress;
            self.selected = 0;
        }
        self.revision += 1;
        self.awaiting = Some(operation.clone());
        self.pending = Some(Request {
            epoch: view.epoch,
            sequence: self.sequence,
            operation,
        });
    }
    pub fn tick(&mut self, now: u64) {
        self.now = now;
        if self.busy() {
            (self.loading_start, self.loading_end) = super::wifi::loading_span(now);
        }
        self.age = now.saturating_sub(self.changed_at).min(800) as u16;
    }
    pub fn input(&mut self, action: Action) {
        let previous = if self.editing() {
            self.text.selected()
        } else {
            self.selected
        };
        self.input_step(action);
        let current = if self.editing() {
            self.text.selected()
        } else {
            self.selected
        };
        if previous != current
            || matches!(action, Action::Confirm | Action::PageUp | Action::PageDown)
        {
            self.changed_at = self.now;
        }
        self.refresh_rows();
    }
    fn input_step(&mut self, action: Action) {
        if self.page == Page::Progress {
            if !self.busy() && matches!(action, Action::Confirm | Action::Back) {
                self.back();
            }
            return;
        }
        if self.page == Page::Detail {
            match action {
                Action::Back => self.back(),
                Action::Down => {
                    self.detail_scroll += Fixed::from_int(24);
                    self.clamp_detail_scroll();
                }
                Action::Up => {
                    self.detail_scroll = (self.detail_scroll - Fixed::from_int(24)).max(Fixed::ZERO)
                }
                _ => {}
            }
            return;
        }
        if self.page == Page::Rename {
            if self.busy() && action != Action::Back {
                return;
            }
            if action == Action::Back && self.text.snapshot_preedit().is_empty() {
                self.back();
            } else {
                self.text.input(action);
            }
            return;
        }
        if action == Action::Back {
            self.back();
            return;
        }
        if self.busy() {
            return;
        }
        if matches!(
            action,
            Action::Up | Action::Left | Action::Down | Action::Right
        ) {
            let count = self.rows_cache.len();
            self.selected = if matches!(action, Action::Up | Action::Left) {
                self.selected.saturating_sub(1)
            } else {
                (self.selected + 1).min(count.saturating_sub(1))
            };
            self.changed_at = self.now;
            return;
        }
        if action == Action::Refresh && self.page == Page::Diagnostics {
            self.send(Operation::Diagnose);
            return;
        }
        if action != Action::Confirm {
            return;
        }
        let Some(item) = self
            .snapshot_rows()
            .get(self.selected)
            .cloned()
            .filter(|r| r.enabled)
        else {
            return;
        };
        match item.key.as_str() {
            "rename" => {
                self.text.begin(&self.snapshot_name());
                self.open(Page::Rename);
            }
            "forget" => {
                self.confirmation = Some(Operation::Forget {
                    id: self.friend.clone(),
                });
                self.open(Page::Confirm);
            }
            "version" => self.open(Page::Version),
            "startup" => self.open(Page::Startup),
            "home" => self.open(Page::Home),
            "original" | "nearby" => {
                self.confirmation = Some(Operation::SetHome {
                    home: if item.key == "nearby" {
                        Home::Howff
                    } else {
                        Home::Original
                    },
                });
                self.open(Page::Confirm);
            }
            "diagnostics" => {
                self.open(Page::Diagnostics);
                self.send(Operation::Diagnose);
            }
            "packages" => {
                self.open(Page::Packages);
                self.send(Operation::Browse {
                    directory: String::new(),
                });
            }
            "cancel" | "finish-update" => self.back(),
            "accept" => {
                if let Some(operation) = self.confirmation.clone() {
                    self.send(operation);
                }
            }
            "standalone" => self.send(Operation::Standalone),
            "install" => {
                if let Some(update) = self.view.as_ref().and_then(|v| v.update.as_ref()) {
                    self.confirmation = Some(Operation::Install {
                        digest: update.digest.clone(),
                    });
                    self.open(Page::Confirm);
                }
            }
            key if key.starts_with("fact-") => {
                self.detail = Some(Fact {
                    label: item.title,
                    value: item.detail,
                });
                self.detail_scroll = Fixed::ZERO;
                self.detail_limit = Fixed::ZERO;
                self.open(Page::Detail);
            }
            key if self.page == Page::Device => {
                self.friend = key.into();
                self.open(Page::Friend);
            }
            key if self.page == Page::Packages => {
                if let Some(package) = self
                    .view
                    .as_ref()
                    .and_then(|v| v.packages.iter().find(|p| p.key == key))
                {
                    if package.directory {
                        self.directories
                            .push((self.view.as_ref().unwrap().directory.clone(), self.selected));
                        self.directory_focus = Some(0);
                    }
                    self.send(if package.directory {
                        Operation::Browse {
                            directory: package.key.clone(),
                        }
                    } else {
                        Operation::Inspect {
                            path: package.key.clone(),
                        }
                    });
                }
            }
            _ => {}
        }
    }
    pub fn submit_name(&mut self) {
        if !self.text.snapshot_preedit().is_empty() {
            self.text.choose_candidates();
            return;
        }
        let name = self.text.snapshot_value().trim().to_string();
        if howff_menu::management::valid_name(&name) {
            self.send(Operation::Rename { name });
        } else {
            self.text.reject("请输入设备名称");
        }
    }
    #[effects]
    fn take_input(&mut self) -> Option<Sourced<Request>> {
        self.take_request()
            .map(|request| self.input_generation.bind(request))
    }
}

impl ManagerHandle {
    pub fn progress_label(&self) -> String {
        self.revision();
        self.page();
        self.snapshot_progress_label()
    }
    pub fn detail_value(&self) -> String {
        self.revision();
        self.page();
        self.snapshot_detail_value()
    }

    pub fn friend_field(&self, index: usize) -> Option<Fact> {
        self.revision();
        self.snapshot_friend_field(index)
    }
    pub fn notice(&self) -> String {
        self.revision();
        self.page();
        self.snapshot_notice()
    }

    pub fn title(&self) -> String {
        self.revision();
        self.page();
        self.snapshot_title()
    }
    pub fn summary(&self) -> String {
        self.revision();
        self.page();
        self.snapshot_summary()
    }
    pub fn row(&self, index: usize) -> Option<Row> {
        self.revision();
        self.snapshot_row(index)
    }

    pub fn window(&self) -> std::ops::Range<usize> {
        self.revision();
        self.page();
        self.selected();
        self.visible_rows();
        self.snapshot_window()
    }
}

impl Manager {
    fn take_request(&mut self) -> Option<Request> {
        self.pending.take()
    }
}

#[cfg(test)]
mod input_contract {
    use super::*;

    #[test]
    fn queued_requests_keep_their_origin_and_rebinding_discards_unsent_requests() {
        let mut model = Manager::default();
        let mut generation = Generation::FIRST;
        model.bind_input(generation);
        model.selected = 3;
        model.pending = Some(Request {
            epoch: 1,
            sequence: 1,
            operation: Operation::Close,
        });
        let queued = model.take_input().unwrap();
        model.pending = Some(Request {
            epoch: 1,
            sequence: 1,
            operation: Operation::Close,
        });
        generation.advance();
        model.bind_input(generation);
        assert!(model.take_input().is_none());
        assert_eq!(queued.generation, Generation::FIRST);
        assert_eq!(model.selected, 3);
        model.pending = Some(Request {
            epoch: 1,
            sequence: 1,
            operation: Operation::Close,
        });
        assert_eq!(model.take_input().unwrap().generation, generation);
    }
}
