use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Value, json};

use super::{Event, OwnedChild, log};
use crate::context::INSTALL;
use crate::system::{mkdir, monotonic};
use crate::{Result, require};

struct Delivery {
    screen: Option<u64>,
    event: Event,
}

#[derive(Clone)]
pub(super) struct Replies {
    screen: u64,
    sender: mpsc::Sender<Delivery>,
}

impl Replies {
    pub(super) fn send(&self, event: Event) {
        let _ = self.sender.send(Delivery {
            screen: Some(self.screen),
            event,
        });
    }
}

pub(super) struct Display {
    _lock: Option<crate::system::Lock>,
    stdin: Option<ChildStdin>,
    gui: Option<OwnedChild>,
    reader: Option<JoinHandle<()>>,
    sender: mpsc::Sender<Delivery>,
    receiver: mpsc::Receiver<Delivery>,
    stopping: Arc<AtomicBool>,
    signals: Vec<signal_hook::SigId>,
    screen: u64,
    input_epoch: f64,
    previous: Value,
}

impl Display {
    pub(super) fn open() -> Result<Self> {
        Self::open_mode(false)
    }
    pub(super) fn open_launcher() -> Result<Self> {
        Self::open_mode(true)
    }
    fn open_mode(launcher: bool) -> Result<Self> {
        let root = Path::new("/run/arkos-nearby-rust/ui");
        mkdir(root)?;
        let lock = crate::system::Lock::try_acquire(&root.join("interface.lock"))?;
        let mut display = Self::spawn(launcher, Arc::new(AtomicBool::new(false)))?;
        display._lock = Some(lock);
        Ok(display)
    }

