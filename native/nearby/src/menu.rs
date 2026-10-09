use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use crate::context::{Context, INSTALL};
use crate::flow::{Flow, Page, Request};
use crate::system::{mkdir, monotonic, private_write, random_id, wall_time, write};
use crate::{Result, require};

pub trait Backend: Send + Sync + 'static {
    fn call(&self, action: &str, payload: &Value) -> Result<Value>;
    fn failure_snapshot(&self) -> Value;
}

enum Event {
    Input(String, f64),
    FrameReady,
    Eof,
    Status(u64, Value),
    Scan(u64, Value),
    Action(Request, Value, bool),
    Prepared(std::result::Result<Box<Context>, String>),
}

enum Outcome {
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

const KEYS: &str = "a = enter\nb = esc\nback = esc\nx = r\nstart = enter\nup = up\ndown = down\nleft = left\nright = right\nl1 = pageup\nr1 = pagedown\nl2 = pageup\nr2 = pagedown\nleft_analog_up = up\nleft_analog_down = down\nleft_analog_left = left\nleft_analog_right = right\nright_analog_up = up\nright_analog_down = down\nright_analog_left = left\nright_analog_right = right\ndeadzone_triggers = 32767\n";

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn run(context: Option<Context>, backend: Option<Arc<dyn Backend>>) -> Result<i32> {
    run_with_fallback(context, backend, true)
}
pub fn run_with_fallback(
    context: Option<Context>,
    backend: Option<Arc<dyn Backend>>,
    single_player: bool,
) -> Result<i32> {
    match run_screen(context, backend, single_player, None)? {
        Outcome::Exit(code) => Ok(code),
        Outcome::Selected(_) => Err("Room flow returned a game selection".into()),
    }
}

pub fn choose_game(browser: &mut crate::catalog::Browser) -> Result<Option<Context>> {
    match run_screen(None, None, false, Some(browser))? {
        Outcome::Selected(context) => Ok(Some(*context)),
        Outcome::Exit(_) => Ok(None),
    }
}

fn run_screen(
    context: Option<Context>,
    backend: Option<Arc<dyn Backend>>,
    single_player: bool,
    mut browser: Option<&mut crate::catalog::Browser>,
) -> Result<Outcome> {
    let root = Path::new("/run/arkos-nearby-rust/ui");
    mkdir(root)?;
    let directory = Temporary(root.join(random_id()?));
    mkdir(&directory.0)?;
    let keys = directory.0.join("keys.gptk");
    private_write(&keys, KEYS.as_bytes())?;
    let logs = Path::new(INSTALL).join("logs");
    mkdir(&logs)?;
    log(json!({"phase":"gui-spawn"}))?;
    let mut mapper = OwnedChild(
        Command::new("/opt/inttools/gptokeyb")
            .args(["-c", keys.to_str().ok_or("Invalid key mapping path")?])
            .env("TERM", "linux")
            .env("LANG", "C.UTF-8")
            .env("LC_ALL", "C.UTF-8")
            .env(
                "SDL_GAMECONTROLLERCONFIG_FILE",
                "/opt/inttools/gamecontrollerdb.txt",
            )
            .stdout(Stdio::from(File::create(logs.join("native-mapper.log"))?))
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut gui = OwnedChild(
        Command::new(format!("{INSTALL}/arkos-nearby-gui"))
            .env("TERM", "linux")
            .env("LANG", "C.UTF-8")
            .env("LC_ALL", "C.UTF-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(File::create(logs.join("native-gui.log"))?))
            .spawn()?,
    );
    let mut stdin = gui.0.stdin.take().ok_or("GUI has no input channel")?;
    let stdout = gui.0.stdout.take().ok_or("GUI has no output channel")?;
    let (sender, receiver) = mpsc::channel();
    let input_sender = sender.clone();
    let reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            let result = (&mut reader).take(16385).read_until(b'\n', &mut line);
            if !matches!(result,Ok(count) if count > 0 && count <= 16384) {
                break;
            }
            if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                let event = if value == json!({"event":"frame_ready"}) {
                    Some(Event::FrameReady)
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
                            )
                        })
                        .map(|s| Event::Input(s.into(), monotonic()))
                };
                if let Some(event) = event {
                    if input_sender.send(event).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = input_sender.send(Event::Eof);
    });
    let stopping = Arc::new(AtomicBool::new(false));
    let mut signals = Vec::new();
    for signal in [libc::SIGTERM, libc::SIGHUP] {
        signals.push(signal_hook::flag::register(signal, stopping.clone())?);
    }
    let mut preparing = None;
    let result = (|| -> Result<Outcome> {
        let mut flow = Flow::new(context);
        if !single_player {
            flow.manual();
        }
        let mut previous = Value::Null;
        let mut last_status = 0.0;
        let mut last_scan = 0.0;
        let mut next_refresh = 0.0;
        let mut status_pending = false;
        let mut scan_pending = false;
        while flow.exit_code.is_none() && !stopping.load(Ordering::Relaxed) {
            if let Some(browser) = &mut browser {
                if browser.exit {
                    return Ok(Outcome::Exit(0));
                }
                if let Some(context) = browser.selected_context.take() {
                    return Ok(Outcome::Selected(Box::new(context)));
                }
                let view = browser.view();
                if view != previous {
                    serde_json::to_writer(&mut stdin, &view)?;
                    stdin.write_all(b"\n")?;
                    stdin.flush()?;
                    previous = view;
                }
                require(
                    mapper.0.try_wait()?.is_none(),
                    "The owned gamepad mapper stopped",
                )?;
                match receiver.recv_timeout(Duration::from_millis(20)) {
                    Ok(Event::Input(action, at)) if at >= browser.input_epoch => {
                        if let Some(entry) = browser.dispatch(&action)? {
                            let sender = sender.clone();
                            preparing = Some(thread::spawn(move || {
                                let result = entry
                                    .prepare()
                                    .map(Box::new)
                                    .map_err(|error| error.to_string());
                                let _ = sender.send(Event::Prepared(result));
                            }));
                        }
                    }
                    Ok(Event::Prepared(result)) => {
                        if let Some(worker) = preparing.take() {
                            let _ = worker.join();
                        }
                        browser.prepared(result.map(|context| *context));
                    }
                    Ok(Event::FrameReady) => {
                        log(json!({"phase":"frame_submitted","frame_submitted":true}))?
                    }
                    Ok(Event::Eof) => return Err("Graphical game browser closed".into()),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
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
                        let _ = sender.send(Event::Status(generation, result));
                    });
                }
                if !scan_pending
                    && flow.page == Page::Rooms
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
                        let _ = sender.send(Event::Scan(generation, result));
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
                        let _ = sender.send(Event::Action(request, result, failed));
                    });
                }
            }
            let view = flow.view();
            if view != previous {
                serde_json::to_writer(&mut stdin, &view)?;
                stdin.write_all(b"\n")?;
                stdin.flush()?;
                previous = view;
            }
            require(
                mapper.0.try_wait()?.is_none(),
                "The owned gamepad mapper stopped",
            )?;
            match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(Event::Input(action, at)) => {
                    if !stopping.load(Ordering::Relaxed) {
                        flow.dispatch(&action, at);
                    }
                }
                Ok(Event::FrameReady) => {
                    log(json!({"phase":"frame_submitted","frame_submitted":true}))?
                }
                Ok(Event::Eof) => return Err("Graphical menu closed before game handoff".into()),
                Ok(Event::Status(generation, result)) => {
                    status_pending = false;
                    if !result.is_null() {
                        flow.status(generation, result);
                    }
                }
                Ok(Event::Scan(generation, result)) => {
                    scan_pending = false;
                    flow.scan(generation, result);
                }
                Ok(Event::Action(request, result, failed)) => {
                    flow.complete(&request, result, failed)
                }
                Ok(Event::Prepared(_)) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(error) => return Err(error.into()),
            }
        }
        log(
            json!({"phase":"finished","exit_code":flow.exit_code.unwrap_or(0),"last_view":previous}),
        )?;
        Ok(Outcome::Exit(flow.exit_code.unwrap_or(0)))
    })();
    if let Some(worker) = preparing {
        let _ = worker.join();
    }
    for signal in signals {
        signal_hook::low_level::unregister(signal);
    }
    drop(stdin);
    drop(gui);
    drop(mapper);
    let _ = reader.join();
    if let Err(error) = &result {
        log(json!({"phase":"failed","error":error.to_string()}))?;
    }
    result
}
