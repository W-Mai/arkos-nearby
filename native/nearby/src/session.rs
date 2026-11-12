use howff_platform::arkos;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::context::Context;
use crate::menu::Backend;
use crate::protocol::{Profile, Room, VerifiedRoom, hex, mac};
use crate::purpose::{SessionIntent, SessionPurpose};
use crate::readiness::ReadyPeer;
use crate::system::{Lock, command, mkdir, monotonic, private_write, random_id, read, write};
use crate::{Result, require};

pub const BASE: &str = arkos::PATHS.runtime;
pub const BINARY: &str = arkos::application::EXECUTABLE.as_str();
pub use crate::compat::KERNEL_SHA;
use crate::platform::radio::{self as platform_radio, ScanRequest};
pub use crate::platform::radio::{DRIVER_SHA, SERVICES, ServicesBefore, WPA_SHA, task};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoomEntry {
    pub id: String,
    pub ssid: String,
    pub bssid: String,
    pub signal: i64,
    pub token: String,
    pub name: String,
    pub same_game_hint: bool,
}
impl RoomEntry {
    pub fn purpose(&self) -> SessionPurpose {
        if self.ssid.starts_with("DIRECT-NW-") {
            SessionPurpose::NetworkOnly
        } else {
            SessionPurpose::Game
        }
    }
    pub fn observed(ssid: &str, bssid: &str, signal: i64, sha: &str) -> Option<Self> {
        let (game, token) = if let Some(token) = ssid.strip_prefix("DIRECT-NW-") {
            (None, token)
        } else {
            let (game, token) = ssid.strip_prefix("DIRECT-NP-")?.split_once('-')?;
            (Some(game), token)
        };
        let bssid = bssid.to_ascii_lowercase();
        if game.is_some_and(|game| !hex(game, 6)) || !hex(token, 13) || !mac(&bssid) {
            return None;
        }
        Some(Self {
            id: format!("{ssid}|{bssid}"),
            ssid: ssid.into(),
            bssid: bssid.clone(),
            signal: signal.clamp(0, 100),
            token: token.into(),
            name: format!(
                "{} {}",
                bssid.replace(':', "").chars().skip(6).collect::<String>(),
                &token[..4]
            ),
            same_game_hint: game.is_some_and(|game| sha.starts_with(game)),
        })
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PairingRequest {
    ProvisionDiscovery,
    Identity,
}
impl PairingRequest {
    pub fn radio_command(self, peer: &str) -> Option<String> {
        (self == Self::ProvisionDiscovery).then(|| format!("WPS_PBC p2p_dev_addr={peer}"))
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub id: String,
    pub self_mac: String,
    pub role: String,
    pub phase: String,
    pub stage: String,
    pub step: String,
    pub context_key: Value,
    #[serde(default)]
    pub purpose: SessionPurpose,
    pub room: RoomEntry,
    pub original_uuid: String,
    pub radio_before: String,
    pub services_before: ServicesBefore,
    pub driver_owned: bool,
    pub mounts: Vec<PathBuf>,
    pub game_session: Option<String>,
    pub deadline: f64,
    pub claim: Option<String>,
    pub claim_deadline: f64,
    pub game_peer_mac: Option<String>,
    pub error: Option<String>,
    pub end_reason: Option<String>,
    pub network_restored: bool,
    pub recovery_errors: Vec<String>,
    pub timings: Vec<Value>,
    pub stage_started: f64,
    #[serde(default)]
    pub pending_pair_mac: Option<String>,
    #[serde(default)]
    pub pending_pair_kind: Option<PairingRequest>,
    #[serde(default)]
    pub approved_mac: Option<String>,
    #[serde(default)]
    pub original_wps_name: Option<String>,
}
impl State {
    pub fn root(&self) -> PathBuf {
        Path::new(BASE).join(&self.id)
    }
    pub fn unit(&self) -> String {
        platform_radio::unit(&self.id)
    }
    pub fn profile(&self) -> String {
        self.unit()
    }
    pub fn intent(&self) -> Result<SessionIntent> {
        match self.purpose {
            SessionPurpose::Game => Ok(SessionIntent::Game(Box::new(read(
                &self.root().join("context.json"),
            )?))),
            SessionPurpose::NetworkOnly => Ok(SessionIntent::NetworkOnly),
        }
    }
    pub fn established(&self) -> bool {
        match self.purpose {
            SessionPurpose::Game => self.phase == "playing",
            SessionPurpose::NetworkOnly => self.phase == "network_connected",
        }
    }
    pub fn closed(&self) -> bool {
        matches!(self.phase.as_str(), "closed" | "failed")
    }

    pub fn network_released(&self) -> bool {
        self.closed() && self.network_restored && self.recovery_errors.is_empty()
    }
}

fn base() -> Result<()> {
    mkdir(Path::new(BASE))?;
    fs::set_permissions(BASE, fs::Permissions::from_mode(0o711))?;
    Ok(())
}
fn lock() -> Result<Lock> {
    base()?;
    Lock::acquire(&Path::new(BASE).join("session.lock"))
}
pub fn while_idle<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let _lock = lock()?;
    require_idle()?;
    operation()
}

pub(crate) fn try_while_idle<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
    base()?;
    let _lock = Lock::try_acquire(&Path::new(BASE).join("session.lock"))?;
    require_idle()?;
    operation()
}

pub(crate) fn require_idle() -> Result<()> {
    require_room_idle()?;
    require(!crate::wifi::pending(), "普通 Wi-Fi 连接仍待恢复")
}

pub(crate) fn recover_wifi<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
    base()?;
    let _lock = Lock::try_acquire(&Path::new(BASE).join("session.lock"))?;
    require_room_idle()?;
    operation()
}

