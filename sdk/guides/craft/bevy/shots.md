# Shots: a built set filmed from plain directions

A shot list in a crew's words ("slow push on the counter, handheld, 35 mm") becomes camera paths that can't lurch,
checked on a framing sheet and for clearance before anything is rendered long. This is AnyShot's loop (MIT) on raea's
camera: `raea/camera` for the moves, `raea make framing` and `lenschart` for the sheets, `raea scene stage
--clearance` for the camera's path through the set. `raea make status` holds the job; the Stop hook won't let you end
before it says done.

## What to do

1. **The plan.** `make: shot({ shots: [...] })` on brief.ts. Each shot: `name` (the file it lands as),
   `description` in a crew's words, `move`, `seconds`, `lens` (mm), `handheld`, `still`, `composition` (the .tsx that
   renders it), `fps`.
2. **Moves from words** (`shotMove` from `raea/camera`; every one is a camera path, keyed in time):

   | The crew says | Move | Notes |
   |---|---|---|
   | push in, dolly in, move in | `dolly` toward the subject | stops 0.3 m short of it |
   | pull out, dolly out, reveal | `dolly` away | starts close, ends wide |
   | track left/right, crab, slide | `truck` | the look moves with it |
   | boom up/down, rise, sink | `pedestal` | straight up or down |
   | crane up over | `crane` | rises while the look holds on the subject |
   | pan, look across | `pan` | turns in place, keyed along the arc |
   | tilt up/down | `tilt` | turns in place vertically |
   | orbit, circle, arc | `orbit` | around the subject, keyed every 10 degrees or less |
   | vertigo, dolly zoom | `dollyZoom` | the lens is solved every frame to hold the subject's size |
   | walk through, follow the path | `walkPath` from the floor plan | 0.4-0.6 m/s at 1.5-1.7 m eye height |
   | lock off, hold | `static` | |

   - **Walkthroughs:** draw the path on `raea make plan`'s floor plan (x right, z away from the entrance, metres)
     and give the points to `walkPath(points, { speed: 0.5, eye: 1.6 })`. It keeps an even pace and rounds the
     corners, easing only at the start and end.
   - **Handheld:** `handheld(path, { profile: "walk" })` adds a walking operator: steps at 1.8 Hz, the aim on a
     spring that lags and settles, slow small reframes, nothing above 1.9 Hz. `"subtle"` for a held, standing camera.
   - **Focus:** `focusAt(focus, path, frame)` gives DepthOfField's `focal_distance`: `{ kind: "point", at }`,
     `{ kind: "track", target: (frame) => [x, y, z] }`, or `{ kind: "rack", from, to, at, over }`, which pulls from one
     subject to the other over `over` frames with holds either side.
   - A move is `shotMove({ kind, from, look, frames, by, angle, fov })`: `by` in metres (dolly in, truck right,
     pedestal or crane up; negative the other way), `angle` in degrees (pan right, tilt up, orbit counter-clockwise).
   - **Lens:** `lens(mm)` is the vertical field of view in degrees for a full-frame focal length. 24-35 mm for a
     walkthrough, 50 for a person's eye, 85-135 to compress a detail.
3. **Choose the lens.** `raea make lenschart SHOT` renders the shot's first frame at 18, 24, 35, 50, 85 and 135 mm
   (`review/lens-SHOT.png`). Pick from it and set the shot's `lens`.
4. **Block it.** `raea make framing` renders every shot's first, middle and last frame onto `review/framing.png`,
   stamped with the name and time. Look at every row: the subject in frame at the start and the end, nothing through
   a wall. Ask it (`raea ask review/framing.png --std make.framing.through_wall --std make.framing.subject_lost`) and
   judge it (`raea judge review --kind shot --by claude`: framing, move, focus, clearance, continuity, grade).
5. **Clearance.** `raea scene stage SHOT.tsx --clearance 0.45` holds the camera 0.45 m (a carried camera) off every solid part on every frame
   (`--clearance 0.15` for a camera on a rig). Glass, decals, volumes and parts under 5 cm are passed over.
6. **Grade.** `<Grade {...gradeLook("cinematic")} />` from `raea/post_process`: clean, cinematic, gritty or vintage,
   each a starting point to change (`gradeLook("vintage", { vignette: 0.2 })`).
7. **Finals** go to `out/shots/NAME.mp4` (or `.png` for a still), long ones on the render farm.

## Why

- A framing sheet catches in one picture what a full render shows after minutes: a subject leaving frame at the end,
  a move that starts inside a wall.
- Clearance is measured on the scene, not seen: a camera 20 cm from a door frame draws the frame's inside for a few
  frames that no one notices until the final.
- Keyed paths are paced by time, so a pan doesn't speed up at its middle and a walk doesn't stop at every corner.

## Common mistakes

- A walkthrough at 1.2 m/s: it reads as a drone. A person looking around walks 0.4-0.6 m/s.
- Handheld noise from random numbers per frame: it jitters. Use `handheld`, which has nothing above 1.9 Hz.
- A shot without its `composition`: `raea make framing` can't draw it, and `raea spec brief.ts --as make` says so.
