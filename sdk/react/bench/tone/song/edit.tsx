// A timeline with a Tone track (@raea/motion/tone), cut on the bar: eight cards, one a bar (Tone.Time("1m").toFrames),
// over drums, bass, chords and a lead written as Tone's docs write them, through effects into the music bus, mastered
// to -14 LUFS. The render's bounce and a play's tap must be the same samples (sdk/react/bench/tone/check.sh).
import { AbsoluteFill, Composition, Mix, Series, registerRoot, useCurrentFrame } from "@raea/motion";
import * as Tone from "@raea/motion/tone";

const FPS = 30;
const BARS = 8;
Tone.getTransport().bpm.value = 120;
const BAR = Tone.Time("1m").toFrames(FPS);

// effects: a delay into a reverb into the music bus, a compressor on the drums
const reverb = new Tone.Reverb({ decay: 2.5, wet: 0.3 }).connect(Tone.buses.music);
const delay = new Tone.FeedbackDelay("8n.", 0.3).connect(reverb);
const drumBus = new Tone.Compressor(-18, 4).connect(Tone.buses.music);

const kick = new Tone.MembraneSynth({ pitchDecay: 0.05, octaves: 6, envelope: { attack: 0.001, decay: 0.4, sustain: 0, release: 0.1 } }).connect(drumBus);
const hat = new Tone.MetalSynth({ frequency: 400, resonance: 6000, envelope: { decay: 0.05 }, volume: -20 }).connect(drumBus);
const snare = new Tone.NoiseSynth({ envelope: { attack: 0.001, decay: 0.15, sustain: 0 }, volume: -8 }).connect(drumBus);
const bass = new Tone.MonoSynth({ oscillator: { type: "sawtooth" }, filter: { Q: 2, rolloff: -24 }, filterEnvelope: { attack: 0.005, decay: 0.2, sustain: 0.3, baseFrequency: 120, octaves: 3 }, volume: -6 }).connect(Tone.buses.music);
const chords = new Tone.PolySynth(Tone.Synth, { oscillator: { type: "fatsawtooth", count: 3, spread: 20 }, envelope: { attack: 0.2, decay: 0.3, sustain: 0.6, release: 1 }, volume: -16 }).connect(reverb);
const lead = new Tone.Synth({ oscillator: { type: "triangle" }, envelope: { attack: 0.01, decay: 0.2, sustain: 0.2, release: 0.4 }, volume: -8 }).connect(delay);

new Tone.Sequence((time, note) => kick.triggerAttackRelease(note, "8n", time), ["C1", null, "C1", null], "4n").start(0);
new Tone.Sequence((time) => snare.triggerAttackRelease("16n", time), [null, "x"], "4n").start(0);
new Tone.Loop((time) => hat.triggerAttackRelease("32n", time, 0.5), "8n").start(0);
const progression = new Tone.Part((time, chord) => chords.triggerAttackRelease(chord, "1m", time), [
  ["0:0", ["A3", "C4", "E4"]],
  ["1:0", ["F3", "A3", "C4"]],
  ["2:0", ["C3", "E3", "G3"]],
  ["3:0", ["G3", "B3", "D4"]],
]);
progression.loop = true;
progression.loopEnd = "4m";
progression.start(0);
new Tone.Part((time, note) => bass.triggerAttackRelease(note, "8n", time), [
  ["0:0", "A1"], ["0:2", "A1"], ["1:0", "F1"], ["1:2", "F1"], ["2:0", "C2"], ["2:2", "C2"], ["3:0", "G1"], ["3:2", "G1"],
  ["4:0", "A1"], ["4:2", "A1"], ["5:0", "F1"], ["5:2", "F1"], ["6:0", "C2"], ["6:2", "C2"], ["7:0", "G1"], ["7:2", "G1"],
]).start(0);
const arp = new Tone.Pattern((time, note) => lead.triggerAttackRelease(note, "16n", time), ["A4", "C5", "E5", "G5"], "upDown");
arp.interval = "8n";
arp.start("4m");
Tone.getTransport().start();

const COLORS = ["#1f2a44", "#3b1f44", "#1f443b", "#44381f", "#1f3444", "#441f2a", "#2a441f", "#2a1f44"];

function Card({ bar }: { bar: number }) {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: COLORS[bar], justifyContent: "center", alignItems: "center", color: "white", fontSize: 140, fontWeight: 800, fontFamily: "Inter, sans-serif", opacity: Math.min(1, 0.4 + f / 10) }}>
      {`bar ${bar + 1}`}
    </AbsoluteFill>
  );
}

function Song() {
  return (
    <AbsoluteFill>
      <Mix lufs={-14} ceiling={-1} />
      <Series>
        {Array.from({ length: BARS }, (_, i) => (
          <Series.Sequence key={i} durationInFrames={BAR}>
            <Card bar={i} />
          </Series.Sequence>
        ))}
      </Series>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="song" component={Song} width={640} height={360} fps={FPS} durationInFrames={BAR * BARS} />);
