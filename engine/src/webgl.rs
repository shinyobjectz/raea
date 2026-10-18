//! WebGL 1/2 (M7) through ANGLE on SwiftShader, the stack headless Chrome uses for software WebGL:
//! EGL contexts in ANGLE's WebGL-compatibility mode with robust resource initialization and 4x MSAA
//! (Chrome's defaults), one per canvas, driven by GL calls the shim records. Most calls are batched
//! per frame; the few WebGL answers synchronously (link status, uniform locations, parameters) go
//! through `query`, which first runs everything recorded so far.
//!
//! The libraries (libEGL, libGLESv2, libvk_swiftshader + its ICD json) are loaded at runtime from
//! `$RAEA_ANGLE_DIR`, `<exe>/../lib/raea/angle`, `<exe>/angle`, or the source tree's
//! vendor/angle/<platform>.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, CString};
use std::path::PathBuf;

use glow::HasContext;
use khronos_egl as egl;
use serde_json::{json, Value};

const PLATFORM_ANGLE: egl::Enum = 0x3202;
const PLATFORM_ANGLE_TYPE: egl::Attrib = 0x3203;
const PLATFORM_ANGLE_TYPE_VULKAN: egl::Attrib = 0x3450;
const PLATFORM_ANGLE_DEVICE_TYPE: egl::Attrib = 0x3209;
const PLATFORM_ANGLE_DEVICE_TYPE_SWIFTSHADER: egl::Attrib = 0x3487;
const CONTEXT_WEBGL_COMPATIBILITY: egl::Int = 0x33AC;
const ROBUST_RESOURCE_INITIALIZATION: egl::Int = 0x3453;
const UNPACK_FLIP_Y_WEBGL: u32 = 0x9240;
const UNPACK_PREMULTIPLY_ALPHA_WEBGL: u32 = 0x9241;
const UNPACK_COLORSPACE_CONVERSION_WEBGL: u32 = 0x9243;

/// Element pixels for texImage2D(..., image): (width, height, straight-alpha RGBA8).
/// An element image source as the shim encodes it: `{"ref": n}`, plus `"src"` for `<img>`s.
pub type ImageLookup<'a> = &'a dyn Fn(&Value) -> Option<(u32, u32, Vec<u8>)>;

struct Lib {
    egl: egl::DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    request_extension: Option<unsafe extern "C" fn(*const c_char)>,
}

#[derive(Clone, Copy)]
enum Obj {
    Buffer(glow::NativeBuffer),
    Texture(glow::NativeTexture),
    Framebuffer(glow::NativeFramebuffer),
    Renderbuffer(glow::NativeRenderbuffer),
    Program(glow::NativeProgram),
    Shader(glow::NativeShader),
    VertexArray(glow::NativeVertexArray),
    Sampler(glow::NativeSampler),
    Query(glow::NativeQuery),
}

/// A texImage2D from an element that arrived inside a synchronous batch (no document access there):
/// done at the next frame flush, before that flush's own calls.
struct DeferredUpload {
    texture: Option<glow::NativeTexture>,
    /// the upload's target (a cube face for cube maps)
    target: u32,
    /// generateMipmap ran on this texture before its pixels arrived: rerun it after the upload
    mipmap: bool,
    args: Value,
    flip_y: bool,
    premultiply: bool,
}

struct Canvas {
    ctx: egl::Context,
    surface: egl::Surface,
    config: egl::Config,
    gl: glow::Context,
    width: i32,
    height: i32,
    objects: HashMap<u64, Obj>,
    uniforms: HashMap<u64, glow::NativeUniformLocation>,
    flip_y: bool,
    premultiply: bool,
    /// texture bound per (unit, target), to know what a deferred upload targets
    active_unit: u32,
    bound_textures: HashMap<(u32, u32), glow::NativeTexture>,
    deferred: Vec<DeferredUpload>,
    /// Chrome's DrawingBuffer: with antialias, WebGL's default framebuffer is a 4x multisampled
    /// renderbuffer pair resolved (blitFramebuffer) at the end of a frame
    msaa: Option<Msaa>,
    alpha: bool,
    depth_stencil: (bool, bool),
}

struct Msaa {
    fbo: glow::NativeFramebuffer,
    color: glow::NativeRenderbuffer,
    depth: Option<glow::NativeRenderbuffer>,
}

impl Canvas {
    fn alloc_msaa(&mut self) {
        let gl = &self.gl;
        unsafe {
            if let Some(m) = self.msaa.take() {
                gl.delete_framebuffer(m.fbo);
                gl.delete_renderbuffer(m.color);
                if let Some(d) = m.depth {
                    gl.delete_renderbuffer(d);
                }
            }
            let samples = gl.get_parameter_i32(glow::MAX_SAMPLES).min(4);
            let (Ok(fbo), Ok(color)) = (gl.create_framebuffer(), gl.create_renderbuffer()) else { return };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, if self.alpha { glow::RGBA8 } else { glow::RGB8 }, self.width, self.height);
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(color));
            let depth = if self.depth_stencil.0 || self.depth_stencil.1 {
                let d = gl.create_renderbuffer().ok();
                gl.bind_renderbuffer(glow::RENDERBUFFER, d);
                gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::DEPTH24_STENCIL8, self.width, self.height);
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, d);
                d
            } else {
                None
            };
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.viewport(0, 0, self.width, self.height);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
            self.msaa = Some(Msaa { fbo, color, depth });
        }
    }

    /// WebGL's `null` framebuffer.
    fn default_fbo(&self) -> Option<glow::NativeFramebuffer> {
        self.msaa.as_ref().map(|m| m.fbo)
    }
}