fn require_room_idle() -> Result<()> {
    if Path::new(BASE).join("session.json").try_exists()? {
        let state = state()?;
        require(
            state.network_released(),
            "Close the room and finish network recovery first",
        )?;
    }
    Ok(())
}
pub fn state() -> Result<State> {
    let state: State = read(&Path::new(BASE).join("session.json"))?;
    require(
        hex(&state.id, 32)
            && crate::compat::valid_mac(&state.self_mac)
            && matches!(state.role.as_str(), "host" | "client"),
        "Invalid local session ownership",
    )?;
    Ok(state)
}
fn save(state: &State) -> Result<()> {
    write(&Path::new(BASE).join("session.json"), state)
}
pub fn current(id: &str) -> Result<State> {
    let state = state()?;
    require(
        hex(id, 32) && state.id == id && !state.closed() && state.phase != "restoring",
        "Session has ended or was replaced",
    )?;
    Ok(state)
}
pub fn update(id: &str, change: impl FnOnce(&mut State)) -> Result<State> {
    let _lock = lock()?;
    let mut state = current(id)?;
    change(&mut state);
    save(&state)?;
    Ok(state)
}
pub fn progress(id: &str, phase: Option<&str>, step: &str, stage: &str) -> Result<State> {
    update(id, |state| {
        state
            .timings
            .push(json!({"stage":state.stage,"ms":(monotonic()-state.stage_started)*1000.0}));
        if state.timings.len() > 64 {
            state.timings.remove(0);
        }
        state.stage_started = monotonic();
        state.stage = stage.into();
        state.step = step.into();
        if let Some(phase) = phase {
            state.phase = phase.into();
        }
    })
}
pub fn identity(force: bool) -> Result<String> {
    crate::compat::identity(force)
}
pub fn credentials() -> Result<Value> {
    let path = arkos::application::PAIRING.path().to_path_buf();
    let meta = path.metadata()?;
    require(
        meta.uid() == 0 && meta.mode() & 0o077 == 0,
        "Wireless credentials must be root-only",
    )?;
    let value: Value = read(&path)?;
    crate::pairing::validate(&value)?;
    Ok(value)
}
pub fn cmd(args: &[&str]) -> Result<String> {
    command(args[0], &args[1..], 40, true)
}
pub fn best(args: &[&str]) -> String {
    command(args[0], &args[1..], 8, false).unwrap_or_default()
}
fn receipt(state: &State) -> Result<ReadyPeer> {
    let value: Value = read(&state.root().join("readiness.json"))?;
    require(
        value["phase"] == "ready" && value["session_id"] == state.id,
        "A live compatible readiness receipt is required",
    )?;
    let peer: ReadyPeer = serde_json::from_value(value["peer"].clone())?;
    require(
        peer.session_id == state.id
            && peer.expires_monotonic.is_finite()
            && peer.expires_monotonic > monotonic(),
        "Peer readiness lease expired",
    )?;
    Ok(peer)
}
pub fn live_peer(state: &State) -> Result<ReadyPeer> {
    let peer = receipt(state)?;
    let observed = crate::readiness::ObservedPeer::resolve(
        &peer.peer_ipv4,
        &fs::read_to_string(state.root().join("leases"))?,
        &cmd(&["iw", "dev", "wlan0", "station", "dump"])?,
        crate::system::wall_time(),
    )?;
    require(
        observed.mac() == peer.peer_mac,
        "Ready peer no longer owns the observed address",
    )?;
    Ok(peer)
}
pub fn status(context: Option<&Context>) -> Result<Value> {
    let state = match state() {
        Ok(state) => state,
        Err(_) => {
            return Ok(json!({"phase":"idle","role":null,"room":null,"ready":false,"step":"idle"}));
        }
    };
    if state.purpose == SessionPurpose::NetworkOnly {
        return crate::network::status(&state, context.is_none());
    }
    let ready = if state.closed() || state.phase == "restoring" {
        false
    } else if state.role == "host" {
        receipt(&state).is_ok()
    } else {
        matches!(state.phase.as_str(), "ready" | "launching" | "playing")
    };
    let present = ready
        || state.role == "host"
            && state.phase == "hosting"
            && read::<Value>(&state.root().join("station-status.json")).is_ok_and(|value| {
                value["peer_present"] == true
                    && monotonic() - value["observed_at"].as_f64().unwrap_or(0.0) < 1.0
            });
    let step = if state.pending_pair_mac.is_some() && state.phase == "hosting" && !ready {
        "approval_pending"
    } else if state.phase == "restoring" {
        "leaving"
    } else if state.closed() {
        if !state.recovery_errors.is_empty() {
            "restore_failed"
        } else if state.error.is_some() {
            "failed"
        } else {
            "closed"
        }
    } else if matches!(state.phase.as_str(), "launching" | "playing") {
        "starting"
    } else if state.phase == "hosting" {
        if ready {
            "host_ready"
        } else if present {
            "peer_preparing"
        } else {
            "waiting_peer"
        }
    } else {
        &state.step
    };
    Ok(
        json!({"id":state.id,"phase":state.phase,"role":state.role,"room":state.room,"ready":ready,"peer_present":present,"step":step,"stage":if present && !ready && state.phase=="hosting" { "设备已连入，正在核对游戏..." } else { &state.stage },"context_matches":context.is_none_or(|c|c.key()==state.context_key),"error":state.error,"end_reason":state.end_reason,"network_restored":state.network_restored,"recovery_errors":state.recovery_errors,"session":state.game_session}),
    )
}

