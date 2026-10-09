mod model;
mod motion;
#[cfg(target_os = "linux")]
mod platform;
mod render;

use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use std::fs::File;
use std::io::BufWriter;
#[cfg(target_os = "linux")]
use std::io::Write;
use std::path::Path;

const FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_nanos(1_000_000_000 / 60);

#[derive(Default)]
struct Timings {
    samples: Vec<f64>,
    count: usize,
    total: f64,
    maximum: f64,
}

impl Timings {
    fn add(&mut self, value: f64) {
        if self.samples.len() < 240 {
            self.samples.push(value);
        } else {
            self.samples[self.count % 240] = value;
        }
        self.count += 1;
        self.total += value;
        self.maximum = self.maximum.max(value);
    }

    fn report(&self) -> serde_json::Value {
        let mut ordered = self.samples.clone();
        ordered.sort_by(f64::total_cmp);
        serde_json::json!({
            "count":self.count,
            "mean_ms":self.total / self.count.max(1) as f64,
            "p95_ms":ordered.get(ordered.len().saturating_sub(1) * 95 / 100),
            "max_ms":self.maximum,
        })
    }
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct FrameMetrics {
    full: Timings,
    dirty: Timings,
    cadence: Timings,
    previous: Option<std::time::Instant>,
}

#[cfg(target_os = "linux")]
impl FrameMetrics {
    fn record(&mut self, start: std::time::Instant, full: bool) {
        if let Some(previous) = self.previous {
            self.cadence
                .add(start.duration_since(previous).as_secs_f64() * 1000.0);
        }
        self.previous = Some(start);
        let target = if full {
            &mut self.full
        } else {
            &mut self.dirty
        };
        target.add(start.elapsed().as_secs_f64() * 1000.0);
    }
}

#[cfg(target_os = "linux")]
impl Drop for FrameMetrics {
    fn drop(&mut self) {
        eprintln!(
            "{}",
            serde_json::json!({
                "ui_frame_metrics":{"full":self.full.report(),"dirty":self.dirty.report(),"cadence":self.cadence.report()},
                "includes_framebuffer_write":true,
            })
        );
    }
}

fn snapshot(input: &Path, output: &Path, time_ms: Option<u64>) -> Result<(), String> {
    let view: model::View = serde_json::from_reader(File::open(input).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let surface = FramebufSurface::with_format(640, 480, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    render::install_fonts(&mut app)?;
    let scene = render::draw(&mut app, &view)?;
    if let Some(now) = time_ms {
        let mut motion = motion::Motion::default();
        motion.retarget(&view.page, scene.focus_rect, 0);
        motion.press(0);
        motion.paint(&mut app, &scene, now)?;
    }
    let file = BufWriter::new(File::create(output).map_err(|e| e.to_string())?);
    let mut encoder = png::Encoder::new(file, 640, 480);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(app.backend.framebuffer().buf.as_slice())
        .map_err(|e| e.to_string())
}

fn benchmark() -> Result<(), String> {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;
    let mut reports = Vec::new();
    for name in ["focus", "room_focus", "waiting", "status"] {
        let damage = Rc::new(RefCell::new(0_u32));
        let output = Rc::clone(&damage);
        let surface =
            FramebufSurface::with_format(640, 480, ColorFormat::BGRA8888, move |_, area| {
                *output.borrow_mut() += u32::from(area.width()) * u32::from(area.height());
            });
        let mut app = App::new(surface);
        app.with_default_widgets().with_default_systems();
        render::install_fonts(&mut app)?;
        let mut view = model::View::default();
        if name == "room_focus" {
            view.page = model::Page::Rooms;
            view.rooms = (0..4)
                .map(|_| model::RoomRow {
                    name: "房间".into(),
                    detail: "同一游戏，可以加入".into(),
                    signal: 75,
                    available: true,
                })
                .collect();
        } else if name != "focus" {
            view.page = model::Page::Host;
        }
        let start = Instant::now();
        let mut scene = render::draw(&mut app, &view)?;
        let full_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut motion = motion::Motion::default();
        motion.retarget(&view.page, scene.focus_rect, 0);
        let mut timing = Timings::default();
        let mut pixels = Timings::default();
        for frame in 0..120 {
            let now = (frame * FRAME_INTERVAL.as_nanos() / 1_000_000) as u64;
            *damage.borrow_mut() = 0;
            let start = Instant::now();
            let previous = view.clone();
            if matches!(name, "focus" | "room_focus") && frame % 15 == 0 {
                view.selected = 1 - view.selected;
                render::update(&mut app, &mut scene, &view);
                motion.retarget(&view.page, render::focus_rect(&view), now);
            }
            if name == "status" {
                view.status = if frame % 2 == 0 {
                    "请稍候"
                } else {
                    "等待好友准备"
                }
                .into();
                render::update(&mut app, &mut scene, &view);
            }
            if !render::can_update(&previous, &view) {
                return Err("Benchmark updates must retain their scene".into());
            }
            if motion.active(&scene, now) {
                motion.paint(&mut app, &scene, now)?;
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            timing.add(elapsed);
            pixels.add(f64::from(*damage.borrow()));
        }
        reports.push(serde_json::json!({"case":name,"full_page_ms":full_ms,"render":timing.report(),"dirty_pixels":{"mean":pixels.total / pixels.count.max(1) as f64,"max":pixels.maximum}}));
    }
    println!(
        "{}",
        serde_json::json!({
            "frames_per_case":120,"target_interval_ms":FRAME_INTERVAL.as_secs_f64()*1000.0,
            "includes_framebuffer_write":false,"cases":reports,
        })
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn live() -> Result<(), String> {
    use std::cell::RefCell;
    use std::io::{self, BufRead};
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    platform::record_init("before_stdin_thread", serde_json::json!({}))
        .map_err(|error| error.to_string())?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            let Ok(view) = serde_json::from_str::<model::View>(&line) else {
                break;
            };
            if tx.send(view).is_err() {
                break;
            }
        }
    });
    platform::record_init("after_thread", serde_json::json!({}))
        .map_err(|error| error.to_string())?;
    platform::record_init("before_terminal", serde_json::json!({"path":"/dev/tty1"}))
        .map_err(|error| error.to_string())?;
    let mut terminal = platform::Terminal::open().map_err(|e| e.to_string())?;
    platform::record_init("after_terminal", serde_json::json!({}))
        .map_err(|error| error.to_string())?;
    platform::record_init("before_fb_open", serde_json::json!({"path":"/dev/fb0"}))
        .map_err(|error| error.to_string())?;
    let mut panel = platform::Framebuffer::open().map_err(|e| e.to_string())?;
    let flush_error = Rc::new(RefCell::new(None));
    let flush_result = Rc::clone(&flush_error);
    let quit = Arc::new(AtomicBool::new(false));
    for event in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(event, Arc::clone(&quit)).map_err(|e| e.to_string())?;
    }
    let surface = FramebufSurface::with_format(
        panel.width,
        panel.height,
        panel.format,
        move |pixels, area| {
            if flush_result.borrow().is_some() {
                return;
            }
            if let Err(error) = panel.flush(pixels, area) {
                *flush_result.borrow_mut() = Some(error);
            }
        },
    );
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    platform::record_init("before_fonts", serde_json::json!({}))
        .map_err(|error| error.to_string())?;
    render::install_fonts(&mut app)?;
    platform::record_init("after_fonts", serde_json::json!({}))
        .map_err(|error| error.to_string())?;
    let mut view: Option<model::View> = None;
    let mut scene = None;
    let mut motion = motion::Motion::default();
    let epoch = Instant::now();
    let mut next_frame = Instant::now();
    let mut was_active = false;
    let mut first_frame_sent = false;
    let mut metrics = FrameMetrics::default();
    while !quit.load(Ordering::Relaxed) {
        let mut latest = None;
        loop {
            match rx.try_recv() {
                Ok(next) => latest = Some(next),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            }
        }
        if let Some(latest) = latest {
            if view.as_ref() != Some(&latest) {
                let frame_start = Instant::now();
                let full;
                if view
                    .as_ref()
                    .is_some_and(|old| render::can_update(old, &latest))
                    && let Some(current_scene) = scene.as_mut()
                {
                    full = false;
                    render::update(&mut app, current_scene, &latest);
                    motion.retarget(
                        &latest.page,
                        render::focus_rect(&latest),
                        epoch.elapsed().as_millis() as u64,
                    );
                    motion.paint(&mut app, current_scene, epoch.elapsed().as_millis() as u64)?;
                    if let Some(error) = flush_error.borrow_mut().take() {
                        return Err(format!("Framebuffer pwrite: {error}"));
                    }
                } else {
                    full = true;
                    let next_scene = render::build(&mut app, &latest)?;
                    motion.retarget(
                        &latest.page,
                        next_scene.focus_rect,
                        epoch.elapsed().as_millis() as u64,
                    );
                    motion.apply(&mut app, &next_scene, epoch.elapsed().as_millis() as u64)?;
                    app.render()
                        .map_err(|error| format!("Menu frame: {error:?}"))?;
                    scene = Some(next_scene);
                }
                metrics.record(frame_start, full);
                if let Some(error) = flush_error.borrow_mut().take() {
                    return Err(format!("Framebuffer pwrite: {error}"));
                }
                view = Some(latest);
                next_frame = frame_start + FRAME_INTERVAL;
                if !first_frame_sent {
                    platform::record_init("after_frame", serde_json::json!({}))
                        .map_err(|error| error.to_string())?;
                    let mut output = io::stdout().lock();
                    serde_json::to_writer(&mut output, &serde_json::json!({"event":"frame_ready"}))
                        .map_err(|error| error.to_string())?;
                    writeln!(output).map_err(|error| error.to_string())?;
                    output.flush().map_err(|error| error.to_string())?;
                    first_frame_sent = true;
                }
            }
        }
        if let Some(scene) = scene.as_ref() {
            let now = epoch.elapsed().as_millis() as u64;
            let active = motion.active(scene, now);
            if (active || was_active) && Instant::now() >= next_frame {
                let frame_start = Instant::now();
                next_frame = frame_start + FRAME_INTERVAL;
                motion.paint(&mut app, scene, now)?;
                metrics.record(frame_start, false);
                if let Some(error) = flush_error.borrow_mut().take() {
                    return Err(format!("Framebuffer pwrite: {error}"));
                }
                was_active = active;
            }
            if !active && !was_active {
                metrics.previous = None;
            }
        }
        for action in terminal.poll().map_err(|e| e.to_string())? {
            if matches!(action, model::Action::Confirm | model::Action::Refresh) {
                motion.press(epoch.elapsed().as_millis() as u64);
            }
            let mut output = io::stdout().lock();
            serde_json::to_writer(&mut output, &action).map_err(|e| e.to_string())?;
            writeln!(output).map_err(|e| e.to_string())?;
            output.flush().map_err(|e| e.to_string())?;
        }
        let active = scene
            .as_ref()
            .is_some_and(|current| motion.active(current, epoch.elapsed().as_millis() as u64));
        let sleep = if active || was_active {
            next_frame
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(2))
        } else {
            Duration::from_millis(8)
        };
        if !sleep.is_zero() {
            thread::sleep(sleep);
        }
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [argument] if argument == "--version" => {
            println!("arkos-nearby-gui 0.1.0 (mirui 0.47.0)");
            Ok(())
        }
        #[cfg(target_os = "linux")]
        [argument] if argument == "--probe" => {
            let panel = platform::Framebuffer::open().map_err(|e| e.to_string())?;
            let mut app = App::headless(640, 480);
            app.with_default_widgets().with_default_systems();
            render::install_fonts(&mut app)?;
            println!(
                "{}",
                serde_json::json!({"width":panel.width,"height":panel.height,"format":format!("{:?}",panel.format),"font":true})
            );
            Ok(())
        }
        [mode, input, output] if mode == "--snapshot" => {
            snapshot(Path::new(input), Path::new(output), None)
        }
        [mode, input, output, time] if mode == "--snapshot" => {
            let time = time.parse().map_err(|_| "Invalid animation time")?;
            snapshot(Path::new(input), Path::new(output), Some(time))
        }
        [argument] if argument == "--benchmark" => benchmark(),
        [] => {
            #[cfg(target_os = "linux")]
            return live();
            #[cfg(not(target_os = "linux"))]
            Err("Use --snapshot on this host".into())
        }
        _ => Err("Usage: arkos-nearby-gui [--version | --snapshot input.json output.png]".into()),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
