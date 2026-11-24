//! The light that bounces, traced on the GPU (docs/bake.feature BK3): each texel's paths through the model's own
//! triangles by wgpu's ray queries (Metal, Vulkan, DX12), on raea-gpu-device, the process's one shared device. A bake
//! runs offline, before any frame, so it builds its own BLAS and TLAS from the meshes it bakes rather than reading
//! systems/rt's from a Bevy render world; the kernel's WGSL (wgsl/diffuse.wgsl) is a module rt can include as it is.
//! A device without ray queries is refused with the reason, and the composition renders unbaked.

use wgpu::util::DeviceExt;

/// What a triangle sends on: the share of light it reflects (diffuse) and the light it gives off, linear RGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub albedo: [f32; 3],
    pub emission: [f32; 3],
}

/// The world a bake traces: triangles in world space, each with its surface.
pub struct World<'a> {
    pub positions: &'a [[f32; 3]],
    pub triangles: &'a [[u32; 3]],
    pub surfaces: &'a [Surface],
}

/// What's traced from: each texel's origin (BK2, already off its surface), its normal, and whether it's inside other
/// geometry (left at zero for BK4 to fill), and the offset each bounce leaves its surface by.
pub struct Starts<'a> {
    pub origin: &'a [[f32; 3]],
    pub normal: &'a [[f32; 3]],
    pub inside: &'a [bool],
    pub offset: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// samples a texel
    pub samples: u32,
    /// a path's most bounces
    pub bounces: u32,
    /// samples a texel in each pass: a pass short enough that the machine stays responsive
    pub per_pass: u32,
    /// the sky's light, linear RGB (the composition's <Environment> comes in BK3's next step)
    pub sky: [f32; 3],
}

impl Default for Settings {
    fn default() -> Self {
        Self { samples: 512, bounces: 4, per_pass: 16, sky: [1.0; 3] }
    }
}

/// Texels a dispatch: under wgpu's 65,535 workgroups of 64, and short enough that the GPU stays the machine's.
const CHUNK: u32 = 1 << 18;

/// The light reaching each texel (the mean its cosine-weighted rays bring back), in `from`'s order; inside texels 0.
pub fn trace(world: &World, from: &Starts, s: &Settings, progress: &mut dyn FnMut(u32, u32)) -> Result<Vec<[f32; 3]>, String> {
    let n = from.origin.len();
    if from.normal.len() != n || from.inside.len() != n || world.surfaces.len() != world.triangles.len() {
        return Err("trace: the texels' origins, normals and inside marks, and the triangles' surfaces, must match".into());
    }
    if n == 0 || world.triangles.is_empty() {
        return Ok(vec![[0.0; 3]; n]);
    }
    let (_, adapter, device, queue) = raea_gpu_device::device()?;
    if !device.features().contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY) {
        let info = adapter.get_info();
        return Err(format!("the light bake needs ray queries, which this GPU doesn't offer ({} on {:?}): the composition renders unbaked", info.name, info.backend));
    }
    // every GPU error is the bake's: caught here and returned, never a black map kept as if it were light
    let scopes = [wgpu::ErrorFilter::Validation, wgpu::ErrorFilter::OutOfMemory, wgpu::ErrorFilter::Internal].map(|f| device.push_error_scope(f));
    let light = dispatch(&device, &queue, world, from, s, progress);
    let errors: Vec<String> = scopes.into_iter().rev().filter_map(|g| pollster::block_on(g.pop())).map(|e| e.to_string()).collect();
    if !errors.is_empty() {
        return Err(format!("the light bake's trace failed on the GPU: {}", errors.join("; ")));
    }
    light
}

