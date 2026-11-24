use super::*;
use crate::seat_facing;

fn part(name: &str, c: Vec3, s: Vec3) -> Part {
    let t = raea_core_shape::cached(&format!("box({}, {}, {})", s.x, s.y, s.z)).unwrap();
    let tris: Vec<[Vec3; 3]> = t.positions.chunks_exact(3).map(|p| [Vec3::from(p[0]) + c, Vec3::from(p[1]) + c, Vec3::from(p[2]) + c]).collect();
    let n = tris.len();
    Part { label: name.into(), tris, node: vec![0; n], nodes: vec![String::new()], lo: c - s / 2.0, hi: c + s / 2.0, excused: false, moving: false }
}

fn rest(parts: &[Part], i: usize) -> Rest {
    rest_of(parts, &Grid::new(parts), &insets(parts), i)
}

#[test]
fn seat_faces_away_from_its_back() {
    let boxed = |c: Vec3, sz: Vec3| part("x", c, sz).tris.into_iter().flatten().collect::<Vec<_>>();
    let legs: Vec<Vec3> = [(-0.2, -0.2), (0.2, -0.2), (-0.2, 0.2), (0.2, 0.2)].iter().flat_map(|&(x, z)| boxed(Vec3::new(x, 0.22, z), Vec3::new(0.03, 0.44, 0.03))).collect();
    let seat = boxed(Vec3::new(0.0, 0.45, 0.0), Vec3::new(0.44, 0.03, 0.44));
    // a chair, its back at -z: faces +z, its seat 0.465 up
    let chair = [legs.clone(), seat.clone(), boxed(Vec3::new(0.0, 0.7, -0.21), Vec3::new(0.44, 0.45, 0.03))].concat();
    let (f, h, _) = seat_facing(&chair).unwrap();
    assert_eq!(f, [0.0, 1.0]);
    assert!((h - 0.465).abs() < 0.03, "{h}");
    // turned: its back at +x faces -x
    let turned: Vec<Vec3> = chair.iter().map(|p| Vec3::new(-p.z, p.y, p.x)).collect();
    assert!(seat_facing(&turned).unwrap().0[0] < -0.99);
    // a back with one high end (a settle's) still faces straight out
    let settle = [chair.clone(), boxed(Vec3::new(-0.2, 0.6, 0.05), Vec3::new(0.03, 0.25, 0.3))].concat();
    assert_eq!(seat_facing(&settle).unwrap().0, [0.0, 1.0]);
    // a backless stool faces any way
    assert!(seat_facing(&[legs, seat].concat()).is_none());
}

#[test]
fn sits_floats_sinks_hangs() {
    let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
    let wall = part("wall", Vec3::new(0.0, 1.0, -1.05), Vec3::new(4.0, 2.0, 0.1));
    let mug = |y: f32| part("mug", Vec3::new(0.3, y + 0.05, 0.2), Vec3::new(0.08, 0.1, 0.08));
    // on it
    let r = rest(&[floor, mug(0.0)], 1);
    assert!(matches!(r, Rest::On { under: 0, gap, .. } if gap.abs() < 1e-4), "{r:?}");
    // 3.5 mm above it
    let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
    let r = rest(&[floor, mug(0.0035)], 1);
    assert!(matches!(r, Rest::On { gap, .. } if (gap - 0.0035).abs() < 1e-4), "{r:?}");
    // 6 mm in
    let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
    let r = rest(&[floor, mug(-0.006)], 1);
    assert!(matches!(r, Rest::On { gap, .. } if (gap + 0.006).abs() < 1e-4), "{r:?}");
    // the floor is the ground
    let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
    assert!(matches!(rest(&[floor], 0), Rest::Ground));
    // a shelf on the wall, 1.2 m up: mounted
    let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
    let shelf = part("shelf", Vec3::new(0.0, 1.2, -0.9), Vec3::new(0.6, 0.03, 0.2));
    let r = rest(&[floor, wall, shelf], 2);
    assert!(matches!(r, Rest::Mounted { on: 1, .. }), "{r:?}");
    // and moved 2 cm off the wall: in the air
    let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
    let wall = part("wall", Vec3::new(0.0, 1.0, -1.05), Vec3::new(4.0, 2.0, 0.1));
    let shelf = part("shelf", Vec3::new(0.0, 1.2, -0.88), Vec3::new(0.6, 0.03, 0.2));
    assert!(matches!(rest(&[floor, wall, shelf], 2), Rest::Air { below: Some((0, _, _)) }));
}

