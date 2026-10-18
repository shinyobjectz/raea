# Games: useInput, useGameState, useGameEvents

## What to do

- **Lay the game out from the frame, never from fixed pixels.** The stage plays a game live in the window, full frame,
  at whatever size the window has, so `const { width, height } = useVideoConfig()` and every position, size and speed
  is a fraction of them: `const paddleH = height * 0.18`, `left: width * 0.04`, `fontSize: height * 0.08`, the ball's
  speed in widths per second. Keep the state in units of the field (0 to 1 across, 0 to 1 down) and scale to pixels
  only where you draw, so a resize mid-game keeps playing. A game written at `const W = 1080, H = 1920` plays
  letterboxed in a small box; that's a bug.
- **A person plays it.** While someone plays, each frame's input has `live: true` and their keys (`ArrowUp`,
  `ArrowDown`, `w`, `s`, ` `) and pointer: move the player's paddle from them, and let the other side be the
  computer. The scripted input timeline is only for renders: use it when `live` is absent.
- **Renders are landscape**: the composition's size is 1920x1080 at 60 fps unless the request says otherwise, never a
  vertical 9:16 unless asked for. `raea render` (each a version) and `raea check` use it.
- Write the game as a pure function: `step(state, input, frame) => nextState`. `input` is that frame's
  `{keys: string[], pointer?: {x, y, down}}`, and the step returns a new state object, never a changed old one.
- Read the state at the current frame with `useGameState(initial, step)`. It is `initial` at frame 0, and at frame
  `f` it is `step` applied to each earlier frame in order. Draw the frame from that state.
- Read this frame's input with `useInput()` (`{keys: []}` when there is none), for example to light the keys on screen.
- Cue sound from what happens in the game with `useGameEvents(initial, step, isEvent)`. It runs the same fold to
  the end of the composition and returns, in order, every frame at which `isEvent(before, after)` holds. Each frame
  is the first one that shows the new state. Then place one sound per event:
  `{hits.map((f) => <Sequence from={f} layout="none"><Audio src={thump} /></Sequence>)}`.
- Keep a contact as a flag on the state for the frame it happens on (`paddleContact`, `bricksHit`), so that both the
  step and `isEvent` can read it.
- Frames count from the nearest enclosing Sequence, so a game inside `<Sequence from={60}>` starts at its own
  frame 0 and reads input at the matching composition frames.
- Give the game its input as a script that lives in the file: an `InputTimeline` (`{"<frame>": {"keys": [...]}}`)
  that the step falls back to when the Clock has none, so `raea render` plays it the same way every time.
- Put the score, the input and the state on screen as parts, and theme them from the brand like any other piece.
- For many entities, keep them in an ECS world (`createWorld()` from `@raea/motion/ecs`). `spawn`, `set`, `query` and
  `system(fn)` build it, and `world.at(frame)` gives the world after frames 0 to frame-1 have run, the same every
  time it is asked for.

## Why

A video frame is a function of its number. `useGameState` folds from the start on every frame, so any frame renders
on its own and the same input always plays the same game, which is what makes a game renderable, checkable and
re-renderable. The fold reuses `initial` each time, so a step that mutates its state corrupts every later frame.

Sound cued from events lands on the frame the thing happens and stays right when the rules change. The approved
Breakout's 31 sounds come from `useGameEvents`, on the same frames that were once placed by hand from a play log.

## Common mistakes

- Mutating the state in `step` (`s.ball.x += s.ball.vx; return s`). Return new objects.
- `Math.random()` in `step`: every frame replays a different game. Seed from the frame or the entity's index.
- `useState` or `useEffect` to hold the game. Nothing persists between frames; the fold is the state.
- Hand-placing sound frames. Use `useGameEvents`.
- An `isEvent` that holds for several frames in a row (a ball touching the paddle for 3 frames), so one hit gives
  three sounds. Test for the change: `after.contact && !before.contact`.
- A game with no input script, so it sits idle.
- A heavy `step`. It runs about `frame` times per rendered frame, so keep it to arithmetic on plain objects.

## Example: a dodge game with input, a score and a sound per point

```tsx
import { AbsoluteFill, Audio, Composition, Sequence, registerRoot, sao, useGameEvents, useGameState, useInput, type FrameInput } from "@raea/motion";
import { css } from "./styles.gen";

const W = 1080;
const FRAMES = 240;
const CHIME = sao("bright short arcade coin chime, dry", { seconds: 0.5 });

interface Game { x: number; drop: { x: number; y: number }; score: number; caught: boolean }
const INITIAL: Game = { x: 480, drop: { x: 200, y: 300 }, score: 0, caught: false };

/** One frame of the rules: move the tray, let the drop fall, catch or respawn it. Pure: returns a new state. */
function step(s: Game, input: FrameInput, frame: number): Game {
  const dx = (input.keys.includes("ArrowRight") ? 24 : 0) - (input.keys.includes("ArrowLeft") ? 24 : 0);
  const x = Math.min(W - 120, Math.max(120, s.x + dx));
  const y = s.drop.y + 18;
  const caught = y >= 1500 && y < 1518 && Math.abs(s.drop.x - x) < 140;
  const respawn = caught || y > 1700;
  const nextX = 160 + ((frame * 389) % 760); // a seeded column, the same every render
  return { x, drop: respawn ? { x: nextX, y: 300 } : { x: s.drop.x, y }, score: s.score + (caught ? 1 : 0), caught };
}

const isPoint = (before: Game, after: Game) => after.score > before.score;

export default function Dodge() {
  const g = useGameState(INITIAL, step);
  const keys = useInput().keys;
  const points = useGameEvents(INITIAL, step, isPoint);
  return (
    <AbsoluteFill style={{ backgroundColor: "var(--brand-background)" }}>
      <style>{css}</style>
      <div style={{ position: "absolute", left: 90, top: 240, fontFamily: "var(--brand-font-heading)", fontWeight: 800, fontSize: 72, color: "var(--brand-text)" }}>
        {`SCORE ${g.score}`}
      </div>
      <div style={{ position: "absolute", left: g.drop.x - 30, top: g.drop.y, width: 60, height: 60, borderRadius: 30, backgroundColor: "var(--brand-accent)", transform: "scale(1)" }} />
      <div style={{ position: "absolute", left: g.x - 120, top: 1520, width: 240, height: 36, borderRadius: 18, backgroundColor: "var(--brand-text)", transform: `scale(${g.caught ? 1.08 : 1})` }} />
      {/* the input on screen: each key lights while it is held */}
      {(["ArrowLeft", "ArrowRight"] as const).map((k, i) => (
        <div key={k} style={{
          position: "absolute", left: 300 + 260 * i, top: 1640, width: 220, padding: "18px 0", borderRadius: 16, textAlign: "center",
          fontFamily: "var(--brand-font)", fontWeight: 700, fontSize: 40,
          backgroundColor: keys.includes(k) ? "var(--brand-accent)" : "transparent", color: "var(--brand-text)",
          border: "3px solid var(--brand-accent)", opacity: 1,
        }}>{i === 0 ? "LEFT" : "RIGHT"}</div>
      ))}
      {points.map((p) => (
        <Sequence key={p} from={p} name={`point ${p}`} layout="none">
          <Audio src={CHIME} gainDb={-6} />
        </Sequence>
      ))}
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="dodge" component={Dodge} width={1080} height={1920} fps={30} durationInFrames={FRAMES} />);
```
