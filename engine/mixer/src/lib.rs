//! raea's one mixer core. What reaches the speaker or the file is PCM through one Firewheel graph (voices, then the
//! master's gain and true-peak limiter) that runs live in the device callback (`live::Live`) and offline as a render's
//! bounce (`offline::bounce`). Both are fed by the same rule (`score::Score`): a frame's sounds and notes become voice
//! events on the frame's sample of the timeline. So the bounce of a run of frames is the live tap of the same frames,
//! sample for sample, once aligned by where the timeline started on the device's clock.
//!
//! Neural models never sit here: generated sound is rendered once, cached as PCM keyed by its inputs, and comes in as a
//! plain source like any file (sdk/react/docs/audio-stack.md).

pub mod conv;
pub mod dsp;
pub mod fx;
pub mod live;
pub mod nodes;
pub mod offline;
pub mod pcm;
pub mod score;
pub mod tone;
pub mod tonegraph;

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The version of the core's mastering (a raw bounce, its gain, the limiter): caches of a master or a mastered track
/// carry it, so one an earlier core made isn't taken for this one's.
pub const MASTER_VERSION: u32 = 3;

/// The core's rate: a render's bounce, and live play when the device takes it.
pub const RATE: u32 = 48_000;

/// One element's sound in a frame: its source at its source time, gain, rate and fades.
#[derive(Clone, Debug)]
pub struct Sound {
    /// shared, not copied: a frame's sounds are made again each frame from the page (ZeroHeap)
    pub key: Arc<str>,
    pub file: Arc<Path>,
    /// seconds into the source this frame shows
    pub at: f64,
    pub volume: f32,
    pub rate: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    /// the bus it plays into (a named channel of the edit's Tone graph: music, sfx, voice, ambience, ui)
    pub bus: Option<Arc<str>>,
}

/// A frame's Tone (@raea/motion/tone): the edit's graph (JSON, the same every frame; its sources' paths resolved) and
/// the events shown this frame (each with its key, node, time on the timeline and what it does).
#[derive(Clone, Debug, Default)]
pub struct ToneCue {
    pub graph: Option<String>,
    pub evs: Vec<serde_json::Value>,
    /// the frame's own samples from a graph stepped beside the game (raea-seedling: bevy_seedling's), exactly the
    /// frame's span of the timeline; a voice of the core from the frame's sample
    pub pcm: Option<std::sync::Arc<raea_media::Audio>>,
    /// the frame's speech (@raea/motion/speech: lines said by Pocket TTS), exactly the frame's span of the timeline at
    /// RATE, mono; played on the Tone graph's voice bus when it has one (so the bed ducks under it), else the master
    pub speech: Option<std::sync::Arc<raea_media::Audio>>,
}

impl ToneCue {
    /// Whether the frame has anything for the mixer.
    pub fn is_empty(&self) -> bool {
        self.graph.is_none() && self.evs.is_empty() && self.pcm.is_none() && self.speech.is_none()
    }
}

/// A source a Tone graph names (a Sampler's sample, a Player's file), opened whole: it's short, and the audio thread
/// never waits for its blocks.
pub fn open_whole(file: &str) -> Option<std::sync::Arc<pcm::Pcm>> {
    let p = pcm::Pcm::open(Path::new(file), true)?;
    let blocks = (p.len_s / pcm::BLOCK).ceil().clamp(1.0, 64.0) as usize;
    for k in 0..blocks {
        p.ensure(k as f64 * pcm::BLOCK * p.rate);
    }
    Some(p)
}

/// `a` as a 32-bit float WAV (stereo when it has two channels).
pub fn write_wav(a: &raea_media::Audio, out: &Path) -> Result<(), String> {
    let ch = a.channels.len().clamp(1, 2);
    let n = a.len();
    let data = (n * ch * 4) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&(ch as u16).to_le_bytes());
    b.extend_from_slice(&a.rate.to_le_bytes());
    b.extend_from_slice(&(a.rate * ch as u32 * 4).to_le_bytes());
    b.extend_from_slice(&((ch * 4) as u16).to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for i in 0..n {
        for c in 0..ch {
            b.extend_from_slice(&a.channels[c][i].to_le_bytes());
        }
    }
    std::fs::write(out, b).map_err(|e| format!("{}: {e}", out.display()))
}

/// The graph both lanes build: voices into the master into the graph's output.
pub(crate) fn graph(cx: &mut firewheel::FirewheelContext, stats: std::sync::Arc<nodes::Stats>, tap: Option<std::sync::Arc<std::sync::Mutex<Vec<f32>>>>, mute: bool) -> Result<(firewheel::node::NodeID, firewheel::node::NodeID), String> {
    let voices = cx.add_node(nodes::VoicesNode { stats }, None).map_err(|e| format!("voices node: {e:?}"))?;
    let master = cx.add_node(nodes::MasterNode { tap, mute }, None).map_err(|e| format!("master node: {e:?}"))?;
    let out = cx.graph_out_node_id();
    cx.connect(voices, master, &[(0, 0), (1, 1)], false).map_err(|e| format!("connect: {e:?}"))?;
    cx.connect(master, out, &[(0, 0), (1, 1)], false).map_err(|e| format!("connect: {e:?}"))?;
    Ok((voices, master))
}

/// The master's settings as an event: `gain_db` on the sum, the limiter's ceiling at `ceiling_db` (dBTP).
pub(crate) fn master_event(gain_db: f64, ceiling_db: f64) -> firewheel::event::NodeEventType {
    nodes::event(nodes::Master { pre: 10f64.powf(gain_db / 20.0) as f32, ceiling: 10f64.powf(ceiling_db / 20.0) as f32, bypass: false })
}
