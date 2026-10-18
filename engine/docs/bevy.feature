Feature: Bevy is the only engine; React runs on jitless V8 inside it and renders as Bevy entities and systems
  Set as the goal by the user on 2026-10-15: no second engine beside Bevy. Bevy is the scheduler, the store and the
  renderer. V8 is a component inside it (Bevy is not a JS interpreter; React needs one). Blitz's pieces (Stylo, Taffy,
  Parley, Vello) become libraries that Bevy systems call, not an engine with its own document and loop.
  The decision on the JS engine itself is engine/docs/js.feature (V8, jitless everywhere).

  Background: where things stood on 2026-10-15
    # Two engines: Blitz (BaseDocument + Stylo + Taffy + Parley + Vello, its own frame loop) and Bevy (rdb's rows,
    # clips, 3D, rules, the GPU and CPU direct paths). Bevy composites a picture Blitz paints.
    # Already bridged: every 2D row is a Bevy entity holding its Blitz node (world.rs install_hosted, DomNode).
    # Prior art: bevy-react (MIT/Apache, github.com/tulustul/bevy-react): React on deno_core's V8 drives bevy_ui
    # through a commit-batched op protocol. Its Rust crate can't link with ours (deno_core 0.404 needs v8 ^149; we
    # pin v8 =152.2.0), but its JS side (npm bevy-react 0.8.1: react 19 + react-reconciler 0.33) looks up its five
    # host calls on globalThis.__bevyHost before Deno.core.ops: op_flush(json), op_emit, op_request, op_animate,
    # op_next_event. Our V8 runtime provides __bevyHost; their reconciler and protocol run unchanged.

  Scenario: B1. One JS engine
    # Done 2026-10-15: measured on iOS (+2.6 MB compressed, js.feature), the app and engine default to V8, Boa's bindings
    # deleted; HostCore.xcframework rebuilt with jitless V8 for iPhone, Simulator and Mac (1.4 GB; V8 symbols, no Boa)
    Then V8 (rusty_v8 =152.2.0) is the only JS engine, jitless, and Boa's bindings are gone

  Scenario: B2. V8 is a Bevy plugin; Bevy's schedule is the only frame loop
    # Spike done 2026-10-15 (raea/react, examples/spike.rs): React 19 with bevy-react's reconciler and bridge
    # (npm 0.8.1, unchanged) on our jitless V8 (=152.2.0) on its own thread, __bevyHost from our prelude; a headless
    # Bevy app got 1 commit, 31 ops, 16 entities in 156 ms, the tree exactly as the JSX (div/span/i/#text, mesh,
    # light, props as components, ChildOf hierarchy). Events Bevy->JS done the same day: ReactOutbox.click(node) ->
    # op_next_event -> the button's onClick -> setState -> React's update ops -> the entity's text "count 0" -> "count 1",
    # 4.2 ms round trip.
    # Blitz's document in Bevy's schedule, first cut done the same day (engine/src/stage.rs): a Composition is a non-Send
    # resource of a Bevy World and a frame is one run of the schedule, seek system then paint system (Composition::render
    # split into seek_frame + paint). `raea render`, `raea snapshot` and the C API's render workers drive the schedule.
    # Parity through it: 107 exact, 61 ok, 2 fail, the same as before. Still to come here: the text-box and scrub paths,
    # and the React plugin and the composition in one App (one World, one schedule)
    Given a Bevy App with the ReactPlugin
    Then a JS thread owns the isolate and runs the bundle (vendor: react, react-reconciler, the bridge; app: the
      composition), and __bevyHost's op_flush hands each commit's op batch to the World over a channel
    And a Bevy system drains the batches each frame, in order, before layout
    And Blitz's document is a Bevy resource stepped by that schedule (no loop of its own)

  Scenario: B3. React's host elements are entities; style, layout, text and paint are systems
    Given the op protocol (Reset, Create, CreateText, CreateTextSpan, Append, Insert, Remove, Update with prop deltas)
    Then Create spawns an entity (ChildOf for the tree), its kind and props as components
    And DOM kinds (div, span, img, svg, ...) get computed style from Stylo, run as a system over changed entities
    And Taffy lays out, Parley shapes text and Vello paints, each a system over what changed
    And the first cut mirrors the ops into entities beside Blitz's document (checked equal), then the entities
      become the source of truth and Blitz's BaseDocument retires (B7)
    # The real risk: Stylo walks a tree through its own TNode/TElement traits, in parallel. Spike it first.
    # Mirror checked equal, 2026-10-15 (raea/react examples/mirror.rs, engine/src/dom.rs): the same App (js/src/app.tsx)
    # through bevy-react's ops -> entities + a DomMirror (raea::dom, Blitz behind it) vs react-dom in today's engine
    # (raea snapshot): SSIM 0.9998 at 540x960; 1 commit, 33 ops, 17 entities; the mirrored page paints in 177 ms
    # (first paint, fonts cold). A click round-trips (onClick -> setState -> update ops -> mirror) and repaints
    # "count 0" -> "count 1". Props map like react-dom: className -> class, style objects -> CSS text (px on numbers
    # but unitless ones), handlers/children/key skipped; mesh/light/camera/text3d stay entities only.
    # Systems, the same day (raea/react DomPlugin; blitz-dom's resolve split into resolve_style + resolve_boxes, and
    # raea::dom's style/layout/raster): every DOM entity carries DomNode (its node in Blitz's arena, which is the style
    # and layout storage behind the entities), and three chained Bevy systems run on the frames React changed the
    # page: Stylo (style), Taffy + Parley (layout and text), Vello (raster). Blitz's damage flags keep each to the
    # changed subtrees. Measured on the 540x960 App (M4):
    #   system                 first frame   after a click (one button's text)
    #   style (Stylo)          0.98 ms       0.01 ms
    #   layout + text          44.80 ms      0.12 ms   (first frame loads the fonts)
    #   raster (Vello CPU)     3.77 ms       1.53 ms
    # The picture is unchanged (SSIM 1.000 against the first cut). Why not Stylo over ECS queries: Stylo's parallel
    # traversal walks a tree through its own TNode/TElement traits; the arena stays that tree, keyed from the entities,
    # rather than a port of Stylo onto Bevy queries for no measured gain

  Scenario: B4. 3D is native
    Then host components <mesh>, <light>, <camera>, <text3d> spawn Bevy 3D entities from props
    # Geometry: the user has used Manifold (CSG: union/difference/intersection, Apache-2.0) for modelling before; a
    # <mesh> built from Manifold operations gives 3D UI real shapes (rounded panels, cut-outs, extruded text, bevels)
    And a DOM subtree can sit on a surface in 3D (painted once to a texture, transformed by the GPU), so text under a
      changing transform doesn't re-rasterise (jetway-16's wobble: 0.146 px on text vs 0.012 on boxes)
    And a JetFlow-style 3D UI scene renders end to end
    # Done 2026-10-15 (raea/react js/src/scene.tsx, src/lower.rs, examples/scene.rs; rdb's `panel` kind): React's
    # <camera>, <light>, <mesh shape>, <surface> (DOM inside) and <text3d> lower to 8 rdb rows written as rrl facts
    # (499 ms, the surfaces' DOM painted by raea::dom into PNGs); motion is rrl rules over those rows; `rrl render`
    # draws it with Bevy: lit meshes, shadows, two UI cards and a 3D headline. A panel is a quad textured with its
    # picture: premultiplied in linear light, a full mip chain, trilinear and 16x anisotropic sampling, unlit.
    # Wobble, measured against ground truth (engine/bench/wobble/wobble.sh, 2026-10-15): the panel's flight card drifting
    # 0.6 px/frame, rendered at 1x and at 4x; the 4x box-filtered to 1x in linear light is the ideal; each glyph's
    # centroid in the render minus the same glyph's in the ideal, over 21 frames; the spread of that error is wobble:
    #   panel text (3D, the card's picture)   median 0.039 px, p95 0.112   (41 glyphs)
    #   2D engine text, the same words         median 0.042 px, p95 0.119   (52 glyphs)
    # Panel text is as steady as 2D text and within a few hundredths of a pixel of a 16x-supersampled render: no wobble.
    # The earlier numbers here (panel text 0.098, bar 0.051, cube 0.019) used the whole frame's ink centroid, which is
    # not a wobble measure: every stripe of a test panel stepped an even 0.60 px, but MSAA flickered an edge stripe's
    # brightness 4%, and 230 px from the centre that moves the whole-frame centroid 0.5 px. Picture resolution (1024,
    # 512, 400 px), anisotropy and the NV12/RGBA readback change nothing; a plain cube steps evenly (+-0.01 px).
    # The 2D wobble's cause: text runs come from Skia/CoreText glyph masks (as Chrome) whose subpixel x is quantised;
    # Chrome moves an animated element's layer as a bitmap instead. A fix must decide from the document (an animated
    # transform), never from the previous frame, so a frame stays a function of its rows

  Scenario: B5. The agent writes React's structure into an edit, as rrl
    # The goal's wording was "a React snippet"; after B5a it means a component and stack rules written in rrl, whose
    # rows go through the same entities and systems as React's host elements
    Then an edit's rrl snippet makes a component's rows, `ugc rdb render` renders them, and a CJ eval passes with it
    # Syntax agreed with rrl's owners (rowlio-44 / an agent session), not invented here

  Scenario: B5a. React's semantics, written as rrl and briefs; nobody writes React (decided by the founder, 2026-10-15)
    # Supersedes both earlier proposals (the React program owning the edit; JSX blocks as sugar inside snippets).
    # There are two surfaces and only two: .rrl (Datalog: facts, keys, rules, expects) and .feature (the brief, in
    # words). No .tsx, no JSX, no angle brackets anywhere an agent or a person writes. React is the semantics and the
    # runtime model under them, not a syntax.
    Given the edit is the ECS rows, the source is the applied .rrl snippets plus the .feature brief, and the move
      log records applied snippets
    Then React's ideas are rrl relations, most of which rrl already has:
      # element            a row: kind, key, columns            card hero {text: "Sale"}
      # children           the parent column (`in`)             caption c1 in hero {...}
      # list + key         `stack` over a set, keyed by it      stack card(P) in shelf {text: P.title} if product P
      # props reuse        a style bundle                       style cap {font: "Avenir Next", size: 76}
      # conditional render a rule's condition                   ... if product P, P.price <= 50
      # reconciliation     rrl apply: a fact replaces its row; a rule's rows are re-derived and diffed by key
      # component          `define`: a named, parameterised group of stack rules (an agent session's proposal, below)
      # state and effects  none in an edit; behaviour is rules over time and K kernels
    And the brief says the same things in words ("each product has a card on the shelf; no card overflows the
      frame"), checked against the rows like any expect, never edited by the agent
    And structure is a coarse move (a stack rule or a component makes a subtree) and tuning is a surgical move (one
      fact or one labelled rule replaced); both are lines in the move log. A rule's rows own only the columns it
      states, so later keys on them (card/42.color = ...) are overrides that survive the rule being re-derived
      # That is rrl apply today (an agent session, from lower.rs): the static stratum is re-derived from scratch on every
      # apply, in file order; derived facts mint ids (card(P) -> card/42) and overrides patch rows in place.
    And minted ids are keyed by the set's ids (card/<product id>), never by position, or overrides land on the wrong row
    And a plain fact still means the whole row ("a fact lists every column its row keeps"): that's how a row is reset
    And when a key leaves the derivation (a product removed in a feedback round), its overrides are orphaned: today an
      error ("an override needs its row"); to be a loud warning listing the orphaned overrides and their move ids, so
      one removed product doesn't block the whole apply
    And components are `define` (proposed by an agent session, canon only after it's measured):
      # define card(P, In) {
      #   stack card(P) in In {w: 320}
      #   title T(P) in card(P) {text: P.name}
      #   image I(P) in card(P) {src: P.photo}
      # }
      # card(P, shelf) if product P
      # `define` expands at apply; the expanded rules land in the move log under the call's label, so `drop` and
      # `replace` work on the call; inner ids are prefixed by the call (card/.../title) and overrides target those;
      # `use` stays file import, its alias a namespace. Measured first: an overnight-harness scenario family building
      # repeated parts with `define` vs plain stack rules (pass rate, quiet regressions)
    And per-frame motion stays in rules and K kernels (0.175 ms/frame for 10k rows), never a re-render per frame:
      pop: C.scale = 1 + 0.1 * sin(local_frame(C) * 0.3) if card C {class: hero}
    And the React runtime (raea/react: bevy-react's reconciler on V8 in Bevy) is engine machinery only: it renders
      React/HTML compositions that arrive from outside (the parity corpus, imported pages) and interactive UI the
      engine ships; it is never a surface an agent writes
    And it's judged by evals: agents writing rrl + briefs on the CJ scenarios (refusals, moves, checks)
    # Why rrl and not React: Datalog can say everything React says (a keyed tree of elements and props at an instant)
    # and React can't say what Datalog says (rules over time, constraints over sets, checks). React is the model.

  Scenario: B5b. .rrl makes rows, .feature checks them (how the two surfaces meet; the user asked, 2026-10-15)
    # Today (rrl/src/lib.rs, rdb::gherkin): the brief is lowered through steps.sql, a vocabulary of phrases ->
    # (row kind, filter, holds, why), into checks over the same rows; every apply runs it; a move that breaks it is
    # refused; an agent may add a scenario only if every step is one the vocabulary knows.
    # In React's terms: .rrl is the component code, .feature the acceptance test, the steps its queries.
    Given an edit (.rrl) and its brief (.feature, beside it, with its steps)
    Then only rrl makes or changes rows: facts, stack, define, rules, and the agent's own expects
    And the brief only checks rows; words never become rows (two authors of a row would break the move log)
    And the step vocabulary is written in rrl, not SQL (the founder, 2026-10-15): a step is a phrase bound to an rrl
      condition, lowered like any expect (rrl -> IR -> SQL; SQL is the execution layer, as WGSL is under K):
      # step given "the captions": caption C
      # step then "their text is at least {px}px": C.size >= px  because "too small to read on a phone"
      # step then "each product has a card on the shelf": card(P) in shelf for every product P
      # steps.sql predates rrl (rdb's Gherkin came first); there are 3 example vocabularies (45 steps) and the
      # overnight harness's shared 45-step one to port
    And steps get the same schema check and name suggestions (near_name) as expects: a mistyped column fails at
      load with a "did you mean", never a check that is silently false (an agent session; the syntax is mine to draft, theirs to review)
    And the vocabulary grows from rrl's schema: each kind or define gives a Given ("the cards"), each column a Then
      ("their {column} is ..."), so a new component is speakable in a brief without hand-written step SQL
    And steps gain set-level checks for structure: "each product has a card on the shelf", "there are {n} cards"
      (existence and counts over a set, not one row at a time)

  Scenario: B5d. Imported React components: fixed parts an edit places and drives (the founder, 2026-10-15)
    # Amends B5a: nobody authors React in an edit, but React that already exists (a client's site, a brand's
    # components, libraries such as Magic UI or shadcn/ui) lowers into rdb as a component, so nothing is reinvented.
    Given a component imported once into a project (a package such as Magic UI's marquee, or a file from a codebase)
    Then it's bundled at import (bun) and stored by its hash like media; neither the edit nor the agent changes it
    And rrl names it as a kind: `use "magicui/marquee" as marquee`, then `marquee m {items: ..., from: 0, to: 90}`
      places one; its row's columns are the component's props, and rdb owns where, when and with what props
    And it renders through raea/react (bevy-react's reconciler on jitless V8 in Bevy): its host elements become
      entities under the row's entity (B3's path; the mirror matches react-dom at SSIM 0.9998)
    And rules and keys may drive its props; each change is a React re-render (2000 cards re-rendered: 86 ms each
      on V8 jitless), so per-frame motion still belongs in rrl rules and K over the row, not in props
    And Tailwind classes compile at import: the classes React actually emits (className in the ops) go through
      Tailwind v4's own compiler, run on our V8, into CSS cached by the class set's hash; the engine already renders
      Tailwind v4 CSS (parity react/shadcn, whose gap is fonts, not CSS)
    And a component's own animation (Motion/framer-motion, CSS transitions, requestAnimationFrame) runs on the
      edit's frame clock, never the wall clock, so a frame renders the same every time
    And the first proof is Magic UI (marquee, number ticker, shimmer button, animated beam) and shadcn's card and
      button, each checked against react-dom in the engine, then placed and checked in an edit

  Scenario: B5f. Designed parts authored as React, the edit in rrl (an experiment the user opened, 2026-10-15)
    # Reopens B5a as an experiment, not a decision: an agent session compares four procedures (rrl rows, plan first, an rrl
    # kit, React parts) on the same real-footage scenarios with the strict judge
    Given a component file (.tsx, its folder beside it) and its props as JSON
    Then `raea-react lower Card.tsx --props '{...}' --into edit.rrl --as card1 [--w W --h H] [--x X --y Y]
      [--from F --to T] [--export Name] [--print]` renders it on V8 in Bevy (bun bundles it against raea/react's
      React), paints its DOM once with the fonts it names, and applies `image card1 {src: "asset:<hash>", x, y, w, h}`
      through `rrl apply` (a move with its why); rrl places, times and moves it as a whole (level 1a, done)
    # Without --w/--h it lays out on a canvas the edit's size and is cropped to what it painted (a pill hugs its
    # text); x/y default to where it laid out. 1.2 s a part. A row's src may also be a plain picture path now.
    And with --rows (level 1b, done) the laid-out DOM becomes rows under one root `box card1`: an element that paints
      a box (bg, gradient, radius, border, shadow) or has a name (data-part, id) is `box card1_<name> in <parent>`,
      its text a `text` row with the computed font, size, weight, colour, alignment, line height and tracking, and
      styled spans inside the text are runs of it (`text card1_hl in card1_label {bg, padding, radius}`), so keys
      reach each part: `card1_bar.w {0: 0, 40: 380} ease out` fills a bar, a run's opacity lights a word
    # Positions are relative to the parent row's padding edge, from Blitz's laid-out boxes (Dom::laid_out, computed
    # CSS serialized by stylo). One rotate(Ndeg) or scale(N) becomes the column; other transforms, filters,
    # clip-path, masks, backdrop-filter and <svg>/<canvas>/<video> are refused by name (paint it whole without
    # --rows). <img> becomes an image row with its picture copied to assets/. Checked against the painted version.

  Scenario: B5e. The brief's future: our own format, compiled by a neural reader into rrl and the shared latent space
    # Direction set by the founder, 2026-10-15; Gherkin + steps.rrl stays the rule source until this is built and measured
    Given a .brief (Gherkin-shaped or not) of lines with a strength: must (hard), should (scored), avoid
    Then a neural reader (NER/REL-style, constrained to a reading grammar) turns each line, once, at authoring time,
      into a typed reading: subject, relation (Allen intervals: during, after, over, until), time span, quality;
      cached by the line's hash and the model's version, so re-checks are deterministic
    And exact parts lower to rrl checks; latent parts lower to hypervector queries: SigLIP text vectors and
      level-encoded measured columns at the leaves, bind(SUBJECT, x) + bind(WHEN, span) + bind(QUALITY, q) as structure
    And "lowers correctly" is a type check: unbinding each role from the compiled query decodes to the reading's
      concept, or the line comes back as a question
    And the edit's segments are encoded the same way; a should scores a margin (STL-style robustness), split per role
    And quality words (quick, calm, holds) are bands from the measured ad norms (knowledge/editing-profile.json)
    # an agent session (who owns the HDC half: role vectors, projection, segment encoding), 2026-10-15:
    # 1. motion words query the L2 window vectors too (window transitions beat V-JEPA 2-L on next-2 s prediction,
    #    R@1 0.180 vs 0.148, and gap filling 0.134 vs 0.101), not only cut-rate and speed columns
    # 2. one fixed projection per encoder on both sides (seed 7 for SigLIP's 768, 11 for YAMNet's 1024, as
    # 3. the decode threshold is per role, from the decoded-similarity distribution on known-correct readings; at
    #    8192 bits, bundles of more than 5 or 6 bound pairs get noisy
    And latent checks are always soft, calibrated per concept against Kelly's rulings before they count

  Scenario: B5c. Kinds in 3D
    Then 3D kinds (mesh, light, camera, text3d) are rrl kinds; a shape is a value (a CSG expression,
      difference(roundedBox(2,1,.2,.1), text3d("SALE")), stored as a column, built by Manifold, cached by its hash,
      re-meshed only when the expression changes); Datalog types gain vectors, transforms and shapes
    And briefs can check 3D through the same vocabulary (overlap, facing the camera, inside the frame)

  Scenario: B6. Nothing regresses
    Then the HTML/React parity corpus through the Bevy path scores at least 107 exact, 61 ok
    And rdb/bench/substrate-gate.sh is green, and the CPU render still equals the GPU's

  Scenario: B7. Blitz's standalone loop retires
    # Done 2026-10-15: Composition::render is gone. Every frame is a run of Bevy's schedule (stage.rs): render,
    # snapshot and --boxes, the C API's snapshot, scrub and render workers, lint --layout and measure (Stage::text_boxes
    # paints the frame collecting text runs; Composition::text_report reads them), the node API (raea_node_*), and
    # rdb's HTML overlay (gpuview's page, html.rs's PagePool workers). What isn't a frame (page keys, media layers,
    # warnings) asks the composition through stage.comp(). Parity 107 exact, 61 ok, 2 fail; engine tests 82/82
    # (the V8 runtime now enters its isolate on each call: two pages on one thread used to panic, a V8-switch bug the
    # node API's tests caught); rdb's substrate gate green.
    # Speed before and after (examples/stage_bench.rs: one composition moved between the two loops in alternating
    # blocks, 60 frames, best of 3, M4, ms per frame; the split is the same composition's seek vs paint):
    #   page                    own loop   Bevy's schedule   difference   of a frame: JS (seek) / paint
    #   react/board             13.56      14.06             +0.51        0.15 / 17.40 (1% JS)
    #   kinetic-type            3.37       3.88              +0.52        0.34 / 3.28 (9%)
    #   product-promo           18.91      19.35             +0.45        1.44 / 20.68 (7%)
    #   gsap-plugins/splittext  2.64       2.36              -0.27        0.71 / 2.33 (23%)
    #   baseline/gsap-basics    1.69       1.21              -0.48        0.39 / 1.50 (21%)
    # The schedule costs between -0.5 and +0.5 ms a frame (within run-to-run noise on a busy Mac); a frame is mostly
    # paint, and JS (the seek: timelines and scripts in V8) is 1-23% of it
    Then nothing calls Blitz's own document loop, and this file records the design and the speed before and after
