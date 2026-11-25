# Whole scenes: brief, build, review, judge

A believable scene is planned before it is modelled, built big to small with a measured check after every step, and
reviewed from the same fixed cameras every round by a judge that has never seen the code. This is the loop the people
who make striking Blender scenes with an agent use (the 11-06 research: dream-loop, AnyScene, kiln, prasenx,
hayashimon, KOBATAKA), each step on a raea verb.

## What to do

The brief template, the builder prompt and the judge prompt to paste are in `bevy/prompts`. A kit of props is
`bevy/kits`; shooting a finished set is `bevy/shots`.

### 0. The job: `make` on brief.ts, held by `raea make status`

- Put the job on the brief: `make: scene({ references, style, dials, objects })` from `raea/brief`. Each object has
  one size (`h`, `dims` or `max`), a `route`, `source: "reference" | "invented"` and, in a scene, `at` and `rot`
  (metres from the floor's front-left corner; rot 0 faces the entrance). Dials you don't set are read from the
  reference, so set only the ones the person asked for (`source: "customer"`).
- `raea make status` decides in code whether you're done: the deliverables on disk, every gate, the judge's rounds.
  The Stop hook asks it at the end of every turn, and you can't stop while it says "not done" (the loop gives way at
  the 8-round cap, on a stall, or when the same thing is missing three turns running). `--plan` writes the gates as
  check lines under the job's task in plan.org.
- `raea make plan` is the stop before anything costs money: `review/plan/floor-plan.png` (every footprint, invented
  ones dashed), a grey-box blockout seen from the entrance with its review sheets, and `comparison.png` beside the
  reference. Show it, fix positions and sizes, and only then generate.
- Plan each object's detail with `lod`, 0 to 4, by how large it will be seen: 0 the hero (half the frame or more),
  1 featured (20%+), 2 dressing (8%+), 3 background (2%+: a grey box reads), 4 far (a silhouette). Build to the
  plan: a 3 or 4 stays a box, a card or a quarter-triangle `_lod1`, and the budget goes to the 0s and 1s. Name the
  part as its object's id (`<Mesh3d name="kiosk">`), or put `lod={3}` on the part itself; a parent
  `<Entity transform lod={4}>` plans everything inside it. Every render then says `lod-closer` when a shot shows a
  part finer than planned (a camera move added later, a box you meant to replace and forgot), and
  `raea scene lod EDIT.tsx` lists every part's plan beside the largest the frames show it, the unplanned ones too.
  Two levels finer is an error; one holds `raea make status` until you build it up and replan it, keep the camera
  off it, or answer it in NOTES.md on a line with `lod` and its name.
- NOTES.md beside the brief ends with `## What the skill got wrong`: what the guides or tools led you into. Status
  checks it's there once the work is.

### 1. The brief: a picture, a size, and what not to add

