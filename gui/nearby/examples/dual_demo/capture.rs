//! Capture prepared mirui scenes and preserve their draw order and clipping.

use super::SoftwareApp;
use mirui::prelude::*;
use mirui::render::backends::scene::SceneRenderer;
use mirui::render::font::{Font, GlyphId, GlyphKind};
use mirui::render::path::{Path, PathCmd};
use mirui::render::raster::FillRule;
use mirui::render::renderer::{DrawRequest, RenderError, RenderRoute, Renderer};
use mirui::render::scene::record::ResourceResolver;
use mirui::render::scene::{Paint, ResourceRef, Scene, SceneOp};
use mirui::render::texture::Texture;
use mirui::surface::Surface;
use mirui::types::{Point, Rect, Transform};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write;

#[derive(Default)]
pub struct Export {
    pub definitions: Vec<Value>,
    definition_keys: BTreeMap<String, usize>,
    fonts: Vec<Font>,
    textures: usize,
}

impl ResourceResolver for Export {
    fn resolve_font(&mut self, font: &Font) -> ResourceRef {
        let index = self
            .fonts
            .iter()
            .position(|f| {
                f.face_id() == font.face_id()
                    && f.revision() == font.revision()
                    && f.size == font.size
            })
            .unwrap_or_else(|| {
                self.fonts.push(font.clone());
                self.fonts.len() - 1
            });
        ResourceRef::Index(index as u32)
    }

    fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
        let index = self.textures;
        self.textures += 1;
        ResourceRef::Token(format!("screen-{index}").into())
    }
}

struct Recorder<'a> {
    inner: SceneRenderer<'a>,
    clips: Vec<Rect>,
}

impl Renderer for Recorder<'_> {
    type TargetState = ();

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        self.inner.route(request)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        self.inner.submit(request)?;
        self.clips.push(request.clip);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        self.inner.flush()
    }
}

pub fn disable_raster_caches(app: &mut SoftwareApp) {
    for entity in app.world.query::<mirui::ui::OffscreenRender>().collect() {
        app.world.remove::<mirui::ui::OffscreenRender>(entity);
    }
}

fn number(value: Fixed) -> f32 {
    value.to_f32()
}

fn rect(area: Rect) -> Value {
    json!([
        number(area.x),
        number(area.y),
        number(area.w),
        number(area.h)
    ])
}

fn matrix(t: Transform) -> Value {
    json!([
        number(t.m00),
        number(t.m10),
        number(t.m01),
        number(t.m11),
        number(t.tx),
        number(t.ty)
    ])
}

fn point(p: Point) -> String {
    format!("{} {}", number(p.x), number(p.y))
}

fn path_data(path: &Path) -> String {
    let mut data = String::new();
    for command in path.commands() {
        match command {
            PathCmd::MoveTo(p) => write!(data, "M{}", point(*p)),
            PathCmd::LineTo(p) => write!(data, "L{}", point(*p)),
            PathCmd::QuadTo { ctrl, end } => write!(data, "Q{} {}", point(*ctrl), point(*end)),
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                write!(data, "C{} {} {}", point(*ctrl1), point(*ctrl2), point(*end))
            }
            PathCmd::Close => write!(data, "Z"),
        }
        .expect("writing to a string");
    }
    data
}

