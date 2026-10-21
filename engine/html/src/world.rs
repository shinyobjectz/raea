//! The World (docs/row.feature, "ECS all the way through"): every row of an edit is an entity, its fields are
//! components, and a frame is one run of a fixed schedule of systems:
//!
//!   clock -> visibility (spans) -> resolve (curves, springs, bindings) -> sync_dom (Blitz) -> layout -> boxes -> paint
//!
//! Renderers are derived views: the Blitz document is patched from components each frame and never read as
//! truth, except that layout writes each row's computed box back as a component (`LayoutBox`). Bevy 3D kinds
//! join this World later as more components and systems.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::SingleThreadedExecutor;
use blitz_dom::{BaseDocument, NodeId};
use serde_json::{Map, Value};

use crate::row_render::{build_document, image_src, imports_of, node_label, paint_document, qn, style_for};
use crate::timeline::{Edit, Prop, Timeline, num, parse_prop};

// === Components ===

/// The row's id, as written in .row (`hook :: title{...}`).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Name(pub String);

/// The row's kind: a native kind (`title`, `box`, ...) or a component (`EndCard`).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Kind(pub String);

/// The parent row's entity (`in = bg`).
#[derive(Component, Debug, Clone, Copy)]
pub struct ParentRow(pub Entity);

/// Where the row sits: its index in the edit (source order) and in the tree (parents first, siblings by order).
#[derive(Component, Debug, Clone, Copy)]
pub struct Order {
    pub row: usize,
    pub tree: usize,
    pub depth: usize,
}

/// Absolute frames the row exists in, clipped to its parent's; `to` is exclusive.
#[derive(Component, Debug, Clone, Copy)]
pub struct Span {
    pub from: i64,
    pub to: i64,
}

/// The authored props: constants, curves, springs and bindings.
#[derive(Component, Debug, Clone)]
pub struct Tracks(pub BTreeMap<String, Prop>);

/// Written by `visibility`: whether the row exists at the current frame, and where in its span it is.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Visible {
    pub on: bool,
    pub frame: i64,
    pub progress: f64,
}

/// Written by `resolve`: every prop's value at the current frame.
#[derive(Component, Debug, Clone, Default)]
pub struct Resolved(pub Map<String, Value>);

/// The row's Blitz element and its text node.
#[derive(Component, Debug, Clone, Copy)]
pub struct DomNode {
    pub el: NodeId,
    pub text: Option<NodeId>,
}

/// Written by `boxes` after layout: the row's border box in edit pixels (absolute), zero while hidden.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct LayoutBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

// === Resources ===

/// The frame being made.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct Clock(pub i64);

/// The compiled edit: settings, data columns and the evaluators for tracks.
#[derive(Resource)]
pub struct Time(pub Timeline);

/// Components implemented in React: kind -> .tsx path.
#[derive(Resource, Default)]
pub struct Imports(pub HashMap<String, String>);

/// The Blitz document the 2D kinds render through (a derived view; see the module docs).
pub struct Dom {
    pub doc: BaseDocument,
    pub scale: f32,
}

/// Hosted rows (`sync_dom_changed`): how many times the DOM was synced and how many row patches that took.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct DomStats {
    pub runs: u64,
    pub patched: u64,
}

/// The last painted frame: premultiplied RGBA8.
#[derive(Resource, Default)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Paint over transparent instead of black (for a host that layers it, e.g. over Bevy 3D)
    pub transparent: bool,
    /// paint without waiting for the GPU: the paint goes to `pending` instead of `rgba` (RowWorld::submit)
    pub defer: bool,
    pub pending: Option<crate::row_render::Painting>,
}

// === Systems ===

/// Which rows exist at the clock's frame, and where in their span.
pub fn visibility(clock: Res<Clock>, mut rows: Query<(&Span, &mut Visible)>) {
    let f = clock.0;
    for (span, mut v) in &mut rows {
        let on = f >= span.from && f < span.to;
        let frame = f - span.from;
        let progress = if on { num(frame as f64 / ((span.to - span.from).max(1) - 1).max(1) as f64).as_f64().unwrap_or(0.0) } else { 0.0 };
        *v = Visible { on, frame, progress };
    }
}

