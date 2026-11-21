# Settings

raea reads each setting from the environment first, then from `~/.raea/env` (or the file `RAEA_ENV_FILE` names):
one `KEY=value` per line, `#` for comments. Nothing has a hidden default host: a feature that needs a service says
which setting is missing.

```sh
mkdir -p ~/.raea && touch ~/.raea/env && chmod 600 ~/.raea/env
```

## Network

| Setting | What |
|---|---|
| `RAEA_OFFLINE=1` | raea makes no network requests: a remote URL not already in raea's cache, a model or sample library not yet installed, a composition's own `fetch()` or WebSocket, Bevy's http asset source and every `raea gen` step are refused with what wanted out. What's on disk keeps working |

## Generation (`raea gen`, optional)

| Setting | What |
|---|---|
| `FISH_API_KEY` | Your Fish Audio key, for an `audio.speech` step on `fish:<model>` (`fish:` alone is s2.1-pro); a plan quotes Fish's price for the words first |

## Local audio models

Two models make sound on your Mac, free, with no account. Each installs itself the first time a composition or a
command asks for it (a few minutes, once), or ahead of time with `raea setup-audio`.

| Setting | What |
|---|---|
| `RAEA_TOKENSYNTH` | Where TokenSynth lives (default `~/.cache/raea-tokensynth`, about 6 GB): an instrument described in words, one note at a time, for a Tone.js `Sampler` |
| `RAEA_SAO` | Where Stable Audio Open Small lives (default `~/.cache/raea-sao`, about 2 GB, Core ML): foley and sound effects |
| `RAEA_AUDIO_AUTOSETUP=0` | Don't install on first use; a missing model's error names the command (`tools/sao/setup.sh` on a Mac or Linux, `uv run --no-project --python 3.11 python -I tools\sao\setup.py` on Windows) |
| `RAEA_SAO_CUDA` | SAO off a Mac: `1` installs ONNX Runtime with CUDA, `0` the CPU build; unset, an NVIDIA GPU (`nvidia-smi`) decides |
| `RAEA_TOOLS_CACHE` | Where raea keeps the tools it fetches when they're not on the PATH (default `~/.cache/raea-tools`, each checked against its SHA-256): uv 0.12.24, which every local model's setup runs through; and on Windows FFmpeg 9.0.2 (about 210 MB), fetched the first time a video raea's own decoders can't read is opened (a phone's or a camera's: H.264 with B-frames, HEVC, VP9) |

From the command line, `raea sao "PROMPT" -o hit.wav` and `raea tokensynth "TIMBRE" --notes C3,C4 -o
notes/` make sounds through the same cache a composition's render uses, and `raea sounds` searches open sample
libraries (pianos, General MIDI instruments, orchestral and percussion samples, each with its licence).

## Checks and guides (optional)

| Setting | What |
|---|---|
| `CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_API_TOKEN` | The decision model (clef-omni on Workers AI) on your own Cloudflare account, for `raea ask`, `raea judge` and the checks that ask. Signed in to raea on Pro or Studio, the app pays from your plan's monthly pool first (docs/ask.feature) |
| `RAEA_AGENT_KEY`, `RAEA_API` | An agent key with the `ask` scope, and the API address it came with: asks from your plan's pool with no app running (a pipeline, CI, a server) |
| `RAEA_ASK_ROUTE` | `app`, `agent` or `own`: ask only that way, rather than the first that answers |
| `RAEA_ASK_DIR` | Where `raea ask` keeps its answers, the clips it cut and its cache (`~/.raea/ask`) |
| `RAEA_CLEF_REPLAY` | A JSON file of answers by question id, given back for every call: for tests, which then never spend |
| `RAEA_GUIDES` | A folder of craft references for the agent's `guide` tool (`captions.md`, `motion-rules.md`, ...) |

## Rendering

| Setting | What |
|---|---|
| `WGPU_BACKEND` | The GPU driver API raea draws with: `dx12`, `vulkan`, `gl` (or several, comma-separated). Unset, raea picks: on Windows, DirectX 12 when the PC has an Intel or AMD GPU (also beside an NVIDIA one: their OpenGL and Vulkan drivers crashed raea there), else wgpu's own pick, which is Vulkan on an NVIDIA-only PC. Set it when the window or a render fails on a GPU, e.g. `WGPU_BACKEND=dx12` |
| `RAEA_VELLO_BUMP=full` | Pages are drawn with Vello's own buffer sizes from the first frame, instead of raea's smaller ones (about 100 MB less GPU memory). A page past raea's sizes is found and drawn again at Vello's anyway, with a line on stderr, so this only saves that redraw |
| `RAEA_PAGE_DRAWN_CHECK=0` | Pages are drawn without checking that Vello drew them. For measuring what the check costs (about 2% in one test): a page past Vello's buffers then shows the frame before it again, or nothing |
| `RAEA_NOT_APPLIED=0` | A page's CSS is parsed without noting what stylo drops, so a render ends without its NOTE of what the page didn't apply (and its manifest's `not_applied` lists only loads that failed). For measuring what the note costs |
| `RAEA_PANIC_FAULT=N` | A panic while frame N draws, for checking what a failing render keeps (bench/frame-panic). The render then ends with an error naming the frame and the panic, and keeps the frames drawn before it |