pub fn scan(context: &Context, refresh: bool, force: bool) -> Result<Value> {
    scan_for(
        &SessionIntent::Game(Box::new(context.clone())),
        refresh,
        force,
    )
}
pub fn scan_for(intent: &SessionIntent, refresh: bool, force: bool) -> Result<Value> {
    let sha = intent
        .context()
        .map(|context| context.identity.content.sha256.as_str())
        .unwrap_or("");
    let mut result = scan_observations(sha, refresh, force)?;
    let rooms: Vec<RoomEntry> = serde_json::from_value(result["rooms"].clone())?;
    result["rooms"] = serde_json::to_value(
        rooms
            .into_iter()
            .filter(|room| room.purpose() == intent.purpose())
            .collect::<Vec<_>>(),
    )?;
    Ok(result)
}
fn scan_observations(sha: &str, refresh: bool, force: bool) -> Result<Value> {
    let owner = identity(false)?;
    base()?;
    let _lock = Lock::acquire(&Path::new(BASE).join("scan.lock"))?;
    let path = Path::new(BASE).join("scan.json");
    let mut cache = read::<Value>(&path).unwrap_or_else(|_| json!({"rooms":[]}));
    let now = monotonic();
    let mut result = collect_scan(&mut cache, &owner, sha, refresh, force, now, |request| {
        platform_radio::scan(request)
    })?;
    write(&path, &cache)?;
    result["scanning"] = platform_radio::scan_activity()
        .unwrap_or(result["refresh_accepted"] == true)
        .into();
    Ok(result)
}

