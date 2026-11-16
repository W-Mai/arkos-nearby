//! Admission and filesystem boundaries for mandatory platform plugins.

use howff_platform::arkos;
use howff_platform::plugin::{PluginManifest, PluginRequirements};
use ring::signature::{ED25519, UnparsedPublicKey};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use crate::identity::decode;
use crate::{Result, require};

pub const MAX_PAYLOAD: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedPlugin {
    pub manifest: PluginManifest,
    pub payload_sha256: String,
}

pub fn requirements<'a>(target: &'a str, mirui_abi: &'a str) -> PluginRequirements<'a> {
    PluginRequirements {
        target,
        host_api: howff_platform::plugin::HOST_API,
        plugin_api: howff_platform::plugin::PLUGIN_API,
        mirui_abi,
    }
}

pub fn admit(
    manifest_bytes: &[u8],
    payload: &[u8],
    required: PluginRequirements<'_>,
    trusted_public_key: &[u8],
) -> Result<AdmittedPlugin> {
    require(
        payload.len() <= MAX_PAYLOAD,
        "Plugin payload exceeds the size limit",
    )?;
    require(trusted_public_key.len() == 32, "Invalid plugin trust key")?;
    let manifest = PluginManifest::from_json(manifest_bytes)?;
    manifest.validate(required)?;
    let digest = format!("{:x}", Sha256::digest(payload));
    require(
        digest == manifest.payload_sha256,
        "Plugin payload digest mismatch",
    )?;
    let signature = decode(&manifest.signature, 64)?;
    let signed = manifest.signing_payload(payload)?;
    UnparsedPublicKey::new(&ED25519, trusted_public_key)
        .verify(&signed, &signature)
        .map_err(|_| "Plugin signature verification failed")?;
    Ok(AdmittedPlugin {
        manifest,
        payload_sha256: digest,
    })
}

pub fn plugin_root() -> &'static Path {
    Path::new(arkos::application::PLUGINS.as_str())
}

pub fn path_inside_plugin_root(path: &Path) -> Result<PathBuf> {
    let root = plugin_root().canonicalize()?;
    let real = path.canonicalize()?;
    require(
        real.starts_with(&root),
        "Plugin path is outside the managed directory",
    )?;
    Ok(real)
}

#[cfg(test)]
mod tests {
    use super::*;
    use howff_platform::plugin::{PLUGIN_API, PLUGIN_SCHEMA, PluginKind};
    use ring::signature::{Ed25519KeyPair, KeyPair};

    use crate::identity::encode;

    fn manifest(payload: &[u8]) -> (PluginManifest, Vec<u8>) {
        let mut value = PluginManifest {
            schema: PLUGIN_SCHEMA,
            plugin_id: "arkos-platform".into(),
            kind: PluginKind::Platform,
            target: "aarch64-unknown-linux-musl".into(),
            host_api: howff_platform::plugin::HOST_API,
            plugin_api: PLUGIN_API,
            mirui_abi: "mirui-0.56.0".into(),
            capabilities: vec!["paths".into(), "launch".into()],
            payload_sha256: format!("{:x}", Sha256::digest(payload)),
            signature: String::new(),
        };
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        value.signature = encode(key.sign(&value.signing_payload(payload).unwrap()).as_ref());
        (value, key.public_key().as_ref().to_vec())
    }

    #[test]
    fn admits_signed_payload_with_matching_abi() {
        let payload = b"platform plugin";
        let (value, key) = manifest(payload);
        let bytes = serde_json::to_vec(&value).unwrap();
        let admitted = admit(
            &bytes,
            payload,
            requirements("aarch64-unknown-linux-musl", "mirui-0.56.0"),
            &key,
        )
        .unwrap();
        assert_eq!(admitted.manifest.plugin_id, "arkos-platform");
        assert_eq!(admitted.payload_sha256, value.payload_sha256);
    }

    #[test]
    fn rejects_tampered_payload_and_wrong_key() {
        let payload = b"platform plugin";
        let (value, key) = manifest(payload);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(
            admit(
                &bytes,
                b"tampered",
                requirements("aarch64-unknown-linux-musl", "mirui-0.56.0"),
                &key,
            )
            .is_err()
        );
        let wrong = [8; 32];
        assert!(
            admit(
                &bytes,
                payload,
                requirements("aarch64-unknown-linux-musl", "mirui-0.56.0"),
                &wrong,
            )
            .is_err()
        );
    }
}
