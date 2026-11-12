use super::room;
use crate::protocol::{GameIdentity, MAX_DESCRIPTOR, Profile, Room, VerifiedRoom};
use serde_json::json;

#[test]
fn room_and_profile_have_fixed_v1_wire_fields() {
    let encoded = serde_json::to_string(&room()).unwrap();
    let expected = concat!(
        r#"{"schema":1,"session_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","host_mac":"0c:c6:55:5d:8e:98","host_ipv4":"192.168.49.1","netplay_port":55435,"frontend_version":"1.22.2","core":{"id":"fceumm","name":"FCEUmm","version":"1"},"content":{"title":"game","sha256":""#,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        r#"","size":1,"crc32":1}}"#,
    );
    assert_eq!(encoded, expected);
    assert_eq!(Room::decode(expected.as_bytes()).unwrap(), room());
    let profile = super::context().profile(&room().session_id);
    let expected = concat!(
        r#"{"session_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","frontend":"retroarch","frontend_sha256":""#,
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        r#"","core_id":"fceumm","core_sha256":""#,
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        r#"","elf_bits":64}"#,
    );
    assert_eq!(serde_json::to_string(&profile).unwrap(), expected);
    let observed = VerifiedRoom::observe(room(), &room().host_mac, &"a".repeat(13)).unwrap();
    serde_json::from_str::<Profile>(expected)
        .unwrap()
        .validate(&observed)
        .unwrap();
}

