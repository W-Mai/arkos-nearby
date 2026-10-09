use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::context::Context;
use crate::protocol::GameIdentity;
use crate::protocol::{ADDRESS, MAX_DESCRIPTOR, Room};
use crate::readiness::{Controller, ObservedPeer};
use crate::session::{cmd, current};
use crate::system::{monotonic, read, wall_time, write};
use crate::{Result, require};

type Packet = (String, Vec<(String, String)>, Vec<u8>);
fn packet(stream: &mut TcpStream) -> Result<Packet> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        header.push(byte[0]);
        require(header.len() <= 8192, "HTTP header limit exceeded")?;
    }
    let text = String::from_utf8(header)?;
    let mut lines = text.split("\r\n");
    let first = lines.next().ok_or("Missing HTTP request")?.to_owned();
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or("Invalid HTTP header")?;
        let key = key.trim().to_ascii_lowercase();
        require(
            !headers.iter().any(|(prior, _)| prior == &key),
            "Duplicate HTTP header",
        )?;
        headers.push((key, value.trim().into()));
    }
    require(
        !headers.iter().any(|(key, _)| key == "transfer-encoding"),
        "Chunked requests are unsupported",
    )?;
    let size = headers
        .iter()
        .find(|(key, _)| key == "content-length")
        .map(|(_, value)| value.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    require(size <= MAX_DESCRIPTOR, "HTTP body limit exceeded")?;
    let mut body = vec![0; size];
    stream.read_exact(&mut body)?;
    Ok((first, headers, body))
}