/// Every visible row's props at the clock's frame.
pub fn resolve(clock: Res<Clock>, time: Res<Time>, mut rows: Query<(&Tracks, &Visible, &mut Resolved)>) {
    for (tracks, v, mut out) in &mut rows {
        out.0.clear();
        if !v.on {
            continue;
        }
        for (name, p) in &tracks.0 {
            out.0.insert(name.clone(), time.0.eval(p, v.frame, clock.0));
        }
    }
}

/// Patch the Blitz document from the components: inline style and text per visible row, hidden otherwise.
/// Rows are patched in source order, as the slice-1 renderer did.
pub fn sync_dom(mut dom: NonSendMut<Dom>, imports: Res<Imports>, rows: Query<(&Order, &Kind, &DomNode, &Visible, &Resolved)>) {
    patch(&mut dom, &imports, rows.iter().collect());
}

/// As `sync_dom`, for only the rows whose visibility or props changed since it last ran (a host's rows, see
/// `install_hosted`): an unchanged row's element isn't touched.
/// A row a host moves (its x or y change over the edit): its text paints from outlines at exact fractional positions,
/// with no whole-pixel baseline snap, so eased motion stays smooth (engine/html/docs/bevy.feature B4). Video renders only
/// (`smooth_moving_text`).
pub fn mark_moving(world: &mut World, e: Entity) {
    let Some(el) = world.get::<DomNode>(e).map(|d| d.el) else { return };
    if let Some(mut dom) = world.get_non_send_mut::<Dom>() {
        dom.doc.raea_moving.insert(el);
    }
}

pub fn sync_dom_changed(mut dom: NonSendMut<Dom>, imports: Res<Imports>, mut stats: ResMut<DomStats>, rows: Query<(&Order, &Kind, &DomNode, &Visible, &Resolved), Or<(Changed<Visible>, Changed<Resolved>)>>) {
    let rows: Vec<_> = rows.iter().collect();
    stats.runs += 1;
    stats.patched += rows.len() as u64;
    patch(&mut dom, &imports, rows);
}

fn patch(dom: &mut Dom, imports: &Imports, mut rows: Vec<(&Order, &Kind, &DomNode, &Visible, &Resolved)>) {
    rows.sort_by_key(|(o, ..)| o.row);
    let pictures: Vec<(NodeId, Option<String>)> = rows.iter().filter(|r| r.1 .0 == "image").filter_map(|r| image_src(&dom.doc, r.2.el, r.3.on).map(|c| (r.2.el, c))).collect();
    let mut m = dom.doc.mutate();
    for (el, c) in pictures {
        match c {
            Some(src) => m.set_attribute(el, qn("src"), &src),
            None => m.clear_attribute(el, qn("src")),
        }
    }
    for (_, kind, node, v, props) in rows {
        if !v.on {
            m.set_attribute(node.el, qn("style"), "display:none");
            continue;
        }
        m.set_attribute(node.el, qn("style"), &style_for(&props.0));
        if let Some(t) = node.text {
            m.set_node_text(t, &node_label(&kind.0, &imports.0, &props.0));
        }
    }
}

/// As `visibility`, writing only when a row appears or disappears (a host's rows have no tracks to place in their
/// span, so `frame` and `progress` stay as they were when it last changed).
pub fn visibility_on(clock: Res<Clock>, mut rows: Query<(&Span, &mut Visible)>) {
    let f = clock.0;
    for (span, mut v) in &mut rows {
        let on = f >= span.from && f < span.to;
        if v.on != on {
            *v = Visible { on, frame: f - span.from, progress: 0.0 };
        }
    }
}

/// Style, layout and animation state for the clock's time (Stylo + Taffy + Parley).
pub fn layout(mut dom: NonSendMut<Dom>, clock: Res<Clock>, time: Res<Time>) {
    dom.doc.resolve(clock.0 as f64 / time.0.edit().fps_f64());
}

