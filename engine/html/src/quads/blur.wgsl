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

// The same sums as bh and bv, made as Skia makes them: the three boxes one after another, each a running sum, so a
// pixel costs a few adds whatever the radius (bh and bv cost 2r+1 multiply-adds a pixel: 481 at blur(80px)). The sums
// are whole numbers, so three boxes in turn give exactly the kernel's sum (it's their convolution); the largest is 255 *
// d^3 < 2^32 (d < 255), and running sums in wrapping u32 arithmetic are exact. One thread a row (bh3) or column (bv3),
// streaming along it with a ring of each of the first two boxes' last values. b.bx = (a1, b1, a2, b2) and b.by.xy =
// (a3, b3): each box's offsets; b.by.z, b.by.w: the two rings' lengths (each box's window and the value leaving it).
// The first box's sums are at most 255 x 255, so its ring keeps them as 16-bit halves (rings1).
// The rings are the thread's own stretch of `rings`, not arrays in the thread: those the driver backs with scratch for
// every thread the GPU can hold at once (8 KB each: ~380 MB, held for the device's life)
struct B3 { origin: vec2<i32>, size: vec2<i32>, page: vec2<i32>, r: i32, divider: u32, bx: vec4<i32>, by: vec4<i32> }
@group(0) @binding(5) var<uniform> b3: B3;
@group(0) @binding(6) var<storage, read_write> rings: array<vec4<u32>>;
@group(0) @binding(9) var<storage, read_write> rings1: array<vec2<u32>>;

// a ring's slot for a running position (positions start a box's reach below zero)
fn slot(pos: i32, n: u32) -> u32 { return u32(pos + 65536) % n; }
fn pack1(q: vec4<u32>) -> vec2<u32> { return vec2<u32>(q.x | (q.y << 16u), q.z | (q.w << 16u)); }
fn unpack1(p: vec2<u32>) -> vec4<u32> { return vec4<u32>(p.x & 0xffffu, p.x >> 16u, p.y & 0xffffu, p.y >> 16u); }
// the x pass's whole numbers (0-255) between bh3 and bv3: 8 bits a channel, half the float scratch's bytes
@group(0) @binding(7) var btmp8: texture_storage_2d<rgba8uint, write>;
@group(0) @binding(8) var bsrc8: texture_2d<u32>;

fn px_h(u: i32, gy: i32) -> vec4<u32> {
  let gx = b3.origin.x + u;
  if (gx < 0 || gx >= b3.page.x) { return vec4<u32>(0u); }
  return vec4<u32>(round(textureLoad(bsrc, vec2<i32>(gx, gy), 0) * 255.0));
}

fn px_v(x: i32, j: i32) -> vec4<u32> {
  return textureLoad(bsrc8, vec2<i32>(x, j), 0);
}

@compute @workgroup_size(64)
fn bh3(@builtin(global_invocation_id) id: vec3<u32>) {
  let y = i32(id.x);
  if (y >= b3.size.y + 2 * b3.r) { return; }
  let gy = b3.origin.y - b3.r + y;
  let a1 = b3.bx.x; let e1 = b3.bx.y; let a2 = b3.bx.z; let e2 = b3.bx.w; let a3 = b3.by.x; let e3 = b3.by.y;
  let rw = b3.size.x;
  if (gy < 0 || gy >= b3.page.y) {
    for (var x = 0; x < rw; x++) { textureStore(btmp8, vec2<i32>(x, y), vec4<u32>(0u)); }
    return;
  }
  let n1 = u32(b3.by.z);
  let n2 = u32(b3.by.w);
  let r1 = u32(id.x) * n1;
  let r2 = u32(id.x) * n2;
  var q1 = vec4<u32>(0u);
  var q2 = vec4<u32>(0u);
  var q3 = vec4<u32>(0u);
  let us = a2 + a3;
  let ue = rw - 1 + e2 + e3;
  // each ring's slot for the positions the step reads and writes, kept as counters that move one a step and wrap by
  // hand (a division a pixel cost ~2.5% of a render): u, v - 1 + a2, v, and the third box's v - e3 - 1 + a3
  var cu1 = slot(us, n1);
  var cl1 = slot(us - e2 - 1 + a2, n1);
  var cv2 = slot(us - e2, n2);
  var cl2 = slot(us - e2 - e3 - 1 + a3, n2);
  var u = us;
  loop {
    if (u > ue) { break; }
    // the first box over the pixels
    if (u == us) {
      q1 = vec4<u32>(0u);
      for (var t = a1; t <= e1; t++) { q1 += px_h(u + t, gy); }
    } else {
      q1 = q1 + px_h(u + e1, gy) - px_h(u - 1 + a1, gy);
    }
    rings1[r1 + cu1] = pack1(q1);
    // the second over the first's, once its window is complete
    let v = u - e2;
    if (v < a3) { continue; }
    if (v == a3) {
      q2 = vec4<u32>(0u);
      for (var t = a2; t <= e2; t++) { q2 += unpack1(rings1[r1 + slot(v + t, n1)]); }
    } else {
      q2 = q2 + unpack1(rings1[r1 + cu1]) - unpack1(rings1[r1 + cl1]);
    }
    rings[r2 + cv2] = q2;
    // the third over the second's: the output pixel
    let x = v - e3;
    if (x < 0) { continue; }
    if (x == 0) {
      q3 = vec4<u32>(0u);
      for (var t = a3; t <= e3; t++) { q3 += rings[r2 + slot(x + t, n2)]; }
    } else {
      q3 = q3 + rings[r2 + cv2] - rings[r2 + cl2];
    }
    textureStore(btmp8, vec2<i32>(x, y), vec4<u32>(fixdiv(q3.x, b3.divider), fixdiv(q3.y, b3.divider), fixdiv(q3.z, b3.divider), fixdiv(q3.w, b3.divider)));
    continuing {
      u++;
      cu1 = select(cu1 + 1u, 0u, cu1 + 1u == n1);
      cl1 = select(cl1 + 1u, 0u, cl1 + 1u == n1);
      cv2 = select(cv2 + 1u, 0u, cv2 + 1u == n2);
      cl2 = select(cl2 + 1u, 0u, cl2 + 1u == n2);
    }
  }
}

