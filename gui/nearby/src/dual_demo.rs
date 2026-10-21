//! Deterministic mirui renderer for the README's dual-handheld animation.

mod feedback;
#[allow(dead_code)]
mod launcher;
#[allow(dead_code)]
mod model;
#[allow(dead_code)]
mod render;
#[allow(dead_code)]
mod settings;
#[allow(dead_code)]
mod theme;

#[path = "../examples/dual_demo/scene.rs"]
mod scene;
#[path = "../examples/dual_demo/story.rs"]
mod story;
#[path = "../examples/dual_demo/capture.rs"]
mod vector_capture;

use mirui::core::cache::MaxSize;
use mirui::core::resource::ResourceManager;
use mirui::prelude::*;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::PhysicalRect;
use std::cell::Cell;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

type SoftwareApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;
thread_local! { static TIME: Cell<u64> = const { Cell::new(0) }; }

fn now_ns() -> u64 {
    TIME.with(Cell::get)
}

fn app(width: u16, height: u16) -> Result<SoftwareApp, String> {
    let surface = FramebufSurface::with_format(
        width,
        height,
        ColorFormat::RGBA8888,
        (|_, _| {}) as fn(&[u8], PhysicalRect),
    );
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    render::install(&mut app)?;
    app.add_resource(mirui::ecs::MonoClock::new(now_ns));
    Ok(app)
}

struct Handheld {
    app: SoftwareApp,
    scene: render::Scene,
    view: model::View,
    palette: theme::Palette,
    press: u8,
}

impl Handheld {
    fn new(view: model::View) -> Result<Self, String> {
        let mut app = app(640, 480)?;
        let scene = render::draw(&mut app, &view)?;
        Ok(Self {
            app,
            scene,
            view,
            palette: theme::Palette::Forest,
            press: 0,
        })
    }

    fn frame(
        &mut self,
        shot: &story::Screen,
        now: u64,
        vector: bool,
    ) -> Result<Texture<'static>, String> {
        if self.palette != shot.palette {
            self.app
                .set_theme(shot.palette.theme())
                .map_err(|e| format!("{e:?}"))?;
            self.palette = shot.palette;
        }
        if self.view != shot.view {
            render::update(&mut self.scene, &shot.view)?;
            self.view = shot.view.clone();
        }
        if self.press != shot.press {
            self.scene.press(now);
            self.press = shot.press;
        }
        render::paint_at(&mut self.app, now)?;
        if vector {
            vector_capture::disable_raster_caches(&mut self.app);
        }
        capture(&mut self.app)
    }
}

fn capture(app: &mut SoftwareApp) -> Result<Texture<'static>, String> {
    app.snapshot()
        .map_err(|e| format!("{e:?}"))?
        .ok_or("Empty snapshot".into())
}

fn write_png(path: &Path, texture: &Texture<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path)?),
        u32::from(texture.width()),
        u32::from(texture.height()),
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder
        .write_header()?
        .write_image_data(texture.read_pixels().map_err(|e| format!("{e:?}"))?.bytes())?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let vector = args.iter().any(|arg| arg == "--scene");
    args.retain(|arg| arg != "--scene");
    let output = args
        .first()
        .ok_or("Usage: dual_demo OUTPUT_DIRECTORY [FRAME_LIMIT] [START_FRAME] [--scene]")?;
    let limit = args
        .get(1)
        .map(|s| s.parse::<u32>())
        .transpose()?
        .unwrap_or(story::FRAMES);
    let start = args
        .get(2)
        .map(|s| s.parse::<u32>())
        .transpose()?
        .unwrap_or(0);
    std::fs::create_dir_all(output)?;
    let first = story::at(0.0);
    let mut screens = [
        Handheld::new(first.screens[0].view.clone())?,
        Handheld::new(first.screens[1].view.clone())?,
    ];
    let mut stage = app(scene::WIDTH, scene::HEIGHT)?;
    stage.add_resource(ResourceManager::new(
        MaxSize::Count(2),
        Texture::owned(1, 1, ColorFormat::RGBA8888).map_err(|e| format!("{e:?}"))?,
    ));
    let state = stage
        .add_model(scene::Promo {
            frame_signal: Signal::new(first.frame),
        })
        .finish();
    let root = stage.spawn_root().id();
    stage.compose(root, scene::stage(state.clone()));
    let mut exporter = vector_capture::Export::default();
    let mut vector_frames = Vec::new();
    for index in 0..(start.saturating_add(limit)).min(story::FRAMES) {
        let now = u64::from(index) * 1000 / u64::from(story::FPS);
        TIME.with(|clock| clock.set(now * 1_000_000));
        let shot = story::at(now as f32 / 1000.0);
        for (side, screen) in screens.iter_mut().enumerate() {
            let texture = screen.frame(&shot.screens[side], now, vector)?;
            stage
                .world
                .resource::<ResourceManager<Texture<'static>>>()
                .unwrap()
                .add_static(scene::SCREEN_KEYS[side], texture);
        }
        state.present(shot.frame);
        stage.tick();
        let texture = capture(&mut stage)?;
        if index >= start {
            write_png(&Path::new(output).join(format!("{index:04}.png")), &texture)?;
            if vector {
                vector_frames.push(serde_json::json!({
                    "host": exporter.capture(&screens[0].app, false)?,
                    "guest": exporter.capture(&screens[1].app, false)?,
                    "stage": exporter.capture(&stage, true)?,
                }));
            }
        }
        if index % story::FPS == 0 {
            println!("Frame {index}/{}", story::FRAMES);
        }
    }
    std::fs::write(
        Path::new(output).join("animation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "width": scene::WIDTH,
            "height": scene::HEIGHT,
            "fps": story::FPS,
            "frames": limit.min(story::FRAMES.saturating_sub(start)),
            "start": start,
        }))?,
    )?;
    if vector {
        serde_json::to_writer(
            BufWriter::new(File::create(Path::new(output).join("scenes.json"))?),
            &serde_json::json!({
                "width":scene::WIDTH, "height":scene::HEIGHT, "fps":story::FPS,
                "frames":vector_frames, "definitions":exporter.definitions,
            }),
        )?;
    }
    Ok(())
}
