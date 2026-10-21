// @raea/motion/tone: Tone.js snippets from its docs and examples (https://tonejs.github.io, the README, the examples
// folder), run unchanged except for the import, checked against the graph and events they compile to.
import { beforeEach, expect, test } from "bun:test";
import * as Tone from "../src/tone";
import { renderFrame } from "../src/test";

beforeEach(() => Tone.reset());

const ev = (until = 10) => Tone.snapshot(until).events;
const kinds = () => Tone.snapshot(0).graph.nodes.map((n: any) => n.kind);
const edges = () => Tone.snapshot(0).graph.edges;

test("README: a Synth to the Destination, triggerAttackRelease", () => {
  const synth = new Tone.Synth().toDestination();
  synth.triggerAttackRelease("C4", "8n");
  const [e] = ev();
  expect(e.e).toBe("ar");
  expect(e.t).toBe(0);
  expect(e.f![0]).toBeCloseTo(261.6256, 3);
  expect(e.d).toBe(0.25);
  expect(edges()).toContainEqual([synth.id, 0, 0]);
});

test("README: triggerAttack now, triggerRelease a second later", () => {
  const synth = new Tone.Synth().toDestination();
  const now = Tone.now();
  synth.triggerAttack("C4", now);
  synth.triggerRelease(now + 1);
  expect(ev().map((e) => [e.e, e.t])).toEqual([["a", 0], ["ra", 1]]);
});

test("README: a PolySynth's chord attacks and one release of them all", () => {
  const synth = new Tone.PolySynth(Tone.Synth).toDestination();
  const now = Tone.now();
  synth.triggerAttack("D4", now);
  synth.triggerAttack("F4", now + 0.5);
  synth.triggerAttack("A4", now + 1);
  synth.triggerAttack("C5", now + 1.5);
  synth.triggerAttack("E5", now + 2);
  synth.triggerRelease(["D4", "F4", "A4", "C5", "E5"], now + 4);
  const es = ev();
  expect(es.filter((e) => e.e === "a").map((e) => e.t)).toEqual([0, 0.5, 1, 1.5, 2]);
  const rel = es.find((e) => e.e === "r")!;
  expect(rel.t).toBe(4);
  expect(rel.f!.length).toBe(5);
  expect(Tone.snapshot(0).graph.nodes.find((n: any) => n.id === synth.id).options.poly).toBe(true);
});

test("README: two Loops on the Transport", () => {
  const synthA = new Tone.FMSynth().toDestination();
  const synthB = new Tone.AMSynth().toDestination();
  new Tone.Loop((time) => {
    synthA.triggerAttackRelease("C2", "8n", time);
  }, "4n").start(0);
  new Tone.Loop((time) => {
    synthB.triggerAttackRelease("C4", "8n", time);
  }, "4n").start("8n");
  Tone.getTransport().start();
  const es = ev(1.01);
  expect(es.filter((e) => e.n === synthA.id).map((e) => e.t)).toEqual([0, 0.5, 1]);
  expect(es.filter((e) => e.n === synthB.id).map((e) => e.t)).toEqual([0.25, 0.75]);
  expect(kinds()).toContain("FMSynth");
  expect(kinds()).toContain("AMSynth");
});

test("README: a Sampler with urls and a baseUrl, played once loaded", async () => {
  const sampler = new Tone.Sampler({
    urls: { C4: "C4.mp3", "D#4": "Ds4.mp3", "F#4": "Fs4.mp3", A4: "A4.mp3" },
    release: 1,
    baseUrl: "media/salamander/",
  }).toDestination();
  await Tone.loaded().then(() => {
    sampler.triggerAttackRelease(["Eb4", "G4", "Bb4"], 4);
  });
  const node = Tone.snapshot(0).graph.nodes.find((n: any) => n.kind === "Sampler");
  expect(node.options.urls).toEqual({ "60": "media/salamander/C4.mp3", "63": "media/salamander/Ds4.mp3", "66": "media/salamander/Fs4.mp3", "69": "media/salamander/A4.mp3" });
  expect(node.options.release).toBe(1);
  const [e] = ev();
  expect(e.f!.map((f) => Math.round(Tone.ftom(f)))).toEqual([63, 67, 70]);
  expect(e.d).toBe(4);
});

test("README: a looping Player through a Distortion", () => {
  const player = new Tone.Player({ url: "media/gurgling_theremin_1.mp3", loop: true, autostart: true });
  const distortion = new Tone.Distortion(0.4).toDestination();
  player.connect(distortion);
  const g = Tone.snapshot(0).graph;
  expect(g.nodes.find((n: any) => n.kind === "Distortion").options.distortion).toBe(0.4);
  expect(g.edges).toContainEqual([player.id, distortion.id, 0]);
  expect(ev()[0].e).toBe("start");
});

