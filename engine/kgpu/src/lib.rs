//! K on the GPU. A kernel is a few lines of a small K dialect; it runs once per element `i` of an output, every
//! intermediate fused into one WGSL compute shader (no temporaries in memory between verbs). The same text runs on
//! the CPU through an interpreter, and the two must agree bit for bit: everything is i32 with wrapping arithmetic,
//! truncating division, and fixed loop orders, so there is nothing for a GPU to reorder.
//!
//! The dialect (K's right-to-left evaluation, no precedence: `a*b+c` is `a*(b+c)`):
//!   name:expr          a binding (later lines see it); an output is a binding the caller names as one
//!   + - * %            add, subtract, multiply, divide (truncating); `-x` at the start of an expression negates
//!   !                  mod (truncating remainder)
//!   & |                min, max (K's)
//!   < > =              comparisons, 1 or 0
//!   i                  the element's index
//!   byte[b;k] word[b;k]          the k-th byte / u32 word of input buffer b
//!   pop[x] xor[a;b] or[a;b] and[a;b] shl[a;n] shr[a;n]   bits (shr is arithmetic)
//!   clamp[x;lo;hi] sel[c;a;b] pack[r;g;b;a]               clamp, choose, four bytes into a word
//!   sum[k;n;expr]      +/ over k = 0..n-1 (K's over, in a fixed order)
//! Lines starting with `/` are comments, as in K.

pub mod gpu;
pub mod k;

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum E {
    Num(i64),
    Name(String),
    Call(String, Vec<E>),
    Bin(char, Box<E>, Box<E>),
    Neg(Box<E>),
}

#[derive(Debug, Clone)]
pub struct Kernel {
    pub lines: Vec<Line>,
}

/// A kernel line: a binding, or a guarded block whose lines run only where its condition is non-zero (names bound
/// outside keep their values where it isn't).
#[derive(Debug, Clone)]
pub enum Line {
    Bind(String, E),
    When(E, Vec<Line>),
}

// === parsing ===

#[derive(Debug, Clone, PartialEq)]
enum T {
    Num(i64),
    Name(String),
    Verb(char),
    Open,
    Close,
    LBr,
    RBr,
    Semi,
    Colon,
}

fn lex(s: &str) -> Result<Vec<T>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() {
            let st = i;
            while i < c.len() && c[i].is_ascii_digit() {
                i += 1;
            }
            out.push(T::Num(c[st..i].iter().collect::<String>().parse().map_err(|e| format!("{e}"))?));
        } else if ch.is_alphabetic() || ch == '_' {
            let st = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(T::Name(c[st..i].iter().collect()));
        } else {
            out.push(match ch {
                '(' => T::Open,
                ')' => T::Close,
                '[' => T::LBr,
                ']' => T::RBr,
                ';' => T::Semi,
                ':' => T::Colon,
                '+' | '-' | '*' | '%' | '!' | '&' | '|' | '<' | '>' | '=' => T::Verb(ch),
                other => return Err(format!("unexpected '{other}'")),
            });
            i += 1;
        }
    }
    Ok(out)
}

struct P {
    t: Vec<T>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<&T> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<T> {
        self.i += 1;
        self.t.get(self.i - 1).cloned()
    }
    fn expect(&mut self, t: T) -> Result<(), String> {
        match self.next() {
            Some(x) if x == t => Ok(()),
            other => Err(format!("expected {t:?}, got {other:?}")),
        }
    }
    /// expr := '-' expr | noun (verb expr)?      (right to left: the verb takes everything to its right)
    fn expr(&mut self) -> Result<E, String> {
        if self.peek() == Some(&T::Verb('-')) {
            self.next();
            return Ok(E::Neg(Box::new(self.expr()?)));
        }
        let n = self.noun()?;
        if let Some(T::Verb(v)) = self.peek().cloned() {
            self.next();
            return Ok(E::Bin(v, Box::new(n), Box::new(self.expr()?)));
        }
        Ok(n)
    }
    fn noun(&mut self) -> Result<E, String> {
        match self.next() {
            Some(T::Num(n)) => Ok(E::Num(n)),
            Some(T::Open) => {
                let e = self.expr()?;
                self.expect(T::Close)?;
                Ok(e)
            }
            Some(T::Name(n)) => {
                if self.peek() == Some(&T::LBr) {
                    self.next();
                    let mut args = vec![self.expr()?];
                    while self.peek() == Some(&T::Semi) {
                        self.next();
                        args.push(self.expr()?);
                    }
                    self.expect(T::RBr)?;
                    Ok(E::Call(n, args))
                } else {
                    Ok(E::Name(n))
                }
            }
            other => Err(format!("expected a noun, got {other:?}")),
        }
    }
}

pub fn parse(src: &str) -> Result<Kernel, String> {
    // `when cond` opens a block, `end` closes it
    let mut stack: Vec<(Option<E>, Vec<Line>)> = vec![(None, Vec::new())];
    for raw in src.lines() {
        let l = raw.trim();
        if l.is_empty() || l.starts_with('/') {
            continue;
        }
        if l == "end" {
            let (cond, body) = stack.pop().ok_or("end without when")?;
            let cond = cond.ok_or("end without when")?;
            stack.last_mut().ok_or("end without when")?.1.push(Line::When(cond, body));
            continue;
        }
        if let Some(c) = l.strip_prefix("when ") {
            let mut p = P { t: lex(c)?, i: 0 };
            let e = p.expr().map_err(|e| format!("{l}: {e}"))?;
            stack.push((Some(e), Vec::new()));
            continue;
        }
        let mut p = P { t: lex(l)?, i: 0 };
        let name = match (p.t.first(), p.t.get(1)) {
            (Some(T::Name(n)), Some(T::Colon)) => {
                let n = n.clone();
                p.i = 2;
                n
            }
            _ => return Err(format!("every line binds a name (name:expr): {l}")),
        };
        let e = p.expr().map_err(|e| format!("{l}: {e}"))?;
        if p.i != p.t.len() {
            return Err(format!("{l}: trailing tokens"));
        }
        stack.last_mut().unwrap().1.push(Line::Bind(name, e));
    }
    if stack.len() != 1 {
        return Err("a when without its end".into());
    }
    Ok(Kernel { lines: stack.pop().unwrap().1 })
}

// === the CPU interpreter (the reference) ===

pub struct Cpu<'a> {
    pub buffers: HashMap<&'a str, &'a [u32]>,
    pub scalars: HashMap<&'a str, i32>,
}

