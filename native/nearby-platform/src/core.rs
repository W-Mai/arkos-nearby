//! Verified core source facts shared by the platform plugin and the host.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CORE_REGISTRY_SCHEMA: u8 = 1;

/// The ownership/source of a core binary.
///
/// The value is deliberately closed: UI and network code must not invent a
/// third provenance when a core is selected or advertised to a peer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreSource {
    OptimizedOwned,
    DeviceInstalled,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoreRecord {
    pub frontend: String,
    pub id: String,
    pub platforms: Vec<String>,
    pub source: CoreSource,
    pub version: String,
    pub binary_sha256: String,
    pub abi: String,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSourceRegistry {
    pub schema: u8,
    pub records: Vec<CoreRecord>,
}

impl CoreSourceRegistry {
    /// The records shipped by the ArkOS platform plugin.
    ///
    /// Device-installed records are added only after the host has inspected a
    /// concrete local core. The plugin therefore never reports an installed
    /// core merely because a filename is present in a preference list.
    pub fn arkos() -> Self {
        Self {
            schema: CORE_REGISTRY_SCHEMA,
            records: vec![CoreRecord {
                frontend: "retroarch".into(),
                id: "nestopia_nearby".into(),
                platforms: vec!["nes".into(), "fds".into()],
                source: CoreSource::OptimizedOwned,
                version: "1.53.27dfdc25-footer1".into(),
                binary_sha256: "2c276c3338e7df8105213f7c2008a8e2a0cd307a89f40f19fbde05fc2c84f430"
                    .into(),
                abi: "elf64".into(),
                capabilities: vec!["deterministic".into(), "netplay".into(), "save".into()],
            }],
        }
    }

    pub fn find(&self, frontend: &str, id: &str) -> Option<&CoreRecord> {
        self.records
            .iter()
            .find(|record| record.frontend == frontend && record.id == id)
    }

    pub fn validate(&self) -> bool {
        if self.schema != CORE_REGISTRY_SCHEMA {
            return false;
        }
        let mut keys = BTreeSet::new();
        self.records.iter().all(|record| {
            identifier(&record.frontend)
                && identifier(&record.id)
                && keys.insert((record.frontend.as_str(), record.id.as_str()))
                && !record.platforms.is_empty()
                && record.platforms.iter().all(|platform| identifier(platform))
                && source_fields_valid(record)
                && record
                    .capabilities
                    .iter()
                    .all(|capability| identifier(capability))
                && unique_identifiers(&record.platforms)
                && unique_identifiers(&record.capabilities)
        })
    }
}

fn source_fields_valid(record: &CoreRecord) -> bool {
    match record.source {
        CoreSource::Unavailable => {
            record.version.is_empty() && record.binary_sha256.is_empty() && record.abi.is_empty()
        }
        CoreSource::OptimizedOwned | CoreSource::DeviceInstalled => {
            !record.version.is_empty()
                && !record.abi.is_empty()
                && record.binary_sha256.len() == 64
                && record
                    .binary_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
        }
    }
}

fn unique_identifiers(values: &[String]) -> bool {
    let unique = values.iter().collect::<BTreeSet<_>>();
    unique.len() == values.len()
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arkos_registry_contains_only_verified_owned_artifacts() {
        let registry = CoreSourceRegistry::arkos();
        assert!(registry.validate());
        let record = registry.find("retroarch", "nestopia_nearby").unwrap();
        assert_eq!(record.source, CoreSource::OptimizedOwned);
        assert_eq!(record.platforms, ["nes", "fds"]);
    }

    #[test]
    fn registry_rejects_duplicate_identity_and_bad_digest() {
        let mut registry = CoreSourceRegistry::arkos();
        registry.records.push(registry.records[0].clone());
        assert!(!registry.validate());
        registry.records.pop();
        registry.records[0].binary_sha256 = "bad".into();
        assert!(!registry.validate());
    }

    #[test]
    fn unavailable_records_cannot_claim_an_artifact() {
        let mut registry = CoreSourceRegistry::arkos();
        registry.records[0].source = CoreSource::Unavailable;
        assert!(!registry.validate());
        registry.records[0].version.clear();
        registry.records[0].binary_sha256.clear();
        registry.records[0].abi.clear();
        assert!(registry.validate());
    }
}