#[test]
fn room_schema_endpoint_shape_and_bounds_fail_closed() {
    let original = serde_json::to_value(room()).unwrap();
    for (field, value) in [
        ("schema", json!(0)),
        ("schema", json!(2)),
        ("session_id", json!("a".repeat(31))),
        ("host_mac", json!("invalid")),
        ("host_ipv4", json!("192.168.1.14")),
        ("netplay_port", json!(8765)),
        ("frontend_version", json!("1.22.2\n")),
        ("application_version", json!("0.13.0")),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        assert!(
            Room::decode(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{field}"
        );
    }
    for field in ["id", "name", "version"] {
        let mut changed = original.clone();
        changed["core"][field] = json!("invalid\n");
        assert!(Room::decode(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    for (field, value) in [
        ("sha256", json!("B".repeat(64))),
        ("size", json!(0)),
        ("size", json!(crate::protocol::MAX_CONTENT + 1)),
        ("path", json!("/roms/game.nes")),
    ] {
        let mut changed = original.clone();
        changed["content"][field] = value;
        assert!(Room::decode(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    let mut boundary = serde_json::to_vec(&room()).unwrap();
    boundary.resize(MAX_DESCRIPTOR, b' ');
    assert!(Room::decode(&boundary).is_ok());
    boundary.push(b' ');
    assert!(Room::decode(&boundary).is_err());
    let duplicate = serde_json::to_string(&room())
        .unwrap()
        .replace(r#""id":"fceumm""#, r#""id":"fceumm","id":"fceumm""#);
    assert!(Room::decode(duplicate.as_bytes()).is_err());
}

#[test]
fn game_compatibility_uses_native_identity_and_content_not_title() {
    let original = room().game();
    let value = serde_json::to_value(&original).unwrap();
    let mut renamed = original.clone();
    renamed.content.title = "本机显示名称".into();
    assert!(original.compatible(&renamed));
    for (section, field, replacement) in [
        (None, "frontend_version", json!("1.22.3")),
        (Some("core"), "id", json!("other_core")),
        (Some("core"), "name", json!("Other core")),
        (Some("core"), "version", json!("2")),
        (Some("content"), "sha256", json!("c".repeat(64))),
        (Some("content"), "size", json!(2)),
        (Some("content"), "crc32", json!(2)),
    ] {
        let mut changed = value.clone();
        if let Some(section) = section {
            changed[section][field] = replacement;
        } else {
            changed[field] = replacement;
        }
        let changed: GameIdentity = serde_json::from_value(changed).unwrap();
        changed.validate().unwrap();
        assert!(!original.compatible(&changed), "{section:?}.{field}");
        assert!(!changed.compatible(&original));
    }
}

#[test]
fn profile_admission_binds_session_core_and_binary_description() {
    let selected = VerifiedRoom::observe(room(), &room().host_mac, &"a".repeat(13)).unwrap();
    let profile = super::context().profile(&room().session_id);
    let original = serde_json::to_value(profile).unwrap();
    for (field, value) in [
        ("session_id", json!("b".repeat(32))),
        ("core_id", json!("other_core")),
        ("frontend", json!("/tmp/frontend")),
        ("frontend_sha256", json!("c".repeat(63))),
        ("core_sha256", json!("D".repeat(64))),
        ("elf_bits", json!(16)),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        assert!(
            serde_json::from_value::<Profile>(changed)
                .unwrap()
                .validate(&selected)
                .is_err(),
            "{field}"
        );
    }
    let mut changed = original.clone();
    changed["core_path"] = json!("/tmp/core.so");
    assert!(serde_json::from_value::<Profile>(changed).is_err());
    for (frontend, bits) in [("retroarch", 64), ("retroarch32", 32)] {
        let mut changed = original.clone();
        changed["frontend"] = json!(frontend);
        changed["elf_bits"] = json!(bits);
        serde_json::from_value::<Profile>(changed)
            .unwrap()
            .validate(&selected)
            .unwrap();
    }
}

#[test]
fn discovery_hints_and_network_rooms_cannot_replace_verified_game_identity() {
    use crate::purpose::{NetworkRoom, SessionPurpose};
    use crate::session::RoomEntry;
    let selected = room();
    let hint = RoomEntry::observed(
        "DIRECT-NP-bbbbbb-aaaaaaaaaaaaa",
        &selected.host_mac,
        120,
        &selected.content.sha256,
    )
    .unwrap();
    assert_eq!(hint.purpose(), SessionPurpose::Game);
    assert!(hint.same_game_hint);
    assert_eq!(hint.signal, 100);
    let mut unrelated = selected.clone();
    unrelated.content.sha256.replace_range(6..7, "c");
    assert!(unrelated.content.sha256.starts_with("bbbbbb"));
    assert!(!selected.game().compatible(&unrelated.game()));
    assert!(VerifiedRoom::observe(selected.clone(), &hint.bssid, &"b".repeat(13)).is_err());
    let network = NetworkRoom::new(selected.session_id.clone(), selected.host_mac.clone()).unwrap();
    assert!(Room::decode(&serde_json::to_vec(&network).unwrap()).is_err());
    assert!(
        serde_json::from_slice::<NetworkRoom>(&serde_json::to_vec(&selected).unwrap()).is_err()
    );
    let advertisement = RoomEntry::observed(
        "DIRECT-NW-aaaaaaaaaaaaa",
        &network.host_mac,
        -1,
        &selected.content.sha256,
    )
    .unwrap();
    assert_eq!(advertisement.purpose(), SessionPurpose::NetworkOnly);
    assert!(!advertisement.same_game_hint);
    assert_eq!(advertisement.signal, 0);
    network
        .observe(&advertisement.bssid, &advertisement.token)
        .unwrap();
    for ssid in [
        "DIRECT-NP-zzzzzz-aaaaaaaaaaaaa",
        "DIRECT-NW-aaaaaaaaaaaa",
        "DIRECT-NP-bbbbbb-AAAAAAAAAAAAA",
    ] {
        assert!(RoomEntry::observed(ssid, &selected.host_mac, 50, "").is_none());
    }
}

#[test]
fn pairing_and_proof_wire_bytes_are_stable() {
    assert_eq!(
        crate::pairing::credential("0123-4567-89ab").unwrap(),
        (
            "e7b5fa864e621a848ce3ef8ac74c894071218a24aedec915".into(),
            "6c635b38f3e2b6e5".into(),
        )
    );
    let message = crate::identity::transcript(
        &"a".repeat(32),
        &"b".repeat(32),
        &"c".repeat(64),
        "02:00:00:00:00:01",
        "02:00:00:00:00:02",
    )
    .unwrap();
    let expected = concat!(
        "arkos-nearby/peer-proof/v1\0",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\0",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\0",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\0",
        "02:00:00:00:00:01\0",
        "02:00:00:00:00:02",
    );
    assert_eq!(message, expected.as_bytes());
}

#[test]
fn signatures_cannot_cross_session_claim_challenge_station_or_role() {
    use crate::identity::{PublicIdentity, encode, transcript};
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use sha2::{Digest, Sha256};
    let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let identity = PublicIdentity {
        schema: 1,
        device_id: format!("{:x}", Sha256::digest(key.public_key().as_ref())),
        public_key: encode(key.public_key().as_ref()),
        name: "Fixture".into(),
    };
    let fields = [
        "a".repeat(32),
        "b".repeat(32),
        "c".repeat(64),
        "02:00:00:00:00:01".into(),
        "02:00:00:00:00:02".into(),
    ];
    let message = |fields: &[String; 5]| {
        transcript(&fields[0], &fields[1], &fields[2], &fields[3], &fields[4]).unwrap()
    };
    let signed = |bytes: &[u8]| {
        encode(
            key.sign(&[bytes, &serde_json::to_vec(&identity).unwrap()].concat())
                .as_ref(),
        )
    };
    let bytes = message(&fields);
    let signature = signed(&bytes);
    identity.verify(&bytes, &signature).unwrap();
    for index in 0..fields.len() {
        let mut changed = fields.clone();
        changed[index] = if index < 3 {
            "d".repeat(fields[index].len())
        } else {
            "02:00:00:00:00:03".into()
        };
        assert!(
            identity.verify(&message(&changed), &signature).is_err(),
            "{index}"
        );
    }
    let mut swapped = fields.clone();
    swapped.swap(3, 4);
    assert!(identity.verify(&message(&swapped), &signature).is_err());
    let owner = [b"owner\0".as_slice(), &bytes].concat();
    assert!(identity.verify(&bytes, &signed(&owner)).is_err());
    let network = [b"howff/network/v1\0".as_slice(), &bytes].concat();
    assert!(identity.verify(&network, &signature).is_err());
}

fn read_packet(bytes: Vec<u8>) -> crate::Result<Vec<u8>> {
    use std::io::Write;
    use std::net::{Shutdown, TcpListener, TcpStream};
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let mut writer = TcpStream::connect(listener.local_addr()?)?;
    let (mut reader, _) = listener.accept()?;
    reader.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    let sender = std::thread::spawn(move || {
        let _ = writer.write_all(&bytes);
        let _ = writer.shutdown(Shutdown::Write);
    });
    let result = crate::http::packet(&mut reader).map(|(_, _, body)| body);
    drop(reader);
    sender.join().unwrap();
    result
}

#[test]
fn control_http_frames_enforce_complete_bounded_unambiguous_messages() {
    let mut boundary =
        format!("POST /ready HTTP/1.1\r\nContent-Length: {MAX_DESCRIPTOR}\r\n\r\n").into_bytes();
    boundary.resize(boundary.len() + MAX_DESCRIPTOR, b' ');
    assert_eq!(read_packet(boundary).unwrap().len(), MAX_DESCRIPTOR);
    for bytes in [
        b"POST /ready HTTP/1.1\r\nContent-Length: 1\r\ncontent-length: 1\r\n\r\nx".to_vec(),
        b"POST /ready HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n".to_vec(),
        b"POST /ready HTTP/1.1\r\nContent-Length: 2\r\n\r\nx".to_vec(),
        format!(
            "POST /ready HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_DESCRIPTOR + 1
        )
        .into_bytes(),
        format!(
            "POST /ready HTTP/1.1\r\nX-Large: {}\r\n\r\n",
            "a".repeat(8192)
        )
        .into_bytes(),
    ] {
        assert!(read_packet(bytes).is_err());
    }
    let empty = read_packet(b"GET /room HTTP/1.1\r\nHost: room\r\n\r\n".to_vec()).unwrap();
    assert!(empty.is_empty());
}