    fn spawn(launcher: bool, stopping: Arc<AtomicBool>) -> Result<Self> {
        let logs = Path::new(INSTALL).join("logs");
        mkdir(&logs)?;
        log(json!({"phase":"gui-spawn"}))?;
        let gui = OwnedChild(
            Command::new(format!("{INSTALL}/arkos-nearby-gui"))
                .env(
                    "ARKOS_NEARBY_LAUNCHER_INPUT",
                    if launcher { "1" } else { "0" },
                )
                .env("SDL_INPUT_LINUX_KEEP_KBD", "1")
                .env("SDL_RENDER_BATCHING", "1")
                .env("TERM", "linux")
                .env("LANG", "C.UTF-8")
                .env("LC_ALL", "C.UTF-8")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::from(File::create(logs.join("native-gui.log"))?))
                .spawn()?,
        );
        let mut display = Self::connect(gui)?;
        display.stopping = stopping;
        for signal in [libc::SIGTERM, libc::SIGHUP] {
            display.signals.push(signal_hook::flag::register(
                signal,
                display.stopping.clone(),
            )?);
        }
        Ok(display)
    }

    pub(super) fn handoff<T>(
        &mut self,
        operation: impl FnOnce(&AtomicBool) -> Result<T>,
        resume: impl FnOnce(Arc<AtomicBool>) -> Result<Self>,
    ) -> Result<T> {
        require(!self.stopped(), "Interface is stopping")?;
        self.close_gui();
        let result = operation(&self.stopping);
        require(!self.stopped(), "Interface was stopped during system tool")?;
        let mut replacement = resume(self.stopping.clone())?;
        replacement._lock = self._lock.take();
        *self = replacement;
        result
    }

    pub(super) fn with_frontend_released<T>(
        &mut self,
        launcher: bool,
        operation: impl FnOnce(&AtomicBool) -> Result<T>,
        ready: impl FnOnce() -> Result<()>,
    ) -> Result<T> {
        self.handoff(operation, |stopping| {
            ready()?;
            Self::spawn(launcher, stopping)
        })
    }

    fn close_gui(&mut self) {
        drop(self.stdin.take());
        drop(self.gui.take());
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }

    fn connect(mut gui: OwnedChild) -> Result<Self> {
        let stdin = gui.0.stdin.take().ok_or("GUI has no input channel")?;
        let stdout = gui.0.stdout.take().ok_or("GUI has no output channel")?;
        let (sender, receiver) = mpsc::channel();
        let input_sender = sender.clone();
        // Capture the screen epoch before starting the reader. The GUI
        // can emit the first key edge immediately after spawn; recording the
        // epoch afterwards would classify that valid edge as stale.
        let input_epoch = monotonic();
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let result = (&mut reader).take(262145).read_until(b'\n', &mut line);
                if !matches!(result, Ok(count) if count > 0 && count <= 262144) {
                    break;
                }
                if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                    let event = if value == json!({"event":"frame_ready"}) {
                        Some(Event::FrameReady)
                    } else if value["event"] == "shelf_layout" {
                        serde_json::from_value::<crate::catalog::shelf::Grid>(value["grid"].clone())
                            .ok()
                            .filter(|grid| grid.valid())
                            .zip(value["launcher"].as_bool())
                            .map(|(grid, launcher)| Event::ShelfLayout(grid, launcher))
                    } else if value["event"] == "list_layout" {
                        value["rows"]
                            .as_u64()
                            .filter(|rows| {
                                (1..=nearby_menu::LIST_WINDOW_ITEMS as u64).contains(rows)
                            })
                            .zip(value["launcher"].as_bool())
                            .map(|(rows, launcher)| Event::ListLayout(rows as usize, launcher))
                    } else if value["event"] == "saves" {
                        serde_json::from_value::<nearby_menu::saves::Request>(
                            value["request"].clone(),
                        )
                        .ok()
                        .filter(nearby_menu::saves::Request::valid)
                        .map(|request| Event::Saves(request, monotonic()))
                    } else if value["event"] == "configuration" {
                        serde_json::from_value::<nearby_menu::configuration::Request>(
                            value["request"].clone(),
                        )
                        .ok()
                        .filter(nearby_menu::configuration::Request::valid)
                        .map(|request| Event::Configuration(request, monotonic()))
                    } else if value["event"] == "wifi" {
                        serde_json::from_value::<nearby_menu::wifi::Request>(
                            value["request"].clone(),
                        )
                        .ok()
                        .filter(nearby_menu::wifi::Request::valid)
                        .map(|request| Event::Wifi(request, monotonic()))
                    } else if value["event"] == "launcher"
                        && matches!(value["action"].as_str(), Some("launch" | "multiplayer"))
                        && value["key"].as_str().is_some_and(|key| {
                            key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
                        })
                    {
                        Some(Event::Launcher(
                            value["action"].as_str().unwrap().into(),
                            value["key"].as_str().unwrap().into(),
                            monotonic(),
                        ))
                    } else {
                        value
                            .as_str()
                            .filter(|s| {
                                matches!(
                                    *s,
                                    "up" | "down"
                                        | "left"
                                        | "right"
                                        | "confirm"
                                        | "back"
                                        | "refresh"
                                        | "page_up"
                                        | "page_down"
                                        | "auxiliary"
                                        | "start"
                                        | "favorite"
                                        | "launcher_home"
                                )
                            })
                            .map(|s| Event::Input(s.into(), monotonic()))
                    };
                    if let Some(event) = event {
                        if input_sender
                            .send(Delivery {
                                screen: None,
                                event,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
            let _ = input_sender.send(Delivery {
                screen: None,
                event: Event::Eof,
            });
        });
        Ok(Self {
            _lock: None,
            stdin: Some(stdin),
            gui: Some(gui),
            reader: Some(reader),
            sender,
            receiver,
            stopping: Arc::new(AtomicBool::new(false)),
            signals: Vec::new(),
            screen: 0,
            input_epoch,
            previous: Value::Null,
        })
    }

    pub(super) fn begin_screen(&mut self) -> Replies {
        let first = self.screen == 0;
        self.screen += 1;
        // Keep the connection epoch for the first screen so a real edge that
        // arrives during startup remains actionable; later screens rebase it
        // to discard input queued for the previous view.
        if !first {
            self.input_epoch = monotonic();
        }
        Replies {
            screen: self.screen,
            sender: self.sender.clone(),
        }
    }

    pub(super) fn stopped(&self) -> bool {
        self.stopping.load(Ordering::Relaxed)
    }

    pub(super) fn present(&mut self, view: Value) -> Result<()> {
        let gui = self.gui.as_mut().ok_or("The graphical interface stopped")?;
        require(
            gui.0.try_wait()?.is_none(),
            "The graphical interface stopped",
        )?;
        if view != self.previous {
            let stdin = self.stdin.as_mut().ok_or("GUI has no input channel")?;
            serde_json::to_writer(&mut *stdin, &view)?;
            stdin.write_all(b"\n")?;
            stdin.flush()?;
            self.previous = view;
        }
        Ok(())
    }

    pub(super) fn receive(&self) -> std::result::Result<Option<Event>, mpsc::RecvTimeoutError> {
        let delivery = self.receiver.recv_timeout(Duration::from_millis(20))?;
        if delivery.screen.is_some_and(|screen| screen != self.screen)
            || matches!(&delivery.event, Event::Input(_, at) | Event::Launcher(_,_,at)|Event::Wifi(_,at)|Event::Configuration(_,at)|Event::Saves(_,at) if *at < self.input_epoch)
        {
            return Ok(None);
        }
        Ok(Some(delivery.event))
    }

    pub(super) fn previous(&self) -> &Value {
        &self.previous
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        for signal in self.signals.drain(..) {
            signal_hook::low_level::unregister(signal);
        }
        self.close_gui();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn display() -> Display {
        let gui = OwnedChild(Command::new("sh").args(["-c", "printf '{\"event\":\"frame_ready\"}\\n'; while IFS= read -r line; do printf '\"confirm\"\\n'; done"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap());
        let display = Display::connect(gui).unwrap();
        assert!(matches!(next(&display), Event::FrameReady));
        display
    }

    fn next(display: &Display) -> Event {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match display.receive() {
                Ok(Some(event)) => return event,
                Ok(None) | Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
                _ => panic!("The GUI did not deliver an event"),
            }
        }
    }

    #[test]
    fn first_input_is_kept_when_it_arrives_before_the_first_frame() {
        let gui = OwnedChild(
            Command::new("sh")
                .args([
                    "-c",
                    "printf '\"confirm\"\\n'; sleep 0.05; printf '{\"event\":\"frame_ready\"}\\n'; sleep 1",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let display = Display::connect(gui).unwrap();
        assert!(matches!(next(&display), Event::Input(action, _) if action == "confirm"));
        assert!(matches!(next(&display), Event::FrameReady));
    }

    #[test]
    fn first_input_is_kept_when_screen_begins_after_reader_start() {
        let (sender, receiver) = mpsc::channel();
        let at = monotonic();
        let mut display = Display {
            _lock: None,
            stdin: None,
            gui: None,
            reader: None,
            sender: sender.clone(),
            receiver,
            stopping: Arc::new(AtomicBool::new(false)),
            signals: Vec::new(),
            screen: 0,
            input_epoch: at,
            previous: Value::Null,
        };
        sender
            .send(Delivery {
                screen: None,
                event: Event::Input("confirm".into(), at),
            })
            .unwrap();
        display.begin_screen();
        assert!(matches!(next(&display), Event::Input(action, _) if action == "confirm"));
    }

    #[test]
    fn screen_handoffs_keep_the_gui_and_close_it_at_session_end() {
        let mut display = display();
        let gui = display.gui.as_ref().unwrap().0.id();
        for page in ["mode", "games", "choice", "host", "rooms", "mode"] {
            display.begin_screen();
            display.present(json!({"page":page})).unwrap();
            assert!(matches!(next(&display), Event::Input(action, _) if action == "confirm"));
            assert_eq!(display.gui.as_ref().unwrap().0.id(), gui);
        }
        drop(display);
        assert_eq!(unsafe { libc::kill(gui as libc::pid_t, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }

    #[test]
    fn external_tools_release_the_gui_but_keep_ownership_and_resume_after_errors() {
        let path = std::env::temp_dir().join(crate::system::random_id().unwrap());
        for failed in [false, true] {
            let mut display = display();
            display._lock = Some(crate::system::Lock::try_acquire(&path).unwrap());
            let old = display.gui.as_ref().unwrap().0.id();
            let stopping = display.stopping.clone();
            let sender = display.sender.clone();
            let result = display.handoff(
                |_| -> Result<()> {
                    assert_eq!(unsafe { libc::kill(old as i32, 0) }, -1);
                    assert!(crate::system::Lock::try_acquire(&path).is_err());
                    sender
                        .send(Delivery {
                            screen: None,
                            event: Event::Input("back".into(), monotonic()),
                        })
                        .unwrap();
                    if failed {
                        Err("Tool failed".into())
                    } else {
                        Ok(())
                    }
                },
                |flag| {
                    assert!(Arc::ptr_eq(&flag, &stopping));
                    let mut replacement = self::display();
                    replacement.stopping = flag;
                    Ok(replacement)
                },
            );
            assert_eq!(result.is_err(), failed);
            assert!(crate::system::Lock::try_acquire(&path).is_err());
            assert!(display.previous().is_null());
            assert!(matches!(
                display.receive(),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            display.begin_screen();
            display.present(json!({"page":"tools"})).unwrap();
            assert!(matches!(next(&display), Event::Input(action, _) if action == "confirm"));
            drop(display);
            drop(crate::system::Lock::try_acquire(&path).unwrap());
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn stopped_or_failed_handoffs_do_not_resurrect_the_interface() {
        let mut display = display();
        assert!(
            display
                .handoff(|_| Ok(()), |_| Err("Resume failed".into()))
                .is_err()
        );
        assert!(display.gui.is_none() && display.reader.is_none() && display.stdin.is_none());
        let mut display = self::display();
        assert!(
            display
                .handoff(
                    |flag| {
                        flag.store(true, Ordering::Relaxed);
                        Ok(())
                    },
                    |_| panic!("Stopped interface must not resume"),
                )
                .is_err()
        );
        assert!(display.stopped());
        assert!(display.gui.is_none());
        assert!(
            display
                .handoff(
                    |_| -> Result<()> { panic!("Stopped interface must not run a tool") },
                    |_| panic!("Stopped interface must not resume"),
                )
                .is_err()
        );
    }

    #[test]
    fn file_changes_discard_only_the_inflight_index_without_waiting_on_the_ui() {
        let browser = crate::catalog::Browser::new().unwrap();
        let job = browser.index_job();
        let (release, waiting) = mpsc::channel();
        let mut menu = super::super::MenuSession {
            display: display(),
            lobby_flow: None,
            launcher: true,
            index_task: Some(thread::spawn(move || {
                waiting.recv().unwrap();
                Some(job.run())
            })),
            index_invalidated: false,
        };
        menu.invalidate_library();
        assert!(menu.take_index().unwrap().is_none());
        assert!(menu.index_task.is_some());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !menu.index_task.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(menu.take_index().unwrap().is_none());
        assert!(!menu.index_invalidated);
        assert!(menu.index_task.is_none());
        let job = browser.index_job();
        menu.index_task = Some(thread::spawn(move || Some(job.run())));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !menu.index_task.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(menu.take_index().unwrap().is_some());
        menu.invalidate_library();
        assert!(!menu.index_invalidated);
    }

    #[test]
    fn direct_entry_prewarming_survives_screen_changes_and_supplies_the_browser() {
        let mut menu = super::super::MenuSession {
            display: display(),
            lobby_flow: None,
            launcher: false,
            index_task: None,
            index_invalidated: false,
        };
        let mut browser = crate::catalog::Browser::new().unwrap();
        menu.prewarm_catalog();
        let owner = menu.index_task.as_ref().unwrap().thread().id();
        for _ in 0..32 {
            menu.display.begin_screen();
            menu.prewarm_catalog();
            browser.reopen();
            menu.ensure_index(&browser);
            assert_eq!(menu.index_task.as_ref().unwrap().thread().id(), owner);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !menu.index_task.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        let mut recheck_snapshot = true;
        menu.poll_browser_index(&mut browser, &mut recheck_snapshot)
            .unwrap();
        assert!(!recheck_snapshot);
        assert!(
            browser.indexing,
            "An earlier snapshot needs a freshness check"
        );
        let seed = browser.index_job().cached().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !menu.index_task.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        menu.poll_browser_index(&mut browser, &mut recheck_snapshot)
            .unwrap();
        assert!(!browser.indexing);
        assert!(Arc::ptr_eq(&browser.index_job().cached().unwrap(), &seed));
        browser.resume();
        menu.ensure_index(&browser);
        assert!(menu.index_task.is_none());
    }

    #[test]
    fn late_workers_and_queued_old_input_do_not_cross_screen_boundaries() {
        let mut display = display();
        let old = display.begin_screen();
        let old_input = display.input_epoch;
        let current = display.begin_screen();
        old.send(Event::Status(0, json!({"ready":true})));
        old.send(Event::Scan(0, json!({"rooms":[]})));
        old.send(Event::Prepared(Err("stale".into())));
        old.send(Event::Action(
            crate::flow::Request {
                token: 1,
                operation: crate::flow::Operation::Create,
                payload: json!({}),
            },
            json!({"ready":true}),
            false,
        ));
        display
            .sender
            .send(Delivery {
                screen: None,
                event: Event::Input("back".into(), old_input - 1.0),
            })
            .unwrap();
        for _ in 0..5 {
            assert!(display.receive().unwrap().is_none());
        }
        current.send(Event::Status(0, json!({"ready":false})));
        assert!(matches!(next(&display), Event::Status(0, value) if value["ready"] == false));
    }
}
