use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::{GameIdentity, Room, hex, mac};
use crate::{Result, require};

#[derive(Clone, Debug)]
pub struct ObservedPeer {
    ipv4: String,
    mac: String,
}

impl ObservedPeer {
    pub fn mac(&self) -> &str {
        &self.mac
    }
    pub fn resolve(ipv4: &str, leases: &str, stations: &str, wall: f64) -> Result<Self> {
        let address: Ipv4Addr = ipv4.parse()?;
        let bytes = address.octets();
        require(
            bytes[..3] == [192, 168, 49] && bytes[3] > 1 && bytes[3] < 255,
            "Client is outside the room subnet",
        )?;
        let stations: BTreeSet<_> = stations
            .lines()
            .filter_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("station ")
                    .and_then(|line| line.split_whitespace().next())
                    .map(str::to_owned)
            })
            .filter(|v| mac(v))
            .collect();
        let mut matches = BTreeSet::new();
        for line in leases.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() >= 3
                && fields[2] == ipv4
                && stations.contains(&fields[1].to_ascii_lowercase())
                && fields[0]
                    .parse::<u64>()
                    .is_ok_and(|expiry| expiry == 0 || expiry as f64 > wall)
            {
                matches.insert(fields[1].to_ascii_lowercase());
            }
        }
        require(
            matches.len() == 1,
            "Client lease does not match a live associated station",
        )?;
        Ok(Self {
            ipv4: ipv4.into(),
            mac: matches.into_iter().next().unwrap(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadyPeer {
    pub session_id: String,
    pub peer_ipv4: String,
    pub peer_mac: String,
    pub core_id: String,
    pub content_sha256: String,
    pub claim_id: Option<String>,
    pub expires_monotonic: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadyClaim {
    session_id: String,
    game: GameIdentity,
    #[serde(default)]
    claim_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    session_id: String,
    claim_id: String,
}

pub struct Controller {
    room: Room,
    pub peer: Option<ReadyPeer>,
    retired: BTreeSet<(String, Option<String>)>,
}
impl Controller {
    pub fn new(room: Room) -> Self {
        Self {
            room,
            peer: None,
            retired: BTreeSet::new(),
        }
    }
    pub fn expire(&mut self, now: f64) {
        if self
            .peer
            .as_ref()
            .is_some_and(|peer| peer.expires_monotonic <= now)
        {
            self.peer = None;
        }
    }
    pub fn refresh(&mut self, leases: &str, stations: &str, now: f64, wall: f64) {
        self.expire(now);
        if self.peer.as_ref().is_some_and(|peer| {
            ObservedPeer::resolve(&peer.peer_ipv4, leases, stations, wall).is_err()
                || ObservedPeer::resolve(&peer.peer_ipv4, leases, stations, wall)
                    .is_ok_and(|observed| observed.mac() != peer.peer_mac)
                || !stations
                    .to_ascii_lowercase()
                    .contains(&format!("station {} ", peer.peer_mac))
        }) {
            self.peer = None;
        }
    }
    pub fn accept(&mut self, bytes: &[u8], observed: &ObservedPeer, now: f64) -> Result<Value> {
        self.expire(now);
        let claim: ReadyClaim = serde_json::from_slice(bytes)?;
        require(
            claim.session_id == self.room.session_id
                && claim.claim_id.as_ref().is_none_or(|id| hex(id, 32)),
            "Ready claim belongs to another room",
        )?;
        claim.game.validate()?;
        require(
            self.peer
                .as_ref()
                .is_none_or(|peer| peer.peer_mac == observed.mac),
            "Another station owns readiness",
        )?;
        require(
            self.retired.len() < 256
                && !self
                    .retired
                    .contains(&(observed.mac.clone(), claim.claim_id.clone())),
            "Readiness claim was retired",
        )?;
        let compatible = self.room.game().compatible(&claim.game);
        if compatible {
            self.peer = Some(ReadyPeer {
                session_id: self.room.session_id.clone(),
                peer_ipv4: observed.ipv4.clone(),
                peer_mac: observed.mac.clone(),
                core_id: claim.game.core.id,
                content_sha256: claim.game.content.sha256,
                claim_id: claim.claim_id.clone(),
                expires_monotonic: now + 30.0,
            });
        } else if self
            .peer
            .as_ref()
            .is_some_and(|peer| peer.peer_mac == observed.mac)
        {
            self.peer = None;
        }
        let mut response = json!({"session_id":self.room.session_id,"compatible":compatible,"mismatches":if compatible { vec![] } else { vec!["game_identity"] }});
        if let Some(id) = claim.claim_id {
            response["claim_id"] = id.into();
            response["lease_seconds"] = 30.into();
        }
        Ok(response)
    }
    pub fn release(&mut self, bytes: &[u8], observed: &ObservedPeer) -> Result<Value> {
        let claim: Release = serde_json::from_slice(bytes)?;
        require(
            claim.session_id == self.room.session_id && hex(&claim.claim_id, 32),
            "Release belongs to another room",
        )?;
        let key = (observed.mac.clone(), Some(claim.claim_id.clone()));
        require(
            self.retired.len() < 256 || self.retired.contains(&key),
            "Readiness retirement limit reached",
        )?;
        self.retired.insert(key);
        if self.peer.as_ref().is_some_and(|peer| {
            peer.peer_mac == observed.mac && peer.claim_id.as_ref() == Some(&claim.claim_id)
        }) {
            self.peer = None;
        }
        Ok(json!({"session_id":self.room.session_id,"claim_id":claim.claim_id,"revoked":true}))
    }
    pub fn status(&mut self, now: f64) -> Value {
        self.expire(now);
        json!({"session_id":self.room.session_id,"phase":if self.peer.is_some() { "ready" } else { "waiting" },"peer":self.peer})
    }
}
