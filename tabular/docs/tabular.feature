Feature: The tabular plane: tabular dreaming, a tabular in-context model scoring futures from rows of the past
  raea keeps everything as rows: the edit's tables and its move log (rdb, rrl), the agent's runs and their grades,
  the facts lifted from finished edits, the columns ingest computes per source frame. A tabular foundation model
  (TabICL v2) learns a task from labelled rows in context, with no training loop, and predicts the label of new
  rows. This plane is where raea does that. M4 Mac, 2026-10-14. Backlog: ../../the roadmap, Rule "Track C: tabular
  dreaming" (C1 is this doc), with Track D's predictive edit as its main customer.

  Background:
    Given the venv: tabular/.venv, Python 3.12, built from tabular/requirements.txt (tabicl 2.0.2, torch 2.14.1,
      scikit-learn 1.9.1, numpy 2.5.4, every package pinned); 719 MB on disk, git-ignored
    And TabICL v2's classifier checkpoint, tabicl-classifier-v2-20260212.ckpt (105 MB), in the Hugging Face cache
      (~/.cache/huggingface/hub/models--jingang--TabICL); TabICL downloads it on first use
    And every script prints its numbers to stdout; nothing here writes into the edit

  Rule: What "tabular dreaming" means here
    The phrase is the founder's (2026-10-14: "tabular dreaming with a tabicl or similar type model", and the roadmap:
    "work specifically on tabular dreaming and previz and the predictive edit"). No file, memory or transcript defines
    it further; this definition is proposed from the evidence and needs the founder's yes:
    - the roadmap, Track D3: "predict a move's effect ... used to rank the agent's candidate moves before rendering
      (tabular dreaming's natural use)"
    - Tablua (~/tablua/README.md): a tabular model "learns from those rows which moves make progress" and scores every
      candidate move before one is taken
      "scores states and candidate actions or forks", TabICLv2 or TabDPT as the in-context option, not TabPFN
    - the G6 scripts below, which already predict an outcome (a pass, a cut, Kelly's style) from rows before paying
      for the thing itself

    Scenario: The definition (proposed)
      Given rows of what happened: a state, the move or edit made, and how it turned out
      When a tabular model is given those rows as context and a new row whose outcome isn't known yet
      Then it imagines the outcome: will this run pass, will this move make the edit better, is this frame a cut,
        does this edit look like Kelly's; that is the dream
      And the dream is cheap (one forward pass, no render, no model call, no agent turn) and is only used where the
        real outcome is expensive: rendering, judging, a full agent run
      And a dream ranks or prunes; the real check (the brief, the render, the judge) still decides
      And it is not generating synthetic rows; that reading of "dreaming" (Dreamer-style rollouts in a learned world
        model) is a later step, listed under what's next

  Rule: Where it lives, and why
    Scenario: tabular/ is its own plane, beside engine/, rdb/, rrl/, meaning/, kgpu/ and media/
      Then tabular/ is a folder, not a crate: today it is Python benches over rows the other planes produce, the way
        meaning/bench/ holds meaning's Python benches
      And it doesn't go in rdb or rrl, because its rows come from everywhere (eval runs in ~/host, rdb's lifted
        facts, meaning's per-frame columns, rrl's move log), and it must not pull torch into a Rust crate's build
      And it doesn't go in meaning, because meaning computes columns from footage; this plane learns from columns
      And when a dream runs inside the agent's loop, the model is ported (the Candle port in
        ~/.cache/moonsplice/plugins/workspace/tabicl, MIT, parity-checked against tabicl 2.0.2, is the starting point)
        and tabular/ becomes a crate; until then it stays Python
    And the layout:
      | path                        | what                                                                        |
      | tabular/requirements.txt    | the venv's pins                                                             |
      | tabular/bench/              | the plane's own scripts (eval_outcomes.py, moved here from meaning/bench)   |
      | tabular/docs/tabular.feature | this file                                                                  |
      | tabular/.venv               | the venv every TabICL script in raea uses (git-ignored)                     |
    And scripts elsewhere use this venv and stay where their data is:
      | script                              | plane   | why it stays there                                            |
      | meaning/bench/cuts_fusion.py        | meaning | per-frame signals from meaning's columns; documented in meaning/docs/meaning.feature |
      | meaning/bench/cuts_eval.py          | meaning | the detector it fuses against (no tabular model)              |
      | rdb/examples/lift/style_models.py   | rdb     | the lifted facts and style.py's split (G3/G6)                 |

  Rule: Models and licences (checked 2026-10-14)
    Scenario: Which tabular models may be used
      Given the candidates
        | model          | code licence | weights licence                    | status                                   |
        | TabICL v2      | BSD-3-Clause (LICENSE in the tabicl 2.0.2 wheel, Soda team @ Inria) | BSD-3-Clause (model card, jingang/TabICL) | used |
        | TabDPT         | Apache-2.0 (layer6ai-labs/TabDPT-inference) | not yet checked (Layer6/TabDPT on Hugging Face) | the alternative; check the weights first |
        | gradient boosting (scikit-learn HistGradientBoosting) | BSD-3-Clause | none | the baseline every result is measured against |
        | TabPFN 2.5 and later | Prior Labs' licence | commercial use needs a licence | not used: the founder's preference (2026-10-12) and the licence |
      Then TabICL v2 is the in-context model, and every result here is reported next to gradient boosting on the same
        split; a tabular model that doesn't beat it isn't used for that task

  Rule: What's measured (2026-10-14, M4, TabICL v2 on CPU, 8 estimators, random_state 0)
    Scenario: Dream a run's outcome from its first five tool calls (tabular/bench/eval_outcomes.py)
      Given every graded run under ~/host/agent/evals/runs with result.json and events.json, infra failures
        excluded: 2,705 rows; the oldest 70% (1,893) as context, the newest 812 to score; 68% pass in both
      And per row: the scenario's family, severity and length, how many Thens and Givens, and the run up to its 5th
        tool call (which tools, errors, spend so far)
      Then held-out AUC, and a replay that stops a run at its 5th call when P(pass) is under a threshold chosen
        out-of-fold on the older runs (at most 5% of passes lost there):
        | model     | AUC   | replay threshold | runs stopped | spend saved            | passes lost |
        | gbm       | 0.614 | 0.20             | 93           | $1.91 of $16.30 (12%)  | 39 of 555   |
        | TabICL v2 | 0.589 | 0.46             | 204          | $2.72 of $16.30 (17%)  | 114 of 555  |
        | family pass rate (no model) | 0.546 | | | |                                                   |
      And so the first calls carry a little signal over the scenario alone, gradient boosting beats TabICL here, and
        neither is good enough to stop runs: the threshold chosen on older runs loses 7% (gbm) and 21% (TabICL) of the
        newer passes, not 5%; the runs drift over time (prompts and tools change), which a time split exposes
      And it took 5 min 14 s in all (TabICL's three out-of-fold fits included)

    Scenario: Dream whether an edit is Kelly's (rdb/examples/lift/style_models.py)
      Given style.py's cards (2,309: 1,500 of Kelly's finished edits, 809 from the corpus), 11 lifted facts, half to
        fit and half to score (a seeded shuffle, not grouped by source)
      Then held-out AUC:
        | model                         | all 11 facts | without duration |
        | gbm                           | 0.911        | 0.875            |
        | TabICL v2                     | 0.930        | 0.890            |
        | the brief (3 readable scenarios, style.py) | 0.73 |               |
      And TabICL beats gradient boosting by 0.015-0.019 here; duration alone carries 0.244 of TabICL's AUC (shuffled),
        then the longest caption's word count (0.042) and the share of time with text (0.031)
      And it took 3 min 3 s

    Scenario: Dream which frames are cuts (meaning/bench/cuts_fusion.py, documented in meaning's doc)
      Given the 20 newest of Buddy's renders with a timeline (58 visible cuts), half to fit and half to score (a
        seeded shuffle of renders), per-frame rows of ffmpeg's scene score, the luma change and the SigLIP 2
        neighbour distance with their local context; a frame is a cut if it's the nearest frame to a timeline cut
      Then held-out precision, recall and F1 within 0.1 s, thresholds chosen out-of-fold on the fitting renders:
        | model                                 | P     | R     | F1    | threshold | out-of-fold F1 |
        | the current detector, re-tuned        | 0.533 | 0.783 | 0.634 |           |                |
        | gbm                                   | 0.645 | 0.783 | 0.707 | 0.05      | 0.697          |
        | TabICL v2 (context capped at 6,000 rows) | 0.562 | 0.783 | 0.655 | 0.35   | 0.801          |
      And both models beat the detector at the same recall; gradient boosting beats TabICL by 0.052 F1, and TabICL's
        out-of-fold F1 (0.801) didn't hold on the scored renders (0.655): its threshold doesn't transfer
      And the first run (signals computed for 20 renders) passed the lane's 15 minutes and was stopped by its own
        14-minute cap before printing; with signals cached, gbm took 1 min 32 s and TabICL 4 min 56 s, run apart
      And 10 renders per side is small: one render more or less moves F1 by several hundredths

  Rule: Every result keeps to the source map
    Every split groups by source, as ../../the roadmap's Rule "The source map is the join key" sets out (that Rule
    is the canon; this one only says what it means for this plane).

    Scenario: What the source map changes here
      Then an edit's row takes its features from its sources' ingest columns at the mapped source time, and
        provenance (which take, where in it, reuse, the source-time jump at each cut) is a column
      And the splits above don't group by source yet: eval_outcomes.py splits by time, style_models.py and
        cuts_fusion.py by a seeded shuffle of cards or renders, and Buddy's renders reuse the same takes, so their
        held-out numbers may be optimistic; regrouping them by source hash is step C2

  Rule: Planned: a tabular vision model for video (CV in ../../the roadmap's Track C)
    Scenario: Columns from what the vision and audio models saw, tasks learned in context (not built)
      Given rows that are frame windows, shots or clips, joined to their sources through the source map
      And columns at a width a tabular model handles: SigLIP 2 scores against a label table (ad-card.feature's) and
        YAMNet's class scores as readable columns, a fixed 32-64-dim projection (PCA or random) of each SigLIP 2 / CLIP-
        like embedding, Perception's numbers, and the edit's own rows
      And the columns built by kgpu as fused K kernels (projection, label gram, per-shot means and deltas, HDC
        transitions), each checked equal to its CPU interpreter as composite.k is; the tabular model itself runs on
        ONNX or Core ML, not in K
      When a task is given as a few labelled rows (good hooks, a creator's taste, cuts)
      Then the model learns it in context and labels the rest
      And the cautions hold: splits by source (GroupKFold over source hash), every column carries the embedding model's
        id and is rebuilt when the model changes, every projection is measured against the raw embedding with a plain
        classifier, every task against gradient boosting on the same columns, and each model's weights licence is
        checked before use
      And the first task is cuts (cuts_fusion.py's rows plus TransNetV2's probability), then good-vs-bad hooks from the
        graded eval runs

    Scenario: The CV column kernels: the spec kgpu builds to (2026-10-14; built)
      Given a clip's ingest folder (perception/perception.feature, `raea-perceive ingest`): siglip.f32 is n x 768 f32
        rows at 8 a second, L2-normalised, with siglip.json (source, model id, fps, n, dim); facts.json's shots give the
        spans (start, end in seconds; row = floor(t * fps))
      And a label table T, L x 768 f32, L2-normalised (SigLIP 2's text side over ad-card.feature's labels;
        knowledge/labels/*.table.json), and a fixed projection P, 768 x 64 f32, N(0, 1/64) from a recorded seed
      Then the kernels are
        | kernel          | in                         | out                         | note                                              |
        | project         | X n x 768, P 768 x 64      | n x 64 f32                  | X·P                                               |
        | label gram      | X n x 768, T L x 768       | n x L f32                   | X·Tᵀ (cosines: both sides are normalised)         |
        | span means      | X n x d, spans k           | k x d f32                   | the rows of each shot averaged                    |
        | deltas          | X n x d                    | n x d f32, and n f32 norms  | row i minus row i-1 (row 0: zeros), and its length |
        | hdc transitions | X n x 768                  | w x 128 u64 (8192 bits)     | sign of 8192 seeded hyperplanes per row, then each 16-row window (2 s) every 8 rows bundled with position permutation, as meaning/bench/temporal's best method |
      And K is 32-bit integer (agreed with an agent session, 2026-10-14): siglip.f32 is quantised once to i32 at 2^15 and
        written beside the floats; P to 2^12 (project's output n x 64 at 2^27, with a float copy), the label gram at 2^30;
        span means and deltas are integer; the HDC hyperplanes are seeded integer-hash ±1 planes (as look_proj.k), not
        Gaussian, with a Rust CPU reference (raea_kgpu::cv_*_reference) that the benches call instead of numpy
      And each kernel is bit-exact against its CPU interpreter; the two float-valued kernels report their error
        against the exact f32 product (about 1e-4 relative, accepted: these are features measured against the raw
        embedding anyway); the outputs go to <ingest>/cv/<kernel>.{i32,f32,u64} with a JSON header (model id, seed,
        shape, scale), so a model change rebuilds them
      And meaning/bench/temporal's HDC result is re-measured once on the Rust reference's hash hyperplanes
      And YAMNet's per-patch scores (n x 521, 0.48 s hop) and embeddings (n x 1024) are a later input: ingest writes
        only the sound spans today

  Rule: C2, every split grouped by source
    Scenario: A run's outcome from its first five tool calls, grouped by footage (2026-10-15)
      Given eval_outcomes.py's 2,705 rows, each keyed by the footage its scenario edits (the take ids and asset files in
        its Givens: one name is one recording), 70 groups; GROUPED=1 scores 5-fold GroupKFold over them
      Then AUC: gradient boosting 0.561, TabICL 0.583, the family's pass rate 0.552 (on the time split: 0.614, 0.589,
        0.546)
      And so part of gradient boosting's edge on the time split was recognising footage it had seen; TabICL holds;
        neither is far above the family alone

    Scenario: Kelly's style, grouped by client (tabular/bench/style_grouped.py, 2026-10-15)
      Given the 2,309 cards grouped by where they come from (Kelly's by client, the corpus's by folder; hashed in memory,
        only counts printed): 28 groups, 24 of them Kelly's clients; 5-fold GroupKFold
      Then AUC: gradient boosting 0.578, TabICL 0.586, against 0.911 and 0.930 on the shuffled split
      And so most of what looked like Kelly's style was recognising her clients' edits; the corpus has only 4 folders,
        so some folds hold no corpus at all, which makes pooled out-of-fold AUC rough; a corpus grouped finer is the fix

    Scenario: Cut fusion, grouped by source (meaning/bench/cuts_fusion.py GROUPED=1, 2026-10-15)
      Given 20 renders split so whole source groups (D1's table) go to one side: 14 groups, 10 renders a side
      Then held-out P/R/F1: the current detector 1.000/0.667/0.800, gradient boosting 0.867/0.542/0.667, TabICL
        0.634/0.958/0.763; on the shuffled split fusion won (0.707 vs 0.634), grouped it loses
      And so the fused signals' edge came from footage seen on both sides; the current detector (now in Rust, A4) stays

    Scenario: CV's first task: cuts with TransNetV2's probability as a column (2026-10-15)
      Given cuts_fusion.py with TRANSNET=1: the per-frame shot-end probability (Python ONNX cache, else Rust through
        raea-perceive), here, its max over 2 frames either side, and whether it's a local peak, beside the other signals;
        the same grouped split (14 source groups, 10 renders a side)
      Then held-out F1: gradient boosting 0.595 (0.667 without it), TabICL 0.703 (0.763 without), the current detector
        0.800
      And so on this split the column doesn't help the fusion models (10 training renders is little to learn from); the
        current detector stays the cut finder, and TransNetV2 stays the cut's kind (previz B4)

  Rule: D1, the table of edits with outcomes (the roadmap Track D)
    Scenario: What a row holds (tabular/bench/edits_table.py, 2026-10-15)
      Given every graded eval run that left an export (~/host/agent/evals/runs; infra failures out)
      Then one row per run, in ~/.cache/raea-tabular/edits.jsonl
        | columns                                                                 | from                                                    |
        | run, scenario, family, when                                             | the run folder and result.json                          |
        | export (sha-256), sources (sha-256s), source_group                      | the export, and the videos its .w.html plays, hashed: the source map's key; the group is one id per set of sources, for splits by source |
        | passed, checks_passed, checks_total, problems, rubric_lines             | result.json                                             |
        | goals_met, goals_partly, goals_unmet, judge_confidence                  | result.json's goal_judge                                |
        | tool_calls, tool_errors, seconds_spent, tokens                          | result.json                                             |
        | seconds, shots, cuts_per_second, moving_share, brightness, face_share, closeup_share, words_per_second, speech_share, music_share | `raea-perceive analyze` on the export (all Rust), cached by its hash |
      And the build stops at --budget seconds and continues from its caches on the next run (lane jobs end at 15 min)
      And the overnight loop's runs (rrl/examples/overnight) join later: their scoring differs (hidden expects)
      And the export features stand in for what previz can compute before rendering (the rows and the sources'
        ingest give the same shots, faces, words and sound); D2 measures whether they predict the grade

    Scenario: The table built (2026-10-15)
      Then 659 rows (413 passed, 62.7%), all with export features (the Rust analyze took 5.4 s an export, over five
        13-minute lane passes), 67 sets of sources by exact hash (232 groups once runs without a resolvable source fall
        back to themselves), 274 rows with the goal judge's verdicts

  Rule: D2, does what an edit shows predict its grade?
    Scenario: The export's features against `passed` (tabular/bench/edit_outcomes.py, 2026-10-15)
      Given D1's 659 rows: the ten export features and the scenario's family
      Then AUC on the newer 30% (time split): the family's pass rate 0.629; gradient boosting 0.615 with the family,
        0.594 without; TabICL 0.649 with, 0.598 without
      And grouped by source (5-fold): family 0.636, gradient boosting 0.620, TabICL 0.645
      And the features that carry it most: the family, then cuts a second and the moving share
      And so what the edit shows adds 0.01-0.02 over the scenario alone, and only for TabICL: `passed` grades whether
        Buddy did what was asked (the checks), not how good the edit is; D2's next is a label of quality (the judge's
        met share, Kelly's picks, ad outcomes) rather than pass/fail

  Rule: D3, does a move help? (tabular dreaming's natural use)
    Scenario: Every overnight move, labelled by replay (tabular/bench/move_effects.py, 2026-10-15)
      Given the overnight loop's scored runs (rrl/examples/overnight/runs, read only; each state checked in a copy of the
        run's project) and their move logs (the edit before each move): each move labelled by the scenario's hidden
        expects (the bank's, never shown to the agent) on the edit before and after it: 804 moves in 47 scenarios, 326
        help, 27 hurt, 451 change nothing; rows in ~/.cache/raea-tabular/moves.jsonl
      Then grouped by scenario (5-fold), predicting "helps": gradient boosting 0.855, TabICL 0.874 on what the agent
        could know, 0.917 and 0.931 adding the share of hidden expects already holding (which only the scorer knows);
        the move kind's rate 0.539
      And the features say what carries it: the move's place in its run alone gives 0.869 (n alone 0.753: 66% of first
        moves help, 18% of fourth), but how many moves the run makes is the future, and the move's own content (its
        kind and size) gives 0.565
      And so, today, "will this move help" is mostly "is it the first real move": ranking candidate moves before
        applying them (D5's dreaming) needs features of what the move changes against what was asked

    Scenario: The move against what was asked (2026-10-15)
      Given features the agent can see, without the move's position: its words' overlap with the founder's request (and
        that over the move's words), whether the row or rule it names is in the request, whether it existed before, how
        many numbers it sets, whether it's an expect, its kind and size
      When the replay is rebuilt (615 moves in the same 47 scenarios: the build's lane budget ends before the last runs)
      Then grouped by scenario: gradient boosting 0.658, TabICL 0.682, against 0.565 for kind and size alone
      And so a move's fit to the request is a real, modest signal for ranking candidates before applying them; the
        next step is the move's effect on the brief's own expects (agent-visible), which `rrl check` gives per move

  Rule: C4, the dream used offline: what skipping low-scored moves would do (2026-10-15)
    Scenario: A replay over the overnight moves
      Given move_effects.py on 942 moves (the overnight loop kept adding runs; 313 help), TabICL's out-of-fold scores
        from the move-against-request features (grouped by scenario: AUC 0.720, gradient boosting 0.678)
      When every move scored under a threshold is skipped
      Then at 0.2: 460 skipped, 380 of the 629 moves that didn't help (60%) and 80 of the 313 that did (26%); at 0.5:
        629 skipped, 489 not helping and 140 helping
      And so it isn't a gate (a quarter of the helpful moves lost at the loosest threshold); it's a ranking hint for
        choosing among candidates, which is how D5 would use it; wiring it into the overnight loop (D4) is rowlio-44's
        call, offered with these numbers
      And rowlio-44 took D4 deterministically (2026-10-15, "c10 candidate"): when a move is the fourth single
        `row.col = value` move in a row on one column, apply's reply suggests one set rule; cycle 10 measures it. The
        model score isn't wired (it would put the tabular venv in rrl's process); a softer score hint can be tested
        later from rrl's move log

  Rule: Layouts judged before they render (the roadmap K3, 2026-10-15)
    Scenario: The graded runs have almost nothing to learn from
      Given tabular/bench/textfit_table.py: `raea lint --layout` at six frames of each graded the host app composition
      When it runs over 641 of 659 (after three kinds of false alarm found there and fixed in the lint)
      Then one is flagged, and it's real (fx-08-zoom-shake throws its text up to 48 px off the frame; its judge passed
        it); no judge complained about text fit; one positive is nothing to train on

    Scenario: Generated plans, labelled by the lint, predicted from the plan alone
      Given tabular/bench/layout_plans.py: 240 card rows like the founder's screenshot (3-8 cards of 120-260 px, titles
        16-24 px over spaced monospace labels 12-17 px, words drawn from a list), each linted at one frame: 87 spill, 85
        have a crowded row, 112 either; features only what a plan knows before rendering (counts, widths, sizes,
        spacing, the estimated widths of the widest title, label and word against the card's inner width)
      When a model is scored split by plan family (GroupKFold, 30 families of 8)
      Then spills: the plain estimate (widest label over inner width) AUC 0.946, gradient boosting 0.997, TabICL 0.993
      And crowded rows: the estimate 0.883, gradient boosting 0.951, TabICL 0.956
      And either: 0.969, 0.972, 0.974
      And so a layout can be scored before any render, and the model earns its keep over the estimate on crowding (the
        interaction of width, size and words); the caveat: one font family and one layout shape, so the next set is the
        agent's real decks and cards varied the same way

  Rule: The base is gradient-boosted trees, in Rust (the founder, 2026-10-15: "run it in rust", "no sidecars, no python")
    Scenario: Why not TabICL as the base
      Given every grouped comparison so far: TabICL and gradient boosting within a few hundredths of each other (C2, D3,
        K3), TabICL's memory growing with its training rows (it attends over all of them), PyTorch only, and no saved
        model to ship
      Then trees are the base and TabICL a small-table cross-check; LightGBM was tried first and set aside: its Rust
        crate builds LightGBM's C++, a sidecar, and in Python its OpenMP clashed with PyTorch's (a segfault when torch
        loaded first)

    Scenario: raea-gbm (tabular/gbm), 2026-10-15
      Given Perpetual 3.0.0-rc.2 (pure Rust, Apache-2.0; boosting that sets its own learning rate and stopping from one
        budget, 0.5 here; the 2.x releases need nightly Rust), sklearn's GroupKFold done in Rust (groups largest first to
        the lightest fold), AUC by rank, the models saved as JSON and reloaded; unit tests pass (3 of 3: AUC by rank,
        groups never split, a threshold learned)
      When `raea-gbm cv` runs over the tables the benches built
      Then the layout plans' spills: AUC 0.997 (the plain width estimate 0.946; sklearn's gradient boosting 0.997)
      And their crowded rows: 0.946 (estimate 0.883; TabICL 0.956, sklearn's 0.951)
      And the agent's moves, does one help: 0.712 (TabICL 0.720)
      And each cross-validation takes 0.1-0.25 s on one thread, and every saved model reloads and predicts the same
      And so C5 (a winning model out of Python) is done: the scorer and its training both run in raea's Rust, ready for
        rdb, the overnight loop or the phone; ranking candidates (dreaming proper) is Perpetual's ranking objective next

    Scenario: Dreaming proper: rank a content's layouts before rendering any (raea-gbm rank, 2026-10-15)
      Given 40 contents (a row of 3-8 cards, titles over spaced labels) each laid out 8 ways (card width 120-260 px,
        title 16-24 px, label 12-17 px, spacing 0-3 px), every layout linted by raea's engine (135 clean, 58 a crowded row
        only, 127 spilling), all generated and labelled from Rust; the best layout is the clean one with the largest
        type (a spill 0, crowded 1, clean 2 plus up to 4 for title and label size)
      When Perpetual's list-wise ranking (ListNet) learns from the plans' numbers alone, contents held out by fold
      Then its top pick is clean for 85% of the held-out contents, never spills, and scores 0.858 of the best layout's
        score; picking the roomiest layout is always clean but scores 0.714 (small type), the largest type 0.433 (it
        spills 43% of the time), a random pick 0.365
      And scored only clean or not, the roomiest layout wins (40 of 40 against the model's 39): the model earns its
        place on the trade-off, how big the type can go before the row breaks
      And so the dreaming loop exists in Rust end to end: candidates generated, labelled by the engine, ranked before
        rendering; what it needs next is candidates and labels from real edits (the agent's own proposals, your picks)

    Scenario: Decks and dashboards ranked before rendering (raea-gbm decks, the roadmap K3, 2026-10-15)
      Given four slide shapes an agent builds (tabular/gbm/src/deck.rs): KPI tiles (a number, its label, a change pill),
        fixed-width pills, dashboard cards (a title over a two-line note, fixed height) and an agenda (numbered steps with
        an owner, the founder's screenshot); 80 contents (20 per shape) each laid out 8 ways (columns, component width and
        height, three text sizes, letter spacing, padding, one of four fonts: Inter, Avenir Next, Georgia, Menlo), every
        layout linted by raea's engine at 1920x1080: 360 clean, 77 a crowded row only, 203 spilling (KPI tiles spill
        most, 75 of 160; dashboard cards least, 25 of 160); the best layout the clean one with the largest type
      When rank.rs's ListNet (now `score_ranking`, shared) learns from the plan's numbers alone (each text kind's
        estimated width over the inner width, the estimated height over the room), contents held out by fold
      Then its top pick scores 0.821 of the best layout; the roomiest layout 0.621, the largest type 0.315 (it spills
        61%), a random pick 0.448; 40 contents gave the same order (0.843, 0.638, 0.347, 0.458)
      But its pick spills on 7.5% of contents against the roomiest's 3.8%: it leans to bigger type, and where the
        spill is a few pixels it loses; a gate that keeps its pick only when the lint passes (or the estimate is under
        1) closes that, at the cost of one lint per pick
      And so K3's ranking holds across shapes and fonts, not only one row of cards


  Rule: What's next
    Scenario: The steps after C1, in order
      Then C2: regroup every split above by source hash and re-measure (the numbers above are the before)
      And C3: D1's table of edits with outcomes becomes this plane's main context: rows are an edit and a move from
        rrl's move log, the label is whether the outcome improved (brief held, judge score, pass); TabICL vs gbm,
        split by source and time
      And C4 (replayed offline 2026-10-15, above): the dream used: rank the agent's candidate moves before rendering
        (D3), and report in the overnight loop (rrl/examples/overnight) whether it would have picked better moves or
        stopped doomed runs
      And C5: if a tabular model wins a task, port it out of Python for the loop (the Candle port of TabICL v2, or
        ONNX / Core ML), and tabular/ becomes a crate
      And later, and only if asked: dreaming as generation (synthetic rows or imagined rollouts in a learned world
        model over the move log)

  Rule: How to re-run
    Scenario: From a clean checkout
      Given uv and Homebrew's python3.12
      When the venv is built:
        """
        cd ~/raea/tabular
        uv venv --python /opt/homebrew/bin/python3.12 .venv
        LANE_FOR=raea-23 lane quick -- uv pip install --python .venv/bin/python -r requirements.txt
        """
      And the benches run through the eval lane, each under 15 minutes:
        """
        cd ~/raea/tabular/bench && LANE_FOR=raea-23 lane eval -- ../.venv/bin/python eval_outcomes.py --k 5
        cd ~/raea/rdb/examples/lift && LANE_FOR=raea-23 lane eval -- ~/raea/tabular/.venv/bin/python style_models.py
        cd ~/raea/meaning/bench && LANE_FOR=raea-23 lane eval -- ~/raea/tabular/.venv/bin/python cuts_fusion.py --renders 20 --models gbm   (then --models tabicl: each under 15 minutes)
        """
      Then MODELS=gbm (or tabicl) runs one model in eval_outcomes.py, DROP=duration drops a fact in style_models.py,
        and cuts_fusion.py's first run computes its signals into ~/.cache/raea-meaning/signals (later runs reuse them)
