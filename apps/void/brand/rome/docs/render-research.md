# Render research: what could make rome's frames cheaper

This is research for the rome film (`film.tsx`, rendered at 960x540, 30 fps). It covers compression, upscaling,
caching and neural techniques for render speed, with measurements taken on 2026-10-19 and 10-20. Every
number here is from the M5 Pro under the machine's ordinary load (load 8-26, other sessions rendering), so they're
paired or GPU-side figures, not quiet-machine ones. The stage-by-stage cost of the film is in
[throughput.md](throughput.md). This page is about levers.

## The finding: the volumetric fog pass is the biggest bar in the heavy scenes

**door, traced.** A 60-frame segment of the door scene (frames 2827-2886) was traced with Xcode's Metal System Trace.
Its GPU time was joined to Bevy's encoder labels. The segment takes 27.9 ms of GPU a frame. The passes that matter:

| pass | GPU ms a frame |
|---|---:|
| volumetric lighting pass (Bevy's VolumetricFog) | 18.1 (65%) |
| quads (the page) | 2.5 |
| bloom, all passes | ~1.4 |
| main opaque pass | 1.2 |
| blur filter | 0.9 |
| depth of field | 0.8 |
| each shadow cascade | ~0.25 |

**Toggles.** Wall ms a frame, the minimum of 3 interleaved runs over 60 frames. The medians are too noisy to use
under this load.

| scene | base | no VolumetricFog | 32 fog steps (128 now) |
|---|---:|---:|---:|
| door (2827) | 44.9 | 10.0 | 11.8 |
| title (40) | 38.6 | 27.5 | 21.7 |

Dropping bloom, DOF, the grade or shadows changed nothing beyond the noise. The scenes that throughput.md finds
heaviest after the off-screen-shadows fix are intro (60.8), plot (75.4), decimus (35.3) and curia (39.4). Each has a
`volumetric` light in a FogVolume.

**Why the fog costs so much.** Bevy 0.19.1's shader is `bevy_pbr/src/volumetric_fog/volumetric_fog.wgsl`. For each
FogVolume it draws a full-resolution pass straight into the view target. Every pixel the volume covers marches
`step_count` steps once for each volumetric directional light, with a cascade lookup and a shadow-map sample at every
step. It then marches `step_count` more steps over the clustered point and spot lights. That second march does a
cluster lookup per step even when no point or spot light is volumetric, and it is the march that sets the fog's
alpha. door's FogVolume (22x10x24 m) contains the camera, so all 518,400 pixels march 2 x 128 steps.

**Render equal to play holds today.** The fog's jitter uses `globals.frame_count`, the engine's own count of frames,
not the film's frame number. raea sets `jitter: 0.0` (engine/rdb/src/engine/scene3d.rs), so fog is a function of the frame.
Any jitter lever below has to seed its noise from the pixel and the film frame, never from `frame_count`.

## Ranked shortlist

The speed-up is for this workload. "Equal" means render equal to play.

1. **Fog at half resolution with a depth-aware upsample.** *Speed:* door's 18 ms becomes about 4.5 ms plus about
   0.3 ms of upsample (estimated from the pixel count). That is about 13 ms a frame off every fog scene, roughly a
   third of the heavy lines. *Quality risk:* low to medium. Fog is low-frequency; thin occluders in light shafts can
   bleed at depth edges, which a bilateral (depth-guided) upsample limits. *Equal:* yes, it is a pure function of the
   frame. *Cost:* the shader can't do this alone. The fog node renders into `view_target.main_texture_view()`, so it
   needs a copy of `bevy_pbr`'s volumetric_fog module in rdb (MIT/Apache) that draws into a half-res target, plus a
   composite pass. Raea's FogVolume rows stay as they are. Two to three days.
2. **Fewer fog steps with deterministic dither.** *Speed:* 128 to 32 steps took door's minimum from 44.9 to 11.8 ms,
   about what removing the fog gives. *Quality risk:* banding in the shafts, unless each pixel's start is offset by
   blue noise (interleaved gradient noise seeded from the pixel and the film frame), which trades banding for fine
   grain. The film already lays grain over the picture at 7%. *Equal:* yes, if the seed is the film frame (or the
   pixel alone). *Cost:* shader-only, with no Bevy fork. The shader is an embedded asset
   (`embedded://bevy_pbr/volumetric_fog/volumetric_fog.wgsl`), and the asset server loads it lazily when the
   pipeline is made, so rdb can register a patched copy after `PbrPlugin` builds. Also in reach this way: skip the
   point/spot cluster loop when no point or spot light is volumetric, keeping only the alpha march. Half a day,
   including SSIM checks of 32, 48 and 64 steps against 128 on the fog scenes. **The next thing to build**, and the
   cheapest; it combines with item 1.
3. **Off-screen lights' shadow maps.** Found and fixed by rome-throughput in `sync_lights`: lights from scenes that
   aren't on screen stayed in the world at power 0 with shadows on (stabs waited 134 ms a frame in the full film,
   against 0.08 ms alone). The film is now 30.2 ms a frame (cut 15, d7645ec).