#[test]
fn set_into_a_wall_and_hung_on_a_carcass() {
    let floor = || part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
    let wall = || part("wall", Vec3::new(0.0, 1.2, -1.0), Vec3::new(4.0, 2.4, 0.2));
    // a sill set 5 cm into the wall, 1 m up, 4 cm over a cupboard top: mounted on the wall, not floating
    let cupboard = part("cupboard", Vec3::new(0.0, 0.47, -0.6), Vec3::new(1.0, 0.94, 0.6));
    let sill = part("sill", Vec3::new(0.0, 0.99, -0.87), Vec3::new(1.0, 0.02, 0.16));
    let r = rest(&[floor(), wall(), cupboard, sill], 3);
    assert!(matches!(r, Rest::Mounted { on: 1, .. }), "{r:?}");
    // a lintel of the wall over a window set into it: neither sunk into the other
    let window = part("window", Vec3::new(0.0, 1.5, -1.0), Vec3::new(1.0, 1.0, 0.1));
    let lintel = part("lintel", Vec3::new(0.0, 2.2, -1.0), Vec3::new(1.2, 0.42, 0.2));
    let ps = [floor(), wall(), window, lintel];
    assert!(!matches!(rest(&ps, 3), Rest::On { gap, .. } if gap < -TOUCH), "{:?}", rest(&ps, 3));
    // a cupboard door 2 cm in front of its carcass, 10 cm off the floor: hung on it
    let carcass = part("carcass", Vec3::new(0.0, 0.45, 0.0), Vec3::new(0.6, 0.9, 0.5));
    let door = part("door", Vec3::new(0.0, 0.5, 0.26 + 0.02), Vec3::new(0.56, 0.7, 0.02));
    let r = rest(&[floor(), carcass, door], 2);
    assert!(matches!(r, Rest::Mounted { on: 1, .. }), "{r:?}");
}

#[test]
fn a_magnet_on_its_paper_on_a_door() {
    // a door's face at z 0.3, a paper on it, a magnet on the paper straddling its top edge: rays straight down
    // meet the paper's top edge in the magnet's foot; it's mounted on the paper, not sunk into it
    let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(4.0, 0.1, 4.0));
    let door = part("door", Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.7, 1.6, 0.6));
    let paper = part("paper", Vec3::new(0.0, 1.5, 0.3005), Vec3::new(0.2, 0.28, 0.001));
    let magnet = part("magnet", Vec3::new(0.0, 1.635, 0.3015), Vec3::new(0.04, 0.04, 0.002));
    let r = rest(&[floor, door, paper, magnet], 3);
    assert!(matches!(r, Rest::Mounted { on: 2, .. } | Rest::Mounted { on: 1, .. }), "{r:?}");
}

#[test]
fn a_mug_on_its_side_rests_on_its_lowest_edge() {
    // a cylinder lying along x on the island
    let top = part("island", Vec3::new(0.0, 0.98, 0.0), Vec3::new(1.0, 0.04, 1.0));
    // manifold's cylinder stands along z: lying, once y is up; its lowest point set 1 mm above the stone
    let t = raea_core_shape::cached("cylinder(0.04, 0.1)").unwrap();
    let low = t.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    let c = Vec3::new(0.0, 1.001 - low, 0.0);
    let tris: Vec<[Vec3; 3]> = t.positions.chunks_exact(3).map(|p| [Vec3::from(p[0]) + c, Vec3::from(p[1]) + c, Vec3::from(p[2]) + c]).collect();
    let n = tris.len();
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for q in tris.iter().flatten() {
        lo = lo.min(*q);
        hi = hi.max(*q);
    }
    let mug = Part { label: "mug".into(), tris, node: vec![0; n], nodes: vec![String::new()], lo, hi, excused: false, moving: false };
    let r = rest(&[top, mug], 1);
    assert!(matches!(r, Rest::On { under: 0, gap, .. } if (gap - 0.001).abs() < 3e-4), "{r:?}");
}

#[test]
fn a_stool_tucked_under_a_table_rests_on_the_floor_by_its_feet() {
    // docs/make.feature MK7: its box reaches over the table's top; its feet are on the floor, and that's what it rests on
    let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(10.0, 0.1, 10.0));
    let top = part("table top", Vec3::new(0.0, 0.75, 0.0), Vec3::new(1.0, 0.04, 1.0));
    let stool = part("stool", Vec3::new(0.5, 0.45, 0.0), Vec3::new(0.4, 0.9, 0.4));
    let r = rest(&[floor, top, stool], 2);
    assert!(matches!(r, Rest::On { under: 0, gap, .. } if gap.abs() < 1e-4), "{r:?}");
}

#[test]
fn a_part_moved_sideways_is_settled_where_it_lands() {
    // docs/make.feature MK9: a stool on the table top pushed off its edge drops to the floor in the same move
    let floor = part("floor", Vec3::new(0.0, -0.05, 0.0), Vec3::new(10.0, 0.1, 10.0));
    let top = part("table top", Vec3::new(0.0, 0.75, 0.0), Vec3::new(1.0, 0.04, 1.0));
    let stool = part("stool", Vec3::new(0.2, 0.92, 0.0), Vec3::new(0.3, 0.3, 0.3));
    let parts = [floor, top, stool];
    let ins = insets(&parts);
    assert!(matches!(rest(&parts, 2), Rest::On { under: 1, .. }));
    let r = rest_after(&parts, &ins, 2, Vec3::new(1.2, 0.0, 0.0));
    assert!(matches!(landing(&r), Some((0, _, gap)) if (gap - 0.77).abs() < 1e-3), "{r:?}");
    // moved along the top it stays on it
    assert!(matches!(rest_after(&parts, &ins, 2, Vec3::new(-0.3, 0.0, 0.0)), Rest::On { under: 1, .. }));
}
