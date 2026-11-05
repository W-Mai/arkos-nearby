use super::Hardware;
use crate::Result;
use nearby_menu::{Level, MenuAction, MenuItem, SystemLevel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Clone, Copy)]
pub(super) struct Reading {
    pub value: u8,
    pub muted: bool,
}

pub(super) trait Driver: Send + 'static {
    fn read(&mut self, control: SystemLevel) -> std::result::Result<Reading, String>;
    fn write(&mut self, control: SystemLevel, target: u8) -> std::result::Result<Reading, String>;
}

#[derive(Default)]
struct State {
    readings: [Option<std::result::Result<Reading, String>>; 2],
    confirmed: [Option<Reading>; 2],
    targets: [Option<u8>; 2],
    revisions: [u64; 2],
    dirty: [bool; 2],
    refresh: bool,
    stop: bool,
}

type Shared = Arc<(Mutex<State>, Condvar)>;

struct Worker {
    shared: Shared,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn start(mut driver: impl Driver) -> std::io::Result<Self> {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let background = shared.clone();
        let thread = thread::Builder::new()
            .name("nearby-levels".into())
            .spawn(move || {
                let (mutex, wake) = &*background;
                loop {
                    let (refresh, targets, revisions) = {
                        let mut state = mutex.lock().unwrap();
                        while !state.stop
                            && !state.refresh
                            && !state.dirty.iter().any(|dirty| *dirty)
                        {
                            state = wake.wait(state).unwrap();
                        }
                        if state.stop {
                            break;
                        }
                        let targets = std::array::from_fn::<_, 2, _>(|index| {
                            if state.dirty[index] {
                                state.targets[index]
                            } else {
                                None
                            }
                        });
                        state.dirty = [false; 2];
                        let refresh = std::mem::take(&mut state.refresh);
                        (refresh, targets, state.revisions)
                    };
                    for control in SystemLevel::ALL {
                        let index = control.index();
                        let result = if let Some(target) = targets[index] {
                            driver.write(control, target)
                        } else if refresh {
                            driver.read(control)
                        } else {
                            continue;
                        };
                        let mut state = mutex.lock().unwrap();
                        if let Ok(reading) = &result {
                            state.confirmed[index] = Some(*reading);
                        }
                        state.readings[index] = Some(result);
                        if state.revisions[index] == revisions[index] {
                            state.targets[index] = None;
                        }
                    }
                }
            })?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.0.lock().unwrap().stop = true;
        self.shared.1.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
pub(crate) struct Controls {
    worker: Option<Worker>,
    error: Option<String>,
}

impl Controls {
    pub fn refresh(&mut self) {
        if self.worker.is_none() {
            match Worker::start(Hardware) {
                Ok(worker) => {
                    self.worker = Some(worker);
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(format!("无法读取系统设置：{error}"));
                    return;
                }
            }
        }
        let worker = self.worker.as_ref().unwrap();
        worker.shared.0.lock().unwrap().refresh = true;
        worker.shared.1.notify_one();
    }

    pub fn items(&self) -> Vec<MenuItem> {
        let snapshot = self.worker.as_ref().map(|worker| {
            let state = worker.shared.0.lock().unwrap();
            (state.readings.clone(), state.targets, state.confirmed)
        });
        let mut items: Vec<_> = SystemLevel::ALL
            .into_iter()
            .map(|control| {
                let index = control.index();
                let reading = snapshot
                    .as_ref()
                    .and_then(|(readings, _, _)| readings[index].as_ref());
                let pending = snapshot
                    .as_ref()
                    .is_some_and(|(_, targets, _)| targets[index].is_some());
                let (item, value, muted) = match reading {
                    Some(Ok(reading)) => (
                        MenuItem::available(control.action(), control.label()),
                        Some(reading.value),
                        reading.muted,
                    ),
                    Some(Err(reason)) => (
                        MenuItem::unavailable(control.action(), control.label(), reason),
                        None,
                        false,
                    ),
                    None => (
                        MenuItem::unavailable(
                            control.action(),
                            control.label(),
                            self.error.as_deref().unwrap_or("正在读取系统设置"),
                        ),
                        None,
                        false,
                    ),
                };
                let last = snapshot
                    .as_ref()
                    .and_then(|(_, _, confirmed)| confirmed[index]);
                item.with_level(Level::new(
                    control,
                    value.or(last.map(|reading| reading.value)),
                    last.map_or(muted, |reading| reading.muted),
                    pending,
                ))
            })
            .collect();
        items.push(MenuItem::available(MenuAction::ShowTools, "系统工具"));
        items.push(MenuItem::available(MenuAction::Back, "更多设置"));
        items
    }

    pub fn adjust(&mut self, control: SystemLevel, increase: bool) -> Result<()> {
        let worker = self.worker.as_ref().ok_or("System levels are not loaded")?;
        let mut state = worker.shared.0.lock().unwrap();
        let index = control.index();
        let current = state.readings[index]
            .as_ref()
            .ok_or("System level is not loaded")?
            .as_ref()
            .map_err(|reason| reason.clone())?
            .value;
        let target = state.targets[index].unwrap_or(current);
        let next = if increase {
            target.saturating_add(5).min(100)
        } else {
            target.saturating_sub(5).max(control.minimum())
        };
        if next != target {
            state.targets[index] = Some(next);
            state.revisions[index] = state.revisions[index].wrapping_add(1);
            state.dirty[index] = true;
            worker.shared.1.notify_one();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    struct Fake {
        writes: Arc<Mutex<Vec<u8>>>,
        started: Option<mpsc::Sender<()>>,
        release: Option<mpsc::Receiver<()>>,
        fail: bool,
    }
    impl Driver for Fake {
        fn read(&mut self, _: SystemLevel) -> std::result::Result<Reading, String> {
            Ok(Reading {
                value: 50,
                muted: false,
            })
        }
        fn write(&mut self, _: SystemLevel, target: u8) -> std::result::Result<Reading, String> {
            if let Some(started) = self.started.take() {
                started.send(()).unwrap();
            }
            if let Some(release) = self.release.take() {
                release.recv_timeout(Duration::from_secs(2)).unwrap();
            }
            self.writes.lock().unwrap().push(target);
            if self.fail {
                Err("Write failed".into())
            } else {
                Ok(Reading {
                    value: target,
                    muted: false,
                })
            }
        }
    }

    fn wait(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !predicate() {
            assert!(Instant::now() < deadline, "Level worker did not publish");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn pending_targets_are_bounded_and_keep_saturation_order_without_blocking_input() {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let (started, ready) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut controls = Controls {
            worker: Some(
                Worker::start(Fake {
                    writes: writes.clone(),
                    started: Some(started),
                    release: Some(gate),
                    fail: false,
                })
                .unwrap(),
            ),
            error: None,
        };
        controls.refresh();
        wait(|| controls.items()[0].enabled());
        controls.adjust(SystemLevel::Volume, true).unwrap();
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..40 {
            controls.adjust(SystemLevel::Volume, true).unwrap();
        }
        controls.adjust(SystemLevel::Volume, false).unwrap();
        let worker = controls.worker.as_ref().unwrap();
        assert_eq!(worker.shared.0.try_lock().unwrap().targets[0], Some(95));
        let level = controls.items()[0].level.unwrap();
        assert_eq!(level.value, Some(50));
        assert!(level.pending);
        release.send(()).unwrap();
        wait(|| {
            controls.items()[0]
                .level
                .is_some_and(|level| level.value == Some(95) && !level.pending)
        });
        assert_eq!(*writes.lock().unwrap(), [55, 95]);
        let items = controls.items();
        assert_eq!(items.iter().filter(|item| item.level.is_some()).count(), 2);
        assert_eq!(items.len(), 4);
        for item in items {
            assert_eq!(
                item,
                serde_json::from_slice::<MenuItem>(&serde_json::to_vec(&item).unwrap()).unwrap()
            );
        }
    }

    #[test]
    fn failed_writes_preserve_confirmed_values_and_disable_adjustment() {
        let mut controls = Controls {
            worker: Some(
                Worker::start(Fake {
                    writes: Arc::default(),
                    started: None,
                    release: None,
                    fail: true,
                })
                .unwrap(),
            ),
            error: None,
        };
        controls.refresh();
        wait(|| controls.items()[0].enabled());
        controls.adjust(SystemLevel::Volume, true).unwrap();
        wait(|| !controls.items()[0].enabled());
        let item = controls.items().remove(0);
        assert_eq!(item.level.unwrap().value, Some(50));
        assert!(!item.level.unwrap().pending);
        assert_eq!(item.reason(), "Write failed");
        assert!(controls.adjust(SystemLevel::Volume, true).is_err());
    }
}