pub fn request(route: &str, payload: Option<&Value>) -> Result<Vec<u8>> {
    require(
        matches!(
            route,
            "/room"
                | "/handheld-profile"
                | "/ready"
                | "/unready"
                | "/room-status"
                | "/health"
                | "/identity-challenge"
        ),
        "Unknown local control route",
    )?;
    let mut stream =
        TcpStream::connect_timeout(&format!("{ADDRESS}:8765").parse()?, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let bytes = payload
        .map(serde_json::to_vec)
        .transpose()?
        .unwrap_or_default();
    let method = if payload.is_some() { "POST" } else { "GET" };
    write!(
        stream,
        "{method} {route} HTTP/1.1\r\nHost: {ADDRESS}:8765\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        bytes.len()
    )?;
    stream.write_all(&bytes)?;
    let (first, _, body) = packet(&mut stream)?;
    require(
        first.starts_with("HTTP/1.1 200 "),
        "Room control request failed",
    )?;
    Ok(body)
}

pub fn serve(id: &str) -> Result<()> {
    let state = current(id)?;
    require(
        state.role == "host",
        "Only the selected room host may serve",
    )?;
    let root = state.root();
    let room = Room::decode(&fs::read(root.join("game/room.json"))?)?;
    let context: Context = read(&root.join("context.json"))?;
    let mut controller = Controller::new(room.clone());
    let owner = crate::identity::load()?;
    let mut challenges: BTreeMap<String, (String, String, f64, crate::handshake::Ephemeral)> =
        BTreeMap::new();
    let listener = TcpListener::bind((ADDRESS, 8765))?;
    listener.set_nonblocking(true)?;
    loop {
        current(id)?;
        let stations = cmd(&["iw", "dev", "wlan0", "station", "dump"])?;
        let leases = fs::read_to_string(root.join("leases")).unwrap_or_default();
        controller.refresh(&leases, &stations, monotonic(), wall_time());
        write(
            &root.join("readiness.json"),
            &controller.status(monotonic()),
        )?;
        write(
            &root.join("station-status.json"),
            &json!({"observed_at":monotonic(),"peer_present":stations.lines().any(|line|line.starts_with("Station "))}),
        )?;
        match listener.accept() {
            Ok((mut stream, source)) => {
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                let outcome = (|| -> Result<Value> {
                    let (first, _, body) = packet(&mut stream)?;
                    let fields: Vec<_> = first.split_whitespace().collect();
                    require(
                        fields.len() == 3 && fields[2] == "HTTP/1.1",
                        "Invalid request line",
                    )?;
                    match (fields[0], fields[1]) {
                        ("GET", "/health") => Ok(json!({"phase":"running","job":&state.id[..12]})),
                        ("GET", "/room") => Ok(serde_json::to_value(&room)?),
                        ("GET", "/handheld-profile") => {
                            Ok(serde_json::to_value(context.profile(id))?)
                        }
                        ("GET", "/room-status") => {
                            let mut status = controller.status(monotonic());
                            status["native_ready"] = crate::game::listening(&current(id)?).into();
                            Ok(status)
                        }
                        ("POST", "/identity-challenge") => {
                            #[derive(Deserialize)]
                            #[serde(deny_unknown_fields)]
                            struct Request {
                                session_id: String,
                                claim_id: String,
                            }
                            let value: Request = serde_json::from_slice(&body)?;
                            crate::require(
                                value.session_id == state.id
                                    && crate::protocol::hex(&value.claim_id, 32),
                                "Identity request belongs to another room",
                            )?;
                            let observed = ObservedPeer::resolve(
                                &source.ip().to_string(),
                                &leases,
                                &stations,
                                wall_time(),
                            )?;
                            let challenge =
                                crate::system::random_id()? + &crate::system::random_id()?;
                            challenges.retain(|_, value| value.2 > monotonic());
                            crate::require(
                                challenges.len() < 64 || challenges.contains_key(observed.mac()),
                                "Identity challenge limit reached",
                            )?;
                            let ephemeral = crate::handshake::Ephemeral::new()?;
                            let transport = ephemeral.public()?;
                            challenges.insert(
                                observed.mac().into(),
                                (
                                    challenge.clone(),
                                    value.claim_id.clone(),
                                    monotonic() + 10.0,
                                    ephemeral,
                                ),
                            );
                            let message = crate::identity::transcript(
                                &state.id,
                                &value.claim_id,
                                &challenge,
                                observed.mac(),
                                &state.self_mac,
                            )?;
                            let message = [
                                b"owner\0".as_slice(),
                                message.as_slice(),
                                transport.as_bytes(),
                            ]
                            .concat();
                            Ok(
                                json!({"identity":owner.public,"session_id":state.id,"claim_id":value.claim_id,"challenge":challenge,"peer_mac":observed.mac(),"owner_mac":state.self_mac,"transport_key":transport,"signature":owner.sign(&message)}),
                            )
                        }
                        ("POST", "/ready" | "/unready") => {
                            let observed = ObservedPeer::resolve(
                                &source.ip().to_string(),
                                &leases,
                                &stations,
                                wall_time(),
                            )?;
                            let response = if fields[1] == "/ready" {
                                #[derive(Deserialize)]
                                #[serde(deny_unknown_fields)]
                                struct AuthReady {
                                    session_id: String,
                                    claim_id: String,
                                    game: GameIdentity,
                                    identity: crate::identity::PublicIdentity,
                                    challenge: String,
                                    signature: String,
                                    transport_key: String,
                                    encryption_nonce: String,
                                    encrypted_credential: String,
                                }
                                let value: AuthReady = serde_json::from_slice(&body)?;
                                require(
                                    value.session_id == state.id,
                                    "Identity proof belongs to another room",
                                )?;
                                let issued = challenges
                                    .remove(observed.mac())
                                    .ok_or("No live identity challenge")?;
                                require(
                                    issued.0 == value.challenge
                                        && issued.1 == value.claim_id
                                        && issued.2 > monotonic(),
                                    "Identity challenge expired or was replaced",
                                )?;
                                let message = crate::identity::transcript(
                                    &state.id,
                                    &value.claim_id,
                                    &value.challenge,
                                    observed.mac(),
                                    &state.self_mac,
                                )?;
                                let signed = [
                                    message.as_slice(),
                                    value.transport_key.as_bytes(),
                                    value.encryption_nonce.as_bytes(),
                                    value.encrypted_credential.as_bytes(),
                                    serde_json::to_vec(&value.game)?.as_slice(),
                                ]
                                .concat();
                                value.identity.verify(&signed, &value.signature)?;
                                let key = issued.3.agree(&value.transport_key, &message)?;
                                let wireless_key = crate::handshake::open(
                                    &key,
                                    &message,
                                    &value.encryption_nonce,
                                    &value.encrypted_credential,
                                )?;
                                require(
                                    value.identity.device_id != owner.public.device_id,
                                    "Another handheld has a cloned identity; explicitly reset it",
                                )?;
                                let approved = crate::identity::known(&value.identity)?
                                    || current(id)?.approved_mac.as_deref() == Some(observed.mac());
                                if !approved {
                                    crate::system::write(
                                        &root.join("pending-device.json"),
                                        &value.identity,
                                    )?;
                                    crate::session::update(id, |state| {
                                        state.pending_pair_mac = Some(observed.mac().into());
                                        state.pending_pair_kind =
                                            Some(crate::session::PairingRequest::Identity);
                                        state.stage = "好友加入，按 A 确认。".into();
                                    })?;
                                    json!({"session_id":state.id,"claim_id":value.claim_id,"approved":false,"compatible":false})
                                } else {
                                    let known = crate::identity::known(&value.identity)?;
                                    {
                                        let _ = known;
                                        crate::identity::remember(
                                            value.identity,
                                            observed.mac().into(),
                                            Some(wireless_key),
                                        )?;
                                    }
                                    let payload = json!({"session_id":value.session_id,"claim_id":value.claim_id,"game":value.game});
                                    controller.accept(
                                        &serde_json::to_vec(&payload)?,
                                        &observed,
                                        monotonic(),
                                    )?
                                }
                            } else {
                                controller.release(&body, &observed)?
                            };
                            write(
                                &root.join("readiness.json"),
                                &controller.status(monotonic()),
                            )?;
                            Ok(response)
                        }
                        _ => Err("Unknown room endpoint".into()),
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
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100))
            }
            Err(error) => return Err(error.into()),
        }
    }
}