fn collect_scan(
    cache: &mut Value,
    owner: &str,
    sha: &str,
    refresh: bool,
    force: bool,
    now: f64,
    mut run: impl FnMut(ScanRequest) -> Result<String>,
) -> Result<Value> {
    let mut result = json!({"refresh_accepted":false});
    if refresh
        && cache["requested_at"]
            .as_f64()
            .is_none_or(|at| now - at >= if force { 0.75 } else { 4.0 })
    {
        // Rejected requests still consume the retry interval; observations are independent.
        cache["requested_at"] = now.into();
        match run(ScanRequest::Refresh) {
            Ok(_) => result["refresh_accepted"] = true.into(),
            Err(error) => result["refresh_error"] = error.to_string().into(),
        }
    }
    let rooms = match run(ScanRequest::List) {
        Ok(rows) => {
            let mut rooms: Vec<_> = rows
                .lines()
                .filter_map(|line| {
                    let (ssid, rest) = line.split_once(':')?;
                    let (bssid, signal) = rest.rsplit_once(':')?;
                    RoomEntry::observed(ssid, bssid, signal.parse().unwrap_or(0), sha)
                        .filter(|room| room.bssid != owner)
                })
                .collect();
            rooms.sort_by_key(|room| (-room.signal, room.id.clone()));
            rooms.dedup_by(|left, right| left.id == right.id);
            rooms
        }
        Err(error) => {
            result["scan_error"] = error.to_string().into();
            if cache["observed_at"]
                .as_f64()
                .is_some_and(|at| now - at <= 9.0)
            {
                serde_json::from_value::<Vec<RoomEntry>>(cache["rooms"].clone())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|room| {
                        RoomEntry::observed(&room.ssid, &room.bssid, room.signal, sha)
                            .filter(|room| room.bssid != owner)
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
    };
    if result["scan_error"].is_null() {
        cache["rooms"] = serde_json::to_value(&rooms)?;
        cache["observed_at"] = now.into();
    }
    result["rooms"] = serde_json::to_value(rooms)?;
    Ok(result)
}

#[cfg(test)]
mod discovery_contracts {
    use super::*;

    const ROW: &str = "DIRECT-NP-aaaaaa-1111111111111:02:aa:bb:cc:dd:01:80";

    #[test]
    fn refresh_failure_cannot_suppress_observations_or_bypass_retry_budget() {
        let mut cache = json!({"rooms":[]});
        let mut requests = Vec::new();
        for (now, force) in [(0.0, false), (0.35, false), (0.75, true), (1.0, true)] {
            let value = collect_scan(
                &mut cache,
                "",
                &"a".repeat(64),
                true,
                force,
                now,
                |request| {
                    requests.push(request);
                    match request {
                        ScanRequest::Refresh => Err("Scanning is already in progress".into()),
                        ScanRequest::List => Ok(ROW.into()),
                    }
                },
            )
            .unwrap();
            assert_eq!(value["rooms"].as_array().unwrap().len(), 1);
            assert!(value["rooms"][0]["same_game_hint"].as_bool().unwrap());
            assert!(value["scan_error"].is_null());
        }
        assert_eq!(
            requests.iter().filter(|r| **r == ScanRequest::List).count(),
            4
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| **r == ScanRequest::Refresh)
                .count(),
            2
        );
    }

    #[test]
    fn refreshes_cannot_extend_observation_lifetime_or_preserve_another_game_hint() {
        let mut cache = json!({"rooms":[]});
        collect_scan(&mut cache, "", &"a".repeat(64), false, false, 10.0, |_| {
            Ok(ROW.into())
        })
        .unwrap();
        for (now, expected) in [(18.0, 1), (19.1, 0), (20.0, 0)] {
            let value = collect_scan(
                &mut cache,
                "",
                &"b".repeat(64),
                true,
                false,
                now,
                |request| match request {
                    ScanRequest::Refresh => Ok(String::new()),
                    ScanRequest::List => Err("No observations available".into()),
                },
            )
            .unwrap();
            assert_eq!(value["rooms"].as_array().unwrap().len(), expected);
            assert_eq!(cache["observed_at"], 10.0);
            if expected != 0 {
                assert_eq!(value["rooms"][0]["same_game_hint"], false);
            }
        }
    }
}
pub fn begin(role: &str, context: &Context, selected: Option<RoomEntry>) -> Result<Value> {
    begin_for(
        role,
        &SessionIntent::Game(Box::new(context.clone())),
        selected,
    )
}
pub fn begin_for(role: &str, intent: &SessionIntent, selected: Option<RoomEntry>) -> Result<Value> {
    require(matches!(role, "host" | "client"), "Invalid room role")?;
    require(
        role == "client" || !matches!(intent, SessionIntent::DiscoverGame(_)),
        "Game discovery is a client intent",
    )?;
    if let Some(room) = &selected {
        require(
            room.purpose() == intent.purpose(),
            "Selected room has another purpose",
        )?;
    }
    let started = monotonic();
    let owner = identity(true)?;
    intent.revalidate()?;
    credentials()?;
    let id;
    {
        let _lock = lock()?;
        require(!crate::wifi::pending(), "普通 Wi-Fi 连接仍待恢复")?;
        if let Ok(existing) = state() {
            require(
                existing.recovery_errors.is_empty(),
                "Restore the previous network before creating another room",
            )?;
            if !existing.closed() {
                require(
                    existing.context_key == intent.key()
                        && existing.role == role
                        && (role == "host"
                            || selected
                                .as_ref()
                                .is_some_and(|room| room.id == existing.room.id)),
                    "Close the active room before choosing another",
                )?;
                return status_for(intent);
            }
        }
        id = random_id()?;
        let root = Path::new(BASE).join(&id);
        mkdir(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o711))?;
        if let Some(context) = intent.context() {
            write(&root.join("context.json"), context)?;
        }
        if matches!(intent, SessionIntent::DiscoverGame(_)) {
            write(&root.join("discover-game.json"), &true)?;
            write(&root.join("origin-game.json"), &intent.context())?;
        }
        let before = platform_radio::capture()?;
        let entry = if role == "host" {
            RoomEntry::observed(
                &match intent.context() {
                    Some(context) => format!(
                        "DIRECT-NP-{}-{}",
                        &context.identity.content.sha256[..6],
                        &id[..13]
                    ),
                    None => format!("DIRECT-NW-{}", &id[..13]),
                },
                &owner,
                100,
                intent
                    .context()
                    .map(|c| c.identity.content.sha256.as_str())
                    .unwrap_or(""),
            )
            .unwrap()
        } else {
            selected.ok_or("Choose a specific room before joining")?
        };
        let state = State {
            id: id.clone(),
            self_mac: owner,
            role: role.into(),
            phase: "starting".into(),
            step: if role == "host" {
                "creating"
            } else {
                "connecting"
            }
            .into(),
            stage: "正在准备无线连接...".into(),
            context_key: intent.key(),
            purpose: intent.purpose(),
            room: entry,
            original_uuid: before.original_uuid,
            radio_before: before.radio_before,
            services_before: before.services_before,
            driver_owned: false,
            mounts: vec![],
            game_session: if role == "host" && intent.purpose() == SessionPurpose::Game {
                Some(id.clone())
            } else {
                None
            },
            deadline: monotonic() + platform_radio::STARTUP_SECONDS as f64,
            claim: None,
            claim_deadline: 0.0,
            game_peer_mac: None,
            error: None,
            end_reason: None,
            network_restored: false,
            recovery_errors: vec![],
            timings: vec![json!({"stage":"入口核对与会话准备","ms":(monotonic()-started)*1000.0})],
            stage_started: monotonic(),
            pending_pair_mac: None,
            pending_pair_kind: None,
            approved_mac: None,
            original_wps_name: None,
        };
        save(&state)?;
        platform_radio::arm(&state)?;
        if let Err(error) = platform_radio::start_worker(&state) {
            drop(_lock);
            let _ = recover(&id, "startup_failure");
            return Err(error);
        }
    }
    status_for(intent)
}

pub fn action(name: &str, context: &Context, payload: &Value) -> Result<Value> {
    action_for(
        name,
        &SessionIntent::Game(Box::new(context.clone())),
        payload,
    )
}
pub fn status_for(intent: &SessionIntent) -> Result<Value> {
    let mut value = status(intent.context())?;
    if let Ok(state) = state() {
        value["network_only"] = (state.purpose == SessionPurpose::NetworkOnly).into();
        if let Ok(summary) = read::<Value>(&state.root().join("room-summary.json")) {
            value["game_title"] = summary["title"].clone();
            value["game_core"] = summary["core"].clone();
        }
        if state.purpose == SessionPurpose::Game {
            if let Ok(context) = read::<Context>(&state.root().join("context.json")) {
                value["game_core"] = context.core_label().into();
                value["game_core_origin"] = json!(context.origin());
            }
        }
        value["context_matches"] =
            (state.purpose == intent.purpose() && state.context_key == intent.key()).into();
        if matches!(state.step.as_str(), "resolving_game" | "selecting_game") {
            let candidates: Vec<crate::catalog::matching::Candidate> =
                read(&state.root().join("candidates.json")).unwrap_or_default();
            value["candidates"] = json!(candidates.iter().map(|candidate| json!({"key":candidate.key,"name":candidate.name,"detail":candidate.detail,"available":candidate.context.is_some(),"directory":false})).collect::<Vec<_>>());
            if let Ok(error) = read::<Value>(&state.root().join("match-error.json")) {
                value["stage"] = error["detail"].clone();
            }
        }
    }
    Ok(value)
}
pub fn action_for(name: &str, intent: &SessionIntent, payload: &Value) -> Result<Value> {
    identity(false)?;
    match name {
        "status" => status_for(intent),
        "scan" => scan_for(intent, false, false),
        "scan_refresh" => scan_for(intent, true, payload["force"] == true),
        "create" => begin_for("host", intent, None),
        "select_game" => {
            let id = payload["session_id"]
                .as_str()
                .ok_or("Selection requires the current session")?;
            let _lock = lock()?;
            let mut state = current(id)?;
            require(
                state.context_key == intent.key(),
                "Selection belongs to another join attempt",
            )?;
            crate::catalog::matching::select(
                &state,
                payload["key"].as_str().ok_or("Select a local candidate")?,
            )?;
            state.step = "checking".into();
            state.stage = "正在准备所选游戏...".into();
            save(&state)?;
            status_for(intent)
        }
        "join" => {
            let selected: RoomEntry = serde_json::from_value(payload["room"].clone())?;
            let actual = RoomEntry::observed(
                &selected.ssid,
                &selected.bssid,
                selected.signal,
                intent
                    .context()
                    .map(|c| c.identity.content.sha256.as_str())
                    .unwrap_or(""),
            )
            .ok_or("Invalid selected room")?;
            require(
                actual.id == selected.id
                    && actual.token == selected.token
                    && actual.bssid != identity(false)?,
                "Selected room identity differs",
            )?;
            begin_for("client", intent, Some(actual))
        }
        "approve" => {
            let id = payload["session_id"]
                .as_str()
                .ok_or("Approval requires the selected room")?;
            let _lock = lock()?;
            let mut state = current(id)?;
            require(
                state.role == "host" && state.context_key == intent.key(),
                "Approval belongs to another game",
            )?;
            let peer = state
                .pending_pair_mac
                .clone()
                .ok_or("No current device request")?;
            require(mac(&peer), "Invalid observed pairing address")?;
            if let Some(command) = state
                .pending_pair_kind
                .ok_or("No current pairing request type")?
                .radio_command(&peer)
            {
                let mut control = crate::radio::Control::new(&state)?;
                require(
                    control.request(&command)? == "OK",
                    "The host could not enable selected-device provisioning",
                )?;
            }
            state.approved_mac = Some(peer);
            state.pending_pair_mac = None;
            state.pending_pair_kind = None;
            state.stage = "正在连接好友...".into();
            save(&state)?;
            status_for(intent)
        }
        "start" => {
            let context = intent
                .context()
                .ok_or("Network-only rooms cannot start games")?;
            context.revalidate()?;
            let id = payload["session_id"]
                .as_str()
                .ok_or("Start requires selected session")?;
            let _lock = lock()?;
            let state = current(id)?;
            require(
                state.role == "host" && state.context_key == intent.key(),
                "Start belongs to another game or session",
            )?;
            live_peer(&state)?;
            private_write(&state.root().join("start-game"), id.as_bytes())?;
            let mut result = status_for(intent)?;
            result["phase"] = "launching".into();
            result["step"] = "starting".into();
            Ok(result)
        }
        "detach" => {
            let id = payload["session_id"]
                .as_str()
                .ok_or("Choose the connected session")?;
            let _lock = lock()?;
            let state = current(id)?;
            require(
                intent.purpose() == SessionPurpose::NetworkOnly
                    && state.purpose == SessionPurpose::NetworkOnly
                    && state.established(),
                "A connected network-only session is required",
            )?;
            crate::network::require_live(&state)?;
            status_for(intent)
        }
        "cancel" => {
            let id = payload["session_id"]
                .as_str()
                .ok_or("Cancel requires selected session")?;
            require(
                hex(id, 32) && state()?.id == id,
                "Cancel belongs to another session",
            )?;
            recover(id, "cancel")
        }
        _ => Err("Unknown fixed room action".into()),
    }
}

pub struct NativeBackend {
    intent: SessionIntent,
    queries: Mutex<()>,
}

/// Manual discovery retains the selected intent through cancellation and recovery.
pub struct DiscoveryBackend {
    origin: Option<Context>,
    intent: Mutex<SessionIntent>,
    queries: Mutex<()>,
}

impl DiscoveryBackend {
    pub fn shared(origin: Option<Context>) -> Arc<dyn Backend> {
        let intent = state()
            .ok()
            .filter(|state| !state.closed() || !state.network_restored)
            .and_then(|state| {
                if state.purpose == SessionPurpose::NetworkOnly {
                    Some(SessionIntent::NetworkOnly)
                } else if state.root().join("discover-game.json").is_file() {
                    let preferred =
                        read::<Option<Box<Context>>>(&state.root().join("origin-game.json"))
                            .ok()
                            .flatten();
                    Some(SessionIntent::DiscoverGame(preferred))
                } else {
                    read::<Context>(&state.root().join("context.json"))
                        .ok()
                        .map(|context| SessionIntent::Game(Box::new(context)))
                }
            })
            .unwrap_or_else(|| SessionIntent::DiscoverGame(origin.clone().map(Box::new)));
        Arc::new(Self {
            origin,
            intent: Mutex::new(intent),
            queries: Mutex::new(()),
        })
    }
}

impl Backend for DiscoveryBackend {
    fn call(&self, action: &str, payload: &Value) -> Result<Value> {
        if matches!(action, "scan" | "scan_refresh") {
            let _lock = self.queries.lock().map_err(|_| "Scan lock failed")?;
            return scan_observations(
                self.origin
                    .as_ref()
                    .map(|context| context.identity.content.sha256.as_str())
                    .unwrap_or(""),
                action == "scan_refresh",
                payload["force"] == true,
            );
        }
        let intent = {
            let mut intent = self
                .intent
                .lock()
                .map_err(|_| "Discovery state lock failed")?;
            if action == "join" {
                let room: RoomEntry = serde_json::from_value(payload["room"].clone())?;
                *intent = if room.purpose() == SessionPurpose::NetworkOnly {
                    SessionIntent::NetworkOnly
                } else {
                    SessionIntent::DiscoverGame(self.origin.clone().map(Box::new))
                };
            } else if action == "create" {
                *intent = self
                    .origin
                    .clone()
                    .map(|context| SessionIntent::Game(Box::new(context)))
                    .unwrap_or(SessionIntent::NetworkOnly);
            }
            intent.clone()
        };
        let mut value = action_for(action, &intent, payload)?;
        value["network_only"] = (intent.purpose() == SessionPurpose::NetworkOnly).into();
        Ok(value)
    }
    fn failure_snapshot(&self) -> Value {
        self.intent
            .lock()
            .ok()
            .and_then(|intent| status_for(&intent).ok())
            .unwrap_or_else(|| json!({}))
    }
}
impl NativeBackend {
    pub fn shared(context: Context) -> Arc<dyn Backend> {
        DiscoveryBackend::shared(Some(context))
    }
    pub fn for_intent(intent: SessionIntent) -> Arc<dyn Backend> {
        Arc::new(Self {
            intent,
            queries: Mutex::new(()),
        })
    }
}
impl Backend for NativeBackend {
    fn call(&self, action_name: &str, payload: &Value) -> Result<Value> {
        if matches!(action_name, "scan" | "scan_refresh") {
            let _lock = self.queries.lock().map_err(|_| "Scan lock failed")?;
            action_for(action_name, &self.intent, payload)
        } else {
            action_for(action_name, &self.intent, payload)
        }
    }
    fn failure_snapshot(&self) -> Value {
        status_for(&self.intent).unwrap_or_else(|_| json!({}))
    }
}

pub fn native_connected(state: &State) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = fs::File::open(state.root().join("game/game.log")) else {
        return false;
    };
    let Ok(meta) = file.metadata() else {
        return false;
    };
    let _ = file.seek(SeekFrom::Start(meta.len().saturating_sub(262144)));
    let mut bytes = Vec::new();
    if file.take(262144).read_to_end(&mut bytes).is_err() {
        return false;
    }
    native_admission(&state.role, &bytes)
}

