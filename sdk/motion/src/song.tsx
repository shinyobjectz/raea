// A song's energy map, frame by frame (spec: "Pacing to a song's energy"). The map is the JSON `raea-motion song
// FILE -o song.json` writes (ai/perception/src/song.rs): beats, downbeats and bars from Beat This!, per-beat features
// 0..1 (loudness, onsets, brightness, low end, high band, energy), sections (the file's own [Section] markers when it
// has them, else novelty) and events (build, drop, stop, hit, dip, break), each section and bar with a suggested cut
// rate. Everything here is a pure function of the map and the frame.
//
//   import map from "./song.json";
//   const s = useSong(map);                     // frame 0 of the enclosing Sequence is the song's start
//   s.energy, s.beatInBar, s.section?.name, s.inBuild && s.buildProgress, s.untilDrop
//   const cuts = song(map, { fps }).cutsAt();   // frames to cut on, paced to the energy
import { useCurrentFrame, useVideoConfig } from "./clock";

export type SongEventKind = "build" | "drop" | "stop" | "hit" | "dip" | "break";
export type SongLevel = "intro" | "low" | "mid" | "high" | "outro";

/** A hit inside a stop: `time` on the beat grid (a beat or half beat), `at` where the onset is. */
export interface SongHit {
  time: number;
  at: number;
  strength: number;
}

export interface SongEvent {
  kind: SongEventKind;
  /** seconds from the song's start: a beat (drop, a downbeat) or the grid's nearest beat or half beat (hit) */
  time: number;
  /** spans (build, stop, dip, break) end here */
  end?: number;
  beat: number;
  bar: number;
  beats?: number;
  /** 0..1 */
  strength: number;
  /** a build or stop: the drop it leads into, seconds */
  into?: number | null;
  /** a stop: its sparse hits */
  hits?: SongHit[];
  /** a drop: the energy of the bar after minus the bar before (negative when the band cuts after one big hit) */
  rise?: number;
  /** a drop: the bar after stays up (false: one hit, then the band cuts out) */
  holds?: boolean;
  afterStop?: boolean;
  afterBuild?: boolean;
  /** a drop: a breath of near-silence just before it */
  gapBefore?: boolean;
  /** a hit: the onset's own time; how far over the groove it stands */
  at?: number;
  over?: number;
}

export interface SongSection {
  name: string;
  level: SongLevel;
  time: number;
  end: number;
  beat: number;
  bar: number;
  energy: number;
  /** suggested cuts per bar: 0.25, 0.5, 1, 2 or 4 */
  cuts: number;
}

export interface SongBar {
  time: number;
  beat: number;
  beats: number;
  energy: number;
  cuts: number;
}

export interface SongFeatures {
  loudness: number[];
  onsets: number[];
  brightness: number[];
  low: number[];
  high: number[];
  energy: number[];
}

export interface SongMap {
  version: number;
  duration: number;
  tempo: number | null;
  beatsPerBar: number;
  beats: number[];
  downbeats: number[];
  /** one value a beat, 0..1 over the song */
  features: SongFeatures;
  bars: SongBar[];
  sections: SongSection[];
  events: SongEvent[];
  sectionsFrom?: "markers" | "novelty";
  boundaries?: { time: number; beat: number; bar: number; strength: number }[];
  source?: string | null;
}

/** Where a moment of the song is. Seconds are the song's; infinities when there's no such event. */
export interface SongMoment {
  time: number;
  /** the beat at or before `time` (-1 before the first) */
  beat: number;
  /** the bar at or before `time` (-1 before the first beat) */
  bar: number;
  /** 0-based position in the bar */
  beatInBar: number;
  /** 0..1 through the beat, and through the bar */
  phase: number;
  barPhase: number;
  /** the features at `time`, interpolated between beats */
  energy: number;
  loudness: number;
  onsets: number;
  brightness: number;
  low: number;
  section: SongSection | null;
  sectionIndex: number;
  /** 0..1 through the section */
  sectionProgress: number;
  /** the bar's suggested cut rate (cuts per bar) */
  cutsPerBar: number;
  /** the last drop at or before `time`, the next after it */
  drop: SongEvent | null;
  nextDrop: SongEvent | null;
  sinceDrop: number;
  untilDrop: number;
  /** the build `time` is in, and 0..1 through it */
  build: SongEvent | null;
  inBuild: boolean;
  buildProgress: number;
  /** the stop `time` is in */
  stop: SongEvent | null;
  inStop: boolean;
  /** the dip or break `time` is in */
  dip: SongEvent | null;
  /** the next event of any kind after `time` */
  next: SongEvent | null;
}