/// The passes over a device known to have ray queries, and the texels' light read back.
fn dispatch(device: &wgpu::Device, queue: &wgpu::Queue, world: &World, from: &Starts, s: &Settings, progress: &mut dyn FnMut(u32, u32)) -> Result<Vec<[f32; 3]>, String> {
    let n = from.origin.len();
    let tlas = accel(device, queue, world);
    let vec4 = |v: [f32; 3], w: f32| [v[0], v[1], v[2], w];
    let positions: Vec<[f32; 4]> = world.positions.iter().map(|p| vec4(*p, 1.0)).collect();
    let surfaces: Vec<[f32; 4]> = world.surfaces.iter().flat_map(|s| [vec4(s.albedo, 0.0), vec4(s.emission, 0.0)]).collect();
    let texels: Vec<[f32; 4]> = (0..n).flat_map(|i| [vec4(from.origin[i], from.inside[i] as u32 as f32), vec4(from.normal[i], 0.0)]).collect();
    let storage = |label: &str, bytes: &[u8], usage: wgpu::BufferUsages| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents: bytes, usage: wgpu::BufferUsages::STORAGE | usage });
    let index: Vec<u32> = world.triangles.iter().flatten().copied().collect();
    let bufs = [
        storage("lightbake positions", bytemuck::cast_slice(&positions), wgpu::BufferUsages::empty()),
        storage("lightbake indices", bytemuck::cast_slice(&index), wgpu::BufferUsages::empty()),
        storage("lightbake surfaces", bytemuck::cast_slice(&surfaces), wgpu::BufferUsages::empty()),
        storage("lightbake texels", bytemuck::cast_slice(&texels), wgpu::BufferUsages::empty()),
        storage("lightbake accum", &vec![0u8; n * 16], wgpu::BufferUsages::COPY_SRC),
    ];
    let params = device.create_buffer(&wgpu::BufferDescriptor { label: Some("lightbake params"), size: 48, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let src = format!("enable wgpu_ray_query;\n{}\n{}", include_str!("wgsl/diffuse.wgsl"), include_str!("wgsl/trace.wgsl"));
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("lightbake trace"), source: wgpu::ShaderSource::Wgsl(src.into()) });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("lightbake trace"), layout: None, module: &module, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
    let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::AccelerationStructure(&tlas) }];
    entries.extend(bufs.iter().enumerate().map(|(k, b)| wgpu::BindGroupEntry { binding: k as u32 + 1, resource: b.as_entire_binding() }));
    entries.push(wgpu::BindGroupEntry { binding: 6, resource: params.as_entire_binding() });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("lightbake trace"), layout: &pipeline.get_bind_group_layout(0), entries: &entries });
    let passes = s.samples.div_ceil(s.per_pass.max(1));
    for pass in 0..passes {
        let per = s.per_pass.max(1).min(s.samples - pass * s.per_pass.max(1));
        for first in (0..n as u32).step_by(CHUNK as usize) {
            let mut p = [0u32; 12];
            for (k, v) in s.sky.iter().enumerate() {
                p[k] = v.to_bits();
            }
            let end = (first + CHUNK).min(n as u32);
            (p[4], p[5], p[6], p[7], p[8], p[9]) = (end, pass, per, s.bounces, from.offset.to_bits(), first);
            queue.write_buffer(&params, 0, bytemuck::cast_slice(&p));
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("lightbake pass") });
            {
                let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("lightbake pass"), timestamp_writes: None });
                cp.set_pipeline(&pipeline);
                cp.set_bind_group(0, &bind, &[]);
                cp.dispatch_workgroups((end - first).div_ceil(64), 1, 1);
            }
            queue.submit([enc.finish()]);
            device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(120)) }).map_err(|e| format!("the light bake's pass {pass}: {e}"))?;
        }
        progress(pass + 1, passes);
    }
    let acc = read(device, queue, &bufs[4], n * 16)?;
    let acc: &[[f32; 4]] = bytemuck::cast_slice(&acc);
    Ok(acc.iter().map(|a| if a[3] > 0.0 { [a[0] / a[3], a[1] / a[3], a[2] / a[3]] } else { [0.0; 3] }).collect())
}

/// The world's one BLAS (its triangles, opaque) under a TLAS of one instance where it stands.
fn accel(device: &wgpu::Device, queue: &wgpu::Queue, world: &World) -> wgpu::Tlas {
    let input = |label: &str, bytes: &[u8]| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents: bytes, usage: wgpu::BufferUsages::BLAS_INPUT });
    let index: Vec<u32> = world.triangles.iter().flatten().copied().collect();
    let (vb, ib) = (input("lightbake blas vertices", bytemuck::cast_slice(world.positions)), input("lightbake blas indices", bytemuck::cast_slice(&index)));
    let size = wgpu::BlasTriangleGeometrySizeDescriptor { vertex_format: wgpu::VertexFormat::Float32x3, vertex_count: world.positions.len() as u32, index_format: Some(wgpu::IndexFormat::Uint32), index_count: Some(index.len() as u32), flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE };
    let flags = (wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE, wgpu::AccelerationStructureUpdateMode::Build);
    let blas = device.create_blas(&wgpu::CreateBlasDescriptor { label: Some("lightbake blas"), flags: flags.0, update_mode: flags.1 }, wgpu::BlasGeometrySizeDescriptors::Triangles { descriptors: vec![size.clone()] });
    let mut tlas = device.create_tlas(&wgpu::CreateTlasDescriptor { label: Some("lightbake tlas"), flags: flags.0, update_mode: flags.1, max_instances: 1 });
    *tlas.get_mut_single(0).expect("a TLAS of one") = Some(wgpu::TlasInstance::new(&blas, [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0], 0, 0xff));
    let entry = wgpu::BlasBuildEntry {
        blas: &blas,
        geometry: wgpu::BlasGeometries::TriangleGeometries(vec![wgpu::BlasTriangleGeometry { size: &size, vertex_buffer: &vb, first_vertex: 0, vertex_stride: 12, index_buffer: Some(&ib), first_index: Some(0), transform_buffer: None, transform_buffer_offset: None }]),
    };
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("lightbake accel") });
    enc.build_acceleration_structures([&entry], [&tlas]);
    queue.submit([enc.finish()]);
    tlas
}

/// A storage buffer's bytes back on the CPU.
fn read(device: &wgpu::Device, queue: &wgpu::Queue, buf: &wgpu::Buffer, size: usize) -> Result<Vec<u8>, String> {
    let back = device.create_buffer(&wgpu::BufferDescriptor { label: Some("lightbake readback"), size: size as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("lightbake readback") });
    enc.copy_buffer_to_buffer(buf, 0, &back, 0, size as u64);
    queue.submit([enc.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    back.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(60)) }).map_err(|e| format!("the light bake's readback: {e}"))?;
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| format!("the light bake's readback: {e}"))?;
    let bytes = back.slice(..).get_mapped_range().map_err(|e| format!("the light bake's readback: {e}"))?.to_vec();
    Ok(bytes)
}

#[cfg(test)]
mod tests;
