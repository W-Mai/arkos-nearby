use howff_platform::arkos;
mod configuration;
mod controls;
mod details;
mod management;
pub mod recipes;
mod saves;
mod storage;
pub mod tools;

use crate::catalog::{Browser, Entry};
use crate::flow::FlowExit;
use crate::menu::MenuSession;
use crate::{Result, require};
use howff_menu::details::{Load, Section};
use howff_menu::{MenuAction, MenuItem};
use serde_json::{Value, json};
use std::path::Path;
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub enum Intent {
    Play(Entry),
    SavedPlay(Entry, String),
    RestorePlay(Entry),
    Multiplayer(Entry),
    Discovery,
    Network,
    Exit,
    KeepNetworkExit,
    Power(PowerAction),
    Tool(tools::Tool),
    Updated(String),
}

pub use crate::platform::system::PowerAction;

fn keep_network_exit_allowed(purpose: crate::purpose::SessionPurpose, established: bool) -> bool {
    purpose == crate::purpose::SessionPurpose::NetworkOnly && established
}

fn exit_items(connected_network: bool) -> Vec<MenuItem> {
    let mut items = vec![MenuItem::available(MenuAction::Back, "返回")];
    if connected_network {
        items.push(MenuItem::available(
            MenuAction::KeepNetworkExit,
            "保留直连，返回原系统",
        ));
    }
    items.push(MenuItem::available(MenuAction::Exit, "退出并恢复 Wi-Fi"));
    items
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    Game,
    System,
    Controls,
    Tools,
    Exit,
    Power,
    Network,
    Details(Section),
    Wifi,
    Configuration,
    Saves,
    Management,
}

pub struct Controller {
    browsers: [Browser; 2],
    tab: usize,
    overlay: Option<Overlay>,
    selection: usize,
    entry: Option<Entry>,
    controls: controls::Controls,
    tools: tools::Tools,
    details: details::Service,
    wifi: crate::wifi::Service,
    wifi_return: MenuAction,
    configuration: configuration::Service,
    saves: saves::Service,
    saves_from_configuration: bool,
    management: management::Service,
    detail_expanded: bool,
    detail_scroll: u32,
    pub changed_tab: bool,
    pub status: String,
}

impl Controller {
    pub fn new() -> Result<Self> {
        let mut result = Self {
            browsers: [Browser::launcher(true)?, Browser::launcher(false)?],
            tab: 0,
            overlay: None,
            selection: 0,
            entry: None,
            controls: controls::Controls::default(),
            tools: tools::Tools::default(),
            details: details::Service::default(),
            wifi: Default::default(),
            wifi_return: MenuAction::OpenWifi,
            configuration: Default::default(),
            saves: Default::default(),
            saves_from_configuration: false,
            management: Default::default(),
            detail_expanded: false,
            detail_scroll: 0,
            changed_tab: false,
            status: String::new(),
        };
        if let Ok(saved) = crate::system::read::<Value>(arkos::application::LAUNCHER.path()) {
            if let Some(key) = saved["last_favorite"].as_str() {
                result.browsers[0].select_key(key);
            }
        }
        Ok(result)
    }
    pub fn shelf_layout(&mut self, grid: crate::catalog::shelf::Grid) {
        self.browsers[0].shelf_layout(grid);
    }
    pub fn list_layout(&mut self, rows: usize) {
        self.browsers[1].list_layout(rows);
    }
    pub fn browser(&mut self) -> &mut Browser {
        &mut self.browsers[self.tab.min(1)]
    }

    pub(crate) fn indexed(&mut self, index: std::sync::Arc<crate::catalog::Index>) {
        for browser in &mut self.browsers {
            browser.accept_index(index.clone());
        }
    }

    fn sync_personal(&mut self) {
        for browser in &mut self.browsers {
            browser.refresh_personal();
        }
    }

