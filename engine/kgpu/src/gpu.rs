//! The GPU backend for K (wgpu: Metal here, Vulkan/DX12 elsewhere). Every vector is a storage buffer that stays on
//! the GPU; work is queued in order and only `download` waits. Fused expressions compile to one kernel each (cached
//! by their WGSL); reductions and scans are block trees (exact for integers in any order); grade is a bitonic sort of
//! (key, index) pairs, so equal keys keep their order.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wgpu::util::DeviceExt;

use crate::k::{Backend, F, Handle};

type Pool = Rc<RefCell<HashMap<u64, Vec<wgpu::Buffer>>>>;

pub struct GpuVec {
    pub buf: wgpu::Buffer,
    pub len: usize,
    pool: Option<Pool>,
}

impl Drop for GpuVec {
    /// the buffer goes back to the pool (work already recorded against it runs in order, so reuse is safe)
    fn drop(&mut self) {
        if let Some(p) = &self.pool {
            p.borrow_mut().entry(self.buf.size()).or_default().push(self.buf.clone());
        }
    }
}

impl Handle for GpuVec {
    fn len(&self) -> usize {
        self.len
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn gv(h: &Rc<dyn Handle>) -> &GpuVec {
    h.as_any().downcast_ref::<GpuVec>().expect("a GPU vector")
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: String,
    pipelines: RefCell<HashMap<String, wgpu::ComputePipeline>>,
    /// kernels dispatched (for the bench)
    pub dispatches: RefCell<usize>,
    /// work recorded but not yet submitted: one submission per read, not per kernel
    enc: RefCell<Option<wgpu::CommandEncoder>>,
    pool: Pool,
}

const WG: u32 = 256;

fn wgsl_f(f: &F, out: &mut String) {
    match f {
        F::Leaf(k) => out.push_str(&format!("bitcast<i32>(l{k}[i])")),
        F::Idx => out.push_str("i32(i)"),
        F::Lit(v) => out.push_str(&format!("({v}i)")),
        F::Un(o, a) => {
            let mut s = String::new();
            wgsl_f(a, &mut s);
            out.push_str(&match o {
                '-' => format!("(0i - {s})"),
                '~' => format!("select(0i, 1i, {s} == 0i)"),
                _ => format!("abs({s})"),
            });
        }
        F::Bin(o, a, b) => {
            let (mut x, mut y) = (String::new(), String::new());
            wgsl_f(a, &mut x);
            wgsl_f(b, &mut y);
            out.push_str(&match o {
                '+' | '-' | '*' | '^' => format!("({x} {o} {y})"),
                // division and mod by zero give 0, as the CPU reference does
                '%' => format!("kdiv({x}, {y})"),
                '!' => format!("kmod({x}, {y})"),
                '&' => format!("min({x}, {y})"),
                '|' => format!("max({x}, {y})"),
                '<' | '>' => format!("select(0i, 1i, {x} {o} {y})"),
                _ => format!("select(0i, 1i, {x} == {y})"),
            });
        }
        F::Gather(k, e) => {
            let mut s = String::new();
            wgsl_f(e, &mut s);
            out.push_str(&format!("kgather{k}({s})"));
        }
    }
}

fn leaves_of(f: &F, out: &mut Vec<usize>) {
    match f {
        F::Leaf(k) | F::Gather(k, _) => {
            out.push(*k);
            if let F::Gather(_, e) = f {
                leaves_of(e, out);
            }
        }
        F::Un(_, a) => leaves_of(a, out),
        F::Bin(_, a, b) => {
            leaves_of(a, out);
            leaves_of(b, out);
        }
        _ => {}
    }
}

const HELPERS: &str = "
fn kdiv(a: i32, b: i32) -> i32 { if (b == 0i) { return 0i; } if (a == -2147483647i - 1i && b == -1i) { return a; } return a / b; }
fn kmod(a: i32, b: i32) -> i32 { if (b == 0i) { return 0i; } if (b == -1i) { return 0i; } return a % b; }
struct Shape { n: u32, stride: u32, a: u32, b: u32 }
";

impl Gpu {
    pub fn new() -> Result<Gpu, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }))
            .map_err(|e| format!("no GPU adapter: {e}"))?;
        let info = adapter.get_info();
        let mut limits = wgpu::Limits::default();
        limits.max_storage_buffer_binding_size = adapter.limits().max_storage_buffer_binding_size;
        limits.max_buffer_size = adapter.limits().max_buffer_size;
        limits.max_storage_buffers_per_shader_stage = adapter.limits().max_storage_buffers_per_shader_stage.min(16);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: limits, ..Default::default() })).map_err(|e| format!("device: {e}"))?;
        Ok(Gpu { device, queue, adapter: format!("{} ({:?})", info.name, info.backend), pipelines: RefCell::new(HashMap::new()), dispatches: RefCell::new(0), enc: RefCell::new(None), pool: Rc::new(RefCell::new(HashMap::new())) })
    }

    fn pipeline(&self, src: &str) -> wgpu::ComputePipeline {
        if let Some(p) = self.pipelines.borrow().get(src) {
            return p.clone();
        }
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("kgpu k"), source: wgpu::ShaderSource::Wgsl(src.into()) });
        let p = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("kgpu k"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        self.pipelines.borrow_mut().insert(src.to_string(), p.clone());
        p
    }

    /// A vector handle over a buffer, returned to the pool when dropped.
    fn vec(&self, buf: wgpu::Buffer, len: usize) -> Rc<dyn Handle> {
        Rc::new(GpuVec { buf, len, pool: Some(self.pool.clone()) })
    }

    /// Record onto the pending encoder.
    fn record(&self, f: impl FnOnce(&mut wgpu::CommandEncoder)) {
        let mut e = self.enc.borrow_mut();
        if e.is_none() {
            *e = Some(self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kgpu run") }));
        }
        f(e.as_mut().unwrap());
    }

    /// Submit what's recorded.
    pub fn flush(&self) {
        if let Some(e) = self.enc.borrow_mut().take() {
            self.queue.submit([e.finish()]);
            raea_gputrace::submitted(&self.queue, "kgpu", "kgpu run");
            raea_gputrace::submitted(&self.queue, "kgpu", "kgpu run");
        }
    }

    fn new_buf(&self, len: usize) -> wgpu::Buffer {
        // sizes round up to 4 KiB so the pool finds matches
        let size = ((len.max(1) * 4) as u64).div_ceil(4096) * 4096;
        if let Some(b) = self.pool.borrow_mut().get_mut(&size).and_then(|v| v.pop()) {
            return b;
        }
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Dispatch `src` over `n` threads with buffers bound in order (binding k = bufs[k]) and the shape at the end.
    fn dispatch(&self, src: &str, bufs: &[&wgpu::Buffer], n: usize, a: u32, b: u32) {
        let groups = (n as u32).div_ceil(WG).max(1);
        let gx = groups.min(65535);
        let gy = groups.div_ceil(gx);
        let shape = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[n as u32, gx * WG, a, b]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let p = self.pipeline(src);
        let layout = p.get_bind_group_layout(0);
        let mut entries: Vec<wgpu::BindGroupEntry> = bufs.iter().enumerate().map(|(k, b)| wgpu::BindGroupEntry { binding: k as u32, resource: b.as_entire_binding() }).collect();
        entries.push(wgpu::BindGroupEntry { binding: bufs.len() as u32, resource: shape.as_entire_binding() });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &entries });
        self.record(|enc| {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&p);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(gx, gy, 1);
        });
        *self.dispatches.borrow_mut() += 1;
    }

    fn header(inputs: usize, outputs: usize) -> String {
        let mut s = String::from(HELPERS);
        for k in 0..inputs {
            s.push_str(&format!("@group(0) @binding({k}) var<storage, read> l{k}: array<u32>;\n"));
        }
        for o in 0..outputs {
            s.push_str(&format!("@group(0) @binding({}) var<storage, read_write> o{o}: array<u32>;\n", inputs + o));
        }
        s.push_str(&format!("@group(0) @binding({}) var<uniform> shape: Shape;\n", inputs + outputs));
        s
    }

    /// Wait for everything queued so far (the bench's barrier).
    pub fn wait(&self) {
        self.flush();
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }

    fn reduce_buf(&self, op: char, buf: &wgpu::Buffer, n: usize) -> wgpu::Buffer {
        // each thread folds 8 elements, then the workgroup folds its 256 in shared memory, in a fixed tree order
        let (id, f) = match op {
            '+' => ("0i", "a + b"),
            '*' => ("1i", "a * b"),
            '&' => ("2147483647i", "min(a, b)"),
            _ => ("(-2147483647i - 1i)", "max(a, b)"),
        };
        let src = format!(
            "{}var<workgroup> sh: array<i32, 256>;
fn op(a: i32, b: i32) -> i32 {{ return {f}; }}
@compute @workgroup_size(256) fn main(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {{
  let g = wid.x + wid.y * nw.x;
  let base = g * 2048u + lid.x;
  var acc: i32 = {id};
  for (var k = 0u; k < 8u; k = k + 1u) {{ let j = base + k * 256u; if (j < shape.a) {{ acc = op(acc, bitcast<i32>(l0[j])); }} }}
  sh[lid.x] = acc;
  workgroupBarrier();
  for (var s = 128u; s > 0u; s = s >> 1u) {{ if (lid.x < s) {{ sh[lid.x] = op(sh[lid.x], sh[lid.x + s]); }} workgroupBarrier(); }}
  if (lid.x == 0u) {{ o0[g] = bitcast<u32>(sh[0]); }}
}}",
            Self::header(1, 1)
        );
        let groups = n.div_ceil(2048).max(1);
        let out = self.new_buf(groups);
        self.dispatch(&src, &[buf, &out], groups * WG as usize, n as u32, 0);
        if groups == 1 { out } else { self.reduce_buf(op, &out, groups) }
    }

    /// Inclusive scan in place over `n` elements: scan each 256 block, scan the block totals (recursively), add back.
    fn scan_buf(&self, op: char, buf: &wgpu::Buffer, n: usize) {
        let (id, f) = match op {
            '+' => ("0i", "a + b"),
            '*' => ("1i", "a * b"),
            '&' => ("2147483647i", "min(a, b)"),
            _ => ("(-2147483647i - 1i)", "max(a, b)"),
        };
        let blocks = n.div_ceil(256);
        let sums = self.new_buf(blocks);
        let src = format!(
            "{}var<workgroup> sh: array<i32, 256>;
fn op(a: i32, b: i32) -> i32 {{ return {f}; }}
@group(0) @binding(0) var<storage, read_write> x: array<u32>;
@group(0) @binding(1) var<storage, read_write> sums: array<u32>;
@group(0) @binding(2) var<uniform> shape: Shape;
@compute @workgroup_size(256) fn main(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {{
  let g = wid.x + wid.y * nw.x;
  let j = g * 256u + lid.x;
  var v: i32 = {id};
  if (j < shape.a) {{ v = bitcast<i32>(x[j]); }}
  sh[lid.x] = v;
  workgroupBarrier();
  for (var s = 1u; s < 256u; s = s << 1u) {{
    var t: i32 = sh[lid.x];
    if (lid.x >= s) {{ t = op(sh[lid.x - s], t); }}
    workgroupBarrier();
    sh[lid.x] = t;
    workgroupBarrier();
  }}
  if (j < shape.a) {{ x[j] = bitcast<u32>(sh[lid.x]); }}
  if (lid.x == 255u) {{ sums[g] = bitcast<u32>(sh[255]); }}
}}",
            HELPERS
        );
        self.dispatch(&src, &[buf, &sums], blocks * 256, n as u32, 0);
        if blocks > 1 {
            self.scan_buf(op, &sums, blocks);
            let add = format!(
                "{}fn op(a: i32, b: i32) -> i32 {{ return {f}; }}
@group(0) @binding(0) var<storage, read_write> x: array<u32>;
@group(0) @binding(1) var<storage, read_write> sums: array<u32>;
@group(0) @binding(2) var<uniform> shape: Shape;
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
  let j = gid.x + gid.y * shape.stride;
  if (j >= shape.n || j < 256u) {{ return; }}
  x[j] = bitcast<u32>(op(bitcast<i32>(sums[j / 256u - 1u]), bitcast<i32>(x[j])));
}}",
                HELPERS
            );
            self.dispatch(&add, &[buf, &sums], n, 0, 0);
        }
    }

    fn copy(&self, src: &wgpu::Buffer, len: usize) -> wgpu::Buffer {
        let out = self.new_buf(len);
        self.record(|enc| enc.copy_buffer_to_buffer(src, 0, &out, 0, (len.max(1) * 4) as u64));
        out
    }

    /// Bytes [off, off+len*4) of a buffer, read back.
    fn read_range(&self, buf: &wgpu::Buffer, off: u64, len: usize) -> Vec<i32> {
        let size = (len * 4) as u64;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.record(|enc| enc.copy_buffer_to_buffer(buf, off, &staging, 0, size));
        self.flush();
        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let v: Vec<i32> = bytemuck::cast_slice(&staging.slice(..).get_mapped_range()).to_vec();
        v
    }
}

