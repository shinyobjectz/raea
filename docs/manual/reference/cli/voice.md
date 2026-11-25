---
title: "raea voice"
summary: "Speech in and out on this computer, as lines of JSON on stdin and stdout: what raea.app's Ideate hears and speaks with. While it speaks it doesn't listen, unless --duplex (headphones: it listens all the time) or --over (speakers: its own voice is taken out of the microphone's sound, new and not yet tried across real rooms)."
order: 87
sources: ["engine/cli/src/cli.rs"]
---

Speech in and out on this computer, as lines of JSON on stdin and stdout: what raea.app's Ideate hears and speaks with. While it speaks it doesn't listen, unless --duplex (headphones: it listens all the time) or --over (speakers: its own voice is taken out of the microphone's sound, new and not yet tried across real rooms).

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea voice [--voice NAME] [--duplex] [--over] [--pause MS] [--from IN.wav] [--to OUT.wav] [--room GAIN] [--fast] | raea voice hear FILE.wav | raea voice say "WORDS" -o OUT.wav [--voice NAME]   (speech in and out on this computer, as lines of JSON on stdin and stdout: what raea.app's Ideate hears and speaks with. While it speaks it doesn't listen, unless --duplex (headphones: it listens all the time) or --over (speakers: its own voice is taken out of the microphone's sound, new and not yet tried across real rooms); --room GAIN is a simulated room for checking --over with files; hear writes a recording's words, say speaks words to a file)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--duplex` | – | Listen all the time, with nothing taken out of the microphone's sound. For headphones: through speakers it hears itself and stops. |
| `--fast` | – | With `--from`: read the recording as fast as it can be heard, not at the pace it was spoken. For tests. |
| `--from` | `IN.wav` | A recording in the microphone's place. |
| `--over` | – | Listen while it speaks, with its own voice taken out of the microphone's sound, so the person can talk over it through speakers. New: checked in a simulated room, not yet across real rooms. The first word or two said over it can be lost. |
| `--pause` | `MS` | How long a silence ends the person's turn, in milliseconds. |
| `--room` | `GAIN` | With `--from` and `--over`: a simulated room. What it says comes back into the recording at this gain (0.5 is a quiet room, 1.5 a loud one). |
| `--to` | `OUT.wav` | A file in the speakers' place: everything it says is written there. |
| `--voice` | `NAME` | The voice it speaks in, one of Pocket's own (`alba` when not given). |
| `-o` | `OUT.wav` | `say`: the file the spoken words are written to. |

<!-- generated:end -->

## What it reads and writes

One JSON object a line. On stdin: `{"say":{"id":"a","text":"..."}}` adds words to what it's saying (send them as they
come), `{"end":"a"}` says that's all of them, `{"stop":true}` stops it mid-sentence, `{"mute":true}` and
`{"mute":false}` close and open the microphone, `{"quit":true}` ends the session.

On stdout: `{"ready":...}` once the models are loaded, `{"speech":"began"}` and `{"speech":"ended"}` as the person
talks, `{"heard":{"text":"..."}}` with their words when their turn ends, `{"saying":{"id":"a"}}`,
`{"said":{"id":"a"}}` or `{"stopped":{"id":"a"}}` for what it spoke, `{"level":-32.5}` for the microphone's loudness,
and `{"error":"..."}`. A microphone raea isn't allowed to use gives silence with no error from the system: after three
seconds of it, the error says to allow raea in the system's privacy settings.

```sh
raea voice hear note.wav                       # a recording's words
raea voice say "Good morning." -o hello.wav    # words to a file
```

The models (Silero's voice detector, Parakeet for the words, Pocket for the voice) run on this computer; nothing is
sent anywhere.
