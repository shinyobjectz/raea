Feature: Scenes that know where things meet
  Shane, 2026-10-21, after the kitchen film's reel: clipping in layouts, things floating or sunk, light breaking between
  shots. "if we put a mesh in, we have to understand ... where it meets a surface. Does it sit? Does it float? Does it
  hang?" He wants a relational, procedural engine underneath, so scene code says what a thing is on and stays clean.
  Today @raea/motion/3d takes positions only: the kitchen's bowl, crumbs and counter things all have hand-typed heights,
  and nothing says when one is wrong.

  raea/scene is that layer, on top of @raea/motion/3d: a lint that measures what every part rests on, a contact model
  per asset, placement by relation (on, hang, in), and room rules a layout solver satisfies. Lightspeed orchestrates;
  the scene SDK session builds it; lighting consistency across a cut comes later, with lightspeed's single kitchen rig.

  Rule: Measured: the kitchen as the island shots see it (wreck.tsx, composition b, frame 0; 2026-10-21, M5 Pro)
    Scenario: Linting the kitchen
      When `raea-motion scene explain kitchen/wreck.tsx --composition b --frame 0` runs
      Then 149 parts are measured in 4-7 s on the CPU, with no engine and no GPU
      And it finds faults a viewer would see:
        | part                                     | finding                                   | why                                     |
        | SM_Bowl2 (the upturned bowl)             | 7.2 mm into the island's stone             | typed y ignores its 0.08 rad tilt       |
        | crumbs 2 and 5                           | 6.1 mm over the plate                      | typed y, Y + 0.012                      |
        | the vintage kettle                       | 39.9 mm into the cutting board (17% of it) | placed by hand at the backsplash        |
        | SM_SpoonCup and SM_KitchenRollHolder01   | 17 and 27 mm into each other               | placed by hand side by side             |
        | SM_DrainingBoard                         | 13.7 mm into the sink (73% of its base)    |                                         |
        | SM_FridgeMagnet04                        | 12.2 mm into its paper                     |                                         |
        | SM_Vase1                                 | 3.0 mm over the dining table               |                                         |
      And the mug, the plate, the glass, the cereal box, the bills and the cereal loops rest on the island top (within
        2 mm), the stools and chairs on the floor, the island top on its body, the ceiling lamp on the ceiling
      And the mug, tipped over about its foot's edge by a hand-made pose (wreck.tsx mugPose), lifts off the stone mid-tip:
        0.0 mm at shot b's frame 3, 2.2 at 7, 5.9 at 11, back to 1.9 at 13 (its foot is rounded, the pose pivots on a
        sharp edge 5.1 cm out)
    Scenario: The island set fixed by it (island-set.tsx)
      Then the bowl's y is Y + 0.0842 (it was Y + 0.077) and the crumbs' y comes from their size and turn (lowest()):
        each measures 0.0 mm on its surface
    Scenario: Noise found on the kitchen and fixed
      Given v1 said a window's wall strip was sunk into it, stacked cupboard doors floated over each other, a sill
        floated 48 mm over the sink, and doors hung off their carcass were "in the air"
      Then walls and their pieces (thin, 0.8 m wide, 5 cm high or more: the strip over a window, the window) don't stand
        on each other, and a part whose box passes 3 mm into a wall's thickness is set into it: mounted
      And a door up to 2.5 cm in front of a carcass that covers it is hung on it (not a shelf off its wall: walls take
        5 mm)
      And the kitchen at shot b's frame 0 goes from 40 errors and 37 warnings to 9 errors and 2 warnings, each one real:
        the counter run's four, the magnet, the vase, the daisies, the floor-under 2 mm in the grass, a snake plant 13.5
        mm into the front sill, and the cereal box's flap at 14°
    Scenario: Parts in flight, and parts on a vertical face
      Given lightspeed's run over shots d-h mid-wreck: every cereal piece and bill in flight floated or sank (d 3 errors,
        e 22, f 20, g 166), and a fridge magnet over its paper's top edge read as sunk into the paper
      Then a part whose world transform differs on the next frame is moving: a note, never a fault, still a surface
      And a part flush against the face of what rays straight down say it's sunk into is mounted on that face
      And d, e, f, g and h lint with 0 errors (warnings left: the cereal box knocked onto its side, bills at rest in
        the air mid-wreck)
    Scenario: Still open
      Then crumbs hovering on each other read as mounted on each other

  Rule: The lint (built: sdk/react/src/motion/scene_lint.rs)
    `raea-motion scene lint|explain EDIT.tsx [--composition ID] [--frame N] [--json]` steps the composition's React to
    the frame without the engine (scene_frames_to, as the clipping check does) and measures every mesh and glTF model
    from its real triangles in the world. See-through, unlit, allowOverlap and animating parts aren't faulted but are
    still surfaces others rest on.

    Scenario: Sits, floats, sinks
      Given a part's feet: its lowest vertices and points across the faces lying there (a slab's corners overhang what
        holds it up; its middle doesn't)
      When rays go straight down through each foot, from up to half its height (at most 5 cm) above it
      Then within 2 mm of a surface it rests on that surface
      And 2 mm to 5 cm above one, touching nothing else, it floats: an error, with the y that sets it down
      And with at least 10% of its feet more than 2 mm under a surface it is sunk: an error, with how deep, the share of
        its base and the y that raises it out
      And a tall part (0.5 m or more) sunk no more than 3% of its height is seated (a door frame in its floor, walls at
        a corner): a note
    Scenario: Hangs and mounts
      Then a part not resting, with something within 5 mm of its top or sides, is mounted on it (a lamp on the ceiling,
        a shelf on its wall)
      And a part touching nothing with nothing within 5 cm under it is in the air: a warning (a bird, or a mistake)
      And the big part with nothing under it is the ground
    Scenario: Clipping anywhere
      Then two parts passing deep into each other are an error wherever they are, by the clipping check's own test
        (clip3d.rs: deep for the smaller part's size, a real share of it, not resting contact, not a foot seated in what's
        below); the render's check still says only those the camera sees
    Scenario: Tilted
      Then a part resting on a surface whose up leans more than 2° off the world's is a warning, unless it's `posed`
        (Mesh and Model take `posed`: the island's upturned bowl, its crumbs and loops)
    Scenario: Machine-readable
      Then --json gives each part its check, level, what it's on, the gap and a fix ({y, dy}: the y of its origin, for a
        part with no parent group moving it), so an agent applies the fixes mechanically
      And lint exits 1 on any error, for CI and agents to gate on
    Scenario: Tested
      Then `cargo test --lib clip3d` checks a mug on the floor, 3.5 mm over it, 6 mm into it; the floor as the ground; a
        shelf on its wall mounted and 2 cm off it in the air; a mug on its side resting on its lowest edge; a sill set
        into its wall mounted over a cupboard 4 cm below; a wall's strip over a window not sunk into it; a door 2 cm in
        front of its carcass hung on it; a magnet on its paper on a door mounted, not sunk

  Rule: A contact model per asset (next)
    Scenario: Worked out from the glTF, cached beside it
      Then each asset gets <asset>.contact.json, keyed by the file's hash: its footprint polygon, its base height, the
        up-facing faces things can stand on (a table's top, a shelf), the faces it hangs or mounts by (a back, a top),
        and a convex hull per node (for clipping and for physics boxes)

  Rule: Placement by relation (built: sdk/motion/src/scene.tsx, @raea/motion/scene)
    Scenario: Standing on a surface, measured
      Given scene.bounds, a native call: a glTF model's box turned (Euler XYZ, as Model takes it) and scaled about its
        origin, from every node's triangles with their transforms (Manifold's import drops node transforms, read the
        kitchen's 10 cm mug as 13 units, and wants closed meshes)
      Then standOn(src, h, rotation, scale) is the y that sets the model's lowest point on h, `<Place src on at>` a Model
        placed so, and boxReach(size, rotation) the same for a Box
      And the island's upturned bowl, placed by <Place on={Y}>, and its crumbs by boxReach measure 0.0 mm on their
        surfaces; the mug's tip, its height from standOn at every frame's angle, measures 0.0 mm at shot b's frames 3,
        7, 9, 11, 12, 13 and 20 (it lifted to 5.9 mm), and the coffee is baked on that pose
      And a script (raea-motion run) imports @raea/motion/scene too, so the coffee's bake spec takes the same pose
    Scenario: Surfaces by name (next)
      Given `<Surface name="island.top">` declared once, or found from a model's up-facing faces
      Then `<Place on="island.top" at={[u, v]} yaw>` sets a part level on it, its y from the surface's top and its own
        base, both measured; `<Place hang="wall.north">` and `<Place in="cabinet.2">` likewise
      And a pure place() gives the same pose to code (a pose track, the mug tipped about its foot's edge)
      And the lint finds nothing to fix in a scene placed this way

  Rule: Room rules (later)
    Scenario: A layout solver
      Then rules like "a sink in a room called kitchen goes under a window" and "stools face the island" place parts,
        and each rule it can't satisfy is said by name

  Rule: A light plan: light set like a cinematographer's, measured, graded on the scene (with lightspeed)
    Shane, 2026-10-21: "at 520 frames the color grade changes or the shot has different lighting"; "create a better
    light plan and extend the scene sdk so that its easy to manage light across components and understand how to make
    it work intelligently alongside how we finish and color grade the footage". At frame 520 (the cut from v1 to the
    island's whip, both on the fridge) the two renders of one fridge differ by 1.2% (luma 155.2 to 153.4), and a
    concept grade cut by time range jumped there (warmth switched off mid-fridge: the grade followed the edit, not the
    scene). The SDK's light is already physical (a DirectionalLight of 1 is 8000 lux, point and spot lights in lumens,
    exposure in EV100), so the plan is real cinematography, not a look applied afterwards.

    Scenario: One rig per location, declared once
      Given a location's light declared once, in physical units: the sun (lux, colour temperature, its direction from
        a time of day or an aim), the sky in each opening (lux, K), practicals (lumens, K), bounce
      Then every shot in that location renders under that rig and only its camera changes (as KitchenLight does)
      And a shot never adds or moves a light, except as a named, planned cheat the meter reports

    Scenario: Exposure from a meter, not typed
      Given a shot's key subject (a named part, or by default what the camera's centre looks at)
      When `raea-motion scene light EDIT.tsx --composition ID [--frame N] [--part NAME] [--json]` runs (CPU, no
        engine, as the lint does)
      Then it gives the incident light on the subject in lux, split light by light, each light's share seen or in
        shadow (rays against the scene's triangles), the light's colour (its mix of colour temperatures), the key to
        fill ratio, and the EV100 an incident meter sets for it (EV = log2(E x 100 / 250))
      And it says the exposure the shot actually sets and the difference in stops
      And a shot's exposure becomes the metered one plus a planned offset (a darker beat, a bright one), so a cut
        between two shots of one subject under one rig is the same brightness by construction

    Scenario: Built: the meter (sdk/react/src/motion/scene_light.rs), on the island shots (wreck.tsx a-h, 2026-10-21)
      When `raea-motion scene light kitchen/wreck.tsx --composition a,b,d,e,f,g,h` runs (62 s, CPU)
      Then each shot is metered at its first and last frames, on its subject and on the whole picture (a matrix meter:
        7 by 5 rays across the frame, the light where each lands averaged in stops), and each cut on the picture
      And it finds: the bounce light over the island (bounce-top, a point light 25 cm above the stone, no shadows) is
        the subject's key in f, 7,600-13,000 lux on the stone, 74-87% of its light, more than the sun (which the café
        curtain blocks there); the shots sit 1-2.7 stops over an incident reading (the camera at EV100 8.7); the
        panorama is cold (about 9,900 K) against the sun's 4,950 K, so the light's colour drifts 60-70 mired at each cut
        whose key changes
      And on the picture a to b, b to d and d to e hold within a third of a stop; e to f jumps +1.67 (f's bills under
        the bounce's hot spot), f to g -1.51
      And the environment and ambient light are metered as the engine draws them: unshadowed (its ambient occlusion
        darkens them in the picture, which the meter doesn't model)

    Scenario: A cut checked as a sequence
      Given an edit of shots
      Then each cut compares the subject's metered exposure and colour either side: a jump over a third of a stop, or
        a white balance drift, that the plan doesn't call for is said before anything renders
      And a shot that reads as another room (its key mostly from a light the others don't see) is said by name

    Scenario: White balance per location, the grade on the scene
      Then white balance is set once per location in kelvin, from the rig's key, not per shot
      And the film's look is one grade (motion/3d's <Grade>, on scene light before tone mapping) declared once and
        applied in every shot of every location, never by time range on the finished cut
      And 2D shots (Paint) go through the same look, so a cut from 2D to 3D shares its curve and colour
      And finishing (grain, halation, vignette) is part of that look, applied once

    Scenario: Grading measured, not eyeballed
      Given things that appear in more than one shot (the counter, the fridge's enamel)
      Then they are the grey card: measured either side of each cut, the same object should read the same, and the
        smallest per-shot exposure and white balance trim that makes it so is solved for and reported
      And the look itself may be fitted to reference stills by their colour statistics (Oklab), as the coffee was
        fitted to Shane's photo
