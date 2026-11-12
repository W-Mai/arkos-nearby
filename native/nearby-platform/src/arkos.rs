//! ArkOS installation, user data and firmware locations.

use crate::{Location, PlatformPaths, Storage};

macro_rules! locations {
    ($root:literal; $($name:ident: $storage:ident = $relative:literal;)+) => {
        pub const ROOT: &str = concat!("/", $root);
        pub const RELATIVE_ROOT: &str = $root;
        $(pub const $name: Location = Location {
            storage: Storage::$storage,
            absolute: concat!("/", $root, "/", $relative),
            relative: concat!($root, "/", $relative),
            local: $relative,
        };)+
        pub const ALL: &[Location] = &[$($name,)+];
        #[cfg(test)]
        pub(super) const NAMED: &[(&str, Location)] = &[$((stringify!($name), $name),)+];
    };
}

pub mod application {
    use super::*;
    locations! { "opt/howff";
        EXECUTABLE: Program = "howff";
        GUI: Program = "howff-gui";
        GUI_BUILD: Program = "howff-gui-build.json";
        ASSETS: Program = "assets";
        CORES: Program = "cores";
        CORE_INSPECT32: Program = "core-inspect32";
        CORE_INSPECT64: Program = "core-inspect64";
        CORE_PROBE: Program = "core-probe64";
        CORE_RUNTIME: Program = "compat/arm64";
        INPUT_FILTER: Program = "assets/home-input.so";
        COMPRESSION: Program = "assets/netplay-compression.so";
        DRIVER: Program = "assets/8188eu.ko";
        SUPPLICANT: Program = "assets/wpa_supplicant";
        THEME: Configuration = "ui-theme";
        SOUND: Configuration = "ui-sound.json";
        HOME_PREFERENCE: Configuration = "launcher-home.json";
        IDENTITY: Personal = "identity.json";
        PRIVATE_KEY: Personal = "identity.pk8";
        DEVICE: Personal = "device.json";
        FRIENDS: Personal = "friends.json";
        PAIRING: Personal = "pair.json";
        LIBRARY: Personal = "library.json";
        LAUNCHER: Personal = "launcher.json";
        SAVE_BACKUPS: Personal = "saves/backups";
        CATALOG_INDEX: Cache = "catalog-index.json";
        LAUNCHER_INDEX: Cache = "launcher-index.json";
        LOGS: Cache = "logs";
        UPDATES: Maintenance = ".updates";
        CONFIGURATION_JOURNAL: Maintenance = "configuration-transaction.json";
        INSTALL_JOURNAL: Maintenance = "install-journal.json";
        INSTALL_RECORD: Maintenance = "install.json";
        ORIGINAL_LAUNCH: Maintenance = "original-launch";
    }
}

pub mod runtime {
    use super::*;
    locations! { "run/howff";
        UI: Runtime = "ui";
        SESSION: Runtime = "session.json";
        WIFI_OPERATION: Runtime = "wifi-operation.json";
        TOOL_OWNER: Runtime = "tool-owner.json";
        HOME_OWNER: Runtime = "home-owner.json";
        HOME_RETURN: Runtime = "home-return-original.json";
    }
}

pub mod transient {
    use super::*;
    locations! { "run";
        LAUNCH: Runtime = "howff/launch";
        SAVE_LAUNCH: Runtime = "howff/save-launch";
        CONFIGURATION: Runtime = "howff/configuration";
        FRONTEND_VERSIONS: Runtime = "howff/frontend-versions";
        MODPROBE: Runtime = "modprobe.d";
    }
}

pub mod home {
    use super::*;
    locations! { "home/ark";
        CONFIGURATION: Firmware = ".config";
        RETROARCH: Firmware = ".config/retroarch";
        RETROARCH32: Firmware = ".config/retroarch32";
        ES_SYSTEMS: Firmware = ".emulationstation/es_systems.cfg";
        ES_SETTINGS: Firmware = ".emulationstation/es_settings.cfg";
        ES_HOOKS: Firmware = ".emulationstation/scripts";
    }
}

pub mod firmware {
    use super::*;
    locations! { "opt";
        FRONTENDS: Firmware = "retroarch/bin";
        RETROARCH: Firmware = "retroarch/bin/retroarch";
        INPUT_MAPPER: Firmware = "inttools/gptokeyb";
        CONTROLLERS: Firmware = "inttools/gamecontrollerdb.txt";
        TOOLS: Firmware = "system";
        SYSTEM_INFO: Firmware = "system/System Info.sh";
        SYSTEM_INFO_FALLBACK: Firmware = "system/Tools/System Info.sh";
        FILE_MANAGER: Firmware = "system/351Files.sh";
        MULTIPLAYER_ENTRY: Firmware = "system/Howff Nearby.sh";
        LAUNCHER_ENTRY: Firmware = "system/Howff Launcher.sh";
    }
    pub const ES_SYSTEMS: &str = super::system_config::ES_SYSTEMS.as_str();
    pub const ES_SETTINGS: &str = super::system_config::ES_SETTINGS.as_str();
    pub const ES_HOOKS: &str = super::system_program::ES_HOOKS.as_str();
    pub const ES_EXECUTABLE: &str = super::system_program::ES_EXECUTABLE.as_str();
    pub const ES_WRAPPER: &str = super::system_program::ES_WRAPPER.as_str();
    pub const ES_WRAPPER_RELATIVE: &str = super::system_program::ES_WRAPPER.relative();
    pub const ES_UNIT: &str = super::system_config::ES_UNIT.as_str();
}

