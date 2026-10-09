use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use ring::rand::SystemRandom;
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::protocol::hex;
use crate::system::{private_write, read, write};
use crate::{Result, require};

pub fn encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub fn decode(value: &str, size: usize) -> Result<Vec<u8>> {
    require(hex(value, size * 2), "Invalid cryptographic field")?;
    (0..value.len())
        .step_by(2)
        .map(|index| Ok(u8::from_str_radix(&value[index..index + 2], 16)?))
        .collect()
}
fn fingerprint(public: &[u8]) -> String {
    format!("{:x}", Sha256::digest(public))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicIdentity {
    pub schema: u8,
    pub device_id: String,
    pub public_key: String,
    pub name: String,
}
impl PublicIdentity {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema == 1
                && self.name.len() <= 64
                && !self.name.is_empty()
                && !self.name.chars().any(char::is_control),
            "Invalid device identity",
        )?;
        let public = decode(&self.public_key, 32)?;
        require(
            self.device_id == fingerprint(&public),
            "Device ID is not its public-key fingerprint",
        )
    }
    pub fn verify(&self, message: &[u8], signature: &str) -> Result<()> {
        self.validate()?;
        let payload = [message, serde_json::to_vec(self)?.as_slice()].concat();
        UnparsedPublicKey::new(&ED25519, decode(&self.public_key, 32)?)
            .verify(&payload, &decode(signature, 64)?)
            .map_err(|_| "Device did not prove possession of its private key".into())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Local {
    public: PublicIdentity,
    bound_mac: String,
}
pub struct Identity {
    pub public: PublicIdentity,
    key: Ed25519KeyPair,
}
impl Identity {
    pub fn sign(&self, message: &[u8]) -> String {
        let payload = [
            message,
            serde_json::to_vec(&self.public)
                .expect("Public identity serialization")
                .as_slice(),
        ]
        .concat();
        encode(self.key.sign(&payload).as_ref())
    }
}

pub fn prepare(mac: &str) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    let root = Path::new(crate::context::INSTALL);
    prepare_at(root, mac)
}
pub fn prepare_at(root: &Path, mac: &str) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    let path = root.join("identity.json");
    if path.exists() {
        let local: Local = read(&path)?;
        require(
            local.bound_mac == mac,
            "Identity was copied or moved to another device; explicitly reset identity before installation",
        )?;
        load_at(root)?;
        return Ok(None);
    }
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| "Identity key generation failed")?;
    let key = Ed25519KeyPair::from_pkcs8(document.as_ref())
        .map_err(|_| "Identity key could not be inspected")?;
    let id = fingerprint(key.public_key().as_ref());
    let local = Local {
        public: PublicIdentity {
            schema: 1,
            device_id: id.clone(),
            public_key: encode(key.public_key().as_ref()),
            name: format!("R36S-{}", &id[..6]),
        },
        bound_mac: mac.into(),
    };
    Ok(Some((
        document.as_ref().to_vec(),
        serde_json::to_vec(&local)?,
    )))
}
pub fn load() -> Result<Identity> {
    let root = Path::new(crate::context::INSTALL);
    load_at(root)
}
fn load_at(root: &Path) -> Result<Identity> {
    let path = root.join("identity.pk8");
    let info = path.metadata()?;
    require(
        info.uid() == unsafe { libc::geteuid() } && info.mode() & 0o077 == 0 && info.len() <= 1024,
        "Identity key must be privately owned",
    )?;
    let key =
        Ed25519KeyPair::from_pkcs8(&fs::read(path)?).map_err(|_| "Invalid local signing key")?;
    let local: Local = read(&root.join("identity.json"))?;
    local.public.validate()?;
    require(
        local.public.public_key == encode(key.public_key().as_ref()),
        "Local key and identity differ",
    )?;
    Ok(Identity {
        public: local.public,
        key,
    })
}
pub fn transcript(
    room: &str,
    claim: &str,
    challenge: &str,
    peer_mac: &str,
    owner_mac: &str,
) -> Result<Vec<u8>> {
    require(
        hex(room, 32)
            && hex(claim, 32)
            && hex(challenge, 64)
            && crate::protocol::mac(peer_mac)
            && crate::protocol::mac(owner_mac),
        "Invalid identity challenge scope",
    )?;
    Ok(
        format!(
            "arkos-nearby/peer-proof/v1\0{room}\0{claim}\0{challenge}\0{peer_mac}\0{owner_mac}"
        )
        .into_bytes(),
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Friend {
    pub identity: PublicIdentity,
    pub last_mac: String,
    pub wireless_key: Option<String>,
}
fn friends_path() -> PathBuf {
    Path::new(crate::context::INSTALL).join("friends.json")
}
pub fn friends() -> Result<BTreeMap<String, Friend>> {
    if !friends_path().exists() {
        return Ok(BTreeMap::new());
    }
    let saved: BTreeMap<String, Friend> = read(&friends_path())?;
    require(saved.len() <= 128, "Too many remembered devices")?;
    for (id, friend) in &saved {
        friend.identity.validate()?;
        require(
            id == &friend.identity.device_id,
            "Remembered device identity differs",
        )?;
    }
    Ok(saved)
}
pub fn known(peer: &PublicIdentity) -> Result<bool> {
    peer.validate()?;
    Ok(friends()?
        .get(&peer.device_id)
        .is_some_and(|friend| friend.identity.public_key == peer.public_key))
}
pub fn remember(peer: PublicIdentity, mac: String, key: Option<String>) -> Result<()> {
    peer.validate()?;
    require(
        peer.device_id != load()?.public.device_id,
        "Another device has the same identity; reset the cloned device",
    )?;
    let mut saved = friends()?;
    require(
        saved.contains_key(&peer.device_id) || saved.len() < 128,
        "Remembered device limit reached",
    )?;
    saved.insert(
        peer.device_id.clone(),
        Friend {
            identity: peer,
            last_mac: mac,
            wireless_key: key,
        },
    );
    write(&friends_path(), &saved)
}
pub fn forget(id: &str) -> Result<()> {
    crate::session::identity(false)?;
    require(hex(id, 64), "Choose a remembered device ID")?;
    crate::session::while_idle(|| {
        let mut saved = friends()?;
        require(saved.remove(id).is_some(), "Device is not remembered")?;
        write(&friends_path(), &saved)
    })
}
pub fn summary() -> Result<serde_json::Value> {
    Ok(serde_json::json!(friends()?.into_iter().map(|(id,friend)|serde_json::json!({"device_id":id,"name":friend.identity.name,"last_mac":friend.last_mac})).collect::<Vec<_>>()))
}
pub fn rename(name: &str) -> Result<()> {
    crate::session::identity(false)?;
    crate::session::while_idle(|| {
        let root = Path::new(crate::context::INSTALL);
        let mut local: Local = read(&root.join("identity.json"))?;
        local.public.name = name.into();
        local.public.validate()?;
        write(&root.join("identity.json"), &local)
    })
}
pub fn reset() -> Result<()> {
    require(
        unsafe { libc::geteuid() } == 0,
        "Identity reset requires root",
    )?;
    let device = crate::compat::detect().device()?;
    crate::session::while_idle(|| {
        let root = Path::new(crate::context::INSTALL);
        for name in ["identity.pk8", "identity.json"] {
            let path = root.join(name);
            if path.exists() {
                fs::rename(&path, path.with_extension("reset-backup"))?;
            }
        }
        let (key, metadata) =
            prepare(&device.mac)?.ok_or("Identity reset did not produce a new key")?;
        private_write(&root.join("identity.pk8"), &key)?;
        private_write(&root.join("identity.json"), &metadata)?;
        write(&friends_path(), &BTreeMap::<String, Friend>::new())?;
        write(&root.join("device.json"), &device)
    })
}