impl Cpu<'_> {
    fn ev(&self, e: &E, env: &mut Vec<(String, i32)>) -> Result<i32, String> {
        Ok(match e {
            E::Num(n) => *n as i32,
            E::Name(n) => match env.iter().rev().find(|(k, _)| k == n) {
                Some((_, v)) => *v,
                None => *self.scalars.get(n.as_str()).ok_or(format!("unknown name {n}"))?,
            },
            E::Neg(a) => self.ev(a, env)?.wrapping_neg(),
            E::Bin(op, a, b) => {
                let (x, y) = (self.ev(a, env)?, self.ev(b, env)?);
                match op {
                    '+' => x.wrapping_add(y),
                    '-' => x.wrapping_sub(y),
                    '*' => x.wrapping_mul(y),
                    '%' => x.wrapping_div(y),
                    '!' => x.wrapping_rem(y),
                    '&' => x.min(y),
                    '|' => x.max(y),
                    '<' => (x < y) as i32,
                    '>' => (x > y) as i32,
                    _ => (x == y) as i32,
                }
            }
            E::Call(f, a) => {
                let buf = |k: usize| -> Result<&[u32], String> {
                    match &a[k] {
                        E::Name(n) => self.buffers.get(n.as_str()).copied().ok_or(format!("{f}: no buffer {n}")),
                        _ => Err(format!("{f}: the first argument is a buffer name")),
                    }
                };
                match f.as_str() {
                    // tex[T;x;y]: one RGBA texel as a packed word (the CPU holds T as packed words, T_w wide)
                    "tex" => {
                        let E::Name(t) = &a[0] else { return Err("tex[T;x;y]: T is a texture name".into()) };
                        let w = *self.scalars.get(format!("{t}_w").as_str()).ok_or(format!("tex: no scalar {t}_w"))?;
                        let (x, y) = (self.ev(&a[1], env)?, self.ev(&a[2], env)?);
                        buf(0)?[(y * w + x) as usize] as i32
                    }
                    "byte" => {
                        let k = self.ev(&a[1], env)? as u32;
                        ((buf(0)?[(k >> 2) as usize] >> (8 * (k & 3))) & 255) as i32
                    }
                    "word" => buf(0)?[self.ev(&a[1], env)? as usize] as i32,
                    "pop" => (self.ev(&a[0], env)? as u32).count_ones() as i32,
                    "xor" => self.ev(&a[0], env)? ^ self.ev(&a[1], env)?,
                    "or" => self.ev(&a[0], env)? | self.ev(&a[1], env)?,
                    "and" => self.ev(&a[0], env)? & self.ev(&a[1], env)?,
                    "shl" => self.ev(&a[0], env)?.wrapping_shl(self.ev(&a[1], env)? as u32),
                    "shr" => self.ev(&a[0], env)?.wrapping_shr(self.ev(&a[1], env)? as u32),
                    "clamp" => {
                        let (x, lo, hi) = (self.ev(&a[0], env)?, self.ev(&a[1], env)?, self.ev(&a[2], env)?);
                        x.max(lo).min(hi)
                    }
                    "sel" => {
                        if self.ev(&a[0], env)? != 0 {
                            self.ev(&a[1], env)?
                        } else {
                            self.ev(&a[2], env)?
                        }
                    }
                    "pack" => {
                        let b = |k: usize, env: &mut Vec<(String, i32)>| self.ev(&a[k], env).map(|v| (v as u32) & 255);
                        (b(0, env)? | (b(1, env)? << 8) | (b(2, env)? << 16) | (b(3, env)? << 24)) as i32
                    }
                    "sum" => {
                        let E::Name(k) = &a[0] else { return Err("sum[k;n;expr]: k is a name".into()) };
                        let n = self.ev(&a[1], env)?;
                        let mut s = 0i32;
                        for j in 0..n {
                            env.push((k.clone(), j));
                            s = s.wrapping_add(self.ev(&a[2], env)?);
                            env.pop();
                        }
                        s
                    }
                    other => return Err(format!("unknown function {other}")),
                }
            }
        })
    }

    fn lines(&self, lines: &[Line], env: &mut Vec<(String, i32)>) -> Result<(), String> {
        for l in lines {
            match l {
                Line::Bind(name, e) => {
                    let v = self.ev(e, env)?;
                    env.push((name.clone(), v));
                }
                Line::When(c, body) => {
                    if self.ev(c, env)? != 0 {
                        self.lines(body, env)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Every output for elements 0..n.
    pub fn run(&self, k: &Kernel, n: usize, outputs: &[&str]) -> Result<Vec<Vec<u32>>, String> {
        let mut out = vec![Vec::with_capacity(n); outputs.len()];
        let mut env = Vec::new();
        for i in 0..n {
            env.clear();
            env.push(("i".to_string(), i as i32));
            self.lines(&k.lines, &mut env)?;
            for (o, name) in outputs.iter().enumerate() {
                out[o].push(env.iter().rev().find(|(k, _)| k == name).ok_or(format!("no output {name}"))?.1 as u32);
            }
        }
        Ok(out)
    }
}

// === WGSL ===

struct Gen<'a> {
    buffers: &'a [&'a str],
    textures: &'a [&'a str],
    scalars: &'a HashMap<&'a str, i32>,
    pre: Vec<String>,
    fresh: usize,
    /// sum's loop variables in scope
    bound: Vec<String>,
    /// every name the kernel binds: one WGSL var each, assigned in order (rebinding is assignment)
    declared: std::collections::BTreeSet<String>,
}

impl Gen<'_> {
    fn name(&self, n: &str) -> Result<String, String> {
        if self.bound.iter().any(|b| b == n) {
            Ok(format!("kl_{n}"))
        } else if n == "i" || self.declared.contains(n) {
            Ok(format!("k_{n}"))
        } else if let Some(v) = self.scalars.get(n) {
            Ok(format!("({v})"))
        } else {
            Err(format!("unknown name {n}"))
        }
    }
    fn e(&mut self, e: &E) -> Result<String, String> {
        Ok(match e {
            E::Num(n) => format!("({})", *n as i32),
            E::Name(n) => self.name(n)?,
            E::Neg(a) => format!("(-{})", self.e(a)?),
            E::Bin(op, a, b) => {
                let (x, y) = (self.e(a)?, self.e(b)?);
                match op {
                    '+' | '-' | '*' => format!("({x} {op} {y})"),
                    // K's % divides (WGSL's % is the remainder, K's !)
                    '%' => format!("({x} / {y})"),
                    '!' => format!("({x} % {y})"),
                    '&' => format!("min({x}, {y})"),
                    '|' => format!("max({x}, {y})"),
                    '<' | '>' => format!("select(0, 1, {x} {op} {y})"),
                    _ => format!("select(0, 1, {x} == {y})"),
                }
            }
            E::Call(f, a) => {
                let buf = |k: usize| -> Result<String, String> {
                    match &a[k] {
                        E::Name(n) if self.buffers.contains(&n.as_str()) => Ok(format!("b_{n}")),
                        _ => Err(format!("{f}: the first argument is an input buffer")),
                    }
                };
                match f.as_str() {
                    "tex" => {
                        let t = match &a[0] {
                            E::Name(n) if self.textures.contains(&n.as_str()) => n.clone(),
                            _ => return Err("tex[T;x;y]: T is a texture input".into()),
                        };
                        format!("kpack(textureLoad(t_{t}, vec2<i32>({}, {}), 0))", self.e(&a[1])?, self.e(&a[2])?)
                    }
                    "byte" => {
                        let b = buf(0)?;
                        let k = self.e(&a[1])?;
                        format!("i32(({b}[u32({k}) >> 2u] >> (8u * (u32({k}) & 3u))) & 255u)")
                    }
                    "word" => {
                        let b = buf(0)?;
                        format!("bitcast<i32>({b}[u32({})])", self.e(&a[1])?)
                    }
                    "pop" => format!("i32(countOneBits(u32({})))", self.e(&a[0])?),
                    "xor" => format!("({} ^ {})", self.e(&a[0])?, self.e(&a[1])?),
                    "or" => format!("({} | {})", self.e(&a[0])?, self.e(&a[1])?),
                    "and" => format!("({} & {})", self.e(&a[0])?, self.e(&a[1])?),
                    "shl" => format!("({} << (u32({}) & 31u))", self.e(&a[0])?, self.e(&a[1])?),
                    "shr" => format!("({} >> (u32({}) & 31u))", self.e(&a[0])?, self.e(&a[1])?),
                    "clamp" => format!("clamp({}, {}, {})", self.e(&a[0])?, self.e(&a[1])?, self.e(&a[2])?),
                    "sel" => {
                        let (c, x, y) = (self.e(&a[0])?, self.e(&a[1])?, self.e(&a[2])?);
                        format!("select({y}, {x}, {c} != 0)")
                    }
                    "pack" => {
                        let v: Vec<String> = (0..4).map(|k| self.e(&a[k]).map(|s| format!("(u32({s}) & 255u)"))).collect::<Result<_, _>>()?;
                        format!("bitcast<i32>({} | ({} << 8u) | ({} << 16u) | ({} << 24u))", v[0], v[1], v[2], v[3])
                    }
                    "sum" => {
                        let E::Name(k) = &a[0] else { return Err("sum[k;n;expr]: k is a name".into()) };
                        let n = self.e(&a[1])?;
                        let id = self.fresh;
                        self.fresh += 1;
                        self.bound.push(k.clone());
                        // the body's own pre-statements belong inside the loop
                        let outer = std::mem::take(&mut self.pre);
                        let body = self.e(&a[2])?;
                        let inner = std::mem::replace(&mut self.pre, outer);
                        self.bound.pop();
                        self.pre.push(format!(
                            "var s_{id}: i32 = 0;\n  for (var kl_{k}: i32 = 0; kl_{k} < {n}; kl_{k} = kl_{k} + 1) {{ {} s_{id} = s_{id} + {body}; }}",
                            inner.join(" ")
                        ));
                        format!("s_{id}")
                    }
                    other => return Err(format!("unknown function {other}")),
                }
            }
        })
    }
}

