// @raea/motion/music: Tone-shaped notes (Transport, Part, Sequence) compiled to note rows, shown as <audio data-note>
// while they sound, and MIDI out and back in.
import { expect, test } from "bun:test";
import { Music, Part, Sequence, Transport, compile, midiOf, parseMidi, toMidi } from "../src/music";
import { renderFrame } from "../src/test";
import * as Tone from "../src/tone";

const t = new Transport({ bpm: 120, timeSignature: [4, 4] });

test("Tone's times in seconds at 120 bpm", () => {
  expect(t.toSeconds("4n")).toBe(0.5);
  expect(t.toSeconds("8n")).toBe(0.25);
  expect(t.toSeconds("1m")).toBe(2);
  expect(t.toSeconds("1:2:2")).toBe(3.25);
  expect(t.toSeconds("8t")).toBeCloseTo(1 / 6, 12);
  expect(t.toSeconds("4n.")).toBe(0.75);
  expect(midiOf("A4")).toBe(69);
  expect(midiOf("C4")).toBe(60);
  expect(midiOf("Bb3")).toBe(58);
});

test("a Sequence's steps, a nested step split evenly, looped to the end", () => {
  const s = new Sequence("hat", ["C1", null, ["C1", "C1"]], "4n", { loop: true });
  const rows = compile(t, [s], 3);
  expect(rows.map((r) => r.start)).toEqual([0, 1, 1.25, 1.5, 2.5, 2.75]);
});

test("a Part with chords loops over its loop length", () => {
  const p = new Part("pad", [{ time: 0, note: ["C4", "E4"], duration: "2n" }], { loop: "1m" });
  const rows = compile(t, [p], 4);
  expect(rows.length).toBe(4);
  expect(rows.map((r) => r.start)).toEqual([0, 0, 2, 2]);
});

test("<Music> plays its notes as Tone events, each once, on its own time", () => {
  Tone.reset();
  const p = new Part("pluck", [{ time: 0.51, note: "A4", duration: 0.1 }]);
  const at = (frame: number) => renderFrame(<Music transport={t} parts={[p]} />, { frame, fps: 30, width: 10, height: 10, durationInFrames: 60 });
  at(14);
  at(15);
  at(16);
  const evs = Tone.snapshot(2).events.filter((e) => e.k.startsWith("music:"));
  expect(evs.length).toBe(1);
  expect(evs[0].t).toBe(0.51);
  expect(evs[0].f![0]).toBeCloseTo(440, 6);
  expect(Tone.snapshot(0).graph.nodes.find((n: any) => n.id === evs[0].n).kind).toBe("PluckSynth");
});

test("MIDI out and back in keeps every note", () => {
  const parts = [new Sequence("kick", ["C1", null, "C1", null], "8n", { loop: true }), new Part("bass", [{ time: "0:1:0", note: "E2", duration: "8n", velocity: 0.5 }], { loop: "1m" })];
  const rows = compile(t, parts, 4);
  const midi = parseMidi(toMidi(t, parts, 4));
  expect(midi.header.ppq).toBe(480);
  expect(midi.header.tempos[0].bpm).toBeCloseTo(120, 6);
  const back = midi.tracks.flatMap((tr) => tr.notes.map((n) => [Math.round(n.time * 1e6) / 1e6, Math.round(n.duration * 1e6) / 1e6]));
  const want = rows.map((r) => [r.start, r.duration]);
  expect(back.sort()).toEqual(want.sort());
  expect(midi.tracks[2].notes[0].midi).toBe(40);
  expect(midi.tracks[2].notes[0].velocity).toBeCloseTo(64 / 127, 6);
});