pub mod system_config {
    use super::*;
    locations! { "etc";
        ES_SYSTEMS: Firmware = "emulationstation/es_systems.cfg";
        ES_SETTINGS: Firmware = "emulationstation/es_settings.cfg";
        ES_UNIT: Firmware = "systemd/system/emulationstation.service";
    }
}

pub mod system_program {
    use super::*;
    locations! { "usr";
        ES_EXECUTABLE: Firmware = "bin/emulationstation/emulationstation";
        ES_WRAPPER: Firmware = "bin/emulationstation/emulationstation.sh";
        ES_HOOKS: Firmware = "bin/emulationstation/scripts";
        PERFORMANCE_HELPER: Firmware = "local/bin/perfnorm";
    }
}

pub const ROM_ROOTS: &[&str] = &["/roms", "/roms2"];

pub(crate) const CONTENT: crate::content::ContentPaths = crate::content::ContentPaths {
    rom_roots: ROM_ROOTS,
    artwork_extra: &[
        application::SAVE_BACKUPS,
        home::RETROARCH,
        home::RETROARCH32,
    ],
    catalog_index: application::CATALOG_INDEX,
    launcher_index: application::LAUNCHER_INDEX,
};
pub const BIOS: &str = "/roms/bios";

pub mod input {
    use super::*;
    locations! { "dev/input/by-path";
        JOYSTICK: Firmware = "platform-odroidgo3-joypad-event-joystick";
    }
}

pub const PATHS: PlatformPaths = PlatformPaths {
    program: application::ROOT,
    configuration: application::ROOT,
    personal: application::ROOT,
    cache: application::ROOT,
    runtime: runtime::ROOT,
    rom_roots: ROM_ROOTS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::{Component, Path};

    #[test]
    fn declared_locations_are_unique_absolute_and_installer_relative() {
        let mut seen = BTreeSet::new();
        for location in [
            application::ALL,
            runtime::ALL,
            transient::ALL,
            home::ALL,
            firmware::ALL,
        ]
        .into_iter()
        .flatten()
        {
            assert!(seen.insert(location.as_str()));
            assert!(location.path().is_absolute());
            assert!(!Path::new(location.relative()).is_absolute());
            assert_eq!(Path::new("/").join(location.relative()), location.path());
            assert!(
                location
                    .path()
                    .components()
                    .all(|part| !matches!(part, Component::ParentDir | Component::CurDir))
            );
        }
    }

    #[test]
    fn application_roles_share_the_existing_layout_without_reclassifying_personal_data() {
        for location in application::ALL {
            assert!(
                location
                    .path()
                    .starts_with(PATHS.root(location.storage()).unwrap())
            );
        }
        assert_eq!(application::PRIVATE_KEY.storage(), Storage::Personal);
        assert_eq!(application::SAVE_BACKUPS.storage(), Storage::Personal);
        assert_eq!(application::CATALOG_INDEX.storage(), Storage::Cache);
        assert_eq!(application::UPDATES.storage(), Storage::Maintenance);
        assert_eq!(PATHS.personal, PATHS.cache);
        assert_eq!(PATHS.rom_roots, ["/roms", "/roms2"]);
    }

    #[test]
    fn howff_runtime_and_firmware_locations_have_distinct_ownership() {
        assert_eq!(PATHS.runtime, "/run/howff");
        assert_eq!(transient::LAUNCH.as_str(), "/run/howff/launch");
        assert_eq!(transient::SAVE_LAUNCH.as_str(), "/run/howff/save-launch");
        assert_eq!(
            transient::FRONTEND_VERSIONS.as_str(),
            "/run/howff/frontend-versions"
        );
        assert_eq!(application::EXECUTABLE.as_str(), "/opt/howff/howff");
        assert_eq!(application::THEME.as_str(), "/opt/howff/ui-theme");
        assert_eq!(home::RETROARCH.as_str(), "/home/ark/.config/retroarch");
    }
}

#[cfg(test)]
mod layout_compatibility {
    use super::*;
    use std::collections::BTreeMap;

