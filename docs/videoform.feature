@spec
Feature: Videoform: the kind of video a person makes again and again, read once from their own finished videos, followed and checked on every cut
  The person, 2026-11-19: an agency or creator "making the same type of video over and over again can drop in a bunch
  of those videos, and raea codifies that style so the user can then drop a bucket of raw footage and raea with great
  accuracy and intent edits the raw footage into the exact style it has learned ... and it all needs to happen FAST".
  Then: the embedding route, on the models raea ships; "the point is to embed them, store the embeds, and be able to
  repeat the style without having to go back to the references themselves"; a videoform is "a kind of fingerprint,
  flowing into what has already been built". The plan, its decisions and why: docs/videoform.md.

  The model this file sets:
    1. A videoform is a fingerprint (docs/fingerprint.feature FP14) with a `videoform` section: its context, its
       references, its vocabulary, its template and its rules. It's found, inherited, taught and shared as a
       fingerprint is.
    2. Each part flows into the part of raea that does that job already: its ranges into its profile (FP6e), its rules
       into the section's `rules`, each in a taste line's shape (words, a check, its values, a level; DR4, DR8), its
       structure into a NAME.script.ts template, its references into brief.ts's `references` shape.
    3. A reference is read once, from the local drive or streamed from a URL, and never needed again: its per-shot rows,
       its vocabulary's vectors, its ranges, its structure and its rules are all that following and checking read.
    4. What stays the same across the references is the videoform; what varies is content. A rule's strength is its
       consistency, and every videoform is tested leave one out.
    5. What ships runs on the models raea ships, on the person's machine. The domain vocabulary is written by the
       person's own AI. No paid model API is in the product.
    6. Checks start from the composition: a videoform's rules run in `raea check PROJECT` before any render, and
       embeddings only for shots that changed.

  Start here (the order of work, and where it touches what exists):
    - The order: VF1 and VF2 (a reference in, a videoform made), VF3 (each reference read once into the kept rows), then
      VF4 with VF12 (the ranges, judged on the test bench). That pair is the gate: it passes on at least five of the
      person's sets before VF1b's platform pages or anything in VF9-VF11 is built.
    - New crates hold the work: raea-app-videoform (apps/videoform: ingest, rights, reading, the verbs, the bench),
      raea-sys-videoform (systems/videoform: the maths over kept rows) and raea-io-resolve (io/resolve, VF1b). The
      videoform section's types are core/brief's videoform.rs.
    - Three small hooks into what exists: fingerprint.ts's optional `videoform` field (systems/fingerprint taxonomy.rs,
      VF2), `raea context` printing a project's videoform (engine/cli guide.rs, VF2), and `raea check PROJECT` holding
      the rules of the fingerprint that judges the piece (apps/direction check.rs, VF8).
    - The bench (VF12) runs after every step, on every set, from the kept files alone; its numbers are the progress.

  Background:
    Given the readers raea has: footage perception (systems/footage), Fingerprint's columns (systems/columns: SigLIP 2,
      CLAP, YAMNet, cached by content hash), Parakeet's words, Beat This!, HTDemucs and Silero VAD (systems/song), OCR,
      Depth Anything 3, and raea check's one decode of the picture (raea-app-check-read once.rs)
    And Fingerprint's fingerprint.ts (`about`, `extends`, `applies`, axes, encoder pins), its profiles (FP6e: ranges by
      measure name, critique, leave-one-out), `which` (FP14c) and evidence rows (FP2c)
    And taste lines' checks (raea's six forms, a check by name, a `.ts` check written with raea/check over the world;
      DR4), run by `raea check PROJECT`
    And the crate limits (a file at most 500 lines, a crate at most 2,000): raea-sys-fingerprint, -fingerprint-use and
      raea-app-fingerprint are each within 150 lines of theirs, so a videoform's code is its own crates, and the
      fingerprint crates grow only by a field and a map
    And the layers: a systems crate depends sideways only on a facility, so reading (which uses footage, columns and
      song) is apps' (raea-app-videoform), and the maths over kept rows is systems' (raea-sys-videoform)

  Rule: VF1. Videos in: from the local drive, or streamed from a URL, read once

    @needs:VF2
    Scenario: VF1. A reference from the drive or a URL, with nothing downloaded
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-app-videoform | apps/videoform/src/ingest.rs      | 220   | paths, folders and URLs to sources: content hash, source URL, title, creator, date; a source read before is recognised and not read again |
        | raea-app-videoform | apps/videoform/src/rights.rs      | 80    | the rights prompt, once per account, and its answer kept with the account |
      When the person runs `raea videoform add NAME PATH|URL...`
      Then a file or a folder on the drive is read where it is, with no copy made
      And a direct link to a media file is decoded straight from the network, and a platform page, a playlist or a
        channel (`--last N`) is resolved (VF1b) into stream URLs that are decoded the same way
      And nothing but the readings is written: no video file is kept, in a cache or anywhere else
      And each source is known by the content hash of its decoded picture and sound, beside its URL, title, creator
        and date, so adding it again reads nothing
      And before the first source from a URL, once per account, the person confirms they have the right to use what
        they add; raea says a videoform keeps measurements and vectors, never the video, and that a platform's terms
        are the person's to keep
      And a login-only or private link is refused with the site's reason, and streams are read at 1080p at most

    @needs:VF1
    Scenario: VF1b. The resolver: yt-dlp, kept current by raea
      Given the plan
        | crate           | file                       | lines | job                                                                    |
        | raea-io-resolve | io/resolve/src/lib.rs      | 60    | a page URL to its videos' stream URLs and headers                      |
        | raea-io-resolve | io/resolve/src/fetch.rs    | 220   | the standalone yt-dlp for this platform fetched into ~/.raea/bin, checked against the release's SHA-256 sums; the pinned version, the daily move forward, the one before kept |
        | raea-io-resolve | io/resolve/src/formats.rs  | 140   | `yt-dlp -J` read: the best stream at 1080p or under, its URL and headers; a playlist's or channel's entries |
      Then yt-dlp (Unlicense) is used only to resolve, never to download, and is fetched on first use as FFmpeg is on
        Windows
      And each raea release pins the version it was tested with; between releases raea checks the latest at most once
        a day when a URL is added, checks it against its sums, and keeps the version before
      And when resolving fails with an extractor error, raea fetches the latest and tries once more; if the new one
        fails where the old one worked, it goes back and says which site failed
      And a site that needs JavaScript run (YouTube) gets a runtime raea ships or fetches, named to yt-dlp
      And `raea doctor` prints the yt-dlp in use, its date and the last site that failed

  Rule: VF2. A videoform is a kind of fingerprint

    Scenario: VF2. `videoform` in fingerprint.ts, and `raea videoform new`
      Given the plan
        | crate                | file                                 | lines | job                                                                    |
        | raea-core-brief      | core/brief/src/videoform.rs          | 140   | the videoform section's types (context, references, vocabulary, template, rules), with their TypeScript |
        | raea-sys-fingerprint | systems/fingerprint/src/taxonomy.rs  | 20    | the optional `videoform` field on a fingerprint                       |
        | raea-app-videoform   | apps/videoform/src/lib.rs            | 120   | `raea videoform`: its usage, flags and the verbs' dispatch             |
        | raea-app-videoform   | apps/videoform/src/verbs.rs          | 240   | new, add, report and refresh                                           |
      When the person runs `raea videoform new NAME --context "…" [--extends FP] [--kinds K] [--tags T] [SOURCE...]`
      Then a fingerprint folder is made with a `videoform` section, `extends` the person's fingerprint by default, and
        `applies` from --kinds and --tags, where fingerprints are kept (the project's, the workspace's or the person's)
      And `raea fingerprint which PATH` finds it for a piece as it finds any fingerprint, and says why
      And `raea context` prints a project's videoform with its brief: its context, template, rules and ranges
      And its references use brief.ts's `references` shape, `{ target, note }`, the target the source's hash and URL

  Rule: VF3. Each reference read once, into the rows a videoform keeps

    @needs:VF1
    Scenario: VF3. One pass, every reader on its share
      Given the plan
        | crate              | file                            | lines | job                                                                    |
        | raea-app-videoform | apps/videoform/src/pass.rs      | 300   | one decode of a file or a stream: shot bounds from the same pass, each shot's frames held until it ends, each reader fed |
        | raea-app-videoform | apps/videoform/src/readers.rs   | 260   | perception, columns, words, the voice's pauses, beats, stems, OCR and depth into per-shot rows |
        | raea-sys-videoform | systems/videoform/src/rows.rs   | 160   | the kept rows: per shot its SigLIP 2 vector, its CLAP vector and its measures, f16, with the encoders' pins |
      Then each reference is decoded once, whether from the drive or a stream, and every reader takes its share
      And each shot gets its SigLIP 2 vector (a few frames pooled), its CLAP vector over its sound, and its measures:
        length, scale, camera move, caption boxes, light and colour, words, pauses at its edges, its place in the video
      And reading runs at about real time or faster, measured on the test sets

    @needs:VF3
    Scenario: VF3b. What's kept is enough
      Then a videoform keeps its per-shot rows, its vocabulary's vectors, its profile, its structure and rules, a small
        still for each shot kind, and the person's evidence, and no reference video
      And ten one-minute references come to about 1.5 MB, small enough to load at once and keep in git
      And every test on the test sets (VF12) reads a set's references once, then runs on the kept files alone, with the
        references moved away, and passes the same

  Rule: VF4-VF8. Writing the videoform from the kept rows

    @needs:VF3
    Scenario: VF4. The ranges, in the videoform's profile
      Given the plan
        | crate                    | file                                     | lines | job                                                                    |
        | raea-sys-videoform       | systems/videoform/src/ranges.rs          | 220   | the measures across the references: shot lengths, cuts a minute, pauses inside a line, between lines and between sections, words a minute, cuts against beats and sentence ends, music under voice, caption size, place and words at once, talking head against cutaway |
        | raea-sys-fingerprint-use | systems/fingerprint-use/src/profile.rs   | 30    | a profile's measures beyond the fixed ones, by name, ranged and critiqued like the rest |
      Then the videoform's profile holds every measure's range across the references, leave one out as FP6e checks it
      And `raea fingerprint score --profile NAME` critiques a cut against the new measures as against the old
      And a measure whose range is wide is recorded as free, not as part of the videoform

    @needs:VF2,VF3
    Scenario: VF5. The vocabulary, written by the person's AI
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/vocab.rs    | 180   | vocabulary.ts's terms against the kept rows: never fires, fires everywhere, confused pairs, recurring shots no term names |
      When the person's AI writes vocabulary.ts from the context (places, things in them with several phrasings,
        near-misses, OCR patterns for text on screen, things to ignore)
      Then its terms become the videoform's word heads (FP2e), embedded once by the text towers
      And `raea videoform report NAME` says what each term did on the references, and the AI revises it from that

    @needs:VF5
    Scenario: VF6. The shot vocabulary
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/kinds.rs    | 220   | the reference shots clustered on vectors and measures; each cluster named from the vocabulary, its ranges and how many references have it |
      Then every reference shot belongs to a shot kind, named in the vocabulary's words, with its scale, move and length
        ranges and the share of references that have it

    @needs:VF6
    Scenario: VF7. The structure, as a script template
      Given the plan
        | crate              | file                                 | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/structure.rs   | 300   | the references aligned by their run of shot kinds and their script's meaning, into slots with their lengths and kinds; the template as NAME.script.ts |
      Then the videoform's slots (open, sections, close) each have a length range, their shot kinds and what's on screen
      And the template is a NAME.script.ts a project made to the videoform starts from, checked by `raea spec`

    @needs:VF6,VF7
    Scenario: VF8. Rules, with their strength, in the videoform's rules
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/rules.rs    | 240   | coverage, order and event rules from the kept rows, each with how strongly it holds; written into the section's `rules`, each a taste line with its check and values |
        | raea-app-direction | apps/direction/src/check.rs       | 15    | tastes(): the rules of the fingerprint `which` names for the project, after the person's, the workspace's and the project's taste |
      Then a rule that holds in most references (a punch-in when a number is said; a close-up of the counters in every
        kitchen) is a line in the section's `rules`, with its strength and its check
      And `raea check PROJECT` holds a piece to the rules of the fingerprint that judges it (raea-sys-fingerprint-use
        `which`), beside the person's, the workspace's and the project's taste
      And the person confirms the strongest rules once, on a contact sheet of shot-kind stills, and each answer is
        evidence in the videoform

    @needs:VF4,VF6
    Scenario: VF8b. When it doesn't hold together
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/groups.rs   | 180   | too few references, references in groups, traits that vary, outliers  |
      Then under five references every rule and range is weak, and nothing gates on it
      And references that fall into clear groups are reported, with a videoform offered for each, never averaged
      And one reference far from the others on several measures is named, for the person to keep or drop

    @needs:VF3
    Scenario: VF8c. References added and removed; encoders upgraded
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/pins.rs     | 100   | the encoders' pins on the kept rows; what carries over an upgrade and what waits for refresh |
      Then adding a reference reads only it and makes the videoform again from the kept rows, in seconds; removing one
        drops its rows; the person's evidence stays through both
      And a new encoder in raea leaves the videoform on its rows' pins, working as it was; the measures carry over; `raea
        videoform refresh NAME SOURCE...` reads the references again for the new encoder, the one time they're needed

  Rule: VF9-VF11. Following it, and checking it fast

    @needs:VF7,VF8
    Scenario: VF9. Following: the footage matched, what's missing said, the cut planned to the template
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-app-videoform | apps/videoform/src/follow.rs      | 200   | new footage read and matched to shot kinds; what the footage lacks; the plan slot by slot |
      Then new raw footage is read as references are, and each shot matched to a shot kind against the kept rows
      And before any cut the agent says what the footage lacks ("no close-up of the counters, which every kitchen in the
        references has"), a shot list for the person
      And the agent plans the cut from the template, slot by slot, and edits to the ranges

    @needs:VF8
    Scenario: VF10. A videoform's rules checked on the composition, before any render
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-app-videoform | apps/videoform/src/checks.rs      | 220   | the videoform's checks by name over the world: order, slot lengths, coverage, shot lengths, pauses, caption places and sizes |
        | raea-app-direction | apps/direction/src/check.rs       | 15    | the videoform's checks run with the project's rules                    |
      Then `raea check PROJECT` runs the videoform's rules with raea's floor and the person's taste, in milliseconds
      And each finding is routed as raea-app-check-cause routes any: a footage's own fault a note, a carried one said

    @needs:VF6,VF10
    Scenario: VF11. Embedding checks, only for shots that changed
      Then each changed shot is held against its slot's shot kind, one SigLIP 2 frame a shot, and the run of shots
        against the structure
      And a shot that didn't change since the last check is not embedded again

  Rule: VF12-VF13. Judged on the person's test sets

    @needs:VF4
    Scenario: VF12. The test bench
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-app-videoform | apps/videoform/src/bench.rs       | 200   | the person's sets as fixtures: leave one out, telling sets apart, edit to it, the verdicts kept |
      Given the person's sets, 8 to 10 videos each from one creator or one format, some with raw footage
      Then for each set, a videoform built from all but one reference holds the one left out (its rules pass, its
        measures in range) more often than other sets' videos do, and rejects other sets' videos
      And where raw footage for a held-out reference exists, the agent edits it to the videoform, set beside the original
      And the bench runs after every step of this file, on every set, from the kept files alone
      And the first proof that matters: on at least five sets, each videoform built leave one out holds its left-out
        video's pacing and shot lengths and rejects other sets' videos, more often than chance by a clear margin

    @needs:VF12
    Scenario: VF13. The person's blind judging, kept as evidence
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-app-videoform | apps/videoform/src/judge.rs       | 150   | a pair shown unlabelled in a random order; the pick, the scores and the words kept, and as evidence |
      Then the person sees the original and raea's edit side by side, unlabelled, in a random order, picks which they'd
        post, scores pacing, shot choice, structure, graphics, sound and overall feel from 1 to 5, and says what's wrong
      And each verdict is kept in the bench's record and is evidence in the videoform: a pick a pair (FP6b), a note a
        label (FP5)

  Rule: Deferred
    Scenario: Not in this file yet
      Then an automatic judge for the test bench (a video-capable language model, paid by the person, in the bench only,
        never shipped: fidelity, pairs, and gaps between what the references show and what the videoform holds)
      And the order of the test sets
