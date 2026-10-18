Feature: The React engine: edits as rows, components in React, time and data in Rust
  An authoring model for raea designed from what the host app's evals and the timeline plan
  (docs/timeline-plan.feature, arm D3) showed, not a copy of another library. Working name: the React engine
  (sdk/ and src/timeline.rs); the product name is open.

  What we saw, and the design each finding forces:
    | finding                                                                   | design                                                            |
    | the Agent fails at iterating (17-iterate-cut 1/3) when it rewrites markup | the edit is rows of component instances; code is only the library |
    | MiniMax M3 mangles nested tool args and over-claims ("added")             | flat typed moves on rows; done = checks and expectations hold     |
    | animation in code (GSAP, another library's interpolate) means editing code       | time lives on props as curves (keys, easing) and bindings         |
    | perception (faces, words, product, depth) lives in side files            | props bind to perception columns, resolved per frame from data     |
    | checks are hand-written per defect                                        | components declare roles; the engine reports facts per frame      |
    | per-frame JS is the cost (Boa is ~50x slower than V8 on big trees)        | Rust evaluates every animated prop column-wise; React gets one     |
    |                                                                           | snapshot per frame and does structure and layout only             |

  How it differs from another library:
    | another library                                        | the React engine                                                 |
    | the edit is code (a component tree in TSX)      | the edit is rows (edit.json); TSX is the component library        |
    | time inside components: useCurrentFrame,        | components never see the frame; props arrive already resolved     |
    |   interpolate, spring                           |   (curves, springs and bindings are data, evaluated in Rust)      |
    | renders in Chrome                               | renders in raea (Blitz + Vello), deterministic, on the iPhone |
    | no data model for perception or checks          | perception columns in, facts and expectations out                 |
    | a licence for companies                         | raea's own (Apache-2.0)                                       |

  Rule: the edit is rows
    Scenario: edit.json
      Then an edit is {fps, width, height, duration, components, instances, expects}
      And fps is a rational "30" or "30000/1001", time is integer frames, seconds only at the edges
      And an instance is a flat row: {id, kind, parent, order, from, to, props}, where from/to are frames and props
        map a name to a value
      And a prop value is one of: a constant; a curve {keys: [[frame, value], ...], ease}; a spring
        {from, to, at, stiffness, damping}; a binding {bind: "face.box" | "words" | ...}
      And instances form a tree through parent and order; a child's frames are relative to its parent's from

  Rule: components are pure
    Scenario: The component contract
      Then a component is a React function of its resolved props, plus `frame` and `progress` props the engine adds
        (frame relative to the instance's from; progress 0..1 over its span) for the rare component that needs them
      And components register a schema: prop names, types, defaults and a role (caption, title, media, product,
        logo, cta, decor) that lint checks and the Agent reads
      And lint rejects state and effects that cross frames (useState, useEffect, Date, Math.random, timers);
        randomness comes from a seeded `rand(frame, id)` the engine supplies

  Rule: Rust owns time and data
    Scenario: Per-frame evaluation (src/timeline.rs)
      Then the instances visible at frame f and their resolved props are computed in Rust: curves column-wise per
        prop kind, springs in closed form, bindings by as-of lookup into perception columns
      And the result is one compact snapshot per frame, handed to the page through a native call; React renders it
        with flushSync
      And the same evaluator exists in JS (sdk/eval) for browser previews and Chrome parity, and tests keep the two
        identical frame for frame
      And a frame is a pure function of the rows and f, so seek order never matters (bench/seek-determinism.py)

  Rule: the Agent works on rows
    Scenario: Moves
      Then moves are flat: set_prop(id, prop, value), set_key(id, prop, frame, value), remove_key(id, prop, frame),
        span(id, from, to), add(id, kind, parent, from, to), remove(id), reorder(id, order)
      And `raea rows edit.json --brief` prints the edit as compact rows; `raea move edit.json moves.json`
        applies moves, refusing a bad one with the reason
    Scenario: Facts and expectations
      Then each frame reports, per instance, its role, box, opacity and visible text (the render table in
        docs/timeline-plan.feature)
      And expects are rows such as {role: "caption", "inside": "safe_zone", during: [from, to]} checked every frame

  Rule: data orientation, K and HDC, where they pay
    Scenario: Where each fits
      Then curves are stored per prop as frame and value columns and evaluated with whole-column operations
        (K-style), which is where most per-frame work goes
      And bindings join perception columns by frame (an as-of join), the same rule as the timeline tables
      And HDC fingerprints of an edit (instances bound to roles and spans, bundled) and of moves (after minus before)
        are derived later (timeline-plan Phase F), never stored as the truth

  Rule: engines and languages, decided by measurement
    Scenario: Where each runs (2026-10-13)
      Then the view is JSX, run by Preact (preact/compat for React libraries when a component needs one): it does less
        work per render than React, and Preact Signals let the per-frame snapshot update only the props that changed,
        without reconciling the tree. The authoring syntax stays React's, so MiniMax M3 writes what it knows
      And procedural behaviours (particles, data-driven layout such as captions that avoid a face, beat-synced cuts)
        are open between Lua and the proposed script (docs/row.feature, under consideration), both run in Rust and
        deterministic; the choice follows the Bevy requirement and the Phase D A/B. A behaviour reads columns and writes props; it never
        touches the view
      And the JS engine is V8 on the Mac (vendor/blitz-v8-script, identical to Boa on the parity corpus, 32x faster on
        React re-renders). On the iPhone, JIT is reserved for WebKit, so the choice is measured on the re-render bench:
        jitless V8 (the v8 crate's iOS build, from source, needs a crate upgrade from 135), JavaScriptCore (the system
        engine, a third port through the same facade), or Boa, which stays until one of them wins
      And React against Preact + signals is measured on the same bench on V8 and Boa before the sdk commits to one

  Rule: the slices
    Scenario: Build order, each slice tested before the next
      Then slice 1 is src/timeline.rs: edit.json parsing, curve, spring and constant evaluation, the per-frame
        snapshot, with unit tests (done 2026-10-13: 5 tests: spans nest and clip, curves ease and blend numbers,
        arrays and colours, springs overshoot and settle, bindings look back, frames are pure, bad edits are refused)
      And slice 2 is sdk/: the runtime that renders a snapshot (Preact with signals, after the React/Preact
        measurement), the base components (Box, Text, Image, Video, Captions) and the JS mirror of the evaluator,
        bundled with bun
      And slice 3 is the CLI: `raea render|snapshot|probe edit.json` builds the page and drives frames
      And slice 4 is parity and determinism: the example edits against Chrome (through the JS mirror) and the
        seek-determinism gate
      And slice 5 is rows --brief and move
      And slice 6 is behaviours (the script or Lua, once chosen): a behaviour reads columns and writes props, with a
        small eval of MiniMax M3 writing them
      And slice 7 is perception bindings and facts (with the timeline plan's Phases B and C)
