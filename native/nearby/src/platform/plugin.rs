//! Host-side admission and typed snapshot loading for the mandatory platform plugin.

use howff_platform::backend::PlatformSnapshot;
use howff_platform::ipc::{PluginRequest, PluginResponse};
use howff_platform::plugin::{PluginManifest, PluginRequirements};
use serde_json::json;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
use std::{os::unix::fs::PermissionsExt, path::PathBuf};

use crate::{Result, require};

/// Public key pinned in the host binary for ArkOS platform plugin admission.
#[allow(dead_code)]
pub const TRUSTED_PLATFORM_PUBLIC_KEY: &[u8; 32] = &[
    0x9b, 0xdc, 0x91, 0x62, 0x46, 0x6a, 0xae, 0x79, 0x53, 0xb4, 0xb4, 0xc6, 0x45, 0xb9, 0x44, 0x25,
    0x54, 0x7d, 0x93, 0x1f, 0xf6, 0xfb, 0x45, 0xc8, 0x8a, 0x68, 0xf0, 0xd3, 0x9a, 0x60, 0x08, 0xfe,
];

pub struct PlatformClient {
    pub manifest: PluginManifest,
    pub snapshot: PlatformSnapshot,
}

#[allow(dead_code)]
pub fn load(
    executable: &Path,
    args: &[&str],
    manifest_bytes: &[u8],
    payload: &[u8],
    required: PluginRequirements<'_>,
    trusted_public_key: &[u8],
    session_id: String,
) -> Result<PlatformClient> {
    let admitted = crate::plugin::admit(manifest_bytes, payload, required, trusted_public_key)?;
    require(
        admitted.manifest.kind == howff_platform::plugin::PluginKind::Platform,
        "Platform plugin manifest has the wrong kind",
    )?;
    let request = PluginRequest::new(1, session_id, "paths".into(), json!({}))?;
    let response = crate::plugin_ipc::invoke(executable, args, request, Duration::from_secs(2))?;
    let snapshot = decode_snapshot(response, &admitted.manifest.plugin_id, required.target)?;
    Ok(PlatformClient {
        manifest: admitted.manifest,
        snapshot,
    })
}

/// Load the installed ArkOS platform plugin and cache its immutable snapshot.
///
/// The ARM64 runtime has no static platform fallback: a missing, unsigned, or
/// invalid plugin is an admission error. Host builds retain a typed fixture so
/// unit tests can exercise the same capability boundary without `/opt/howff`.
pub fn require_capability(capability: &str) -> Result<PlatformSnapshot> {
    require(identifier(capability), "Invalid platform capability")?;
    let client = installed_client()?;
    require(
        client
            .manifest
            .capabilities
            .iter()
            .any(|candidate| candidate == capability),
        "Platform plugin does not provide the requested capability",
    )?;
    require(
        client
            .snapshot
            .capabilities
            .iter()
            .any(|candidate| candidate == capability),
        "Platform snapshot does not provide the requested capability",
    )?;
    Ok(client.snapshot.clone())
}

