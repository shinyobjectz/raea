//! An edit's Tone graph (@raea/motion/tone writes it once per edit as JSON: nodes with Tone's kinds and options, and
//! the connections between them) built on the core's Firewheel graph. Each instrument plays from its own pair of the
//! voices node's outputs; every other Tone node (effects, Channel, Volume, Gain, Panner, Destination, buses) is a
//! ToneFx node; Tone's connections are Firewheel edges, a ducked channel's key on its sidechain input; the
//! Destination goes to the master. Live and offline build the same graph from the same JSON.
//!
//! The JSON: {"nodes": [{"id": 3, "kind": "Synth", "options": {...}, "name": "music"?}], "edges": [[from, to, input]],
//! "mods": [[lfo, node, param]]} where `to` 0 is the Destination, `input` 1 is a sidechain (a duck's key), and each mod
//! is an LFO connected to an effect's parameter (the LFO's settings go to that node, which runs it on the timeline).
//! A Convolver's impulse response (`url`) is decoded here and handed to its node.

use std::collections::HashMap;
use std::sync::Arc;

use firewheel::FirewheelContext;
use firewheel::node::NodeID;
use serde_json::Value;

use crate::fx::FxNode;
use crate::pcm::Pcm;
use crate::tone::{Inst, Kind};

/// The voices node's stereo outputs: port 0 goes straight to the master, the rest to instruments and buses.
pub const PORTS: usize = 32;

#[derive(Default)]
pub struct Built {
    pub hash: u64,
    /// each instrument: its settings and the voices port it plays from
    pub insts: HashMap<u32, (Arc<Inst>, usize)>,
    /// each other Tone node's Firewheel node
    pub fx: HashMap<u32, NodeID>,
    /// a named channel (a bus: music, sfx, voice, ambience, ui) and the voices port that feeds it
    pub bus_port: HashMap<String, usize>,
    /// connections made from the voices node (to undo on a rebuild)
    from_voices: Vec<NodeID>,
}

pub fn hash(json: &str) -> u64 {
    json.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

impl Built {
    /// Takes the graph down (a rebuild for a changed graph).
    pub fn tear_down(&mut self, cx: &mut FirewheelContext, voices: NodeID) {
        for n in self.fx.values() {
            let _ = cx.remove_node(*n);
        }
        for n in self.from_voices.drain(..) {
            cx.disconnect_all_between(voices, n);
        }
        *self = Built::default();
    }
}

/// Builds `json` on `cx`: ToneFx nodes, the voices ports, the edges. `origin`: the clock sample of the timeline's 0.
pub fn build(cx: &mut FirewheelContext, voices: NodeID, master: NodeID, json: &str, origin: i64, open: &mut dyn FnMut(&str) -> Option<Arc<Pcm>>) -> Result<Built, String> {
    let spec: Value = serde_json::from_str(json).map_err(|e| format!("the Tone graph: {e}"))?;
    let nodes = spec.get("nodes").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let edges: Vec<(u32, u32, u8)> = spec.get("edges").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|e| Some((e.get(0)?.as_u64()? as u32, e.get(1)?.as_u64()? as u32, e.get(2).and_then(|x| x.as_u64()).unwrap_or(0) as u8))).collect()).unwrap_or_default();
    let mut b = Built { hash: hash(json), ..Default::default() };
    // solo: any soloed channel mutes every channel that isn't
    let soloing = nodes.iter().any(|n| n.get("options").and_then(|o| o.get("solo")).and_then(|v| v.as_bool()) == Some(true));
    let mut port = 1usize;
    // the Destination (id 0): Tone's master volume, into the core's master
    let dest_opts = nodes.iter().find(|n| n.get("id").and_then(|v| v.as_u64()) == Some(0)).and_then(|n| n.get("options").cloned()).unwrap_or(Value::Null);
    let dest = cx.add_node(FxNode { kind: "Destination".into(), params: dest_opts, origin, muted: false, ir: None }, None).map_err(|e| format!("{e:?}"))?;
    // LFOs on parameters: each target node gets its LFOs' settings as "mods" in its options
    let mods: Vec<(u32, u32, String)> = spec.get("mods").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|m| Some((m.get(0)?.as_u64()? as u32, m.get(1)?.as_u64()? as u32, m.get(2)?.as_str()?.to_string()))).collect()).unwrap_or_default();
    let opts_of = |id: u32| nodes.iter().find(|n| n.get("id").and_then(|v| v.as_u64()) == Some(id as u64)).and_then(|n| n.get("options").cloned()).unwrap_or(Value::Null);
    cx.connect(dest, master, &[(0, 0), (1, 1)], false).map_err(|e| format!("{e:?}"))?;
    b.fx.insert(0, dest);
    for n in &nodes {
        let Some(id) = n.get("id").and_then(|v| v.as_u64()).map(|v| v as u32) else { continue };
        if id == 0 {
            continue;
        }
        let kind = n.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        if kind == "LFO" {
            continue;
        }
        let mut opts = n.get("options").cloned().unwrap_or(Value::Null);
        let mine: Vec<Value> = mods.iter().filter(|m| m.1 == id).map(|(lfo, _, p)| {
            let mut o = opts_of(*lfo);
            if let Value::Object(m) = &mut o {
                m.insert("p".into(), Value::String(p.clone()));
            }
            o
        }).collect();
        if !mine.is_empty()
            && let Value::Object(m) = &mut opts
        {
            m.insert("mods".into(), Value::Array(mine));
        }
        let ir = (kind == "Convolver").then(|| opts.get("url").and_then(|u| u.as_str()).and_then(|f| raea_media::decode_audio_window(std::path::Path::new(f), 0.0, 30.0).ok()).map(Arc::new)).flatten();
        if let Some(k) = Kind::parse(kind) {
            if port >= PORTS {
                return Err(format!("the Tone graph has more than {} instruments and buses", PORTS - 1));
            }
            b.insts.insert(id, (Arc::new(Inst::parse(k, &opts, id as u64, open)), port));
            port += 1;
        } else {
            let muted = soloing && matches!(kind, "Channel" | "Solo") && opts.get("solo").and_then(|v| v.as_bool()) != Some(true);
            let node = cx.add_node(FxNode { kind: kind.into(), params: opts.clone(), origin, muted, ir }, None).map_err(|e| format!("{e:?}"))?;
            b.fx.insert(id, node);
            if let Some(name) = n.get("name").and_then(|v| v.as_str()) {
                if port >= PORTS {
                    return Err(format!("the Tone graph has more than {} instruments and buses", PORTS - 1));
                }
                b.bus_port.insert(name.to_string(), port);
                let p = port as u32 * 2;
                cx.connect(voices, node, &[(p, 0), (p + 1, 1)], false).map_err(|e| format!("{e:?}"))?;
                b.from_voices.push(node);
                port += 1;
            }
        }
    }
    for (from, to, input) in edges {
        let Some(dst) = b.fx.get(&to).copied() else { continue };
        let ins = if input == 1 { (2u32, 3u32) } else { (0, 1) };
        if let Some((_, p)) = b.insts.get(&from) {
            let p = *p as u32 * 2;
            cx.connect(voices, dst, &[(p, ins.0), (p + 1, ins.1)], false).map_err(|e| format!("{e:?}"))?;
            b.from_voices.push(dst);
        } else if let Some(src) = b.fx.get(&from).copied() {
            cx.connect(src, dst, &[(0, ins.0), (1, ins.1)], true).map_err(|e| format!("the Tone graph has a cycle at {from} -> {to}: {e:?}"))?;
        }
    }
    Ok(b)
}