test("README: one Player into a Filter and a FeedbackDelay in parallel", () => {
  const player = new Tone.Player({ url: "media/theremin.mp3", loop: true, autostart: true });
  const filter = new Tone.Filter(400, "lowpass").toDestination();
  const feedbackDelay = new Tone.FeedbackDelay(0.125, 0.5).toDestination();
  player.connect(filter);
  player.connect(feedbackDelay);
  const g = Tone.snapshot(0).graph;
  expect(g.nodes.find((n: any) => n.kind === "Filter").options).toMatchObject({ frequency: 400, type: "lowpass" });
  expect(g.nodes.find((n: any) => n.kind === "FeedbackDelay").options).toMatchObject({ delayTime: 0.125, feedback: 0.5 });
  expect(g.edges.filter((e: number[]) => e[0] === player.id).length).toBe(2);
});

test("docs: a Sequence with nested subdivisions", () => {
  const synth = new Tone.Synth().toDestination();
  new Tone.Sequence((time, note) => {
    synth.triggerAttackRelease(note, 0.1, time);
  }, ["C4", ["E4", "D4", "E4"], "G4", ["A4", "G4"]]).start(0);
  Tone.Transport.start();
  const es = ev(0.99);
  expect(es.map((e) => Math.round(Tone.ftom(e.f![0])))).toEqual([60, 64, 62, 64, 67, 69, 67]);
  expect(es.map((e) => Math.round(e.t * 1000) / 1000)).toEqual([0, 0.25, 0.333, 0.417, 0.5, 0.75, 0.875]);
  // and it loops (Tone's Sequence loops by default): a second pass from 1 s
  expect(ev(1.01).length).toBe(8);
});

test("docs: a Part of [time, note] pairs", () => {
  const synth = new Tone.Synth().toDestination();
  new Tone.Part((time, note) => {
    synth.triggerAttackRelease(note, "8n", time);
  }, [[0, "C2"], ["0:2", "C3"], ["0:3:2", "G2"]]).start(0);
  Tone.Transport.start();
  expect(ev().map((e) => e.t)).toEqual([0, 1, 1.75]);
});

test("docs: a Part of objects with time, looped", () => {
  const synth = new Tone.Synth().toDestination();
  const part = new Tone.Part((time, value) => {
    synth.triggerAttackRelease(value.note, "8n", time, value.velocity);
  }, [{ time: 0, note: "C3", velocity: 0.9 }, { time: "0:2", note: "C4", velocity: 0.5 }]).start(0);
  part.loop = true;
  part.loopEnd = "1m";
  Tone.Transport.start();
  // loop set after start() takes effect on the next start in Tone; ours schedules at start: the first pass plays
  expect(ev(1.5).map((e) => [e.t, e.v])).toEqual([[0, 0.9], [1, 0.5]]);
});

test("docs: a Pattern up and down", () => {
  const synth = new Tone.Synth().toDestination();
  const pattern = new Tone.Pattern((time, note) => {
    synth.triggerAttackRelease(note, 0.25, time);
  }, ["C2", "D4", "E5", "A6"], "upDown");
  pattern.start(0);
  Tone.Transport.start();
  expect(ev(3.4).map((e) => Math.round(Tone.ftom(e.f![0])))).toEqual([36, 62, 76, 93, 76, 62, 36]);
});

test("docs: scheduleRepeat starting and stopping an Oscillator", () => {
  const osc = new Tone.Oscillator().toDestination();
  Tone.getTransport().scheduleRepeat((time) => {
    osc.start(time).stop(time + 0.1);
  }, "8n");
  Tone.getTransport().start();
  const es = ev(0.6);
  expect(es.filter((e) => e.e === "a").map((e) => e.t)).toEqual([0, 0.25, 0.5]);
  expect(es.filter((e) => e.e === "ra").map((e) => Math.round(e.t * 10) / 10)).toEqual([0.1, 0.4]);
});

test("docs: chain into the Destination, Param ramps, Channel sends", () => {
  const filter = new Tone.Filter(1500, "lowpass");
  const synth = new Tone.MonoSynth().chain(filter, Tone.Destination);
  filter.frequency.rampTo(400, 2, 1);
  const reverb = new Tone.Channel().receive("reverb").toDestination();
  const voice = new Tone.Channel({ volume: -6, pan: 0.3 }).toDestination();
  voice.send("reverb", -12);
  const g = Tone.snapshot(0).graph;
  expect(g.edges).toContainEqual([synth.id, filter.id, 0]);
  expect(g.edges).toContainEqual([filter.id, 0, 0]);
  const p = ev().find((e) => e.e === "param")!;
  expect(p).toMatchObject({ n: filter.id, t: 1, p: "frequency", val: 400, ramp: 2 });
  const send = g.nodes.find((n: any) => n.kind === "Gain");
  expect(send.options.gain).toBeCloseTo(Tone.dbToGain(-12), 9);
  expect(g.edges).toContainEqual([send.id, reverb.id, 0]);
  expect(g.nodes.find((n: any) => n.id === voice.id).options).toMatchObject({ volume: -6, pan: 0.3 });
});

