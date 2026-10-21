//! Gaussian splats (`<Splat>` in @raea/motion/3d): a scene or an object reconstructed as millions of 3D gaussians
//! (a 3DGS `.ply` with spherical harmonics up to degree 3, an antimatter15 `.splat`, or a Niantic `.spz`), drawn by
//! splat.wgsl as screen-space ellipses (EWA projection of each covariance), back to front, blended over the scene.
//!
//! The splats are one mesh entity with this module's material, so they draw in Bevy's transparent pass: they test
//! against the depth the opaque meshes wrote (a mesh in front hides them), sit on layer 0 (a frame `<Look>`'s camera
//! draws them into its cells, so pixel, ink and paint apply), and are placed, scaled and hidden by sync_meshes as any
//! mesh is. They write no depth and cast no shadow.
//!
//! The order is this module's, per frame: each splat's centre where the row's effects put it (anim_center, the
//! shader's own steps), its depth along the scene camera's view, a stable radix sort far to near, ties by index.
//! It's a function of the frame's rows and camera alone, so frame N sorts and draws the same in a render and in play.
//!
//! Columns of a splat row (all but src optional; every number keyed per frame):
//!   src (.ply / .splat / .spz; a `x.ply.json` beside it with `transform`, a column-major 4x4, places the file in the
//!   world: its rotation, uniform scale and offset), x y z rot_x rot_y rot_z scale_x scale_y scale_z visible,
//!   morph_to (a second splat file, the first frame's) and morph (0..1),
//!   twist (radians a metre about the vertical through the centre), wave_amp wave_len wave_phase (a ripple from the
//!   centre), explode (0..1), disperse (0..1) disperse_mode ('wind' or 'dust') wind_x wind_y wind_z curl,
//!   reveal (0..1) reveal_mode ('radius' or 'plane') reveal_x reveal_y reveal_z (the radius's origin) reveal_ax
//!   reveal_ay reveal_az (the plane's axis) reveal_soft reveal_glow, pulse pulse_scale pulse_glow, splat_scale,
//!   opacity, brightness, saturation, contrast, tint (hex) | tint_r tint_g tint_b, glow (hex), seed, crop (metres
//!   from the centre past which nothing draws), bands (flat light bands; a paint or cartoon `<Look>` sets them),
//!   paint (0..1: the splats themselves painted: flat bands, grown into strokes with hard edges)

use super::*;
use bevy::light::NotShadowCaster;
use bevy::pbr::{Material, MaterialPlugin, MeshMaterial3d};
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use std::sync::{Arc, Mutex};

pub(super) fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "splat.wgsl");
    app.add_plugins((MaterialPlugin::<SplatMaterial>::default(), MaterialPlugin::<SplatMask>::default()))
        .add_systems(PostUpdate, sync_splats.after(bevy::transform::TransformSystems::Propagate));
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct SplatMaterial {
    #[uniform(0)]
    p: SplatParams,
    #[storage(1, read_only)]
    splats: Handle<ShaderBuffer>,
    #[storage(2, read_only)]
    sh: Handle<ShaderBuffer>,
    #[storage(3, read_only)]
    target: Handle<ShaderBuffer>,
    #[storage(4, read_only)]
    order: Handle<ShaderBuffer>,
}

#[derive(ShaderType, Clone, Copy, PartialEq, Debug)]
pub(super) struct SplatParams {
    sh_rot: Mat3,
    center: Vec3,
    radius: f32,
    count: u32,
    sh_deg: u32,
    sh_words: u32,
    morph: f32,
    seed: f32,
    twist: f32,
    wave_amp: f32,
    wave_len: f32,
    wave_phase: f32,
    explode: f32,
    disperse: f32,
    disperse_mode: u32,
    wind: Vec3,
    curl: f32,
    reveal: f32,
    reveal_mode: u32,
    reveal_soft: f32,
    reveal_glow: f32,
    reveal_origin: Vec3,
    crop: f32,
    reveal_axis: Vec3,
    bands: f32,
    pulse: f32,
    pulse_scale: f32,
    pulse_glow: f32,
    splat_scale: f32,
    opacity: f32,
    brightness: f32,
    saturation: f32,
    contrast: f32,
    tint: Vec4,
    glow_color: Vec4,
    paint: f32,
}

impl Material for SplatMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://rdb/engine/splat.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://rdb/engine/splat.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        // both faces of every quad (a splat's corners may come out either way round), depth tested, never written
        descriptor.primitive.cull_mode = None;
        if let Some(d) = descriptor.depth_stencil.as_mut() {
            d.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// The same splats on the looked models' layers (1 to 30), drawn by each look's camera into its model's cells as an
/// eraser: where a splat is in front of the model (the model's own depth test decides), the model's cells are cut by
/// the splat's alpha (dst * (1 - a), so the order doesn't matter), and a model standing inside a splat scene is hidden
/// by what's in front of it, as a mesh would be.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct SplatMask {
    #[uniform(0)]
    p: SplatParams,
    #[storage(1, read_only)]
    splats: Handle<ShaderBuffer>,
    #[storage(2, read_only)]
    sh: Handle<ShaderBuffer>,
    #[storage(3, read_only)]
    target: Handle<ShaderBuffer>,
    #[storage(4, read_only)]
    order: Handle<ShaderBuffer>,
}

impl Material for SplatMask {
    fn vertex_shader() -> ShaderRef {
        "embedded://rdb/engine/splat.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://rdb/engine/splat.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        use bevy::render::render_resource::{BlendComponent, BlendFactor, BlendOperation, BlendState};
        descriptor.primitive.cull_mode = None;
        if let Some(d) = descriptor.depth_stencil.as_mut() {
            d.depth_write_enabled = Some(false);
        }
        if let Some(f) = descriptor.fragment.as_mut() {
            f.shader_defs.push("SPLAT_MASK".into());
            let erase = BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::OneMinusSrcAlpha, operation: BlendOperation::Add };
            for t in f.targets.iter_mut().flatten() {
                t.blend = Some(BlendState { color: erase, alpha: erase });
            }
        }
        Ok(())
    }
}

