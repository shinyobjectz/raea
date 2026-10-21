//! @raea/motion/ecs on Bevy's own ECS: each world is a `bevy::ecs::world::World`. A component is a real Bevy component
//! registered by name at run time (its value a JSON object, in Bevy's archetype tables); an entity holds any number of
//! them; queries are Bevy's (QueryBuilder with/without by component id) with column conditions on top; parent and child
//! are Bevy's `ChildOf` relationship (despawning a parent takes its children); resources and per-frame events sit beside
//! the world. Entities carry stable ids of ours, so a world copies (for `at(frame)`) and comes back the same.
//!
//! The first version's calls (one kind per entity: spawn {kind, cols}, query {kind, where}, column, count, rows) are
//! kept: a kind is a component named after it.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use bevy::ecs::component::{ComponentCloneBehavior, ComponentDescriptor, ComponentId, StorageType};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::QueryBuilder;
use bevy::ecs::world::World;
use bevy::ptr::OwningPtr;
use serde_json::{Map, Value, json};

type R<T> = Result<T, String>;

/// One world: Bevy's, plus our ids, the component names, resources and this frame's events.
struct BWorld {
    w: World,
    comps: BTreeMap<String, ComponentId>,
    ids: BTreeMap<u64, Entity>,
    back: HashMap<Entity, u64>,
    /// the kind an entity was spawned with (the first version's single component)
    kind: HashMap<u64, String>,
    next: u64,
    resources: BTreeMap<String, Value>,
    events: BTreeMap<String, Vec<Value>>,
}

unsafe fn drop_value(p: OwningPtr<'_>) {
    // SAFETY: every dynamic component's value is a serde_json::Value (its descriptor's layout)
    unsafe { p.drop_as::<Value>() }
}

impl BWorld {
    fn new() -> BWorld {
        BWorld { w: World::new(), comps: BTreeMap::new(), ids: BTreeMap::new(), back: HashMap::new(), kind: HashMap::new(), next: 0, resources: BTreeMap::new(), events: BTreeMap::new() }
    }

    /// The component registered for `name` (made on first use).
    fn comp(&mut self, name: &str) -> ComponentId {
        if let Some(c) = self.comps.get(name) {
            return *c;
        }
        // SAFETY: the layout and drop are serde_json::Value's, which is Send + Sync; no relationship
        let d = unsafe { ComponentDescriptor::new_with_layout(name.to_string(), StorageType::Table, std::alloc::Layout::new::<Value>(), Some(drop_value), true, ComponentCloneBehavior::Ignore, None) };
        let id = self.w.register_component_with_descriptor(d);
        self.comps.insert(name.to_string(), id);
        id
    }

    fn entity(&self, id: u64) -> R<Entity> {
        self.ids.get(&id).copied().ok_or_else(|| format!("no entity {id}"))
    }

    fn insert(&mut self, e: Entity, name: &str, value: Value) {
        let c = self.comp(name);
        OwningPtr::make(value, |ptr| {
            // SAFETY: `c` was registered with Value's layout and `ptr` holds a Value
            unsafe {
                self.w.entity_mut(e).insert_by_id(c, ptr);
            }
        });
    }

    fn get(&self, e: Entity, name: &str) -> Option<&Value> {
        let c = *self.comps.get(name)?;
        // SAFETY: as insert
        self.w.get_by_id(e, c).map(|p| unsafe { p.deref::<Value>() })
    }

    fn spawn(&mut self, comps: &Map<String, Value>) -> u64 {
        let e = self.w.spawn_empty().id();
        self.next += 1;
        let id = self.next;
        self.ids.insert(id, e);
        self.back.insert(e, id);
        for (k, v) in comps {
            self.insert(e, k, v.clone());
        }
        id
    }

    /// The names of the components an entity has, sorted.
    fn names(&self, e: Entity) -> Vec<String> {
        self.comps.iter().filter(|(_, c)| self.w.get_by_id(e, **c).is_some()).map(|(n, _)| n.clone()).collect()
    }

