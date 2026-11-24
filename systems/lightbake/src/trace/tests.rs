use super::*;
use crate::texels::tests::{cuboid, glb, set};
use crate::texels::{request, texels};

fn world(mesh: &parry3d::shape::TriMesh, surfaces: &[Surface]) -> (Vec<[f32; 3]>, Vec<[u32; 3]>, Vec<Surface>) {
    (mesh.vertices().iter().map(|v| v.to_array()).collect(), mesh.indices().to_vec(), surfaces.to_vec())
}

/// The light on t's texels, each triangle's surface given by its centroid.
fn run(t: &crate::texels::Texels, surface: impl Fn([f32; 3]) -> Surface, s: &Settings) -> Vec<[f32; 3]> {
    let v = t.mesh.vertices();
    let surfaces: Vec<Surface> = t.mesh.indices().iter().map(|i| surface(std::array::from_fn(|k| i.iter().map(|&j| v[j as usize][k]).sum::<f32>() / 3.0))).collect();
    let (p, i, s2) = world(&t.mesh, &surfaces);
    let normal: Vec<[f32; 3]> = t.layout.texels.iter().map(|x| x.n).collect();
    let starts = Starts { origin: &t.origin, normal: &normal, inside: &t.inside, offset: t.offset };
    trace(&World { positions: &p, triangles: &i, surfaces: &s2 }, &starts, s, &mut |_, _| {}).unwrap()
}

/// A white set under a white sky: whatever bounces, every texel's light is the sky's (a white furnace). The block sunk
/// in the slab and the box set by its node make real concave corners; paths there bounce until they find the sky.
#[test]
fn a_white_furnace_is_the_sky_at_every_texel() {
    let t = texels(&set(), &request(8.0, 1024)).unwrap();
    let s = Settings { samples: 64, bounces: 64, per_pass: 16, sky: [1.0; 3] };
    let light = run(&t, |_| Surface { albedo: [1.0; 3], emission: [0.0; 3] }, &s);
    let open: Vec<[f32; 3]> = light.iter().zip(&t.inside).filter(|(_, i)| !**i).map(|(l, _)| *l).collect();
    let off = open.iter().filter(|l| l.iter().any(|c| (c - 1.0).abs() > 0.01)).count();
    println!("furnace: {off} of {} off by more than 1%", open.len());
    let worst = open.iter().flatten().fold(0.0f32, |m, c| m.max((c - 1.0).abs()));
    println!("furnace: worst {worst}, mean {}", open.iter().map(|l| l[0]).sum::<f32>() / open.len() as f32);
    assert!(open.len() > 500 && off == 0, "{off} of {} texels more than 1% off the sky, worst {worst}", open.len());
}

/// A black wall 40 m long and 2 m high on a black floor under a white sky: the floor's light at a distance d from its
/// foot is what the sky leaves, 1/2 (1 + d / sqrt(d^2 + h^2)) (an endless wall's form factor), which only cosine-weighted
/// rays give.
#[test]
fn a_walls_shadow_on_the_floor_is_its_form_factor() {
    let set = glb(&[(cuboid([0.0, -0.1, -20.0], [6.0, 0.0, 20.0]), [0.0; 3]), (cuboid([-0.2, -0.1, -20.0], [0.0, 2.0, 20.0]), [0.0; 3])]);
    let t = texels(&set, &request(4.0, 1024)).unwrap();
    let light = run(&t, |_| Surface { albedo: [0.0; 3], emission: [0.0; 3] }, &Settings { samples: 512, ..Settings::default() });
    for d in [0.5f32, 1.0, 2.0, 4.0] {
        let v: Vec<(f32, f32)> = t.layout.texels.iter().zip(&light).filter(|(x, _)| x.p[1].abs() < 1e-3 && x.n[1] > 0.9 && x.p[2].abs() < 4.0 && (x.p[0] - d).abs() < 0.13).map(|(x, l)| (0.5 * (1.0 + x.p[0] / (x.p[0] * x.p[0] + 4.0).sqrt()), l[0])).collect();
        let (want, got) = v.iter().fold((0.0, 0.0), |(a, b), (w, g)| (a + w / v.len() as f32, b + g / v.len() as f32));
        println!("wall: d {d} m: {got:.4} for {want:.4} over {} texels", v.len());
        assert!(v.len() > 20 && (got - want).abs() < 0.01, "{d} m from the wall the floor gets {got}, not {want}");
    }
}

