use std::collections::BTreeSet;
use std::io::Read;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compat::{KERNEL_SHA, PROFILE};
use crate::protocol::hex;
use crate::{Result, require};

#[cfg(feature = "release-bundle")]
const EMBEDDED: &[u8] = include_bytes!(env!("HOWFF_BUNDLE"));
#[cfg(not(feature = "release-bundle"))]
const EMBEDDED: &[u8] = &[];

const MAX_BUNDLE_BYTES: u64 = 64 * 1024 * 1024;

pub const ALLOWED: &[&str] = &[
    "howff-gui",
    "core-inspect32",
    "core-inspect64",
    "core-probe64",
    "core-probe-build.json",
    "assets/8188eu.ko",
    "assets/wpa_supplicant",
    "assets/build.json",
    "assets/netplay-compression.so",
    "assets/netplay-compression-build.json",
    "assets/home-input.so",
    "assets/home-input-build.json",
    "core-inspect-build.json",
    "howff-gui-build.json",
    "font/OFL.txt",
    "font/font-source.json",
    "licenses/NOTICE.txt",
    "licenses/wpa-COPYING.txt",
    "licenses/driver-COPYING.txt",
    "release-sources.json",
    "licenses/ring-LICENSE.txt",
    "licenses/pinyin-LICENSE.txt",
    "compat/arm64/libstdc++.so.6",
    "compat/arm64/libgcc_s.so.1",
    "compat/arm64/source.json",
    "licenses/gcc-runtime-COPYING.txt",
    "licenses/netplay-compression-COPYING.txt",
    "licenses/core-probe-ABI-LICENSE.txt",
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
    pub product: String,
    pub schema: u8,
    pub version: String,
    pub profile: String,
    pub kernel_sha256: String,
    pub gui_version: String,
    pub files: Vec<Asset>,
}
impl Manifest {
    pub(crate) fn validate_profile(&self) -> Result<()> {
        require(
            self.product == "howff"
                && self.schema == 1
                && self.profile == PROFILE
                && self.kernel_sha256 == KERNEL_SHA
                && !self.version.is_empty()
                && self.version.len() <= 64,
            "Unsupported Howff release profile",
        )
    }
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
            packed.len() as u64 <= MAX_BUNDLE_BYTES,
            "Release bundle is oversized",
        )?;
        let mut data = Vec::new();
        flate2::read::GzDecoder::new(packed)
            .take(MAX_BUNDLE_BYTES + 1)
            .read_to_end(&mut data)?;
        require(
            data.len() >= 12 && data.len() as u64 <= MAX_BUNDLE_BYTES && data[..8] == *b"HOWFF001",
            "Invalid release bundle",
        )?;
        let length = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        require(
            length <= 65536 && 12 + length <= data.len(),
            "Invalid release manifest length",
        )?;
        let manifest: Manifest = serde_json::from_slice(&data[12..12 + length])?;
        manifest.validate_profile()?;
        require(
            manifest.gui_version == "0.56.0",
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

#[cfg(all(test, feature = "release-bundle"))]
mod tests {
    use super::*;

    #[test]
    fn embedded_bundle_requires_explicit_howff_identity() {
        use std::io::Write;
        let bundle = Bundle::embedded().unwrap();
        for product in [None, Some("external")] {
            let mut manifest = serde_json::to_value(&bundle.manifest).unwrap();
            if let Some(product) = product {
                manifest["product"] = product.into();
            } else {
                manifest.as_object_mut().unwrap().remove("product");
            }
            let header = serde_json::to_vec(&manifest).unwrap();
            let mut bytes = b"HOWFF001".to_vec();
            bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&header);
            bytes.extend_from_slice(&bundle.data[bundle.start..]);
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(&bytes).unwrap();
            assert!(Bundle::decode(&encoder.finish().unwrap()).is_err());
        }
    }

    #[test]
    fn embedded_renderer_matches_its_manifest_and_build_record() {
        let bundle = Bundle::embedded().expect("The built release must decode");
        let asset = |name: &str| {
            bundle
                .manifest
                .files
                .iter()
                .find(|asset| asset.name == name)
                .expect("The release contains its renderer and build record")
        };
        let report: serde_json::Value =
            serde_json::from_slice(bundle.bytes(asset("howff-gui-build.json"))).unwrap();
        let renderer = asset("howff-gui");
        assert_eq!(report["mirui"], bundle.manifest.gui_version);
        let target = match report["backend"].as_str() {
            Some("sdl-gpu") => "aarch64-unknown-linux-gnu",
            None => "aarch64-unknown-linux-musl",
            Some(backend) => panic!("Unexpected renderer backend: {backend}"),
        };
        assert_eq!(report["target"], target);
        assert_eq!(report["sha256"], renderer.sha256);
        assert_eq!(report["bytes"], renderer.size);
        assert_eq!(
            report["cargo_lock_sha256"],
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../../gui/nearby/Cargo.lock"))
            )
        );
        let lock = include_str!("../../../gui/nearby/Cargo.lock");
        for (name, field) in [
            ("mirui", "mirui_crate_sha256"),
            ("mirui-macros", "mirui_macros_crate_sha256"),
            ("mirx", "mirx_crate_sha256"),
        ] {
            let package = lock
                .split("[[package]]")
                .find(|package| {
                    package
                        .lines()
                        .any(|line| line == format!("name = \"{name}\""))
                })
                .unwrap();
            let checksum = package
                .lines()
                .find_map(|line| line.strip_prefix("checksum = \""))
                .unwrap()
                .trim_end_matches('"');
            assert_eq!(report[field], checksum);
        }
        let sources: serde_json::Value =
            serde_json::from_slice(bundle.bytes(asset("release-sources.json"))).unwrap();
        for (field, report_field) in [
            ("version", "mirui"),
            ("mirx_version", "mirx"),
            ("sha256", "sha256"),
            ("source_sha256", "source_sha256"),
            ("cargo_lock_sha256", "cargo_lock_sha256"),
            ("mirui_crate_sha256", "mirui_crate_sha256"),
            ("mirui_macros_crate_sha256", "mirui_macros_crate_sha256"),
            ("mirx_crate_sha256", "mirx_crate_sha256"),
        ] {
            assert!(report[report_field].as_str().is_some());
            assert_eq!(sources["renderer"][field], report[report_field]);
        }
    }
}
