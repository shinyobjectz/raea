// The Tone layer's newer parts on the one mixer core: an LFO on a filter's frequency, exponential ramps, the
// Transport looping a bar while its tempo ramps from 90 to 140 bpm, a Convolver on a generated impulse response
// (media/ir.wav, check.sh writes it), a PitchShift a fifth up, and Tone's three-band EQ3. The render's bounce and a
// play's tap must be the same samples (sdk/react/bench/tone/check.sh).
import { AbsoluteFill, Composition, Mix, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import * as Tone from "@raea/motion/tone";

const FPS = 30;
const SECONDS = 8;

const hall = new Tone.Convolver(staticFile("ir.wav")).connect(Tone.buses.music);
hall.wet.value = 0.35;
const eq = new Tone.EQ3(-6, 2, -3).connect(hall);
const filter = new Tone.Filter(800, "lowpass").connect(eq);
const lfo = new Tone.LFO("2n", 300, 3000).start();
lfo.connect(filter.frequency);
const shift = new Tone.PitchShift(7).connect(Tone.buses.music);
shift.wet.value = 0.5;
const sweep = new Tone.Filter(200, "lowpass").connect(shift);
sweep.frequency.exponentialRampTo(6000, 4, 1);

const pad = new Tone.PolySynth(Tone.Synth, { oscillator: { type: "sawtooth" }, envelope: { attack: 0.05, decay: 0.2, sustain: 0.5, release: 0.6 }, volume: -14 }).connect(filter);
const lead = new Tone.Synth({ oscillator: { type: "square" }, envelope: { attack: 0.01, decay: 0.1, sustain: 0.3, release: 0.2 }, volume: -16 }).connect(sweep);
const kick = new Tone.MembraneSynth({ volume: -6 }).connect(Tone.buses.music);

Tone.getTransport().bpm.value = 90;
Tone.getTransport().bpm.rampTo(140, 6);
Tone.getTransport().setLoopPoints(0, "1m");
Tone.getTransport().loop = true;
new Tone.Loop((time) => kick.triggerAttackRelease("C1", "8n", time), "4n").start(0);
Tone.getTransport().schedule((time) => pad.triggerAttackRelease(["A3", "C4", "E4"], "2n", time), 0);
new Tone.Sequence((time, note) => lead.triggerAttackRelease(note, "16n", time), ["A4", "C5", "E5", "G5"], "8n").start(0);
Tone.getTransport().start();

function Gaps() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#13202e", justifyContent: "center", alignItems: "center", color: "white", fontSize: 72, fontWeight: 800, fontFamily: "Inter, sans-serif" }}>
      <Mix lufs={-14} ceiling={-1} />
      {`${(f / FPS).toFixed(1)} s`}
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="gaps" component={Gaps} width={640} height={360} fps={FPS} durationInFrames={FPS * SECONDS} />);