// ---- clouds: a file read once, its splats in the world's frame

/// One splat file, read and placed by its sidecar's transform: what the GPU draws and what the sort reads.
pub(super) struct Cloud {
    n: usize,
    /// per splat: centre xyz + opacity, scale xyz + 0, rotation xyzw, colour rgb + 0 (the shader's `Gauss`)
    gauss: Vec<[f32; 16]>,
    /// the rest of the spherical harmonics, f16 pairs, `sh_words` per splat
    sh: Vec<u32>,
    sh_deg: u32,
    sh_words: u32,
    /// the world's rotation of the file (its transpose turns view directions back into the file's)
    rot: Mat3,
    center: Vec3,
    radius: f32,
}

impl Cloud {
    fn pos(&self, i: usize) -> Vec3 {
        let g = &self.gauss[i];
        Vec3::new(g[0], g[1], g[2])
    }
}

static CLOUDS: Mutex<Option<HashMap<PathBuf, Arc<Cloud>>>> = Mutex::new(None);

/// A splat file's cloud, read the first time it's asked for (a file is read once per process, whatever uses it).
pub(super) fn cloud(path: &Path) -> Result<Arc<Cloud>, String> {
    let mut g = CLOUDS.lock().unwrap();
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(c) = map.get(path) {
        return Ok(c.clone());
    }
    let c = Arc::new(read_cloud(path)?);
    map.insert(path.to_path_buf(), c.clone());
    Ok(c)
}

/// Gaussians as a file has them, before placing: centre, linear scale, rotation (w x y z, normalised), opacity 0..1,
/// degree-0 colour (0.5 + C0 dc), and the rest of the harmonics as floats, coefficient-major (k * 3 + channel).
struct Raw {
    pos: Vec<[f32; 3]>,
    scale: Vec<[f32; 3]>,
    rot: Vec<[f32; 4]>,
    opacity: Vec<f32>,
    color: Vec<[f32; 3]>,
    deg: u32,
    rest: Vec<f32>,
}

const SH_C0: f32 = 0.282_094_8;

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn read_cloud(path: &Path) -> Result<Cloud, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("splat {}: {e}", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let raw = match ext.as_str() {
        "ply" => read_ply(&bytes),
        "splat" => read_splat(&bytes),
        "spz" => read_spz(&bytes),
        _ => Err("a .ply (3D gaussian splatting), .splat or .spz file".to_string()),
    }
    .map_err(|e| format!("splat {}: {e}", path.display()))?;
    // the sidecar's placing (raea-motion splat train writes it): a rotation, a uniform scale and an offset
    let side = path.with_file_name(format!("{}.json", path.file_name().and_then(|f| f.to_str()).unwrap_or("")));
    let m = match std::fs::read_to_string(&side) {
        Ok(s) => {
            let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| format!("{}: {e}", side.display()))?;
            match v.get("transform").and_then(|t| t.as_array()) {
                Some(a) if a.len() == 16 => {
                    let f: Vec<f32> = a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
                    Mat4::from_cols_slice(&f)
                }
                _ => Mat4::IDENTITY,
            }
        }
        Err(_) => Mat4::IDENTITY,
    };
    Ok(place(raw, m))
}

fn place(raw: Raw, m: Mat4) -> Cloud {
    let (s3, q, t) = m.to_scale_rotation_translation();
    let s = (s3.x.abs() + s3.y.abs() + s3.z.abs()) / 3.0;
    let rot = Mat3::from_quat(q);
    let deg = raw.deg;
    let k = ((deg + 1) * (deg + 1) - 1) as usize;
    let sh_words = ((k * 3) as u32).div_ceil(2);
    let mut gauss = Vec::with_capacity(raw.pos.len());
    let mut sh = Vec::with_capacity(raw.pos.len() * sh_words as usize);
    for i in 0..raw.pos.len() {
        // nothing a pixel would show
        if raw.opacity[i] < 1.0 / 255.0 {
            continue;
        }
        let p = q * Vec3::from_array(raw.pos[i]) * s + t;
        let [w, x, y, z] = raw.rot[i];
        let r = (q * Quat::from_xyzw(x, y, z, w)).normalize();
        let sc = Vec3::from_array(raw.scale[i]) * s;
        let c = raw.color[i];
        gauss.push([p.x, p.y, p.z, raw.opacity[i], sc.x, sc.y, sc.z, 0.0, r.x, r.y, r.z, r.w, c[0], c[1], c[2], 0.0]);
        let rest = &raw.rest[i * k * 3..(i + 1) * k * 3];
        for w in 0..sh_words as usize {
            let a = rest.get(w * 2).copied().unwrap_or(0.0);
            let b = rest.get(w * 2 + 1).copied().unwrap_or(0.0);
            sh.push(f16_bits(a) as u32 | (f16_bits(b) as u32) << 16);
        }
    }
    if sh.is_empty() {
        sh.push(0);
    }
    let n = gauss.len();
    // a robust centre and size: the median on each axis, and how far 80% of the splats lie from it
    let (center, radius) = if n == 0 {
        (Vec3::ZERO, 1.0)
    } else {
        let med = |a: usize| {
            let mut v: Vec<f32> = gauss.iter().map(|g| g[a]).collect();
            let mid = v.len() / 2;
            *v.select_nth_unstable_by(mid, f32::total_cmp).1
        };
        let c = Vec3::new(med(0), med(1), med(2));
        let mut d: Vec<f32> = gauss.iter().map(|g| (Vec3::new(g[0], g[1], g[2]) - c).length()).collect();
        let k80 = (d.len() * 8 / 10).min(d.len() - 1);
        let r = *d.select_nth_unstable_by(k80, f32::total_cmp).1;
        (c, r.max(1e-3))
    };
    Cloud { n, gauss, sh, sh_deg: deg, sh_words, rot, center, radius }
}

