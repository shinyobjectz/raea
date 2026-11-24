//! Synthetic fronts checked by number: walls with and without an overhang on a ground, a slab's arris, walls facing
//! south and north, and the bake end to end (the same bytes twice, TEXCOORD_1 on every primitive).

use std::path::Path;

use parry3d::glamx::Vec3;
use serde_json::{Value, json};

use super::*;

type Prim = (&'static str, Vec<Vec3>, Vec<[u32; 3]>);

/// A quad from o along a then b; its normal is a x b.
fn quad(o: Vec3, a: Vec3, b: Vec3) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    (vec![o, o + a, o + a + b, o + b], vec![[0, 1, 2], [0, 2, 3]])
}

fn join(parts: Vec<(Vec<Vec3>, Vec<[u32; 3]>)>) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let (mut p, mut t) = (Vec::new(), Vec::new());
    for (pp, tt) in parts {
        let base = p.len() as u32;
        p.extend(pp);
        t.extend(tt.into_iter().map(|q| q.map(|i| i + base)));
    }
    (p, t)
}

/// A box's six faces, normals out.
fn cuboid(lo: Vec3, hi: Vec3) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let d = hi - lo;
    let (x, y, z) = (Vec3::X * d.x, Vec3::Y * d.y, Vec3::Z * d.z);
    join(vec![
        quad(lo, z, y),
        quad(lo + x, y, z),
        quad(lo, x, z),
        quad(lo + y, z, x),
        quad(lo, y, x),
        quad(lo + z, x, y),
    ])
}

/// A .glb of the primitives, each its own material by name, with flat normals.
fn glb(prims: Vec<Prim>) -> Vec<u8> {
    let (mut bin, mut acc, mut views, mut ps, mut mats) = (Vec::<u8>::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut put = |bytes: Vec<u8>, a: Value, bin: &mut Vec<u8>| {
        views.push(json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len()}));
        bin.extend(bytes);
        let mut a = a;
        a["bufferView"] = json!(views.len() - 1);
        acc.push(a);
        acc.len() - 1
    };
    for (k, (name, p, t)) in prims.into_iter().enumerate() {
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        for tri in &t {
            let (a, b, c) = (p[tri[0] as usize], p[tri[1] as usize], p[tri[2] as usize]);
            let n = (b - a).cross(c - a).normalize();
            for v in [a, b, c] {
                pos.push(v);
                nrm.push(n);
            }
        }
        let (lo, hi) = pos.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(l, h), v| (l.min(*v), h.max(*v)));
        let f = |vs: &[Vec3]| vs.iter().flat_map(|v| [v.x, v.y, v.z]).flat_map(f32::to_le_bytes).collect::<Vec<u8>>();
        let pa = put(f(&pos), json!({"componentType": 5126, "type": "VEC3", "count": pos.len(), "min": lo.to_array(), "max": hi.to_array()}), &mut bin);
        let na = put(f(&nrm), json!({"componentType": 5126, "type": "VEC3", "count": nrm.len()}), &mut bin);
        let idx: Vec<u8> = (0..pos.len() as u32).flat_map(u32::to_le_bytes).collect();
        let ia = put(idx, json!({"componentType": 5125, "type": "SCALAR", "count": pos.len()}), &mut bin);
        ps.push(json!({"attributes": {"POSITION": pa, "NORMAL": na}, "indices": ia, "material": k}));
        mats.push(json!({"name": name}));
    }
    let j = json!({
        "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": ps}], "materials": mats, "accessors": acc, "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}],
    });
    raea_sys_glb::glb_bytes(&j, &bin)
}

/// Wall A (x 0-4) under a 1 m overhang at 2.5 m, wall B (x 20-24) open, both facing +z on a ground at y 0.
fn front() -> Vec<Prim> {
    let (gp, gt) = quad(Vec3::new(-40.0, 0.0, 20.0), Vec3::X * 80.0, Vec3::NEG_Z * 40.0);
    let (ap, at) = quad(Vec3::ZERO, Vec3::X * 4.0, Vec3::Y * 3.0);
    let (bp, bt) = quad(Vec3::new(20.0, 0.0, 0.0), Vec3::X * 4.0, Vec3::Y * 3.0);
    let (op, ot) = cuboid(Vec3::new(0.0, 2.5, 0.0), Vec3::new(4.0, 2.6, 1.0));
    vec![("ground", gp, gt), ("wall", ap, at), ("wall", bp, bt), ("slab", op, ot)]
}

fn env(v: Value) -> Env {
    Env::of(&v).unwrap()
}