/// Write each row's computed box back as a component (absolute, in edit pixels).
pub fn boxes(dom: NonSend<Dom>, mut rows: Query<(&DomNode, &Visible, &mut LayoutBox)>) {
    for (node, v, mut b) in &mut rows {
        let new = if v.on { absolute_box(&dom.doc, node.el) } else { LayoutBox::default() };
        if *b != new {
            *b = new;
        }
    }
}

fn absolute_box(doc: &BaseDocument, id: NodeId) -> LayoutBox {
    let Some(node) = doc.get_node(id) else { return LayoutBox::default() };
    let l = node.final_layout();
    let (mut x, mut y) = (l.location.x, l.location.y);
    let mut cur = node.layout_parent.get();
    while let Some(p) = cur.and_then(|p| doc.get_node(p)) {
        x += p.final_layout().location.x;
        y += p.final_layout().location.y;
        cur = p.layout_parent.get();
    }
    LayoutBox { x, y, w: l.size.width, h: l.size.height }
}

/// Paint the document into `Frame`.
pub fn paint(mut dom: NonSendMut<Dom>, mut frame: ResMut<Frame>) {
    let (w, h, scale, clear) = (frame.width, frame.height, dom.scale, !frame.transparent);
    let frame = &mut *frame;
    if frame.defer {
        frame.pending = Some(crate::row_render::submit_document(&mut dom.doc, w, h, scale, clear));
        return;
    }
    paint_document(&mut dom.doc, w, h, scale, clear, &mut frame.rgba);
}

// === Styles ===

/// `brand :: Style{...}`: name -> tracks. A style is a bundle: its tracks are applied under the row's own at spawn.
fn styles_of(edit: &Value, timeline: &Timeline) -> Result<HashMap<String, BTreeMap<String, Prop>>, String> {
    let mut out = HashMap::new();
    for (name, fields) in edit["styles"].as_object().into_iter().flatten() {
        let mut tracks = BTreeMap::new();
        for (k, v) in fields.as_object().ok_or(format!("style {name}: expected Style{{...}}"))? {
            let p = parse_prop(v).map_err(|e| format!("style {name}.{k}: {e}"))?;
            if let Prop::Bind(col) = &p {
                if !timeline.edit().data.contains_key(col) {
                    return Err(format!("style {name}.{k}: no data column {col:?}"));
                }
            }
            tracks.insert(k.clone(), p);
        }
        out.insert(name.clone(), tracks);
    }
    Ok(out)
}

