//! Equal parts share their assets (docs/control.feature FC1a, closing C1; runtime/scene3d re-exports this crate): a
//! mesh row's Mesh is made once per shape and a plain StandardMaterial once per value, so N equal parts hold one
//! Handle<Mesh> and one Handle<StandardMaterial> and Bevy's automatic batching draws them as one, as it does for an
//! example that shares its handles. A shared material a frame changes (a part's colour animating) is first copied to
//! that part alone (sync_meshes), so one part's change never shows on another. RAEA_SHARE_ASSETS=0 makes every part its
//! own again. What no part holds any more is let go: a part whose size changes each frame is a new shape each frame,
//! and the film (docs/stage-guards.feature SG4a) held 234,000 meshes a minute in, 450 MB and their GPU buffers, nearly
//! all for shapes no part had any more.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use bevy::prelude::*;

/// A part whose StandardMaterial other parts hold too: sync_meshes copies it before changing it.
#[derive(Component)]
pub struct SharedMaterial;

#[derive(Resource, Default)]
struct Shared {
    meshes: HashMap<u64, Handle<Mesh>>,
    materials: HashMap<String, Handle<StandardMaterial>>,
    /// the meshes held after the last sweep: the next comes when they've doubled, so sweeping costs nothing a call
    swept: usize,
}

impl Shared {
    /// Let go of what only the share holds (Bevy drops the asset, and its GPU copy, once its last handle goes).
    fn sweep(&mut self) {
        if self.meshes.len() >= (2 * self.swept).max(1024) {
            self.meshes.retain(|_, h| !unheld(h));
            self.materials.retain(|_, h| !unheld(h));
            self.swept = self.meshes.len();
        }
    }
}

/// Whether nothing but the share holds `h`.
fn unheld<A: Asset>(h: &Handle<A>) -> bool {
    matches!(h, Handle::Strong(a) if std::sync::Arc::strong_count(a) == 1)
}

/// Whether parts share (on unless RAEA_SHARE_ASSETS=0).
pub fn sharing() -> bool {
    std::env::var("RAEA_SHARE_ASSETS").map_or(true, |v| v != "0")
}

/// The mesh for `key` (everything that decides its vertices), made by `make` the first time.
pub fn shared_mesh(world: &mut World, key: &str, make: impl FnOnce() -> Result<Mesh, String>) -> Result<Handle<Mesh>, String> {
    if !sharing() {
        let m = make()?;
        return Ok(world.resource_mut::<Assets<Mesh>>().add(m));
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    let k = h.finish();
    if let Some(m) = world.get_resource::<Shared>().and_then(|s| s.meshes.get(&k)).cloned() {
        return Ok(m);
    }
    let m = make()?;
    let m = world.resource_mut::<Assets<Mesh>>().add(m);
    let mut s = world.get_resource_or_insert_with(Shared::default);
    s.sweep();
    s.meshes.insert(k, m.clone());
    Ok(m)
}

/// The material equal to `m`, field for field (its Debug form is the key: every field, a texture by its handle), made
/// the first time; and whether it's shared (to mark the part SharedMaterial).
pub fn shared_material(world: &mut World, m: StandardMaterial) -> (Handle<StandardMaterial>, bool) {
    if !sharing() {
        return (world.resource_mut::<Assets<StandardMaterial>>().add(m), false);
    }
    let key = format!("{m:?}");
    if let Some(h) = world.get_resource::<Shared>().and_then(|s| s.materials.get(&key)).cloned() {
        return (h, true);
    }
    let h = world.resource_mut::<Assets<StandardMaterial>>().add(m);
    world.get_resource_or_insert_with(Shared::default).materials.insert(key, h.clone());
    (h, true)
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    /// A shape no part holds any more is let go once the share has doubled; one a part still holds stays shared.
    #[test]
    fn meshes_no_part_holds_are_let_go() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        let cube = || Ok(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
        let kept = super::shared_mesh(&mut world, "held", cube).unwrap();
        for i in 0..3000 {
            let _ = super::shared_mesh(&mut world, &format!("box {i}"), cube).unwrap();
        }
        let held = world.resource::<super::Shared>().meshes.len();
        assert!(held < 1100, "{held} meshes still in the share");
        assert_eq!(super::shared_mesh(&mut world, "held", cube).unwrap().id(), kept.id());
    }
}