fn classes() -> Classes {
    Classes::of(&json!({"ground": "ground", "*": "plaster"})).unwrap()
}

struct Baked {
    at: charts::Atlas,
    c0: Vec<[u8; 4]>,
    c2: Vec<[u8; 4]>,
    c3: Vec<[u8; 4]>,
}

fn run(prims: Vec<Prim>, e: &Env) -> Baked {
    let sc = scene::read(&glb(prims), &classes()).unwrap();
    let mesh = raea_sys_atlas::rays::mesh_of(&sc);
    let at = charts::build(&sc, &e.ask(), &mesh).unwrap();
    let n = (at.w * at.h) as usize;
    let (mut c0, mut c2, mut c3) = (vec![[0u8; 4]; n], vec![[0u8; 4]; n], vec![[0u8; 4]; n]);
    rays::causes(&sc, &at, e, &mesh, &mut c0, &mut c2, &mut c3);
    Baked { at, c0, c2, c3 }
}

impl Baked {
    /// The mean of a channel over texels whose point and normal pass `keep`.
    fn mean(&self, map: usize, ch: usize, keep: impl Fn(Vec3, Vec3) -> bool) -> f32 {
        let m = match map {
            0 => &self.c0,
            3 => &self.c3,
            _ => &self.c2,
        };
        let v: Vec<f32> = self.at.texels.iter().filter(|t| keep(t.p, t.n)).map(|t| m[t.y as usize * self.at.w as usize + t.x as usize][ch] as f32 / 255.0).collect();
        assert!(!v.is_empty(), "no texels pass the filter");
        v.iter().sum::<f32>() / v.len() as f32
    }
}

fn on_wall(p: Vec3, n: Vec3, x0: f32, x1: f32, y0: f32, y1: f32) -> bool {
    n.z > 0.99 && p.z.abs() < 0.01 && p.x > x0 && p.x < x1 && p.y > y0 && p.y < y1
}

#[test]
fn overhang_shelters_the_wall_under_it() {
    let e = env(json!({"density": 8, "rain": {"dir": [0, -1, -0.5], "spread": 20}, "rays": {"sky": 64}}));
    let b = run(front(), &e);
    let under = b.mean(0, 0, |p, n| on_wall(p, n, 0.5, 3.5, 1.6, 2.4));
    let open = b.mean(0, 0, |p, n| on_wall(p, n, 20.5, 23.5, 1.6, 2.4));
    assert!(open > 0.1, "an open wall facing the rain gets some: {open}");
    assert!(under < 0.4 * open, "under the overhang {under}, open {open}");
}

#[test]
fn ground_is_the_height_over_the_walk() {
    let e = env(json!({"density": 8}));
    let b = run(front(), &e);
    for y in [0.3f32, 1.0, 2.0, 2.9] {
        let g = b.mean(2, 0, |p, n| on_wall(p, n, 20.5, 23.5, y - 0.07, y + 0.07)) * 4.0;
        assert!((g - y).abs() < 0.1, "at {y} m the ground channel reads {g} m");
    }
    // the slab's top: the ray down passes through the slab (not a walking class) to the ground
    let top = b.mean(2, 0, |p, n| n.y > 0.99 && (p.y - 2.6).abs() < 0.01 && p.x > 1.0 && p.x < 3.0 && p.z > 0.2 && p.z < 0.8) * 4.0;
    assert!((top - 2.6).abs() < 0.05, "the slab's top reads {top} m");
}

#[test]
fn arrises_wear_and_a_wall_foot_does_not() {
    let e = env(json!({"density": 16}));
    let b = run(front(), &e);
    let arris = b.mean(0, 3, |p, n| n.y > 0.99 && (p.y - 2.6).abs() < 0.01 && p.z > 0.97 && p.x > 1.0 && p.x < 3.0);
    let middle = b.mean(0, 3, |p, n| on_wall(p, n, 21.0, 23.0, 1.0, 2.0));
    let foot = b.mean(0, 3, |p, n| on_wall(p, n, 21.0, 23.0, 0.0, 0.04));
    assert!(arris > 0.5, "the slab's front arris {arris}");
    assert!(middle == 0.0, "a wall's middle {middle}");
    assert!(foot < 0.05, "a wall's foot on the ground {foot}");
}