    fn despawn(&mut self, id: u64) -> R<()> {
        let e = self.entity(id)?;
        // Bevy despawns the children with it (ChildOf links them): their ids go too
        let mut gone = vec![e];
        let mut i = 0;
        while i < gone.len() {
            if let Some(ch) = self.w.get::<Children>(gone[i]) {
                gone.extend(ch.iter());
            }
            i += 1;
        }
        self.w.despawn(e);
        for g in gone {
            if let Some(gid) = self.back.remove(&g) {
                self.ids.remove(&gid);
                self.kind.remove(&gid);
            }
        }
        Ok(())
    }

    /// Entities with every `with` component and none of `without`, by Bevy's query; ours in id order.
    fn find(&mut self, with: &[String], without: &[String]) -> Vec<u64> {
        let with_ids: Vec<ComponentId> = with.iter().map(|n| self.comp(n)).collect();
        let without_ids: Vec<ComponentId> = without.iter().map(|n| self.comp(n)).collect();
        let mut qb = QueryBuilder::<Entity>::new(&mut self.w);
        for c in &with_ids {
            qb.with_id(*c);
        }
        for c in &without_ids {
            qb.without_id(*c);
        }
        let mut q = qb.build();
        let mut out: Vec<u64> = q.iter(&self.w).filter_map(|e| self.back.get(&e).copied()).collect();
        out.sort_unstable();
        out
    }

    /// A column condition: "Comp.field" (or "field" of the first `with` component) compared with a value.
    fn passes(&self, e: Entity, default: Option<&str>, cond: &Value) -> bool {
        let Some(a) = cond.as_array() else { return false };
        let (col, op, val) = (a.first().and_then(Value::as_str).unwrap_or(""), a.get(1).and_then(Value::as_str).unwrap_or("=="), a.get(2).cloned().unwrap_or(Value::Null));
        let (comp, field) = match col.split_once('.') {
            Some((c, f)) => (c, f),
            None => (default.unwrap_or(""), col),
        };
        let x = self.get(e, comp).and_then(|v| v.get(field)).cloned().unwrap_or(Value::Null);
        let num = |v: &Value| match v {
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::Number(n) => n.as_f64(),
            _ => None,
        };
        match op {
            "==" => x == val || (num(&x).is_some() && num(&x) == num(&val)),
            "!=" => !(x == val || (num(&x).is_some() && num(&x) == num(&val))),
            _ => match (num(&x), num(&val)) {
                (Some(y), Some(w)) => match op {
                    "<" => y < w,
                    "<=" => y <= w,
                    ">" => y > w,
                    ">=" => y >= w,
                    _ => false,
                },
                _ => false,
            },
        }
    }

    /// Everything, to copy the world: each entity's id, kind, components and parent, plus resources and events.
    fn dump(&self) -> Value {
        let ents: Vec<Value> = self
            .ids
            .iter()
            .map(|(id, e)| {
                let comps: Map<String, Value> = self.names(*e).into_iter().filter_map(|n| self.get(*e, &n).cloned().map(|v| (n, v))).collect();
                let parent = self.w.get::<ChildOf>(*e).and_then(|c| self.back.get(&c.parent()).copied());
                json!({"id": id, "kind": self.kind.get(id), "components": comps, "parent": parent})
            })
            .collect();
        json!({"entities": ents, "next": self.next, "resources": self.resources, "events": self.events})
    }