fn native_admission(role: &str, bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    if role == "host" {
        text.lines().any(|line| {
            line.contains("ArkOS_peer")
                && (line.to_ascii_lowercase().contains("joined as player 2")
                    || line.contains("已加入为玩家「2")
                    || line.contains("已加入为玩家 2"))
        })
    } else if role == "client" {
        text.to_ascii_lowercase()
            .contains("you have joined as player 2")
            || text.contains("您已作为玩家「2")
            || text.contains("您已作为玩家 2")
    } else {
        false
    }
}

fn confirmed(state: &State) -> Result<()> {
    let peer = if state.role == "host" {
        Some(live_peer(state)?.peer_mac)
    } else {
        None
    };
    update(&state.id, |state| {
        state.phase = "playing".into();
        state.stage = "双方已连接，游戏中。".into();
        state.game_peer_mac = peer;
    })?;
    platform_radio::disarm(state);
    Ok(())
}

pub fn observe_pairing(state: &State, control: &mut crate::radio::Control) -> Result<()> {
    if let Some(peer) = control.enrollee()? {
        if state.approved_mac.as_deref() != Some(&peer) {
            update(&state.id, |state| {
                state.pending_pair_mac = Some(peer.clone());
                state.pending_pair_kind = Some(PairingRequest::ProvisionDiscovery);
                state.stage = format!("好友 {} 加入，按 A 确认。", &peer.replace(':', "")[6..]);
            })?;
        }
    }
    Ok(())
}