    fn reopen(&mut self) {
        for browser in &mut self.browsers {
            browser.reopen();
        }
    }
    fn game(&self) -> Option<Entry> {
        self.browsers[self.tab.min(1)].game()
    }
    fn options(&self) -> Vec<MenuItem> {
        use MenuAction::*;
        match self.overlay {
            Some(Overlay::Game) => vec![
                MenuItem::available(Play, "开始游戏"),
                if self.entry.as_ref().is_some_and(Entry::netplay) {
                    MenuItem::available(Multiplayer, "附近联机")
                } else {
                    MenuItem::unavailable(Multiplayer, "附近联机", "此游戏暂未接入联机")
                },
                MenuItem::available(
                    ToggleFavorite,
                    if self.browsers[self.tab].is_favorite() {
                        "移出收藏"
                    } else {
                        "收藏游戏"
                    },
                ),
                MenuItem::available(ShowGameDetails, "游戏详情"),
                MenuItem::available(ShowGameSettings, "游戏设置"),
                MenuItem::available(ShowSaveManager, "存档管理"),
                MenuItem::available(Back, "返回"),
            ],
            Some(Overlay::Details(Section::Overview)) => vec![
                MenuItem::available(ShowCoreDetails, "运行核心")
                    .with_value("共享选择规则与安装来源"),
                MenuItem::available(ShowFirmwareDetails, "BIOS 文件")
                    .with_value("固件声明与文件状态"),
                MenuItem::available(ShowSaveDetails, "存档与配置").with_value("单机来源与联机隔离"),
            ],
            Some(Overlay::Details(section)) => match &self.details.load {
                Load::Ready { document } => document
                    .rows(section)
                    .iter()
                    .map(|row| {
                        MenuItem::available(ExpandDetail, &row.name).with_value(&row.summary)
                    })
                    .collect(),
                _ => vec![],
            },
            Some(Overlay::System) => vec![
                MenuItem::available(CycleTheme, "切换主题"),
                MenuItem::available(ToggleSound, "音效开关"),
                MenuItem::available(ShowControls, "系统控制"),
                MenuItem::available(ShowDevice, "设备与好友"),
                MenuItem::available(ShowApplication, "应用管理"),
                if Self::retained() {
                    MenuItem::available(ManageNetwork, "管理直连网络")
                } else {
                    MenuItem::unavailable(ManageNetwork, "管理直连网络", "尚未保留直连网络")
                },
                MenuItem::available(ShowExit, "返回原系统"),
                MenuItem::available(ShowPower, "电源"),
            ],
            Some(Overlay::Controls) => self.controls.items(),
            Some(Overlay::Tools) => self.tools.items(),
            Some(Overlay::Wifi | Overlay::Configuration | Overlay::Saves | Overlay::Management) => {
                vec![]
            }
            Some(Overlay::Exit) => exit_items(crate::session::state().ok().is_some_and(|state| {
                keep_network_exit_allowed(state.purpose, state.established())
            })),
            Some(Overlay::Power) => vec![
                MenuItem::available(Back, "返回"),
                MenuItem::available(Poweroff, "确认关机"),
                MenuItem::available(Reboot, "确认重启"),
            ],
            Some(Overlay::Network) => vec![
                MenuItem::available(Play, "保留直连，开始游戏"),
                MenuItem::available(RestorePlay, "恢复 Wi-Fi 后开始"),
                MenuItem::available(Back, "返回"),
            ],
            None => vec![],
        }
    }
    pub fn retained() -> bool {
        crate::session::state()
            .is_ok_and(|s| s.purpose == crate::purpose::SessionPurpose::NetworkOnly && !s.closed())
    }
    fn show(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
        self.selection = 0;
        self.entry = self.game();
    }
    pub fn view(&mut self) -> Value {
        self.details.poll();
        self.wifi.poll();
        self.management.poll();
        self.configuration.poll();
        self.saves.poll();
        if self.overlay == Some(Overlay::Saves) && self.saves.close_after && !self.saves.view.busy {
            self.close_saves();
        }
        let mut view = self.browser().view();
        let shelf = self.browsers[0].view();
        let catalog = self.browsers[1].view();
        let game = self.game();
        let options = self.options();
        let title = match self.overlay {
            Some(Overlay::Game) => self.entry.as_ref().map(|e| e.name()).unwrap_or("游戏操作"),
            Some(Overlay::System) => "启动器设置",
            Some(Overlay::Controls) => "系统控制",
            Some(Overlay::Tools) => "系统工具",
            Some(Overlay::Wifi) => self.wifi.title(),
            Some(Overlay::Management) => "设备与好友",
            Some(Overlay::Configuration) => "游戏设置",
            Some(Overlay::Saves) => "存档",
            Some(Overlay::Exit) => "返回原系统",
            Some(Overlay::Power) => "电源",
            Some(Overlay::Network) => "当前保留着直连网络",
            Some(Overlay::Details(Section::Overview)) => "游戏详情",
            Some(Overlay::Details(Section::Core)) => "运行核心",
            Some(Overlay::Details(Section::Firmware)) => "BIOS 文件",
            Some(Overlay::Details(Section::Saves)) => "存档与配置",
            Some(Overlay::Details(Section::Storage)) => "存储空间",
            None => "",
        };
        view["launcher"] = json!({"enabled":true,"tab":self.tab,"navigation":false,"key":game.as_ref().map(Entry::key).unwrap_or_default(),"netplay":game.as_ref().is_some_and(Entry::netplay),"overlay":title,"overlay_kind":match self.overlay {Some(Overlay::Game)=>"game",Some(Overlay::System)=>"system",Some(Overlay::Controls)=>"controls",Some(Overlay::Tools)=>"tools",Some(Overlay::Exit)=>"exit",Some(Overlay::Power)=>"power",Some(Overlay::Network)=>"network",Some(Overlay::Details(_))=>"details",Some(Overlay::Wifi)=>"wifi",Some(Overlay::Configuration)=>"configuration",Some(Overlay::Saves)=>"saves",Some(Overlay::Management)=>"management",None=>""},"items":options,"selected":self.selection,"retained":Self::retained()});
        view["launcher"]["notice"] = self.status.clone().into();
        if self.overlay == Some(Overlay::Management) {
            view["launcher"]["management"] =
                serde_json::to_value(&self.management.view).unwrap_or_default();
        }
        if self.overlay == Some(Overlay::Wifi) {
            view["launcher"]["overlay_kind"] = "wifi".into();
            view["launcher"]["wifi"] = serde_json::to_value(&self.wifi.view).unwrap_or_default();
            view["launcher"]["selected"] = self.wifi.view.selected.into();
        }
        if self.overlay == Some(Overlay::Saves) {
            view["launcher"]["saves"] = serde_json::to_value(&self.saves.view).unwrap_or_default();
        }
        if self.overlay == Some(Overlay::Configuration) {
            view["launcher"]["configuration"] =
                serde_json::to_value(&self.configuration.view).unwrap_or_default();
        }
        if let Some(Overlay::Details(section)) = self.overlay {
            view["launcher"]["overlay_kind"] = "details".into();
            view["launcher"]["details"] = serde_json::to_value(howff_menu::details::View {
                section,
                load: self.details.load.clone(),
                expanded: self.detail_expanded,
                scroll: self.detail_scroll,
            })
            .unwrap_or_default();
        }
        view["launcher_shelf"] = shelf;
        view["launcher_catalog"] = catalog;
        if self.tab == 0 {
            view["core"] = "本机游戏".into();
            view["title"] = "个人收藏".into();
        }
        if Self::retained() {
            view["core"] =
                format!("{} · 直连已保留", view["core"].as_str().unwrap_or_default()).into();
        }
        if !self.status.is_empty() {
            view["status"] = self.status.clone().into();
        }
        view["back_label"] = if self.tab == 0 || self.browser().root() {
            "返回"
        } else {
            "上级"
        }
        .into();
        view
    }
    fn tab(&mut self, value: usize) -> Option<Intent> {
        self.tab = value;
        self.changed_tab = true;
        self.browser().resume();
        None
    }
    pub fn gesture(&mut self, action: &str, key: &str) -> Result<Option<Intent>> {
        if self.overlay.is_some() {
            return Ok(None);
        }
        let Some(entry) = self.game().filter(|e| e.key() == key) else {
            return Ok(None);
        };
        self.remember(&entry)?;
        match action {
            "launch" if Self::retained() => {
                self.show(Overlay::Network);
                Ok(None)
            }
            "launch" => Ok(Some(Intent::Play(entry))),
            "multiplayer" if entry.netplay() => Ok(Some(Intent::Multiplayer(entry))),
            "multiplayer" => {
                self.status = "此游戏的启动方式暂未接入附近联机".into();
                Ok(None)
            }
            _ => Ok(None),
        }
    }
    fn remember(&self, entry: &Entry) -> Result<()> {
        if self.tab == 0 {
            crate::system::write(
                arkos::application::LAUNCHER.path(),
                &json!({"schema":1,"last_favorite":entry.key()}),
            )?;
        }
        Ok(())
    }
    pub fn input(&mut self, action: &str) -> Result<Option<Intent>> {
        self.status.clear();
        if matches!(
            self.overlay,
            Some(Overlay::Configuration | Overlay::Saves | Overlay::Management)
        ) {
            return Ok(None);
        }
        if self.overlay == Some(Overlay::Wifi) {
            let return_room = action == "confirm"
                && self.wifi.view.page == howff_menu::wifi::Page::Busy
                && Self::retained();
            if self.wifi.input(action) {
                self.overlay = Some(Overlay::Tools);
                self.selection = self
                    .options()
                    .iter()
                    .position(|item| item.action == self.wifi_return)
                    .unwrap_or(0);
            }
            return Ok(return_room.then_some(Intent::Network));
        }
        if let Some(overlay) = self.overlay {
            let options = self.options();
            if action == "back" {
                self.close_overlay(overlay);
                return Ok(None);
            }
            if matches!(action, "up" | "down" | "left" | "right") {
                if matches!(overlay, Overlay::Details(_)) {
                    if self.detail_expanded && matches!(action, "up" | "down") {
                        self.detail_scroll = if action == "up" {
                            self.detail_scroll.saturating_sub(24)
                        } else {
                            self.detail_scroll.saturating_add(24).min(8192)
                        };
                        return Ok(None);
                    }
                    if !options.is_empty() {
                        let delta = if matches!(action, "up" | "left") {
                            -1
                        } else {
                            1
                        };
                        self.selection = (self.selection as isize + delta)
                            .clamp(0, options.len() as isize - 1)
                            as usize;
                        self.detail_expanded = false;
                        self.detail_scroll = 0;
                    }
                    return Ok(None);
                }
                if overlay == Overlay::Controls {
                    match action {
                        "up" => self.selection = if self.selection >= 2 { 1 } else { 0 },
                        "down" => {
                            self.selection = match self.selection {
                                0 => 1,
                                1 => 2,
                                index => index,
                            }
                        }
                        "left" | "right" => {
                            if let Some(control) = options
                                .get(self.selection)
                                .and_then(|item| howff_menu::SystemLevel::from_action(item.action))
                            {
                                if let Err(error) = self.controls.adjust(control, action == "right")
                                {
                                    self.status = format!("设置未完成：{error}");
                                }
                            } else {
                                self.selection = if action == "left" { 2 } else { 3 };
                            }
                        }
                        _ => {}
                    }
                    return Ok(None);
                }
                let delta = match action {
                    "up" => -2,
                    "down" => 2,
                    "left" => -1,
                    _ => 1,
                };
                self.selection =
                    (self.selection as isize + delta).clamp(0, options.len() as isize - 1) as usize;
                return Ok(None);
            }
            if action != "confirm" {
                return Ok(None);
            }
            let Some(item) = options.get(self.selection) else {
                return Ok(None);
            };
            if !item.enabled() {
                self.status = item.reason().into();
                return Ok(None);
            }
            let selected = item.action;
            if overlay == Overlay::Tools
                && matches!(selected, MenuAction::OpenWifi | MenuAction::NetworkInfo)
            {
                self.wifi_return = selected;
                self.wifi.open(selected == MenuAction::NetworkInfo);
                self.overlay = Some(Overlay::Wifi);
                return Ok(None);
            }
            match selected {
                MenuAction::ShowStorage => {
                    self.details.storage();
                    self.overlay = Some(Overlay::Details(Section::Storage));
                    self.selection = 0;
                    self.detail_expanded = false;
                    self.detail_scroll = 0;
                    return Ok(None);
                }
                MenuAction::ShowSaveManager => {
                    self.open_saves(false);
                    return Ok(None);
                }
                MenuAction::ShowGameSettings => {
                    if let Some(entry) = self.entry.clone() {
                        self.configuration.open(entry);
                        self.overlay = Some(Overlay::Configuration);
                    }
                    return Ok(None);
                }
                MenuAction::ShowGameDetails => {
                    if let Some(entry) = self.entry.clone() {
                        self.details.open(entry);
                        self.overlay = Some(Overlay::Details(Section::Overview));
                        self.selection = 0;
                        self.detail_expanded = false;
                        self.detail_scroll = 0;
                    }
                    return Ok(None);
                }
                MenuAction::ShowCoreDetails
                | MenuAction::ShowFirmwareDetails
                | MenuAction::ShowSaveDetails => {
                    self.overlay = Some(Overlay::Details(match selected {
                        MenuAction::ShowCoreDetails => Section::Core,
                        MenuAction::ShowFirmwareDetails => Section::Firmware,
                        _ => Section::Saves,
                    }));
                    self.selection = 0;
                    self.detail_expanded = false;
                    self.detail_scroll = 0;
                    return Ok(None);
                }
                MenuAction::ExpandDetail => {
                    self.detail_expanded = !self.detail_expanded;
                    self.detail_scroll = 0;
                    return Ok(None);
                }
                _ => {}
            }
            if overlay == Overlay::Tools {
                if let Some(tool) = tools::Tool::from_action(selected) {
                    return Ok(Some(Intent::Tool(tool)));
                }
            }
            if matches!(overlay, Overlay::Controls | Overlay::Tools | Overlay::Power)
                && selected == MenuAction::Back
            {
                self.close_overlay(overlay);
                return Ok(None);
            }
            if overlay == Overlay::Controls && selected != MenuAction::ShowTools {
                return Ok(None);
            }
            self.overlay = None;
            return match selected {
                MenuAction::Play if overlay != Overlay::Network => {
                    let key = self.entry.as_ref().map(Entry::key).unwrap_or_default();
                    self.gesture("launch", &key)
                }
                MenuAction::Multiplayer => {
                    let key = self.entry.as_ref().map(Entry::key).unwrap_or_default();
                    self.gesture("multiplayer", &key)
                }
                MenuAction::ToggleFavorite => {
                    self.browser().dispatch("refresh")?;
                    self.sync_personal();
                    Ok(None)
                }
                MenuAction::ManageNetwork => Ok(Some(Intent::Network)),
                MenuAction::ShowDevice | MenuAction::ShowApplication => {
                    self.management
                        .open(if selected == MenuAction::ShowApplication {
                            howff_menu::management::Entry::Application
                        } else {
                            howff_menu::management::Entry::Device
                        });
                    self.show(Overlay::Management);
                    Ok(None)
                }
                MenuAction::ShowControls => {
                    self.controls.refresh();
                    self.show(Overlay::Controls);
                    Ok(None)
                }
                MenuAction::ShowTools => {
                    self.tools.refresh();
                    self.show(Overlay::Tools);
                    Ok(None)
                }
                MenuAction::ShowExit => {
                    self.show(Overlay::Exit);
                    Ok(None)
                }
                MenuAction::ShowPower => {
                    self.show(Overlay::Power);
                    Ok(None)
                }
                MenuAction::Exit => Ok(Some(Intent::Exit)),
                MenuAction::KeepNetworkExit => Ok(Some(Intent::KeepNetworkExit)),
                MenuAction::Poweroff => Ok(Some(Intent::Power(PowerAction::Poweroff))),
                MenuAction::Reboot => Ok(Some(Intent::Power(PowerAction::Reboot))),
                MenuAction::Play | MenuAction::RestorePlay => {
                    Ok(self
                        .entry
                        .clone()
                        .map(if selected == MenuAction::RestorePlay {
                            Intent::RestorePlay
                        } else {
                            Intent::Play
                        }))
                }
                MenuAction::CycleTheme | MenuAction::ToggleSound | MenuAction::Back => Ok(None),
                MenuAction::Volume
                | MenuAction::Brightness
                | MenuAction::OpenWifi
                | MenuAction::NetworkInfo
                | MenuAction::SystemInfo
                | MenuAction::ShowStorage
                | MenuAction::ShowGameDetails
                | MenuAction::ShowGameSettings
                | MenuAction::ShowSaveManager
                | MenuAction::ShowCoreDetails
                | MenuAction::ShowFirmwareDetails
                | MenuAction::ShowSaveDetails
                | MenuAction::ExpandDetail
                | MenuAction::FileManager => Err("System control action outside its panel".into()),
            };
        }
        if action == "launcher_home" {
            return Ok(Some(Intent::Discovery));
        }
        if self.browser().editing() {
            self.browser().dispatch(action)?;
            return Ok(None);
        }
        if action == "start" && !self.browser().searching() {
            self.show(Overlay::System);
            return Ok(None);
        }
        if action == "back" && self.tab == 0 {
            self.browser().dispatch("shelf_home")?;
            return Ok(None);
        }
        if action == "back" && self.browser().root() {
            return Ok(self.tab(0));
        }
        if action == "auxiliary" && self.tab == 1 && self.game().is_some() {
            self.browser().dispatch("refresh")?;
            self.sync_personal();
            return Ok(None);
        }
        if matches!(action, "confirm" | "down")
            && self.tab == 0
            && self
                .browser()
                .current()
                .is_some_and(|entry| entry.library_link())
        {
            return Ok(self.tab(1));
        }
        if action == "confirm" && self.game().is_some() {
            self.show(Overlay::Game);
            return Ok(None);
        }
        self.browser().dispatch(action)?;
        Ok(None)
    }
    fn close_overlay(&mut self, overlay: Overlay) {
        if let Overlay::Details(section) = overlay {
            if self.detail_expanded {
                self.detail_expanded = false;
                self.detail_scroll = 0;
                return;
            }
            self.overlay = Some(if section == Section::Storage {
                self.details.cancel();
                Overlay::Tools
            } else if section == Section::Overview {
                self.details.cancel();
                Overlay::Game
            } else {
                Overlay::Details(Section::Overview)
            });
            let action = match section {
                Section::Overview => MenuAction::ShowGameDetails,
                Section::Core => MenuAction::ShowCoreDetails,
                Section::Firmware => MenuAction::ShowFirmwareDetails,
                Section::Saves => MenuAction::ShowSaveDetails,
                Section::Storage => MenuAction::ShowStorage,
            };
            self.selection = self
                .options()
                .iter()
                .position(|item| item.action == action)
                .unwrap_or_default();
            return;
        }
        let parent = match overlay {
            Overlay::Controls => Some((Overlay::System, MenuAction::ShowControls)),
            Overlay::Tools => Some((Overlay::Controls, MenuAction::ShowTools)),
            Overlay::Power => Some((Overlay::System, MenuAction::ShowPower)),
            _ => None,
        };
        if let Some((parent, action)) = parent {
            if parent == Overlay::Controls {
                self.controls.refresh();
            }
            self.show(parent);
            self.selection = self
                .options()
                .iter()
                .position(|item| item.action == action)
                .unwrap_or_default();
        } else {
            self.overlay = None;
        }
    }
    pub fn configuration_request(&mut self, request: howff_menu::configuration::Request) {
        if self.overlay != Some(Overlay::Configuration)
            || request.epoch() != self.configuration.view.epoch
        {
            return;
        }
        if matches!(
            request,
            howff_menu::configuration::Request::OpenSaves { .. }
        ) {
            if !self.configuration.view.saving {
                self.open_saves(true);
            }
        } else if matches!(request, howff_menu::configuration::Request::Close { .. }) {
            if !self.configuration.view.saving {
                self.overlay = Some(Overlay::Game);
                self.selection = self
                    .options()
                    .iter()
                    .position(|item| item.action == MenuAction::ShowGameSettings)
                    .unwrap_or(0);
            }
        } else {
            self.configuration.request(request);
        }
    }
    fn open_saves(&mut self, from_configuration: bool) {
        if let Some(entry) = self.entry.clone() {
            self.saves.open(entry);
            self.saves_from_configuration = from_configuration;
            self.overlay = Some(Overlay::Saves);
        }
    }
    fn close_saves(&mut self) {
        self.saves.close_after = false;
        self.overlay = Some(if self.saves_from_configuration {
            Overlay::Configuration
        } else {
            Overlay::Game
        });
        if !self.saves_from_configuration {
            self.selection = self
                .options()
                .iter()
                .position(|item| item.action == MenuAction::ShowSaveManager)
                .unwrap_or(0);
        }
    }
    pub fn saves_request(&mut self, request: howff_menu::saves::Request) {
        if self.overlay == Some(Overlay::Saves) {
            self.saves.request(request);
        }
    }
    pub fn save_launch(&mut self) -> Option<Intent> {
        if self.overlay != Some(Overlay::Saves) {
            return None;
        }
        let entry = self.entry.clone()?;
        self.saves
            .take_launch()
            .map(|id| Intent::SavedPlay(entry, id))
    }
    pub fn management_request(
        &mut self,
        request: howff_menu::management::Request,
    ) -> Option<Intent> {
        if self.overlay != Some(Overlay::Management) {
            return None;
        }
        if self.management.view.busy
            && self
                .management
                .view
                .update
                .as_ref()
                .is_some_and(|u| u.phase == howff_menu::management::UpdatePhase::Installing)
        {
            return None;
        }
        if request.epoch != self.management.view.epoch {
            return None;
        }
        if matches!(
            request.operation,
            howff_menu::management::Operation::Standalone
        ) {
            self.overlay = None;
            self.tab(0);
            return None;
        }
        if matches!(request.operation, howff_menu::management::Operation::Close) {
            if let Some(token) = self.management.token.take() {
                return Some(Intent::Updated(token));
            }
            self.show(Overlay::System);
            self.selection = self
                .options()
                .iter()
                .position(|item| {
                    item.action
                        == if self.management.view.entry == howff_menu::management::Entry::Device {
                            MenuAction::ShowDevice
                        } else {
                            MenuAction::ShowApplication
                        }
                })
                .unwrap_or(0);
        } else {
            self.management.request(request);
        }
        None
    }
    pub fn wifi_request(&mut self, request: howff_menu::wifi::Request) {
        if self.overlay == Some(Overlay::Wifi) {
            self.wifi.credentials(request);
        }
    }
    pub fn restore_network(&mut self) -> Result<()> {
        if !Path::new(crate::session::BASE)
            .join("session.json")
            .exists()
        {
            return Ok(());
        }
        let s = crate::session::state()?;
        if !s.closed() || !s.network_restored {
            crate::session::recover(&s.id, "launcher_exit")?;
        }
        require(
            crate::session::state().is_ok_and(|s| s.closed() && s.network_restored),
            "Network recovery is unfinished",
        )
    }
}

