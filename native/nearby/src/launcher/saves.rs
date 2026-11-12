use crate::catalog::Entry;
use crate::configuration::Snapshot;
use crate::saves::{Identity, Store, launch};
use crate::{Result, require};
use howff_menu::details::Fact;
use howff_menu::saves::{Document, Request, View};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

enum Work {
    Open(u64, Entry),
    Command(Request),
}
type Reply = (View, Option<String>);
type Channels = (SyncSender<Work>, Receiver<Reply>);

fn session(document: &mut Document) {
    session_document(document, crate::session::state().ok().as_ref());
}

fn session_document(document: &mut Document, state: Option<&crate::session::State>) {
    let state = state.filter(|s| !s.network_released());
    document.room_active = state.is_some();
    document.session = if let Some(state) = state {
        vec![
            Fact::new("房间", state.room.name.clone()),
            Fact::new("状态", state.phase.clone()),
            Fact::new(
                "游戏进度",
                state.root().join("game/saves").to_string_lossy(),
            ),
            Fact::new(
                "即时存档",
                state.root().join("game/states").to_string_lossy(),
            ),
            Fact::new("自动保存", "关闭"),
        ]
    } else {
        vec![
            Fact::new("联机会话", "未在房间中"),
            Fact::new("游戏进度", "随房间创建"),
            Fact::new("即时存档", "由会话管理"),
        ]
    };
}

fn open(entry: &Entry) -> Result<(crate::context::Context, Snapshot, Store)> {
    if crate::session::require_idle().is_err() {
        require(
            !Path::new(crate::configuration::JOURNAL).exists(),
            "配置恢复尚未完成",
        )?;
    }
    let (context, snapshot) = super::configuration::read_save_paths(entry)?;
    let store = Store::new(
        Path::new(crate::saves::ROOT),
        Identity::from_context(&context),
        snapshot.saves.clone(),
        entry.name().into(),
    )?;
    if crate::session::require_idle().is_ok() {
        crate::session::try_while_idle(|| store.recover())?;
    }
    Ok((context, snapshot, store))
}

fn worker(incoming: Receiver<Work>, outgoing: SyncSender<Reply>) {
    let mut current = None;
    let mut view = View::default();
    while let Ok(work) = incoming.recv() {
        let mut launch = None;
        let result = (|| -> Result<()> {
            match work {
                Work::Open(epoch, entry) => {
                    view = View {
                        epoch,
                        ..Default::default()
                    };
                    current = None;
                    current = Some((entry.clone(), open(&entry)?));
                }
                Work::Command(request) if request.epoch() == view.epoch => {
                    let (entry, (context, snapshot, store)) =
                        current.as_mut().ok_or("存档尚未读取")?;
                    match request {
                        Request::Reload { .. } => {
                            let fresh = open(entry)?;
                            *context = fresh.0;
                            *snapshot = fresh.1;
                            *store = fresh.2;
                        }
                        Request::Manage {
                            key,
                            revision,
                            operation,
                            tab,
                            ..
                        } => {
                            view.return_tab = tab;
                            view.return_key = key.clone();
                            let intent = crate::session::try_while_idle(|| {
                                require(
                                    crate::context::parse_selected(
                                        &context.frontend,
                                        &context.argv,
                                        false,
                                    )? == *context,
                                    "游戏或核心已变化",
                                )?;
                                snapshot.graph.unchanged()?;
                                store.operate(&key, &revision, operation)
                            })?;
                            if let Some(intent) = intent {
                                launch = Some(launch::issue(context, store.paths(), intent)?);
                            }
                            view.notice = match operation {
                                howff_menu::saves::Operation::Backup => "备份已完成",
                                howff_menu::saves::Operation::Delete => "存档已删除",
                                howff_menu::saves::Operation::Restore => "存档已恢复",
                                _ => "正在启动游戏",
                            }
                            .into();
                            view.completed = view.completed.wrapping_add(1);
                        }
                        Request::Start { tab, .. } => {
                            view.return_tab = tab;
                            crate::session::try_while_idle(|| {
                                snapshot.graph.unchanged()?;
                                launch = Some(launch::issue(
                                    context,
                                    store.paths(),
                                    launch::Intent::Configured,
                                )?);
                                Ok(())
                            })?;
                        }
                        _ => {}
                    }
                }
                _ => return Ok(()),
            }
            if let Some((_, (_, _, store))) = &mut current {
                view.document = Some(store.scan()?);
            }
            Ok(())
        })();
        if let Some(document) = view.document.as_mut() {
            session(document);
        }
        view.error = result.err().map(|e| e.to_string()).unwrap_or_default();
        view.busy = false;
        if !view.error.is_empty()
            && let Some(id) = launch.take()
        {
            launch::discard(&id);
        }
        if let Err(error) = outgoing.send((view.clone(), launch)) {
            if let Some(id) = error.0.1 {
                launch::discard(&id);
            }
            break;
        }
    }
}