pub fn worker(id: &str) -> Result<()> {
    identity(true)?;
    let result = (|| -> Result<()> {
        let mut local = current(id)?;
        if local.purpose == SessionPurpose::NetworkOnly {
            return crate::network::worker(&local);
        }
        let deferred = local.root().join("discover-game.json").is_file();
        let context: Option<Context> = if deferred {
            read(&local.root().join("context.json")).ok()
        } else {
            Some(read(&local.root().join("context.json"))?)
        };
        let mut radio_control = None;
        if local.role == "host" {
            let context = context.as_ref().ok_or("Host requires a selected game")?;
            progress(id, None, "creating", "正在核对游戏与联机核心...")?;
            let (context, settings) = crate::arcade::host(&local, context)?;
            write(&local.root().join("context.json"), &context)?;
            crate::game::prepare(&local, &context, None, &settings)?;
            progress(id, None, "creating", "正在准备无线连接...")?;
            radio_control = Some(crate::radio::host(&local)?);
            crate::radio::services(&local)?;
            progress(
                id,
                Some("hosting"),
                "waiting_peer",
                "房间已创建，等待好友加入。",
            )?;
        } else {
            thread::scope(|scope| -> Result<()> {
                let verification = scope.spawn(|| {
                    if deferred {
                        Ok(())
                    } else {
                        context
                            .as_ref()
                            .ok_or("Client requires a selected game")?
                            .revalidate()
                    }
                });
                let connection = crate::radio::join(&local);
                let verified = verification
                    .join()
                    .map_err(|_| "Local preparation failed")?;
                connection?;
                verified
            })?;
            progress(id, Some("joining"), "checking", "连接成功，正在核对游戏...")?;
            let room = Room::decode(&crate::http::request("/room", None)?)?;
            let verified = VerifiedRoom::observe(room, &local.room.bssid, &local.room.token)?;
            let profile: Profile =
                serde_json::from_slice(&crate::http::request("/handheld-profile", None)?)?;
            profile.validate(&verified)?;
            write(
                &local.root().join("room-summary.json"),
                &json!({"title":verified.room().content.title,"core":verified.room().core.name}),
            )?;
            let matched = if deferred {
                crate::catalog::matching::resolve(&local, &verified, &profile, context.as_ref())?
            } else {
                context
                    .as_ref()
                    .ok_or("Client requires a selected game")?
                    .matching(&verified, &profile)?
            };
            let claim = random_id()?;
            local = update(id, |state| {
                state.game_session = Some(verified.room().session_id.clone());
                state.claim = Some(claim);
            })?;
            write(&local.root().join("context.json"), &matched)?;
            if crate::arcade::requires_probe(&matched) {
                progress(id, None, "checking", "正在检查街机 ROM 与 BIOS...")?;
            }
            let settings = crate::arcade::client(&local, &matched)?;
            crate::game::prepare(&local, &matched, Some(&verified), &settings)?;
            progress(id, None, "notifying", "游戏核对完成，正在通知房主...")?;
        }
        let mut launched = false;
        let mut ready_at = 0.0;
        let mut checked = 0.0;
        let mut failures = 0;
        let mut missing_since = None;
        let mut native_ready = false;
        loop {
            local = current(id)?;
            let now = monotonic();
            require(
                local.phase == "playing" || now < local.deadline,
                "Room startup expired before native connection",
            )?;
            if local.role == "host" {
                if let Some(control) = &mut radio_control {
                    observe_pairing(&local, control)?;
                }
                require(
                    best(&[
                        "systemctl",
                        "is-active",
                        &format!("{}-http.service", local.unit()),
                    ]) == "active",
                    "Room service stopped",
                )?;
                if !launched && local.root().join("start-game").is_file() {
                    require(
                        fs::read_to_string(local.root().join("start-game"))? == id,
                        "Stale game marker",
                    )?;
                    live_peer(&local)?;
                    crate::game::start(&local)?;
                    launched = true;
                    fs::remove_file(local.root().join("start-game"))?;
                    progress(id, Some("launching"), "starting", "正在打开联机游戏...")?;
                }
                if let Some(owner) = &local.game_peer_mac {
                    if !best(&["iw", "dev", "wlan0", "station", "dump"])
                        .to_ascii_lowercase()
                        .contains(&format!("station {owner} "))
                    {
                        let since = missing_since.get_or_insert(now);
                        if now - *since >= 3.0 {
                            update(id, |state| state.end_reason = Some("peer_left".into()))?;
                            break;
                        }
                    } else {
                        missing_since = None;
                    }
                }
            } else {
                if now - checked >= 1.0 {
                    match crate::http::request("/room-status", None)
                        .and_then(|bytes| Ok(serde_json::from_slice::<Value>(&bytes)?))
                    {
                        Ok(status)
                            if status["session_id"].as_str() == local.game_session.as_deref() =>
                        {
                            failures = 0;
                            native_ready = status["native_ready"] == true;
                        }
                        _ => failures += 1,
                    }
                    checked = now;
                    if failures >= 2 {
                        update(id, |state| state.end_reason = Some("host_closed".into()))?;
                        break;
                    }
                }
                if failures == 0 && local.phase != "playing" && now - ready_at >= 8.0 {
                    let context: Context = read(&local.root().join("context.json"))?;
                    context.revalidate()?;
                    let challenge: Value = serde_json::from_slice(&crate::http::request(
                        "/identity-challenge",
                        Some(&json!({"session_id":local.game_session,"claim_id":local.claim})),
                    )?)?;
                    let owner: crate::identity::PublicIdentity =
                        serde_json::from_value(challenge["identity"].clone())?;
                    let identity = crate::identity::load()?;
                    crate::identity::check_owner(&identity.public, &owner, &local.room.bssid)?;
                    require(
                        challenge["session_id"].as_str() == local.game_session.as_deref()
                            && challenge["claim_id"].as_str() == local.claim.as_deref()
                            && challenge["peer_mac"] == local.self_mac
                            && challenge["owner_mac"] == local.room.bssid,
                        "Identity response belongs to another connection",
                    )?;
                    let nonce = challenge["challenge"]
                        .as_str()
                        .ok_or("No identity challenge")?;
                    let message = crate::identity::transcript(
                        local.game_session.as_deref().unwrap(),
                        local.claim.as_deref().unwrap(),
                        nonce,
                        &local.self_mac,
                        &local.room.bssid,
                    )?;
                    owner.verify(
                        &[
                            b"owner\0".as_slice(),
                            message.as_slice(),
                            challenge["transport_key"]
                                .as_str()
                                .ok_or("No owner transport key")?
                                .as_bytes(),
                        ]
                        .concat(),
                        challenge["signature"]
                            .as_str()
                            .ok_or("No owner signature")?,
                    )?;
                    let sent = monotonic();
                    let ephemeral = crate::handshake::Ephemeral::new()?;
                    let transport = ephemeral.public()?;
                    let key =
                        ephemeral.agree(challenge["transport_key"].as_str().unwrap(), &message)?;
                    let (encryption_nonce, encrypted) = crate::handshake::seal(
                        &key,
                        &message,
                        credentials()?["psk"].as_str().unwrap(),
                    )?;
                    let signed = [
                        message.as_slice(),
                        transport.as_bytes(),
                        encryption_nonce.as_bytes(),
                        encrypted.as_bytes(),
                        serde_json::to_vec(&context.identity)?.as_slice(),
                    ]
                    .concat();
                    let value = json!({"session_id":local.game_session,"claim_id":local.claim,"game":context.identity,"identity":identity.public,"challenge":nonce,"signature":identity.sign(&signed),"transport_key":transport,"encryption_nonce":encryption_nonce,"encrypted_credential":encrypted});
                    let receipt: Value =
                        serde_json::from_slice(&crate::http::request("/ready", Some(&value))?)?;
                    if receipt["approved"] == false {
                        progress(id, None, "checking", "等待房主确认加入...")?;
                        thread::sleep(Duration::from_millis(250));
                        continue;
                    }
                    require(
                        receipt["compatible"] == true
                            && receipt["session_id"].as_str() == local.game_session.as_deref()
                            && receipt["claim_id"].as_str() == local.claim.as_deref()
                            && receipt["lease_seconds"] == 30,
                        "Host rejected prepared game readiness",
                    )?;
                    if !crate::identity::known(&owner)? {
                        let key = fs::read_to_string(local.root().join("provisioned-key"))?;
                        crate::identity::remember(owner, local.room.bssid.clone(), Some(key))?;
                    }
                    update(id, |state| state.claim_deadline = sent + 30.0)?;
                    ready_at = monotonic();
                    if !launched {
                        progress(
                            id,
                            Some("ready"),
                            "waiting_start",
                            "对方已确认准备完成，等待房主开始。",
                        )?;
                    }
                }
                if !launched && failures == 0 && native_ready {
                    crate::game::start(&current(id)?)?;
                    launched = true;
                    progress(id, Some("launching"), "starting", "正在打开联机游戏...")?;
                }
            }
            if launched {
                let game: Value = read(&local.root().join("game/game.json"))?;
                if !matches!(game["phase"].as_str(), Some("armed" | "running")) {
                    break;
                }
                if local.phase != "playing" && native_connected(&local) {
                    confirmed(&local)?;
                }
            }
            thread::sleep(Duration::from_millis(if local.phase == "playing" {
                500
            } else {
                200
            }));
        }
        Ok(())
    })();
    if let Err(error) = &result {
        let _ = update(id, |state| state.error = Some(error.to_string()));
    }
    let recovery = recover(id, "worker_exit");
    result?;
    recovery?;
    Ok(())
}

