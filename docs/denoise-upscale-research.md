# Denoisers and upscalers: what raea should standardise on (research, 2026-11-24)

**Decided 2026-11-24 (Shane):** standardise on what raea can own: its own WGSL denoiser and upscaler on every OS and GPU.
MetalFX and DLSS-RR stay optional backends. The build is docs/denoise.feature (DN0-DN11).

Asked during MALL RAT (Raea/anchor-tenant). That film runs Solari with MetalFX's denoised scaler and is clean at full size;
what's left there is in "What MALL RAT still shows". This note looks wider: every GPU and OS raea runs on, real-time play
and offline finals, and one contract every backend reads. Bevy 0.20, wgpu 30. Sources at the end; nothing here is
measured by me unless it says so.

## The landscape

### Denoisers for a traced frame (temporal, per frame)

| Denoiser | Kind | macOS | Windows | Linux | GPUs | Licence | For raea |
|---|---|---|---|---|---|---|---|
| MetalFX denoised scaler (`MTLFXTemporalDenoisedScaler`) | neural, denoise + upscale in one | yes (macOS 26) | - | - | Apple silicon | Apple SDK | **shipped** (RT4, metalfx.rs) |
| DLSS Ray Reconstruction 4.5 | neural, denoise + upscale in one | - | yes | yes | NVIDIA RTX, driver 580+ | NVIDIA RTX SDK licence (object-code redistribution inside an app) | Bevy's own `dlss` feature (`dlss_wgpu`, Vulkan); Bevy 0.20 already updated it to RR 4.5 for Solari. Not built in raea yet |
| AMD FSR Ray Regeneration 1.2.1 | neural, denoise only | - | yes (Windows 11) | - | Radeon RX 9000 only | signed DX12 DLL, binary-only, no reverse engineering | **no**: DX12-only, Solari traces through wgpu's Vulkan/Metal |
| NVIDIA NRD 4.18 (ReBLUR, ReLAX, SIGMA) | hand-tuned spatio-temporal | via NRI's new Metal 4 backend | yes | yes | any D3D12/Vulkan GPU | NVIDIA RTX SDK licence: object code in an app, no port of its source | possible later; needs diffuse/specular split and hit distances out of Solari, and a native (non-wgpu) integration through NRI |
| SVGF / A-SVGF / ReBLUR-style, written from the papers | hand-tuned temporal | yes | yes | yes | anything wgpu runs | ours | **the portable tier**: WGSL, every OS |
| pmndrs/denoiser `denoiser/realtime` | clean-room WGSL temporal (diffuse, specular, shadow, AO) | yes | yes | yes | WebGPU; validated on Apple GPUs only | MIT | a ready start for the portable tier, or its reference |

### Single-image denoisers (stills, holds, bakes)

| Denoiser | macOS | Windows | Linux | Licence | For raea |
|---|---|---|---|---|---|
| Intel Open Image Denoise 2.x | CPU + Metal (Apple silicon) | CPU + CUDA/HIP/SYCL | CPU + CUDA/HIP/SYCL | Apache-2.0 (code and weights) | **stills, held frames, light bakes** (raea light bake, BK). Single-frame, so it flickers on moving video; Intel's temporal OIDN 3 has slipped to the second half of 2026 |
| OIDN weights in WGSL/ONNX (pmndrs/denoiser) | yes | yes | yes | MIT + Apache-2.0 weights | the same networks without Intel's runtime; ONNX fits raea's ort port rule |
| NVIDIA OptiX denoiser | - | yes | yes | proprietary | no: NVIDIA-only, and OIDN covers it everywhere |

### Upscalers (temporal, from a smaller frame)

| Upscaler | macOS | Windows | Linux | GPUs | Licence | For raea |
|---|---|---|---|---|---|---|
| MetalFX temporal scaler | yes | - | - | Apple | Apple SDK | shipped (upscale.rs; `scale` on SolariLighting) |
| DLSS Super Resolution | - | yes | yes | NVIDIA RTX | NVIDIA RTX SDK | with RR, the same Bevy feature |
| AMD FSR Upscaling 4.1.2 (ML) | - | yes | yes (Vulkan now at parity, SDK 2.4) | RDNA 2/3/4 + Ryzen AI | binary SDK | optional AMD tier |
| AMD FSR 3.1.6 (analytic) | portable source | yes | yes | any | FidelityFX SDK source (pmndrs ships an FSR3 port too) | **the portable upscaler** |
| Intel XeSS 3 SR | - | yes | yes (Vulkan) | Intel best, cross-vendor SM 6.4 | Intel SDK (licence not checked) | optional; FSR 3.1 covers the same ground openly |
| Bevy TAA / SMAA / CAS | yes | yes | yes | any | MIT/Apache | anti-aliasing at full size, not upscaling |

