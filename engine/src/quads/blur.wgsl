// A filter's blur (CSS `filter: blur()`) as Skia, Chrome and raea's vello_cpu paint it: three box blurs per axis, as
// one integer kernel (kw, 2r+1 taps), summed exactly over 8-bit values and divided once by Skia's 32-bit fixed point
// ((sum * divider + 2^31) >> 32); the x pass rounds to 8 bits (kept as whole numbers in the scratch) before the y pass.
// Transparent past the page. In place: bh reads the layer whole into the scratch before bv writes it back.
struct B { origin: vec2<i32>, size: vec2<i32>, page: vec2<i32>, r: i32, divider: u32 }

@group(0) @binding(0) var bsrc: texture_2d<f32>;
@group(0) @binding(1) var<uniform> b: B;
@group(0) @binding(2) var btmp: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var bout: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<storage, read> kw: array<u32>;

// (v * d + 2^31) >> 32 in 32-bit halves
fn fixdiv(v: u32, d: u32) -> u32 {
  let v0 = v & 0xffffu;
  let v1 = v >> 16u;
  let d0 = d & 0xffffu;
  let d1 = d >> 16u;
  let p00 = v0 * d0;
  let p01 = v0 * d1;
  let p10 = v1 * d0;
  let mid = (p00 >> 16u) + (p01 & 0xffffu) + (p10 & 0xffffu);
  let lo = (p00 & 0xffffu) | ((mid & 0xffffu) << 16u);
  var hi = v1 * d1 + (p01 >> 16u) + (p10 >> 16u) + (mid >> 16u);
  if (lo >= 0x80000000u) { hi += 1u; }
  return min(hi, 255u);
}

@compute @workgroup_size(16, 16)
fn bh(@builtin(global_invocation_id) id: vec3<u32>) {
  let x = i32(id.x);
  let y = i32(id.y);
  if (x >= b.size.x || y >= b.size.y + 2 * b.r) { return; }
  let gy = b.origin.y - b.r + y;
  var acc = vec4<u32>(0u);
  if (gy >= 0 && gy < b.page.y) {
    for (var k = -b.r; k <= b.r; k++) {
      let gx = b.origin.x + x + k;
      if (gx >= 0 && gx < b.page.x) {
        let p = vec4<u32>(round(textureLoad(bsrc, vec2<i32>(gx, gy), 0) * 255.0));
        acc += kw[k + b.r] * p;
      }
    }
  }
  textureStore(btmp, vec2<i32>(x, y), vec4<f32>(f32(fixdiv(acc.x, b.divider)), f32(fixdiv(acc.y, b.divider)), f32(fixdiv(acc.z, b.divider)), f32(fixdiv(acc.w, b.divider))));
}

@compute @workgroup_size(16, 16)
fn bv(@builtin(global_invocation_id) id: vec3<u32>) {
  let x = i32(id.x);
  let y = i32(id.y);
  if (x >= b.size.x || y >= b.size.y) { return; }
  var acc = vec4<u32>(0u);
  for (var k = -b.r; k <= b.r; k++) {
    let p = vec4<u32>(textureLoad(bsrc, vec2<i32>(x, y + b.r + k), 0));
    acc += kw[k + b.r] * p;
  }
  textureStore(bout, b.origin + vec2<i32>(x, y), vec4<f32>(f32(fixdiv(acc.x, b.divider)), f32(fixdiv(acc.y, b.divider)), f32(fixdiv(acc.z, b.divider)), f32(fixdiv(acc.w, b.divider))) / 255.0);
}
