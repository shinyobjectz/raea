// LightBench's rays (joined after light.wgsl, whose bindings and private state they use): the random stream, the
// BVH's walks (any hit for shadow rays, the nearest hit for bounces) and the directions rays are chosen in.

fn pcg(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

fn rnd() -> f32 {
    seed = pcg(seed);
    return f32(seed >> 8u) / 16777216.0;
}

fn hit_box(o: vec3<f32>, inv: vec3<f32>, mn: vec3<f32>, mx: vec3<f32>, tmax: f32) -> bool {
    let t0 = (mn - o) * inv;
    let t1 = (mx - o) * inv;
    let lo = min(t0, t1);
    let hi = max(t0, t1);
    let near = max(max(lo.x, lo.y), max(lo.z, 0.0));
    let far = min(min(hi.x, hi.y), min(hi.z, tmax));
    return near <= far;
}

const MISS: f32 = 3.4e38;

// Möller-Trumbore, both sides: how far along the ray triangle i is, MISS if it isn't on it
fn tri_t(o: vec3<f32>, d: vec3<f32>, i: u32) -> f32 {
    let v0 = tris[3u * i].xyz;
    let e1 = tris[3u * i + 1u].xyz;
    let e2 = tris[3u * i + 2u].xyz;
    let pv = cross(d, e2);
    let det = dot(e1, pv);
    if abs(det) < 1e-12 {
        return MISS;
    }
    let inv = 1.0 / det;
    let tv = o - v0;
    let u = dot(tv, pv) * inv;
    if u < 0.0 || u > 1.0 {
        return MISS;
    }
    let qv = cross(tv, e1);
    let v = dot(d, qv) * inv;
    if v < 0.0 || u + v > 1.0 {
        return MISS;
    }
    let t = dot(e2, qv) * inv;
    return select(MISS, t, t > 0.0);
}

fn hit_tri(o: vec3<f32>, d: vec3<f32>, i: u32, tmax: f32) -> bool {
    return tri_t(o, d, i) < tmax;
}

// whether anything lies on the ray before tmax: the BVH, or every triangle when brute force is asked for
fn occluded(o: vec3<f32>, d: vec3<f32>, tmax: f32) -> bool {
    rays += 1u;
    if p.mode.w != 0u {
        let n = arrayLength(&tris) / 3u;
        for (var i = 0u; i < n; i++) {
            if hit_tri(o, d, i, tmax) {
                return true;
            }
        }
        return false;
    }
    let inv = 1.0 / select(d, vec3(1e-12), abs(d) < vec3(1e-12));
    var stack: array<u32, STACK>;
    var sp = 0u;
    var i = 0u;
    loop {
        let n = nodes[i];
        if hit_box(o, inv, n.mn, n.mx, tmax) {
            if n.count > 0u {
                for (var t = n.a; t < n.a + n.count; t++) {
                    if hit_tri(o, d, t, tmax) {
                        return true;
                    }
                }
            } else if sp < STACK {
                stack[sp] = n.a;
                sp++;
                i++;
                continue;
            }
        }
        if sp == 0u {
            break;
        }
        sp--;
        i = stack[sp];
    }
    return false;
}

struct Hit { t: f32, i: u32 }

// where the ray enters a box (0 if it starts inside), MISS if it doesn't before tmax
fn box_t(o: vec3<f32>, inv: vec3<f32>, mn: vec3<f32>, mx: vec3<f32>, tmax: f32) -> f32 {
    let t0 = (mn - o) * inv;
    let t1 = (mx - o) * inv;
    let lo = min(t0, t1);
    let hi = max(t0, t1);
    let near = max(max(lo.x, lo.y), max(lo.z, 0.0));
    let far = min(min(hi.x, hi.y), min(hi.z, tmax));
    return select(MISS, near, near <= far);
}

// the nearest triangle on the ray before tmax (i 0xFFFFFFFF if none): at each inner node the nearer child first and
// the farther kept with where the ray enters it, so once a hit is nearer than a kept box that box is skipped; every
// triangle with brute force
fn closest(o: vec3<f32>, d: vec3<f32>, tmax: f32) -> Hit {
    rays += 1u;
    var best = Hit(tmax, 0xFFFFFFFFu);
    if p.mode.w != 0u {
        let n = arrayLength(&tris) / 3u;
        for (var i = 0u; i < n; i++) {
            let t = tri_t(o, d, i);
            if t < best.t {
                best = Hit(t, i);
            }
        }
        return best;
    }
    let inv = 1.0 / select(d, vec3(1e-12), abs(d) < vec3(1e-12));
    if box_t(o, inv, nodes[0].mn, nodes[0].mx, best.t) == MISS {
        return best;
    }
    var stack: array<u32, STACK>;
    var enter: array<f32, STACK>;
    var sp = 0u;
    var i = 0u;
    loop {
        let n = nodes[i];
        var next = 0xFFFFFFFFu;
        if n.count > 0u {
            for (var t = n.a; t < n.a + n.count; t++) {
                let h = tri_t(o, d, t);
                if h < best.t {
                    best = Hit(h, t);
                }
            }
        } else {
            let tl = box_t(o, inv, nodes[i + 1u].mn, nodes[i + 1u].mx, best.t);
            let tr = box_t(o, inv, nodes[n.a].mn, nodes[n.a].mx, best.t);
            if tl < MISS && tr < MISS {
                let left_first = tl <= tr;
                next = select(n.a, i + 1u, left_first);
                if sp < STACK {
                    stack[sp] = select(i + 1u, n.a, left_first);
                    enter[sp] = max(tl, tr);
                    sp++;
                }
            } else if tl < MISS {
                next = i + 1u;
            } else if tr < MISS {
                next = n.a;
            }
        }
        // nothing to go down into: the nearest kept box the ray still reaches before its best hit
        while next == 0xFFFFFFFFu && sp > 0u {
            sp--;
            if enter[sp] < best.t {
                next = stack[sp];
            }
        }
        if next == 0xFFFFFFFFu {
            break;
        }
        i = next;
    }
    return best;
}

// an orthonormal basis about unit n (Duff et al.): its columns are two tangents and n
fn basis(n: vec3<f32>) -> mat3x3<f32> {
    let s = select(-1.0, 1.0, n.z >= 0.0);
    let a = -1.0 / (s + n.z);
    let b = n.x * n.y * a;
    return mat3x3(vec3(1.0 + s * n.x * n.x * a, s * b, -s * n.x), vec3(b, s + n.y * n.y * a, -n.y), n);
}

// a direction about n, chosen in proportion to its cosine
fn cosine_dir(n: vec3<f32>) -> vec3<f32> {
    let phi = 2.0 * PI * rnd();
    let r2 = rnd();
    let r = sqrt(r2);
    return normalize(basis(n) * vec3(r * cos(phi), r * sin(phi), sqrt(max(0.0, 1.0 - r2))));
}

// a direction within the cone about unit axis whose half-angle has cosine cos_max, every direction in it as likely
fn cone_dir(axis: vec3<f32>, cos_max: f32) -> vec3<f32> {
    let phi = 2.0 * PI * rnd();
    let ct = 1.0 - rnd() * (1.0 - cos_max);
    let st = sqrt(max(0.0, 1.0 - ct * ct));
    return normalize(basis(axis) * vec3(st * cos(phi), st * sin(phi), ct));
}