/// A closed box glowing 1 all round with an albedo of a half: every point inside gets 1 + 1/2 + 1/4 ... = 2.
#[test]
fn a_glowing_box_adds_up_its_bounces() {
    let inward = cuboid([-1.0; 3], [1.0; 3]).into_iter().map(|[a, b, c]| [a, c, b]).collect();
    let t = texels(&glb(&[(inward, [0.0; 3])]), &request(8.0, 1024)).unwrap();
    let light = run(&t, |_| Surface { albedo: [0.5; 3], emission: [1.0; 3] }, &Settings { samples: 64, bounces: 64, per_pass: 16, sky: [0.0; 3] });
    let mean = light.iter().zip(&t.inside).filter(|(_, i)| !**i).map(|(l, _)| l[0]).sum::<f32>() / t.inside.iter().filter(|i| !**i).count() as f32;
    println!("box: {mean:.4} for 2, {} of {} texels marked inside", t.inside.iter().filter(|i| **i).count(), t.inside.len());
    assert!((mean - 2.0).abs() < 0.01, "the glowing box's light is {mean}, not 2");
}

/// A white floor beside a red wall: the floor near the wall gets redder than the floor far from it.
#[test]
fn a_red_wall_bleeds_onto_the_floor() {
    let floor = cuboid([-1.0, -0.1, -2.0], [1.0, 0.0, 2.0]);
    let wall = cuboid([-1.0, 0.0, -2.0], [-0.9, 2.0, 2.0]);
    let t = texels(&glb(&[(floor, [0.0; 3]), (wall, [0.0; 3])]), &request(16.0, 1024)).unwrap();
    let red = |c: [f32; 3]| Surface { albedo: if c[1] > 0.5 { [0.9, 0.0, 0.0] } else { [0.9; 3] }, emission: [0.0; 3] };
    let light = run(&t, red, &Settings { samples: 128, bounces: 4, per_pass: 32, sky: [1.0; 3] });
    // floor top texels near the wall (x -0.9..-0.7) and far (x > 0.6)
    let mean = |near: bool| {
        let v: Vec<f32> = t.layout.texels.iter().zip(&light).zip(&t.inside).filter(|((x, _), i)| !**i && x.p[1].abs() < 1e-3 && x.n[1] > 0.9 && if near { x.p[0] < -0.7 } else { x.p[0] > 0.6 }).map(|((_, l), _)| l[1] / l[0]).collect();
        v.iter().sum::<f32>() / v.len() as f32
    };
    let (near, far) = (mean(true), mean(false));
    assert!(near < far * 0.97, "green over red near the red wall {near}, far from it {far}");
}

#[test]
fn the_same_bake_gives_the_same_bytes() {
    let t = texels(&set(), &request(8.0, 1024)).unwrap();
    let s = Settings { samples: 16, per_pass: 8, ..Settings::default() };
    let surf = |c: [f32; 3]| Surface { albedo: [0.5 + 0.1 * c[0].abs().fract(); 3], emission: [0.0; 3] };
    let (a, b) = (run(&t, surf, &s), run(&t, surf, &s));
    assert!(a.iter().any(|l| l[0] > 0.1), "no light at all");
    assert_eq!(a, b);
}

#[test]
fn mismatched_inputs_are_refused() {
    let starts = Starts { origin: &[[0.0; 3]], normal: &[], inside: &[], offset: 1e-4 };
    assert!(trace(&World { positions: &[], triangles: &[], surfaces: &[] }, &starts, &Settings::default(), &mut |_, _| {}).is_err());
}

/// The measured run (docs/bake.feature BK3): RAEA_BAKE_GLB at 64 texels a metre, every surface 0.7 grey under a white
/// sky, RAEA_BAKE_SAMPLES (default 512) a texel: the trace's time alone.
/// `cargo test -p raea-sys-lightbake trace::tests::measured -- --ignored --nocapture`
#[test]
#[ignore]
fn measured() {
    let glb = std::fs::read(std::env::var("RAEA_BAKE_GLB").expect("RAEA_BAKE_GLB: a .glb")).unwrap();
    let samples = std::env::var("RAEA_BAKE_SAMPLES").ok().and_then(|v| v.parse().ok()).unwrap_or(512);
    let t = texels(&glb, &request(64.0, 4096)).unwrap();
    let t0 = std::time::Instant::now();
    let light = run(&t, |_| Surface { albedo: [0.7; 3], emission: [0.0; 3] }, &Settings { samples, ..Settings::default() });
    let lit = light.iter().zip(&t.inside).filter(|(_, i)| !**i).map(|(l, _)| l[0]).collect::<Vec<_>>();
    println!("{} texels, {} triangles, {samples} samples, 4 bounces: {:.2} s, mean light {:.3}", light.len(), t.mesh.indices().len(), t0.elapsed().as_secs_f64(), lit.iter().sum::<f32>() / lit.len() as f32);
}
