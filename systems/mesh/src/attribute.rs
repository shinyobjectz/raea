//! A mesh's own vertex attributes (docs/control.feature FC2d): Bevy's MeshVertexAttribute by its name, id and format,
//! as shader_advanced/custom_vertex_attribute's ATTRIBUTE_BLEND_COLOR is one, inserted into a mesh with its values as
//! Mesh::with_inserted_attribute inserts them, and named in a material's vertex buffer layout.

use std::collections::BTreeSet;
use std::sync::Mutex;

use bevy::mesh::{MeshVertexAttribute, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::VertexFormat;
use serde_json::Value;

/// Each name given once for all time: MeshVertexAttribute holds a `&'static str`, as Bevy's are constants.
static NAMES: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());

/// The vertex formats an attribute may have, by Bevy's VertexFormat name.
fn format(f: &str) -> Option<VertexFormat> {
    Some(match f {
        "Float32" => VertexFormat::Float32,
        "Float32x2" => VertexFormat::Float32x2,
        "Float32x3" => VertexFormat::Float32x3,
        "Float32x4" => VertexFormat::Float32x4,
        "Uint32" => VertexFormat::Uint32,
        "Uint32x2" => VertexFormat::Uint32x2,
        "Uint32x3" => VertexFormat::Uint32x3,
        "Uint32x4" => VertexFormat::Uint32x4,
        "Sint32" => VertexFormat::Sint32,
        "Sint32x4" => VertexFormat::Sint32x4,
        _ => return None,
    })
}

/// The attribute { name, id, format } names.
pub fn attribute(v: &Value) -> Result<MeshVertexAttribute, String> {
    let name = v.get("name").and_then(Value::as_str).ok_or_else(|| format!("a mesh attribute is {{ name, id, format }}, got {v}"))?;
    let id = v.get("id").and_then(Value::as_u64).ok_or_else(|| format!("mesh attribute {name}: its id is a whole number"))?;
    let f = v.get("format").and_then(Value::as_str).unwrap_or_default();
    let format = format(f).ok_or_else(|| format!("mesh attribute {name}: format {f:?} (Float32, Float32x2/3/4, Uint32, Uint32x2/3/4, Sint32, Sint32x4)"))?;
    let mut names = NAMES.lock().map_err(|_| "mesh attribute names: poisoned")?;
    let name: &'static str = match names.get(name) {
        Some(n) => n,
        None => {
            let n: &'static str = Box::leak(name.into());
            names.insert(n);
            n
        }
    };
    Ok(MeshVertexAttribute::new(name, id, format))
}

/// One vertex's value as `n` numbers (a number alone when n is 1).
fn row(v: &Value, n: usize) -> Option<Vec<f64>> {
    match v {
        Value::Number(x) if n == 1 => x.as_f64().map(|x| vec![x]),
        Value::Array(a) if a.len() == n => a.iter().map(Value::as_f64).collect(),
        _ => None,
    }
}

/// An attribute's values, one a vertex, in its format.
fn values(a: &MeshVertexAttribute, vals: &[Value]) -> Result<VertexAttributeValues, String> {
    let n = match a.format { VertexFormat::Float32 | VertexFormat::Uint32 | VertexFormat::Sint32 => 1, VertexFormat::Float32x2 | VertexFormat::Uint32x2 => 2, VertexFormat::Float32x3 | VertexFormat::Uint32x3 => 3, _ => 4 };
    let rows: Vec<Vec<f64>> = vals.iter().map(|v| row(v, n)).collect::<Option<_>>().ok_or_else(|| format!("mesh attribute {}: each value is {n} number(s)", a.name))?;
    use VertexAttributeValues as A;
    let (f, u, s) = (|x: f64| x as f32, |x: f64| x as u32, |x: f64| x as i32);
    Ok(match a.format {
        VertexFormat::Float32 => A::Float32(one(&rows, f)),
        VertexFormat::Float32x2 => A::Float32x2(each(&rows, f)),
        VertexFormat::Float32x3 => A::Float32x3(each(&rows, f)),
        VertexFormat::Float32x4 => A::Float32x4(each(&rows, f)),
        VertexFormat::Uint32 => A::Uint32(one(&rows, u)),
        VertexFormat::Uint32x2 => A::Uint32x2(each(&rows, u)),
        VertexFormat::Uint32x3 => A::Uint32x3(each(&rows, u)),
        VertexFormat::Uint32x4 => A::Uint32x4(each(&rows, u)),
        VertexFormat::Sint32 => A::Sint32(one(&rows, s)),
        _ => A::Sint32x4(each(&rows, s)),
    })
}

/// Each vertex's N numbers as one value of the format's type.
fn each<T, const N: usize>(rows: &[Vec<f64>], c: impl Fn(f64) -> T) -> Vec<[T; N]> {
    rows.iter().map(|r| std::array::from_fn(|i| c(r[i]))).collect()
}

/// Each vertex's one number.
fn one<T>(rows: &[Vec<f64>], c: impl Fn(f64) -> T) -> Vec<T> {
    rows.iter().map(|r| c(r[0])).collect()
}

/// The mesh with each attribute of `json` ([[{ name, id, format }, values], ...]) inserted; "" leaves it as it is. Each
/// has a value per vertex, as Bevy's Mesh::insert_attribute asks.
pub fn with_attributes(mut mesh: Mesh, json: &str) -> Result<Mesh, String> {
    if json.is_empty() {
        return Ok(mesh);
    }
    let list: Vec<(Value, Vec<Value>)> = serde_json::from_str(json).map_err(|e| format!("attributes are [[{{ name, id, format }}, values], ...]: {e}"))?;
    for (a, vals) in list {
        let a = attribute(&a)?;
        if vals.len() != mesh.count_vertices() {
            return Err(format!("mesh attribute {}: {} values for {} vertices", a.name, vals.len(), mesh.count_vertices()));
        }
        let v = values(&a, &vals)?;
        mesh.insert_attribute(a, v);
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_meshs_own_attribute_is_inserted_per_vertex() {
        let cube = bevy::prelude::Mesh::from(bevy::prelude::Cuboid::default());
        let json = format!("[[{{\"name\":\"BlendColor\",\"id\":988540917,\"format\":\"Float32x4\"}},{}]]", serde_json::json!(vec![[1.0, 0.0, 0.0, 1.0]; 24]));
        let m = super::with_attributes(cube.clone(), &json).unwrap();
        let a = super::attribute(&serde_json::json!({ "name": "BlendColor", "id": 988540917, "format": "Float32x4" })).unwrap();
        assert_eq!(m.attribute(a).map(|v| v.len()), Some(24));
        assert!(super::with_attributes(cube, &json.replace("988540917", "x")).is_err());
    }
}
