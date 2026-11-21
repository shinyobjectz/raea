@spec
Feature: Direction that holds: one world of facts, findings and rules, so a person's notes become checks and a project is built in an order that can't skip modelling, blocking or review
  Shane, 11-10, after a two-minute film went through eleven passes and was abandoned ("the video is dead in the
  water"): "i think the problem is you have kept reworking the same scenes again and again, the whole video is now
  broken ... i will have to make sure we instead of even fixing understand". Then: "take time to put all of the
  pieces together from my notes into one large spec and feature. how do we truly fix all of these at the core."

  His findings, in his words:
    1. Notes. "the inability to understand my notes as imperative thoughts that need further design. When I leave a
       note on a comment, I could be encapsulating a lot of things. Why is it that these notes become kind of like not
       seen or not heard, or when I say something in one round, it doesn't get heard in another ... The first step of
       taking these notes should be converting them to basically some sort of TypeScript test that we can validate
       against. What is it that they're asking for? Can we create a rule? Can we create a controller? How do
       controllers become rules? These are things that need to be inked into our skills."
    2. Looks. "we have an overreliance on our shaders that we wrote for sample projects in raea ... the paint and
       other looks ... are really hurting us because the point should be that you design your own shader for every
       project, for every video, or if you're saving it, you're saving it as a controller across the project, or a
       component across the project".
    3. Components. "this project was bound to fail because we didn't build a component library at the very beginning
       that was highly organized. Every prop should be a component. Every component has props that we animate. Those
       are rigs. We need to have a much more structured approach".
    4. Blocking. "blocking first was actually really good ... we got the best results when we didn't even worry about
       camera angles and we just thought about blocking ... make sure that we understand clipping and rigs within
       blocking scenes ... the computer jumps on the chair. In almost every scene, the chair and the computer would
       then be misaligned or broken, and it would be clipping through the chair. And then we wouldn't get the same
       telemetry back about clipping. Where did that telemetry go? It used to be there. Those kind of controllers
       should help with clipping. How we measure clipping, what components are attached".
    5. Medium. "I kept saying I want 2D animated text with occlusion, and you kept putting in 3D text extrusion ... if
       I want 3D text, I'll say 3D text. If I want 2D text, I'll say 2D text ... you're not over relying on oh it's a
       3D project so it's 3D text ... we are compositing layers and good content leverages 2D and 3D in composites".
       And of the lyrics: "think of how a lyric video would be and we want all lyrics like that".
    6. Sense and detail. "the desks are all facing the wrong direction and why is there kitchen props on the desk?
       That makes no sense. We need semantic layers ... we should be very focused on the taxonomies of these
       procedural elements ... we need to over detail the scenes, meaning detail that the camera might not ever see
       ... If the goal is to create a cityscape outside ... graffiti and billboards and cars ... medians and
       sidewalks and people and windows and fire escapes ... plastering a billboard over a street because I said I
       want a billboard there, it just doesn't make any sense. Things have to look good."
       Of the places it flew through: "all the worlds you keep featuring are like voxel low poly. i dont want that.
       i want highly defined and highly modeled scenes and world of varying style ... you have put zero time into
       any world modeling work i asked for."
    7. Review. "there's clearly a lack of checking any of the check systems, like how we match up, first of all
       camera angle consistency ... The visualization of a shot is not working because there's so much movement going
       on ... we're losing a lot of quality in the movement that the stills aren't showing you. Same with the audio
       since you can't listen to the audio, you're not seeing any of that."
    8. One system. "check files and controllers and the cli results and data should all be intertwined somehow. maybe
       theres a better way to use the ecs system for all of these at least in format and standard but checks should
       apply to raea as a whole not just bevy".

  What the agent on that film did, as the failure each part below is tested against:
    - It patched each note and logged a row with a still. Nothing kept checking it. Asks made in one round (2D type,
      modelled places, real meshes, not primitives) were re-read or dropped in a later one. It never restated what a
      note held, so a note holding three asks got one answered.
    - Asked for "our own shader", it added options to the stock paint look.
    - Its props were one-off meshes inside scene files, placed by loops with no idea what a prop is or where it goes.
      Fourteen "worlds" were loops of spheres, boxes and cones.
    - raea printed its clipping lines on every render and it filtered them out of every run (`grep -v clipping`).
      Its own rider-and-seat check measured the seat's angle, not whether the rider was in the chair's back.
    - It judged from single frames. A value that changes over a shot was right in every still and frozen in the
      film (a camera column the continuous render doesn't refresh): only the full render showed it.
    - It wrote eight check scripts inside the project (motion, framing, seat, light, clipping, lyric placement, frame
      jumps, cue sheet). Each had its own output; none fed another; three were left with flags unread.
    - It never heard the mix.

  What raea already has (read 11-10; each is built on, none is replaced), and the gap in each:
    | what                                   | where                                              | the gap                                                                 |
    | an entity-component world, TS or engine | sdk/app/src/ecs.ts (raea/ecs: spawn, find, relations, resources, systems; JsStore under bun) | used by games only; a composition isn't read into one |
    | a composition stepped with no engine   | runtime/export/src/fed.rs scene_frames (each frame's 3D items), runtime/export/src/scene.rs | 3D items only; the page's text and layers aren't in it |
    | an edit as rows and keyed columns      | core/program (ir, eval, timeline)                  | the renderer's own; nothing queries it for checks                        |
    | what each frame shows, as laid out     | `raea render --manifest` (runtime/export/src/lib.rs write_manifest): each text's words and box, each video's source and time, the 3D's clipping, the sounds heard, the layers | written only when asked for; read by `raea check` alone; its clipping list is a second copy of what was printed |
    | parts that clip, float, sink           | systems/contact (every render), `raea scene lint`   | findings are printed text or loose JSON `{check, level, says}`           |
    | an export's floor                      | apps/check (`raea check`: text, picture, flash, sound; `--quick`; `--taste`) | its own `Issue {rule, p, at, says}`; exports only     |
      # `raea check` reads a video through FFmpeg where it's installed and through raea's own decoders where it isn't
      # (apps/check/read; RAEA_CHECK_NATIVE=1 forces them): the same findings either way, held there by tests/parity.rs.
      # 0.1.7 refused any video without FFmpeg, though `raea media probe` and `loudness` never needed it.
    | a person's standing rules with checks  | core/brief/src/taste.rs (`Rule { text, check }`), the taste loop (docs/craft-system.md J) | six closed forms, all about an export's text and sound; a rule is proposed only when a correction repeats; nothing per project or per note |
    | notes as tasks                         | review.feature R3 (to do, working, done), a pin's `part` | "done" is the agent's word; nothing proves it or keeps it       |
    | notes mapped to a verb                 | skills/skills/raea/craft/verbs                      | one verb a note; a note that holds several asks loses the rest           |
    | a plan frozen before building          | core/brief/src/plan.rs (parts, levels, counts, `seen_from`), `raea solid info --expect` | one object at a time; not a place, not a project   |
    | rooms that know what things are        | raea/room (`layRun` roles, `faceToward` from a model's measured front, `checkLayout`, `checkDecor`) | interiors against walls only; a set built any other way gets none of it |
    | placement by relation                  | scene.feature (`<Place on>`; surfaces by name and a contact model per asset are "next") | still next                                    |
    | a shader over a whole scene            | shaders.feature SH4 (`<SurfaceShader>`)             | the skill offers `<Look mode="paint">` first (3d.md); nothing says a project should have its own |
    | every drawn thing's layer key and mask | review.feature R2, systems/layers (page built, 3D next) | made for selecting in the player; not used to mask one layer by another |
    | text behind a subject                  | goal.feature Track G: "depth-based text layering (G5, measured; not rendered yet)" | not rendered                             |
    | measures per frame, per cut, over time | goal.feature Track G's catalogue (previz columns)   | for footage edits; reported, not enforced; not run on a 3D or React piece |
    | controllers                            | core/brief/src/controller.rs, apps/workspace/src/controllers.rs | settings only (choice, list, agents, workflow); no rule, no check |
    | components registered and linted       | projects.feature PJ8 (`raea project scan`)          | a component is "a .tsx a composition imports"; it carries no meaning     |
    | the order of work                      | skills/skills/raea/craft/ROUTER.md                  | storyboard and camera map before anything is blocked; no component or blocking step |
    | the next step                          | apps/workspace/src/next.rs                          | says "render ...: stills to look at first"                               |
    | skill evals                            | skills.feature SK7                                  | no task takes notes over two rounds                                      |

  The rules this plan sets:
    1. One world. A project is read into one world of entities and components in raea/ecs's form, the same under bun
       and in the engine: its layers, text, parts, shots, cuts, cues; its notes; its rules; and every finding. A check
       is a system over that world. A rule is a resource in it. A command's result is what it added to it. Nothing in
       the format is 3D's or Bevy's: a caption, a cut and a cue are entities a check asks about as it does a mesh.
    2. One finding. Every result raea gives has one shape, is kept (in the render's manifest and the project's record),
       and can be asked for again. No command only prints.
    3. A note is an ask to be designed. It is first restated (everything it holds, its medium, what it doesn't ask
       for) and given a check that fails now. Nothing changes before that.
    4. A check is code where the ask can be measured, and a named clip the person signs where it can't; it says
       which. A row with a still is neither.
    5. Rounds hold. Every earlier round's checks run in every later round; a regression names the note it breaks.
    6. A rule climbs: a note's check, kept for the project, becomes a project rule (a line in its taste.ts); kept
       across projects, the person's taste. It is the same check at each level, moved, never rewritten.
    7. The order of work is components, blocking, look, cameras, sound, each reviewed before the next. A later stage
       is refused on a failing earlier one.
    8. Every prop is a component whose typed props are its rig, and which says what it is and where it belongs.
       Nothing is placed by code that can't read that. Modelling is a stage, with a floor, and comes first.
    9. A project's look is its own, kept in the project. raea's sample looks are examples to read.
    10. The medium of an element is the person's word for it. A piece is a composite of 2D and 3D layers.
    11. What moves is reviewed moving; what sounds is measured; what was not seen or heard is said.
    12. A finding is fixed or carried with its reason. An agent never filters raea's findings out of a run.
    13. A measure an agent can satisfy cheaply is found first: every kind of check ships with cheats that must fail.
    14. Each of these lands in the raea skill through raea-skills, and an eval shows an agent with only the skill
        follows it.

  Background:
    Given the raea skill is raea-skills' (the skills/ submodule); a change to it is a raea-skills PR and a pin bump
    And a project is a folder in raea's format (projects.feature), its rules in its own and its workspace's taste.ts
    And raea/ecs is the format: `spawn(components)`, `find({ with, without, where })`, relations as components,
      resources by name, a system `(world, frame) => void`, `world.at(frame)` the same whenever it's asked for

  # =====================================================================================================================
  Rule: Part 0. Decisions

    @done
    Scenario: DR0. The decisions this plan needs (decided 11-10; Shane can veto any line later)
      # Status 11-10: decided. Shane set the goal to build this file with "DR0 first (write my recommended decisions
      # into the file as decided; I can veto later)". Each line below is now what the later scenarios build on; a
      # veto changes the line here and the scenarios that read it, in one commit.
      Then the world is raea/ecs's (sdk/app/src/ecs.ts), grown, not a second store: the reading of a project into it
        is Rust (it reuses scene_frames and the page's layout), exposed to TS checks through the same Store calls
      And a finding's shape is `Finding { check, level: "error" | "warn" | "note", about: Entity[], frames: [a, b] |
        null, says, measure: { name, value, unit, limit } | null, fix: string | null, by: string }`, and the two
        ranks in use today map onto it (apps/check's P0 and P1 are errors, P2 a warn, P3 a note; lint's levels as
        they are)
      And a project's notes and their checks live in the project, in git: notes/<round>/<id>.ts, one file a note,
        `export default note({ words, from, pin, asks: [...] })` with each ask's check beside its words; no Markdown
      And a check is a TS function, typed from the world's components, run by `raea check` with no flag for where it
        came from; raea's own checks stay in Rust and are registered under the same names
      And a controller gains one field type, `rule`: `{ text, check, level }`, the check a name (one of raea's) with
        its values or a path to a project check; a project rule is a controller with rule fields
      # Fixed in the code (DR4): the field's words are its `label`, as every controller field's are, and its values are
      # a field of their own: `{ label, type: "rule", check, values?, level? }`.
      # 11-20 (projects.feature PJ9): controllers are gone; a rule is a taste line `{ text, check, values?, level? }`,
      # its check one of the six forms, a name or a path, in the person's, the workspace's or the project's taste.ts.
      And the taste file's six forms become six of raea's named checks (same names, same values), so
        `{ form: "text-above", value: 62 }` still reads and no taste file changes
      And the stages are recorded in .raea/project (`[stage] components = passed <sha>` ...), set by the stage's
        review, read by `raea next` and by the refusals
      And what the stock paint look gained on 11-10 (light as a gradient; worlds cut per pixel with styles; the
        camera columns it needs each frame) is taken out of systems/look into the worked example of a project look
        (DR24), not landed as part of the stock look
      And every scenario below gets its plan table (crate, file, lines, job) when it is claimed; the tables given
        here are for Parts 1 and 2, which everything else stands on

  # =====================================================================================================================
  Rule: Part 1. One world: a project read into raea/ecs, for every kind of piece (finding 8)

    @needs:DR0 @done
    Scenario: DR1. A composition's frame as a world, with no render
      # Status 11-10: built. `raea world` reads any composition's frames into raea-core-facts worlds, each entity's
      # Source from the code stamps. Real reads (release binary, M5 Pro): content/rome/film.tsx frame 900 is the 3D
      # layer (camera moving, parts measured, the caesar-bg model "not measured": raea can't read its triangles), four
      # cues with their sources, a picture at 7% and seven runs of laid-out type ("49 BC" 34 px, "The Dictator" 72 px,
      # the caption at the bottom); content/reel/edit.tsx at 314-318 shows the cut from shot noir-lamp (221-316) to the
      # next at 316; examples/games/breakout.tsx frame 60 has its 34 parts and "SCORE 10" at 60 px. Layout costs 0.33 s
      # on Rome's frame (0.39 s against 0.06 s with --no-layout) and 80 ms a frame on the reel (1.02 s against 0.21 s
      # for ten frames). Proved by 8 tests in
      # raea-runtime-world (a page read as shots, footage, cues and text in document order, a <style>'s CSS not read as
      # words; 3D as one layer with its camera's velocity and measured parts; a stamp's file, line and component; a
      # transcript's lines), 2 in raea-app-direction (one find across media; a bad query or range says what it takes)
      # and 1 in raea-page (letters set one a span read as their words). The cheat: a part's kind comes from its
      # shape, so a box its author named and propped as 3D text "SALE" is a mesh with no Text (scene.rs). Not seen:
      # content/anything/edit/scene.tsx throws its own "not a song map" error before any frame; the reel's flying
      # letters (v/build/text.tsx) are one solid a glyph, not Text3D, so they read as Parts with no Text, which is what
      # they are. Found on the way and fixed: the render's manifest joined a word set one letter a span as "4 9 B C".
      Given the plan
        | crate              | file                           | lines | job                                                                    |
        | raea-core-facts    | core/facts/src/components.rs   | 300   | an Item, a page box, a clip, a cue as components (the names and fields below) |
        | raea-core-facts    | core/facts/src/words.rs        | 40    | a transcript's words, as `raea check` reads them (its Word, moved here) |
        | raea-runtime-world | runtime/world/src/lib.rs       | 140   | a composition bundled and stepped to its frames, as a bake steps them (React only, nothing drawn), each frame read into a world |
        | raea-runtime-world | runtime/world/src/scene.rs     | 200   | the 3D: plates, the 3D layer with its camera and parts (measured from their triangles), 3D text, the 2D drawn over it |
        | raea-runtime-world | runtime/world/src/timeline.rs  | 330   | the page and timeline: layers in document order, shots (Sequences), clips, cues, lines (the project's words.json, <Speech>) |
        | raea-runtime-world | runtime/world/src/page.rs      | 60    | the page laid out with nothing drawn (BevyPage::lay_out) and read as the render's manifest reads it |
        | raea-runtime-world | runtime/world/src/source.rs    | 90    | Source from the stamps: file, line, the component named at the nearest use, the layer key |
        | raea-app-direction | apps/direction/src/world.rs    | 200   | `raea world`: the frame's world as text or JSON, or one query of it    |
      And three new crates hold Parts 1 and 2 (raea-cli and raea-runtime-export are at their 2,000-line limits, and
        raea-app-check is six lines under its own): core/facts, runtime/world, apps/direction
      And a project of any kind: a React video, a footage edit, a page, a game, a 3D scene, or several composited
      When `raea world EDIT.tsx [--composition ID] [--frame N | --at A-B|A,B] [--find JSON] [--no-layout] [--json]` runs
      Then the frame is one world: its 3D items, timeline, clips and cues read as `raea scene lint` reads a 3D frame
        today (React stepped, no engine, no GPU)
      And the page's text and boxes come from the page's layout, which before this only a render did (the manifest's
        "as laid out", which `raea check --quick` gets by rendering three stills): this scenario adds a layout-only
        pass (the render's own page synced and laid out by the main world's schedules, the render world's never run,
        so nothing is drawn); with `--no-layout`, or where the page can't be made, its text carries `needs: "render"`
      And its entities carry these components, each field measured, none typed by the author:
        | component  | on                               | fields                                                                  |
        | Source     | everything                        | file, line, the component's name and its use chain (review.feature R2a's key) |
        | Layer      | each composited layer             | medium: "2d-type" / "2d-graphic" / "3d" / "footage" / "page"; order; opacity; blend |
        | Text       | a run of type, 2D or 3D           | words, font, size px at 1080, box in the frame, extruded (depth), lines |
        | Part       | a 3D mesh or model                | component, tags, world box, transform, triangles, materials             |
        | Camera     | the frame's camera                | eye, look, lens or ortho height, near, far; its velocity to the next frame (its move) |
        | Shot       | a stretch between two cuts        | its name, from, to (a Sequence's box: <Sequence> stamps data-raea-end); the shot around it as its parent |
        | Clip       | a piece of footage on the timeline | source file and its hash, in, out, at; its words (the transcript's)     |
        | Cue        | a sound placed on the timeline    | source, at, gain, bus                                                   |
        | Line       | a sung or spoken line             | its words with their times (the project's words.json); a <Speech> line's text and start, `needs: "transcript"` for its times |
      And relations are components too (`Facts::relate`, `{ to }`): ChildOf is every entity's parent here; Rides, Holds,
        StandsOn and HangsFrom are set by DR16, Masks and MaskedBy by DR19, SungAt by DR19's lyric checks, CausedBy by DR21
      # Fixed in the code (DR1): the plan gave a Shot "its camera's move" and a Cue "its cause's entity". A world is one
      # frame, and a shot's whole move needs every frame of it: each frame's Camera carries its velocity instead, and a
      # range read (--at A-B) is the shot's move frame by frame. A sound's cause is nothing the tree says yet; DR21
      # gives a cue its cause, so CausedBy is its to set. A <Speech> line has no word times until it's transcribed.
      And a query spans media: "every Text whose Layer is 2d-type and that no Part masks during its Line" is one find
      And the same frame gives the same world whenever it's asked for, and a range of frames gives one a frame

    @needs:DR1 @done
    Scenario: DR2. What only a render knows joins the same world
      # Status 11-10: built. Every render writes its manifest beside its output (OUT.render.json, or DIR/NAME.render.json
      # for frames; --manifest still moves it): 54 KB and no measurable time on a 150-frame shot. `raea world` joins the
      # composition's newest render in the project's record (the newest whose manifest has this composition's size and
      # rate and whose output still exists), or the one `--render OUT` names; `--no-render` joins none. It adds each
      # text's Contrast against its pixels, the frame's Picture (a resource) and each shot's (the mean of its frames; a
      # shot's first frame is the cut into it, so its motion is left out), the Mix (integrated LUFS, true peak), each
      # cue's Onset, and Covers from a render with --layers (NAME.layers: the share of a layer's box that layers drawn
      # after it, neither inside nor around it, cover). raea-sys-measure measures the output with ffmpeg the first time
      # a number is asked for and keeps it in the manifest's "measured", so asking again reads it back; raea check's
      # contrast and loudness call it too. Rendered names the render and says stale when the composition changed after
      # it; Waits names each component a render would add and what it waits for.
      # Real run (release binary, M5 Pro, a scratch project: two shots, a caption a band covers from frame 30, a grey
      # line on grey, a beep at 1 s, a silent cue at 2 s): the caption 18.1:1 and the grey line 1.2:1; the band covers
      # 52% of the caption at frame 35 and nothing at frame 10; the dark shot's luma 0.112, the light one's 0.842; the
      # beep heard at 1.00 s (+0 ms), the silent cue "not heard where it starts"; the mix -27.4 LUFS. Three frames read
      # the first time, measuring, in 0.97 s; one frame read back from the manifest in 0.52-0.59 s.
      # The cheats, each fails: --no-render adds nothing and lists what waits; a composition changed after its render
      # says stale; a still rendered after the video is the newest render, so Mix and Onset wait for a video render,
      # Covers for one with --layers, and a frame it didn't draw takes nothing of another frame's; --render naming the
      # video joins it again.
      # Found and fixed on the way: `raea render --layers` recorded masks of no layer unless RAEA_SOURCE_STAMPS=1 was
      # set (the layer pass keys pixels by source stamps; fixed on main by the layer pass's lane, 43429e6f2); an onset
      # read 20 ms early on an AAC encoder's pre-echo (-111 dB out of digital silence), so a rise now counts once it is
      # within 10 dB of where the sound goes; a shot's motion counted the cut into it; --render dated the render by its
      # manifest, which a read rewrites, instead of by its output.
      # Proved by tests in raea-sys-measure (4), raea-runtime-world (12: renders 3, covers 1), raea-core-facts (8),
      # raea-app-check (27), raea-app-direction (5) and raea-sys-layers (7).
      # Not seen, or left: the test render wasn't watched or listened to; its numbers are what the measures read. What
      # the page draws over a 3D part isn't kept (NAME.layers holds only each pixel's frontmost layer), so no part is
      # ever covered. Plain elements are ordered by document order, not z-index. Light and movement are read from 64x64
      # grey frames, so motion is small for small things (a 200 px box moving 15 px a frame reads 0.001). An onset is
      # found to 10 ms. Contrast is the text against the half of its box least like it, so a caption half under a band
      # reads as its visible half; the band is what Covers says.
      Given the plan
        | crate              | file                           | lines | job                                                                    |
        | raea-sys-measure   | systems/measure/src/lib.rs     | 220   | a render's output measured (ffmpeg reads it): each frame's luminance and motion, a box's contrast against its text's colour, the mix's loudness and true peak, the onset nearest a time |
        | raea-core-facts    | core/facts/src/rendered.rs     | 90    | what a render adds to a world: Covers, Contrast, Picture, Onset; the world's resources (Mix, the render it came from, what waits for one) |
        | raea-runtime-world | runtime/world/src/renders.rs   | 240   | a composition's newest render found (the project's record, or --render), its manifest's frames joined to the frame's entities, its measures kept in its manifest |
        | raea-runtime-world | runtime/world/src/covers.rs    | 160   | NAME.layers (a render with --layers) read: how much of each layer the layers drawn over it cover this frame |
      Then every render writes its manifest (today only with --manifest), beside its output
      And the manifest is entities too, added to the frame's world when a check asks for them: the layer
        masks (review.feature R2b) as `Covers { layer, share }` between layers, each text's contrast against what is
        behind it, each shot's measured motion and luminance (goal.feature Track G's columns), the mix's loudness and
        each cue's measured onset
      And what a render adds says it needs one: a world with no render, or a frame the render didn't draw, has a
        `Waits` resource naming each component it would add and what it waits for ("a render", "a video render", "a
        render with --layers", "a render of frame N")
      # Fixed in the code (DR2): this line said "a check says whether it needs them; `raea check` on a project with no
      # render runs the rest and lists what waits for one". A check that declares `needs` is DR4's (`check({ id, text,
      # needs, level }, ...)`), and before DR4 there is no set of checks for `raea check` to run "the rest" of. DR2
      # gives the world `Waits`, the data that needs it; DR4 now holds the `raea check` half.

    @needs:DR1 @done
    Scenario: DR3. One finding, for every command
      # Status 11-10: built. One Finding (core/facts finding.rs: its check; error, warn or note; the entities it is
      # about; frames; says; measure; fix; by) is all these write: the render's clipping (systems/contact found.rs) and
      # exposure (light-meter), `raea scene lint|light`, `raea check` (--quick, --taste), `raea gen check`, `raea solid
      # info --expect`, `raea project scan`, raea/room's checkLayout and checkDecor, `raea judge`. Every render (a video,
      # --at stills, a still, --review, a .wav) says them, writes them into its manifest and counts them on its last
      # line; an error exits 1 unless `--despite CHECK WHY` (RAEA_DESPITE for harnesses that compare pixels). Each run of
      # a file in a project keeps one entry in .raea/record/: what, what it wrote and its kind, the commit (+ when the
      # file had changed), the findings, what was let through. A render that is part of another (a review's tiles, a
      # quick check's stills, solid view's scene) keeps none. `raea findings` sets each thing's latest run against the
      # one before of its kind and lists new, carried (since which run), renamed, fixed and let through; `raea next`
      # steps by it: fix what the last render found, render the video, again once the composition changed, check it,
      # polish what the check found, show it.
      # Real runs (release binary, M5 Pro, a scratch project): a crate 0.24 m into a pillar renders "error clips frames
      # 0-29 crate, pillar", exits 1 and is kept; `raea next` says to fix it; the render's own move (+0.397 m along x)
      # renders 0 errors and `raea findings` lists the clip fixed (at 9969526+); `raea next` then asks for the check,
      # and after it to show the video. `raea solid info` on a mug that misses its volume expect: one error, exit 1.
      # `raea gen check` on knot.glb: a pivot warn whose fix is the command to run, and two notes. `raea project scan`:
      # its count last.
      # DR9's cheats, run for real, each fail. --despite exits 0, and the error stays standing with its reason. With
      # stderr dropped, the last line still counts 1 error (exit 1). The same render again, copied from the export
      # cache, says the error again. The crate renamed reads "renamed (the same, about crate before: not fixed)". The
      # camera turned away keeps the clip as a note that it stands in the scene unseen, carried, not fixed (and `raea
      # scene lint` still errors). Proved by tests in raea-core-facts (8), raea-app-direction (5), raea-sys-contact
      # (13), raea-app-workspace (7), raea-app-check (27), raea-registry (6), raea-app-judge (4), raea-sys-light-meter
      # (4), raea-io-gen (25) and raea-core-review (2).
      # Found and fixed on the way: a bare file name kept nothing (its parent is ""); a clip's span left out its last
      # frame; gen check's fitted fixes were JSON; a review kept 13 entries (one a tile) and was set against videos; a
      # video copied from the export cache said nothing it had found; a review's right, top and iso cameras kept the
      # scene camera's Transform rotation, so they faced its way and those tiles were empty.
      # Not seen, or left: `raea judge` wasn't run on a real clip (a model call costs; its Findings are tested on
      # fixtures). A part scaled to nothing or deleted reads as fixed (DR16's relations know what a part rests on);
      # renamed and moved at once reads as fixed and new, though the error still stands in the count. The clipping
      # check passes an overlap under 4% of the smaller part's diagonal (DEPTH_SHARE), so the render's own move left the
      # crate 5.3 cm in the pillar, which the top view shows. The review's top tile has +x to the left.
      Given the plan
        | crate              | file                           | lines | job                                                                    |
        | raea-core-facts    | core/facts/src/finding.rs      | 240   | Finding, its JSON, its one-line text; the two old ranks and the older commands' JSON read as it |
        | raea-sys-contact   | systems/contact/src/found.rs   | 140   | the clipping check's and the lint's results as Findings about entities |
        | raea-app-check     | apps/check/src/lib.rs          | 240   | raea check's Issue as Finding in its report (rule -> check, p -> level, at -> frames) |
      # Fixed in the code (DR3): the plan gave raea check's Issue -> Finding a file of 150 lines in apps/direction. It is
      # six lines in apps/check's report, which writes check.json, and its crate had the room; a second crate would
      # have converted raea check's output after the fact instead of raea check writing Findings.
        | raea-core-facts    | core/facts/src/record.rs       | 220   | the project's record: findings by render and by commit, read back      |
      # Fixed in the code (DR3): the plan put the record in apps/direction. The render, `raea check` and `raea next`
      # all write or read it, and apps/direction brings raea-runtime-world with it; core/facts, which each already uses
      # for Finding, holds it instead, and the registry's `up` (the nearest .raea/project) moved there with it.
        | raea-app-direction | apps/direction/src/findings.rs | 160   | `raea findings`                                                        |
      Then these write Findings and nothing else: the render's clipping check, `raea scene lint|light`, `raea check`
        (and --quick, --taste), `raea gen check`, `raea solid info --expect`, `raea project scan`, raea/room's
        checkLayout and checkDecor, `raea judge`
      And each finding names the entities it is about (the two parts that clip, the text that is cut off), so
        "everything found about this part" is a query
      And every render, video or `--at` stills, writes its findings into its manifest and prints their count last,
        errors first, whatever its output is piped through; `raea render` exits 1 on an error unless `--despite`
      And .raea/record/ keeps them by render and commit; `raea findings [--about NAME] [--check ID] [--open]
        [--since RENDER] [--json]` reads them, and says what was fixed, what is new and what is carried since
      # Fixed in the code (DR3): set against "the one before" of any kind, a review's tiles read as fixed by the video
      # after it, and a part renamed read as fixed. Each run is set against the one before of its kind (video, review,
      # still, frames, sound, check); a finding gone under one name and new under another with the same check, frames
      # and measure is "renamed"; what --despite let through is listed. A clip no frame shows is said as a note, so
      # turning the camera away doesn't read as a fix.
      And `raea next` reads from it (not from a check.json's modified time, as next.rs does now)

    @needs:DR1,DR3 @done
    Scenario: DR4. A check is a system; raea's own are written the same way
      # 11-20 (projects.feature PJ9): the rules come from the taste files alone (the person's, the workspace's and now the
      # project's own); a taste line holds what a controller's rule field held, and bench/direction/proof's rules are its
      # taste.ts's lines.
      # Status 11-10: built. A rule is a line and the check that holds it (core/facts rule.rs): one of raea's checks by
      # name with its values, or a project's .ts check by its path. A controller field of type `rule` ({ label, check,
      # values?, level? }) is one; taste's six forms are six named checks with the same names and values. `raea check
      # PROJECT|EDIT.tsx` gathers every rule that applies (raea's floor, the person's taste, the workspace's and the
      # project's controllers), plans the frames each reads, reads those frames' worlds once (DR1, joined with the
      # newest render: DR2), runs raea's named checks in Rust and the project's in raea's JS host (sdk/raea check.ts,
      # imported as raea/check), and keeps the findings in the project's record (DR3). A check that needs a render and
      # has none waits, and the report says what for. A check that throws, a rule file that can't be read and a
      # check's output that can't be read are each a finding, never dropped. `raea check VIDEO.mp4` is the floor, as
      # before. The manual's raea check page has the project form and a check written out.
      # The proof, bench/direction/proof/proof.sh (release binary, M5 Pro; its media made with ffmpeg, nothing
      # downloaded): five checks that import nothing of Bevy's, each run on a piece that holds it (0 findings each) and
      # on a cheat that must fail it. The lyric cheat sets the line as 3D type 0.002 m deep, facing the camera so its
      # frame reads as flat type (seen), with nothing in front: "3d type, 0.002 m deep" and "nothing is in front of the
      # line", frames 6-54. The cut cheat cuts at 1.50 s and 2.50 s, inside "two" and "three": two errors. The mix cheat
      # places a beep that starts after 150 ms of silence: "heard 150 ms after where it is placed". The ad cheat's
      # caption overflows a fixed-height box to 89% down the frame, past the band from 80%. The ride cheat turns the
      # rider with its seat but sets it 10 cm into the seat's back: "10 cm into seat-back", as the geometry has it. And
      # a taste line (text-above 75%, passed with --taste) fails the ad cheat at 89% as the named check of its form.
      # Twelve cases, all as expected, in 18.8 s with the four renders they need.
      # Found and fixed on the way: a text's box was its element's even when its words were drawn past it (the caption
      # cheat passed: engine/page's manifest now grows the box over its lines); a word a cut passes through was in
      # neither clip's words (the cut cheat passed); a part's world box is the box around it as turned, so a rider on a
      # turned seat read as inside its back (a Part now has its `own` box before its transform); a relative
      # EDIT.tsx found no project (its parent is ""); the help's flag check refused --only, and read `[A | B]` as two
      # arguments.
      # Proved by tests in raea-app-direction (9), raea-core-facts (11: the generated TS types match the committed
      # file), raea-runtime-world (12), raea-app-check (27), raea-core-brief (11), raea-app-workspace (7), raea-page
      # (7), and the proof.
      # Not seen, or left: of the proof's renders I looked at one frame each of the lyric and its cheat; the others
      # were not watched, and the mix was not listened to (the cue's 150 ms is the onset measure). A check whose file
      # fails as its bundle loads is a finding only after the JS host's 120 s limit. A composition's newest render is
      # found by its file, size and rate, so two compositions of one file with the same size share one. The cue check
      # tests placement only: what causes a cue is DR21's CausedBy. The rider check names its parts in the rule's
      # values until DR16's Rides says it. Shane's taste files have no line with a check today, so his taste adds no
      # rule to a run until one does.
      Given the plan
        | crate              | file                           | lines | job                                                                    |
        | raea-app-direction | apps/direction/src/systems.rs  | 300   | raea's checks registered by name with their values; a project's TS checks run beside them |
        | raea-core-facts    | core/facts/src/rule.rs         | 180   | Rule: text, check (a name and values, or a path), level; taste's Check as six of them |
        | raea-app-direction | apps/direction/src/check.rs    | 320   | `raea check PROJECT|EDIT.tsx`: the rules that apply, the frames each reads, the worlds read once, the report and the record |
        | raea-app-direction | apps/direction/src/ts.rs       | 140   | a project's TS checks bundled with raea/check and run on the worlds in raea's JS host |
        | raea-core-brief    | core/brief/src/controller.rs   | 240   | the controller field type `rule`                                       |
      # Fixed in the code (DR4): the plan had systems.rs run the project's TS checks too. The command (which rules
      # apply, which frames to read) and the call into the JS host are files of their own, so systems.rs holds raea's
      # named checks alone. The field type `rule` is DR8's in its plan (rule_field.rs), but `raea check` can't run a
      # project's rules until a controller can hold one, so it is built here, in controller.rs beside the other kinds.
      And sdk/raea/src/check.ts (about 220 lines): `check({ id, text, needs, level, frames }, (world, frame, values)
        => ...)`, `found(...)`, and the components' types generated from core/facts (ts-rs, as core/brief's are)
      # Fixed in the code (DR4): the system was `(world, frame)`. A rule carries values (the caption band's 0.8, the
      # rider's part names), and a check reads them from its third argument, so one check file serves rules with
      # different limits. The types are generated to sdk/raea/src/facts.gen.ts; a test fails when they differ.
      Then `raea check [PROJECT|EDIT.tsx] [--composition ID] [--only ID] [--taste T] [--render OUT | --no-render]
        [--json]` runs every check that applies: raea's floor, the person's taste, the workspace's and the project's
        rules
      # Fixed in the code (DR4): this line had `--stage S`, "the project's notes" and VIDEO.mp4 as a third form. Stages
      # are DR10's and notes DR5's and DR6's: until then there is no stage for --stage to pick and no note to run, so
      # each joins `raea check` in its own scenario (DR6 and DR10 now say so). A video has no composition to read into a
      # world, so `raea check VIDEO.mp4` stays apps/check's floor, and a project's floor runs on its newest video render.
      # --composition picks one of a file's compositions; --render and --no-render are DR2's, as `raea world` has them.
      And a check says whether it needs a render (`needs: "render"`); on a project or composition with no render,
        `raea check` runs the checks that need none and lists each that waits, with what it waits for (DR2's Waits)
      And a check is told once what it ranges over (`frames: "all" | "cuts" | "lines" | "once" | [..]`) and is run
        there; it finds entities, measures, and calls `found(...)`; it never prints
      # Fixed in the code (DR4): "once" is added, for a check of the whole piece (the floor, loudness, a still's longest
      # hold): it runs on one world, which holds the render's resources.
      And the same check text reads for a mesh and for a caption: `world.find({ with: ["Text", "Layer"] })`
      And proved in small on 11-10 (bench/direction/proto/world.ts, plain bun, raea/ecs as it is today, no engine):
        one world holding a 2D lyric layer, a 3D rider and seat, a footage cut and a sound cue; four checks as
        systems; findings and a three-ask note as entities; six cases and two cheats, all as expected
      And the proof that it isn't 3D's: five checks that import nothing of Bevy's, each with a cheat that fails it
        | check                                                                  | on                         |
        | a lyric line is 2D type, inside the frame, masked by the subject       | a React layer over footage |
        | a cut lands on a word's end                                            | a footage edit             |
        | a cue falls within a frame of its cause                                | a mix                      |
        | a caption clears the platform's button band                            | a vertical ad              |
        | a rider turns with its seat and is in no part of it                    | a 3D scene                 |

  # =====================================================================================================================
  Rule: Part 2. Notes become asks and checks, and rounds hold (finding 1)

    @needs:DR4 @done
    Scenario: DR5. A note is restated before anything changes
      # Status 11-10: built. A note is a file, notes/<round>/n<N>.ts, whose default export is note({...}) (raea/note):
      # the person's words as a list, the first line as sent and each later one a correction they made, kept; where it
      # came from ("app ITEM cm-N", "session"); its round; its pin as the app sent it (moment, place, part); whether it
      # asks for a redesign; the asks the person corrected since; and its asks (Ask: text, medium, about, not, verb,
      # check, values, proof). Note, Ask, Pin and Proof are world facts (core/facts note.rs) and TS types
      # (facts.gen.ts); in a project's world each note is an entity with a Note. `raea notes take` writes what was sent
      # or said, all at once as one round, never twice; `raea notes ls` gives each note's state ("to restate" until it
      # has asks and none is corrected; then "to check" until each ask has a check or a clip to sign, DR6) and what its
      # restatement owes: one ask from several sentences of the note as first sent, a medium its words name that no ask
      # asks for or turns down in its `not`, a redesign whose asks keep nothing, an ask the person corrected. `raea
      # notes post` shows each restated note's asks on its comment through `raea show --on ITEM --working cm-N --ask
      # TEXT...`, and posts none that still owes something. The raea app (raea-feed 4f84878, raea.app 0.1.0-dev.1109.81)
      # shows them under the comment as "<agent> read it as:", numbered, each with Correct, and "Correct the whole
      # note"; a correction is an inbox line ({kind: "correction", comment, ask, text}) that the next `raea notes take`
      # writes into the note as a new line of its words ("on ask 2: ..."), with the ask in `corrected`. The verb is
      # per ask (Ask.verb; ROUTER.md's moves say a note with three asks is three moves).
      # Fixed in the code (DR5): "the task reads 'to restate'" is the note's state in `raea notes ls`, beside its
      # file; the app's comment task reads working with the asks once they're posted. A correction is no new sentence
      # of the note: a one-ask note corrected stayed flagged "one ask from 2 sentences" after it was restated, so
      # sentences are counted on the words as first sent. `raea notes post` keeps what it showed
      # (.raea/notes-shown.json): a second post doesn't mark a comment the agent has since settled working again, and a
      # correction taken clears its note there, so a restatement in the same words is still shown as its answer.
      # The proof, end to end in a copy of raea.app 0.1.0-dev.1109.81 run headless with its own state and a scratch
      # workspace (the release raea of this commit; nothing downloaded or spent): a still shown with `raea show`; an
      # edit comment left on it and sent through the page ("I want 2D animated text with occlusion, not 3D text
      # extrusion. Think of how a lyric video would be."); `raea notes take` wrote notes/1/n1.ts with those words. A
      # restatement of one ask with no medium got four flags from `raea notes ls` (one ask from 2 sentences; 2D, 3D and
      # extruded said and not asked or turned down); two asks with their medium and `not` owed nothing. `raea notes
      # post` put "Claude Code read it as:" with both on the comment, and a second post sent nothing. Correct on ask 2,
      # typed in the page, wrote the correction line and showed "You: ..." under it; the next take wrote it in as "on
      # ask 2: no, the whole line at once, on the beat" with corrected [2], ls said to restate it, post held it back,
      # and a second take wrote nothing. Restated, post showed the new ask 2 and the page dropped the "You:" line. I
      # looked at the page at each step (frames from its window).
      # Proved by tests in raea-core-facts (16: five for notes, and the generated TS types match) and raea-app-notes
      # (6: take once and by round, corrections once, ls's states and flags, post once and none owing, files in order,
      # a note as an entity).
      # Not seen, or left: an agent restating a note on its own (here I wrote the asks); a note pinned to a part (the
      # comment had no pin) or a moment; a note on an item as a whole, which post can only report; and the craft
      # verbs are named per ask but nothing checks one is in the set. `about` is the agent's word: the pin's part is
      # carried as the app sent it, and finding it in the world is DR6's check.
      Given the plan
        | crate              | file                           | lines | job                                                                    |
        | raea-core-facts    | core/facts/src/note.rs         | 230   | Note: words, from, round, pin, asks; Ask: text, medium, about, not, check, proof |
        | raea-app-notes     | apps/notes/src/lib.rs          | 220   | `raea notes take, ls, post`; the project's notes read; a note as an entity in the world |
        | raea-app-notes     | apps/notes/src/take.rs         | 280   | what the app sent and what was said, as note files; the person's corrections written in |
        | raea-app-notes     | apps/notes/src/post.rs         | 120   | each restated note's asks posted back on its comment in the app, once  |
      # Fixed in the code (DR5): the notes are a crate of their own, raea-app-notes (apps/notes), not apps/direction
      # notes.rs. apps/direction is 1,504 of its 2,000 lines with its tests, and DR6 and DR7 add to it; `raea notes
      # check, retire, keep` come with DR6, DR7 and DR8, in this crate, which apps/direction reads the notes through.
      And sdk/raea/src/note.ts (about 90 lines): `note({...})` for notes/<round>/<id>.ts
      And notes sent from the app (review.feature R3a: their words, their pin with its frame, place and part), or
        said in a session
      When `raea notes take` runs (the agent runs it first, on receiving them)
      Then each note is a file with the person's words verbatim and no asks yet, and the task reads "to restate"
      And the agent writes its asks: every separate thing the note holds, one ask each; the medium where the words
        name one (2D, 3D, flat, extruded, real, modelled, voxel); what it is about (the pin's part, found in the
        world, or a query); and `not`: what the note does not ask for, so a change keeps the rest
      And a note with one ask and more than one sentence of words is flagged for a second reading, and a note that
        asks for a redesign ("from scratch", "not the vibe", "redesign") is marked so: its asks say what is kept
      And the verbs (skills/skills/raea/craft/verbs) are per ask, not per note
      And the restated asks are posted back on the note in the app before work starts, where the person can correct
        one in a word; a correction is a new line of the note, kept

    @needs:DR5 @done
    Scenario: DR6. Each ask gets a check that fails now, or a clip to sign
      # Status 11-10: built. An ask names its check (one of raea's by name, or a .ts check by its path from the note's
      # folder), its values, the composition it is about (`of`) and, where it can't be measured, its proof, a clip to
      # sign. `raea notes check` runs each ask's check as a rule through `raea check`'s run (apps/direction, lent by the
      # CLI), only on the composition the ask names (Rule.of), and keeps what each ask has been through in
      # .raea/notes-checks.json: a check whose first run passes is refused (exit 1); one that fails is kept with its
      # values, and the work may change; a check changed after it failed is to be shown failing again. Each finding's
      # `by` is its ask (note n1.2), so two asks running one check are counted apart, and `raea check` runs the notes'
      # checks beside the rules, marking their findings with the ask. A clip is cut with ffmpeg from the composition's
      # newest render (the shot's frames, or the ask's), again only when the render or the frames change, and nothing
      # is cut from a render the composition changed after. The person's sign comes from the app's inbox with the
      # render's sha256 and their name when signed in; `raea notes take` keeps the last one per ask. An ask reads to
      # check, failing, to sign, passing or signed, the note its worst (to restate before any); `raea notes ls` says
      # each, and `raea notes post` sends the badge, each ask's state and each clip to sign (`raea show --working
      # --status --states`, `--sign N --clip --render`). raea/check gained a per-rule memo, `after` (run once past the
      # last frame), `named`, `turned` and `inside`; five worked checks ship in sdk/guides/craft/checks, copied into the
      # skill as craft/checks: rides, flat-type, faces, stands-on, shot-length, no-part.
      # Run on: bench/direction/asks/asks.sh with this commit's raea, every case as expected (35). Each worked check
      # passes its piece and fails every cheat (ride turned 33.2 degrees and moved 29 mm, 10 cm into the chair's back,
      # the rider gone; the lyric 2 mm deep, with nothing in front of it, its words changed; a desk turned 180 degrees,
      # the kettle on a desk, the desks renamed so neither check has anything to measure; the reveal 2.0 s, the shot
      # renamed; an orb, renamed to lamp-glass, scaled tiny). The notes' flow on a project of its own: three notes said,
      # restated, two failing first and one refused (its check named a part the office hasn't), that check fixed and
      # failing, the work changed until all pass, the reveal's clip cut (4.0 s, 120 frames), a sign kept, and the
      # shortened hold reopening it. And end to end in a copy of the raea app (0.1.0-dev.1109.87, its own state; the
      # app side is raea-feed 5fa93c7 and 1317a9d): the hold shown, "Hold the reveal longer, and let me see it."
      # written and sent in the page, taken, posted as "to restate", restated as a check and a clip, checked failing
      # and posted ("failing", ask 2 "to check"), the hold made 4 s and rendered, checked passing, the clip posted
      # under ask 2 with "Sign: this is what I asked for", the agent's --done refused ("cm-1 is a note: it closes when
      # the person retires it, not when the agent marks it done"), signed in the page (the inbox line's render matched
      # the clip's), taken and posted as passing; the hold shortened again read to check, and its new render's clip
      # came back unsigned. I looked at the page at each step and at the clip's frames: the reveal alone, no lead-in.
      # Proved by tests in raea-core-facts (16), raea-app-notes (10: post with states and clips, an ask's check failing
      # first, refused, counted apart and run alone with --only, a changed check; signs named and unnamed, a newer
      # render and a changed composition; no clip from a stale render; how an ask names its composition),
      # raea-app-direction (16), raea-runtime-world (15: an element that covers is named by its component) and
      # engine/cli/conformance/show.test.ts (5: what --status, --states and --sign send, and what's refused first).
      # Not seen, or left: an agent restating a note and choosing its check on its own (I wrote the asks); a sign with
      # a name (the copy had no account signed in); the pieces are flat colour and boxes, so no clip had motion to
      # judge; the five checks name things by name, model file or component until DR11 and DR16 give them kinds and
      # relations; and a person retiring a note is DR7's.
      Given the plan
        | crate              | file                           | lines | job                                                                    |
        | raea-app-notes     | apps/notes/src/check.rs        | 260   | `raea notes check`: each ask's check run as a rule on the work as it stands (the CLI lends apps/direction's run); a first run that passes is refused |
        | raea-app-notes     | apps/notes/src/state.rs        | 220   | what each ask has been through, in .raea/notes-checks.json: its check as it first failed, its last run, its clip and the person's sign; to check, failing, passing, to sign, signed |
        | raea-app-notes     | apps/notes/src/post.rs         | 220   | each note's state on its comment in the app, and each clip to sign     |
        | raea-app-notes     | apps/notes/src/take.rs         | 300   | the person's signs from the app: their name and the render's hash      |
        | raea-app-direction | apps/direction/src/check.rs    | 400   | the notes' asks as rules beside the others, each finding's `by` its ask; the notes' rules alone, for `raea notes check` |
        | raea-core-facts    | core/facts/src/note.rs         | 260   | an ask's composition (Ask.of): where its check runs and its clip is cut from |
        | raea-core-facts    | core/facts/src/rule.rs         | 200   | a rule held to one composition (Rule.of), as an ask's is               |
        | raea-runtime-world | runtime/world/src/covers.rs    | 180   | a plain element that covers another is named by the component it was drawn in |
        | raea-runtime-world | runtime/world/src/renders.rs   | 300   | the composition's folder kept with its render, for that element's Source |
        | raea-cli           | engine/cli/src/cli.rs          | 380   | `raea notes` lent apps/direction's run                                 |
      And engine/cli/src/show.ts: `--status S --states S1,S2` with `--working` (the note's state for the badge, and each
        ask's) and `--sign N --clip FILE --render HASH` (a clip for the person to sign under ask N); sdk/raea/src/check.ts: a check's memo, kept
        across its frames, and its `after`, run once past its last frame (a shot that runs to its end is measured, and
        a part the ask names that no frame has is found, not passed); `named(world, id, kinds)`, how a check names a
        part, a shot or a layer until DR11's taxonomy; and the worked checks, each a .ts file in
        sdk/guides/craft/checks (copied into the skill as craft/checks), with a piece that holds it and a cheat in
        bench/direction/asks, run by its asks.sh
      # Fixed in the code (DR6): the worked checks' table had two rows whose facts no world holds yet. "this is low
      # poly" is DR14's detail floor, measured at a component's `seenFrom` (DR11's meta); "the light isn't timed to the
      # cut" needs the subject's own pixels in the render, which for a 3D or footage subject is DR19's mask. Each row
      # moves to that scenario and ships with its cheat there. The other five are built on the world as it is, the
      # parts named in the ask's values (by name, model file or component) until DR11's taxonomy and DR16's relations
      # say them: the rider's offset and turn are held to the seat's as on its first frame until DR16 gives it a
      # socket, and no part of the rider is inside a part of the seat (box in box: a 3D part is in no Covers); a front
      # is the model's own axis given in the values until DR11 measures it, and StandsOn is measured from the boxes (a
      # bottom on a top, inside its footprint) until DR16 declares it; "occlusion" is a cover by the subject where the
      # render has one (a page layer), and a 3D or footage subject is DR19's.
      Then an ask's check is a system (DR4) written in the note's file or named from raea's, and `raea notes check`
        must show it failing on the work as it stands before the work may change (a check that already passes is
        refused: it isn't measuring the ask)
      And `raea check` runs the project's notes' checks with its rules (moved here from DR4, which has no notes to run)
      And each finding carries the ask that asked for it (`by`), so two asks that use one named check are told
        apart (the prototype counted both asks' findings against each until it did)
      And where the ask can't be measured, its proof is a clip: `proof: { shot | frames, sign: true }`; it passes only
        when the person marks it in the app (their name, the render's hash), and reopens when its source changes
      And the app's task badge (R3) reads: to restate, to check, failing, to sign, passing, signed; "done" is never
        typed by the agent
      # Fixed in the code (DR6): "to sign" is added: a clip waiting on the person is neither the agent's to check nor
      # passing, and the badge says whose move it is: a clip not cut yet, or cut from a render that's no longer the
      # work, is the agent's (to check), as the app copy showed when "yours to sign" sat on an ask with no clip. And an ask names its composition (Ask.of), not only its clip:
      # a project holds several, and an ask about the office's desks run on the hold's shots finds nothing there to
      # measure and passes, so it would be refused for the wrong reason. Its check runs only where it applies (Rule.of;
      # a controller's rules leave it unset and run everywhere, as before), and its clip is cut from that composition's newest render.
      And worked checks ship in the skill for the asks that recur, each with its cheat:
        | the ask, as a person says it                         | the check                                                     |
        | "the seat turns with the computer"                    | Rides: offset and turn from its socket every frame; no Covers of the rider by the seat's parts |
        | "2D text with occlusion"                              | Text.extruded is false; Layer.medium is 2d-type; MaskedBy the subject where they meet |
        | "desks face the wrong way", "kitchen props on a desk" | every Part's `meta.front` toward what it serves; no Part of kind X StandsOn kind Y           |
        | "hold this longer"                                    | the Shot is at least N seconds                                |
        | "no orbs anywhere"                                    | no Part of the named component in any frame                   |

    @needs:DR6
    Scenario: DR7. Rounds hold
      When `raea check` runs, or a render is asked for
      Then every round's notes run; one that passed in an earlier round and fails now is a regression, reported
        first, with its note's words and the commit that broke it (the record, DR3)
      And `raea render` of a composition whose project has a note to restate, to check, or regressed is refused,
        naming them, unless `--despite ID "why"`, which is written into the manifest and shown on the post
      And a note is retired only by the person (`raea notes retire ID --by NAME`, or in the app), and a redesign's
        note retires the checks it names as replaced, never silently

    @needs:DR7
    Scenario: DR8. A note's check climbs to a rule, and to taste
      Given the plan
        | crate              | file                           | lines | job                                                                    |
        | raea-app-workspace | apps/workspace/src/rules.rs    | 80    | `raea notes keep ID --as project, workspace or taste`                  |
      # Fixed in the code (DR4): the field type `rule` and reading rule fields as checks were planned here
      # (rule_field.rs, and rules.rs's first job). `raea check` needed both to run a project's rules, so DR4 built them:
      # Kind::Rule in core/brief controller.rs, and the rules read in apps/direction check.rs.
      # 11-20 (projects.feature PJ9): controllers are gone, so a kept check becomes a taste line, and the rules that
      # apply are listed by `raea check --json` and the app's rules_read, not a controller verb.
      Then `raea notes keep` moves a note's check to a line in a taste.ts (the project's, the workspace's or the
        person's), its check named by path; the note keeps a pointer; the check's text is not copied
      And the taste loop (docs/craft-system.md J) proposes this when a correction repeats, as it does now, and now
        for any check, not six forms
      And a rule's values (a limit, a list of kinds) are edited in its taste line, and the check reads them: "how do
        rules get made" is this

    @needs:DR6
    Scenario: DR9. A check that can be gamed is found before it's trusted
      Then every named check ships with cheats that must fail it, run in CI: the ask met on one frame only; the part
        hidden, renamed, scaled to nothing or moved off frame; the text made 2D by a depth of 0.001; the rider
        detached from its seat; the check's own limit edited
      And an agent may not edit a note's check after it first failed, except to tighten it; a loosened check is a
        finding against the commit

  # =====================================================================================================================
  Rule: Part 3. The order of work, enforced (findings 3, 4 and 7)

    @needs:DR3
    Scenario: DR10. Five stages, each with its review, recorded
      Then the order in the skill's craft/ROUTER.md becomes: context; shape (the brief); storyboard (what happens,
        with no cameras); components; blocking; look; cameras; sound; evidence
      And each stage closes by its review (`raea stage review components` ...), which runs that stage's checks and
        makes what a person looks at (a sheet for components, clips for the rest); the person's yes, or the checks
        alone where the brief says so, records it in .raea/project
      And `raea check --stage S` runs that stage's checks alone (moved here from DR4, which has no stages to pick)
      And `raea next` names the open stage and its first failing check, and no longer says "stills to look at first"
      And starting a later stage's work on a failing earlier one is a finding at every render, not a refusal of the
        file save: a camera can be sketched early, but nothing with a camera can be posted

  # =====================================================================================================================
  Rule: Part 4. Components with meaning, and modelling with a floor (findings 3 and 6)

    @needs:DR10
    Scenario: DR11. A project begins with its component library
      Given `raea init --components`, or a project whose brief names what it holds
      Then components/ has one file for each prop, set piece and character: a default export with typed props, the
        animatable ones declared as its rig (`rig({ open: range(0, 1), turn: angle(), ... })`), and `meta`
      And `meta` says: what it is (a path in the project's taxonomy: furniture/desk, kitchen/kettle, street/sign);
        its real size; its up and front (measured from the model as raea/room's faceToward does, not typed); what it
        stands on, hangs from or sits in; what may stand on it; its sockets (a seat, a slot, a plug); how near it is
        seen (`seenFrom`, as core/brief's Plan has it)
      And `raea project scan` (PJ8) reads it; a mesh or model built inline in a scene file is a finding ("a prop
        outside components/"), with the move as its fix
      And `raea components sheet` renders each alone: three views, its rig at rest and at each extreme, its size
        against a person; the stage's review is this sheet

    @needs:DR11
    Scenario: DR12. Placement reads what things are, anywhere, not only against a room's walls
      Given raea/room (roles, measured fronts, checkLayout) and scene.feature's surfaces by name and contact model
        per asset, built here where still "next"
      Then `place({ what, on | in | against | along, facing, count, spacing, seed })` places by relation and kind in
        any set; it refuses a pairing the taxonomy doesn't allow, and turns each thing by its measured front
      And a `populate` of many (desks down a floor, cars on a street) takes rules, not coordinates: what serves what
        (a chair its desk, a desk its aisle), what belongs on what, how many, how they vary
      And the checks, as raea's named ones: every part with a front faces what it serves; nothing stands on a kind it
        can't; nothing floats or sinks (the lint); no two of a populated kind are identical beyond a share
      And the cases: an office of forty desks, none facing a wall, no kitchenware on one; a street whose billboard is
        on a roof or a gantry built to carry it

    @needs:DR11
    Scenario: DR13. A place has a written plan of what it holds, before it's built
      Given core/brief's Plan (an object's parts, levels and counts, frozen before building)
      Then a place (a room, a street, a world) has a plan of the same kind: its zones, what each holds by taxonomy,
        the counts, and the detail that makes it that place, including what a camera may never see
      And the plan is written from what such a place has (a street: kerbs, medians, pavements, crossings, signs,
        shopfronts, windows, fire escapes, wires, bins, wear, people, traffic), frozen, and the build is checked
        against it: a planned kind with no component, or a count unmet, is a finding
      And a thing added because a note asked for it ("a billboard there") goes through the plan: where such a thing
        stands, on what, among what

    @needs:DR11
    Scenario: DR14. A detail floor, and modelling is a stage
      Then `raea components check` measures each component at its `seenFrom`: the frame share it fills, its distinct
        parts, its triangles and edge detail against that share, its surface detail (textures, or shader detail), and
        whether it is one primitive or a few; under the floor is a finding, and the stage doesn't close
      And the floor is a rule (DR8) a project can raise, never an agent
      And the worked check for a note's "this is low poly", "more detail" ships in the skill with its cheat (moved
        here from DR6, whose world had no `seenFrom`): the component passes its detail floor at its nearest
      And how a component is made follows the skill's routing (built in code when exact, a permissive library, or
        generated), and a spend or a download is the person's yes, asked once for the stage with its quote
      And the stage's time is the largest in the plan the agent gives: a brief with twelve places says so before
        any is built, and offers fewer, finished

  # =====================================================================================================================
  Rule: Part 5. Blocking first, with rigs and clipping (finding 4)

    @needs:DR11
    Scenario: DR15. A blocking composition with no cameras
      Then `blocking.tsx` is the set and the cast on their marks through the piece's time, registered as its own
        composition, drawn from fixed views (top, front, side, three-quarter), never a shot's camera
      And `raea blocking check` needs no render: the clipping check on every frame, the lint, every relation (DR16),
        every mark reached when its line or beat says
      And it answers fast enough to be run on every save: a scene of 5,000 parts in under ten seconds a frame and
        a two-minute piece in under two minutes, measured in CI. Measured 11-10: `raea scene lint --frame 27 --json`
        on one 80-frame shot of the abandoned film (a few thousand parts) had given no answer after 7 minutes at
        3.4 GB and was stopped; whether it is slow or stuck there isn't known yet, and finding out is this scenario's
        first step
      And `raea stage review blocking` makes one clip a scene from the fixed views, with findings marked on the frames
      And the cameras stage reads positions from blocking and may not move the cast

    @needs:DR15,DR4
    Scenario: DR16. A relation is declared once and measured every frame
      Then `<Rides rider seat socket facing>`, `<Holds>`, `<SitsOn>`, `<PlugsInto>` say who is on what; the rider's
        place and turn come from the socket (the component's `meta`), so they can't drift apart
      And the check: the rider's offset and angle from its socket, in millimetres and degrees, every frame it holds;
        and no part of the rider inside any part of the seat (the clipping check, asked for that pair, without the
        share and depth allowances it gives parts at rest)
      And getting on and off are part of it: a path that passes through the seat's back or arm is a finding
      And the case that failed for a whole film: a computer on a swivel chair

    @needs:DR3
    Scenario: DR17. Clipping telemetry can't go missing, and says what is attached
      Then the clipping lines are Findings in the manifest and the record (DR3), counted last on every run
      And each names both parts' components, what each is attached to (ChildOf, Rides, StandsOn), and the frames
      And a part that passes through another on purpose carries `allowOverlap="why"`: the reason is required, and
        `raea findings --carried` lists them for the person
      And the skill says, in its core: never filter raea's findings out of a command's output

  # =====================================================================================================================
  Rule: Part 6. A look of the project's own (finding 2)

    @needs:DR10
    Scenario: DR18. The project's look is a file in the project, with its props
      Given shaders.feature SH4 (`<SurfaceShader>`: one shader over every lit surface of a scene) and frame passes
      Then looks/<name>.wesl is the project's shading, and looks/<name>.ts its props: the uniforms as typed props
        (colours, ramps, widths), so a look is tuned in the app and animated like any rig
      And `<ProjectLook name>` puts it on a scene; the look stage's review is one scene from blocking under it, in a
        clip, beside the brief's references
      And `raea init` writes no `<Look mode>`; skills/skills/raea/3d.md and 3d/post.md present the frame looks as
        worked examples with their source, under "writing your project's look"
      And a stock look in a project with no look of its own is a finding at the look stage ("a sample look as the
        project's"), carried only by the person

    @needs:DR18
    Scenario: DR24. A worked project look, from what was written on 11-10
      Then the gradient light, the world cuts (a mesh drawn only inside its ball, in the main pass and the prepass)
        and the nine styles are an example project look under examples/, on SurfaceShader, with its props
      And the stock paint look is as it was before 11-10; the camera columns a continuous render refreshes each frame
        (runtime/scene/src/live.rs cam_keyed) take a look's declared uniforms, not a list of names

  # =====================================================================================================================
  Rule: Part 7. Layers, and the medium asked for (finding 5)

    @needs:DR2
    Scenario: DR19. A 2D layer masked by what is in a 3D or footage layer
      Given systems/layers (each pixel's frontmost layer) and goal.feature's G5 (a person's matte and depth)
      Then `<Layer medium="2d-type" maskedBy={["hero", "desk"]} depth={z}>` composites flat type or graphics behind
        the named parts and in front of the rest: the mask is the engine's own, exact to the pixel, for a 3D scene
        from its depth and for footage from its matte
      And raea's text module (sdk/raea/src/text.ts, grown) has the lyric-video set: a line as words, each arriving on its sung time, with the motions a
        lyric video uses (slide, wipe by a mask, scale, track), all 2D
      And an element's medium is recorded on its Layer and its Text, so a note's ask (DR5) can check it: extruded
        letters fail an ask that said 2D, flat ones fail an ask that said 3D
      And the skill's core says: the scene being 3D decides nothing about a title, a lyric or a graphic
      And with the subject's own pixels in the render, two of DR6's worked checks ship with their cheats (moved here
        from DR6, whose render had no mask of a 3D or footage subject): "2D text with occlusion" MaskedBy a 3D or
        footage subject where they meet, and "the light isn't timed to the cut": the lit share of the subject crosses
        its half within a frame of the cut

  # =====================================================================================================================
  Rule: Part 8. Review in motion and in sound (finding 7)

    @needs:DR10
    Scenario: DR20. What is reviewed is what plays
      Then `raea stage review` and `raea show` make, for each shot, a clip and a strip of every third frame; a
        single still is offered only for a thing that doesn't move
      And a still drawn alone must equal its frame in the continuous render: `raea render --at` stills are checked
        against the same frames stepped in sequence, and a difference is a finding naming the value that held
      And the measures of goal.feature Track G's catalogue run on any piece, as raea's named checks: eye trace and
        screen direction at a cut, an unmotivated jump in brightness or warmth, a subject lost from frame, a pop in a
        camera's path or lens, a hold too short to read
      And the project-local scripts the film grew (motion, framing, seat, light, frame jumps) are these checks,
        once, in raea, as systems over the world
      And a stage closes with every finding fixed or carried with a reason; a check that ran and wasn't read (its
        findings never queried by the session that ran it) is itself a finding

    @needs:DR20
    Scenario: DR21. Sound is measured against the picture, and what wasn't heard is said
      Given apps/check's sound rules (loudness, true peak, a bed against the voice, a sound that stops dead)
      Then the sound stage adds: each Cue's measured onset within a frame of its CausedBy; no Cue whose cause is no
        longer in the picture; nothing in the picture that should sound and has no Cue (from the blocking's
        impacts); the mix against the music by section
      And `raea stage review sound` posts the mix with its cues marked on a waveform under the strip
      And every post an agent makes ends with what it did not watch in motion and did not hear

    @needs:DR2,DR4 @done
    Scenario: DR28. Text on screen is timed by when it can be read, and text that can't be read is broken
      # Status 11-10: built. A render's frames are read once, in order (runtime/world pixels.rs: a video by raea's own
      # decoders, a still by its file), and each Text on each frame gets its Contrast and an Ocr (core/facts
      # rendered.rs: the words Apple Vision reads in its box, and the share of its own words among them). OCR reads the
      # box fast first; where that misses a word, it reads the text's line across the frame accurately, then the box
      # (a short word alone reads as well upside down: the manual's title example's "you" was "nof"). What it read is
      # kept in the render's manifest, so a second check reads nothing again, and a crop that is the picture last read
      # in its place but for coding noise (2 of 255 between still frames of an H.264 render) isn't read again.
      # text-read (apps/direction read.rs) runs on every composition's newest render as one of raea's own rules, holds
      # each text's reading time to the frames it can be read on, says which were lost and why (off the frame, opacity,
      # size, covered, contrast, not shown), and calls more than 3 frames in a row that can be read and that OCR
      # doesn't read broken: an error. A text OCR reads on under half its frames is a warn; what it couldn't measure
      # is a note.
      # The proof, bench/direction/read/proof.sh (release binary, M5 Pro; nothing downloaded): a caption that holds (0
      # findings) and five cheats, each 540x960 and 3 s, rendered with --layers. Faded (12% opacity): "can't be read on
      # any frame of it (frames 0-89 at 12% opacity)". Camo (the colour of its ground): "1.1:1 against what's behind
      # it". Covered (a panel over it until frame 75): "can be read for 0.5 s (frames 0-74 100% covered)". Flicker (on
      # one frame in ten): one warn, "is up 2.7 s but can be read for 0.3 s". Broken (mirrored): the error, "OCR reads
      # "pnimnom" in its box". Six cases, all as expected. The manual's title and hello examples check clean, in 2.5 s
      # and 1.8 s from no record (212 and 88 OCR reads).
      # Found and fixed on the way: the title's lone "you" (above); a covered text blamed on its contrast (covers are
      # judged first now); a flicker said as nine findings of "0.0 s" (runs a blink apart are one text).
      # Proved by tests in raea-app-direction (16), raea-runtime-world (15), raea-core-facts (11: the generated TS
      # types match), raea-app-check (27), raea-sys-vision (2), and the proof.
      # Not seen, or left: of the proof I looked at a contact sheet of each case's frames; none was watched in motion,
      # and nothing here has sound. Off the Mac, OCR waits and the report says so (Open, below). 3D type has no box in
      # the world yet, so its OCR reads the whole frame. What covers a text is known only from a render made with
      # --layers; without one, covers aren't judged and that is a note. The first checks of the title took 28-36 s
      # while other work was running on the Mac; I couldn't make that happen again (2.5 s each time since, including
      # the first check after a new build), so its cause isn't known.
      # Shane, 11-10, verbatim: "also q coupel notes we need to maintain too. we need to automatically measure reading
      # time of text on screen and note how long its actually visible including chekcign if its visible in OCR. if
      # text exists and is visible and has no ocr its broken."
      # Restated as asks (DR5's form): (1) every run of text gets the time it needs to be read, measured with no one
      # asking; (2) how long it is actually visible, which is not how long it is in the tree; (3) whether OCR reads
      # it while it is visible; (4) text that is there and visible but that OCR doesn't read is an error; (5) these
      # are kept: they run on every render, every round. Not asked: a new reading speed (craft/floor.md's 0.3 s a
      # word, a second at least, stays the check's value), a change to any piece's text, or a check of spelling.
      Given the plan
        | crate              | file                           | lines | job                                                                    |
        | raea-core-facts    | core/facts/src/rendered.rs     | 90    | Ocr: what OCR reads in a text's box on a frame of the render, and the share of its words |
        | raea-runtime-world | runtime/world/src/pixels.rs    | 240   | the render's frames decoded once, in order: a text's contrast, and what OCR reads in its box |
        | raea-app-direction | apps/direction/src/read.rs     | 270   | text-read: on, visible and read for each text; reading time against visible; visible and unread is broken |
      And a render (every render has its manifest and findings, DR2 and DR3), of 2D or 3D text (Text, either medium)
      Then each Text is given three times, in seconds: `on` (the frames it is in the world: today's "held"), `visible`
        (the frames it is in the frame, at an opacity, size and contrast against what is behind it that can be
        read, and covered by no other layer past a share, DR2's Covers) and `read` (the frames on which OCR reads its
        words in its box: Apple Vision's text recognition on the Mac, systems/vision text_lines)
      And `need` is its reading time: 0.3 s a word and at least a second, as raea check's reading rule has it, with
        its exception kept (a caption the voice says as it shows needs no time of its own)
      And "reading" is measured against `visible`, not `on`: a word faded in over half its time, or behind a part
        for some of it, has less time to be read than it is up, and the finding says which frames were lost and why
      And text that is visible on a frame and that OCR doesn't read there is broken: an error naming the Text, its
        Source and the frames; a run OCR reads on less than half its visible frames is a warn (OCR drops a word for a
        frame or two, as textread's GAP allows)
      And it is a named raea check (DR4) run on every render with no flag, so it holds every round (DR7)
      # Fixed in the code (DR28): "run on every render" is `raea check` running text-read on each composition's newest
      # render as one of raea's own rules, beside the floor, with no rule or flag asking; `raea render` itself doesn't
      # OCR its frames. A render's own findings are what its drawing knows (clipping, exposure), and every harness render
      # (P26's rows, a review's tiles) would pay for OCR it never reads. DR7 runs `raea check` with every round, which is
      # what makes it hold every round. Where text-read runs, the floor's own reading rule (held against `on`) is its,
      # so one line isn't said twice by two measures; `raea check VIDEO.mp4`, with no composition, keeps the floor's.
      And its cheats fail it: the reading time met by holding the text at an opacity no one can read; the text the
        colour of what's behind it; the text behind a part for most of its time; the text visible only on a frame
        here and there
      # Open: off the Mac, the portable OCR (systems/perceive ocr_portable) fetches its models into ~/.cache/raea;
      # that download is asked for once, with its size, before DR28 runs on Linux or Windows.

  # =====================================================================================================================
  Rule: Part 9. Into the skill, proven, then used

    @needs:DR5,DR10,DR11,DR15,DR18,DR19,DR20
    Scenario: DR22. The raea skill teaches this
      Then the core of skills/skills/raea has: the stages in order; "restate the note, then its check, then the
        work"; "the medium is the person's word"; "never filter findings"; "a project's look is its own"; "every prop
        is a component"; and each stage's commands (the flags in the generated references)
      And craft/ROUTER.md's order of work and its "a change keeps; a redesign replaces" are rewritten on
        asks and checks
      And it lands as a raea-skills PR and a pin bump; the installed skill is that checkout

    @needs:DR22
    Scenario: DR23. An agent with only the skill does it right, and can't cheat it
      Given the skill evals (skills.feature SK7)
      Then new tasks, each scored by raea's findings, not by the agent's word:
        | task                                                                             | passes when                                       |
        | five notes on a piece, one holding three asks, one saying "2D"                    | seven asks restated; each check failed first; the medium checked |
        | a second round of notes on the same piece                                         | no regression of round one                        |
        | populate an office and a street from a taxonomy                                   | DR12's checks                                     |
        | a rider onto, on and off a seat                                                   | DR16's check, on every frame                      |
        | a lyric line behind a subject                                                     | DR19's check                                      |
        | a place asked for in one sentence                                                 | its plan written first; the floor met or fewer places offered |
        | a value that changes over a shot                                                  | reviewed as a clip; the still-against-sequence check clean |
      And every cheat of DR9 fails

    @needs:DR23
    Scenario: DR25. One short piece made this way, start to finish, before any long one
      Then a thirty-second piece goes through the stages in order with a person's notes over two rounds
      And its record shows: every note restated with its asks, each check failed first, no regression, no finding
        filtered, a look of its own, every prop a component over its floor, blocking closed before a camera, and
        every review a clip
      And only then is a longer film begun

  # =====================================================================================================================
  Rule: Order, and what is deliberately not here

    Scenario: DR26. The order to build in
      Then Part 1 (DR1 to DR4) first: the world, the one finding and checks as systems are what everything reads
      And Part 2 (DR5 to DR9) second: it changes how every later piece of this plan is itself reviewed
      And DR17 and DR20's still-against-sequence check early, as they are small and each would have stopped a
        failure of the film on its own
      And DR28 straight after DR4: Shane asked for it on 11-10 as a check to keep, and it reads only Part 1
      And Parts 3 to 8 after, in their order; Part 9 as each part lands (its lines into the skill with it), the
        evals and the short piece last

    Scenario: DR27. Not in this plan
      Then no film is repaired here: the abandoned one stays as it is
      And no new kind of renderer, no new asset store, and no model or generation service is added
      And judging taste by a model is as it is (`raea judge`); here taste is the person's signature on a clip
