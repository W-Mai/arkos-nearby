use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Security {
    Open,
    #[default]
    Psk,
    Enterprise,
    Unsupported,
}
impl Security {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "开放",
            Self::Psk => "WPA / WPA2",
            Self::Enterprise => "企业认证",
            Self::Unsupported => "其它认证",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub key: String,
    pub ssid: String,
    pub security: Security,
    pub signal: u8,
    pub saved: bool,
    pub connected: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Info {
    pub ssid: String,
    pub address: String,
    pub gateway: String,
    pub signal: u8,
    pub security: Security,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    #[default]
    List,
    Password,
    Hidden,
    Security,
    Connecting,
    Cancelling,
    Restoring,
    Success,
    Info,
    Details,
    Forget,
    Off,
    Empty,
    Busy,
    Failed,
}
impl Page {
    pub fn keyboard(self) -> bool {
        matches!(self, Self::Password | Self::Hidden)
    }
    pub fn pending(self) -> bool {
        matches!(self, Self::Connecting | Self::Cancelling | Self::Restoring)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub epoch: u64,
    pub page: Page,
    pub networks: Vec<Network>,
    pub selected: usize,
    pub target: String,
    pub ssid: String,
    pub scanning: bool,
    pub stage: u8,
    pub info: Option<Info>,
    pub notice: String,
}

// Credentials travel only on the transient GUI-to-controller channel.
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Secret(String);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn value(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Secret([redacted])")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Connect { epoch: u64, secret: Secret },
    Hidden { epoch: u64, ssid: String },
}
impl Request {
    pub fn epoch(&self) -> u64 {
        match self {
            Self::Connect { epoch, .. } | Self::Hidden { epoch, .. } => *epoch,
        }
    }
    pub fn valid(&self) -> bool {
        match self {
            Self::Connect { secret, .. } => valid_psk(secret.value()),
            Self::Hidden { ssid, .. } => valid_ssid(ssid),
        }
    }
}
pub fn valid_ssid(value: &str) -> bool {
    (1..=32).contains(&value.len()) && !value.contains(['\0', '\n', '\r'])
}
pub fn valid_psk(value: &str) -> bool {
    !value.contains(['\0', '\n', '\r'])
        && ((8..=63).contains(&value.len())
            || value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_have_a_redacted_debug_and_no_presentation_field() {
        let secret = Secret::new("a'b: c\\d".into());
        let request = Request::Connect { epoch: 3, secret };
        assert!(request.valid());
        assert!(!format!("{request:?}").contains("a'b"));
        for value in [
            "'quoted'",
            "  spaces  ",
            "0123456789abcdef".repeat(4).as_str(),
        ] {
            assert!(valid_psk(value));
        }
        assert!(!valid_psk("short"));
        assert!(!valid_psk("new\nline"));
        assert!(valid_ssid("  'camp':\\  "));
        assert!(!valid_ssid(&"界".repeat(11)));
    }
}
