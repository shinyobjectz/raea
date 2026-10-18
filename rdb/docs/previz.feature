Feature: Previz: the edit seen and judged before anything renders
  Status 2026-10-14: what exists, re-run and measured today. There are three previz pieces, built at different times
  for different questions. They don't share inputs yet.

    | piece                   | where                                   | question it answers                                           |
    | timeline previz (CPU)   | rdb/src/previz.rs, `rdb previz`         | what each frame of this edit shows, which frames to look at, which cuts read as cuts, what a snippet changes |
    | timeline previz (GPU)   | rdb/src/gpuview.rs, `rdb gpuview --sketch` | the same sketch and look vectors on the GPU, fast enough to run on every snippet |
    | layout previz           | engine/src/previz.rs, `raea previz`     | is this shot's layout (labelled boxes) good: safe zones, face, thirds, the creator's wants |
  And the design for previz as a product (shot cards, the camera overlay, attention, the layout reader in Perception) is
    ~/host/docs/previz.feature; the layout previz is its scorer

  Rule: A sketch is the rows drawn cheaply, not a render
    The rows already say what every frame shows. A sketch draws that straight from them at 144x256: each visible clip
    sampled from a small proxy of its source, cover-cropped, zoomed and panned exactly as the engine does it, by z then
    id; each text row as a filled box in its colour (no glyphs). Nothing is generated and no model runs.

    Scenario: The UGC example, CPU, 2026-10-14
      When `rdb previz rdb/examples/ugc/ugc.sql -o OUT --vs rdb/examples/out/ugc-g8.mp4` runs
      Then OUT holds storyboard.png (12 picks), even.png (12 evenly spaced), vs-NNNNN.png (sketch | real render every
        120 frames) and previz.json
      And it measured
        | measure                                   | value                                       |
        | frames                                    | 738                                         |
        | decoding the proxies (929 source frames)  | 5.9 s, most of the run                      |
        | the frames' states                        | 0.19 s                                      |
        | sketching all 738 frames                  | 0.10 s (about 7,400 frames a second)        |
        | look and row hypervectors                 | 0.45 s                                      |
        | SSIM, sketch against the real render      | mean 0.956, worst 0.924 (every 30th frame)   |
        | storyboard by look, worst gap             | 308 bits, against 467 for evenly spaced     |
        | storyboard by look + rows, worst gap      | 975 bits, against 1118 evenly spaced        |
      And of the 9 authored cuts, 8 read as cuts (the look moves more than in 95% of within-shot frame steps); the one
        that doesn't is frame 693, where the talking head ends under full-screen b-roll, which a viewer wouldn't see
        either

    Scenario: The UGC example, GPU, 2026-10-14
      When `rdb gpuview rdb/examples/ugc/ugc.sql --sketch -o sheet.png` runs
      Then the whole edit sketches in 25 ms of GPU warm (60 ms cold, after a 13 s first ingest of 929 proxies)
      And look hypervectors take 24 ms, all-pairs distances 3.2 ms, the storyboard 6.8 ms
      And the storyboard's worst gap is 174 bits against 273 evenly spaced, the same as g8.feature recorded

  Rule: Two hypervectors for each frame, what the edit says and what it shows
    Scenario: The two sides
      Then what the edit says is row_hv: each visible row's identity bound to the bundle of its columns, the frame the
        bundle of its rows, a clip's place in its source a column (so a shot drifts slowly and a cut jumps)
      And what it shows is a look: a 9 x 16 grid of mean colours in opponent space (luma, red-green, yellow-blue),
        signed onto 2048 random hyperplanes
      And the storyboard is farthest-point picks under Hamming distance; "reads as a cut" compares a cut's look step
        with the within-shot steps

    Scenario: One look, on the CPU and the GPU (B1, done 2026-10-14)
      Given the look is kgpu's look_grid.k and look_proj.k: 9 x 16 cell sums (luma r+g+b, red-green, (r+g)/2 - blue),
        luma centred on the frame's mean cell, signed onto 2048 integer-hash hyperplanes
      And previz.rs look_words is the same function on the CPU (i32 wrapping as the GPU's)
      When examples/look_parity sketches every frame of the UGC edit on the GPU and computes both looks on its pixels
      Then 738 of 738 frames are bit-equal (worst 0 bits)
      And the storyboards agree: GPU worst gap 371 bits against 592 evenly spaced, CPU 376 against 587 (each on its own
        sketch, which differ slightly)

    Scenario: Known gap: the look is colour, not content
      Given the storyboard picks frames 3 and 17, both blurred shots of the same person at the start
      Then the colour grid reacts to blur and colour shifts, not to what is in the frame
      And the fix is already built elsewhere: SigLIP 2 frame columns (meaning/docs/meaning.feature) and Perception's
        tracks and layout (~/host/app/Host/Agent/Perception.swift); previz reads neither yet

    Scenario: Content in the look (B2, 2026-10-14)
      Given previz::content: each frame's top clip (by z, then id) read at its place in its source, that source's
        SigLIP 2 row from its ingest (`raea-perceive ingest`; the edit's media ids are the same sha-256 the ingest store
        is keyed by), signed onto 2048 seeded hyperplanes; text and overlays stay the rows' hypervector's job
      And a storyboard by look plus content, each distance over its own median across frame pairs (look 544 bits,
        content 228), written to storyboard-content.png beside previz.json's "content"
      When the UGC edit's three sources are ingested and `rdb previz` runs
      Then all 738 frames have content, and among the 12 picks the near-duplicate pairs (SigLIP 2 cosine 0.9 or more)
        are 45 by look and content, against 51 by look, 53 by rows and look, 55 evenly spaced; the closest pair is
        0.968, against 0.983, 0.999 and 0.989
      And the gain is modest because the edit is one talking-head take: most of its frames are 0.9 alike whatever is
        picked; the picks now span its framings (wide, leaning in, the waves, the b-roll, the second person)

    Scenario: Faces in the sketch and the hard rules on every frame (B3, 2026-10-15)
      Given each source's face track from its ingest (faces.json: Vision's faces and the person mask's box, 4 a
        second), mapped onto the canvas through each clip's cover, zoom and pan (previz::zones), a face under a clip
        drawn above it hidden; each text row as the extent of its words, line by line as the sketch draws them
      And each frame's faces (subject: the person's box grown to hold its face, else a box around the face) and texts
        scored by the layout scorer (engine/src/previz.rs) as a 9:16 layout; storyboard-faces.png outlines the faces
      When `rdb previz` runs on the UGC edit
      Then 628 of 738 frames show a face, the face boxes sit on the faces in the sketch, and the hard rules fail on
        | rule                                          | frames | why, on looking                                         |
        | a subject's face overlaps text                | 514    | the captions at 0.70 of the height run across the creator's mouth and chin |
        | text reaches the platform's button bands      | 349    | long caption lines reach the right-hand band (940 px)    |
        | a subject's face reaches the button bands     | 269    | the second person at the right edge, and close framings |
        | a subject's face is cut off by the frame      | 92     | the second person leaning out at the edges              |
        | text is outside the 4:5 feed crop             | 74     | the opening title at the top                            |
      And the frames and their spans are in previz.json's "rules"; the run adds 7 s of proxies and 1 s of hypervectors
        to previz (the faces and the scoring are milliseconds)

    Scenario: Where the eye goes (G1, 2026-10-15)
      Given each source's attention track from its ingest (attention.json: Vision's attention saliency map 4 times a
        second, its weighted centre, its peak, and focus = 1 - its entropy over the largest it can be), and
        previz::attention mapping the top clip's centre and peak onto the canvas through its cover, zoom and pan
      When `rdb previz` runs on the UGC edit
      Then previz.json's "attention" has focus for all 738 frames (median 0.126; it falls to 0.07 while the colour-bar
        b-roll is on screen), the eye's jump at each authored cut (0.010-0.021 of the canvas within the take,
        0.046-0.060 into and out of b-roll), and the first look of each shot: the attention peak lands on a face at 9 of
        10 shot starts (the miss is the b-roll, which has none)
      And these are reported, not judged: Track D decides whether a jump or a focus level moves the outcome

    Scenario: The mix as a timeline (G3, 2026-10-15)
      Given rdb/src/mixline.rs: the voice stem (the clips' sound) and the music stem (the audio rows) each measured
        before the master (momentary loudness per frame, as `hearing` reads them), and the master: short-term loudness
        (3 s) and the sample peak each second; while someone speaks (voice over -40 LUFS), how far under the voice the
        music sits, against the house duck of 23 dB (editing-quality.feature S1), under 18 dB counted as crowding
      When `rdb previz` runs on the UGC edit (previz.json's "mix")
      Then the master is -14.2 LUFS integrated (S2's -16 to -10) and peaks at -1.5 dBFS (no second over -1; sample
        peak, true peak is still to do)
      And the music sits 16.6 dB under the voice at the median, against the house 23: it crowds the voice on 410 of
        the 656 frames with speech, a finding about the edit's music bed, not the measure

    Scenario: Does an authored cut read as one, by TransNetV2 (B4, 2026-10-15)
      Given previz writes its sketches at 48 x 27 RGB (sketch-48x27.rgb), and `raea-perceive transnet-frames` runs
        TransNetV2 on them in Rust (ONNX through ort; against the Python ONNX on 40 renders: same frame counts, cuts at
        0.5 within a frame on 40 of 40 and identical on 39, mean difference 0.00012, 19 ms a frame)
      When it reads the UGC edit's 9 authored cuts (a cut "reads" when the probability within 2 frames is 0.5 or more)
      Then it reads the 3 cuts into and out of b-roll (0.79-0.99) and none of the 5 jump cuts within the take (0.008-
        0.404); the 9th (frame 693, the take ending under full-screen b-roll) neither reads, as the look said; it reads
        no cut that isn't authored
      And so TransNetV2 tells a new shot from a jump cut rather than judging whether a cut reads: the colour look reads
        8 of 9; a jump cut is visible to a viewer, so the look's reading stays, and TransNetV2's becomes the cut's kind

    Scenario: Composition and every platform's bands (G2, 2026-10-15)
      Given previz::PLATFORM_BANDS: the house band and six practitioners' measurements at 1080 x 1920 with their sources
        (postplanify.com's TikTok 108/120/320/60, Reels 210/84/310/0, Shorts 120/96/300/0 px top/right/bottom/left;
        daohuy.com's with the description text showing: TikTok 75/110/340, Reels 75/110/275, Shorts 80/100/220); no
        platform publishes these and they disagree, so they are reported side by side and averaged, the house band
        (stricter than all six) staying the hard rule
      When `rdb previz` runs on the UGC edit (previz.json's rules.composition)
      Then the biggest face's eye line sits 0.013 of the frame from the nearest third or golden line at the median, and
        the captions' centres 0.055
      And frames with a face or text in each platform's bands: house 315 / 349, TikTok 288 / 259, Reels 187 / 206, Shorts
        195 / 206, the described variants 202-223 / 206-259; the six platforms average 220 / 233

    Scenario: The light, and its jumps at cuts (G4, 2026-10-15)
      Given each source's light track from its ingest (light.json, twice a second: the frame's mean luma and warmth
        (R - B over R + G + B), and the framing coach's read of the light on the face through the Vision trait), and
        previz::light reading the top clip's sample at its place in its source
      When `rdb previz` runs on the UGC edit (previz.json's "light")
      Then the light on the face is "window behind (backlit)" on 583 of 738 frames (about 2 stops), even front light
        on 45, no face on 110: the creator is lit from behind, which the live coach would have flagged
      And at the cuts: the jump cuts inside the take change nothing (the same continuous footage); into and out of the
        b-roll, luma moves by up to 0.026 and warmth by up to 0.14 (back into the take): the catalogue's brightness and
        colour-temperature continuity, reported; the 2-a-second sampling is coarse for flashes (G's flash safety is
        its own check)

    Scenario: Flash safety, the one hard rule (2026-10-15)
      Given rdb/src/flash.rs: WCAG 2.3.1's general flash threshold on the sketches' relative luminance (a flash: a pair
        of opposing changes of 10% or more of the brighter, the darker under 0.80, over a quarter of the frame; more
        than three in any one second fails; the red-flash test and the small-area refinement are not in it)
      Then its unit tests pass (4 of 4): steady footage passes, a strobe at 5 flashes a second fails, exactly 3 a
        second passes, the same strobe over a tenth of the frame passes
      And the UGC edit passes (2 flash-sized transitions, at the colour-bar b-roll), in previz.json's
        rules.flash_safety

    Scenario: Do the measures move with outcomes? (G6, 2026-10-15)
      Given rdb::previz_report::run (moved from main.rs, output unchanged) and rrl/examples/previz.rs, which compiles an
        .rrl edit and runs it; tabular/bench/g6_trends.py over the overnight loop's scored runs (read only), comparing
        passing and failing runs of the same scenario
      When it reads 312 final edits in 49 scenarios (299 passed)
      Then only 8 scenarios have runs both ways, and in them the measures almost never differ: text to the power lines
        in 2 (higher when passing), face over text, text outside the feed crop and loudness in 1 each, every median
        difference 0; focus, eye jumps, light, the duck, bands and flashes don't differ at all
      And so this data can't say which measures matter: each overnight scenario changes one thing and nearly always
        passes, so the whole edit's perception doesn't move with its grade; G6 needs edits that vary in quality and a
        quality label (Kelly's picks, ad outcomes, the goal judge's met share on rendered runs)

    Scenario: Sound at the cuts: mid-speech and mid-sound (the catalogue, 2026-10-15)
      Given mixline::edges: at each sound lane's start and end inside the edit, the source's level 20 ms inside and
        20 ms outside; an edge where the next lane carries the same source on at the same moment is continuous sound,
        not an edge; a loud sound (over -35 dBFS both sides) cut there without a fade is cut mid-sound
      And a cut is mid-speech when it cuts the sound (an edge within a frame of it) and the voice stem's level in the
        60 ms around it is within 10 dB of its median while talking; picture cuts over continuous sound (b-roll over the
        voice, a jump cut on contiguous audio) are counted apart
      When `rdb previz` runs on the UGC edit
      Then no cut is mid-speech, 3 picture cuts sit over speech (the b-roll and a contiguous jump cut), and one edge is
        cut mid-sound: the take's voice at 23.1 s, under the closing b-roll, at -34 dB without a fade
      When s2 starts 0.56 s later in the take instead (inside "We're simulating them")
      Then frames 126 and 155 are mid-speech, and both edges at 126 are cut mid-sound (-18.7 and -20.8 dB, no fade)

    Scenario: Repetition, text against its background, text over motion (the catalogue, 2026-10-15)
      Given previz.json's "catalogue": the shots between authored cuts with each one's mean SigLIP 2 row (B2) compared
        with the next, their lengths; each text's colour against the luminance of the sketch drawn without its texts
        (WCAG's contrast ratio; the captions' stroke and shadow, which help, aren't counted); the frame-to-frame change
        of the picture behind the text
      When `rdb previz` runs on the UGC edit
      Then 7 of its 9 back-to-back shot pairs are 0.95 alike or more (the one take; only the colour-bar b-roll breaks
        it, 0.55), the longest shot holds 5.9 s
      And the captions' white against the picture behind is 4.75:1 at the median, under 3:1 on 102 frames (lowest
        2.18), and 83 frames have a busy picture behind the text (over 8 levels a frame)

    Scenario: Orphaned mouths at shot edges (the catalogue, 2026-10-15)
      Given ingest's mesh.json (the face mesh 8 times a second: mouth openness, head turn, gaze, the face's centre) and
        the voice stem's level every 10 ms (mixline::voice_levels); at each shot's start and end, the mouth moving (a
        step over 0.03 between samples in the 0.3 s inside the edge) while the voice is 20 dB under its median there
      When `rdb previz` runs on the UGC edit (previz.json's catalogue.orphaned_mouths)
      Then two edges are flagged: the end of the 0.4 s shot at frames 329-342, in the pause after "...what breaks."
        where the mouth opens for the next sentence (0.05 to 0.097) with no sound before the cut, and the edit's first
        0.3 s, where the mouth opens about 0.1 s before the first sound of "All right" (a lead-in, borderline)
      And look room waits: the head turn's sign has to be checked on footage with a known turn before a cramped side
        can be told from an open one

    Scenario: Glyph cutoff (the catalogue, 2026-10-15)
      Given each text's words' extent (B3's zones) reaching more than 1% past the frame, in spans; spans under a reading
        beat (0.3 s) counted as flickers
      Then the UGC edit has none, and with its first caption moved to x 700 (its centred "All right" past the right
        edge) frames 0-22 are flagged, the caption's whole span

    Scenario: Animation principles that can be read off the frames (the catalogue, 2026-10-15)
      Given rdb/src/animation.rs: every numeric column of every overlay row followed through the frames and split into
        moves, each measured for timing (its length), easing (slower at both ends than in the middle), follow-through
        (past its target, then settling) and anticipation (first the other way); a stretch reversing more than twice is
        a loop, set apart; staging, arcs, appeal and silhouette are a viewer's call; unit tests pass (3 of 3: an eased
        move, a linear one, a move with anticipation and overshoot)
      When `rdb previz` runs (previz.json's "animation")
      Then the UGC edit has 2 moves (0.23 s, linear) and the hook 61 moves and 14 loops in 19 rows: median 2.27 s, 5%
        eased, 20% overshooting, 44% with a counter-move, one under 4 frames
      And the hook's counter-moves are mostly its bobbing (part of a sine wave reads as a dip before the rise), so on
        oscillating motion the anticipation share over-reads; it's sound on discrete moves (the tests)

    Scenario: True peak, and where the creator looks (G3, G1; 2026-10-15)
      Given the mix's true peak (the master upsampled 4x with raea-media's band-limited resampler, then peaked, as
        BS.1770 measures it) and the gaze from ingest's mesh track (the share of face frames whose gaze is within the
        performance read's cone toward the camera)
      When `rdb previz` runs on the UGC edit
      Then the master's true peak is -0.9 dBTP: over editing-quality.feature S2's -1, though its sample peak is -1.5
        (the master's limiter holds samples to the ceiling, not the peaks between them): a finding about the mastering
      And an agent session made raea_media::audio::master's ceiling a true-peak one (b405bd0, a 4x-oversampled envelope); the
        same run now reads -1.5 dBTP, so S2 passes on the house render
      And the creator looks toward the camera on 92% of the 628 frames with a face

    Scenario: The focal point and what competes with it (the catalogue, 2026-10-15)
      Given ingest's attention track now carries a rival per sample (perception/src/ingest.rs › rival): the saliency
        map's mass above its median in a disc of 0.1 of the picture, the strongest disc 0.25 or more from the strongest,
        and its mass over that one's (Vision's maps are smooth blobs: a single cell's maximum put the rival at 0.84-0.95
        on every clip, so mass it is); unit tests pass (3 of 3: one blob has no rival under 0.1, two equal blobs over
        0.8, a 0.4 blob reads between 0.3 and 0.5); the fixtures were re-ingested (attention only, about 7 s for the take)
      When `rdb previz` runs on the UGC edit (previz.json's attention.focal_point: a rival at 0.8 of the peak or more,
        not on the peak's own face, competes)
      Then the rival's median is 0.905 and two places compete on 562 of 738 frames (0.76): 511 with the peak on one
        face and the rival on another, because the take has a second person standing behind the creator (two faces in
        97 of its 103 samples), and 51 on the b-roll with both off any face
      And the take's 6 one-face samples read as a single focus (rival as low as 0.10): the measure tells a lone subject
        from a shared frame; whether a second face hurts an ad is Track D's question, so it is reported, not enforced

    Scenario: Diegetic continuity at cuts (the catalogue, 2026-10-15)
      Given rdb/src/mixline.rs › diegetic, per authored cut: a shot's own sound (its clip's `sound`, else its `src` when
        it sounds; a muted clip's own is the sound another clip of the same picture source names, through the source
        map); whether the outgoing one still plays its picture's moment 0.2 s after the cut (carries), whether the
        incoming one plays its picture's moment 0.2 s before (leads), and the incoming sound 0.1 s in: in step (within
        1.5 frames), out of step, silent, or none of its own; out of step and silent are mismatches
      When `rdb previz` runs (previz.json's "diegetic")
      Then the UGC edit's 9 cuts have no mismatch: 5 continuous (punch-ins on one take), 2 L-cuts into the b-roll, 1
        J-cut back to the creator, 1 straight
      And on variants of it (shot s5 muted; s5 muted with its voice laid as an audio row in step; the same row a second
        late) the check reads: silent at 342 (1 mismatch), in step (0), out of step at 342 (1)
      And on the 81 distinct graded overnight edits (377 cuts): 202 continuous, 78 L-cuts, 49 J-cuts, 48 straight; every
        incoming shot with sound is in step and none is a mismatch: the agent never mutes or slips a take, so this
        guards against a mistake the runs don't make yet (it is reported, not enforced)

    Scenario: Screen direction, match on action and graphic match at cuts (the catalogue, 2026-10-15)
      Given rdb/src/direction.rs: the way things move in the 0.3 s either side of a cut, in canvas widths a second: the
        largest face's centre (B3's zones) when it is seen at both ends, else the picture's shift from the span's first
        sketch to its last (block matching on luma within 12 px; frame to frame, a slow pan is under a sketch pixel);
        under 0.15 is still (a talking head sways about 0.1); kept, reversed, from or to still; a match on action is
        kept at a half to twice the speed, a graphic match a change of picture source that doesn't read as a cut (the
        look hypervectors, as "cuts" reports); unit tests pass (3 of 3: a 3 px and a -2 px pan found, a flat picture
        still, the four ways across a cut)
      When `rdb previz` runs (previz.json's "screen_direction")
      Then the UGC edit's 9 cuts are all still: a talking head and still b-roll have no direction to keep or break
      And with its first b-roll split at frame 274 into two zoom-2 pans (pan_x -1 to 0, then 0 to 1) the cut reads
        kept, -0.29 then -0.26 widths a second (the crop moving right moves the picture left; 0.27 expected) and a match
        on action; with the second pan reversed (0 to -1) it reads reversed (-0.29 then +0.26)
      And on 79 graded overnight edits (399 cuts; 250 read by the face both sides): 398 still, 1 reversed (a sway at a
        punch-in), no match on action, no graphic match: the bank's edits recut one talking head, so the measure is
        ready for footage with movement and has none to judge yet

    Scenario: Cuts on beats or phrases (the catalogue, 2026-10-15)
      Given rdb/src/beats.rs, a model-free beat tracker (Ellis's dynamic programming, 2007): spectral-flux onsets 100
        times a second (46 ms Hann windows, their lead measured on clicks and taken out), the tempo from the onsets'
        autocorrelation between 60 and 180 BPM weighted toward 120, no tempo unless the peak is 3 times the range's mean;
        unit tests pass (3 of 3: click tracks at 120 and 95 BPM found within 2 BPM and every beat within 20 ms of its
        click (all at 0 ms); steady noise has none)
      And mixline::cut_timing: the music lanes' beats with their gain held flat (the duck at every speech edge would
        read as onsets at the cuts), the voice's pauses (150 ms, 20 dB under its talking median) and the edges of the
        edit's speech rows; a cut is on the beat within 50 ms (chance: 0.1 s over the period), on a phrase in or within
        0.1 s of a pause or a speech row's edge
      When `rdb previz` runs (previz.json's "cut_timing")
      Then the UGC music bed reads 96.0 BPM with pulse strength 3.9 (click tracks 17-37, noise near 1): a weak pulse;
        librosa's onset autocorrelation peaks at the same 0.627 s, but its beat tracker places almost no beats on it
      And of the UGC edit's 9 cuts 4 are on the beat (1.4 by chance) and 7 on a phrase; the other 2 are the b-roll
        inserts over running speech; 3 of its sentence ends dip only 11-18 dB (a word gap mid-sentence dips 29), so the
        speech rows find them and level alone doesn't
      And on 124 graded overnight edits (604 cuts) 238 cuts are on the beat (95 by chance) and 490 on a phrase, but
        the cuts sit at only 30 distinct frames (one take, one bed), 8 of them on a beat (0.27 against 0.16 by chance):
        too few to say the agent cuts to the music; runs pass 99-100% either way, so outcomes can't separate it yet
      And Beat This! (a learned tracker) would read a weak pulse better; it stays the next step if beats start to matter

    Scenario: Mouths moving with no voice, anywhere in a shot (the catalogue's diegetic source mismatch, 2026-10-15)
      Given previz_report::silent_mouths: spans of 0.6 s or more where the top clip's mouth moves (a step over 0.03 in
        openness against the mesh sample 1/8 s before) while the voice is 20 dB under its talking median; the voice is
        the clip lanes and any audio row playing a source a clip names as its sound (widened today: a take's voice laid
        as its own row counted as silence before)
      When `rdb previz` runs (previz.json's catalogue.silent_mouths)
      Then the UGC edit has none: its two shorter spans (frames 18-32 and 438-446, 0.3-0.5 s, the mouth opening to
        0.11-0.14) are breaths before a phrase, which is why the floor is 0.6 s
      And with s5 muted, 170 frames are flagged (340-365 and 374-517, nearly all of the shot); with its voice laid as
        an audio row in step, none; a second late, none here (the diegetic check reads that one as out of step)
      And on 112 graded overnight edits none is flagged (the agent never mutes a take); impacts and instruments wait
        for a sound-event match (Track H's CLAP)

    Scenario: Music baked into a clip's own sound (Track H3, 2026-10-15)
      Given mixline's baked_music: where a clip lane's source has ingest stems (perception's stems.json, HTDemucs's
        levels 20 times a second) and its music stem's loud end (90th percentile) is over -45 dB, the music stem against
        the vocals at the source moment each frame plays, with the lane's gain, over the frames someone speaks
      When `rdb previz` runs on a variant of the UGC edit whose only sound is its own finished render (voice and bed
        mixed, as a clip under the takes; no music row)
      Then the baked music sits a median 13.9 dB under the voice and crowds it (under 18 dB) on 495 of 720 speech
        frames, where the original edit's separate lanes read 16.6 dB and 410 of 656 frames: the measure survives the
        music being baked in (speech is found on the whole mix there, so a few music-only frames count as speech)
      And on the original edit it reports nothing: the take's clean voice has stems too, but its music stem's loud end
        is -54 dB (HTDemucs's leakage) against the render's -29 and the bed's -22, under the -45 dB line

  Rule: Layout previz scores boxes, deterministically
    Scenario: The scorer, 2026-10-14
      Given a layout: ratio, seconds, zones in canvas fractions (subject with face, product, text with words, motion,
        logo, button, video) and the creator's wants (eye line, room above)
      When `raea previz score layout.json --json` runs
      Then hard rules fail it (safe-zone bands, text over the face or product, marks overlapping, a want not met, a
        face cut by the frame or the 4:5 Reels crop) and soft scores rank the ones that pass (power lines, balance,
        breathing room, reading time)
      And `raea previz html` draws it as labelled boxes for a snapshot
      And `cargo test --release --lib previz` in engine/ passes 10 of 10

  Rule: Next, in order
    Scenario: Make previz read what perception already knows
      Then the look gains SigLIP 2 vectors from the meaning plane's columns, projected to the same 2048 bits, so the
        storyboard and "reads as a cut" judge content; measured against today's numbers above
      And the sketch draws Perception's face and person boxes, so the layout scorer's rules (text over a face, a face
        cut by a zoom) run on every frame of a real edit, not only on a planned layout
      And the CPU and GPU looks become one function, checked equal (done 2026-10-14, B1)

  Rule: Depth touches, judged before rendering (the roadmap G5, 2026-10-15)
    Scenario: The tracks
      Given perception's ingest step `matte` (ingest.rs matte_track, 4 a second, 18 s for the 25 s fixture take): on a
        grid of 96 rows, the person mask (Vision), Depth Anything 3's inverse depth (0-255 over the sample's nearest)
        and each cell's sharpness (the mean absolute Laplacian of the grey picture), with each sample's face boxes
      Then previz's catalogue gains depth_touches, mapped through the top clip's cover, zoom and pan as attention is

    Scenario: Text behind the head
      Given a text row declaring `behind = 'person'`
      Then previz measures the share of the text's box the person matte covers at its worst frame: under 0.03 the touch
        does nothing ("none"), up to 0.25 it reads as a look ("ok"), up to 0.5 it's "borderline", past that it hides
        the words ("fail")
      And on three fixtures (rdb/examples/g5/behind-*.sql: "GLOW" at 300 px over the fixture take at y 50, 200 and 600,
        the heights chosen from a scan of the matte, since the take's second person stands into the top half at times):
        | fixture    | worst share | mean share | verdict    |
        | ok         | 0.212       | 0.015      | ok         |
        | borderline | 0.467       | 0.083      | borderline |
        | fail       | 0.994       | 0.674      | fail       |

    Scenario: A focus blur
      Given a clip declaring `focus_blur`, and fixtures made by perception's raea-focus (the take blurred by depth the
        way a portrait mode does: a focus plane at the face, halfway to the background, or at the background; each
        pixel blurred by its depth's distance from the plane, 10 px at most at 540 x 960; then ingested)
      Then previz measures the face's sharpness over the background's (outside the person matte), the median over the
        clip: 2 or more sets the subject apart ("ok"), 1 to 2 is "borderline", under 1 the focus missed the face ("fail")
      And on the fixtures (rdb/examples/g5/focus-*.sql; the blurred takes are made by the command in each file, not
        committed):
        | fixture                      | face over background | sharpest tenth from the face in depth | verdict    |
        | focus at the face, 10 px     | 3.60                 | 0.007                                 | ok         |
        | the take as shot (reference) | 1.72                 | 0.151                                 | borderline |
        | plane halfway, 10 px         | 1.59                 | 0.026                                 | borderline |
        | focus at the background      | 0.70                 | 0.249                                 | fail       |
      And the bound sits at 2 because the take as shot already reads 1.7: a blur that doesn't beat that sets nothing
        apart (a first bound of 1.5 passed the missed plane, and a 2 px blur at the face read 2.47, still sharp, so it
        was no borderline fixture)
      But rdb renders neither touch yet (text over a clip has no person matte above it, and clips aren't blurred): the
        blurs here are baked into the footage, and the text fixtures are judged from their rows, as previz judges any
        edit before it renders
