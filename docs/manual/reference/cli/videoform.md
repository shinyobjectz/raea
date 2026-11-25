---
title: "raea videoform"
summary: "A kind of video you make again and again, read once from its references: files on your drive or links, keeping only what was measured, never the video."
order: 46
sources: ["apps/videoform/src/lib.rs", "apps/videoform/src/ingest.rs", "apps/videoform/src/found.rs", "apps/videoform/src/vocab.rs", "systems/videoform/src/ranges.rs", "systems/videoform/src/vocab.rs", "systems/videoform/src/kinds.rs", "systems/videoform/src/structure.rs", "systems/videoform-rules/src/lib.rs", "io/resolve/src/lib.rs"]
---

A videoform is a kind of video you make again and again (a listing walkthrough, a street interview, a product
overview), read from a handful of references. It's a [fingerprint](fingerprint.md) with a `videoform` section in its
`fingerprint.ts` (its context and its references), so it's found, inherited and shared as any fingerprint is: `new`
makes one, extending your own fingerprint, and it judges projects tagged with its name (`tags = NAME` in
`.raea/project`) unless `--kinds` or `--tags` say which; `raea context` prints it for those projects. `add` reads each
reference once and keeps only its readings, in `videoform/` beside `fingerprint.ts`: where it came from, its title,
creator and date, and a row for each shot (cut to cut, frame by frame): its length, framing, faces, camera, brightness
and look, scene labels, words on screen, the words said and the pauses at its edges, how much speech and music is
heard, its loudness, how near its cut is to a beat, and its picture and sound vectors (SigLIP 2 and CLAP), about 90 KB
for a minute-long reference. Following and checking work from the readings alone, so a reference is never needed again.

A reference is a file or a folder on your drive, read where it is, or a link: a platform page (YouTube, TikTok,
Instagram, Vimeo, X), a site's page with its own player, a playlist or a channel (its newest `--last N`), or a direct
link to a file. Links are resolved by yt-dlp, which raea fetches into `~/.raea/bin` on first use and checks against its
release's SHA-256 sums; when a site breaks, raea fetches the latest and tries once more. A link's streams are held in a
private folder while they're read (1080p at most) and dropped after. Before the first link, once per account, you
confirm you have the right to use what you add. A source read before is known by its link or its content hash and
isn't read again.

After every `add` (and on `report`), raea says what the references hold, one line a measure: shot lengths, cuts a
minute, words a minute, the pause at a cut, framing, faces, the camera, words on screen and captions, speech and music,
cuts on the beat, loudness. Each has its range, and whether it **holds** (they agree), is **loose** or is **free** (it
varies too much to be part of the videoform, so it isn't checked); a measure the references split into two groups on
says which (one set cutting every 1.2 s, another every 2.5 s), and a reference far from the rest on several measures is
named for you to keep or take out. Where that isn't what you want, say so with `set`: `ignore` (not the videoform's),
`free` (not checked), `keep` (checked though it varies) or `target LO HI` (checked against your range). Your word is
kept in `fingerprint.ts` with why, and stays as references are added. raea's delivery floor comes first: every cut is
mastered to -14 LUFS with its true peak at most -1 dBTP, whatever the references measure. `score` reads a cut as a
reference is read and says which checked measures fall outside, and which way to move them.

