//! A row's mesh from JSON: one of Bevy's primitive shapes by name, meshed by Bevy's own meshers (rings and extrusions
//! of its 2D ones too), or a buffer mesh's own vertex buffers. raea-sys-material re-exports it as its `mesh`.

use bevy::prelude::*;

mod attribute;
mod indexed;
pub use attribute::{attribute, with_attributes};
pub use indexed::buffer_indexed;

/// A <BufferMesh>'s vertex colours (`colors`: r, g, b, a in linear light for each vertex, or "#rrggbb" sRGB each),
/// laid out per triangle corner as buffer_tris lays its positions; None without them. StandardMaterial multiplies its
/// base colour by them (Bevy's VERTEX_COLORS).
pub fn buffer_colors(json: &str) -> Result<Option<Vec<[f32; 4]>>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("a buffer mesh isn't JSON: {e}"))?;
    let n = v.get("positions").and_then(|p| p.as_array()).map(|p| p.len() / 3).unwrap_or(0);
    Ok(vertex_colors(&v)?.map(|per| buffer_corners(&v, n).iter().map(|&i| per[i.min(n.saturating_sub(1))]).collect()))
}

/// A buffer mesh's colours, one for each vertex (None without them).
fn vertex_colors(v: &serde_json::Value) -> Result<Option<Vec<[f32; 4]>>, String> {
    let Some(c) = v.get("colors").filter(|c| !c.is_null()) else { return Ok(None) };
    let a = c.as_array().ok_or("a buffer mesh's colors: an array")?;
    let n = v.get("positions").and_then(|p| p.as_array()).map(|p| p.len() / 3).unwrap_or(0);
    let per: Vec<[f32; 4]> = if a.iter().all(|x| x.is_string()) {
        a.iter().map(|x| {
            let s = x.as_str().unwrap();
            Srgba::hex(s.trim_start_matches('#')).map(|c| LinearRgba::from(c).to_f32_array()).map_err(|e| format!("a buffer mesh's colour {s:?}: {e}"))
        }).collect::<Result<_, _>>()?
    } else {
        let f: Vec<f32> = a.iter().map(|x| x.as_f64().map(|f| f as f32).ok_or_else(|| format!("a buffer mesh's colors: numbers or \"#rrggbb\", got {x}"))).collect::<Result<_, _>>()?;
        if f.len() != n * 4 {
            return Err(format!("a buffer mesh's colors: r, g, b, a for each of its {n} vertices, got {} numbers", f.len()));
        }
        f.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
    };
    if per.len() != n {
        return Err(format!("a buffer mesh's colors: one for each of its {n} vertices, got {}", per.len()));
    }
    Ok(Some(per))
}

