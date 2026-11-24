@spec
Feature: raea/wear, weathering that has a cause
  Shane, 2026-11-17, pointing at a geometry-nodes damage kit (grunge masks by AO or rules, edge wear, streaks,
  rule-based scatter, broken windows, cracked roads) and at a leaf made from stacks of procedural masks: a raea-native
  version, "realistic procedural and highly intelligent seeding ... for what makes sense on wear, wear by material,
  crumbling infrastructure, doors, walls, plates", and the same for how textures are built and made cheap.

  raea's wear was shader-only (systems/wear: noise, screen-space curvature, grime on faces turned down;
  systems/procmat: a pattern with dust, dirt and edges). Nothing in it knows where water runs, what is sheltered, where
  hands touch or what a surface is made of, so the marks are scattered evenly rather than caused. The fix is the
  method of the literature: the agents of wear are traced through the geometry from their sources, and they leave
  their marks where the physics puts them. Dorsey, Pedersen and Hanrahan, "Flow and Changes in Appearance" (1996):
  drops of water flowing over surfaces, absorbing, dissolving and depositing. Chen et al., "Visual Simulation of
  Weathering by gamma-ton Tracing" (2005): aging particles from rain, sun, sky and leaks, picking up and depositing
  ("stain bleeding"). Each material then answers each cause its own way: black gypsum crust forms on carbonate stone
  where rain does not wash it, and rain-washed faces recede and stay light (Bonazza, NABU MedShip 2019).

  Three parts, each a standard of its own:
    1. A bake (raea-sys-weathering): a glTF and an environment in, its cause maps out (where rain reaches and runs,
       what the sun and sky see, edges and hollows, height over the ground, where hands, feet and wheels touch, what
       the water carries and leaves), on a second UV set it adds, with the spots where the rules say a thing belongs.
       A pure function of its request, kept by content as raea-sys-bake's bakes are.
    2. Recipes (raea/wear): one WESL surface hook per material class (docs/shaders.feature SH1, SH2, SH4) reading the
       same maps, so plaster, brick, sandstone, painted wood, iron, zinc, copper, slate, glass, enamel and brass each
       answer rain, shelter, run-off, damp, sun, touch and age as that material does.
    3. Damage from the same maps: plaster blown off to the brick behind it, paint to the wood, enamel to the iron;
       cracks from the corners of openings; broken panes; debris where the damage is.
  Decided with Shane the same day: the recipes ship as an SDK part (raea/wear, not only an example to copy);
  examples/materials/wear/ is its demo, which closes docs/examples.feature EX9 (raea-sys-wear deleted); detail
  textures baked from mask stacks, mips on texture arrays and BC compression wait until the look is proven (WR8).
  Shane, later that day: every season and weather, not winter only (spring, summer, autumn, winter, snow, blizzard,
  rain, thunder; docs/realism.feature D17, LT16). So a bake holds only causes, which hold all year, and what a season
  or a day's weather does (lying snow, wet, ice, rivulets, puddles, driven snow, fallen leaves, algae) is drawn from
  uniforms over those causes, never baked in (WR9).

  Rule: 1. The causes

    @done
    Scenario: WR1. Cause maps baked from the geometry
      Given a .glb (static meshes; skins, morph targets, sparse accessors and a material already reading TEXCOORD_1
        are refused) and an environment: up, north, the rain's direction and cone, the driving snow's direction and
        cone, the latitude, the walking surfaces as material classes or a height, texels a metre (one number, or by
        class)
      When `wear.bake` runs
      Then it returns the .glb with TEXCOORD_1 and four RGBA8 maps over that atlas, kept by content in the store
        | map     | r                                          | g                                     | b                                   | a                              |
        | causes0 | sky: rain's reach, n.d over the rain's cone | sun: share of the year's sun path in sight | cavity: occlusion within 0.5 m | edge: nearness to a convex arris |
        | causes1 | flow: run-off passing (WR2)                | deposit: what it left (WR2)           | deposit kind (WR2)                  | touch (WR3)                    |
        | causes2 | ground: height over the walk below, 0-4 m  | element: the chart's own number       | open: share of the whole sky seen   | class: 16 x (its place + 1) (WR5) |
        | causes3 | winter sun: share of the December path in sight (where snow lingers) | drift: driven snow's reach, n.d over its cone | litter: where blown leaves and snow settle (concave, at the walk, in the lee) | spare |
      And the year's sun is what fades and dries (it acts all year); the winter sun only decides where snow and ice
        outlast a thaw, so it has its own channel rather than a season in the request
      And the atlas is planar charts, packed on shelves with a 1-texel gutter, built in five steps
        | step   | what                                                                                                     | why                                                    |
        | flood  | a primitive's triangles joined over shared (welded) edges while within 15 degrees of the first's plane  | a curved bay or a draped street stays one chart        |
        | merge  | coplanar charts (1/64 in the normal, 2 cm in offset) whose rectangles touch, joined                     | a wall cut in strips by its openings shares no edges   |
        | split  | a chart filling under half its rectangle cut on a 64-texel grid                                         | a street network round its blocks, gutters at one eaves height |
        | absorb | a chart under 1.5 texels across (or 6 in area) folded into the big chart within 35 cm of its plane     | a reveal or a frame's face reads the wall it sits on   |
        | hide   | a triangle whose centre and three corners all see the back of something in nine rays of ten: no texels | ground under a house, party walls against each other   |
      And a level chart is turned in its plane to its smallest rectangle; a wall keeps its rows down the fall line
      And the atlas is also the light bake's (docs/bake.feature BK1, 11-23): raea_sys_atlas::layout(glb, request) returns it alone,
        with no causes traced, and given a bake's request it is that bake's atlas byte for byte, so a lightmap and the
        cause maps read one TEXCOORD_1. Measured there against xatlas, these charts were 160 times faster on the
        atelier's 655 000 triangles, with half the charts and less seam and stretch
      And rays are parry3d's TriMesh BVH (in the tree through rapier3d); samples are a rotated R2 sequence per texel
      And each triangle has a material class from the request's table, by glTF material name or by COLOR_0's alpha
        code: plaster, brick, sandstone, granite, wood, iron, zinc, copper, slate, felt, glass, enamel, brass, ground
      And the crate, as built
        | file                             | lines | job                                                              |
        | systems/weathering/src/lib.rs    | 208   | the request, its content key, the store, the bake's steps        |
        | systems/atlas/src/lib.rs         | 121   | raea-sys-atlas (a facility, split out 11-23): Ask (up, north, density, max_side from the request's env), lay() for both bakes, layout() alone |
        | systems/glb/src/lib.rs           | 208   | raea-sys-glb (a facility): the .glb read, unused data dropped, written |
        | systems/atlas/src/scene.rs       | 323   | glTF to triangles with a class each, and back with TEXCOORD_1    |
        | systems/atlas/src/charts.rs      | 436   | charts, their orientation and packing, texel to surface point    |
        | systems/atlas/src/merge.rs       | 193   | merge, split and absorb                                          |
        | systems/atlas/src/rays.rs        | 78    | hidden triangles; the samples both bakes' rays share             |
        | systems/weathering/src/rays.rs   | 193   | sky, sun, cavity, open, edge and ground                          |
        | systems/weathering/src/maps.rs   | 51    | the maps as PNGs, dilated over the gutters                       |
        | systems/weathering/src/tests.rs  | 279   | synthetic fronts checked by number                               |
      And proof (`cargo test -p raea-sys-weathering`, 9 tests): under a 1 m overhang the sky term is under 0.4 of an open
        wall's; a south wall sees the winter sun and a north one doesn't; the ground channel reads the height to 0.1 m,
        through a slab to the ground under it; a slab's arris wears and a wall's foot doesn't; a 4 x 4 m court 12 m
        deep sees under 0.35 of the sky; a floor shut in a house gets no texels; charts never overlap and keep their
        density within 5%, by class where asked; the same request gives the same bytes, TEXCOORD_1 on every primitive
      And measured on a 250 m city tile (214,869 triangles, 6 materials) on an M5 Pro Mac (the last two rows with roofs
        at 1-2 texels a metre and the ground at 2, the rest 4 throughout)
        | step                                      | atlas       | texels covered | charts | rays   |
        | flood only, 2-texel gutter                | 4096 x 3928 | 6.70 M         | 80,726 | 7.8 s  |
        | merge and absorb, 1-texel gutter          | 4096 x 2868 | 5.67 M         | 17,658 | 6.8 s  |
        | hide and split                            | 4096 x 2252 | 4.90 M         | 18,786 | 4.5 s  |
        | level charts turned                       | 4096 x 1828 | 4.81 M         | 16,024 | 5.0 s  |
        | density by class (roofs 1-2, ground 2)    | 4096 x 1400 | 3.81 M         | 15,743 | 4.1 s  |
        | hidden by four points, open channel       | 4096 x 1364 | 4.06 M         | 16,213 | 4.9 s  |
      And what the maps then cost uncompressed is 22 MB each a tile (BC compression and dropping causes2 where a
        model's own uv already says the height are WR8's)

    @done
    Scenario: WR2. Water: run-off, drips and what it carries
      Given causes0's sky term
      Then drops land where rain reaches, in proportion to it, and run down their surface (gravity projected onto it);
        on a ledge (a face within 30 degrees of level) they run to its lower edge and gather toward its ends, and leave
        at an overhang's edge, fall straight down (a ray) and run on over the surface they land on
      And a drop takes up what the surface it crosses gives (copper green, iron rust, zinc white, lime, and soot from
        a sheltered ledge) and leaves it downstream, so a stain bleeds from its cause: flow counts the water passing,
        deposit what it left, kind the most of it
      And as built (11-17), in its own crate, raea-sys-runoff, a facility the bake builds on: plain data in (positions,
        triangles, normals, a role each, each corner's texel in the atlas, the BVH), sums out, so other bakes can use it
        | crate               | file                           | lines | job                                                            |
        | raea-sys-runoff     | systems/runoff/src/lib.rs      | 141   | the input, the sums (fixed point, 64-bit), the map's bytes     |
        | raea-sys-runoff     | systems/runoff/src/walk.rs     | 253   | one drop's walk: faces, edges, corners, drips, what it carries |
        | raea-sys-runoff     | systems/runoff/src/tests.rs    | 148   | a wall with a sill and an iron bracket, checked by number      |
        | raea-sys-weathering | systems/weathering/src/flow.rs | 54    | a drop per texel the rain reaches; roles by class; causes1     |
      And the walk's rules:
        | where the drop is                              | what it does                                                   |
        | on a face                                      | runs down it (gravity projected), each drop up to 4 degrees off the fall line (ASSUMED), so a tear widens |
        | on a level face                                | keeps one heading, drawn per drop, so a ledge sheds off each edge by its share |
        | a face carries on behind what stands on it     | the run stops at whatever it meets head on just off the face (a sill on the wall it runs down) |
        | at an edge into a face that climbs away        | runs along the corner (a ledge's back, against the wall)       |
        | at an edge onto an underside, or a free edge   | looks along the edge 0.25 m for a wall met head on and clings into that corner (a sill's end), else drips: a ray straight down, 70% kept |
        | on a walking surface or a roof (slate, felt)   | ends (the roof drains to its gutter)                           |
      And iron gives rust, zinc white, copper green, and every other face that looks up gives the dust that lies on it;
        what a drop carries it leaves over the next metres (half in 1.2 m), and it soaks away (1/e in 8 m)
      And causes1 r = water passing a texel per metre across (log, 0.02-20), g = what was left per square metre (log,
        1e-4 to 0.1), b = the kind left most (0, 60, 120, 180, 240: none, grime, rust, copper, zinc); a wall's own
        driven rain counts in r, so the streaks a recipe draws come from g
      And a bake keys on raea-sys-runoff's VERSION as well as its own, so a change to the walk is never served stale
      And proof (`cargo test -p raea-sys-runoff`, 4 tests): under a 1.6 m sill the wall below its ends gets over 5x the
        water below its middle and over 3x the wall's beside it; an iron bracket leaves rust on the wall below it and
        nothing beside it; the same rain gives the same sums; a face that drains ends a drop there
      And measured: on the city tile it adds about 0.4 s to the bake (4.9 to 5.3 s); a 32 texel-a-metre test bed
        (tools/wear/testbed.ts in the game project: a front, a firewall, a rubble heap, a kerb) shows the tears under
        sill ends and the railing's rust
      And still open: a deposit's kind is filtered with its neighbours in the shader (a kind map wants nearest
        sampling, which the shared sampler doesn't give); a stain on the ground (drops end on it)

    @done
    Scenario: WR3. Touch, and where things belong
      Given the classes and the ground channel
      Then touch is written by rules, never by hand: a door leaf (wood, standing on the walking surface, 0.7-1.6 m
        wide, 1.9 m or more tall) gets a handle band at 0.95-1.1 m on its lock side and a kick band at 0-0.3 m; a
        tread is worn at its middle; a rail's top where hands slide; a gateway's jambs at hub height, 0.4-0.9 m; a
        passage's walls at the shoulder, 1.2-1.6 m, where two walls stand under 3 m apart
      And spots.json lists where the rules say a thing belongs, each with who put it there: bills on sheltered plinth
        faces at 0.5-2.5 m (a bill poster), a plate on a corner at 2.5-3 m (the city), a nameplate beside a door (the
        tenant), debris under damaged plaster (the weather)
      And as built (11-17), in its own crate, raea-sys-touch, a facility the bake builds on as it does on run-off:
        plain data in (triangles, a material each, which are walked on, the BVH, and each texel's point with what the
        other maps say there), touch and spots out
        | crate               | file                            | lines | job                                                         |
        | raea-sys-touch      | systems/touch/src/lib.rs        | 137   | the input, parts (one material, welded at 1 mm), the output |
        | raea-sys-touch      | systems/touch/src/bands.rs      | 296   | doors, treads, rails, passages and gateways, and the bands  |
        | raea-sys-touch      | systems/touch/src/spots.rs      | 167   | the spot rules                                              |
        | raea-sys-touch      | systems/touch/src/tests.rs      | 248   | synthetic fronts checked by number                          |
        | raea-sys-weathering | systems/weathering/src/touch.rs | 47    | the scene and texels handed over; causes1 a and spots.json  |
      And the rules as built (every number ASSUMED until a source is read, except where a note names one)
        | what                 | found by                                                                                    | marks                     |
        | a door leaf          | a wood part 0.7-1.6 m across, 1.9 m or more tall, at most 0.12 m thick, with a floor in front or behind within 0.2 m of its foot, looked for from 0.3 m over it (a leaf set into a step stands on the step) | handle band 0.95-1.1 m over that floor within 0.3 m of the lock edge; kick band 0-0.3 m, full at the lock edge and 0.35 at the hinge |
        | the lock edge        | two leaves in one plane whose edges meet within 8 cm: the meeting edge; else one edge, by a seed from the leaf's place | |
        | a tread              | an up face of masonry or wood, not walked on, 0.2-0.45 m deep and 0.6-4 m wide, in a flight (another tread 0.1-0.23 m up or down beside it) or a first step at most 0.26 m up | dished: 1 - x^2 across its width |
        | a rail               | a metal or wood part, long and thin, its top 0.75-1.2 m over the walk                         | its top                   |
        | a passage            | a wall facing another under 3 m off, both at least 0.6 m deep, the floor between them walked | 0.8 at the shoulder, 1.2-1.6 m |
        | a gateway            | walls 2.2-4.5 m apart, the floor between walked, a ceiling within 6 m                        | the hub band, 0.4-0.9 m   |
      And the spots as built
        | what         | by          | where                                                                                     |
        | bill         | bill poster | a plain wall (no opening within 3 m across, none from 0.5 m below to 2.5 m above), out of the rain, 0.95-2.05 m up, open ground in front, 3 m apart; Berlin's columns (Litfaß, from 1855) were brought in to keep bills off house fronts |
        | street plate | city        | each face of a corner whose arris runs from the walk past 3.5 m, 2.5-3 m up, both faces open for 4 m |
        | nameplate    | tenant      | beside a single leaf's lock edge, 1.5 m over its floor, on whichever side sees more sky (a hall sees none; a balcony over a street door hides only part) |
        | debris       | weather     | on the ground in front of plaster soaked at its foot (run-off and rain), 1.5 m apart      |
      And a bake keys on raea-sys-touch's VERSION as well, and writes spots.json beside the maps (its path in meta.json)
      And proof (`cargo test -p raea-sys-touch`, 9 tests; `-p raea-sys-weathering`, 10): on a test front the handle
        band lands on one side of the leaf, never on the wall, and the nameplate beside that side; a pair of leaves is
        held where they meet; a leaf set into a step is held from the step's top; treads are worn at the middle and a
        window sill isn't one; two walls 2.5 m apart wear at the shoulder, and at the hub only under a ceiling; a
        window's reveals are no passage; bills go on a plain wall and never beside a door; a corner gets one plate a
        face; the same input gives the same spots; through the bake, a door's band and its nameplate
      And measured: on the 32 texel-a-metre test bed (i-acp-mwlah7s6-107) the door is held on its lock side, kicked in
        patches, its nameplate beside it, a plate on each corner and one bill, on the firewall; on a 4 texel-a-metre
        city tile (5.2 M texels) the rules take 0.45 s and find 1,398 leaves, 246 treads, 1,140 nameplates, 303 street
        plates and 63 bills
      And still open: plates land on court corners and steps in a frontage too (which faces are on a street is the
        map's to say, a hand-off); at 4 texels a metre a 15 cm handle band is under one texel, so the city's doors
        want their own density class; no rails were found on the tile (its balconies' railings aren't modelled there)

    @needs:WR1 @done
    Scenario: WR4. `wear.bake` and `raea wear bake`
      Then a composition calls `wear.bake` (a native call, as physics.bake is) and gets the glTF and maps by file, and
        `raea wear bake REQUEST.json [-o OUT.json]` runs the same bake from a shell
      And the plan
        | crate             | file                              | lines | job                                                      |
        | raea-runtime-wear | runtime/wear/src/lib.rs          | 60    | the native call in the composition's folder (registered from thread.rs as raea-runtime-shots is; raea-runtime-react is at 1,965 lines) |
        | docs               | docs/manual/reference/cli/wear.md     | 40    | the verb's reference                    |
      And as built (11-17)
        | crate             | file                              | lines | job                                                      |
        | raea-runtime-wear | runtime/wear/src/lib.rs           | 67    | `wear.bake` and `raea wear bake`: the src found by the runtime's media rule, the bake kept in the composition's asset store, timed as made on first use |
        | raea-runtime-wear | runtime/wear/src/tests.rs         | 97    | a wall and floor baked once into a project's store, a missing src, the verb's -o |
        | raea-runtime-react | src/thread.rs                    | +5    | registers it with the composition's folder and media rule (the crate needs no engine) |
        | raea-cli          | src/cli.rs, src/tests.rs          | +3    | the verb, its help line, its usage in the tests' list    |
        | docs              | docs/manual/reference/cli/wear.md, index.md; tools/manual-ref/cli.ts | 49 | the page, its row, its section |
      And proof: `cargo test -p raea-runtime-wear` passes (3 tests), and the verb's help and manual page pass L1's
        manual reference check

  Rule: 2. The materials

    @needs:WR4 @done
    Scenario: WR5. raea/wear: a recipe per material
      Given `wearShader({ classes, weather, age })` from raea/wear
      Then it gives `{ wesl, uniforms, textures }` for a <Model>'s <Recover>, a <MeshStandardMaterial shader> or a
        <SurfaceShader>: the three maps as its textures, a `surface` hook that reads them and the material's class,
        and one function per class answering each cause
        | class     | rain-washed            | sheltered                 | under a ledge              | ground band                          | edge, touch                     |
        | plaster   | clean, paint chalky    | soot grey, darkest under cornices | dark tears from sill ends | splash to 0.4 m, damp tide 0.6-1.2 m | chipped arrises low down, grease band |
        | brick     | mortar washed out      | soot black                | efflorescence              | tide line, green on north faces      | spalled faces                   |
        | sandstone | washed light           | black crust               | black and white streaks    |                                      | rounded arrises                 |
        | wood      | sun-faded on south and west, alligatored |           | peeling at bottom rails    | rot at the foot                      | worn to the wood where held and kicked |
        | iron      |                        |                           | rust on the stone below    | rust at the foot                     | paint flaking at joints, bright where held |
        | zinc, copper | patina              |                           | white or green streaks     |                                      |                                 |
        | glass     |                        | a grime film at the bottom and corners |               |                                      |                                 |
        | enamel    |                        |                           | rust bled from its screws  |                                      | chipped to black iron at holes and edges |
        | brass     | dull brown             |                           |                            |                                      | bright where touched            |
      And the season and the weather (WR9), soot, years since the last repair and an upkeep that varies by element are
        uniforms; what years of weather left (soot, crust, fading, peeling, rust, touch) is the same in every season
      And a debug mode shows one cause in false colour
      And the plan
        | crate     | file                          | lines | job                                          |
        | raea/wear | sdk/wear/src/index.ts         | 200   | wearBake, wearShader, the class table         |
        | raea/wear | sdk/wear/src/causes.ts        | 120   | the WESL that reads the maps                  |
        | raea/wear | sdk/wear/src/recipes.ts       | 420   | one WESL function per class                   |
        | raea/wear | sdk/wear/src/debug.ts         | 60    | a cause in false colour                       |
        | raea/wear | sdk/wear/test/wear.test.ts    | 120   | the table, the shader's text, the uniforms    |
      And as built (11-19)
        | crate               | file                              | lines | job                                                                 |
        | raea/wear           | sdk/wear/src/index.ts             | 199   | wearBake, wearShader, WEAR_CLASSES, seasonOf, the surface hook       |
        | raea/wear           | sdk/wear/src/causes.ts            | 125   | the maps read (element and class by textureLoad), noise, the face's frame, the marks every class shares |
        | raea/wear           | sdk/wear/src/recipes.ts           | 251   | a function per class (14), the shared stains, damp and splash, WR9's weather over them |
        | raea/wear           | sdk/wear/src/debug.ts             | 73    | a cause in false colour from a `lit` hook (14 causes, class too)     |
        | raea/wear           | sdk/wear/test/wear.test.ts        | 116   | the table against the bake's, the shader's text, the uniforms, debug |
        | raea-sys-weathering | src/rays.rs, maps.rs, scene.rs    | +30   | causes2's alpha is the class (16 x (place + 1)); gutters copy class and element; TEXCOORD_0 where a primitive had none; version 6 |
        | raea                | sdk/raea/src/native-calls.ts      | +2    | wear.bake typed (tools/native-types.sh --write)                     |
      And its shape: `wearShader({ baked, classes?, age: { years, soot, upkeep }, weather: { day, wet, snow, storm, ice,
        damp?, dust?, leaves?, melt? }, up?, debug? })`; `baked` is wearBake's answer (its maps and density), `classes`
        limits the recipes compiled, and the season's damp, dust and leaves follow from `day` unless given. Authors put
        it on <ExtendedMaterial base={<StandardMaterial/>} extension={<ShaderExtension {...wear}/>}> (one glTF
        material by the base's `material`) or <SurfaceShader {...wear}>; a mesh without the bake's TEXCOORD_1 is left
        as it was (`@if(VERTEX_UVS_B)`)
      And the class comes from the bake (causes2's alpha, read by textureLoad), so one shader serves every material and
        COLOR_0 alpha codes alike; Bevy's standard surface reads the first UV set wherever there's a second, so the bake
        gives a primitive with none the atlas's as both
      And proof: `bun test` in sdk/wear (12 tests); `cargo test -p raea-sys-weathering` (11, one new: each texel's class
        and the gutters' a real one); a street front of all 14 classes made in code, baked at 16 texels a metre and
        rendered on the Mac (11-19): the class view names every part right; after 80 sooty years the cornice is
        streaked, the sills drip, the plinth and the brick's foot carry splash and a damp tide, the south door is
        faded, the railing rusted; rain, snow with a storm and autumn render; an unbaked sphere under the same shader is
        unchanged. The corner house with stills of every cause and condition is WR7's

    @needs:WR5,WR3 @done
    Scenario: WR6. Crumbling
      Then where damage is high (ground band, edges, sheltered damp, neglect), plaster is gone to the brick 2 cm behind
        it under a shadowed lip, paint to primer and wood, enamel to iron; cracks run from the corners of openings;
        panes are broken or boarded by neglect; slates are missing; and spots.json puts debris under the losses
      And the cracked-road fracture of the kit waits for a piece that needs it
      And as built (11-19)
        | crate               | file                              | lines | job                                                                 |
        | raea-sys-damage     | systems/damage/src/lib.rs         | 241   | an opening's corners, a crack from about half of them (by a seed per corner), each point's loss potential (damp foot, soaked faces, knocked arrises, along a crack), debris under the losses where the rain falls, landed on the walk below; version 3 |
        | raea-sys-damage     | systems/damage/src/tests.rs       | 190   | an opening's four corners and no others, cracks along the wall, what loosens a coat, debris under the foot (none indoors, none on a plinth), about half the corners cracking |
        | raea-sys-weathering | src/lib.rs, rays.rs, touch.rs     | +33   | causes3 a's crack, the loss potential and the debris spots from raea-sys-damage |
        | raea-sys-touch      | src/lib.rs, spots.rs              | +18   | a spot's loss, the seed shared with damage                           |
        | raea/wear           | sdk/wear/src/causes.ts            | 159   | neglect = (1 - e^(-years / 25)) x (1.15 - upkeep); abandon, the share of a house past what a lived-in one lets go (smoothstep from 0.85 of neglect at its base upkeep) |
        | raea/wear           | sdk/wear/src/recipes.ts           | 303   | plaster off to the brick under its lip, paint to primer and wood, enamel flaked to iron, cracks opening by neglect (stepped through brick's joints), panes broken (38%) and boarded (16%) by abandon, slates gone by neglect and abandon |
        | raea/wear           | sdk/wear/src/index.ts             | 257   | wearDebris: the bake's debris spots where loss x abandon reaches 0.3 |
      And proof: `cargo test -p raea-sys-damage` (5 tests) and `-p raea-sys-weathering`; `bun test` in sdk/wear (22
        tests: neglect and upkeep, abandonment's threshold in the shader's text, debris none at upkeep 0.35, all at 90
        years and 0.05, some at 80 and 0.2); the corner house (WR7) rendered after 80 years and as a ruin: plaster off
        the rusticated foot and around the openings, cracks from the window corners, the ruin's panes broken and
        boarded and slates gone, heaps on the walk under the losses and none inside or on the plinth

    @needs:WR5 @done
    Scenario: WR9. Every season and weather, from uniforms over the causes
      Given the look's date and weather, the values the realism lane's sets carry: `day` (the day of the year, 1-366)
        and `hour`, from which the season's phase (day / 365.25) and the sun follow; snow, wet and ice as weather of
        their own (a December day can be dry and a March day snowed); `leaf` (the trees' cover, 0-1) where the trees
        need it; the wind; and the time for what runs
      Then each condition is drawn from the causes, and nothing of it is baked
        | condition | what it does on the surfaces                                                                     | causes it reads                         |
        | spring    | damp lingers on north faces and plinths; green algae where flow and shade meet; dust washed off | sun, flow, ground, sky                  |
        | summer    | dry: dust lies on ledges and treads, run-off stains read pale, wood at its most faded           | sun, deposit, sky                       |
        | autumn    | fallen leaves on the walk where litter is high, wet and dark after rain; leaves in gutters       | litter, ground, flow                    |
        | winter    | frost on faces the winter sun misses; old snow greyed by soot in the lee                       | winter sun, litter, open                |
        | snow      | snow lying on faces open above (ledges, sills, cornices, roofs, treads), deeper in the lee, gone where the winter sun reaches and where feet pass | open, winter sun, litter, touch         |
        | blizzard  | snow as above, and driven snow plastered on faces the wind reaches, drifts against walls         | drift, litter, open                     |
        | rain      | darkened where the rain reaches; rivulets running down the flow map's paths; puddles on the walk where it is hollow | sky, flow, cavity, ground               |
        | thunder   | heavy rain (wet full, rivulets full, puddles splashed); the flash and the sky are the light's (docs/realism.feature) | as rain                                 |
      And a condition can change during a shot (rain starting, snow settling): the uniforms take a value per frame and
        the look follows, with wetting and drying eased over the minutes they take (the rates ASSUMED until measured)
      And the wind's direction for driven snow is a bake input (causes3 g), ASSUMED east to north-east for Berlin's
        winter storms until the DWD's records are read; the rain's stays west
      And the bake's half as built (11-17), in its own crate, raea-sys-exposure, which also took the sun's path out
        of the bake (raea-sys-weathering 4 keys on it; the request's sun.season is gone)
        | crate               | file                            | lines | job                                                   |
        | raea-sys-exposure   | systems/exposure/src/lib.rs     | 199   | the sun by date, driven weather, litter              |
        | raea-sys-exposure   | systems/exposure/src/tests.rs   | 91    | synthetic fronts checked by number                   |
        | raea-sys-weathering | systems/weathering/src/rays.rs  | 260   | causes0 g the year's sun; causes3 per texel          |
      And its rules: the year's sun is the middle day of each month, every 2 hours the sun stands 3 degrees up;
        December's is days 335, 355 and 15, every hour; drift is the sky term's measure up a storm's cone (from the
        east-north-east, 60 degrees off the vertical, 25 wide, all ASSUMED); litter on faces looking up is 0.7 x the
        share of eight low rays (10 degrees up) meeting something within 1.5 m, plus 0.3 x the share of five rays
        toward the wind meeting something within 3 m (weights and reaches ASSUMED)
      And proof (`cargo test -p raea-sys-exposure`, 4 tests): the noon sun stands 14.1 degrees at the December
        solstice and 60.9 in June at 52.5 N; a south wall sees December's sun and a north one none, though it sees
        some of the year's; a wall facing the storm takes as much drift as a level roof and its lee none; litter
        gathers in a kerb's gutter and in a wall's lee, never on open road or a face that doesn't look up; through
        the bake (`-p raea-sys-weathering`), the same for December and the year on causes3 r and causes0 g
      And measured: the 32 texel-a-metre test bed bakes in 2.0 s of rays (1.5 s before causes3); the 4 texel-a-metre
        city tile in 13.8 s under another lane's load; a prototype of the eight conditions over the causes, in the
        game project's own wear recipe (components/wear/look.ts: snow lying on what looks up and is open, banked where
        litter settles, thinned by December's sun, trodden off by touch, plastered by a storm by drift; rain wetting
        what the sky term reaches, streaming down the run-off paths, puddling in hollows; leaves by litter; dust),
        posted as i-acp-mwlb9fb4-110
      And still open: the mapping from the look's `day` and weather to these values is WR5's (the prototype takes
        storm, leaves, dust and melt as values of their own); frost and algae; the flash of thunder is the light's
      And the shader's half as built (11-19): the eight conditions are WR5's recipes over these causes (spring's damp
        and algae, summer's dust, autumn's leaves, frost where December's sun misses, snow lying and driven, rain
        wetting, streaming and puddling), named once, and weather that changes during a shot eases toward the sky's
        | crate     | file                     | lines | job                                                                       |
        | raea/wear | sdk/wear/src/weather.ts  | 79    | WEAR_CONDITIONS (the eight as weather), weatherAt(base, spells, minutes): each value eased exactly toward the sky's per spell, so any frame draws alone; WEATHER_RATES (wetting in about 3 minutes, drying over an hour, snow settling over 20 and melting over 2 hours, driven snow in 10 and off in 30, frost over 30 and gone in 20; ASSUMED until measured) |
        | raea/wear | sdk/wear/test/wear.test.ts | 216 | rain soaks and dries at its rates, frames stand alone with no jump at a spell, snow, storm and frost, spells out of order refused, each condition in its season |
      And proof: `bun test` in sdk/wear; the corner house under all eight by day and at dusk, and a shower crossing it
        (WR7's rain-passes: dry, soaked and drying at frames 0, 40 and 119)

  Rule: 3. Shown, then cheap

    @needs:WR5 @done
    Scenario: WR7. The demo, and EX9
      Then examples/materials/wear/ is a corner house made in code (plinth, rusticated ground floor, plastered storeys,
        sills, string course, cornice on consoles, door, iron balcony, zinc gutter and downpipe, an enamel street
        plate, a sandstone surround, steps, a cellar hatch), baked and shaded, with stills of every cause in false
        colour and the house under each of WR9's eight conditions, by day and at dusk
      And `wear` on MeshStandardMaterial and <WearExtension> are raea/wear's, and systems/wear, its spawn branch and
        its row columns are deleted in the same landing (docs/examples.feature EX9)
      And as built (11-19)
        | crate         | file                                    | lines | job                                                         |
        | example       | examples/materials/wear/house.ts        | 260   | the corner house as solids (each 8 corners), a hip roof, openings cut through each band, one .glb material per class |
        | example       | examples/materials/wear/make-house.ts   | 11    | writes media/house.glb                                       |
        | example       | examples/materials/wear/wear.tsx        | 179   | before, new, after (80 sooty years), ruin, ruin-near, door; debug-CAUSE for all 16; the eight conditions and their -dusk; rain-passes (120 frames, half a minute each); paint (worn paint with no bake) |
        | example       | examples/materials/wear/wear.test.ts    | 85    | every part WR7 names, the openings through the walls, the .glb, every cause and condition listed, the paint still |
        | raea/render   | sdk/render/src/worn.ts                  | 119   | worn paint as a surface shader on the public hook (WORN_WESL, wornShader): systems/wear's shader ported, same noise, curvature edges, grime, rust and chips in the mesh's own space |
        | raea/render   | sdk/render/src/3d/materials.tsx         | +12   | a material's `wear` becomes that shader (none at amount 0; refused beside `shader` or `grid`) |
        | deleted       | systems/wear, spawn.rs, raea_plugins.rs, runtime/scene parts.rs | -480 | raea-sys-wear, its WearPlugin, spawn branch and row columns; its type out of the registry snapshot, bevy.gen.tsx and sdk-coverage |
      And worn paint's shader lives in raea/render beside the material that takes it, not in raea/wear: raea/wear is
        built on raea/render, so the other way would be a cycle. <WearExtension> and `wear` keep their props, and the
        guides are unchanged
      And proof: `bun test` in examples/materials/wear (7 tests) and conformance bevy3d.test.tsx (wear's shader, its
        uniforms, none at amount 0, alone); a parity scene of worn paint (sharp, turned and round parts, every setting)
        rendered through systems/wear and through worn.ts matches (RMSE 3.8e-5 of full scale); every still, and
        rain-passes at frames 0, 40 and 119, render with 0 errors on the Mac. Clay now draws worn paint as it draws any custom surface shader

    @needs:WR7
    Scenario: WR8. Detail textures from mask stacks, and what they cost
      Then each class's detail (brick bond, plaster grain, stone tooling, wood grain) is a stack of procedural masks
        rendered by raea once into tiling layers, instead of noise evaluated per pixel per frame
      And texture arrays get mips (SH9 arrays have none, so a tiling layer shimmers far off) and maps can be BC
        compressed into KTX2, with the memory and frame time each saves measured
      And measured first (11-19, M5 Pro, the corner house at 4K, 121-127 frames each, plain and worn interleaved): with
        the machine quiet, 17.2 ms a frame plain and 17.2 worn, 17.6 and 17.7; under other sessions' load both rise
        together (18-75 ms). The recipes' per-pixel noise costs no frame time a render can see, so detail layers
        would save nothing yet; a live scene at 60 fps with many baked buildings is where to measure again
      And the maps' memory: four RGBA8 maps of 2048 x 1108 for the house, 36 MB on the GPU; BC7 would take that to
        about 9 MB, but causes2's alpha is the class code read exactly by textureLoad (it needs a map of its own
        before the rest compress), and an iPhone has no BC (ASTC there). Worth it for a city of baked buildings, not
        one house
      And still open, so not done: the detail layers, mips on arrays (they come with the detail layers: adding them
        now would grow the city's 40-layer room atlas by a third for no wear user), and BC/KTX2 maps with the class
        split out and a fallback where BC isn't