pub fn run() -> Result<i32> {
    run_page(false)
}
pub fn run_application() -> Result<i32> {
    run_page(true)
}
fn run_page(application: bool) -> Result<i32> {
    crate::session::identity(false)?;
    let mut controller = Controller::new()?;
    if application {
        controller.overlay = Some(Overlay::Management);
        controller
            .management
            .open(howff_menu::management::Entry::Application);
    }
    let mut display = None;
    loop {
        if display.is_none() {
            display = Some(MenuSession::open_launcher()?);
        }
        let intent = display.as_mut().unwrap().choose_launcher(&mut controller)?;
        let refresh_library = !matches!(&intent, Intent::Tool(tool) if *tool != tools::Tool::Files);
        let result = (|| -> Result<bool> {
            match intent {
                Intent::Play(ref entry) | Intent::RestorePlay(ref entry) => {
                    display
                        .as_mut()
                        .unwrap()
                        .launcher_busy(&mut controller, "正在准备游戏")?;
                    if matches!(intent, Intent::RestorePlay(_)) {
                        controller.restore_network()?;
                    }
                    let prepared = recipes::prepare(entry.path())?;
                    drop(display.take());
                    single_player(prepared, None)?;
                }
                Intent::SavedPlay(entry, ticket) => {
                    let result = (|| -> Result<()> {
                        display
                            .as_mut()
                            .unwrap()
                            .launcher_busy(&mut controller, "正在载入存档")?;
                        crate::session::require_idle()?;
                        let prepared = recipes::prepare(entry.path())?;
                        drop(display.take());
                        single_player(prepared, Some(&ticket))
                    })();
                    crate::saves::launch::discard(&ticket);
                    controller
                        .saves
                        .returned(result.err().map(|e| e.to_string()));
                }
                Intent::Multiplayer(entry) => {
                    let menu = display.as_mut().unwrap();
                    if Controller::retained() {
                        menu.run_network()?;
                        if Controller::retained() {
                            return Ok(false);
                        }
                    }
                    menu.launcher_busy(&mut controller, "正在准备联机")?;
                    let context = entry.prepare()?;
                    menu.run_with_fallback(
                        Some(context.clone()),
                        Some(crate::session::NativeBackend::shared(context)),
                        false,
                    )?;
                }
                Intent::Discovery => {
                    let menu = display.as_mut().unwrap();
                    while let Some(mode) = menu.choose_mode()? {
                        match mode {
                            FlowExit::SelectGame => {
                                controller.tab(1);
                                break;
                            }
                            FlowExit::NetworkOnly => {
                                if menu.run_network()? {
                                    break;
                                }
                            }
                            FlowExit::KeepNetwork => break,
                            FlowExit::CurrentGame => {}
                        }
                    }
                }
                Intent::Network => {
                    display.as_mut().unwrap().run_network()?;
                }
                Intent::Tool(tool) => {
                    let menu = display.as_mut().unwrap();
                    let result = crate::session::try_while_idle(|| {
                        let prepared = tool.prepare()?;
                        menu.launcher_busy(&mut controller, &format!("正在打开{}", tool.name()))?;
                        menu.with_frontend_released(
                            |stopping| prepared.run(stopping),
                            tools::recover_pending,
                        )
                    });
                    controller.tools.refresh();
                    if tool == tools::Tool::Files {
                        menu.invalidate_library();
                    }
                    if menu.stopping() {
                        return Ok(true);
                    }
                    if let Err(error) = result {
                        crate::menu::log(
                            json!({"system_tool":tool.name(),"system_tool_error":error.to_string()}),
                        )?;
                        controller.status = format!("未能完成{}，请重试或返回", tool.name());
                    }
                }
                Intent::Updated(token) => {
                    drop(display.take());
                    crate::updates::restart(&token)?;
                }
                Intent::KeepNetworkExit => {
                    let state = crate::session::state()?;
                    require(
                        keep_network_exit_allowed(state.purpose, state.established()),
                        "直连网络尚未连接",
                    )?;
                    crate::network::require_live(&state)?;
                    drop(display.take());
                    return Ok(true);
                }
                Intent::Exit | Intent::Power(_) => {
                    display
                        .as_mut()
                        .unwrap()
                        .launcher_busy(&mut controller, "正在退出并恢复网络")?;
                    controller.restore_network()?;
                    drop(display.take());
                    if let Intent::Power(action) = intent {
                        crate::platform::system::power(action)?;
                    }
                    return Ok(true);
                }
            }
            if crate::session::state()
                .is_ok_and(|s| s.purpose == crate::purpose::SessionPurpose::Game && !s.closed())
            {
                drop(display.take());
                wait_game()?;
            }
            Ok(false)
        })();
        match result {
            Ok(true) => return Ok(0),
            Ok(false) => {}
            Err(error) => {
                controller.status = format!("操作未完成：{error}");
                crate::menu::log(json!({"launcher_error":error.to_string()}))?;
            }
        }
        if refresh_library {
            controller.reopen();
        } else {
            for browser in &mut controller.browsers {
                browser.resume();
            }
        }
    }
}

