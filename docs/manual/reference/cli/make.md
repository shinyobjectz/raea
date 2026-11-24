---
title: "raea make"
summary: "A scene, kit or shot made to its plan: the plan stop, the kit's references, generators, pick, prep and landing, a shot list's framing and lens charts, and whether it's done, decided in code; the agent's Stop hook holds it there."
order: 44
sources: ["apps/make/src/lib.rs", "apps/make/src/status.rs", "apps/make/src/asked.rs", "apps/make/src/plan.rs", "apps/make/src/kit.rs", "apps/make/src/kitout.rs", "apps/make/src/shots.rs", "apps/make/src/floor.rs", "apps/make/src/sheet.rs", "core/brief/src/make.rs", "apps/check/make/src/lib.rs"]
---

`raea make status` reads the job on a project's brief.ts (`make: scene({...})`, `kit({...})` or `shot({...})` from
`raea/brief`) and says what's still missing, from what's on disk and never from what the agent says:

- the kind's deliverables (a scene's three or more stills; a kit's GLB, LOD1 and thumbnail per object, its lineup
  and kit.json; a shot's still or move per shot), or the plan's own `deliverables`;
- NOTES.md, ending with a "What the skill got wrong" section;
- each composition the plan names has a render or check newer than it that found no error (the project's record);
- a fresh critic (`raea judge --kind KIND`) has judged a round newer than the newest deliverable on the job's own
  kind's rubric, and each blocking defect it named is fixed or answered in NOTES.md by its name; once the judge is
  calibrated against your eye, two passes in a row as well;
- each `raea ask` question of the make set that's calibrated (`raea ask calibrate`) has been asked of the job's
  latest sheets (clay and colour, the reference sheet for a kit, the framing sheet for a shot) and didn't flag, or is
  answered in NOTES.md. A question not calibrated yet is advice: status lists what it flagged and doesn't wait on it.
  An answer counts for the sheet it was asked of, matched by the file's bytes, so a sheet rendered again is asked
  again.

`--plan` writes the gates as check lines under the job's own task in the project's plan.org, keeping the person's
mark (DONE, DROPPED) and their own lines; the Stop hook runs it so.

A job is done when all of that holds, and also at its round cap (8 by default) or when three rounds in a row haven't
beaten the best before them: then it's delivered short, with what's missing listed. raea's Stop hook (`raea hook`)
asks it at the end of every turn in a project whose brief has `make`, and keeps an agent working until it's done; the
same open items block a session three times at most. The loop it holds is in docs/make.feature.

```sh
raea make status
raea make status ~/Documents/raea/acme/projects/flight-desk-kit --json
```

## The steps

Each step reads the plan from the brief.ts at or above DIR and works beside it.

- **The plan stop** (any kind with placed objects): `raea make plan` draws `review/plan/floor-plan.png` (every
  object's footprint at its `at`, turned by its `rot`, labelled with its height; invented ones dashed), renders a
  grey-box blockout of the plan from the entrance (`blockout.png`, and its review sheets unless `--no-review`), and
  puts the first reference, the blockout and the floor plan on `comparison.png`.
- **A kit**, in order:
  - `raea make refs` makes each object's reference from the plan's style images, in its style paragraph, with the
    scan template (one object, whole, on plain grey, even light), then the reference sheet `review/refs.png`.
    `--only` remakes some, `--dry` quotes it and spends nothing.
  - `raea make gen` sends each reference to Tripo H3.1 (150 000 faces for a hero, 60 000 for a prop) and Meshy on
    fal, into `out/kit/gen/ID/` (`--tripo`, `--meshy` to change either; `--dry` quotes it).
  - `raea make pick-sheet` fits both models to the object's real size and draws each in colour and in clay beside
    the reference: `review/picks.png`. `raea make pick ID tripo|meshy "why"` records the pick in out/kit/picks.json.
  - `raea make prep` fits each pick to its real size with `raea gen check`, makes a quarter-triangle `ID_lod1.glb`
    beside it (`raea blender run blender/decimate.py`), and writes a `.glb.json` sidecar for each.
  - `raea make kit` lands the kit as a component set: the workspace's `shared/components/KIT/` (else the project's
    `components/KIT/`, or `--to`), one component per object importing its model and LOD from beside it, `lineup.tsx`
    with a 1 m bar, `cover.png`, `thumbs/` and `kit.json`; and the job's own `out/kit/` record that status counts.
- **A shot list**: `raea make framing` renders every shot's first, middle and last frame (from its `seconds` at its
  `fps`, 30 when left out) with `raea render --at` and puts them on `review/framing.png`, a row a shot, stamped with
  its name and time. `raea make lenschart SHOT` renders a shot's first frame at 18, 24, 35, 50, 85 and 135 mm
  (RAEA_LENS) onto `review/lens-SHOT.png`. Each shot needs its `composition`.

