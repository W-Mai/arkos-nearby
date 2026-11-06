use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    Lobby,
    Candidates,
    Mode,
    #[default]
    Choice,
    Host,
    Rooms,
    Joined,
    Leaving,
    Unavailable,
    Games,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    ResolvingGame,
    SelectingGame,
    #[default]
    Idle,
    Creating,
    WaitingPeer,
    PeerPreparing,
    ApprovalPending,
    HostReady,
    Connecting,
    Checking,
    Notifying,
    WaitingStart,
    Starting,
    Leaving,
    Closed,
    Failed,
    RestoreFailed,
    RoomActive,
    NetworkConfirming,
    NetworkConnected,
    NetworkReconnecting,
    ApprovalPendingClient,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomRow {
    #[serde(default)]
    pub key: String,
    pub name: String,
    pub detail: String,
    pub signal: u8,
    pub available: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    #[serde(default)]
    pub return_to_launcher: bool,
    #[serde(default)]
    pub launcher: LauncherView,
    #[serde(default)]
    pub launcher_shelf: Option<Box<View>>,
    #[serde(default)]
    pub launcher_catalog: Option<Box<View>>,
    #[serde(default)]
    pub library: LibraryView,
    #[serde(default)]
    pub platforms: bool,
    #[serde(default)]
    pub lobby: bool,
    #[serde(default)]
    pub current_game: bool,
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub artwork_neighbors: Vec<String>,
    #[serde(default)]
    pub network_only: bool,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub local_address: String,
    #[serde(default)]
    pub peers: Vec<NetworkPeer>,
    pub page: Page,
    pub title: String,
    pub core: String,
    pub selected: usize,
    #[serde(default)]
    pub room_selected: Option<usize>,
    pub rooms: Vec<RoomRow>,
    pub room_name: String,
    pub ready: bool,
    pub busy: bool,
    #[serde(default)]
    pub refreshing: bool,
    pub status: String,
    #[serde(default)]
    pub phase: Phase,
    #[serde(default)]
    pub back_label: String,
    #[serde(default)]
    pub entries: Vec<GameRow>,
}

impl View {
    pub fn scene_page(&self) -> Page {
        if self.lobby && self.page == Page::Mode {
            Page::Lobby
        } else {
            self.page
        }
    }

    pub fn can_switch_theme(&self) -> bool {
        matches!(self.page, Page::Mode | Page::Choice) && !self.busy
    }

    pub fn host_action_enabled(&self) -> bool {
        (self.ready || self.connected || self.phase == Phase::ApprovalPending) && !self.busy
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPeer {
    pub name: String,
    pub address: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GameRow {
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub icon: String,
    #[serde(default = "available")]
    pub available: bool,
    #[serde(default)]
    pub key: String,
    pub name: String,
    pub detail: String,
    pub directory: bool,
}

impl Default for GameRow {
    fn default() -> Self {
        Self {
            artwork: String::new(),
            favorite: false,
            icon: String::new(),
            available: true,
            key: String::new(),
            name: String::new(),
            detail: String::new(),
            directory: false,
        }
    }
}

fn available() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_rows_are_display_only_and_old_room_views_remain_valid() {
        assert!(serde_json::from_value::<GameRow>(serde_json::json!({"name":"game","detail":"select","directory":false,"path":"/roms/game.zip"})).is_err());
        let view: View = serde_json::from_value(serde_json::json!({"page":"choice","title":"game","core":"core","selected":0,"rooms":[],"room_name":"","ready":false,"busy":false,"status":""})).unwrap();
        assert!(view.entries.is_empty());
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Refresh,
    PageUp,
    PageDown,
    Auxiliary,
    Favorite,
    LauncherHome,
    Start,
    #[serde(skip)]
    #[cfg(any(target_os = "linux", test))]
    Theme,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryKind {
    Platforms,
    #[default]
    Directory,
    Favorites,
    Search,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct LibraryView {
    pub kind: LibraryKind,
    pub load: nearby_menu::LibraryLoad,
    pub query: String,
    pub keyboard: bool,
    pub key_selected: usize,
    pub row_selected: usize,
    pub position: usize,
    pub total: usize,
    pub sort: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct LauncherView {
    pub enabled: bool,
    pub tab: usize,
    pub navigation: bool,
    pub key: String,
    pub netplay: bool,
    pub overlay: String,
    pub overlay_kind: String,
    pub notice: String,
    pub items: Vec<LauncherItem>,
    pub selected: usize,
    pub retained: bool,
    pub details: Option<nearby_menu::details::View>,
    pub wifi: Option<nearby_menu::wifi::View>,
    pub management: Option<nearby_menu::management::View>,
    pub configuration: Option<nearby_menu::configuration::View>,
    pub saves: Option<nearby_menu::saves::View>,
}
pub use nearby_menu::{MenuAction, MenuItem as LauncherItem};

impl LauncherView {
    #[cfg(any(target_os = "linux", test))]
    pub fn selected_action(&self) -> Option<MenuAction> {
        self.items
            .get(self.selected)
            .filter(|item| self.enabled && !self.overlay.is_empty() && item.enabled())
            .map(|item| item.action)
    }
}

#[cfg(test)]
mod menu_contract_tests {
    use super::*;

    #[test]
    fn menu_identity_and_availability_survive_wire_round_trip_and_reordering() {
        let items = [
            LauncherItem::available(MenuAction::ToggleSound, "音效开关"),
            LauncherItem::unavailable(MenuAction::ManageNetwork, "管理直连网络", "未保留直连网络"),
            LauncherItem::available(MenuAction::CycleTheme, "切换主题"),
        ];
        let mut view = LauncherView {
            enabled: true,
            overlay: "设置".into(),
            items: items.to_vec(),
            ..Default::default()
        };
        for _ in 0..items.len() {
            let round_trip: LauncherView =
                serde_json::from_value(serde_json::to_value(&view).unwrap()).unwrap();
            for (index, item) in round_trip.items.iter().enumerate() {
                let mut selected = round_trip.clone();
                selected.selected = index;
                assert_eq!(
                    selected.selected_action(),
                    item.enabled().then_some(item.action)
                );
                assert_eq!(item.enabled(), item.reason().is_empty());
            }
            view.items.rotate_left(1);
        }
        view.overlay.clear();
        assert_eq!(view.selected_action(), None);
    }
}
