use serde::{Deserialize, Serialize};

pub mod configuration;
pub mod details;
pub mod input;
pub mod management;
pub mod saves;
pub mod shelf;
pub mod wifi;

pub const LIST_WINDOW_ITEMS: usize = 8;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum LibraryLoad {
    #[default]
    Ready,
    Loading,
    Failed {
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MenuAction {
    Play,
    Multiplayer,
    ToggleFavorite,
    ShowGameDetails,
    ShowGameSettings,
    ShowSaveManager,
    ShowCoreDetails,
    ShowFirmwareDetails,
    ShowSaveDetails,
    ExpandDetail,
    CycleTheme,
    ToggleSound,
    ShowControls,
    ShowDevice,
    ShowApplication,
    ShowTools,
    OpenWifi,
    NetworkInfo,
    SystemInfo,
    ShowStorage,
    FileManager,
    Volume,
    Brightness,
    ManageNetwork,
    ShowExit,
    ShowPower,
    Exit,
    KeepNetworkExit,
    Poweroff,
    Reboot,
    RestorePlay,
    #[default]
    Back,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Availability {
    #[default]
    Available,
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct MenuItem {
    pub action: MenuAction,
    pub name: String,
    pub availability: Availability,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SystemLevel {
    Volume,
    Brightness,
}

impl SystemLevel {
    pub const ALL: [Self; 2] = [Self::Volume, Self::Brightness];
    pub fn index(self) -> usize {
        match self {
            Self::Volume => 0,
            Self::Brightness => 1,
        }
    }
    pub fn action(self) -> MenuAction {
        match self {
            Self::Volume => MenuAction::Volume,
            Self::Brightness => MenuAction::Brightness,
        }
    }
    pub fn from_action(action: MenuAction) -> Option<Self> {
        match action {
            MenuAction::Volume => Some(Self::Volume),
            MenuAction::Brightness => Some(Self::Brightness),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Volume => "音量",
            Self::Brightness => "亮度",
        }
    }
    pub fn minimum(self) -> u8 {
        match self {
            Self::Volume => 0,
            Self::Brightness => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub value: Option<u8>,
    pub minimum: u8,
    pub maximum: u8,
    pub step: u8,
    pub muted: bool,
    pub pending: bool,
}

impl Level {
    pub fn new(control: SystemLevel, value: Option<u8>, muted: bool, pending: bool) -> Self {
        Self {
            value,
            minimum: control.minimum(),
            maximum: 100,
            step: 5,
            muted,
            pending,
        }
    }
}

impl MenuItem {
    pub fn available(action: MenuAction, name: impl Into<String>) -> Self {
        Self {
            action,
            name: name.into(),
            availability: Availability::Available,
            value: String::new(),
            level: None,
        }
    }

    pub fn unavailable(
        action: MenuAction,
        name: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            action,
            name: name.into(),
            availability: Availability::Unavailable {
                reason: reason.into(),
            },
            value: String::new(),
            level: None,
        }
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }
    pub fn with_level(mut self, level: Level) -> Self {
        self.level = Some(level);
        self
    }

    pub fn enabled(&self) -> bool {
        matches!(self.availability, Availability::Available)
    }

    pub fn reason(&self) -> &str {
        match &self.availability {
            Availability::Available => "",
            Availability::Unavailable { reason } => reason,
        }
    }
}
