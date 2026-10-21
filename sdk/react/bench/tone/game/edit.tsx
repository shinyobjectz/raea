// A game with sound (@raea/motion/tone): a ball run by a state machine (sdk/motion/src/machine.ts), its landings firing a
// MembraneSynth one-shot on their exact frame (<Sfx>: seeded pitch and volume variation, at most 4 voices, the oldest
// stolen), a coin every 45th frame (an SAO sound, made once, round-robin over two pitches); a Tone music bed on the
// music bus, ducked 12 dB under a voice line on the voice bus (voice.wav, made by the check: `say`, not committed).
// The render's bounce and a play's tap must be the same samples (sdk/react/bench/tone/check.sh).
import { AbsoluteFill, Audio, Composition, Mix, Sequence, machine, registerRoot, staticFile, useCurrentFrame, useGameEvents, useGameState } from "@raea/motion";
import type { Snapshot } from "@raea/motion/machine";
import * as Tone from "@raea/motion/tone";

const FLOOR = 300;

// ---- the game: a ball whose life is a machine --------------------------------------------------------------------
interface Ball { y: number; vy: number; sm: Snapshot }
const ball = machine<Ball, null>()({
  id: "ball",
  initial: "fall",
  states: {
    fall: { tick: (b) => ({ ...b, vy: b.vy + 0.8, y: Math.min(FLOOR, b.y + b.vy + 0.8) }), always: { to: "land", when: (b) => b.y >= FLOOR } },
    land: { tick: (b) => ({ ...b, y: FLOOR, vy: -Math.max(9, Math.abs(b.vy) * 0.8) }), after: { 1: "rise" } },
    rise: { tick: (b) => ({ ...b, vy: b.vy + 0.8, y: b.y + b.vy + 0.8 }), always: { to: "fall", when: (b) => b.vy >= 0 } },
  },
});
interface Game { ball: Ball; frame: number }
const INITIAL: Game = { ball: { y: 40, vy: 0, sm: ball.start() }, frame: 0 };
const fold = (g: Game, _input: unknown, frame: number): Game => ({ ball: ball.step(g.ball, null), frame });
const landed = (b: Game, a: Game) => ball.state(a.ball) === "land" && ball.state(b.ball) !== "land";

// ---- the sound -----------------------------------------------------------------------------------------------------
Tone.getTransport().bpm.value = 100;
Tone.buses.music.duck(Tone.buses.voice, { amount: -12, threshold: -45, attack: 0.04, release: 0.4 });
Tone.buses.music.volume.value = -4;
const thump = Tone.vary(new Tone.MembraneSynth({ pitchDecay: 0.03, octaves: 4, envelope: { attack: 0.001, decay: 0.25, sustain: 0 }, maxPolyphony: 4, poly: true }), { pitch: 0.5, volume: 2 }).connect(Tone.buses.sfx);
const coin = new Tone.Sampler({ urls: { C5: Tone.sao("a bright arcade coin chime, short", { seconds: 1 }) }, release: 0.2 }).connect(Tone.buses.sfx);
const pad = new Tone.PolySynth(Tone.Synth, { oscillator: { type: "fatsine", count: 3, spread: 12 }, envelope: { attack: 0.4, decay: 0.5, sustain: 0.8, release: 1.5 }, volume: -10 }).connect(Tone.buses.music);
const pluck = new Tone.PluckSynth({ volume: -4 }).connect(new Tone.FeedbackDelay("8n", 0.25).connect(Tone.buses.music));
const chords = new Tone.Part((t, c) => pad.triggerAttackRelease(c, "2m", t), [["0:0", ["C4", "E4", "G4"]], ["2:0", ["A3", "C4", "E4"]]]);
chords.loop = true;
chords.loopEnd = "4m";
chords.start(0);
new Tone.Sequence((t, n) => pluck.triggerAttackRelease(n, "8n", t), ["C5", "G4", "E5", null, "D5", "G4", "C5", null], "8n").start(0);
Tone.getTransport().start();

function Game() {
  const frame = useCurrentFrame();
  const g = useGameState(INITIAL, fold);
  const lands = useGameEvents(INITIAL, fold, landed);
  const coinNow = frame > 0 && frame % 45 === 0;
  return (
    <AbsoluteFill style={{ background: "#0e1620" }}>
      <Mix lufs={-14} ceiling={-1.5} />
      <div style={{ position: "absolute", left: 300, top: g.ball.y, width: 40, height: 40, borderRadius: 20, background: "#f2c14e" }} />
      <div style={{ position: "absolute", left: 0, right: 0, top: FLOOR + 40, height: 4, background: "#557" }} />
      <Tone.Sfx when={lands.includes(frame)} play={(t) => thump.triggerAttackRelease("C2", "8n", t)} />
      <Tone.Sfx when={coinNow} play={(t, f) => coin.triggerAttackRelease((f / 45) % 2 ? "C5" : "G5", "8n", t)} />
      <Sequence from={90} durationInFrames={150}>
        <Audio src={staticFile("voice.wav")} bus="voice" />
      </Sequence>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="game" component={Game} width={640} height={400} fps={30} durationInFrames={300} />);