#[test]
fn a_south_wall_sees_the_december_sun_and_a_north_one_only_summers() {
    let (sp, st) = quad(Vec3::ZERO, Vec3::X * 4.0, Vec3::Y * 3.0);
    let (np, nt) = quad(Vec3::new(14.0, 0.0, -10.0), Vec3::NEG_X * 4.0, Vec3::Y * 3.0);
    let e = env(json!({"density": 4, "north": [0, 0, -1], "sun": {"lat": 52.5}, "walk": {"y": 0}}));
    let b = run(vec![("wall", sp, st), ("wall", np, nt)], &e);
    let (south, north) = (b.mean(3, 0, |_, n| n.z > 0.99), b.mean(3, 0, |_, n| n.z < -0.99));
    assert!(south > 0.3 && north < 0.02, "December: south {south}, north {north}");
    let (south, north) = (b.mean(0, 1, |_, n| n.z > 0.99), b.mean(0, 1, |_, n| n.z < -0.99));
    assert!(north > 0.0 && south > 3.0 * north, "the year: south {south}, north {north}");
}

#[test]
fn charts_keep_their_density_and_never_overlap() {
    let e = env(json!({"density": 8}));
    let b = run(front(), &e);
    let r = &b.at.rects;
    for (i, a) in r.iter().enumerate() {
        for c in &r[i + 1..] {
            let apart = a[0] + a[2] <= c[0] || c[0] + c[2] <= a[0] || a[1] + a[3] <= c[1] || c[1] + c[3] <= a[1];
            assert!(apart, "charts {a:?} and {c:?} overlap");
        }
    }
    // the open wall's chart: 4 m by 3 m at 8 texels a metre
    let wall = b.at.texels.iter().find(|t| on_wall(t.p, t.n, 20.5, 23.5, 1.0, 2.0)).unwrap();
    let rect = r[b.at.chart_of[wall.tri as usize] as usize];
    let across = (rect[2] - 2 * charts::GUTTER) as f32 / (4.0 * b.at.density);
    let down = (rect[3] - 2 * charts::GUTTER) as f32 / (3.0 * b.at.density);
    assert!((0.95..=1.05).contains(&across) && (0.95..=1.05).contains(&down), "density off: {across} {down}");
}