fn wait_game() -> Result<()> {
    while let Ok(s) = crate::session::state() {
        if s.purpose != crate::purpose::SessionPurpose::Game || s.closed() {
            break;
        }
        if s.phase == "restore_failed" {
            return Err("Network restoration failed; reopen Nearby to retry".into());
        }
        thread::sleep(Duration::from_millis(250));
    }
    if let Ok(s) = crate::session::state() {
        require(
            s.network_restored || s.purpose == crate::purpose::SessionPurpose::NetworkOnly,
            "Network restoration failed",
        )?;
    }
    Ok(())
}

fn single_player(launch: recipes::Launch, ticket: Option<&str>) -> Result<()> {
    crate::menu::log(json!({"single_player":launch.path,"launch_command":launch.command}))?;
    let solo = crate::platform::system::Solo::acquire()?;
    let log_path = arkos::application::LOGS.path().join("launcher-game.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    log.set_len(0)?;
    let expected = ticket
        .map(crate::saves::launch::expected_state)
        .transpose()?
        .flatten();
    let mut child = crate::menu::OwnedChild(
        solo.command(&launch.command, ticket, &log_path, log)?
            .spawn()?,
    );
    let mut reader = std::io::BufReader::new(std::fs::File::open(log_path)?);
    let mut line = Vec::new();
    let started = std::time::Instant::now();
    let mut queued = false;
    let mut recorded = false;
    let status = loop {
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        if let Some(path) = &expected
            && (!queued || started.elapsed() < Duration::from_secs(30))
        {
            use std::io::BufRead;
            for _ in 0..128 {
                line.clear();
                if reader.read_until(b'\n', &mut line)? == 0 {
                    break;
                }
                match crate::saves::launch::state_result(&line, path) {
                    crate::saves::launch::StateResult::Failed => {
                        return Err("即时存档载入失败，请重新读取或选择其他存档".into());
                    }
                    crate::saves::launch::StateResult::Queued => queued = true,
                    _ => {}
                }
            }
            require(
                queued || started.elapsed() < Duration::from_secs(30),
                "核心未接受即时存档，请重新读取",
            )?;
        }
        if !recorded && solo.running(&launch.path) {
            crate::catalog::library::record_path(&launch.path);
            recorded = true;
        }
        thread::sleep(Duration::from_millis(150));
    };
    require(status.success(), "Game process exited with an error")?;
    require(
        expected.is_none() || queued,
        "核心未接受即时存档，请重新读取",
    )
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    #[test]
    fn exit_choices_separate_preserving_an_established_network_from_restoring_wifi() {
        use crate::purpose::SessionPurpose;
        for (purpose, established) in [
            (SessionPurpose::Game, false),
            (SessionPurpose::Game, true),
            (SessionPurpose::NetworkOnly, false),
            (SessionPurpose::NetworkOnly, true),
        ] {
            let connected = keep_network_exit_allowed(purpose, established);
            assert_eq!(
                connected,
                purpose == SessionPurpose::NetworkOnly && established
            );
            let items = exit_items(connected);
            let actions: Vec<_> = items.iter().map(|item| item.action).collect();
            assert_eq!(actions.first(), Some(&MenuAction::Back));
            assert_eq!(actions.last(), Some(&MenuAction::Exit));
            assert_eq!(actions.contains(&MenuAction::KeepNetworkExit), connected);
            assert!(items.iter().all(MenuItem::enabled));
            for item in items {
                let value = serde_json::to_value(&item).unwrap();
                assert_eq!(serde_json::from_value::<MenuItem>(value).unwrap(), item);
            }
        }
    }

    #[test]
    fn detail_navigation_retains_game_identity_and_returns_to_each_parent_action() {
        let entry: Entry = serde_json::from_value(json!({"path":"/roms/nes/Test.nes","name":"Test","directory":false,"frontend":"retroarch","core":"fceumm","platform":"nes","count":0,"image":null})).unwrap();
        let mut controller = Controller {
            browsers: [true, false].map(|favorites| {
                Browser::launcher_with_inventory(
                    crate::catalog::IndexJob::local().unwrap(),
                    favorites,
                )
                .unwrap()
            }),
            tab: 0,
            overlay: None,
            selection: 0,
            entry: None,
            controls: Default::default(),
            tools: Default::default(),
            details: Default::default(),
            wifi: Default::default(),
            wifi_return: MenuAction::OpenWifi,
            configuration: Default::default(),
            saves: Default::default(),
            saves_from_configuration: false,
            management: Default::default(),
            detail_expanded: false,
            detail_scroll: 0,
            changed_tab: false,
            status: String::new(),
        };
        controller.entry = Some(entry.clone());
        controller.overlay = Some(Overlay::Game);
        controller.selection = controller
            .options()
            .iter()
            .position(|item| item.action == MenuAction::ShowGameDetails)
            .unwrap();
        assert!(controller.input("confirm").unwrap().is_none());
        for _ in 0..1000 {
            controller.details.poll();
            if matches!(controller.details.load, Load::Ready { .. }) {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        let Load::Ready { document } = &controller.details.load else {
            panic!("Details did not finish")
        };
        assert_eq!(document.key, entry.key());
        for (index, section) in [Section::Core, Section::Firmware, Section::Saves]
            .into_iter()
            .enumerate()
        {
            controller.selection = index;
            controller.input("confirm").unwrap();
            assert!(controller.overlay == Some(Overlay::Details(section)));
            assert_eq!(controller.entry.as_ref().unwrap().key(), entry.key());
            controller.input("confirm").unwrap();
            assert!(controller.detail_expanded);
            controller.input("down").unwrap();
            assert_eq!(controller.selection, 0);
            assert_eq!(controller.detail_scroll, 24);
            controller.input("back").unwrap();
            assert!(!controller.detail_expanded);
            controller.input("back").unwrap();
            assert!(controller.overlay == Some(Overlay::Details(Section::Overview)));
            assert_eq!(controller.selection, index);
        }
        controller.input("back").unwrap();
        assert!(controller.overlay == Some(Overlay::Game));
        assert_eq!(
            controller.options()[controller.selection].action,
            MenuAction::ShowGameDetails
        );
        assert_eq!(controller.entry.as_ref().unwrap().key(), entry.key());
        controller.input("back").unwrap();
        assert!(controller.overlay.is_none());
    }

    #[test]
    fn library_activation_and_modal_back_have_separate_owners() {
        for action in ["confirm", "down"] {
            let mut controller = Controller {
                browsers: [
                    Browser::launcher_with_inventory(
                        crate::catalog::IndexJob::local().unwrap(),
                        true,
                    )
                    .unwrap(),
                    Browser::launcher_with_inventory(
                        crate::catalog::IndexJob::local().unwrap(),
                        false,
                    )
                    .unwrap(),
                ],
                tab: 0,
                overlay: None,
                selection: 0,
                entry: None,
                controls: controls::Controls::default(),
                tools: tools::Tools::default(),
                details: Default::default(),
                wifi: Default::default(),
                wifi_return: MenuAction::OpenWifi,
                configuration: Default::default(),
                saves: Default::default(),
                saves_from_configuration: false,
                management: Default::default(),
                detail_expanded: false,
                detail_scroll: 0,
                changed_tab: false,
                status: String::new(),
            };
            assert!(controller.browser().current().unwrap().library_link());
            controller.show(Overlay::Controls);
            controller.input("back").unwrap();
            assert!(controller.overlay == Some(Overlay::System));
            assert_eq!(controller.tab, 0);
            controller.input("back").unwrap();
            controller.input(action).unwrap();
            assert_eq!(controller.tab, 1);
            controller.input("back").unwrap();
            assert_eq!(controller.tab, 0);
            assert!(controller.browser().current().unwrap().library_link());
            controller.input("back").unwrap();
            assert!(!controller.browser().exit);
        }
    }

    #[test]
    fn controls_return_to_the_parent_action_and_unavailable_controls_do_not_close() {
        for back in ["back", "confirm"] {
            let mut controller = Controller {
                browsers: [Browser::new().unwrap(), Browser::new().unwrap()],
                tab: 0,
                overlay: Some(Overlay::Controls),
                selection: 0,
                entry: None,
                controls: controls::Controls::default(),
                tools: tools::Tools::default(),
                details: Default::default(),
                wifi: Default::default(),
                wifi_return: MenuAction::OpenWifi,
                configuration: Default::default(),
                saves: Default::default(),
                saves_from_configuration: false,
                management: Default::default(),
                detail_expanded: false,
                detail_scroll: 0,
                changed_tab: false,
                status: String::new(),
            };
            assert!(controller.input("confirm").unwrap().is_none());
            assert!(controller.overlay == Some(Overlay::Controls));
            assert!(!controller.status.is_empty());
            if back == "confirm" {
                controller.selection = controller
                    .options()
                    .iter()
                    .position(|item| item.action == MenuAction::Back)
                    .unwrap();
            }
            assert!(controller.input(back).unwrap().is_none());
            assert!(controller.overlay == Some(Overlay::System));
            assert_eq!(
                controller.options()[controller.selection].action,
                MenuAction::ShowControls
            );
            assert!(controller.input("back").unwrap().is_none());
            assert!(controller.overlay.is_none());
            controller.show(Overlay::Power);
            assert_eq!(controller.options()[0].action, MenuAction::Back);
            assert!(controller.input(back).unwrap().is_none());
            assert_eq!(
                controller.options()[controller.selection].action,
                MenuAction::ShowPower
            );
            for (action, operation, command) in [
                (MenuAction::Poweroff, PowerAction::Poweroff, "poweroff"),
                (MenuAction::Reboot, PowerAction::Reboot, "reboot"),
            ] {
                controller.show(Overlay::Power);
                controller.selection = controller
                    .options()
                    .iter()
                    .position(|item| item.action == action)
                    .unwrap();
                assert!(
                    matches!(controller.input("confirm").unwrap(), Some(Intent::Power(value)) if value == operation)
                );
                assert_eq!(operation.command(), command);
            }
            controller.tools.refresh();
            controller.show(Overlay::Tools);
            controller.selection = controller
                .options()
                .iter()
                .position(|item| item.action == MenuAction::ShowStorage)
                .unwrap();
            assert!(controller.input("confirm").unwrap().is_none());
            assert!(controller.overlay == Some(Overlay::Details(Section::Storage)));
            controller.detail_expanded = true;
            controller.input("back").unwrap();
            assert!(!controller.detail_expanded);
            assert!(controller.overlay == Some(Overlay::Details(Section::Storage)));
            controller.input("back").unwrap();
            assert!(controller.overlay == Some(Overlay::Tools));
            assert_eq!(
                controller.options()[controller.selection].action,
                MenuAction::ShowStorage
            );
        }
    }
}
