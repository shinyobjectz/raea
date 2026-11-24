# Upgrading Bevy

raea depends on upstream Bevy crates and has not forked Bevy itself. A Bevy release is still more than a version bump,
because the versions are pinned exactly and a few things are tied to them. This is the checklist for the next bump
(written at Bevy 0.20.0 and wgpu 30.0.1, 2026-10-22; first written 2026-10-18 at Bevy 0.19.1 and wgpu 29.0.4).

Start with `tools/bevy-pins.sh`: it lists every Bevy, wgpu and plugin pin in the tree, the [patch] entries for them,
what each lockfile resolved (and exits 1 when a lock mixes Bevy or wgpu versions, or when Bevy's glam has scalar-math,
libm or fast-math on), the vendored crates raea changed with
their upstream base, and the Bevy shaders raea swaps at load with the upstream hash each was taken from. If it lists
something this file doesn't, this file is out of date: fix it here.

Bevy 0.20 needs rustc 1.97.1 or newer (`rust-version` in its Cargo.toml; the script prints it). The toolchain is
pinned in rust-toolchain.toml at the root (1.97.1, what 0.20 was ported and verified on), and rustup's proxies read it
in every checkout and on the CI runner; Homebrew's rust is uninstalled so nothing on PATH comes ahead of them. A bump
that needs a newer rustc moves that pin in the same branch.

## The pipeline: three lanes

When a Bevy release comes out, run these three in order as lanes, `tools/agents/agent start bevy-scout` and so on (their briefs
are docs/handoff/bevy-{scout,porter,verifier}.md, and each reads this file):

1. **bevy-scout** (read-only): can raea move yet? It compares crates.io against the pins, names the plugins that
   haven't caught up, picks out the migration-guide items that hit raea's code, and checks whether upstream now covers
   what raea's forks (step 3) still change. Ask it something like "a new Bevy is out; can we upgrade?".
2. **bevy-porter**, once the scout says ready: does steps 1 to 4 below on its own branch (`bevy-<version>`) in a
   worktree, never on the shared branch. It moves the pins, refreshes the lockfiles, re-vendors and ports the code, then
   hands over the branch.
3. **bevy-verifier**: runs step 5's checks on the branch and on the shared branch's tip, compares them, and says whether
   the branch is safe to merge, or lists what moved for the porter to fix.

Merging the branch is the person's call, made on the verifier's report.

## 1. Wait for the plugins

Bevy breaks its API every release, and these plugin crates need a version for the new Bevy before raea can move. When
one lags, raea can still move on a git pin to the plugin's merged port, or on a vendored copy with the port applied, as
0.20 did; replace each with the release once it exists (git pins and ports are kept only until then).