struct State {
    lib: Lib,
    canvases: HashMap<u64, Canvas>,
}

thread_local! {
    static STATE: RefCell<Option<Result<State, String>>> = const { RefCell::new(None) };
}

fn angle_dir() -> Option<PathBuf> {
    let platform = if cfg!(target_arch = "aarch64") { "darwin-arm64" } else { "darwin-x64" };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(d) = crate::env_os("ANGLE_DIR") {
        candidates.push(PathBuf::from(d));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("../lib/raea/angle"));
            candidates.push(dir.join("angle"));
            candidates.push(dir.join("../Frameworks/angle"));
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/angle").join(platform));
    candidates.into_iter().find(|d| d.join("libEGL.dylib").is_file() && d.join("libGLESv2.dylib").is_file())
}

fn open_lib() -> Result<Lib, String> {
    let dir = angle_dir().ok_or("WebGL needs ANGLE (libEGL/libGLESv2) and SwiftShader; set RAEA_ANGLE_DIR")?;
    // ANGLE's Vulkan backend finds SwiftShader through the ICD manifest next to it
    if std::env::var_os("VK_ICD_FILENAMES").is_none() {
        // SAFETY: set once on the thread that first creates a WebGL context, before ANGLE loads Vulkan
        unsafe { std::env::set_var("VK_ICD_FILENAMES", dir.join("vk_swiftshader_icd.json")) };
    }
    let egl = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required_from_filename(dir.join("libEGL.dylib")) }
        .map_err(|e| format!("loading ANGLE: {e}"))?;
    let display = unsafe {
        egl.get_platform_display(
            PLATFORM_ANGLE,
            egl::DEFAULT_DISPLAY,
            &[PLATFORM_ANGLE_TYPE, PLATFORM_ANGLE_TYPE_VULKAN, PLATFORM_ANGLE_DEVICE_TYPE, PLATFORM_ANGLE_DEVICE_TYPE_SWIFTSHADER, egl::ATTRIB_NONE],
        )
    }
    .map_err(|e| format!("ANGLE display: {e}"))?;
    egl.initialize(display).map_err(|e| format!("ANGLE init: {e}"))?;
    let request_extension = egl.get_proc_address("glRequestExtensionANGLE").map(|p| unsafe { std::mem::transmute::<_, unsafe extern "C" fn(*const c_char)>(p) });
    Ok(Lib { egl, display, request_extension })
}

fn with_state<R>(f: impl FnOnce(&mut State) -> Result<R, String>) -> Result<R, String> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.is_none() {
            *s = Some(open_lib().map(|lib| State { lib, canvases: HashMap::new() }));
        }
        match s.as_mut().unwrap() {
            Ok(state) => f(state),
            Err(e) => Err(e.clone()),
        }
    })
}

impl State {
    fn canvas(&mut self, id: u64, width: i32, height: i32, version: u8, attrs: &Value) -> Result<&mut Canvas, String> {
        let lib = &self.lib;
        let (w, h) = (width.max(1), height.max(1));
        if let Some(c) = self.canvases.get_mut(&id) {
            if c.width != w || c.height != h {
                // a resized canvas gets a new drawing buffer
                let surface = lib.egl.create_pbuffer_surface(lib.display, c.config, &[egl::WIDTH, w, egl::HEIGHT, h, egl::NONE]).map_err(|e| format!("pbuffer: {e}"))?;
                let _ = lib.egl.destroy_surface(lib.display, c.surface);
                c.surface = surface;
                c.width = w;
                c.height = h;
                lib.egl.make_current(lib.display, Some(c.surface), Some(c.surface), Some(c.ctx)).map_err(|e| format!("make current: {e}"))?;
                if c.msaa.is_some() {
                    c.alloc_msaa();
                }
            }
        } else {
            let antialias = attrs.get("antialias").and_then(Value::as_bool).unwrap_or(true);
            let alpha = attrs.get("alpha").and_then(Value::as_bool).unwrap_or(true);
            let depth = attrs.get("depth").and_then(Value::as_bool).unwrap_or(true);
            let stencil = attrs.get("stencil").and_then(Value::as_bool).unwrap_or(false);
            let renderable = if version >= 2 { egl::OPENGL_ES3_BIT } else { egl::OPENGL_ES2_BIT };
            let mut want = vec![
                egl::RED_SIZE, 8, egl::GREEN_SIZE, 8, egl::BLUE_SIZE, 8, egl::ALPHA_SIZE, if alpha { 8 } else { 0 },
                egl::DEPTH_SIZE, if depth { 24 } else { 0 }, egl::STENCIL_SIZE, if stencil { 8 } else { 0 },
                egl::SURFACE_TYPE, egl::PBUFFER_BIT, egl::RENDERABLE_TYPE, renderable,
            ];
            want.push(egl::NONE);
            let config = lib.egl.choose_first_config(lib.display, &want).map_err(|e| format!("EGL config: {e}"))?.ok_or("no EGL config")?;
            let surface = lib.egl.create_pbuffer_surface(lib.display, config, &[egl::WIDTH, w, egl::HEIGHT, h, egl::NONE]).map_err(|e| format!("pbuffer: {e}"))?;
            let ctx = lib
                .egl
                .create_context(lib.display, config, None, &[egl::CONTEXT_CLIENT_VERSION, if version >= 2 { 3 } else { 2 }, CONTEXT_WEBGL_COMPATIBILITY, 1, ROBUST_RESOURCE_INITIALIZATION, 1, egl::NONE])
                .map_err(|e| format!("EGL context: {e}"))?;
            lib.egl.make_current(lib.display, Some(surface), Some(surface), Some(ctx)).map_err(|e| format!("make current: {e}"))?;
            let gl = unsafe { glow::Context::from_loader_function(|s| lib.egl.get_proc_address(s).map_or(std::ptr::null(), |p| p as *const _)) };
            self.canvases.insert(
                id,
                Canvas {
                    ctx,
                    surface,
                    config,
                    gl,
                    width: w,
                    height: h,
                    objects: HashMap::new(),
                    uniforms: HashMap::new(),
                    flip_y: false,
                    premultiply: false,
                    active_unit: 0,
                    bound_textures: HashMap::new(),
                    deferred: Vec::new(),
                    msaa: None,
                    alpha,
                    depth_stencil: (depth, stencil),
                },
            );
            let c = self.canvases.get_mut(&id).unwrap();
            if antialias {
                c.alloc_msaa();
            }
        }
        let c = self.canvases.get_mut(&id).unwrap();
        lib.egl.make_current(lib.display, Some(c.surface), Some(c.surface), Some(c.ctx)).map_err(|e| format!("make current: {e}"))?;
        Ok(c)
    }
}