A videoform's context says what matters, and its references show how it's done. Your AI writes the context out as
`vocabulary.ts` beside `fingerprint.ts` (`raea videoform vocab NAME` starts one): the places and things to look for, each
phrased a few ways and with what it's easily taken for; sounds; words on screen and words said, as patterns; the
sections the videos move through (the hook, the challenge, the call to action), each found by the terms it names; and
the context's words for how it feels ("snappy"), with the measures that say them, which are then checked even where the
references vary. Every `add` and `report` says what each term did on the references (never fires, fires everywhere,
fires with another on the same shots, missing where the context says it's always there), where each section was
found, and shots that recur with no term naming them, for your AI to revise from.

From the vocabulary come the shot kinds and the structure. Each reference shot is gathered with the shots like it in
the other references, by how it stands against the vocabulary's terms (less what its own video shows throughout) and
by its scale, its people and whether anyone's talking: each kind is named in the vocabulary's words ("host with
microphone, stranger answering: medium, one person"), or "unnamed" with what's seen when no term stands out, for your
AI to name. The references are then cut at their sections, in order, into slots, each with how long it runs, its shots
and the kinds it's made of, and written as `NAME.script.ts` beside `fingerprint.ts`: the template a project made to the
videoform starts from, a scene a section and a beat a kind, checked by `raea spec`. Sections shown by the same terms
can't be told apart where they meet, and the report says so.

Last come the rules: what holds in most of the references, each with how strongly it holds (in how many of the
references it could hold in). What every video has (a kind of shot, a term shown, said or heard); how it opens and
closes; what each section is made of, more than the rest of the video is, and which section comes first; wherever a
place is shown, its things with it ("wherever it shows the kitchen, it shows the counters close up"); and what follows
a word said or shown (a punch-in, words on screen), where that's more than chance. They're written as `NAME.taste.ts`
beside the template, taste lines each naming the check `raea check` holds a cut to with it.
Under five references every rule is a note; a strong one warns. Each reading keeps a small still a shot, and each
report makes a contact sheet a shot kind from them (`videoform/kinds/kind-N.jpg`, the kind a rule's `kind-N` names) to
confirm the rules on: `raea videoform set NAME RULE keep` makes one an error to break, `free` a note, and `ignore`
drops it, each kept through every reference added.

`follow` takes new raw footage to the videoform. It reads the footage once, as references are read, a piece a row (a
sentence said, or up to three seconds with no talk), and matches each piece to the shot kinds. It says first what the
footage lacks that most references have, as a shot list. Then it plans the cut: where the footage talks, the voice
first (each line's last take, in the order said, the ask at the close), then the picture over it, section by section,
at the kinds' own lengths, the speaker in sync or B-roll of the kind under the voice. Your AI picks the lines by what
they say with `--say`, and raea cuts the picture over them. Added with `--brand`, a brand's videos keep what only they
hold (its end card, its offer on screen) in that brand's own fingerprint, not in the videoform.

How the references are layered is part of the videoform too. Each shot's layers are read with it: a person cut out over
another picture (the speaker keyed over B-roll), a picture inset, a card laid over (a photo, a testimonial, the
product), a plain backdrop the product sits on, a split frame, a screen or page shown, each with its place in the frame.
The report gives the share of shots in each, the shot kinds tell a layered kind from the same shot alone, and the rules
can say where layers go. `follow` makes a layered kind from two pieces of the footage at once, in the place the
references put it, and says when the footage lacks what it can't make (a screen recording). A videoform read before
layers were gets them by adding its references again: only the layers are read.

See [docs/videoform.md](https://github.com/shinyobjectz/raea/blob/main/docs/videoform.md) for the
plan.

```sh
raea videoform rights                                   # once per account, before the first link
raea videoform new street-interview https://www.example.com/videos/one/ https://youtu.be/abc --context "street interviews for a brand"
raea videoform add street-interview --from street-interview.txt --sheet sheets/   # a list of links, one a line
raea videoform add listings ~/Videos/listings/          # a folder, read where it is
raea videoform ls street-interview
raea videoform report street-interview                  # what they hold, to correct
raea videoform set street-interview loudness ignore --why "the quiet ones weren't mastered"
raea videoform set street-interview cuts_per_min target 30 50
raea videoform vocab street-interview                   # vocabulary.ts started, for your AI to write from the context
raea spec ~/.raea/fingerprints/street-interview/street-interview.script.ts   # the template, checked
raea videoform set street-interview opens:price keep    # a rule confirmed on the contact sheets
raea videoform score street-interview out/cut.mp4       # a cut against what's checked
raea videoform add ugc-ads ~/Videos/brandx/ --brand "Brand X"   # these are Brand X's: what only they hold is Brand X's
raea videoform follow ugc-ads ~/Footage/shoot-12/ --plan plan.json   # what's missing, and the cut planned
raea videoform follow ugc-ads ~/Footage/shoot-12/ --say 128,129,146,370,165 --plan plan.json   # your AI's lines
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea videoform new NAME --context "…" [--extends FP] [--kinds K] [--tags T] [SOURCE...] | raea videoform add NAME SOURCE... [--from LIST] [--last N] [--context "…"] [--sheet DIR] [--brand B] [--group G] | raea videoform docs NAME [GROUP] | raea videoform ls [NAME] [--json] | raea videoform report NAME [--json] | raea videoform set NAME MEASURE ignore|free|keep|target [LO HI] [--why "…"] | raea videoform unset NAME MEASURE | raea videoform score NAME FILE [--json] | raea videoform vocab NAME | raea videoform follow NAME FOOTAGE... [--seconds S] [--shape W:H] [--brand B] [--plan FILE] [--script FILE] [--say N,N,...] [--brief FILE,...] | raea videoform kit BRAND [FILE|DIR...] [--json] | raea videoform rights [--yes]   (a kind of video read once from its references, docs/videoform.md: a fingerprint with a videoform section, found and inherited as any fingerprint is; new makes it, extending your own fingerprint unless --extends names another, judging projects tagged with its name unless --kinds or --tags say which; add reads each file or folder on the drive where it is, and each link (a platform page, a page with its own player, a playlist or channel's newest --last N, a direct link to a file; LIST a file of them, one a line) once, keeping only its readings (source, title, creator, date, shots and pace, how each shot is layered, loudness, Fingerprint's columns by content hash) and never the video, and a reference read before layers were gets only its layers read, making the videoform first when it's new; --context is what the videos are, --sheet DIR a contact sheet of each to look at, --brand B the brand the videos read now are for (what only one brand's videos hold is mined from them alone and written to that brand's own fingerprint, brand-B, made when there's none; what holds across brands is the videoform's); --group G the group they're in (a brand and its round, "Dutch R1"; the brand when it's left out) with the documents given among the sources (a brief, a brand's guide, notes: PDF, Word, text, or a still named), each read once into passages and kept by content hash, and --context, the person's words, kept as theirs and over any document (VF8f): every add then finds each passage where its group's videos carry it out and how, what two groups or more asked and how it's made is the videoform's, what one group asked its brand's, and briefs.ts, the rules your AI reads in them that a similarity can't (a never, an order, a choice), measured on each group's videos; docs lists each passage with its id and where it was carried out, for your AI to write briefs.ts from; ls says what each videoform has read; report says what its references hold, measure by measure (shot lengths, cuts a minute, words a minute, pauses, framing, captions, how it's layered (a person cut out over another picture, a picture inset, a card laid over, a plain backdrop, a split frame, a screen shown), speech and music, cuts on the beat, loudness): each range, how strongly it holds left one out, measures where they fall in two groups, and references far from the rest, as add does after reading; set is your word over theirs, kept with why (ignore: not the videoform's; free: it varies, not checked; keep: checked though it varies; target LO HI: checked against yours), unset takes it back, and raea's delivery floor (-14 LUFS, true peak -1 dBTP) comes before both; score reads a cut as a reference is read and says which checked measures fall outside, and which way to move; vocab starts vocabulary.ts beside fingerprint.ts for your AI to write from the context (the context says what matters, the references show how it's done: places and things to look for, sounds, words on screen and said, the sections the videos move through, and the context's words for how they feel, with the measures that say it), and once it's written, report and vocab say what each term did on the references (never fires, fires everywhere, fires with another, missing where the context says always), where each section was found, shots that recur with no term naming them, the shot kinds the references are made of (named in the vocabulary's words, with their scale, people and length, and how many references have each), and the structure, section by section, written as NAME.script.ts, the template a project made to the videoform starts from; and the rules that hold in most of the references (what every video has, how it opens and closes, what each section is made of and which comes first, a place's things shown with it, what follows a word said or shown), each with how strongly it holds, written as NAME.taste.ts, taste lines with their checks, with a contact sheet a shot kind from the references' stills to confirm them on (set NAME RULE keep|free|ignore is your word on one: kept, a rule broken is an error); follow reads new raw footage as references are read, once (a sentence said or a few seconds with no talk a piece, kept by content hash), matches each piece to the shot kinds, says what the footage lacks that most references have (a shot list), and plans the cut slot by slot from the structure to its lengths, a layered kind made from two pieces at once in the place the references put it (the speaker cut out over B-roll, B-roll in a box over the speaker) (--seconds S for another length; --shape W:H the frame it's made for; --brand B each card, backdrop and inset shown from B's kit, the asset that fits what's said over it, and what the kit lacks said; --plan FILE writes it as JSON, its pieces numbered; --script FILE writes it as NAME.script.ts, the script a project that approves its script first (the brief's approve: ["script"]) shows the person before anything is edited; --say N,N,... the lines your AI chose by what they say, in its order, with the picture cut over them; --brief FILE,... the new job's documents, and the project's brief.ts, each passage set against what the videoform learned of briefs: the ask it's like and how that's made, or new); kit is a brand's kit, kept in its fingerprint (brand-B, made when there's none) so a team shares it: each still in a folder given and each still or clip named, copied in once by content hash and read for what it is (a logo, a product, a person, a testimonial, a screenshot, a graphic, a clip), its words and colours; with no files, what it holds; rights is the person's once-per-account confirmation that they may use what they add from links)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--brand` | `B` | With `add`: the brand the videos read now are for. What only one brand's videos hold is mined from them alone and written to that brand's own fingerprint (brand-B, made when there's none); what holds across brands is the videoform's. |
| `--brief` | `FILE,...` | With `follow`: the new job's documents (a brief, a brand's guide), with the project's brief.ts, each passage set against what the videoform learned of briefs: the ask it's like and how that was made, or new. |
| `--context` | `"…"` | What the videos are and what to look out for, in your words ("listing walkthroughs for social"), kept as yours with the group: your words outrank any document's. |
| `--extends` | `FP` | With `new`: the fingerprint it builds on. Without it, your own fingerprint (the account's default, or `me`) when you have one. |
| `--from` | `LIST` | A file of sources, one a line (links, files or folders); a line starting with # is skipped. |
| `--group` | `G` | With `add`: the group the videos and documents given now are in, a brand and its round ("Dutch R1"); the brand when it's left out. What two groups or more asked, and how it was made, is the videoform's. |
| `--json` | – | With `ls`: every reference's readings as JSON. With `report`: what's found, every reference's value of each measure. With `score`: the cut's measures and the ones out. |
| `--kinds` | `K` | With `new`: the kinds of piece it judges (film, ad, game, playable, still, sound), comma-separated. |
| `--last` | `N` | For a playlist or a channel: read only its newest N videos. |
| `--plan` | `FILE` | With `follow`: write the plan as JSON to FILE: every piece of the footage numbered with its shot kind, what the footage lacks, the lines said and the picture over them. |
| `--say` | `N,N,...` | With `follow`: the lines your AI chose by what they say, as the plan's piece numbers in the order they're to be heard; raea cuts the picture over them, section by section. |
| `--script` | `FILE` | With `follow`: write the plan as FILE, a NAME.script.ts the person approves before anything is cut. |
| `--seconds` | `S` | With `follow`: the cut's length in seconds, in place of the references' median. |
| `--shape` | `W:H` | With `follow`: the frame the cut is made for, as 9:16. |
| `--sheet` | `DIR` | Write a contact sheet of each reference read into DIR, to look at what was read. |
| `--tags` | `T` | With `new`: the project tags it judges, comma-separated. Without --kinds or --tags, a project tagged with the videoform's name. |
| `--why` | `"…"` | With `set`: why you're correcting the measure, kept with your correction. |
| `--yes` | – | With `rights`: your own confirmation that you have the right to use what you add. An agent never gives it for you. |

<!-- generated:end -->