| crate | pinned (at 0.20.0) | where |
|---|---|---|
| avian3d (physics) | =0.8.0 (no `f32` feature since 0.8: f32 is its default) | engine/bevy-plugins/Cargo.toml |
| bevy_hanabi (GPU particles) | git rev 77630ff, the merge of djeedai/bevy_hanabi#576 (0.20 port), until a release | engine/bevy-plugins/Cargo.toml |
| bevy_tweening | git rev 2341b0c, the merge of djeedai/bevy_tweening#176 (0.20 port), until a release | engine/bevy-plugins/Cargo.toml |
| bevy_seedling (audio) | =0.8.0, vendored and ported to Bevy 0.20 (CorvusPrudens/bevy_seedling#134) | engine/seedling/Cargo.toml, vendor/bevy_seedling, root [patch] |
| firewheel (the mixer core) | =0.12.1, its five crates vendored and ported (BillyDM/Firewheel#144) | systems/mix/Cargo.toml, engine/seedling/Cargo.toml, vendor/firewheel*, [patch] in the root |

bevy_seedling is vendored without its cpal 0.18 paths, because cpal 0.18 can't link beside the legacy app's cpal 0.15. Take the new
upstream release and apply that change again; at 0.20 the copy also carries the Bevy 0.20 port (marked `raea:`), which
goes once a release has it. It pins Firewheel, and raea-mixer has to move to the same Firewheel, so the mixer core's
tap = bounce check (step 5) belongs to the same change. Firewheel 0.12 is vendored under vendor/firewheel* only for its
Bevy 0.20 port (bevy_reflect 0.20 and a glam-33 feature); drop the copy and both [patch] blocks once a release has it.

bevy_pbr is vendored (vendor/bevy_pbr, the root [patch]) for one change Bevy has no hook for: the sun's far shadow map
(raea-sys-rt far.rs; docs/realism.feature LT21) bound at 39 and 40 of every mesh view group, a texture and its uniform
(`RaeaFarSun` and `RAEA_FAR_SUN_BYTES` in render/mesh_view_bindings.rs, the struct and bindings in
render/mesh_view_bindings.wesl, a stand-in and a uniform of zeros where there's none). Every change is marked `raea:`.
Re-take the new release's crate, apply those hunks again, and check that the indices are still free; shadows.wesl is
left as Bevy's, because raea-sys-lod swaps it at load (its hash checked) to read the map past the last cascade.

## 2. Bump every pin together

All of these move in one commit. Mixed Bevy versions don't build, and mixed wgpu versions can't share a device.

- `bevy = "=0.20.0"`: engine/render, sdk/react, core/animate, engine/bevy-plugins and engine/seedling (Cargo.toml in
  each), and apps/legacy/shell-bevy once it is in the tree. `bevy_clipboard` moves with it wherever it is used.
- `bevy_ecs = "=0.20.0"`: engine/html.
- `wgpu = "=30.0.1"`: engine/html, engine/render, apps/legacy/shell, systems/k and gpu/device (sdk/react reaches it through
  raea-render). It must equal the version the new Bevy uses, because quads, raea-render and Bevy draw on one device.
  engine/html/bench/gpu/raster (`wgpu = "30"`, outside the workspace) draws on its own device, so it is separate. The
  root lock also carries wgpu 29.0.4 for `pixels`, dioxus-native's iOS-simulator window renderer, which has its own
  device and isn't built on macOS (tools/bevy-pins.sh knows it).
- Then refresh Cargo.lock in the same commit (`cargo metadata --offline`, online when new crates are needed) and check
  it with `cargo check --locked`.

## 3. Re-take the forked Bevy shaders

wgpu-core is no longer vendored: wgpu 30 moved the fence's sync into wgpu-hal (gfx-rs/wgpu#9475), so Metal's drawable
acquire no longer waits behind another thread's submit (the stall ad90377 patched), and wgpu 30's Metal returns
`Occluded` at once for a hidden window, which quads already handles.

Three of Bevy's shaders are still changed, all swapped in at load by `bevy_api::swap_embedded_shader`
(engine/render/src/engine/bevy_api.rs). It checks Bevy's embedded shader against the FNV-1a hash of the one raea's change
was taken from and panics at startup when a release changes or drops it, so a bump can't draw with a stale fork:

- bevy_pbr/volumetric_fog/volumetric_fog.wesl: a full copy (engine/render/src/engine/volumetric_fog.wesl) with 12
  `raea:` hunks: the march's dither seeded from the pixel and the film frame (Bevy seeds it from the frame count, so a
  render and a play differ), FOG_HALF's depth binding and raster scale for fog_half.rs, and the reference's whole-ray
  length and scattered ambient (RAEA_FOG_REFERENCE). Re-take it from the new release, apply the hunks again, and record
  the new hash in scene3d.rs's `install_fog_shader`. Also re-read Bevy's volumetric_fog/render.rs against
  fog_half.rs and froxel.rs, which build their own pipelines around Bevy's fog bindings and view layouts.
- bevy_pbr/light_probe/environment_filter.wesl: not copied; one line patched where it loads (16 times the sample
  count, environment.rs). If the line moved, the patch's `replace_once` fails loudly; fix the line and record the new
  hash.
- bevy_post_process/dof/dof.wesl: not copied; patched where it loads (dof.rs) so depth of field keeps alpha: the
  bokeh's box blurs average all four channels, and the Gaussian passes blur alpha with a copy of
  gaussian_blur.wesl's kernel (that library loads as Bevy builds its plugin, too early to swap). Without it the paint
  look's sky turns black under a <DepthOfField>. If an anchor moved, `replace_n` fails loudly; also compare the
  kernel in dof.rs with the new gaussian_blur.wesl, then record the new hash.
- bevy_post_process/effect_stack/post_process.wesl: not copied; patched where it loads (effect_stack.rs) so the effect
  stack (chromatic aberration, lens distortion, vignette) keeps alpha: alpha follows the aberration's samples instead
  of the 1.0 Bevy writes. Without it the paint look's sky turns black under a <Grade aberration> above Bevy's 1e-4
  cut-off. If an anchor moved, `replace_once` fails loudly; compare `raea_aberration_alpha` with the new
  chromatic_aberration.wesl's sampling, then record the new hash.

Two more are patched after they load, as assets, because Bevy loads them as libraries while its plugins build, before
any swap can run (`prepass_pass` in engine/render/src/engine/compose.rs, hash-checked the same way, panicking on a
mismatch):

- bevy_pbr/prepass/prepass.wesl and bevy_pbr/render/pbr_prepass.wesl: the normal prepass's alpha carries the mesh's
  composition pass (its MeshTag's two low bits; 1.0, as Bevy writes it, for the default pass), which the outline, the
  paint look and the compose passes read. If an anchor moved, `replace_once` fails loudly; check that the normal is
  still written to an Rgb10a2 target with its alpha free, then record the new hashes.

compose.rs also orders its systems around two of Bevy's crate-private Core3d systems (`depth_of_field` and
`post_processing`, the effect stack) by name, from a ScheduleBuildPass (`Around`). If a release renames them it logs
"compose: Bevy's depth_of_field or post_processing system not found" and the composition passes' grade and bloom hold
come out in the wrong place: sdk/raea/bench/bevy/passes/check.sh fails on the page's colours.

Prefer this patch-at-load form to a copy for any new change to a Bevy shader: a bump then re-checks a hash instead of
re-applying a diff.

## 4. Port raea's code that uses Bevy

Expect a few days of migration per release. In order of size:

- raea-render: the 3D engine (src/engine.rs, src/engine/scene3d.rs), the GPU view and its feed of pages into the 3D
  (src/gpuview.rs), and the live engine's rows (Engine::set_rows, remove_rows). Its own render passes reach Bevy's
  render API through src/engine/bevy_api.rs where they can (the view's depth binding, the HDR format, the shader swap,
  the defaults put back), so a release that moves those changes one file. Its shaders are WESL since 0.20 (all but
  froxel_minmax.wgsl, which imports nothing); module paths follow the file (`raea_render::engine::froxel_fog`).
- Changed defaults: Bevy's new default is the one raea wants (Shane, 10-22: "if 0.20 wants something or standardizes
  something its likely what we need to adopt"). A default that changes how renders look is shown to Shane as
  before/after stills before it lands; until then the old value is put back in one place,
  `bevy_api::install_0_19_defaults`, and listed under "Pin-backs and forks kept" below. Don't pin a default silently.
- react: live 3D for games (runtime/scene LiveScene), the DOM mirror's ECS (src/lib.rs), and Animato's
  sampling (src/animate.rs).
- animate: the Bevy plugin (src/bevy.rs: FrameClock, TransformAnimation, AnimatePlugin).
- bevy-plugins: the generic reflect rows. Regenerate their TypeScript with `raea-bevy-sdk`, and regenerate
  sdk/math/src/animate.gen.ts (`raea-bevy-sdk --types-only`).
- seedling: the platform plugin over raea-mixer's cpal 0.15 backend.
- raea-runtime-js (TS and React UI on bevy_ui, docs/ts-native.feature P20): src/host.rs (elements, input, focus, text
  fields), src/tw.rs (styles), src/svg.rs and the plugin in src/lib.rs. Nothing else in the crate, and nothing in
  the legacy shell's TSX layer (apps/legacy/shell-bevy/src/tsx.rs), names bevy_ui. The bevy_ui changes 0.20 brings are listed in
  the crate's docs (src/lib.rs, "Bevy upgrades").

## 5. Prove nothing moved

Every check below must hold at the same values as before the bump (render = play at SSIM 1.000000):

- tools/bevy-pins.sh exits 0 (no lock mixes Bevy or wgpu versions, and Bevy's glam is plain).
- sdk/raea/bench/bevy/check.sh: every 3D fixture, render against baked play, seeks included.
- sdk/react/bench/games/replay.sh: pong, platformer and arena3d, render against live play.
- engine/render: `cargo test --release --lib` passes (the substrate gate went with raea-render's SQL, 2026-10-23).
- sdk/react/bench/games/export.sh: exported players pass their self-tests and render 1.000000.
- sdk/react/bench/games/audio.sh and sdk/react/bench/tone/check.sh: tap = bounce, difference 0 (seedling's fixture included).
- the legacy app's conformance suite, headless (apps/legacy/shell/conformance, its bevy substrate at the full tier): the baseline's
  pass count (69/0/0 on 10-22). (apps/legacy/shell/bench/games_play.py drove the Dioxus window's MCP and went with it, 222ed773.)
- bench/lightspeed/feel.sh: arena3d's frame work p99 and p99.9 paired before and after. A Bevy release can change
  render cost, and arena3d's frame is mostly Bevy's render (docs/lightspeed.feature, F2).

## Pin-backs and forks kept, and what lets each go

Each line is something raea keeps where Bevy has its own, with the measured reason and what would let it go. Written in
the adopt-0.20 pass (2026-10-22); stills in that pass's report. Anything not listed here uses Bevy's own.

- Tonemapping::None as 0.19 drew it (`bevy_api::Untonemapped`). 0.20's None skips the deband dither, grading and the
  clamp. Measured on the fixtures: studio (tone mapping "none") moves at most 22 of 255, mean 0.09, SSIM 0.9985 (the
  dither's pattern, plus a few highlights); paint at most 2 of 255. Goes when Shane takes 0.20's None.
- ScreenSpaceTransmission required on every Camera3d. Without it 0.20 draws a transmissive material by its alpha mode,
  and a clear glass slab turns black (sdk/raea/bench/bevy-ts/cases/transmission.tsx, SSIM 0.46). Goes when the SDK
  opts a camera in the 0.20 way: a camera component from TSX, or a camera that draws a transmissive material. Shane
  picks which. No sdk/raea/bench/bevy fixture draws transmission, so the render = play gate can't see it yet.
- environment_filter.wesl's sample count x16 (environment.rs). Measured: at Bevy's count the environment fixture's
  glossy torus speckles (at most 32 of 255, SSIM 0.9982). Goes when Bevy's filter takes a sample count (generate.rs
  still hard-codes 32 x 2^(roughness x 4) at 0.20); an upstream PR would do it.
- volumetric_fog.wesl, a full copy (lightspeed's lane). Bevy seeds the march's jitter from `globals.frame_count`
  (volumetric_fog.wesl:202 at 0.20), so a frame rendered alone and the same frame in play march different samples:
  render = play can't hold. It also carries FOG_HALF and the reference mode. Goes when Bevy's fog takes a seed (an
  upstream PR); fog_half.rs stays until Bevy has a half-resolution fog.
- CPU mip chains (scene3d.rs mips_rgba8 and mip_gltf_textures, engine.rs mip_chain). 0.20's two mip paths don't cover
  them: MipGenerationJobs downsamples on the GPU through storage textures, which can't be sRGB (Rgba8UnormSrgb isn't in
  its formats), so a base colour would be averaged in gamma, not linear light; panels need premultiplied averages; and
  CompressedImageSaver runs only in Bevy's asset processing (lossy BC7/ASTC through ctt), not on images loaded at run
  time. Goes for baked assets once lightspeed's bake (F5) runs CompressedImageSaver; mip_chain stays for run-time images.
  Its feature (compressed_image_saver) is out of the build since 0.1.8: ctt's AVX2 encoders run C++ initializers before
  main on any CPU, so the bake has to bring it back without them (sdk/raea/docs/bevy-plugins.md).
- rt.rs (the traced-AO and passes' BLAS/TLAS) beside bevy_solari's scene layer. Solari's binder skips any mesh that
  isn't u32-indexed with exactly position, normal, UV0 and tangent and `enable_raytracing` (raea's own shapes are
  unindexed), and writes ray mask 0xFF on every instance (rt.rs's CASTS bit keeps NotShadowCaster stand-ins out of sun
  rays). Goes when raea's meshes carry indices and tangents and the CASTS filter moves to "no RaytracingMesh3d"
  (docs/bevy-0.20.feature, "Replace rt.rs with Solari's scene layer").
- sdk/react's reset (lib.rs, "reset"): a map of React ids drained, not a query, so despawn_all doesn't fit; it stays.
- bevy_ui_render's copy (UI encoded for the camera's CompositingSpace, gradients, glyph contrast and placement,
  shadows) and bevy_text's synthetic bold and italic and `GlyphRaster` went with raea-runtime-js's bevy_ui host in the
  overhaul (10-27): every page is Blitz's, and only that host drew CSS through bevy_ui. bevy_ui_render is crates.io's;
  bevy_text and bevy_ui keep only the parley 0.12 and Taffy 0.15 port below.
- parley 0.12 and Taffy 0.15 in the vendored bevy_text, bevy_ui and bevy_ui_widgets (vendor/bevy_ui_widgets,
  vendored for this alone, the root [patch]), where 0.20 has parley 0.11 and Taffy 0.14: Blitz
  0.3.0-beta.4's versions, so the page and Bevy share one text and one layout engine (docs/blitz-bevy.feature B0;
  tools/one-version.sh). The port: Taffy's alignment keywords (non-Option, `NORMAL`), parley's InlineBox baseline,
  `font_metrics` and `hanging_advance`. Two fixes it needed: parley 0.12 lays lines out as CSS line boxes with a strut
  from the root style, so bevy_text's pipeline.rs sets the root text's font and line height as the defaults (a
  `leading-none` text-sm paragraph drew 16 px lines, not 14); and Taffy 0.15 keeps a containing block's absolutely
  positioned descendants in a hoisted list rebuilt only when it lays the block out as a container, so ui_surface.rs
  dirties nodes before they go or move and swaps a node left with no children for a fresh one (`unhoist`), else
  rounding visited a removed node (a panic: invalid SlotMap key; five corpus cases, focus and forms). Goes when Bevy
  takes parley 0.12 and Taffy 0.15 (Bevy main is on 0.11 and 0.14 at 10-24); the Taffy list is worth an upstream issue.
- No fontique fork on the Bevy side. bevy_text's parley 0.12.0 uses crates.io's fontique 0.12.0 (raea-render's font
  check, its only other user, was deleted 10-27 with no callers). The fork in
  engine/html/vendor/fontique (CoreText weights and member order, Blink's synthetic-bold threshold, Chrome's generic
  families, unicode_ranges and weight_range overrides) serves only Blitz's git parley through the root
  [patch."https://github.com/linebender/parley"], and goes with engine/html. `@font-face` weight ranges are done
  without it: raea-runtime-js registers a ranged or variable face at each hundred of its range, a hundredth off on the side
  CSS's matching looks first, so fontique's Synthesis sets the `wght` axis to the asked weight (css.rs `register`).
  unicode-range waits for a fontique release with per-face ranges.

Adopted in that pass, pixels identical on every fixture: SSAO's radius is Bevy's default (it was set to the same
value), the glTF scene wait is Bevy's WorldInstanceReady (it counted frames), and the load's clear and store::clear are
`World::despawn_all`.

## What a Bevy bump doesn't touch

The 2D page engine (Blitz, Stylo, Parley and quads in engine/), the mixer core's own code, the media crate, the TSX
runtime (sdk/react/js/lite) and the legacy app's chrome don't depend on Bevy, apart from the engine's use of bevy_ecs as a data
store. Vendored Blitz, Stylo and Parley follow their own upstreams on their own schedule.

## Keeping the next bump small

What made 0.19 to 0.20 cost more than a version change, and what now absorbs it:

- Shader language: Bevy moved from naga_oil to WESL, and 18 of raea-render's shaders were converted by hand (imports first,
  `#ifdef` as `@if` on declarations, int defs as `constants::NAME`, two `a * b ^ c` expressions WESL groups
  differently). That was a one-time cost; new shaders should be written in WESL.
- Plugins behind Bevy: Firewheel and bevy_seedling had no release for 0.20, so both are vendored with their port, and
  hanabi and tweening are on git revs. Each is marked as temporary in its Cargo.toml and in step 1; the first job of the
  next bump is to replace them with releases.
- Bevy's render API drifting under raea-render's own passes (depth views split by aspect, the HDR format constant gone,
  ShaderBuffer's constructors, `#[extract_app]` on extract derives, RenderQueue and friends behind constructors):
  new Bevy-facing calls in a pass go through bevy_api.rs rather than being written out in the pass.
- Forked shaders: a copy has to be re-taken by hand every release. Patch Bevy's shader at load where the change is a
  line or two, and keep the hash check on any copy.
- Defaults Bevy changes: put the old value back in one place (bevy_api.rs) so the next bump can see the list.
- Shared math crates: a Bevy release can move onto a glam another dependency already uses, and Cargo then unifies
  their features. At 0.20 Rapier's `enhanced-determinism` turned on glam's scalar-math and libm for Bevy too, and
  Bevy's GPU structs lost their alignment (meshes drew wrong or failed validation). rapier3d now goes without it, and
  tools/bevy-pins.sh fails if it comes back. Without it Rapier's bakes are still the same run to run, but not the
  same as before: the verifier's re-bakes at 0.20 against 0.19 moved 49 of the kitchen film's 144 bodies more than
  1 cm (the worst 37.5 m: a body that falls differently ends somewhere else), and the breakfast egg 95 mm. Shane took
  the new physics as it is (10-22). A bake is cached by its request, so a composition keeps its cached bake, and its
  look, until something re-bakes it.
- naga: each wgpu release can tighten WGSL. wgpu 30 wants `enable wgpu_binding_array;` before a binding_array
  (engine/html/src/quads/fine.wgsl). Shaders that wgpu compiles directly (quads, GPU kernel, raea-render's nv12) are checked only
  at run time, so render something after the bump before anything else.
- Reflected type paths move with Bevy's crates (Tonemapping is bevy_render::view::Tonemapping since 0.20); the
  regenerated TypeScript shows them.
