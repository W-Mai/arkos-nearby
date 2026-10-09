use crate::protocol::{ContentIdentity, CoreIdentity, GameIdentity, Room, VerifiedRoom};

fn room() -> Room {
    Room::new(
        "a".repeat(32),
        "0c:c6:55:5d:8e:98".into(),
        GameIdentity {
            frontend_version: "1.22.2".into(),
            core: CoreIdentity {
                id: "fceumm".into(),
                name: "FCEUmm".into(),
                version: "1".into(),
            },
            content: ContentIdentity {
                title: "game".into(),
                sha256: "b".repeat(64),
                size: 1,
                crc32: 1,
            },
        },
    )
    .unwrap()
}

#[test]
fn wire_identity_never_contains_paths_or_duplicate_fields() {
    let room = room();
    let bytes = serde_json::to_vec(&room).unwrap();
    assert_eq!(Room::decode(&bytes).unwrap(), room);
    let mut value = serde_json::to_value(&room).unwrap();
    value["core"]["path"] = "/opt/unknown.so".into();
    assert!(Room::decode(&serde_json::to_vec(&value).unwrap()).is_err());
    let duplicate =
        String::from_utf8(bytes)
            .unwrap()
            .replacen("\"schema\":1", "\"schema\":1,\"schema\":1", 1);
    assert!(Room::decode(duplicate.as_bytes()).is_err());
}

#[test]
fn launch_authority_requires_the_observed_radio_and_room_token() {
    assert!(VerifiedRoom::observe(room(), "0c:c6:55:5d:8e:98", &"a".repeat(13)).is_ok());
    assert!(VerifiedRoom::observe(room(), "0c:91:60:d7:37:93", &"a".repeat(13)).is_err());
    assert!(VerifiedRoom::observe(room(), "0c:c6:55:5d:8e:98", &"c".repeat(13)).is_err());
    let mut value = room();
    value.core.id = "../../outside".into();
    assert!(value.validate().is_err());
}

fn context() -> crate::context::Context {
    crate::context::Context {
        frontend: "retroarch".into(),
        frontend_path: "/opt/retroarch/bin/retroarch".into(),
        frontend_sha256: "c".repeat(64),
        core_id: "fceumm".into(),
        core_path: "/home/ark/.config/retroarch/cores/fceumm_libretro.so".into(),
        game_path: "/roms/game.nes".into(),
        title: "game".into(),
        identity: room().game(),
        core_sha256: "d".repeat(64),
        core_elf_bits: 64,
        block_extract: false,
        supported: true,
        link_mode: None,
        argv: vec![],
    }
}

#[test]
fn unsupported_handheld_link_cannot_offer_create_or_join_actions() {
    let mut context = context();
    context.game_path = "/roms/gba/test.zip".into();
    context.supported = false;
    let mut flow = crate::flow::Flow::new(Some(context));
    assert_eq!(flow.page, crate::flow::Page::Unavailable);
    flow.dispatch("confirm", crate::system::monotonic() + 1.0);
    assert!(flow.request.is_none());
    flow.dispatch("back", crate::system::monotonic() + 1.0);
    assert_eq!(flow.exit_code, Some(230));
}

#[test]
fn navigation_merges_exit_and_binds_cleanup_to_the_completed_session() {
    use crate::flow::{Flow, Operation, Page};
    let mut flow = Flow::new(Some(context()));
    flow.dispatch("confirm", crate::system::monotonic() + 1.0);
    let create = flow.request.take().unwrap();
    flow.dispatch("back", crate::system::monotonic() + 1.0);
    flow.dispatch("back", crate::system::monotonic() + 1.0);
    assert!(flow.request.is_none());
    let id = "a".repeat(32);
    flow.complete(
        &create,
        serde_json::json!({"id":id,"phase":"opening","role":"host"}),
        false,
    );
    let cancel = flow.request.take().unwrap();
    assert_eq!(cancel.operation, Operation::Cancel);
    assert_eq!(cancel.payload["session_id"], id);
    flow.complete(&create, serde_json::json!({"id":"b".repeat(32)}), false);
    flow.complete(
        &cancel,
        serde_json::json!({"id":id,"phase":"closed","network_restored":false}),
        false,
    );
    assert_eq!(flow.page, Page::Leaving);
    assert_eq!(flow.phase, "restore_failed");
    flow.dispatch("confirm", crate::system::monotonic() + 1.0);
    let retry = flow.request.take().unwrap();
    flow.complete(
        &retry,
        serde_json::json!({"id":id,"phase":"closed","network_restored":true,"recovery_errors":[]}),
        false,
    );
    assert_eq!(flow.page, Page::Choice);
    assert_eq!(flow.exit_code, None);
}