/// The row's tracks with its styles underneath: `style = {base, big}` applies base, then big, then the row's own.
fn with_styles(props: &BTreeMap<String, Prop>, styles: &HashMap<String, BTreeMap<String, Prop>>) -> BTreeMap<String, Prop> {
    let names: Vec<String> = match props.get("style") {
        Some(Prop::Const(Value::String(s))) => vec![s.clone()],
        Some(Prop::Const(Value::Array(a))) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => vec![],
    };
    let mut out = BTreeMap::new();
    for n in &names {
        if let Some(s) = styles.get(n) {
            out.extend(s.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }
    out.extend(props.iter().filter(|(k, _)| *k != "style").map(|(k, v)| (k.clone(), v.clone())));
    out
}

// === The World for one edit ===

pub struct RowWorld {
    pub world: World,
    schedule: Schedule,
}

/// Spawn one entity per row of `edit` into `world` (raea's own, or a host's such as a Bevy app's) and add
/// the resources the systems need. Returns row id -> entity, so a host can attach its own components.
pub fn install(world: &mut World, edit: &Value, dir: &Path, scale: f32) -> Result<HashMap<String, Entity>, String> {
    install_mode(world, edit, dir, scale, false)
}

/// As `install`, for a host that computes its rows' values itself (rdb: its edit's columns, every frame): no
/// Tracks; each row's props start as its constants in `Resolved`, and the host writes `Resolved` when values change.
/// Run `hosted_systems`. A prop that isn't a constant (a curve, spring or binding) is refused.
pub fn install_hosted(world: &mut World, edit: &Value, dir: &Path, scale: f32) -> Result<HashMap<String, Entity>, String> {
    install_mode(world, edit, dir, scale, true)
}

fn install_mode(world: &mut World, edit: &Value, dir: &Path, scale: f32, hosted: bool) -> Result<HashMap<String, Entity>, String> {
    let timeline = Timeline::new(Edit::from_value(edit)?);
    let imports = imports_of(edit);
    let (doc, nodes) = build_document(&timeline, &imports, dir, scale);
    let (width, height) = (timeline.edit().width, timeline.edit().height);
    let styles = styles_of(edit, &timeline)?;
    let mut entities: HashMap<String, Entity> = HashMap::new();
    for (tree, &(i, from, to, depth)) in timeline.order().iter().enumerate() {
        let inst = &timeline.edit().instances[i];
        let (el, text) = nodes[&inst.id];
        let props = with_styles(&inst.props, &styles);
        let e = world
            .spawn((
                Name(inst.id.clone()),
                Kind(inst.kind.clone()),
                Order { row: i, tree, depth },
                Span { from, to },
                Visible::default(),
                DomNode { el, text },
                LayoutBox::default(),
            ))
            .id();
        if hosted {
            let mut consts = Map::new();
            for (name, p) in props {
                let Prop::Const(v) = p else { return Err(format!("{}.{name}: a hosted row's props are constants (its host writes them per frame)", inst.id)) };
                consts.insert(name, v);
            }
            world.entity_mut(e).insert(Resolved(consts));
        } else {
            world.entity_mut(e).insert((Tracks(props), Resolved::default()));
        }
        if let Some(p) = inst.parent.as_ref().and_then(|p| entities.get(p)) {
            world.entity_mut(e).insert(ParentRow(*p));
        }
        entities.insert(inst.id.clone(), e);
    }
    world.insert_resource(Clock(0));
    world.insert_resource(DomStats::default());
    world.insert_resource(Imports(imports));
    world.insert_resource(Frame {
        width: (width as f32 * scale).round() as u32,
        height: (height as f32 * scale).round() as u32,
        rgba: Vec::new(),
        transparent: false,
        defer: false,
        pending: None,
    });
    world.insert_resource(Time(timeline));
    world.insert_non_send(Dom { doc, scale });
    Ok(entities)
}

/// The frame's systems in order, for any schedule (raea's, or a host's Update).
pub fn systems() -> bevy_ecs::schedule::ScheduleConfigs<bevy_ecs::system::ScheduleSystem> {
    (visibility, resolve, sync_dom, layout, boxes, paint).chain()
}

/// The frame's systems for rows from `install_hosted`: the host has written `Resolved`; only changed rows are patched.
pub fn hosted_systems() -> bevy_ecs::schedule::ScheduleConfigs<bevy_ecs::system::ScheduleSystem> {
    (visibility_on, sync_dom_changed, layout, boxes, paint).chain()
}

impl RowWorld {
    /// Spawn one entity per row of `edit` (from row::parse, plus any `data` columns); assets resolve against `dir`.
    pub fn new(edit: &Value, dir: &Path, scale: f32) -> Result<RowWorld, String> {
        let mut world = World::new();
        install(&mut world, edit, dir, scale)?;
        Ok(RowWorld::with(world, systems()))
    }

    /// As `new`, for a host that writes each row's `Resolved` itself (see `install_hosted`).
    pub fn new_hosted(edit: &Value, dir: &Path, scale: f32) -> Result<RowWorld, String> {
        let mut world = World::new();
        install_hosted(&mut world, edit, dir, scale)?;
        Ok(RowWorld::with(world, hosted_systems()))
    }

    fn with(world: World, systems: bevy_ecs::schedule::ScheduleConfigs<bevy_ecs::system::ScheduleSystem>) -> RowWorld {
        let mut schedule = Schedule::default();
        schedule.set_executor(SingleThreadedExecutor::new());
        schedule.add_systems(systems);
        RowWorld { world, schedule }
    }

    pub fn frames(&self) -> i64 {
        self.world.resource::<Time>().0.edit().frames
    }

    pub fn fps(&self) -> (u32, u32) {
        self.world.resource::<Time>().0.edit().fps
    }

    pub fn pixel_size(&self) -> (u32, u32) {
        let f = self.world.resource::<Frame>();
        (f.width, f.height)
    }

    /// Run the schedule for frame `f`; the pixels are in `Frame` (and copied into `out`).
    pub fn render(&mut self, f: i64, out: &mut Vec<u8>) {
        self.step(f);
        out.clear();
        out.extend_from_slice(&self.world.resource::<Frame>().rgba);
    }

    /// `render` without waiting for the GPU: frame `f`'s paint, to finish while the caller does other work (the next
    /// frame's composite). A frame whose schedule painted nothing new hands back the last pixels.
    pub fn submit(&mut self, f: i64) -> crate::row_render::Painting {
        self.world.resource_mut::<Frame>().defer = true;
        self.step(f);
        let mut fr = self.world.resource_mut::<Frame>();
        fr.defer = false;
        fr.pending.take().unwrap_or_else(|| crate::row_render::Painting::Done(fr.rgba.clone()))
    }

    /// Run the schedule for frame `f`.
    pub fn step(&mut self, f: i64) {
        self.world.resource_mut::<Clock>().0 = f;
        self.schedule.run(&mut self.world);
    }

    /// The entity for a row id.
    pub fn row(&mut self, id: &str) -> Option<Entity> {
        let mut q = self.world.query::<(Entity, &Name)>();
        q.iter(&self.world).find(|(_, n)| n.0 == id).map(|(e, _)| e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row_render::RowRenderer;

    fn hook() -> (Value, std::path::PathBuf) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/row");
        let mut rows = crate::row::parse(&std::fs::read_to_string(dir.join("hook.row")).unwrap()).unwrap_or_else(|e| panic!("{e}"));
        rows["data"] = serde_json::from_str(&std::fs::read_to_string(dir.join("words.json")).unwrap()).unwrap();
        (rows, dir)
    }

    #[test]
    fn the_world_paints_what_the_slice_1_renderer_paints() {
        let (rows, dir) = hook();
        let mut old = RowRenderer::new(&rows, &dir, 0.25).unwrap();
        let mut new = RowWorld::new(&rows, &dir, 0.25).unwrap();
        assert_eq!(old.pixel_size(), new.pixel_size());
        let (mut a, mut b) = (Vec::new(), Vec::new());
        // every frame forward, then a few seeks back
        for f in (0..new.frames()).chain([120, 3, 96, 0]) {
            old.render(f, &mut a);
            new.render(f, &mut b);
            assert!(a == b, "frame {f} differs");
        }
    }

    #[test]
    fn rows_are_entities_and_layout_writes_boxes_back() {
        let (rows, dir) = hook();
        let mut w = RowWorld::new(&rows, &dir, 0.25).unwrap();
        let hook = w.row("hook").unwrap();
        let bg = w.row("bg").unwrap();
        assert_eq!(w.world.get::<ParentRow>(hook).unwrap().0, bg);
        assert_eq!(w.world.get::<Kind>(hook).unwrap().0, "title");
        w.step(20);
        let v = *w.world.get::<Visible>(hook).unwrap();
        assert!(v.on && v.frame == 20);
        let b = *w.world.get::<LayoutBox>(hook).unwrap();
        assert!(b.w > 0.0 && b.h > 0.0, "the title has a box: {b:?}");
        let y20 = b.y;
        assert_eq!(w.world.get::<Resolved>(hook).unwrap().0["y"], serde_json::json!(620));
        w.step(2);
        assert!(w.world.get::<LayoutBox>(hook).unwrap().y > y20, "it moves up into place");
        w.step(95);
        assert!(!w.world.get::<Visible>(hook).unwrap().on, "and is gone after its span");
        assert_eq!(*w.world.get::<LayoutBox>(hook).unwrap(), LayoutBox::default());
    }
}