/// One of Bevy's primitive shapes as Bevy's mesher makes it, its reflected fields given over its default: {primitive:
/// "Torus", minor_radius: 0.2, major_radius: 1}. 3D: Sphere (`uv: [sectors, stacks]` or `ico: n` for its kind, ico 5
/// by default), Cuboid, Cylinder, Capsule3d, Cone, ConicalFrustum, Torus, Plane3d (`subdivisions: n`), Tetrahedron, Triangle3d, Segment3d,
/// Polyline3d. 2D (flat, facing +Z): Circle, CircularSector, CircularSegment, Rectangle, RegularPolygon, Ellipse,
/// Annulus, Rhombus, Capsule2d, Triangle2d, ConvexPolygon, Segment2d, Polyline2d (`vertices` for the last three). A 2D
/// shape given `inner` (the same shape's fields) is Bevy's Ring of the two, or given `ring: thickness` its to_ring (the
/// shapes with Bevy's Inset); {primitive: "Extrusion", base: <a 2D shape, or ring>, half_depth} is Bevy's Extrusion.
pub fn primitive_mesh(o: &serde_json::Map<String, serde_json::Value>) -> Result<Mesh, String> {
    use bevy::mesh::Extrudable;
    use bevy::shape::*;
    type Obj = serde_json::Map<String, serde_json::Value>;
    fn fill<T: Default + bevy::reflect::PartialReflect>(name: &str, o: &Obj) -> Result<T, String> {
        let mut x = T::default();
        for (k, v) in o.iter().filter(|(k, _)| !["primitive", "inner", "ring", "uv", "ico", "subdivisions"].contains(&k.as_str())) {
            let bevy::reflect::ReflectMut::Struct(st) = x.reflect_mut() else { return Err(format!("{name} has no fields")) };
            let f = st.field_mut(k).ok_or_else(|| format!("{name} has no field {k:?}"))?;
            apply_json(f, v).map_err(|e| format!("{name}.{k}: {e}"))?;
        }
        Ok(x)
    }
    fn points<const N: usize>(name: &str, o: &Obj) -> Result<Vec<[f32; N]>, String> {
        let bad = || format!("{name}.vertices: a list of [x, y{}] points", if N == 3 { ", z" } else { "" });
        let a = o.get("vertices").and_then(|v| v.as_array()).ok_or_else(bad)?;
        a.iter()
            .map(|p| {
                let p = p.as_array().filter(|p| p.len() == N).ok_or_else(bad)?;
                let mut out = [0f32; N];
                for (i, x) in p.iter().enumerate() {
                    out[i] = x.as_f64().ok_or_else(bad)? as f32;
                }
                Ok(out)
            })
            .collect()
    }
    /// a 2D shape flat or extruded (`depth`), alone or as a ring
    fn flat<P>(o: &Obj, make: &dyn Fn(&Obj) -> Result<P, String>, inset: Option<fn(P, f32) -> P>, depth: Option<f32>) -> Result<Mesh, String>
    where
        P: Primitive2d + Meshable + Clone,
        P::Output: Extrudable,
    {
        let shape = make(o)?;
        let inner = match (o.get("inner"), o.get("ring").and_then(|t| t.as_f64())) {
            (Some(serde_json::Value::Object(i)), _) => Some(make(i)?),
            (_, Some(t)) => Some(inset.ok_or("ring: a thickness is Bevy's to_ring, for the shapes with Inset (Circle, CircularSegment, Rectangle, RegularPolygon, Rhombus, Capsule2d, Triangle2d); give inner instead")?(shape.clone(), t as f32)),
            _ => None,
        };
        Ok(match (inner, depth) {
            (None, None) => shape.mesh().build(),
            (None, Some(h)) => Extrusion { base_shape: shape, half_depth: h }.mesh().build(),
            (Some(i), None) => Ring::new(shape, i).mesh().build(),
            (Some(i), Some(h)) => Extrusion { base_shape: Ring::new(shape, i), half_depth: h }.mesh().build(),
        })
    }
    fn two_d(o: &Obj, depth: Option<f32>) -> Result<Option<Mesh>, String> {
        let name = o.get("primitive").and_then(|p| p.as_str()).unwrap_or("");
        macro_rules! inset {
            ($($t:ident),*) => { match name { $(stringify!($t) => return flat::<$t>(o, &|o| fill::<$t>(name, o), Some(|p: $t, t| p.inset(t)), depth).map(Some),)* _ => {} } };
        }
        macro_rules! plain {
            ($($t:ident),*) => { match name { $(stringify!($t) => return flat::<$t>(o, &|o| fill::<$t>(name, o), None, depth).map(Some),)* _ => {} } };
        }
        inset!(Circle, CircularSegment, Rectangle, RegularPolygon, Rhombus, Capsule2d, Triangle2d);
        plain!(CircularSector, Ellipse, Annulus);
        if name == "ConvexPolygon" {
            let make = |o: &Obj| ConvexPolygon::new(points::<2>(name, o)?.into_iter().map(Vec2::from)).map_err(|e| format!("ConvexPolygon: {e}"));
            return flat(o, &make, None, depth).map(Some);
        }
        Ok(None)
    }
    let name = o.get("primitive").and_then(|p| p.as_str()).unwrap_or("");
    if name == "Extrusion" {
        let Some(serde_json::Value::Object(base)) = o.get("base") else { return Err("Extrusion: base is a 2D shape ({ primitive: \"Circle\", radius: 1 })".into()) };
        let h = o.get("half_depth").and_then(|h| h.as_f64()).unwrap_or(0.5) as f32;
        return two_d(base, Some(h))?.ok_or_else(|| format!("Extrusion: no extrudable 2D shape {:?}", base.get("primitive")));
    }
    if let Some(m) = two_d(o, None)? {
        return Ok(m);
    }
    match name {
        "Sphere" => {
            let s = fill::<Sphere>(name, o)?;
            match (o.get("uv").and_then(|u| u.as_array()), o.get("ico").and_then(|i| i.as_u64())) {
                (Some(u), _) if u.len() == 2 => Ok(s.mesh().uv(u[0].as_u64().unwrap_or(32) as u32, u[1].as_u64().unwrap_or(18) as u32)),
                (_, Some(n)) => s.mesh().ico(n as u32).map_err(|e| format!("Sphere: {e}")),
                _ => Ok(s.into()),
            }
        }
        "Cuboid" => Ok(fill::<Cuboid>(name, o)?.into()),
        "Cylinder" => Ok(fill::<Cylinder>(name, o)?.into()),
        "Capsule3d" => Ok(fill::<Capsule3d>(name, o)?.into()),
        "Cone" => Ok(fill::<Cone>(name, o)?.into()),
        "ConicalFrustum" => Ok(fill::<ConicalFrustum>(name, o)?.into()),
        "Torus" => Ok(fill::<Torus>(name, o)?.into()),
        "Plane3d" => {
            let p = fill::<Plane3d>(name, o)?;
            Ok(match o.get("subdivisions").and_then(|n| n.as_u64()) {
                Some(n) => p.mesh().subdivisions(n as u32).build(),
                None => p.into(),
            })
        }
        "Tetrahedron" => Ok(fill::<Tetrahedron>(name, o)?.into()),
        "Triangle3d" => Ok(fill::<Triangle3d>(name, o)?.into()),
        "Segment3d" => Ok(fill::<Segment3d>(name, o)?.into()),
        "Segment2d" => Ok(fill::<Segment2d>(name, o)?.into()),
        "Polyline3d" => Ok(Polyline3d::new(points::<3>(name, o)?.into_iter().map(Vec3::from)).into()),
        "Polyline2d" => Ok(Polyline2d::new(points::<2>(name, o)?.into_iter().map(Vec2::from)).into()),
        _ => Err(format!("no primitive {name:?} (see primitive_mesh: Bevy's 3D and 2D primitives, Ring and Extrusion)")),
    }
}

