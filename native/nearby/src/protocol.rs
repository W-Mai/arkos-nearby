use serde::{Deserialize, Serialize};

use crate::{Result, require};

pub const PORT: u16 = 55435;
pub const ADDRESS: &str = "192.168.49.1";
pub const MAX_CONTENT: u64 = 512 * 1024 * 1024;
pub const MAX_DESCRIPTOR: usize = 16384;

pub fn hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn mac(value: &str) -> bool {
    let parts: Vec<_> = value.split(':').collect();
    parts.len() == 6 && parts.iter().all(|part| hex(part, 2))
}

fn text(value: &str, size: usize) -> bool {
    !value.is_empty() && value.len() <= size && value.chars().all(|c| c >= ' ')
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CoreIdentity {
    pub id: String,
    pub name: String,
    pub version: String,
}

impl CoreIdentity {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.id.is_empty()
                && self.id.len() <= 128
                && self
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "Invalid core identifier",
        )?;
        require(
            text(&self.name, 31) && text(&self.version, 31),
            "Invalid native core identity",
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContentIdentity {
    pub title: String,
    pub sha256: String,
    pub size: u64,
    pub crc32: u32,
}

impl ContentIdentity {
    pub fn validate(&self) -> Result<()> {
        require(
            text(&self.title, 256)
                && hex(&self.sha256, 64)
                && self.size > 0
                && self.size <= MAX_CONTENT,
            "Invalid local content identity",
        )
    }
    pub fn same_bytes(&self, other: &Self) -> bool {
        self.sha256 == other.sha256 && self.size == other.size && self.crc32 == other.crc32
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GameIdentity {
    pub frontend_version: String,
    pub core: CoreIdentity,
    pub content: ContentIdentity,
}

impl GameIdentity {
    pub fn validate(&self) -> Result<()> {
        require(text(&self.frontend_version, 64), "Invalid frontend version")?;
        self.core.validate()?;
        self.content.validate()
    }
    pub fn compatible(&self, other: &Self) -> bool {
        self.frontend_version == other.frontend_version
            && self.core == other.core
            && self.content.same_bytes(&other.content)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Room {
    pub schema: u8,
    pub session_id: String,
    pub host_mac: String,
    pub host_ipv4: String,
    pub netplay_port: u16,
    pub frontend_version: String,
    pub core: CoreIdentity,
    pub content: ContentIdentity,
}

impl Room {
    pub fn new(id: String, owner: String, game: GameIdentity) -> Result<Self> {
        let room = Self {
            schema: 1,
            session_id: id,
            host_mac: owner,
            host_ipv4: ADDRESS.into(),
            netplay_port: PORT,
            frontend_version: game.frontend_version,
            core: game.core,
            content: game.content,
        };
        room.validate()?;
        Ok(room)
    }
    pub fn game(&self) -> GameIdentity {
        GameIdentity {
            frontend_version: self.frontend_version.clone(),
            core: self.core.clone(),
            content: self.content.clone(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema == 1
                && hex(&self.session_id, 32)
                && mac(&self.host_mac)
                && self.host_ipv4 == ADDRESS
                && self.netplay_port == PORT,
            "Invalid room endpoint or identity",
        )?;
        self.game().validate()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        require(bytes.len() <= MAX_DESCRIPTOR, "Oversized room descriptor")?;
        let room: Self = serde_json::from_slice(bytes)?;
        room.validate()?;
        Ok(room)
    }
}

#[derive(Clone, Debug)]
pub struct VerifiedRoom(Room);

impl VerifiedRoom {
    pub fn observe(room: Room, selected_mac: &str, token: &str) -> Result<Self> {
        room.validate()?;
        require(
            room.host_mac == selected_mac && hex(token, 13) && room.session_id.starts_with(token),
            "Selected radio does not own this room",
        )?;
        Ok(Self(room))
    }
    pub fn room(&self) -> &Room {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub session_id: String,
    pub frontend: String,
    pub frontend_sha256: String,
    pub core_id: String,
    pub core_sha256: String,
    pub elf_bits: u8,
}

impl Profile {
    pub fn validate(&self, room: &VerifiedRoom) -> Result<()> {
        require(
            self.session_id == room.room().session_id
                && self.core_id == room.room().core.id
                && matches!(self.frontend.as_str(), "retroarch" | "retroarch32")
                && matches!(self.elf_bits, 32 | 64)
                && hex(&self.frontend_sha256, 64)
                && hex(&self.core_sha256, 64),
            "Room profile does not match the observed room",
        )
    }
}
