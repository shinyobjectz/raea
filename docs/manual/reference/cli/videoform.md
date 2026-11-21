---
title: "raea videoform"
summary: "A kind of video you make again and again, read once from its references: files on your drive or links, keeping only what was measured, never the video."
order: 45
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
a word said or shown (a punch-in, words on screen), where that's more than chance. They're written as `NAME.rules.ts`
beside the template, a controller whose fields are rules, each naming the check `raea check` holds a cut to with it.
Under five references every rule is a note; a strong one warns. Each reading keeps a small still a shot, and each
report makes a contact sheet a shot kind from them (`videoform/kinds/kind-N.jpg`, the kind a rule's `kind-N` names) to
confirm the rules on: `raea videoform set NAME RULE keep` makes one an error to break, `free` a note, and `ignore`
drops it, each kept through every reference added.

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
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea videoform new NAME --context "…" [--extends FP] [--kinds K] [--tags T] [SOURCE...] | raea videoform add NAME SOURCE... [--from LIST] [--last N] [--context "…"] [--sheet DIR] | raea videoform ls [NAME] [--json] | raea videoform report NAME [--json] | raea videoform set NAME MEASURE ignore|free|keep|target [LO HI] [--why "…"] | raea videoform unset NAME MEASURE | raea videoform score NAME FILE [--json] | raea videoform vocab NAME | raea videoform rights [--yes]   (a kind of video read once from its references, docs/videoform.md: a fingerprint with a videoform section, found and inherited as any fingerprint is; new makes it, extending your own fingerprint unless --extends names another, judging projects tagged with its name unless --kinds or --tags say which; add reads each file or folder on the drive where it is, and each link (a platform page, a page with its own player, a playlist or channel's newest --last N, a direct link to a file; LIST a file of them, one a line) once, keeping only its readings (source, title, creator, date, shots and pace, loudness, Fingerprint's columns by content hash) and never the video, making the videoform first when it's new; --context is what the videos are, --sheet DIR a contact sheet of each to look at; ls says what each videoform has read; report says what its references hold, measure by measure (shot lengths, cuts a minute, words a minute, pauses, framing, captions, speech and music, cuts on the beat, loudness): each range, how strongly it holds left one out, measures where they fall in two groups, and references far from the rest, as add does after reading; set is your word over theirs, kept with why (ignore: not the videoform's; free: it varies, not checked; keep: checked though it varies; target LO HI: checked against yours), unset takes it back, and raea's delivery floor (-14 LUFS, true peak -1 dBTP) comes before both; score reads a cut as a reference is read and says which checked measures fall outside, and which way to move; vocab starts vocabulary.ts beside fingerprint.ts for your AI to write from the context (the context says what matters, the references show how it's done: places and things to look for, sounds, words on screen and said, the sections the videos move through, and the context's words for how they feel, with the measures that say it), and once it's written, report and vocab say what each term did on the references (never fires, fires everywhere, fires with another, missing where the context says always), where each section was found, shots that recur with no term naming them, the shot kinds the references are made of (named in the vocabulary's words, with their scale, people and length, and how many references have each), and the structure, section by section, written as NAME.script.ts, the template a project made to the videoform starts from; and the rules that hold in most of the references (what every video has, how it opens and closes, what each section is made of and which comes first, a place's things shown with it, what follows a word said or shown), each with how strongly it holds, written as NAME.rules.ts, a controller of rule fields, with a contact sheet a shot kind from the references' stills to confirm them on (set NAME RULE keep|free|ignore is your word on one: kept, a rule broken is an error); rights is the person's once-per-account confirmation that they may use what they add from links)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--context` | `"…"` | What the videos are, in your words ("listing walkthroughs for social"), kept as the videoform's context. |
| `--extends` | `FP` | With `new`: the fingerprint it builds on. Without it, your own fingerprint (the account's default, or `me`) when you have one. |
| `--from` | `LIST` | A file of sources, one a line (links, files or folders); a line starting with # is skipped. |
| `--json` | – | With `ls`: every reference's readings as JSON. With `report`: what's found, every reference's value of each measure. With `score`: the cut's measures and the ones out. |
| `--kinds` | `K` | With `new`: the kinds of piece it judges (film, ad, game, playable, still, sound), comma-separated. |
| `--last` | `N` | For a playlist or a channel: read only its newest N videos. |
| `--sheet` | `DIR` | Write a contact sheet of each reference read into DIR, to look at what was read. |
| `--tags` | `T` | With `new`: the project tags it judges, comma-separated. Without --kinds or --tags, a project tagged with the videoform's name. |
| `--why` | `"…"` | With `set`: why you're correcting the measure, kept with your correction. |
| `--yes` | – | With `rights`: your own confirmation that you have the right to use what you add. An agent never gives it for you. |

<!-- generated:end -->
