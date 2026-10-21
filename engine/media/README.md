# media

`raea-media`: decoding and encoding behind one seam (`Backend`), so an edit's clips, voice and music never depend on
one tool. A decoder answers "the frame showing at time t", so frames come back the same in any order of requests.
Muxing is pure Rust everywhere (muxide, vendored in `vendor/muxide`).

| Backend | What it uses |
|---|---|
| `apple` | AVFoundation and VideoToolbox: HEVC and H.264 decode, H.264, HEVC and AAC encode |
| `soft` | portable software: openh264, re_mp4, Symphonia, Opus |
| `av1` | dav1d decode, rav1e encode |
| `ffmpeg` | the ffmpeg CLI, as a baseline for tests |

Audio is handled as columns: decoding, mixing, resampling and loudness are raea's own code. The `raea-media` binary
benchmarks and cross-checks the backends. [`docs/licences.feature`](docs/licences.feature) records what each backend
links and under which licence.

Used by [`engine/rdb/`](../rdb), [`sdk/react/`](../../sdk/react), [`ai/perception/`](../../ai/perception) and [`engine/kgpu/`](../kgpu).