```sh
raea make plan
raea make refs --dry && raea make refs
raea make gen --only rotary_phone
raea make pick rotary_phone meshy "Tripo fused the handset to the cradle"
raea make prep && raea make kit --name flight-desk
raea make framing --only counter_push
raea make lenschart counter_push
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea make status [DIR] [--json] [--plan]   (a scene, kit or shot held to its plan, `make` on brief.ts: what's missing, each gate (renders and checks, raea ask's calibrated questions, the critic on the kind's rubric), the judge's rounds, and whether it's done, decided in code; --plan writes the gates as check lines under the job's task in plan.org; the agent's Stop hook holds it there)
raea make plan [DIR] [--no-review]   (the plan stop: review/plan/floor-plan.png, a grey-box blockout from the entrance and its review sheets, comparison.png beside reference 0)
raea make refs [DIR] [--only ID,...] [--model M] [--dry]   (a kit: every object's reference from the style images, in the plan's style, then review/refs.png; --dry quotes it)
raea make sheet refs [DIR]   (review/refs.png again: the style images, then every reference)
raea make gen [DIR] [--only ID,...] [--tripo M] [--meshy M] [--dry]   (each reference to both generators, Tripo H3.1 and Meshy on fal: out/kit/gen/ID/)
raea make pick-sheet [DIR] [--only ID,...]   (review/picks.png: reference | Tripo | Tripo clay | Meshy | Meshy clay, fitted to size)
raea make pick ID tripo|meshy "why" [DIR]   (the pick and its reason, out/kit/picks.json)
raea make prep [DIR] [--only ID,...]   (each pick fitted to its real size, a quarter-triangle _lod1 beside it, .glb.json sidecars)
raea make kit [DIR] [--name KIT] [--to FOLDER]   (the kit as a component set: shared/components/KIT, lineup.tsx, cover.png, thumbs, kit.json)
raea make framing [DIR] [--only NAME,...] [--out DIR]   (a shot list: each shot's first, middle and last frame on framing.png, stamped)
raea make lenschart SHOT [DIR] [--out DIR]   (a shot's first frame at 18, 24, 35, 50, 85 and 135 mm on lens-SHOT.png)
```

Headless: it needs nothing running and opens no window. Takes up to 5 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--dry` | – | Quote the generation (refs, gen) and make nothing. |
| `--json` | – | Print the status as JSON: missing, gates, judge, done, short and why. |
| `--meshy` | `M` | The Meshy model for `gen` (default `fal:meshy/v7.1/image-to-3d`). |
| `--model` | `M` | The image model for `refs` (default raea gen image's). |
| `--name` | `KIT` | The kit's folder name for `kit` (default the brief's directory name). |
| `--no-review` | – | `plan` without the blockout's `raea render --review` sheets. |
| `--only` | `ID,...` | Only these objects (refs, gen, pick-sheet, prep) or shots (framing), by id or name. |
| `--out` | `DIR` | Where the sheet or chart is written (default the job's review/). |
| `--plan` | – | Also write the gates as check lines under the job's task in plan.org. |
| `--to` | `FOLDER` | Where `kit` lands the components (default the workspace's shared/components/KIT). |
| `--tripo` | `M` | The Tripo model for `gen` (default `tripo:H3.1`). |

<!-- generated:end -->
