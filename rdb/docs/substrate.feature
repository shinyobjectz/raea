Feature: One substrate: the edit as typed columns in Bevy's World, written in SQL, computed in K, checked in Gherkin
  What we read and write today, what the spikes proved, and the direction to build. Criteria (the founder, 2026-10-14):
  more DRY than today, highly performant, every toolchain doing its real job (K, SQL, Gherkin, Bevy, Blitz-dom), for
  video now and games next. Spikes: examples/spike_*.rs; numbers: bench/substrate-spikes.txt. M4, 2026-10-14.

  Background: today there is no one database
    Given an edit lives in six shapes at once:
      | where                         | shape                                         | role                                   |
      | db::State                     | rows of maps per kind, keys, rules, moves     | the truth while editing (our engine)   |
      | the .duckdb file              | typed tables, rebuilt by the duckdb CLI       | storage; read-only queries             |
      | ir::Program + inserted rows   | compiled rules over rows of maps              | what rendering starts from             |
      | eval::frame_state(frame)      | a fresh copy of every row, every frame        | the evaluated frame                    |
      | Bevy World (Row, Vals)        | one map per entity, overwritten each tick     | the current frame for the renderer     |
      | raea's Tracks / Resolved    | JSON props and raea's own timeline model    | the 2D (Blitz) rows: a second time model |
    And "K" means three things: the rule evaluator (K-style set-at-a-time, but Rust over Vec<V> enums), the kgpu
      dialect (real K compiled to WGSL over typed GPU columns), and raea-meaning's k.rs (hypervectors on the CPU)
    And the same logic is written several times:
      | logic                                    | copies |
      | clip cover / zoom / pan maths            | 6 (engine x2, gpuview x2, previz, comptest); the f32/f64 bug came from this |
      | callers evaluating frames themselves     | 7 (render, scrub, sketch, previz, brief check, audio lanes, overlay) |
      | decoder set-up and caching               | 4      |
      | column helpers (num)                     | 3      |
      | GPU pool LRU                             | 2      |
      | rows -> Blitz props                      | 3      |

  Scenario: V1-V5, what the spikes proved
    Then the evaluator doesn't care where rows live: it runs through ir::Set, so storage swaps under the same code
    And V2, today's cost: the hook 23 us a frame (80% rules); the UGC edit 177 us a frame, 92% of it one 9-frame
      transition recomputing whole neighbouring frames, and every consumer above pays it again
    And V3, columns + the whole timeline at once, the same evaluator: the UGC edit 119-144 -> 14.8-27 ms for 738 frames (5-8x over two runs; the spike still clones every raw frame),
      the hook 3.3 -> 3.1 ms; 0 of 888 frames differ from today, value for value
    And V4, Bevy's World as the store: agent-defined columns registered at run time (f64 or String components), each
      kind a marker so its rows share tables, base values read in place from table columns (zero copy), each frame's
      writes in an overlay so the base is never mutated: 0 frames differ; raw frames 8.4 -> 1.6 ms (UGC), 3.8 -> 3.3
      ms (hook); building the World 0.06-0.22 ms
    And V5, games: the hook's ring rules over N spheres, typed f64 column loops against today's evaluator, values
      identical:
      | spheres | today per frame | typed per frame | speed-up |
      | 1,000   | 1.0-1.1 ms      | 0.013-0.026 ms  | 37-86x   |
      | 10,000  | 9.5-10.6 ms     | 0.12-0.13 ms    | 78-81x   |
      | 100,000 | 116-120 ms (misses 60 fps) | 1.2-1.3 ms | 92-98x |

  Scenario: The direction: one substrate, every toolchain in its place
    Then the truth is the move log: the SQL snippets applied, with snapshots (event sourcing; the move table already)
    And the store is Bevy's World: a row is an entity, a column a typed component registered at run time per
      (kind, column), a kind a marker; one copy of the edit, columnar, shared by everything that reads it
    And one language: .rrl (Datalog, the founder's pick, 2026-10-14; front end by rowlio-44) is what the agent, files and
      briefs use; it lowers into this crate's IR (ir.rs), and rdb's SQL front end becomes legacy. Facts are
      rows, keys are keys, labelled rules are the ordered per-frame updates, recursive rules a bounded fixpoint inside a
      frame (hoisted when frame-invariant), `next` rules the game schedule; queries run on the same columns; storage is
      the log plus a columnar snapshot, so no duckdb CLI anywhere
    And K is the compute: a rule is a K expression over typed columns, run natively on the CPU (typed loops) or on the
      GPU (kgpu's WGSL) from one source; the K interpreter stays the exactness reference
    And Gherkin is the check: a brief lowers to K predicates over the evaluated columns, for every frame at once
    And Blitz-dom draws the 2D rows from the same components: a 2D row's style columns feed its DOM node, Bevy's change
      detection says which nodes to restyle (the overlay memo, generalised); raea's separate Tracks/timeline model
      goes, SQL keys and rules are the one time model
    And Bevy schedules and renders: the same compiled rules run as systems

  Scenario: Video and games are two schedules over the same rules
    Then video evaluates the timeline: a frame is a pure function of (base columns, keys, rules, frame), computed for
      all frames at once (frames x rows, transitions as a scan over the series), any order, in parallel, cached by
      digest; render, scrub, sketch, audio and briefs read it instead of re-evaluating
    And games run the same compiled rules per tick: state carries from tick to tick, input arrives as rows (events),
      the rules write the World in place
    And the edit says which: a kind whose rules read their own previous tick is game state; everything else stays a
      function of the frame, so a video can hold a game-like simulation by scanning it once over the timeline

  Scenario: Build it in phases, each measured against today's references
    Then phase 1, the store: the World holds the edit (runtime typed components per kind+column), db::State and the
      engine's Vals map go; pass: the hook and UGC hashes unchanged (examples/out/ref), apply/undo/moves identical
    And phase 2, the timeline: one evaluation of all frames shared by render, scrub, sketch, previz, audio and check;
      pass: frames identical, the UGC timeline <= 15 ms, the six callers reduced to one
    And phase 3, compiled K: rules lowered from the IR to typed K kernels (CPU loops, kgpu on the GPU), the Vec<V>
      evaluator kept as the reference; pass: = the reference value for value, 10,000 rows <= 0.2 ms a frame
    And phase 4, storage: the log + a columnar snapshot, the agent's query on our columns; pass: save/load/undo round
      trip exact on every example, no duckdb process, the same answers on the G5 queries
      (the edit is an .rdb, a render database: JSON lines of a header, the state with its rows as columns, then the
      move log; query.rs answers one SELECT per table; bench/queries.sql against DuckDB's recorded answers)
    And phase 5, Blitz on the same entities: 2D rows' columns feed DOM nodes via change detection, raea's Tracks go;
      pass: overlay pixels identical, redraws only on changed rows
      (done: raea's install_hosted; the engine and gpuview write each 2D row's frame values into its node's Resolved
      through one props_2d, only when they differ; UGC's full render patched 91 rows in 34 syncs, not 612; .row edits
      keep Tracks, which rdb no longer uses)
    And phase 6, the agent bridge on top (edit, query, check, look, undo, render), then the CJ variations eval
      (done 2026-10-14: `ugc rdb new|media|apply|query|look|render|check|undo|moves` in ugc-agent's shell, run by the
      app's host (ugc-eval's runs rdb). Seven rounds of the private CJ scenario (cj-variations-rdb) taught the engine:
      UPDATE edit, media by (id, path) with project paths, IN/BETWEEN/LIKE in rules, clips that name a sound play it,
      sound = '' mutes, settings checked, render warns of picture and sound gaps. Round 7: 8/9 checks, three Reels
      rendered at 30, 30 and 60 s with word captions; the miss is a deliberate 2 s end card on black)
    And each phase deletes what it replaces: the duplicated cover maths, decoder caches, pool and num helpers become
      one module each as the store and timeline land

  Scenario: Risks the spikes didn't cover yet
    Then CTEs weren't in the spikes (both edits lacked them); they become temporary columns in the overlay
    And a column holding numbers in one kind and text in another collides when components are named by column: name
      them by (kind, column)
    And strings and absent values need a typed representation (an interned string column, a presence mask)
    And parallel frames need the World read-only while frames evaluate; Bevy's World is Sync for reads
    And raea's 2D timeline (Tracks) carries animation the SQL keys may not express yet; phase 5 measures that

  Rule: Decided: page paint stays on CPU workers while wgpu is split (2026-10-15)
    Scenario: vello_gpu for the page
      Given anyrender_vello_hybrid 0.11 (vello_gpu 0.3) matches engine's anyrender 0.14 and vello_common 0.3 but needs
        wgpu 30, and Bevy 0.19 pins wgpu 29 for rdb and kgpu
      Then the page would paint on a second GPU device and come back through the CPU to kgpu's: about 5 ms a frame on
        one core (estimate), against html::PagePool's 2.9 ms a frame on eight cores (measured, kinetic-type)
      And so the page stays on vello_cpu workers until the stack moves to one wgpu, when the paint can land in kgpu's
        texture with no copy

