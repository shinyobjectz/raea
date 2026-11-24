use parry3d::query::PointQuery;

use super::*;

pub(crate) type Tri = [[f32; 3]; 3];

/// A .glb with a node per part (its translation, its own material), flat normals, every triangle its own vertices.
pub(crate) fn glb(parts: &[(Vec<Tri>, [f32; 3])]) -> Vec<u8> {
    let (mut bin, mut acc, mut views, mut meshes, mut nodes, mut mats) = (Vec::<u8>::new(), vec![], vec![], vec![], vec![], vec![]);
    for (k, (tris, at)) in parts.iter().enumerate() {
        let (mut p, mut n) = (vec![], vec![]);
        for t in tris {
            let [a, b, c] = t.map(Vec3::from);
            let nn = (b - a).cross(c - a).normalize().to_array();
            p.extend(t);
            n.extend([nn; 3]);
        }
        for (i, data) in [&p, &n].into_iter().enumerate() {
            views.push(json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": data.len() * 12}));
            bin.extend(data.iter().flatten().flat_map(|c| c.to_le_bytes()));
            let mut a = json!({"bufferView": views.len() - 1, "componentType": 5126, "count": data.len(), "type": "VEC3"});
            if i == 0 {
                let (lo, hi) = data.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(l, h), v| (std::array::from_fn(|j| l[j].min(v[j])), std::array::from_fn(|j| h[j].max(v[j]))));
                (a["min"], a["max"]) = (json!(lo), json!(hi));
            }
            acc.push(a);
        }
        meshes.push(json!({"primitives": [{"attributes": {"POSITION": acc.len() - 2, "NORMAL": acc.len() - 1}, "material": k}]}));
        nodes.push(json!({"mesh": k, "translation": at}));
        mats.push(json!({"name": format!("part{k}")}));
    }
    let doc = json!({"asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": (0..parts.len()).collect::<Vec<_>>()}], "nodes": nodes, "meshes": meshes, "materials": mats, "accessors": acc, "bufferViews": views, "buffers": [{"byteLength": bin.len()}]});
    let mut js = doc.to_string().into_bytes();
    js.resize(js.len().div_ceil(4) * 4, b' ');
    bin.resize(bin.len().div_ceil(4) * 4, 0);
    let mut out = b"glTF".to_vec();
    for w in [2, 12 + 8 + js.len() + 8 + bin.len()] {
        out.extend((w as u32).to_le_bytes());
    }
    for (chunk, kind) in [(js, b"JSON"), (bin, b"BIN\0")] {
        out.extend((chunk.len() as u32).to_le_bytes());
        out.extend(kind);
        out.extend(chunk);
    }
    out
}

/// A box's twelve triangles, wound outwards.
pub(crate) fn cuboid(lo: [f32; 3], hi: [f32; 3]) -> Vec<Tri> {
    let mut out = vec![];
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for side in [false, true] {
            let corner = |a: bool, b: bool| {
                let mut q = [0.0; 3];
                (q[axis], q[u], q[v]) = (if side { hi[axis] } else { lo[axis] }, if a { hi[u] } else { lo[u] }, if b { hi[v] } else { lo[v] });
                q
            };
            let [p0, p1, p2, p3] = [corner(false, false), corner(true, false), corner(true, true), corner(false, true)];
            out.extend(if side { [[p0, p1, p2], [p0, p2, p3]] } else { [[p0, p2, p1], [p0, p3, p2]] });
        }
    }
    out
}

/// A floor slab 20 cm thick, a block sunk 10 cm into it, and a small box set on it by its node's translation.
pub(crate) fn set() -> Vec<u8> {
    glb(&[
        (cuboid([-2.0, -0.2, -2.0], [2.0, 0.0, 2.0]), [0.0; 3]),
        (cuboid([-0.25, -0.1, -0.25], [0.25, 0.4, 0.25]), [0.0; 3]),
        (cuboid([-0.1, 0.0, -0.1], [0.1, 0.2, 0.1]), [1.2, 0.0, 0.0]),
    ])
}

#[test]
fn texels_sit_on_the_world_surface_and_their_origins_just_off_it() {
    let t = texels(&set(), &request(16.0, 1024)).unwrap();
    assert!(t.layout.texels.len() > 1000 && t.offset > 0.0 && t.offset < 1e-3, "{} texels, offset {}", t.layout.texels.len(), t.offset);
    for (x, o) in t.layout.texels.iter().zip(&t.origin) {
        let on = t.mesh.project_local_point(Vec3::from(x.p), false).point;
        assert!(on.distance(Vec3::from(x.p)) < 1e-3, "texel at {:?} is off the surface (nearest {on:?})", x.p);
        let moved = Vec3::from(*o) - Vec3::from(x.p);
        assert!((moved.dot(Vec3::from(x.n)) - t.offset).abs() < 0.2 * t.offset && moved.length() < 0.51 / t.layout.density, "the origin of {:?} moved {moved:?} for an offset of {}", x.p, t.offset);
    }
}