@compute @workgroup_size(64)
fn bv3(@builtin(global_invocation_id) id: vec3<u32>) {
  let x = i32(id.x);
  if (x >= b3.size.x) { return; }
  let a1 = b3.bx.x; let e1 = b3.bx.y; let a2 = b3.bx.z; let e2 = b3.bx.w; let a3 = b3.by.x; let e3 = b3.by.y;
  let rh = b3.size.y;
  let r = b3.r;
  let n1 = u32(b3.by.z);
  let n2 = u32(b3.by.w);
  let r1 = u32(id.x) * n1;
  let r2 = u32(id.x) * n2;
  var q1 = vec4<u32>(0u);
  var q2 = vec4<u32>(0u);
  var q3 = vec4<u32>(0u);
  // output row y reads the scratch's rows y + r + k: the boxes run over j = y + r
  let us = a2 + a3;
  let ue = rh - 1 + e2 + e3;
  // each ring's slot for the positions the step reads and writes, kept as counters that move one a step and wrap by
  // hand (a division a pixel cost ~2.5% of a render): u, v - 1 + a2, v, and the third box's v - e3 - 1 + a3
  var cu1 = slot(us, n1);
  var cl1 = slot(us - e2 - 1 + a2, n1);
  var cv2 = slot(us - e2, n2);
  var cl2 = slot(us - e2 - e3 - 1 + a3, n2);
  var u = us;
  loop {
    if (u > ue) { break; }
    if (u == us) {
      q1 = vec4<u32>(0u);
      for (var t = a1; t <= e1; t++) { q1 += px_v(x, u + t + r); }
    } else {
      q1 = q1 + px_v(x, u + e1 + r) - px_v(x, u - 1 + a1 + r);
    }
    rings1[r1 + cu1] = pack1(q1);
    let v = u - e2;
    if (v < a3) { continue; }
    if (v == a3) {
      q2 = vec4<u32>(0u);
      for (var t = a2; t <= e2; t++) { q2 += unpack1(rings1[r1 + slot(v + t, n1)]); }
    } else {
      q2 = q2 + unpack1(rings1[r1 + cu1]) - unpack1(rings1[r1 + cl1]);
    }
    rings[r2 + cv2] = q2;
    let y = v - e3;
    if (y < 0) { continue; }
    if (y == 0) {
      q3 = vec4<u32>(0u);
      for (var t = a3; t <= e3; t++) { q3 += rings[r2 + slot(y + t, n2)]; }
    } else {
      q3 = q3 + rings[r2 + cv2] - rings[r2 + cl2];
    }
    textureStore(bout, b3.origin + vec2<i32>(x, y), vec4<f32>(f32(fixdiv(q3.x, b3.divider)), f32(fixdiv(q3.y, b3.divider)), f32(fixdiv(q3.z, b3.divider)), f32(fixdiv(q3.w, b3.divider))) / 255.0);
    continuing {
      u++;
      cu1 = select(cu1 + 1u, 0u, cu1 + 1u == n1);
      cl1 = select(cl1 + 1u, 0u, cl1 + 1u == n1);
      cv2 = select(cv2 + 1u, 0u, cv2 + 1u == n2);
      cl2 = select(cl2 + 1u, 0u, cl2 + 1u == n2);
    }
  }
}