fn b64(v: &Value) -> Vec<u8> {
    use base64::Engine as _;
    v.get("b64").and_then(Value::as_str).map(|s| base64::engine::general_purpose::STANDARD.decode(s).unwrap_or_default()).unwrap_or_default()
}

fn f32s(v: &Value) -> Vec<f32> {
    let bytes = b64(v);
    bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn i32s(v: &Value) -> Vec<i32> {
    let bytes = b64(v);
    bytes.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn u32s(v: &Value) -> Vec<u32> {
    i32s(v).into_iter().map(|x| x as u32).collect()
}

/// Pixels to upload from an element: flipped / premultiplied per the WebGL unpack flags.
/// The binding point of a texture upload target (cube faces upload through TEXTURE_CUBE_MAP).
fn bind_target(target: u32) -> u32 {
    if (glow::TEXTURE_CUBE_MAP_POSITIVE_X..=glow::TEXTURE_CUBE_MAP_NEGATIVE_Z).contains(&target) {
        glow::TEXTURE_CUBE_MAP
    } else {
        target
    }
}

fn element_pixels(img: (u32, u32, Vec<u8>), flip_y: bool, premultiply: bool) -> (u32, u32, Vec<u8>) {
    let (w, h, mut px) = img;
    if premultiply {
        for p in px.chunks_exact_mut(4) {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }
    if flip_y {
        let row = (w * 4) as usize;
        let mut out = Vec::with_capacity(px.len());
        for y in (0..h as usize).rev() {
            out.extend_from_slice(&px[y * row..(y + 1) * row]);
        }
        px = out;
    }
    (w, h, px)
}

impl Canvas {
    fn obj(&self, v: &Value) -> Option<Obj> {
        v.get("h").and_then(Value::as_u64).and_then(|h| self.objects.get(&h).copied())
    }
    fn buffer(&self, v: &Value) -> Option<glow::NativeBuffer> {
        match self.obj(v) {
            Some(Obj::Buffer(b)) => Some(b),
            _ => None,
        }
    }
    fn texture(&self, v: &Value) -> Option<glow::NativeTexture> {
        match self.obj(v) {
            Some(Obj::Texture(t)) => Some(t),
            _ => None,
        }
    }
    fn program(&self, v: &Value) -> Option<glow::NativeProgram> {
        match self.obj(v) {
            Some(Obj::Program(p)) => Some(p),
            _ => None,
        }
    }
    fn shader(&self, v: &Value) -> Option<glow::NativeShader> {
        match self.obj(v) {
            Some(Obj::Shader(s)) => Some(s),
            _ => None,
        }
    }
    fn uniform(&self, v: &Value) -> Option<glow::NativeUniformLocation> {
        v.get("u").and_then(Value::as_u64).and_then(|u| self.uniforms.get(&u).copied())
    }

    /// texImage2D / texSubImage2D from an element source: args = [target, level, (x, y,) internalformat|format..., {ref}]
    fn upload_element(&mut self, name: &str, op: &Value, img: (u32, u32, Vec<u8>), flip_y: bool, premultiply: bool) {
        let gl = &self.gl;
        let n = |i: usize| op.get(i).and_then(Value::as_i64).unwrap_or(0);
        let (w, h, px) = element_pixels(img, flip_y, premultiply);
        unsafe {
            if name == "texImage2D" {
                // (target, level, internalformat, format, type, source)
                gl.tex_image_2d(n(1) as u32, n(2) as i32, n(3) as i32, w as i32, h as i32, 0, n(4) as u32, n(5) as u32, glow::PixelUnpackData::Slice(Some(&px)));
            } else {
                // texSubImage2D(target, level, x, y, format, type, source)
                gl.tex_sub_image_2d(n(1) as u32, n(2) as i32, n(3) as i32, n(4) as i32, w as i32, h as i32, n(5) as u32, n(6) as u32, glow::PixelUnpackData::Slice(Some(&px)));
            }
        }
    }

    fn run_deferred(&mut self, images: ImageLookup<'_>) {
        let pending = std::mem::take(&mut self.deferred);
        for d in pending {
            let Some(source) = d.args.get(if d.args[0] == "texImage2D" { 6 } else { 7 }).filter(|v| v["ref"].is_u64()) else { continue };
            let Some(img) = images(source) else { continue };
            let bind = bind_target(d.target);
            unsafe {
                self.gl.bind_texture(bind, d.texture);
            }
            let name = d.args[0].as_str().unwrap_or("").to_string();
            self.upload_element(&name, &d.args, img, d.flip_y, d.premultiply);
            let restore = self.bound_textures.get(&(self.active_unit, bind)).copied();
            unsafe {
                if d.mipmap {
                    self.gl.generate_mipmap(bind);
                }
                self.gl.bind_texture(bind, restore);
            }
        }
    }

    /// Run recorded calls. Inside a synchronous batch (`final_pass` false) element uploads whose pixels
    /// `images` can't supply yet are deferred to the frame flush.
    fn run(&mut self, ops: &[Value], images: ImageLookup<'_>, final_pass: bool, request_extension: Option<unsafe extern "C" fn(*const c_char)>) {
        let trace = crate::env_os("DEBUG_GL").is_some();
        for op in ops {
            let Some(name) = op.get(0).and_then(Value::as_str) else { continue };
            if trace {
                trace_op(op);
            }
            // GL coerces booleans to 1/0 (three.js passes texture.flipY to pixelStorei)
            let n = |i: usize| op.get(i).and_then(|v| v.as_f64().or_else(|| v.as_bool().map(|b| b as u8 as f64))).unwrap_or(0.0);
            let u = |i: usize| n(i) as u32;
            let i = |i: usize| n(i) as i32;
            let f = |i: usize| n(i) as f32;
            let b = |i: usize| op.get(i).and_then(Value::as_bool).unwrap_or(false);
            let arg = |k: usize| op.get(k).cloned().unwrap_or(Value::Null);
            let gl = &self.gl;
            unsafe {
                match name {
                    "create" => {
                        let id = op.get(2).and_then(Value::as_u64).unwrap_or(0);
                        let obj = match op.get(1).and_then(Value::as_str).unwrap_or("") {
                            "buffer" => gl.create_buffer().ok().map(Obj::Buffer),
                            "texture" => gl.create_texture().ok().map(Obj::Texture),
                            "framebuffer" => gl.create_framebuffer().ok().map(Obj::Framebuffer),
                            "renderbuffer" => gl.create_renderbuffer().ok().map(Obj::Renderbuffer),
                            "program" => gl.create_program().ok().map(Obj::Program),
                            "vertexArray" => gl.create_vertex_array().ok().map(Obj::VertexArray),
                            "sampler" => gl.create_sampler().ok().map(Obj::Sampler),
                            "query" => gl.create_query().ok().map(Obj::Query),
                            kind if kind.starts_with("shader:") => kind[7..].parse::<u32>().ok().and_then(|t| gl.create_shader(t).ok()).map(Obj::Shader),
                            _ => None,
                        };
                        if let Some(o) = obj {
                            self.objects.insert(id, o);
                        }
                    }
                    "delete" => {
                        if let Some(id) = op.get(1).and_then(|v| v["h"].as_u64()) {
                            match self.objects.remove(&id) {
                                Some(Obj::Buffer(x)) => gl.delete_buffer(x),
                                Some(Obj::Texture(x)) => gl.delete_texture(x),
                                Some(Obj::Framebuffer(x)) => gl.delete_framebuffer(x),
                                Some(Obj::Renderbuffer(x)) => gl.delete_renderbuffer(x),
                                Some(Obj::Program(x)) => gl.delete_program(x),
                                Some(Obj::Shader(x)) => gl.delete_shader(x),
                                Some(Obj::VertexArray(x)) => gl.delete_vertex_array(x),
                                Some(Obj::Sampler(x)) => gl.delete_sampler(x),
                                Some(Obj::Query(x)) => gl.delete_query(x),
                                None => {}
                            }
                        }
                    }
                    "requestExtension" => {
                        if let (Some(req), Some(ext)) = (request_extension, op.get(1).and_then(Value::as_str)) {
                            if let Ok(c) = CString::new(ext) {
                                req(c.as_ptr());
                            }
                        }
                    }
                    "shaderSource" => {
                        if let Some(s) = self.shader(&arg(1)) {
                            gl.shader_source(s, op.get(2).and_then(Value::as_str).unwrap_or(""));
                        }
                    }
                    "compileShader" => {
                        if let Some(s) = self.shader(&arg(1)) {
                            gl.compile_shader(s);
                        }
                    }
                    "attachShader" | "detachShader" => {
                        if let (Some(p), Some(s)) = (self.program(&arg(1)), self.shader(&arg(2))) {
                            if name == "attachShader" { gl.attach_shader(p, s) } else { gl.detach_shader(p, s) }
                        }
                    }
                    "linkProgram" => {
                        if let Some(p) = self.program(&arg(1)) {
                            gl.link_program(p);
                        }
                    }
                    "bindAttribLocation" => {
                        if let Some(p) = self.program(&arg(1)) {
                            gl.bind_attrib_location(p, u(2), op.get(3).and_then(Value::as_str).unwrap_or(""));
                        }
                    }
                    "useProgram" => gl.use_program(self.program(&arg(1))),
                    "bindBuffer" => gl.bind_buffer(u(1), self.buffer(&arg(2))),
                    "bindBufferBase" => gl.bind_buffer_base(u(1), u(2), self.buffer(&arg(3))),
                    "bindBufferRange" => gl.bind_buffer_range(u(1), u(2), self.buffer(&arg(3)), i(4), i(5)),
                    "bufferData" => {
                        if op.get(2).is_some_and(Value::is_number) {
                            gl.buffer_data_size(u(1), i(2), u(3));
                        } else {
                            gl.buffer_data_u8_slice(u(1), &b64(&arg(2)), u(3));
                        }
                    }
                    "bufferSubData" => gl.buffer_sub_data_u8_slice(u(1), i(2), &b64(&arg(3))),
                    "vertexAttribPointer" => gl.vertex_attrib_pointer_f32(u(1), i(2), u(3), b(4), i(5), i(6)),
                    "vertexAttribIPointer" => gl.vertex_attrib_pointer_i32(u(1), i(2), u(3), i(4), i(5)),
                    "enableVertexAttribArray" => gl.enable_vertex_attrib_array(u(1)),
                    "disableVertexAttribArray" => gl.disable_vertex_attrib_array(u(1)),
                    "vertexAttribDivisor" => gl.vertex_attrib_divisor(u(1), u(2)),
                    "vertexAttrib4f" => gl.vertex_attrib_4_f32(u(1), f(2), f(3), f(4), f(5)),
                    "vertexAttrib1f" => gl.vertex_attrib_1_f32(u(1), f(2)),
                    "vertexAttrib2f" => gl.vertex_attrib_2_f32(u(1), f(2), f(3)),
                    "vertexAttrib3f" => gl.vertex_attrib_3_f32(u(1), f(2), f(3), f(4)),
                    "bindVertexArray" => gl.bind_vertex_array(match self.obj(&arg(1)) {
                        Some(Obj::VertexArray(v)) => Some(v),
                        _ => None,
                    }),
                    "activeTexture" => {
                        self.active_unit = u(1).saturating_sub(glow::TEXTURE0);
                        gl.active_texture(u(1));
                    }
                    "bindTexture" => {
                        let t = self.texture(&arg(2));
                        match t {
                            Some(t) => {
                                self.bound_textures.insert((self.active_unit, u(1)), t);
                            }
                            None => {
                                self.bound_textures.remove(&(self.active_unit, u(1)));
                            }
                        }
                        gl.bind_texture(u(1), t);
                    }
                    "texParameteri" => gl.tex_parameter_i32(u(1), u(2), i(3)),
                    "texParameterf" => gl.tex_parameter_f32(u(1), u(2), f(3)),
                    "pixelStorei" => match u(1) {
                        UNPACK_FLIP_Y_WEBGL => self.flip_y = i(2) != 0,
                        UNPACK_PREMULTIPLY_ALPHA_WEBGL => self.premultiply = i(2) != 0,
                        UNPACK_COLORSPACE_CONVERSION_WEBGL => {}
                        p => gl.pixel_store_i32(p, i(2)),
                    },
                    "texImage2D" | "texSubImage2D" => {
                        let src_index = if name == "texImage2D" { 6 } else { 7 };
                        let element = op.get(src_index).filter(|v| v["ref"].is_u64());
                        if let Some(r) = element {
                            match images(r) {
                                Some(img) => {
                                    let (fy, pm) = (self.flip_y, self.premultiply);
                                    self.upload_element(name, op, img, fy, pm);
                                }
                                None if !final_pass => {
                                    let target = u(1);
                                    let texture = self.bound_textures.get(&(self.active_unit, bind_target(target))).copied();
                                    self.deferred.push(DeferredUpload { texture, target, mipmap: false, args: op.clone(), flip_y: self.flip_y, premultiply: self.premultiply });
                                }
                                None => {}
                            }
                        } else if name == "texImage2D" {
                            // (target, level, internalformat, width, height, border, format, type, pixels|null)
                            let data = op.get(9).filter(|v| v.is_object()).map(b64);
                            gl.tex_image_2d(u(1), i(2), i(3), i(4), i(5), i(6), u(7), u(8), glow::PixelUnpackData::Slice(data.as_deref()));
                        } else {
                            // (target, level, x, y, width, height, format, type, pixels)
                            let data = op.get(9).filter(|v| v.is_object()).map(b64);
                            gl.tex_sub_image_2d(u(1), i(2), i(3), i(4), i(5), i(6), u(7), u(8), glow::PixelUnpackData::Slice(data.as_deref()));
                        }
                    }
                    "texImage3D" => {
                        let data = op.get(11).filter(|v| v.is_object()).map(b64);
                        gl.tex_image_3d(u(1), i(2), i(3), i(4), i(5), i(6), i(7), u(8), u(9), glow::PixelUnpackData::Slice(data.as_deref()));
                    }
                    "texStorage2D" => gl.tex_storage_2d(u(1), i(2), u(3), i(4), i(5)),
                    "texStorage3D" => gl.tex_storage_3d(u(1), i(2), u(3), i(4), i(5), i(6)),
                    "generateMipmap" => {
                        gl.generate_mipmap(u(1));
                        let bound = self.bound_textures.get(&(self.active_unit, u(1))).copied();
                        if let Some(d) = self.deferred.iter_mut().rev().find(|d| d.texture == bound && bind_target(d.target) == u(1)) {
                            d.mipmap = true;
                        }
                    }
                    "copyTexSubImage2D" => gl.copy_tex_sub_image_2d(u(1), i(2), i(3), i(4), i(5), i(6), i(7), i(8)),
                    "bindFramebuffer" => gl.bind_framebuffer(u(1), match self.obj(&arg(2)) {
                        Some(Obj::Framebuffer(fb)) => Some(fb),
                        _ => self.msaa.as_ref().map(|m| m.fbo),
                    }),
                    "bindRenderbuffer" => gl.bind_renderbuffer(u(1), match self.obj(&arg(2)) {
                        Some(Obj::Renderbuffer(rb)) => Some(rb),
                        _ => None,
                    }),
                    "framebufferTexture2D" => gl.framebuffer_texture_2d(u(1), u(2), u(3), self.texture(&arg(4)), i(5)),
                    "framebufferTextureLayer" => gl.framebuffer_texture_layer(u(1), u(2), self.texture(&arg(3)), i(4), i(5)),
                    "framebufferRenderbuffer" => gl.framebuffer_renderbuffer(u(1), u(2), u(3), match self.obj(&arg(4)) {
                        Some(Obj::Renderbuffer(rb)) => Some(rb),
                        _ => None,
                    }),
                    "renderbufferStorage" => gl.renderbuffer_storage(u(1), u(2), i(3), i(4)),
                    "renderbufferStorageMultisample" => gl.renderbuffer_storage_multisample(u(1), i(2), u(3), i(4), i(5)),
                    "blitFramebuffer" => gl.blit_framebuffer(i(1), i(2), i(3), i(4), i(5), i(6), i(7), i(8), u(9), u(10)),
                    "drawBuffers" => gl.draw_buffers(&op.get(1).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|v| v as u32).collect::<Vec<_>>()).unwrap_or_default()),
                    "readBuffer" => gl.read_buffer(u(1)),
                    "invalidateFramebuffer" => gl.invalidate_framebuffer(u(1), &op.get(2).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|v| v as u32).collect::<Vec<_>>()).unwrap_or_default()),
                    "enable" => gl.enable(u(1)),
                    "disable" => gl.disable(u(1)),
                    "blendFunc" => gl.blend_func(u(1), u(2)),
                    "blendFuncSeparate" => gl.blend_func_separate(u(1), u(2), u(3), u(4)),
                    "blendEquation" => gl.blend_equation(u(1)),
                    "blendEquationSeparate" => gl.blend_equation_separate(u(1), u(2)),
                    "blendColor" => gl.blend_color(f(1), f(2), f(3), f(4)),
                    "depthFunc" => gl.depth_func(u(1)),
                    "depthMask" => gl.depth_mask(b(1)),
                    "depthRange" => gl.depth_range_f32(f(1), f(2)),
                    "colorMask" => gl.color_mask(b(1), b(2), b(3), b(4)),
                    "cullFace" => gl.cull_face(u(1)),
                    "frontFace" => gl.front_face(u(1)),
                    "polygonOffset" => gl.polygon_offset(f(1), f(2)),
                    "scissor" => gl.scissor(i(1), i(2), i(3), i(4)),
                    "viewport" => gl.viewport(i(1), i(2), i(3), i(4)),
                    "lineWidth" => gl.line_width(f(1)),
                    "sampleCoverage" => gl.sample_coverage(f(1), b(2)),
                    "stencilFunc" => gl.stencil_func(u(1), i(2), u(3)),
                    "stencilFuncSeparate" => gl.stencil_func_separate(u(1), u(2), i(3), u(4)),
                    "stencilMask" => gl.stencil_mask(u(1)),
                    "stencilMaskSeparate" => gl.stencil_mask_separate(u(1), u(2)),
                    "stencilOp" => gl.stencil_op(u(1), u(2), u(3)),
                    "stencilOpSeparate" => gl.stencil_op_separate(u(1), u(2), u(3), u(4)),
                    "clearColor" => gl.clear_color(f(1), f(2), f(3), f(4)),
                    "clearDepth" => gl.clear_depth_f32(f(1)),
                    "clearStencil" => gl.clear_stencil(i(1)),
                    "clear" => gl.clear(u(1)),
                    "clearBufferfv" => gl.clear_buffer_f32_slice(u(1), u(2), &f32s(&arg(3))),
                    "clearBufferiv" => gl.clear_buffer_i32_slice(u(1), u(2), &i32s(&arg(3))),
                    "clearBufferuiv" => gl.clear_buffer_u32_slice(u(1), u(2), &u32s(&arg(3))),
                    "clearBufferfi" => gl.clear_buffer_depth_stencil(u(1), u(2), f(3), i(4)),
                    "drawArrays" => gl.draw_arrays(u(1), i(2), i(3)),
                    "drawElements" => gl.draw_elements(u(1), i(2), u(3), i(4)),
                    "drawArraysInstanced" => gl.draw_arrays_instanced(u(1), i(2), i(3), i(4)),
                    "drawElementsInstanced" => gl.draw_elements_instanced(u(1), i(2), u(3), i(4), i(5)),
                    "drawRangeElements" => gl.draw_elements(u(1), i(4), u(5), i(6)),
                    "uniformBlockBinding" => {
                        if let Some(p) = self.program(&arg(1)) {
                            gl.uniform_block_binding(p, u(2), u(3));
                        }
                    }
                    "bindSampler" => gl.bind_sampler(u(1), match self.obj(&arg(2)) {
                        Some(Obj::Sampler(s)) => Some(s),
                        _ => None,
                    }),
                    "samplerParameteri" => {
                        if let Some(Obj::Sampler(s)) = self.obj(&arg(1)) {
                            gl.sampler_parameter_i32(s, u(2), i(3));
                        }
                    }
                    "samplerParameterf" => {
                        if let Some(Obj::Sampler(s)) = self.obj(&arg(1)) {
                            gl.sampler_parameter_f32(s, u(2), f(3));
                        }
                    }
                    n if n.starts_with("uniform") => {
                        let loc = self.uniform(&arg(1));
                        let loc = loc.as_ref();
                        match n {
                            "uniform1f" => gl.uniform_1_f32(loc, f(2)),
                            "uniform2f" => gl.uniform_2_f32(loc, f(2), f(3)),
                            "uniform3f" => gl.uniform_3_f32(loc, f(2), f(3), f(4)),
                            "uniform4f" => gl.uniform_4_f32(loc, f(2), f(3), f(4), f(5)),
                            "uniform1i" => gl.uniform_1_i32(loc, i(2)),
                            "uniform2i" => gl.uniform_2_i32(loc, i(2), i(3)),
                            "uniform3i" => gl.uniform_3_i32(loc, i(2), i(3), i(4)),
                            "uniform4i" => gl.uniform_4_i32(loc, i(2), i(3), i(4), i(5)),
                            "uniform1ui" => gl.uniform_1_u32(loc, u(2)),
                            "uniform2ui" => gl.uniform_2_u32(loc, u(2), u(3)),
                            "uniform3ui" => gl.uniform_3_u32(loc, u(2), u(3), u(4)),
                            "uniform4ui" => gl.uniform_4_u32(loc, u(2), u(3), u(4), u(5)),
                            "uniform1fv" => gl.uniform_1_f32_slice(loc, &f32s(&arg(2))),
                            "uniform2fv" => gl.uniform_2_f32_slice(loc, &f32s(&arg(2))),
                            "uniform3fv" => gl.uniform_3_f32_slice(loc, &f32s(&arg(2))),
                            "uniform4fv" => gl.uniform_4_f32_slice(loc, &f32s(&arg(2))),
                            "uniform1iv" => gl.uniform_1_i32_slice(loc, &i32s(&arg(2))),
                            "uniform2iv" => gl.uniform_2_i32_slice(loc, &i32s(&arg(2))),
                            "uniform3iv" => gl.uniform_3_i32_slice(loc, &i32s(&arg(2))),
                            "uniform4iv" => gl.uniform_4_i32_slice(loc, &i32s(&arg(2))),
                            "uniform1uiv" => gl.uniform_1_u32_slice(loc, &u32s(&arg(2))),
                            "uniformMatrix2fv" => gl.uniform_matrix_2_f32_slice(loc, b(2), &f32s(&arg(3))),
                            "uniformMatrix3fv" => gl.uniform_matrix_3_f32_slice(loc, b(2), &f32s(&arg(3))),
                            "uniformMatrix4fv" => gl.uniform_matrix_4_f32_slice(loc, b(2), &f32s(&arg(3))),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn answer(&mut self, q: &Value, uniform_ids: &mut u64) -> Value {
        let gl = &self.gl;
        let name = q.get(0).and_then(Value::as_str).unwrap_or("");
        let u = |i: usize| q.get(i).and_then(Value::as_f64).unwrap_or(0.0) as u32;
        let s = |i: usize| q.get(i).and_then(Value::as_str).unwrap_or("").to_string();
        let arg = |k: usize| q.get(k).cloned().unwrap_or(Value::Null);
        unsafe {
            match name {
                "getShaderParameter" => match self.shader(&arg(1)) {
                    Some(sh) if u(2) == glow::COMPILE_STATUS => json!(gl.get_shader_compile_status(sh)),
                    Some(_) if u(2) == glow::DELETE_STATUS => json!(false),
                    Some(_) => Value::Null,
                    None => Value::Null,
                },
                "getShaderInfoLog" => self.shader(&arg(1)).map(|sh| json!(gl.get_shader_info_log(sh))).unwrap_or(Value::Null),
                "getShaderSource" => Value::Null,
                "getProgramParameter" => match self.program(&arg(1)) {
                    Some(p) if u(2) == glow::LINK_STATUS => json!(gl.get_program_link_status(p)),
                    Some(p) => json!(gl.get_program_parameter_i32(p, u(2))),
                    None => Value::Null,
                },
                "getProgramInfoLog" => self.program(&arg(1)).map(|p| json!(gl.get_program_info_log(p))).unwrap_or(Value::Null),
                "getActiveUniform" => self.program(&arg(1)).and_then(|p| gl.get_active_uniform(p, u(2))).map(|a| json!({"size": a.size, "type": a.utype, "name": a.name})).unwrap_or(Value::Null),
                "getActiveAttrib" => self.program(&arg(1)).and_then(|p| gl.get_active_attribute(p, u(2))).map(|a| json!({"size": a.size, "type": a.atype, "name": a.name})).unwrap_or(Value::Null),
                "getUniformLocation" => match self.program(&arg(1)).and_then(|p| gl.get_uniform_location(p, &s(2))) {
                    Some(loc) => {
                        *uniform_ids += 1;
                        self.uniforms.insert(*uniform_ids, loc);
                        json!(*uniform_ids)
                    }
                    None => Value::Null,
                },
                "getAttribLocation" => self.program(&arg(1)).map(|p| json!(gl.get_attrib_location(p, &s(2)).map(|l| l as i64).unwrap_or(-1))).unwrap_or(json!(-1)),
                "getFragDataLocation" => self.program(&arg(1)).map(|p| json!(gl.get_frag_data_location(p, &s(2)))).unwrap_or(json!(-1)),
                "getUniformBlockIndex" => self.program(&arg(1)).map(|p| json!(gl.get_uniform_block_index(p, &s(2)).map(|l| l as i64).unwrap_or(0xFFFF_FFFF))).unwrap_or(json!(0xFFFF_FFFFu32)),
                "getActiveUniformBlockParameter" => self.program(&arg(1)).map(|p| json!(gl.get_active_uniform_block_parameter_i32(p, u(2), u(3)))).unwrap_or(Value::Null),
                "getActiveUniformBlockName" => self.program(&arg(1)).map(|p| json!(gl.get_active_uniform_block_name(p, u(2)))).unwrap_or(Value::Null),
                "checkFramebufferStatus" => json!(gl.check_framebuffer_status(u(1))),
                "getError" => json!(gl.get_error()),
                "getSupportedExtensions" => {
                    let exts: Vec<String> = gl.supported_extensions().iter().cloned().collect();
                    json!(exts)
                }
                "getParameter" => {
                    let p = u(1);
                    match p {
                        glow::VENDOR | glow::RENDERER | glow::VERSION | glow::SHADING_LANGUAGE_VERSION => json!(gl.get_parameter_string(p)),
                        glow::VIEWPORT | glow::SCISSOR_BOX => {
                            let mut v = [0i32; 4];
                            gl.get_parameter_i32_slice(p, &mut v);
                            json!(v)
                        }
                        glow::MAX_VIEWPORT_DIMS => {
                            let mut v = [0i32; 2];
                            gl.get_parameter_i32_slice(p, &mut v);
                            json!(v)
                        }
                        glow::COLOR_CLEAR_VALUE | glow::BLEND_COLOR => {
                            let mut v = [0f32; 4];
                            gl.get_parameter_f32_slice(p, &mut v);
                            json!(v)
                        }
                        glow::ALIASED_LINE_WIDTH_RANGE | glow::ALIASED_POINT_SIZE_RANGE | glow::DEPTH_RANGE => {
                            let mut v = [0f32; 2];
                            gl.get_parameter_f32_slice(p, &mut v);
                            json!(v)
                        }
                        _ => json!(gl.get_parameter_i32(p)),
                    }
                }
                "getShaderPrecisionFormat" => {
                    let (range, precision) = gl.get_shader_precision_format(u(1), u(2)).map(|f| ((f.range_min, f.range_max), f.precision)).unwrap_or(((127, 127), 23));
                    json!({"rangeMin": range.0, "rangeMax": range.1, "precision": precision})
                }
                "readPixels" => {
                    let (x, y, w, h) = (u(1) as i32, u(2) as i32, u(3) as i32, u(4) as i32);
                    let mut px = vec![0u8; (w.max(0) * h.max(0) * 4) as usize];
                    gl.read_pixels(x, y, w, h, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
                    use base64::Engine as _;
                    json!(base64::engine::general_purpose::STANDARD.encode(px))
                }
                _ => Value::Null,
            }
        }
    }

    /// The drawing buffer as straight-alpha RGBA8, top row first.
    fn frame(&self) -> (u32, u32, Vec<u8>) {
        let (w, h) = (self.width, self.height);
        let mut px = vec![0u8; (w * h * 4) as usize];
        unsafe {
            if let Some(fbo) = self.default_fbo() {
                // resolve the multisampled drawing buffer, then restore WebGL's bindings
                self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
                self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
                let scissor = self.gl.is_enabled(glow::SCISSOR_TEST);
                self.gl.disable(glow::SCISSOR_TEST);
                self.gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
                if scissor {
                    self.gl.enable(glow::SCISSOR_TEST);
                }
            }
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            self.gl.read_pixels(0, 0, w, h, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
            if let Some(fbo) = self.default_fbo() {
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            }
        }
        let row = (w * 4) as usize;
        let mut out = Vec::with_capacity(px.len());
        for y in (0..h as usize).rev() {
            out.extend_from_slice(&px[y * row..(y + 1) * row]);
        }
        // the drawing buffer is premultiplied (premultipliedAlpha: true)
        for p in out.chunks_exact_mut(4) {
            let a = p[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut p[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        (w as u32, h as u32, out)
    }
}

thread_local! {
    static UNIFORM_IDS: RefCell<u64> = const { RefCell::new(0) };
}

/// Frame flush: run `ops` for canvas `id`, then return its drawing buffer.
pub fn flush(id: u64, width: i32, height: i32, version: u8, attrs: &Value, ops: &[Value], images: ImageLookup<'_>) -> Result<(u32, u32, Vec<u8>), String> {
    with_state(|st| {
        let req = st.lib.request_extension;
        let c = st.canvas(id, width, height, version, attrs)?;
        c.run_deferred(images);
        c.run(ops, images, true, req);
        unsafe {
            c.gl.flush();
        }
        Ok(c.frame())
    })
}

/// Synchronous query: run what was recorded so far, then answer `q` (getProgramParameter, ...).
/// `images` here can only decode `<img>` sources by src (no document access); others wait for the flush.
#[allow(clippy::too_many_arguments)]
pub fn query(id: u64, width: i32, height: i32, version: u8, attrs: &Value, ops: &[Value], q: &Value, images: ImageLookup<'_>) -> Result<Value, String> {
    with_state(|st| {
        let req = st.lib.request_extension;
        let c = st.canvas(id, width, height, version, attrs)?;
        c.run(ops, images, false, req);
        Ok(UNIFORM_IDS.with(|ids| c.answer(q, &mut ids.borrow_mut())))
    })
}

/// Whether ANGLE can be loaded (WebGL available).
pub fn available() -> bool {
    angle_dir().is_some()
}

/// RAEA_DEBUG_GL: one line per replayed call, binary payloads shown by size.
fn trace_op(op: &Value) {
    let args: Vec<String> = op
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| match a.get("b64").and_then(Value::as_str) {
            Some(b) => format!("<{} bytes>", b.len() / 4 * 3),
            None => {
                let s = a.to_string();
                if s.len() > 80 { format!("{}…", &s[..80]) } else { s }
            }
        })
        .collect();
    eprintln!("[gl] {}", args.join(" "));
}