/// f32 to IEEE half (round to nearest even), as WGSL's unpack2x16float reads it.
fn f16_bits(f: f32) -> u16 {
    let x = f.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let e = ((x >> 23) & 0xff) as i32;
    let m = x & 0x7f_ffff;
    if e == 0xff {
        return sign | 0x7c00 | if m != 0 { 0x200 } else { 0 };
    }
    let e2 = e - 127 + 15;
    if e2 >= 0x1f {
        return sign | 0x7c00;
    }
    if e2 <= 0 {
        if e2 < -10 {
            return sign;
        }
        let m = m | 0x80_0000;
        let shift = (14 - e2) as u32;
        let half = 1u32 << (shift - 1);
        let mut r = m >> shift;
        let rem = m & ((1 << shift) - 1);
        if rem > half || (rem == half && (r & 1) == 1) {
            r += 1;
        }
        return sign | r as u16;
    }
    let mut r = ((e2 as u32) << 10) | (m >> 13);
    let rem = m & 0x1fff;
    if rem > 0x1000 || (rem == 0x1000 && (r & 1) == 1) {
        r += 1;
    }
    sign | r as u16
}

/// A 3DGS PLY: binary little endian vertices with x y z, f_dc_0..2, f_rest_*, opacity (logit), scale_0..2 (log),
/// rot_0..3 (w x y z). Properties may come in any order, as float, double or uchar.
fn read_ply(b: &[u8]) -> Result<Raw, String> {
    let end = b.windows(11).position(|w| w == b"end_header\n").ok_or("no end_header: not a PLY")? + 11;
    let head = std::str::from_utf8(&b[..end]).map_err(|_| "a PLY header that isn't text")?;
    if !head.contains("binary_little_endian") {
        return Err("only binary_little_endian PLY files".into());
    }
    let mut n = 0usize;
    let mut props: Vec<(String, usize)> = Vec::new();
    let mut in_vertex = false;
    let mut skip_before = 0usize;
    for line in head.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["element", "vertex", c] => {
                n = c.parse().map_err(|_| "a bad vertex count")?;
                in_vertex = true;
            }
            ["element", ..] => {
                if !in_vertex && n == 0 {
                    // an element before the vertices: not handled (none of the 3DGS writers put one there)
                    skip_before += 1;
                }
                in_vertex = false;
            }
            ["property", ty, name] if in_vertex => {
                let size = match *ty {
                    "float" | "float32" | "int" | "uint" | "int32" | "uint32" => 4,
                    "double" | "float64" => 8,
                    "uchar" | "uint8" | "char" | "int8" => 1,
                    "short" | "ushort" | "int16" | "uint16" => 2,
                    _ => return Err(format!("a property type I can't read: {ty}")),
                };
                props.push((format!("{ty}:{name}"), size));
            }
            _ => {}
        }
    }
    if skip_before > 0 {
        return Err("an element before the vertices".into());
    }
    let stride: usize = props.iter().map(|p| p.1).sum();
    let need = end + n * stride;
    if b.len() < need {
        return Err(format!("{n} vertices of {stride} bytes need {need} bytes, the file has {}", b.len()));
    }
    let mut offs: HashMap<&str, (usize, &str)> = HashMap::new();
    let mut o = 0;
    for (tn, size) in &props {
        let (ty, name) = tn.split_once(':').unwrap();
        offs.insert(name, (o, ty));
        o += size;
    }
    let read = |i: usize, name: &str| -> Option<f32> {
        let (off, ty) = *offs.get(name)?;
        let at = end + i * stride + off;
        Some(match ty {
            "double" | "float64" => f64::from_le_bytes(b[at..at + 8].try_into().unwrap()) as f32,
            "uchar" | "uint8" => b[at] as f32 / 255.0,
            "float" | "float32" => f32::from_le_bytes(b[at..at + 4].try_into().unwrap()),
            _ => 0.0,
        })
    };
    for need in ["x", "y", "z"] {
        if !offs.contains_key(need) {
            return Err(format!("no {need} property"));
        }
    }
    let gs = offs.contains_key("scale_0") && offs.contains_key("rot_0") && offs.contains_key("opacity");
    if !gs {
        return Err("not a gaussian splat PLY (no scale_0, rot_0 or opacity; a plain point cloud?)".into());
    }
    let rest_n = (0..).take_while(|k| offs.contains_key(format!("f_rest_{k}").as_str())).count();
    let per = rest_n / 3;
    let deg = match per {
        0 => 0,
        1..=3 => 1,
        4..=8 => 2,
        _ => 3,
    };
    let k = ((deg + 1) * (deg + 1) - 1) as usize;
    let rest_names: Vec<String> = (0..rest_n).map(|k| format!("f_rest_{k}")).collect();
    let mut r = Raw { pos: Vec::with_capacity(n), scale: Vec::with_capacity(n), rot: Vec::with_capacity(n), opacity: Vec::with_capacity(n), color: Vec::with_capacity(n), deg, rest: Vec::with_capacity(n * k * 3) };
    let has_dc = offs.contains_key("f_dc_0");
    for i in 0..n {
        let g = |s: &str| read(i, s).unwrap_or(0.0);
        r.pos.push([g("x"), g("y"), g("z")]);
        r.scale.push([g("scale_0").exp(), g("scale_1").exp(), g("scale_2").exp()]);
        let q = Vec4::new(g("rot_0"), g("rot_1"), g("rot_2"), g("rot_3"));
        let q = if q.length_squared() > 0.0 { q.normalize() } else { Vec4::X };
        r.rot.push(q.to_array());
        r.opacity.push(sigmoid(g("opacity")));
        r.color.push(if has_dc {
            [0.5 + SH_C0 * g("f_dc_0"), 0.5 + SH_C0 * g("f_dc_1"), 0.5 + SH_C0 * g("f_dc_2")]
        } else {
            [g("red"), g("green"), g("blue")]
        });
        // the file's are channel-major (all of red's coefficients, then green's, then blue's)
        for kk in 0..k {
            for c in 0..3 {
                r.rest.push(if kk < per { read(i, &rest_names[c * per + kk]).unwrap_or(0.0) } else { 0.0 });
            }
        }
    }
    Ok(r)
}

