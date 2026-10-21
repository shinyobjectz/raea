# Upgrading Bevy

raea depends on upstream Bevy crates and has not forked Bevy itself. A Bevy release is still more than a version bump,
because the versions are pinned exactly and a few things are tied to them. This is the checklist for the next bump
(written 2026-10-18 at Bevy 0.19.1 and wgpu 29.0.4).

## The pipeline: three subagents

When a Bevy release comes out, run these three in order (they live in .claude/agents/, and each reads this file):

1. **bevy-scout** (read-only): can raea move yet? It compares crates.io against the pins, names the plugins that
   haven't caught up, picks out the migration-guide items that hit raea's code, and checks whether the wgpu-core fix
   has landed upstream. Ask it something like "a new Bevy is out; can we upgrade?".
2. **bevy-porter**, once the scout says ready: does steps 1 to 4 below on its own branch (`bevy-<version>`) in a
   worktree, never on the shared branch. It moves the pins, refreshes the lockfiles, re-vendors and ports the code, then
   hands over the branch.
3. **bevy-verifier**: runs step 5's checks on the branch and on the shared branch's tip, compares them, and says whether
   the branch is safe to merge, or lists what moved for the porter to fix.

Merging the branch is the person's call, made on the verifier's report.

## 1. Wait for the plugins

Bevy breaks its API every release, and three plugin crates have to publish a version for the new Bevy before raea can
move:

| crate | pinned | where |
|---|---|---|
| avian3d (physics) | =0.7.0 | engine/bevy-plugins/Cargo.toml |
| bevy_hanabi (GPU particles) | =0.19.0 | engine/bevy-plugins/Cargo.toml |
| bevy_tweening | =0.16.0 | engine/bevy-plugins/Cargo.toml |
| bevy_seedling (audio) | =0.8.0, vendored | engine/seedling/Cargo.toml, engine/seedling/vendor/bevy_seedling |
| firewheel (the mixer core) | =0.12.0 | engine/mixer/Cargo.toml, engine/seedling/Cargo.toml |

bevy_seedling is vendored without its cpal 0.18 paths, because cpal 0.18 can't link beside void's cpal 0.15. Take the new
upstream release and apply that change again. It pins Firewheel, and raea-mixer has to move to the same Firewheel, so
the mixer core's tap = bounce check (step 4) belongs to the same change.

## 2. Bump every pin together

All of these move in one commit. Mixed Bevy versions don't build, and mixed wgpu versions can't share a device.

- `bevy = "=0.19.1"`: rdb, react, animate, bevy-plugins, seedling and ios-probe (Cargo.toml in each).
- `bevy_ecs = "=0.19.1"`: engine (and spikes/deno-modules).
- `wgpu = "=29.0.4"`: engine, rdb, react (through rdb), void, kgpu, gputrace, ios-probe, bench/hdc-skip and
  bench/render-smoke. It must equal the version the new Bevy uses, because quads, rdb and Bevy draw on one device.
  void/native and engine/html/bench/gpu/raster use wgpu 30 on their own device, so they are separate.
- Then refresh every lockfile in the same commit: react, rdb, void, sdk/react/ios, ios-probe, browser and the rest
  (`cargo metadata --offline` in each), and check each with `cargo check --locked`.

## 3. Re-apply the wgpu-core patch

engine/html/vendor/wgpu-core is wgpu-core 29.0.4 with one change in src/present.rs (marked `raea:`): Metal's drawable
acquire happens outside the device's fence lock, so void's stage doesn't stall on present (commit ad90377). void
patches it in (apps/void/Cargo.toml, `[patch.crates-io] wgpu-core`). On a wgpu bump, vendor the new wgpu-core and apply that
one change again. Check upstream first, since the fix may have landed there.

## 4. Port raea's code that uses Bevy

Expect a few days of migration per release. In order of size:

- rdb: the 3D engine (src/engine.rs, src/engine/scene3d.rs), the GPU view and its feed of pages into the 3D
  (src/gpuview.rs), and the live engine's rows (Engine::set_rows, remove_rows).
- react: live 3D for games (src/motion/scene3d.rs LiveScene), the DOM mirror's ECS (src/lib.rs), and Animato's
  sampling (src/animate.rs).
- animate: the Bevy plugin (src/bevy.rs: FrameClock, TransformAnimation, AnimatePlugin).
- bevy-plugins: the generic reflect rows. Regenerate their TypeScript with `raea-bevy-sdk`, and regenerate
  sdk/motion/src/animate.gen.ts (`raea-bevy-sdk --types-only`).
- seedling: the platform plugin over raea-mixer's cpal 0.15 backend.

## 5. Prove nothing moved

Every check below must hold at the same values as before the bump (render = play at SSIM 1.000000):

- sdk/motion/bench/bevy/check.sh: every 3D fixture, render against baked play, seeks included.
- sdk/react/bench/games/replay.sh: pong, platformer and arena3d, render against live play.
- engine/rdb/bench/substrate-gate.sh: 14 of 14.
- sdk/react/bench/games/export.sh: exported players pass their self-tests and render 1.000000.
- sdk/react/bench/games/audio.sh and sdk/react/bench/tone/check.sh: tap = bounce, difference 0 (seedling's fixture included).
- apps/void/bench/games_play.py: every game in a headless void, its 3D drawn (play against render 0.999 or better).
- bench/lightspeed/feel.py: arena3d's frame work p99 and p99.9 paired before and after. A Bevy release can change
  render cost, and arena3d's frame is mostly Bevy's render (docs/lightspeed.feature, F2).

## What a Bevy bump doesn't touch

The 2D page engine (Blitz, Stylo, Parley and quads in engine/), the mixer core's own code, the media crate, the TSX
runtime (sdk/react/js/lite) and void's chrome don't depend on Bevy, apart from the engine's use of bevy_ecs as a data
store. Vendored Blitz, Stylo and Parley follow their own upstreams on their own schedule.
