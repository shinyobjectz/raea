Feature: .brief, one file from the client's ask to the compiled edit (the roadmap Track P)
  The founder, 2026-10-17: "fix the fountain format so that it's entirely in .brief. It's its own format, which brings
  together the goals of the fountain format; we had to make it more machine readable while allowing for a bit more
  diverse creative planning and checking against our runtimes. A way to plan with variables, plan with
  generalizations."

  Today three files say what one piece is: the client's brief (Markdown, split by raea-beats into beats, Track F),
  script.fountain (score's notation, compiled by score/scorec.py, Track R) and the rrl expects and Must: rules the
  checks read. Fountain is kept for what it measurably does (the screenplay form beat rrl by +1.95 on the judge's ad
  score, quoted lines anchor at 0.90-0.97; score/bench/results.feature), and replaced where it fails a machine: its
  readers guess (unknown tokens 6.0 a script written cold), it has no way to say "a shot of the product here, which
  one we don't know yet", and no place for the rules the edit must keep.

  Rule: The goal
    Scenario: What .brief is when it's done
      Then one plain-text file, `NAME.brief` (a space's own is `brief.brief`), holds the piece at every stage of
        planning: the ask and its rules, beats still open, lines quoted from takes, parts, sound, and the checks
      And it reads as Fountain to a writer and to M3 (title page, `# sections`, `= synopses`, `.TAKE`, `@SPEAKER`,
        `(delivery)`, quoted lines, `[[parts | utilities]]`, `---` stylesheet), so what M3 already writes well stays
      And it has one grammar with one reading per line: `raea-brief parse` gives the intent graph as JSON (sections,
        beats, lines with source spans, parts with anchors, sound, variables, checks) or refuses with line:column and
        what was expected; `raea-brief fmt` writes it back unchanged in meaning
      And it compiles to one target, an @raea/motion composition (edit.tsx, scorec's SDK backend; the founder, 2026-10-17:
        TypeScript/React is the only path, rrl's backend goes), and every check compiles from the same graph, never
        from the text
      And it is one way in, not a required step: the agent may write edit.tsx and use `raea cut` / `raea add` with no
        brief at all
      And `[[parts]]` name components in the space's components/ (shadcn-style, copied from ~/raea/cli/registry:
        captions, screen-fit, face-frame, callout, end-card so far; styled only from brand/brand.css tokens through
        Tailwind classes, cli/template/theme.css)

  Rule: What the format adds to Fountain (a proposal; settled in P0 with an agent session)
    Scenario: Plan with generalizations: open beats
      Then a `~` line is a beat not yet bound to footage: `~ product: the cleanser pumped into wet hands
        [find: product hands action]`, the same kind and finds raea-beats writes (to-camera, b-roll, product, screen,
        end-card; face, speech, product, hands, screen, text, place, action, logo)
      And the compiler resolves it against the source table (F2): bound to a clip and moment when covered, kept open
        and named in the sheet when weak, a make() prompt or a reshoot item when missing (F3)
      And a quoted line under `@SPEAKER` is a beat bound exactly, as in score today; a `~ to-camera:` beat with no
        quote asks for the line that says it, which the sheet proposes from the transcripts for the writer to quote
    Scenario: Plan with variables and variants
      Then `$name: value` on the title page names a value used anywhere after it (`$product`, `$launch`, a brand
        colour); an undefined variable is refused where it's used
      And `Format:` may list several deliverables (`9:16, 30 s, tiktok | 1:1, 15 s, instagram`); a section, line or
        part tagged `{1:1}` belongs to that cut only, so one brief compiles to every cut the client asked for
    Scenario: Checks the runtimes run
      Then a `!` line states a rule the edit must keep (`! never says "game changer"`, `! nothing covers the face`,
        `! the hook reads within half a second`), from a fixed vocabulary each lowering to a TS predicate or a
        `raea check` fault over the render manifest (render.json: each clip's take, in, out and box, each part's box
        per frame) and cuts.json, an agent session's phase 4 shape, or refused as unknown
      And the brief's set-aside units (dates, deliverables, instructions) land here or in variables, so nothing in the
        client's ask is lost between the brief and the edit
      And the sheet answers each check pass or fail with the frame, as it answers each line's timing today

  Rule: A workspace holds many briefs (the founder, 2026-10-17: "it could be many files for a workspace like
    anything.brief")
    Scenario: Pieces and shared briefs
      Then a .brief with a `Format:` line is a piece: `raea script NAME.brief` compiles it to NAME.tsx, and
        `raea script` with no name compiles every piece in the space
      And a .brief without one is shared: its variables, checks and open beats apply only to the pieces that name it
        with `Uses: client.brief, brand.brief` on their title page, in that order, the piece's own last (a value
        set twice takes the later one, as CSS does; checks add up and none can be removed)
      And the client's ask is one shared brief (raea-beats writes it: the ask's beats as `~` lines, its set-aside units
        as variables and `!` checks), and each piece made from it is checked against it: the sheet lists each of its
        beats as covered by a piece or by none, so three cuts of one launch answer the ask together
      And a `Uses:` cycle or a missing file is refused where it's named

  Rule: Every find is a perception fact (measured by survey, 2026-10-17)
    Scenario: What the find words can be checked with today
      Then face, speech, text and screen have detectors (ingest's faces.json tracks at 4 fps, VAD and performance's
        speaking, read-text rows, layout's screen zone and screenshot flag); hands and place have partial ones (the
        hands command, not in ingest; Apple scene labels, empty on the portable backend); product, logo and action have
        none and come only from a SigLIP phrase
      And the sheet says how each beat was bound: by a fact, or by a SigLIP score against FIND_THRESHOLD, so a
        product beat bound by a phrase alone is shown as weaker than a face beat bound by a track
      And a variable can carry a picture (`$product: media/milky-jelly.png`, `$logo:` from brands.json) so product and
        logo beats search image against frames, not text against frames, until perception has an object detector
        (object.rs's EdgeTAM tracker is there, not yet in ingest)
    Scenario: Finds take the facts perception has that the words lack
      Then a find may be a predicate over a moment's facts: `people:1`, `speaking`, `eye-contact`, `framing:close-up`,
        `still` or `moving`, `no-text` (no captions burned in), `screen`, `bright`; each lowers to a column of the
        moment record (Track L's slots and L3 tracks), never to a guess
      And a `!` check uses the same predicates over the render instead of the footage (`! speaking -> face`: the
        speaker is in the picture whenever they talk)
    Scenario: The source table F2 needs (the gap)
      Then today it's three stores: the ingest folder per clip (~/.cache/raea-ingest), the SigLIP columns at 8 fps
        (~/.cache/raea-meaning/cols) and the takes' words.json; `raea-meaning find` returns one best frame per clip
      And F2 needs one table of moments (take, from, to, the facts above, the SigLIP row, the words said) with
        top-k spans per query, so a `~` beat binds to a span the compiler can cut, not to a frame
    Scenario: Perception at compile time follows the clip, not the take
      Then the compiler reads the face, safe area and burned-in text for each kept clip's span from the ingest store
        (faces.json is already per frame at 4 fps), not one see-layout still per take reused for every clip of it, and
        caches nothing it can read from ingest
      And the face it places around is the speaker's: the face track that is speaking (performance's speaking) when
        more than one is in shot
      And perception.json is written per frame over each clip (the engine's percAt already takes a frame-keyed map),
        so Region, captions and callouts follow a speaker who moves
    Scenario: What the rest of the path drops today (owners named, not ours to change)
      Then the host's render passes --words but never --perception, so in a `raea` project Region has no face
        (an agent session, host.rs)
      And framing="face" and punchIn only mark the element, and the proof (proofBoxes) is never run by the renderer
        (an agent session, react/src/motion.rs)
      And --safe-top/--safe-bottom aren't CSS variables, so screen-fit and end-card can't keep out of the UI zones
        (@raea/motion)
      And `raea check` runs cuts and edge text only; the face and text-over-face checks still read rrl, so `!` checks
        need render.json (each part's box per frame) and the render perceived again (an agent session's phase 4)

  Rule: P0 and P1, measured (2026-10-17)
    Scenario: The grammar and the parser
      Given brief/format.md (the grammar), brief/src/format.rs (the parser, in Rust) and `raea-brief parse|fmt|convert|
        compile`; 13 conformance cases in brief/conformance/cases, each saying the reading or the refusals (line:col) it
        gives (cargo test)
      When the 110 scripts the bench wrote in Fountain (s2b's 10 compiled ones; the 100 written cold in stage 1) are
        converted, parsed and read back into score's Script (brief/conformance/parity.py)
      Then 110 of 110 give the compiler the same Script front.parse reads from the Fountain, in everything the notation
        defines; 110 of 110 keep what it doesn't (unknown utilities, title keys, style) as comments; fmt reads back the
        same graph 110 of 110
      And the corpus said what writers mean by Fountain's marks: `!` stood before a shot description 157 times and
        `@must { }` held the rules 50 times, both dropped by score's reader; convert makes them `~` beats and `!` checks,
        and the parser refuses `!Action ...` with the open beat it should be
      And front.parse's misreadings convert to what was meant, counted as divergences: a note left open (`[[... ]`)
        is words of the line to front.parse and a comment here; a fenced answer is read as front.parse reads it
    Scenario: The ten eval briefs as the client's ask
      Then `raea-beats brief BEATS.json` writes each split brief as a shared .brief: its 42 beats as `~` lines with
        their words and source as comments, the set-aside units as comments; all ten parse (brief/examples/*.brief)
      And turning set-aside units into variables and checks is still the agent's (a date is `$launch`, "don't say
        game changer" is `! never says "game changer"`)

  Rule: P3, P3b and P4, first slice (2026-10-17)
    Scenario: Open beats bound to footage (score/compiler/moments.py)
      Given a moment per SigLIP frame of each ingested take (8 a second): faces, the speaker's track, speaking and eye
        contact, framing, motion, text, screen, luma, and whether words are said
      When a beat is searched: its detected finds filter the moments, its phrase (or $picture) ranks them, a span grows
        from each peak (1.5-4 s), and the best span is covered (>= 0.10), weak (>= 0.06) or missing
      Then on media/fixtures (a screen recording, a two-person take, b-roll; no transcripts at all) a brief of four
        open beats compiles: "two people in glasses" covered at 0.116 on the take, "a dashboard with a chart" weak at
        0.072 on the screen recording, "coffee poured into a cup" missing (0.046; find stock or shoot it), the end card
        named a part
      And a brief with no transcripts compiles when it has open beats (an agent session's Arock case: a silent screen recording
        no line can come from); a script that resolves to nothing is refused, not written as an empty composition
    Scenario: A workspace of pieces (raea-brief compile --all; score/cover.py)
      Then every *.brief with a Format: compiles, each to NAME.tsx with its own NAME.words.json, NAME.perception.json
        and NAME.checks.json; a piece with several deliverables to NAME-9x16.tsx, NAME-1x1.tsx, each the right shape and
        holding only what its {ratio} tags allow
      And each beat of a shared brief is answered against the footage each piece cut (the ask's phrase on its clips'
        frames where its finds hold): on the fixtures, "two people" covered by three cuts at 0.112, the screen and the
        coffee by none, with the closest
    Scenario: Checks (sheet.checks; NAME.checks.json)
      Then `never says "..."` and the length are answered at compile time, pass or fail with the line or the seconds;
        the other musts and FACT -> FACT go to checks.json for `raea check` on the render (an agent session's render.json)
    Scenario: Limits the measurements found
      Then the speaker is the longest-visible face, not the one whose mouth moves with the voice
      And the picture threshold (0.70, image against frame) isn't calibrated, nor is "weak" (0.06)
      And ingest's mesh step aborts on a long take (576 s, an Objective-C exception), so it gets faces and facts but no
        SigLIP column and its beats can't be searched (posted for an agent session)

  Rule: P1b, measured (score/bench/p1b.py, 2026-10-17)
    Scenario: Perception per clip from the ingest store against one still per take
      Given the ten s2b spaces compiled both ways (RAEA_SCORE_INGEST=0 and 1), their 29 takes ingested once (two abort
        in ingest's mesh step after faces and facts are written); the truth is each take's ingest, the speaker's
        confirmed face at every 4 fps sample, through each version's own crop
      Then a part's band holds the speaker's face in 64 of 482 samples a part shows, against 107 of 482 (13.3% against
        22.2%): framing and bands from each clip's own span, not from the middle of the take's first line
      And captions over captions burned into the source: 0 both ways (ingest found no caption-like burned text in
        these takes), so the measure says nothing on these takes
      And perception.json is frame-keyed at 4 fps over each clip (a no-face entry where the speaker isn't seen), so
        Region-placed components follow the face in the render

  Rule: P5's gate, stage 1 (score/bench/stage_brief.py, 2026-10-17)
    Scenario: M3 writes .brief cold and revises once, as c3 did in Fountain
      Given the same 10 scenarios, requests, transcripts and parts kit as stage 1, two reps each; the notation page
        arms/b1_brief.md; one revision from what the compiler says (the parser's refusals, score's sheet); measured as
        stage 1 measures c3 and judged by the same judge (gemini-4-argon, temperature 0)
      Then, beside stage 1's rows:
        | arm                                      | ad   | follow | fit  | verbatim | length ok | unknown |
        | a2 Fountain + CSS, cold                  | 5.25 | 3.55   | 5.30 | 0.90     | 0.50      | 6.0     |
        | c3 Fountain + CSS, revised               | 5.80 | 3.95   | 6.40 | 0.93     | 0.80      | 0.8     |
        | b1 .brief, cold (parser v2)              | 5.50 | 4.35   | 5.00 | 0.90     | 0.50      | 0.7     |
        | c4 revised from parser v1's sheet        | 4.95 | 4.70   | 5.85 | 0.97     | 0.65      | 0.2     |
        | c5 revised from parser v2's sheet        | 5.65 | 4.65   | 6.10 | 0.94     | 0.70      | 0.2     |
        | c6 revised, the sheet saying what to cut | 5.00 | 4.40   | 6.05 | 0.97     | 0.85      | 0.2     |
      And written cold, .brief beats Fountain + CSS on the ad (+0.25), on followable (+0.80) and on what the reader
        can't read (0.7 refusals against 6.0 unknown tokens); with parser v1 the cold answers had 2.4
      And parser v2 reads the forms M3 wrote that have one meaning: an apostrophe inside an anchor (on:'don't know'),
        a must without its "the" or with its length in seconds, several phrases after never says, people:N-M, a note on
        its own line after the line it's on (front.parse reads the same; parity holds 110/110)
      And the length was the gap: told "OVER by 3.3 s" the writer cut to 6.7 s; told "OVER by 24 s" it shortened lines
        by changing their words, so no take said them. c6's sheet names the whole lines to drop (the fewest that land it
        inside) or the seconds to add, and for a line no take says, what the take does say; length ok 0.85 and unknown
        0.2 pass the gate (0.80, 0.8)
      And the judge's ad score moved 4.95-5.65 over three revisions of the same cold answers, and c6's 5.00 is under
        c3's 5.80: a cost to watch in stage 2, not hidden
      And c3 and a0 also missed the length on client-a and kelly in both reps; the sheet improvements are the
        compiler's (c3 had the old sheet)

  Rule: P5's gate, stage 2 (score/bench/stage2_brief.py, 2026-10-17)
    Scenario: Rep 0 of c3 and c6 rendered on the SDK with the same parts and kit, watched by stage 2's judge
      Given both arms compiled to @raea/motion (c3's Fountain through scorec, c6's .brief through raea-brief compile),
        perception per clip from the ingest store, the registry's lockup, statcard, notthat, follow and captions, one
        house brand kit for every scenario (no per-client kit on disk is reliable), no sound, as stage 2b
      Then c3 renders 10 of 10, judge mean 2.70; c6 renders 9 of 10, mean 2.78 on the nine (2.50 with the failure as 0)
      And c6's tenth refuses to compile: a prop left open (`text:"+ try $brand |`) that its one revision didn't fix;
        Fountain's reader would have read it wrong without saying
      And every render of both arms is rejected; the judge names the render and the cuts more than the plan: cards over
        the face, captions dropping out for a sentence or overlapping a card, cuts mid-word, blooper takes
      And so the gate as written isn't met: 3.1 was c3 on rrl's renderer and parts kit, and on the SDK c3 itself scores
        2.70; 10 of 10 renders misses by the one refusal; the user re-based the gate (the next rule), and P5 was done

  Rule: P5's gate, re-based, and P5 done (2026-10-17)
    Scenario: The gate on the renderer both formats now share
      Given the founder's call (2026-10-17): gate on .brief matching or beating Fountain on the SDK renderer, with the agent's
        refusal loop given the second pass it has in the product
      When a c6 answer the parser still refuses goes back to M3 with its refusals only (c7; one of the 20, the open quote)
      Then c7 renders 10 of 10, judge mean 2.80, against Fountain's 10 of 10 at 2.70 on the same renderer, parts and
        kit; length ok 0.85 (Fountain 0.80); unknown 0.0 (Fountain 0.8)
      And so the Fountain path left the compiler: scorec reads .brief only (through raea-brief), refuses a
        script.fountain with the command that converts it, and score's Fountain reader is kept in score/bench/fountain.py
        to re-run the recorded comparisons; the converted brief compiles to the same edit.tsx, words and perception the
        Fountain did (client-e, byte for byte) and parity stays 110 of 110
      And void's agent writes no Fountain (an agent session: `--sop ts` by default, 496a850); its old score tool goes with rrl
      And what the judge rejects in both arms is the render's craft (cards over the face, captions dropping out, cuts
        mid-word, blooper takes): compiler and registry work, not the format's

  Rule: Ink against .brief, and the decision (2026-10-17)
    Scenario: The same 10 scenarios written in ink's raea profile (arms/i1_ink.md), revised and fixed as c7 was
      Given M3 writing ink cold (i1), one revision from the parser's refusals and the sheet (i6), the refusal loop's pass
        (i7); rep 0 of i7 rendered on the SDK with c7's parts, house kit and judge (stage2_brief.py)
      Then, beside .brief:
        | arm                       | ad   | follow | fit  | verbatim | length ok | unknown | readable cold | renders | judge (render) |
        | c7 .brief, revised, fixed | 4.90 | 4.40   | 6.20 | 0.97     | 0.85      | 0.0     | 1.00          | 10/10   | 2.80           |
        | i7 ink, revised, fixed    | 5.35 | 4.10   | 5.40 | 0.89     | 0.75      | 0.0     | 0.60          | 10/10   | 2.80           |
      And ink renders the same (2.80, 10 of 10) and reads a little better to the plan judge (+0.45, inside its own swing
        between rounds), but M3 writes it worse: 40% of cold answers unreadable, fewer lines said verbatim, the length
        and the fit missed more often
      And so .brief stays the format the agent and people write; ink stays a supported surface (raea-brief reads .ink,
        `raea-brief ink` writes it) for pieces that branch or keep state, and blade-ink is the runtime a .brief can run on

  Rule: The brief checked on the render (raea-brief check, @raea/data/check.ts; 2026-10-17)
    Scenario: The 10 c7 renders re-rendered with the manifest and checked (--battery beside each brief's own rules)
      Given each piece's facts as tables: its graph, the words as cut, the render manifest, faces read back from the
        rendered frames twice a second, perception's moments of the takes on screen (score/bench/check_c7.py; the run's
        record, all 10 checked with every rule measured, is score/bench/check_c7.json)
      Then, beside what stage 2's judge named:
        | scenario       | judge | the checks fail                                                        | the judge named, the checks agree       |
        | shawn-williams | 3     | text over a face 3.5 s (42%), a 11.8 s shot                            | captions across his face 0:03-0:15      |
        | client-a| 3     | speaking without a face 18.5 s, 67 s of 34-46, a 16.2 s shot           | the callout over his eyes at 0:18       |
        | apollo         | 3     | a card over the face 6.5-10 s (26%), 47 s of 30-40, a 17.7 s shot      | the card over her face 0:06-0:09        |
        | client-e     | 1     | speaking without a face on 15 of 24 samples, an 11.7 s shot            | rehearsal footage, the speaker off shot |
        | client-e    | 3     | text over a face 31.5 s (81%), speaking without a face 37 of 50        | captions on the kid's face ~0:34        |
        | dr-moy         | 4     | a 6.5 s shot                                                           | (none of the judge's: missing word, captions dropping out) |
        | kelly          | 3     | text over a face 3.5-6 s and 65.5 s (82%), 77.6 s of 51-69             | the title over her eyes 0:02-0:05, the card over her face 1:01-1:05 |
        | client-1  | 3     | speaking without a face 17 s, a 10.5 s shot from 16 s, text over a face 26.5 s | dead air while he looks away 0:16-0:19 |
        | client-2  | 3     | nothing but 5.9 s without a cut                                        | (the judge's: his face cut by the frame's edge, captions dropping out) |
        | client-3  | 2     | 24.4 s of 25.5-34.5, a 12.5 s shot from 8 s                            | the blooper take from 0:08              |
      And every place the judge named text over a face, the checks found it, but one: client-3, where the judge says
        the captions cover the mouth and the geometry says 7.2% at most, the captions under the chin (the frames agree
        with the geometry); client-a's callout hides the eyes so no face is read there, and the check names it as
        speaking without a face
      And what the judge names that no rule measures: captions dropping out, text over other text, an ending cut off
        mid-sentence (a cut mid-word is caught; mid-sentence is the sheet's "reads to check"), duplicate and blooper takes
        as such, low contrast: the edge of what the rule set covers
    Scenario: raea check runs it (an agent session, 130af72)
      Then `raea check NAME` runs `raea-brief check NAME SPACE --video` for a piece compiled from a brief; its FAILs are the
        agent's faults ("brief ! the hook reads within half a second: nothing is shown or said in the first half second",
        on the fixture space's vtest-A); every compiled piece has its own .score/NAME.graph.json for it
    Scenario: The rest of this goal's constructs
      Then typed parts: on the registry's components (what every bench space's components/ holds: stage2_brief.py copies
        the registry's lockup, statcard, notthat, follow and captions into each), 73 of the 110 corpus scripts and 19 of
        the 20 c7 briefs type clean;
        the refusals are real (35 `statcard big:90` where big is a string, a notthat without its `not`, captions given
        l1-l3); a delivery just before its speaker is now read as that speaker's
      And on spaces whose components the agent wrote: the 38 eval spaces with components/ (void's runs of 2026-10-16
        and 17) give 111 components, 47 of them the agent's own or changed by it; props are read for every one (103 have
        props, the other 8 take none); on one with the agent's `stamp` (words: string, required; sub: string), a brief
        line `[[stamp sub:"verified" color:"gold"]]` is refused for `color` and for the missing `words`,
        `[[stamp words:270]]` for writing a string as a number, and `[[stamp words:"SAFE" sub:"for you"]]` passes
      And claims, alternatives, variants and pacing compile on real spaces (client-e: four cuts of a two-variant,
        two-deliverable brief; "See the bass?" picked over "There he is." with the reason); coverage of a shared ask runs
        per cut; @raea/data's loadBrief answers which line is said and which parts show at a frame by query
      And the renderer leak this found (60 MB a frame on a 16:9 edit, after a4eb038) was fixed by an agent session (c21d7e8)

  Rule: c8, the full feature set on the bench (stage_brief.py, stage2_brief.py; 2026-10-17)
    Scenario: The b1 cold answers revised with everything, then the check loop
      Given the notation page with alternatives, claims and pacing rules; one revision from the parser's refusals (typed
        against the registry's components), the sheet (with what to drop or add) and the compile-time checks (c8r); then
        the loop the product runs, twice: refusals, failing checks and the length's "To fit" sent back (c8, then c8l)
      Then, beside c7:
        | arm                                 | ad   | follow | fit  | verbatim | length ok | unknown | renders | judge (render) |
        | c7 .brief, revised, refusal pass    | 4.90 | 4.40   | 6.20 | 0.97     | 0.85      | 0.0     | 10/10   | 2.80           |
        | c8 full set, one check-loop pass    | 5.85 | 4.95   | 6.55 | 0.98     | 0.75      | 0.0     | 10/10   | 3.00           |
        | c8l full set, two check-loop passes | 5.00 | 4.50   | 6.25 | 0.98     | 1.00      | 0.0     | 10/10   | 3.00           |
      And so the gate holds: 10 of 10 renders, the judge's mean on the videos 3.00 (c7 2.80), every length inside its band,
        nothing the parser can't read; the plan judge's ad score drops when lines are dropped to fit (5.85 to 5.00), and
        the videos' judge doesn't follow it
      And every render of both is judged: c8's client-1 first came back from the judge empty and was judged again (3);
        c8l, which rewrote that script, rendered and judged its own (3); c8l's ten: 4 3 3 2 3 3 4 3 3 2, mean 3.00
      And M3 used neither alternatives nor claims unprompted; the length came from the loop sending the failing length
        and its "To fit" back
      And the spend, estimated from the counts (the OpenRouter key's own usage, $21.58 that day, is shared with other
        sessions' eval runs): 220 plan judgings at about 9k tokens (about $2.5-5), 44 video judgings at about 10k (about
        $0.5-1), 161 MiniMax-M3 calls through the host app at about 35k (about $1.7): about $5-8, under the $10 asked

  Rule: The plan
    Scenario: Steps, with owners
      Then P0 (raea-01 with an agent session): the grammar written down (brief/format.md) with conformance cases, its corpus
        every script.fountain the bench has written (score/bench/out/s2b, s2c, c3) and the ten eval briefs
      And P1 (raea-01): the parser in Rust in raea/brief (`raea-brief parse|fmt`), its graph equal to front.parse's
        Script on every corpus script (parity), every corpus script converted with nothing dropped, and the edit.tsx
        compiled from the graph equal to sdk.py's from the Fountain (no rrl parity; compiler/rrl.py is deleted)
      And P2 (raea-01 then an agent session): the CLI contract, `raea-brief parse FILE` (graph JSON on stdout; refusals with
        line:column on stderr, exit 2) and `raea-brief compile FILE SPACE [--o edit.tsx]` (parse, then scorec's SDK
        backend on the graph; the COMPILED line and the sheet on stdout); an agent session wires it as `raea script
        [NAME.brief]`, a host op in void, and the procedures teach the format; script.fountain still read until P5
      And P1b (raea-01): the compiler's perception per kept clip from the ingest store, the speaking face, and
        perception.json per frame; measured on the stage-2 scripts as faces the parts cover and captions over burned-in
        text, before and after
      And P3 (raea-01, on Track F): the moments table and top-k span search, open beats resolved against it (F2) with
        how each was bound, and proposed lines in the sheet; raea-beats writes the client's ask as a shared .brief
      And P3b: workspaces of many briefs (`Uses:`, pieces, coverage of the shared ask across pieces)
      And P4: variables and variants (one brief, every deliverable), and `!` checks lowered to the runtimes' checks
      And P5: the Fountain path deleted only when the stage-2 bench matches or beats c3 on it: judge mean at least
        3.1, length ok at least 0.80, unknown tokens at most 0.8, renders 10 of 10
      And every step above is done (2026-10-17), and the brief model after it (the rules above: checks on the render,
        typed parts, claims, alternatives, variants, pacing, the brief as entities, c8)
