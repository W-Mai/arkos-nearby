use crate::{Result, require};
use nearby_menu::management::{Device, Fact, Home, Operation, Request, UpdatePhase, View};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

type Reply = std::result::Result<(View, Option<String>), String>;
struct Worker {
    sender: SyncSender<(Request, View)>,
    replies: Receiver<(u64, Reply)>,
}

#[derive(Default)]
pub(super) struct Service {
    pub view: View,
    pub token: Option<String>,
    worker: Option<Worker>,
    sequence: u64,
    pending: Option<Request>,
}

fn refresh(view: &mut View) -> Result<()> {
    view.room_busy = crate::session::require_idle().is_err();
    let local = crate::identity::load()?.public;
    let summary = crate::identity::summary()?;
    view.device = Device {
        id: local.device_id,
        name: local.name,
        address: String::new(),
    };
    view.friends = summary
        .as_array()
        .into_iter()
        .flatten()
        .map(|value| Device {
            id: value["device_id"].as_str().unwrap_or_default().into(),
            name: value["name"].as_str().unwrap_or_default().into(),
            address: value["last_mac"].as_str().unwrap_or_default().into(),
        })
        .collect();
    view.version = env!("CARGO_PKG_VERSION").into();
    view.profile = crate::compat::PROFILE.into();
    view.home = match crate::home::Preference::load()?.default_home {
        crate::home::DefaultHome::Original => Home::Original,
        crate::home::DefaultHome::Nearby => Home::Nearby,
    };
    view.origin = match crate::home::foreground_origin()? {
        crate::home::ForegroundOrigin::BorrowedEs => "从原系统打开",
        crate::home::ForegroundOrigin::StandaloneUnit => "独立运行",
        crate::home::ForegroundOrigin::OriginalWrapper => "开机首页",
    }
    .into();
    Ok(())
}

fn execute(request: &Request, mut view: View) -> Result<(View, Option<String>)> {
    let mut token = None;
    view.notice.clear();
    view.error = false;
    match &request.operation {
        Operation::Refresh | Operation::Standalone => {}
        Operation::Rename { name } => {
            require(
                nearby_menu::management::valid_name(name),
                "名称需为 1–64 字节",
            )?;
            crate::identity::rename(name.trim())?;
            view.notice = "设备名称已保存".into();
        }
        Operation::Forget { id } => {
            crate::identity::forget(id)?;
            view.notice = "好友已移除".into();
        }
        Operation::SetHome { home } => {
            crate::home::Preference::save(match home {
                Home::Original => crate::home::DefaultHome::Original,
                Home::Nearby => crate::home::DefaultHome::Nearby,
            })?;
            view.notice = "开机首页已保存".into();
        }
        Operation::Diagnose => {
            let compatibility = crate::compat::detect();
            let installed = crate::installer::installation_status()
                .map(|v| format!("{} 个文件，校验通过", v["files"]))
                .unwrap_or_else(|e| e.to_string());
            let gui = std::fs::read(
                std::path::Path::new(crate::context::INSTALL).join("nearby-gui-build.json"),
            )
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
            let renderer = gui
                .as_ref()
                .map(|v| {
                    format!(
                        "mirui {} · {}",
                        v["mirui"].as_str().unwrap_or("—"),
                        v["backend"].as_str().unwrap_or("—")
                    )
                })
                .unwrap_or_else(|| "构建信息未找到".into());
            view.facts = vec![
                Fact {
                    label: "安装文件".into(),
                    value: installed,
                },
                Fact {
                    label: "应用版本".into(),
                    value: env!("CARGO_PKG_VERSION").into(),
                },
                Fact {
                    label: "设备配置".into(),
                    value: crate::compat::PROFILE.into(),
                },
                Fact {
                    label: "系统检查".into(),
                    value: if compatibility.problems().is_empty() {
                        "正常".into()
                    } else {
                        compatibility.problems().join("\n")
                    },
                },
                Fact {
                    label: "安装位置".into(),
                    value: crate::context::INSTALL.into(),
                },
                Fact {
                    label: "图形组件".into(),
                    value: renderer,
                },
            ];
        }
        Operation::Close => {}
        Operation::Browse { directory } => {
            (view.directory, view.packages) = crate::updates::browse(directory)?;
        }
        Operation::Inspect { path } => {
            view.update = Some(crate::updates::inspect(path)?);
        }
        Operation::Install { digest } => {
            let update = view.update.as_ref().ok_or("请先选择安装包")?;
            require(
                update.compatible && &update.digest == digest,
                "安装包已改变，请重新选择",
            )?;
            token = Some(crate::updates::apply(update)?);
            view.update.as_mut().unwrap().phase = UpdatePhase::Complete;
            view.notice = "安装完成".into();
        }
    }
    refresh(&mut view)?;
    view.revision = request.sequence;
    view.busy = false;
    Ok((view, token))
}