- `brief.ts` (`brief()` from `raea/brief`) holds one place, one time, one look:
  - `look`: three to five mood words, plus the real thing it must look like ("film stills of a 1990s Tokyo
    laundromat", never "a game").
  - `references`: the target, which is a "real in-engine screenshot" of the scene from hero camera 1
    (`raea gen image "..." --aspect 16:9 -o media/ref`), and real photos of the kind of place, used for judging only.
  - `notDoing`: the do-not-add list (weather, time of day, people, UI, real brands). Without it the agent adds rain
    and neon.
  - `sections`:
    - Space: real metres. Real places are small; a shop is 3.5 x 6 m.
    - Zones.
    - Hero cameras: position, target, lens.
    - Practicals: each real light source and its kelvin.
    - Objects: one row per object, `id | what | real size m | route | faces | seen from m`.
    - Wear.
    - Printed matter.
    - Motion.
    - Budget: generation cents and judge rounds.
- Each hero object gets a `NAME.parts.ts` (`parts()`) with its concept (what, style, size, seenFrom, material) and parts, each
  with a role, a level (primary, secondary, tertiary), a parent, a count and `near` detail. Below it, add a connection
  map with one line per joint: `door <-> cabinet  hinge side  touches`.
- Write the plan from how the real thing is built: look up the product, the catalogue or a drawing.

### 2. Route every object: which tool makes which kind of thing

Ask one question of each object: **is it defined by numbers, by a rule, or by its looks?**
- **Numbers** (sizes, a hole that must fit, a hinge line): Manifold solids, or `raea/room` for the shell.
- **A rule** (a blend of bodies, a branching, a field of copies): `sdf.*`, `grow.*`, `<Instances>`.
- **Its looks** (a worn tool, a lamp, food, anything with character): `raea gen` or a CC0 library model.

The TSX is always the scene. Blender is only a workshop an asset passes through on its way into `media/`, when a
check names a script (`bevy/blender`). It never places, lights, frames, materials or animates the scene.

Every row ends as a component: `<Shelf>`, `<Kettle>`, `<Fern>`, each with its size, a fixture that renders it alone,
and its materials as jobs (`<Mat job="steel" />`), so a note can name it and the next scene can reuse it
(`raea guide components`).

| the object | made by | then in Blender when |
|---|---|---|
| **numbers**: walls, counters, shelves, boxes, frames, pipes, sign boards, anything a hinge must fit | Manifold solids (`manifold.*`, `<Mesh3d shape={solid}>`, `bevy/meshes`) | it needs real UVs or baked AO (`raea solid info` names bake.py when the file's `needs.textured` is set), a cloth falls on it (cloth.py), or a fillet solids can't make: `raea solid export` first, and log the gap |
| **numbers**: the room shell, runs of cabinets, openings, hung decor | `raea/room` (`bevy/rooms`) | never |
| **a rule**: a creature, a limb, a handle grown from a body, a melted or poured form | `sdf.*` fields with smooth unions, `.solid()` (`bevy/meshes`, "Organic shapes and growth") | it must bend (rig.py, after `raea solid export`) |
| **a rule**: too many or too organic to place by hand: trees, terrain, boulders, succulents, a vase's profile | `grow.tree`, `grow.terrain`, `grow.boulder`, `grow.rosette`, `grow.loft` | never |
| **a rule**: a lawn, a meadow, a field of copies | `<Grass>`, `<Instances>` with `scatter()` (`bevy/meshes`) | never |
| **looks**, seen close (under about 2 m) or a hero: appliances, tools, electronics, crockery, food | `raea gen` mesh, `tripo:H3.1` (`motion/gen`) | `raea gen check` names a script: over the triangle budget for `--seen` (decimate.py), debris or holes (repair.py), one blob where `--parts` names moving parts (split.py), no skeleton with `--rig` (rig.py) |
| **looks**, seen far, to be edited or rigged, or numerous | `raea gen` mesh, `tripo:P2` (quads) | the same checks; P2's quads rig better |
| **looks**: an exact real asset exists under CC0 (Poly Haven, ambientCG) | `<Model>` (`bevy/assets`) | it's FBX, OBJ or .blend (convert.py), or heavier than its distance needs (decimate.py) |
| far background | a generated picture on an `ImagePlane` | never |

- Every generated row carries its real height, and for boxy things its dims (generators invent depth):
  `raea gen mesh "..." --h 0.9 --turn 0` measures the model and fits it on the way in (`raea gen check` reports
  size, facing, pivot, pieces and triangles).
- A reference picture for each generated prop uses the scan template: *Studio product photograph of a single real
  object, photorealistic, true colours, [condition], soft even diffused light, plain pale grey seamless background,
  the whole object in frame with margin, three-quarter view from slightly above, sharp focus, no people, no text, no
  brand names, no logos.*

### 3. Build, one stage at a time; a stage that fails its check is fixed first

0. **Plan.** Every object row has a route and a real size, and every hero has a `NAME.parts.ts`.
1. **Space.** Block the room as boxes at real size. Place furniture by relations, not typed coordinates:
   `place({ room, objects, rules })` from raea/3d. Rules are `on`, `against` a wall, `beside`, `near`, `apart`,
   `facing`, `align` and `clear` (floor kept free in front). It returns each object's `center`, `base` and `yaw`,
   plus `found` for whatever couldn't hold. Check: `raea run checks.ts` with `checkLayout(plan)` prints no
   errors. Then review it in clay (step R).
2. **Generate in bulk, early.** Put every generated row in one flow:
   - Quote it first with `raea gen plan props.flow.ts`.
   - Run it in the background with `raea gen run props.flow.ts -o media/gen`.
   - Look at every reference picture before its mesh runs.
3. **Objects, big to small.** Go in this order:
   - Furniture, then heroes, then mid-size things, then clutter. Block each one out at its planned bounds first.
   - Then work each part through its levels: silhouette, then structure, then detail.
   - Build ONE of a repeated thing, check it, then pattern it.
   - After each object, run `raea scene lint EDIT.tsx --json --part ID`. It must show nothing floating, sunk or
     clipping. Apply its `{y, dy}` fixes in code, never by eye. Each joint in the connection map must touch.
4. **Light, practicals first.** Give every practical its real kelvin, and add fills last, weak.
   - Check with `raea scene light EDIT.tsx --part HERO --json`.
   - Read the EXPOSURE line `raea render` prints for every still. It gives mean luma, the 1st and 99th percentiles,
     what clips and what crushes, and the stops from mid grey.
5. **Surfaces.** Give every material one wear recipe, at different strengths: chipped edges, dirt in gaps, dust on
   tops, grime at the floor line (`ProceduralExtension`'s `dust`, `dirt` and `edges`, or `WearExtension` on paint; see `bevy/materials`). Leave nothing a
   flat default colour. Lay out printed matter as a 2D composition, render it to a PNG, use the PNG as the map, then
   age it.
6. **Motion, last and small.** First `raea scene stage EDIT.tsx`: it steps every frame without drawing and holds it
   to the checks the render fails on (3D words too small or turned too far to read, a part that snaps, a camera that
   lurches or goes in and back out, frames with almost nothing on screen). A sign nobody is meant to read takes
   `read={false}`. Then `raea render EDIT.tsx -o out/scene.mp4` and `raea check out/scene.mp4`.
   - Once things move, check how they move. Render with `--layers`, and render `--aov -o out/scene.aov` beside it for
     depth. Then run `raea fingerprint things out/scene.mp4 --json > things.json` and
     `raea check out/scene.mp4 --things things.json`. It finds a thing floating or passing through another, the
     camera lurching mid-move, and an empty frame.

### R. Review after every stage and every fix

- Run `raea render EDIT.tsx --review review/rN`. It renders the same fixed cameras every round, computed from the
  scene's bounds and written to `review/rN/cameras.json`: hero (the scene's own camera), front, right, top, iso, and
  low at 0.3 m.
- Each camera is rendered in colour and in clay (every material light grey, so form and assembly are judged without
  surfaces).
- The cameras are stitched into `colour.png` and `clay.png`, with a JSON sidecar of the tiles and their exposure.
  `--clay` alone renders any frame in clay.
- Open the sheets and look at every tile yourself. "Looks good" is not a review. Name each problem with its tile and
  its fix, for example "the dryer doors are flat grey discs".
- Hand the sheets to the judge (below). Fix every blocking defect it names, or write down why it is wrong and what
  you checked.
- Change one thing per material per round. Re-run the stage's checks after every fix.

### The judge, and the gates

Three tiers, each gating once it has earned it (`raea make status` lists them):

1. **Measured checks always gate:** `raea scene lint` (floats and sinks at the feet, clips with the move that clears
   them, faces z-fighting in one plane), `raea gen check` on every generated model, `raea scene stage --clearance 0.45` on
   a moving camera, and the deliverables' counts.
2. **`raea ask` questions gate one at a time, once calibrated.** The make set (`raea ask SHEET --set make`, or the
   command status prints) asks yes/no about one sheet: floating, intersecting, blobby, a stand-in, flat colour,
   plastic, opaque glass, garbled hero text, a real brand, too dark or bright; on a reference sheet, an outlier, two
   objects, a brand; on a framing sheet, a camera through a wall or a subject lost. Every answer lands in the ask
   ledger, matched to the sheet's bytes, so a re-rendered sheet needs asking again. A question gates once
   `raea ask calibrate` passes it (16 problem and 16 clean labels, held-out AUC 0.8); until then it's advice in the
   status.
3. **A fresh critic for the fixes.** `raea judge review/rN --kind scene --by claude --brief brief.ts --target
   media/ref/target.png --json` (or clef, kimi) scores the kind's rubric (scene: layout, props, signage, materials,
   light, atmosphere, plausibility, composition; layout and props judged on clay), on a production bar: don't
   nitpick, at most five fixes. It sees the sheets and the brief's words only, never the code or your log. Code
   decides the pass; the thresholds aren't in the prompt. A round judged on another kind's rubric doesn't count.
   Fix every blocking defect it names, or answer it in NOTES.md (its words, then what you checked). Its score gates
   once `raea judge calibrate --kind scene` agrees with Shane's eye (`raea judge eye`).

### Stopping

`raea make status` says it; you don't decide it.

- **Done:** every deliverable on disk, every gate clear, and two fresh critic passes in a row on the latest renders.
- **Stall:** three rounds without beating the best score, or the same defect named twice. Stop tweaking and change
  the approach for that part: another route (solid, field, Tripo HD, Meshy), a rebuilt light plan, or a moved camera.
  A defect that comes back after two fixes belongs to the method, not the dose.
- **Cap:** 8 critic rounds a job. At the cap or a stall the job is delivered short: status lists what's still
  missing, and the Stop hook lets you end with it said.

## Why

- Fixed cameras and a clay pass are what every strong source judges on. Colour hides bad form, and a moving camera
  hides regressions.
- A judge that has seen the code grades the intent, not the pixels. A judge that knows the pass line gets gamed.
- Measured checks (lint, light, exposure, gen check) catch what eyes miss: a 3 cm float, a fridge at 4 m tall, a
  frame two stops under.

## Common mistakes

- Modelling the hero from primitives when it has character: route it to Tripo HD, or to a field if it is organic.
- Placing generated models by nudging transforms: fit them on the way in (`--h`, `--turn`), then lint.
- Several fixes to light and materials at once: they are coupled, so do one a round.
- Calling a stage done because the command ran: every claim needs an image or a check's output.
- Raising a part's `lod` plan to silence `lod-closer` without building it up: the plan is what you built it for.
