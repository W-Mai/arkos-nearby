//! The active platform's wireless implementation.

pub(crate) use super::arkos::radio::has_ipv4;
pub use super::arkos::radio::{
    Control, DRIVER_SHA, SERVICES, WPA_SHA, dhcp_hook, host, join, restore, services,
};

pub use super::arkos::lease::task;
pub(crate) use super::arkos::lease::{
    STARTUP_SECONDS, ScanRequest, arm, capture, disarm, quiesce, refresh_original,
    restore_connection, scan, scan_activity, start_worker, unit, validate_recovery,
};

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiceState {
    pub active: bool,
    #[serde(default)]
    pub enabled: String,
}

/// A named snapshot of services managed by the active wireless backend.
#[derive(Clone, Serialize)]
#[serde(transparent)]
pub struct ServicesBefore(pub(crate) BTreeMap<String, ServiceState>);

impl ServicesBefore {
    pub(crate) fn active(&self, unit: &str) -> bool {
        self.0.get(unit).is_some_and(|service| service.active)
    }
}

impl<'de> Deserialize<'de> for ServicesBefore {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let services = BTreeMap::<String, ServiceState>::deserialize(deserializer)?;
        if services.len() != SERVICES.len()
            || SERVICES.iter().any(|unit| !services.contains_key(*unit))
        {
            return Err(serde::de::Error::custom(
                "Wireless service snapshot belongs to another backend",
            ));
        }
        Ok(Self(services))
    }
}

pub(crate) struct Before {
    pub original_uuid: String,
    pub radio_before: String,
    pub services_before: ServicesBefore,
}

pub(crate) struct Recovery {
    pub restored: bool,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    #[test]
    fn named_service_journals_preserve_backend_identity() {
        for active in [[false, false], [false, true], [true, false], [true, true]] {
            let value: ServicesBefore = serde_json::from_value(serde_json::json!(
                SERVICES
                    .into_iter()
                    .zip(active)
                    .map(|(unit, active)| (
                        unit,
                        ServiceState {
                            active,
                            enabled: String::new()
                        }
                    ))
                    .collect::<BTreeMap<_, _>>()
            ))
            .unwrap();
            for (unit, active) in SERVICES.into_iter().zip(active) {
                assert_eq!(value.active(unit), active);
            }
            let named = serde_json::to_value(&value).unwrap();
            assert!(named.is_object());
            let round_trip: ServicesBefore = serde_json::from_value(named).unwrap();
            for unit in SERVICES {
                assert_eq!(value.active(unit), round_trip.active(unit));
            }
        }
        for invalid in [
            serde_json::json!([]),
            serde_json::json!([true]),
            serde_json::json!([true, false]),
            serde_json::json!([true, false, true]),
            serde_json::json!({}),
            serde_json::json!({"foreign.service":{"active":true,"enabled":"enabled"}}),
        ] {
            assert!(serde_json::from_value::<ServicesBefore>(invalid).is_err());
        }
    }
}
