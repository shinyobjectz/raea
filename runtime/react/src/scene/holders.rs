//! The page elements that hold a frame's 3D, and the 3D read from React's tree.

use super::*;

/// The frame's 3D (each data-3d root in the page's order, its 3D children under it): what a render turns into rows.
pub fn scene_of(world: &mut World) -> Vec<Node3> {
    let now = SCENE3D.with(|g| { g.set(g.get() + 1); g.get() });
    if now % 120 == 0 {
        PROPS3D.with(|c| c.borrow_mut().retain(|_, v| v.2 + 2 >= now));
        AT3D.with_borrow_mut(|a| PROPS3D.with_borrow(|p| a.retain(|e, _| p.contains_key(e))));
        if world.contains_resource::<Kept3d>() {
            world.resource_scope(|w, mut k: Mut<Kept3d>| k.of.retain(|e, _| w.get_entity(*e).is_ok()));
        }
    }
    let Some(surf) = surface(world) else { return Vec::new() };
    // the 3D roots under the surface (a data-3d element stops the walk; its 3D is read3d's), each subtree no commit
    // touched since the last scene handed on as it was (read.rs Kept3d); the surface's own straight into the scene, not
    // kept and copied out (10 000 cubes beside a turning camera were copied three times a frame; FC1b)
    let dirty = dirty3d(world);
    world.resource_scope(|world, mut kept: Mut<Kept3d>| { let mut v = Vec::new(); kids3d(world, surf, &mut kept, &dirty, &mut v); v })
}

/// The composition's surface (`<surface id="motion">`): the element its page and its 3D hang from.
pub fn surface(world: &mut World) -> Option<Entity> {
    world.query::<(Entity, &ReactKind, Option<&ReactProps>)>().iter(world).find(|(_, k, p)| k.0 == "surface" && p.and_then(|p| p.0.get("id")).and_then(|v| v.as_str()) == Some("motion")).map(|(e, _, _)| e)
}

/// The elements holding the 3D (holders_of's), into `out` (ZeroHeap: live play's present asks each frame)
pub fn holder_entities(world: &mut World, out: &mut Vec<Entity>) {
    out.clear();
    out.extend(fresh_holders(world).0);
}

/// Each 3D root's ancestors up to the surface, nearest first, in the page's order: a walk down from the surface that
/// stops at each data-3d element (a 3D root), so the 3D's own parts are never visited. Before, every element's props
/// were looked at each frame to find the roots (half of many_lights' React step, its 100 000 lights).
fn holders_walk(world: &mut World, out: &mut Vec<Entity>) {
    let Some(surf) = world.query::<(Entity, &ReactKind)>().iter(world).find(|(_, k)| k.0 == "surface").map(|(e, _)| e) else { return };
    let world: &World = world;
    let mut stack: Vec<(Entity, usize)> = vec![(surf, 0)];
    let mut path: Vec<Entity> = Vec::new();
    while let Some((e, depth)) = stack.pop() {
        path.truncate(depth);
        if e != surf
            && let Some(p) = world.get::<ReactProps>(e)
            && p.0.contains_key("data-3d")
        {
            for a in path.iter().rev().filter(|a| **a != surf) {
                if !out.contains(a) {
                    out.push(*a);
                }
            }
            continue;
        }
        path.push(e);
        if let Some(kids) = world.get::<Children>(e) {
            stack.extend(kids.iter().rev().map(|k| (k, depth + 1)));
        }
    }
}

/// The page elements holding the frame's 3D (each 3D root's ancestors up to the surface, nearest first) with their
/// props: a render clears their backgrounds (the 3D is drawn under the page) and the nearest one's colour is the 3D's.
pub fn holders_of(world: &mut World) -> Vec<(Entity, Map<String, Value>)> {
    fresh_holders(world).0.into_iter().map(|e| (e, world.get::<ReactProps>(e).map(|p| p.0.clone()).unwrap_or_default())).collect()
}