impl Backend for Gpu {
    fn name(&self) -> &'static str {
        "gpu"
    }

    fn upload(&self, v: &[i32]) -> Rc<dyn Handle> {
        let data: &[i32] = if v.is_empty() { &[0] } else { v };
        let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        });
        Rc::new(GpuVec { buf, len: v.len(), pool: None })
    }

    fn download(&self, h: &Rc<dyn Handle>) -> Vec<i32> {
        let g = gv(h);
        if g.len == 0 {
            return vec![];
        }
        self.read_range(&g.buf, 0, g.len)
    }

    fn download_one(&self, h: &Rc<dyn Handle>, i: usize) -> i32 {
        self.read_range(&gv(h).buf, (i * 4) as u64, 1)[0]
    }

    fn eval(&self, f: &F, leaves: &[Rc<dyn Handle>], len: usize) -> Rc<dyn Handle> {
        let mut used = Vec::new();
        leaves_of(f, &mut used);
        let mut src = Self::header(leaves.len(), 1);
        for k in 0..leaves.len() {
            src.push_str(&format!(
                "fn kgather{k}(j: i32) -> i32 {{ if (j < 0i || u32(j) >= {}u) {{ return 0i; }} return bitcast<i32>(l{k}[u32(j)]); }}\n",
                leaves[k].len()
            ));
        }
        let mut e = String::new();
        wgsl_f(f, &mut e);
        // every input bound, even one only a gather reads: an automatic layout keeps only what main reaches
        let touch: String = (0..leaves.len()).map(|k| format!(" + l{k}[0]")).collect();
        src.push_str(&format!(
            "@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
  let i = gid.x + gid.y * shape.stride;
  if (shape.a == 4294967295u) {{ o0[0] = 0u{touch}; }}
  if (i >= shape.n) {{ return; }}
  o0[i] = bitcast<u32>({e});
}}"
        ));
        let out = self.new_buf(len);
        let mut bufs: Vec<&wgpu::Buffer> = leaves.iter().map(|h| &gv(h).buf).collect();
        bufs.push(&out);
        if len > 0 {
            self.dispatch(&src, &bufs, len, 0, 0);
        }
        let _ = used;
        self.vec(out, len)
    }

    fn reduce(&self, op: char, f: &F, leaves: &[Rc<dyn Handle>], len: usize) -> i32 {
        if len == 0 {
            return match op {
                '+' => 0,
                '*' => 1,
                '&' => i32::MAX,
                _ => i32::MIN,
            };
        }
        // first pass: each thread folds 8 elements of the fused expression (nothing is written out), each workgroup
        // folds its 256 in a fixed tree; then the partials reduce as a plain buffer
        let (id, fold) = match op {
            '+' => ("0i", "a + b"),
            '*' => ("1i", "a * b"),
            '&' => ("2147483647i", "min(a, b)"),
            _ => ("(-2147483647i - 1i)", "max(a, b)"),
        };
        let mut src = Self::header(leaves.len(), 1);
        for k in 0..leaves.len() {
            src.push_str(&format!(
                "fn kgather{k}(j: i32) -> i32 {{ if (j < 0i || u32(j) >= {}u) {{ return 0i; }} return bitcast<i32>(l{k}[u32(j)]); }}\n",
                leaves[k].len()
            ));
        }
        let mut e = String::new();
        wgsl_f(&f.clone(), &mut e);
        let touch: String = (0..leaves.len()).map(|k| format!(" + l{k}[0]")).collect();
        src.push_str(&format!(
            "var<workgroup> sh: array<i32, 256>;
fn op(a: i32, b: i32) -> i32 {{ return {fold}; }}
fn fx(i: u32) -> i32 {{ return {}; }}
@compute @workgroup_size(256) fn main(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nw: vec3<u32>) {{
  if (shape.b == 4294967295u) {{ o0[0] = 0u{touch}; }}
  let g = wid.x + wid.y * nw.x;
  let base = g * 2048u + lid.x;
  var acc: i32 = {id};
  for (var k = 0u; k < 8u; k = k + 1u) {{ let j = base + k * 256u; if (j < shape.a) {{ acc = op(acc, fx(j)); }} }}
  sh[lid.x] = acc;
  workgroupBarrier();
  for (var s = 128u; s > 0u; s = s >> 1u) {{ if (lid.x < s) {{ sh[lid.x] = op(sh[lid.x], sh[lid.x + s]); }} workgroupBarrier(); }}
  if (lid.x == 0u) {{ o0[g] = bitcast<u32>(sh[0]); }}
}}",
            e
        ));
        let groups = len.div_ceil(2048).max(1);
        let out = self.new_buf(groups);
        let mut bufs: Vec<&wgpu::Buffer> = leaves.iter().map(|h| &gv(h).buf).collect();
        bufs.push(&out);
        self.dispatch(&src, &bufs, groups * WG as usize, len as u32, 0);
        let fin = if groups == 1 { out } else { self.reduce_buf(op, &out, groups) };
        self.read_range(&fin, 0, 1)[0]
    }

    fn scan(&self, op: char, h: &Rc<dyn Handle>) -> Rc<dyn Handle> {
        let g = gv(h);
        let out = self.copy(&g.buf, g.len);
        if g.len > 0 {
            self.scan_buf(op, &out, g.len);
        }
        self.vec(out, g.len)
    }

    fn wher(&self, h: &Rc<dyn Handle>) -> Rc<dyn Handle> {
        let g = gv(h);
        if g.len == 0 {
            return self.vec(self.new_buf(0), 0);
        }
        // counts (negative counts count 0), inclusive scan, total = last; each i writes i at [ends[i]-count, ends[i])
        let counts = self.eval(&F::Bin('|', Box::new(F::Leaf(0)), Box::new(F::Lit(0))), std::slice::from_ref(h), g.len);
        let ends = self.scan('+', &counts);
        let total = self.download_one(&ends, g.len - 1).max(0) as usize;
        let out = self.new_buf(total);
        let src = format!(
            "{}@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
  let i = gid.x + gid.y * shape.stride;
  if (i >= shape.n) {{ return; }}
  let c = bitcast<i32>(l0[i]);
  let e = bitcast<i32>(l1[i]);
  for (var k = e - c; k < e; k = k + 1i) {{ o0[u32(k)] = i; }}
}}",
            Self::header(2, 1)
        );
        self.dispatch(&src, &[&gv(&counts).buf, &gv(&ends).buf, &out], g.len, 0, 0);
        self.vec(out, total)
    }

    fn grade(&self, h: &Rc<dyn Handle>, desc: bool) -> Rc<dyn Handle> {
        let g = gv(h);
        let n = g.len;
        if n == 0 {
            return self.vec(self.new_buf(0), 0);
        }
        let m = n.next_power_of_two();
        // keys (descending sorts the bitwise complement, which reverses i32 order) and indices, padded to a power
        // of two with keys that sort last
        let keys = self.new_buf(m);
        let idx = self.new_buf(m);
        let init = format!(
            "{}@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
  let i = gid.x + gid.y * shape.stride;
  if (i >= shape.n) {{ return; }}
  if (i < shape.a) {{ o0[i] = select(l0[i], ~l0[i], shape.b == 1u); }} else {{ o0[i] = 0x7fffffffu; }}
  o1[i] = i;
}}",
            Self::header(1, 2)
        );
        self.dispatch(&init, &[&g.buf, &keys, &idx], m, n as u32, desc as u32);
        // bitonic network over (key, index): compare-exchange pairs i, i^j
        let step = format!(
            "{}@group(0) @binding(0) var<storage, read_write> k: array<u32>;
@group(0) @binding(1) var<storage, read_write> ix: array<u32>;
@group(0) @binding(2) var<uniform> shape: Shape;
fn less(a: u32, b: u32) -> bool {{ let ka = bitcast<i32>(k[a]); let kb = bitcast<i32>(k[b]); return ka < kb || (ka == kb && ix[a] < ix[b]); }}
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
  let i = gid.x + gid.y * shape.stride;
  if (i >= shape.n) {{ return; }}
  let l = i ^ shape.b;
  if (l <= i) {{ return; }}
  let up = (i & shape.a) == 0u;
  let swap = select(less(i, l), less(l, i), up);
  if (swap) {{ let tk = k[i]; k[i] = k[l]; k[l] = tk; let ti = ix[i]; ix[i] = ix[l]; ix[l] = ti; }}
}}",
            HELPERS
        );
        let mut kk = 2;
        while kk <= m {
            let mut j = kk / 2;
            while j > 0 {
                self.dispatch(&step, &[&keys, &idx], m, kk as u32, j as u32);
                j /= 2;
            }
            kk *= 2;
        }
        // padded entries sorted last (their keys are the largest, their indices the largest among ties), so the
        // first n indices are the grade
        let out = self.copy(&idx, n);
        self.vec(out, n)
    }

    fn join(&self, a: &Rc<dyn Handle>, b: &Rc<dyn Handle>) -> Rc<dyn Handle> {
        let (ga, gb) = (gv(a), gv(b));
        let out = self.new_buf(ga.len + gb.len);
        self.record(|enc| {
            if ga.len > 0 {
                enc.copy_buffer_to_buffer(&ga.buf, 0, &out, 0, (ga.len * 4) as u64);
            }
            if gb.len > 0 {
                enc.copy_buffer_to_buffer(&gb.buf, 0, &out, (ga.len * 4) as u64, (gb.len * 4) as u64);
            }
        });
        self.vec(out, ga.len + gb.len)
    }
}
