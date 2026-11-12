use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::identity::{Identity, PublicIdentity};
use crate::protocol::{ADDRESS, hex};
use crate::purpose::{NetworkRoom, SessionPurpose};
use crate::readiness::ObservedPeer;
use crate::session::{self, State, best, current, progress, update};
use crate::system::{monotonic, random_id, read, wall_time, write};
use crate::{Result, require};

const LEASE: f64 = 15.0;
const HEARTBEAT: f64 = 2.0;
const DOMAIN: &[u8] = b"howff/network/v1\0";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Peer {
    identity: PublicIdentity,
    mac: String,
    ipv4: String,
    claim: String,
    sequence: u64,
    acknowledged: bool,
    expires: f64,
}

#[derive(Default)]
struct Membership {
    peers: BTreeMap<String, Peer>,
    retired: BTreeSet<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pulse {
    session_id: String,
    claim: String,
    sequence: u64,
    leave: bool,
}

impl Pulse {
    fn message(&self) -> Result<Vec<u8>> {
        require(
            hex(&self.session_id, 32) && hex(&self.claim, 32),
            "Invalid membership scope",
        )?;
        Ok([DOMAIN, b"heartbeat\0", serde_json::to_vec(self)?.as_slice()].concat())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SignedPulse {
    pulse: Pulse,
    signature: String,
}

impl Membership {
    fn expire(&mut self, now: f64) {
        self.peers.retain(|_, peer| peer.expires > now);
    }
    fn accept(&mut self, peer: Peer, now: f64) -> Result<()> {
        self.expire(now);
        require(
            self.peers.len() < 16 && !self.retired.contains(&peer.claim),
            "Membership unavailable",
        )?;
        require(
            self.peers.values().all(|existing| {
                existing.mac != peer.mac && existing.identity.device_id != peer.identity.device_id
            }),
            "This device already has an active membership",
        )?;
        self.peers.insert(peer.claim.clone(), peer);
        Ok(())
    }
    fn pulse(
        &mut self,
        id: &str,
        request: SignedPulse,
        observed: &ObservedPeer,
        now: f64,
    ) -> Result<Value> {
        self.expire(now);
        let pulse = request.pulse;
        require(
            pulse.session_id == id && !self.retired.contains(&pulse.claim),
            "Membership belongs to another session",
        )?;
        let peer = self
            .peers
            .get_mut(&pulse.claim)
            .ok_or("Membership expired")?;
        require(
            peer.mac == observed.mac()
                && peer.ipv4 == observed.ipv4()
                && pulse.sequence > peer.sequence,
            "Stale membership or changed station",
        )?;
        peer.identity
            .verify(&pulse.message()?, &request.signature)?;
        peer.sequence = pulse.sequence;
        peer.acknowledged = true;
        peer.expires = now + LEASE;
        if pulse.leave {
            require(
                self.retired.len() < 256,
                "Membership retirement limit reached",
            )?;
            self.peers.remove(&pulse.claim);
            self.retired.insert(pulse.claim.clone());
        }
        Ok(
            json!({"session_id":id,"claim":pulse.claim,"sequence":pulse.sequence,"connected":!pulse.leave,"lease_seconds":LEASE as u64}),
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChallengeRequest {
    session_id: String,
    claim: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    session_id: String,
    claim: String,
    challenge: String,
    identity: PublicIdentity,
    transport_key: String,
    encryption_nonce: String,
    encrypted_credential: String,
    signature: String,
}

fn scope(id: &str, claim: &str, challenge: &str, peer: &str, owner: &str) -> Result<Vec<u8>> {
    Ok([
        DOMAIN,
        crate::identity::transcript(id, claim, challenge, peer, owner)?.as_slice(),
    ]
    .concat())
}

fn proof_message(message: &[u8], proof: &Proof) -> Vec<u8> {
    [
        message,
        proof.transport_key.as_bytes(),
        proof.encryption_nonce.as_bytes(),
        proof.encrypted_credential.as_bytes(),
    ]
    .concat()
}

fn signed_response(owner: &Identity, value: Value) -> Result<Value> {
    let signature = owner.sign(
        &[
            DOMAIN,
            b"response\0",
            serde_json::to_vec(&value)?.as_slice(),
        ]
        .concat(),
    );
    Ok(json!({"value":value,"signature":signature}))
}

fn verify_response(owner: &PublicIdentity, bytes: &[u8]) -> Result<Value> {
    let response: Value = serde_json::from_slice(bytes)?;
    let value = &response["value"];
    owner.verify(
        &[DOMAIN, b"response\0", serde_json::to_vec(value)?.as_slice()].concat(),
        response["signature"]
            .as_str()
            .ok_or("Missing owner response signature")?,
    )?;
    Ok(value.clone())
}

pub fn serve(state: &State) -> Result<()> {
    require(
        state.role == "host" && state.purpose == SessionPurpose::NetworkOnly,
        "Invalid network service owner",
    )?;
    let room = NetworkRoom::new(state.id.clone(), state.self_mac.clone())?;
    let owner = crate::identity::load()?;
    let mut members = Membership::default();
    let mut challenges: BTreeMap<String, (String, String, f64, crate::handshake::Ephemeral)> =
        BTreeMap::new();
    let listener = TcpListener::bind((ADDRESS, 8765))?;
    listener.set_nonblocking(true)?;
    let mut published = 0.0;
    loop {
        current(&state.id)?;
        let now = monotonic();
        members.expire(now);
        challenges.retain(|_, challenge| challenge.2 > now);
        if now - published >= 0.4 {
            let peers: Vec<_> = members
                .peers
                .values()
                .filter(|peer| peer.acknowledged)
                .cloned()
                .collect();
            write(
                &state.root().join("network-status.json"),
                &json!({"session_id":state.id,"observed_at":now,"local_ipv4":ADDRESS,"peers":peers}),
            )?;
            published = now;
        }
        let (mut stream, source) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let outcome = (|| -> Result<Value> {
            let (request, _, body) = crate::http::packet(&mut stream)?;
            let fields: Vec<_> = request.split_whitespace().collect();
            require(fields.len() == 3, "Invalid request line")?;
            match (fields[0], fields[1]) {
                ("GET", "/health") => Ok(json!({"phase":"running","job":&state.id[..12]})),
                ("GET", "/network/room") => Ok(serde_json::to_value(&room)?),
                (
                    "POST",
                    route @ ("/network/challenge" | "/network/connect" | "/network/heartbeat"),
                ) => {
                    let station = ObservedPeer::resolve(
                        &source.ip().to_string(),
                        &fs::read_to_string(state.root().join("leases"))?,
                        &session::cmd(&["iw", "dev", "wlan0", "station", "dump"])?,
                        wall_time(),
                    )?;
                    match route {
                        "/network/challenge" => {
                            let request: ChallengeRequest = serde_json::from_slice(&body)?;
                            require(
                                request.session_id == state.id && hex(&request.claim, 32),
                                "Challenge belongs to another room",
                            )?;
                            require(
                                challenges.len() < 64 || challenges.contains_key(station.mac()),
                                "Too many challenges",
                            )?;
                            let nonce = random_id()? + &random_id()?;
                            let ephemeral = crate::handshake::Ephemeral::new()?;
                            let transport = ephemeral.public()?;
                            let message = scope(
                                &state.id,
                                &request.claim,
                                &nonce,
                                station.mac(),
                                &state.self_mac,
                            )?;
                            let signature = owner.sign(
                                &[b"owner\0".as_slice(), &message, transport.as_bytes()].concat(),
                            );
                            challenges.insert(
                                station.mac().into(),
                                (
                                    nonce.clone(),
                                    request.claim.clone(),
                                    monotonic() + 10.0,
                                    ephemeral,
                                ),
                            );
                            Ok(
                                json!({"session_id":state.id,"claim":request.claim,"challenge":nonce,"peer_mac":station.mac(),"owner_mac":state.self_mac,"identity":owner.public,"transport_key":transport,"signature":signature}),
                            )
                        }
                        "/network/connect" => {
                            let proof: Proof = serde_json::from_slice(&body)?;
                            require(
                                proof.session_id == state.id,
                                "Proof belongs to another room",
                            )?;
                            let issued = challenges
                                .remove(station.mac())
                                .ok_or("No live challenge")?;
                            require(
                                issued.0 == proof.challenge
                                    && issued.1 == proof.claim
                                    && issued.2 > monotonic(),
                                "Challenge expired or changed",
                            )?;
                            let message = scope(
                                &state.id,
                                &proof.claim,
                                &proof.challenge,
                                station.mac(),
                                &state.self_mac,
                            )?;
                            proof
                                .identity
                                .verify(&proof_message(&message, &proof), &proof.signature)?;
                            let key = issued.3.agree(&proof.transport_key, &message)?;
                            let credential = crate::handshake::open(
                                &key,
                                &message,
                                &proof.encryption_nonce,
                                &proof.encrypted_credential,
                            )?;
                            let approved = crate::identity::approve_peer(
                                &state.id,
                                &owner.public,
                                &proof.identity,
                                station.mac(),
                                credential,
                            )?;
                            if approved {
                                members.accept(
                                    Peer {
                                        identity: proof.identity,
                                        mac: station.mac().into(),
                                        ipv4: station.ipv4().into(),
                                        claim: proof.claim.clone(),
                                        sequence: 0,
                                        acknowledged: false,
                                        expires: monotonic() + LEASE,
                                    },
                                    monotonic(),
                                )?;
                            }
                            signed_response(
                                &owner,
                                json!({"session_id":state.id,"claim":proof.claim,"challenge":proof.challenge,"approved":approved,"peer_ipv4":station.ipv4()}),
                            )
                        }
                        _ => signed_response(
                            &owner,
                            members.pulse(
                                &state.id,
                                serde_json::from_slice(&body)?,
                                &station,
                                monotonic(),
                            )?,
                        ),
                    }
                }
                _ => Err("Unknown network-only endpoint".into()),
            }
        })();
        let (code, value) = match outcome {
            Ok(value) => ("200 OK", value),
            Err(error) => ("409 Conflict", json!({"error":error.to_string()})),
        };
        let body = serde_json::to_vec(&value)?;
        let _ = write!(
            stream,
            "HTTP/1.1 {code}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(&body);
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ClientLink {
    room: NetworkRoom,
    owner: PublicIdentity,
    claim: String,
    sequence: u64,
    local_ipv4: String,
}

fn connect(state: &State) -> Result<ClientLink> {
    let room: NetworkRoom = serde_json::from_slice(&crate::http::request("/network/room", None)?)?;
    room.observe(&state.room.bssid, &state.room.token)?;
    let identity = crate::identity::load()?;
    let claim = random_id()?;
    loop {
        require(
            monotonic() < current(&state.id)?.deadline,
            "Network pairing expired",
        )?;
        let challenge: Value = serde_json::from_slice(&crate::http::request(
            "/network/challenge",
            Some(&json!({"session_id":room.session_id,"claim":claim})),
        )?)?;
        let owner: PublicIdentity = serde_json::from_value(challenge["identity"].clone())?;
        crate::identity::check_owner(&identity.public, &owner, &room.host_mac)?;
        require(
            challenge["session_id"] == room.session_id
                && challenge["claim"] == claim
                && challenge["peer_mac"] == state.self_mac
                && challenge["owner_mac"] == room.host_mac,
            "Challenge belongs to another connection",
        )?;
        let nonce = challenge["challenge"].as_str().ok_or("Missing challenge")?;
        let transport = challenge["transport_key"]
            .as_str()
            .ok_or("Missing transport key")?;
        let message = scope(
            &room.session_id,
            &claim,
            nonce,
            &state.self_mac,
            &room.host_mac,
        )?;
        owner.verify(
            &[b"owner\0".as_slice(), &message, transport.as_bytes()].concat(),
            challenge["signature"]
                .as_str()
                .ok_or("Missing owner signature")?,
        )?;
        let ephemeral = crate::handshake::Ephemeral::new()?;
        let public = ephemeral.public()?;
        let key = ephemeral.agree(transport, &message)?;
        let credential = session::credentials()?;
        let (encryption_nonce, encrypted_credential) = crate::handshake::seal(
            &key,
            &message,
            credential["psk"].as_str().ok_or("Missing credential")?,
        )?;
        let mut proof = Proof {
            session_id: room.session_id.clone(),
            claim: claim.clone(),
            challenge: nonce.into(),
            identity: identity.public.clone(),
            transport_key: public,
            encryption_nonce,
            encrypted_credential,
            signature: String::new(),
        };
        proof.signature = identity.sign(&proof_message(&message, &proof));
        let reply = verify_response(
            &owner,
            &crate::http::request("/network/connect", Some(&serde_json::to_value(&proof)?))?,
        )?;
        require(
            reply["session_id"] == room.session_id
                && reply["claim"] == claim
                && reply["challenge"] == nonce,
            "Connection acknowledgement differs",
        )?;
        if reply["approved"] == true {
            let address = fs::read_to_string(state.root().join("client-address"))?;
            require(
                reply["peer_ipv4"].as_str() == Some(address.trim()),
                "Assigned address differs from host lease",
            )?;
            if !crate::identity::known(&owner)? {
                crate::identity::remember(
                    owner.clone(),
                    room.host_mac.clone(),
                    Some(fs::read_to_string(state.root().join("provisioned-key"))?),
                )?;
            }
            return Ok(ClientLink {
                room,
                owner,
                claim,
                sequence: 0,
                local_ipv4: address.trim().into(),
            });
        }
        progress(
            &state.id,
            None,
            "approval_pending_client",
            "等待房主确认加入...",
        )?;
        thread::sleep(Duration::from_millis(500));
    }
}

fn heartbeat(link: &mut ClientLink, leave: bool) -> Result<Value> {
    link.sequence = link
        .sequence
        .checked_add(1)
        .ok_or("Membership sequence exhausted")?;
    let pulse = Pulse {
        session_id: link.room.session_id.clone(),
        claim: link.claim.clone(),
        sequence: link.sequence,
        leave,
    };
    let request = SignedPulse {
        signature: crate::identity::load()?.sign(&pulse.message()?),
        pulse,
    };
    let response = verify_response(
        &link.owner,
        &crate::http::request("/network/heartbeat", Some(&serde_json::to_value(&request)?))?,
    )?;
    require(
        response["session_id"] == link.room.session_id
            && response["claim"] == link.claim
            && response["sequence"] == link.sequence
            && response["connected"] == !leave
            && response["lease_seconds"] == LEASE as u64,
        "Membership response differs",
    )?;
    Ok(response)
}

fn connected(state: &State) -> Result<()> {
    if !current(&state.id)?.established() {
        progress(
            &state.id,
            Some("network_connected"),
            "network_connected",
            "组网已完成，按 A 保持连接并返回系统。",
        )?;
        crate::platform::radio::disarm(state);
    }
    Ok(())
}

pub fn worker(state: &State) -> Result<()> {
    require(
        state.purpose == SessionPurpose::NetworkOnly,
        "Expected network-only session",
    )?;
    let mut control = if state.role == "host" {
        let control = crate::radio::host(state)?;
        crate::radio::services(state)?;
        progress(
            &state.id,
            Some("hosting"),
            "waiting_peer",
            "房间已创建，等待好友组网。",
        )?;
        Some(control)
    } else {
        crate::radio::join(state)?;
        progress(
            &state.id,
            Some("joining"),
            "network_confirming",
            "已分配地址，正在确认连接...",
        )?;
        None
    };
    let mut link = if state.role == "client" {
        Some(connect(state)?)
    } else {
        None
    };
    let mut next_pulse = 0.0;
    let mut valid_until = monotonic() + LEASE;
    loop {
        let local = current(&state.id)?;
        let now = monotonic();
        require(
            local.established() || now < local.deadline,
            "Network startup expired",
        )?;
        if let Some(control) = &mut control {
            session::observe_pairing(&local, control)?;
            require(
                best(&[
                    "systemctl",
                    "is-active",
                    &format!("{}-http.service", local.unit()),
                ]) == "active",
                "Network service stopped",
            )?;
            let snapshot = snapshot(&local)?;
            let peers = snapshot["peers"]
                .as_array()
                .ok_or("Invalid network membership")?;
            if !peers.is_empty() {
                connected(&local)?;
            } else if local.established() {
                update(&state.id, |state| {
                    state.end_reason = Some("peer_left".into())
                })?;
                return Ok(());
            }
        } else if let Some(link) = &mut link {
            if now >= next_pulse {
                let sent = monotonic();
                let result = heartbeat(link, false);
                write(&state.root().join("network-client.json"), link)?;
                if result.is_ok() {
                    valid_until = sent + LEASE;
                    let peer = Peer {
                        identity: link.owner.clone(),
                        mac: link.room.host_mac.clone(),
                        ipv4: ADDRESS.into(),
                        claim: link.claim.clone(),
                        sequence: link.sequence,
                        acknowledged: true,
                        expires: valid_until,
                    };
                    write(
                        &state.root().join("network-status.json"),
                        &json!({"session_id":state.id,"observed_at":sent,"local_ipv4":link.local_ipv4,"peers":[peer]}),
                    )?;
                    connected(&local)?;
                }
                next_pulse = sent + HEARTBEAT;
            }
            if monotonic() >= valid_until {
                update(&state.id, |state| {
                    state.end_reason = Some("host_closed".into())
                })?;
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn snapshot(state: &State) -> Result<Value> {
    require(
        !state.closed() && state.phase != "restoring",
        "Network membership ended",
    )?;
    let mut value: Value = read(&state.root().join("network-status.json"))?;
    require(
        value["session_id"] == state.id
            && value["observed_at"]
                .as_f64()
                .is_some_and(|at| at <= monotonic() && monotonic() - at < LEASE),
        "Network status expired",
    )?;
    let peers: Vec<Peer> = serde_json::from_value(value["peers"].clone())?;
    value["peers"] = serde_json::to_value(
        peers
            .into_iter()
            .filter(|peer| peer.acknowledged && peer.expires > monotonic())
            .collect::<Vec<_>>(),
    )?;
    Ok(value)
}

pub fn require_live(state: &State) -> Result<()> {
    let value = snapshot(state)?;
    require(
        value["peers"]
            .as_array()
            .is_some_and(|peers| !peers.is_empty()),
        "The network has no connected peers",
    )
}

pub fn status(state: &State, matches: bool) -> Result<Value> {
    let snapshot = snapshot(state).unwrap_or_else(|_| json!({}));
    let connected = state.established()
        && snapshot["peers"]
            .as_array()
            .is_some_and(|peers| !peers.is_empty());
    let step = if state.phase == "restoring" {
        "leaving"
    } else if !state.recovery_errors.is_empty() {
        "restore_failed"
    } else if state.closed() {
        if state.error.is_some() {
            "failed"
        } else {
            "closed"
        }
    } else if state.pending_pair_mac.is_some() {
        "approval_pending"
    } else if state.established() && !connected {
        "network_reconnecting"
    } else {
        state.step.as_str()
    };
    let peers: Vec<_> = snapshot["peers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|peer| json!({"name":peer["identity"]["name"],"address":peer["ipv4"]}))
        .collect();
    let stage = if step == "network_reconnecting" {
        "连接暂时中断，正在等待对方恢复..."
    } else {
        &state.stage
    };
    Ok(
        json!({"id":state.id,"purpose":"network_only","phase":state.phase,"role":state.role,"room":state.room,"ready":false,"connected":connected,"peer_present":connected,"step":step,"stage":stage,"context_matches":matches,"error":state.error,"end_reason":state.end_reason,"network_restored":state.network_restored,"recovery_errors":state.recovery_errors,"local_address":snapshot["local_ipv4"],"peers":peers}),
    )
}

pub fn leave(state: &State) {
    if state.role == "client" {
        if let Ok(mut link) = read::<ClientLink>(&state.root().join("network-client.json")) {
            let _ = heartbeat(&mut link, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use sha2::{Digest, Sha256};

    #[test]
    fn protocol_compatibility_network_scope_and_heartbeat_have_fixed_wire_bytes() {
        let id = "a".repeat(32);
        let claim = "b".repeat(32);
        let challenge = "c".repeat(64);
        let game = crate::identity::transcript(
            &id,
            &claim,
            &challenge,
            "02:00:00:00:00:01",
            "02:00:00:00:00:02",
        )
        .unwrap();
        assert_eq!(
            scope(
                &id,
                &claim,
                &challenge,
                "02:00:00:00:00:01",
                "02:00:00:00:00:02",
            )
            .unwrap(),
            [b"howff/network/v1\0".as_slice(), &game].concat()
        );
        let heartbeat = Pulse {
            session_id: id,
            claim,
            sequence: 7,
            leave: false,
        };
        assert_eq!(
            heartbeat.message().unwrap(),
            concat!(
                "howff/network/v1\0heartbeat\0",
                r#"{"session_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","claim":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","sequence":7,"leave":false}"#,
            )
            .as_bytes()
        );
    }

    #[test]
    fn membership_requires_signed_live_station_and_fresh_sequence() {
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let public = PublicIdentity {
            schema: 1,
            device_id: format!("{:x}", Sha256::digest(key.public_key().as_ref())),
            public_key: crate::identity::encode(key.public_key().as_ref()),
            name: "Peer".into(),
        };
        let id = "a".repeat(32);
        let claim = "b".repeat(32);
        let peer = Peer {
            identity: public.clone(),
            mac: "02:00:00:00:00:01".into(),
            ipv4: "192.168.49.2".into(),
            claim: claim.clone(),
            sequence: 0,
            acknowledged: false,
            expires: 20.0,
        };
        let station = ObservedPeer::resolve(
            &peer.ipv4,
            "0 02:00:00:00:00:01 192.168.49.2 peer *",
            "Station 02:00:00:00:00:01 (on wlan0)",
            1.0,
        )
        .unwrap();
        let signed = |sequence, leave| {
            let pulse = Pulse {
                session_id: id.clone(),
                claim: claim.clone(),
                sequence,
                leave,
            };
            let bytes = [
                pulse.message().unwrap(),
                serde_json::to_vec(&public).unwrap(),
            ]
            .concat();
            SignedPulse {
                pulse,
                signature: crate::identity::encode(key.sign(&bytes).as_ref()),
            }
        };
        let mut members = Membership::default();
        members.accept(peer.clone(), 1.0).unwrap();
        assert!(!members.peers[&claim].acknowledged);
        assert!(members.accept(peer.clone(), 1.0).is_err());
        let mut forged = signed(1, false);
        forged.pulse.leave = true;
        assert!(members.pulse(&id, forged, &station, 2.0).is_err());
        assert!(!members.peers[&claim].acknowledged);
        assert!(
            members
                .pulse(&"c".repeat(32), signed(1, false), &station, 2.0)
                .is_err()
        );
        members.pulse(&id, signed(1, false), &station, 2.0).unwrap();
        assert!(members.peers[&claim].acknowledged);
        assert!(members.pulse(&id, signed(1, false), &station, 3.0).is_err());
        let changed = ObservedPeer::resolve(
            "192.168.49.3",
            "0 02:00:00:00:00:01 192.168.49.3 peer *",
            "Station 02:00:00:00:00:01 (on wlan0)",
            1.0,
        )
        .unwrap();
        assert!(members.pulse(&id, signed(2, false), &changed, 3.0).is_err());
        members.pulse(&id, signed(2, true), &station, 3.0).unwrap();
        assert!(members.peers.is_empty());
        assert!(members.accept(peer.clone(), 4.0).is_err());
        let mut members = Membership::default();
        members.accept(peer, 1.0).unwrap();
        assert!(
            members
                .pulse(&id, signed(1, false), &station, 20.0)
                .is_err()
        );
    }

    #[test]
    fn network_signatures_are_domain_separated_from_game_proofs() {
        let id = "a".repeat(32);
        let claim = "b".repeat(32);
        let challenge = "c".repeat(64);
        let args = ("0c:91:60:d7:37:93", "0c:c6:55:5d:8e:98");
        let message = scope(&id, &claim, &challenge, args.0, args.1).unwrap();
        assert_ne!(
            message,
            crate::identity::transcript(&id, &claim, &challenge, args.0, args.1).unwrap()
        );
        assert!(message.starts_with(DOMAIN));
        assert_ne!(
            Pulse {
                session_id: id.clone(),
                claim: claim.clone(),
                sequence: 1,
                leave: false
            }
            .message()
            .unwrap(),
            Pulse {
                session_id: id,
                claim,
                sequence: 1,
                leave: true
            }
            .message()
            .unwrap()
        );
    }
}