#[test]
fn previous_game_cleanup_completes_even_when_the_selected_game_changes() {
    use crate::flow::{Flow, Page};
    for context_matches in [true, false] {
        let mut flow = Flow::new(Some(context()));
        let id = "a".repeat(32);
        flow.status(0,serde_json::json!({"id":id,"role":"host","phase":"restoring","step":"leaving","context_matches":context_matches,"stage":"正在恢复当前网络...","network_restored":false,"recovery_errors":[]}));
        assert_eq!(flow.page, Page::Leaving);
        assert_eq!(flow.phase, "leaving");
        let generation = flow.generation;
        flow.status(generation,serde_json::json!({"id":id,"role":"host","phase":"closed","step":"closed","context_matches":context_matches,"network_restored":true,"recovery_errors":[]}));
        assert_eq!(flow.page, Page::Choice);
        assert_eq!(flow.phase, "idle");
        assert_eq!(flow.view()["room_name"], "");
        assert_eq!(flow.view()["ready"], false);
    }
}

#[test]
fn observed_readiness_expires_and_retired_claims_cannot_return() {
    use crate::readiness::{Controller, ObservedPeer};
    let stations = "Station 0c:91:60:d7:37:93 (on wlan0)";
    let leases = "200 0c:91:60:d7:37:93 192.168.49.10 phone *";
    assert!(ObservedPeer::resolve("192.168.49.10", leases, "", 100.0).is_err());
    assert!(ObservedPeer::resolve("192.168.49.10", leases, stations, 201.0).is_err());
    let peer = ObservedPeer::resolve("192.168.49.10", leases, stations, 100.0).unwrap();
    let mut controller = Controller::new(room());
    let claim = "e".repeat(32);
    let ready = serde_json::to_vec(
        &serde_json::json!({"session_id":"a".repeat(32),"claim_id":claim,"game":room().game()}),
    )
    .unwrap();
    assert_eq!(
        controller.accept(&ready, &peer, 1.0).unwrap()["compatible"],
        true
    );
    controller.expire(31.0);
    assert!(controller.peer.is_none());
    controller.accept(&ready, &peer, 32.0).unwrap();
    let release =
        serde_json::to_vec(&serde_json::json!({"session_id":"a".repeat(32),"claim_id":claim}))
            .unwrap();
    controller.release(&release, &peer).unwrap();
    assert!(controller.accept(&ready, &peer, 33.0).is_err());
    let wrong: serde_json::Value = serde_json::json!({"session_id":"a".repeat(32),"claim_id":"f".repeat(32),"game":room().game(),"command":"launch"});
    assert!(
        controller
            .accept(&serde_json::to_vec(&wrong).unwrap(), &peer, 33.0)
            .is_err()
    );
}

#[test]
fn native_configuration_confines_all_owned_write_destinations() {
    let path = std::path::Path::new("/run/arkos-nearby-rust/session/game");
    let overrides = crate::game::configuration(path).unwrap();
    let original = "savefile_directory = \"/roms/saves\"\ncache_directory = \"/roms/cache\"\nconfig_save_on_exit = \"true\"\nvideo_driver = \"gl\"\n";
    let isolated = crate::game::isolated(original, &overrides);
    assert!(!isolated.contains("/roms/"));
    assert_eq!(isolated.matches("savefile_directory =").count(), 1);
    assert!(isolated.contains("video_driver = \"gl\""));
    assert!(isolated.contains("config_save_on_exit = \"false\""));
    let wrong = std::path::Path::new("/run/session\"\ncache_directory=\"/roms");
    assert!(crate::game::configuration(wrong).is_err());
}

#[test]
fn address_reassignment_cannot_preserve_another_stations_readiness() {
    use crate::readiness::{Controller, ObservedPeer};
    let stations = "Station 0c:91:60:d7:37:93 (on wlan0)\nStation 0c:c6:55:5d:8e:98 (on wlan0)";
    let peer = ObservedPeer::resolve(
        "192.168.49.10",
        "200 0c:91:60:d7:37:93 192.168.49.10 * *",
        stations,
        100.0,
    )
    .unwrap();
    let mut controller = Controller::new(room());
    let bytes=serde_json::to_vec(&serde_json::json!({"session_id":"a".repeat(32),"claim_id":"e".repeat(32),"game":room().game()})).unwrap();
    controller.accept(&bytes, &peer, 1.0).unwrap();
    controller.refresh(
        "200 0c:c6:55:5d:8e:98 192.168.49.10 * *",
        stations,
        2.0,
        100.0,
    );
    assert!(controller.peer.is_none());
}

#[test]
fn renderer_contract_contains_display_fields_without_launch_authority() {
    let flow = crate::flow::Flow::new(Some(context()));
    let view = flow.view();
    let keys: std::collections::BTreeSet<_> = view
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let expected: std::collections::BTreeSet<_> = [
        "page",
        "title",
        "core",
        "selected",
        "rooms",
        "room_name",
        "ready",
        "busy",
        "refreshing",
        "status",
        "phase",
        "back_label",
    ]
    .into_iter()
    .collect();
    assert_eq!(keys, expected);
    let encoded = serde_json::to_string(&view).unwrap();
    assert!(!encoded.contains("/opt/"));
    assert!(!encoded.contains("/roms/"));
    assert!(!encoded.contains("core_path"));
    assert!(!encoded.contains("session_id"));
}