/// The kernel as one WGSL compute shader: inputs are read-only u32 buffers, scalars are baked in as constants,
/// each output is a u32 buffer written at `i`.
pub fn wgsl(k: &Kernel, buffers: &[&str], scalars: &HashMap<&str, i32>, outputs: &[&str]) -> Result<String, String> {
    wgsl_tex(k, buffers, &[], scalars, outputs)
}

/// As `wgsl`, with texture inputs too (bound after the buffers, as texture_2d<f32> views whose texels are read back
/// as exact bytes: give them a Unorm view, not sRGB). Binding order: buffers, textures, outputs, the shape uniform.
pub fn wgsl_tex(k: &Kernel, buffers: &[&str], textures: &[&str], scalars: &HashMap<&str, i32>, outputs: &[&str]) -> Result<String, String> {
    fn names(lines: &[Line], out: &mut std::collections::BTreeSet<String>) {
        for l in lines {
            match l {
                Line::Bind(n, _) => {
                    out.insert(n.clone());
                }
                Line::When(_, b) => names(b, out),
            }
        }
    }
    fn emit(g: &mut Gen, lines: &[Line], body: &mut String, ind: usize) -> Result<(), String> {
        let pad = " ".repeat(ind);
        for l in lines {
            match l {
                Line::Bind(name, e) => {
                    let x = g.e(e)?;
                    for p in g.pre.drain(..) {
                        body.push_str(&format!("{pad}{p}\n"));
                    }
                    body.push_str(&format!("{pad}k_{name} = {x};\n"));
                }
                Line::When(c, b) => {
                    let x = g.e(c)?;
                    for p in g.pre.drain(..) {
                        body.push_str(&format!("{pad}{p}\n"));
                    }
                    body.push_str(&format!("{pad}if ({x} != 0) {{\n"));
                    emit(g, b, body, ind + 2)?;
                    body.push_str(&format!("{pad}}}\n"));
                }
            }
        }
        Ok(())
    }
    let mut declared = std::collections::BTreeSet::new();
    names(&k.lines, &mut declared);
    declared.remove("i");
    let mut g = Gen { buffers, textures, scalars, pre: Vec::new(), fresh: 0, bound: Vec::new(), declared: declared.clone() };
    let mut body = String::new();
    for n in &declared {
        body.push_str(&format!("  var k_{n}: i32 = 0;\n"));
    }
    emit(&mut g, &k.lines, &mut body, 2)?;
    let mut s = String::new();
    for (b, name) in buffers.iter().enumerate() {
        s.push_str(&format!("@group(0) @binding({b}) var<storage, read> b_{name}: array<u32>;\n"));
    }
    for (t, name) in textures.iter().enumerate() {
        s.push_str(&format!("@group(0) @binding({}) var t_{name}: texture_2d<f32>;\n", buffers.len() + t));
    }
    let base = buffers.len() + textures.len();
    for (o, name) in outputs.iter().enumerate() {
        s.push_str(&format!("@group(0) @binding({}) var<storage, read_write> o_{name}: array<u32>;\n", base + o));
    }
    s.push_str("fn kpack(c: vec4<f32>) -> i32 { let u = vec4<u32>(round(clamp(c, vec4<f32>(0.0), vec4<f32>(1.0)) * 255.0)); return bitcast<i32>(u.x | (u.y << 8u) | (u.z << 16u) | (u.w << 24u)); }\n");
    s.push_str("struct Shape { n: u32, stride: u32 }\n");
    s.push_str(&format!("@group(0) @binding({}) var<uniform> shape: Shape;\n", base + outputs.len()));
    s.push_str("@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>) {\n");
    s.push_str("  let ii = gid.x + gid.y * shape.stride;\n  if (ii >= shape.n) { return; }\n  let k_i: i32 = i32(ii);\n");
    s.push_str(&body);
    for name in outputs {
        s.push_str(&format!("  o_{name}[ii] = bitcast<u32>(k_{name});\n"));
    }
    s.push_str("}\n");
    Ok(s)
}

// === the GPU ===

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: String,
}

pub struct Compiled {
    pub pipeline: wgpu::ComputePipeline,
    buffers: Vec<String>,
    outputs: Vec<String>,
    textures: usize,
    pub wgsl: String,
}

/// Seconds spent in each phase of a GPU run.
#[derive(Debug, Default, Clone)]
pub struct Times {
    pub upload: f64,
    pub compute: f64,
    pub readback: f64,
}

impl Times {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"upload_ms": self.upload * 1e3, "compute_ms": self.compute * 1e3, "readback_ms": self.readback * 1e3})
    }
}