fn installed_client() -> Result<&'static PlatformClient> {
    static CLIENT: OnceLock<std::result::Result<PlatformClient, String>> = OnceLock::new();
    let result = CLIENT.get_or_init(|| load_installed().map_err(|error| error.to_string()));
    match result {
        Ok(client) => Ok(client),
        Err(error) => Err(error.clone().into()),
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn load_installed() -> Result<PlatformClient> {
    use howff_platform::arkos::application;

    let executable = confined_file(application::PLATFORM_PLUGIN_EXECUTABLE.path())?;
    let manifest = confined_file(application::PLATFORM_PLUGIN_MANIFEST.path())?;
    let trust_key = confined_file(application::PLATFORM_PLUGIN_TRUST_KEY.path())?;
    require(
        std::fs::read(trust_key)?.as_slice() == TRUSTED_PLATFORM_PUBLIC_KEY,
        "Platform plugin trust key does not match the host admission key",
    )?;
    let payload = std::fs::read(&executable)?;
    load(
        &executable,
        &[],
        &std::fs::read(manifest)?,
        &payload,
        crate::plugin::requirements("aarch64-unknown-linux-musl", "mirui-0.56.0"),
        TRUSTED_PLATFORM_PUBLIC_KEY,
        crate::system::random_id()?,
    )
}

#[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
fn load_installed() -> Result<PlatformClient> {
    // The host-side fixture keeps native unit tests independent from the
    // device filesystem. It is never compiled into the ARM64 runtime.
    let snapshot = PlatformSnapshot::arkos("arkos-platform", "aarch64-unknown-linux-musl");
    Ok(PlatformClient {
        manifest: PluginManifest {
            schema: howff_platform::plugin::PLUGIN_SCHEMA,
            plugin_id: "arkos-platform".into(),
            kind: howff_platform::plugin::PluginKind::Platform,
            target: "aarch64-unknown-linux-musl".into(),
            host_api: howff_platform::plugin::HOST_API,
            plugin_api: howff_platform::plugin::PLUGIN_API,
            mirui_abi: "mirui-0.56.0".into(),
            capabilities: snapshot.capabilities.clone(),
            payload_sha256: "0".repeat(64),
            signature: "host-fixture".into(),
        },
        snapshot,
    })
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn confined_file(path: &Path) -> Result<PathBuf> {
    let root = crate::plugin::plugin_root().canonicalize()?;
    let root_metadata = std::fs::symlink_metadata(&root)?;
    require(
        root_metadata.is_dir() && root_metadata.permissions().mode() & 0o022 == 0,
        "Managed plugin directory is writable by an untrusted group",
    )?;
    let real = crate::plugin::path_inside_plugin_root(path)?;
    let metadata = std::fs::symlink_metadata(&real)?;
    require(
        metadata.is_file() && metadata.permissions().mode() & 0o022 == 0,
        "Platform plugin asset is not a regular file",
    )?;
    Ok(real)
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

#[allow(dead_code)]
pub fn decode_snapshot(
    response: PluginResponse,
    expected_plugin: &str,
    expected_target: &str,
) -> Result<PlatformSnapshot> {
    require(
        response.ok,
        response
            .error
            .as_deref()
            .unwrap_or("Platform plugin failed"),
    )?;
    let body = response
        .body
        .ok_or("Platform plugin returned no snapshot")?;
    let snapshot: PlatformSnapshot = serde_json::from_value(body)?;
    require(
        snapshot.validate(expected_plugin, expected_target),
        "Platform plugin returned invalid facts",
    )?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use howff_platform::backend::PlatformSnapshot;
    use howff_platform::ipc::FRAME_SCHEMA;

    #[test]
    fn accepts_a_valid_typed_snapshot() {
        let response = PluginResponse {
            schema: FRAME_SCHEMA,
            request_id: 1,
            ok: true,
            body: Some(
                serde_json::to_value(PlatformSnapshot::arkos(
                    "arkos-platform",
                    "aarch64-unknown-linux-musl",
                ))
                .unwrap(),
            ),
            error: None,
        };
        let snapshot =
            decode_snapshot(response, "arkos-platform", "aarch64-unknown-linux-musl").unwrap();
        assert_eq!(snapshot.services.len(), 2);
        assert_eq!(snapshot.recipes.events, ["game-start", "game-end"]);
    }

    #[test]
    fn rejects_an_invalid_plugin_response() {
        let mut snapshot = PlatformSnapshot::arkos("arkos-platform", "aarch64-unknown-linux-musl");
        snapshot.rom_roots.clear();
        let response = PluginResponse {
            schema: FRAME_SCHEMA,
            request_id: 1,
            ok: true,
            body: Some(serde_json::to_value(snapshot).unwrap()),
            error: None,
        };
        assert!(decode_snapshot(response, "arkos-platform", "aarch64-unknown-linux-musl").is_err());
    }

    #[test]
    fn capability_guard_uses_the_admitted_snapshot() {
        let snapshot = require_capability("paths").unwrap();
        assert!(snapshot.capabilities.iter().any(|value| value == "paths"));
        assert!(require_capability("missing").is_err());
    }
}
