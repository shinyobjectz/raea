// the split emission gathered over frames at the output's size (RAEA_SOLARI_EMISSIVE=taa): each frame's jittered samples
// weighted by their distance to this output pixel's centre, added to its history reprojected by the motion, which is
// dropped where the depth there changed (a disocclusion), what it showed has gone, or the camera cut. It's clamped to
// the samples about it only where they show every surface it was gathered over (a twig's pixel that misses its sky
// this frame keeps it) or what it showed has gone (RT32)
@group(0) @binding(9) var depth: texture_depth_2d;
@group(0) @binding(10) var motion: texture_2d<f32>;
@group(0) @binding(11) var hist_in: texture_2d<f32>;
@group(0) @binding(12) var hist_sampler: sampler;
@group(0) @binding(13) var hist_out: texture_storage_2d<rgba16float, write>;
// each pixel's depth (r); the nearest and farthest depths its history was gathered over (a, b); and the evidence for
// the nearest (g: 65536 if it was moving when last seen, plus the frames since it was last about times 256, plus how
// often it is about, of 255)
@group(0) @binding(14) var hist_depth_in: texture_storage_2d<rgba32float, read>;
@group(0) @binding(15) var hist_depth_out: texture_storage_2d<rgba32float, write>;
// the most frames' weight the history holds: past it, it fades as 1 / HISTORY_CAP a frame
const HISTORY_CAP: f32 = 16.0;
// a still pixel's history held within what's about it now, its darkest and brightest times these
const STILL_LOW: f32 = 0.95;
const STILL_HIGH: f32 = 1.05;
// a moving pixel's history held under this many times the brightest about it, plus this
const MOVING_GAIN: f32 = 4.0;
const MOVING_FLOOR: f32 = 0.02;
// the history at `uv` through a Catmull-Rom filter in five bilinear taps (Jimenez's), so a moving view isn't blurred a
// little more each frame as a bilinear read would; never below zero (its lobes ring on a hard edge)
fn history_at(uv: vec2<f32>, size: vec2<f32>) -> vec4<f32> {
    let at = uv * size;
    let t1 = floor(at - 0.5) + 0.5;
    let f = at - t1;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let t0 = (t1 - 1.0) / size;
    let t3 = (t1 + 2.0) / size;
    let t12 = (t1 + w2 / w12) / size;
    var r = textureSampleLevel(hist_in, hist_sampler, vec2<f32>(t12.x, t0.y), 0.0) * (w12.x * w0.y);
    r += textureSampleLevel(hist_in, hist_sampler, vec2<f32>(t0.x, t12.y), 0.0) * (w0.x * w12.y);
    r += textureSampleLevel(hist_in, hist_sampler, t12, 0.0) * (w12.x * w12.y);
    r += textureSampleLevel(hist_in, hist_sampler, vec2<f32>(t3.x, t12.y), 0.0) * (w3.x * w12.y);
    r += textureSampleLevel(hist_in, hist_sampler, vec2<f32>(t12.x, t3.y), 0.0) * (w12.x * w3.y);
    let sum = w12.x * w0.y + w0.x * w12.y + w12.x * w12.y + w3.x * w12.y + w12.x * w3.y;
    return max(r / sum, vec4<f32>(0.0));
}
@compute @workgroup_size(8, 8, 1)
fn restore_taa(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = vec2<u32>(params[1].xy);
    if (id.x >= size.x || id.y >= size.y) { return; }
    let p = vec2<i32>(id.xy);
    let out_size = params[1].xy;
    let in_size = params[1].zw;
    // this output pixel in the jittered input, where the input's sample centres are whole numbers
    let q = (vec2<f32>(id.xy) + 0.5) * in_size / out_size - params[3].xy - 0.5;
    let near = vec2<i32>(round(q));
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    // the depths about this pixel: its history is kept if its depth lies among them, so a twig's pixel that showed
    // sky through it last frame keeps what it gathered, and a surface that moved off what it was showing loses it
    var zlo = 1e9;
    var zhi = 0.0;
    // the least and most emission about it this frame
    var elo = vec3<f32>(1e30);
    var ehi = vec3<f32>(0.0);
    // the nearest surface about this pixel, whose motion it takes: at a silhouette the sample nearest the pixel may be
    // the surface behind, and the history read from where that one was crawled along the edge (photo 2's window)
    var front = clamp(near, vec2<i32>(0), vec2<i32>(in_size) - 1);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let i = near + vec2<i32>(dx, dy);
            if (any(i < vec2<i32>(0)) || any(i >= vec2<i32>(in_size))) { continue; }
            let zi = textureLoad(depth, i, 0);
            zlo = min(zlo, zi);
            zhi = max(zhi, zi);
            if (zi >= zhi) { front = i; }
            let s = textureLoad(emit, i).rgb;
            elo = min(elo, s);
            ehi = max(ehi, s);
            // the distance in output pixels, a Blackman-Harris-like falloff, nothing past one pixel
            let d = (vec2<f32>(i) - q) * out_size / in_size;
            let d2 = dot(d, d);
            if (d2 >= 1.0) { continue; }
            // params[2].w: how fast a sample's weight falls with its distance (RAEA_EMIT_TAA_SHARP)
            let w = exp(-params[2].w * d2);
            sum += w * s;
            weight += w;
        }
    }
    // the most emission as far as a moving pixel's history read reaches (its Catmull-Rom taps, two pixels out): bounded
    // within one pixel, an emitter's edge drifting toward a pixel lit its history through that read before any sample
    // about it showed the edge, and the ceiling held it dark until the edge came within reach, then let it jump 80 levels
    // (photo 2's window). A still pixel's history is read at its own texel, so its bound stays one pixel out: two, and a
    // moving orb's 30000 nits stayed in a 2-pixel rim about it (+2.07 stops on its tile, +0.36 at one)
    var mhi = ehi;
    for (var dy = -2; dy <= 2; dy++) {
        for (var dx = -2; dx <= 2; dx++) {
            if (abs(dx) < 2 && abs(dy) < 2) { continue; }
            let i = near + vec2<i32>(dx, dy);
            if (any(i < vec2<i32>(0)) || any(i >= vec2<i32>(in_size))) { continue; }
            mhi = max(mhi, textureLoad(emit, i).rgb);
        }
    }
    let here = clamp(near, vec2<i32>(0), vec2<i32>(in_size) - 1);
    let z = textureLoad(depth, here, 0);
    // where this pixel was last frame (Bevy's motion is this frame's uv less last frame's)
    let uv = (vec2<f32>(id.xy) + 0.5) / out_size;
    let moved = textureLoad(motion, front, 0).xy;
    let was = uv - moved;
    let still = all(abs(moved * in_size) < vec2<f32>(0.01));
    var hist = vec4<f32>(0.0);
    // for RAEA_EMIT_TAA_DEBUG=4: the history's brightness before and after it's held, and which rules held it (1 kept,
    // 2 still, 4 faded, 8 every surface about, 16 failed at a moving pixel, 32 gone, 64 faded and held)
    var dbg_h = vec3<f32>(0.0);
    var dbg_f = 0.0;
    // the depths this pixel's history was gathered over, from now on, and the evidence for the nearest of them: how
    // many frames since it was last about, how often it is, and whether it was moving when last seen, all carried with
    // the history as it's reprojected. A history not taken starts again: where something moved in, it's about every
    // frame (it leaves nothing as it moves off), and after a cut nothing is known yet (a twig's pixel may drop once, and
    // what moves off at once leaves nothing)
    var dlo = zlo;
    var dhi = zhi;
    var gap = 0.0;
    var rate = select(1.0, 0.5, still && params[0].z < 0.5);
    var moving = !still;
    let inside = all(was >= vec2<f32>(0.0)) && all(was <= vec2<f32>(1.0));
    if (params[0].z < 0.5 && inside) {
        let h = history_at(was, out_size);
        let hd = textureLoad(hist_depth_in, min(vec2<i32>(was * out_size), vec2<i32>(out_size) - 1));
        let hz = hd.r;
        // reverse depth is 1 / distance up to a constant: within 5% of the depths about it. This test is a moving pixel's:
        // where a twig hides sky in some of the jittered frames it drops whichever is rarer about it (sky, mostly), which
        // lost a forest's canopy a quarter of its light, so a still pixel's history is judged by RT32's tests below
        // RAEA_EMIT_TAA_DEBUG=2 never takes the history, 3 always does (a test of what the depth test costs)
        let among = hz >= zlo * 0.95 && hz <= zhi * 1.05;
        let was_moving = hd.g >= 65536.0;
        let ev = hd.g - select(0.0, 65536.0, was_moving);
        let was_gap = floor(ev / 256.0);
        let was_rate = (ev - was_gap * 256.0) / 255.0;
        let near_about = hd.a >= zlo * 0.95 && hd.a <= zhi * 1.05;
        // what a pixel showed may have gone (RT32). A still pixel's history is dropped where none of the depths it was
        // gathered over is about it now (a pickup taken, a light that starts off from rest: the orb's place stayed white
        // for seconds). Where its nearest surface has been away longer than its comings and goings explain (a twig that
        // shows in some jittered frames is away a few frames at a time; the rim of a light that left, or an edge slid
        // off by parallax, never comes back), what's left is held to what's about it now and its evidence starts again.
        // A nearest surface that was moving and isn't about a still pixel has moved off it: the orb's rim, seen for a frame
        // or two as it passed over a desk whose own evidence was young (its rate 0.89, under the 0.9 a twig's gaps are
        // judged by), stayed as a ring at every place it had been (+1.7 stops on its tile). And one away for the jitter's
        // whole cycle (params[3].z frames) has gone whatever its rate: a twig is found again by some jitter position
        // within one, and the rim of an orb that rested there, seen in some of them as a twig is, stayed forever
        let seen = hd.b <= zhi * 1.05 && hd.a >= zlo * 0.95;
        let away = was_gap + 1.0;
        let faded = !near_about && ((was_rate >= 0.9 && away >= 4.0 / was_rate - 3.0) || (still && was_moving) || away >= max(params[3].z, 8.0));
        let gone = still && !seen;
        let kept = select(among, !gone, still);
        let same = (kept || params[0].w == 3.0) && params[0].w != 2.0;
        var hrgb = h.rgb;
        dbg_f = select(0.0, 1.0, kept) + select(0.0, 2.0, still) + select(0.0, 4.0, faded) + select(0.0, 32.0, gone);
        // held to the samples about it: where the view is still, within them; where it moves, only a light that isn't
        // there any more (four times the brightest about it): an edge's history under a drifting view isn't spanned by
        // one frame's samples, and holding it to them flickered photo 2's window edges 80 levels from frame to frame
        let clamped = select(min(hrgb, mhi * MOVING_GAIN + MOVING_FLOOR), clamp(hrgb, elo * STILL_LOW, ehi * STILL_HIGH), still);
        if (!kept && !still && params[0].w != 3.0) {
            // a history that failed its test at a moving pixel is kept as one frame's (below), held to what's about it
            // now: a moving silhouette's mix of its two sides is, and a light taken while the camera moves is gone at
            // once (kept as it was, half of it stayed and faded over seconds)
            hrgb = clamped;
            dbg_f += 16.0;
        }
        if (kept && faded) {
            if (params[0].w != 3.0) { hrgb = clamped; }
            dbg_f += 64.0;
        } else if (kept) {
            // the far end eased toward what's about where the view moves, as parallax slides another surface behind
            dlo = min(zlo, select(mix(hd.b, zlo, 0.125), hd.b, still));
            dhi = max(zhi, hd.a);
            if (zhi > hd.a * 1.05) {
                // a surface nearer than any it saw: the nearest now, seen once
                rate = 0.5;
            } else if (near_about) {
                // the frames it was away counted in once it's back, so how long it may stay away is judged by how often
                // it came before it went (decayed while away, the rate let a rim that never came back stay forever)
                rate = mix(was_rate * pow(0.875, was_gap), 1.0, 0.125);
            } else {
                gap = was_gap + 1.0;
                rate = was_rate;
                moving = was_moving;
            }
            // every surface it was gathered over is about it now (within parallax's reach where the view moves): its
            // emission changes only with jitter across their detail, which the samples about it span, so it's held to
            // them (a lamp switched off goes dark at once, rim and all, not over a second)
            let reach = select(1.5, 1.05, still);
            if (hd.b * reach >= zlo && hd.a <= zhi * reach && params[0].w != 3.0) {
                hrgb = clamped;
                dbg_f += 8.0;
            }
        }
        // a history that fails the test still counts as one frame's: dropped outright, a pixel on a moving silhouette's
        // trailing side was drawn from its frame's samples alone, and crawled (photo 2's window, 300 pixels a frame
        // jumping 80 levels); kept at full weight, whatever moved off it would trail behind
        var cap = select(1.0, HISTORY_CAP, same);
        if (params[0].w == 2.0 || (gone && params[0].w != 3.0)) { cap = 0.0; }
        hist = vec4<f32>(hrgb, min(h.a, cap));
        dbg_h = vec3<f32>(dot(h.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)), dot(hrgb, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0);
    }
    var e: vec3<f32>;
    var held: f32;
    if (hist.a + weight > 1e-4) {
        e = (hist.rgb * hist.a + sum) / (hist.a + weight);
        held = hist.a + weight;
    } else {
        // no sample near and no history: the frame's emission between its samples, as `restore` puts it back
        let i = vec2<i32>(floor(q));
        let t = q - floor(q);
        e = mix(mix(emitted(i), emitted(i + vec2<i32>(1, 0)), t.x), mix(emitted(i + vec2<i32>(0, 1)), emitted(i + vec2<i32>(1, 1)), t.x), t.y);
        held = 0.0;
    }
    textureStore(hist_out, p, vec4<f32>(e, held));
    textureStore(hist_depth_out, p, vec4<f32>(z, select(0.0, 65536.0, moving) + min(gap, 255.0) * 256.0 + round(rate * 255.0), dlo, dhi));
    let c = textureLoad(color, p);
    // RAEA_EMIT_TAA_DEBUG=1: this frame's sample weight, the history's weight (of HISTORY_CAP) and whether the history
    // was taken, as red, green and blue
    // RAEA_EMIT_TAA_DEBUG=4: red where a bound halved the history, green which held it (1 every surface about, 0.5 a
    // failed moving history, 0.25 a faded one), blue where the pixel counts as still
    if (params[0].w == 4.0) {
        let fl = u32(dbg_f);
        let lowered = select(0.0, 1.0, dbg_h.y < 0.5 * dbg_h.x);
        let kind = select(select(select(0.0, 0.25, (fl & 64u) != 0u), 0.5, (fl & 16u) != 0u), 1.0, (fl & 8u) != 0u);
        textureStore(color, p, vec4<f32>(lowered, kind, select(0.0, 1.0, (fl & 2u) != 0u), c.a));
        return;
    }
    if (params[0].w == 1.0) {
        textureStore(color, p, vec4<f32>(weight, hist.a / HISTORY_CAP, f32(hist.a > 0.0), c.a));
        return;
    }
    textureStore(color, p, vec4<f32>(c.rgb + e, c.a));
}
