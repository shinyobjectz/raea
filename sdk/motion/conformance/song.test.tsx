// song(), songAt() and useSong(), from spec/motion.feature ("Pacing to a song's energy"): a small map in the shape
// `raea-motion song` writes, at 120 BPM (a beat every 0.5 s, a bar every 2 s), read frame by frame.
import { beforeEach, describe, expect, test } from "bun:test";
import { Sequence, SongPlot, song, songAt, useSong, type SongMap } from "../src";
import { renderFrame } from "../src/test";

const beats = Array.from({ length: 32 }, (_, i) => i * 0.5); // 16 s, 8 bars
const ramp = beats.map((_, i) => (i < 8 ? 0.2 : i < 14 ? 0.2 + (i - 8) * 0.1 : i < 16 ? 0.1 : 0.9));
const map: SongMap = {
  version: 1,
  duration: 16,
  tempo: 120,
  beatsPerBar: 4,
  beats,
  downbeats: beats.filter((_, i) => i % 4 === 0),
  features: { loudness: ramp, onsets: ramp, brightness: ramp, low: ramp, high: ramp, energy: ramp },
  bars: Array.from({ length: 8 }, (_, j) => ({ time: j * 2, beat: j * 4, beats: 4, energy: j < 2 ? 0.2 : j < 4 ? 0.5 : 0.9, cuts: j < 2 ? 0.5 : j < 4 ? 1 : 2 })),
  sections: [
    { name: "Verse", level: "low", time: 0, end: 4, beat: 0, bar: 0, energy: 0.2, cuts: 0.5 },
    { name: "Pre-Chorus", level: "mid", time: 4, end: 8, beat: 8, bar: 2, energy: 0.5, cuts: 1 },
    { name: "Chorus", level: "high", time: 8, end: 16, beat: 16, bar: 4, energy: 0.9, cuts: 2 },
  ],
  events: [
    { kind: "build", time: 4, end: 7, beat: 8, bar: 2, beats: 6, strength: 0.5, into: 8 },
    { kind: "stop", time: 7, end: 8, beat: 14, bar: 3, beats: 2, strength: 0.4, into: 8, hits: [{ time: 7.5, at: 7.51, strength: 0.6 }] },
    { kind: "drop", time: 8, beat: 16, bar: 4, strength: 0.8, rise: 0.7, holds: true, afterStop: true, afterBuild: true },
    { kind: "hit", time: 12.25, at: 12.26, beat: 24, bar: 6, strength: 0.9, over: 3 },
  ],
  sectionsFrom: "markers",
};

let got: any;
beforeEach(() => { got = undefined; });
const at = (node: any, frame: number) => renderFrame(node, { frame, fps: 30, width: 1080, height: 1920, durationInFrames: 480 });
function Ask({ f }: { f: () => any }) { got = f(); return null; }

describe("songAt", () => {
  test("the beat, the bar, the place in each", () => {
    const m = songAt(map, 5.25);
    expect(m.beat).toBe(10);
    expect(m.bar).toBe(2);
    expect(m.beatInBar).toBe(2);
    expect(m.phase).toBeCloseTo(0.5, 9);
    expect(m.barPhase).toBeCloseTo(2.5 / 4, 9);
  });
  test("features interpolate between beats", () => {
    expect(songAt(map, 5.25).energy).toBeCloseTo(0.2 + 2.5 * 0.1, 9);
  });
  test("the section and its cut rate", () => {
    const m = songAt(map, 9);
    expect(m.section?.name).toBe("Chorus");
    expect(m.sectionProgress).toBeCloseTo(1 / 8, 9);
    expect(m.cutsPerBar).toBe(2);
  });
  test("inside a build: its progress, and the drop ahead", () => {
    const m = songAt(map, 5.5);
    expect(m.inBuild).toBe(true);
    expect(m.buildProgress).toBeCloseTo(0.5, 9);
    expect(m.untilDrop).toBeCloseTo(2.5, 9);
    expect(m.sinceDrop).toBe(Infinity);
    expect(m.next?.kind).toBe("stop");
  });
  test("a stop, then the drop and time since it", () => {
    expect(songAt(map, 7.6).inStop).toBe(true);
    const m = songAt(map, 9);
    expect(m.drop?.time).toBe(8);
    expect(m.sinceDrop).toBeCloseTo(1, 9);
    expect(m.untilDrop).toBe(Infinity);
    expect(m.inBuild || m.inStop).toBe(false);
  });
  test("before the first beat", () => {
    expect(songAt(map, -1).beat).toBe(-1);
  });
  test("a map that isn't one fails, saying how to make one", () => {
    expect(() => songAt({} as SongMap, 0)).toThrow(/raea-motion song/);
  });
});

describe("song", () => {
  const s = song(map, { fps: 30 });
  test("frames from song times, offset by where frame 0 sits", () => {
    expect(s.frame(8)).toBe(240);
    expect(song(map, { fps: 30, offset: 4 }).frame(8)).toBe(120);
    expect(s.at(240).drop?.time).toBe(8);
  });
  test("onBeat and onDownbeat list frames, every nth, in a range", () => {
    expect(s.onBeat(1, { to: 60 })).toEqual([0, 15, 30, 45]);
    expect(s.onBeat(2, { from: 30, to: 120 })).toEqual([30, 60, 90]);
    expect(s.onDownbeat(2)).toEqual([0, 120, 240, 360]);
    expect(() => s.onBeat(0)).toThrow();
  });
  test("eventsOf gives each event's frame", () => {
    expect(s.eventsOf("hit").map((e) => e.frame)).toEqual([368]);
  });
  test("cutsAt paces cuts to each section's rate, the drop, a stop's hits and a build's second half", () => {
    const cuts = s.cutsAt();
    // verse: 0.5 a bar, from its first bar: second 0 only (bar 1 skipped)
    expect(cuts.filter((f) => f < 120)).toEqual([0]);
    // pre-chorus at 1 a bar (4 s); the bar at 6 s starts in the build's second half: 2 a bar (6 s, 7 s); the stop's hit
    expect(cuts.filter((f) => f >= 120 && f < 240)).toEqual([120, 180, 210, 225]);
    // the drop, then 2 a bar through the chorus, and the hit isn't a cut by itself
    expect(cuts.filter((f) => f >= 240 && f < 300)).toEqual([240, 270]);
    expect(cuts.includes(368)).toBe(false);
  });
  test("cutsAt by bar, scaled, without drops", () => {
    const cuts = s.cutsAt({ by: "bar", scale: 2, drops: false, stops: false, builds: false, from: 240, to: 300 });
    expect(cuts).toEqual([240, 255, 270, 285]);
  });
});

describe("useSong", () => {
  test("the moment at the current frame, with the map bound to the fps", () => {
    at(<Ask f={() => useSong(map)} />, 240);
    expect(got.drop.time).toBe(8);
    expect(got.song.frame(4)).toBe(120);
  });
  test("in the frames of the enclosing Sequence, offset into the song", () => {
    at(<Sequence from={30}><Ask f={() => useSong(map, { offset: 4 })} /></Sequence>, 75);
    // local frame 45 = 1.5 s, 4 s in: 5.5 s, half way through the build
    expect(got.buildProgress).toBeCloseTo(0.5, 9);
  });
});

test("<SongPlot> draws the sections by name and every drop", () => {
  const html = at(<SongPlot map={map} />, 0);
  expect(html).toContain("Pre-Chorus");
  expect(html).toContain("120 BPM");
  expect(html).toContain(">8.0<");
});
