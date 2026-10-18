# .brief

A `.brief` is a plain-text plan for one video, or something several plans share. It reads as Fountain (a screenplay
form, which M3 writes well and which made better ads than rrl did; score/bench/results.feature) with one reading for
every line. `raea-brief parse FILE` turns it into the intent graph (JSON); anything it can't read is refused with
`FILE:line:col` and what it expected. The spec, the goal and the plan are in brief.feature and format.feature.

```
Title: Milky Jelly: the gel that doesn't strip
Format: 9:16, 30 s, tiktok | 1:1, 15 s, instagram
Uses: client.brief
Angle: dry skin can use a gel
$product: Milky Jelly
$pack: media/milky-jelly.png

! never says "game changer"
! nothing covers the face

# the objection
= Name the fear before the product.
.TAKE02 - KITCHEN
@MAYA
(warm, a little sheepish)
I always thought gel cleansers would wreck my skin. [[captions keys:"wreck" | place-low]]

# the pour
~ product: the cleanser pumped into wet hands [find: $pack hands action still]

# the ask
@MAYA
Try $product. [[end-card text:"$product" | to-end place-center]] {9:16}

---
:root { music: "warm room tone, soft keys"; music-gain: -24; }
```

## The file

A file is a title page, a body, and an optional stylesheet after a line that is exactly `---`. A file wrapped whole in
one Markdown code fence (```` ``` ````) is read as what's inside it. Lines are trimmed; a blank line ends a speaker's
block. Columns in refusals count from 1 on the line as written.

## Title page

`Key: value` lines from the top of the file to the first blank line.

| key | what it is |
|---|---|
| `Title:` | the piece's name |
| `Format:` | the deliverables: `RATIO, LENGTH[, PLATFORM]`, several joined by `\|`. A file with a Format is a **piece** (it compiles to an edit); without one it is **shared** (it only applies through another's `Uses:`) |
| `Uses:` | shared briefs, comma-separated paths relative to this file, applied in order, this file last |
| `Mode:` `Angle:` `Hook:` `Refuses:` | the strategy's lines, kept in the graph for the critique |
| `Credit:` `Author:` `Draft date:` | kept, unused |
| `$name: value` | a variable. A value ending in .png, .jpg, .jpeg, .webp or .heic is a picture (its path relative to the space); anything else is text |

LENGTH is `30 s`, `25 to 35 s` or `25-35 s`; a single length allows 15% either way. RATIO is `W:H`. PLATFORM is
tiktok, reels, instagram (reels' bands), shorts, youtube (shorts' bands) or omitted (the house bands). Any other key is
refused.

## Body

Each non-blank line is one element, chosen by how it starts:

| starts with | element |
|---|---|
| `#` | a section: `# the crack`. Its name is the writer's own; more `#`s nest it |
| `=` | the section's synopsis: what the beat does for the viewer |
| `.` | a scene heading naming the take the lines below come from: `.TAKE03 - AT THE KILN` (the take's file name without its extension, then anything after ` - `). `.BROLL - TAKE01` names TAKE01 |
| `@` | a speaker: `@JUNE`, `@SHANE (V.O.)`. An all-capitals line under 40 characters not ending in `:` is a speaker too (Fountain's character cue) |
| `(` … `)` | delivery for the next line: `(flat, then laughing)` |
| `PUNCH IN 1.2X:` | zoom the next line 1.2 times. A bare `PUNCH IN:` is read and flagged by the compiler |
| `CUT TO:` `SMASH CUT TO:` `MATCH CUT TO:` `FADE IN:` `FADE OUT.` `PUNCH OUT:` | a transition; read and kept, not acted on |
| `>` … `<` | text on screen with no line under it |
| `//` | a comment: kept by fmt, read by nothing (also allowed above and among the title page's lines) |
| `~` | an open beat: a shot not yet bound to footage (below) |
| `!` | a check: a rule the edit must keep (below) |
| anything else, under a speaker | a line: words quoted exactly from the take. A line is a cut, so a word changed is a line no take says |

Text that isn't under a speaker and starts with none of these is refused. A shot written as Fountain action
(`!Action The bracket clicks shut`, or a plain description) is refused with the open beat it should be: in the 110
scripts the bench wrote in Fountain, `!` stood 157 times before a shot description and `@must { }` held the rules 50
times, and score's reader dropped both silently. `raea-brief convert` turns them into `~` beats and `!` checks.

### Notes on a line: `[[ ... ]]`

`[[name prop:"value" prop:[a, b] prop:3 | utilities]]` after a line is a part timed to that line. `name` is a component
in the space's `components/` (or its parts kit); `component name:"x" ...` is the same spelled out; a note with no name
is utilities alone. Props are `key:"text"`, `key:[list]` or `key:number`. Utilities after `|`:

- timing: `on:'words'`, `until:'words'`, `to-end`, `stagger-after:NAME`
- placement: `place-top`, `place-low`, `place-center`, `place-full`, `place-auto`
- motion: `enter-fade|rise|slide|pop|bounce`, `settle`, `exit-fade`, `dur-sm|md|lg`, `ease-standard|emphasized|decelerate|accelerate`
- punch-in: `punch-in:1.2`
- foley: `sfx:'prompt'`, `sfx-gain:-14`
- a claim: `claim:ID`, the line or part makes the claim `$claim.ID` declares (below)

Anything else after `|` is refused at its column. A note on a synopsis or a section heading is refused (style goes in
the stylesheet).

**Typed parts.** When the brief is compiled for a space (or parsed with `--space SPACE`), each part's props are
checked against its component's own TypeScript props (the type of `components/NAME.tsx`'s default export's
parameter, read by `@raea/data/props.ts`): a prop the component hasn't, a required one missing, a value of the
wrong kind (`big:90` where `big` is a string: `big:"90"`) or outside a string union (`region` is `auto`, `top`,
`bottom` or `center`) is refused where its line stands. Captions' `keys` is the compiler's own, not the component's.

### Variables

`$name` anywhere in a line, an open beat, a check or a note's props is replaced by its value; an undefined variable is
refused where it's used. A picture variable can only stand in an open beat's finds or a prop (the component gets the
path).

### Deliverables and variants

`{9:16}` or `{1:1, 4:5}` at the end of a line or an open beat keeps that element in those deliverables only. A ratio
no `Format:` names is refused.

`{A}` (a name) at the end of a section heading makes the section one variant's: `# hook {A}` and `# hook {B}` are
two hooks for an experiment, and everything in a section without a name is in every variant. A line or beat can
carry its own name too. `raea-brief compile` makes every variant of every deliverable, `NAME-A-9x16.tsx`,
`NAME-B-1x1.tsx` and so on (`--variant A` makes one); coverage of a shared ask and the checks run per cut. A ratio on a
section heading is refused (a section takes a variant's name; deliverables tag lines and beats).

```
Title: Ferro oat milk, two hooks
Format: 9:16, 30 s, tiktok | 1:1, 15 s, instagram

# hook {A}
@MAYA
I used to hate oat milk.

# hook {B}
@MAYA
Nobody warned me about oat milk.

# proof
@MAYA
It's creamy, and it's 90 calories. {9:16}
```

### Alternatives

`first read || second read` on a line gives the compiler alternatives. Before it places anything it picks, for
each such line, a read the transcripts don't flag (no stutter, no rehearsal talk, not cut off mid-sentence, not an
early read of a line said again) that brings the running total nearest the middle of the length the Format allows;
the sheet says which read it took and why the others lost. Every alternative is quoted word for word, like a line.

```
Title: Colitt, the first look
Format: 9:16, 12 s

# hook
@DAD
There he is. || See the bass?
```

### Claims

`$claim.ID: "what it says" source "where it's proven"` on the title page (here, or in a shared brief this uses)
declares a claim; the source may wait, and `! claims sourced` says so. A line or part says it makes the claim with
the utility `claim:ID`; an ID no brief declares is refused. `! claims sourced` fails at compile time and on the render
for a claim with no source, with the frames where it's said or shown.

```
Title: Alpha Lion, the safety claim
Format: 9:16, 20 s
$claim.gras: "GRAS verified" source "GRAS notice GRN-1234"

! claims sourced

# proof
@SHAWN
Making it a safe choice for you and it's GRAS verified. [[statcard big:"270+" | claim:gras]]
```

### Open beats: `~`

`~ KIND: PHRASE [find: FIND ...]` is a shot the plan wants that no line pins down yet. KIND is to-camera, b-roll,
product, screen or end-card (raea-beats' kinds). PHRASE says what happens, in words SigLIP and the critique can read.
FINDs are perception facts, each one of:

| find | reads |
|---|---|
| `face` `speech` `text` `screen` | detected (faces.json tracks, VAD and speaking, read-text, layout's screen zone) |
| `hands` `place` | partly detected (the hands command; scene labels) |
| `product` `logo` `action` | SigLIP on PHRASE only (ingest has no object detector) |
| `$picture` | the picture searched against frames (product, logo) |
| `people:N` | N faces in shot |
| `speaking` `eye-contact` `still` `moving` `bright` `no-text` | performance, motion, light and read-text facts |
| `framing:close-up\|medium\|wide` | shot framing |

The compiler binds each open beat to a span of footage, or names it in the sheet as weak or missing (with how it was
matched: by a fact, or by a SigLIP score against FIND_THRESHOLD). How (score/compiler/moments.py):

- every take with picture is read from its ingest (`raea-perceive ingest`, once per take): a moment is one SigLIP frame
  (8 a second) with the facts true then
- the beat's detected finds filter the moments; among those that pass, the phrase's SigLIP score (or the picture's)
  ranks them; a span grows from each peak while the finds hold and the score stays within 0.03 of the peak, 1.5 to 4 s
- b-roll, product and screen beats that don't ask for speech prefer moments where nobody is talking
- a span the edit already keeps isn't bound again
- the best span is **covered** at FIND_THRESHOLD (0.10) or more, **weak** at 0.06 or more (both are cut into the
  edit, in the beat's place), **missing** below (the sheet says what to do: record the screen, reshoot the creator,
  or find stock); an `end-card` beat is a part, not footage
- a picture is matched image against frame at 0.70, a threshold no labelled set has calibrated

### Checks: `!`

`! RULE` is kept by the edit and answered pass or fail in the sheet. RULE is one of:

- `never says "WORDS"`: no kept line says WORDS
- `the hook reads within half a second`, `nothing covers the face`, `captions read on a phone`, `everything inside the
  safe zone`, `ends on a word, never mid-word`, `no black frames`, `the length is within the format`
- `FACT -> FACT`: whenever the first perception fact holds in the render, so does the second (`speaking -> face`)
- `claims sourced`: every claim a line or part makes (`claim:ID`) has a source
- `no shot longer than N s`, `the hook ends by N s`, `a cut at least every N s` (a cut, or new text on screen)

A must may drop its "the", and the length may be given in seconds (`length within 30 s`); `never says` takes several
phrases. Anything else is refused. Checks add up across `Uses:`; none can be removed.

**On the render.** The compiler answers what it can before anything renders (what the lines say, the length, the
hook's end, the claims). `raea-brief check NAME SPACE` answers every rule on the rendered video: it loads the piece's
facts as tables in `@raea/data` (its graph, the words as cut, the render manifest `NAME.render.json` with each frame's
text and video boxes, faces read back from the rendered frames twice a second, and perception's moments of the
takes on screen) and evaluates each rule as a query over them:

| rule | on the render |
|---|---|
| the hook reads within half a second | text on screen, or a word said, before 0.5 s |
| nothing covers the face | the text on each face, all of it, under 8% of the face (faces read back from the render) |
| captions read on a phone | every text at least 2.5% of the frame's height |
| everything inside the safe zone | every text inside the platforms' safe rows and the frame |
| ends on a word, never mid-word | no word runs through a cut or the end |
| no black frames | ffmpeg's blackdetect |
| the length is within the format | the render's length against the deliverable's |
| never says | the words as cut |
| FACT -> FACT | at each read-back frame: face and people:N from the render; speaking, speech from the words; text, no-text from the manifest; eye-contact, framing, still, moving, bright, screen from the moment on screen; product, logo, action, hands, place by SigLIP on it |
| claims sourced | the claims made, and where an unsourced one is said or shown |
| pacing | shots from the manifest (where the source or its time jumps); the hook's end from the timeline |

It prints a PASS, FAIL or NOT MEASURED line a rule, writes `NAME.check.json`, and exits 0 when every rule passes, 1 on
a fail, 2 when there's no render to check. `--battery` adds a standard set to the piece's own rules (the seven musts,
`speaking -> face`, `claims sourced`, `no shot longer than 6 s`, `a cut at least every 4 s`).

## Stylesheet

After `---`, CSS rules: `SELECTOR { property: value; ... }`.

| selector | properties |
|---|---|
| `:root` | `background`, `music` (a prompt), `music-gain`, `sfx-gain`, `platform`, `eyes` (the eye line, a percentage) |
| `captions` | `place`, `keys` |

`music` without `music-gain` isn't added, and nor is foley without a gain. Any other selector or property is refused at
its column (an `@must { }` block becomes `!` lines).

## The graph

`raea-brief parse FILE` prints:

```
{ "file", "kind": "piece"|"shared", "title", "format", "deliverables": [{ratio, secs: [lo, hi], platforms}],
  "uses": [path], "meta": {Mode, Angle, ...}, "vars": {name: {value, picture}},
  "sections": [{name, depth, synopsis, at}],
  "items": [ {"type": "line", section, take, speaker, delivery, punch, text, notes: [{kind, props, utils, anchor}], variants, at}
           | {"type": "beat", section, kind, phrase, find: [..], variants, at}
           | {"type": "screen", section, text, notes, at}
           | {"type": "transition", text, at} ],
  "checks": [{rule, form, args, at}], "style": {selector: {property: value}} }
```

`at` is `[line, col]`. On a refusal it prints nothing on stdout and every refusal on stderr, and exits 2.
`raea-brief fmt FILE` prints the file back in one canonical layout; parsing what it prints gives the same graph.
`asks` are the open beats of the shared briefs this one `Uses:`, each with the file it came from.

## Compiling

`raea-brief compile FILE SPACE [--o OUT] [--cut RATIO] [--no-sound]` (what `raea script` runs) compiles a piece through
score's SDK backend to `SPACE/NAME.tsx`, an @raea/motion composition, with `NAME.words.json`, `NAME.perception.json`
(the speaker's face per 4 fps sample, from the takes' ingest) and `NAME.checks.json` beside it (`edit.tsx` keeps the
plain names). A piece with several deliverables compiles each to `NAME-9x16.tsx`, `NAME-1x1.tsx` unless `--cut` or
`--o` picks one. It prints the COMPILED line and the sheet: every line and bound beat timed, the total against the
Format and what would bring it inside, open beats with how they were bound, and each check pass, fail or left for
the render. `raea-brief compile --all SPACE` compiles every piece, then answers each shared brief's beats against the
footage each piece cut. `raea-brief convert SCRIPT.fountain` writes a score script as a .brief.

## Ink

A piece can also be written in [ink](https://github.com/inkle/ink), inkle's narrative scripting language, in the raea
profile below. `raea-brief` compiles and runs it with blade-ink (Rust; inkjs is the TypeScript runtime), along the
choices `--path 0,1` names (the first choice where the path runs out), and reads what it plays as the .brief it is:
every reading and refusal above holds, and the graph the compiler gets is the same (`raea-brief lower STORY.ink`
prints that .brief; `raea-brief ink FILE.brief` writes a .brief as ink). Ink's own VAR, LIST, conditionals,
interpolation (`{product}`), knots, diverts and choices work as ink has them: the compiler sees what the story plays.

```ink
# title: Pottery studio: the bowl that cracked
# format: 9:16, 20 s, tiktok | 1:1, 15 s, instagram
# check: never says "game changer"
# music: "sparse marimba, warm room tone"
# music-gain: -24
VAR hours = "forty"

-> the_crack

=== the_crack ===
JUNE (flat, then laughing): That's {hours} hours, gone. #section: the crack #take:TAKE03 #part:label text:"40 hours" | on:'forty hours' enter-fade place-top
-> why

=== why ===
JUNE: The wall was thinner on one side, and the kiln found it. #take:TAKE01 #punch:1.15
>> b-roll: the cracked bowl turned in two hands #find:hands still
* [the fix] -> the_fix
* [the lesson] -> the_lesson

=== the_fix ===
JUNE: So this one's getting a thicker foot. #part:captions keys:"thicker" | place-low
-> END

=== the_lesson ===
JUNE: Every crack tells you where the clay was thin. #only:1:1
-> END
```

| ink | the .brief it plays |
|---|---|
| `# title:` `# format:` `# uses:` `# angle:` `# hook:` `# mode:` `# refuses:` `# $name:` | the title page |
| `# check: RULE` (or `~ check("RULE")` from story logic) | `! RULE` |
| `# music:` `# music-gain:` `# sfx-gain:` `# platform:` `# background:` `# eyes:` `# captions-place:` `# captions-keys:` | the stylesheet (ink reads `{...}` in a tag as logic, so there's no CSS in a tag) |
| `=== knot ===`, `#section: name`, `#synopsis: ...` | `# name` (the knot's own name without a `#section`), `= ...` |
| `SPEAKER: words` / `SPEAKER (delivery): words` | `@SPEAKER`, `(delivery)`, the line |
| `#take:TAKE01` (holds until the next) | `.TAKE01` |
| `#part:NAME props \| utilities` (as many as the line has) | `[[NAME props \| utilities]]` |
| `#punch:1.15` / `#punch:` | `PUNCH IN 1.15X:` / `PUNCH IN:` |
| `#only:1:1` | `{1:1}` |
| `>> KIND: what is shown #find:...` (or `~ beat(...)` from story logic, played between lines) | `~ KIND: ... [find: ...]` |
| `>TEXT<` | `>TEXT<` |
| `_` with tags | a line that only carries its tags (a section with nothing in it yet) |

In a tag, `#`, `{` and `}` are written `&num;`, `&lbrace;`, `&rbrace;`; in spoken words, ink's own marks are escaped
with `\` (the converter does both). Ink collapses a run of spaces in text to one.
