@spec
Feature: Ask: questions about a piece's picture and sound, answered by the decision model and paid from the person's plan
  Shane, 2026-11-20: "a decision reviewer type api that calls and utilizes their raea subscription ... for checks and
  things, or extending checks to be done quickly based on audio and video, maybe even being able to ask a question
  about how does this sound based on the visuals". Planned "so that it sufficiently understands first what is
  required of it, what types of options come standard, how ask works and can be improved over time". And: "the agent
  or human may want to have a check automated in their own pipelines that asks, or ... fingerprint checks to also call
  an ask call to double check, or ... a script that collates a bunch of ask calls in parallel and processes the needed
  clips (which should be the case anyway)".

  The model this file sets:
    1. The decision model is clef-omni on Workers AI: typed questions (yes or no, a choice, a graded level) about text,
       up to 4 stills, 4 sounds of 5 minutes and 2 videos of 60 s at 2 fps, answered in probabilities, never in free
       text; the same request gets the same answer. It costs by the media it reads (about $0.006 a minute of video;
       sound costs little), not by the questions, so every question about the same media goes in one call.
    2. One way to ask, for every caller: `raea ask` on the command line, a person's own pipeline or CI, a script that
       asks many questions over many clips, `raea check`, the fingerprint double-checking its own flags, the app's
       agents, its critique and game checks, and `raea judge`. One question format, one client, one parallel runner.
    3. Ask understands a question before it spends: what the question needs (picture, sound, both, the words heard, the
       words on screen, the brief), whether this file can answer it, and what the piece is. A question that can't be
       answered isn't asked; it says why, or its variant is asked instead.
    4. It's paid from the person's raea plan by default (Pro's and Studio's monthly pool, which stops when it's spent),
       else from their own Cloudflare account; with neither it refuses and says how to get one.
    5. Local measures come first. Clef gets what can't be measured, only for windows that changed, cached by content.
       Timing (sync, freezes, dropouts) stays local: clef samples video at 2 fps and was weakest there (11-18).
    6. An answer is advice until its question is calibrated on the person's own labels; every answer is kept, so each
       question earns its threshold, a better wording can be measured against the old one, and a well-calibrated
       question can later be taught to a free local head.

  Background:
    Given clef's four callers today, each with its own client and all on the person's own Cloudflare keys: the app's
      critique (apps/app host/critique.ts: the export's first minute at 480 px with its sound as mono MP3, the rubric's
      questions for the brief's modes), its game check (host/playcheck.ts), `raea judge` (apps/judge clef.rs, review
      sheets on a frozen rubric) and the playtest DSL (sdk/app playtest.ts, which holds clef's verified limits)
    And raea-api's `POST /v1/clef/ask` (raea-platform PR #15, not deployed): an account's questions charged to its pool
      (Pro $2, Studio $20 a month, stopping when spent), cached in R2 by request, under a daily ceiling across everyone;
      images and videos only
    And the check packs (render, hook, storyboard, brand: 22 checks), each clef question kept with its `label_words`,
      its AUC, its threshold at 10% false positives and its status (advisory or warn), labelled from people's notes
    And the fingerprint's evidence rows, where another eye's readings (`source: "eye:NAME"`) train a head and never
      count as the person's (FP6d), its AUC and quantile (raea-sys-fingerprint head.rs) and its gates (16 labels of
      each kind, 3 projects, a held-out AUC of 0.8: train.rs)
    And taste lines (core/brief taste.rs): words, a check (one of six forms, a named check, a `.ts` check), values and a
      level, held by `raea check PROJECT` (apps/direction check.rs) through raea's named checks (systems.rs `named()`)
    And what clef has shown: its yes is soft (0.15 to 0.51 on real defects), so a threshold is per question; it answers
      what's asked, so the rubric's sound question, which assumes a voice, called a promo with no voice "weak" until
      `sound-silent` was asked in its place (docs/craft-system.md)

  Rule: AK1. One question format and one client

    @done
    Scenario: AK1. Questions, and the standard library
      Given the plan
        | crate         | file                       | lines | job                                                                    |
        | raea-core-ask | core/ask/src/lib.rs        | 40    | the crate's exports                                                    |
        | raea-core-ask | core/ask/src/question.rs   | 220   | a question: id, ask, type (yes, choose, grade, rate) with its options or levels, sees and hears, uses, when, window, label_words, calibration; its version, a hash of its words, type and options; TypeScript by ts-rs |
        | raea-core-ask | core/ask/src/library.rs    | 200   | the standard sets: rubric (read from sdk/guides/craft/rubric.md, one source), av, defects, platform, decide; a set or a question by id |
        | raea-core-ask | core/ask/src/limits.rs     | 60    | clef's limits (questions, ids, options, levels, stills, videos, sounds, sizes, tokens, price), checked before anything is sent |
        | raea-core-ask | core/ask/library/av.json   | 80    | picture with sound: the sound fits what's on screen, the music's mood matches the picture, the music fights the voice, the voice suits who's on screen, the ambience matches the place, effects mark moments |
        | raea-core-ask | core/ask/library/defects.json | 70 | the fingerprint's axes with no labels yet: muffled voice, harsh music, glitch, fake, barren, unreadable text, shaky camera |
        | raea-core-ask | core/ask/library/platform.json | 160 | the check packs' clef questions, with their label words and calibration |
        | raea-core-ask | core/ask/library/decide.json | 50  | the best of these for the brief; the nearest to a reference; whether a fix changed what a viewer sees |
      Then a question is clef's own shape: `yes` (a probability of yes), `choose` (2 to 255 options), `grade` (ordered
        levels, poor to excellent unless given, read as the expected level) or `rate` (2 to 10 levels)
      And `when` says what a question needs to be asked (modes; a voice heard or none; a game), and `swap` names the
        question asked in its place when it doesn't apply, so `sound` to `sound-silent` is data, not code
      And every answer reads as a problem score from 0 to 1 whatever its type (a yes's probability on the answer that
        flags, a choice's on its flagging options, one less a grade's level over its top), so one calibration fits all
      And the limits are raea-api's where they're stricter than clef's (ids of 64 characters, 16 options), so a call that
        passes them passes on every route
      Status: done 11-20. raea-core-ask: four types, 49 standard questions in five sets (the rubric's read from
        rubric.md), the platform packs' three calibrated questions keeping their thresholds; its tests read every
        standard question, check the limits and the TypeScript declarations (sdk/raea/src/ask.gen.ts).

    @needs:AK1 @done
    Scenario: AK1b. One clef client, routed to the person's plan
      Given the plan
        | crate        | file                    | lines | job                                                                    |
        | raea-io-clef | io/clef/src/lib.rs      | 40    | the crate's exports                                                    |
        | raea-io-clef | io/clef/src/client.rs   | 160   | one call: state, questions, stills, videos and sounds as data URLs; its answers and cost; apps/judge clef.rs moved here and widened |
        | raea-io-clef | io/clef/src/media.rs    | 180   | the media cut with raea's FFmpeg: a window as 480 px H.264, at most 60 s; its sound as mono 16 kHz MP3; a still as JPEG at most 1,600 px |
        | raea-io-clef | io/clef/src/route.rs    | 160   | the route: the app's host (~/.raea/app-host.json, /host/ask) when signed in with a pool; RAEA_AGENT_KEY to raea-api; the person's Cloudflare keys; else a refusal that says how to get one |
        | raea-io-clef | io/clef/src/cache.rs    | 80    | answers by request hash in ~/.raea/ask/cache, so an unchanged question on unchanged media costs nothing |
      Then a call goes the first way that's open: the app (signing in stays the app's alone), an agent key with the
        `ask` scope (a pipeline, CI, raea cloud), the person's own `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN`
      And with none it refuses: "sign in to raea on Pro or Studio, set RAEA_AGENT_KEY, or add your Cloudflare keys
        (docs/settings.md)"
      And `raea judge` asks through it, and apps/judge's own clef.rs is gone
      Status: done 11-20. raea-io-clef's tests, on a fake transport: a route that can't answer hands on, a spent pool
        stops unless the person has their own keys, the plan's answer carries what's left and a repeat comes from the
        cache, none open refuses with how to get one, a call over the limits is never sent. A real call went the
        own-keys route (AK3's status); the app route answers once AK4 adds /host/ask. raea judge's tests pass on it.

  Rule: AK2-AK3. `raea ask`

    @needs:AK1b @done
    Scenario: AK2. Ask understands the question before it spends
      Given the plan
        | crate        | file                    | lines | job                                                                    |
        | raea-app-ask | apps/ask/src/prepare.rs | 260   | the file's facts (length, picture, sound, the words heard from a words file, whether anyone speaks) and the project's brief (title, mode, audience, believe, logline, voice); each question kept, swapped or dropped with why (not knowing whether anyone speaks, both variants kept); the state paragraph, with what the caller measured (`--state`; raea check passes its own, AK5) |
      Then a question about sound on a file with none, or a stills-only render, isn't asked, and says it waits for one
      And a piece where no one speaks is asked `sound-silent` in place of `sound`, and told it has no voice
      And the state says what the piece is (its modes, the brief's audience and belief, its storyboard's logline, what
        raea measured), as critique.ts writes it today
      And `--dry` prints exactly what would be sent (the state, each call's questions, the media and its size) and an
        estimated cost, and spends nothing
      Status: done 11-20. prepare.rs reads the file (ffprobe), the nearest brief.ts (`raea spec --as brief`) and a words
        file; apps/ask tests: a silent piece asks sound-silent with why, a file with no sound isn't listened to, the
        state names the modes, audience, belief and what the caller measured. `--dry` on an 8 s clip printed one call
        of 11 questions, its state, 26 KB of video and 63 KB of sound, about $0.0009, and sent nothing.

    @needs:AK2 @done
    Scenario: AK3. The runner, the record and the verb
      Given the plan
        | crate        | file                    | lines | job                                                                    |
        | raea-app-ask | apps/ask/src/lib.rs     | 60    | the crate's exports and its usage                                      |
        | raea-app-ask | apps/ask/src/verbs.rs   | 260   | `raea ask FILE [QUESTION]` (--choose, --grade, --rate, --set, --std, --from/--to, --at, --with, --brief, --dry, --json, --gate, --jobs), `--batch FILE`, `ask list` |
        | raea-app-ask | apps/ask/src/runner.rs  | 280   | the batch planned (grouped by file, window and media), each clip cut once, calls in parallel (4 at once unless --jobs), at most 64 questions each, answers streamed as they come; a pool spent midway keeps what came and reports the rest as waiting |
        | raea-app-ask | apps/ask/src/record.rs  | 160   | every answer in ~/.raea/ask/answers.jsonl (question and version, media hash, window, project, item, answer, cost, route); the cut media kept by hash in ~/.raea/ask/media, at most 2 GB, oldest out first |
      When the person or an agent runs `raea ask out/piece.mp4 "Does the music fight the voice?"`
      Then it answers in words with the probability, the window it looked at, what it cost and the pool left, and says
        it's advice until the question is calibrated
      And `raea ask --batch asks.json` (or JSON lines on stdin; each entry a file, a window, and questions, a set or
        standard ids) cuts each clip once and asks everything in parallel, and `raea check`, the critique and the
        fingerprint call the same runner
      And `--json` is a stable schema for pipelines, and `--gate` exits 1 when a calibrated question fails (`--gate any`:
        when any answer passes its threshold)
      And sdk/raea's `raea/ask` gives scripts `ask` and `askMany`, typed from the generated questions, over
        `raea ask --batch --json`, from a `.ts` check, a `raea run` script or plain Node
      And docs/manual/reference/cli/ask.md is its reference, written in the same change
      Status: done 11-20. One call on the person's own Cloudflare keys asked 4 questions of a test pattern with a tone
        for $0.0003 (test pattern 0.88 yes, anyone speaking 0.00, the sound fits 0.06), and the same again came from the
        cache for $0; calls.jsonl, answers.jsonl and the two cut clips were kept. apps/ask tests: 65 questions make two
        calls, windows and what a question looks at split calls, a pick or a before-and-after without `--with` isn't
        asked, a clip used by two calls is cut once, the voice answer sets the variant that doesn't apply aside, and a
        pool spent on the third call keeps two calls' answers and sends nothing more. `raea check`, the critique and
        the fingerprint move onto the runner in AK5, AK4 and AK8. docs/manual/reference/sdk/ask.md is `raea/ask`'s.

  Rule: AK4. On the person's plan

    @needs:AK3
    Scenario: AK4. The pool, through the app or an agent key
      Then raea-api's `/v1/clef/ask` takes sounds (up to 4 MP3s) beside stills and videos, says the pool left with
        each answer, and agent keys gain an `ask` scope that asks and can do nothing else
      And the app's host answers `/host/ask` by adding its device token and passing the call on, with the API's answer
        back as it came: a plan with no pool (402 plan_required) or signed out (401) sends the CLI on to the next route
      And the app's critique and game check ask through `raea ask --json`, and their own clef code is gone
      And the app's agents get an `ask` tool (HOST) with `raea ask`'s inputs
      Status 11-20: built, in review. raea-platform #16 (on #15): audio, the criteria clef takes, pool_left_usd and the
        `ask` scope, with test/pool.sh reading what a fake clef was sent. raea-feed's feed-ask branch (on #98):
        /host/ask, the `ask` tool, and the critique's judge on `raea ask --batch - --json` with `--brief none`, its
        tests, typecheck and the protocol and hub tests passing. It's done when they merge and raea-api is deployed,
        which waits on Shane's go.

  Rule: AK5. Standing questions in `raea check`

    @needs:AK3
    Scenario: AK5. A taste line that asks
      Then a taste line with `check: "ask"` is asked in its own words ("The music never fights the voice."), and
        `check: "ask:ID"` asks a standard question; its `values` can set the type, options, sees, hears and window
      And `raea check PROJECT` asks every such line of a composition in one batch on its newest render, waits without
        a render, says what it cost, and asks nothing again on an unchanged render
      And an answer is a finding at the line's level once its question is calibrated, and a note until then
      And `--no-ask` leaves them out

  Rule: AK6-AK7. Better with use

    @needs:AK3 @done
    Scenario: AK6. Labels
      Given the plan
        | crate        | file                    | lines | job                                                                    |
        | raea-app-ask | apps/ask/src/labels.rs  | 220   | an item's notes matched to the questions' label words (a match is yes, an approval with none is no); `raea ask verdict ANSWER yes|no`; an agent's own look kept as another eye |
      Then the person's labels count toward calibration, and an agent's are kept as `eye:agent` and never counted
      Status 11-20: done. A label belongs to a question and the clips a call sent (by hash), never to one answer, so
        new words asked of the same clips have the same labels. An approval is a post the person marked done (the
        feed's PL8c approval). A note labels the render it was left on: each call keeps its file's identity (size and a
        hash of its first and last MB), matched to the post's copy in the feed, so a later render to the same path
        isn't labelled by an earlier one's note. Only the person's notes count (author kind person), pinned inside the
        window (1.5 s before to 0.5 s after a pin) or not pinned. A verdict is the person's answer in the question's
        own terms (a question flagged on no takes no as the problem; a grade's lower half is one) and beats a note,
        which beats an approval. Tests: a_persons_note_on_the_posted_render_labels_it_and_an_approval_labels_it_clean,
        a_verdict_reads_in_the_questions_terms_and_another_eyes_isnt_counted

    @needs:AK6 @done
    Scenario: AK7. Calibrate, and try new words
      Given the plan
        | crate        | file                      | lines | job                                                                    |
        | raea-app-ask | apps/ask/src/calibrate.rs | 240   | per question version: its AUC and threshold at 10% false positives from the person's labels, through the fingerprint's gates; `ask try "new words" --like ID` replayed on the kept media |
      Then a question with 16 labels of each kind from 3 projects and a held-out AUC of 0.8 or more is calibrated, at
        its threshold
      And one under 0.6 after 32 labels says it doesn't separate, with its misses, so its words can be fixed
      And `raea ask try` asks the new words on the labelled answers' kept media and prints both AUCs, and a standard
        question's words change only with a better AUC on raea's bench
      And a standard question ships with raea's calibration from that bench, and the person's own replaces it once it
        passes the gates
      Status 11-20: done. `raea ask calibrate [ID]` reads the feed's labels again and holds each labelled version to
        them through the fingerprint's own auc, quantile and gates (raea-sys-fingerprint). Clef isn't trained on the
        labels, so its AUC on them is held out already; the threshold, the one thing fitted, flags 10% of the clean
        labels. A version that passes is kept in ~/.raea/ask/calibration.json and goes out with the question from then
        on (the runner's plan applies it, so every caller has it, and `raea ask list` shows it); one that stops passing
        loses it. `raea ask try` sends the new words with each labelled call's kept clips and state through the same
        runner, so its answers are recorded and calibrate as their own version later. The platform set's 17 questions
        ship with the packs' calibration (AK1); the other sets have none from raea's bench yet. Test:
        labels_from_three_projects_calibrate_a_question_and_new_words_are_measured_on_the_same_clips (18 of each from 3
        projects calibrate at AUC 1.0, flat answers don't separate and list their misses, and new words on the same 36
        clips go from 0.5 to 1.0)

  Rule: AK8. The fingerprint asks too

    @needs:AK3 @done
    Scenario: AK8. A double check on the fingerprint's flags
      Given the plan
        | crate                | file                     | lines | job                                                                    |
        | raea-app-double      | apps/double/src/lib.rs   | 160   | a score's flagged moments, each asked in its axis's own words on its own window through raea-app-ask's runner; the answers beside the flags and kept as evidence |
        | raea-app-fingerprint | apps/fingerprint/src/verbs.rs | 15 | `score --ask` and fingerprint.ts's `ask` hand the flagged moments to raea-app-double |
      And 11-20: the double check is its own crate, using raea-app-ask and the fingerprint, since raea-app-ask holds
        1841 of its 2000 lines after AK7
      Then `raea fingerprint score FILE --ask` (or `ask: "flags"` in fingerprint.ts) asks each flagged moment, and
        `ask: "unsure"` also each moment between an axis's plausible and strong bands
      And each flag says "clef agrees (0.71)" or "clef doesn't see it (0.08)", and each answer is an `eye:clef`
        evidence row that trains the head and never counts as the person's
      And it costs by the flags, never the whole piece
      And the app's rule_check asks the rules nothing measures, and the fingerprint with `--ask unsure`, before it asks
        the agent for its own verdicts, so the agent opens only the frames clef flags
      Status 11-20: done. raea-app-double (apps/double/src/lib.rs, 165 lines) asks each flag, and with `unsure` each run
        of windows an axis puts in its plausible band unflagged, as a yes/no question in the axis's first words (a
        quality axis is flagged on no), seeing the picture or hearing the sound as the axis does, on that moment's
        window, through raea-app-ask's runner: moments on the same window share a call, and the question carries the
        axis's words as its label words, so the person's notes calibrate it (AK6, AK7). A moment it couldn't ask (no
        route, the pool spent) says why, and the score goes on. Each answer replaces clef's last `eye:clef` row for that
        moment in the ledger. In the app (raea-feed), rule_check scores with `--ask unsure`: a gating flag clef doesn't
        see becomes one to look at, not to fix; advice flags clef sees none of hold; a moment the fingerprint was only
        unsure of is flagged where clef sees it; the rules nothing measures go to clef in one call, and the agent's
        verdict settles any of them. Tests: flags_are_asked_and_unsure_adds_the_plausible_windows_run_together,
        an_axis_asks_as_it_senses_and_a_quality_is_flagged_on_no,
        each_flag_is_asked_on_its_own_window_and_said_beside_it_and_kept_as_clefs; the feed's rules-tools tests. Real
        run: a 10 s render's five flags; clef saw no `dark` on a frame that is plainly lit (the fingerprint had it
        gating), agreed it's a computer-generated scene (0.96), $0.0007, and a second run cost nothing (cache). The
        same run showed the fingerprint judging sound axes on a render with no sound track (an empty clip's CLAP
        vector); fixed apart from AK8, in raea-sys-columns: sound is in a file only when it has levels

  Rule: AK9. Handing a question to a local head

    @needs:AK7,AK8
    Scenario: AK9. A measurement: can a calibrated question be learned locally
      Given one calibrated question with its kept answers and the person's labels
      When a fingerprint head is trained on its windows' SigLIP or CLAP columns from clef's answers
      Then its held-out AUC on the person's labels is set beside clef's
      And only if it matches does `raea check` ask the local head first and clef only in its unsure band
