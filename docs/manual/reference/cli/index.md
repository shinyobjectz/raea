---
title: "CLI"
summary: "Every raea verb: what it does, its forms, where it runs and each flag."
order: 1
sources: ["engine/cli/src/cli.rs", "engine/cli/src/usage.rs", "tools/manual-ref/cli.ts"]
---

`raea` is one program with a verb for each job. Each verb has a page here with its forms, where it runs, every flag
it takes and what each one does. The forms and flags are generated from the binary itself (`raea help --json`), so
they match the version they were made from; the prose around them is written by hand.

## How every verb behaves

- **Help.** `raea help` (or `raea` alone) lists every verb's forms. `raea help --json` gives the same as data: each
  verb's forms, its flags, how many values each flag takes, how many arguments it takes and where it runs.
- **Strict arguments.** A verb refuses a flag or an argument its forms don't name, rather than ignoring it, so a
  typo is caught. The five verbs with parsers of their own (`media`, `gen`, `physics`, `blender`, `godot`) list their own forms.
- **Output.** A verb prints its result on stdout and exits 0. When it can't do what it was asked, it prints a line
  starting with `REFUSED` that says why and what to do instead, and exits 1. A file refused again is told what
  changed since the last refusal.
- **Where it runs.** Almost every verb is headless: it needs nothing running and opens no window, so it works over
  SSH, in CI and from an agent. `raea library` needs the raea app running, and adding a folder waits for a person's
  yes in the app.