/// A JSON value onto a reflected field: a number onto f32/f64/u32/usize/i32, [x, y(, z)] onto Vec2/Vec3/Dir2/Dir3,
/// an object onto a struct's fields (an Annulus's inner_circle).
pub fn apply_json(f: &mut dyn bevy::reflect::PartialReflect, v: &serde_json::Value) -> Result<(), String> {
    let any = f.try_as_reflect_mut().map(|r| r.as_any_mut());
    if let Some(any) = any {
        let n = v.as_f64();
        let arr: Option<Vec<f32>> = v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN) as f32).collect());
        if let (Some(x), Some(n)) = (any.downcast_mut::<f32>(), n) { *x = n as f32; return Ok(()); }
        if let (Some(x), Some(n)) = (any.downcast_mut::<f64>(), n) { *x = n; return Ok(()); }
        if let (Some(x), Some(n)) = (any.downcast_mut::<u32>(), n) { *x = n as u32; return Ok(()); }
        if let (Some(x), Some(n)) = (any.downcast_mut::<usize>(), n) { *x = n as usize; return Ok(()); }
        if let (Some(x), Some(n)) = (any.downcast_mut::<i32>(), n) { *x = n as i32; return Ok(()); }
        if let Some(a) = &arr {
            if let (Some(x), 2) = (any.downcast_mut::<Vec2>(), a.len()) { *x = Vec2::new(a[0], a[1]); return Ok(()); }
            if let (Some(x), 3) = (any.downcast_mut::<Vec3>(), a.len()) { *x = Vec3::new(a[0], a[1], a[2]); return Ok(()); }
            if let (Some(x), 3) = (any.downcast_mut::<Dir3>(), a.len()) { *x = Dir3::new(Vec3::new(a[0], a[1], a[2])).map_err(|e| e.to_string())?; return Ok(()); }
            if let (Some(x), 2) = (any.downcast_mut::<Dir2>(), a.len()) { *x = Dir2::new(Vec2::new(a[0], a[1])).map_err(|e| e.to_string())?; return Ok(()); }
            // a list of points (a Tetrahedron's vertices, a triangle's): each onto its element
        }
    }
    match (f.reflect_mut(), v) {
        (bevy::reflect::ReflectMut::Struct(st), serde_json::Value::Object(o)) => {
            for (k, x) in o {
                let g = st.field_mut(k).ok_or_else(|| format!("no field {k:?}"))?;
                apply_json(g, x).map_err(|e| format!("{k}: {e}"))?;
            }
            Ok(())
        }
        (bevy::reflect::ReflectMut::TupleStruct(ts), _) if ts.field_len() == 1 => apply_json(ts.field_mut(0).unwrap(), v),
        (bevy::reflect::ReflectMut::Array(ar), serde_json::Value::Array(a)) if ar.len() == a.len() => {
            for (i, x) in a.iter().enumerate() {
                apply_json(ar.get_mut(i).unwrap(), x).map_err(|e| format!("[{i}]: {e}"))?;
            }
            Ok(())
        }
        _ => Err(format!("can't set it from {v}")),
    }
}

