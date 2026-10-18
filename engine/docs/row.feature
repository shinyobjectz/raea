Feature: .row: edits, systems and scenes written as plain Odin declarations, implemented in Rust
  The founder, 2026-10-13: "lets just do plain odin for now". A .row file is Odin syntax with raea's meaning: each
  declaration is an entity and its components (a row), Rust parses it (src/row.rs), evaluates time (src/timeline.rs)
  and renders it (src/row_render.rs: Blitz, Stylo, Taffy, Parley, Vello). The Odin compiler never sees it, and
  nothing in it runs except procedures, which run in Rust once slice 4 lands. React stays outside the syntax: a
  .tsx component is imported and used as a kind. The layer has to cover Bevy (3D, games) later, so it is ECS all
  the way through (see "ECS all the way through").

  Rule: the syntax
    Scenario: One declaration per element
      Then `name :: kind{...}` declares an element: the name is its id, the kind its component bundle (lowercase is a
        native kind drawn by Rust: box, stack, title, text, image, video, captions; capitalised is a component, an
        imported .tsx or later a .row component)
      And a leading string is the element's text, `in = parent` its parent, `span = 0..<90` (or `0..=89`) its frames,
        `from = 96` or `to = 90` a half-open span, `style = brand` or `style = {brand, big}` its styles
      And a `{ ... }` block right after an element declares its children (the same rows as `in = parent`)
      And `name.prop = value` sets one prop after the fact; declaration order is sibling order
    Scenario: Values
      Then values are Odin's: numbers, strings, enums (`.out`, `.center`), lists `{1080, 1920}`, structs `{a = 1}`
      And `{0 = 760, 14 = 620}` (Odin's designated indices) is a curve, frame = value; `ease(.out, {...})` eases it
      And `spring(from, to, at = 0, stiffness = 180, damping = 12)`, `bind("column")`, `asset("file.png")`
      And a call `dodge(self.box, face)` is kept as source until procedures run
    Scenario: Declarations that aren't elements
      Then `import EndCard "./EndCard.tsx"`, `edit :: Edit{fps = 30, size = {1080, 1920}, frames = 450}`,
        `words :: bind("words")`, `brand :: Style{font = "Montserrat", color = "#fff"}`, `dodge :: proc(...) {...}`,
        and `expect caps inside .safe_zone during 0..<450`
    Scenario: The example (examples/row/hook.row; hook-nested.row is the same rows with a block)
      Then it reads:
        """
        import EndCard "./EndCard.tsx"

        edit  :: Edit{fps = 30, size = {1080, 1920}, frames = 150}
        words :: bind("words")

        bg :: box{w = 1080, h = 1920, bg = "linear-gradient(180deg, #1b1f3a, #3a1b2f)"}

        hook :: title{"Stop scrolling", in = bg, span = 0..<90}
        hook.y       = ease(.out, {0 = 760, 14 = 620})
        hook.opacity = {0 = 0, 10 = 1}

        caps :: captions{in = bg, words = words}
        end  :: EndCard{in = bg, from = 96, y = 760, title = "Shop the kit"}
        """
    Scenario: Rows are the truth
      Then `parse(print(rows)) == rows` and printing is stable; `raea row print` prints the canonical form (one
        declaration per element, curves and springs on the lines after it)
      And mistakes are refused with line:column and a reason (unknown names and styles, a parent declared later, a
        name declared twice, a span that isn't from..<to)

  Rule: ECS all the way through
    Scenario: What is what
      Then a declaration spawns an entity; its fields are components; `Style{}` is a reusable bundle; `name.prop = v`
        writes one component
      And systems run in one fixed schedule: time (curves, springs), bindings (perception columns), procedures
        (behaviours), layout, then the renderers
      And every renderer is a derived view that never holds truth: Blitz's DOM is patched from components each frame
        and computed boxes are written back as a component; a React component is pure (props in, pixels out, no state
        across frames); Bevy 3D kinds live in the same World
      And the World is bevy_ecs (standalone, no Bevy renderer needed), so 2D, React and 3D share one data model

  Rule: procedures are systems
    Scenario: Behaviours
      Then a procedure computes props from columns per frame: typed, data-oriented (arrays and structs of arrays),
        no heap the engine doesn't own, no I/O, no clock, seeded randomness only
      And an example:
        """
        follow_face :: proc(caps: []Caption, face: Box) { for &c in caps { c.y = min(c.y, face.y - c.h - 24) } }
        """
      And they run in a small register bytecode interpreter in Rust (deterministic, no JIT, runs on iOS); if they
        outgrow it, WebAssembly under Wasmtime (Pulley on iOS)

  Rule: why this shape
    Scenario: What it buys
      Then one line per fact makes one edit one line change, which the engine diffs into a typed move: the Agent's
        iteration problem (17-iterate-cut) without asking MiniMax M3 for nested JSON
      And every element is named, so moves target stable names (set_key(hook, y, 14, 600))
      And a grammar we own gives line-precise errors, types, a formatter and lint
      And determinism is a property of the language, so the seek gate holds by construction
    Scenario: The risk, measured
      Then models have seen less Odin than JSX, so the grammar (about a page) sits in the Agent's cached prompt
        prefix, and the timeline plan's Phase D A/B compares .row edits with JSON moves on the same evals

  Rule: slices
    Scenario: Build order
      Then slice 1 is the parser and printer, plain Odin only (done 2026-10-13)
      And slice 2 is the bevy_ecs World: parse into entities and typed components, the timeline and the Blitz sync as
        systems, frames byte-identical to the slice-1 renderer
      And slice 3 is design and output: Style bundles, stack layout, image and video kinds, mp4
      And slice 4 is procedures (the interpreter above), slice 5 type checking against component schemas, slice 6
        text edits to moves, slice 7 the the HTML composition format importer
      And the Bevy probe runs 3D kinds (mesh, camera, light) in the same World, on the Mac and the iPhone

  Rule: the other repos
    Scenario: What moves where
        engine, React/.row plus Bevy); the C ABI (include/raea.h) stays for embedders
