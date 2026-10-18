Feature: CSS selects, SQL transforms, K computes, Bevy ECS runs it (the probe that was spikes/kss)
  Status 2026-10-15: the spikes/kss probe is deleted (it no longer built once vendor/blitz-vibey-script went, and rrl
  replaced the .k.sql direction); the numbers below are its record (docs/consolidation.feature D3).
  The founder, 2026-10-13: Odin and React are dropped as authoring layers. The surface is relational: CSS declares and
  selects, SQL states the transforms, K is the column engine underneath, and Bevy's World is the runtime. Bevy draws
  3D; raea's systems (Blitz, Stylo) draw 2D with real CSS. Both are views of the same entities.

  Rule: the layers
    Scenario: Who does what
      Then CSS selects: `kind#id { ... }` and `@repeat N kind.class { ... }` spawn rows; `.ring:not(.still)[glow]` picks
        a set (kind, #id, .class, :not(.class), [has-column])
      And SQL transforms: `UPDATE <selectors> SET a = e, ... [WHERE e];` runs in file order, never cascades; SET is a
        pipeline (a new name is a local array); WHERE masks the writes; `(SELECT e FROM sel)` reads another set (a join)
      And K computes: every expression is a whole-column kernel; MAX/MIN/SUM(e) reduce the set (K's |/ &/ +/),
        `OVER (ORDER BY i)` scans it (K's \\), LEAST/GREATEST are & and |, AND/OR are & and | on 0/1 columns
      And the same sheet can say it all in .k.css (`.ring { pos.x: 2.4 * cos(a); }`); both compile to one IR and render
        byte-identical frames
      And Bevy's World is the runtime: the sheet's rows are entities, its statements systems in one fixed order, time a
        frame counter resource (never Bevy's clock)

  Rule: what the probe showed (2026-10-13, M4)
    Scenario: Results
      Then hook.k.css and hook.k.sql (a turning cube, a 14-sphere ring as one set with a reduction, two eased titles)
        render 150 frames at 1080x1920 in about 5 s with Blitz 2D (3 s with bevy_ui text); all 150 decoded frames match
      And frames are identical in any seek order once pipelined rendering is off and each GPU readback carries the frame
        number in a 1-px tag row (cropped); without the tag, readbacks arrive late and a frame can repeat the one before
      And raea's World runs inside Bevy's: raea::world::install() spawns the 2D rows into the app's World, the sheet's
        components move onto those same entities, raea::world::systems() run in Update, and the transparent Blitz
        frame is a full-screen UI image over the 3D camera
      And bevy_ui text (Bevy's default font, no CSS) is far below Blitz's typography; 2D stays on Blitz
      And bevy-react (a React reconciler emitting ops onto bevy_ui) isn't needed: with no React authoring, the sheet
        emits the ops itself, and bevy_ui would lose the CSS fidelity Blitz gives

  Rule: what is open
    Scenario: Next
      Then compile selectors to typed bevy queries and kernels to typed columns (the probe interprets a per-entity map)
      And the A/B with MiniMax M3: the same edits written as .k.sql and .k.css, scored on the same evals
      And iPhone: Bevy 0.19 on Metal for iOS, binary size (the probe is 115 MB unstripped on the Mac with raea inside)
      And Bevy 0.20 when the shared toolchain reaches Rust 1.97 (raea pins bevy_ecs 0.19.1 to match)