/// A mesh from its own vertex buffers (<BufferMesh>, or a Mesh handle given as {positions, ...}): JSON
/// {positions: [x, y, z, ...], indices?: [i, ...], normals?: [x, y, z, ...], uvs?: [u, v, ...]}, a triangle list
/// (indexed, or each three positions a triangle), as flat triangles; without normals each face's own. Its texture
/// coordinates when it gave them.
pub fn buffer_tris(json: &str) -> Result<(raea_core_shape::Tris, Option<Vec<[f32; 2]>>), String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("a buffer mesh isn't JSON: {e}"))?;
    let nums = |k: &str| -> Result<Option<Vec<f32>>, String> {
        match v.get(k) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::Array(a)) => a.iter().map(|x| x.as_f64().map(|f| f as f32).ok_or_else(|| format!("a buffer mesh's {k}: numbers, got {x}"))).collect::<Result<Vec<_>, _>>().map(Some),
            Some(x) => Err(format!("a buffer mesh's {k}: an array of numbers, got {x}")),
        }
    };
    let pos = nums("positions")?.ok_or("a buffer mesh needs positions: [x, y, z, ...]")?;
    if pos.len() % 3 != 0 || pos.is_empty() {
        return Err(format!("a buffer mesh's positions: x, y, z for each vertex, got {} numbers", pos.len()));
    }
    let n = pos.len() / 3;
    let idx: Vec<usize> = match nums("indices")? {
        Some(i) => i.iter().map(|x| *x as usize).collect(),
        None => (0..n).collect(),
    };
    if idx.len() % 3 != 0 || idx.iter().any(|&i| i >= n) {
        return Err(format!("a buffer mesh's indices: three a triangle, each under {n} (the vertices), got {}", idx.len()));
    }
    let normals = nums("normals")?;
    let uvs = nums("uvs")?;
    if normals.as_ref().is_some_and(|x| x.len() != pos.len()) || uvs.as_ref().is_some_and(|u| u.len() != n * 2) {
        return Err("a buffer mesh's normals are x, y, z and its uvs u, v for each vertex".into());
    }
    let p3 = |i: usize| [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]];
    let mut t = raea_core_shape::Tris { positions: Vec::with_capacity(idx.len()), normals: Vec::with_capacity(idx.len()) };
    let mut uv = uvs.as_ref().map(|_| Vec::with_capacity(idx.len()));
    for tri in idx.chunks(3) {
        let (a, b, c) = (Vec3::from(p3(tri[0])), Vec3::from(p3(tri[1])), Vec3::from(p3(tri[2])));
        let face = (b - a).cross(c - a).normalize_or_zero().to_array();
        for &i in tri {
            t.positions.push(p3(i));
            t.normals.push(match &normals {
                Some(nr) => [nr[i * 3], nr[i * 3 + 1], nr[i * 3 + 2]],
                None => face,
            });
            if let (Some(out), Some(u)) = (uv.as_mut(), uvs.as_ref()) {
                out.push([u[i * 2], u[i * 2 + 1]]);
            }
        }
    }
    Ok((t, uv))
}

/// A buffer mesh's triangle corners: the vertex each one is (its `indices`, or each of its `verts` vertices in turn).
pub fn buffer_corners(v: &serde_json::Value, verts: usize) -> Vec<usize> {
    match v.get("indices").and_then(serde_json::Value::as_array) {
        Some(i) => i.iter().map(|x| x.as_f64().unwrap_or(0.0) as usize).collect(),
        None => (0..verts).collect(),
    }
}

