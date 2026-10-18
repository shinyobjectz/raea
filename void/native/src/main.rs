//! raea void on Blitz (spike): the window, the void's CSS, a composer, and two custom widgets painted on the GPU:
//! the box's turning cube and the stage, which shows frames as images (rdb's frames go here).

use std::f64::consts::PI;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyrender::{PaintScene, RenderContext, Scene};
use blitz_dom::node::ComputedStyles;
use dioxus::prelude::*;
use dioxus_native::{Config, CustomWidgetAttr, Widget};
use kurbo::{Affine, BezPath, Point, Stroke};
use peniko::{Color, Fill};

const CSS: &str = include_str!("../void.css");

/// The box: a wireframe cube turning, drawn as twelve edges projected in perspective.
struct Cube {
    t0: Instant,
}

impl Widget for Cube {
    fn requires_redraw(&self) -> bool {
        true
    }

    fn paint(&mut self, _: &mut dyn RenderContext, _: &ComputedStyles, w: u32, h: u32, scale: f64) -> Scene {
        let mut scene = Scene::new();
        let t = self.t0.elapsed().as_secs_f64();
        let (ry, rx) = (t * 2.0 * PI / 14.0, -24f64.to_radians());
        let s = (w.min(h) as f64) * 0.3;
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        let project = |p: [f64; 3]| -> Point {
            let (x, z) = (p[0] * ry.cos() + p[2] * ry.sin(), -p[0] * ry.sin() + p[2] * ry.cos());
            let (y, z) = (p[1] * rx.cos() - z * rx.sin(), p[1] * rx.sin() + z * rx.cos());
            let k = 4.0 / (4.0 + z);
            Point::new(cx + x * s * k, cy + y * s * k)
        };
        let v: Vec<[f64; 3]> = (0..8).map(|i| [if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 }]).collect();
        let mut path = BezPath::new();
        for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
            path.move_to(project(v[a]));
            path.line_to(project(v[b]));
        }
        scene.stroke(&Stroke::new(0.9 * scale), Affine::IDENTITY, Color::from_rgba8(255, 255, 255, 210), None, &path);
        scene
    }
}

/// The stage: the newest frame (RGBA), scaled to fit, centred. One GPU texture on the renderer's own device, written
/// in place when a new frame arrives (drawing a new image per frame would fill vello_hybrid's atlases in seconds:
/// AtlasLimitReached, measured), re-made only when the frame's size changes.
struct Stage {
    frame: Arc<Mutex<Option<(u64, u32, u32, Arc<Vec<u8>>)>>>,
    gpu: Option<StageGpu>,
}

struct StageGpu {
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    id: anyrender::ResourceId,
    /// The frame number written last.
    shown: u64,
}

impl Widget for Stage {
    fn requires_redraw(&self) -> bool {
        true
    }

    fn destroy_surfaces(&mut self) {
        self.gpu = None;
    }

    fn paint(&mut self, ctx: &mut dyn RenderContext, _: &ComputedStyles, w: u32, h: u32, _scale: f64) -> Scene {
        let mut scene = Scene::new();
        let Some((n, fw, fh, px)) = self.frame.lock().unwrap().clone() else { return scene };
        // (re)make the texture for this size
        if self.gpu.as_ref().is_none_or(|g| g.texture.width() != fw || g.texture.height() != fh) {
            let Some(handle) = ctx.renderer_specific_context().and_then(|c| c.downcast::<wgpu_context::DeviceHandle>().ok()) else {
                return scene;
            };
            if let Some(old) = self.gpu.take() {
                ctx.unregister_resource(old.id);
            }
            let texture = handle.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("void stage"),
                size: wgpu::Extent3d { width: fw, height: fh, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let Ok(id) = ctx.try_register_custom_resource(Box::new(texture.clone())) else { return scene };
            self.gpu = Some(StageGpu { queue: handle.queue.clone(), texture, id, shown: u64::MAX });
        }
        let g = self.gpu.as_mut().unwrap();
        if g.shown != n {
            g.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &g.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &px,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * fw), rows_per_image: Some(fh) },
                wgpu::Extent3d { width: fw, height: fh, depth_or_array_layers: 1 },
            );
            g.shown = n;
        }
        let k = (w as f64 / fw as f64).min(h as f64 / fh as f64);
        let (dx, dy) = ((w as f64 - fw as f64 * k) / 2.0, (h as f64 - fh as f64 * k) / 2.0);
        let brush = peniko::ImageBrush { image: g.id, sampler: peniko::ImageSampler::default() };
        scene.fill(Fill::NonZero, Affine::translate((dx, dy)) * Affine::scale(k), anyrender::Paint::Resource(brush), None, &kurbo::Rect::new(0.0, 0.0, fw as f64, fh as f64));
        scene
    }
}

fn test_frame(w: u32, h: u32, t: f64) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            px[i] = (30.0 + 60.0 * (x as f64 / w as f64)) as u8;
            px[i + 1] = (30.0 + 40.0 * ((y as f64 / h as f64 + t * 0.2).sin().abs())) as u8;
            px[i + 2] = 80;
            px[i + 3] = 255;
        }
    }
    px
}

fn app() -> Element {
    let frame = use_hook(|| Arc::new(Mutex::new(None)));
    let mut draft = use_signal(String::new);
    let mut sent = use_signal(Vec::<String>::new);
    // test frames from a thread (Dioxus Native has no Tokio runtime); the stage widget redraws itself
    use_hook({
        let frame = frame.clone();
        move || {
            std::thread::spawn(move || {
                let t0 = Instant::now();
                for n in 0u64.. {
                    let px = test_frame(540, 960, t0.elapsed().as_secs_f64());
                    *frame.lock().unwrap() = Some((n, 540, 960, Arc::new(px)));
                    std::thread::sleep(std::time::Duration::from_millis(33));
                }
            });
        }
    });
    rsx! {
        style { {CSS} }
        div { class: "void",
            div { class: "prism" }
            div { class: "stage",
                div { class: "screen",
                    object { class: "stage-widget", data: CustomWidgetAttr::new(Stage { frame: frame.clone(), gpu: None }) }
                }
            }
            div { class: "box",
                object { class: "cube-widget", data: CustomWidgetAttr::new(Cube { t0: Instant::now() }) }
            }
            div { class: "transcript",
                for (i, l) in sent().iter().enumerate() {
                    div { key: "{i}", class: "line you", "{l}" }
                }
            }
            div { class: "composer-wrap",
                div { class: "composer",
                    button { class: "orb", span { class: "core" } }
                    input {
                        placeholder: "Make anything",
                        value: "{draft}",
                        oninput: move |e| draft.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter && !draft().trim().is_empty() {
                                sent.write().push(draft());
                                draft.set(String::new());
                            }
                        },
                    }
                    span { class: "kbd", "↵" }
                }
            }
        }
    }
}

fn main() {
    use dioxus_native::winit::platform::macos::WindowAttributesMacOS;
    let attrs = dioxus_native::WindowAttributes::default()
        .with_title("void")
        .with_surface_size(dioxus_native::LogicalSize::new(1180.0, 820.0))
        .with_platform_attributes(Box::new(
            WindowAttributesMacOS::default().with_titlebar_transparent(true).with_title_hidden(true).with_fullsize_content_view(true),
        ));
    let cfg = Config::new().with_window_attributes(attrs).with_base_color(Color::BLACK);
    dioxus_native::launch_cfg(app, vec![], vec![Box::new(cfg)]);
}