    fn load(d: &Value) -> BWorld {
        let mut w = BWorld::new();
        let ents = d["entities"].as_array().cloned().unwrap_or_default();
        for en in &ents {
            let id = en["id"].as_u64().unwrap_or(0);
            let e = w.w.spawn_empty().id();
            w.ids.insert(id, e);
            w.back.insert(e, id);
            if let Some(k) = en["kind"].as_str() {
                w.kind.insert(id, k.to_string());
            }
            for (k, v) in en["components"].as_object().into_iter().flatten() {
                w.insert(e, k, v.clone());
            }
        }
        for en in &ents {
            if let (Some(id), Some(p)) = (en["id"].as_u64(), en["parent"].as_u64()) {
                let (c, p) = (w.ids[&id], w.ids[&p]);
                w.w.entity_mut(c).insert(ChildOf(p));
            }
        }
        w.next = d["next"].as_u64().unwrap_or(0);
        w.resources = d["resources"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
        w.events = d["events"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_array().cloned().unwrap_or_default())).collect()).unwrap_or_default();
        w
    }

    /// The first version's row: {id, kind, ...the kind component's columns}.
    fn row(&self, id: u64) -> R<Value> {
        let e = self.entity(id)?;
        let kind = self.kind.get(&id).cloned().unwrap_or_default();
        let mut m = self.get(e, &kind).and_then(Value::as_object).cloned().unwrap_or_default();
        m.insert("id".into(), json!(id));
        m.insert("kind".into(), json!(kind));
        Ok(Value::Object(m))
    }
}

thread_local! {
    static WORLDS: RefCell<(u64, HashMap<u64, BWorld>)> = RefCell::new((0, HashMap::new()));
}

