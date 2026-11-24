// Generated from raea-core-brief (core/brief) by its `declarations`; edit the Rust, not this.

export type Brief = { title: string, logline?: string, 
/**
 * who the work is for (a person or a client)
 */
for?: string, 
/**
 * YYYY-MM-DD
 */
due?: string, 
/**
 * who watches or plays it, and where
 */
audience?: string, 
/**
 * the one thing they should think afterwards
 */
believe?: string, 
/**
 * pitch, explain, story, play, tool: an open set, one guide each; a piece can mix two
 */
mode?: Array<string>, 
/**
 * the narrator and the tone
 */
voice?: string, sound?: string, 
/**
 * the project's committed world: palette by role, type, camera language, light, references
 */
look?: string, 
/**
 * the anti-goals
 */
notDoing?: Array<string>, 
/**
 * facts an agent wrote and the person hasn't confirmed, by key ("voice", "audience")
 */
assumed?: Array<string>, 
/**
 * prose sections beyond the fields above (Goal, Background), in order
 */
sections?: Array<Section>, deliver?: Array<Deliverable>, references?: Array<Reference>, agents?: Array<Agent>, 
/**
 * the project's stories, imported explicitly (raea/story, docs/story.md): one, or several by name. Kept as written:
 * raea reads nothing in it, the story SDK and the checks that import it do
 */
story?: unknown, 
/**
 * a scene, kit or shot made to a frozen plan and held to the review loop (docs/make.feature): `scene()`, `kit()`
 * or `shot()` from "raea/brief"; `raea make status` decides when it's done
 */
make?: Make, };

export type Section = { heading: string, text: string, };

export type Deliverable = { name: string, 
/**
 * "90 s", "16:9", "with sound"
 */
specs?: Array<string>, 
/**
 * the feed item it lands as
 */
item?: string, 
/**
 * its own brief module, when it differs from the project in mode, look or audience
 */
brief?: string, 
/**
 * its script module (storyboard or screenplay)
 */
script?: string, };

export type Reference = { target: string, note?: string, };

export type Agent = { name: string, role?: string, };

export type Script = { title: string, logline?: string, scenes: Array<Scene>, };

export type Scene = { 
/**
 * where and when ("INT. SENATE - NIGHT", or "The brief page")
 */
heading?: string, beats: Array<Beat>, 
/**
 * what the scene must land
 */
payoff?: string, 
/**
 * the story beat it plays, by its id (raea/story's compileScript writes it; docs/story.feature ST0c)
 */
beat?: string, };

export type Beat = { 
/**
 * what's seen (a screenplay's action)
 */
onScreen: string, 
/**
 * the move ("push in on the knife", "orbit left, keeps moving through the cut")
 */
camera?: string, 
/**
 * how this beat is cut into ("match on motion", "hard cut")
 */
cut?: string, lines?: Array<Line>, 
/**
 * sound and music cues ("strings drop out", "the theme's second hit")
 */
cues?: Array<string>, seconds?: number, 
/**
 * the word the beat cuts on, so a new take keeps the cut in sync with the transcript
 */
onWord?: string, };

export type Line = { 
/**
 * a character, or "narrator"
 */
who: string, says: string, 
/**
 * a screenplay's parenthetical ("quietly")
 */
how?: string, };

export type Taste = { title: string, 
/**
 * when this file is read ("Read for pitch, explain and story")
 */
note?: string, for?: string, 
/**
 * the kinds of work with their own taste file (motion, games, sound, ui)
 */
kinds?: Array<string>, sections: Array<TasteSection>, };

export type TasteSection = { 
/**
 * "Always", "Never", "Camera"
 */
heading: string, rules: Array<Rule>, };

export type Rule = { text: string, check?: RuleCheck, 
/**
 * what a named or a .ts check is held to (`{ band: 0.8 }`); a form carries its own value
 */
values?: Record<string, number | string | boolean>, 
/**
 * how bad what its check finds is: error, warn or note (unset: a form's findings are errors, another check's keep
 * its own level)
 */
level?: RuleLevel, };

export type RuleCheck = Check | string;

export type RuleLevel = "error" | "warn" | "note";

export type Check = { "form": "text-hold", "value": number } | { "form": "text-above", "value": number } | { "form": "text-below", "value": number } | { "form": "text-margin", "value": number } | { "form": "loudness", "value": number } | { "form": "still-max", "value": number };

export type Parts = { concept: Concept, 
/**
 * by name; the build's parts are typed from these keys, so a planned part the build skips is a tsc error
 */
parts: { [key in string]: Part }, };

export type Concept = { 
/**
 * what it is ("flight feather", "toy rocking horse")
 */
what: string, style: string, 
/**
 * its largest extent, in metres
 */
size: number, 
/**
 * how close it's seen, in metres: sets how much detail up close must hold
 */
seenFrom: number, material?: string, use?: string, };

export type Level = "primary" | "secondary" | "tertiary";

export type Count = number | { min: number, max: number, };

export type Part = { 
/**
 * the label it carries into the world ("seat", "leaf", "gem")
 */
role: string, level: Level, 
/**
 * the part it sits on or in; none for the root
 */
parent?: string, count?: Count, 
/**
 * its own, else its parent's, else the concept's
 */
material?: string, style?: string, 
/**
 * what a near crop of it must show
 */
near?: string, };