    // Application locations are owned by Howff; firmware locations remain platform facts.
    const EXPECTED: &str = r#"application::ASSETS	/opt/howff/assets
application::CATALOG_INDEX	/opt/howff/catalog-index.json
application::COMPRESSION	/opt/howff/assets/netplay-compression.so
application::CONFIGURATION_JOURNAL	/opt/howff/configuration-transaction.json
application::CORES	/opt/howff/cores
application::CORE_INSPECT32	/opt/howff/core-inspect32
application::CORE_INSPECT64	/opt/howff/core-inspect64
application::CORE_PROBE	/opt/howff/core-probe64
application::CORE_RUNTIME	/opt/howff/compat/arm64
application::DEVICE	/opt/howff/device.json
application::DRIVER	/opt/howff/assets/8188eu.ko
application::EXECUTABLE	/opt/howff/howff
application::FRIENDS	/opt/howff/friends.json
application::GUI	/opt/howff/howff-gui
application::GUI_BUILD	/opt/howff/howff-gui-build.json
application::HOME_PREFERENCE	/opt/howff/launcher-home.json
application::IDENTITY	/opt/howff/identity.json
application::INPUT_FILTER	/opt/howff/assets/home-input.so
application::INSTALL_JOURNAL	/opt/howff/install-journal.json
application::INSTALL_RECORD	/opt/howff/install.json
application::LAUNCHER	/opt/howff/launcher.json
application::LAUNCHER_INDEX	/opt/howff/launcher-index.json
application::LIBRARY	/opt/howff/library.json
application::LOGS	/opt/howff/logs
application::ORIGINAL_LAUNCH	/opt/howff/original-launch
application::PAIRING	/opt/howff/pair.json
application::PRIVATE_KEY	/opt/howff/identity.pk8
application::SAVE_BACKUPS	/opt/howff/saves/backups
application::SOUND	/opt/howff/ui-sound.json
application::SUPPLICANT	/opt/howff/assets/wpa_supplicant
application::THEME	/opt/howff/ui-theme
application::UPDATES	/opt/howff/.updates
firmware::CONTROLLERS	/opt/inttools/gamecontrollerdb.txt
firmware::FILE_MANAGER	/opt/system/351Files.sh
firmware::FRONTENDS	/opt/retroarch/bin
firmware::INPUT_MAPPER	/opt/inttools/gptokeyb
firmware::LAUNCHER_ENTRY	/opt/system/Howff Launcher.sh
firmware::MULTIPLAYER_ENTRY	/opt/system/Howff Nearby.sh
firmware::RETROARCH	/opt/retroarch/bin/retroarch
firmware::SYSTEM_INFO	/opt/system/System Info.sh
firmware::SYSTEM_INFO_FALLBACK	/opt/system/Tools/System Info.sh
firmware::TOOLS	/opt/system
home::CONFIGURATION	/home/ark/.config
home::ES_HOOKS	/home/ark/.emulationstation/scripts
input::JOYSTICK	/dev/input/by-path/platform-odroidgo3-joypad-event-joystick
home::ES_SETTINGS	/home/ark/.emulationstation/es_settings.cfg
home::ES_SYSTEMS	/home/ark/.emulationstation/es_systems.cfg
home::RETROARCH	/home/ark/.config/retroarch
home::RETROARCH32	/home/ark/.config/retroarch32
runtime::HOME_OWNER	/run/howff/home-owner.json
runtime::HOME_RETURN	/run/howff/home-return-original.json
runtime::SESSION	/run/howff/session.json
runtime::TOOL_OWNER	/run/howff/tool-owner.json
runtime::UI	/run/howff/ui
runtime::WIFI_OPERATION	/run/howff/wifi-operation.json
system_config::ES_SETTINGS	/etc/emulationstation/es_settings.cfg
system_config::ES_SYSTEMS	/etc/emulationstation/es_systems.cfg
system_config::ES_UNIT	/etc/systemd/system/emulationstation.service
system_program::ES_EXECUTABLE	/usr/bin/emulationstation/emulationstation
system_program::ES_HOOKS	/usr/bin/emulationstation/scripts
system_program::ES_WRAPPER	/usr/bin/emulationstation/emulationstation.sh
system_program::PERFORMANCE_HELPER	/usr/local/bin/perfnorm
transient::CONFIGURATION	/run/howff/configuration
transient::FRONTEND_VERSIONS	/run/howff/frontend-versions
transient::LAUNCH	/run/howff/launch
transient::MODPROBE	/run/modprobe.d
transient::SAVE_LAUNCH	/run/howff/save-launch
"#;

    #[test]
    fn every_named_location_preserves_the_existing_arkos_layout() {
        let mut actual = BTreeMap::new();
        for (module, entries) in [
            ("application", application::NAMED),
            ("runtime", runtime::NAMED),
            ("transient", transient::NAMED),
            ("home", home::NAMED),
            ("firmware", firmware::NAMED),
            ("system_config", system_config::NAMED),
            ("system_program", system_program::NAMED),
            ("input", input::NAMED),
        ] {
            for (name, location) in entries {
                assert!(
                    actual
                        .insert(format!("{module}::{name}"), location.as_str().to_string())
                        .is_none()
                );
                assert_eq!(
                    std::path::Path::new("/").join(location.relative()),
                    location.path()
                );
                if location.storage() == Storage::Runtime {
                    assert!(location.path().starts_with("/run"));
                }
            }
        }
        let expected: BTreeMap<_, _> = EXPECTED
            .lines()
            .map(|line| {
                let (name, path) = line.split_once('\t').unwrap();
                (name.to_string(), path.to_string())
            })
            .collect();
        assert_eq!(actual, expected);
    }
}