impl Gpu {
    /// Whether this machine has a hardware GPU adapter (a software one, such as lavapipe, doesn't count: the CPU render
    /// is the CPU route; docs/cpu.feature C1/C3). Asked once per process.
    pub fn available() -> bool {
        static A: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *A.get_or_init(|| {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }))
                .is_ok_and(|a| a.get_info().device_type != wgpu::DeviceType::Cpu)
        })
    }

    pub fn new() -> Result<Gpu, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }))
            .map_err(|e| format!("no GPU adapter: {e}"))?;
        let info = adapter.get_info();
        // the adapter's own limits: always grantable (the defaults ask for 16 inter-stage variables, more than an A15's
        // 15), and they carry its big storage buffers
        let limits = adapter.limits();
        // timestamps, where the adapter has them (stage timing on the GPU; nothing else depends on them)
        let required_features = adapter.features() & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: limits, required_features, ..Default::default() })).map_err(|e| format!("device: {e}"))?;
        Ok(Gpu { device, queue, adapter: format!("{} ({:?})", info.name, info.backend) })
    }

    pub fn compile(&self, k: &Kernel, buffers: &[&str], scalars: &HashMap<&str, i32>, outputs: &[&str]) -> Result<Compiled, String> {
        self.compile_tex(k, buffers, &[], scalars, outputs)
    }

    pub fn compile_tex(&self, k: &Kernel, buffers: &[&str], textures: &[&str], scalars: &HashMap<&str, i32>, outputs: &[&str]) -> Result<Compiled, String> {
        let src = wgsl_tex(k, buffers, textures, scalars, outputs)?;
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("kgpu k"), source: wgpu::ShaderSource::Wgsl(src.clone().into()) });
        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("kgpu k"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Compiled { pipeline, buffers: buffers.iter().map(|s| s.to_string()).collect(), outputs: outputs.iter().map(|s| s.to_string()).collect(), textures: textures.len(), wgsl: src })
    }

    /// Run over elements 0..n; returns the outputs and how long each phase took.
    pub fn run(&self, c: &Compiled, inputs: &[&[u32]], n: usize) -> Result<(Vec<Vec<u32>>, Times), String> {
        self.run_tex(c, inputs, &[], n)
    }

    /// A storage buffer on this device (`bytes` rounded up to a word), optionally filled.
    pub fn buffer(&self, bytes: usize, data: Option<&[u8]>) -> wgpu::Buffer {
        let size = (bytes.max(4) as u64).div_ceil(4) * 4;
        let b = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        if let Some(d) = data {
            // straight from the caller's bytes when they fill whole words (NV12 frames do); padded otherwise
            if d.len() as u64 == size {
                self.queue.write_buffer(&b, 0, d);
            } else {
                let mut d = d.to_vec();
                d.resize(size as usize, 0);
                self.queue.write_buffer(&b, 0, &d);
            }
        }
        b
    }

    /// Run over resident buffers: nothing uploaded, nothing read back (the outputs stay on the GPU). Waits for it
    /// when `wait`, so the caller can time the GPU work.
    pub fn dispatch(&self, c: &Compiled, inputs: &[&wgpu::Buffer], textures: &[&wgpu::TextureView], outputs: &[&wgpu::Buffer], n: usize, wait: bool) {
        self.dispatch_ts(c, inputs, textures, outputs, n, wait, None)
    }

    /// As `dispatch`, with the pass's start and end written to queries `i` and `i + 1` of a timestamp query set.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_ts(&self, c: &Compiled, inputs: &[&wgpu::Buffer], textures: &[&wgpu::TextureView], outputs: &[&wgpu::Buffer], n: usize, wait: bool, ts: Option<(&wgpu::QuerySet, u32)>) {
        use wgpu::util::DeviceExt;
        let groups = n.div_ceil(256) as u32;
        let gx = groups.min(65535).max(1);
        let gy = groups.div_ceil(gx).max(1);
        let shape = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[n as u32, gx * 256]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();
        for (b, buf) in inputs.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: b as u32, resource: buf.as_entire_binding() });
        }
        for (t, view) in textures.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: (inputs.len() + t) as u32, resource: wgpu::BindingResource::TextureView(view) });
        }
        let base = inputs.len() + textures.len();
        for (o, buf) in outputs.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: (base + o) as u32, resource: buf.as_entire_binding() });
        }
        entries.push(wgpu::BindGroupEntry { binding: (base + outputs.len()) as u32, resource: shape.as_entire_binding() });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &c.pipeline.get_bind_group_layout(0), entries: &entries });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kgpu run") });
        {
            let timestamp_writes = ts.map(|(q, i)| wgpu::ComputePassTimestampWrites { query_set: q, beginning_of_pass_write_index: Some(i), end_of_pass_write_index: Some(i + 1) });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes });
            pass.set_pipeline(&c.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        self.queue.submit([enc.finish()]);
        raea_gputrace::submitted(&self.queue, "kgpu", "kgpu run");
        if wait {
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        }
    }

    /// The first `bytes` of a buffer, read back.
    pub fn read(&self, buf: &wgpu::Buffer, bytes: usize) -> Vec<u8> {
        let size = (bytes as u64).div_ceil(4) * 4;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kgpu run") });
        enc.copy_buffer_to_buffer(buf, 0, &staging, 0, size);
        self.queue.submit([enc.finish()]);
        raea_gputrace::submitted(&self.queue, "kgpu", "kgpu run");
        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let v = staging.slice(..).get_mapped_range()[..bytes].to_vec();
        v
    }

    /// As `run`, with texture views bound after the buffers.
    pub fn run_tex(&self, c: &Compiled, inputs: &[&[u32]], textures: &[&wgpu::TextureView], n: usize) -> Result<(Vec<Vec<u32>>, Times), String> {
        assert_eq!(textures.len(), c.textures);
        use wgpu::util::DeviceExt;
        let mut t = Times::default();
        let t0 = std::time::Instant::now();
        let ins: Vec<wgpu::Buffer> = inputs
            .iter()
            .map(|d| {
                let data: &[u32] = if d.is_empty() { &[0] } else { d };
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(data), usage: wgpu::BufferUsages::STORAGE })
            })
            .collect();
        let size = (n.max(1) * 4) as u64;
        let outs: Vec<wgpu::Buffer> = c
            .outputs
            .iter()
            .map(|_| self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false }))
            .collect();
        let groups = n.div_ceil(256) as u32;
        let gx = groups.min(65535);
        let gy = groups.div_ceil(gx);
        let shape = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[n as u32, gx * 256]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let layout = c.pipeline.get_bind_group_layout(0);
        let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();
        for (b, buf) in ins.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: b as u32, resource: buf.as_entire_binding() });
        }
        for (t, view) in textures.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: (ins.len() + t) as u32, resource: wgpu::BindingResource::TextureView(view) });
        }
        let base = ins.len() + textures.len();
        for (o, buf) in outs.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: (base + o) as u32, resource: buf.as_entire_binding() });
        }
        entries.push(wgpu::BindGroupEntry { binding: (base + outs.len()) as u32, resource: shape.as_entire_binding() });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &entries });
        self.queue.submit([]);
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("poll: {e}"))?;
        t.upload = t0.elapsed().as_secs_f64();

        let t1 = std::time::Instant::now();
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kgpu run") });
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&c.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        self.queue.submit([enc.finish()]);
        raea_gputrace::submitted(&self.queue, "kgpu", "kgpu run");
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("poll: {e}"))?;
        t.compute = t1.elapsed().as_secs_f64();

        let t2 = std::time::Instant::now();
        let staging: Vec<wgpu::Buffer> = outs
            .iter()
            .map(|_| self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }))
            .collect();
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kgpu run") });
        for (o, s) in outs.iter().zip(&staging) {
            enc.copy_buffer_to_buffer(o, 0, s, 0, size);
        }
        self.queue.submit([enc.finish()]);
        raea_gputrace::submitted(&self.queue, "kgpu", "kgpu run");
        for s in &staging {
            s.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        }
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("poll: {e}"))?;
        let mut result = Vec::new();
        for s in &staging {
            let v: Vec<u32> = bytemuck::cast_slice(&s.slice(..).get_mapped_range()).to_vec();
            result.push(v[..n].to_vec());
        }
        t.readback = t2.elapsed().as_secs_f64();
        let _ = &c.buffers;
        Ok((result, t))
    }
}

/// The NV12 conversion as plain Rust, the same fixed-point formula as kernels/nv12.k: the native reference.
/// `rgba` is (w x (h+1)) packed RGBA words, the last row carrying the frame tag at x = 8.
pub fn nv12_reference(rgba: &[u32], w: usize, h: usize) -> Vec<u32> {
    let ch = |c: u32, s: u32| ((c >> s) & 255) as i32;
    let luma = |c: u32| (16 + ((47 * ch(c, 0) + 157 * ch(c, 8) + 16 * ch(c, 16) + 128) >> 8)).clamp(0, 255) as u32;
    let mut out = Vec::with_capacity(w * h / 4 + w * h / 8 + 1);
    for k in 0..w * h / 4 {
        let p = 4 * k;
        let (y, x) = (p / w, p % w);
        let px = |d: usize| rgba[y * w + x + d];
        out.push(luma(px(0)) | luma(px(1)) << 8 | luma(px(2)) << 16 | luma(px(3)) << 24);
    }
    let cw = w / 2;
    for j in 0..w * h / 8 {
        let mut word = 0u32;
        for k in 0..2 {
            let s = 2 * j + k;
            let (cr, cc) = (s / cw, s % cw);
            let t = [rgba[2 * cr * w + 2 * cc], rgba[2 * cr * w + 2 * cc + 1], rgba[(2 * cr + 1) * w + 2 * cc], rgba[(2 * cr + 1) * w + 2 * cc + 1]];
            let sum = |sh: u32| t.iter().map(|c| ch(*c, sh)).sum::<i32>();
            let (r, g, b) = (sum(0), sum(8), sum(16));
            let u = (128 + ((-26 * r - 87 * g + 112 * b + 512) >> 10)).clamp(0, 255) as u32;
            let v = (128 + ((112 * r - 102 * g - 10 * b + 512) >> 10)).clamp(0, 255) as u32;
            word |= (u | v << 8) << (16 * k);
        }
        out.push(word);
    }
    out.push(rgba[h * w + 8]);
    out
}

