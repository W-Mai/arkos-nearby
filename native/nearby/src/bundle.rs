use std::collections::BTreeSet;
use std::io::Read;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compat::{KERNEL_SHA, PROFILE};
use crate::protocol::hex;
use crate::{Result, require};

#[cfg(feature = "release-bundle")]
const EMBEDDED: &[u8] = include_bytes!(env!("ARKOS_NEARBY_BUNDLE"));
#[cfg(not(feature = "release-bundle"))]
const EMBEDDED: &[u8] = &[];

pub const ALLOWED: &[&str] = &[
    "arkos-nearby-gui",
    "core-inspect32",
    "core-inspect64",
    "assets/8188eu.ko",
    "assets/wpa_supplicant",
    "assets/build.json",
    "assets/netplay-compression.so",
    "assets/netplay-compression-build.json",
    "core-inspect-build.json",
    "nearby-gui-build.json",
    "font/OFL.txt",
    "font/font-source.json",
    "licenses/NOTICE.txt",
    "licenses/wpa-COPYING.txt",
    "licenses/driver-COPYING.txt",
    "release-sources.json",
    "licenses/ring-LICENSE.txt",
    "compat/arm64/libstdc++.so.6",
    "compat/arm64/libgcc_s.so.1",
    "compat/arm64/source.json",
    "licenses/gcc-runtime-COPYING.txt",
    "licenses/netplay-compression-COPYING.txt",
    "cores/nestopia_nearby_libretro.so",
    "cores/owned-core-build.json",
    "licenses/nestopia-COPYING.txt",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub name: String,
    pub sha256: String,
    pub size: u64,
    pub offset: u64,
    pub mode: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u8,
    pub version: String,
    pub profile: String,
    pub kernel_sha256: String,
    pub gui_version: String,
    pub files: Vec<Asset>,
}
pub struct Bundle {
    pub manifest: Manifest,
    data: Vec<u8>,
    start: usize,
}

impl Bundle {
    pub fn embedded() -> Result<Self> {
        require(
            !EMBEDDED.is_empty(),
            "This development build has no embedded release assets",
        )?;
        Self::decode(EMBEDDED)
    }
    pub fn decode(packed: &[u8]) -> Result<Self> {
        require(
            packed.len() <= 32 * 1024 * 1024,
            "Release bundle is oversized",
        )?;
        let mut data = Vec::new();
        flate2::read::GzDecoder::new(packed)
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut data)?;
        require(
            data.len() >= 12 && data.len() <= 32 * 1024 * 1024 && data[..8] == *b"ARKNP001",
            "Invalid release bundle",
        )?;
        let length = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        require(
            length <= 65536 && 12 + length <= data.len(),
            "Invalid release manifest length",
        )?;
        let manifest: Manifest = serde_json::from_slice(&data[12..12 + length])?;
        require(
            manifest.schema == 1
                && manifest.profile == PROFILE
                && manifest.kernel_sha256 == KERNEL_SHA
                && manifest.gui_version == "0.47.0"
                && !manifest.version.is_empty(),
            "Unsupported release profile",
        )?;
        let start = 12 + length;
        let mut names = BTreeSet::new();
        let mut offset = 0;
        for asset in &manifest.files {
            require(
                ALLOWED.contains(&asset.name.as_str())
                    && names.insert(asset.name.as_str())
                    && hex(&asset.sha256, 64)
                    && matches!(asset.mode, 0o644 | 0o755)
                    && asset.size > 0
                    && asset.offset == offset,
                "Invalid or duplicate release asset",
            )?;
            offset = offset
                .checked_add(asset.size)
                .ok_or("Release asset overflow")?;
            require(
                offset <= data.len().saturating_sub(start) as u64,
                "Release asset is out of bounds",
            )?;
            let bytes = &data[start + asset.offset as usize..start + offset as usize];
            require(
                format!("{:x}", Sha256::digest(bytes)) == asset.sha256,
                "Embedded asset checksum differs",
            )?;
        }
        require(
            offset == data.len().saturating_sub(start) as u64 && names.len() == ALLOWED.len(),
            "Release asset set is incomplete",
        )?;
        Ok(Self {
            manifest,
            data,
            start,
        })
    }
    pub fn bytes(&self, asset: &Asset) -> &[u8] {
        &self.data
            [self.start + asset.offset as usize..self.start + (asset.offset + asset.size) as usize]
    }
}
