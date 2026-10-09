use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::content::digest;
use crate::context::{Context, INSTALL};
use crate::menu::Backend;
use crate::protocol::{Profile, Room, VerifiedRoom, hex, mac};
use crate::readiness::ReadyPeer;
use crate::system::{Lock, command, mkdir, monotonic, private_write, random_id, read, write};
use crate::{Result, require};

pub const BASE: &str = "/run/arkos-nearby-rust";
pub const BINARY: &str = "/opt/arkos-nearby/arkos-nearby";
pub use crate::compat::KERNEL_SHA;
pub const DRIVER_SHA: &str = "e099dcd8f6df0e02749d08c4af201bb1d26ca460a369247630782f0888468965";
pub const WPA_SHA: &str = "a874e1df5c862972d3c3220d38a69d043830b10122e23f49462c1d84597b301e";
pub const SERVICES: [&str; 2] = ["NetworkManager.service", "wpa_supplicant.service"];

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
    pub fn observed(ssid: &str, bssid: &str, signal: i64, sha: &str) -> Option<Self> {
        let (game, token) = ssid.strip_prefix("DIRECT-NP-")?.split_once('-')?;
        let bssid = bssid.to_ascii_lowercase();
        if !hex(game, 6) || !hex(token, 13) || !mac(&bssid) {
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
            same_game_hint: sha.starts_with(game),
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
    pub room: RoomEntry,
    pub original_uuid: String,
    pub radio_before: String,
    pub services_before: [bool; 2],
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
        format!("arkos-nearby-rust-{}", self.id)
    }
    pub fn profile(&self) -> String {
        self.unit()
    }
    pub fn closed(&self) -> bool {
        matches!(self.phase.as_str(), "closed" | "failed")
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
    if Path::new(BASE).join("session.json").exists() {
        let state = state()?;
        require(
            state.closed() && state.network_restored && state.recovery_errors.is_empty(),
            "Close the room and finish restoring Wi-Fi before changing pairing or installation",
        )?;
    }
    operation()
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
    let path = Path::new(INSTALL).join("pair.json");
    let meta = path.metadata()?;
    require(
        meta.uid() == 0 && meta.mode() & 0o077 == 0,
        "Wireless credentials must be root-only",
    )?;
    let value: Value = read(&path)?;
    let psk = value["psk"]
        .as_str()
        .ok_or("Missing installed room credential")?;
    require(
        (8..=63).contains(&psk.len()) && psk.bytes().all(|b| b.is_ascii_alphanumeric()),
        "Invalid installed room credential",
    )?;
    Ok(value)
}
pub fn cmd(args: &[&str]) -> Result<String> {
    command(args[0], &args[1..], 40, true)
}
pub fn best(args: &[&str]) -> String {
    command(args[0], &args[1..], 8, false).unwrap_or_default()
}
pub fn task(state: &State, suffix: &str, mode: &str, properties: &[String]) -> Result<()> {
    let mut args = vec![
        "--collect".into(),
        format!("--unit={}{}", state.unit(), suffix),
    ];
    args.extend_from_slice(properties);
    args.extend([BINARY.into(), mode.into(), state.id.clone()]);
    command(
        "systemd-run",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        15,
        true,
    )?;
    Ok(())
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
    let owner = identity(false)?;
    base()?;
    let _lock = Lock::acquire(&Path::new(BASE).join("scan.lock"))?;
    let path = Path::new(BASE).join("scan.json");
    let mut cache = read::<Value>(&path)
        .unwrap_or_else(|_| json!({"rooms":[],"requested_at":0,"observed_at":0}));
    let now = monotonic();
    let mut active = false;
    let result = (|| -> Result<Vec<RoomEntry>> {
        if refresh
            && now - cache["requested_at"].as_f64().unwrap_or(0.0) >= if force { 0.75 } else { 4.0 }
        {
            command(
                "nmcli",
                &["device", "wifi", "rescan", "ifname", "wlan0"],
                4,
                true,
            )?;
            cache["requested_at"] = now.into();
            active = true;
        }
        let rows = command(
            "nmcli",
            &[
                "-t",
                "--escape",
                "no",
                "-f",
                "SSID,BSSID,SIGNAL",
                "device",
                "wifi",
                "list",
                "ifname",
                "wlan0",
                "--rescan",
                "no",
            ],
            4,
            true,
        )?;
        let mut rooms = Vec::new();
        for line in rows.lines() {
            if let Some((ssid, rest)) = line.split_once(':') {
                if let Some((bssid, signal)) = rest.rsplit_once(':') {
                    if let Some(room) = RoomEntry::observed(
                        ssid,
                        bssid,
                        signal.parse().unwrap_or(0),
                        &context.identity.content.sha256,
                    )
                    .filter(|room| room.bssid != owner)
                    {
                        rooms.push(room);
                    }
                }
            }
        }
        rooms.sort_by_key(|room| (-room.signal, room.id.clone()));
        rooms.dedup_by(|left, right| left.id == right.id);
        Ok(rooms)
    })();
    let scanning = scan_activity().unwrap_or(active);
    match result {
        Ok(rooms) => {
            cache["rooms"] = serde_json::to_value(&rooms)?;
            cache["observed_at"] = now.into();
            write(&path, &cache)?;
            Ok(json!({"rooms":rooms,"scanning":scanning}))
        }
        Err(error) => Ok(
            json!({"rooms":if now-cache["requested_at"].as_f64().unwrap_or(0.0)<=9.0 { cache["rooms"].clone() } else {json!([])},"scan_error":error.to_string(),"scanning":scanning}),
        ),
    }
}
fn scan_activity() -> Option<bool> {
    let path = best(&[
        "busctl",
        "call",
        "fi.w1.wpa_supplicant1",
        "/fi/w1/wpa_supplicant1",
        "fi.w1.wpa_supplicant1",
        "GetInterface",
        "s",
        "wlan0",
    ]);
    let interface = path.split('"').nth(1)?;
    let value = best(&[
        "busctl",
        "get-property",
        "fi.w1.wpa_supplicant1",
        interface,
        "fi.w1.wpa_supplicant1.Interface",
        "Scanning",
    ]);
    match value.as_str() {
        "b true" => Some(true),
        "b false" => Some(false),
        _ => None,
    }
}

pub fn begin(role: &str, context: &Context, selected: Option<RoomEntry>) -> Result<Value> {
    let started = monotonic();
    let owner = identity(true)?;
    context.revalidate()?;
    credentials()?;
    let id;
    {
        let _lock = lock()?;
        if let Ok(existing) = state() {
            require(
                existing.recovery_errors.is_empty(),
                "Restore the previous network before creating another room",
            )?;
            if !existing.closed() {
                require(
                    existing.context_key == context.key()
                        && existing.role == role
                        && (role == "host"
                            || selected
                                .as_ref()
                                .is_some_and(|room| room.id == existing.room.id)),
                    "Close the active room before choosing another",
                )?;
                return status(Some(context));
            }
        }
        id = random_id()?;
        let root = Path::new(BASE).join(&id);
        mkdir(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o711))?;
        write(&root.join("context.json"), context)?;
        let original = cmd(&["nmcli", "-g", "GENERAL.CON-UUID", "device", "show", "wlan0"])?;
        let entry = if role == "host" {
            RoomEntry::observed(
                &format!(
                    "DIRECT-NP-{}-{}",
                    &context.identity.content.sha256[..6],
                    &id[..13]
                ),
                &owner,
                100,
                &context.identity.content.sha256,
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
            context_key: context.key(),
            room: entry,
            original_uuid: if original == "--" {
                String::new()
            } else {
                original
            },
            radio_before: best(&["nmcli", "radio", "wifi"]),
            services_before: SERVICES
                .map(|service| best(&["systemctl", "is-active", service]) == "active"),
            driver_owned: false,
            mounts: vec![],
            game_session: if role == "host" {
                Some(id.clone())
            } else {
                None
            },
            deadline: monotonic() + 300.0,
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
        task(
            &state,
            "-recovery",
            "recover",
            &[
                "--on-active=300s".into(),
                "--timer-property=AccuracySec=1s".into(),
                "--setenv=ARKOS_NEARBY_RECOVERY=startup".into(),
            ],
        )?;
        let finalize = format!(
            "--property=ExecStopPost=/usr/bin/systemd-run --collect --unit={}-finalize {BINARY} recover {}",
            state.unit(),
            state.id
        );
        if let Err(error) = task(
            &state,
            "",
            "worker",
            &[
                "--property=RuntimeMaxSec=infinity".into(),
                "--property=TimeoutStopSec=10".into(),
                finalize,
            ],
        ) {
            drop(_lock);
            let _ = recover(&id, "startup_failure");
            return Err(error);
        }
    }
    status(Some(context))
}

pub fn action(name: &str, context: &Context, payload: &Value) -> Result<Value> {
    identity(false)?;
    match name {
        "status" => status(Some(context)),
        "scan" => scan(context, false, false),
        "scan_refresh" => scan(context, true, payload["force"] == true),
        "create" => begin("host", context, None),
        "join" => {
            let selected: RoomEntry = serde_json::from_value(payload["room"].clone())?;
            let actual = RoomEntry::observed(
                &selected.ssid,
                &selected.bssid,
                selected.signal,
                &context.identity.content.sha256,
            )
            .ok_or("Invalid selected room")?;
            require(
                actual.id == selected.id
                    && actual.token == selected.token
                    && actual.bssid != identity(false)?,
                "Selected room identity differs",
            )?;
            begin("client", context, Some(actual))
        }
        "approve" => {
            let id = payload["session_id"]
                .as_str()
                .ok_or("Approval requires the selected room")?;
            let _lock = lock()?;
            let mut state = current(id)?;
            require(
                state.role == "host" && state.context_key == context.key(),
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
            status(Some(context))
        }
        "start" => {
            context.revalidate()?;
            let id = payload["session_id"]
                .as_str()
                .ok_or("Start requires selected session")?;
            let _lock = lock()?;
            let state = current(id)?;
            require(
                state.role == "host" && state.context_key == context.key(),
                "Start belongs to another game or session",
            )?;
            live_peer(&state)?;
            private_write(&state.root().join("start-game"), id.as_bytes())?;
            let mut result = status(Some(context))?;
            result["phase"] = "launching".into();
            result["step"] = "starting".into();
            Ok(result)
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
    context: Context,
    queries: Mutex<()>,
}
impl NativeBackend {
    pub fn shared(context: Context) -> Arc<dyn Backend> {
        Arc::new(Self {
            context,
            queries: Mutex::new(()),
        })
    }
}
impl Backend for NativeBackend {
    fn call(&self, action_name: &str, payload: &Value) -> Result<Value> {
        if matches!(action_name, "scan" | "scan_refresh") {
            let _lock = self.queries.lock().map_err(|_| "Scan lock failed")?;
            action(action_name, &self.context, payload)
        } else {
            action(action_name, &self.context, payload)
        }
    }
    fn failure_snapshot(&self) -> Value {
        status(Some(&self.context)).unwrap_or_else(|_| json!({}))
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
    best(&[
        "systemctl",
        "stop",
        &format!("{}-recovery.timer", state.unit()),
    ]);
    Ok(())
}

pub fn worker(id: &str) -> Result<()> {
    identity(true)?;
    let result = (|| -> Result<()> {
        let mut local = current(id)?;
        let context: Context = read(&local.root().join("context.json"))?;
        let mut radio_control = None;
        if local.role == "host" {
            context.revalidate()?;
            radio_control = Some(crate::radio::host(&local)?);
            crate::game::prepare(&local, &context, None)?;
            crate::radio::services(&local)?;
            progress(
                id,
                Some("hosting"),
                "waiting_peer",
                "房间已创建，等待好友加入。",
            )?;
        } else {
            thread::scope(|scope| -> Result<()> {
                let verification = scope.spawn(|| context.revalidate());
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
            let matched = context.matching(&verified, &profile)?;
            let claim = random_id()?;
            local = update(id, |state| {
                state.game_session = Some(verified.room().session_id.clone());
                state.claim = Some(claim);
            })?;
            write(&local.root().join("context.json"), &matched)?;
            crate::game::prepare(&local, &matched, Some(&verified))?;
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
                    if let Some(peer) = control.enrollee()? {
                        if local.approved_mac.as_deref() != Some(&peer) {
                            update(id, |state| {
                                state.pending_pair_mac = Some(peer.clone());
                                state.pending_pair_kind = Some(PairingRequest::ProvisionDiscovery);
                                state.stage = format!(
                                    "好友 {} 加入，按 A 确认。",
                                    &peer.replace(':', "")[6..]
                                );
                            })?;
                        }
                    }
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
                    require(
                        owner.device_id != identity.public.device_id,
                        "Another handheld has a cloned identity; reset the cloned device",
                    )?;
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
                    let existing = crate::identity::friends()?;
                    require(
                        existing
                            .values()
                            .filter(|friend| friend.last_mac == local.room.bssid)
                            .all(|friend| {
                                friend.identity.device_id == owner.device_id
                                    && friend.identity.public_key == owner.public_key
                            }),
                        "The remembered room owner changed identity; forget and re-pair explicitly",
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
    if local.id != id || local.phase == "closed" || reason == "startup" && local.phase == "playing"
    {
        return status(None);
    }
    require(
        digest(Path::new("/boot/Image"))? == KERNEL_SHA,
        "Recovery kernel identity changed",
    )?;
    if let Ok(owner) = fs::read_to_string("/sys/class/net/wlan0/address") {
        require(
            owner.trim() == local.self_mac,
            "Recovery belongs to another device",
        )?;
    }
    let active = best(&["nmcli", "-g", "GENERAL.CON-UUID", "device", "show", "wlan0"]);
    let profile = best(&[
        "nmcli",
        "-g",
        "GENERAL.CONNECTION",
        "device",
        "show",
        "wlan0",
    ]);
    let address = best(&["ip", "-4", "-o", "address", "show", "dev", "wlan0"]);
    if !active.is_empty()
        && active != "--"
        && profile != local.profile()
        && !address.contains("192.168.49.")
    {
        local.original_uuid = active;
    }
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
    best(&[
        "systemctl",
        "stop",
        &format!("{}-recovery.timer", local.unit()),
    ]);
    if reason != "worker_exit" {
        best(&["systemctl", "stop", &format!("{}.service", local.unit())]);
    }
    if local.role == "client" && local.claim.is_some() {
        let _ = crate::http::request(
            "/unready",
            Some(&json!({"session_id":local.game_session,"claim_id":local.claim})),
        );
    }
    attempt("game and display", crate::game::recover(&local));
    attempt("wireless", crate::radio::restore(&local));
    local.stage = "正在恢复当前网络...".into();
    save(&local)?;
    if !local.original_uuid.is_empty() {
        attempt(
            "original Wi-Fi",
            cmd(&[
                "nmcli",
                "--wait",
                "20",
                "connection",
                "up",
                "uuid",
                &local.original_uuid,
            ])
            .map(|_| ()),
        );
    }
    if local.radio_before == "disabled" {
        attempt(
            "original radio",
            cmd(&["nmcli", "radio", "wifi", "off"]).map(|_| ()),
        );
    }
    let active = best(&["nmcli", "-g", "GENERAL.CON-UUID", "device", "show", "wlan0"]);
    let ipv4 = best(&["ip", "-4", "-o", "address", "show", "dev", "wlan0"]);
    let restored = if local.original_uuid.is_empty() {
        !ipv4.contains("192.168.49.")
            && best(&[
                "nmcli",
                "-g",
                "GENERAL.CONNECTION",
                "device",
                "show",
                "wlan0",
            ]) != local.profile()
    } else {
        active == local.original_uuid
    };
    if !restored {
        errors.push("Original network is not restored".into());
    }
    if let Err(error) = crate::game::restore_menu(&local) {
        errors.push(format!("menu restoration: {error}"));
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
