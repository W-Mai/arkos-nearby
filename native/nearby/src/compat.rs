use std::fs;
#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::content::digest;
use crate::protocol::mac;
use crate::{Result, require};

pub const PROFILE: &str = "arkos4clone-4.4.189-rtl8188eu";
pub const KERNEL_SHA: &str = "73a123c06c67c002605bef637ef458aa43448a501c48ce27756f16e846d57f7a";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub schema: u8,
    pub mac: String,
    pub profile: String,
    pub kernel_sha256: String,
}

pub fn valid_mac(value: &str) -> bool {
    mac(value)
        && value != "00:00:00:00:00:00"
        && u8::from_str_radix(&value[..2], 16).is_ok_and(|byte| byte & 1 == 0)
}

impl Device {
    pub fn validate(&self, actual_mac: &str) -> Result<()> {
        require(
            self.schema == 1
                && self.profile == PROFILE
                && self.kernel_sha256 == KERNEL_SHA
                && self.mac == actual_mac
                && valid_mac(actual_mac),
            "Installation belongs to another device or firmware; run the installer on this handheld",
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Panel {
    pub width: u32,
    pub height: u32,
    pub bits: u32,
    pub red: u32,
    pub blue: u32,
    pub rotation: u32,
}
impl Panel {
    pub fn supported(&self) -> bool {
        self.width == 640
            && self.height == 480
            && self.rotation == 0
            && match self.bits {
                32 => (self.red == 16 && self.blue == 0) || (self.red == 0 && self.blue == 16),
                16 => self.red == 11 && self.blue == 0,
                _ => false,
            }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Facts {
    pub schema: u8,
    pub architecture: String,
    pub platform: String,
    pub mac: String,
    pub kernel_sha256: String,
    pub usb_id: String,
    pub panel: Option<Panel>,
    pub dependencies: Vec<Check>,
}
impl Facts {
    pub fn problems(&self) -> Vec<String> {
        let mut result = Vec::new();
        for (ok, message) in [
            (
                self.architecture == "aarch64" && self.platform == "linux",
                "Requires Linux ARM64",
            ),
            (valid_mac(&self.mac), "No supported local wlan0 address"),
            (
                self.kernel_sha256 == KERNEL_SHA,
                "No driver profile for this kernel",
            ),
            (
                matches!(self.usb_id.as_str(), "0bda:8179" | "0bda:0179"),
                "Requires the inspected RTL8188EU adapter",
            ),
            (
                self.panel.as_ref().is_some_and(Panel::supported),
                "Requires a supported 640x480 framebuffer",
            ),
        ] {
            if !ok {
                result.push(message.into());
            }
        }
        result.extend(
            self.dependencies
                .iter()
                .filter(|check| !check.ok)
                .map(|check| format!("{}: {}", check.name, check.detail)),
        );
        result
    }
    pub fn device(&self) -> Result<Device> {
        require(self.problems().is_empty(), &self.problems().join("; "))?;
        Ok(Device {
            schema: 1,
            mac: self.mac.clone(),
            profile: PROFILE.into(),
            kernel_sha256: self.kernel_sha256.clone(),
        })
    }
}

fn panel() -> Option<Panel> {
    #[cfg(target_os = "linux")]
    {
        let file = File::open("/dev/fb0").ok()?;
        let mut fields = [0u32; 40];
        // Linux fb_var_screeninfo consists of forty u32 fields, including bitfields.
        if unsafe { libc::ioctl(file.as_raw_fd(), 0x4600, fields.as_mut_ptr()) } < 0 {
            return None;
        }
        Some(Panel {
            width: fields[0],
            height: fields[1],
            bits: fields[6],
            red: fields[8],
            blue: fields[14],
            rotation: fields[34],
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn usb_id() -> String {
    let alias = fs::read_to_string("/sys/class/net/wlan0/device/modalias")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if let Some(value) = alias.strip_prefix("usb:v") {
        if value.len() >= 9 && value.as_bytes()[4] == b'p' {
            return format!("{}:{}", &value[..4], &value[5..9]);
        }
    }
    if let Ok(mut path) = Path::new("/sys/class/net/wlan0/device").canonicalize() {
        for _ in 0..4 {
            if let (Ok(vendor), Ok(product)) = (
                fs::read_to_string(path.join("idVendor")),
                fs::read_to_string(path.join("idProduct")),
            ) {
                return format!("{}:{}", vendor.trim(), product.trim()).to_ascii_lowercase();
            }
            if !path.pop() {
                break;
            }
        }
    }
    String::new()
}

pub fn detect() -> Facts {
    let mut checks = Vec::new();
    for program in [
        "systemd-run",
        "systemctl",
        "nmcli",
        "iw",
        "ip",
        "modprobe",
        "insmod",
        "mount",
        "umount",
        "findmnt",
        "getent",
        "pgrep",
        "sudo",
        "dialog",
    ] {
        let available = std::env::var_os("PATH").is_some_and(|paths| {
            std::env::split_paths(&paths).any(|path| path.join(program).is_file())
        });
        checks.push(Check {
            name: program.into(),
            ok: available,
            detail: if available {
                "available"
            } else {
                "missing system command"
            }
            .into(),
        });
    }
    for file in [
        "/usr/sbin/dnsmasq",
        "/usr/sbin/dhclient",
        "/opt/inttools/gptokeyb",
        "/opt/inttools/gamecontrollerdb.txt",
        "/etc/systemd/system/emulationstation.service",
        "/opt/retroarch/bin/retroarch",
        "/lib/ld-linux-aarch64.so.1",
        "/lib/ld-linux-armhf.so.3",
        "/usr/lib/aarch64-linux-gnu/libnl-3.so.200",
        "/usr/lib/aarch64-linux-gnu/libnl-genl-3.so.200",
        "/usr/lib/aarch64-linux-gnu/libcrypto.so.1.1",
        "/usr/lib/aarch64-linux-gnu/libssl.so.1.1",
    ] {
        checks.push(Check {
            name: file.into(),
            ok: Path::new(file).is_file(),
            detail: "required existing runtime component".into(),
        });
    }
    checks.push(Check {
        name: "ark account".into(),
        ok: fs::read_to_string("/etc/passwd")
            .is_ok_and(|text| text.lines().any(|line| line.starts_with("ark:"))),
        detail: "native game account".into(),
    });
    Facts {
        schema: 1,
        architecture: std::env::consts::ARCH.into(),
        platform: std::env::consts::OS.into(),
        mac: fs::read_to_string("/sys/class/net/wlan0/address")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase(),
        kernel_sha256: digest(Path::new("/boot/Image")).unwrap_or_default(),
        usb_id: usb_id(),
        panel: panel(),
        dependencies: checks,
    }
}

pub fn identity(force: bool) -> Result<String> {
    require(
        unsafe { libc::geteuid() } == 0,
        "Nearby actions require the installed root wrapper",
    )?;
    let owner = fs::read_to_string("/sys/class/net/wlan0/address")?
        .trim()
        .to_ascii_lowercase();
    require(valid_mac(&owner), "No valid local wireless address")?;
    let path = Path::new(crate::context::INSTALL).join("device.json");
    if path.is_file() {
        let info = path.metadata()?;
        require(
            info.uid() == 0 && info.mode() & 0o077 == 0,
            "Device identity record must be root-only",
        )?;
        let device: Device = crate::system::read(&path)?;
        device.validate(&owner)?;
    }
    if force {
        require(
            digest(Path::new("/boot/Image"))? == KERNEL_SHA,
            "No tested wireless driver for the installed kernel",
        )?;
    }
    Ok(owner)
}