## What the research says, in one line each

- Every vendor has converged on **one neural pass that denoises and upscales together** (MetalFX, DLSS-RR); AMD's is
  the exception (separate denoise, then FSR) and the only one locked to DX12.
- All of them read **the same guides**: noisy colour, diffuse albedo, specular albedo (with Fresnel), world-space
  signed normals, linear roughness, depth, motion; optional specular hit distance, a strength or reactive mask, and a
  transparency overlay. raea already writes most of these (metalfx_guides.wgsl, Solari's PSR guides).
- Quality comes mostly from **the input**: Apple, NVIDIA and AMD all say the same: cut noise at the source (NEE,
  importance sampling, ReSTIR), avoid correlated random numbers, keep the guides noise-free, put the reflected
  surface's albedo/normal/roughness in a mirror's pixel (primary surface replacement), blend glass's guides by Fresnel,
  and get motion vectors right. NVIDIA warns that ReSTIR's reuse breaks the independence a denoiser assumes; Bevy
  Solari already permutes its temporal reuse (restir.wesl `permute_pixel`).
- **No neural temporal denoiser is open and cross-vendor.** OIDN is the only open neural one, and it is single-frame
  until OIDN 3. So every engine that runs everywhere carries a hand-tuned temporal fallback (AdaEngine's design does
  exactly this: RR where it can, SVGF in WGSL everywhere else).

## Proposed standard for raea

1. **One guide contract, backend-neutral** (`DenoiseInputs`): colour, diffuse albedo, specular albedo, normal (signed,
   world), roughness, depth (Solari's PSR depth where a mirror replaces), motion, emission split, strength mask; plus
   two raea doesn't write yet:
   - **specular hit distance** (MetalFX, DLSS-RR and NRD all take it; it keeps glossy reflections sharp and stable:
     the waxed mall floor);
   - **diffuse and specular radiance kept apart** (demodulated), which the portable tier and NRD need. Solari gives one
     colour today, so this is the main engine change.
   One debug view per input (Apple's first advice), so a bad guide is seen before the denoiser is blamed.
2. **One switch with a ladder**: `<SolariLighting denoiser="auto" | "metalfx" | "dlss" | "portable" | "off">`, `auto`
   picking, in order: MetalFX (Apple silicon, macOS 26), DLSS-RR (NVIDIA RTX, Windows/Linux), portable (everything
   else). A backend that can't run falls to the next and says so once, with the reason, in the render's trace (the
   way MetalFX's absence is reported now).
3. **Upscaling follows the denoiser**: the joint ones (MetalFX, DLSS-RR) upscale themselves through `scale`; the
   portable tier upscales with FSR 3.1 (open, any GPU); DLSS SR, MetalFX's temporal scaler and FSR 4.1 serve raster
   views on their own hardware.
4. **The portable tier is ours, in WGSL**, written from SVGF/A-SVGF and the ReBLUR chapter (not NRD's source), with
   pmndrs/denoiser's MIT `denoiser/realtime` as the starting point or the reference to beat.
5. **Stills and bakes use OIDN** (Apache-2.0, every OS, GPU where there is one): posters, thumbnails, a held frame
   rendered converged, and raea light bake's lightmaps. When OIDN 3's temporal model ships, it becomes the candidate
   to replace the portable tier.
6. **A film's finals have their own profile**, because a render isn't real time: more samples, the shutter (raea's
   `<MotionBlur>`, already checked against 32 sub-frames in raytracing.feature) and, where the time is there, several
   sub-frames averaged per frame. Motion blur both makes a camera real and hides the frames a denoiser has no history
   for (doorways, whip pans).

## Shipping DLSS: what NVIDIA's licence asks (read 2026-11-24, NVIDIA/DLSS LICENSE.txt, v. March 14, 2024 + RTX supplement)

raea is FSL-1.1-ALv2. NVIDIA's SDK can't be made subject to an open-source licence (4e), so it stays out of raea's
source: the build fetches the SDK from NVIDIA's own GitHub, pinned by hash, behind the `dlss` feature, and nothing of it
is committed. Whether it then ships linked into the binary or as an optional piece the app downloads on NVIDIA machines,
raea is distributing it, which the licence allows (object code inside an application with material functionality of
its own, 1c and 2a). Downloading on demand keeps it off Macs and AMD machines; it doesn't change the obligations, which
attach to any application that "incorporates, or is based on" the SDK:
- **notify NVIDIA before commercial release** (supplement 4: developer.nvidia.com/sw-notification);
- **NVIDIA's marks** on the splash screen and in the About box (supplement 7.1b), and no implied endorsement (4c);
- **terms at least as protective** as NVIDIA's passed on to users for that component (2c): a section in raea's EULA;
- **NVIDIA GPUs only** (supplement 1), which the backend ladder already is;
- **cloud: only on hosts running NVIDIA vGPU software**; any other cloud service use is outside the licence
  (supplement 3). raea cloud's rented GPUs (Modal, sprites) would need vGPU hosts or NVIDIA's written agreement;
- NVIDIA may update it over the air and may disable a broken integration as a last resort (supplement 6).
Questions about terms go to nvidia-rtx-license-questions@nvidia.com (19). This is a reading of the text, not legal advice.

## The check every backend has to pass (parity, as every model on every OS does)

A denoise bench beside lightbench, same scenes on every backend, against a converged reference (`<Pathtracer>` held
long, or Cycles through tools/cycles-port):
- **error**: FLIP and RMSE against the reference, per frame;
- **bias**: mean light against the reference (catches MetalFX's constant floor, which camcorder gain made a glow in
  MALL RAT);
- **flicker**: the difference between a frame and the last one warped by motion, on still surfaces;
- **blotches**: low-frequency residual energy (tools/noise.sh in anchor-tenant measures this today);
- **lag**: frames to settle after a disocclusion and after a light changes (the 6-frame emitter trail found under
  MetalFX);
- **cost**: ms a frame at 1080p.
Scenes: a dark room under high gain, a glossy floor, walking through a doorway, a whip pan, neon and emissive signs,
foliage, fog, a mirror, glass. The mall's takes already hold most of them.

## What MALL RAT still shows (2026-11-24)

- Finals at full size: no blotches; the grain left is the camcorder's own gain, by design.
- No shutter: every frame is sharp, which reads as CG and leaves disocclusions (doorways, t8's whip pan) visible for
  a few frames. `<MotionBlur>` is the cheap fix.
- Glossy floors without a hit distance guide; storefront glass left out because blended materials draw after the
  denoiser (NOTES.md). Glass needs Solari to trace it and the denoiser to get Fresnel-blended guides; the largest
  piece of work here.

## Order of work (proposed)

1. `<MotionBlur>` in camcorder films' finals; then sub-frame averaging as a finals option.
2. The guide contract written down as code (one struct, one debug view per input); add specular hit distance.
3. The denoise bench, run on MetalFX first, so every later backend has numbers to meet.
4. DLSS-RR through Bevy's `dlss` feature on the person's own NVIDIA machine (Windows, Linux), checked against MetalFX on
   the bench. Not on raea cloud: see "Shipping DLSS" (the licence leaves cloud use out unless the host runs NVIDIA vGPU).
5. The portable WGSL tier, from the papers / pmndrs, on the bench against both.
6. OIDN for stills and light bake.
7. Glass: Solari traces it, Fresnel-blended guides.
Each becomes scenarios in docs/raytracing.feature when it's taken on.

## Sources

- Apple WWDC25 "Go further with Metal 4 games" (denoised scaler inputs and pitfalls); WWDC26 359 "Build real-time neural
  rendering pipelines with Metal" (inputs, strength mask, transparency overlay, PSR, motion vectors);
  MTLFXTemporalDenoisedScalerDescriptor docs (specular hit distance, reactive mask).
- NVIDIA DLSS-RR Integration Guide (2026-08; Windows and Linux, driver 580+, ReSTIR permutation advice); NVIDIA-RTX/NRD
  README v4.18 and LICENSE; NVIDIA-RTX/NRI README (Metal 4 and WebGPU backends).
- AMD GPUOpen: FSR SDK 2.4 (2026-10-28), FSR Ray Regeneration 1.2.1 manual (RX 9000, DX12, Windows 11).
- Intel: Open Image Denoise site (v2.2 Metal device), XeSS 3 developer page.
- Bevy 0.17 and 0.20 release notes (Solari on macOS without a denoiser; dlss_wgpu at RR 4.5); JMS55's Solari write-up.
- pmndrs/denoiser docs (realtime denoisers, temporal spec, nppd-fssa research note: licences and OIDN 3's slip).
- AdaEngine design "path-tracing-denoising" (RR where available, portable SVGF elsewhere).
- raea: docs/raytracing.feature (RT4, motion blur scenario), docs/render-thirdparty.md, engine/render/raytrace/src/metalfx.rs.
