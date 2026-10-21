mod feedback;
// Host capture renders launcher state without a physical input source.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
mod launcher;
mod model;
#[cfg(target_os = "linux")]
mod platform;
mod render;
mod settings;
#[cfg(any(target_os = "linux", test))]
mod sound;
mod theme;

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
                "includes_framebuffer_write":!cfg!(feature = "gpu"),"backend":if cfg!(feature = "gpu") { "sdl-gpu" } else { "framebuffer" },
            })
        );
    }
}

fn snapshot(
    input: &Path,
    output: &Path,
    time_ms: Option<u64>,
    size: [u16; 2],
) -> Result<(), String> {
    let view: model::View = serde_json::from_reader(File::open(input).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let surface = FramebufSurface::with_format(size[0], size[1], ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    render::install(&mut app)?;
    if let Some(palette) = std::env::var("ARKOS_NEARBY_THEME")
        .ok()
        .and_then(|value| theme::Palette::parse(&value))
    {
        app.set_theme(palette.theme())
            .map_err(|error| format!("Theme: {error:?}"))?;
    }
    let scene = render::draw(&mut app, &view)?;
    if let Some(now) = time_ms {
        scene.press(0);
        render::paint_at(&mut app, now)?;
    }
    let file = BufWriter::new(File::create(output).map_err(|e| e.to_string())?);
    let mut encoder = png::Encoder::new(file, u32::from(size[0]), u32::from(size[1]));
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let texture = app.backend.framebuffer();
    let pixels = texture.read_pixels().map_err(|e| format!("{e:?}"))?;
    writer
        .write_image_data(pixels.bytes())
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
        render::install(&mut app)?;
        let mut view = model::View::default();
        if name == "room_focus" {
            view.page = model::Page::Rooms;
            view.rooms = (0..4)
                .map(|_| model::RoomRow {
                    key: String::new(),
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
        let mut timing = Timings::default();
        let mut pixels = Timings::default();
        for frame in 0..120 {
            let now = (frame * FRAME_INTERVAL.as_nanos() / 1_000_000) as u64;
            *damage.borrow_mut() = 0;
            let start = Instant::now();
            if matches!(name, "focus" | "room_focus") && frame % 15 == 0 {
                view.selected = 1 - view.selected;
                render::update(&mut scene, &view)?;
            }
            if name == "status" {
                view.status = if frame % 2 == 0 {
                    "请稍候"
                } else {
                    "等待好友准备"
                }
                .into();
                render::update(&mut scene, &view)?;
            }
            render::paint_at(&mut app, now)?;
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
    platform::record_init(
        "before_display_open",
        serde_json::json!({"backend":if cfg!(feature = "gpu") { "sdl-gpu" } else { "framebuffer" }}),
    )
        .map_err(|error| error.to_string())?;
    #[cfg(not(feature = "gpu"))]
    let mut panel = platform::Framebuffer::open().map_err(|e| e.to_string())?;
    let flush_error = Rc::new(RefCell::new(None::<std::io::Error>));
    #[cfg(not(feature = "gpu"))]
    let flush_result = Rc::clone(&flush_error);
    let quit = Arc::new(AtomicBool::new(false));
    for event in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(event, Arc::clone(&quit)).map_err(|e| e.to_string())?;
    }
    #[cfg(not(feature = "gpu"))]
    let mut app = {
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
        App::new(surface)
    };
    #[cfg(feature = "gpu")]
    let mut app = {
        use mirui::surface::sdl_gpu::{
            SdlGpuConfig, SdlGpuDriver, SdlGpuFactory, SdlGpuSurface, SdlWindowMode,
        };
        let surface = SdlGpuSurface::with_config(
            "Nearby Multiplayer",
            640,
            480,
            SdlGpuConfig {
                driver: SdlGpuDriver::OpenGles2 { samples: 0 },
                window: SdlWindowMode::FullscreenDesktop,
                vsync: true,
            },
        )
        .map_err(|e| e.to_string())?;
        eprintln!(
            "{}",
            serde_json::json!({"renderer":surface.renderer_name(),"backend":"sdl-gpu","video":std::env::var("SDL_VIDEODRIVER").ok()})
        );
        App::with_factory(surface, SdlGpuFactory::new())
    };
    app.with_default_widgets().with_default_systems();
    platform::record_init("before_fonts", serde_json::json!({}))
        .map_err(|error| error.to_string())?;
    render::install(&mut app)?;
    app.add_plugin(mirui::app::plugins::StdInstantClockPlugin);
    let preferences = sound::install(&mut app);
    let sound = sound::register(&mut app)?;
    let launcher = render::launcher(&mut app);
    let intents = Rc::new(RefCell::new(Vec::new()));
    let pending = Rc::clone(&intents);
    launcher
        .on_effect(move |intent: &launcher::Intent| pending.borrow_mut().push(intent.clone()))
        .map_err(|error| format!("Launcher effects: {error:?}"))?;
    let shelf_layouts = Rc::new(RefCell::new(Vec::new()));
    let pending_layouts = Rc::clone(&shelf_layouts);
    render::subscribe_shelf_layout(&app, move |request| {
        pending_layouts.borrow_mut().push(*request)
    })?;
    let mut gamepad = launcher::input::Input::open().map_err(|error| error.to_string())?;
    let settings = render::settings(&mut app);
    let palette = theme::Palette::load(Path::new(theme::SETTINGS));
    settings.restore(palette, preferences);
    app.set_theme(palette.theme())
        .map_err(|error| format!("Theme: {error:?}"))?;
    let confirmation = app.audio().ok_or("Audio plugin is unavailable")?;
    settings::connect(
        &settings,
        app.audio().ok_or("Audio plugin is unavailable")?,
        Path::new(theme::SETTINGS).into(),
        Path::new(settings::SOUND_FILE).into(),
        move || sound::play(&confirmation, Some(sound::Cue::Confirm)),
    )?;
    platform::record_init("after_fonts", serde_json::json!({}))
        .map_err(|error| error.to_string())?;
    let mut view: Option<model::View> = None;
    let mut scene = None;
    let mut next_frame = Instant::now();
    let mut first_frame_sent = false;
    let mut metrics = FrameMetrics::default();
    while !quit.load(Ordering::Relaxed) {
        let mut rendered = false;
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
                sound.present(&latest, render::now(&app));
                let frame_start = Instant::now();
                let full = scene.is_none();
                if let Some(current_scene) = scene.as_mut() {
                    render::update(current_scene, &latest)?;
                    render::tick(&mut app)?;
                } else {
                    let next_scene = render::draw(&mut app, &latest)?;
                    scene = Some(next_scene);
                }
                metrics.record(frame_start, full);
                rendered = true;
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
        if scene.is_some() && !rendered && Instant::now() >= next_frame {
            let frame_start = Instant::now();
            next_frame = frame_start + FRAME_INTERVAL;
            render::tick(&mut app)?;
            metrics.record(frame_start, false);
            if let Some(error) = flush_error.borrow_mut().take() {
                return Err(format!("Framebuffer pwrite: {error}"));
            }
        }
        for request in shelf_layouts.borrow_mut().drain(..) {
            let mut output = io::stdout().lock();
            serde_json::to_writer(&mut output, &request).map_err(|e| e.to_string())?;
            writeln!(output).map_err(|e| e.to_string())?;
            output.flush().map_err(|e| e.to_string())?;
        }
        let mut actions = terminal.poll().map_err(|e| e.to_string())?;
        if !actions.is_empty() {
            launcher.cancel();
        }
        if let Some(input) = &mut gamepad {
            actions.extend(
                input
                    .poll(&launcher, render::now(&app))
                    .map_err(|e| e.to_string())?,
            );
        }
        for intent in intents.borrow_mut().drain(..) {
            match intent {
                launcher::Intent::Action(action) => actions.push(action),
                launcher::Intent::Game { key, multiplayer } => {
                    sound.input(model::Action::Confirm, render::now(&app));
                    if let Some(scene) = &scene {
                        scene.press(render::now(&app));
                    }
                    let mut output = io::stdout().lock();
                    serde_json::to_writer(&mut output, &serde_json::json!({"event":"launcher", "action":if multiplayer {"multiplayer"}else{"launch"}, "key":key})).map_err(|e|e.to_string())?;
                    writeln!(output).map_err(|e| e.to_string())?;
                    output.flush().map_err(|e| e.to_string())?;
                }
            }
        }
        for mut action in actions {
            let library = view
                .as_ref()
                .is_some_and(|view| view.page == model::Page::Games);
            if action == model::Action::Theme && library {
                action = model::Action::Auxiliary;
            }
            if action == model::Action::Start && !library {
                action = model::Action::Confirm;
            }
            if matches!(action, model::Action::Theme) {
                if let Some(scene) = scene.as_mut() {
                    render::cycle_theme(&mut app, scene)?;
                }
                continue;
            }
            if action == model::Action::Confirm
                && let Some(current) = &view
                && current.launcher.overlay_kind == "system"
            {
                match current.launcher.selected {
                    0 => {
                        settings.cycle_theme();
                        app.set_theme(settings.palette().theme())
                            .map_err(|e| format!("Theme: {e:?}"))?;
                    }
                    1 => {
                        let mut preferences = settings.sound();
                        preferences.enabled = !preferences.enabled;
                        settings.set_sound(preferences);
                    }
                    _ => {}
                }
            }
            sound.input(action, render::now(&app));
            if let Some(scene) = scene.as_ref() {
                match action {
                    model::Action::Back => scene.navigation_intent(true),
                    model::Action::Confirm => scene.navigation_intent(false),
                    _ => {}
                }
            }
            if matches!(
                action,
                model::Action::Confirm | model::Action::Refresh | model::Action::Auxiliary
            ) && let Some(scene) = scene.as_ref()
            {
                scene.press(render::now(&app));
            }
            let mut output = io::stdout().lock();
            serde_json::to_writer(&mut output, &action).map_err(|e| e.to_string())?;
            writeln!(output).map_err(|e| e.to_string())?;
            output.flush().map_err(|e| e.to_string())?;
        }
        let sleep = next_frame
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(2));
        if !sleep.is_zero() {
            thread::sleep(sleep);
        }
    }
    Ok(())
}

#[cfg(all(target_os = "linux", feature = "gpu"))]
fn gpu_probe(output: &Path, input: Option<&Path>) -> Result<(), String> {
    use mirui::surface::sdl_gpu::{
        SdlGpuConfig, SdlGpuDriver, SdlGpuFactory, SdlGpuSurface, SdlWindowMode,
    };
    let surface = SdlGpuSurface::with_config(
        "Nearby Multiplayer",
        640,
        480,
        SdlGpuConfig {
            driver: SdlGpuDriver::OpenGles2 { samples: 0 },
            window: SdlWindowMode::FullscreenDesktop,
            vsync: true,
        },
    )
    .map_err(|e| e.to_string())?;
    let renderer = surface.renderer_name();
    if renderer != "opengles2" {
        return Err(format!("Expected GLES acceleration, found {renderer}"));
    }
    let mut app = App::with_factory(surface, SdlGpuFactory::new());
    app.with_default_widgets().with_default_systems();
    render::install(&mut app)?;
    let view = if let Some(input) = input {
        serde_json::from_reader(File::open(input).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else {
        model::View {
            page: model::Page::Mode,
            ..Default::default()
        }
    };
    let start = std::time::Instant::now();
    let _scene = render::build(&mut app, &view)?;
    let texture = app
        .snapshot()
        .map_err(|e| format!("{e:?}"))?
        .ok_or("No GPU frame captured")?;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    let pixels = texture.read_pixels().map_err(|e| format!("{e:?}"))?;
    let bytes = pixels.bytes();
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(output).map_err(|e| e.to_string())?),
        640,
        480,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(|e| e.to_string())?
        .write_image_data(bytes)
        .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({"renderer":renderer,"first_frame_ms":elapsed,"capture":output,"bytes":bytes.len()})
    );
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [argument] if argument == "--version" => {
            println!("arkos-nearby-gui 0.1.0 (mirui 0.51.0)");
            Ok(())
        }
        #[cfg(all(target_os = "linux", not(feature = "gpu")))]
        [argument] if argument == "--probe" => {
            let panel = platform::Framebuffer::open().map_err(|e| e.to_string())?;
            let mut app = App::headless(panel.width, panel.height);
            app.with_default_widgets().with_default_systems();
            render::install(&mut app)?;
            println!(
                "{}",
                serde_json::json!({"width":panel.width,"height":panel.height,"format":format!("{:?}",panel.format),"font":true})
            );
            Ok(())
        }
        #[cfg(all(target_os = "linux", feature = "gpu"))]
        [mode, output] if mode == "--gpu-probe" => gpu_probe(Path::new(output), None),
        #[cfg(all(target_os = "linux", feature = "gpu"))]
        [mode, input, output] if mode == "--gpu-probe" => {
            gpu_probe(Path::new(output), Some(Path::new(input)))
        }
        #[cfg(all(target_os = "linux", feature = "sound"))]
        [argument] if argument == "--audio-probe" => sound::probe(),
        [mode, input, output] if mode == "--snapshot" => {
            snapshot(Path::new(input), Path::new(output), None, [640, 480])
        }
        [mode, input, output, time] if mode == "--snapshot" => {
            let time = time.parse().map_err(|_| "Invalid animation time")?;
            snapshot(Path::new(input), Path::new(output), Some(time), [640, 480])
        }
        [mode, input, output, width, height] if mode == "--snapshot" => {
            let width = width.parse().map_err(|_| "Invalid viewport width")?;
            let height = height.parse().map_err(|_| "Invalid viewport height")?;
            if width == 0 || height == 0 {
                return Err("Viewport dimensions must be positive".into());
            }
            snapshot(Path::new(input), Path::new(output), None, [width, height])
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