#[test]
fn a_bake_is_the_same_bytes_twice_and_adds_texcoord_1() {
    let tmp = std::env::temp_dir().join(format!("weathering-test-{}", std::process::id()));
    let src = tmp.join("front.glb");
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(&src, glb(front())).unwrap();
    let req = json!({"env": {"density": 4, "rays": {"sky": 8, "cavity": 4}}, "classes": {"ground": "ground"}});
    let a = bake(&tmp.join("a"), &src, &req).unwrap();
    let b = bake(&tmp.join("b"), &src, &req).unwrap();
    for k in ["file"] {
        assert_eq!(std::fs::read(a[k].as_str().unwrap()).unwrap(), std::fs::read(b[k].as_str().unwrap()).unwrap());
    }
    for m in ["causes0", "causes1", "causes2", "causes3"] {
        assert_eq!(std::fs::read(a["maps"][m].as_str().unwrap()).unwrap(), std::fs::read(b["maps"][m].as_str().unwrap()).unwrap(), "{m}");
    }
    assert_eq!(std::fs::read(a["spots"].as_str().unwrap()).unwrap(), std::fs::read(b["spots"].as_str().unwrap()).unwrap(), "spots");
    let out = std::fs::read(a["file"].as_str().unwrap()).unwrap();
    assert!(raea_sys_atlas::layout(&glb(front()), &req).is_ok_and(|l| l.glb == out && l.texels.len() as u64 == a["texels"]), "a layout alone is the bake's atlas");
    let sc = scene::read(&out, &Classes::default()).unwrap();
    assert_eq!(sc.prims.len(), 4);
    for m in sc.glb.json["meshes"].as_array().unwrap() {
        for p in m["primitives"].as_array().unwrap() {
            assert!(p["attributes"].get("TEXCOORD_1").is_some(), "a primitive without TEXCOORD_1");
            assert_eq!(p["attributes"]["TEXCOORD_0"], p["attributes"]["TEXCOORD_1"], "a primitive with no UV set of its own gets the atlas's as its first");
            assert!(p["attributes"].get("NORMAL").is_some(), "NORMAL was dropped");
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn each_texel_carries_its_class_and_the_gutters_a_real_one() {
    let cl = Classes::of(&json!({"ground": "ground", "slab": "iron", "*": "plaster"})).unwrap();
    let e = env(json!({"density": 4, "rays": {"sky": 8, "cavity": 4}}));
    let sc = scene::read(&glb(front()), &cl).unwrap();
    let mesh = raea_sys_atlas::rays::mesh_of(&sc);
    let at = charts::build(&sc, &e.ask(), &mesh).unwrap();
    let n = (at.w * at.h) as usize;
    let (mut c0, mut c2, mut c3) = (vec![[0u8; 4]; n], vec![[0u8; 4]; n], vec![[0u8; 4]; n]);
    rays::causes(&sc, &at, &e, &mesh, &mut c0, &mut c2, &mut c3);
    let codes = [scene::Class::Ground.code(), scene::Class::Iron.code(), scene::Class::Plaster.code()];
    assert_eq!(codes, [224, 96, 16], "the codes the shader decodes");
    for t in &at.texels {
        assert_eq!(c2[t.y as usize * at.w as usize + t.x as usize][3], sc.class[t.tri as usize].code(), "texel {},{}", t.x, t.y);
    }
    // the gutters copy a neighbour's class and element rather than averaging two into a third
    let mut covered = at.covered();
    let before = covered.iter().filter(|c| **c).count();
    maps::dilate(&mut c2, &mut covered, at.w, at.h, charts::GUTTER, &[1, 3]);
    assert!(covered.iter().filter(|c| **c).count() > before, "dilation filled no gutter");
    let elements: std::collections::HashSet<u8> = at.texels.iter().map(|t| c2[t.y as usize * at.w as usize + t.x as usize][1]).collect();
    for (i, px) in c2.iter().enumerate() {
        if covered[i] {
            assert!(codes.contains(&px[3]), "texel {i} has class code {}", px[3]);
            assert!(elements.contains(&px[1]), "texel {i} has element {}, no chart's", px[1]);
        }
    }
}

/// A bake of a real file, run by hand: RAEA_WEAR_SRC (a .glb), RAEA_WEAR_REQ (a request .json), RAEA_WEAR_STORE.
///   cargo test --release -p raea-sys-weathering -- --ignored bake_a_file --nocapture
#[test]
#[ignore]
fn bake_a_file() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} not set"));
    let req: Value = serde_json::from_str(&std::fs::read_to_string(var("RAEA_WEAR_REQ")).unwrap()).unwrap();
    let out = bake(Path::new(&var("RAEA_WEAR_STORE")), Path::new(&var("RAEA_WEAR_SRC")), &req).unwrap();
    println!("{out:#}");
}


#[test]
fn a_floor_shut_in_a_house_gets_no_texels() {
    let (fp, ft) = quad(Vec3::new(31.0, 0.01, -7.0), Vec3::X * 2.0, Vec3::NEG_Z * 2.0);
    let (hp, ht) = cuboid(Vec3::new(30.0, 0.0, -10.0), Vec3::new(34.0, 3.0, -6.0));
    let mut prims = front();
    prims.push(("house", hp, ht));
    prims.push(("floor", fp, ft));
    let e = env(json!({"density": 8}));
    let b = run(prims, &e);
    assert_eq!(b.at.hidden, 2, "the shut floor's two triangles, and nothing else");
    assert!(b.at.texels.iter().all(|t| !(t.n.y > 0.99 && t.p.y < 0.02 && t.p.y > 0.0 && t.p.x > 31.0 && t.p.x < 33.0 && t.p.z < -7.0 && t.p.z > -9.0)), "a texel on the shut floor");
}

#[test]
fn a_class_can_ask_its_own_density() {
    let e = env(json!({"density": {"*": 8, "ground": 2}}));
    let b = run(front(), &e);
    let chart = |keep: &dyn Fn(Vec3, Vec3) -> bool| {
        let t = b.at.texels.iter().find(|t| keep(t.p, t.n)).unwrap();
        b.at.rects[b.at.chart_of[t.tri as usize] as usize]
    };
    let g = 2 * charts::GUTTER;
    let ground = chart(&|p, n| n.y > 0.99 && p.y.abs() < 0.01);
    let wall = chart(&|p, n| on_wall(p, n, 20.5, 23.5, 1.0, 2.0));
    let (gw, gh) = ((ground[2] - g).max(ground[3] - g), (ground[2] - g).min(ground[3] - g));
    assert_eq!((gw, gh), (160, 80), "the 80 x 40 m ground at 2 texels a metre");
    assert_eq!((wall[2] - g, wall[3] - g), (32, 24), "the 4 x 3 m wall at 8");
}


#[test]
fn a_court_sees_less_sky_than_the_street() {
    // a 4 x 4 m court shut in by 12 m walls (normals in), beside the open ground
    let walls = join(vec![
        quad(Vec3::new(30.0, 0.0, -14.0), Vec3::X * 4.0, Vec3::Y * 12.0),
        quad(Vec3::new(34.0, 0.0, -10.0), Vec3::NEG_X * 4.0, Vec3::Y * 12.0),
        quad(Vec3::new(30.0, 0.0, -10.0), Vec3::NEG_Z * 4.0, Vec3::Y * 12.0),
        quad(Vec3::new(34.0, 0.0, -14.0), Vec3::Z * 4.0, Vec3::Y * 12.0),
    ]);
    let mut prims = front();
    prims.push(("court", walls.0, walls.1));
    let b = run(prims, &env(json!({"density": 8, "rays": {"cavity": 32}})));
    let court = b.mean(2, 2, |p, n| n.y > 0.99 && p.y.abs() < 0.01 && p.x > 31.0 && p.x < 33.0 && p.z > -13.0 && p.z < -11.0);
    let street = b.mean(2, 2, |p, n| n.y > 0.99 && p.y.abs() < 0.01 && p.x > 10.0 && p.x < 16.0 && p.z > 5.0 && p.z < 15.0);
    assert!(street > 0.9, "the open ground sees the sky: {street}");
    assert!(court < 0.35, "a deep court's floor sees little: {court}");
}

#[test]
fn a_door_is_held_on_one_side_and_gets_its_nameplate() {
    let wall = |x: f32, y: f32, w: f32, h: f32| quad(Vec3::new(x, y, 0.0), Vec3::X * w, Vec3::Y * h);
    let (gp, gt) = quad(Vec3::new(-10.0, 0.0, 10.0), Vec3::X * 20.0, Vec3::NEG_Z * 20.0);
    let (wp, wt) = join(vec![wall(-4.0, 0.0, 3.45, 4.0), wall(0.55, 0.0, 3.45, 4.0), wall(-0.55, 2.3, 1.1, 1.7)]);
    let (dp, dt) = cuboid(Vec3::new(-0.5, 0.0, -0.3), Vec3::new(0.5, 2.25, -0.25));
    let prims = vec![("ground", gp, gt), ("wall", wp, wt), ("door", dp, dt)];
    let classes = Classes::of(&json!({"ground": "ground", "door": "wood", "*": "plaster"})).unwrap();
    let req = json!({"env": {"density": {"*": 4, "wood": 20}, "rays": {"sky": 8, "cavity": 4}}, "classes": {"ground": "ground", "door": "wood"}});
    let e = env(req["env"].clone());
    let sc = scene::read(&glb(prims.clone()), &classes).unwrap();
    let mesh = raea_sys_atlas::rays::mesh_of(&sc);
    let at = charts::build(&sc, &e.ask(), &mesh).unwrap();
    let n = (at.w * at.h) as usize;
    let (mut c0, mut c1, mut c2) = (vec![[0u8; 4]; n], vec![[0u8; 4]; n], vec![[0u8; 4]; n]);
    let mut c3 = vec![[0u8; 4]; n];
    rays::causes(&sc, &at, &e, &mesh, &mut c0, &mut c2, &mut c3);
    flow::flow(&sc, &at, &e, &mesh, &c0, &mut c1);
    let (_, crack) = touch::touch(&sc, &at, &e, &mesh, &c0, &mut c1, &c2);
    assert_eq!(crack.len(), at.texels.len());
    let touched = |keep: &dyn Fn(Vec3, Vec3) -> bool| at.texels.iter().any(|t| keep(t.p, t.n) && c1[t.y as usize * at.w as usize + t.x as usize][3] > 128);
    let held = |side: f32| touched(&|p, n| n.z > 0.99 && (p.z + 0.25).abs() < 0.01 && (0.97..1.08).contains(&p.y) && p.x * side > 0.25);
    assert!(held(1.0) != held(-1.0), "the handle band on one side of the leaf only");
    assert!(!touched(&|p, n| n.z > 0.99 && p.z.abs() < 0.01 && p.x.abs() > 0.6), "nothing on the wall");
    let tmp = std::env::temp_dir().join(format!("weathering-door-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("door.glb"), glb(prims)).unwrap();
    let out = bake(&tmp.join("store"), &tmp.join("door.glb"), &req).unwrap();
    let spots: Value = serde_json::from_str(&std::fs::read_to_string(out["spots"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(spots.as_array().unwrap().iter().filter(|s| s["what"] == "nameplate").count(), 1, "{spots}");
    // the plaster's foot sheds: debris in front of it, each with its loss potential (raea-sys-damage)
    assert!(spots.as_array().unwrap().iter().any(|s| s["what"] == "debris" && s["loss"].as_f64().is_some_and(|l| l >= 0.5)), "{spots}");
    let _ = std::fs::remove_dir_all(&tmp);
}
