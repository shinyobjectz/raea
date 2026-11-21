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
       into a controller's rule fields (DR4, DR8), its structure into a NAME.script.ts template, its references into
       brief.ts's `references` shape.
    3. A reference is read once, from the local drive or streamed from a URL, and never needed again: its per-shot rows,
       its vocabulary's vectors, its ranges, its structure and its rules are all that following and checking read.
    4. What stays the same across the references is the videoform; what varies is content. A rule's strength is its
       consistency, and every videoform is tested leave one out.
    5. What ships runs on the models raea ships, on the person's machine. The domain vocabulary is written by the
       person's own AI. No paid model API is in the product.
    6. Checks start from the composition: a videoform's rules run in `raea check PROJECT` before any render, and
       embeddings only for shots that changed.

  Background:
    Given the readers raea has: footage perception (systems/footage), Fingerprint's columns (systems/columns: SigLIP 2,
      CLAP, YAMNet, cached by content hash), Parakeet's words, Beat This!, HTDemucs and Silero VAD (systems/song), OCR,
      Depth Anything 3, and raea check's one decode of the picture (raea-app-check-read once.rs)
    And Fingerprint's fingerprint.ts (`about`, `extends`, `applies`, axes, encoder pins), its profiles (FP6e: ranges by
      measure name, critique, leave-one-out), `which` (FP14c) and evidence rows (FP2c)
    And controllers' `rule` fields and raea/check's `.ts` checks over the world (DR4), run by `raea check PROJECT`
    And the crate limits (a file at most 500 lines, a crate at most 2,000): raea-sys-fingerprint, -fingerprint-use and
      raea-app-fingerprint are each within 150 lines of theirs, so a videoform's code is its own crates, and the
      fingerprint crates grow only by a field and a map
    And the layers: a systems crate depends sideways only on a facility, so reading (which uses footage, columns and
      song) is apps' (raea-app-videoform), and the maths over kept rows is systems' (raea-sys-videoform)

  Rule: VF1. Videos in: from the local drive, or streamed from a URL, read once

    @needs:VF2 @done
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
      And on 11-20 (raea-app-videoform, raea-io-resolve): `raea videoform add NAME SOURCE... [--from LIST] [--last N]
        [--context] [--sheet DIR]` reads a drive file in place (a 62.5 s 1080x1920 render: 21 shots, -14.4 LUFS, read in
        44 s; added again, known by its hash in 2 s) and a site's page with its own player (streettalk.com's AG1 street
        interview, a Bunny CDN file served only to its own page: resolved by yt-dlp's generic extractor with the page as
        referrer; 44.6 s 720x1280, 28 shots, 36 cuts a minute, median shot 1.2 s, -14.7 LUFS, read in 28 s); a reference
        is a line of ~/.raea/videoforms/NAME/references.jsonl (source, title, creator, date, views, shots, pace,
        loudness, about 1.5 KB) with its columns in the column cache by content hash
      And a link's chosen streams are held in a private folder while the readers read them, and the folder is removed
        when they're done, whether they succeed or fail (a stopped run's is swept on the next): the readers take a file,
        so reading the stream itself without holding it is VF3's one pass; the content hash is the held files' sha256
        until then. VF2 makes the folder fingerprint.ts's `videoform` section
      And on 11-21, the social sites (the person: "We need to be able to 'watch' videos from all the social media
        sites, especially Instagram, Facebook and TikTok"): an Instagram reel and a Facebook reel resolve with no login
        and read end to end. Three gaps in reading what they serve, each fixed where it was and not by site: the
        DASH streams Instagram and YouTube serve are fragmented MP4, whose frames are in moofs (raea media probe read 0
        of them; raea-app-media fragments.rs); macOS doesn't decode the reel's VP9, so a codec raea decodes comes
        before a taller picture (the plain H.264 file beside it; raea-io-resolve formats.rs); and their sound is
        HE-AAC, which Symphonia decodes at half the rate its track states, so read at the stated rate it lasted half as
        long (274 words a minute); the decoder's own rate now, the same on every platform (raea-io-soft audio.rs),
        rows VERSION 4 to read every reference again, and a reading whose sound and picture differ by over a second is
        refused. TikTok's server refused the person's post ("Your IP address is blocked from accessing this post") to
        yt-dlp from a home connection; raea doesn't get round a site's refusal, and the file saved from the app is
        added from the drive

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

    @done
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
      And on 11-20: `raea videoform new street-interview --context "…" URL` made the fingerprint through `raea fingerprint
        new` (raea's base axes, its words and heads; --extends the account's default or `me` when there is one), wrote the
        section (context, references) into its fingerprint.ts, and read the AG1 page into videoform/references.jsonl
        beside it; with no --kinds or --tags it applies to projects tagged with its name, and fingerprint new's account
        `default` is taken back, so a videoform never judges everything: a project tagged `street-interview` gets it from
        `raea fingerprint which` and a VIDEOFORM section in `raea context` after the brief, an untagged one doesn't, and
        which()'s account fallback skips videoforms (raea-sys-fingerprint-use's test). `add` makes the videoform first
        when it's new. A reference's target is its link (or its path), its note the title and creator; its content hash
        is its line's in references.jsonl. `report` and `refresh` come with VF5 and VF8c

  Rule: VF3. Each reference read once, into the rows a videoform keeps

    @needs:VF1
    Scenario: VF3. One pass, every reader on its share
      Given the plan
        | crate              | file                            | lines | job                                                                    |
        | raea-app-videoform | apps/videoform/src/pass.rs      | 270   | one decode of a file or a stream: shot bounds from the same pass, each shot's frames held until it ends, each reader fed |
        | raea-app-videoform | apps/videoform/src/readers.rs   | 230   | perception, columns, words, the voice's pauses, beats, stems, OCR and depth into per-shot rows |
        | raea-sys-videoform | systems/videoform/src/rows.rs   | 160   | the kept rows: per shot its SigLIP 2 vector, its CLAP vector and its measures, f16, with the encoders' pins |
      Then each reference is decoded once, whether from the drive or a stream, and every reader takes its share
      And each shot gets its SigLIP 2 vector (a few frames pooled), its CLAP vector over its sound, and its measures:
        length, scale, camera move, caption boxes, light and colour, words, pauses at its edges, its place in the video
      And reading runs at about real time or faster, measured on the test sets
      And on 11-20 (progress, not done): the readers run side by side on the held files, each decoding them itself:
        frame-by-frame cuts (raea media cuts) give the shots, and footage perception reads each of them (raea-perceive
        analyze --shots: framing, faces, camera, brightness, look, scene labels, words on screen with their boxes; its
        own cut rule, built for raw footage, folded three cuts of the AG1 interview's first 4 s into one shot);
        Parakeet the words, and the pauses at each shot's edges from them; Beat This! the beats, and each cut's
        distance from one; YAMNet's own AudioSet scores (Speech, Music) what's heard per shot (CLAP's words against
        everyday sounds called a raw phone clip of a man talking music, so they aren't used); SigLIP 2 and CLAP pooled
        per shot into the rows (raea-sys-videoform rows.rs), about 90 KB a minute-long reference. A 62.5 s 1080x1920
        video read with nothing cached in 44.6 s (0.71x real time); AG1 (44.6 s) in 15 s with its columns cached
      And not read yet: stems (HTDemucs) and voice activity (Silero) aren't in a release, nor Core ML YAMNet, which
        footage perception's sound spans need (perception now goes without them rather than failing), nor depth
        (perception's framing gives the scale); one decode shared by every reader is still to do, and with the readers
        side by side it matters less for speed than the plan thought (11-21: pass.rs and readers.rs re-planned from what
        they came to, 215 and 164 lines, with the shared decode, stems and voice activity still to come)
      And on 11-21 (later): a reading keeps a 160 px still from each shot's middle (VF8's contact sheets), and one
        without them is read again; footage perception read the words on screen 30 times a video, so a 3-minute reel's
        captions stopped a minute in (37 words read on one, 93 now): it reads every 2 s of the video, 30 to 300 times.
        Its framing is a person's, so an empty room is "no people" at any scale (all 121 shots of the walkthroughs).
        On the debug build (no release build on hand), with nothing else running, a reading took 1.9 times the
        reel's length for the 2-3 minute reels and 3.4 times for StreetTalk's 40 s ads; not comparable with the
        release build's 0.71 above, and not measured again on one

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
        | raea-sys-videoform       | systems/videoform/src/ranges.rs          | 310   | the measures across the references: shot lengths, cuts a minute, pauses inside a line, between lines and between sections, words a minute, cuts against beats and sentence ends, music under voice, caption size, place and words at once, talking head against cutaway |
      Then the videoform's profile holds every measure's range across the references, leave one out as FP6e checks it
      And `raea videoform score NAME FILE` critiques a cut against the new measures, as `raea fingerprint score --profile`
        does against the old (its measures need the cut read as a reference is, so it's the videoform's verb, and
        Fingerprint's profile and its fixed measures stay as they are)
      And a measure whose range is wide is recorded as free, not as part of the videoform
      And on 11-21 (progress, not done): 21 measures from the kept rows alone (length; cuts a minute; median, longest-tenth
        and first shot; words a minute spoken; the voice's pause at a cut; close-up, medium, wide and empty shots; faces;
        camera moving; brightness; words on screen; captions of what's said and where they sit; speech; music; cuts on
        the beat) and loudness and true peak from each reading, into videoform/found.json, each with its range, its
        leave-one-out share and holds, loose or free by its width. On StreetTalk's 8 drive-to-retail ads, one of them
        scored against the videoform has every checked measure in range, and a drone overview of a town has 8 of 15 out
      And not measured yet: pauses inside a line, between lines and between sections; cuts against sentence ends; music
        under the voice (only whether it's heard); caption size and words at once; talking head against cutaway (VF6's
        shot kinds will say)

    @needs:VF3 @done
    Scenario: VF4b. What it found, said to the person, and their corrections kept
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-core-brief    | core/brief/src/videoform.rs       | 40    | the section's corrections: a measure, what the person says of it (ignore, free, keep, target and its range) and why |
        | raea-app-videoform | apps/videoform/src/found.rs       | 180   | the report after every add and on `report`; `set`, `unset` and `score` |
      Given the person, 2026-11-20, of the drive-to-retail set's loudness (three references 9 LU quieter than the rest):
        "I would tell you to ignore the loudness difference in this videoform and tell you to deliver all videos with
        proper mastering"
      Then after every `add`, and on `raea videoform report NAME`, the person reads what the references hold, a line a
        measure: its range, holds, loose or free, the two groups the references fall in when they do (one cutting about
        every 1.2 s, another every 2.5 s), and the references far from the rest on three measures or more
      And `raea videoform set NAME MEASURE ignore|free|keep|target [LO HI] --why "…"` is their word over the references',
        kept in fingerprint.ts's section with why, through references added and read again; `unset` takes it back
      And a measure is decided by raea's delivery floor first (loudness at -14 LUFS, true peak at most -1 dBTP: the
        references' range is reported, never followed, and those far from the floor are named as likely not mastered),
        then the person's word, then the references
      And on 11-21: built, and the person's loudness correction is the first, on st-drive-to-retail

    @needs:VF2,VF3 @done
    Scenario: VF5. The vocabulary, written by the person's AI
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/vocab.rs    | 320   | vocabulary.ts's terms against the kept rows: never fires, fires everywhere, fires with another, missing where the context says always; each section and where it's found; recurring shots no term names |
        | raea-core-brief    | core/brief/src/videoform.rs       | 120   | vocabulary.ts's types: places and things, sounds, patterns on screen and said, sections, feel words, things to ignore |
        | raea-app-videoform | apps/videoform/src/vocab.rs       | 300   | `raea videoform vocab` (a vocabulary.ts started from the context), the phrasings embedded once and kept, the report |
      Given the person, 2026-11-19, of a real estate walkthrough: "raea should be on the lookout for all the real estate
        specific elements -- maybe every time the creator shows a kitchen they show a closeup of the countertops": the
        context says what matters, and the references show how it's done
      When the person's AI writes vocabulary.ts from the context (places, things in them with several phrasings,
        near-misses, OCR patterns for text on screen, things to ignore)
      And, of the context's claims about form and feel (the person's drive-to-retail brief, 11-20: "There is always a host
        and one or more subjects ... usually some sort of challenge or game ... Cutting is snappy ... always end with
        some sort of call to action"): words said as patterns, what the context says is always or usually there, the
        sections the videos move through (open, middle, close, each shown by terms and patterns named), and feel words
        with the measures that say them
      Then its terms become the videoform's word heads (FP2e), embedded once by the text towers
      And `raea videoform report NAME` says what each term did on the references, and the AI revises it from that
      And a feel word keeps its measures checked though the references vary, after the floor and the person's own word
        and before the references (VF4b's order: floor, the person, the context, the references)
      And on 11-21: built. `raea videoform vocab NAME` starts vocabulary.ts from the context; every `add` and `report`
        says what it did, or that the AI hasn't written it yet. On StreetTalk's 8 drive-to-retail ads, written from the
        brief: the host and a stranger in all 8, the call to action found at the close of all 8 (from 2 to 10 s before
        the end), the hook in all 8, the challenge in all 8, the reactions in 7; one round of revision took "tasting"
        from 120 shots to 38 with near-misses, but a whole shot's pooled picture vector still calls a stranger talking in
        three ads with no food "tasting": word heads on pooled shots are advice, and VF6's shot kinds and per-frame
        vectors are where finer things are told apart. 7 s for the report with 108 phrasings embedded, under a second
        once they're kept

    @needs:VF5 @done
    Scenario: VF6. The shot vocabulary
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/kinds.rs    | 280   | the reference shots clustered on vectors and measures; each cluster named from the vocabulary, its ranges and how many references have it |
      Then every reference shot belongs to a shot kind, named in the vocabulary's words, with its scale, move and length
        ranges and the share of references that have it
      And on 11-21: built. A shot is described by its pooled picture vector against the vocabulary's terms, their
        near-misses and the background, standardised over every shot and less half its own reference's mean (on raw
        vectors, one reference's shots are nearer each other than the same kind of shot in another: its street, its
        people), with its scale, people and talk beside; average linkage gathers them, and a group in one reference
        only is that reference's content, its shots joining the nearest kind when near enough. On StreetTalk's 8: ten
        kinds, 3 of 195 shots their own reference's; two kinds (60 shots) no term names, said as "unnamed" with what's
        seen, for the AI to name; "tasting", 38 shots as a word head on single shots, is a kind of 5 shots in 4
        references. Kept in videoform/kinds.json, said after the vocabulary

    @needs:VF6 @done
    Scenario: VF7. The structure, as a script template
      Given the plan
        | crate              | file                                 | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/structure.rs   | 270   | the references aligned by their run of shot kinds and their script's meaning, into slots with their lengths and kinds; the template as NAME.script.ts |
      Then the videoform's slots (open, sections, close) each have a length range, their shot kinds and what's on screen
      And the template is a NAME.script.ts a project made to the videoform starts from, checked by `raea spec`
      And on 11-21: built. Each reference is cut at the vocabulary's sections, in order (a section starts at the first
        shot it's shown on after the one before's start; the first found starts the video; with no sections, the open,
        the middle and the close), into slots with their start, length, shots, kinds and what's said; the template is
        brief.ts's Script, a scene a slot and a beat a kind, with what's said as the payoff. Sections shown by the same
        terms are said to the AI, which can't tell where one ends. On StreetTalk, two rounds as the person's AI: the
        hook shared "the game" with the challenge (hook 16.5 s), and the reactions, all through the game, made a
        section of their own after it (25 s); given its own terms and folded in, the hook is 8.0 s (6.2 to 9.3), the
        game 31.4 s (25.4 to 37.2), the call to action 3.6 s (2.2 to 5.8), each in 8 of 8, and `raea spec` reads it

    @needs:VF6,VF7 @done
    Scenario: VF8. Rules, with their strength, into a controller
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-sys-videoform-rules | systems/videoform-rules/src/lib.rs | 360 | coverage, order and event rules from the kept rows, each with how strongly it holds; written as rule fields of the controller `videoform.rules` names |
      # Fixed in the code (VF8, 11-21): rules.rs was raea-sys-videoform's at 240 lines. It came to 360, and with
      # VF8b and VF8c still to come that crate was full, so the rules are a crate of their own beside it.
      Then a rule that holds in most references (a punch-in when a number is said; a close-up of the counters in every
        kitchen) is a rule field with its strength and its check
      And the person confirms the strongest rules once, on a contact sheet of shot-kind stills, and each answer is
        evidence in the videoform
      And on 11-21: seven sorts of rule, each held to a named check for VF10 (videoform-covers, -opens, -closes,
        -slot, -order, -together, -event) with its values: what every video has (a kind, or a term shown, said or
        heard), its first and last shot, what a section has more than the rest of the video (1.5 times as often), which
        section comes first, a place's things in every run of its shots, and what the shot or the next does when a word
        is said or shown, 1.5 times as often as after any shot (what opens the video, and words on screen after words
        on screen, are left to the opens rules). A rule holds in 70% of the references it could, three or more; strong
        (a warning) at 85% with five references or more, else a note; `raea videoform set NAME RULE keep` makes it an
        error, `free` a note, `ignore` drops it, kept in fingerprint.ts as a correction with why. Written as
        NAME.rules.ts (a controller, `raea spec` checks it) and videoform/rules.json, and `videoform.rules` names the
        file (it was a list of taste lines). Each reading keeps a 160 px still a shot (videoform/stills/HASH/N.jpg,
        about 10 KB each), and every report makes videoform/kinds/kind-N.jpg, a kind's stills across the references
      And on Flagstaff's 7 walkthrough reels (Brannon Harbur, Instagram): the brief's opening graphic is found, "Its
        first shot shows Flagstaff on screen" in 6 of 7 (the one without is a Scottsdale home) and the price in 6 of 7
        (the one without says "two homes for the price of one"); "When the price is said, the shot or the next shows
        price on screen" (5 of 6, 4.5 times as often as elsewhere); the title card before the tour, the tour before the
        ask. On StreetTalk's 8: "When laughter is heard, the shot or the next shows reaction" (3 of 3), "When store is on
        screen, the shot or the next shows product in hand" (6 of 6), the call to action shows the product in hand (7
        of 8). 23 and 37 rules; 1.5 s a report
      And not yet: the plan's "a close-up of the counters in every kitchen" isn't found on the reels: footage
        perception's framing is a person's (an empty room reads "no people", never a close-up), and "the kitchen" fires
        in 2 of 7 references, so `together` has too few to hold in. The sheets show it: Flagstaff's kind named "the
        counters, the kitchen" is bathrooms, closets and a laundry, named by weak scores (VF6's naming), which is what
        the person's look at the sheet is for

    @needs:VF4,VF6
    Scenario: VF8b. When it doesn't hold together
      Given the plan
        | crate              | file                              | lines | job                                                                    |
        | raea-sys-videoform | systems/videoform/src/groups.rs   | 110   | references in groups across the measures, a videoform offered for each |
      # Fixed in the plan (VF8, 11-21): groups.rs was 180 lines for four jobs, three of them built since in VF4b's
      # ranges.rs: under FEW references every range is weak and nothing gates (and VF8's rules are notes), a measure
      # whose references fall in two clear groups says so, and a reference far from the rest on three measures or
      # more is named. What's left is the references grouped across the measures, and a videoform offered for each.
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
        | raea-app-videoform-checks | apps/videoform-checks/src/lib.rs | 220 | the videoform's checks by name over the world: order, slot lengths, coverage, shot lengths, pauses, caption places and sizes |
        | raea-app-direction | apps/direction/src/check.rs       | 15    | the videoform's checks run with the project's rules                    |
      # Fixed in the plan (VF8, 11-21): checks.rs was raea-app-videoform's, which holds the reading (the encoders,
      # footage perception, Parakeet) and was full with VF9 to come. raea check runs the checks over a world in
      # milliseconds and needs none of that, so they're a crate of their own that apps/direction calls.
      Then `raea check PROJECT` runs the videoform's rules with raea's floor and the person's taste, in milliseconds
      And each finding is routed as raea-app-check-cause routes any: a footage's own fault a note, a carried one said

    @needs:VF6,VF10
    Scenario: VF11. Embedding checks, only for shots that changed
      Then each changed shot is held against its slot's shot kind, one SigLIP 2 frame a shot, and the run of shots
        against the structure
      And a shot that didn't change since the last check is not embedded again

  Rule: VF12-VF13. Judged on the person's test sets
    (The bench is its own crate, raea-app-videoform-bench, 11-21: raea-app-videoform with following and checking
    planned in comes to its crate limit.)

    @needs:VF4
    Scenario: VF12. The test bench
      Given the plan
        | crate                    | file                              | lines | job                                                                    |
        | raea-app-videoform-bench | apps/videoform-bench/src/bench.rs       | 200   | the person's sets as fixtures: leave one out, telling sets apart, edit to it, the verdicts kept |
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
        | crate                    | file                              | lines | job                                                                    |
        | raea-app-videoform-bench | apps/videoform-bench/src/judge.rs       | 150   | a pair shown unlabelled in a random order; the pick, the scores and the words kept, and as evidence |
      Then the person sees the original and raea's edit side by side, unlabelled, in a random order, picks which they'd
        post, scores pacing, shot choice, structure, graphics, sound and overall feel from 1 to 5, and says what's wrong
      And each verdict is kept in the bench's record and is evidence in the videoform: a pick a pair (FP6b), a note a
        label (FP5)

  Rule: Deferred
    Scenario: Not in this file yet
      Then an automatic judge for the test bench (a video-capable language model, paid by the person, in the bench only,
        never shipped: fidelity, pairs, and gaps between what the references show and what the videoform holds)
      And the order of the test sets
