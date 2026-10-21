// The player under the stage: play and pause, the time, a track to scrub, the length. The stage's player as void hands
// it in (voidIn "player"), driven by publishing numbered controls ({ control: { n, do, frame } }), which void runs on
// the stage once each. A panel named player stands in for void's own player bar.
import { AbsoluteFill, applyActions, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

type Player = { frame: number; frames: number; rate: number; playing: boolean };
// a seek is kept as the pointer's x until it's drawn: where x falls on the track depends on the panel's width, which
// the render has (props) and the step doesn't
type Control = { n: number; do: "play" | "pause" | "seek"; frame?: number; x?: number };
type S = { player: Player; control: Control; down: boolean };

const BUTTON = 44, TIME = 56, PAD = 12;
const clock = (f: number, rate: number) => {
  const t = Math.floor(Math.max(0, f) / (rate || 30));
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
};
const next = (s: S, c: Omit<Control, "n">): S => ({ ...s, control: { ...c, n: s.control.n + 1 } });

export const actions: PanelActions<S> = {
  play: { about: "Play the video on the stage", run: (s) => next(s, { do: "play" }) },
  pause: { about: "Pause it", run: (s) => next(s, { do: "pause" }) },
  seek: { about: "Move the playhead to a time", args: { seconds: "number" }, run: (s, a) => next(s, { do: "seek", frame: Math.round(Number(a.seconds) * (s.player.rate || 30)) }) },
};

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const pl = voidIn<Player>(input, "player");
  if (pl) s = { ...s, player: pl };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down && p.x < PAD + BUTTON) s = next(s, { do: s.player.playing ? "pause" : "play" });
  else if (p && down && p.x >= PAD + BUTTON + TIME && (!s.down || s.control.do === "seek")) {
    if (s.control.do !== "seek" || s.control.x !== p.x || !s.down) s = next(s, { do: "seek", x: p.x });
  }
  return down === s.down ? s : { ...s, down };
};

export default function PlayerBar({ width }: { width: number }) {
  const s = useGameState<S>({ player: { frame: 0, frames: 0, rate: 30, playing: false }, control: { n: 0, do: "pause" }, down: false }, step);
  const { frame, frames, rate, playing } = s.player;
  const left = PAD + BUTTON + TIME, right = width - PAD - TIME;
  const along = (x: number) => Math.min(1, Math.max(0, (x - left) / Math.max(1, right - left)));
  const c = s.control;
  const control = c.do === "seek" && c.x !== undefined ? { n: c.n, do: "seek", frame: Math.round(along(c.x) * Math.max(0, frames - 1)) } : c;
  const fill = frames > 1 ? (frame / (frames - 1)) * 100 : 0;
  // a game has no timeline (frames 0): it plays on, live
  if (frames === 0) {
    return (
      <AbsoluteFill data-void-publish={JSON.stringify({ control })} style={{ background: "#111113", flexDirection: "row", alignItems: "center", gap: 8, padding: `0 ${PAD + 12}px`, fontFamily: "Inter", fontSize: 12, color: "#8b8b94" }}>
        <div style={{ width: 8, height: 8, borderRadius: 4, background: "#22c55e" }} />
        Live: it plays on as you play it
      </AbsoluteFill>
    );
  }
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ control })} style={{ background: "#111113", flexDirection: "row", alignItems: "center", padding: `0 ${PAD}px`, fontFamily: "Inter", fontSize: 12, color: "#c9c9d1" }}>
      <div style={{ width: BUTTON, height: 32, display: "flex", alignItems: "center", justifyContent: "center" }}>
        <svg width={16} height={16} viewBox="0 0 24 24">
          {playing ? (
            <>
              <rect x={6} y={5} width={4} height={14} rx={1} fill="#e8e8ea" />
              <rect x={14} y={5} width={4} height={14} rx={1} fill="#e8e8ea" />
            </>
          ) : (
            <path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z" fill="#e8e8ea" />
          )}
        </svg>
      </div>
      <div style={{ width: TIME }}>{clock(frame, rate)}</div>
      <div style={{ flex: 1, height: 32, display: "flex", alignItems: "center", position: "relative" }}>
        <div style={{ position: "absolute", left: 0, right: 0, height: 4, borderRadius: 2, background: "#2a2a30" }} />
        <div style={{ position: "absolute", left: 0, width: `${fill}%`, height: 4, borderRadius: 2, background: "#2b6cff" }} />
        <div style={{ position: "absolute", left: `${fill}%`, width: 12, height: 12, marginLeft: -6, borderRadius: 6, background: "#ffffff" }} />
      </div>
      <div style={{ width: TIME, textAlign: "right" }}>{clock(frames, rate)}</div>
    </AbsoluteFill>
  );
}
