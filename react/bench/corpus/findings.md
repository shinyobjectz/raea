# Corpus findings (2026-10-18)

`react/bench/corpus/run.sh` runs the edits in `list.txt` through raea-motion's main paths:
- the render to MP4;
- render vs baked play, by SSIM at four frames;
- for games, a live play with random keys, then the same frames rendered from its input log.

The corpus holds 40 edits:
- 28 real agent-written compositions: 11 of score's ad edits on real footage, 17 of void's own TSX (11 ads, 6 games);
- 10 fixtures from git;
- 2 synthetic edits: a deck and a 3D shot.

The first pass failed 18 of 40. After the fixes below, 35 pass and the 5 that fail are all 3D, from two open causes (findings 5 and 6). Some of the first pass's failures were the harness's own: render didn't get the words and perception that play reads by itself (finding 7).

## Results (final pass, binary with every fix below)

| result | kind | edit | render ms/frame | detail |
|---|---|---|---|---|
| PASS | timeline | timelines/titles | 18.2 | worst SSIM 1.000; 3294 CPU stretches |
| PASS | game | games/pong | 5.3 | worst SSIM 1.000 |
| PASS | game | games/platformer | 3.8 | worst SSIM 1.000 |
| PASS | timeline | games/tones | 1.3 | worst SSIM 1.000 |
| PASS | timeline | golden/01-card-fade | 5.7 | worst SSIM 1.000 |
| PASS | timeline | golden/02-sequence-timing | 5.6 | worst SSIM 1.000 |
| PASS | timeline | golden/03-video-take | 6.9 | worst SSIM 1.000 |
| FAIL | 3d | golden/04-mesh-sign | 5.9 | render vs baked play: worst SSIM 0.000236 |
| FAIL | 3d | golden/05-world-solid | 6.7 | render vs baked play: worst SSIM 0.000236 |
| FAIL | 3d | golden/06-world-entities | 8.0 | render vs baked play: worst SSIM 0.0004 |
| PASS | timeline | fixtures/deck | 4.0 | worst SSIM 1.000 |
| FAIL | 3d | fixtures/box3d | 9.0 | render vs baked play: worst SSIM 0.709593 |
| PASS | timeline | c8_brief_full/client-a | 6.3 | worst SSIM 1.000 |
| PASS | timeline | c8_brief_full/client-a | 8.0 | worst SSIM 1.000 |
| PASS* | timeline | c8_brief_full/client-b | 5.1 | worst SSIM 1.000 |
| PASS | timeline | c8_brief_full/client-e | 8.3 | worst SSIM 1.000 |
| PASS | timeline | c8_brief_full/client-e | 9.2 | worst SSIM 1.000 |
| PASS | timeline | c8_brief_full/client-1 | 7.4 | worst SSIM 1.000 |
| PASS | timeline | c8_brief_full/client-2 | 7.5 | worst SSIM 1.000 |
| PASS | timeline | c8_brief_full/client-c | 6.2 | worst SSIM 1.000 |
| PASS | timeline | c8_brief_full/client-d | 8.5 | worst SSIM 1.000 |
| PASS | timeline | i7_ink_loop/client-3 | 5.0 | worst SSIM 1.000 |
| PASS | timeline | c3_xsheet/client-b.9x16 | 4.9 | worst SSIM 1.000 |
| PASS | timeline | 20261017-ts-nemotron/a-15-second-arock-promo | 16.1 | worst SSIM 1.000 |
| PASS | timeline | 20261017-ts-nemotron/a-30-second-jetflow-explainer | 9.2 | worst SSIM 1.000 |
| PASS | timeline | 20261017-ts-nemotron/client-a | 5.5 | worst SSIM 1.000 |
| PASS | timeline | 20261017-ts-nemotron/best-delivery-across-takes | 4.0 | worst SSIM 1.000 |
| PASS | timeline | 20261017-ts-nemotron/every-agency-is-a-state-machine | 4.4 | worst SSIM 1.000 |
| PASS | timeline | 20261017-ts-nemotron/automate-vs-simulate-as-a-designed-moment | 4.0 | worst SSIM 1.000 |
| PASS | timeline | 20261017-ts-smoke/client-a | 5.2 | worst SSIM 1.000 |
| PASS | timeline | 20261017-project/a-ugc-ad-that-needs-the-person-s-call | 9.9 | worst SSIM 1.000 |
| PASS | timeline | 20261017-project-glm2/a-ugc-ad-that-needs-the-person-s-call | 8.4 | worst SSIM 1.000 |
| PASS | timeline | 20261017-project-glm3/a-ugc-ad-that-needs-the-person-s-call | 8.6 | worst SSIM 1.000 |
| PASS | timeline | 20261017-project-glm3/an-ad-and-a-game-in-one-project-switching-between-them | 8.4 | worst SSIM 1.000 |
| PASS | game | 20261017-project/pong-a-change-and-back-to-the-first-version/pong | 10.2 | worst SSIM 1.000; 370 CPU stretches |
| PASS | game | 20261017-project-glm/an-ad-and-a-game-in-one-project-switching-between-them/pong | 4.3 | worst SSIM 1.000 |
| PASS | game | 20261017-project-glm2/an-ad-and-a-game-in-one-project-switching-between-them/pong | 11.5 | worst SSIM 1.000; 148 CPU stretches |
| PASS | game | 20261017-project-glm3/an-ad-and-a-game-in-one-project-switching-between-them/pong | 4.4 | worst SSIM 1.000; 79 CPU stretches |
| PASS | timeline | 20261017-project-glm/pong-a-change-and-back-to-the-first-version/pong | 2.3 | worst SSIM 1.000; 1390 CPU stretches |
| PASS | timeline | 20261017-project-glm3/pong-a-change-and-back-to-the-first-version/pong | 5.5 | worst SSIM 1.000; 47523 CPU stretches |

