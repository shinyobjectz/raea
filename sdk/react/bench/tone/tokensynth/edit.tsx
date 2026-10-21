// A Sampler filled by TokenSynth (@raea/motion/tone `tokensynth`): two root notes of "a warm upright bass" rendered
// locally once (CPU), cached as PCM by their timbre, note, velocity, seed and device, normalised, and pitched between by
// the Sampler; a bass line over a kick. Rendering makes the roots if they aren't made (tools/tokensynth/setup.sh); a
// play only finds them. The render's bounce and a play's tap must be the same samples.
import { AbsoluteFill, Composition, Mix, registerRoot } from "@raea/motion";
import * as Tone from "@raea/motion/tone";

Tone.getTransport().bpm.value = 96;
const bass = new Tone.Sampler({ urls: Tone.tokensynth("a warm upright bass", ["C2", "C3"], { seconds: 1.5 }), release: 0.3 }).connect(Tone.buses.music);
const kick = new Tone.MembraneSynth({ octaves: 5, volume: -6 }).connect(Tone.buses.music);
new Tone.Sequence((t, n) => kick.triggerAttackRelease(n, "8n", t), ["C1", null, "C1", null], "4n").start(0);
new Tone.Part((t, n) => bass.triggerAttackRelease(n, "4n", t), [
  ["0:0", "C2"], ["0:1", "E2"], ["0:2", "G2"], ["0:3", "A2"],
  ["1:0", "F2"], ["1:1", "A2"], ["1:2", "C3"], ["1:3", "D3"],
  ["2:0", "G2"], ["2:1", "B2"], ["2:2", "D3"], ["2:3", "F3"],
  ["3:0", "C2"], ["3:1", "G2"], ["3:2", "C3"],
]).start(0);
Tone.getTransport().start();

const Edit = () => (
  <AbsoluteFill style={{ background: "#16131c" }}>
    <Mix lufs={-14} ceiling={-1.5} />
  </AbsoluteFill>
);

registerRoot(() => <Composition id="tokensynth" component={Edit} width={640} height={360} fps={30} durationInFrames={330} />);
