//! The edit's store (docs/substrate.feature, phase 1): its rows as entities in a Bevy World. A row is an entity with a
//! RowKey (kind, id); each of its columns is a component registered at run time per (kind, column, type), numbers as
//! f64 and text as String, so a kind's rows share tables and each column is contiguous there. The World keeps the
//! rows' insertion order (the edit's order) in the Schema resource. The evaluator reads the base through `gather`,
//! which turns it into columns (eval::Cols); a frame's values are derived from the base and never written into it.

use std::alloc::Layout;
use std::collections::HashMap;

use bevy::ecs::component::{ComponentCloneBehavior, ComponentDescriptor, ComponentId, StorageType};
use bevy::ecs::ptr::OwningPtr;
use bevy::prelude::{Component, Entity, Resource, World};

use crate::eval::{Cols, Tup};
use crate::ir::V;

/// A row's identity: its kind (table) and id.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct RowKey {
    pub kind: String,
    pub id: String,
}

const NUM: usize = 0;
const STR: usize = 1;

/// The store's catalogue: per kind its columns' components (numbers, text), the rows in order, rows by (kind, id).
#[derive(Resource, Default)]
pub struct Schema {
    cols: HashMap<String, HashMap<String, [Option<ComponentId>; 2]>>,
    order: Vec<Entity>,
    by: HashMap<String, HashMap<String, Entity>>,
}

unsafe fn drop_string(p: OwningPtr<'_>) {
    unsafe { p.drop_as::<String>() };
}

fn schema(world: &mut World) -> &mut Schema {
    if !world.contains_resource::<Schema>() {
        world.init_resource::<Schema>();
    }
    world.resource_mut::<Schema>().into_inner()
}

