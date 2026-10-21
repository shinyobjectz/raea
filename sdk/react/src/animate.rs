//! Declared animations sampled in Rust (raea-animate, motion's <Animate>). An element that carries `data-raea-anim`
//! (the JSON raea_animate::Animation its <Animate> or <Mesh animate> sent once, at mount) becomes an `Animated`
//! entity; each update, after React's commits are applied, every Animated element is sampled at raea's frame clock and
//! what it sets is written where the frame is drawn from: its style keys (page elements), its data-props (3D nodes,
//! so the scene's rows carry it), its text (a counter), on the entity's props and on the page mirror alike. The frame
//! clock is the composition's own: the frame marker each commit carries (data-raea-frame), so a render, live play, a
//! baked timeline's replay and a seek all sample the same function of the same frame. React never re-renders for it.

use bevy::prelude::*;
use raea_animate::bevy::FrameClock;
use serde_json::{Map, Value, json};

use crate::{DomMirror, ReactKind, ReactNode, ReactProps, ReactText};

/// An element's compiled animation, and the text it was compiled from.
#[derive(Component)]
pub struct Animated {
    src: String,
    compiled: raea_animate::Compiled,
    /// React's own `display` for an element with a show window (put back when it shows)
    display: Option<Value>,
}

/// Where the frame marker is, and the fps its config last said.
#[derive(Default)]
pub(crate) struct MarkerCache {
    entity: Option<Entity>,
    config: String,
    fps: f64,
}

/// Animations that failed to compile, said once each.
#[derive(Default)]
pub(crate) struct Warned(std::collections::HashSet<String>);

#[allow(clippy::too_many_arguments)]
pub(crate) fn sample_animations(
    mut commands: Commands,
    mut clock: ResMut<FrameClock>,
    mut q: Query<(Entity, &ReactNode, &mut ReactProps, Option<&Animated>)>,
    kids: Query<&Children>,
    mut texts: Query<(&ReactNode, &ReactKind, &mut ReactText), Without<ReactProps>>,
    mut mirror: Option<NonSendMut<DomMirror>>,
    mut marker: Local<MarkerCache>,
    mut warned: Local<Warned>,
) {
    // the frame: the marker's, found once and kept
    let has_marker = |p: &ReactProps| p.0.contains_key("data-raea-frame");
    if marker.entity.is_none_or(|e| q.get(e).map(|(_, _, p, _)| !has_marker(&p)).unwrap_or(true)) {
        marker.entity = q.iter().find(|(_, _, p, _)| has_marker(p)).map(|(e, ..)| e);
    }
    let Some(me) = marker.entity else { return };
    let Ok((_, _, mp, _)) = q.get(me) else { return };
    let Some(frame) = mp.0.get("data-raea-frame").and_then(Value::as_f64) else { return };
    if frame < 0.0 {
        return;
    }
    if let Some(conf) = mp.0.get("data-raea-config").and_then(Value::as_str) {
        if conf != marker.config {
            marker.fps = serde_json::from_str::<Value>(conf).ok().and_then(|v| v["fps"].as_f64()).unwrap_or(30.0);
            marker.config = conf.to_string();
        }
    }
    let next = FrameClock { frame: frame as i64, fps: if marker.fps > 0.0 { marker.fps } else { 30.0 } };
    if *clock != next {
        *clock = next;
    }
    let f = clock.frame as f64;

    let mut ops: Vec<Value> = Vec::new();
    for (e, node, mut props, animated) in &mut q {
        let Some(src) = props.0.get("data-raea-anim").and_then(Value::as_str) else {
            if animated.is_some() {
                commands.entity(e).remove::<Animated>();
            }
            continue;
        };
        // compiled once per declaration (React sends the same text every frame it doesn't change)
        let fresh;
        let (compiled, display) = match animated {
            Some(a) if a.src == src => (&a.compiled, a.display.clone()),
            _ => {
                // a declaration that leaves fps to the clock (0) counts at the composition's rate
                let parsed = serde_json::from_str::<raea_animate::Animation>(src).map_err(|e| e.to_string()).and_then(|mut a| {
                    if a.fps <= 0.0 {
                        a.fps = clock.fps;
                    }
                    raea_animate::compile(&a)
                });
                // React's display, as it was before a show window ever hid it
                let display = match animated {
                    Some(a) => a.display.clone(),
                    None => props.0.get("style").and_then(|st| st.get("display")).cloned(),
                };
                match parsed {
                    Ok(c) => {
                        commands.entity(e).insert(Animated { src: src.to_string(), compiled: c.clone(), display: display.clone() });
                        fresh = c;
                        (&fresh, display)
                    }
                    Err(err) => {
                        if warned.0.insert(src.to_string()) {
                            warn!("raea-animate: an <Animate> on element {} doesn't compile: {err}", node.0);
                        }
                        continue;
                    }
                }
            }
        };
        let s = compiled.sample(f);
        if props.0.contains_key("data-3d") {
            // a 3D node: its props are a JSON string the scene's rows are read from
            let mut p: Map<String, Value> = props.0.get("data-props").and_then(Value::as_str).and_then(|t| serde_json::from_str(t).ok()).unwrap_or_default();
            if s.patch_3d(&mut p) {
                let text = Value::String(Value::Object(p).to_string());
                props.0.insert("data-props".into(), text.clone());
                ops.push(json!({"op": "update", "id": node.0, "props": {"data-props": text}}));
            }
        } else {
            let mut want = s.css();
            if compiled.show_from.is_some() || compiled.show_until.is_some() {
                want.insert("display".into(), if s.hidden { Value::String("none".into()) } else { display.clone().unwrap_or(Value::Null) });
            }
            let style = props.0.entry("style").or_insert_with(|| Value::Object(Map::new()));
            if !style.is_object() {
                *style = Value::Object(Map::new());
            }
            let style = style.as_object_mut().unwrap();
            let delta: Map<String, Value> = want.into_iter().filter(|(k, v)| style.get(k) != Some(v)).collect();
            if !delta.is_empty() {
                for (k, v) in &delta {
                    style.insert(k.clone(), v.clone());
                }
                ops.push(json!({"op": "update", "id": node.0, "props": {"style": Value::Object(delta)}}));
            }
        }
        // a counter: the element's first text child
        if let Some(t) = &s.text {
            if let Ok(children) = kids.get(e) {
                for c in children.iter() {
                    if let Ok((cn, kind, mut ct)) = texts.get_mut(c) {
                        if kind.0.starts_with('#') {
                            if ct.0 != *t {
                                ct.0 = t.clone();
                                ops.push(json!({"op": "updateText", "id": cn.0, "text": t}));
                            }
                            break;
                        }
                    }
                }
            }
        }
    }
    if !ops.is_empty() {
        if let Some(m) = mirror.as_deref_mut() {
            if crate::mirror_ops(m, &ops) {
                m.dirty = true;
            }
        }
    }
}