impl Service {
    pub fn open(&mut self, entry: nearby_menu::management::Entry) {
        self.view.entry = entry;
        self.view.busy = false;
        self.view.revision = 0;
        self.view.epoch = self.view.epoch.wrapping_add(1).max(1);
        self.sequence = 0;
        self.request(Request {
            epoch: self.view.epoch,
            sequence: 1,
            operation: Operation::Refresh,
        });
    }

    pub fn request(&mut self, request: Request) {
        if request.epoch != self.view.epoch || request.sequence <= self.sequence || self.view.busy {
            return;
        }
        if self.worker.is_none() {
            let (sender, incoming) = mpsc::sync_channel::<(Request, View)>(1);
            let (outgoing, replies) = mpsc::sync_channel(1);
            std::thread::spawn(move || {
                while let Ok((request, view)) = incoming.recv() {
                    let epoch = request.epoch;
                    let mut failed = view.clone();
                    let result = match execute(&request, view) {
                        Ok(value) => Ok(value),
                        Err(error) => {
                            let _ = refresh(&mut failed);
                            failed.error = true;
                            failed.busy = false;
                            failed.revision = request.sequence;
                            failed.notice = error.to_string();
                            if matches!(request.operation, Operation::Install { .. })
                                && let Some(update) = &mut failed.update
                            {
                                update.phase = UpdatePhase::Failed;
                            }
                            Ok((failed, None))
                        }
                    };
                    if outgoing.send((epoch, result)).is_err() {
                        break;
                    }
                }
            });
            self.worker = Some(Worker { sender, replies });
        }
        if matches!(
            request.operation,
            Operation::Inspect { .. } | Operation::Browse { .. }
        ) {
            self.view.update = None;
        }
        self.sequence = request.sequence;
        self.view.busy = true;
        if matches!(request.operation, Operation::Install { .. })
            && let Some(update) = &mut self.view.update
        {
            update.phase = UpdatePhase::Installing;
        }
        self.view.notice.clear();
        self.pending = Some(request);
        self.flush();
    }

    fn flush(&mut self) {
        let Some(request) = self.pending.take() else {
            return;
        };
        let Some(worker) = &self.worker else {
            self.pending = Some(request);
            return;
        };
        match worker.sender.try_send((request.clone(), self.view.clone())) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.pending = Some(request),
            Err(TrySendError::Disconnected(_)) => {
                self.view.busy = false;
                self.view.error = true;
                self.view.revision = self.sequence;
                self.view.notice = "设备服务已退出，请重新打开".into();
                self.worker = None;
            }
        }
    }

    pub fn poll(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        match worker.replies.try_recv() {
            Ok((epoch, result)) if epoch == self.view.epoch => {
                self.view.busy = false;
                match result {
                    Ok((mut view, token)) => {
                        self.token = token;
                        view.entry = self.view.entry;
                        self.view = view;
                    }
                    Err(error) => {
                        self.view.revision = self.sequence;
                        self.view.notice = error;
                        self.view.error = true;
                        self.view.room_busy = crate::session::require_idle().is_err();
                    }
                }
            }
            Ok(_) | Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.view.busy = false;
                self.worker = None;
            }
        }
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nearby_menu::management::Entry;

    #[test]
    fn management_projection_and_requests_keep_public_identity_and_typed_actions() {
        let view = View {
            device: Device {
                id: "a".repeat(64),
                name: "小林的掌机".into(),
                address: String::new(),
            },
            ..Default::default()
        };
        let json = serde_json::to_value(view).unwrap();
        assert_eq!(json["device"].as_object().unwrap().len(), 3);
        for invalid in [
            r#"{"epoch":1,"sequence":1,"operation":{"kind":"run","command":"shell"}}"#,
            r#"{"epoch":1,"sequence":1,"operation":{"kind":"forget","id":"x","secret":"key"}}"#,
        ] {
            assert!(serde_json::from_str::<Request>(invalid).is_err());
        }
    }
    #[test]
    fn reopened_management_uses_a_new_epoch_and_discards_the_retired_reply() {
        let (sender, incoming) = mpsc::sync_channel(1);
        let (outgoing, replies) = mpsc::sync_channel(1);
        let mut service = Service {
            worker: Some(Worker { sender, replies }),
            ..Default::default()
        };
        service.open(Entry::Device);
        let (first, _) = incoming.recv().unwrap();
        service.open(Entry::Application);
        let (second, _) = incoming.recv().unwrap();
        assert_ne!(first.epoch, second.epoch);
        let mut old = service.view.clone();
        old.epoch = first.epoch;
        old.notice = "retired".into();
        old.busy = false;
        outgoing.send((first.epoch, Ok((old, None)))).unwrap();
        service.poll();
        assert!(service.view.busy);
        assert_ne!(service.view.notice, "retired");
        let mut current = service.view.clone();
        current.busy = false;
        current.revision = second.sequence;
        current.device.name = "当前掌机".into();
        outgoing.send((second.epoch, Ok((current, None)))).unwrap();
        service.poll();
        assert!(!service.view.busy);
        assert_eq!(service.view.entry, Entry::Application);
        assert_eq!(service.view.device.name, "当前掌机");
        service.request(first);
        assert!(incoming.try_recv().is_err());
    }
}