PASS* (client-b) passed on a re-run. In the final pass, one frame render was refused with "didn't commit frame 625 in 20 s" while the load average was about 41. See finding 9.

The render ms/frame figures are wall times on a shared, loaded machine, not measurements.

## Fixed

### 1. Play ignored score's `export const config` (all 11 ad edits)
Score's SDK backend writes `export const config = {width, height, fps, durationInFrames}` and registers no `<Composition>`. The engine never read it. A render got the right size and length only because score passes `--w --h --fps --frames`. A baked or live play has no such flags, so it played the CLI's default: 60 frames at 1080x1920. Every s2sdk edit played two seconds, and 16:9 and 1:1 edits played at the wrong shape.

Void's agents write the same form. `20261017-project/a-ugc-ad-that-needs-the-person-s-call` has `export const config = { width, height, fps, durationInFrames: total }`, built from variables.

Fix (6652c2e): the bundle entry (`react/src/motion.rs`, `entry()`) takes positive numbers from `M.config` when no Composition is registered.

### 2. Logged game input lost its `live` mark (3 of 3 void games)
Agents write games that play their own script unless a person is playing: `const inp = input.live ? input : scriptAt(frame)`. Live play marks each frame's input `live: true`. The play's `--input-log` wrote only `{keys, pointer}`, so a render of the log played the script, a different match.

Before the fix, replay SSIM was 0.976 to 0.992. The player's paddle sat at the bottom in the render and at the top in the play.

Affected edits, all void spaces: `20261017-project/pong-a-change-and-back-to-the-first-version`, `20261017-project-glm2/an-ad-and-a-game-...`, `20261017-project-glm3/an-ad-and-a-game-...`.

Fix (2503a23): `react/src/play.rs` logs `"live": true` with each frame. All three now replay at 1.000, and `react/bench/games/replay.sh` still passes.

### 3. Open-GOP HEVC failed to seek (client-e, and iPhone takes)
The render was refused with `VideoToolbox couldn't decode a frame (status -17694)`. The failing clips are HEVC takes (`7.1.mov` and `7.3.mov`, on a client drive) whose CRA keyframes are followed by leading RASL pictures (NAL types 9 and 8). `media/src/vtseek.rs` `frame_at` started a fresh seek at the CRA and decoded every sample up to the target, leading pictures included. Those reference the GOP before, which was never decoded, so VideoToolbox failed them ("reference missing"), and the error failed the whole seek.

`media/examples/vtseek_check` reproduces it on an ordinary iPhone take too: 40 random seeks into `media/real/cj.mov` panicked before the fix.

Fix (ea9a3a1): a seek that starts at its keyframe skips the samples shown before that keyframe. HEVC forbids trailing pictures from referencing RASL pictures, and a target that is itself a leading picture already starts from the keyframe before.

After the fix, vtseek_check gives worst diff 0 against AVAssetReader on take04, cj.mov and the H.264 fixture; the H.264 result is unchanged. client-e renders and matches play at 1.000.

### 4. A throw showed only JSC's stack, not the message (void's ugc ad)
`20261017-project/a-ugc-ad-that-needs-the-person-s-call` rendered without its words was refused with `the composition threw at frame 0: findPhrase@bundle.js:13725:16 ...`, with no reason given. The entry formatted errors as `e.stack || e.message`. V8's stack starts with the message, but JSC's doesn't.

Fix (a99e43b): the error is now `name: message` followed by the stack. The refusal reads `Error: word("automating"): the phrase is never said on the timeline`, which an agent can act on.

## Open (not fixed)

### 5. Baked play can't show 3D the way a render does (golden 04, 05, 06; fixtures/box3d)
Repro: run `raea-motion play motion/conformance/golden/04-mesh-sign/edit.tsx --baked --offscreen --3d --seconds 2 --snapshot 0.66 P.png` and `RAEA_READBACK=rgba raea-motion render motion/conformance/golden/04-mesh-sign/edit.tsx --frame 20 -o R.png`, then compare. SSIM is 0.0002, and P.png is all 00000000.

