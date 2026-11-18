//! Read-only device telemetry, independent of room and Internet connectivity.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub battery: Option<Battery>,
    pub wireless: Wireless,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    pub percent: Option<u8>,
    pub charging: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wireless {
    #[default]
    Unknown,
    Disconnected,
    Connected,
}

/// Linux power-supply and network sysfs interfaces. Reads never scan or change Wi-Fi.
pub struct LinuxStatus {
    root: PathBuf,
}

impl LinuxStatus {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn read(&self) -> Status {
        Status {
            battery: self.battery(),
            wireless: self.wireless(),
        }
    }

    fn battery(&self) -> Option<Battery> {
        let mut supplies = fs::read_dir(self.root.join("sys/class/power_supply"))
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        supplies.sort();
        supplies.into_iter().find_map(|path| {
            if read(&path.join("type"))? != "Battery"
                || read(&path.join("scope")).as_deref() == Some("Device")
                || read(&path.join("present")).as_deref() == Some("0")
            {
                return None;
            }
            Some(Battery {
                percent: read(&path.join("capacity"))
                    .and_then(|value| value.parse::<u8>().ok())
                    .filter(|value| *value <= 100),
                charging: read(&path.join("status")).as_deref() == Some("Charging"),
            })
        })
    }

    fn wireless(&self) -> Wireless {
        let Ok(interfaces) = fs::read_dir(self.root.join("sys/class/net")) else {
            return Wireless::Unknown;
        };
        let legacy = read(&self.root.join("proc/net/wireless")).unwrap_or_default();
        let mut state = Wireless::Disconnected;
        for entry in interfaces.filter_map(Result::ok) {
            let path = entry.path();
            let name = entry.file_name();
            let legacy_interface = legacy.lines().any(|line| {
                line.split_once(':')
                    .is_some_and(|(interface, _)| interface.trim() == name.to_string_lossy())
            });
            if !path.join("wireless").is_dir()
                && !path.join("phy80211").exists()
                && !legacy_interface
            {
                continue;
            }
            // Carrier describes association even before DHCP or without an Internet route.
            match read(&path.join("carrier")).as_deref() {
                Some("1") => return Wireless::Connected,
                Some("0") => (),
                _ => state = Wireless::Unknown,
            }
        }
        state
    }
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_uses_system_battery_and_wireless_carrier_without_an_ip() {
        let root = std::env::temp_dir().join(format!("howff-status-{}", std::process::id()));
        let write = |path: &str, value: &str| {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, value).unwrap();
        };
        write("sys/class/power_supply/accessory/type", "Battery");
        write("sys/class/power_supply/accessory/scope", "Device");
        write("sys/class/power_supply/accessory/capacity", "100");
        write("sys/class/power_supply/battery/type", "Battery");
        write("sys/class/power_supply/battery/capacity", "87\n");
        write("sys/class/power_supply/battery/status", "Charging\n");
        write("sys/class/net/eth0/carrier", "1");
        write("sys/class/net/wlan0/carrier", "0");
        write("proc/net/wireless", "Inter-| sta\n wlan0: 0000 0. 0. 0.\n");
        let source = LinuxStatus::new(&root);
        assert_eq!(
            source.read(),
            Status {
                battery: Some(Battery {
                    percent: Some(87),
                    charging: true
                }),
                wireless: Wireless::Disconnected,
            }
        );
        write("sys/class/net/wlan0/carrier", "1");
        assert_eq!(source.read().wireless, Wireless::Connected);
        write("sys/class/power_supply/battery/capacity", "255");
        assert_eq!(source.read().battery.unwrap().percent, None);
        write("sys/class/power_supply/battery/present", "0");
        assert_eq!(source.read().battery, None);
        fs::remove_file(root.join("sys/class/net/wlan0/carrier")).unwrap();
        assert_eq!(source.read().wireless, Wireless::Unknown);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(source.read(), Status::default());
    }
}
