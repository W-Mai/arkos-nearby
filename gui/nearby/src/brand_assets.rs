//! Export product artwork from the application's mirui components.

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
#[path = "../examples/dual_demo/capture.rs"]
mod vector_capture;

use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::PhysicalRect;
use serde_json::{Value, json};
use std::path::Path;

type SoftwareApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn capture(name: bool, exporter: &mut vector_capture::Export) -> Result<Value, String> {
    let surface = FramebufSurface::with_format(
        if name { 96 } else { 48 },
        48,
        ColorFormat::RGBA8888,
        (|_, _| {}) as fn(&[u8], PhysicalRect),
    );
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    render::install(&mut app)?;
    let root = app.spawn_root().id();
    if name {
        app.compose(root, render::branding::wordmark(ColorToken::Primary));
    } else {
        app.compose(root, render::branding::logo(48));
    }
    app.tick();
    vector_capture::disable_raster_caches(&mut app);
    app.render().map_err(|error| format!("{error:?}"))?;
    exporter.capture(&app, false)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("Pass an output JSON path")?;
    let mut exporter = vector_capture::Export::default();
    let logo = capture(false, &mut exporter)?;
    let name = capture(true, &mut exporter)?;
    std::fs::write(
        Path::new(&output),
        serde_json::to_vec_pretty(&json!({
            "brand": howff_menu::brand::NAME,
            "logo": logo,
            "name": name,
            "definitions": exporter.definitions,
        }))?,
    )?;
    Ok(())
}
