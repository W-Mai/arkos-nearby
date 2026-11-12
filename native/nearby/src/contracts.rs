use crate::protocol::{ContentIdentity, CoreIdentity, GameIdentity, Room, VerifiedRoom};

mod protocol_compatibility;

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

pub(crate) fn context() -> crate::context::Context {
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
fn lobby_keeps_explicit_selection_and_recovers_before_returning() {
    use crate::flow::{Flow, Operation, Page};
    use serde_json::json;
    let now = || crate::system::monotonic() + 1.0;
    let mut flow = Flow::lobby(Some(context()), true);
    assert_eq!(flow.page, Page::Lobby);
    flow.scan(flow.generation, json!({"rooms":[{"id":"first","ssid":"DIRECT-NW-aaaaaaaaaaaaa","token":"aaaaaaaaaaaaa","signal":70},{"id":"second","ssid":"DIRECT-NP-bbbbbb-bbbbbbbbbbbbb","token":"bbbbbbbbbbbbb","signal":60}]}));
    flow.dispatch("down", now());
    flow.dispatch("refresh", now());
    assert!(flow.request.is_none());
    flow.dispatch("confirm", now());
    let request = flow.request.take().unwrap();
    assert_eq!(request.operation, Operation::Join);
    assert_eq!(request.payload["room"]["id"], "second");
    flow.dispatch("back", now());
    flow.dispatch("back", now());
    assert_eq!(flow.page, Page::Leaving);
    let id = "e".repeat(32);
    flow.complete(
        &request,
        json!({"id":id,"phase":"joining","step":"checking","role":"client","context_matches":true}),
        false,
    );
    let cancel = flow.request.take().unwrap();
    assert_eq!(cancel.operation, Operation::Cancel);
    flow.complete(
        &cancel,
        json!({"id":id,"phase":"closed","network_restored":true,"recovery_errors":[]}),
        false,
    );
    assert_eq!(flow.page, Page::Lobby);
    assert_eq!(flow.view()["selected"], 1);
    flow.complete(
        &request,
        json!({"id":id,"phase":"ready","ready":true}),
        false,
    );
    assert_eq!(flow.page, Page::Lobby);
    assert_eq!(flow.view()["ready"], false);
}

#[test]
fn candidates_require_available_local_key_and_do_not_imply_readiness() {
    use crate::flow::{Flow, Operation, Page};
    use serde_json::json;
    let now = || crate::system::monotonic() + 1.0;
    let mut flow = Flow::lobby(None, true);
    flow.scan(
        flow.generation,
        json!({"rooms":[{"id":"selected","signal":80}]}),
    );
    flow.dispatch("confirm", now());
    let join = flow.request.take().unwrap();
    let id = "f".repeat(32);
    flow.complete(&join,json!({"id":id,"role":"client","context_matches":true,"phase":"joining","step":"selecting_game","candidates":[{"key":"unavailable","name":"Other","detail":"Different content","directory":false,"available":false},{"key":"valid-local-key","name":"Game","detail":"Verified content","directory":false,"available":true}]}),false);
    assert_eq!(flow.page, Page::Candidates);
    assert_eq!(flow.view()["ready"], false);
    flow.dispatch("confirm", now());
    assert!(flow.request.is_none());
    flow.dispatch("down", now());
    flow.dispatch("confirm", now());
    let select = flow.request.take().unwrap();
    assert_eq!(select.operation, Operation::SelectGame);
    assert_eq!(
        select.payload,
        json!({"key":"valid-local-key","session_id":id})
    );
    assert_eq!(flow.page, Page::Joined);
    assert_eq!(flow.view()["ready"], false);
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
fn only_game_loading_entries_can_resume_single_player_on_cancel() {
    use crate::flow::{Flow, Page};
    for supported in [false, true] {
        let mut game = context();
        game.supported = supported;
        for manual in [false, true] {
            let mut flow = Flow::new(Some(game.clone()));
            if manual {
                flow.manual();
            }
            assert_eq!(
                flow.page,
                if supported {
                    Page::Choice
                } else {
                    Page::Unavailable
                }
            );
            assert_eq!(flow.view()["current_game"], true);
            assert_eq!(flow.view()["title"], game.title);
            flow.dispatch("back", crate::system::monotonic() + 1.0);
            assert_eq!(flow.exit_code, Some(if manual { 0 } else { 230 }));
            assert!(flow.request.is_none());
        }
    }
    for connected in [false, true] {
        let mut flow = Flow::lobby(None, connected);
        flow.manual();
        assert_eq!(flow.view()["current_game"], false);
        assert_eq!(flow.view()["network_only"], false);
        flow.dispatch("back", crate::system::monotonic() + 1.0);
        if !connected {
            assert_eq!(flow.page, Page::Lobby);
            assert!(flow.exit_code.is_none());
            flow.dispatch("back", crate::system::monotonic() + 1.0);
        }
        assert_eq!(flow.exit_code, Some(0));
        assert!(flow.request.is_none());
    }
    let mut network = Flow::network();
    network.dispatch("back", crate::system::monotonic() + 1.0);
    assert_eq!(network.exit_code, Some(0));
    assert!(network.request.is_none());
}

#[test]
fn network_completion_can_detach_but_cannot_offer_game_start() {
    use crate::flow::{Flow, FlowExit, Operation, Page};
    for role in ["host", "client"] {
        let mut flow = Flow::network();
        let id = "a".repeat(32);
        flow.status(0, serde_json::json!({"id":id,"purpose":"network_only","phase":"network_connected","role":role,"step":"network_connected","context_matches":true,"connected":true,"ready":false,"peer_present":true}));
        assert_eq!(flow.view()["ready"], false);
        assert!(matches!(flow.page, Page::Host | Page::Joined));
        flow.dispatch("confirm", crate::system::monotonic() + 1.0);
        let detach = flow.request.take().unwrap();
        assert_eq!(detach.operation, Operation::Detach);
        assert_eq!(detach.payload["session_id"], id);
        flow.complete(
            &detach,
            serde_json::json!({"id":id,"connected":true}),
            false,
        );
        assert_eq!(flow.outcome, Some(FlowExit::KeepNetwork));
        assert!(flow.request.is_none());
    }
}

#[test]
fn network_confirmation_and_cancel_are_bound_to_current_session() {
    use crate::flow::{Flow, Operation};
    let mut flow = Flow::network();
    let id = "a".repeat(32);
    flow.status(0, serde_json::json!({"id":id,"role":"host","phase":"hosting","step":"approval_pending","context_matches":true,"connected":false}));
    flow.dispatch("confirm", crate::system::monotonic() + 1.0);
    let approval = flow.request.take().unwrap();
    assert_eq!(approval.operation, Operation::Approve);
    flow.dispatch("back", crate::system::monotonic() + 1.0);
    flow.dispatch("back", crate::system::monotonic() + 1.0);
    flow.complete(
        &approval,
        serde_json::json!({"id":id,"phase":"hosting"}),
        false,
    );
    let cancel = flow.request.take().unwrap();
    assert_eq!(cancel.operation, Operation::Cancel);
    assert_eq!(cancel.payload["session_id"], id);
    assert!(flow.outcome.is_none());
}

#[test]
fn network_advertisements_do_not_claim_game_compatibility() {
    use crate::purpose::SessionPurpose;
    use crate::session::RoomEntry;
    let network =
        RoomEntry::observed("DIRECT-NW-1111111111111", "0c:c6:55:5d:8e:98", 80, "000000").unwrap();
    assert_eq!(network.purpose(), SessionPurpose::NetworkOnly);
    assert!(!network.same_game_hint);
    let game = RoomEntry::observed(
        "DIRECT-NP-aaaaaa-1111111111111",
        "0c:c6:55:5d:8e:98",
        80,
        "aaaaaa",
    )
    .unwrap();
    assert_eq!(game.purpose(), SessionPurpose::Game);
    assert!(game.same_game_hint);
    assert!(RoomEntry::observed("DIRECT-NW-invalid", "0c:c6:55:5d:8e:98", 80, "").is_none());
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
fn direct_lobby_resumes_network_ownership_and_waits_for_recovery() {
    use crate::flow::{Flow, FlowExit, Operation, Page};
    use serde_json::json;
    let now = || crate::system::monotonic() + 1.0;
    for role in ["host", "client"] {
        let id = "a".repeat(32);
        let mut flow = Flow::lobby(None, true);
        flow.status(flow.generation, json!({"id":id,"role":role,"phase":"network_connected","step":"network_connected","network_only":true,"connected":true,"ready":false,"context_matches":true}));
        assert_eq!(flow.view()["network_only"], true);
        assert_eq!(flow.view()["ready"], false);
        flow.dispatch("confirm", now());
        let request = flow.request.take().unwrap();
        assert_eq!(request.operation, Operation::Detach);
        flow.complete(&request, json!({"id":id,"connected":true}), false);
        assert_eq!(flow.outcome, Some(FlowExit::KeepNetwork));
    }
    for matches in [true, false] {
        let id = "b".repeat(32);
        let mut flow = Flow::lobby(None, true);
        flow.status(flow.generation, json!({"id":id,"role":"host","phase":"restoring","step":"leaving","context_matches":matches,"network_restored":false}));
        assert_eq!(flow.page, Page::Leaving);
        flow.dispatch("back", now());
        assert!(flow.request.is_none());
        flow.status(
            flow.generation,
            json!({"id":id,"phase":"closed","network_restored":true,"recovery_errors":[]}),
        );
        assert_eq!(flow.page, Page::Lobby);
        assert_eq!(flow.view()["network_only"], false);
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
    let path = std::path::Path::new("/run/howff/session/game");
    let overrides = crate::game::configuration(path, None).unwrap();
    let original = "savefile_directory = \"/roms/saves\"\ncache_directory = \"/roms/cache\"\nconfig_save_on_exit = \"true\"\nvideo_driver = \"gl\"\n";
    let isolated = crate::game::isolated(original, &overrides);
    assert!(!isolated.contains("/roms/"));
    assert_eq!(isolated.matches("savefile_directory =").count(), 1);
    assert!(isolated.contains("video_driver = \"gl\""));
    assert!(isolated.contains("config_save_on_exit = \"false\""));
    let wrong = std::path::Path::new("/run/session\"\ncache_directory=\"/roms");
    assert!(crate::game::configuration(wrong, None).is_err());
}

#[test]
fn netplay_compression_is_scoped_to_the_inspected_state_frontend() {
    use crate::handheld_link::LinkMode;
    use crate::netplay_compression::{FRONTEND_SHA, eligible, environment};
    let mut compatible = context();
    compatible.frontend_sha256 = FRONTEND_SHA.into();
    for mode in [None, Some(LinkMode::GbDual)] {
        compatible.link_mode = mode;
        assert!(eligible(&compatible));
    }
    compatible.link_mode = Some(LinkMode::GbaAuto);
    assert!(!eligible(&compatible));
    assert!(environment(&compatible).unwrap().is_none());
    compatible.link_mode = None;
    for alteration in 0..4 {
        let mut other = compatible.clone();
        match alteration {
            0 => other.frontend = "retroarch32".into(),
            1 => other.frontend_path = "/opt/retroarch32/bin/retroarch".into(),
            2 => other.frontend_sha256 = "0".repeat(64),
            _ => other.core_elf_bits = 32,
        }
        assert!(!eligible(&other));
        assert!(environment(&other).unwrap().is_none());
    }
}

#[test]
fn ordinary_and_network_controller_initialization_share_the_same_ports() {
    use crate::handheld_link::LinkMode;
    for mode in [None, Some(LinkMode::GbDual), Some(LinkMode::GbaAuto)] {
        let generated =
            crate::game::configuration(std::path::Path::new("/run/game"), mode).unwrap();
        let original = "input_max_users = \"16\"\ninput_libretro_device_p3 = \"1\"\nauto_remaps_enable = \"true\"\ninput_player1_a_btn = \"2\"\nnetplay_check_frames = \"10\"\n";
        let merged = crate::game::isolated(original, &generated);
        let values: std::collections::BTreeMap<_, _> = merged
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.trim(), value.trim().trim_matches('"')))
            .collect();
        let users: usize = values["input_max_users"].parse().unwrap();
        assert_eq!(
            users,
            if mode.is_none() || mode == Some(LinkMode::GbDual) {
                2
            } else {
                1
            }
        );
        assert!(!generated.contains("input_libretro_device_p"));
        let mut context = context();
        context.link_mode = mode;
        for host in [true, false] {
            let args =
                crate::game::frontend_arguments(&context, std::path::Path::new("/run/game"), host);
            let devices: std::collections::BTreeMap<usize, u8> = args
                .iter()
                .filter_map(|arg| arg.strip_prefix("--device="))
                .map(|value| {
                    let (port, device) = value.split_once(':').unwrap();
                    (port.parse().unwrap(), device.parse().unwrap())
                })
                .collect();
            assert_eq!(devices.len(), 16);
            assert_eq!(
                args.iter()
                    .filter(|arg| arg.starts_with("--device="))
                    .count(),
                16
            );
            for port in 1..=16 {
                let handshake = devices[&port];
                let ordinary = if port <= users { handshake } else { 0 };
                assert_eq!(ordinary, handshake);
                assert_eq!(handshake, u8::from(port <= users));
            }
            assert_eq!(args[0], context.frontend_path.to_str().unwrap());
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["-c", "/run/game/base.cfg"])
            );
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["-L", context.core_path.to_str().unwrap()])
            );
            assert!(args.contains(&context.game_path.to_string_lossy().into_owned()));
            assert!(args.contains(&if host { "-H" } else { "--connect=192.168.49.1" }.to_string()));
        }
        assert_eq!(values["auto_remaps_enable"], "false");
        assert_eq!(values["netplay_check_frames"], "600");
        assert_eq!(values["input_player1_a_btn"], "2");
    }
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
        "room_selected",
        "rooms",
        "room_name",
        "ready",
        "busy",
        "refreshing",
        "status",
        "phase",
        "back_label",
        "network_only",
        "connected",
        "local_address",
        "peers",
        "lobby",
        "current_game",
        "entries",
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
fn display_row_keys_follow_identity_across_signal_and_name_changes() {
    let mut flow = crate::flow::Flow::new(Some(context()));
    flow.page = crate::flow::Page::Rooms;
    let mut first = serde_json::json!({"id":"a".repeat(32),"signal":90});
    let second = serde_json::json!({"id":"b".repeat(32),"signal":10});
    flow.scan(
        flow.generation,
        serde_json::json!({"rooms":[first.clone(),second.clone()]}),
    );
    let before = flow.view()["rooms"].as_array().unwrap().clone();
    assert_ne!(before[0]["key"], before[1]["key"]);
    assert_eq!(before[0]["key"].as_str().unwrap().len(), 64);
    first["signal"] = 1.into();
    first["name"] = "Changed label".into();
    flow.scan(flow.generation, serde_json::json!({"rooms":[second,first]}));
    let after = flow.view()["rooms"].as_array().unwrap().clone();
    assert_eq!(before[0]["key"], after[1]["key"]);
    assert_eq!(before[1]["key"], after[0]["key"]);
    assert!(flow.request.is_none());
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

#[test]
fn discovery_actions_preserve_room_selection_and_keep_scanning() {
    use crate::flow::{Flow, Page};
    use serde_json::json;
    let now = || crate::system::monotonic() + 1.0;
    let mut flow = Flow::lobby(None, true);
    let rooms = json!({"rooms":[{"id":"first","signal":80},{"id":"second","signal":60}]});
    flow.scan(flow.generation, rooms.clone());
    flow.dispatch("down", now());
    flow.dispatch("down", now());
    assert_eq!(flow.page, Page::Mode);
    assert!(flow.scans());
    assert_eq!(flow.view()["room_selected"], 1);
    flow.scan(flow.generation, rooms);
    assert_eq!(flow.view()["rooms"].as_array().unwrap().len(), 2);
    flow.dispatch("up", now());
    assert_eq!(flow.page, Page::Lobby);
    assert_eq!(flow.view()["selected"], 1);
    assert!(flow.request.is_none());
}