fn color(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn paint(value: &Paint) -> Result<Color, String> {
    match value {
        Paint::Color(c) => Ok(Color::rgba(c.r, c.g, c.b, c.a)),
        _ => Err("This storyboard exporter requires solid path paints".into()),
    }
}

impl Export {
    fn define(&mut self, value: Value) -> usize {
        let key = value.to_string();
        if let Some(index) = self.definition_keys.get(&key) {
            return *index;
        }
        let index = self.definitions.len();
        self.definition_keys.insert(key, index);
        self.definitions.push(value);
        index
    }

    fn glyph(&mut self, font: &Font, id: GlyphId, size: u16) -> Result<Value, String> {
        let glyph = font
            .glyph_by_id(id, size)
            .ok_or("Missing glyph in captured font")?;
        let GlyphKind::Raster {
            samples,
            stride,
            region,
            representation,
            bearing_x,
            bearing_y,
        } = glyph.kind
        else {
            return Err("This storyboard requires the bundled raster pixel fonts".into());
        };
        let mirx::font::FontRepresentationKind::Coverage { bits } = representation.kind() else {
            return Err("Pixel glyph capture requires coverage samples".into());
        };
        if ![1, 2, 4, 8].contains(&bits) {
            return Err(format!("Unsupported coverage depth: {bits}"));
        }
        let mut paths = BTreeMap::<u8, String>::new();
        let sample = |x: u32, y: u32| {
            let bit = (region.x() + x) as usize * usize::from(bits);
            let byte = samples[(region.y() + y) as usize * stride as usize + bit / 8];
            (byte >> (8 - usize::from(bits) - bit % 8)) & ((1_u16 << bits) - 1) as u8
        };
        for y in 0..region.height() {
            let mut x = 0;
            while x < region.width() {
                let alpha = sample(x, y);
                let start = x;
                x += 1;
                while x < region.width() && sample(x, y) == alpha {
                    x += 1;
                }
                if alpha != 0 {
                    write!(
                        paths.entry(alpha).or_default(),
                        "M{start} {y}h{}v1h-{}Z",
                        x - start,
                        x - start
                    )
                    .expect("writing to a string");
                }
            }
        }
        let paths: Vec<_> = paths.into_iter().map(|(alpha, d)| json!({"d":d, "opacity": f32::from(alpha) / ((1_u16 << bits) - 1) as f32})).collect();
        let index = self.define(json!({"kind":"glyph", "paths":paths}));
        Ok(
            json!({"ref": index, "scale": f32::from(size) / f32::from(representation.design_ppem()), "x":number(bearing_x), "y": -number(bearing_y)}),
        )
    }

    pub fn capture(&mut self, app: &SoftwareApp, screens: bool) -> Result<Value, String> {
        self.textures = 0;
        let mut scene = Scene::new();
        let clips = {
            let mut recorder = Recorder {
                inner: SceneRenderer::new(&mut scene, self),
                clips: Vec::new(),
            };
            mirui::ui::render_system::render(
                &app.world,
                app.root.ok_or("Missing scene root")?,
                &app.backend.viewport(),
                &mut recorder,
            )
            .map_err(|error| format!("Scene capture: {error:?}"))?;
            recorder.clips
        };
        if self.textures != if screens { 2 } else { 0 } {
            return Err(format!(
                "Unexpected raster content in scene: {} textures",
                self.textures
            ));
        }
        let mut nodes = Vec::new();
        for (op, clip) in scene.ops.iter().zip(clips) {
            let mut node = self.node(op)?;
            node["clip"] = rect(clip);
            nodes.push(node);
        }
        Ok(json!(nodes))
    }

    fn node(&mut self, op: &SceneOp) -> Result<Value, String> {
        let (mut attrs, transform, ink, opacity) = match op {
            SceneOp::FillRect {
                area,
                transform,
                color,
                radius,
                opa,
                ..
            } => (
                json!({"tag":"rect", "x":number(area.x), "y":number(area.y), "width":number(area.w), "height":number(area.h), "rx":number(*radius)}),
                *transform,
                *color,
                *opa,
            ),
            SceneOp::Border {
                area,
                transform,
                color,
                width,
                radius,
                opa,
                ..
            } => {
                let half = *width / Fixed::from_int(2);
                (
                    json!({"tag":"rect", "x":number(area.x + half), "y":number(area.y + half), "width":number(area.w - *width), "height":number(area.h - *width), "rx":number((*radius - half).max(Fixed::ZERO)), "stroke-width":number(*width), "fill":"none"}),
                    *transform,
                    *color,
                    *opa,
                )
            }
            SceneOp::FillPath {
                path,
                transform,
                paint: p,
                opa,
                fill_rule,
            } => (
                json!({"tag":"use", "ref":self.define(json!({"kind":"path", "d":path_data(path), "fill-rule":if *fill_rule == FillRule::EvenOdd {"evenodd"} else {"nonzero"}}))}),
                *transform,
                paint(p)?,
                *opa,
            ),
            SceneOp::Line {
                p1,
                p2,
                transform,
                color,
                width,
                opa,
            } => (
                json!({"tag":"line", "x1":number(p1.x), "y1":number(p1.y), "x2":number(p2.x), "y2":number(p2.y), "stroke-width":number(*width)}),
                *transform,
                *color,
                *opa,
            ),
            SceneOp::GlyphRun {
                font: ResourceRef::Index(index),
                ppem,
                pos,
                transform,
                color,
                opa,
                glyphs,
            } => {
                let font = self.fonts[*index as usize].clone();
                let mut uses = Vec::new();
                if let Some(first) = glyphs.first() {
                    for glyph in glyphs.iter() {
                        let mut shape = self.glyph(&font, glyph.glyph_id(), *ppem)?;
                        shape["x"] = json!(
                            shape["x"].as_f64().unwrap()
                                + f64::from(glyph.origin.x - first.origin.x + glyph.offset.x)
                                    / 256.0
                        );
                        shape["y"] = json!(
                            shape["y"].as_f64().unwrap()
                                + f64::from(glyph.origin.y - first.origin.y + glyph.offset.y)
                                    / 256.0
                                + f64::from(number(font.metrics(*ppem).ascender))
                        );
                        uses.push(shape);
                    }
                }
                let index = self.define(json!({"kind":"text", "glyphs":uses}));
                (
                    json!({"tag":"use", "ref":index, "x":number(pos.x), "y":number(pos.y)}),
                    *transform,
                    *color,
                    *opa,
                )
            }
            SceneOp::Blit {
                texture: ResourceRef::Token(token),
                pos,
                size,
                transform,
                opa,
                ..
            } => (
                json!({"tag":"screen", "ref":token, "x":number(pos.x), "y":number(pos.y), "width":number(size.x), "height":number(size.y)}),
                *transform,
                Color::rgb(255, 255, 255),
                *opa,
            ),
            _ => return Err(format!("Unsupported scene operation: {op:?}")),
        };
        attrs["transform"] = matrix(transform);
        attrs["opacity"] = json!(f32::from(opacity) / 255.0 * f32::from(ink.a) / 255.0);
        if attrs.get("stroke-width").is_some() {
            attrs["stroke"] = json!(color(ink));
        } else {
            attrs["fill"] = json!(color(ink));
        }
        Ok(attrs)
    }
}