test("docs: units and the Transport's time", () => {
  expect(Tone.Time("4n").toSeconds()).toBe(0.5);
  expect(Tone.Time("1m").toSeconds()).toBe(2);
  expect(Tone.Time("8t").toSeconds()).toBeCloseTo(1 / 6, 12);
  expect(Tone.Time("1:2:2").toSeconds()).toBe(3.25);
  expect(Tone.Frequency("A4").toFrequency()).toBe(440);
  expect(Tone.Frequency(60, "midi").toNote()).toBe("C4");
  expect(Tone.Frequency("C4").transpose(7).toNote()).toBe("G4");
  Tone.Transport.bpm.value = 90;
  expect(Tone.Time("4n").toSeconds()).toBeCloseTo(60 / 90, 12);
  expect(Tone.Time("1m").toFrames(30)).toBe(80);
});

test("swing delays every other subdivision", () => {
  const synth = new Tone.Synth().toDestination();
  Tone.Transport.swing = 0.5;
  Tone.Transport.swingSubdivision = "8n";
  new Tone.Loop((t) => synth.triggerAttackRelease("C4", "16n", t), "8n").start(0);
  Tone.Transport.start();
  expect(ev(0.9).map((e) => e.t)).toEqual([0, 0.3125, 0.5, 0.8125]);
});

test("raea: buses, ducking and a game's one-shot on its frame", () => {
  Tone.buses.music.duck(Tone.buses.voice, { amount: -10 });
  const sfx = Tone.vary(new Tone.MembraneSynth({ maxPolyphony: 4, poly: true }).connect(Tone.buses.sfx), { pitch: 0.3, volume: 2 });
  const g = Tone.snapshot(0).graph;
  const music = g.nodes.find((n: any) => n.name === "music");
  const voice = g.nodes.find((n: any) => n.name === "voice");
  expect(music.options.duck.amount).toBe(-10);
  expect(g.edges).toContainEqual([voice.id, music.id, 1]);
  const html = renderFrame(<Tone.Sfx when={true} play={(t) => sfx.triggerAttackRelease("C2", "16n", t)} />, { frame: 42, fps: 30, width: 10, height: 10, durationInFrames: 60 });
  const m = /data-tone-evs="([^"]*)"/.exec(html)!;
  const evs = JSON.parse(m[1].replace(/&quot;/g, '"'));
  expect(evs[0].t).toBeCloseTo(42 / 30, 12);
  expect(evs[0].k).toContain(":42:");
  expect(renderFrame(<Tone.Sfx when={false} play={() => sfx.triggerAttackRelease("C2", "16n")} />, { frame: 43, fps: 30, width: 10, height: 10, durationInFrames: 60 })).not.toContain("data-tone-evs");
});

test("raea: generated sources in a Sampler (TokenSynth roots) and a Player (SAO)", () => {
  const s = new Tone.Sampler({ urls: Tone.tokensynth("a warm upright bass", ["C2", "C3"]) }).toDestination();
  const p = new Tone.Player(Tone.sao("a wooden door slams")).toDestination();
  const g = Tone.snapshot(0).graph;
  const sn = g.nodes.find((n: any) => n.id === s.id);
  expect(Object.keys(sn.options.urls)).toEqual(["36", "48"]);
  expect(sn.options.urls["36"]).toMatchObject({ generate: "tokensynth", prompt: "a warm upright bass", midi: 36 });
  expect(g.nodes.find((n: any) => n.id === p.id).options.url).toMatchObject({ generate: "sao", prompt: "a wooden door slams" });
});

test("ToneOut shows the graph and the frame's events", () => {
  const synth = new Tone.Synth().toDestination();
  synth.triggerAttackRelease("C4", "8n", 1);
  const at = (frame: number) => renderFrame(<Tone.ToneOut />, { frame, fps: 30, width: 10, height: 10, durationInFrames: 60 });
  expect(at(10)).toContain("data-tone-graph");
  expect(at(10)).not.toContain("data-tone-evs");
  expect(at(30)).toContain("data-tone-evs");
  expect(at(32)).toContain("data-tone-evs");
  expect(at(33)).not.toContain("data-tone-evs");
});