/** The last index i with v[i] <= t (-1 when t is before v[0]); v ascending. */
function floorIndex(v: readonly number[], t: number): number {
  let lo = 0;
  let hi = v.length - 1;
  if (hi < 0 || t < v[0]) return -1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (v[mid] <= t) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

function check(map: SongMap): void {
  if (!map || !Array.isArray(map.beats) || !map.features || !Array.isArray(map.events)) {
    throw new TypeError("song: not a song map (make one with `raea-motion song FILE -o song.json`)");
  }
}

function featureAt(v: readonly number[] | undefined, i: number, phase: number): number {
  if (!v || v.length === 0) return 0;
  if (i < 0) return v[0];
  const a = v[Math.min(i, v.length - 1)];
  const b = v[Math.min(i + 1, v.length - 1)];
  return a + (b - a) * phase;
}

const SPANS = new Set<SongEventKind>(["build", "stop", "dip", "break"]);

/** Where the song is at `t` seconds: a pure function of the map. */
export function songAt(map: SongMap, t: number): SongMoment {
  check(map);
  const beats = map.beats;
  const n = beats.length;
  const bpb = map.beatsPerBar || 4;
  const gap = map.tempo ? 60 / map.tempo : n > 1 ? (beats[n - 1] - beats[0]) / (n - 1) : 0.5;
  const beat = floorIndex(beats, t);
  const beatEnd = beat < 0 ? beats[0] ?? gap : beat + 1 < n ? beats[beat + 1] : beats[beat] + gap;
  const beatStart = beat < 0 ? beatEnd - gap : beats[beat];
  const phase = Math.min(1, Math.max(0, (t - beatStart) / Math.max(1e-9, beatEnd - beatStart)));
  const barStarts = map.bars.map((b) => b.time);
  const bar = floorIndex(barStarts, t);
  const barRow = bar >= 0 ? map.bars[bar] : null;
  const beatInBar = barRow && beat >= 0 ? beat - barRow.beat : beat < 0 ? 0 : beat % bpb;
  const barLen = barRow ? barRow.beats : bpb;
  const barPhase = Math.min(1, Math.max(0, (beatInBar + phase) / Math.max(1, barLen)));
  const f = map.features;
  const sectionIndex = floorIndex(map.sections.map((s) => s.time), t);
  const section = sectionIndex >= 0 ? map.sections[sectionIndex] : null;
  const sectionProgress = section ? Math.min(1, Math.max(0, (t - section.time) / Math.max(1e-9, section.end - section.time))) : 0;
  const drops = map.events.filter((e) => e.kind === "drop");
  const di = floorIndex(drops.map((d) => d.time), t);
  const drop = di >= 0 ? drops[di] : null;
  const nextDrop = drops[di + 1] ?? null;
  const within = (k: SongEventKind | SongEventKind[]) =>
    map.events.find((e) => (Array.isArray(k) ? k.includes(e.kind) : e.kind === k) && e.end !== undefined && t >= e.time && t < e.end) ?? null;
  const build = within("build");
  const stop = within("stop");
  const dip = within(["dip", "break"]);
  const next = map.events.find((e) => e.time > t) ?? null;
  return {
    time: t,
    beat,
    bar,
    beatInBar,
    phase,
    barPhase,
    energy: featureAt(f.energy, beat, phase),
    loudness: featureAt(f.loudness, beat, phase),
    onsets: featureAt(f.onsets, beat, phase),
    brightness: featureAt(f.brightness, beat, phase),
    low: featureAt(f.low, beat, phase),
    section,
    sectionIndex,
    sectionProgress,
    cutsPerBar: barRow ? barRow.cuts : section ? section.cuts : 0.25,
    drop,
    nextDrop,
    sinceDrop: drop ? t - drop.time : Infinity,
    untilDrop: nextDrop ? nextDrop.time - t : Infinity,
    build,
    inBuild: build !== null,
    buildProgress: build && build.end !== undefined ? (t - build.time) / Math.max(1e-9, build.end - build.time) : 0,
    stop,
    inStop: stop !== null,
    dip,
    next,
  };
}

export interface SongOptions {
  /** the composition's frame rate */
  fps: number;
  /** seconds into the song at frame 0 (where an <Audio> of it starts: its startFrom over fps) */
  offset?: number;
}

export interface FrameRange {
  /** first frame (inclusive), default 0 */
  from?: number;
  /** last frame (exclusive), default the song's end */
  to?: number;
}

export interface CutOptions extends FrameRange {
  /** "section" (default) takes each section's rate, steady across it; "bar" each bar's own */
  by?: "section" | "bar";
  /** multiplies every rate (2: twice as many cuts), kept on the ladder 0.25..4 */
  scale?: number;
  /** cut on every drop (default true) */
  drops?: boolean;
  /** cut on a stop's hits (default true) */
  stops?: boolean;
  /** double the rate through a build's second half (default true) */
  builds?: boolean;
  /** cuts closer than this many frames are merged, the earlier kept (default a quarter second) */
  minGap?: number;
}

export interface Song {
  map: SongMap;
  fps: number;
  offset: number;
  /** the frame a song time falls on */
  frame(seconds: number): number;
  /** the song's seconds at a frame */
  seconds(frame: number): number;
  /** where the song is at a frame */
  at(frame: number): SongMoment;
  /** frames of every `every`-th beat (1: each beat), counted from the song's first */
  onBeat(every?: number, range?: FrameRange): number[];
  /** frames of every `every`-th downbeat */
  onDownbeat(every?: number, range?: FrameRange): number[];
  /** frames of every event of a kind (a build's or stop's start), with the event */
  eventsOf(kind: SongEventKind, range?: FrameRange): { frame: number; event: SongEvent }[];
  /** frames to cut on, paced to the energy: each section's (or bar's) rate, on its beats, plus drops, a stop's hits,
   *  and a build's second half at double rate */
  cutsAt(options?: CutOptions): number[];
}

const LADDER = [0.25, 0.5, 1, 2, 4];

function onLadder(r: number): number {
  return LADDER.reduce((best, x) => (Math.abs(Math.log2(x) - Math.log2(r)) < Math.abs(Math.log2(best) - Math.log2(r)) ? x : best), 1);
}

/** A song map bound to a frame rate (and where frame 0 sits in the song): frames from song times, and the reverse. */
export function song(map: SongMap, options: SongOptions): Song {
  check(map);
  const { fps, offset = 0 } = options;
  if (!(fps > 0)) throw new RangeError(`song: fps must be positive, got ${fps}`);
  const frame = (s: number) => Math.round((s - offset) * fps);
  const seconds = (f: number) => f / fps + offset;
  const inRange = (r: FrameRange = {}) => {
    const from = r.from ?? 0;
    const to = r.to ?? frame(map.duration) + 1;
    return (f: number) => f >= from && f < to;
  };
  const every = (times: readonly number[], k: number, r?: FrameRange) => {
    if (!Number.isInteger(k) || k < 1) throw new RangeError(`song: every must be a whole number from 1, got ${k}`);
    const ok = inRange(r);
    return times.filter((_, i) => i % k === 0).map(frame).filter(ok);
  };
  const s: Song = {
    map,
    fps,
    offset,
    frame,
    seconds,
    at: (f) => songAt(map, seconds(f)),
    onBeat: (k = 1, r) => every(map.beats, k, r),
    onDownbeat: (k = 1, r) => every(map.downbeats, k, r),
    eventsOf: (kind, r) => {
      const ok = inRange(r);
      return map.events.filter((e) => e.kind === kind).map((event) => ({ frame: frame(event.time), event })).filter((x) => ok(x.frame));
    },
    cutsAt: (o = {}) => {
      const { by = "section", scale = 1, drops = true, stops = true, builds = true } = o;
      const minGap = o.minGap ?? Math.max(1, Math.round(fps / 4));
      const ok = inRange(o);
      const times: number[] = [];
      const bpb = map.beatsPerBar || 4;
      const builds_ = map.events.filter((e) => e.kind === "build");
      const stops_ = map.events.filter((e) => e.kind === "stop");
      const sectionOf = (t: number) => map.sections[Math.max(0, floorIndex(map.sections.map((x) => x.time), t))];
      map.bars.forEach((bar, j) => {
        const sec = sectionOf(bar.time);
        // a stop's bars cut on its hits instead
        if (stops && stops_.some((st) => st.end !== undefined && bar.time >= st.time && bar.time < st.end)) return;
        let rate = (by === "bar" ? bar.cuts : sec?.cuts ?? bar.cuts) * scale;
        const b = builds_.find((e) => e.end !== undefined && bar.time >= (e.time + e.end) / 2 && bar.time < e.end);
        if (builds && b) rate *= 2;
        rate = onLadder(Math.min(4, Math.max(0.25, rate)));
        if (rate < 1) {
          // one cut every 1/rate bars, counted from the section's first bar
          const k = Math.round(1 / rate);
          const first = sec ? map.bars.findIndex((x) => x.time >= sec.time - 1e-6) : 0;
          if ((j - Math.max(0, first)) % k === 0) times.push(bar.time);
          return;
        }
        // rate cuts spread over the bar's beats
        const len = bar.beats || bpb;
        const step = len / rate;
        for (let c = 0; c < rate; c++) {
          const at = bar.beat + Math.round(c * step);
          if (at < bar.beat + len && at < map.beats.length) times.push(map.beats[at]);
        }
      });
      if (drops) for (const e of map.events) if (e.kind === "drop") times.push(e.time);
      if (stops) for (const st of stops_) for (const h of st.hits ?? []) times.push(h.time);
      const frames = [...new Set(times.map(frame))].filter(ok).sort((a, b) => a - b);
      // drops win where cuts crowd: keep a drop's frame, then the earlier of the rest
      const dropFrames = new Set(drops ? map.events.filter((e) => e.kind === "drop").map((e) => frame(e.time)) : []);
      const out: number[] = [];
      for (const f of frames) {
        const last = out[out.length - 1];
        if (last === undefined || f - last >= minGap) out.push(f);
        else if (dropFrames.has(f) && !dropFrames.has(last)) out[out.length - 1] = f;
      }
      return out;
    },
  };
  return s;
}

export interface UseSongOptions {
  /** seconds into the song at frame 0 of the enclosing Sequence */
  offset?: number;
}

/** Where the song is at the current frame (frame 0 of the enclosing Sequence is `offset` seconds into the song), with
 *  the map bound to the composition's fps as `song`. */
export function useSong(map: SongMap, options: UseSongOptions = {}): SongMoment & { song: Song } {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = song(map, { fps, offset: options.offset ?? 0 });
  return { ...s.at(f), song: s };
}

// ---- the plot ----------------------------------------------------------------------------------------------------

const LEVEL_COLOUR: Record<SongLevel, string> = { intro: "#3a3f4b", low: "#2f5d8a", mid: "#b8862b", high: "#c2412d", outro: "#3a3f4b" };
const EVENT_COLOUR: Record<SongEventKind, string> = {
  build: "#f2c14e", drop: "#ff4d4d", stop: "#7fd1ff", hit: "#ffffff", dip: "#9b8cff", break: "#9b8cff",
};

export interface SongPlotProps {
  map: SongMap;
  width?: number;
  height?: number;
  title?: string;
}

/** The map drawn for a human to check: section bands named, the energy curve with builds, stops, dips and drops on
 *  it, hits, the feature lanes, and the pacing (cuts per bar). `raea-motion song FILE --plot OUT.png` renders it. */
export function SongPlot({ map, width = 2400, height = 1350, title }: SongPlotProps) {
  check(map);
  const L = 150;
  const R = 40;
  const W = width - L - R;
  const dur = Math.max(1e-6, map.duration);
  const x = (t: number) => L + (t / dur) * W;
  const beats = map.beats;
  const f = map.features;
  const font = "Helvetica, Arial, sans-serif";
  const rows = {
    title: [20, 70],
    sections: [90, 160],
    energy: [180, 640],
    loudness: [670, 760],
    low: [775, 865],
    onsets: [880, 970],
    brightness: [985, 1075],
    pace: [1100, 1230],
    axis: [1240, 1300],
  } as const;
  const sy = height / 1350;
  const Y = (v: number) => v * sy;
  const line = (v: readonly number[], top: number, bottom: number) =>
    beats.map((t, i) => `${i === 0 ? "M" : "L"}${x(t).toFixed(1)},${Y(bottom - (bottom - top) * (v[i] ?? 0)).toFixed(1)}`).join(" ");
  const area = (v: readonly number[], top: number, bottom: number) =>
    beats.length ? `${line(v, top, bottom)} L${x(beats[beats.length - 1]).toFixed(1)},${Y(bottom)} L${x(beats[0]).toFixed(1)},${Y(bottom)} Z` : "";
  const label = (text: string, left: number, top: number, size: number, colour = "#e8e8e8", weight = 400, extra: Record<string, unknown> = {}) => (
    <div key={`${text}-${left}-${top}`} style={{ position: "absolute", left, top: Y(top), fontFamily: font, fontSize: size * sy, color: colour, fontWeight: weight, whiteSpace: "nowrap", ...extra }}>{text}</div>
  );
  const [eT, eB] = rows.energy;
  const lanes: [keyof typeof rows, readonly number[], string, string][] = [
    ["loudness", f.loudness, "loudness", "#cfd8dc"],
    ["low", f.low, "low end", "#ff8a65"],
    ["onsets", f.onsets, "onsets", "#81c784"],
    ["brightness", f.brightness, "brightness", "#64b5f6"],
  ];
  const spans = map.events.filter((e) => e.end !== undefined);
  const instants = map.events.filter((e) => e.end === undefined);
  const secondsTicks: number[] = [];
  for (let t = 0; t <= dur; t += 10) secondsTicks.push(t);
  const [pT, pB] = rows.pace;
  const paceY = (c: number) => pB - ((Math.log2(c) + 2) / 4) * (pB - pT);
  const pacePath = map.bars
    .map((b, j) => {
      const end = map.bars[j + 1]?.time ?? dur;
      return `${j === 0 ? "M" : "L"}${x(b.time).toFixed(1)},${Y(paceY(b.cuts)).toFixed(1)} L${x(end).toFixed(1)},${Y(paceY(b.cuts)).toFixed(1)}`;
    })
    .join(" ");
  return (
    <div style={{ position: "absolute", left: 0, top: 0, width, height, background: "#111317", overflow: "hidden" }}>
      <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} style={{ position: "absolute", left: 0, top: 0 }}>
        {/* bars, faint */}
        {map.bars.map((b, j) => (
          <rect key={`bar${j}`} x={x(b.time)} y={Y(eT)} width={1} height={Y(rows.brightness[1] - eT)} fill={j % 4 === 0 ? "#2a2e36" : "#1b1e24"} />
        ))}
        {/* sections */}
        {map.sections.map((s, i) => (
          <rect key={`sec${i}`} x={x(s.time)} y={Y(rows.sections[0])} width={Math.max(1, x(s.end) - x(s.time) - 2)} height={Y(rows.sections[1] - rows.sections[0])} fill={LEVEL_COLOUR[s.level] ?? "#444"} />
        ))}
        {map.sections.map((s, i) => (
          <rect key={`secline${i}`} x={x(s.time)} y={Y(rows.sections[1])} width={2} height={Y(rows.pace[1] - rows.sections[1])} fill="#ffffff" opacity={0.18} />
        ))}
        {/* spans on the energy lane */}
        {spans.map((e, i) => {
          const x0 = x(e.time);
          const x1 = x(e.end ?? e.time);
          if (e.kind === "build") {
            return <path key={`span${i}`} d={`M${x0},${Y(eB)} L${x1},${Y(eT)} L${x1},${Y(eB)} Z`} fill={EVENT_COLOUR.build} opacity={0.28} />;
          }
          return <rect key={`span${i}`} x={x0} y={Y(eT)} width={Math.max(2, x1 - x0)} height={Y(eB - eT)} fill={EVENT_COLOUR[e.kind]} opacity={e.kind === "stop" ? 0.3 : 0.18} />;
        })}
        {/* energy */}
        <path d={area(f.energy, eT, eB)} fill="#ffffff" opacity={0.08} />
        <path d={line(f.energy, eT, eB)} fill="none" stroke="#ffffff" strokeWidth={3 * sy} />
        {/* the feature lanes */}
        {lanes.map(([row, v, , colour]) => {
          const [t, b] = rows[row];
          return <path key={`lane${row}`} d={area(v, t, b)} fill={colour} opacity={0.75} />;
        })}
        {/* drops and hits */}
        {instants.map((e, i) =>
          e.kind === "drop" ? (
            <g key={`ev${i}`}>
              <rect x={x(e.time) - 2} y={Y(eT - 10)} width={4} height={Y(rows.brightness[1] - eT + 10)} fill={EVENT_COLOUR.drop} opacity={0.4 + 0.6 * e.strength} />
              <path d={`M${x(e.time) - 12},${Y(eT - 26)} L${x(e.time) + 12},${Y(eT - 26)} L${x(e.time)},${Y(eT - 4)} Z`} fill={EVENT_COLOUR.drop} />
            </g>
          ) : (
            <rect key={`ev${i}`} x={x(e.time) - 1.5} y={Y(eB - 40 - 50 * e.strength)} width={3} height={Y(40 + 50 * e.strength)} fill={EVENT_COLOUR.hit} opacity={0.9} />
          ),
        )}
        {spans.filter((e) => e.kind === "stop").flatMap((e, i) =>
          (e.hits ?? []).map((h, k) => <circle key={`sh${i}-${k}`} cx={x(h.time)} cy={Y(eB - 20)} r={4 * sy} fill={EVENT_COLOUR.stop} />),
        )}
        {/* pacing */}
        {[0.25, 0.5, 1, 2, 4].map((c) => (
          <rect key={`pg${c}`} x={L} y={Y(paceY(c))} width={W} height={1} fill="#2a2e36" />
        ))}
        <path d={pacePath} fill="none" stroke="#f2c14e" strokeWidth={3 * sy} />
        {/* axis */}
        {secondsTicks.map((t) => (
          <rect key={`tick${t}`} x={x(t)} y={Y(rows.axis[0])} width={1} height={Y(t % 30 === 0 ? 16 : 8)} fill="#8a8f98" />
        ))}
      </svg>
      {label(title ?? `${map.source ?? "song"}  ·  ${map.tempo ?? "?"} BPM  ·  ${map.beatsPerBar}/4  ·  ${Math.floor(dur / 60)}:${String(Math.round(dur % 60)).padStart(2, "0")}  ·  sections from ${map.sectionsFrom ?? "?"}`, L, rows.title[0], 34, "#ffffff", 700)}
      {label("build", width - R - 640, rows.title[0] + 8, 22, EVENT_COLOUR.build, 700)}
      {label("drop", width - R - 540, rows.title[0] + 8, 22, EVENT_COLOUR.drop, 700)}
      {label("stop · its hits", width - R - 450, rows.title[0] + 8, 22, EVENT_COLOUR.stop, 700)}
      {label("hit", width - R - 270, rows.title[0] + 8, 22, EVENT_COLOUR.hit, 700)}
      {label("dip / break", width - R - 200, rows.title[0] + 8, 22, EVENT_COLOUR.dip, 700)}
      {map.sections.map((s) => label(s.name, x(s.time) + 8, rows.sections[0] + 8, 22, "#ffffff", 700, { maxWidth: Math.max(0, x(s.end) - x(s.time) - 14), overflow: "hidden" }))}
      {map.sections.map((s) => label(`${s.time.toFixed(1)}s · ${s.level} · ${s.cuts}/bar`, x(s.time) + 8, rows.sections[0] + 40, 16, "#e0e0e0", 400, { maxWidth: Math.max(0, x(s.end) - x(s.time) - 14), overflow: "hidden" }))}
      {instants.filter((e) => e.kind === "drop").map((e) => label(`${e.time.toFixed(1)}`, x(e.time) + 6, eT + 2, 18, EVENT_COLOUR.drop, 700))}
      {spans.filter((e) => e.kind === "build").map((e, i) => label(`build ${e.time.toFixed(1)}-${(e.end ?? 0).toFixed(1)}`, x(e.time) + 4, eB - 26 - (i % 2) * 22, 15, EVENT_COLOUR.build, 700))}
      {spans.filter((e) => e.kind === "dip" || e.kind === "break").map((e) => label(`${e.kind} ${e.time.toFixed(1)}`, x(e.time) + 4, eT + 6, 15, EVENT_COLOUR.dip, 700))}
      {label("energy", 16, eT, 22, "#ffffff", 700)}
      {lanes.map(([row, , name, colour]) => label(name, 16, rows[row][0] + 20, 20, colour, 700))}
      {label("cuts / bar", 16, pT + 10, 20, "#f2c14e", 700)}
      {[0.25, 1, 4].map((c) => label(String(c), L - 46, paceY(c) - 10, 15, "#8a8f98"))}
      {secondsTicks.filter((t) => t % 30 === 0).map((t) => label(`${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`, x(t) - 18, rows.axis[0] + 20, 18, "#8a8f98"))}
    </div>
  );
}
