@spec
Feature: LightBench: a small Rust + Bevy renderer that proves raea's lighting and geometry techniques before they ship
  Shane, 2026-11-18 (the goal): "use LightBench to validate the techniques in a runnable Rust + Bevy renderer inspired
  by Nanite and MegaLights", started from five real photos rebuilt in raea (Raea/lightbench), whose biggest mismatches
  rank what LightBench works on. Native desktop, one perspective camera, static opaque geometry and dynamic point
  lights; Bevy for the ECS, the window, assets and render integration, with LightBench's own passes in WGSL on Bevy's
  wgpu. No Bevy fork and no hardware ray tracing: its shadow rays go through its own triangle BVH, so it is also the
  path for GPUs without ray queries. A technique proven here moves into raea Light (docs/realism.feature).

  The crate is bench/lightbench (`lightbench --help`). Every claim below is a run of it, at a fixed resolution and seed,
  whose numbers are written down beside the scenario.

  Background:
    Given the procedural scene: a ground, a detailed torus knot, a wall that occludes it, a pillar outside the view
    And N point lights that move, each with a range past which it adds nothing

  # LB1: the reference path
  Scenario: LB1a the G-buffer holds depth, world-space normals and albedo
    When LightBench draws the scene with --view normal, --view albedo and --view depth
    Then each shows the scene's own normals, colours and distances (Bevy's deferred prepass, decoded by LightBench)

  Scenario: LB1b the all-lights reference
    When LightBench draws --mode all
    Then every pixel sums every light, each times one shadow ray through the BVH, with the same equations the sampled
      path uses
    And the BVH's answers match a brute-force test of every triangle (unit tests, and --brute on the GPU)

  Scenario: LB1c casters outside the view still cast
    Then the pillar outside the frame throws its shadow into it

  # LB2: adaptive geometry
  Scenario: LB2a culling and discrete LODs, then a cluster hierarchy chosen by projected error
    Then a cluster's projected error is focal_px * error / depth (with the instance's scale and the camera inside its
      bounds counted), 1 px by default; a parent or its children are drawn, never both, and shared edges don't crack
    And farther is coarser, a narrower field of view at the same distance is finer, and --lod colour shows it

  Scenario: LB2b selection on the GPU with indirect draws, the CPU path kept; then hierarchical depth occlusion

  # LB3: sampled lighting
  Scenario: LB3a K light samples a pixel (4 by default), uniform then weighted
    When LightBench draws --mode uniform or --mode weighted with --k K
    Then each pixel traces K shadow rays whatever the number of lights, and every light that adds light can be chosen
    And the raw average converges to --mode all as samples grow (RMSE against it, written down per K)

  Scenario: LB3b temporal accumulation, reprojected, rejected on depth and normal, reset on a camera cut; --raw drops it

  # LB5: bounce light
  Scenario: LB5 diffuse bounces through the BVH, lit by the same light sampling, against the every-light reference
    When LightBench draws with --bounce N
    Then each pixel follows N cosine-chosen rays to the nearest triangle, and lights each hit as it lights a pixel
      (one light sample, or every light with --mode all), so the reference and the sampled path differ only in which
      lights they look at
    And the sampled bounce converges to the reference as frames grow (RMSE against it, written down) with no bias
    And a sampled pixel traces at most K + 2N rays whatever the number of lights

  Scenario: LB6 a sun and a sky, in a closed room lit through a window
    When LightBench draws --scene room, or --sun on and --sky X
    Then the sun is one shadow ray a lit point to a point on its disc, the same in every mode, so its shadows soften
      as --sun-size grows; a bounce ray that leaves the scene brings the sky
    And in the room the bounce lights what the sun patch and the window don't, and converges to the reference
    And a sampled pixel traces at most K + 1 + 3N rays

  Scenario: LB7 a world radiance cache and a half-resolution bounce, toward 60 fps
    When LightBench draws with --cache on, and --bounce-res half
    Then a bounce ray reads the hashed cell it hits and feeds it a fresh sample, so a pixel gets every bounce for two
      rays, converging to a many-bounce reference with a small bias (written down by cell size)
    And at half resolution one pixel of each 2 x 2 block traces the bounce and the rest take it up by depth and normal
    And the room's frame time is measured on its own at 1280x720, toward 60+ fps

  # LB4: instrumentation and acceptance
  Scenario: LB4 sweeps
    Then camera and light-count sweeps at a fixed resolution print triangles drawn, rays a pixel and pass timings
    And frame time is measured on its own, on documented hardware, toward 60+ fps
