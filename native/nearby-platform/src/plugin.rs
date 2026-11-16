//! Versioned metadata for a Howff platform, resource, or core plugin.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const PLUGIN_SCHEMA: u8 = 1;
pub const HOST_API: u16 = 1;
pub const PLUGIN_API: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Platform,
    Resource,
    Core,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub schema: u8,
    pub plugin_id: String,
    pub kind: PluginKind,
    pub target: String,
    pub host_api: u16,
    pub plugin_api: u16,
    pub mirui_abi: String,
    pub capabilities: Vec<String>,
    pub payload_sha256: String,
    pub signature: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PluginRequirements<'a> {
    pub target: &'a str,
    pub host_api: u16,
    pub plugin_api: u16,
    pub mirui_abi: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    InvalidField(&'static str),
    Mismatch(&'static str),
    DuplicateCapability,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(f, "invalid plugin manifest field: {field}"),
            Self::Mismatch(field) => write!(f, "plugin manifest does not match {field}"),
            Self::DuplicateCapability => f.write_str("plugin manifest repeats a capability"),
        }
    }
}

impl std::error::Error for ManifestError {}

impl PluginManifest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn signing_payload(&self, payload: &[u8]) -> Result<Vec<u8>, serde_json::Error> {
        let mut unsigned = self.clone();
        unsigned.signature.clear();
        Ok([serde_json::to_vec(&unsigned)?, payload.to_vec()].concat())
    }

    pub fn validate(&self, required: PluginRequirements<'_>) -> Result<(), ManifestError> {
        if self.schema != PLUGIN_SCHEMA {
            return Err(ManifestError::Mismatch("schema"));
        }
        if !identifier(&self.plugin_id) {
            return Err(ManifestError::InvalidField("plugin_id"));
        }
        if self.target != required.target {
            return Err(ManifestError::Mismatch("target"));
        }
        if self.host_api != required.host_api {
            return Err(ManifestError::Mismatch("host_api"));
        }
        if self.plugin_api != required.plugin_api {
            return Err(ManifestError::Mismatch("plugin_api"));
        }
        if self.mirui_abi != required.mirui_abi || self.mirui_abi.is_empty() {
            return Err(ManifestError::Mismatch("mirui_abi"));
        }
        if self.capabilities.len() > 64 {
            return Err(ManifestError::InvalidField("capabilities"));
        }
        let mut sorted = self.capabilities.clone();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() != self.capabilities.len() {
            return Err(ManifestError::DuplicateCapability);
        }
        if self
            .capabilities
            .iter()
            .any(|capability| !identifier(capability))
        {
            return Err(ManifestError::InvalidField("capabilities"));
        }
        if !hex_digest(&self.payload_sha256) {
            return Err(ManifestError::InvalidField("payload_sha256"));
        }
        if self.signature.is_empty() || self.signature.len() > 4096 {
            return Err(ManifestError::InvalidField("signature"));
        }
        Ok(())
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

fn hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> PluginManifest {
        PluginManifest {
            schema: PLUGIN_SCHEMA,
            plugin_id: "arkos-platform".into(),
            kind: PluginKind::Platform,
            target: "aarch64-unknown-linux-musl".into(),
            host_api: HOST_API,
            plugin_api: PLUGIN_API,
            mirui_abi: "mirui-0.56.0".into(),
            capabilities: vec!["paths".into(), "launch".into()],
            payload_sha256: "a".repeat(64),
            signature: "signed-payload".into(),
        }
    }

    fn requirements() -> PluginRequirements<'static> {
        PluginRequirements {
            target: "aarch64-unknown-linux-musl",
            host_api: HOST_API,
            plugin_api: PLUGIN_API,
            mirui_abi: "mirui-0.56.0",
        }
    }

    #[test]
    fn accepts_matching_manifest() {
        assert!(manifest().validate(requirements()).is_ok());
    }

    #[test]
    fn rejects_abi_and_target_mismatches() {
        let mut value = manifest();
        value.mirui_abi = "mirui-0.55.0".into();
        assert_eq!(
            value.validate(requirements()),
            Err(ManifestError::Mismatch("mirui_abi"))
        );
        value = manifest();
        value.target = "x86_64-unknown-linux-gnu".into();
        assert_eq!(
            value.validate(requirements()),
            Err(ManifestError::Mismatch("target"))
        );
    }

    #[test]
    fn rejects_duplicate_or_malformed_fields() {
        let mut value = manifest();
        value.capabilities.push("paths".into());
        assert_eq!(
            value.validate(requirements()),
            Err(ManifestError::DuplicateCapability)
        );
        value = manifest();
        value.payload_sha256 = "not-a-digest".into();
        assert_eq!(
            value.validate(requirements()),
            Err(ManifestError::InvalidField("payload_sha256"))
        );
    }

    #[test]
    fn json_parser_denies_unknown_fields() {
        let json = r#"{
            "schema": 1,
            "plugin_id": "arkos-platform",
            "kind": "platform",
            "target": "aarch64-unknown-linux-musl",
            "host_api": 1,
            "plugin_api": 1,
            "mirui_abi": "mirui-0.56.0",
            "capabilities": [],
            "payload_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "signature": "signed",
            "unexpected": true
        }"#;
        assert!(PluginManifest::from_json(json.as_bytes()).is_err());
    }
}