/// nv12_reference on `threads` threads (the CPU render, docs/cpu.feature C1): the same formula per output word, the
/// words split into bands; bit-equal.
pub fn nv12_cpu(rgba: &[u32], w: usize, h: usize, threads: usize) -> Vec<u32> {
    let ch = |c: u32, s: u32| ((c >> s) & 255) as i32;
    let luma = |c: u32| (16 + ((47 * ch(c, 0) + 157 * ch(c, 8) + 16 * ch(c, 16) + 128) >> 8)).clamp(0, 255) as u32;
    let (ny, nuv) = (w * h / 4, w * h / 8);
    let mut out = vec![0u32; ny + nuv + 1];
    let cw = w / 2;
    let word = |j: usize| -> u32 {
        if j < ny {
            let p = 4 * j;
            let (y, x) = (p / w, p % w);
            let px = |d: usize| rgba[y * w + x + d];
            return luma(px(0)) | luma(px(1)) << 8 | luma(px(2)) << 16 | luma(px(3)) << 24;
        }
        let j = j - ny;
        let mut wd = 0u32;
        for k in 0..2 {
            let s = 2 * j + k;
            let (cr, cc) = (s / cw, s % cw);
            let t = [rgba[2 * cr * w + 2 * cc], rgba[2 * cr * w + 2 * cc + 1], rgba[(2 * cr + 1) * w + 2 * cc], rgba[(2 * cr + 1) * w + 2 * cc + 1]];
            let sum = |sh: u32| t.iter().map(|c| ch(*c, sh)).sum::<i32>();
            let (r, g, b) = (sum(0), sum(8), sum(16));
            let u = (128 + ((-26 * r - 87 * g + 112 * b + 512) >> 10)).clamp(0, 255) as u32;
            let v = (128 + ((112 * r - 102 * g - 10 * b + 512) >> 10)).clamp(0, 255) as u32;
            wd |= (u | v << 8) << (16 * k);
        }
        wd
    };
    let n = ny + nuv;
    let per = n.div_ceil(threads.max(1));
    std::thread::scope(|sc| {
        for (b, chunk) in out[..n].chunks_mut(per).enumerate() {
            let word = &word;
            sc.spawn(move || {
                for (i, o) in chunk.iter_mut().enumerate() {
                    *o = word(b * per + i);
                }
            });
        }
    });
    out[n] = rgba[h * w + 8];
    out
}

/// kernels/p010buf.k as plain Rust, on `threads` threads: RGBA ((w x (h+1)) packed words, the last row carrying the
/// frame tag at x = 8) to P010 words (two 16-bit luma samples a word, then one Cb Cr pair a word; 10-bit BT.709
/// limited range in each sample's top 10 bits), the tag last. Bit-equal to the kernel.
pub fn p010_cpu(rgba: &[u32], w: usize, h: usize, threads: usize) -> Vec<u32> {
    let ch = |c: u32, s: u32| ((c >> s) & 255) as i32;
    let luma = |c: u32| (64 + ((47864 * ch(c, 0) + 161017 * ch(c, 8) + 16255 * ch(c, 16) + 32768) >> 16)) as u32;
    let (ny, nuv) = (w * h / 2, w * h / 4);
    let mut out = vec![0u32; ny + nuv + 1];
    let cw = w / 2;
    let word = |j: usize| -> u32 {
        if j < ny {
            let p = 2 * j;
            return luma(rgba[p]) << 6 | luma(rgba[p + 1]) << 22;
        }
        let j = j - ny;
        let (cr, cc) = (j / cw, j % cw);
        let t = [rgba[2 * cr * w + 2 * cc], rgba[2 * cr * w + 2 * cc + 1], rgba[(2 * cr + 1) * w + 2 * cc], rgba[(2 * cr + 1) * w + 2 * cc + 1]];
        let sum = |sh: u32| t.iter().map(|c| ch(*c, sh)).sum::<i32>();
        let (r, g, b) = (sum(0), sum(8), sum(16));
        let u = (512 + ((-26383 * r - 88755 * g + 115138 * b + 131072) >> 18)) as u32;
        let v = (512 + ((115138 * r - 104581 * g - 10557 * b + 131072) >> 18)) as u32;
        u << 6 | v << 22
    };
    let n = ny + nuv;
    let per = n.div_ceil(threads.max(1));
    std::thread::scope(|sc| {
        for (b, chunk) in out[..n].chunks_mut(per).enumerate() {
            let word = &word;
            sc.spawn(move || {
                for (i, o) in chunk.iter_mut().enumerate() {
                    *o = word(b * per + i);
                }
            });
        }
    });
    out[n] = rgba[h * w + 8];
    out
}

/// One clip slot of the composite (the 12 words of kernels/composite.k's C buffer).
#[derive(Clone, Copy, Debug, Default)]
pub struct Slot {
    pub on: i32,
    pub bx: i32,
    pub by: i32,
    pub ex: i32,
    pub ey: i32,
    pub sw: i32,
    pub sh: i32,
    pub cx: i32,
    pub cy: i32,
    pub dx: i32,
    pub dy: i32,
    pub al: i32,
}

impl Slot {
    pub fn words(&self) -> [i32; 12] {
        [self.on, self.bx, self.by, self.ex, self.ey, self.sw, self.sh, self.cx, self.cy, self.dx, self.dy, self.al]
    }
    /// A slot that draws nothing (and reads inside a 4x4 dummy plane: give it at least 24 bytes).
    pub fn off() -> Slot {
        Slot { sw: 4, sh: 4, ..Default::default() }
    }
}

/// kernels/composite.k as plain Rust (the native reference): background words (w x h, from the target), up to four
/// NV12 planes with their slots, the premultiplied overlay; RGBA words out.
pub fn composite_reference(bg: &[u32], w: usize, h: usize, planes: &[&[u8]], slots: &[Slot], overlay: &[u32]) -> Vec<u32> {
    composite_reference_masked(bg, w, h, planes, slots, overlay, None)
}

/// A masked slot's opacity (on = 2): byte s of the pixel's mask word is its coverage (none: 0), and opacity scales by
/// it as composite.k does, ((al * cov) + 127) / 255; an unmasked slot keeps its opacity exactly.
#[inline]
fn masked_al(sl: &Slot, s: usize, mask: Option<&[u32]>, i: usize) -> i32 {
    if sl.on > 1 {
        let cov = mask.and_then(|m| m.get(i)).map(|m| ((m >> (8 * s)) & 255) as i32).unwrap_or(0);
        (sl.al * cov + 127) / 255
    } else {
        sl.al
    }
}