/// antimatter15's .splat: 32 bytes a splat (centre f32 x3, scale f32 x3, rgba u8, rotation u8 x4 as w x y z).
fn read_splat(b: &[u8]) -> Result<Raw, String> {
    if b.len() % 32 != 0 {
        return Err("a .splat file is 32 bytes a splat".into());
    }
    let n = b.len() / 32;
    let mut r = Raw { pos: Vec::with_capacity(n), scale: Vec::with_capacity(n), rot: Vec::with_capacity(n), opacity: Vec::with_capacity(n), color: Vec::with_capacity(n), deg: 0, rest: Vec::new() };
    let f = |at: usize| f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    for i in 0..n {
        let o = i * 32;
        r.pos.push([f(o), f(o + 4), f(o + 8)]);
        r.scale.push([f(o + 12), f(o + 16), f(o + 20)]);
        r.color.push([b[o + 24] as f32 / 255.0, b[o + 25] as f32 / 255.0, b[o + 26] as f32 / 255.0]);
        r.opacity.push(b[o + 27] as f32 / 255.0);
        let q = Vec4::new((b[o + 28] as f32 - 128.0) / 128.0, (b[o + 29] as f32 - 128.0) / 128.0, (b[o + 30] as f32 - 128.0) / 128.0, (b[o + 31] as f32 - 128.0) / 128.0);
        r.rot.push(if q.length_squared() > 0.0 { q.normalize().to_array() } else { [1.0, 0.0, 0.0, 0.0] });
    }
    Ok(r)
}

/// Niantic's .spz (versions 2 and 3): gzip of a 16-byte header and the splats' fields one after another, fixed
/// point. Decompressed by the system's gzip (native only), so rdb takes no inflate crate for it.
fn read_spz(b: &[u8]) -> Result<Raw, String> {
    #[cfg(feature = "native")]
    let d = {
        use std::io::Write;
        let mut child = std::process::Command::new("gzip").arg("-dc").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().map_err(|e| format!("gzip: {e}"))?;
        let mut stdin = child.stdin.take().unwrap();
        let input = b.to_vec();
        let w = std::thread::spawn(move || stdin.write_all(&input));
        let out = child.wait_with_output().map_err(|e| format!("gzip: {e}"))?;
        let _ = w.join();
        if !out.status.success() {
            return Err("not gzip: a .spz is gzipped".into());
        }
        out.stdout
    };
    #[cfg(not(feature = "native"))]
    let d: Vec<u8> = return Err(".spz needs the native engine".into());
    if d.len() < 16 || &d[0..4] != b"NGSP" {
        return Err("no NGSP magic".into());
    }
    let u32at = |at: usize| u32::from_le_bytes(d[at..at + 4].try_into().unwrap());
    let ver = u32at(4);
    let n = u32at(8) as usize;
    let deg = d[12] as u32;
    let frac = d[13];
    if !(2..=3).contains(&ver) || deg > 3 {
        return Err(format!("spz version {ver}, degree {deg}: I read versions 2 and 3, degree up to 3"));
    }
    let k = ((deg + 1) * (deg + 1) - 1) as usize;
    let rot_bytes = if ver == 3 { 4 } else { 3 };
    let need = 16 + n * (9 + 1 + 3 + 3 + rot_bytes + k * 3);
    if d.len() < need {
        return Err(format!("{n} splats need {need} bytes, it has {}", d.len()));
    }
    // the fields one after another: positions (24-bit fixed point), alphas, colours, scales, rotations, harmonics
    let o_pos = 16;
    let o_a = o_pos + n * 9;
    let o_c = o_a + n;
    let o_s = o_c + n * 3;
    let o_r = o_s + n * 3;
    let o_sh = o_r + n * rot_bytes;
    let scale = 1.0 / (1u32 << frac) as f32;
    let mut r = Raw { pos: Vec::with_capacity(n), scale: Vec::with_capacity(n), rot: Vec::with_capacity(n), opacity: Vec::with_capacity(n), color: Vec::with_capacity(n), deg, rest: Vec::with_capacity(n * k * 3) };
    for i in 0..n {
        let mut p = [0f32; 3];
        for (a, v) in p.iter_mut().enumerate() {
            let at = o_pos + i * 9 + a * 3;
            let mut x = d[at] as i32 | (d[at + 1] as i32) << 8 | (d[at + 2] as i32) << 16;
            if x & 0x80_0000 != 0 {
                x |= -0x100_0000;
            }
            *v = x as f32 * scale;
        }
        r.pos.push(p);
        r.opacity.push(d[o_a + i] as f32 / 255.0);
        let c = |a: usize| ((d[o_c + i * 3 + a] as f32 / 255.0) - 0.5) / 0.15;
        r.color.push([0.5 + SH_C0 * c(0), 0.5 + SH_C0 * c(1), 0.5 + SH_C0 * c(2)]);
        let s = |a: usize| (d[o_s + i * 3 + a] as f32 / 16.0 - 10.0).exp();
        r.scale.push([s(0), s(1), s(2)]);
        let q = if ver == 3 {
            // smallest three: the largest component's index in the top two bits, the others 10 bits each
            let v = u32::from_le_bytes(d[o_r + i * 4..o_r + i * 4 + 4].try_into().unwrap());
            let largest = (v >> 30) as usize;
            let mut q = [0f32; 4];
            let mut sum = 0.0;
            let mut bits = v;
            for c in (0..4).rev() {
                if c == largest {
                    continue;
                }
                let mag = (bits & 0x1ff) as f32 / 511.0 * std::f32::consts::FRAC_1_SQRT_2;
                let neg = (bits >> 9) & 1 == 1;
                bits >>= 10;
                q[c] = if neg { -mag } else { mag };
                sum += q[c] * q[c];
            }
            q[largest] = (1.0 - sum).max(0.0).sqrt();
            // x y z w in the file
            Vec4::new(q[3], q[0], q[1], q[2])
        } else {
            let x = d[o_r + i * 3] as f32 / 127.5 - 1.0;
            let y = d[o_r + i * 3 + 1] as f32 / 127.5 - 1.0;
            let z = d[o_r + i * 3 + 2] as f32 / 127.5 - 1.0;
            Vec4::new((1.0 - x * x - y * y - z * z).max(0.0).sqrt(), x, y, z)
        };
        r.rot.push(q.normalize().to_array());
        for j in 0..k * 3 {
            r.rest.push((d[o_sh + i * k * 3 + j] as f32 - 128.0) / 128.0);
        }
    }
    Ok(r)
}