#[test]
fn a_point_on_an_edge_is_moved_off_it_and_never_far() {
    let [a, b, c] = [Vec3::ZERO, Vec3::X, Vec3::Z];
    for p in [a, b, Vec3::X * 0.5, (b + c) * 0.5] {
        let q = off_edges([a, b, c], p, 1e-3, 0.1);
        assert!(q.x > 0.9e-3 && q.z > 0.9e-3 && 1.0 - q.x - q.z > 0.9e-3 && q.distance(p) < 5e-3, "{p} went to {q}");
    }
    assert!(off_edges([a, b, c], Vec3::new(0.3, 0.0, 0.3), 1e-3, 0.1).distance(Vec3::new(0.3, 0.0, 0.3)) < 1e-6, "a point in the open stays");
    let needle = [Vec3::ZERO, Vec3::X * 4.0, Vec3::new(4.0, 0.0, 2e-4)];
    assert!(off_edges(needle, Vec3::X * 0.5, 1e-3, 0.01).distance(Vec3::X * 0.5) < 0.0101);
}

#[test]
fn texels_inside_the_slab_are_marked_and_those_in_the_open_are_not() {
    let t = texels(&set(), &request(16.0, 1024)).unwrap();
    let (mut sunk, mut open, mut under) = (0, 0, 0);
    for (x, &inside) in t.layout.texels.iter().zip(&t.inside) {
        let [px, py, pz] = x.p;
        if (-0.18..-0.02).contains(&py) && px.abs() < 0.26 && pz.abs() < 0.26 {
            // the block's sides within the slab's thickness, away from its faces
            assert!(inside, "a texel of the block inside the slab at {:?} isn't marked", x.p);
            sunk += 1;
        } else if py > 0.02 || (-0.18..-0.02).contains(&py) {
            // in the open: above the floor, or the slab's own sides
            assert!(!inside, "a texel in the open at {:?} is marked inside", x.p);
            open += (py > 0.02) as usize;
        } else if py.abs() < 1e-3 && x.n[1] > 0.9 && px.abs() < 0.2 && pz.abs() < 0.2 {
            assert!(inside, "the floor under the block at {:?} isn't marked", x.p);
            under += 1;
        } else if py.abs() < 1e-3 && x.n[1] > 0.9 && px.abs() > 0.4 && (px - 1.2).abs() > 0.2 {
            assert!(!inside, "open floor at {:?} is marked inside", x.p);
        }
    }
    assert!(sunk > 10 && open > 100 && under > 10, "sunk {sunk}, open {open}, under {under}");
}

#[test]
fn the_same_model_gives_the_same_texels() {
    let (a, b) = (texels(&set(), &request(16.0, 1024)).unwrap(), texels(&set(), &request(16.0, 1024)).unwrap());
    assert!(a.layout == b.layout && a.origin == b.origin && a.inside == b.inside);
}

#[test]
fn a_worn_models_request_gives_its_wear_atlas() {
    let req = json!({"env": {"density": 8, "rays": {"sky": 4, "cavity": 4}}});
    let t = texels(&set(), &req).unwrap();
    assert_eq!(t.layout, raea_sys_atlas::layout(&set(), &req).unwrap());
}

/// The measured run (docs/bake.feature BK2): RAEA_BAKE_GLB's texels at 64 a metre, how many are inside, and the time.
/// `cargo test -p raea-sys-lightbake measured -- --ignored --nocapture`
#[test]
#[ignore]
fn measured() {
    let glb = std::fs::read(std::env::var("RAEA_BAKE_GLB").expect("RAEA_BAKE_GLB: a .glb")).unwrap();
    let t0 = std::time::Instant::now();
    let layout = raea_sys_atlas::layout(&glb, &request(64.0, 4096)).unwrap();
    let t1 = std::time::Instant::now();
    let t = texels(&glb, &request(64.0, 4096)).unwrap();
    let inside = t.inside.iter().filter(|&&i| i).count();
    if let Ok(out) = std::env::var("RAEA_BAKE_OUT") {
        // the atlas: grey where a texel is in the open, red where it's inside, black where there's none (a PPM)
        let mut img = vec![0u8; (layout.w * layout.h * 3) as usize];
        for (x, &i) in t.layout.texels.iter().zip(&t.inside) {
            let k = 3 * (x.y as usize * layout.w as usize + x.x as usize);
            img[k..k + 3].copy_from_slice(if i { &[230, 40, 30] } else { &[150, 150, 150] });
        }
        std::fs::write(out, [format!("P6 {} {} 255\n", layout.w, layout.h).into_bytes(), img].concat()).unwrap();
    }
    println!("{}x{} {} texels, {} inside ({:.2}%), offset {:.1e} m; layout {:.2} s, texels with it {:.2} s", layout.w, layout.h, t.layout.texels.len(), inside, 100.0 * inside as f64 / t.layout.texels.len() as f64, t.offset, (t1 - t0).as_secs_f64(), t1.elapsed().as_secs_f64());
}