/// A layered Canvas3D's slot: the `<img data-raea-scene>` it renders, whose place in the page's paint order the 3D's
/// frame takes (as a texture), so page elements before it in the tree are under the 3D and those after it over it.
pub fn scene_slot(world: &mut World) -> Option<u64> {
    fresh_holders(world).1.and_then(|s| world.get::<crate::ReactNode>(s)).map(|n| n.0)
}

/// The holders and the slot as the page last had them, worked out again only when the page changed: a child added,
/// moved or taken away (or a commit's note that a parent lost one, `stale`), or new props on a holder, on the slot
/// or on an element that becomes one (a page element's other props, a frame's mark, change nothing here). Each was a
/// look at every element's props every frame (100 000 lights).
#[derive(Resource)]
pub(crate) struct HolderCache {
    #[allow(clippy::type_complexity)]
    watch: bevy::ecs::system::SystemState<(Query<'static, 'static, (), Or<(Changed<Children>, Changed<ChildOf>)>>, Query<'static, 'static, (Entity, &'static ReactProps), Changed<ReactProps>>)>,
    holders: Vec<Entity>,
    slot: Option<Entity>,
    pub(crate) stale: bool,
}

fn fresh_holders(world: &mut World) -> (Vec<Entity>, Option<Entity>) {
    if !world.contains_resource::<HolderCache>() {
        let watch = bevy::ecs::system::SystemState::new(world);
        world.insert_resource(HolderCache { watch, holders: Vec::new(), slot: None, stale: true });
    }
    let stale = world.resource_scope(|world, mut c: Mut<HolderCache>| {
        let (holders, slot) = (c.holders.clone(), c.slot);
        let Ok((moved, props)) = c.watch.get(world) else { return true };
        let changed = moved.iter().next().is_some() || props.iter().any(|(e, p)| holders.contains(&e) || slot == Some(e) || p.0.contains_key("data-raea-scene"));
        // what a commit changes directly on the World after this has a newer tick than this read (read.rs dirty3d)
        world.increment_change_tick();
        std::mem::take(&mut c.stale) || changed
    });
    if stale {
        let mut holders = Vec::new();
        holders_walk(world, &mut holders);
        let slot = world.query::<(Entity, &ReactProps)>().iter(world).find(|(_, p)| p.0.contains_key("data-raea-scene")).map(|(e, _)| e);
        let mut c = world.resource_mut::<HolderCache>();
        (c.holders, c.slot) = (holders, slot);
    }
    let c = world.resource::<HolderCache>();
    (c.holders.clone(), c.slot)
}

/// The 3D's clear colour for a render or play: a layered Canvas3D's `clearColor` (its slot's data-clear, which its parts
/// blend over in linear light as Bevy's over its ClearColor), else transparent, the page under it; else the nearest
/// holder's background. A camera in a box of the page clears to nothing as well: its view is that box's picture, in the
/// page's paint order, so what the page paints under the box shows where the 3D has nothing (a sibling layer under an
/// <R3F> was cut out to the nearest ancestor's colour, #235).
pub fn scene_clear(world: &mut World) -> Option<String> {
    let Some(s) = fresh_holders(world).1 else {
        if crate::VIEW_BOX.lock().unwrap().is_some() {
            return Some("#00000000".into());
        }
        return clear_of(&holders_of(world));
    };
    Some(world.get::<ReactProps>(s).and_then(|p| p.0.get("data-clear")?.as_str().map(str::to_string)).unwrap_or_else(|| "#00000000".into()))
}

/// The 3D's clear colour: the nearest holder's background, as a render takes it.
pub fn clear_of(holders: &[(Entity, Map<String, Value>)]) -> Option<String> {
    holders.iter().find_map(|(_, p)| p.get("style").and_then(|s| s.as_object()).and_then(|st| st.get("background").or(st.get("backgroundColor"))).and_then(|v| v.as_str()).map(str::to_string))
}
