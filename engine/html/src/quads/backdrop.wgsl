// A backdrop filter's blur (Op::Backdrop): the page as composited so far, blurred over one region, in two separable
// passes. h: rows of the region (and r above and below it) blurred across into a float scratch; v: the scratch blurred
// down into the region of the page-sized output, at the page's own coordinates. Edges repeat the page's last pixel.
// m: a colour matrix after the blur (a filter's saturate(), brightness(), ...): rows R G B A, the offsets, then
// m[5].x 1 when there is one. It works on straight colour, as filters do, and the result is premultiplied again.
struct U { origin: vec2<i32>, size: vec2<i32>, page: vec2<i32>, r: i32, sigma: f32, m: array<vec4<f32>, 6> }

fn colour_matrix(p: vec4<f32>) -> vec4<f32> {
  if (u.m[5].x < 0.5) { return p; }
  let c = select(vec4<f32>(0.0), vec4<f32>(p.rgb / p.a, p.a), p.a > 0.0);
  let o = clamp(vec4<f32>(dot(u.m[0], c), dot(u.m[1], c), dot(u.m[2], c), dot(u.m[3], c)) + u.m[4], vec4<f32>(0.0), vec4<f32>(1.0));
  return vec4<f32>(o.rgb * o.a, o.a);
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<uniform> u: U;
@group(0) @binding(2) var tmp_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var out: texture_storage_2d<rgba8unorm, write>;

fn weight(k: i32) -> f32 {
  let x = f32(k);
  return exp(-x * x / (2.0 * u.sigma * u.sigma));
}

@compute @workgroup_size(16, 16)
fn h(@builtin(global_invocation_id) id: vec3<u32>) {
  let x = i32(id.x);
  let y = i32(id.y);
  if (x >= u.size.x || y >= u.size.y + 2 * u.r) { return; }
  let sy = clamp(u.origin.y - u.r + y, 0, u.page.y - 1);
  var acc = vec4<f32>(0.0);
  var total = 0.0;
  for (var k = -u.r; k <= u.r; k++) {
    let w = weight(k);
    let sx = clamp(u.origin.x + x + k, 0, u.page.x - 1);
    acc += w * textureLoad(src, vec2<i32>(sx, sy), 0);
    total += w;
  }
  textureStore(tmp_out, vec2<i32>(x, y), acc / total);
}

@compute @workgroup_size(16, 16)
fn v(@builtin(global_invocation_id) id: vec3<u32>) {
  let x = i32(id.x);
  let y = i32(id.y);
  if (x >= u.size.x || y >= u.size.y) { return; }
  var acc = vec4<f32>(0.0);
  var total = 0.0;
  for (var k = -u.r; k <= u.r; k++) {
    let w = weight(k);
    acc += w * textureLoad(src, vec2<i32>(x, y + u.r + k), 0);
    total += w;
  }
  textureStore(out, u.origin + vec2<i32>(x, y), colour_matrix(acc / total));
}
