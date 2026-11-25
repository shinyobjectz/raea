---
title: "raea media"
summary: "Probe, cut, measure and encode media natively, the same on the Mac and the iPhone, with no ffmpeg."
order: 62
sources: ["io/media/src", "engine/cli/src/cli.rs"]
---

`raea media` does the media work agents used to reach for ffmpeg and ffprobe for, on raea's own decoders and
encoders: probing a file, pulling a still or a contact sheet, reading loudness and motion, finding cuts and freezes,
trimming, joining and encoding. It works the same on every platform raea runs on, and nothing calls an ffmpeg binary.

```sh
raea media probe take.mov                          # duration, size, fps, codec and audio, as JSON
raea media still take.mov --at 12.5 -o at-12.png
raea media sheet piece.mp4 -o sheet.png            # a contact sheet of the whole piece
raea media loudness piece.mp4                      # integrated LUFS and true peak
raea media cuts footage.mp4 --threshold 0.08       # where the shots change
raea media trim take.mov --from 12 --to 18.5 -o line.mp4
```

`raea ffmpeg ...` and `raea ffprobe ...` read the command lines models write by habit and run them as these verbs;
what they can't read is refused with the verb or edit that does it.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
`raea media VERB ...`: the agent's media work, native and the same on the Mac and the iPhone (docs/media.feature M1).
What agents used ffmpeg and ffprobe for, on raea-media (the platform's decoders and encoders, Symphonia's audio):

  raea media probe FILE                                   duration, size, fps, codec, audio (JSON)
  raea media still FILE --at S [--width W] -o OUT.png|.jpg   the frame showing at S (an image: itself), W px wide at most
  raea media sheet FILE... [--n 12] [--width 240] -o OUT  a contact sheet (n frames of one file, or each file's middle),
                                                          and OUT.labels.json: each cell's time or file, for raea show
  raea media audio FILE [--rate 16000] [--mono] [--from S --to S] -o OUT.wav|-   (- is f32le on stdout)
  raea media loudness FILE [--from S --to S]              integrated LUFS, mean and peak dB, true peak
  raea media motion FILE [--from S --to S] [--fps 6]      how much the picture changes (mean luma difference, YDIF)
  raea media cuts FILE [--threshold 0.08]                 shot changes (ffmpeg's scene score)
  raea media freezes FILE [--noise 0.001] [--min 2]       held pictures (ffmpeg's freezedetect)
  raea media proxy FILE [--height 720] -o OUT.mp4         a smaller copy
  raea media share FILE [--kbps 2600] [--height H] -o OUT.mp4   a copy to send: any player takes it, held to a bitrate
  raea media lod MODEL.glb [--level 1] [--texture 1024] -o OUT.glb   a model at a lower level of detail (triangles halved a level)
  raea media trim FILE --from S --to S -o OUT.mp4         a stretch, frame-exact (re-encoded)
  raea media concat A B ... -o OUT.mp4                    one after another (same aspect)
  raea media encode --size WxH [--fps 60] [--quality Q] [--audio FILE] -o OUT.mp4   raw RGBA frames on stdin, signed

`raea ffmpeg ...` and `raea ffprobe ...` read the command lines models write by reflex and run them as these verbs; what
they can't read is refused with the verb or edit that does it. No ffmpeg binary is involved.
```

Headless: it needs nothing running and opens no window. Its arguments go to a parser of its own, whose forms are listed above.

<!-- generated:end -->