pub fn recover(id: &str, reason: &str) -> Result<Value> {
    require(
        unsafe { libc::geteuid() } == 0 && hex(id, 32),
        "Invalid recovery authority",
    )?;
    let _lock = lock()?;
    let mut local = state()?;
    if local.id != id || local.phase == "closed" || reason == "startup" && local.established() {
        return status(None);
    }
    platform_radio::validate_recovery(&local)?;
    platform_radio::refresh_original(&mut local);
    let played = local.phase == "playing";
    local.phase = "restoring".into();
    local.step = "leaving".into();
    local.stage = "正在结束本次联机...".into();
    if local.end_reason.is_none() {
        local.end_reason = Some(
            if reason == "cancel" {
                "canceled"
            } else if played {
                "game_ended"
            } else {
                reason
            }
            .into(),
        );
    }
    save(&local)?;
    let mut errors = Vec::new();
    let mut attempt = |name: &str, result: Result<()>| {
        if let Err(error) = result {
            errors.push(format!("{name}: {error}"));
        }
    };
    platform_radio::quiesce(&local, reason);
    if local.purpose == SessionPurpose::Game {
        attempt("arcade load check", crate::arcade::stop(&local));
        if local.role == "client" && local.claim.is_some() {
            let _ = crate::http::request(
                "/unready",
                Some(&json!({"session_id":local.game_session,"claim_id":local.claim})),
            );
        }
        attempt("game and display", crate::game::recover(&local));
    } else {
        crate::network::leave(&local);
    }
    attempt("wireless", crate::radio::restore(&local));
    local.stage = "正在恢复当前网络...".into();
    save(&local)?;
    let connection = platform_radio::restore_connection(&local);
    errors.extend(connection.errors);
    let restored = connection.restored;
    if local.purpose == SessionPurpose::Game {
        if let Err(error) = crate::game::restore_menu(&local) {
            errors.push(format!("menu restoration: {error}"));
        }
    }
    local.network_restored = restored && errors.is_empty();
    local.recovery_errors = errors;
    local.phase = if local.recovery_errors.is_empty() {
        "closed"
    } else {
        "failed"
    }
    .into();
    local.step = if local.network_restored {
        "closed"
    } else {
        "restore_failed"
    }
    .into();
    local.stage = if local.network_restored {
        "本次联机已结束，原网络已恢复。"
    } else {
        "网络恢复未完成，请重试退出。"
    }
    .into();
    save(&local)?;
    status(None)
}

