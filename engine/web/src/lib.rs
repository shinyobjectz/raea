//! raea in a browser (docs/web-export.md M2): raea-render's 3D engine, compiled to wasm32, drawing on WebGPU into a
//! page's canvas. The page hands each frame's scene (the `data-3d` parts its React tree rendered, as the tree of
//! `{kind, props, shape, kids}` that native play reads from its own page) and the frame's number; the scene becomes rows
//! as live play makes them (raea-runtime-scene's LiveScene), and only what changed is set on the engine, which draws
//! the frame into its target and shows the target on the canvas, nearest-neighbour (canvas.rs).
//!
//! The engine opens in two halves: a browser's GPU device comes asynchronously, so `frame` starts it at the first
//! frame with any 3D and returns false until the device is up, then finishes it and draws from then on.

use std::path::Path;

use raea_render::engine::{Engine, Opening};
use raea_runtime_scene::{LiveScene, Shot};
use wasm_bindgen::prelude::*;

mod canvas;
mod scene;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

/// One canvas's raea: its engine, opened at the first frame with 3D.
#[wasm_bindgen]
pub struct Raea {
    canvas: web_sys::HtmlCanvasElement,
    /// the size the scene is drawn at (the canvas shows it scaled, nearest-neighbour)
    w: u32,
    h: u32,
    fps: f64,
    ls: LiveScene,
    parts: scene::Parts,
    opening: Option<Opening>,
    engine: Option<Engine>,
    window: Option<bevy::prelude::Entity>,
}

#[wasm_bindgen]
impl Raea {
    /// A raea for `canvas`, drawing its scenes at `width` x `height` (the canvas's own pixel size is what it shows at)
    /// and `fps` frames a second.
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: web_sys::HtmlCanvasElement, width: u32, height: u32, fps: f64) -> Result<Raea, JsError> {
        if width == 0 || height == 0 || !(fps > 0.0) {
            return Err(JsError::new(&format!("raea: a size and a frame rate, got {width}x{height} at {fps}")));
        }
        Ok(Raea { canvas, w: width, h: height, fps, ls: LiveScene::default(), parts: scene::Parts::default(), opening: None, engine: None, window: None })
    }

    /// Frame `f` with its scene (JSON: the frame's 3D parts, as scene.rs reads them) and the colour
    /// behind it ("#rrggbb", or none): true when it was drawn, false while the engine is still opening.
    pub fn frame(&mut self, scene: &str, clear: Option<String>, f: f64) -> Result<bool, JsError> {
        // a plain JS number (an i64 would be a BigInt there)
        let f = f as i64;
        let nodes = self.parts.nodes(scene).map_err(|e| JsError::new(&e))?;
        let shot = Shot { scene: &nodes, clear: clear.as_deref() };
        if self.engine.is_none() && self.opening.is_none() {
            if nodes.is_empty() {
                return Ok(false);
            }
            let p = self.ls.open(shot, self.w, self.h, self.fps);
            let mut o = Engine::begin(&p, &raea_render::inserted_of(&p), Path::new(""), None, true).map_err(|e| JsError::new(&format!("the 3D didn't open: {e}")))?;
            self.window = Some(canvas::attach(o.world_mut(), &self.canvas));
            self.opening = Some(o);
            return Ok(false);
        }
        if let Some(mut o) = self.opening.take() {
            if !o.ready() {
                self.opening = Some(o);
                return Ok(false);
            }
            let mut e = o.finish().map_err(|e| JsError::new(&format!("the 3D didn't open: {e}")))?;
            e.set_rows(&[]).map_err(|e| JsError::new(&e))?;
            canvas::show(&mut e, self.window.expect("a window with the opening"));
            self.engine = Some(e);
        }
        let e = self.engine.as_mut().expect("opened");
        let (rows, gone) = self.ls.frame(shot);
        if !gone.is_empty() {
            e.remove_rows(&gone).map_err(|e| JsError::new(&e))?;
        }
        if !rows.is_empty() {
            e.set_rows(&rows).map_err(|e| JsError::new(&e))?;
        }
        e.draw(f);
        if let Some(err) = e.take_error() {
            web_sys::console::warn_1(&format!("raea: frame {f}: {err}").into());
        }
        Ok(true)
    }

    /// The canvas's pixel size changed (its CSS size times the device's pixel ratio): the window follows it.
    pub fn resize(&mut self, width: u32, height: u32) {
        if let (Some(e), Some(w)) = (self.engine.as_mut(), self.window) {
            canvas::resize(e.world_mut(), w, width, height);
        }
    }
}
