use crate::content::digest;
use crate::platform::radio::{Before, Recovery, ServiceState, ServicesBefore};
use crate::session::{BINARY, State, best, cmd};
use crate::system::command;
use crate::{Result, require};
use std::fs;
use std::path::Path;

use super::radio::SERVICES;

pub const STARTUP_SECONDS: u64 = 300;

pub fn unit(id: &str) -> String {
    format!("howff-{id}")
}

pub fn capture() -> Result<Before> {
    let original = cmd(&["nmcli", "-g", "GENERAL.CON-UUID", "device", "show", "wlan0"])?;
    Ok(Before {
        original_uuid: if original == "--" {
            String::new()
        } else {
            original
        },
        radio_before: best(&["nmcli", "radio", "wifi"]),
        services_before: ServicesBefore(
            SERVICES
                .into_iter()
                .map(|service| {
                    (
                        service.to_string(),
                        ServiceState {
                            active: best(&["systemctl", "is-active", service]) == "active",
                            enabled: best(&["systemctl", "is-enabled", service]),
                        },
                    )
                })
                .collect(),
        ),
    })
}

pub fn task(state: &State, suffix: &str, mode: &str, properties: &[String]) -> Result<()> {
    let mut args = vec![
        "--collect".into(),
        format!("--unit={}{}", state.unit(), suffix),
    ];
    args.extend_from_slice(properties);
    args.extend([BINARY.into(), mode.into(), state.id.clone()]);
    command(
        "systemd-run",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        15,
        true,
    )?;
    Ok(())
}

pub fn arm(state: &State) -> Result<()> {
    task(
        state,
        "-recovery",
        "recover",
        &[
            format!("--on-active={STARTUP_SECONDS}s"),
            "--timer-property=AccuracySec=1s".into(),
            "--setenv=HOWFF_RECOVERY=startup".into(),
        ],
    )
}

pub fn start_worker(state: &State) -> Result<()> {
    task(
        state,
        "",
        "worker",
        &[
            "--property=RuntimeMaxSec=infinity".into(),
            "--property=TimeoutStopSec=10".into(),
            format!(
                "--property=ExecStopPost=/usr/bin/systemd-run --collect --unit={}-finalize {BINARY} recover {}",
                state.unit(),
                state.id
            ),
        ],
    )
}

pub fn disarm(state: &State) {
    best(&[
        "systemctl",
        "stop",
        &format!("{}-recovery.timer", state.unit()),
    ]);
}

pub fn quiesce(state: &State, reason: &str) {
    disarm(state);
    if reason != "worker_exit" {
        best(&["systemctl", "stop", &format!("{}.service", state.unit())]);
    }
}

pub fn validate_recovery(state: &State) -> Result<()> {
    require(
        digest(Path::new("/boot/Image"))? == crate::compat::KERNEL_SHA,
        "Recovery kernel identity changed",
    )?;
    if let Ok(owner) = fs::read_to_string("/sys/class/net/wlan0/address") {
        require(
            owner.trim() == state.self_mac,
            "Recovery belongs to another device",
        )?;
    }
    Ok(())
}

pub fn refresh_original(state: &mut State) {
    let active = best(&["nmcli", "-g", "GENERAL.CON-UUID", "device", "show", "wlan0"]);
    let profile = best(&[
        "nmcli",
        "-g",
        "GENERAL.CONNECTION",
        "device",
        "show",
        "wlan0",
    ]);
    let address = best(&["ip", "-4", "-o", "address", "show", "dev", "wlan0"]);
    if !active.is_empty()
        && active != "--"
        && profile != state.profile()
        && !address.contains("192.168.49.")
    {
        state.original_uuid = active;
    }
}

pub fn restore_connection(state: &State) -> Recovery {
    let mut errors = Vec::new();
    if !state.original_uuid.is_empty() {
        if let Err(error) = cmd(&[
            "nmcli",
            "--wait",
            "20",
            "connection",
            "up",
            "uuid",
            &state.original_uuid,
        ]) {
            errors.push(format!("original Wi-Fi: {error}"));
        }
    }
    if state.radio_before == "disabled" {
        if let Err(error) = cmd(&["nmcli", "radio", "wifi", "off"]) {
            errors.push(format!("original radio: {error}"));
        }
    }
    let active = best(&["nmcli", "-g", "GENERAL.CON-UUID", "device", "show", "wlan0"]);
    let ipv4 = best(&["ip", "-4", "-o", "address", "show", "dev", "wlan0"]);
    let restored = if state.original_uuid.is_empty() {
        !ipv4.contains("192.168.49.")
            && best(&[
                "nmcli",
                "-g",
                "GENERAL.CONNECTION",
                "device",
                "show",
                "wlan0",
            ]) != state.profile()
    } else {
        active == state.original_uuid
    };
    if !restored {
        errors.push("Original network is not restored".into());
    }
    Recovery { restored, errors }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScanRequest {
    Refresh,
    List,
}

impl ScanRequest {
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Refresh => &["device", "wifi", "rescan", "ifname", "wlan0"],
            Self::List => &[
                "-t",
                "--escape",
                "no",
                "-f",
                "SSID,BSSID,SIGNAL",
                "device",
                "wifi",
                "list",
                "ifname",
                "wlan0",
                "--rescan",
                "no",
            ],
        }
    }
}

pub(crate) fn scan(request: ScanRequest) -> Result<String> {
    command("nmcli", request.args(), 4, true)
}

pub(crate) fn scan_activity() -> Option<bool> {
    let path = best(&[
        "busctl",
        "call",
        "fi.w1.wpa_supplicant1",
        "/fi/w1/wpa_supplicant1",
        "fi.w1.wpa_supplicant1",
        "GetInterface",
        "s",
        "wlan0",
    ]);
    let interface = path.split('"').nth(1)?;
    let value = best(&[
        "busctl",
        "get-property",
        "fi.w1.wpa_supplicant1",
        interface,
        "fi.w1.wpa_supplicant1.Interface",
        "Scanning",
    ]);
    match value.as_str() {
        "b true" => Some(true),
        "b false" => Some(false),
        _ => None,
    }
}