There are three causes:
- **Opt-in flag.** Play draws a scene only with `--3d` (`play.rs`, `live.scene_3d()`). Without it, the play shows the page and no 3D at all.
- **The 3D arrives late.** `Live::scene_3d` builds the scene's rows on a thread by stepping React through every frame. Until they arrive, frames are presented without the 3D and with the holders' backgrounds still on. A frame-0 snapshot came out either way between two runs (SSIM 0.908, then 0.0002).
- **Snapshots skip the 3D.** `Live::snapshot` (`live.rs`) paints only the DOM page (`paint_transparent`) and never composites the scene texture that `present` draws underneath. Once the scene is in, the holders are transparent and the snapshot is empty.

So no play snapshot of a 3D edit can match its render, and gate 5 has no automated check against play.

Suggested fix: read the snapshot back from the presented stage (page over scene), and let `--baked` with a 3D scene wait for its rows before frame 0, as the render does.

### 6. A gradient behind Canvas3D renders black (fixtures/box3d)
Repro: `react/bench/corpus/fixtures/box3d/edit.tsx` holds its `<Canvas3D>` in an `<AbsoluteFill style={{background: "linear-gradient(...)"}}>`. Rendering it with `RAEA_READBACK=rgba raea-motion render react/bench/corpus/fixtures/box3d/edit.tsx --frame 40 -o R.png` draws the 3D on black.

A render clears the holders' backgrounds so the 3D shows through, and takes the nearest holder's background as the scene's clear colour (`motion.rs` `clear_of`; `program`: `hex_of(clear)`, else `#000000`). A gradient isn't a hex colour, so it becomes black.

There is no authoring workaround. A sibling div behind the Canvas3D is page content, and the page draws over the 3D, so the div hides the scene. Agents use gradients constantly; the void and score kits put them on most cards.

Suggested fix: draw a holder's non-solid background as a page layer beneath the scene, rather than reducing it to a clear colour.

### 7. Render and play read an edit's words and perception differently
A baked play loads `words.json` and `perception.json` from beside the edit by itself (`live.rs` `open_baked`). A render uses them only with `--words` and `--perception`.

Without those flags, 10 of 11 score edits failed render vs play at SSIM 0.856 to 0.978: captions were missing, and perception-placed cards sat about 150 px off. Score and the harness pass the flags, so both pass.

Anyone running `raea-motion render EDIT.tsx` by hand still gets the edit without its captions. Suggested fix: have the render default to the files beside the edit, as play does.

### 8. CPU paint in play
The play report's cpu_paint is not zero for these edits:
- `timelines/titles`: 3294 stretches, all `layer` (its blurred glow);
- three void games: 79 to 370 stretches, `path_fill` or `layer`;
- two scripted void pongs: 1390 and 47523 stretches with every `why` counter at zero.

The last two mean some vello_cpu stretch is counted (`engine/src/quads/mod.rs`, `stats.fallbacks`) on a path that records no reason. Every `begin_capture` in `record.rs` calls `why()` first, so a reused recording may be counted again. Not chased further.

### 9. Smaller things seen
- **A frame render timed out once under load.** In the final pass, one frame render of `c8_brief_full/client-b` was refused with "didn't commit frame 625 in 20 s" while the load average was about 41. The edit passed in the three other runs, so the 20 s commit limit can fail a correct edit on a busy machine.
- **Generated sounds are silent in the harness.** Six edits use `sao(...)` or `lyria(...)`. The harness turns generation off (`RAEA_GEN_PAID=0`, no local SAO), so uncached sounds drop out with a note. Pixels are unaffected.
- **`useInput` games render their whole length.** platformer is 36000 frames (5 min at 120 fps) and takes about 137 s; pong is 4668 frames.
- **No panics, crashes or hangs.** No panic or crash file in any session, and no hang. No edit hit the lite runtime's insert_nodes_before panic that was fixed in be09791. No failure involved video cuts or the new VideoFrames prefetch; finding 3 is in the decoder below it.

## Corpus

| kind | count | edits |
|---|---|---|
| ad timelines on real footage, with captions and parts | 11 | score `s2sdk` (c8_brief_full x9, i7_ink_loop, c3_xsheet 9x16) |
| void agent ads and promos (FaceFrame, ScreenFit, slides, words) | 11 | `~/.raea-void/evals/runs/20261017-*` |
| games (`useInput`) | 6 | bench pong and platformer; void pong x4 |
| scripted matches (no input) | 2 | void glm and glm3 pong |
| motion graphics, no footage | 4 | titles, tones, golden 01 and 02 |
| deck | 1 | fixtures/deck (synthetic) |
| 3D | 4 | golden 04, 05, 06; fixtures/box3d (synthetic) |
| video under a title | 1 | golden 03 |

Of these, 28 are agent-written (11 score, 17 void), 10 are fixtures from git and 2 are synthetic. Void and score wrote no 3D scenes and no decks in TSX: void's deck scenarios predate the TSX switch and are rrl. The 3D coverage is therefore the conformance goldens plus the synthetic fixture.

Footage stays local and uncommitted:
- The score edits read `media/` links: `void/evals/fixtures/real/media` leads to `media/real`, which links into `~/.raea-void/media`, `~/Downloads` and eval runs.
- colitt and kelly read a client drive, read-only.
- Void's spaces read their own `media/` folders.

`list.txt` lists the edits by path only, and `run.sh` prints SKIP for any that aren't on the Mac it runs on.
