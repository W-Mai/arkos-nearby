use crate::catalog::Entry;
use crate::configuration::{Snapshot, definitions};
use crate::{Result, require};
use nearby_menu::configuration::{Document, Load, Projection, Request, View};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

enum Work {
    Open(u64, Entry),
    Edit(Request),
}
type Channels = (SyncSender<Work>, Receiver<(View, bool)>);

#[derive(Default)]
pub(super) struct Service {
    pub view: View,
    pending: Option<Work>,
    channels: Option<Channels>,
}

pub(super) fn read(entry: &Entry) -> Result<(crate::context::Context, Snapshot)> {
    read_document(entry, true)
}

pub(super) fn read_save_paths(entry: &Entry) -> Result<(crate::context::Context, Snapshot)> {
    read_document(entry, false)
}

fn read_document(entry: &Entry, options: bool) -> Result<(crate::context::Context, Snapshot)> {
    crate::configuration::recover()?;
    let context = super::recipes::configuration_context(entry.path())?;
    // Use the same content-verified arcade selection as ordinary launch.
    let context = if crate::core_choice::arcade_order(entry.path()).is_empty() {
        context
    } else {
        let id = crate::system::random_id()?;
        let root = Path::new("/run/arkos-nearby-configuration").join(&id);
        crate::system::mkdir(&root)?;
        let result = crate::arcade::single_player(&root, &id, &context);
        let stopped = crate::arcade::stop_launch(&root, &id);
        let cleaned = std::fs::remove_dir_all(&root);
        let (selected, _) = result?;
        stopped?;
        cleaned?;
        selected
    };
    let env = Snapshot::environment(&context)?;
    let document = Document {
        key: entry.key(),
        title: entry.name().into(),
        platform: entry.platform_name().into(),
        core: context.identity.core.name.clone(),
        ..Default::default()
    };
    let mut initial = Snapshot::load(env.clone(), document.clone(), vec![])?;
    if !options {
        if let Some(policy) = crate::core_choice::solo_options(&context.game_path, &context.core_id)
        {
            initial.lock_options(policy);
        }
        return Ok((context, initial));
    }
    let system = initial
        .settings()
        .base
        .lines()
        .find_map(|line| {
            line.strip_prefix("system_directory = \"")
                .and_then(|v| v.strip_suffix('"'))
                .map(std::path::PathBuf::from)
        })
        .unwrap_or_else(|| Path::new("/roms/bios").into());
    let options = definitions::read(&context, &system)?;
    let mut snapshot = Snapshot::load(env, document, options)?;
    if let Some(policy) = crate::core_choice::solo_options(&context.game_path, &context.core_id) {
        snapshot.lock_options(policy);
    }
    Ok((context, snapshot))
}

fn worker(incoming: Receiver<Work>, outgoing: SyncSender<(View, bool)>) {
    let mut current: Option<(crate::context::Context, Snapshot)> = None;
    let mut entry = None;
    let mut view = View::default();
    while let Ok(work) = incoming.recv() {
        let saving = matches!(&work, Work::Edit(Request::Save { .. }));
        let opened = match work {
            Work::Open(epoch, game) => {
                view = View {
                    epoch,
                    ..Default::default()
                };
                entry = Some(game);
                true
            }
            Work::Edit(Request::Reload { epoch }) if epoch == view.epoch => true,
            Work::Edit(request) if request.epoch() == view.epoch => {
                let result = (|| -> Result<()> {
                    let (context, snapshot) = current.as_ref().ok_or("设置尚未载入")?;
                    let (revision, scope, changes, save) = match &request {
                        Request::Save {
                            revision,
                            scope,
                            changes,
                            ..
                        } => (revision, *scope, changes, true),
                        Request::Preview {
                            revision,
                            scope,
                            changes,
                            ..
                        } => (revision, *scope, changes, false),
                        _ => return Ok(()),
                    };
                    require(
                        *revision == snapshot.document.revision,
                        "配置已变化，请重新读取",
                    )?;
                    if save {
                        context.revalidate()?;
                        crate::session::try_while_idle(|| {
                            snapshot.save(scope, changes, Path::new(crate::configuration::JOURNAL))
                        })?;
                        view.saved = view.saved.wrapping_add(1);
                        let context = context.clone();
                        let snapshot = match snapshot.reload() {
                            Ok(snapshot) => snapshot,
                            Err(error) => {
                                view.load = Load::Failed {
                                    message: format!("已保存，重新读取未完成：{error}"),
                                };
                                current = None;
                                return Err(error);
                            }
                        };
                        view.load = Load::Ready {
                            document: snapshot.document.clone(),
                        };
                        current = Some((context, snapshot));
                        view.projection = None;
                    } else {
                        view.projection = Some(Projection {
                            scope,
                            changes: changes.clone(),
                            facts: snapshot.preview(scope, changes)?,
                        });
                    }
                    Ok(())
                })();
                view.error = result.err().map(|e| e.to_string()).unwrap_or_default();
                false
            }
            _ => continue,
        };
        if opened {
            current = None;
            let result =
                crate::session::try_while_idle(|| read(entry.as_ref().ok_or("游戏已关闭")?));
            match result {
                Ok((context, snapshot)) => {
                    view.load = Load::Ready {
                        document: snapshot.document.clone(),
                    };
                    current = Some((context, snapshot));
                    view.error.clear();
                }
                Err(error) => {
                    view.load = Load::Failed {
                        message: error.to_string(),
                    }
                }
            }
        }
        view.saving = false;
        if outgoing.send((view.clone(), saving)).is_err() {
            break;
        }
    }
}

impl Service {
    pub fn open(&mut self, entry: Entry) {
        self.view = View {
            epoch: self.view.epoch.wrapping_add(1),
            ..Default::default()
        };
        if self.channels.is_none() {
            let (requests, incoming) = mpsc::sync_channel(1);
            let (outgoing, replies) = mpsc::sync_channel(1);
            match std::thread::Builder::new()
                .name("game-settings".into())
                .spawn(move || worker(incoming, outgoing))
            {
                Ok(_) => self.channels = Some((requests, replies)),
                Err(error) => {
                    self.view.load = Load::Failed {
                        message: error.to_string(),
                    };
                    return;
                }
            }
        }
        self.pending = Some(Work::Open(self.view.epoch, entry));
        self.poll();
    }
    pub fn request(&mut self, request: Request) {
        if request.epoch() != self.view.epoch || !request.valid() || self.view.saving {
            return;
        }
        self.view.error.clear();
        if matches!(request, Request::Save { .. }) {
            self.view.saving = true;
        }
        self.pending = Some(Work::Edit(request));
        self.poll();
    }
    pub fn poll(&mut self) {
        let Some((requests, replies)) = &self.channels else {
            return;
        };
        loop {
            match replies.try_recv() {
                Ok((view, saved)) if view.epoch == self.view.epoch => {
                    let saving = self.view.saving && !saved;
                    self.view = view;
                    self.view.saving |= saving;
                }
                Ok(_) => {}
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.view.error = "设置任务已结束".into();
                    self.view.saving = false;
                    self.channels = None;
                    return;
                }
            }
        }
        if let Some(work) = self.pending.take() {
            match requests.try_send(work) {
                Ok(()) => {}
                Err(TrySendError::Full(work)) => self.pending = Some(work),
                Err(TrySendError::Disconnected(_)) => {
                    self.view.saving = false;
                    self.view.error = "设置任务已结束".into();
                }
            }
        }
    }
}
