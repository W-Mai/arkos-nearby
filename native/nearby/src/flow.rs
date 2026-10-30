use std::collections::BTreeMap;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::context::Context;
use crate::protocol::hex;
use crate::system::monotonic;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Lobby,
    Candidates,
    Mode,
    Choice,
    Host,
    Rooms,
    Joined,
    Leaving,
    Unavailable,
}
impl Page {
    pub fn name(self) -> &'static str {
        match self {
            Self::Lobby => "lobby",
            Self::Candidates => "candidates",
            Self::Mode => "mode",
            Self::Choice => "choice",
            Self::Host => "host",
            Self::Rooms => "rooms",
            Self::Joined => "joined",
            Self::Leaving => "leaving",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    SelectGame,
    Detach,
    Create,
    Join,
    Start,
    Cancel,
    Approve,
}
impl Operation {
    pub fn name(self) -> &'static str {
        match self {
            Self::SelectGame => "select_game",
            Self::Detach => "detach",
            Self::Create => "create",
            Self::Join => "join",
            Self::Start => "start",
            Self::Cancel => "cancel",
            Self::Approve => "approve",
        }
    }
}

#[derive(Clone)]
pub struct Request {
    pub token: u64,
    pub operation: Operation,
    pub payload: Value,
}
struct Record {
    room: Value,
    seen: f64,
    available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowExit {
    CurrentGame,
    NetworkOnly,
    SelectGame,
    KeepNetwork,
}

pub struct Flow {
    lobby: bool,
    candidate: usize,
    pub outcome: Option<FlowExit>,
    network_only: bool,
    pub context: Option<Context>,
    pub page: Page,
    pub generation: u64,
    pub input_epoch: f64,
    pub exit_code: Option<i32>,
    pub request: Option<Request>,
    pub refresh_requested: bool,
    pub phase: String,
    choice: usize,
    selected: Option<String>,
    rooms: BTreeMap<String, Record>,
    target: Option<Value>,
    state: Value,
    message: String,
    operation: Option<Operation>,
    request_id: u64,
    session_id: Option<String>,
    pending_back: Option<Option<Page>>,
    join_authorized: bool,
    peer_seen: bool,
    peer_left: bool,
    scanning: Option<bool>,
    refreshing_until: f64,
    single_player: bool,
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn truth(value: &Value, key: &str) -> bool {
    value[key].as_bool() == Some(true)
}
fn errors(value: &Value) -> bool {
    value["recovery_errors"]
        .as_array()
        .is_some_and(|v| !v.is_empty())
}
pub fn label(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn room_name(value: &Value) -> String {
    let token = value["token"]
        .as_str()
        .or_else(|| value["session_id"].as_str())
        .unwrap_or("");
    let prefix: String = token.chars().take(4).collect();
    if prefix.is_empty() {
        "房间".into()
    } else {
        format!("房间 {}", prefix.to_ascii_uppercase())
    }
}

impl Flow {
    pub fn new(context: Option<Context>) -> Self {
        let page = if context.as_ref().is_some_and(|context| context.supported) {
            Page::Choice
        } else {
            Page::Unavailable
        };
        let unavailable = "暂时无法核对当前游戏，按 B 继续单机。";
        Self {
            lobby: false,
            candidate: 0,
            outcome: None,
            network_only: false,
            context,
            page,
            generation: 0,
            input_epoch: monotonic(),
            exit_code: None,
            request: None,
            refresh_requested: false,
            phase: "idle".into(),
            choice: 0,
            selected: None,
            rooms: BTreeMap::new(),
            target: None,
            state: json!({"phase":"idle"}),
            message: if page == Page::Unavailable {
                unavailable
            } else {
                "请选择创建房间或加入房间。"
            }
            .into(),
            operation: None,
            request_id: 0,
            session_id: None,
            pending_back: None,
            join_authorized: false,
            peer_seen: false,
            peer_left: false,
            scanning: None,
            refreshing_until: 0.0,
            single_player: true,
        }
    }
    pub fn modes() -> Self {
        let mut flow = Self::network();
        flow.page = Page::Mode;
        flow.message = "请选择仅组网或选择游戏。".into();
        flow
    }
    pub fn lobby(context: Option<Context>, connected: bool) -> Self {
        let mut flow = Self::new(context);
        flow.lobby = true;
        flow.single_player = false;
        flow.set_page(if connected { Page::Lobby } else { Page::Mode });
        flow.message = "选择附近房间，或向下创建自己的房间。".into();
        flow
    }
    pub fn scans(&self) -> bool {
        matches!(self.page, Page::Rooms | Page::Lobby) || self.lobby && self.page == Page::Mode
    }
    pub fn resume_lobby(&mut self) {
        self.outcome = None;
        self.exit_code = None;
        self.network_only = false;
        self.set_page(Page::Lobby);
        self.refresh_requested = true;
    }
    fn entry_page(&self) -> bool {
        self.page == Page::Choice || self.lobby && matches!(self.page, Page::Lobby | Page::Mode)
    }
    pub fn network() -> Self {
        let mut flow = Self::new(None);
        flow.network_only = true;
        flow.single_player = false;
        flow.page = Page::Choice;
        flow.message = "建立局域网连接，供游戏或应用使用。".into();
        flow
    }
    pub fn manual(&mut self) {
        self.single_player = false;
        if self.page == Page::Unavailable {
            self.message = "这款游戏暂时无法联机，按 B 返回游戏列表。".into();
        }
    }
    fn set_page(&mut self, page: Page) {
        self.page = page;
        self.generation += 1;
        self.input_epoch = monotonic();
        self.scanning = None;
        self.refreshing_until = if matches!(page, Page::Rooms | Page::Lobby) {
            monotonic() + 3.0
        } else {
            0.0
        };
        self.refresh_requested = matches!(page, Page::Rooms | Page::Lobby);
    }
    fn order(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.rooms.keys().cloned().collect();
        ids.sort_by_key(|id| {
            let record = &self.rooms[id];
            (
                !record.available,
                -(record.room["signal"].as_i64().unwrap_or(0)),
                id.clone(),
            )
        });
        ids
    }
    fn begin(&mut self, operation: Operation, mut payload: Value) {
        if self.operation.is_some() {
            return;
        }
        self.generation += 1;
        self.request_id += 1;
        self.operation = Some(operation);
        if matches!(
            operation,
            Operation::Start | Operation::Cancel | Operation::Approve | Operation::Detach
        ) {
            payload = json!({"session_id": self.session_id});
        }
        if operation == Operation::SelectGame {
            payload["session_id"] = json!(self.session_id);
        }
        if matches!(operation, Operation::Create | Operation::Join) {
            self.state = json!({"phase":"starting","role":if operation == Operation::Create { "host" } else { "client" },"ready":false});
            self.session_id = None;
            self.peer_seen = false;
            self.peer_left = false;
        }
        self.phase = match operation {
            Operation::SelectGame => "checking",
            Operation::Detach => "network_connected",
            Operation::Create => "creating",
            Operation::Join => "connecting",
            Operation::Start => "starting",
            Operation::Cancel => "leaving",
            Operation::Approve => "peer_preparing",
        }
        .into();
        self.message = match operation {
            Operation::SelectGame => "正在准备所选游戏...",
            Operation::Create => "正在创建当前游戏的房间...",
            Operation::Join => "正在加入所选房间并核对游戏...",
            Operation::Start => "正在开始游戏...",
            Operation::Cancel => "正在结束本次联机并恢复原网络...",
            Operation::Approve => "正在连接好友...",
            Operation::Detach => "保持网络连接，正在返回系统...",
        }
        .into();
        self.request = Some(Request {
            token: self.request_id,
            operation,
            payload,
        });
    }
    fn restored(&self) -> bool {
        string(&self.state, "phase") == "closed"
            && truth(&self.state, "network_restored")
            && !errors(&self.state)
    }
    fn finish_leave(&mut self) {
        let destination = self.pending_back.take().unwrap_or(Some(Page::Choice));
        self.session_id = None;
        self.join_authorized = false;
        self.target = None;
        self.state = json!({"phase":"idle","ready":false});
        self.phase = "idle".into();
        self.peer_seen = false;
        self.peer_left = false;
        if self.lobby {
            self.network_only = false;
        }
        if let Some(page) = destination {
            self.set_page(page);
            self.message = if page == Page::Rooms {
                "已离开房间，可选择其他房间。"
            } else {
                "请选择创建房间或加入房间。"
            }
            .into();
        } else {
            self.exit_code = Some(if self.single_player { 230 } else { 0 });
        }
    }
    fn back(&mut self) {
        if self.pending_back.is_some() {
            if matches!(self.phase.as_str(), "restore_failed" | "room_active")
                && self.operation.is_none()
            {
                self.begin(Operation::Cancel, json!({}));
            }
            return;
        }
        if self.page == Page::Unavailable {
            self.exit_code = Some(if self.single_player { 230 } else { 0 });
            return;
        }
        let destination = match self.page {
            Page::Lobby => None,
            Page::Mode if self.lobby => Some(Page::Lobby),
            Page::Joined | Page::Candidates if self.lobby => Some(Page::Lobby),
            Page::Choice | Page::Mode => None,
            Page::Joined | Page::Candidates => Some(Page::Rooms),
            _ => Some(Page::Choice),
        };
        self.pending_back = Some(destination);
        if self.operation.is_none() && (self.session_id.is_none() || self.restored()) {
            self.finish_leave();
            return;
        }
        self.join_authorized = false;
        self.set_page(Page::Leaving);
        self.phase = "leaving".into();
        if self.operation.is_none() {
            self.begin(Operation::Cancel, json!({}));
        } else {
            self.message = "正在取消当前操作，随后恢复原网络...".into();
        }
    }
    pub fn dispatch(&mut self, action: &str, received: f64) {
        if received < self.input_epoch || self.exit_code.is_some() {
            return;
        }
        if action == "back" {
            self.back();
            return;
        }
        if self.pending_back.is_some() {
            if action == "confirm"
                && matches!(self.phase.as_str(), "restore_failed" | "room_active")
                && self.operation.is_none()
            {
                self.begin(Operation::Cancel, json!({}));
            }
            return;
        }
        if action == "refresh" {
            if self.scans() && self.operation.is_none() {
                self.refresh_requested = true;
                self.scanning = None;
                self.refreshing_until = monotonic() + 3.0;
                self.message = "正在刷新房间列表...".into();
            }
            return;
        }
        if self.operation.is_some() || self.page == Page::Unavailable {
            return;
        }
        if self.lobby && self.page == Page::Mode && matches!(action, "up" | "page_up") {
            if action == "up" && self.choice == 2 {
                self.choice = 0;
                return;
            }
            self.set_page(Page::Lobby);
            return;
        }
        if self.lobby && self.page == Page::Mode && self.context.is_some() && action == "down" {
            self.choice = 2;
            return;
        }
        if self.page == Page::Lobby
            && (action == "page_down"
                || action == "down" && self.order().last() == self.selected.as_ref())
        {
            self.set_page(Page::Mode);
            return;
        }
        if self.page == Page::Candidates {
            let candidates = self.state["candidates"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            match action {
                "up" => self.candidate = self.candidate.saturating_sub(1),
                "down" => {
                    self.candidate = (self.candidate + 1).min(candidates.len().saturating_sub(1))
                }
                "confirm" => {
                    if let Some(candidate) = candidates
                        .get(self.candidate)
                        .filter(|candidate| candidate["available"] == true)
                    {
                        let key = candidate["key"].clone();
                        self.set_page(Page::Joined);
                        self.begin(Operation::SelectGame, json!({"key":key}));
                    }
                }
                _ => {}
            }
            return;
        }
        let direction = match action {
            "up" | "left" | "page_up" => -1,
            "down" | "right" | "page_down" => 1,
            "confirm" => 0,
            _ => return,
        };
        if direction != 0 {
            if matches!(self.page, Page::Choice | Page::Mode) {
                self.choice = (self.choice as i32 + direction).clamp(
                    0,
                    if self.lobby && self.context.is_some() {
                        2
                    } else {
                        1
                    },
                ) as usize;
            } else if self.scans() && !self.rooms.is_empty() {
                let ids = self.order();
                let index = ids
                    .iter()
                    .position(|id| Some(id) == self.selected.as_ref())
                    .unwrap_or(0);
                let distance = if action.starts_with("page_") { 3 } else { 1 };
                self.selected = Some(
                    ids[(index as i32 + direction * distance).clamp(0, ids.len() as i32 - 1)
                        as usize]
                        .clone(),
                );
            }
            return;
        }
        if self.phase == "leaving" {
            return;
        }
        if matches!(self.phase.as_str(), "closed" | "failed") {
            self.back();
            return;
        }
        match self.page {
            Page::Mode => {
                self.outcome = Some(if self.choice == 2 {
                    FlowExit::CurrentGame
                } else if self.choice == 0 {
                    FlowExit::NetworkOnly
                } else {
                    FlowExit::SelectGame
                });
                self.exit_code = Some(0);
            }
            Page::Host | Page::Joined
                if self.network_only
                    && self.phase == "network_connected"
                    && truth(&self.state, "connected") =>
            {
                self.begin(Operation::Detach, json!({}));
            }
            Page::Choice if self.choice == 0 => {
                self.set_page(Page::Host);
                self.begin(Operation::Create, json!({}));
            }
            Page::Choice => {
                self.phase = "idle".into();
                self.message = "正在搜索附近房间，请选择后按 A 加入。".into();
                self.set_page(Page::Rooms);
            }
            Page::Host if self.phase == "approval_pending" => {
                self.begin(Operation::Approve, json!({}))
            }
            Page::Host if truth(&self.state, "ready") && self.state["context_matches"] != false => {
                self.begin(Operation::Start, json!({}))
            }
            Page::Rooms | Page::Lobby => {
                let room = self.selected.as_ref().and_then(|id| self.rooms.get(id));
                if let Some(record) = room.filter(|record| record.available) {
                    let room = record.room.clone();
                    self.target = Some(room.clone());
                    self.join_authorized = true;
                    self.set_page(Page::Joined);
                    self.begin(Operation::Join, json!({"room":room}));
                } else {
                    self.message = "该房间已离线，请等待刷新或选择其他房间。".into();
                }
            }
            _ => {}
        }
    }
    pub fn scan(&mut self, generation: u64, result: Value) {
        if generation != self.generation || !self.scans() {
            return;
        }
        let now = monotonic();
        let rows = result["rooms"].as_array().cloned().unwrap_or_default();
        let mut present = std::collections::BTreeSet::new();
        for room in &rows {
            if let Some(id) = room["id"].as_str() {
                present.insert(id.to_owned());
                self.rooms.insert(
                    id.into(),
                    Record {
                        room: room.clone(),
                        seen: now,
                        available: true,
                    },
                );
            }
        }
        for (id, record) in &mut self.rooms {
            if !present.contains(id) {
                record.available = false;
            }
        }
        self.expire();
        if self.selected.is_none() {
            self.selected = self.order().first().cloned();
        }
        self.scanning = result["scanning"].as_bool();
        self.message = if result["scan_error"].is_string() {
            "搜索暂时不可用，请稍候重试。"
        } else if rows.is_empty() {
            "正在搜索附近房间，请稍候。"
        } else {
            "选择房间后按 A 加入。"
        }
        .into();
    }
    pub fn expire(&mut self) {
        let now = monotonic();
        self.rooms.retain(|id, record| {
            if now - record.seen > 9.0 {
                record.available = false;
            }
            record.available || now - record.seen <= 15.0 || Some(id) == self.selected.as_ref()
        });
    }
    pub fn status(&mut self, generation: u64, value: Value) {
        if generation != self.generation {
            return;
        }
        self.accept_status(value);
    }
    fn accept_status(&mut self, value: Value) {
        let id = string(&value, "id").to_owned();
        let backend_phase = string(&value, "phase");
        if backend_phase == "restoring"
            && self.entry_page()
            && self.session_id.is_none()
            && self.operation.is_none()
            && hex(&id, 32)
        {
            self.session_id = Some(id.clone());
            self.pending_back = Some(Some(if self.lobby {
                Page::Lobby
            } else {
                Page::Choice
            }));
            self.set_page(Page::Leaving);
        }
        if self.pending_back.is_some() && self.session_id.as_deref() == Some(&id) {
            self.state = value;
            if self.operation.is_none() && self.restored() {
                self.finish_leave();
            } else if string(&self.state, "phase") == "restoring" {
                self.phase = "leaving".into();
                self.message = string(&self.state, "stage").into();
            } else if errors(&self.state) {
                self.phase = "restore_failed".into();
                self.message = "原网络尚未恢复，按 A 重试退出。".into();
            }
            return;
        }
        if value["context_matches"] == false {
            if self.session_id.is_none()
                && self.operation.is_none()
                && self.entry_page()
                && !matches!(backend_phase, "idle" | "closed")
                && hex(&id, 32)
            {
                self.session_id = Some(id);
                self.pending_back = Some(Some(if self.lobby {
                    Page::Lobby
                } else {
                    Page::Choice
                }));
                self.set_page(Page::Leaving);
                self.phase = if backend_phase == "restoring" {
                    "leaving"
                } else if errors(&value) {
                    "restore_failed"
                } else {
                    "room_active"
                }
                .into();
                self.message = if backend_phase == "restoring" {
                    string(&value, "stage")
                } else {
                    "上一场联机尚未结束，按 A 关闭后继续。"
                }
                .into();
                self.state = value;
            }
            return;
        }
        if self.session_id.is_none() {
            if self.operation.is_some()
                || !self.entry_page()
                || matches!(backend_phase, "idle" | "closed")
                || backend_phase == "failed" && !errors(&value)
                || !hex(&id, 32)
                || !matches!(string(&value, "role"), "host" | "client")
            {
                return;
            }
            self.session_id = Some(id.clone());
            self.join_authorized = string(&value, "role") == "client";
            self.set_page(if self.join_authorized {
                Page::Joined
            } else {
                Page::Host
            });
        }
        if self.session_id.as_deref() != Some(&id) {
            return;
        }
        self.state = value;
        if self.state["network_only"].is_boolean() {
            self.network_only = truth(&self.state, "network_only");
        }
        if self.pending_back.is_some() {
            if self.operation.is_none() && self.restored() {
                self.finish_leave();
            } else if string(&self.state, "phase") == "restoring" {
                self.message = string(&self.state, "stage").into();
            }
            return;
        }
        self.phase = if self.operation == Some(Operation::Start) {
            "starting"
        } else {
            string(&self.state, "step")
        }
        .into();
        if self.phase == "restore_failed" {
            self.pending_back = Some(Some(
                if matches!(self.page, Page::Joined | Page::Candidates) {
                    if self.lobby { Page::Lobby } else { Page::Rooms }
                } else if self.lobby {
                    Page::Lobby
                } else {
                    Page::Choice
                },
            ));
            self.join_authorized = false;
            self.set_page(Page::Leaving);
            self.message = "原网络尚未恢复，按 A 重试退出。".into();
            return;
        }
        if truth(&self.state, "peer_present") || truth(&self.state, "ready") {
            self.peer_seen = true;
            self.peer_left = false;
        } else if self.peer_seen && self.phase == "waiting_peer" {
            self.peer_left = true;
        }
        self.message = string(&self.state, "stage").into();
        if self.phase == "selecting_game" && self.page == Page::Joined && self.operation.is_none() {
            self.candidate = 0;
            self.set_page(Page::Candidates);
        } else if self.page == Page::Candidates
            && !matches!(self.phase.as_str(), "selecting_game" | "resolving_game")
        {
            self.set_page(Page::Joined);
        }
        if self.phase == "host_ready" {
            self.message = "对方已准备好，按 A 开始游戏。".into();
        } else if self.peer_left && self.phase == "waiting_peer" {
            self.message = "好友已离开，房间仍在等待重新加入。".into();
        } else if matches!(self.phase.as_str(), "closed" | "failed") {
            self.join_authorized = false;
            if string(&self.state, "end_reason") == "host_closed" {
                self.message = "房主已关闭房间，原网络已恢复。".into();
            }
        }
        if string(&self.state, "phase") == "playing"
            && string(&self.state, "role") == "client"
            && self.join_authorized
        {
            self.exit_code = Some(0);
        }
    }
    pub fn complete(&mut self, request: &Request, result: Value, failed: bool) {
        if request.token != self.request_id || self.operation != Some(request.operation) {
            return;
        }
        self.operation = None;
        self.generation += 1;
        if let Some(id) = result["id"].as_str().filter(|id| hex(id, 32)) {
            if matches!(request.operation, Operation::Create | Operation::Join) {
                self.session_id = Some(id.into());
            }
            if Some(id) == self.session_id.as_deref() {
                self.state = result;
            }
        }
        if self.pending_back.is_some() && request.operation != Operation::Cancel {
            if self.session_id.is_some() {
                self.begin(Operation::Cancel, json!({}));
            } else {
                self.finish_leave();
            }
            return;
        }
        if request.operation == Operation::Cancel {
            if failed || !self.restored() {
                self.phase = "restore_failed".into();
                self.message = "退出尚未完成，按 A 重试恢复网络。".into();
            } else {
                self.finish_leave();
            }
        } else if failed {
            self.phase = "failed".into();
            self.join_authorized = false;
            self.message = match request.operation {
                Operation::Create => "创建失败，请稍后再试。",
                Operation::Join if self.network_only => "加入失败，请确认房间仍在等待连接。",
                Operation::Join => "加入失败，请确认双方游戏一致后重试。",
                _ => "暂时无法开始，请等待对方准备好后重试。",
            }
            .into();
        } else if request.operation == Operation::Detach {
            self.outcome = Some(FlowExit::KeepNetwork);
            self.exit_code = Some(0);
        } else if request.operation == Operation::Start {
            self.exit_code = Some(0);
        } else {
            self.accept_status(self.state.clone());
        }
    }
    pub fn view(&self) -> Value {
        let joined_game = self.session_id.is_some() && !self.network_only;
        let title = if self.network_only {
            "仅组网".into()
        } else if joined_game && self.state["game_title"].is_string() {
            label(string(&self.state, "game_title"))
        } else {
            self.context
                .as_ref()
                .map(|context| label(&context.title))
                .unwrap_or_else(|| "附近的掌机".into())
        };
        let core = if joined_game && self.state["game_core"].is_string() {
            label(string(&self.state, "game_core"))
        } else {
            self.context
                .as_ref()
                .map(|context| context.core_label())
                .unwrap_or_default()
        };
        let ids = self.order();
        let rows: Vec<_> = if self.scans() {
            ids.iter().map(|id| { let record = &self.rooms[id]; json!({"key":format!("{:x}", Sha256::digest(id.as_bytes())),"name":room_name(&record.room),"detail":if string(&record.room,"ssid").starts_with("DIRECT-NW-") { "仅组网 · 保持局域网连接" } else if truth(&record.room,"same_game_hint") { "当前游戏 · 加入后核对" } else { "游戏房间 · 加入后查找本地游戏" },"signal":record.room["signal"].as_i64().unwrap_or(0).clamp(0,100),"available":record.available}) }).collect()
        } else {
            vec![]
        };
        let selected = if self.page == Page::Candidates {
            self.candidate.min(3)
        } else if matches!(self.page, Page::Choice | Page::Mode) {
            self.choice
        } else {
            ids.iter()
                .position(|id| Some(id) == self.selected.as_ref())
                .unwrap_or(0)
        };
        let room = if matches!(self.page, Page::Joined | Page::Leaving) {
            self.target.as_ref().unwrap_or(&self.state["room"])
        } else {
            &self.state["room"]
        };
        let back = if self.page == Page::Lobby {
            if self.context.is_some() {
                "单机游玩"
            } else {
                "退出"
            }
        } else if self.network_only && self.phase == "network_connected" {
            "断开并恢复 Wi-Fi"
        } else if self.page == Page::Mode || self.network_only && self.page == Page::Choice {
            "返回"
        } else if self.phase == "room_active" {
            "关闭房间"
        } else if self.phase == "restore_failed" {
            "重试退出"
        } else if self.pending_back.is_some() {
            "请稍候"
        } else if self.page == Page::Choice {
            if self.single_player {
                "单机游玩"
            } else {
                "换游戏"
            }
        } else if matches!(self.page, Page::Rooms | Page::Unavailable)
            || matches!(self.phase.as_str(), "closed" | "failed")
        {
            "返回"
        } else if self.page == Page::Host {
            "关闭房间"
        } else if self.phase == "waiting_start" {
            "离开房间"
        } else {
            "取消加入"
        };
        json!({"lobby":self.lobby,"current_game":self.context.is_some(),"entries":if self.page == Page::Candidates { self.state["candidates"].as_array().cloned().unwrap_or_default().into_iter().skip(self.candidate.saturating_sub(3)).take(4).collect::<Vec<_>>() } else { vec![] },"network_only":self.network_only,"local_address":self.state["local_address"].as_str().unwrap_or(""),"peers":self.state["peers"].as_array().cloned().unwrap_or_default(),"connected":truth(&self.state,"connected"),"page":self.page.name(),"title":title,"core":core,"selected":selected,"room_selected":ids.iter().position(|id| Some(id) == self.selected.as_ref()),"rooms":rows,"room_name":if room.is_null() { String::new() } else { room_name(room) },"ready":truth(&self.state,"ready") && self.state["context_matches"] != false && self.pending_back.is_none() && matches!(self.phase.as_str(),"host_ready"|"waiting_start"|"starting"),"busy":self.operation.is_some(),"refreshing":self.scans() && self.scanning.unwrap_or(monotonic()<self.refreshing_until),"status":label(&self.message),"phase":self.phase,"back_label":back})
    }
}
