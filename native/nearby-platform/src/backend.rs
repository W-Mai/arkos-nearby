//! Typed facts returned by a platform plugin.

use crate::{Location, Storage, arkos, core::CoreSourceRegistry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub const BACKEND_SCHEMA: u8 = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocationRecord {
    pub storage: Storage,
    pub absolute: String,
    pub relative: String,
    pub local: String,
}

impl LocationRecord {
    pub fn from_location(location: Location) -> Self {
        Self {
            storage: location.storage(),
            absolute: location.as_str().into(),
            relative: location.relative().into(),
            local: location.local().into(),
        }
    }

    fn validate(&self) -> bool {
        Path::new(&self.absolute).is_absolute()
            && !Path::new(&self.relative).is_absolute()
            && !Path::new(&self.local).is_absolute()
            && [
                self.absolute.as_str(),
                self.relative.as_str(),
                self.local.as_str(),
            ]
            .iter()
            .all(|value| {
                Path::new(value)
                    .components()
                    .all(|part| !matches!(part, std::path::Component::ParentDir))
            })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtworkSourceRecord {
    pub id: String,
    pub kind: crate::content::ArtworkKind,
    pub root: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceRecord {
    pub id: String,
    pub unit: String,
    pub definition_path: String,
    pub owner_kind: String,
    pub listener_template: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeSource {
    pub systems_paths: Vec<String>,
    pub settings_path: String,
    pub metadata_root: String,
    pub hooks_roots: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSource {
    pub launcher_index: String,
    pub multiplayer_index: String,
}

fn absolute(path: &str) -> bool {
    Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|part| !matches!(part, std::path::Component::ParentDir))
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformSnapshot {
    pub schema: u8,
    pub plugin_id: String,
    pub target: String,
    pub rom_roots: Vec<String>,
    pub locations: Vec<LocationRecord>,
    pub artwork_sources: Vec<ArtworkSourceRecord>,
    pub core_sources: CoreSourceRegistry,
    pub capabilities: Vec<String>,
    pub catalog: CatalogSource,
    pub services: Vec<ServiceRecord>,
    pub recipes: RecipeSource,
}

impl PlatformSnapshot {
    pub fn validate(&self, expected_plugin: &str, expected_target: &str) -> bool {
        if self.schema != BACKEND_SCHEMA
            || self.plugin_id != expected_plugin
            || self.target != expected_target
            || self.rom_roots.is_empty()
            || self
                .rom_roots
                .iter()
                .any(|root| !Path::new(root).is_absolute())
            || self.locations.iter().any(|location| !location.validate())
        {
            return false;
        }
        let unique = self
            .locations
            .iter()
            .map(|location| location.absolute.as_str())
            .collect::<BTreeSet<_>>();
        let cache_locations = self
            .locations
            .iter()
            .filter(|location| location.storage == Storage::Cache)
            .map(|location| location.absolute.as_str())
            .collect::<BTreeSet<_>>();
        unique.len() == self.locations.len()
            && self
                .capabilities
                .iter()
                .all(|capability| identifier(capability))
            && absolute(&self.catalog.launcher_index)
            && absolute(&self.catalog.multiplayer_index)
            && self.catalog.launcher_index != self.catalog.multiplayer_index
            && cache_locations.contains(self.catalog.launcher_index.as_str())
            && cache_locations.contains(self.catalog.multiplayer_index.as_str())
            && self
                .artwork_sources
                .iter()
                .all(|source| identifier(&source.id) && absolute(&source.root))
            && self.core_sources.validate()
            && self.services.iter().all(|service| {
                identifier(&service.id)
                    && !service.unit.is_empty()
                    && absolute(&service.definition_path)
                    && service.owner_kind.len() <= 64
                    && service
                        .listener_template
                        .as_deref()
                        .is_none_or(|template| !template.is_empty() && template.len() <= 128)
            })
            && !self.recipes.systems_paths.is_empty()
            && self.recipes.systems_paths.iter().all(|path| absolute(path))
            && absolute(&self.recipes.settings_path)
            && absolute(&self.recipes.metadata_root)
            && self.recipes.hooks_roots.iter().all(|path| absolute(path))
            && self.recipes.events.iter().all(|event| identifier(event))
    }

    pub fn arkos(plugin_id: &str, target: &str) -> Self {
        let mut locations = Vec::new();
        for group in [
            arkos::application::ALL,
            arkos::runtime::ALL,
            arkos::transient::ALL,
            arkos::home::ALL,
            arkos::firmware::ALL,
            arkos::system_config::ALL,
            arkos::system_program::ALL,
            arkos::input::ALL,
        ] {
            locations.extend(group.iter().copied().map(LocationRecord::from_location));
        }
        Self {
            schema: BACKEND_SCHEMA,
            plugin_id: plugin_id.into(),
            target: target.into(),
            rom_roots: arkos::ROM_ROOTS.iter().map(|root| (*root).into()).collect(),
            locations,
            artwork_sources: arkos::ARTWORK_SOURCES
                .iter()
                .map(|source| ArtworkSourceRecord {
                    id: source.id.into(),
                    kind: source.kind,
                    root: source.root.into(),
                })
                .collect(),
            core_sources: CoreSourceRegistry::arkos(),
            capabilities: vec![
                "paths".into(),
                "launch".into(),
                "services".into(),
                "recipes".into(),
                "artwork".into(),
            ],
            catalog: CatalogSource {
                launcher_index: arkos::application::LAUNCHER_INDEX.as_str().into(),
                multiplayer_index: arkos::application::CATALOG_INDEX.as_str().into(),
            },
            services: vec![
                ServiceRecord {
                    id: "emulationstation".into(),
                    unit: "emulationstation.service".into(),
                    definition_path: arkos::firmware::ES_UNIT.into(),
                    owner_kind: "original_wrapper".into(),
                    listener_template: Some("howff-home-listen-{pid}".into()),
                },
                ServiceRecord {
                    id: "howff-launcher".into(),
                    unit: "howff-launcher.service".into(),
                    definition_path: arkos::application::EXECUTABLE.as_str().into(),
                    owner_kind: "borrowed_es".into(),
                    listener_template: None,
                },
            ],
            recipes: RecipeSource {
                systems_paths: vec![
                    arkos::home::ES_SYSTEMS.as_str().into(),
                    arkos::firmware::ES_SYSTEMS.into(),
                ],
                settings_path: arkos::home::ES_SETTINGS.as_str().into(),
                metadata_root: arkos::home::CONFIGURATION.as_str().into(),
                hooks_roots: vec![
                    arkos::firmware::ES_HOOKS.into(),
                    arkos::home::ES_HOOKS.as_str().into(),
                ],
                events: vec!["game-start".into(), "game-end".into()],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_arkos_snapshot_is_valid() {
        let snapshot = PlatformSnapshot::arkos("arkos-platform", "aarch64-unknown-linux-musl");
        assert!(snapshot.validate("arkos-platform", "aarch64-unknown-linux-musl"));
        assert_eq!(snapshot.rom_roots, ["/roms", "/roms2"]);
    }

    #[test]
    fn snapshot_rejects_parent_paths_and_duplicate_locations() {
        let mut snapshot = PlatformSnapshot::arkos("arkos-platform", "aarch64-unknown-linux-musl");
        snapshot.locations[0].relative = "../outside".into();
        assert!(!snapshot.validate("arkos-platform", "aarch64-unknown-linux-musl"));
        snapshot = PlatformSnapshot::arkos("arkos-platform", "aarch64-unknown-linux-musl");
        snapshot.locations.push(snapshot.locations[0].clone());
        assert!(!snapshot.validate("arkos-platform", "aarch64-unknown-linux-musl"));
    }

    #[test]
    fn snapshot_rejects_relative_catalog_indices() {
        let mut snapshot = PlatformSnapshot::arkos("arkos-platform", "aarch64-unknown-linux-musl");
        snapshot.catalog.launcher_index = "catalog-index.json".into();
        assert!(!snapshot.validate("arkos-platform", "aarch64-unknown-linux-musl"));
    }

    #[test]
    fn snapshot_rejects_unowned_or_duplicate_catalog_indices() {
        let mut snapshot = PlatformSnapshot::arkos("arkos-platform", "aarch64-unknown-linux-musl");
        snapshot.catalog.launcher_index = "/opt/howff/device.json".into();
        assert!(!snapshot.validate("arkos-platform", "aarch64-unknown-linux-musl"));
        snapshot = PlatformSnapshot::arkos("arkos-platform", "aarch64-unknown-linux-musl");
        snapshot.catalog.multiplayer_index = snapshot.catalog.launcher_index.clone();
        assert!(!snapshot.validate("arkos-platform", "aarch64-unknown-linux-musl"));
    }
}