- **Sessions.** Each `render` and `play` records its session; [`raea traces`](traces.md) lists them.
- **Version.** `raea --version` prints the version. The agent skill and these pages match it.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json` -->

## Make

Render, play and package what you wrote.

| Verb | What it does |
| --- | --- |
| [`raea render`](render.md) | Render a composition to an MP4, a PNG or a folder of frames, on the GPU and headless. |
| [`raea play`](play.md) | Play a game live in a window, or headless with scripted keys, snapshots and a timing report. |
| [`raea export-game`](export-game.md) | Package a game as a program of its own: a Mac app or a command-line program. |
| [`raea export`](export.md) | The composition as a CapCut draft ready in its timeline: its footage and sounds as clips of their own files at their trims and gains, everything raea draws as transparent overlay clips above them. |
| [`raea bundle`](bundle.md) | Bundle a composition into one JavaScript file that render_bundle draws, on the Mac or the iPhone. |
| [`raea app`](app.md) | Run, check headless and package a desktop app written as one TSX file on @raea/ui. |
| [`raea run`](run.md) | Run a TypeScript program on the engine, headless, and print what it prints. |

## Projects

The workspace, its projects and what a project holds.

| Verb | What it does |
| --- | --- |
| [`raea init`](init.md) | Add a project's starting files where they're missing: edit.tsx, cuts.ts, components/ and AGENTS.md. |
| [`raea workspace`](workspace.md) | Make, adopt or show a workspace: the git repository that holds a studio's projects and house taste. |
| [`raea project`](project.md) | Make a project in the workspace, list them, or move one between inline and a repository of its own. |
| [`raea spec`](spec.md) | Print a spec file's default export as JSON, checked against its schema: briefs, scripts, taste and an object's parts. |
| [`raea component`](component.md) | Copy a ready-made component or set into the project's components/, or the workspace's shared/components/, to use and change as your own. |
| [`raea part`](part.md) | The old name of raea component: copy a ready-made component into the project's components/, to use and change as your own. |
| [`raea add`](add.md) | Add, remove and install npm packages for a project, with no Node or npm: raea.lock and node_modules. |
| [`raea brand`](brand.md) | Read a brand kit from a website on this machine: colours, fonts and logos into brand/ and BRAND.md. |
| [`raea library`](library.md) | Search and manage the person's media libraries in the raea app: folders and drives indexed on this machine, searched by meaning. |
| [`raea index`](indexer.md) | Make every picture on a folder or a drive searchable by words, embedded on this Mac's Neural Engine, and keep the index open for the raea app. |

## Check

What's wrong before anyone watches it.

| Verb | What it does |
| --- | --- |
| [`raea check`](check.md) | Check a composition's text in a second, a project against every rule it holds, or an export against the floor: text, contrast, safe areas, freezes, flashing, cues and loudness. |
| [`raea judge`](judge.md) | Judge a 3D scene's review sheets blind, by a model on a frozen rubric or a fresh agent session, with the pass decided in code. |
| [`raea fingerprint`](fingerprint.md) | A person's sense of good work, learned on this Mac: axes with small heads over on-device encoders that score a render moment by moment and say what they can't judge yet. |
| [`raea videoform`](videoform.md) | A kind of video you make again and again, read once from its references: files on your drive or links, keeping only what was measured, never the video. |
| [`raea findings`](findings.md) | What each render and check found, read back from the project's record: fixed, new and carried since the one before. |
| [`raea scene`](scene.md) | Find floating, sunk and clipping 3D parts, say what each part rests on, and meter a shot's light. |
| [`raea light`](light.md) | Measure a scene's light on each light path against its path-traced reference: distance in stops, the worst tile, flicker and cost. |
| [`raea world`](world.md) | A frame as one world: its layers, text, 3D parts, camera, shots, clips, cues and lines as entities, queried across media. |
| [`raea solid`](solid.md) | Measure, view and export a solid: parts built from code with exact volumes, joints and parameters. |
| [`raea next`](next.md) | The two or three most useful next steps for a project, read from its brief, compositions, renders and checks. |
| [`raea doctor`](doctor.md) | Find what doesn't match this version of raea: old brief files, an older workspace format, an agent's stale skill. |
| [`raea hook`](hook.md) | Every agent's tool hooks: media opened in the Mac's viewer goes to raea show, and the quick check after a composition is saved. |
| [`raea traces`](traces.md) | List the sessions raea apps left, newest first, with how each ended and its glitches, or tell one in full. |

## Footage

Takes, cuts and the camera, from your own media.

| Verb | What it does |
| --- | --- |
| [`raea media`](media.md) | Probe, cut, measure and encode media natively, the same on the Mac and the iPhone, with no ffmpeg. |
| [`raea cut`](cut.md) | Cut a take to the lines it says, transcribing it first, and write cuts.ts and words.json. |
| [`raea frame`](frame.md) | Say how a take fits the edit: a talking take for <FaceFrame>, a screen recording for <ScreenFit>, from where its face is. |
| [`raea track`](track.md) | Solve a clip's camera on this machine, so 3D can stand in its world with <Matchmove>. |
| [`raea splat`](splat.md) | Train footage of a real place or thing into a 3D Gaussian splat for <Splat>, on this Mac's GPU. |
| [`raea mezz`](mezz.md) | Make an intra-only ProRes copy of each long-GOP source over 1080p, so scrubbing it is smooth. |
| [`raea neural`](neural.md) | Run DLSS 5 neural rendering over a render's frames on the Mac, through your own MLX-DLSS build and weights. |

## Sound

Music read, foley and instruments made on this machine.

| Verb | What it does |
| --- | --- |
| [`raea song`](song.md) | Read a song's energy map: beats, bars, sections, builds, drops, stops, hits and cuts per bar. |
| [`raea sounds`](sounds.md) | Search open sample libraries for instruments and drums, and fetch one for a Sampler. |
| [`raea sao`](sao.md) | Make a sound effect from a description with Stable Audio Open, on this machine. |
| [`raea tokensynth`](tokensynth.md) | Make an instrument's notes from a description of its timbre with TokenSynth, on this machine. |
| [`raea setup-audio`](setup-audio.md) | Install or check the local audio models: Stable Audio Open for foley and TokenSynth for instruments. |

## Generate and 3D tools

Generated media, physics and weathering bakes, clothes and motion for SOMA's body, Blender and Godot.

| Verb | What it does |
| --- | --- |
| [`raea gen`](gen.md) | Generate images, video, music, sound, speech, 3D models and text through made.app, fal, Replicate, OpenRouter, Modal or ComfyUI. |
| [`raea physics`](physics.md) | Make the bakes <Physics> and <Goo> play: rigid bodies, soft goo, and parts cut out of a .glb. |
| [`raea wear`](wear.md) | Bake what weathers a model: where rain reaches and runs, what the sun sees, edges, hollows and touch, as maps raea/wear reads. |
| [`raea clothes`](clothes.md) | Clothes for SOMA's body from words: make sews a garment from a sewing pattern (GarmentCode, MIT), cut to SOMA's measurements and draped by raea's own cloth, as assets/clothes/NAME.json and its .glb; cover lays one on the skin by procedure; wear puts garments on a body. |
| [`raea motion`](motion.md) | Body motion made on this machine by Kimodo (NVIDIA), on MLX on a Mac and on PyTorch on Linux and Windows: a SOMA clip from a line of words and, with --keys, Kimodo constraints (full-body keyframes, hand or foot targets, a root path, as sdk/body/tools/between.ts writes them), as the clip sdk/body/tools/fromkimodo.ts reads. |
| [`raea blender`](blender.md) | Run a Blender Python script headless, with raea's library of mesh scripts: convert, decimate, repair, split, bake, rig, cloth. |
| [`raea godot`](godot.md) | Run a TypeScript script against Godot's whole API in a headless Godot with GodotJS, record what it draws as frames, or open the editor on the project. |

## Agents and setup

What an agent reads and shows the person, and the tools raea installs.

| Verb | What it does |
| --- | --- |
| [`raea guide`](guide.md) | Print raea's craft guides: the floor read before any edit, the modes, the notes and the 3D guides. |
| [`raea context`](context.md) | Print what to read before working on a project: its taste, brief and look, the floor and its modes' guides. |
| [`raea notes`](notes.md) | A person's notes as files the work is held to: each taken word for word, then restated as asks before anything changes. |
| [`raea show`](show.md) | Show the person something in the raea app, as an item they can use and answer: stills, a video, a sound, a script, an interactive .tsx or a question. |
| [`raea skill`](skill.md) | Print the agent skill this raea ships, matched to its version, or one of its references. |
| [`raea skills`](skills.md) | Install raea's agent skill where Claude Code, Codex, Kimi or Gemini read skills, with its MCP server and hook. |
| [`raea setup`](setup.md) | Find Blender, or download its current LTS, and install raea's add-on and the MCP add-on. |
| [`raea bug`](bug.md) | Report a bug in raea itself to raea's server: a line saying what's wrong and what happened, with raea's version and the OS added, no account needed. |
| [`raea ask`](ask.md) | Ask about a piece's picture and sound: clef-omni answers in words and a probability, paid from your raea plan, and every answer is kept so a question can be calibrated. |

<!-- generated:end -->