fn u(v: &Value, k: &str) -> R<u64> {
    v.get(k).and_then(Value::as_u64).ok_or_else(|| format!("{k}: a number"))
}
fn s<'a>(v: &'a Value, k: &str) -> R<&'a str> {
    v.get(k).and_then(Value::as_str).ok_or_else(|| format!("{k}: a string"))
}
fn strings(v: &Value, k: &str) -> Vec<String> {
    v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn with_world<T>(v: &Value, f: impl FnOnce(&mut BWorld) -> R<T>) -> R<T> {
    let id = u(v, "world")?;
    WORLDS.with(|ws| {
        let mut ws = ws.borrow_mut();
        let w = ws.1.get_mut(&id).ok_or_else(|| format!("no world {id}"))?;
        f(w)
    })
}

fn put(w: BWorld) -> Value {
    WORLDS.with(|ws| {
        let mut ws = ws.borrow_mut();
        ws.0 += 1;
        let id = ws.0;
        ws.1.insert(id, w);
        json!({"world": id})
    })
}

/// The ecs.* calls.
pub fn call(name: &str, v: &Value) -> R<Value> {
    match name {
        "ecs.new" => Ok(put(BWorld::new())),
        "ecs.clone" => {
            let d = with_world(v, |w| Ok(w.dump()))?;
            Ok(put(BWorld::load(&d)))
        }
        "ecs.free" => {
            let id = u(v, "world")?;
            WORLDS.with(|ws| ws.borrow_mut().1.remove(&id));
            Ok(Value::Null)
        }
        "ecs.spawn" => with_world(v, |w| {
            let mut comps = v.get("components").and_then(Value::as_object).cloned().unwrap_or_default();
            let kind = v.get("kind").and_then(Value::as_str).map(str::to_string);
            if let Some(k) = &kind {
                comps.insert(k.clone(), Value::Object(v.get("cols").and_then(Value::as_object).cloned().unwrap_or_default()));
            }
            let id = w.spawn(&comps);
            if let Some(k) = kind {
                w.kind.insert(id, k);
            }
            if let Some(p) = v.get("parent").and_then(Value::as_u64) {
                let (c, p) = (w.entity(id)?, w.entity(p)?);
                w.w.entity_mut(c).insert(ChildOf(p));
            }
            Ok(json!({"id": id}))
        }),
        "ecs.set" => with_world(v, |w| {
            let id = u(v, "id")?;
            let e = w.entity(id)?;
            let kind = w.kind.get(&id).cloned().ok_or_else(|| format!("entity {id} has no kind (use ecs.insert)"))?;
            let mut cur = w.get(e, &kind).and_then(Value::as_object).cloned().unwrap_or_default();
            for (k, x) in v.get("cols").and_then(Value::as_object).into_iter().flatten() {
                cur.insert(k.clone(), x.clone());
            }
            w.insert(e, &kind, Value::Object(cur));
            Ok(Value::Null)
        }),
        "ecs.get" => with_world(v, |w| w.row(u(v, "id")?)),
        "ecs.despawn" => with_world(v, |w| w.despawn(u(v, "id")?).map(|_| Value::Null)),
        "ecs.insert" => with_world(v, |w| {
            let e = w.entity(u(v, "id")?)?;
            for (k, x) in v.get("components").and_then(Value::as_object).into_iter().flatten() {
                w.insert(e, k, x.clone());
            }
            Ok(Value::Null)
        }),
        "ecs.remove" => with_world(v, |w| {
            let e = w.entity(u(v, "id")?)?;
            for n in strings(v, "names") {
                if let Some(c) = w.comps.get(&n).copied() {
                    w.w.entity_mut(e).remove_by_id(c);
                }
            }
            Ok(Value::Null)
        }),
        "ecs.components" => with_world(v, |w| {
            let e = w.entity(u(v, "id")?)?;
            let m: Map<String, Value> = w.names(e).into_iter().filter_map(|n| w.get(e, &n).cloned().map(|x| (n, x))).collect();
            Ok(Value::Object(m))
        }),
        "ecs.find" => with_world(v, |w| {
            let with = strings(v, "with");
            let without = strings(v, "without");
            let conds: Vec<Value> = v.get("where").and_then(Value::as_array).cloned().unwrap_or_default();
            let ids = w.find(&with, &without);
            let default = with.first().cloned();
            let ids: Vec<u64> = ids.into_iter().filter(|id| conds.iter().all(|c| w.passes(w.ids[id], default.as_deref(), c))).collect();
            if v.get("rows").and_then(Value::as_bool).unwrap_or(false) {
                let rows: Vec<Value> = ids
                    .iter()
                    .map(|id| {
                        let e = w.ids[id];
                        let comps: Map<String, Value> = w.names(e).into_iter().filter_map(|n| w.get(e, &n).cloned().map(|x| (n, x))).collect();
                        json!({"id": id, "components": comps})
                    })
                    .collect();
                return Ok(json!({"ids": ids, "rows": rows}));
            }
            Ok(json!({"ids": ids}))
        }),
        // the first version's query: a kind's entities by its columns
        "ecs.query" => with_world(v, |w| {
            let kind = s(v, "kind")?.to_string();
            let conds: Vec<Value> = v.get("where").and_then(Value::as_array).cloned().unwrap_or_default();
            let ids: Vec<u64> = w.find(std::slice::from_ref(&kind), &[]).into_iter().filter(|id| w.kind.get(id) == Some(&kind)).filter(|id| conds.iter().all(|c| w.passes(w.ids[id], Some(&kind), c))).collect();
            Ok(json!({"ids": ids}))
        }),
        "ecs.column" => with_world(v, |w| {
            let kind = s(v, "kind")?.to_string();
            let col = s(v, "col")?;
            let ids: Vec<u64> = w.find(std::slice::from_ref(&kind), &[]).into_iter().filter(|id| w.kind.get(id) == Some(&kind)).collect();
            Ok(json!(ids.iter().map(|id| w.get(w.ids[id], &kind).and_then(|c| c.get(col)).and_then(Value::as_f64).map(|x| json!(x)).unwrap_or(Value::Null)).collect::<Vec<_>>()))
        }),
        "ecs.count" => with_world(v, |w| {
            let kind = s(v, "kind")?.to_string();
            Ok(json!(w.find(std::slice::from_ref(&kind), &[]).into_iter().filter(|id| w.kind.get(id) == Some(&kind)).count()))
        }),
        "ecs.rows" => with_world(v, |w| {
            let mut ids: Vec<(String, u64)> = w.kind.iter().map(|(id, k)| (k.clone(), *id)).collect();
            ids.sort();
            Ok(Value::Array(ids.into_iter().filter_map(|(_, id)| w.row(id).ok()).collect()))
        }),
        "ecs.parent" => with_world(v, |w| {
            let c = w.entity(u(v, "id")?)?;
            match v.get("parent").and_then(Value::as_u64) {
                Some(p) => {
                    let p = w.entity(p)?;
                    w.w.entity_mut(c).insert(ChildOf(p));
                }
                None if v.get("parent").is_some_and(Value::is_null) => {
                    w.w.entity_mut(c).remove::<ChildOf>();
                }
                None => {}
            }
            Ok(json!(w.w.get::<ChildOf>(c).and_then(|p| w.back.get(&p.parent()).copied())))
        }),
        "ecs.children" => with_world(v, |w| {
            let e = w.entity(u(v, "id")?)?;
            let mut out: Vec<u64> = w.w.get::<Children>(e).map(|c| c.iter().filter_map(|x| w.back.get(&x).copied()).collect()).unwrap_or_default();
            out.sort_unstable();
            Ok(json!(out))
        }),
        "ecs.resource" => with_world(v, |w| {
            let name = s(v, "name")?.to_string();
            if let Some(x) = v.get("value") {
                w.resources.insert(name.clone(), x.clone());
            }
            Ok(w.resources.get(&name).cloned().unwrap_or(Value::Null))
        }),
        "ecs.send" => with_world(v, |w| {
            w.events.entry(s(v, "name")?.to_string()).or_default().push(v.get("value").cloned().unwrap_or(Value::Null));
            Ok(Value::Null)
        }),
        "ecs.events" => with_world(v, |w| Ok(json!(w.events.get(s(v, "name")?).cloned().unwrap_or_default()))),
        "ecs.clear_events" => with_world(v, |w| {
            w.events.clear();
            Ok(Value::Null)
        }),
        // what renders, straight from the world: entities with a Mesh and a Transform as 3D nodes (world space,
        // parents' transforms composed), and lights; the bridge draws these without a React element per entity
        "ecs.scene" => with_world(v, |w| {
            let mut out = Vec::new();
            for id in w.find(&["Mesh".into()], &[]) {
                let e = w.ids[&id];
                let mesh = w.get(e, "Mesh").cloned().unwrap_or(Value::Null);
                let mut t = transform_of(w.get(e, "Transform"));
                let mut p = w.w.get::<ChildOf>(e).map(|c| c.parent());
                while let Some(pe) = p {
                    let pt = transform_of(w.get(pe, "Transform"));
                    t = compose(&pt, &t);
                    p = w.w.get::<ChildOf>(pe).map(|c| c.parent());
                }
                out.push(json!({"id": id, "kind": "Mesh", "mesh": mesh, "position": [t[0], t[1], t[2]], "rotation": [t[3], t[4], t[5]], "scale": t[6]}));
            }
            for id in w.find(&["Light".into()], &[]) {
                out.push(json!({"id": id, "kind": "Light", "light": w.get(w.ids[&id], "Light").cloned().unwrap_or(Value::Null)}));
            }
            Ok(Value::Array(out))
        }),
        o => Err(format!("no native call {o}")),
    }
}

/// [x, y, z, rx, ry, rz, scale] from a Transform component ({x, y, z, rx, ry, rz, scale}, all optional).
fn transform_of(t: Option<&Value>) -> [f64; 7] {
    let g = |k: &str, d: f64| t.and_then(|t| t.get(k)).and_then(Value::as_f64).unwrap_or(d);
    [g("x", 0.0), g("y", 0.0), g("z", 0.0), g("rx", 0.0), g("ry", 0.0), g("rz", 0.0), g("scale", 1.0)]
}

/// A child's transform in its parent's space, made world space (rotation by z, then y, then x; uniform scale).
fn compose(parent: &[f64; 7], child: &[f64; 7]) -> [f64; 7] {
    let (sx, cx) = parent[3].sin_cos();
    let (sy, cy) = parent[4].sin_cos();
    let (sz, cz) = parent[5].sin_cos();
    let (mut x, mut y, mut z) = (child[0] * parent[6], child[1] * parent[6], child[2] * parent[6]);
    // z
    let (x1, y1) = (x * cz - y * sz, x * sz + y * cz);
    x = x1;
    y = y1;
    // y
    let (x2, z2) = (x * cy + z * sy, -x * sy + z * cy);
    x = x2;
    z = z2;
    // x
    let (y3, z3) = (y * cx - z * sx, y * sx + z * cx);
    y = y3;
    z = z3;
    [parent[0] + x, parent[1] + y, parent[2] + z, parent[3] + child[3], parent[4] + child[4], parent[5] + child[5], parent[6] * child[6]]
}

/// The ecs.* names with their TypeScript types, for the registry.
pub const CALLS: &[(&str, &str, &str, &str)] = &[
    ("ecs.new", "{}", "{ world: number }", "a new world (Bevy's)"),
    ("ecs.clone", "{ world: number }", "{ world: number }", "a copy of a world"),
    ("ecs.free", "{ world: number }", "null", "drop a world"),
    ("ecs.spawn", "{ world: number; kind?: string; cols?: Record<string, number | string | boolean>; components?: Record<string, unknown>; parent?: number }", "{ id: number }", "a new entity: a kind with columns, and/or any components"),
    ("ecs.set", "{ world: number; id: number; cols: Record<string, number | string | boolean> }", "null", "columns merged into an entity's kind"),
    ("ecs.get", "{ world: number; id: number }", "{ id: number; kind: string } & Record<string, number | string | boolean>", "an entity's row (its kind's columns)"),
    ("ecs.despawn", "{ world: number; id: number }", "null", "remove an entity and its children"),
    ("ecs.insert", "{ world: number; id: number; components: Record<string, unknown> }", "null", "components added to (or replaced on) an entity"),
    ("ecs.remove", "{ world: number; id: number; names: string[] }", "null", "components taken off an entity"),
    ("ecs.components", "{ world: number; id: number }", "Record<string, unknown>", "an entity's components by name"),
    ("ecs.find", "{ world: number; with?: string[]; without?: string[]; where?: [string, \"==\" | \"!=\" | \"<\" | \"<=\" | \">\" | \">=\", number | string | boolean][]; rows?: boolean }", "{ ids: number[]; rows?: { id: number; components: Record<string, unknown> }[] }", "entities with every `with` component and no `without` one (Bevy's query), passing every condition (\"Comp.field\")"),
    ("ecs.query", "{ world: number; kind: string; where: [string, \"==\" | \"!=\" | \"<\" | \"<=\" | \">\" | \">=\", number | string | boolean][] }", "{ ids: number[] }", "ids of kind's entities passing every condition"),
    ("ecs.column", "{ world: number; kind: string; col: string }", "(number | null)[]", "one numeric column of a kind, in id order"),
    ("ecs.count", "{ world: number; kind: string }", "number", "how many entities of a kind"),
    ("ecs.rows", "{ world: number }", "({ id: number; kind: string } & Record<string, number | string | boolean>)[]", "every kind's rows, by kind then id"),
    ("ecs.parent", "{ world: number; id: number; parent?: number | null }", "number | null", "an entity's parent (set it with parent; null clears it)"),
    ("ecs.children", "{ world: number; id: number }", "number[]", "an entity's children"),
    ("ecs.resource", "{ world: number; name: string; value?: unknown }", "unknown", "a resource (set it with value)"),
    ("ecs.send", "{ world: number; name: string; value?: unknown }", "null", "an event for this frame"),
    ("ecs.events", "{ world: number; name: string }", "unknown[]", "this frame's events of a name"),
    ("ecs.clear_events", "{ world: number }", "null", "the frame's events cleared (each step starts with none)"),
    ("ecs.scene", "{ world: number }", "{ id: number; kind: \"Mesh\" | \"Light\"; mesh?: unknown; light?: unknown; position?: [number, number, number]; rotation?: [number, number, number]; scale?: number }[]", "the world's Mesh+Transform entities (world space) and lights, to draw"),
];

/// How many worlds are alive (media.live's leak check).
pub fn live() -> usize {
    WORLDS.with(|ws| ws.borrow().1.len())
}
