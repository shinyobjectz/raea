// raea GI's screen pass (lib.rs; docs/realism.feature LT8): each pixel's indirect diffuse light, from the four screen
// probes around it (probes.wesl, filtered), each weighed by how near the pixel lies to it on the screen, how alike their normals
// are and how near the pixel is to the probe's plane, so light doesn't cross an edge. Where no probe is on the pixel's
// surface (a thing smaller than a probe's tile), the world cache is read at the pixel's own surface with the sun's
// share taken out (raster draws the sun, its shadows with it). Written as the irradiance volume's light is (irradiance
// over pi), and 1 in alpha where there was light to take; 0 where the pixel is sky, outside the view, or its cell is
// new, so the main pass draws Bevy's own light there.
import raea_sys_rt::cache::{find, hash, radiance, cells, sunlit, LIFE, NONE};
import raea_sys_gi::frame::{surface, view, soft, out_tex, p, TILE};

const PI: f32 = 3.14159265;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(out_tex);
    if (any(id.xy >= size)) {
        return;
    }
    let px = vec2<i32>(id.xy);
    let s = surface(px);
    if (s.at.w <= 0.0) {
        textureStore(out_tex, px, vec4(0.0));
        return;
    }
    let at = s.at.xyz;
    let n = s.n;
    let reach = 0.05 + 0.02 * length(at - view.world_position);
    let tiles = vec2<i32>(p.run.yz);
    // where the pixel lies between the middles of the tiles around it
    let g = (vec2<f32>(id.xy) + 0.5) / f32(TILE) - 0.5;
    let i0 = vec2<i32>(floor(g));
    let f = g - floor(g);
    var sum = vec3(0.0);
    var weight = 0.0;
    var best = 0.0;
    for (var k = 0; k < 4; k++) {
        let o = vec2(k & 1, k >> 1);
        let t = clamp(i0 + o, vec2(0), tiles - 1);
        let q = soft[u32(t.x + t.y * tiles.x)];
        if (q.at.w <= 0.0) {
            continue;
        }
        let b = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1);
        let same = pow(max(dot(q.n.xyz, n), 0.0), 8.0) * exp(-abs(dot(q.n.xyz, at - q.at.xyz)) / reach);
        // a little of each probe whatever the distance, so one on another surface can be stood in for by its neighbour
        let w = (b + 0.02) * same;
        sum += q.e.rgb * w;
        weight += w;
        best = max(best, same);
    }
    if (best > 0.1) {
        textureStore(out_tex, px, vec4(sum / weight / PI, 1.0));
        return;
    }
    var seed = hash(id.x * 1973u + id.y * 9277u + p.run.x);
    let c = find(at + n * 0.02, n, LIFE, &seed);
    if (c == NONE) {
        textureStore(out_tex, px, vec4(0.0));
        return;
    }
    let r = radiance[c];
    if (r.a < 1.0) {
        textureStore(out_tex, px, vec4(0.0));
        return;
    }
    // the sun as the cell saw it: on its own side, the share of its learning the sun was in view
    let sun = p.sun.xyz * max(dot(cells[c].n.xyz, p.sun_dir.xyz), 0.0) * sunlit[c];
    textureStore(out_tex, px, vec4(max(r.rgb - sun, vec3(0.0)) / PI, 1.0));
}