/// Texture coordinates for a Manifold mesh (it has none): each vertex projected along its face's main axis onto the
/// mesh's bounds, so a picture map covers each side of a box once, the right way round seen from outside.
pub fn box_uvs(pos: &[[f32; 3]], nor: &[[f32; 3]]) -> Vec<[f32; 2]> {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = |k: usize| (hi[k] - lo[k]).max(1e-6);
    let a = |p: &[f32; 3], k: usize| (p[k] - lo[k]) / span(k);
    let b = |p: &[f32; 3], k: usize| (hi[k] - p[k]) / span(k);
    pos.iter().zip(nor).map(|(p, n)| {
        let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
        if ax >= ay && ax >= az {
            if n[0] > 0.0 { [b(p, 2), b(p, 1)] } else { [a(p, 2), b(p, 1)] }
        } else if ay >= az {
            if n[1] > 0.0 { [a(p, 0), a(p, 2)] } else { [a(p, 0), b(p, 2)] }
        } else if n[2] > 0.0 {
            [a(p, 0), b(p, 1)]
        } else {
            [b(p, 0), b(p, 1)]
        }
    }).collect()
}


/// A mesh row's shape as a Bevy Mesh (docs/media.feature M2): a sphere alone is Bevy's Sphere::mesh() (icosphere or UV
/// sphere), a cuboid alone its Cuboid's (UVs a face each) and a cylinder its Cylinder's, as a Bevy Mesh3d's; a <BufferMesh> ("buffer:{positions,
/// indices, normals, uvs, colors}") its own vertex buffers; anything else Manifold's, flat-shaded. `uvs` gives it
/// texture coordinates (a picture map, a grid's outline) and `tangents` tangents made from them (a normal map).
pub fn shape_mesh(id: &str, expr: &str, uvs: bool, tangents: bool) -> Result<Mesh, String> {
    let ball = raea_core_shape::sphere_of(expr).map(|s| match s {
        raea_core_shape::SphereOf::Ico { radius, subdivisions } => Sphere::new(radius).mesh().ico(subdivisions).map_err(|e| format!("mesh {id}: {e}")),
        raea_core_shape::SphereOf::Uv { radius, sectors, stacks } => Ok(Sphere::new(radius).mesh().uv(sectors, stacks)),
    }).or_else(|| raea_core_shape::cuboid_of(expr).map(|[w, h, d]| Ok(Mesh::from(Cuboid::new(w, h, d)))))
        .or_else(|| raea_core_shape::cylinder_of(expr).map(|(r, h)| Ok(Mesh::from(Cylinder::new(r, h)))))
        // a plane alone: Bevy's one face, facing its normal (a box of no depth has two)
        .or_else(|| raea_core_shape::plane_of(expr).map(|(n, [w, h])| Ok(Mesh::from(Plane3d::new(Vec3::from(n), Vec2::new(w / 2.0, h / 2.0))))));
    if let Some(mut m) = ball.transpose()? {
        if tangents {
            m.generate_tangents().map_err(|e| format!("mesh {id}: normalMap: its tangents: {e}"))?;
        }
        return Ok(m);
    }
    if let Some(mut m) = expr.strip_prefix("buffer:").map(|b| indexed::buffer_indexed(b, uvs, vertex_colors)).transpose().map_err(|e| format!("mesh {id}: {e}"))?.flatten() {
        if tangents {
            m.generate_tangents().map_err(|e| format!("mesh {id}: normalMap: its tangents: {e}"))?;
        }
        return Ok(m);
    }
    let (tris, own_uvs, own_colors) = match expr.strip_prefix("buffer:") {
        Some(b) => {
            let c = buffer_colors(b).map_err(|e| format!("mesh {id}: {e}"))?;
            let (t, u) = buffer_tris(b).map_err(|e| format!("mesh {id}: {e}"))?;
            (std::sync::Arc::new(t), u, c)
        }
        None => (raea_core_shape::cached(expr).map_err(|e| format!("mesh {id}: {e}"))?, None, None),
    };
    let mut m = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, tris.positions.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, tris.normals.clone());
    // a <BufferMesh colors>: per-vertex colours, which StandardMaterial multiplies its colour by
    if let Some(c) = own_colors {
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, c);
    }
    if own_uvs.is_some() || uvs {
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, own_uvs.unwrap_or_else(|| box_uvs(&tris.positions, &tris.normals)));
        if tangents {
            m.generate_tangents().map_err(|e| format!("mesh {id}: normalMap: its tangents: {e}"))?;
        }
    }
    Ok(m)
}
