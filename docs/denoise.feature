@spec
Feature: raea Denoise: one denoiser and upscaler raea owns, on every OS and GPU
  Shane, 2026-11-24: "go with whatever we can standardize even if theres an open source version we build ourselves".
  The research is docs/denoise-upscale-research.md. No vendor denoiser runs everywhere: MetalFX is Apple's, DLSS-RR is
  NVIDIA's (not allowed in a cloud service off NVIDIA vGPU), FSR Ray Regeneration is DX12 on RDNA 4 only, and NRD's
  source can't be ported. So the standard is raea's own: a temporal denoiser and upscaler in WGSL, run through wgpu on
  Metal, Vulkan and DX12, on the Mac, Windows, Linux and raea cloud (GPU and lavapipe) alike, under raea's own licence.
  MetalFX and DLSS-RR stay as optional backends, chosen by name, and held to the same bench.

  Written from the published work, never from NRD's code: SVGF (Schied et al. 2017), A-SVGF (Schied et al. 2018),
  ReBLUR's recurrent blur (Zhdan, Ray Tracing Gems II, 2021) for the hit-distance and specular ideas, and FSR 2/3's
  MIT source for the temporal upscaler. Stills use OIDN's networks (Apache-2.0 weights) on ort.

  Every number this feature sets comes from DN0's bench, measured; none is guessed here.

  One denoiser for both renderers. docs/realism.feature plans raealism (LT13 retires Solari) with "a denoiser slot"
  on Windows and Linux (LT12), OIDN on the film tier (LT19), and docs/bake.feature cleans lightmaps (BK4). This feature
  is that slot and that OIDN: DN1's inputs are written by whichever renderer draws the view (Solari today, raealism
  later), DN10's OIDN is the one LT19 and BK4 call, and the temporal parts reuse raea's steady filter
  (raea-sys-post steady.rs: its jitter, its YCoCg clip) rather than a second copy.

  Background:
    Given Solari's real-time lighting from raea's vendored bevy_solari (vendor/bevy_solari), wgpu 30, Bevy 0.20
    And raea-render-raytrace, at about 1,900 of its 2,000 lines, so the denoiser is a crate of its own

  Rule: Measured before it's built

    Scenario: DN0. A denoise bench that every backend passes, against a converged truth
      Given the bench scenes: a dark room under 18 dB of camcorder gain, a waxed floor's reflections, walking through
        a doorway, a whip pan, neon and emissive signs, foliage, fog, a mirror, a light switched off (MALL RAT's takes
        hold most of them; small cases of each under bench/denoise/cases)
      And each scene's truth: Solari's `<Pathtracer>` held still per frame until it converges, or Cycles through
        tools/cycles-port where the path tracer differs
      When `raea bench denoise [--backend raea|metalfx|dlss|off] [CASE...]` renders every case
      Then it reports, per case and backend: FLIP and RMSE against the truth; bias (mean light against the truth);
        flicker (the error's change frame to frame against the truth's own shot, so motion cancels); a whole-frame
        spike (one frame's mean light lifted or dropped alone); blotches (residual energy
        below 1/16 of the frame's width); lag (frames to settle after a disocclusion and after a light goes out);
        and ms a frame at 1080p on the GPU it ran on
      And MetalFX's numbers on the Mac are the first bar: DN8 makes raea's the default when it meets them
      And the plan:
        | crate                 | file                          | lines | job                                          |
        | raea-denoise-bench    | engine/render/denoise/bench/src/lib.rs     | 250 | the cases, the truth, the runs              |
        | raea-denoise-bench    | engine/render/denoise/bench/src/metrics.rs | 380 | FLIP, RMSE, bias, flicker, spike, blotch, lag |
        | raea-denoise-bench    | engine/render/denoise/bench/src/report.rs  | 150 | the table and its JSON                      |

  Rule: One set of inputs

    @needs:DN0
    Scenario: DN1. One guide contract every backend reads, each input visible on its own
      Given a view under SolariLighting
      Then one pass writes DenoiseInputs: diffuse albedo, specular albedo (with Fresnel), world-space signed normal,
        linear roughness, depth (the reflection's under primary surface replacement), motion (a mirror's own), the
        emission split, a strength mask, a mesh id for disocclusion; the same textures MetalFX reads today
        (metalfx_guides.wgsl moves here, unchanged in what it writes)
      And `RAEA_DENOISE_VIEW=albedo|specular|normal|roughness|depth|motion|hitdist|diffuse|specular-light|history`
        draws that input instead of the frame, so a bad input is seen before the denoiser is blamed
      And the plan:
        | crate        | file                                   | lines | job                                    |
        | raea-denoise | engine/render/denoise/src/lib.rs       | 150   | the plugin, the switch, the order      |
        | raea-denoise | engine/render/denoise/src/inputs.rs    | 250   | DenoiseInputs: textures, their sizes   |
        | raea-denoise | engine/render/denoise/src/inputs.wgsl  | 200   | the guide pass                         |
        | raea-denoise | engine/render/denoise/src/view.rs      | 120   | RAEA_DENOISE_VIEW                      |

    @needs:DN1
    Scenario: DN2. Solari hands over its light in two parts, with how far each travelled
      Given the vendored bevy_solari's resolve
      Then diffuse and specular light are written apart, each divided by its albedo (demodulated), with the diffuse and
        specular hit distance of each pixel's first bounce
      And the composite (light times albedo, plus emission) is unchanged when nothing denoises it: off matches today's
        frame to the bit
      And the plan:
        | crate        | file                                               | lines | job                          |
        | bevy_solari  | vendor/bevy_solari/src/realtime/split.wesl         | 150   | the split and the distances  |
        | bevy_solari  | vendor/bevy_solari/src/realtime/split.rs           | 120   | its textures and bindings    |

  Rule: The denoiser

    @needs:DN2
    Scenario: DN3. Light is gathered over frames where the surface is the same one
      Given each part's light from DN2 and its history
      Then each pixel's history is found through motion (a bicubic read), kept where depth, normal and mesh id agree
        and dropped where they don't (a disocclusion), and blended by how many frames it holds (up to a cap from DN0)
      And a camera cut (SolariLighting reset) drops every pixel's history
      And the plan:
        | crate        | file                                       | lines | job                              |
        | raea-denoise | engine/render/denoise/src/temporal.rs      | 200   | history textures, the cap, reset |
        | raea-denoise | engine/render/denoise/src/temporal.wgsl    | 300   | reprojection and the tests       |

    @needs:DN3
    Scenario: DN4. A new or changed surface is smoothed in space, by how sure each pixel is
      Given each pixel's light, its variance over its history and its history's length
      Then an edge-aware à-trous filter (normal, depth, luminance against variance) runs 5 times on diffuse light, wide
        where a pixel's history is short and narrow where it's long (SVGF)
      And a pixel with a short history borrows its variance from its neighbours
      And the plan:
        | crate        | file                                     | lines | job                              |
        | raea-denoise | engine/render/denoise/src/spatial.rs     | 180   | passes and ping-pong textures    |
        | raea-denoise | engine/render/denoise/src/spatial.wgsl   | 260   | variance, à-trous                |

    @needs:DN4
    Scenario: DN5. Reflections stay sharp and follow what they show
      Given the specular light, its hit distance and the surface's roughness
      Then a glossy pixel's history is found where its reflection was (the virtual point at the hit distance), and its
        filter's radius follows its roughness and hit distance (a near reflection on a smooth floor is barely blurred)
      And DN0's waxed floor scores on FLIP and flicker against the truth and MetalFX
      And the plan:
        | crate        | file                                      | lines | job                              |
        | raea-denoise | engine/render/denoise/src/specular.wgsl   | 300   | virtual reprojection, radius      |

    @needs:DN4
    Scenario: DN6. A light that changes is followed at once; a rare bright sample doesn't flash
      Given a light switched off, a door opened, a sign turned on
      Then a pixel whose light has changed more than its history explains (A-SVGF's gradient, sampled sparsely) drops
        its history in proportion, so DN0's lag after a light goes out is within MetalFX's
      And a sample far above its neighbourhood is clamped before it enters the history
      And the plan:
        | crate        | file                                      | lines | job                              |
        | raea-denoise | engine/render/denoise/src/antilag.wgsl    | 220   | gradients, clamp                 |

    @needs:DN5,DN6
    Scenario: DN7. The light is put back on its surfaces and brought to the view's size
      Given the denoised diffuse and specular light
      Then each is multiplied by its albedo and the emission added (DN2's composite)
      And a view drawn below its size (`scale`) is brought up, and a full-size one anti-aliased, by a temporal
        upscaler of raea's from FSR 2/3's MIT source, with the projection's jitter
      And the plan:
        | crate        | file                                      | lines | job                              |
        | raea-denoise | engine/render/denoise/src/compose.wgsl    | 120   | remodulation                     |
        | raea-denoise | engine/render/denoise/src/upscale.rs      | 200   | the upscaler's passes            |
        | raea-denoise | engine/render/denoise/src/upscale.wgsl    | 400   | FSR 2/3's accumulate and sharpen |

  Rule: One switch, every platform

    @needs:DN7
    Scenario: DN8. `denoiser` picks the backend, raea's by default once it meets the bar
      Given `<SolariLighting denoiser="raea" | "metalfx" | "dlss" | "off">` (RAEA_DENOISER the same)
      Then "raea" runs everywhere wgpu runs
      And "metalfx" (Mac, macOS 26) and "dlss" (NVIDIA RTX, the person's own machine, the `dlss` build feature with
        NVIDIA's SDK fetched at build time, never committed) run where they can, and elsewhere fall back to "raea"
        and say so once with the reason
      And the default is "raea" once DN0 shows it meeting MetalFX's numbers on the bench; until then the Mac keeps
        "metalfx" by default
      And the render's trace records which backend drew each view and why

    @needs:DN8
    Scenario: DN9. raea cloud's renders are denoised, and match the Mac's
      Given a Solari render split across raea cloud's GPUs (cloud-render.feature CR4) and one on lavapipe
      Then each frame is denoised by "raea"
      And DN0's cases rendered on Modal's NVIDIA GPUs and on the Mac differ by less than DN0's flicker bar

  Rule: Stills, bakes and finals

    @needs:DN0
    Scenario: DN10. A still, a held frame or a lightmap is denoised by OIDN's networks on ort
      Given OIDN 2's weights (Apache-2.0, fetched to raea's models folder) as ONNX, on ort through systems/ml (Core ML
        on the Mac, CUDA or DirectML or CPU elsewhere)
      When `raea render --at` writes a still with `--denoise`, or raea light bake writes a lightmap
      Then the colour, albedo and normal go through OIDN's network, tiled above 1080p
      And the Mac's output and Linux's agree within DN0's RMSE bar (every model, every OS)

    @needs:DN8
    Scenario: DN11. A film's finals profile: more samples, a shutter, sub-frames
      Given `raea render --final` on a 3D composition
      Then Solari runs at the finals' samples, raea's MotionBlur shutter is on (its angle from the camera's, 0.5 by
        default), and with `--subframes N` each frame is the mean of N sub-frames across the shutter
      And DN0 reports what each costs and buys on the doorway and whip-pan cases

  Rule: Everyone can see the options

    @done
    Scenario: DN12. What raea has for light, denoising and upscaling is written where agents look
      Given a person who asks an agent about DLSS, a denoiser, lighting or ray tracing (Shane, 11-24: "so its clear
        what the options are and i dont have to keep inquiring")
      Then sdk/guides/craft/bevy/solari.md (`raea guide bevy/solari`) has a section "Denoisers, upscalers and neural
        passes": each option, its state, where it runs, what it can't do and what its licence allows, from
        docs/denoise-upscale-research.md, with `raea neural mlxdlss` marked as a re-shading test that never ships
      And raea-skills' copy carries it (tools/skills.sh gen, a raea-skills PR, the pin bumped in the same landing)
      And each DN that lands updates its row in the same commit
      # Status (11-24): the section is on branch denoise; raea-skills #123 carries the copy, unmerged, for the
      # landing to merge with the pin bump.