/// composite_reference with M, the per-pixel mask words of masked slots (kernels/composite.k's M).
pub fn composite_reference_masked(bg: &[u32], w: usize, h: usize, planes: &[&[u8]], slots: &[Slot], overlay: &[u32], mask: Option<&[u32]>) -> Vec<u32> {
    // WGSL's clamp, which the K interpreter also uses: min(max(e, lo), hi) (no panic when lo > hi)
    let cl = |e: i32, lo: i32, hi: i32| e.max(lo).min(hi);
    let byte = |p: &[u8], k: i32| -> i32 { if k >= 0 && (k as usize) < p.len() { p[k as usize] as i32 } else { 0 } };
    (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            let c = bg[y as usize * w + x as usize];
            let (mut r, mut g, mut b) = ((c & 255) as i32, ((c >> 8) & 255) as i32, ((c >> 16) & 255) as i32);
            for (s, sl) in slots.iter().enumerate() {
                let p = planes[s];
                let inside = sl.on != 0 && x >= sl.bx && x < sl.ex && y >= sl.by && y < sl.ey;
                let fx = sl.cx.wrapping_add((x - sl.bx).wrapping_mul(sl.dx)).wrapping_add(sl.dx >> 1).wrapping_sub(32768);
                let fy = sl.cy.wrapping_add((y - sl.by).wrapping_mul(sl.dy)).wrapping_add(sl.dy >> 1).wrapping_sub(32768);
                let ax = cl(fx >> 16, 0, sl.sw - 2);
                let ay = cl(fy >> 16, 0, sl.sh - 2);
                let wx = (((fx >> 8) & 255) * (fx > -1) as i32).clamp(0, 255);
                let wy = (((fy >> 8) & 255) * (fy > -1) as i32).clamp(0, 255);
                let (y00, y10) = (byte(p, ay * sl.sw + ax), byte(p, ay * sl.sw + ax + 1));
                let (y01, y11) = (byte(p, (ay + 1) * sl.sw + ax), byte(p, (ay + 1) * sl.sw + ax + 1));
                let ly = (y00 * (256 - wx) * (256 - wy) + y10 * wx * (256 - wy) + y01 * (256 - wx) * wy + y11 * wx * wy + 32768) >> 16;
                let gx = (fx.wrapping_add(32768) >> 1) - 32768;
                let gy = (fy.wrapping_add(32768) >> 1) - 32768;
                let qx = cl(gx >> 16, 0, sl.sw / 2 - 2);
                let qy = cl(gy >> 16, 0, sl.sh / 2 - 2);
                let vx = (((gx >> 8) & 255) * (gx > -1) as i32).clamp(0, 255);
                let vy = (((gy >> 8) & 255) * (gy > -1) as i32).clamp(0, 255);
                let ci = sl.sw * sl.sh + qy * sl.sw + 2 * qx;
                let bil = |o: i32| {
                    let (a, b, c, d) = (byte(p, ci + o), byte(p, ci + o + 2), byte(p, ci + sl.sw + o), byte(p, ci + sl.sw + o + 2));
                    ((a * (256 - vx) * (256 - vy) + b * vx * (256 - vy) + c * (256 - vx) * vy + d * vx * vy + 32768) >> 16) - 128
                };
                let (cu, cv) = (bil(0), bil(1));
                let k = 298 * (ly - 16);
                let rr = cl((k + 459 * cv + 128) >> 8, 0, 255);
                let gg = cl((k - 55 * cu - 136 * cv + 128) >> 8, 0, 255);
                let bb = cl((k + 541 * cu + 128) >> 8, 0, 255);
                if inside {
                    let al = masked_al(sl, s, mask, i);
                    r = (al * rr + (256 - al) * r) >> 8;
                    g = (al * gg + (256 - al) * g) >> 8;
                    b = (al * bb + (256 - al) * b) >> 8;
                }
            }
            let o = overlay[i];
            let oa = ((o >> 24) & 255) as i32;
            r = (o & 255) as i32 + (r * (255 - oa) + 127) / 255;
            g = ((o >> 8) & 255) as i32 + (g * (255 - oa) + 127) / 255;
            b = ((o >> 16) & 255) as i32 + (b * (255 - oa) + 127) / 255;
            (r as u32 & 255) | (g as u32 & 255) << 8 | (b as u32 & 255) << 16 | 255 << 24
        })
        .collect()
}

/// kernels/composite.k for the CPU render (docs/cpu.feature C2): the same integer arithmetic as composite_reference,
/// term for term, so the bytes are equal; fast because each slot only touches its own box, the source coordinates are
/// worked out once per column and once per row, an empty overlay pixel is skipped, and rows are split over `threads`.
pub fn composite_cpu(bg: &[u32], w: usize, h: usize, planes: &[&[u8]], slots: &[Slot], overlay: &[u32], threads: usize) -> Vec<u32> {
    composite_cpu_masked(bg, w, h, planes, slots, overlay, threads, None)
}

/// composite_cpu with M, the per-pixel mask words of masked slots (kernels/composite.k's M).
#[allow(clippy::too_many_arguments)]
pub fn composite_cpu_masked(bg: &[u32], w: usize, h: usize, planes: &[&[u8]], slots: &[Slot], overlay: &[u32], threads: usize, mask: Option<&[u32]>) -> Vec<u32> {
    let cl = |e: i32, lo: i32, hi: i32| e.max(lo).min(hi);
    let mut out = bg[..w * h].to_vec();
    let threads = threads.max(1).min(h.max(1));
    let rows_per = h.div_ceil(threads);
    // per slot, per column of its box: (luma column ax, its weight wx, chroma column qx, its weight vx)
    let cols: Vec<Vec<(i32, i32, i32, i32)>> = slots
        .iter()
        .map(|sl| {
            if sl.on == 0 {
                return Vec::new();
            }
            (sl.bx.max(0)..sl.ex.min(w as i32))
                .map(|x| {
                    let fx = sl.cx.wrapping_add((x - sl.bx).wrapping_mul(sl.dx)).wrapping_add(sl.dx >> 1).wrapping_sub(32768);
                    let ax = cl(fx >> 16, 0, sl.sw - 2);
                    let wx = (((fx >> 8) & 255) * (fx > -1) as i32).clamp(0, 255);
                    let gx = (fx.wrapping_add(32768) >> 1) - 32768;
                    let qx = cl(gx >> 16, 0, sl.sw / 2 - 2);
                    let vx = (((gx >> 8) & 255) * (gx > -1) as i32).clamp(0, 255);
                    (ax, wx, qx, vx)
                })
                .collect()
        })
        .collect();
    std::thread::scope(|sc| {
        for (band, chunk) in out.chunks_mut(rows_per * w).enumerate() {
            let cols = &cols;
            sc.spawn(move || {
                let y0 = band * rows_per;
                let byte = |p: &[u8], k: i32| -> i32 { if k >= 0 && (k as usize) < p.len() { p[k as usize] as i32 } else { 0 } };
                for (s, sl) in slots.iter().enumerate() {
                    if sl.on == 0 {
                        continue;
                    }
                    let p = planes[s];
                    let x0 = sl.bx.max(0);
                    for y in (y0 as i32).max(sl.by)..((y0 + chunk.len() / w) as i32).min(sl.ey) {
                        let fy = sl.cy.wrapping_add((y - sl.by).wrapping_mul(sl.dy)).wrapping_add(sl.dy >> 1).wrapping_sub(32768);
                        let ay = cl(fy >> 16, 0, sl.sh - 2);
                        let wy = (((fy >> 8) & 255) * (fy > -1) as i32).clamp(0, 255);
                        let gy = (fy.wrapping_add(32768) >> 1) - 32768;
                        let qy = cl(gy >> 16, 0, sl.sh / 2 - 2);
                        let vy = (((gy >> 8) & 255) * (gy > -1) as i32).clamp(0, 255);
                        let row = &mut chunk[(y as usize - y0) * w..(y as usize - y0 + 1) * w];
                        for (k, &(ax, wx, qx, vx)) in cols[s].iter().enumerate() {
                            let x = (x0 + k as i32) as usize;
                            let (y00, y10) = (byte(p, ay * sl.sw + ax), byte(p, ay * sl.sw + ax + 1));
                            let (y01, y11) = (byte(p, (ay + 1) * sl.sw + ax), byte(p, (ay + 1) * sl.sw + ax + 1));
                            let ly = (y00 * (256 - wx) * (256 - wy) + y10 * wx * (256 - wy) + y01 * (256 - wx) * wy + y11 * wx * wy + 32768) >> 16;
                            let ci = sl.sw * sl.sh + qy * sl.sw + 2 * qx;
                            let bil = |o: i32| {
                                let (a, b, c, d) = (byte(p, ci + o), byte(p, ci + o + 2), byte(p, ci + sl.sw + o), byte(p, ci + sl.sw + o + 2));
                                ((a * (256 - vx) * (256 - vy) + b * vx * (256 - vy) + c * (256 - vx) * vy + d * vx * vy + 32768) >> 16) - 128
                            };
                            let (cu, cv) = (bil(0), bil(1));
                            let kk = 298 * (ly - 16);
                            let rr = cl((kk + 459 * cv + 128) >> 8, 0, 255);
                            let gg = cl((kk - 55 * cu - 136 * cv + 128) >> 8, 0, 255);
                            let bb = cl((kk + 541 * cu + 128) >> 8, 0, 255);
                            let c = row[x];
                            let (r, g, b) = ((c & 255) as i32, ((c >> 8) & 255) as i32, ((c >> 16) & 255) as i32);
                            let al = masked_al(sl, s, mask, y as usize * w + x);
                            let r = (al * rr + (256 - al) * r) >> 8;
                            let g = (al * gg + (256 - al) * g) >> 8;
                            let b = (al * bb + (256 - al) * b) >> 8;
                            row[x] = (r as u32 & 255) | (g as u32 & 255) << 8 | (b as u32 & 255) << 16 | (c & 0xff00_0000);
                        }
                    }
                }
                // the overlay (premultiplied); an empty pixel leaves the colour as it is, and the result is opaque
                let base = y0 * w;
                for (j, px) in chunk.iter_mut().enumerate() {
                    let o = overlay[base + j];
                    let (mut r, mut g, mut b) = ((*px & 255) as i32, ((*px >> 8) & 255) as i32, ((*px >> 16) & 255) as i32);
                    if o != 0 {
                        let oa = ((o >> 24) & 255) as i32;
                        r = (o & 255) as i32 + (r * (255 - oa) + 127) / 255;
                        g = ((o >> 8) & 255) as i32 + (g * (255 - oa) + 127) / 255;
                        b = ((o >> 16) & 255) as i32 + (b * (255 - oa) + 127) / 255;
                    }
                    *px = (r as u32 & 255) | (g as u32 & 255) << 8 | (b as u32 & 255) << 16 | 255 << 24;
                }
            });
        }
    });
    out
}

