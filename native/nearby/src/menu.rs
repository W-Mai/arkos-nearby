use std::fs;
use std::path::Path;
use std::process::Child;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use crate::Result;
use crate::context::{Context, INSTALL};
use crate::flow::{Flow, FlowExit, Request};
use crate::purpose::SessionPurpose;
use crate::system::{mkdir, monotonic, wall_time, write};

pub trait Backend: Send + Sync + 'static {
    fn call(&self, action: &str, payload: &Value) -> Result<Value>;
    fn failure_snapshot(&self) -> Value;
}

enum Event {
    Input(String, f64),
    ShelfLayout(crate::catalog::shelf::Grid, bool),
    ListLayout(usize, bool),
    Launcher(String, String, f64),
    Wifi(nearby_menu::wifi::Request, f64),
    Configuration(nearby_menu::configuration::Request, f64),
    Saves(nearby_menu::saves::Request, f64),
    FrameReady,
    Eof,
    Status(u64, Value),
    Scan(u64, Value),
    Action(Request, Value, bool),
    Prepared(std::result::Result<Box<Context>, String>),
    Searched(crate::catalog::SearchResult),
}

enum Outcome {
    Mode(FlowExit),
    Exit(i32),
    Selected(Box<Context>),
}

pub struct OwnedChild(pub Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        unsafe {
            libc::kill(self.0.id() as i32, libc::SIGTERM);
        }
        for _ in 0..30 {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn log(values: Value) -> Result<()> {
    let root = Path::new(INSTALL).join("logs");
    mkdir(&root)?;
    let path = root.join("last-session-native.json");
    let mut record = if values["phase"] == "context" {
        if path.is_file() {
            fs::rename(&path, root.join("last-session-native.previous.json"))?;
        }
        json!({})
    } else {
        crate::system::read::<Value>(&path).unwrap_or_else(|_| json!({}))
    };
    for (key, value) in values.as_object().ok_or("Diagnostic must be an object")? {
        record[key] = value.clone();
    }
    record["updated_at"] = wall_time().into();
    let bytes = serde_json::to_vec(&record)?;
    if bytes.len() > 262144 {
        record = json!({"phase":record["phase"],"exit_code":record["exit_code"],"error":"Diagnostic limit exceeded"});
    }
    write(&path, &record)
}

mod display;

pub struct MenuSession {
    display: display::Display,
    lobby_flow: Option<Flow>,
    launcher: bool,
    index_task: Option<thread::JoinHandle<Option<Arc<crate::catalog::Index>>>>,
    index_invalidated: bool,
}

impl MenuSession {
    pub(crate) fn prewarm_catalog(&mut self) {
        if self.index_task.is_none() {
            self.index_task = Some(thread::spawn(|| {
                crate::catalog::IndexJob::local().ok().map(|job| job.run())
            }));
        }
    }

    fn ensure_index(&mut self, browser: &crate::catalog::Browser) {
        if self.index_task.is_none() && browser.indexing {
            let job = browser.index_job();
            self.index_task = Some(thread::spawn(move || Some(job.run())));
        }
    }

    fn poll_browser_index(
        &mut self,
        browser: &mut crate::catalog::Browser,
        recheck_snapshot: &mut bool,
    ) -> Result<()> {
        if let Some(index) = self.take_index()? {
            browser.accept_index(index);
            if std::mem::take(recheck_snapshot) {
                browser.indexing = true;
            }
        }
        if self.index_task.is_none() {
            *recheck_snapshot = false;
        }
        self.ensure_index(browser);
        Ok(())
    }

    pub(crate) fn invalidate_library(&mut self) {
        self.index_invalidated = self.index_task.is_some();
    }

    fn take_index(&mut self) -> Result<Option<Arc<crate::catalog::Index>>> {
        if !self
            .index_task
            .as_ref()
            .is_some_and(|task| task.is_finished())
        {
            return Ok(None);
        }
        let index = self.index_task.take().unwrap().join();
        if std::mem::take(&mut self.index_invalidated) {
            return Ok(None);
        }
        index.map_err(|_| "Library indexing failed".into())
    }

    pub fn stopping(&self) -> bool {
        self.display.stopped()
    }

    pub fn with_frontend_released<T>(
        &mut self,
        operation: impl FnOnce(&std::sync::atomic::AtomicBool) -> Result<T>,
        ready: impl FnOnce() -> Result<()>,
    ) -> Result<T> {
        self.display
            .with_frontend_released(self.launcher, operation, ready)
    }

    pub fn launcher_busy(
        &mut self,
        controller: &mut crate::launcher::Controller,
        message: &str,
    ) -> Result<()> {
        let mut view = controller.view();
        view["busy"] = true.into();
        view["status"] = message.into();
        view["launcher"]["key"] = "".into();
        self.display.begin_screen();
        self.display.present(view)
    }
    pub fn open_launcher() -> Result<Self> {
        Ok(Self {
            display: display::Display::open_launcher()?,
            lobby_flow: None,
            launcher: true,
            index_task: None,
            index_invalidated: false,
        })
    }
    pub fn choose_launcher(
        &mut self,
        controller: &mut crate::launcher::Controller,
    ) -> Result<crate::launcher::Intent> {
        let mut sender = self.display.begin_screen();
        while !self.display.stopped() {
            if controller.changed_tab {
                controller.changed_tab = false;
                sender = self.display.begin_screen();
            }
            if let Some(index) = self.take_index()? {
                controller.indexed(index);
            }
            self.ensure_index(controller.browser());
            if let Some(job) = controller.browser().search_job() {
                let replies = sender.clone();
                thread::spawn(move || replies.send(Event::Searched(job.run())));
            }
            self.display.present(controller.view())?;
            if let Some(intent) = controller.save_launch() {
                return Ok(intent);
            }
            let outcome = match self.display.receive() {
                Ok(Some(Event::Input(action, _))) => controller.input(&action)?,
                Ok(Some(Event::Launcher(action, key, _))) => controller.gesture(&action, &key)?,
                Ok(Some(Event::Saves(request, _))) => {
                    controller.saves_request(request);
                    None
                }
                Ok(Some(Event::Configuration(request, _))) => {
                    controller.configuration_request(request);
                    None
                }
                Ok(Some(Event::Wifi(request, _))) => {
                    controller.wifi_request(request);
                    None
                }
                Ok(Some(Event::ShelfLayout(grid, true))) => {
                    controller.shelf_layout(grid);
                    None
                }
                Ok(Some(Event::ListLayout(rows, true))) => {
                    controller.list_layout(rows);
                    None
                }
                Ok(Some(Event::Searched(result))) => {
                    controller.browser().searched(result);
                    None
                }
                Ok(Some(Event::FrameReady)) => {
                    log(json!({"phase":"launcher_frame_submitted"}))?;
                    None
                }
                Ok(Some(Event::Eof)) => return Err("Launcher display closed".into()),
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(error) => return Err(error.into()),
            };
            if let Some(intent) = outcome {
                log(json!({"phase":"launcher_action","last_view":self.display.previous()}))?;
                return Ok(intent);
            }
        }
        log(json!({"phase":"launcher_exit","last_view":self.display.previous()}))?;
        Ok(crate::launcher::Intent::Exit)
    }
    pub fn open() -> Result<Self> {
        Ok(Self {
            display: display::Display::open()?,
            lobby_flow: None,
            launcher: false,
            index_task: None,
            index_invalidated: false,
        })
    }
    pub fn run_with_fallback(
        &mut self,
        context: Option<Context>,
        backend: Option<Arc<dyn Backend>>,
        single_player: bool,
    ) -> Result<i32> {
        match self.run_screen(
            context,
            backend,
            single_player,
            None,
            SessionPurpose::Game,
            false,
        )? {
            Outcome::Exit(code) => Ok(code),
            Outcome::Mode(_) => Err("Game flow returned a mode selection".into()),
            Outcome::Selected(_) => Err("Room flow returned a game selection".into()),
        }
    }

    pub fn choose_game(
        &mut self,
        browser: &mut crate::catalog::Browser,
    ) -> Result<Option<Context>> {
        match self.run_screen(
            None,
            None,
            false,
            Some(browser),
            SessionPurpose::Game,
            false,
        )? {
            Outcome::Selected(context) => Ok(Some(*context)),
            Outcome::Exit(_) => Ok(None),
            Outcome::Mode(_) => Err("Browser returned a mode selection".into()),
        }
    }

    pub fn choose_mode(&mut self) -> Result<Option<FlowExit>> {
        self.lobby(None, false)
    }

    fn lobby(&mut self, context: Option<Context>, single_player: bool) -> Result<Option<FlowExit>> {
        let backend = crate::session::DiscoveryBackend::shared(context.clone());
        match self.run_screen(
            context,
            Some(backend),
            single_player,
            None,
            SessionPurpose::Game,
            true,
        )? {
            Outcome::Mode(mode) => Ok(Some(mode)),
            Outcome::Exit(_) => Ok(None),
            _ => Err("Invalid mode result".into()),
        }
    }

    pub fn run_network(&mut self) -> Result<bool> {
        match self.run_screen(
            None,
            Some(crate::session::DiscoveryBackend::shared(None)),
            false,
            None,
            SessionPurpose::NetworkOnly,
            false,
        )? {
            Outcome::Mode(FlowExit::KeepNetwork) => Ok(true),
            Outcome::Exit(_) => Ok(false),
            _ => Err("Invalid network result".into()),
        }
    }

    fn run_screen(
        &mut self,
        context: Option<Context>,
        backend: Option<Arc<dyn Backend>>,
        single_player: bool,
        mut browser: Option<&mut crate::catalog::Browser>,
        purpose: SessionPurpose,
        mode_picker: bool,
    ) -> Result<Outcome> {
        let sender = self.display.begin_screen();
        let mut recheck_snapshot = self.index_task.is_some();
        if let Some(browser) = &mut browser {
            browser.reopen();
            self.poll_browser_index(browser, &mut recheck_snapshot)?;
        }
        let mut preparing = None;
        let result = (|| -> Result<Outcome> {
            let mut flow = if mode_picker {
                let connected = !crate::session::best(&[
                    "nmcli",
                    "-g",
                    "IP4.ADDRESS",
                    "device",
                    "show",
                    "wlan0",
                ])
                .is_empty();
                self.lobby_flow
                    .take()
                    .unwrap_or_else(|| Flow::lobby(context, connected))
            } else if purpose == SessionPurpose::NetworkOnly {
                Flow::network()
            } else {
                Flow::new(context)
            };
            if !single_player {
                flow.manual();
            }
            let mut last_status = 0.0;
            let mut last_scan = 0.0;
            let mut next_refresh = 0.0;
            let mut status_pending = false;
            let mut scan_pending = false;
            while flow.exit_code.is_none() && !self.display.stopped() {
                if let Some(browser) = &mut browser {
                    self.poll_browser_index(browser, &mut recheck_snapshot)?;
                    if browser.exit {
                        return Ok(Outcome::Exit(0));
                    }
                    if let Some(context) = browser.selected_context.take() {
                        return Ok(Outcome::Selected(Box::new(context)));
                    }
                    if let Some(job) = browser.search_job() {
                        let sender = sender.clone();
                        thread::spawn(move || sender.send(Event::Searched(job.run())));
                    }
                    self.display.present(browser.view())?;
                    match self.display.receive() {
                        Ok(Some(Event::ShelfLayout(grid, false))) => browser.shelf_layout(grid),
                        Ok(Some(Event::ListLayout(rows, false))) => browser.list_layout(rows),
                        Ok(Some(Event::Input(action, at))) if at >= browser.input_epoch => {
                            if let Some(entry) = browser.dispatch(&action)? {
                                let sender = sender.clone();
                                preparing = Some(thread::spawn(move || {
                                    let result = entry
                                        .prepare()
                                        .map(Box::new)
                                        .map_err(|error| error.to_string());
                                    sender.send(Event::Prepared(result));
                                }));
                            }
                        }
                        Ok(Some(Event::Prepared(result))) => {
                            if let Some(worker) = preparing.take() {
                                let _ = worker.join();
                            }
                            browser.prepared(result.map(|context| *context));
                        }
                        Ok(Some(Event::Searched(result))) => browser.searched(result),
                        Ok(Some(Event::FrameReady)) => {
                            log(json!({"phase":"frame_submitted","frame_submitted":true}))?
                        }
                        Ok(Some(Event::Eof)) => return Err("Graphical game browser closed".into()),
                        Ok(None) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(error) => return Err(error.into()),
                        _ => {}
                    }
                    continue;
                }
                let now = monotonic();
                flow.expire();
                if let Some(backend) = &backend {
                    if !status_pending && now - last_status >= 0.25 {
                        status_pending = true;
                        last_status = now;
                        let generation = flow.generation;
                        let backend = backend.clone();
                        let sender = sender.clone();
                        thread::spawn(move || {
                            let result = backend.call("status", &json!({})).unwrap_or(Value::Null);
                            sender.send(Event::Status(generation, result));
                        });
                    }
                    if !scan_pending
                        && flow.scans()
                        && (flow.refresh_requested || now - last_scan >= 0.35)
                    {
                        scan_pending = true;
                        last_scan = now;
                        let force = flow.refresh_requested;
                        flow.refresh_requested = false;
                        let refresh = force || now >= next_refresh;
                        if refresh {
                            next_refresh = now + 4.0;
                        }
                        let generation = flow.generation;
                        let backend = backend.clone();
                        let sender = sender.clone();
                        thread::spawn(move || {
                            let result = backend
                                .call(
                                    if refresh { "scan_refresh" } else { "scan" },
                                    &json!({"force":force}),
                                )
                                .unwrap_or_else(
                                    |error| json!({"rooms":[],"scan_error":error.to_string()}),
                                );
                            sender.send(Event::Scan(generation, result));
                        });
                    }
                    if let Some(request) = flow.request.take() {
                        let backend = backend.clone();
                        let sender = sender.clone();
                        thread::spawn(move || {
                            let (result, failed) =
                                match backend.call(request.operation.name(), &request.payload) {
                                    Ok(value) => (value, false),
                                    Err(error) => {
                                        let mut value = backend.failure_snapshot();
                                        value["error"] = error.to_string().into();
                                        (value, true)
                                    }
                                };
                            sender.send(Event::Action(request, result, failed));
                        });
                    }
                }
                let mut view = flow.view();
                view["return_to_launcher"] = self.launcher.into();
                self.display.present(view)?;
                match self.display.receive() {
                    Ok(Some(Event::Input(action, at))) => {
                        if !self.display.stopped() {
                            flow.dispatch(&action, at);
                        }
                    }
                    Ok(Some(Event::FrameReady)) => {
                        log(json!({"phase":"frame_submitted","frame_submitted":true}))?
                    }
                    Ok(Some(Event::Eof)) => {
                        return Err("Graphical menu closed before game handoff".into());
                    }
                    Ok(Some(Event::Status(generation, result))) => {
                        status_pending = false;
                        if !result.is_null() {
                            flow.status(generation, result);
                        }
                    }
                    Ok(Some(Event::Scan(generation, result))) => {
                        scan_pending = false;
                        flow.scan(generation, result);
                    }
                    Ok(Some(Event::Action(request, result, failed))) => {
                        flow.complete(&request, result, failed)
                    }
                    Ok(Some(
                        Event::Prepared(_)
                        | Event::Searched(_)
                        | Event::ShelfLayout(_, _)
                        | Event::ListLayout(_, _)
                        | Event::Launcher(_, _, _)
                        | Event::Wifi(_, _)
                        | Event::Configuration(_, _)
                        | Event::Saves(_, _),
                    )) => {}
                    Ok(None) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            log(
                json!({"phase":"finished","exit_code":flow.exit_code.unwrap_or(0),"last_view":self.display.previous()}),
            )?;
            let outcome = match flow.outcome {
                Some(mode) => Outcome::Mode(mode),
                None => Outcome::Exit(flow.exit_code.unwrap_or(0)),
            };
            if mode_picker && flow.outcome.is_some() {
                flow.resume_lobby();
                self.lobby_flow = Some(flow);
            }
            Ok(outcome)
        })();
        if let Some(worker) = preparing {
            let _ = worker.join();
        }
        if let Err(error) = &result {
            log(json!({"phase":"failed","error":error.to_string()}))?;
        }
        result
    }
}

pub fn run(context: Option<Context>, backend: Option<Arc<dyn Backend>>) -> Result<i32> {
    MenuSession::open()?.run_with_fallback(context, backend, true)
}

pub fn run_network() -> Result<bool> {
    MenuSession::open()?.run_network()
}
