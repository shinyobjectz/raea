// The timeline (apps/void/shell.feature: Terminals): the stage's player as a control: play or pause, the time, a ruler of
// the composition's length in seconds, and its clips (its Sequences, void:timeline: each one's span, how deeply it's
// nested, what media it holds) in lanes under it, with the playhead; a click or drag anywhere under the ruler seeks.
// It hears the player as void:player and drives it by publishing { control: { n, do, frame } }, as any panel can.
import { AbsoluteFill, applyActions, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";
import { nextControl, useTimeline } from "@raea/terminal";
import type { Clip, Player, PlayerControl } from "@raea/terminal";

const HEAD = 36, LEFT = 14, RIGHT = 14, BTN = 26, RULER = 22, LANE = 22;

// the player kept here, not by usePlayer: a click seeks by the frames it has
type S = { player: Player; control: PlayerControl; down: boolean; dragging: boolean };

// a clip's colour by what it holds
const tint = (c: Clip) => (c.holds.includes("video") ? "#3b5bd6" : c.holds.includes("audio") ? "#2f9e6a" : c.holds.includes("img") ? "#8a5bd6" : "#33333c");

// clips in lanes: by depth, then the first lane at that depth free at its start
const lanes = (clips: Clip[]) => {
  const ends: number[][] = [];
  const out: { c: Clip; lane: number }[] = [];
  for (const c of [...clips].sort((a, b) => a.depth - b.depth || a.start - b.start)) {
    const row = (ends[c.depth] ??= []);
    let i = row.findIndex((e) => e <= c.start);
    if (i < 0) i = row.push(0) - 1;
    row[i] = c.end;
    out.push({ c, lane: i });
  }
  // lanes numbered down the page, depth by depth
  const base: number[] = [];
  ends.reduce((n, row, d) => ((base[d] = n), n + (row?.length ?? 0)), 0);
  return out.map(({ c, lane }) => ({ c, lane: (base[c.depth] ?? 0) + lane }));
};

const go = (s: S, what: "play" | "pause" | "seek", frame = 0): S => ({ ...s, control: nextControl(s.control, what, frame) });

export const actions: PanelActions<S> = {
  play: { about: "Play the composition on the stage", run: (s) => go(s, "play") },
  pause: { about: "Pause the composition on the stage", run: (s) => go(s, "pause") },
  seek: { about: "Move the playhead to a time, in seconds", args: { seconds: "number" }, run: (s, a) => go(s, "seek", Number(a.seconds ?? 0) * (s.player.rate || 30)) },
};

// the panel's width, as last laid out (the ruler runs across it)
let W = 800;
const track = () => [LEFT, W - RIGHT];
const frameAt = (s: S, x: number) => {
  const [a, b] = track();
  return Math.min(1, Math.max(0, (x - a) / Math.max(1, b - a))) * Math.max(0, s.player.frames - 1);
};
const playAt = [LEFT, LEFT + BTN];

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const p0 = voidIn<Player>(input, "player");
  if (p0) s = { ...s, player: p0 };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    if (p.y < HEAD && p.x >= playAt[0] && p.x < playAt[1]) s = s.player.playing ? actions.pause.run(s, {}) : actions.play.run(s, {});
    else if (p.y >= HEAD && s.player.frames > 0) s = { ...go(s, "seek", frameAt(s, p.x)), dragging: true };
  } else if (p && down && s.dragging) {
    const f = Math.round(frameAt(s, p.x));
    if (f !== s.control.frame) s = go(s, "seek", f);
  }
  if (!down && s.dragging) s = { ...s, dragging: false };
  return down === s.down ? s : { ...s, down };
};

const clock = (f: number, rate: number) => {
  const t = Math.max(0, f) / (rate || 30);
  return `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}.${String(Math.floor((t % 1) * 100)).padStart(2, "0")}`;
};

export default function Timeline({ width, height }: { width: number; height: number }) {
  W = width;
  const s = useGameState<S>({ player: { frame: 0, frames: 0, rate: 30, playing: false }, control: { n: 0, do: "", frame: 0 }, down: false, dragging: false }, step);
  const clips = useTimeline();
  const { frame, frames, rate, playing } = s.player;
  const [a, b] = track();
  const secs = frames / (rate || 30);
  // a tick a second, or every 5 or 10 when they'd crowd
  const every = secs / Math.max(1, (b - a) / 60) > 5 ? 10 : secs / Math.max(1, (b - a) / 60) > 1 ? 5 : 1;
  const ticks = frames > 0 ? Array.from({ length: Math.floor(secs / every) + 1 }, (_, i) => i * every) : [];
  const x = frames > 1 ? a + (frame / (frames - 1)) * (b - a) : a;
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ control: s.control })} style={{ background: "#0e0e10", color: "#c9c9d1", fontFamily: "Inter", fontSize: 12 }}>
      <div style={{ position: "absolute", left: playAt[0], top: 5, width: BTN, height: BTN, borderRadius: 6, background: "#1f1f25", display: "flex", alignItems: "center", justifyContent: "center" }}>
        <svg width={12} height={12} viewBox="0 0 24 24">
          <path d={playing ? "M7 4v16 M17 4v16" : "M6 4l14 8-14 8z"} fill={playing ? "none" : "#e8e8ea"} stroke="#e8e8ea" strokeWidth={playing ? 3 : 1.5} strokeLinejoin="round" />
        </svg>
      </div>
      <div style={{ position: "absolute", left: playAt[1] + 12, top: 0, height: HEAD, display: "flex", alignItems: "center", gap: 8, fontVariantNumeric: "tabular-nums" }}>
        <span style={{ color: "#e8e8ea" }}>{clock(frame, rate)}</span>
        <span style={{ color: "#6b6b74" }}>/ {clock(frames, rate)}</span>
        <span style={{ color: "#6b6b74" }}>{frames ? `${frame} of ${frames} frames at ${rate} fps` : "nothing timed on the stage (a game runs on)"}</span>
      </div>
      <div style={{ position: "absolute", left: a, width: b - a, top: HEAD, height: RULER, borderBottom: "1px solid #24242a" }}>
        {ticks.map((t) => (
          <div key={t} style={{ position: "absolute", left: (t / Math.max(secs, 1e-6)) * (b - a), top: 0, height: RULER, borderLeft: "1px solid #2c2c33", paddingLeft: 4, color: "#6b6b74", fontSize: 10, lineHeight: `${RULER}px` }}>
            {`${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`}
          </div>
        ))}
      </div>
      <div style={{ position: "absolute", left: a, width: b - a, top: HEAD + RULER + 8, height: Math.max(0, height - HEAD - RULER - 16), overflow: "hidden" }}>
        {frames > 0
          ? lanes(clips).map(({ c, lane }, i) => (
              <div key={i} style={{ position: "absolute", left: (c.start / frames) * (b - a), width: Math.max(2, ((c.end - c.start) / frames) * (b - a) - 1), top: lane * LANE, height: LANE - 3, borderRadius: 4, background: tint(c), color: "#e8e8ea", fontSize: 11, lineHeight: `${LANE - 3}px`, paddingLeft: 6, boxSizing: "border-box", overflow: "hidden", whiteSpace: "nowrap" }}>
                {c.name}
              </div>
            ))
          : null}
      </div>
      {frames > 0 ? <div style={{ position: "absolute", left: x - 1, top: HEAD, width: 2, bottom: 6, background: "#4f7bff" }} /> : null}
    </AbsoluteFill>
  );
}
