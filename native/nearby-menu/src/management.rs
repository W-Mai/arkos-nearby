use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Home {
    #[default]
    Original,
    Howff,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    #[default]
    Device,
    Application,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub address: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub key: String,
    pub name: String,
    pub directory: bool,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    #[default]
    Ready,
    Installing,
    Complete,
    Failed,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub phase: UpdatePhase,
    pub path: String,
    pub version: String,
    pub digest: String,
    pub compatible: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub epoch: u64,
    pub entry: Entry,
    pub revision: u64,
    pub busy: bool,
    pub error: bool,
    pub room_busy: bool,
    pub notice: String,
    pub device: Device,
    pub friends: Vec<Device>,
    pub version: String,
    pub profile: String,
    pub origin: String,
    pub home: Home,
    pub facts: Vec<Fact>,
    pub directory: String,
    pub packages: Vec<Package>,
    pub update: Option<Update>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Refresh,
    Standalone,
    Rename { name: String },
    Forget { id: String },
    SetHome { home: Home },
    Diagnose,
    Browse { directory: String },
    Inspect { path: String },
    Install { digest: String },
    Close,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub epoch: u64,
    pub sequence: u64,
    pub operation: Operation,
}

pub fn valid_name(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 64 && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_names_bound_utf8_bytes_and_control_characters() {
        assert!(valid_name("小林's 掌机"));
        assert!(valid_name(&"字".repeat(21)));
        for invalid in [
            String::new(),
            " ".into(),
            "字".repeat(22),
            "x\u{0085}".into(),
        ] {
            assert!(!valid_name(&invalid));
        }
    }
}
