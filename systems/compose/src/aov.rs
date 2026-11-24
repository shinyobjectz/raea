//! RT1 (docs/raytracing.feature): each frame's motion vectors and depth read back from the 3D cameras' prepasses, the
//! guide data a neural or denoising pass after the render needs (MLX-DLSS's run-sequence, MetalFX). raea-render's
//! Engine asks for it before the first draw (want_aov: `prepasses` on every 3D camera) and reads each frame's first
//! draw (`read`).

use bevy::core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass, ViewPrepassTextures};
use bevy::prelude::*;

use crate::passes::PrepassRead;

/// One frame's guide data at the prepass's size (the frame's, times RAEA_SS).
pub struct Aov {
    pub w: u32,
    pub h: u32,
    /// w x h x 2, Bevy's motion vectors: this frame's UV minus the last frame's (x right, y down)
    pub motion: Vec<f32>,
    /// w x h, reverse Z (1 at the near plane, 0 at infinity)
    pub depth: Vec<f32>,
}

/// Each 3D camera draws both prepasses, single-sampled on every frame: PrepassRead keeps the row's post-processing from
/// putting MSAA back (#159: raea-sys-post's sync did on the first frame whose post changed, msaa_for_3d on each frame
/// of a live scene, and frame 2's readback was refused). Marked by PrepassRead, not by the motion-vector prepass, which
/// a camera with motion blur has already.
pub fn prepasses(mut cmd: Commands, q: Query<Entity, (With<Camera3d>, Or<(Without<PrepassRead>, Without<DepthPrepass>, Without<MotionVectorPrepass>)>)>) {
    for e in &q {
        cmd.entity(e).insert((DepthPrepass, MotionVectorPrepass, Msaa::Off, PrepassRead));
    }
}

// the motion texture is made without COPY_SRC (bevy_core_pipeline's prepare_core_3d_depth_textures), so it's read in
// a shader, not copied: both textures into one buffer, three floats a pixel
const AOV_WGSL: &str = r#"
@group(0) @binding(0) var depth_t: texture_depth_2d;
@group(0) @binding(1) var motion_t: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<f32>;
@group(0) @binding(3) var<uniform> dims: vec4<u32>;
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= dims.x || id.y >= dims.y) { return; }
    let p = vec2<i32>(id.xy);
    let m = textureLoad(motion_t, p, 0).xy;
    let i = (id.y * dims.x + id.x) * 3u;
    out[i] = m.x;
    out[i + 1u] = m.y;
    out[i + 2u] = textureLoad(depth_t, p, 0);
}
"#;

/// The guide data of the frame just drawn, read from the render world `rw`: the biggest view with both prepasses (the
/// frame's camera; a screen's or a mirror's is smaller), cropped to the frame, `fw` x `fh` before RAEA_SS.
pub fn read(rw: &mut World, fw: u32, fh: u32) -> Result<Aov, String> {
    // the biggest view with both (the frame's camera; a screen's or a mirror's is smaller)
    let mut q = rw.query::<&ViewPrepassTextures>();
    let pick = q
        .iter(rw)
        .filter(|t| matches!(t, ViewPrepassTextures { depth: Some(_), motion_vectors: Some(_), .. }))
        .max_by_key(|t| u64::from(t.size.width) * u64::from(t.size.height))
        .ok_or("no 3D camera drew a depth and motion-vector prepass")?;
    // the target is a row taller than the frame (the frame tag's row, open.rs), at the frame's scale (RAEA_SS)
    let (w, h) = (pick.size.width, pick.size.height.min(fh * (pick.size.width / fw.max(1)).max(1)));
    // the bindings below are single-sampled: a multisampled prepass is said here, not given to the GPU, whose
    // validation error stops the render
    let samples = pick.motion_vectors.as_ref().unwrap().texture.texture.sample_count();
    if samples > 1 {
        return Err(format!("the prepass is multisampled ({samples} samples)"));
    }
    let depth = pick.depth_only_view().ok_or("the prepass has no depth view")?.clone();
    let motion = pick.motion_vectors.as_ref().unwrap().texture.default_view.clone();
    let device = rw.resource::<bevy::render::renderer::RenderDevice>().wgpu_device().clone();
    let queue = rw.resource::<bevy::render::renderer::RenderQueue>().clone();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("aov"), source: wgpu::ShaderSource::Wgsl(AOV_WGSL.into()) });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("aov"), layout: None, module: &module, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
    let size = w as u64 * h as u64 * 12;
    let out = device.create_buffer(&wgpu::BufferDescriptor { label: Some("aov"), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
    let back = device.create_buffer(&wgpu::BufferDescriptor { label: Some("aov readback"), size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let dims = wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor { label: Some("aov dims"), contents: bytemuck::cast_slice(&[w, h, 0u32, 0u32]), usage: wgpu::BufferUsages::UNIFORM });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("aov"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&*depth) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&*motion) },
            wgpu::BindGroupEntry { binding: 2, resource: out.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: dims.as_entire_binding() },
        ],
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("aov") });
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
    }
    enc.copy_buffer_to_buffer(&out, 0, &back, 0, size);
    queue.submit([enc.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    back.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(60)) }).map_err(|e| format!("the guide data's readback: {e}"))?;
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| format!("the guide data's readback: {e}"))?;
    let data = back.slice(..).get_mapped_range().map_err(|e| format!("the guide data's readback: {e:?}"))?;
    let px: &[f32] = bytemuck::cast_slice(&data);
    let n = (w * h) as usize;
    let (mut motion, mut depth) = (Vec::with_capacity(n * 2), Vec::with_capacity(n));
    for p in px.chunks_exact(3) {
        motion.extend_from_slice(&p[..2]);
        depth.push(p[2]);
    }
    drop(data);
    back.unmap();
    Ok(Aov { w, h, motion, depth })
}
