mod nm;

use crate::Result;
use howff_menu::wifi::{Info, Network, Page, Request, Secret, Security, View, valid_ssid};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TryRecvError},
};
use std::thread::JoinHandle;

#[derive(Clone, Default)]
struct Access {
    network: Network,
    profile: Option<String>,
    hidden: bool,
}
#[derive(Clone, Default)]
struct Snapshot {
    radio: bool,
    rows: Vec<Access>,
    info: Option<Info>,
    uuid: Option<String>,
}
trait Driver: Send + 'static {
    fn snapshot(&mut self, refresh: bool) -> Result<Snapshot>;
    fn enable(&mut self) -> Result<Snapshot>;
    fn disconnect(&mut self, target: &Access, forget: bool) -> Result<Snapshot>;
    fn connect(
        &mut self,
        target: &Access,
        secret: &Secret,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u8),
    ) -> Result<Snapshot>;
}
enum Command {
    Scan(bool),
    Enable,
    Connect(Access, Secret),
    Disconnect(Access, bool),
}
enum Reply {
    Stage(u64, u8),
    Done(u64, std::result::Result<Snapshot, String>),
}
struct Worker {
    sender: Option<SyncSender<(u64, Command)>>,
    replies: Receiver<Reply>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    fn new(mut driver: impl Driver) -> std::io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<(u64, Command)>(1);
        let (out, replies) = mpsc::sync_channel(8);
        let cancel = Arc::new(AtomicBool::new(false));
        let stopping = cancel.clone();
        let thread = std::thread::Builder::new()
            .name("nearby-wifi".into())
            .spawn(move || {
                while let Ok((epoch, command)) = requests.recv() {
                    let result = match command {
                        Command::Scan(refresh) => driver.snapshot(refresh),
                        Command::Enable => driver.enable(),
                        Command::Disconnect(target, forget) => driver.disconnect(&target, forget),
                        Command::Connect(target, secret) => {
                            driver.connect(&target, &secret, &stopping, &mut |stage| {
                                let _ = out.try_send(Reply::Stage(epoch, stage));
                            })
                        }
                    }
                    .map_err(|e| e.to_string());
                    if out.send(Reply::Done(epoch, result)).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            replies,
            cancel,
            thread: Some(thread),
        })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            drop(std::mem::replace(&mut self.replies, mpsc::channel().1));
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
pub struct Service {
    pub view: View,
    worker: Option<Worker>,
    rows: Vec<Access>,
    target: Option<Access>,
    running: bool,
    operation: u64,
    refresh_pending: bool,
    info_entry: bool,
    return_to_list: bool,
    cancelled: bool,
    mutating: bool,
    active_connect: bool,
    queued: Option<Command>,
    list_key: Option<String>,
    list_added: bool,
}
impl Service {
    pub fn open(&mut self, info: bool) {
        self.view.epoch = self.view.epoch.wrapping_add(1);
        self.view.selected = 0;
        self.target = None;
        self.view.target.clear();
        self.view.ssid.clear();
        self.info_entry = info;
        self.view.page = if info { Page::Info } else { Page::List };
        self.view.notice.clear();
        self.refresh_pending = !info;
        if self.worker.is_none() {
            match Worker::new(nm::NetworkManager) {
                Ok(worker) => self.worker = Some(worker),
                Err(_) => {
                    self.view.page = Page::Failed;
                    self.view.notice = "无法启动网络任务".into();
                    return;
                }
            }
        }
        self.scan(false);
    }
    fn submit(&mut self, command: Command) {
        if self.running {
            self.queued = Some(command);
            return;
        }
        let Some(worker) = &self.worker else {
            return;
        };
        self.operation = self.operation.wrapping_add(1);
        self.cancelled = false;
        worker.cancel.store(false, Ordering::Relaxed);
        self.mutating = !matches!(command, Command::Scan(_));
        self.active_connect = matches!(command, Command::Connect(..));
        if self.mutating && !self.view.page.pending() {
            self.view.notice = "正在更新网络".into();
        }
        if worker
            .sender
            .as_ref()
            .unwrap()
            .try_send((self.operation, command))
            .is_ok()
        {
            self.running = true;
        }
    }
    fn scan(&mut self, refresh: bool) {
        self.view.scanning = true;
        if self.running {
            self.refresh_pending |= refresh;
            return;
        }
        self.submit(Command::Scan(refresh));
    }
    fn selected_key(&self) -> Option<String> {
        self.view
            .networks
            .get(self.view.selected)
            .map(|r| r.key.clone())
    }
    fn accept(&mut self, snapshot: Snapshot) {
        let selected = if self.view.page == Page::List {
            self.selected_key()
        } else {
            self.list_key.clone()
        };
        self.rows = snapshot.rows;
        self.view.networks = self.rows.iter().map(|r| r.network.clone()).collect();
        self.view.info = snapshot.info;
        self.view.selected = selected
            .and_then(|key| self.view.networks.iter().position(|r| r.key == key))
            .unwrap_or(0);
        if matches!(
            self.view.page,
            Page::List | Page::Off | Page::Empty | Page::Busy
        ) {
            self.view.page = if !snapshot.radio {
                Page::Off
            } else if self.rows.is_empty() {
                Page::Empty
            } else {
                Page::List
            };
        }
    }
    fn remember_list(&mut self) {
        self.list_key = self.selected_key();
        self.list_added = self.view.selected == self.rows.len();
    }
    fn return_list(&mut self) {
        self.view.page = if self.rows.is_empty() {
            Page::Empty
        } else {
            Page::List
        };
        self.view.selected = if self.list_added {
            self.rows.len()
        } else {
            self.list_key
                .as_ref()
                .and_then(|key| self.rows.iter().position(|r| &r.network.key == key))
                .unwrap_or(0)
        };
    }
    pub fn poll(&mut self) {
        loop {
            let result = match self.worker.as_ref().map(|w| w.replies.try_recv()) {
                Some(Ok(reply)) => reply,
                Some(Err(TryRecvError::Disconnected)) => {
                    self.view.notice = "网络任务已结束".into();
                    self.running = false;
                    break;
                }
                _ => break,
            };
            match result {
                Reply::Stage(epoch, stage) if epoch == self.operation => {
                    if stage == 3 {
                        self.view.page = Page::Restoring;
                    } else {
                        self.view.stage = stage;
                    }
                }
                Reply::Done(epoch, result) if epoch == self.operation => {
                    self.running = false;
                    self.mutating = false;
                    self.view.scanning = false;
                    let was_connecting = self.active_connect;
                    match result {
                        Ok(snapshot) => {
                            self.view.notice.clear();
                            self.accept(snapshot);
                            if was_connecting {
                                if let Some(target) = self.target.as_ref() {
                                    self.list_key = self
                                        .rows
                                        .iter()
                                        .find(|row| {
                                            row.network.ssid == target.network.ssid
                                                && row.network.security == target.network.security
                                        })
                                        .map(|row| row.network.key.clone());
                                    self.list_added = false;
                                }
                                self.view.page = Page::Success;
                                self.view.stage = 2;
                            }
                            if self.cancelled {
                                self.return_list();
                                self.view.notice = "已取消连接".into();
                            }
                            if self.return_to_list {
                                self.return_list();
                                self.return_to_list = false;
                            }
                        }
                        Err(error) => {
                            let room_busy = crate::session::state()
                                .is_ok_and(|state| !state.network_released());
                            self.view.notice = if self.cancelled {
                                "已取消连接".into()
                            } else if room_busy {
                                "附近联机正在使用无线网络".into()
                            } else {
                                error
                            };
                            self.view.page = if self.cancelled {
                                self.return_list();
                                self.view.page
                            } else if was_connecting
                                && self
                                    .target
                                    .as_ref()
                                    .is_some_and(|r| r.network.security == Security::Psk)
                            {
                                Page::Password
                            } else if room_busy {
                                Page::Busy
                            } else {
                                Page::Failed
                            };
                        }
                    }
                    if let Some(command) = self.queued.take() {
                        self.submit(command);
                    } else if self.refresh_pending && !self.view.page.pending() {
                        self.refresh_pending = false;
                        self.scan(true);
                    }
                }
                _ => {}
            }
        }
    }
    pub fn title(&self) -> &str {
        match self.view.page {
            Page::Password | Page::Security | Page::Connecting | Page::Details => &self.view.ssid,
            Page::Info => "网络信息",
            Page::Hidden => "添加网络",
            Page::Success => "连接完成",
            Page::Forget => "忘记网络",
            Page::Cancelling => "正在取消",
            Page::Restoring => "正在恢复网络",
            Page::Busy => "当前网络",
            _ => "Wi-Fi 设置",
        }
    }
    fn target(&mut self, index: usize) {
        if let Some(row) = self.rows.get(index).cloned() {
            self.view.target = row.network.key.clone();
            self.view.ssid = row.network.ssid.clone();
            self.target = Some(row);
        }
    }
    fn connect(&mut self, secret: Secret) {
        self.refresh_pending = false;
        if let Some(target) = self.target.clone() {
            self.view.page = Page::Connecting;
            self.view.stage = 0;
            self.view.notice.clear();
            self.submit(Command::Connect(target, secret));
        }
    }
    pub fn credentials(&mut self, request: Request) {
        if request.epoch() != self.view.epoch || !request.valid() {
            return;
        }
        match request {
            Request::Connect { secret, .. } if self.view.page == Page::Password => {
                self.connect(secret)
            }
            Request::Hidden { ssid, .. } if self.view.page == Page::Hidden && valid_ssid(&ssid) => {
                self.target = Some(Access {
                    network: Network {
                        key: "hidden".into(),
                        ssid: ssid.clone(),
                        security: Security::Psk,
                        ..Default::default()
                    },
                    hidden: true,
                    profile: None,
                });
                self.view.ssid = ssid;
                self.view.page = Page::Security;
                self.view.selected = 0;
            }
            _ => {}
        }
    }
    // true means the caller can restore the original Tools action.
    pub fn input(&mut self, action: &str) -> bool {
        let page = self.view.page;
        if self.running && self.mutating && !page.pending() {
            return false;
        }
        if action == "back" {
            if page.pending() {
                if let Some(worker) = &self.worker {
                    worker.cancel.store(true, Ordering::Relaxed);
                }
                self.queued = None;
                self.refresh_pending = false;
                self.cancelled = true;
                if page == Page::Connecting {
                    self.view.page = Page::Cancelling;
                }
                return false;
            }
            if self.running {
                self.refresh_pending = false;
            }
            self.view.epoch = self.view.epoch.wrapping_add(1);
            match page {
                Page::List | Page::Off | Page::Empty | Page::Busy => return true,
                Page::Info if self.info_entry => return true,
                Page::Forget => {
                    self.view.page = Page::Details;
                    self.view.selected = 1;
                }
                Page::Password
                | Page::Hidden
                | Page::Info
                | Page::Details
                | Page::Success
                | Page::Failed => {
                    self.return_list();
                }
                Page::Security => self.view.page = Page::Hidden,
                _ => {}
            }
            return false;
        }
        if page.pending() {
            return false;
        }
        if action == "refresh" && matches!(page, Page::List | Page::Empty) {
            self.scan(true);
            return false;
        }
        if action == "auxiliary" && page == Page::List {
            self.remember_list();
            self.info_entry = false;
            self.view.page = Page::Info;
            self.view.selected = 0;
            return false;
        }
        let count = match page {
            Page::List => self.rows.len() + 1,
            Page::Security | Page::Details | Page::Forget | Page::Failed | Page::Empty => 2,
            _ => 1,
        };
        if matches!(action, "up" | "down" | "left" | "right") {
            let delta = if matches!(action, "up" | "left") {
                -1
            } else {
                1
            };
            self.view.selected =
                (self.view.selected as isize + delta).clamp(0, count as isize - 1) as usize;
            return false;
        }
        if action != "confirm" {
            return false;
        }
        if page == Page::List {
            self.remember_list();
        }
        match page {
            Page::List if self.view.selected == self.rows.len() => {
                self.view.page = Page::Hidden;
                self.view.selected = 0;
                self.view.epoch += 1;
            }
            Page::List => {
                self.target(self.view.selected);
                let target = self.target.as_ref().unwrap();
                if target.network.connected {
                    self.view.page = Page::Details;
                    self.view.selected = 0;
                } else if target.profile.is_some() || target.network.security == Security::Open {
                    self.connect(Secret::default());
                } else if target.network.security == Security::Psk {
                    self.view.page = Page::Password;
                    self.view.selected = 0;
                    self.view.epoch += 1;
                } else {
                    self.view.notice = "此网络需要预先配置认证信息".into();
                }
            }
            Page::Off => self.submit(Command::Enable),
            Page::Empty if self.view.selected == 1 => {
                self.view.page = Page::Hidden;
                self.view.selected = 0;
                self.view.epoch += 1;
            }
            Page::Empty => self.scan(true),
            Page::Security => {
                if self.view.selected == 1 {
                    if let Some(target) = &mut self.target {
                        target.network.security = Security::Open;
                    }
                    self.connect(Secret::default());
                } else {
                    self.view.page = Page::Password;
                    self.view.epoch += 1;
                }
            }
            Page::Details => {
                if self.view.selected == 1 {
                    self.view.page = Page::Forget;
                    self.view.selected = 0;
                } else if let Some(target) = self.target.clone() {
                    self.return_to_list = true;
                    self.submit(Command::Disconnect(target, false));
                }
            }
            Page::Forget if self.view.selected == 1 => {
                if let Some(target) = self.target.clone() {
                    self.return_to_list = true;
                    self.submit(Command::Disconnect(target, true));
                }
            }
            Page::Forget => {
                self.view.page = Page::Details;
                self.view.selected = 1;
            }
            Page::Failed if self.view.selected == 0 => {
                if self.target.is_some() {
                    self.connect(Secret::default());
                } else {
                    self.return_list();
                    self.scan(true);
                }
            }
            Page::Busy => return true,
            Page::Success | Page::Info | Page::Failed => {
                return self.input("back");
            }
            _ => {}
        }
        false
    }
}

pub fn recover(token: &str) -> Result<()> {
    crate::session::recover_wifi(|| nm::recover(token))
}
pub fn pending() -> bool {
    nm::pending()
}
pub fn recover_pending() -> Result<()> {
    if let Some(token) = nm::pending_token()? {
        recover(&token)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    struct Fake {
        calls: Arc<Mutex<Vec<&'static str>>>,
        snapshot: Snapshot,
    }
    impl Driver for Fake {
        fn snapshot(&mut self, _: bool) -> Result<Snapshot> {
            self.calls.lock().unwrap().push("scan");
            std::thread::sleep(Duration::from_millis(50));
            Ok(self.snapshot.clone())
        }
        fn enable(&mut self) -> Result<Snapshot> {
            self.snapshot(false)
        }
        fn disconnect(&mut self, _: &Access, _: bool) -> Result<Snapshot> {
            self.snapshot(false)
        }
        fn connect(
            &mut self,
            target: &Access,
            secret: &Secret,
            cancel: &AtomicBool,
            progress: &mut dyn FnMut(u8),
        ) -> Result<Snapshot> {
            assert_eq!(secret.value(), "'quoted'");
            self.calls.lock().unwrap().push("connect");
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            progress(1);
            let mut snapshot = self.snapshot.clone();
            snapshot.rows[0].network.connected = true;
            snapshot.info = Some(Info {
                ssid: target.network.ssid.clone(),
                address: "192.0.2.1".into(),
                ..Default::default()
            });
            Ok(snapshot)
        }
    }
    fn service() -> (Service, Arc<Mutex<Vec<&'static str>>>) {
        let row = Access {
            network: Network {
                key: "key".into(),
                ssid: "a:b\\'".into(),
                security: Security::Psk,
                ..Default::default()
            },
            ..Default::default()
        };
        let snapshot = Snapshot {
            radio: true,
            rows: vec![row.clone()],
            ..Default::default()
        };
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Service {
                rows: vec![row.clone()],
                view: View {
                    networks: vec![row.network],
                    ..Default::default()
                },
                worker: Some(
                    Worker::new(Fake {
                        calls: calls.clone(),
                        snapshot,
                    })
                    .unwrap(),
                ),
                ..Default::default()
            },
            calls,
        )
    }
    fn finish(service: &mut Service) {
        let end = Instant::now() + Duration::from_secs(2);
        while service.running && Instant::now() < end {
            service.poll();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!service.running);
    }
    #[test]
    fn selecting_during_scan_queues_activation_and_preserves_secret_outside_the_view() {
        let (mut service, calls) = service();
        service.open(false);
        service.input("confirm");
        assert_eq!(service.view.page, Page::Password);
        service.credentials(Request::Connect {
            epoch: service.view.epoch,
            secret: Secret::new("'quoted'".into()),
        });
        finish(&mut service);
        assert_eq!(service.view.page, Page::Success);
        assert_eq!(*calls.lock().unwrap(), vec!["scan", "connect"]);
        let value = serde_json::to_string(&service.view).unwrap();
        assert!(!value.contains("quoted"));
        assert!(!value.contains("secret"));
        service.input("back");
        assert_eq!(service.view.page, Page::List);
        assert!(service.view.networks[0].connected);
    }
    #[test]
    fn repeated_cancel_discards_queued_activation_and_stale_credentials() {
        let (mut service, calls) = service();
        service.open(false);
        service.input("confirm");
        let epoch = service.view.epoch;
        service.credentials(Request::Connect {
            epoch: epoch - 1,
            secret: Secret::new("'quoted'".into()),
        });
        assert!(service.queued.is_none());
        service.credentials(Request::Connect {
            epoch,
            secret: Secret::new("'quoted'".into()),
        });
        service.input("back");
        service.input("back");
        assert_eq!(service.view.page, Page::Cancelling);
        finish(&mut service);
        assert_eq!(service.view.page, Page::List);
        assert_eq!(*calls.lock().unwrap(), vec!["scan"]);
        service.submit(Command::Enable);
        assert_eq!(service.view.notice, "正在更新网络");
        finish(&mut service);
        assert!(service.view.notice.is_empty());
    }
    #[test]
    fn list_focus_tracks_identity_through_keyboard_and_background_reordering() {
        let first = Access {
            network: Network {
                key: "first".into(),
                ssid: "First".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let second = Access {
            network: Network {
                key: "second".into(),
                ssid: "Second".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut service = Service {
            rows: vec![first.clone(), second.clone()],
            view: View {
                networks: vec![first.network.clone(), second.network.clone()],
                selected: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        service.input("confirm");
        assert_eq!(service.view.page, Page::Password);
        service.accept(Snapshot {
            radio: true,
            rows: vec![first.clone(), first, second],
            ..Default::default()
        });
        service.input("back");
        assert_eq!(service.view.selected, 2);
        assert_eq!(service.selected_key().as_deref(), Some("second"));
    }
}