test("docs: an LFO connected to a filter's frequency", () => {
  const filter = new Tone.Filter(400, "lowpass").toDestination();
  const lfo = new Tone.LFO("4n", 400, 4000).start();
  lfo.connect(filter.frequency);
  const g = Tone.snapshot(0).graph;
  expect(g.mods).toEqual([[lfo.id, filter.id, "frequency"]]);
  const o = g.nodes.find((n: any) => n.id === lfo.id).options;
  expect(o.frequency).toBe(2);
  expect([o.min, o.max, o.started]).toEqual([400, 4000, 0]);
});

test("docs: exponential ramps where Tone uses them", () => {
  const sweep = new Tone.Filter(200, "lowpass").toDestination();
  sweep.frequency.exponentialRampTo(8000, 2, 1);
  const fall = new Tone.Filter(8000, "lowpass").toDestination();
  fall.frequency.exponentialRampToValueAtTime(100, 4);
  const ps = ev().filter((e) => e.e === "param");
  expect(ps.map((e: any) => [e.n, e.t, e.val, e.ramp, e.curve])).toEqual([[fall.id, 0, 100, 4, "exp"], [sweep.id, 1, 8000, 2, "exp"]]);
});

test("docs: the Transport loops a measure", () => {
  const synth = new Tone.Synth().toDestination();
  Tone.getTransport().schedule((time) => synth.triggerAttackRelease("C4", "8n", time), "0:2:0");
  Tone.getTransport().loop = true;
  Tone.getTransport().loopStart = 0;
  Tone.getTransport().loopEnd = "1m";
  Tone.getTransport().start();
  // at 120 bpm a measure is 2 s: the event on beat 3 comes round every 2 s
  expect(ev(7).filter((e) => e.e === "ar").map((e) => e.t)).toEqual([1, 3, 5]);
});

test("docs: a Loop inside a looped window, setLoopPoints", () => {
  const synth = new Tone.Synth().toDestination();
  new Tone.Loop((time) => synth.triggerAttackRelease("C4", "16n", time), "4n").start(0);
  Tone.getTransport().setLoopPoints(0, "2n");
  Tone.getTransport().loop = true;
  Tone.getTransport().start();
  expect(ev(2.01).filter((e) => e.e === "ar").map((e) => e.t)).toEqual([0, 0.5, 1, 1.5, 2]);
});

test("docs: bpm.rampTo speeds the grid up", () => {
  const synth = new Tone.Synth().toDestination();
  Tone.getTransport().bpm.value = 60;
  Tone.getTransport().bpm.rampTo(120, 4);
  new Tone.Loop((time) => synth.triggerAttackRelease("C4", "16n", time), 1).start(0);
  Tone.getTransport().start();
  const ts = ev(8).filter((e) => e.e === "ar").map((e) => e.t);
  // a beat a second at 60 bpm, then closer together as the tempo rises to 120 over 4 s, then two a second
  const gaps = ts.slice(1).map((t, i) => t - ts[i]);
  expect(gaps[0]).toBeLessThan(1);
  expect(gaps[0]).toBeGreaterThan(0.8);
  for (let i = 1; i < gaps.length; i++) expect(gaps[i]).toBeLessThanOrEqual(gaps[i - 1] + 1e-9);
  expect(gaps[gaps.length - 1]).toBeCloseTo(0.5, 6);
  // beats from 0 to 4 s: the ramp's average tempo, 90 bpm, is 6 beats
  expect(ts.filter((t) => t < 4 - 1e-9).length).toBe(6);
});

test("docs: a Convolver with an impulse response, PitchShift up a fifth, EQ3", () => {
  const conv = new Tone.Convolver("media/ir/hall.wav").toDestination();
  const shift = new Tone.PitchShift(7).connect(conv);
  const eq = new Tone.EQ3(-10, 3, -6).connect(shift);
  const player = new Tone.Player("media/loop.wav").connect(eq);
  player.start();
  const nodes = Tone.snapshot(0).graph.nodes;
  expect(nodes.find((n: any) => n.kind === "Convolver").options.url).toBe("media/ir/hall.wav");
  expect(nodes.find((n: any) => n.kind === "PitchShift").options.pitch).toBe(7);
  expect(nodes.find((n: any) => n.kind === "EQ3").options).toMatchObject({ low: -10, mid: 3, high: -6 });
  shift.pitch = -12;
  expect(ev().find((e) => e.e === "param")).toMatchObject({ p: "pitch", val: -12 });
});

test("Tone.Offline says why it can't run", () => {
  expect(() => (Tone.Offline as any)(() => {}, 2)).toThrow(/no audio engine/);
});
