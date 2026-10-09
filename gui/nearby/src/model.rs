use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Page {
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
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomRow {
    pub name: String,
    pub detail: String,
    pub signal: u8,
    pub available: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub page: Page,
    pub title: String,
    pub core: String,
    pub selected: usize,
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GameRow {
    pub name: String,
    pub detail: String,
    pub directory: bool,
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg(target_os = "linux")]
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
}