/// kernels/look_grid.k as plain Rust: per frame, 9 x 16 cells x (r+g+b, r-g, (r+g)/2-b) summed over the cell.
pub fn look_grid_reference(s: &[u32], pw: usize, ph: usize, frames: usize) -> Vec<i32> {
    let (cw, ch) = (pw / 9, ph / 16);
    let mut out = Vec::with_capacity(frames * 432);
    for f in 0..frames {
        for c in 0..144 {
            let (cx, cy) = (c % 9, c / 9);
            let base = f * pw * ph + cy * ch * pw + cx * cw;
            let mut acc = [0i32; 3];
            for j in 0..cw * ch {
                let p = s[base + (j / cw) * pw + j % cw];
                let (r, g, b) = ((p & 255) as i32, ((p >> 8) & 255) as i32, ((p >> 16) & 255) as i32);
                acc[0] = acc[0].wrapping_add(r + g + b);
                acc[1] = acc[1].wrapping_add(r - g);
                acc[2] = acc[2].wrapping_add(((r + g) >> 1) - b);
            }
            out.extend(acc);
        }
    }
    out
}

/// kernels/look_proj.k as plain Rust: 64 words per frame, bit b the sign of the hash-weighted feature sum.
pub fn look_proj_reference(g: &[i32], frames: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(frames * 64);
    for f in 0..frames {
        // luma centred on the frame's mean cell (look_proj.k's m)
        let m = (0..144).map(|c| g[f * 432 + 3 * c]).fold(0i32, |a, x| a.wrapping_add(x)) / 144;
        for wd in 0..64i32 {
            let mut word = 0i32;
            for b in 0..32i32 {
                let mut s = 0i32;
                for k in 0..432i32 {
                    let h = (((wd * 32 + b).wrapping_mul(73856093)) ^ k.wrapping_mul(19349663)).wrapping_add(40503).wrapping_mul(-1640531535);
                    let sign = 2 * ((h >> 15) & 1) - 1;
                    let x = g[f * 432 + k as usize].wrapping_sub(if k % 3 == 0 { m } else { 0 });
                    s = s.wrapping_add(x.wrapping_mul(sign));
                }
                word = word.wrapping_add(((s > 0) as i32).wrapping_shl(b as u32));
            }
            out.push(word as u32);
        }
    }
    out
}

// === CV columns (tabular Track C): native references of kernels/cv_*.k, the same integer functions ===

/// cv_span.k: the mean of each span's rows (spans[s]..spans[s+1]), per dimension; an empty span is 0.
pub fn cv_span_reference(x: &[i32], d: usize, spans: &[i32]) -> Vec<i32> {
    let mut out = Vec::new();
    for s in 0..spans.len() - 1 {
        let (a, b) = (spans[s], spans[s + 1]);
        for j in 0..d {
            out.push(if b > a { (a..b).fold(0i32, |acc, t| acc.wrapping_add(x[t as usize * d + j])) / (b - a) } else { 0 });
        }
    }
    out
}

/// cv_delta.k: each row minus the row before (row 0 zeros).
pub fn cv_delta_reference(x: &[i32], d: usize, n: usize) -> Vec<i32> {
    (0..n * d).map(|i| { let (r, j) = (i / d, i % d); if r > 0 { x[r * d + j].wrapping_sub(x[(r - 1) * d + j]) } else { 0 } }).collect()
}

/// cv_norm.k: each delta row's squared norm, on deltas shifted down 7 bits.
pub fn cv_norm_reference(dl: &[i32], d: usize, n: usize) -> Vec<i32> {
    (0..n).map(|r| (0..d).fold(0i32, |a, j| { let v = dl[r * d + j] >> 7; a.wrapping_add(v.wrapping_mul(v)) })).collect()
}

/// cv_hdc.k: each row signed onto 8192 seeded integer-hash hyperplanes, 256 words a row.
pub fn cv_hdc_reference(x: &[i32], d: usize, n: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(n * 256);
    for r in 0..n {
        for wd in 0..256i32 {
            let mut word = 0i32;
            for b in 0..32i32 {
                let mut s = 0i32;
                for k in 0..d as i32 {
                    let h = (((wd * 32 + b).wrapping_mul(83492791)) ^ k.wrapping_mul(1103515245)).wrapping_add(7919).wrapping_mul(-1640531535);
                    let sign = 2 * ((h >> 15) & 1) - 1;
                    s = s.wrapping_add(x[r * d + k as usize].wrapping_mul(sign));
                }
                word = word.wrapping_add(((s > 0) as i32).wrapping_shl(b as u32));
            }
            out.push(word as u32);
        }
    }
    out
}

/// cv_win.k: 16-row windows every 8 rows of 256-word hypervectors, each row's words rotated by its place, bundled by
/// majority (more than 8 of 16).
pub fn cv_win_reference(hv: &[u32], windows: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(windows * 256);
    for w in 0..windows {
        for wd in 0..256usize {
            let mut word = 0u32;
            for b in 0..32 {
                let ones = (0..16).filter(|&r| hv[(w * 8 + r) * 256 + (wd + 256 - r) % 256] >> b & 1 == 1).count();
                if ones > 8 {
                    word |= 1 << b;
                }
            }
            out.push(word);
        }
    }
    out
}