/// The component for (kind, column) holding numbers (NUM) or text (STR), registered the first time it's needed.
fn column(world: &mut World, kind: &str, col: &str, ty: usize) -> ComponentId {
    if let Some(id) = schema(world).cols.get(kind).and_then(|c| c.get(col)).and_then(|t| t[ty]) {
        return id;
    }
    let name = format!("{kind}.{col}{}", if ty == NUM { "" } else { ":text" });
    let (layout, drop) = if ty == NUM { (Layout::new::<f64>(), None) } else { (Layout::new::<String>(), Some(drop_string as unsafe fn(OwningPtr<'_>))) };
    // SAFETY: f64 and String are Send + Sync, and drop_string drops exactly a String
    let id = world.register_component_with_descriptor(unsafe { ComponentDescriptor::new_with_layout(name, StorageType::Table, layout, drop, true, ComponentCloneBehavior::Default, None) });
    schema(world).cols.entry(kind.to_string()).or_default().entry(col.to_string()).or_default()[ty] = Some(id);
    id
}

/// The rows, in the edit's order.
pub fn rows(world: &World) -> &[Entity] {
    world.get_resource::<Schema>().map(|s| s.order.as_slice()).unwrap_or(&[])
}

pub fn key(world: &World, e: Entity) -> Option<&RowKey> {
    world.get::<RowKey>(e)
}

pub fn find(world: &World, kind: &str, id: &str) -> Option<Entity> {
    world.get_resource::<Schema>()?.by.get(kind)?.get(id).copied()
}

/// Add a row at the end; its columns become components.
pub fn spawn(world: &mut World, t: &Tup) -> Entity {
    let e = world.spawn(RowKey { kind: t.kind.clone(), id: t.id.clone() }).id();
    for (c, v) in &t.vals {
        set(world, e, c, v.clone());
    }
    let s = schema(world);
    s.order.push(e);
    s.by.entry(t.kind.clone()).or_default().insert(t.id.clone(), e);
    e
}

/// Move a row onto another entity (one something else spawned, e.g. a 2D node): it keeps its place in the edit's order
/// and its columns; the old entity goes.
pub fn adopt(world: &mut World, old: Entity, new: Entity) {
    let Some(t) = tup(world, old) else { return };
    world.entity_mut(new).insert(RowKey { kind: t.kind.clone(), id: t.id.clone() });
    for (c, v) in &t.vals {
        set(world, new, c, v.clone());
    }
    let s = schema(world);
    if let Some(slot) = s.order.iter_mut().find(|e| **e == old) {
        *slot = new;
    }
    s.by.entry(t.kind.clone()).or_default().insert(t.id.clone(), new);
    world.despawn(old);
}

/// Remove a row (the entity goes with it, and whatever else was attached to it).
pub fn despawn(world: &mut World, e: Entity) {
    if let Some(k) = world.get::<RowKey>(e).cloned() {
        let s = schema(world);
        s.order.retain(|x| *x != e);
        if let Some(m) = s.by.get_mut(&k.kind) {
            m.remove(&k.id);
        }
    }
    world.despawn(e);
}

/// Remove every row.
pub fn clear(world: &mut World) {
    for e in rows(world).to_vec() {
        despawn(world, e);
    }
}

/// Replace the rows with `rows` (in order).
pub fn load(world: &mut World, rows: &[Tup]) -> Vec<Entity> {
    clear(world);
    rows.iter().map(|t| spawn(world, t)).collect()
}

/// One value of a row's column (None: the row doesn't have it).
pub fn get(world: &World, e: Entity, col: &str) -> Option<V> {
    let kind = &world.get::<RowKey>(e)?.kind;
    let ids = world.get_resource::<Schema>()?.cols.get(kind)?.get(col)?;
    let er = world.get_entity(e).ok()?;
    if let Some(p) = ids[NUM].and_then(|id| er.get_by_id(id).ok()) {
        // SAFETY: NUM components are f64
        return Some(V::N(unsafe { *p.deref::<f64>() }));
    }
    let p = ids[STR].and_then(|id| er.get_by_id(id).ok())?;
    // SAFETY: STR components are String
    Some(V::S(unsafe { p.deref::<String>() }.clone()))
}

/// Set a row's column: the value goes in the component of its type (a column can hold numbers in one row and text in
/// another, as SQL values can).
pub fn set(world: &mut World, e: Entity, col: &str, v: V) {
    let Some(kind) = world.get::<RowKey>(e).map(|k| k.kind.clone()) else { return };
    let (ty, other) = if matches!(v, V::N(_)) { (NUM, STR) } else { (STR, NUM) };
    let id = column(world, &kind, col, ty);
    let other_id = schema(world).cols[&kind][col][other];
    let mut ew = world.entity_mut(e);
    if let Some(o) = other_id {
        if ew.contains_id(o) {
            ew.remove_by_id(o);
        }
    }
    // SAFETY: the component for `ty` has the layout of f64 (NUM) or String (STR), and the value moves into it
    match v {
        V::N(x) => OwningPtr::make(x, |p| unsafe {
            ew.insert_by_id(id, p);
        }),
        V::S(s) => OwningPtr::make(s, |p| unsafe {
            ew.insert_by_id(id, p);
        }),
    }
}

/// A row as a Tup.
pub fn tup(world: &World, e: Entity) -> Option<Tup> {
    let k = world.get::<RowKey>(e)?;
    let cols = world.get_resource::<Schema>()?.cols.get(&k.kind);
    let vals = cols.map(|c| c.keys().filter_map(|col| get(world, e, col).map(|v| (col.clone(), v))).collect()).unwrap_or_default();
    Some(Tup { kind: k.kind.clone(), id: k.id.clone(), vals })
}

/// Every row, in order.
pub fn tups(world: &World) -> Vec<Tup> {
    rows(world).iter().filter_map(|&e| tup(world, e)).collect()
}

/// The base as columns for the evaluator, rows in the edit's order (row r is rows(world)[r]).
pub fn gather(world: &World) -> Cols {
    let order = rows(world);
    let (mut kinds, mut ids) = (Vec::with_capacity(order.len()), Vec::with_capacity(order.len()));
    let Some(s) = world.get_resource::<Schema>() else { return Cols::default() };
    for &e in order {
        let k = world.get::<RowKey>(e).expect("a row has a key");
        kinds.push(k.kind.clone());
        ids.push(k.id.clone());
    }
    let mut c = Cols { kind: std::sync::Arc::new(kinds), id: std::sync::Arc::new(ids), cols: HashMap::new() };
    for (r, &e) in order.iter().enumerate() {
        let Some(cols) = s.cols.get(&c.kind[r]) else { continue };
        let Ok(er) = world.get_entity(e) else { continue };
        for (col, ids) in cols {
            let v = if let Some(p) = ids[NUM].and_then(|id| er.get_by_id(id).ok()) {
                Some(V::N(unsafe { *p.deref::<f64>() }))
            } else {
                ids[STR].and_then(|id| er.get_by_id(id).ok()).map(|p| V::S(unsafe { p.deref::<String>() }.clone()))
            };
            if let Some(v) = v {
                c.set(r, col, v);
            }
        }
    }
    c
}

/// Write rows of `cols` (gathered from this World, so row r is rows(world)[r]) back into the base.
pub fn write_back(world: &mut World, cols: &Cols, which: &[usize]) {
    let order = rows(world).to_vec();
    for &r in which {
        let e = order[r];
        for (c, col) in &cols.cols {
            if let Some(v) = col.get(r) {
                if get(world, e, c) != Some(v.clone()) {
                    set(world, e, c, v);
                }
            }
        }
    }
}

/// The edit's rows in a World of their own (the CLI, previews and tests; the engine's rows live in its App's World).
pub struct Store {
    pub world: World,
}

impl Store {
    pub fn new(rows: &[Tup]) -> Store {
        let mut world = World::new();
        load(&mut world, rows);
        Store { world }
    }
    pub fn tups(&self) -> Vec<Tup> {
        tups(&self.world)
    }
    pub fn gather(&self) -> Cols {
        gather(&self.world)
    }
}

impl Clone for Store {
    fn clone(&self) -> Store {
        Store::new(&self.tups())
    }
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Store({} rows)", rows(&self.world).len())
    }
}

impl PartialEq for Store {
    fn eq(&self, other: &Store) -> bool {
        self.tups() == other.tups()
    }
}

impl Default for Store {
    fn default() -> Store {
        Store::new(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_mixed_types() {
        let rows = vec![
            Tup { kind: "title".into(), id: "a".into(), vals: [("y".into(), V::N(10.0)), ("text".into(), V::S("hi".into()))].into() },
            Tup { kind: "title".into(), id: "b".into(), vals: [("y".into(), V::S("top".into()))].into() },
            Tup { kind: "clip".into(), id: "a".into(), vals: [("from".into(), V::N(3.0))].into() },
        ];
        let mut s = Store::new(&rows);
        assert_eq!(s.tups(), rows);
        let e = find(&s.world, "title", "b").unwrap();
        set(&mut s.world, e, "y", V::N(5.0));
        assert_eq!(get(&s.world, e, "y"), Some(V::N(5.0)));
        assert_eq!(s.gather().to_tups()[1].vals["y"], V::N(5.0));
        let a = find(&s.world, "title", "a").unwrap();
        despawn(&mut s.world, a);
        assert_eq!(s.tups().len(), 2);
    }
}