#[test]
fn compatibility_accepts_local_unicast_devices_without_a_mac_allowlist() {
    use crate::compat::{Device, Facts, KERNEL_SHA, PROFILE, Panel, valid_mac};
    let facts = Facts {
        schema: 1,
        architecture: "aarch64".into(),
        platform: "linux".into(),
        mac: "02:ab:cd:12:34:56".into(),
        kernel_sha256: KERNEL_SHA.into(),
        usb_id: "0bda:8179".into(),
        panel: Some(Panel {
            width: 640,
            height: 480,
            bits: 32,
            red: 16,
            blue: 0,
            rotation: 0,
        }),
        dependencies: vec![],
    };
    assert!(facts.problems().is_empty());
    let device = facts.device().unwrap();
    assert!(device.validate(&facts.mac).is_ok());
    assert!(device.validate("02:ab:cd:12:34:57").is_err());
    assert!(!valid_mac("ff:ff:ff:ff:ff:ff"));
    assert!(!valid_mac("00:00:00:00:00:00"));
    let mut wrong = facts.clone();
    wrong.kernel_sha256 = "0".repeat(64);
    assert!(wrong.device().is_err());
    let wrong = Device {
        schema: 1,
        mac: facts.mac,
        profile: PROFILE.into(),
        kernel_sha256: "0".repeat(64),
    };
    assert!(wrong.validate("02:ab:cd:12:34:56").is_err());
}

#[test]
fn pairing_credential_is_group_bound_and_rejects_ambiguous_input() {
    use crate::pairing::{credential, display, generate, normalize};
    assert_eq!(normalize("abcd-1234-wxyz").unwrap(), "ABCD1234WXYZ");
    assert_eq!(display("ABCD1234WXYZ").unwrap(), "ABCD-1234-WXYZ");
    assert_eq!(
        credential("abcd-1234-wxyz").unwrap(),
        credential("ABCD1234WXYZ").unwrap()
    );
    assert_ne!(
        credential("ABCD1234WXYZ").unwrap(),
        credential("ABCD1234WXY0").unwrap()
    );
    let (key, group) = credential("ABCD1234WXYZ").unwrap();
    assert_eq!(key.len(), 48);
    assert_eq!(group.len(), 16);
    for wrong in [
        "",
        "123456",
        "ABCD1234WXYI",
        "ABCD1234WXYO",
        "../../abcdef",
        "ABCD1234WXYZ\n",
    ] {
        assert!(normalize(wrong).is_err());
    }
    let first = generate().unwrap();
    assert!(normalize(&first).is_ok());
    assert_ne!(first, generate().unwrap());
}

#[test]
fn signing_identity_binds_names_challenges_and_room_scope() {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let key = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
    let key = Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap();
    let public = key.public_key().as_ref();
    use sha2::{Digest, Sha256};
    let identity = crate::identity::PublicIdentity {
        schema: 1,
        device_id: format!("{:x}", Sha256::digest(public)),
        public_key: crate::identity::encode(public),
        name: "Handheld".into(),
    };
    let message = crate::identity::transcript(
        &"a".repeat(32),
        &"b".repeat(32),
        &"c".repeat(64),
        "02:aa:bb:cc:dd:ee",
        "02:aa:bb:cc:dd:ef",
    )
    .unwrap();
    let payload = [
        message.as_slice(),
        serde_json::to_vec(&identity).unwrap().as_slice(),
    ]
    .concat();
    let signature = crate::identity::encode(key.sign(&payload).as_ref());
    assert!(identity.verify(&message, &signature).is_ok());
    let other = crate::identity::transcript(
        &"d".repeat(32),
        &"b".repeat(32),
        &"c".repeat(64),
        "02:aa:bb:cc:dd:ee",
        "02:aa:bb:cc:dd:ef",
    )
    .unwrap();
    assert!(identity.verify(&other, &signature).is_err());
    let mut renamed = identity.clone();
    renamed.name = "Someone else".into();
    assert!(renamed.verify(&message, &signature).is_err());
    let mut forged = identity;
    forged.device_id = "0".repeat(64);
    assert!(forged.validate().is_err());
}

#[test]
fn exchanged_credentials_are_authenticated_to_the_current_room_scope() {
    use crate::handshake::{Ephemeral, open, seal};
    let alice = Ephemeral::new().unwrap();
    let bob = Ephemeral::new().unwrap();
    let a = alice.public().unwrap();
    let b = bob.public().unwrap();
    let scope = b"room and observed station";
    let left = alice.agree(&b, scope).unwrap();
    let right = bob.agree(&a, scope).unwrap();
    let (nonce, cipher) = seal(&left, scope, "privatewirelesscredential2026").unwrap();
    assert_eq!(
        open(&right, scope, &nonce, &cipher).unwrap(),
        "privatewirelesscredential2026"
    );
    assert!(open(&right, b"another room", &nonce, &cipher).is_err());
    let mut changed = cipher;
    let replacement = if changed.starts_with("00") {
        "01"
    } else {
        "00"
    };
    changed.replace_range(0..2, replacement);
    assert!(open(&right, scope, &nonce, &changed).is_err());
}