/// The CV projection: d x p, N(0, 1/p), from `seed` (splitmix64 + Box-Muller), row-major (row k = input dimension k).
/// The one generator every user of the projection shares.
pub fn cv_projection_matrix(seed: u64, d: usize, p: usize) -> Vec<f32> {
    let mut s = seed;
    let mut next = || {
        s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let sd = (1.0 / p as f64).sqrt();
    (0..d * p).map(|_| {
        let u1 = ((next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64;
        let u2 = ((next() >> 11) as f64) / (1u64 << 53) as f64;
        ((-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos() * sd) as f32
    }).collect()
}

/// A float column quantized for the CV kernels (round to nearest at 2^shift).
pub fn cv_quantize(x: &[f32], shift: u32) -> Vec<i32> {
    let s = (1u64 << shift) as f64;
    x.iter().map(|v| (*v as f64 * s).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32).collect()
}

/// cv_proj.k: X (2^15) . P (2^13), n x p at 2^28.
pub fn cv_proj_reference(x: &[i32], pm: &[i32], d: usize, p: usize, n: usize) -> Vec<i32> {
    (0..n * p).map(|i| { let (r, j) = (i / p, i % p); (0..d).fold(0i32, |a, k| a.wrapping_add(x[r * d + k].wrapping_mul(pm[k * p + j]))) }).collect()
}

/// cv_gram.k: X (2^15) . T^T (2^15), n x m at 2^30.
pub fn cv_gram_reference(x: &[i32], t: &[i32], d: usize, m: usize, n: usize) -> Vec<i32> {
    (0..n * m).map(|i| { let (r, l) = (i / m, i % m); (0..d).fold(0i32, |a, k| a.wrapping_add(x[r * d + k].wrapping_mul(t[l * d + k]))) }).collect()
}

// === Energy scans (engine/rdb/docs/energy.feature): native references of kernels/energy_*.k ===

fn at(c: &[i32], i: i64) -> i32 {
    c[i.clamp(0, c.len() as i64 - 1) as usize]
}

/// energy_smooth.k: windowed mean over 2r+1 rows, edges clamped.
pub fn energy_smooth_reference(c: &[i32], r: i64) -> Vec<i32> {
    (0..c.len() as i64).map(|i| (0..2 * r + 1).fold(0i32, |s, j| s.wrapping_add(at(c, i - r + j))) / (2 * r + 1) as i32).collect()
}

/// energy_nodes.k: 1 peak, 2 trough, 3 inflection, 0 slope (first two rows and the last 0).
pub fn energy_nodes_reference(s: &[i32]) -> Vec<i32> {
    let n = s.len() as i64;
    (0..n).map(|i| {
        let (p, a, b, c) = (at(s, i - 2), at(s, i - 1), at(s, i), at(s, i + 1));
        if i < 2 || i > n - 2 {
            return 0;
        }
        let (s1, s2) = ((b + p) - 2 * a, (c + a) - 2 * b);
        if b > a && c <= b { 1 } else if a > b && b <= c { 2 } else if (s1 > 0 && s2 < 0) || (s1 < 0 && s2 > 0) { 3 } else { 0 }
    }).collect()
}

/// energy_contrast.k: windowed variance (Q16) over windowed mean (Q8) + 1, on the column at Q8.
pub fn energy_contrast_reference(c: &[i32], r: i64) -> Vec<i32> {
    let w = (2 * r + 1) as i32;
    (0..c.len() as i64).map(|i| {
        let x = |j: i64| at(c, i - r + j) >> 8;
        let m = (0..w as i64).fold(0i32, |s, j| s.wrapping_add(x(j))) / w;
        let q = (0..w as i64).fold(0i32, |s, j| s.wrapping_add((x(j) - m).wrapping_mul(x(j) - m))) / w;
        q.wrapping_mul(256) / (m + 1)
    }).collect()
}

/// energy_flat.k: 1 where contrast is under t.
pub fn energy_flat_reference(k: &[i32], t: i32) -> Vec<i32> {
    k.iter().map(|v| (t > *v) as i32).collect()
}

/// energy_cov.k: per row (cov, va, vb) over 2r+1 rows at Q8.
pub fn energy_cov_reference(a: &[i32], b: &[i32], r: i64) -> Vec<i32> {
    let w = (2 * r + 1) as i32;
    let mut out = Vec::new();
    for row in 0..a.len() as i64 {
        let xa = |j: i64| at(a, row - r + j) >> 8;
        let xb = |j: i64| at(b, row - r + j) >> 8;
        let ma = (0..w as i64).fold(0i32, |s, j| s.wrapping_add(xa(j))) / w;
        let mb = (0..w as i64).fold(0i32, |s, j| s.wrapping_add(xb(j))) / w;
        let sum = |f: &dyn Fn(i64) -> i32| (0..w as i64).fold(0i32, |s, j| s.wrapping_add(f(j))) / w;
        out.push(sum(&|j| (xa(j) - ma).wrapping_mul(xb(j) - mb)));
        out.push(sum(&|j| (xa(j) - ma).wrapping_mul(xa(j) - ma)));
        out.push(sum(&|j| (xb(j) - mb).wrapping_mul(xb(j) - mb)));
    }
    out
}

#[cfg(test)]
mod p010_tests {
    use super::*;

    /// kernels/p010buf.k (through the interpreter, which the GPU matches bit for bit) equals p010_cpu.
    #[test]
    fn p010buf_kernel_equals_native() {
        let (w, h) = (64usize, 34usize);
        let mut s = 0x2545_f491_4f6c_dd1du64;
        let words: Vec<u32> = (0..w * (h + 1))
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                (s as u32) | 0xff00_0000
            })
            .collect();
        let k = parse(include_str!("../kernels/p010buf.k")).unwrap();
        let sc: HashMap<&str, i32> = [("w", w as i32), ("h", h as i32), ("T_w", w as i32)].into();
        let cpu = Cpu { buffers: [("F", &words[..]), ("T", &words[..])].into(), scalars: sc };
        let n = w * h / 2 + w * h / 4 + 1;
        let interp = cpu.run(&k, n, &["px"]).unwrap().into_iter().next().unwrap();
        assert_eq!(interp, p010_cpu(&words, w, h, 3));
    }

    /// Every 8-bit colour, painted flat, comes back exact through a float BT.709 limited-range decode of its P010.
    #[test]
    fn p010_round_trip_is_exact_for_every_colour() {
        let (kr, kb) = (0.2126f64, 0.0722f64);
        let kg = 1.0 - kr - kb;
        // per red level: a 512 x 512 frame of 2x2 blocks, green across, blue down (and the tag row)
        let (w, h) = (512usize, 512usize);
        let mut worst = 0i32;
        let mut frame = vec![0u32; w * (h + 1)];
        for r in 0..256u32 {
            for y in 0..h {
                for x in 0..w {
                    frame[y * w + x] = 0xff00_0000 | ((y as u32 / 2) << 16) | ((x as u32 / 2) << 8) | r;
                }
            }
            let out = p010_cpu(&frame, w, h, 8);
            let (ny, cw) = (w * h / 2, w / 2);
            for b in 0..256usize {
                for g in 0..256usize {
                    let p = 2 * b * w + 2 * g;
                    let yv = (out[p / 2] >> 6) & 1023;
                    let c = out[ny + b * cw + g];
                    let (u, v) = ((c >> 6) & 1023, (c >> 22) & 1023);
                    let yf = (yv as f64 / 4.0 - 16.0) / 219.0;
                    let (uf, vf) = ((u as f64 / 4.0 - 128.0) / 224.0, (v as f64 / 4.0 - 128.0) / 224.0);
                    let rf = yf + 2.0 * (1.0 - kr) * vf;
                    let bf = yf + 2.0 * (1.0 - kb) * uf;
                    let gf = (yf - kr * rf - kb * bf) / kg;
                    let q = |f: f64| (f * 255.0).round().clamp(0.0, 255.0) as i32;
                    let e = (q(rf) - r as i32).abs().max((q(gf) - g as i32).abs()).max((q(bf) - b as i32).abs());
                    worst = worst.max(e);
                }
            }
        }
        assert_eq!(worst, 0, "worst channel error over all 16.7M colours");
    }
}
