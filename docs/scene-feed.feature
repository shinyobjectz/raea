@spec
Feature: Scene: a feed item that is raea 3D looked at from inside
  Shane, 2026-11-13: "create one new feed 'scene' which allow you to look inside a raea 3d scene and a 'storyboard' of
  them or a grid of props, characters or anything you might want to inspect, comment on or anything. this is critical
  that this is a highly intentional but minimalist way to inspect a scene with a timeline unless its just static or a
  prop and be able to actually move around in the scene, make comments in 3d space, and possibly move, rotate and
  scale objects in the scene unless they are procedural to which we might need an item details panel that has
  components that are procedural show up with those props. A big part of this is being able to highlight a problem
  with the mesh, maybe how the shader looks or a clipping or artifact problem. being able to even stage the scene by
  drawing components in 3d space or adjust motion trajectories like i would a spline." Later the same day: "timeline
  should be same sdk and components we use for watch", and it is for v0.1.4.

  What there was (read 11-13): a 3D scene reached the feed only as a render (watch), a still (look) or a game (play).
  The preview pages (docs/preview.feature) had the pieces under them: the stage picks 3D parts and answers where
  points of the scene are on the picture, a free camera, a part's JSX read and set where it is written. None of it was
  a thing an agent could post for the person to walk around in, and nothing moved a part, drew one or touched motion.

  The item:
    - `scene` is a feed item type. Its entries are files the stage can open: a 3D composition's .tsx, a component's
      .tsx (a prop, a character), a model, a splat. One entry opens straight into the inspector; several are a board
      first.
    - The file is the document. Every change made in the inspector is written into the scene's own code, in the form
      it is already written in, with Undo. Nothing is kept beside it.
    - Only what matters now is on the picture: the scene, the one toolbar, and the timeline when time is part of it.
      A picked part brings its card; a tool brings its own marks. Cruft is clutter (Shane, 11-13): everything a
      person can do is a tool in the side toolbar, where they look for it, and what a thing is is said in its tooltip,
      not in words on the picture.

  Background:
    Given the app's code is apps/app (raea-feed): src/scene, src/detail/views/Scene.tsx, host/scene, samples/scene
    And the inspector is built on stage.show, stage.pick, stage.view, stage.locate, stage.seek and stage.play as
      docs/preview.feature PV1 left them, and on three calls added for it (raea-sys-hold, raea-runtime-live
      pick3d.rs, raea-page-stage): stage.part (where a part stands), stage.hold (a part drawn where a drag has it)
      and stage.wire (wireframe). On a raea without them it still works: handles from the code's numbers, no live
      drag, and Wireframe says it can't yet
    And everything the inspector knows of the camera is where points of the scene are on the picture (stage.locate):
      a plane of the scene is mapped to the picture by a homography from four of its points (src/scene/math.ts), so a
      drag is read in true metres from the scene's own camera as from a free one, whatever lens the scene wrote

  Rule: SF1. The item, its board and how it is posted
    # Status 11-13: built on raea-feed branch scene. feed_post { type: "scene", file, files?, names? } (host/acp/hub.ts
    # scenePost: each file stays where it is and is checked to be one the stage opens); `raea show` posts models, or
    # several 3D files, as a scene (host/show.ts typeOf); the built-in world has one (i-set: samples/scene room.tsx,
    # crate.tsx, lamp.tsx). Checked in Chrome (e2e/scene.spec.ts) and in raea's headless window: the card's stills, the
    # board, an entry opened, Esc back to the board and then the feed.

    @done
    Scenario: SF1a. One scene, or a board of them
      Given a scene item with one entry
      Then it opens in the inspector
      Given one with several (a storyboard's shots, a grid of props or characters)
      Then it opens on a board: each entry's still, its name, what the poster called it, and its open comments' count;
        shots are numbered in order
      And a click opens an entry, B or Esc goes back to the board, Esc again to the feed
      And the feed's card is the first four stills

    @done
    Scenario: SF1b. Posting one
      Then an agent posts it with feed_post type scene (file, and files for a board, names for what each is called)
      And `raea show` of a model, or of several 3D files, shows them as a scene
      And an item written into a project's .raea/feed with type scene and entries is drawn the same

  Rule: SF2. Looking around, with a timeline only when there is time
    # Status 11-13: built (src/scene/Inspector.tsx; the camera is an orbit about a target, math.ts turned, slid,
    # dollied, walked, framed, between). Checked in the headless window on the sample room: a drag turned the view, the
    # wheel moved out, F came back, a timed scene played and a press on the strip went to 0:01.5 with the ball mid-arc;
    # a prop alone had no strip.

    @done
    Scenario: SF2a. Moving through the scene
      When the pointer is at rest (no other tool on)
      Then a drag turns the view about what it looks at, Shift-drag slides it, the wheel moves in and out, the arrow
        keys walk along the ground the way the camera faces, Page Up and Page Down rise and sink
      And Alt-drag or the right button does the same from any tool
      And F frames the picked part, or with none goes back to the scene's own camera; a double click does too
      And the scene opens on its own camera, as its author framed it

    @done
    Scenario: SF2d. The tools' bar
      Then the side toolbar holds every tool: Look, Comment, Draw, Path, Check, Wireframe, Quality, and Board on an
        item with several entries; Move, Rotate and Scale stand in it only while a part a hand can change is picked
      And each says what it does and its key in its tooltip, as do the handles, the keys, the card's rows and buttons
        and the kinds of problem a comment can mark

    @done
    Scenario: SF2e. The view has a toolbar of its own
      # Shane, 11-13: "lets have separated toolbars for movement controls and maybe some kind of hint if movement
      # controls have a shortcut by putting it in the tooltip". src/scene/NavBar.tsx, on the stage's right edge.
      Then across from the tools is the view's bar: Orbit (1), Pan (2) and Zoom (3) set what a plain drag does, Frame
        (F) goes onto the picked part or the whole scene, and the scene's camera (0) goes back to the shot as written
      And each tooltip says its key and the gesture that does the same from any mode (Shift-drag slides, scrolling
        moves in and out, the arrows walk)

    @done
    Scenario: SF2b. The timeline is the watch view's own
      Given a scene whose composition has a length
      Then under the picture is the feed's one timeline (detail/blocks/Filmstrip.tsx, as under a render): play and the
        time on its row, the playhead, the comments' marks at their moments, a press or a drag to seek
      And its lanes are the scene's own frames, cuts and sound, from a render of the scene as it is now (host/scene
        film: kept by the file's state, made in the background a moment after it is live and after each change, not
        while this Mac is short of memory); the strip keeps one height before and after it arrives
      Given a still scene, a prop or a model
      Then there is no timeline

    @done
    Scenario: SF2c. A prop or a model alone
      Then a component opens alone under plain light from the front three-quarter (its own camera and lights when it
        brings them), framed by what is drawn; a model as its preview page shows it

  Rule: SF3. Comments in the scene's space
    # Status 11-13: built (src/scene/Pins.tsx; model/types.ts Pin.space; model/pin.ts spaceLines). A pin is a place
    # pin with `space`: the point in the scene's space, the part's stamp, a radius, what it marks, the view it was
    # written from, a path. Checked in the headless window: a comment on the table top marked Shader posted as an
    # edit, its pin stayed on the surface through a turn and a move out, and its mark stood at 0:00 on the timeline. A
    # drag marked a region as Artifact (its ring drawn on the table); opened from its mark after a seek to 0:01.5 it
    # went back to 0:00 with its thread open, and Esc closed the thread and stayed in the entry. A path of three
    # points posted as a comment.

    @done
    Scenario: SF3a. A note on a point of a surface
      When Comment is on and I click a part
      Then the feed's own composer opens beside the point, and the comment is pinned to that point of that part
      And its pin is drawn where the point is from any camera and at any frame; away from its moment it is fainter
      And the agent reads where it is in the scene's space, on which part and where that part is written

    @done
    Scenario: SF3b. What it marks
      Then the composer offers Mesh, Shader, Clipping and Artifact, and the comment carries the one chosen: the agent
        is told which kind of problem it is
      And a drag instead of a click marks a region: a ring about the point, its radius in metres

    @done
    Scenario: SF3c. A comment opened
      When a comment is opened from its pin, the notes or the timeline
      Then the scene goes to its entry and its moment, and the view moves to the one it was written from (the scene's
        own camera when it was written from that)
      And Esc closes the comment first, then the entry

    @done
    Scenario: SF3d. What raea itself measures
      When Check is pressed
      Then raea's scene lint (docs/scene.feature) runs on the frame shown and says what floats, sinks or clips

    @done
    Scenario: SF3e. Seeing what a mesh is made of
      When Wireframe is on
      Then every mesh is drawn as its edges over its shading (Bevy's wireframe pass, on the stage's scenes only: a
        render's graph never has it), through play, a seek and a change to the code

  Rule: SF4. Moving, turning and scaling a part by hand
    # Status 11-13: built (src/scene/Gizmo.tsx; host/scene/place.ts). Both forms raea scenes are written in are read
    # and written: Bevy's Transform chain (fromXyz, withRotation(Quat.fromEuler("XYZ", ...)), withScale; a lookingAt
    # kept last) and position/rotation/scale props; a part with none gets its tag's form and the imports it needs.
    # Checked in the headless window (blitz/scene.mjs): the crate's x handle dragged 66 px wrote fromXyz(2.3878, 0.35,
    # -1.2) with its rotation untouched, the stage drew it there, and Undo put the file back to the byte. By hand
    # after: the y ring turned it to Quat.fromEuler("XYZ", 0, 0.106, 0), the y handle scaled it to withScale([1,
    # 1.4656, 1]), and two Undos put the line back. The handles are elements over the picture and a drag is followed
    # on the window: raea's window draws an inline SVG as a picture, so its shapes take no pointer. host/scene: 15 tests.

    @done
    Scenario: SF4a. Handles on a part placed by numbers
      When I click a part whose position is written as numbers
      Then Move (G), Turn (R) and Scale (S) are in the toolbar, and three axes stand from where it stands: a drag
        along one moves it along that axis, the middle slides it over the ground; Turn's rings turn it about an axis,
        Scale's handles size it; Shift snaps (10 cm, 15 degrees, a tenth)
      And while it is dragged its outline and the amount show; on release the new placement is written into its JSX
        in the form it is in, and the stage shows it
      And its card has the same numbers to type over, and Undo

    @done
    Scenario: SF4c. The part follows the drag
      # checked in the built app's headless window: mid-drag the jar stood 43.2 cm along x with its card still
      # reading its old numbers, and the release wrote them
      Then while a handle is dragged the part itself is drawn where the drag has it (the stage holds it there;
        nothing is written), its outline and handles with it, and the amount beside it
      And the release writes it; a drag let go where it began writes nothing and the part goes back

    @done
    Scenario: SF4d. A part inside a group that moves or turns it
      # the sample's jar on a trolley turned 0.7 rad: a 43.25 cm drag along the world's x wrote (-0.1308, 0.65,
      # -0.2786) from (0.2, 0.65, 0), that step turned back by the trolley's turn
      Then its handles stand on it where it is drawn (the stage says where a part stands)
      And a step in the world is written as the step in the space its code counts in: raea's groups are worked into
        each part's own place before the engine sees them, so that space is read from the part's turn and size in the
        world against those its code writes

    @done
    Scenario: SF4b. A part placed by code has no handles
      Given a part whose position is computed (a frame's arithmetic, a spring, a prop, a loop's index)
      Then its card shows that code, Move is off and says why, and it is changed through its props (SF5)
      And a channel that is still numbers (a computed position with a plain scale) keeps its handle

  Rule: SF5. The details of what is picked: a procedural part's props
    # Status 11-13: built (src/scene/Details.tsx; host/scene/routes.ts read, prop, default). A pick's stamp names the
    # components it was drawn in (its uses); each use's own element is read where it is written. Checked in the
    # headless window: a cone of the sample's row showed "Drawn by Cones" with count, gap and z; count typed to 6
    # wrote <Cones count={6}> and six cones stood; the crate prop alone showed size and color.

    @done
    Scenario: SF5a. The card
      When a part is picked
      Then one card over the picture says its name, where it is written, where it stands, and its own props
      And nothing else is there until something is picked

    @done
    Scenario: SF5b. Drawn by a component
      Given a part a component draws (a row, a scattering, a rig)
      Then the card lists each component it was drawn in, nearest first, with the props it was given where it is
        used, as controls; a change is written there and the stage shows it
      And a prop looked at alone lists its own props, a change writing its default

  Rule: SF6. Staging: drawing parts into the scene
    # Status 11-13: built (host/scene/draw.ts; Inspector.tsx Draw). Checked in the headless window: a footprint
    # dragged on the floor wrote a <Mesh3d name="box"> with its Cuboid and material after the floor's element, the
    # imports added, standing on the floor; it was picked with its handles, and Remove took it out whole.

    @done
    Scenario: SF6a. A primitive laid out on a surface
      When Draw is on and I drag on a surface
      Then a box, sphere, cylinder, cone or plane is laid out on the level through that point, its size shown as it
        is dragged
      And on release it is an element in the scene's file, among the scene's own top-level parts, written as raea 3D
        is written by hand; it is picked, to be moved, sized or removed

  Rule: SF7. Motion as a path of keys
    # Status 11-13: built (src/scene/MotionPath.tsx; host/scene/motion.ts). A part's declared motion is its animate
    # tracks on x, y and z (raea/animation track, or plain keys). Checked in the headless window: the sample's ball
    # showed its path through keys 0, 45 and 90; the key at 45 dragged over the ground wrote 45: 0.8614 on x and
    # 45: 2.4329 on z and left y.

    @done
    Scenario: SF7a. Keys moved as a spline's points
      Given a picked part whose animate declares x, y or z tracks with numbers
      Then its path is drawn through the scene as the tracks travel it, with a key at each keyed frame
      And a drag slides a key over the ground under it, with Shift straight up and down; the release writes that
        frame's values; a click on a key goes to its frame; the card adds a key at the frame shown

    @done
    Scenario: SF7b. Motion that isn't keys is drawn on
      Given motion worked out in code
      Then Path draws a line through the scene (points on its surfaces), posted as a comment the agent reads as the
        points to move along

  Rule: SF9. Light on the machine, and lighter when it must be
    # Status 11-13: built (src/scene/quality.ts, useLocated.ts stir; host/scene/routes.ts pressure). Shane, 11-13:
    # "make sure its highly performant and has ways to help kick back down preview rendering if swapping or
    # something". Measured in the headless window on the sample room (the window's whole process group): 3.2 GB at
    # the feed, 3.6 GB after blitz/scene.mjs (five reopenings), 3.7 GB after eight size changes and eight seconds of
    # play. At half size a part was picked and its handles stood on it. The rules for stepping down are tested
    # (quality.test.ts); a step down from real memory pressure was not seen live. Found and fixed on the way: a status
    # read while the stage was reopening a file said "playing", and the reopening after it played on by itself
    # (src/preview/stage.ts).

    @done
    Scenario: SF9a. Quality
      Then Quality in the toolbar draws the scene at its own size, a half or a quarter: the same scene from the same
        camera, every pick, pin and handle working on it
      And on Auto it steps down by itself when this Mac is short of memory (the system's memory pressure, read by the
        host) or when the scene plays under half its frame rate for two seconds, and says why once, in a line over
        the toolbar and in the tool's tooltip

    @done
    Scenario: SF9b. Nothing runs that nothing needs
      Then what is drawn over the picture follows it closely only while something changes (the camera, the clock, a
        drag, a write being drawn) and looks in now and then while it stands still
      And nothing is outlined under the pointer while the scene plays
      And while a scene is opening the host draws no stills beside it
      And a write keeps the picture, the handles and the pins where they are while the stage opens the file again,
        on the frame it stood on

  Rule: SF10. What goes to the agent
    # Status 11-13: built (model/types.ts SceneItem.byHand; model/pin.ts byHandLines, spaceLines; store.tsx
    # sendComments). Shane, 11-13: "are we really going to count every single edit that was made? are we counting the
    # diffs?" No. model/pin.test.ts: 3 tests. The feed's send keycap (every item) reads "Send" with the count as a
    # badge: "Send 1 edit" ran together as "Send 1edit" in raea's window.

    @done
    Scenario: SF10a. Comments are the edits; changes by hand are already made
      Then the count on Send is the comments marked as edits for the agent, as on every item
      And a part moved, turned, scaled, drawn or removed, a key dragged and a prop set are written when they are made:
        never counted, never waiting
      And the next round sent carries one line a thing changed by hand, for the agent to keep, and then forgets them
      And each comment in a scene tells the agent its point in the scene's space, the part and where it is written

  Rule: SF8. Open
    # Status 11-13: SF8e done for 0.1.3 (Shane: the scene feed ships in 0.1.3, not 0.1.4). raea-feed #10 merged
    # (24b2eb5) with raea-skills #58 (faa9938: collaborate.md's row, reference/feed.md regenerated), and raea pins it
    # (#133, 2cb213634); SKILLS_STRICT=1 tools/skills.sh check passes against the app at raea-feed main.
    @done
    Scenario: SF8e. Taught to agents
      # feed_post's type list is in the generated skills reference. Teaching `scene` is a change to raea-skills
      # (skills/raea/collaborate.md's "When you need to..." table, and reference/feed.md regenerated), which is public
      # and merges by PR, with the merge of raea-feed's branch scene
      Then the raea skill says when to post a scene and what the person can do with it