export type Videoform = { 
/**
 * What the videos are, in the person's words ("street interviews for a brand: the host, a stranger, the product in
 * hand"); the person's own AI expands it into the vocabulary
 */
context: string, 
/**
 * Each reference as a brief names one: its link or its path as the target, its title and creator as the note.
 * What was read of each is kept beside fingerprint.ts in videoform/references.jsonl, by content hash.
 */
references?: Array<Reference>, 
/**
 * The vocabulary.ts the person's AI wrote from the context: the things to look out for (VF5)
 */
vocabulary?: string, 
/**
 * The NAME.script.ts a project made to the videoform starts from: its slots, their order and lengths (VF7)
 */
template?: string, 
/**
 * The NAME.taste.ts its rules are written to (VF8): taste lines, each with the check that holds it (a standing rule
 * is a taste line, docs/projects.feature PJ9), mined from the references with how strongly each holds
 */
rules?: string, 
/**
 * The person's word on what was found, by measure, over what the references show (VF4b): kept through references
 * added and read again, each with why
 */
corrections?: Array<Correction>, };

export type Correction = { measure: string, say: string, range?: [number, number], why?: string, };

export type Vocabulary = { 
/**
 * The context it was written from
 */
context: string, 
/**
 * The places the videos are in, each with the things in it
 */
places?: Array<Term>, 
/**
 * Things seen anywhere (the host with a microphone, the product in hand)
 */
things?: Array<Term>, 
/**
 * Things heard, each with how it sounds (a crowd laughing, a whoosh)
 */
sounds?: Array<Term>, 
/**
 * Words on screen, as patterns (a price, a store's name)
 */
text?: Array<Pattern>, 
/**
 * Words said, as patterns ("available at", "link in bio")
 */
said?: Array<Pattern>, 
/**
 * The parts the videos move through (the host's opener, the challenge, the call to action), each found by the
 * terms and patterns it names
 */
sections?: Array<Section>, 
/**
 * The context's words for how it feels ("snappy"), each with the measures that say it (raea videoform report
 * names them): those measures are checked, by the context's word, even where the references vary
 */
feel?: Array<Feel>, 
/**
 * What's in the references but not the videoform's (a watermark, a sponsor's end card), phrased as looks
 */
ignore?: Array<string>, };

export type Term = { name: string, 
/**
 * Several ways to say it, as a caption would ("a man holding a microphone on a city street")
 */
looks: Array<string>, 
/**
 * What it's easily taken for, so a shot nearer those isn't counted
 */
unlike?: Array<string>, 
/**
 * A place's things
 */
things?: Array<Term>, 
/**
 * What the context says of it: always (in every video) or usually
 */
how?: string, };

export type Pattern = { name: string, pattern: string, how?: string, };

export type Section = { name: string, at?: string, shows: Array<string>, how?: string, };

export type Feel = { word: string, measures: Array<string>, };

export type Make = { kind: MakeKind, 
/**
 * the pictures the job answers to (paths beside the brief): what is in it and where for a scene, the style for a kit
 */
references?: Array<string>, 
/**
 * one paragraph every reference prompt carries: era, palette, materials, wear, and one fictional maker
 */
style?: string, 
/**
 * by name, from the kind's set (DIALS); one left out is read from the references
 */
dials?: { [key in string]: MakeDial }, objects?: Array<MakeObject>, shots?: Array<MakeShot>, 
/**
 * the compositions the job's checks hold (relative to the brief): each needs a clean render or check newer than it
 */
compositions?: Array<string>, 
/**
 * what must exist when it's done, relative to the brief; replaces the kind's own list (`deliverables`)
 */
deliverables?: Array<string>, 
/**
 * the most critic rounds the job gets; at the cap it is delivered with what's still missing (8 by default)
 */
rounds?: number, };

export type MakeKind = "scene" | "kit" | "shot";

export type MakeSource = "customer" | "reference";

export type MakeDialValue = number | string;

export type MakeDial = MakeDialValue | { value: MakeDialValue, source?: MakeSource, 
/**
 * what the value meant for this job, in one line
 */
means?: string, };

export type MakeRoute = "solid" | "rule" | "gen" | "library" | "picture";

export type MakeOrigin = "reference" | "invented";

export type MakePick = { 
/**
 * "tripo-h31", "meshy", ...
 */
generator: string, 
/**
 * one line: why this one ("sharper dial numerals; Tripo fused the handset to the cradle")
 */
why: string, };

export type MakeObject = { 
/**
 * snake_case, unique in the job
 */
id: string, label: string, 
/**
 * real height in metres
 */
h?: number, 
/**
 * [width, depth, height] in metres, for boxy things (generators invent depth)
 */
dims?: [number, number, number], 
/**
 * largest extent in metres, for things that lie flat (tools, papers)
 */
max?: number, route?: MakeRoute, 
/**
 * one object, its condition and its materials; never a real brand
 */
prompt?: string, source?: MakeOrigin, 
/**
 * where it is in reference 0: [x0, y0, x1, y1] in 0..1
 */
crop?: [number, number, number, number], 
/**
 * where it stands in a scene: metres, the floor's front-left corner at the origin, rot 0 = facing the entrance
 */
at?: [number, number, number], rot?: number, hero?: boolean, pick?: MakePick, 
/**
 * what was fixed by script ("glass made transmissive, label kept opaque")
 */
fixes?: Array<string>, };

export type MakeShot = { 
/**
 * [a-z0-9_]: the file it lands as (out/shots/<name>.png or .mp4)
 */
name: string, 
/**
 * in a crew's words: "slow push along the counter, rack focus to the register"
 */
description?: string, 
/**
 * static, dolly, truck, pedestal, crane, pan, tilt, orbit, dollyZoom, path
 */
move?: string, seconds?: number, lens?: number, 
/**
 * none, subtle, walk
 */
handheld?: string, 
/**
 * a still rather than a move
 */
still?: boolean, 
/**
 * the composition that renders it (relative to the brief): `raea make framing` and `lenschart` draw it
 */
composition?: string, 
/**
 * its frame rate, for the frames `seconds` spans (30 when left out)
 */
fps?: number, };
