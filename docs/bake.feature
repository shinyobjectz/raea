@spec
Feature: Light baked by raea: bounce light into lightmaps on raea's own ray tracer, no Blender
  Shane, 2026-11-23, while a test agent built the atelier scene (an architect's loft, every object modeled in raea)
  and baked its bounce light through Blender: "worth researching how we could do baking of bounce lights without
  blender and in raea", then "write in a spec and have another subagent buidl all 5".

  Why: the realism recipe (sdk/guides/craft/bevy/realism.md, docs/blender-research.md) halves the gap to Cycles with a
  sky, a balanced sun and a lens finish, and halves it again with bounce light baked into <Lightmap> (RMSE against
  Cycles 0.154 -> 0.081 -> 0.040 on the test set). Today that bake is Cycles through `raea blender open` + look.py +
  lightbake.py. It costs a Blender install, an export round trip that drops things (the atelier agent found
  `raea blender open` losing the export's meshes/ folder), lightmaps only on a glTF <Model> (so the baked set is one
  model, and `scene lint` and the review see one part), and a lit scene that isn't quite the scene raea draws.

  Blender stays what it is good at for raea: geometry steps (bevel, boolean, remesh, UV, geometry nodes) and Cycles as
  the reference the bake is measured against. Light is computed by raea.

  The model this file sets:
    1. A bake is a pure function of what it's asked: the static parts (their geometry, materials and placement), the
       sky, the lights it carries and the settings. It is kept in the composition's asset store under that key, as
       systems/bake keeps physics bakes (docs: systems/bake/src/lib.rs), so a render, a `render --at` and a play read
       the same bytes and nothing is baked twice.
    2. What is baked is the light that bounces: the sky, emissive surfaces and the lights marked baked, through every
       bounce, plus the sun's bounce. The sun's direct light stays live (as lightbake.py subtracts it), so anything
       that moves still casts its shadow and is lit as before.
    3. It runs on systems/rt: the same BLAS and TLAS the traced shadows and raea GI read, through wgpu ray queries
       (Metal on Apple silicon's ray hardware, Vulkan, DX12). Every OS raea ships on; never Mac-only. A device without
       ray queries is told so, with the reason, and the composition renders unbaked.
    4. Shading is Cycles', from tools/cycles-port (each function held to Cycles' own output), so the bake and the
       reference agree by construction, and LT5's path tracer (docs/realism.feature) reuses the same kernel.
    5. It lights raea's own meshes (Mesh3d, solids, raea/room) directly, as well as a <Model>'s, with no export.

  Background:
    Given systems/rt (raea-sys-rt): BLAS per mesh from Bevy's own buffers, a TLAS of visible opaque instances,
      glass left out so sky and sun reach through windows; its traced AO, raea GI's world probes and cache (LT2, LT3)
    And tools/cycles-port/cycles_bsdf.wgsl: GGX/Beckmann D, G2, Fresnel, F82 conductors, VNDF sampling, passing against
      Cycles on an M5 Pro; diffuse and energy-conserving Oren-Nayar next
    And <Lightmap> (sdk/render/src/bevy3d/lightmap.tsx, systems/material lightmap.rs): Bevy's lightmap on a <Model>'s
      glTF meshes by name, sampled by the mesh's second UVs, scaled by its material's lightmap_exposure
    And lightbake.py (sdk/guides/craft/bevy/blender/): Cycles' DIFFUSE bake per mesh into a 16-bit PNG, smart-project
      lightmap UVs, sun subtracted, edges smoothed; the reference this file measures against
    And Godot 4's LightmapGI (MIT) as the design to learn from: xatlas UVs, texels rasterized in lightmap space, a GPU
      ray tracer, a denoiser, dilation; Unity's progressive lightmapper and Cycles' own bake as the other two

  Rule: the bar (BK0)

    @done
    Scenario: BK0. The bake's test set, its references and its numbers
      Given the plan
        | crate              | file                                 | lines | job                                                                 |
        | raea-app-lightbake | apps/lightbake/src/lib.rs            | 220   | `raea bake light EDIT.tsx [--composition ID] [--samples N] [--compare DIR]`: bake, then report |
        | raea-app-lightbake | apps/lightbake/src/compare.rs        | 220   | the same set's Cycles bake (lightbake.py) and Cycles still (look.py --engine cycles) beside raea's: stops and RMSE per mesh and per frame, time, memory |
        | raea-app-lightbake | apps/lightbake/sets.json             | 40    | the test set: blender-research's table set, a Cornell box, a sunlit room with one window, the atelier's static set |
      Then each set has a Cycles reference at 1024 samples: its lightmaps and a still from a fixed camera
      And `raea bake light --compare DIR` writes DIR/report.json and a table: for each set, raea's bake against Cycles'
        bake (mean and worst-tile stops per mesh) and raea's frame against the Cycles still (RMSE, as blender-research
        measured), bake time and peak memory
      And the bar is: the frame with raea's bake within 0.045 RMSE of Cycles on the table set (lightbake.py gives
        0.040), no mesh off by more than a third of a stop on average, in under a quarter of lightbake.py's time
      And the numbers before any BK step lands are recorded in this file, as LT1's are in docs/realism.feature
      And built 11-23: `raea bake light --compare DIR` (apps/lightbake), the sets in apps/lightbake/sets.json, their
        compositions in bench/bake/ (each set's -cycles composition draws lightbake.py's maps from media/baked/). The
        atelier is listed but has no reference yet: its Manifold solids stop `raea blender open`, so it joins at BK5
        with its own tools/bake.sh bake as the comparison
      And measured 11-23 on the M5 Pro, before any bake code: Cycles at 1024 samples (look.py's still at 1280x720,
        lightbake.py at the set's texels a metre), each raea frame against the Cycles still
        | set     | frame                  | RMSE   | mean stops | worst tile | lightbake.py  | the still     |
        | table   | raea, unbaked          | 0.0964 | 0.355      | 3.10       | 242 s, 851 MB | 139 s, 1.8 GB |
        | table   | raea, Cycles' maps     | 0.0445 | 0.090      | 1.97       |               |               |
        | cornell | raea, unbaked          | 0.7075 | 5.449      | 8.64       | 235 s, 328 MB | 272 s, 1.7 GB |
        | cornell | raea, Cycles' maps     | 0.0758 | 0.140      | 1.00       |               |               |
        | window  | raea, unbaked          | 0.1884 | 1.210      | 3.32       | 126 s, 767 MB | 506 s, 2.1 GB |
        | window  | raea, Cycles' maps     | 0.1446 | 1.121      | 3.02       |               |               |
      And what they say: on the table set, Cycles' maps take raea from 0.096 to 0.045, as blender-research's 0.154 ->
        0.040 did. Today's unbaked frame is nearer Cycles than the guide's sun + ambient was, but its shaded floor is
        darker (39/255 from the sky alone, against the guide's 0.61). The table's worst tile with the maps is the chrome
        ball, which raster raea draws reflecting only the sky (LT9's). The bar's time is a quarter of 242 s: 60 s
        for the table set. The Cornell box, lit only by its emissive panel, is black in raster raea and within 0.076
        of Cycles with the maps: emissives and colour bleeding carry. The window room is not near Cycles even with
        Cycles' own maps: lightbake.py's maps are cooler than the light in Cycles' still (walls facing the window come
        out neutral where the still is orange), and neither raea's <Environment> (0.151 with it at 0) nor
        lightbake.py's indirect clamp (0.145 with --clamp 0) explains it. Until that is found, the window room's
        frame numbers aren't a bar; BK5's per-mesh numbers on it (map_stops, texel for texel on lightbake.py's UVs)
        don't go through the tone curve and still count

  Rule: the five parts (BK1-BK5)

    @needs:BK0
    @done
    Scenario: BK1. Lightmap UVs for every static mesh
      Given the plan
        | crate               | file                              | lines | job                                                                 |
        | raea-sys-weathering | systems/weathering/src/lib.rs     | 265   | `layout(glb, request)`: charts.rs's atlas alone, no causes traced: the model with TEXCOORD_1, the atlas's size and density, every texel's point |
        | raea-sys-weathering | systems/weathering/src/tests.rs   | 351   | a layout alone is the bake's atlas, byte for byte |
      Then every mesh a bake covers gets non-overlapping second UVs, charts at least 2 texels apart at its resolution
      And its texel density follows its world area (a 6 m wall and a 6 cm stapler at the same texels a metre, capped
        by `texels` and a per-bake atlas size), so nothing is stretched or wasted
      And the UVs are a pure function of the mesh: the same mesh gives the same atlas, cached with the bake
      And thin and degenerate triangles are measured: a chart under a texel is merged or flagged, never left unlit
      And one second UV set serves the lightmap and raea/wear's cause maps (Shane, 11-23: the bake works with the wear
        SDK): systems/weathering (docs/wear.feature WR1) already lays TEXCOORD_1 out for its cause maps and refuses a
        material that reads it, so a second atlas would collide. The light bake lays its maps over weathering's atlas,
        `raea_sys_atlas::layout` (in raea-sys-weathering when BK1 landed; split out at BK2), with the request `raea wear bake` takes: given the wear bake's own request, it
        is the wear bake's atlas byte for byte (the test says so); a light-only bake asks {env: {density: texels,
        max_side}}
      And built 11-23 after measuring the two on the test set: xatlas (MIT, vendored and built through cc, a map per
        mesh at 64 texels a metre, 1024 a side, padding 2) against weathering's charts.rs (planar charts within 15
        degrees, joined and folded by merge.rs, shelf-packed into one atlas with a 1-texel gutter each side, triangles
        shut inside a closed shell given no texels), both at 64 texels a metre over each set's world triangles
        | set     | layout      | islands | texels | used  | seam texels | stretch | no texels | time    |
        | table   | weathering  | 196     | 2.06 M | 73.8% | 28 428      | 0.000   | 110       | 13 ms   |
        | table   | xatlas      | 54      | 1.20 M | 69.2% | 19 494      | 0.011   | 0         | 69 ms   |
        | window  | weathering  | 61      | 1.32 M | 75.1% | 31 368      | 0.000   | 23        | 7 ms    |
        | window  | xatlas      | 72      | 1.22 M | 85.5% | 37 544      | 0.008   | 0         | 24 ms   |
        | atelier | weathering  | 15 642  | 7.45 M | 51.1% | 545 675     | 0.021   | 38 030    | 1.6 s   |
        | atelier | xatlas      | 28 913  | 7.90 M | 51.0% | 707 545     | 0.042   | 29 426    | 255 s   |
        (islands: groups of triangles joined in the UVs; texels: the maps' area; used: the share on a surface; seam
        texels: the islands' outlines; stretch: each triangle's texels a square metre against the layout's density,
        area-weighted mean |log2|; no texels: hidden triangles, and for weathering also the details folded onto the
        chart they sit on, which read its texels; for xatlas the triangles its fit shrank under a texel)
      And what they say: on the atelier, the one real set, weathering's charts are 160 times faster, with half the
        islands, 23% less seam and half the stretch at the same use, and xatlas's 255 s for UVs alone would spend the
        bar's whole 60 s four times over. xatlas is a little better on the small sets (the table's ball and vase are
        curved, the window room packs to 85%). Weathering's charts are the atlas; the xatlas vendoring went
      And a model whose material already reads TEXCOORD_1 is refused with the reason, as WR1 refuses it (a model
        lightbake.py baked); keeping a model's own second UVs is BK5's question, and the bake keys of `raea wear bake`
        don't change (weathering's bytes are the same)

    @needs:BK1
    @done
    Scenario: BK2. Each texel's place in the world
      Given the plan
        | crate              | file                                 | lines | job                                                                 |
        | raea-sys-atlas     | systems/atlas/src/lib.rs             | 130   | the atlas both bakes share, split out of raea-sys-weathering (it was at 1999 of 2000 lines and a system may build only on a facility): Ask, layout() |
        | raea-sys-atlas     | systems/atlas/src/scene.rs           | 323   | moved from raea-sys-weathering as it was                            |
        | raea-sys-atlas     | systems/atlas/src/charts.rs          | 436   | moved                                                               |
        | raea-sys-atlas     | systems/atlas/src/merge.rs           | 193   | moved                                                               |
        | raea-sys-atlas     | systems/atlas/src/rays.rs            | 80    | the hidden-triangle step and the samples, moved from weathering's rays.rs |
        | raea-sys-lightbake | systems/lightbake/src/lib.rs         | 20    | the crate: the bake over raea/wear's atlas                          |
        | raea-sys-lightbake | systems/lightbake/src/texels.rs      | 160   | the layout's texels (BK1: weathering's world point, normal and triangle under each, an edge band of 0.75 of a texel) with their ray origins; the inside test; the model's world triangles |
        | raea-sys-lightbake | systems/lightbake/src/texels/tests.rs | 160  | a slab with a block sunk in it and a box set on it by its node: points on the surface, the sunk part marked, the open not; the same texels twice; a worn model's request gives its wear atlas; the measured run |
      Then each mesh's atlas has, per texel, its world position, shading normal and whether it is covered, read from
        BK1's layout (charts.rs already rasterizes every triangle in its chart, a triangle under a texel included,
        and fills the edge band), not rasterized a second time
      And a texel whose position is inside other geometry (a table leg's foot in the floor, a wall's end inside the
        next wall) is found by a short ray test and marked, so it neither goes black nor leaks light into the bake
      And positions are offset along the normal by a scale-aware epsilon, so the trace never hits its own surface
      And built 11-23: `raea_sys_lightbake::texels::texels(glb, request)` lays the model out (BK1's layout, raea_sys_atlas::laid; the
        light-only request is `request(texels, max_side)`, a worn model passes its wear bake's), takes
        the model's world triangles as the parry3d BVH the atlas was laid out over, moves each point
        off its surface by 1e-5 of the model's size or farthest coordinate (at least 10 microns), and casts 16
        cosine-weighted rays from each: a texel is inside when at least half its rays meet a back face and more meet
        back faces than front ones (a floor that is one plane is met from below one way and not at all the other).
        CPU and rayon for now, as weathering's own rays; BK3's GPU pass may take the test over
      And raea-sys-atlas (a facility) now holds the atlas, out of raea-sys-weathering, so the light bake builds on it
        and not sideways on another system: weathering's bakes are the same bytes and keys after the move (the table
        and window sets baked before and after, 12 files equal), and its 11 tests pass
      And BK2 landed with a leak, found by BK3's glowing box and fixed 11-23: a chart's edge band takes its points from
        the edge itself, so an origin on a floor's edge was in the plane of the wall standing on it and half its rays
        left a closed room (408 of the box's 1944 texels read 1.16 for 2.0: a dark line along every joint). Its tests
        had left a strip along the contact lines out of their checks. Each origin is now four offsets in from its
        triangle's edges, moved half a texel at most (a needle narrower than that would send it to its far end), and
        the box's edge texels read 2.015. The triangles come from raea_sys_atlas::laid, numbered as the texels number
        them; the layout's model is no longer read back
      And measured 11-23 at 64 texels a metre, debug build on the M5 Pro (BK1's layout included in both times), after
        the fix; the atelier's row is from before it
        | set     | atlas     | texels    | inside           | offset   | layout | with texels |
        | table   | 2048x1008 | 1 549 350 | 8 772 (0.57%)    | 0.17 mm  | 0.27 s | 3.1 s       |
        | cornell | 1024x296  | 232 060   | 19 372 (8.4%)    | 0.04 mm  | 0.05 s | 0.7 s       |
        | window  | 2048x644  | 979 867   | 99 153 (10.1%)   | 0.08 mm  | 0.16 s | 2.7 s       |
        | atelier | 4096x1820 | 4 136 248 | 200 649 (4.9%)   | 0.15 mm  | 41 s   | 70 s        |
      And what they say: what's marked is what overlaps. In the window room it is the band where each wall box runs
        into the next wall, the floor and the ceiling, the floor under the sofa's and table's feet, and the walls
        behind the window's piers. Those are a tenth of its texels, which would have gone black or leaked the outside
        in. The texel step costs seconds in a debug build. The atelier's layout takes 41 s in debug against 1.6 s in
        a release raea (BK1), so the bar's 60 s is the trace's to spend

    @needs:BK2
    Scenario: BK3. The light that bounces, traced on raea's ray tracer
      Given the plan
        | crate              | file                                 | lines | job                                                                 |
        | raea-sys-lightbake | systems/lightbake/src/trace/tests.rs | 160   | a white furnace, a wall's form factor, a glowing box, a red wall's bleed, the same bytes twice; the measured run |
        | raea-sys-lightbake | systems/lightbake/src/wgsl/diffuse.wgsl | 60 | the diffuse BSDF and the paths' random numbers, a module systems/rt and LT5's path tracer can include as it is |
        | raea-sys-lightbake | systems/lightbake/src/trace.rs       | 260   | the passes: dispatch per atlas, samples accumulated over passes, progress, cancel |
        | raea-sys-lightbake | systems/lightbake/src/wgsl/trace.wgsl | 320   | per texel: cosine-weighted paths against the bake's own TLAS; the sky from the composition's <Environment>, emissives, baked lights; Russian roulette; Cycles' BSDFs from cycles-port at each bounce |
      Then each texel integrates the incoming diffuse light over its hemisphere along paths of up to `bounces`
        (default 4) through a BLAS and TLAS the bake builds itself from the meshes raea draws, by the wgpu ray queries
        systems/rt uses. (Not rt's own: a bake runs offline, before any frame, with no Bevy render world to hold them,
        and over rt its tests would be a cold engine build. What is shared is the WGSL, not the structures), the materials at each hit being the composition's
        StandardMaterials (base colour, its texture, roughness, metallic, emissive) as the raster reads them
      And the colour that bounces is the albedo raea draws at the hit, extensions included: ProceduralExtension's
        patterns (systems/procmat, its dust, dirt and edges), WearExtension (systems/wear) and raea/wear's recipes over
        the cause maps (sdk/wear), so a grimy brick wall bounces grimy brick. The kernel evaluates them from one WGSL
        module shared with the raster, not a copy (Shane, 11-23); Blender's export drops these materials, so this is
        a bake Cycles' round trip can't make
      And the sky is the composition's <Environment> panorama at its intensity and rotation; emissive surfaces light
        the set; the sun's direct light is left out at the first hit and kept at every later bounce (so a sunlit floor
        lights the ceiling), and a light marked `bake` is carried whole
      And samples accumulate over passes (`samples`, default 512 a texel), a pass small enough that the window and the
        machine stay responsive, with progress printed and a stall guard's line each pass
      And a white furnace (a white set under a white sky) converges to the sky's value at every texel, within 1%
      And the same request on the same machine gives the same bytes (fixed seeds per texel and pass)
      And a device without ray queries is refused with its name and backend, and any GPU error is the bake's error:
        never a black map kept as light (the first kernel failed to compile on Metal and came back as zeros)
      And in progress, 11-23: the trace of a constant sky, emissive triangles and an albedo a triangle, Lambert (written
        here: cycles-port isn't on main). A white furnace reads the sky at all 2834 open texels; a 40 m black wall's
        shadow is its form factor within 0.002 at 0.5, 1, 2 and 4 m; a closed box glowing 1 with an albedo of a half
        reads 2.004 for 2; breaking the cosine, the offset, the roulette, its weight, the emission or their order each
        fails one of them. Only Metal has run it. At 512 samples and 4 bounces, every surface 0.7 grey under a white
        sky, the trace alone takes 0.8 s on the table set, 0.3 s on the Cornell box and 0.9 s on the window room
        (M5 Pro; linear in samples, 5.6 s at 4096). Not built: the <Environment> panorama, baked lights, the sun left
        out at the first hit, textures, and the albedo raea draws under procmat, wear and sdk/wear

    @needs:BK3
    Scenario: BK4. A clean map: denoised, dilated, no seams
      Given the plan
        | crate              | file                                 | lines | job                                                                 |
        | raea-sys-lightbake | systems/lightbake/src/clean.rs       | 220   | denoise guided by normal and position, dilation past chart edges, seam texels averaged, 16-bit out |
        | raea-sys-lightbake | systems/lightbake/src/clean.wgsl     | 120   | the edge-aware filter (a-trous on normal, position and chart), as Godot's JNLM does it |
      Then the noise at 512 samples a texel is filtered without blurring across a crease, a chart edge or a contact
        shadow (the filter's weights from normal, position and chart id)
      And every chart is dilated by its padding, so bilinear and mip sampling never reads an empty texel
      And texels that meet across a UV seam are made equal, so a seam never shows as a line in the frame
      And the map is 16-bit HDR (KTX2, which Bevy reads, or 16-bit PNG as lightbake.py writes), with the exposure
        that puts it at raea's brightness, as lightbake.py's lightmap_exposure does
      And Open Image Denoise (Apache-2.0) may replace the filter where it is installed; the built-in filter is the
        default on every OS

    @needs:BK4
    Scenario: BK5. A bake in a composition: <LightBake> and `raea bake light`
      Given the plan
        | crate              | file                                 | lines | job                                                                 |
        | raea-sys-lightbake | systems/lightbake/src/lib.rs         | 200   | the bake as a pure function of its request, kept in the asset store by key; the native call |
        | raea-sys-lightbake | systems/lightbake/src/apply.rs       | 160   | each baked mesh's lightmap and second UVs on raea's own Mesh3d and on a <Model>'s meshes |
        | @raea/render       | sdk/render/src/bevy3d/lightbake.tsx  | 120   | <LightBake samples bounces texels> around the static parts it bakes; re-exported from raea/3d |
      Then `<LightBake>` around a composition's static parts bakes them on the first frame a render, a play or a
        `render --at` needs it, and every later frame and run reads the kept bake
      And it lights raea's own Mesh3d, solids and raea/room parts in place, as well as a <Model>'s meshes, so the
        parts stay parts: `raea scene lint`, `raea render --review` and `raea make` see each one, not one baked model
      And a change to a static part, a material, the sky or a setting changes the key and bakes again; a change to
        anything outside <LightBake> (the camera, a moving part, the sun's direct light) doesn't
      And `raea bake light EDIT.tsx` bakes ahead of a render and says what it baked (meshes, texels, samples, time)
      And `raea guide bevy/lights` gets a "Baked light" section that puts <LightBake> first, with lightbake.py kept as
        the Cycles reference to compare against; the realism guide's bake step points to it once that guide lands
      And on BK0's test set, a composition with <LightBake> meets the bar, and the atelier's static set bakes in raea
        with its Blender bake as the comparison
      And the test set gains a worn set: the table set's parts under ProceduralExtension and WearExtension materials
        with their cause maps, its frame measured against raea's own frame of the same parts with the bounce read from
        the drawn albedo (its Cycles reference stays flat-colour, since the export drops the extensions, and says so)