fn bytes_of(g: &[[f32; 16]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(g.len() * 64);
    for s in g {
        for v in s {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

fn words_of(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|v| v.to_le_bytes()).collect()
}

// ---- morph: each splat of the source given one of the target's

/// The position along a 3D Hilbert curve of a point in the unit cube (10 bits an axis; Skilling's transform).
fn hilbert(p: Vec3) -> u32 {
    let b = 10u32;
    let mut x = [(p.x.clamp(0.0, 1.0) * 1023.0) as u32, (p.y.clamp(0.0, 1.0) * 1023.0) as u32, (p.z.clamp(0.0, 1.0) * 1023.0) as u32];
    let m = 1u32 << (b - 1);
    // inverse undo
    let mut q = m;
    while q > 1 {
        let pm = q - 1;
        for i in 0..3 {
            if x[i] & q != 0 {
                x[0] ^= pm;
            } else {
                let t = (x[0] ^ x[i]) & pm;
                x[0] ^= t;
                x[i] ^= t;
            }
        }
        q >>= 1;
    }
    // Gray encode
    for i in 1..3 {
        x[i] ^= x[i - 1];
    }
    let mut t = 0;
    let mut q = m;
    while q > 1 {
        if x[2] & q != 0 {
            t ^= q - 1;
        }
        q >>= 1;
    }
    for v in x.iter_mut() {
        *v ^= t;
    }
    // interleave, most significant bit first
    let mut h = 0u32;
    for bit in (0..b).rev() {
        for v in &x {
            h = (h << 1) | ((v >> bit) & 1);
        }
    }
    h
}

/// Each cloud's splats in Hilbert order within its own bounds (so like parts meet: a head finds a head).
fn curve_order(c: &Cloud) -> Vec<u32> {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for i in 0..c.n {
        lo = lo.min(c.pos(i));
        hi = hi.max(c.pos(i));
    }
    let size = (hi - lo).max(Vec3::splat(1e-6));
    let mut keyed: Vec<(u32, u32)> = (0..c.n).map(|i| (hilbert((c.pos(i) - lo) / size), i as u32)).collect();
    keyed.sort_unstable();
    keyed.into_iter().map(|(_, i)| i).collect()
}

/// The target's splat each source splat morphs into: the source's and target's Hilbert orders matched by rank (a
/// cheap stand-in for optimal transport that keeps neighbours together), as the shader's `target_splats` reads them.
fn matched(a: &Cloud, b: &Cloud) -> Vec<[f32; 16]> {
    let mut out = vec![[0f32; 16]; a.n];
    if b.n == 0 {
        return out;
    }
    let oa = curve_order(a);
    let ob = curve_order(b);
    for (rank, &i) in oa.iter().enumerate() {
        let j = ob[(rank * b.n / a.n.max(1)).min(b.n - 1)] as usize;
        out[i as usize] = b.gauss[j];
    }
    out
}

// ---- the frame's effects on a splat's centre (splat.wgsl's `animate`, its moving steps)

fn pcg(v: u32) -> u32 {
    let s = v.wrapping_mul(747796405).wrapping_add(2891336453);
    let w = ((s >> ((s >> 28) + 4)) ^ s).wrapping_mul(277803737);
    (w >> 22) ^ w
}

fn hash(seed: f32, i: u32, k: u32) -> f32 {
    (pcg(i ^ pcg(k.wrapping_add(seed.to_bits()))) >> 8) as f32 / 16777216.0
}

fn potential(x: Vec3) -> Vec3 {
    Vec3::new(
        (x.y * 1.7 + 1.3).sin() + (x.z * 2.3 + 0.4).cos() + 0.5 * (x.y * 3.9 + x.z * 1.1 + 2.0).sin(),
        (x.z * 1.9 + 2.9).sin() + (x.x * 2.1 + 1.7).cos() + 0.5 * (x.z * 4.1 + x.x * 1.3 + 0.7).sin(),
        (x.x * 1.5 + 0.2).sin() + (x.y * 2.7 + 2.2).cos() + 0.5 * (x.x * 3.7 + x.y * 1.2 + 1.5).sin(),
    )
}

fn curl(x: Vec3) -> Vec3 {
    let e = 0.05;
    let (dx, dy, dz) = (Vec3::X * e, Vec3::Y * e, Vec3::Z * e);
    let (px0, px1) = (potential(x - dx), potential(x + dx));
    let (py0, py1) = (potential(x - dy), potential(x + dy));
    let (pz0, pz1) = (potential(x - dz), potential(x + dz));
    Vec3::new((py1.z - py0.z) - (pz1.y - pz0.y), (pz1.x - pz0.x) - (px1.z - px0.z), (px1.y - px0.y) - (py1.x - py0.x)) / (2.0 * e)
}

fn smooth01(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Whether the row's effects move any splat (else the file's centres are where they draw).
fn moves(p: &SplatParams) -> bool {
    p.morph > 0.0 || p.twist != 0.0 || p.wave_amp != 0.0 || p.explode > 0.0 || p.disperse > 0.0
}

fn anim_center(p: &SplatParams, i: u32, home: Vec3, target: Option<&[f32; 16]>) -> Vec3 {
    let r = p.radius.max(1e-4);
    let mut pos = home;
    if p.morph > 0.0 {
        if let Some(t) = target {
            let m = smooth01(p.morph);
            let lift = (std::f32::consts::PI * m).sin() * 0.35 * r;
            pos = pos.lerp(Vec3::new(t[0], t[1], t[2]), m) + lift * (Vec3::new(hash(p.seed, i, 11), hash(p.seed, i, 12), hash(p.seed, i, 13)) - 0.5) * 2.0;
        }
    }
    if p.twist != 0.0 {
        let ang = p.twist * (pos.y - p.center.y);
        let (s, c) = ang.sin_cos();
        let d = pos - p.center;
        pos = p.center + Vec3::new(c * d.x + s * d.z, d.y, -s * d.x + c * d.z);
    }
    if p.wave_amp != 0.0 {
        let d = (pos - p.center).xz().length();
        pos.y += p.wave_amp * (std::f32::consts::TAU * d / p.wave_len.max(1e-3) - p.wave_phase).sin();
    }
    if p.explode > 0.0 {
        let e = p.explode;
        let jit = Vec3::new(hash(p.seed, i, 21), hash(p.seed, i, 22), hash(p.seed, i, 23)) - 0.5;
        let dir = (pos - p.center + jit * 0.6 * r + Vec3::new(0.0, 1e-4, 0.0)).normalize();
        pos += dir * e * r * 2.0 * (0.4 + hash(p.seed, i, 24));
        pos.y -= 1.2 * r * e * e;
    }
    if p.disperse > 0.0 {
        let w = (p.wind + Vec3::new(0.0, 0.0, 1e-6)).normalize();
        let o = if p.disperse_mode == 0 { ((pos - p.center).dot(w) / r + 1.0) * 0.5 } else { (pos - p.center).length() / (1.5 * r) };
        let o = (o * 0.8 + hash(p.seed, i, 51) * 0.2).clamp(0.0, 1.0);
        if p.disperse_mode == 0 {
            let local = ((p.disperse - 0.55 * o) / 0.45).clamp(0.0, 1.0);
            pos += w * local * local * r * 3.0 + curl(pos * (1.4 / r) + Vec3::splat(p.seed)) * (p.curl * r * 0.25 * local);
            pos.y += local * local * r * 0.4;
        } else {
            let local = smooth01((p.disperse - 0.5 * o) / 0.5);
            let (u, v) = (hash(p.seed, i, 31), hash(p.seed, i, 32));
            let rr = hash(p.seed, i, 33).powf(0.333) * r * 1.6;
            let th = std::f32::consts::TAU * u;
            let ph = (2.0 * v - 1.0).acos();
            let q = p.center + rr * Vec3::new(ph.sin() * th.cos(), ph.cos(), ph.sin() * th.sin());
            let swirl = curl(q * (1.4 / r) + Vec3::splat(p.seed)) * (p.curl * r * 0.12);
            pos = pos.lerp(q + swirl, local);
        }
    }
    pos
}

// ---- the sort

/// A float's bits made to sort as the float does (as an unsigned integer).
fn ordered(f: f32) -> u32 {
    let b = f.to_bits();
    if b & 0x8000_0000 != 0 { !b } else { b | 0x8000_0000 }
}

/// The splats far to near from the camera, ties by index: a stable LSD radix sort (four 8-bit passes) of each splat's
/// depth where the frame's effects put it. `keys` and `tmp` are scratch kept between frames.
fn sort(c: &Cloud, target: Option<&[[f32; 16]]>, p: &SplatParams, model: Mat4, cam: Vec3, fwd: Vec3, out: &mut Vec<u32>, keys: &mut Vec<u32>) {
    let n = c.n;
    keys.resize(n, 0);
    let mv = moves(p);
    // the camera into the splats' own space, so each depth is one dot product
    let inv = model.inverse();
    let cam_l = inv.transform_point3(cam);
    let fwd_l = model.transpose().transform_vector3(fwd);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let chunk = n.div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for (t, ks) in keys.chunks_mut(chunk).enumerate() {
            s.spawn(move || {
                for (j, k) in ks.iter_mut().enumerate() {
                    let i = t * chunk + j;
                    let home = c.pos(i);
                    let pos = if mv { anim_center(p, i as u32, home, target.map(|t| &t[i])) } else { home };
                    // far first: the larger the depth, the smaller the key
                    *k = !ordered((pos - cam_l).dot(fwd_l));
                }
            });
        }
    });
    let mut a: Vec<u32> = (0..n as u32).collect();
    let mut b = vec![0u32; n];
    for pass in 0..4 {
        let shift = pass * 8;
        let mut count = [0usize; 257];
        for &i in &a {
            count[((keys[i as usize] >> shift) & 0xff) as usize + 1] += 1;
        }
        for d in 0..256 {
            count[d + 1] += count[d];
        }
        for &i in &a {
            let d = ((keys[i as usize] >> shift) & 0xff) as usize;
            b[count[d]] = i;
            count[d] += 1;
        }
        std::mem::swap(&mut a, &mut b);
    }
    *out = a;
}

// ---- the row

/// A splat row's entity: its material, cloud, morph target, and the last sort (the camera and effects it was for).
#[derive(Component)]
pub(super) struct Splats {
    mat: Handle<SplatMaterial>,
    mask: Handle<SplatMask>,
    cloud: Arc<Cloud>,
    target: Option<Arc<Vec<[f32; 16]>>>,
    order: Handle<ShaderBuffer>,
    last: Option<(Mat4, Vec3, Vec3, SplatParams)>,
    keys: Vec<u32>,
}

fn s(v: &BTreeMap<String, V>, k: &str) -> Option<String> {
    match v.get(k) {
        Some(V::S(s)) => Some(s.clone()),
        _ => None,
    }
}

fn params(c: &Cloud, v: &BTreeMap<String, V>, bands: f32) -> SplatParams {
    let g = |k: &str, d: f32| num(v, k, d);
    let tint = if v.contains_key("tint_r") {
        Color::srgb(g("tint_r", 1.0), g("tint_g", 1.0), g("tint_b", 1.0))
    } else {
        colour(v.get("tint"), [1.0, 1.0, 1.0, 1.0])
    }
    .to_srgba();
    let glow = colour(v.get("glow"), [1.0, 0.92, 0.75, 1.0]).to_srgba();
    let rmode = match s(v, "reveal_mode").as_deref() {
        Some("plane") => 2,
        Some("radius") => 1,
        _ if v.contains_key("reveal") => 1,
        _ => 0,
    };
    SplatParams {
        sh_rot: c.rot.transpose(),
        center: c.center,
        radius: c.radius,
        count: c.n as u32,
        sh_deg: c.sh_deg,
        sh_words: c.sh_words,
        morph: g("morph", 0.0).clamp(0.0, 1.0),
        seed: g("seed", 0.0),
        twist: g("twist", 0.0),
        wave_amp: g("wave_amp", 0.0),
        wave_len: g("wave_len", 1.0),
        wave_phase: g("wave_phase", 0.0),
        explode: g("explode", 0.0).max(0.0),
        disperse: g("disperse", 0.0).clamp(0.0, 1.0),
        disperse_mode: if s(v, "disperse_mode").as_deref() == Some("dust") { 1 } else { 0 },
        wind: Vec3::new(g("wind_x", 1.0), g("wind_y", 0.15), g("wind_z", 0.0)),
        curl: g("curl", 1.0),
        reveal: g("reveal", 1.0).clamp(0.0, 1.0),
        reveal_mode: rmode,
        reveal_soft: g("reveal_soft", 0.15).max(1e-3),
        reveal_glow: g("reveal_glow", 0.0).max(0.0),
        reveal_origin: Vec3::new(g("reveal_x", c.center.x), g("reveal_y", c.center.y), g("reveal_z", c.center.z)),
        crop: g("crop", 0.0).max(0.0),
        reveal_axis: Vec3::new(g("reveal_ax", 0.0), g("reveal_ay", 1.0), g("reveal_az", 0.0)),
        bands: g("bands", bands).max(0.0),
        pulse: g("pulse", 0.0),
        pulse_scale: g("pulse_scale", 0.15),
        pulse_glow: g("pulse_glow", 0.0),
        splat_scale: g("splat_scale", 1.0).max(0.0),
        opacity: g("opacity", 1.0).clamp(0.0, 1.0),
        brightness: g("brightness", 1.0).max(0.0),
        saturation: g("saturation", 1.0).max(0.0),
        contrast: g("contrast", 1.0).max(0.0),
        tint: Vec4::new(tint.red, tint.green, tint.blue, 1.0),
        glow_color: Vec4::new(glow.red, glow.green, glow.blue, 1.0),
        paint: g("paint", 0.0).clamp(0.0, 1.0),
    }
}

/// A splat row: its file read (once per file) and drawn as one mesh of a quad a splat, its morph target matched.
pub(super) fn spawn(world: &mut World, e: Entity, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<(), String> {
    let vals = &t.vals;
    let src = s(vals, "src").ok_or(format!("splat {}: needs src (a .ply, .splat or .spz file)", t.id))?;
    let c = cloud(&resolve_src(inserted, &src, dir)?)?;
    let target = match s(vals, "morph_to").filter(|s| !s.is_empty()) {
        Some(to) => {
            let b = cloud(&resolve_src(inserted, &to, dir)?)?;
            Some(Arc::new(matched(&c, &b)))
        }
        None => None,
    };
    let n = c.n.max(1);
    // a quad a splat: its corners at (±1, ±1) and its place in the order in z
    let mut pos = Vec::with_capacity(n * 4);
    let mut idx = Vec::with_capacity(n * 6);
    for i in 0..n as u32 {
        let z = i as f32;
        pos.extend_from_slice(&[[-1.0, -1.0, z], [1.0, -1.0, z], [1.0, 1.0, z], [-1.0, 1.0, z]]);
        let b = i * 4;
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    let mut m = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::RENDER_WORLD);
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_indices(bevy::mesh::Indices::U32(idx));
    let mesh = world.resource_mut::<Assets<Mesh>>().add(m);
    let gpu = |w: &mut World, bytes: Vec<u8>, keep: bool| {
        let usage = if keep { bevy::asset::RenderAssetUsages::default() } else { bevy::asset::RenderAssetUsages::RENDER_WORLD };
        w.resource_mut::<Assets<ShaderBuffer>>().add(ShaderBuffer::new(&bytes, usage))
    };
    let g = if c.n == 0 { vec![[0f32; 16]] } else { c.gauss.clone() };
    let splats = gpu(world, bytes_of(&g), false);
    let sh = gpu(world, words_of(&c.sh), false);
    let tgt = gpu(world, bytes_of(target.as_deref().map(|t| t.as_slice()).filter(|t| !t.is_empty()).unwrap_or(&[[0f32; 16]])), false);
    let order = gpu(world, words_of(&(0..n as u32).collect::<Vec<_>>()), true);
    let p0 = params(&c, vals, 0.0);
    let mask = world.resource_mut::<Assets<SplatMask>>().add(SplatMask { p: p0, splats: splats.clone(), sh: sh.clone(), target: tgt.clone(), order: order.clone() });
    let mat = world.resource_mut::<Assets<SplatMaterial>>().add(SplatMaterial { p: p0, splats, sh, target: tgt, order: order.clone() });
    // the eraser on the looks' layers, a child so it moves and hides with the splats
    let layers = bevy::camera::visibility::RenderLayers::from_layers(&(1..=30).collect::<Vec<_>>());
    let m2 = mesh.clone();
    let child = world.spawn((Mesh3d(m2), MeshMaterial3d(mask.clone()), Transform::default(), Visibility::Inherited, layers, NotShadowCaster, bevy::camera::visibility::NoFrustumCulling)).id();
    world.entity_mut(e).add_child(child);
    world.entity_mut(e).insert((
        Mesh3d(mesh),
        MeshMaterial3d(mat.clone()),
        scene3d::transform_of(vals),
        NotShadowCaster,
        bevy::camera::visibility::NoFrustumCulling,
        Splats { mat, mask, cloud: c, target, order, last: None, keys: Vec::new() },
    ));
    Ok(())
}

/// Each splat row's effects from the frame's values, and its splats sorted for the frame's camera (after transforms
/// are propagated, so the camera and the splats are where this frame puts them). A frame whose camera, placement and
/// effects are the last sort's keeps that order (the same rows sort the same).
pub(super) fn sync_splats(
    frame: Res<FrameRows>,
    mut mats: ResMut<Assets<SplatMaterial>>,
    mut masks: ResMut<Assets<SplatMask>>,
    mut bufs: ResMut<Assets<ShaderBuffer>>,
    cams: Query<(Entity, &GlobalTransform), With<Camera3d>>,
    mut q: Query<(Entity, &GlobalTransform, &mut Splats)>,
) {
    // the scene's camera: the one a row moves
    let Some((cam_e, cam)) = cams.iter().find(|(e, _)| frame.ix.contains_key(e)) else { return };
    let cv = frame.vals(cam_e);
    // a paint or cartoon <Look>: the splats' light in flat bands too (more of them than a surface's: a photograph's
    // light varies more within a part than a flat-coloured mesh's)
    let bands = match cv.as_ref().and_then(|v| s(v, "frame_look")).as_deref() {
        Some("paint") | Some("cartoon") => num(cv.as_ref().unwrap(), "paint_bands", 3.0) + 2.0,
        _ => 0.0,
    };
    let cam_pos = cam.translation();
    let fwd = cam.forward().as_vec3();
    for (e, gt, mut sp) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let np = params(&sp.cloud, &v, bands);
        if mats.get(&sp.mat).is_some_and(|m| m.p != np) {
            if let Some(mut m) = mats.get_mut(&sp.mat) {
                m.p = np;
            }
            if let Some(mut m) = masks.get_mut(&sp.mask) {
                m.p = np;
            }
        }
        let model = gt.to_matrix();
        let key = (model, cam_pos, fwd, np);
        if sp.last.as_ref() == Some(&key) {
            continue;
        }
        let mut order = Vec::new();
        let sp = &mut *sp;
        sort(&sp.cloud, sp.target.as_deref().map(|t| t.as_slice()), &np, model, cam_pos, fwd, &mut order, &mut sp.keys);
        if order.is_empty() {
            order.push(0);
        }
        if let Some(mut b) = bufs.get_mut(&sp.order) {
            b.data = Some(words_of(&order));
        }
        sp.last = Some(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud_of(pts: &[[f32; 3]]) -> Cloud {
        let raw = Raw {
            pos: pts.to_vec(),
            scale: vec![[0.1; 3]; pts.len()],
            rot: vec![[1.0, 0.0, 0.0, 0.0]; pts.len()],
            opacity: vec![1.0; pts.len()],
            color: vec![[0.5; 3]; pts.len()],
            deg: 0,
            rest: Vec::new(),
        };
        place(raw, Mat4::IDENTITY)
    }

    #[test]
    fn half_floats_round_as_wgsl_reads_them() {
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(-2.0), 0xc000);
        assert_eq!(f16_bits(0.0), 0);
        assert_eq!(f16_bits(65504.0), 0x7bff);
        assert_eq!(f16_bits(1e6), 0x7c00);
        // subnormal: 2^-24
        assert_eq!(f16_bits(5.960_464_5e-8), 1);
    }

    #[test]
    fn the_sort_is_far_to_near_and_stable() {
        let c = cloud_of(&[[0.0, 0.0, -1.0], [0.0, 0.0, -5.0], [1.0, 0.0, -5.0], [0.0, 0.0, -3.0]]);
        let p = params(&c, &BTreeMap::new(), 0.0);
        let (mut out, mut keys) = (Vec::new(), Vec::new());
        sort(&c, None, &p, Mat4::IDENTITY, Vec3::ZERO, Vec3::NEG_Z, &mut out, &mut keys);
        // 1 and 2 tie at depth 5: index order
        assert_eq!(out, vec![1, 2, 3, 0]);
        let again = out.clone();
        sort(&c, None, &p, Mat4::IDENTITY, Vec3::ZERO, Vec3::NEG_Z, &mut out, &mut keys);
        assert_eq!(out, again);
    }

    #[test]
    fn effects_are_a_function_of_the_row() {
        let c = cloud_of(&[[0.3, 0.2, 0.1], [-0.4, 0.5, 0.2], [0.1, -0.3, 0.6]]);
        let mut v = BTreeMap::new();
        v.insert("disperse".to_string(), V::N(0.6));
        v.insert("explode".to_string(), V::N(0.3));
        v.insert("twist".to_string(), V::N(0.5));
        v.insert("seed".to_string(), V::N(7.0));
        let p = params(&c, &v, 0.0);
        let a: Vec<Vec3> = (0..3).map(|i| anim_center(&p, i, c.pos(i as usize), None)).collect();
        let b: Vec<Vec3> = (0..3).map(|i| anim_center(&p, i, c.pos(i as usize), None)).collect();
        assert_eq!(a, b);
        assert!(a.iter().zip(0..3).all(|(p, i)| (*p - c.pos(i)).length() > 1e-3));
    }

    #[test]
    fn morph_matches_every_source_splat() {
        let a = cloud_of(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 1.0]]);
        let b = cloud_of(&[[5.0, 5.0, 5.0], [6.0, 6.0, 6.0]]);
        let m = matched(&a, &b);
        assert_eq!(m.len(), 4);
        assert!(m.iter().all(|g| g[0] >= 5.0));
    }

    #[test]
    fn hilbert_neighbours_are_close() {
        // consecutive cells along the curve are face neighbours
        let mut cells: Vec<(u32, [i32; 3])> = Vec::new();
        for x in 0..8 {
            for y in 0..8 {
                for z in 0..8 {
                    let p = Vec3::new(x as f32, y as f32, z as f32) * (128.0 / 1023.0) + Vec3::splat(0.0001);
                    cells.push((hilbert(p), [x, y, z]));
                }
            }
        }
        cells.sort();
        let far = cells.windows(2).filter(|w| (0..3).map(|k| (w[0].1[k] - w[1].1[k]).abs()).sum::<i32>() > 1).count();
        assert_eq!(far, 0, "the curve jumps {far} times");
    }
}