4. **MetalFX spatial upscaling of the 3D layer only** (`MTLFXSpatialScaler`, macOS 13+; FSR 1-class).
   *Speed:* render the 3D at 0.67-0.75 scale, about 1.8-2.2x fewer pixels in every per-pixel pass, fog included.
   The scaler itself should be well under 1 ms at 540p (an estimate; no Mac timings were found). The page (type)
   stays at full resolution. *Quality risk:* medium; it softens texture detail, and Digital Foundry rated the
   spatial mode poor at 2x per axis, so keep the scale at about 0.75. *Equal:* yes, one frame in and one frame out.
   *Cost:* `objc2-metal-fx` 0.3.2 binds it (Zlib/Apache/MIT). wgpu 29's `as_hal` gives the raw `MTLTexture`. A
   plugin exists, `bevy_metalfx` 0.4.2 (MIT/Apache, for Bevy 0.18 on wgpu 29, one author, about 170 downloads), so
   porting it to 0.19.1 is small, but read it first. Check that bloom, DOF and the fog all honour the lower resolution
   (Bevy 0.17's notes warn that some effects don't). Two days.
5. **Cached 3D plates for static spans.** Lightspeed's noir documentary renders 1920x1080 at 7.7-8.5 ms a frame from
   pre-rendered 3D plates played as video. The equivalent for rome is to render each scene's 3D once (render cache
   keyed by the scene's rows) and composite the page over it on re-renders. This pays when the film is re-cut
   (type, captions, timing) more than when it is re-lit, and needs no change to Bevy. *Equal:* yes, if the plate is
   bit-identical to the live 3D (VideoToolbox's 8-bit is not, so the plate would be 10-bit HEVC or ProRes).
6. **Compressed textures (KTX2 BC7 or ASTC 4x4).** Bevy 0.19 loads both on Metal, and its `CompressedImageSaver`
   (feature `compressed_image_saver`) writes BCn, or ASTC with `BEVY_COMPRESSED_IMAGE_SAVER_ASTC`. *Speed:* small at
   540p, since Apple GPUs already compress read-only textures losslessly. The win is memory (about 4x on the 2k PBR
   sets; a full render peaked at 15.6 GB under memguard's 24 GB) and load time. *Quality risk:* low (normal maps need
   BC5). *Equal:* yes.
7. **Baked lighting for static sets** (Bevy's `Lightmap`, `IrradianceVolume`; bake in Blender, export with
   `bevy-baked-gi`). This saves lighting on lights that don't make shafts. Volumetric lights must keep their shadow
   maps, so it doesn't touch the fog. Asset-pipeline work.
8. **Frame interpolation (RIFE on the Neural Engine, or MPSGraph via `rife-metal`; MIT/Apache).** Render even frames
   and synthesise odd ones. This is offline only, since it needs frame N+1, and it can't play live. Frame N stays a
   function of N if rendering N alone renders both neighbours. *Quality risk:* high for this film; shafts in fog, DOF,
   thin geometry and disocclusion smear. Not recommended.

**Rejected.**
- Anything that blends earlier frames breaks render equal to play: Bevy's TAA, `MTLFXTemporalScaler`, MetalFX's
  temporal denoiser and its frame interpolator. Warming up k frames before each frame costs k times as much.
- DLSS is NVIDIA and Vulkan only. FSR isn't in Bevy.
- Gaussian splats: they don't take Bevy's lights, shadows or fog, and the sets would have to be captured.
- NVIDIA neural texture compression has no Metal path.
- Encode is already VideoToolbox. Lightspeed's G6 measured zero-copy at about 0.13 ms of a frame (the copies), so
  IOSurface can't move rome. At 960x540 the encoder's floor is about 1 ms a frame.

**A note for later.** raea-32's engine agent skipped motion blur because render must equal play. A deterministic
version is sub-frame accumulation: render k sub-frames at t + i/(k*fps) and average them, at k times the 3D cost. The
fog levers above would pay for it.

## How it was measured (to repeat it)

- **Segments.** A copy of film.tsx whose root is
  `<Sequence from={-START} layout="none"><Film /></Sequence>` with `durationInFrames=N`, rendered with
  `RAEA_EXPORT_CACHE=0`. Without that, a repeat comes from the export cache in 0.4 s. Variants removed or edited one
  element (`<VolumetricFog steps>`, `<DepthOfField>`, `<Bloom>`, `<Grade>`, `castShadow` in a shots copy), and each
  repetition ran them in shuffled order.
- **GPU per pass.** `xcrun xctrace record --template 'Metal System Trace' --launch -- raea-motion render SEG.tsx ...`,
  then export the `metal-gpu-intervals` and `metal-application-encoders-list` tables and join them on `encoder-id`.
  Sum each encoder label's durations and group by pid: the trace records every process on the GPU, other sessions'
  renders included. bench/render-smoke/trace.py has the XML reader. A 60-frame trace is about 210 MB; delete it after
  reading.
- **Fog and `--at`.** Lightspeed reports that `render --at a,b,c` with a FogVolume can lose the Metal device on the
  second frame. Use segments or `--frame N`.

## Next steps

1. Build item 2 behind `RAEA_FOG_STEPS` / a `<VolumetricFog steps dither>` prop: the patched embedded shader, with
   pixel- and film-frame-seeded dither and the skipped empty cluster loop. Check SSIM against 128 steps on stills from
   intro, plot, decimus and curia, and check render equal to play with `--frame N` against a full render's frame N.
2. Then item 1, half-res fog, as rdb's own copy of the fog node. Measure it the same way: the GPU pass time from a
   Metal trace, then SSIM.
3. Item 4, MetalFX spatial upscaling, if the 3D's other passes become the bar once fog is cheap.

Sources:
- Bevy's volumetric fog: https://github.com/bevyengine/bevy/blob/main/crates/bevy_pbr/src/volumetric_fog/volumetric_fog.wesl
- Wronski, volumetric fog (SIGGRAPH 2014): https://bartwronski.com/wp-content/uploads/2014/08/bwronski_volumetric_fog_siggraph2014.pdf
- Hillaire, Frostbite's volumetric rendering (2015): https://www.slideshare.net/slideshow/physically-based-and-unified-volumetric-rendering-in-frostbite/51840934
- bevy_metalfx: https://github.com/mistakeknot/bevy_metalfx
- objc2-metal-fx: https://docs.rs/objc2-metal-fx
- wgpu's MetalFX discussion: https://github.com/gfx-rs/wgpu/discussions/9320
- Bevy 0.17's release notes (DLSS, upscaling): https://bevy.org/news/bevy-0-17
- WWDC25 MetalFX session: https://developer.apple.com/videos/play/wwdc2025/211
- Bevy's compressed image saver: https://github.com/bevyengine/bevy/blob/main/crates/bevy_image/src/compressed_image_saver/mod.rs
- Apple, optimising texture data: https://developer.apple.com/documentation/metal/optimizing-texture-data
- Apple, tile-based deferred rendering: https://developer.apple.com/documentation/metal/tailor-your-apps-for-apple-gpus-and-tile-based-deferred-rendering
- Bevy's mixed-lighting example: https://bevy.org/examples/3d-rendering/mixed-lighting/
- bevy-baked-gi: https://github.com/pcwalton/bevy-baked-gi
- Practical-RIFE: https://github.com/hzwer/Practical-RIFE
- rife-metal: https://github.com/cinemore/rife-metal
- FILM: https://github.com/google-research/frame-interpolation