#[derive(Default)]
pub(super) struct Service {
    pub view: View,
    channels: Option<Channels>,
    pending: Option<Work>,
    pub launch: Option<String>,
    pub close_after: bool,
}
impl Service {
    pub fn open(&mut self, entry: Entry) {
        let epoch = self.view.epoch.wrapping_add(1);
        self.view = View {
            epoch,
            busy: true,
            ..Default::default()
        };
        self.close_after = false;
        if self.channels.is_none() {
            let (send, input) = mpsc::sync_channel(1);
            let (output, replies) = mpsc::sync_channel(1);
            match std::thread::Builder::new()
                .name("save-files".into())
                .spawn(move || worker(input, output))
            {
                Ok(_) => self.channels = Some((send, replies)),
                Err(e) => {
                    self.view.busy = false;
                    self.view.error = e.to_string();
                    return;
                }
            }
        }
        self.pending = Some(Work::Open(epoch, entry));
        self.poll();
    }
    pub fn reload(&mut self) {
        self.request(Request::Reload {
            epoch: self.view.epoch,
        });
    }
    pub fn request(&mut self, request: Request) {
        if request.epoch() != self.view.epoch || !request.valid() {
            return;
        }
        if matches!(request, Request::Close { .. }) {
            self.close_after = true;
            if let Some(id) = self.launch.take() {
                launch::discard(&id);
            }
            self.view.notice = "完成当前操作后返回".into();
            return;
        }
        if self.view.busy {
            return;
        }
        self.view.error.clear();
        self.view.notice.clear();
        self.view.busy = true;
        self.pending = Some(Work::Command(request));
        self.poll();
    }
    pub fn poll(&mut self) {
        let Some((send, replies)) = &self.channels else {
            return;
        };
        loop {
            match replies.try_recv() {
                Ok((view, launch)) => {
                    if view.epoch == self.view.epoch {
                        self.view = view;
                        if self.close_after {
                            if let Some(id) = launch {
                                launch::discard(&id);
                            }
                        } else {
                            self.launch = launch;
                        }
                    } else if let Some(id) = launch {
                        launch::discard(&id);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.view.busy = false;
                    self.view.error = "存档任务已结束".into();
                    self.channels = None;
                    return;
                }
            }
        }
        if let Some(work) = self.pending.take() {
            match send.try_send(work) {
                Ok(()) => {}
                Err(TrySendError::Full(work)) => self.pending = Some(work),
                Err(TrySendError::Disconnected(_)) => {
                    self.view.busy = false;
                    self.view.error = "存档任务已结束".into();
                }
            }
        }
    }
    pub fn take_launch(&mut self) -> Option<String> {
        self.launch.take()
    }
    pub fn returned(&mut self, error: Option<String>) {
        self.view.error = error.unwrap_or_default();
        if self.view.error.is_empty() {
            self.reload();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_session_projection_keeps_ownership_until_recovery_is_complete() {
        let mut state: crate::session::State = serde_json::from_value(serde_json::json!({
            "id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "self_mac":"02:00:00:00:00:01",
            "role":"host", "phase":"closed", "stage":"", "step":"", "context_key":[],
            "room":{"id":"room", "ssid":"room", "bssid":"02:00:00:00:00:01", "signal":0, "token":"token", "name":"Room A", "same_game_hint":false},
            "original_uuid":"", "radio_before":"enabled", "services_before":{"NetworkManager.service":{"active":true,"enabled":"enabled"},"wpa_supplicant.service":{"active":true,"enabled":"enabled"}},
            "driver_owned":false, "mounts":[], "deadline":0.0, "claim_deadline":0.0,
            "network_restored":true, "recovery_errors":["NetworkManager"], "timings":[], "stage_started":0.0
        })).unwrap();
        let mut document = Document::default();
        for purpose in [
            crate::purpose::SessionPurpose::Game,
            crate::purpose::SessionPurpose::NetworkOnly,
        ] {
            state.purpose = purpose;
            session_document(&mut document, Some(&state));
            assert!(
                document.room_active,
                "An incomplete release still owns its files"
            );
            assert!(document.session.iter().any(|fact| fact.value == "Room A"));
            assert!(
                document
                    .session
                    .iter()
                    .any(|fact| fact.value == state.root().join("game/saves").to_string_lossy())
            );
        }
        state.recovery_errors.clear();
        session_document(&mut document, Some(&state));
        assert!(!document.room_active);
        assert!(
            document
                .session
                .iter()
                .all(|fact| !fact.value.contains(&state.id))
        );
        state.phase = "hosting".into();
        session_document(&mut document, Some(&state));
        assert!(document.room_active);
        state.phase = "closed".into();
        state.network_restored = false;
        session_document(&mut document, Some(&state));
        assert!(document.room_active);
        session_document(&mut document, None);
        assert!(!document.room_active);
    }
}