#[cfg(test)]
mod native_admission_tests {
    use super::native_admission;

    #[test]
    fn network_ownership_is_released_only_after_complete_recovery_for_both_purposes() {
        let mut state: super::State = serde_json::from_value(serde_json::json!({
            "id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "self_mac":"02:00:00:00:00:01",
            "role":"host", "phase":"closed", "stage":"", "step":"", "context_key":[],
            "room":{"id":"room", "ssid":"room", "bssid":"02:00:00:00:00:01", "signal":0, "token":"token", "name":"room", "same_game_hint":false},
            "original_uuid":"", "radio_before":"enabled", "services_before":{"NetworkManager.service":{"active":true,"enabled":"enabled"},"wpa_supplicant.service":{"active":true,"enabled":"enabled"}},
            "driver_owned":false, "mounts":[], "deadline":0.0, "claim_deadline":0.0,
            "network_restored":true, "recovery_errors":[], "timings":[], "stage_started":0.0
        })).unwrap();
        for purpose in [
            super::SessionPurpose::Game,
            super::SessionPurpose::NetworkOnly,
        ] {
            state.purpose = purpose;
            for phase in [
                "created",
                "joining",
                "waiting",
                "ready",
                "playing",
                "network_connected",
                "restoring",
                "restore_failed",
                "closed",
                "failed",
                "unrecognized",
            ] {
                state.phase = phase.into();
                for restored in [false, true] {
                    state.network_restored = restored;
                    for errors in [vec![], vec!["NetworkManager".to_string()]] {
                        state.recovery_errors = errors;
                        assert_eq!(
                            state.network_released(),
                            matches!(phase, "closed" | "failed")
                                && restored
                                && state.recovery_errors.is_empty()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn unrelated_log_encoding_cannot_hide_the_current_native_admission() {
        let mut host = b"content.zip#\xb3\xac\xbc\xb6\n".to_vec();
        host.extend_from_slice(
            "[INFO] [Netplay] 「ArkOS_peer」已加入为玩家「2」 (ping: 14 ms)\n".as_bytes(),
        );
        assert!(native_admission("host", &host));
        let mut client = b"content.zip#\xb3\xac\xbc\xb6\n".to_vec();
        client
            .extend_from_slice("[INFO] [Netplay] 您已作为玩家「2」加入 (ping: 12 ms)\n".as_bytes());
        assert!(native_admission("client", &client));
        assert!(!native_admission("host", &client));
        assert!(!native_admission(
            "host",
            b"content\xff\n[INFO] [Netplay] connected"
        ));
        assert!(!native_admission("other", &host));
    }
}
