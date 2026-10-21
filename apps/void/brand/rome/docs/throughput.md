# The Ides of March: render throughput

What a render of the rome film (`film.tsx`, 4:29, 8,064 frames at 1920x1080, 30 fps) costs, where the time goes, and
what has been done about it. Numbers are wall time on the M5 Pro (64 GB), measured under the machine's ordinary load
(other sessions' renders and builds running) as paired A/B runs, not on a quiet machine.

## How to measure

- `raea-motion render film.tsx -o OUT.mp4` prints the whole render's `ms a frame` on its last line.
- `RAEA_FRAME_LOG=FILE` writes one line a frame, tab-separated, in ms: `react` (the frame's React commit settled),
  `sync` (props read, videos placed, the 3D's holders), `draw3d` (Bevy's update: systems, extract, prepare, queue,
  passes recorded and submitted), `page` (Blitz's style, layout and paint over the 3D), `gpu` (rdb's wait for the
  frame's GPU work). Their sum is close to the printed `ms a frame`. A frame-logged render never comes from the
  export cache.
- `RAEA_FRAME_LOG_SERIAL=1` also waits for the GPU after the 3D and after the page (`gpu3d`, `gpupage`), so the GPU's
  time splits by stage. It runs one stage at a time, so use it to attribute, not to time the whole.
- Only a fed render (an .mp4/.mov) logs; `--frame` and `--at` take another path and write no log.
- A scene's or a line's cost: average the log's rows over its frames (`timeline.json` gives each line's start).
- Delete each full render once it's measured (about 340 MB, and a copy lands in `~/Library/Caches/raea/exports`
  unless the render was frame-logged or `RAEA_EXPORT_CACHE=0`), and keep at least 5 GB free before starting one.

## Where the time goes

The film is GPU-bound: in the frame log, the GPU wait is two thirds of a frame, and Bevy's CPU side (`draw3d`) most of
the rest. React, sync and the page together are about 3 ms.

By narration line (ms a frame, frame log of the current build, film.tsx as cut 14 renders it: skies, AO, SMAA and
4096-texel shadows):

| line | frames | ms a frame | GPU wait |
|---|---:|---:|---:|
| intro (title) | 180 | 60.8 | 49.9 |
| open | 553 | 14.3 | 5.0 |
| dictator | 778 | 18.4 | 7.0 |
| plot | 549 | 75.4 | 61.5 |
| omens | 676 | 30.6 | 19.5 |
| decimus | 516 | 35.3 | 26.7 |
| street | 748 | 13.0 | 5.3 |
| curia | 433 | 39.4 | 28.8 |
| strike (seized, stabs) | 744 | 67.0 | 59.8 |
| wounds | 773 | 13.6 | 5.2 |
| funeral | 674 | 17.4 | 7.5 |
| coin | 558 | 15.7 | 8.5 |
| close | 882 | 24.8 | 15.8 |

The expensive lines are the ones with volumetric lights in a FogVolume (the title's brazier, plot's shaft over sixty
busts, the stabs' blood fog and key, curia's shaft): Bevy's volumetric pass ray-marches 128 steps a pixel through
every volumetric light's shadow map. The plain-lit lines cost 13-18 ms.

## What changed

### Twice real time: the light filtered once, React a frame ahead, a parallel start (2026-10-20)

The whole film at 1920x1080 now renders in about 2 minutes (13-14 ms a frame), half the build before. Nothing in the
film changed; every change is in the engine.

- **The environment light is filtered once per panorama** (engine/rdb/src/engine/environment.rs). Bevy's
  GeneratedEnvironmentMapLight re-ran its radiance and irradiance filters every frame for the same panorama
  (`lightprobe_radiance_map`, `lightprobe_irradiance_map`: 2.2 ms of GPU a frame in a Metal System Trace of curia). Each
  panorama now has a bake entity with its own targets; once the render world has seen Bevy filter it, its generator
  comes off and its targets keep the light. The camera carries a plain EnvironmentMapLight that points at the targets
  of the panorama its row picks. The filter is deterministic, so the light is the same: the `environment` fixture
  (which cuts from one panorama to another at frame 30) is byte-identical at frames 0, 30 and 59.
- **React renders the next frame while this one is drawn** (render_fed in sdk/react/src/motion.rs). Once a frame's commit
  has settled, the next frame is asked for before the frame's 3D and page are drawn. Its commits wait in the inbox
  until the next settle, so nothing the frame reads changes. `react` in the frame log went from 1.4 to 0.4 ms.
  `RAEA_REACT_AHEAD=0` turns it off.
- **The 3D's rows are stepped the way a bake steps them** (scene_frames_to). React renders a range of frames back to
  back (`raea-frames`, now also in React's entry), and each commit is applied on its own (OneBatch). The film splits
  across four runtimes on threads. Before, each frame was one event, waited on the JS thread and back, so the
  hand-off cost more than the frame itself under load: 5.4 s to 2.9 s quiet, 14.6 s to 7.2 s at load 30.
- **The clipping check runs on its own thread** while the engine opens, and the render joins it at its end. It took
  2.5-4.5 s before the engine could open.
- **A picture shown a few frames ago keeps its texture** (engine/html/src/quads/gpu.rs). The page's image cache dropped
  anything two frames unused, so the grain, eight 1920x1080 PNGs shown in turn, made and uploaded an 8 MB texture
  every frame. Images used in the last 32 frames now stay while they fit in 72 MB, most recent first.
- **Fonts are re-checked only when a font property changes** (mirror_update_raw in sdk/react/src/lib.rs). An update
  carries the whole style object, so a caption re-sent its font-family each frame. Each frame then hashed every
  stylesheet, including the film's 1.5 MB of inlined faces.

`RAEA_STARTUP_TRACE` now marks the render's start: the rows stepped, the program made, the engine open, and the first
fed frame.

**Speed.** The whole film to .mp4, frame-logged, `RAEA_EXPORT_CACHE=0`, in two pairs back to back (the second in the
other order), at load 6-15 with other sessions' builds running:

| | before | after | before | after |
|---|---:|---:|---:|---:|
| wall time, whole render | 233.1 s | 127.2 s | 250.6 s | 119.2 s |
| printed ms a frame | 26.7 | 14.3 | 29.2 | 13.4 |
| to the first frame | 17.5 s | 11.4 s | 14.6 s | 10.1 s |

By stage (frame log, ms a frame, pair 1):

| | react | sync | draw3d | page | gpu wait | sum |
|---|---:|---:|---:|---:|---:|---:|
| before | 1.4 | 0.3 | 7.7 | 1.9 | 14.3 | 25.6 |
| after | 0.4 | 0.4 | 7.4 | 1.8 | 3.1 | 13.0 |

By line, after (pair 1): the title 17.8, open 9.6, dictator 14.3, plot 22.1, omens 14.1, decimus 11.7, street 9.5,
curia 15.3, strike 12.7, wounds 10.9, funeral 14.3, coin 13.0, close 10.4. Most of the GPU wait went away. The
filter's own 2.2 ms doesn't account for all of it, so the per-frame filter and the uploads were probably also
holding up the frames queued behind them. Under heavy load the render is still slower: at load 25-35, with another
session's full render of the film and a splat trainer on the GPU, before ran 347-394 s and after 482 s (alone),
because every stage then waits on the GPU or the scheduler.

**Same picture.** PSNR of each whole render against a render by the build before (Y, U, V averaged, ffmpeg):

| | mean | worst |
|---|---:|---:|
| before against before (two runs) | 54.8 dB | 40.3 dB (frame 576) |
| after against before, pair 1 | 53.8 dB | 40.2 dB (frame 575) |
| after against before, pair 2 | 54.6 dB | 40.2 dB (frame 575) |

The worst frames, 561-590, are the same for two runs of the old build: the dagger on the floor in the open, whose
fine stone texture the encoder renders differently each time. Differences elsewhere are noise-like (grain, encoder,
SSAO's noise, below), with no structure at 8x contrast. Stills from `--frame` at 40, 300, 1000, 1560, 2200, 2827,
3228, 3500, 4185, 4700, 5400, 6100 and 6200 are identical to the build before, or within single-level noise (PSNR
79-105 dB). The bevy fixtures environment, ssao, volumetric-half, fire, lights and model pass `check.sh` (render
equals play, SSIM 1.000000). The Hero's Journey's start (2400 frames, the live 3D path) renders the same frame hashes
as the build before on every frame, and its `--at 300,1500` stills are byte-identical.

**Not deterministic, before or after.** Bevy's SSAO picks its noise by `globals.frame_count % 64`. That is Bevy's
update count, which depends on how many warm-up and model-loading updates ran before the first frame. So two renders
of the film can differ by a noise pattern over stretches of frames (one after run differed from the other in
decimus and funeral at 44-48 dB). Seeding it from the film frame, as the fog dither is, would make renders
reproducible and make render equal play.

### Volumetric fog at half resolution (2026-10-20)

The film's fog is now `<VolumetricFog steps={32} dither resolution={0.5} />` (lever 1 of
[render-research.md](render-research.md)). `resolution` is new in the SDK (camera column `vfog_resolution`): below 1,
engine/rdb/src/engine/fog_half.rs marches the fog into a texture 1/2 (or 1/3, 1/4) the frame's size each way and puts it
over the frame. Bevy's fog module is private, so fog_half.rs is a copy of its prepare and draw (no fork of the
crate), and a camera with it carries raea's `HalfFog` instead of Bevy's `VolumetricFog`, so Bevy's own fog systems
pass it by. Three passes a frame, none with history: a depth downsample (each texel's depth from its 2x2 block, the
nearest on even texels of a checkerboard and the farthest on odd ones); the march (raea's copy of Bevy's shader with
`FOG_HALF`: each texel's ray through its block's centre, stopped at the downsampled depth, the dither on the texel
grid; blended as Bevy blends it, its alpha keeping what of the scene still shows); and a joint bilateral composite
(each pixel the bilinear mix of its four nearest texels, each weighted by how alike its depth is to the pixel's,
falling back to the texel nearest in depth of the 3x3 around it), blended onto the frame as scene x alpha + light.

**Quality.** Stills from `--frame N` (1920x1080: `--w`/`--h` don't change a still's size), SSIM of half resolution
against full resolution, both at 32 dithered steps, every source file and the binary frozen for the run. Regions as
in the 32-step table below.

| frame | scene | region | whole frame |
|---|---|---:|---:|
| 40 | title, picture | 0.99988 | 0.99991 |
| 150 | title, picture | 0.99987 | 0.99990 |
| 1560 | plot, shaft | 0.99902 | 0.99986 |
| 1900 | plot, shaft | 0.99979 | 0.99993 |
| 4185 | curia, shaft | 0.99850 | 0.99941 |
| 4235 | curia, shaft | 0.99847 | 0.99937 |

All hold 0.998, about what 32 dithered steps hold against 128. By eye at 1x, and at 3x on curia's column edges
against the shaft and the statue in it, the two can't be told apart: no halo at the column's edges (the difference at
25x contrast is a faint trace along the column and the statue's lit folds). What half resolution does lose is the
hull's own edge: a fog volume's box edge, which shows only with `ambient` above 0 (the film's is 0), steps at 2 px
(the `volumetric-half` fixture's smoke box).

**Render equals play.** The new fixture `sdk/motion/bench/bevy/volumetric-half` (volumetric-dither with
`resolution={0.5}`, at default 4x MSAA, so it covers the multisampled depth; the film covers single-sampled) passes
`check.sh` at SSIM 1.000000 on frames 0, 30 and 59, as do the other 15 fixtures. Its frame 45 against the same
frame at full resolution: SSIM 0.9917 (the smoke box's lit edge, above).

**Speed.** Whole film, frame-logged `.txt` renders (no encoder), frozen sources and binary, in the order full, half,
half, full, under load 5-14 with another session's renders running:

| | full res 1 | half 1 | half 2 | full res 2 |
|---|---:|---:|---:|---:|
| whole film, frame log, ms a frame | 24.0 | 20.8 | 22.0 | 25.5 |
| intro (title) | 33.9 | 26.1 | 25.6 | 35.6 |
| plot | 42.4 | 31.6 | 33.6 | 45.3 |
| decimus | 24.3 | 16.6 | 17.7 | 20.4 |
| curia | 29.4 | 25.7 | 27.9 | 29.5 |
| strike | 30.7 | 20.7 | 22.5 | 30.8 |
| omens | 24.0 | 21.5 | 24.1 | 24.7 |
| plain lines (street, wounds, funeral, coin) | 18.5-22.2 | 16.9-21.7 | 18.2-22.6 | 16.5-21.6 |

About 13% off the whole film (24.8 to 21.4 ms a frame, the pairs' means) and a quarter to a third off the fog lines
(title 35 to 26, plot 44 to 33, strike 31 to 22); curia gains least (29.5 to 27), so its frame is mostly something
other than the march. The second full run's open and dictator (30 and 31 against 17 and 22) were slowed by the other
session's render, so they are left out of the comparison. The film had changed since the 32-step measurements
below, so its totals aren't comparable with theirs.

### Volumetric fog at 32 dithered steps (2026-10-20)

The film's `<VolumetricFog>` went from `steps={128}` to `steps={32} dither` (lever 2 of
[render-research.md](render-research.md)). `dither` is new in the SDK: rdb registers its own copy of Bevy's fog shader
(engine/rdb/src/engine/volumetric_fog.wgsl, over the embedded one) in which each pixel's march starts a fraction of a step in,
by interleaved gradient noise seeded from the pixel and the film frame (Bevy's `jitter` carries 1 + the frame mod 64,
set each frame from the engine's clock by `scene3d::sync_fog_dither`). Bevy's own jitter is seeded from its update
count, which a frame rendered alone and the same frame in play don't share. Without `dither` the copy draws what
Bevy's did: a still at 128 steps is byte-identical to the build before (curia 4185; plot 1560 at SSIM 1.000000).

Everything below was measured on the film as of 49ad895, before 08baf1d put every scene at night and relit the door
and the street by moonlight.

**Quality.** Stills from `--frame N` (1920x1080), SSIM against 128 steps, every source file frozen for the run.
"Shaft" is the shaft's own region (plot 640x420 at 540,132; curia 560x620 at 440,150); the title's smoke fills the
picture, which is measured inside the letterbox.

| frame | scene | 32 | 32 dither | 48 | 48 dither | 64 | 64 dither |
|---|---|---:|---:|---:|---:|---:|---:|
| 40 | title, picture | 0.99940 | 0.99954 | 0.99961 | 0.99965 | 0.99975 | 0.99970 |
| 150 | title, picture | 0.99929 | 0.99947 | 0.99955 | 0.99959 | 0.99971 | 0.99965 |
| 1560 | plot, shaft | 0.99823 | 0.99904 | 0.99887 | 0.99953 | 0.99921 | 0.99969 |
| 1900 | plot, shaft | 0.99718 | 0.99821 | 0.99765 | 0.99921 | 0.99818 | 0.99993 |
| 4185 | curia, shaft | 0.99785 | 0.99827 | 0.99830 | 0.99916 | 0.99884 | 0.99948 |
| 4235 | curia, shaft | 0.99782 | 0.99818 | 0.99827 | 0.99912 | 0.99880 | 0.99948 |
| 2827 | door, picture | 0.999998 | 0.999998 | 0.999999 | 0.999999 | 0.999998 | 0.999998 |
| 4695, 4728 | stabs, picture | 0.999999 | | 0.999999 | | 0.999999 | |

Every step count holds 0.995. Dither raises the shafts' SSIM at each count (32 dithered is about where 48 plain is),
since what fewer steps lose is banding: at 12x contrast the difference from 128 is contour rings of one or two levels
along the shafts and a shadow edge moved by a pixel. At 1x, plot and curia at 32 dithered can't be told from 128 by
eye. door's fog (density 0.01) and the stabs' blood fog barely show in a still, so they move nothing. The open
(frame 300, 10 s) has no FogVolume.

**Render equals play.** A dithered fixture, `sdk/motion/bench/bevy/volumetric-dither` (the volumetric fixture at 32 steps
with `dither`), passes `check.sh` at SSIM 1.000000 on frames 0, 30 and 59 render against baked play, as do the other
14; its dither visibly changes the picture (SSIM 0.9948 against the same frame undithered, 0.9876 against the next
frame). Per-frame hashes can't settle it for the film: two renders of the same 30-frame segment hash differently on
every frame, dithered or not, and an H.264 frame of a full render against the `--frame` still sits at SSIM 0.98
(codec), a hair closer to the dithered still than to the plain one on all six frames.

**Speed.** Whole film, two pairs under load 8-16 (frame-logged; `.txt` hashes skip the encoder, `.mp4` encodes):

| | 128 steps | 32 dither |
|---|---:|---:|
| pair 1 (`.txt`), printed ms a frame | 31.1 | 19.8 (-36%) |
| pair 2 (`.mp4`), printed ms a frame | 34.0 | 22.9 (-33%) |

By line, frame log (pair 1, then pair 2), ms a frame:

| line | 128 | 32 dither | 128 | 32 dither |
|---|---:|---:|---:|---:|
| intro (title) | 56.7 | 26.0 | 63.7 | 30.5 |
| plot | 70.2 | 33.6 | 84.1 | 39.3 |
| omens | 27.4 | 17.3 | 34.7 | 24.4 |
| decimus | 33.1 | 14.0 | 38.0 | 19.6 |
| curia | 37.5 | 20.8 | 43.9 | 27.0 |
| strike | 62.6 | 22.3 | 73.5 | 29.2 |
| close | 23.3 | 13.0 | 25.0 | 22.8 |
| plain lines (street, wounds, funeral) | 11-15 | 10-15 | 15-19 | 15-18 |

A third dithered run measured 30.5 while another session's GPU work doubled even the plain lines (open 37.8 against
10-14), so it is left out. Interleaved 60-frame segments (minimum of 3) agree on title (44.5 to 25.6) and plot (40.9
to 23.9); door (27.3 to 22.6), curia and stabs were inside the noise at that length. Dither itself costs nothing
measurable (32 plain and 32 dithered: 25.6 and 26.0 on curia).

### Dark lights draw no shadow maps (2026-10-20)

Every scene's lights are in the one Canvas3D for the whole film; a light outside its scene is dark (power 0) but was
still extracted with its shadow maps on, so each frame rendered all twenty shadow-casting lights' maps at 4096
texels. `sync_lights` (engine/rdb/src/engine/scene3d.rs) now turns a light's shadow maps off while its power is 0 and back
on when it's lit (the light's own setting is kept in `CastsShadows`).

Two pairs, the second in the other order (the old build is the same code with the dark lights' maps left on):

| | pair 1 before | after | pair 2 before | after |
|---|---:|---:|---:|---:|
| whole film, printed ms a frame | 47.5 | 31.2 (-34%) | 49.8 | 39.5 (-21%) |
| whole film, frame log: GPU wait | 34.2 | 20.6 | 36.8 | 27.3 |
| whole film, frame log: draw3d | 8.9 | 6.2 | 8.7 | 7.0 |
| stabs (frames 4666-4755), frame log | 105.5 | 75.2 (-29%) | 104.9 | 78.9 (-25%) |

Load was 4-5 for pair 1; in pair 2 another session's render took it to 20 during the after run, so pair 2 understates
the gain. The whole film at 31 ms a frame renders in about 4.2 minutes, a little under its own length.

Same picture: stills from plot (1560, 1599), curia (4185, 4235) and stabs (4695, 4728) against the build before
are byte-identical except plot's, at SSIM 1.000000 (PSNR 96-99 dB: single-level differences); every
`sdk/motion/bench/bevy` fixture's frames 0, middle and last are byte-identical, and `check.sh` passes all 13 at SSIM 1.0.

The first try hid dark lights instead (`Visibility::Hidden`, so Bevy wouldn't extract them). Don't: it put out
curia's and plot's volumetric shafts (SSIM 0.947 on curia). Bevy 0.19's `prepare_lights` gives a spot light its
volumetric and shadow flags by its index in the spot block less a count of point lights, and lights coming and going
from extraction threw that arithmetic off. Keep every light extracted; change only what it draws.

### The master's resampling on threads (2026-10-20)

`media::soft_resample` (the 4x oversampling the master's true-peak limiter reads, up to five passes over the whole
4.5 minutes) ran on one thread: 26.7 s a render after the last frame. It now splits each channel's samples across up
to 8 threads and skips the bounds test for samples whose taps all land inside the input. Each sample sums the same
taps in the same order, so the output is the same bits (`resample_tests::threaded_resample_is_the_same_bits`).
One pass over 4.5 minutes of stereo, 48 kHz to 192 kHz: 3.5 s before, 0.25 s after (14x, under load 20), so five
passes take about 1.3 s where they took about 18 s.

## What's left

- Volumetric scenes: plot (22 ms a frame after the changes above), curia (15) and the title (18) are still the heaviest
  lines. Curia gained least from half resolution, so its remaining cost is worth attributing with
  `RAEA_FRAME_LOG_SERIAL=1`; smaller FogVolumes around the shafts would cut the march further.
- Shadow map size: 4096 texels for every lit shadow-casting light. A scene with one key light could keep 4096 for it
  and give fill lights less.
