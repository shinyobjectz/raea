# Settings

raea reads each setting from the environment first, then from `~/.raea/env` (or the file `RAEA_ENV_FILE` names):
one `KEY=value` per line, `#` for comments. Nothing has a hidden default host: a feature that needs a service says
which setting is missing.

```sh
mkdir -p ~/.raea && touch ~/.raea/env && chmod 600 ~/.raea/env
```

## The agent's model

void's agent streams the Anthropic Messages API, so any provider that serves it works. Pick one with
`RAEA_LLM_PROVIDER`:

| `RAEA_LLM_PROVIDER` | Endpoint | Key variable | Model |
|---|---|---|---|
| `openrouter` (default) | `https://openrouter.ai/api` | `OPENROUTER_API_KEY` | `RAEA_LLM_MODEL`, default `z-ai/glm-5.3-flash` |
| `anthropic` | `https://api.anthropic.com` | `ANTHROPIC_API_KEY` | `RAEA_LLM_MODEL` (required) |
| `zai` | `https://api.z.ai/api/anthropic` | `ZAI_API_KEY` | `RAEA_LLM_MODEL` (required) |
| `deepseek` | `https://api.deepseek.com/anthropic` | `DEEPSEEK_API_KEY` | `RAEA_LLM_MODEL` (required) |
| `moonshot` | `https://api.moonshot.ai/anthropic` | `MOONSHOT_API_KEY` | `RAEA_LLM_MODEL` (required) |
| `custom` | `RAEA_LLM_BASE_URL` | `RAEA_LLM_API_KEY` | `RAEA_LLM_MODEL` (required) |

`RAEA_LLM_API_KEY` overrides the provider's own key variable. For a `custom` endpoint that wants
`Authorization: Bearer` instead of `x-api-key`, set `RAEA_LLM_AUTH=bearer`.

```sh
# ~/.raea/env
RAEA_LLM_PROVIDER=anthropic
RAEA_LLM_MODEL=claude-sonnet-5-5
ANTHROPIC_API_KEY=...
```

## raea's API (optional)

A few features call a small HTTP service you run yourself: brand kits, generated speech (Fish narration), and
remote check runners. Without it, everything else works; those features say they need `RAEA_API`. Music and sound
effects don't need it: TokenSynth (an instrument's notes) and Stable Audio Open (foley) run on your Mac and install
themselves the first time they're used (see "Local audio models" below).

| Setting | What |
|---|---|
| `RAEA_API` | The service's base URL, e.g. `http://localhost:8787` |
| `RAEA_API_TOKEN` | A bearer token for it |
| `RAEA_GEN_API`, `RAEA_GEN_TOKEN` | A separate service for generation only (default: `RAEA_API`, `RAEA_API_TOKEN`) |
| `RAEA_GEN_PAID=0` | Turn paid generation off; `RAEA_GEN_BUDGET=N` caps requests per run |

The generation contract is two endpoints, each answering with audio (`audio/mpeg`, `audio/wav` or `audio/mp4`):

```
POST {RAEA_GEN_API}/v1/gen/music    {"prompt": "...", ...params}   Authorization: Bearer {RAEA_GEN_TOKEN}
POST {RAEA_GEN_API}/v1/gen/speech   {"text": "...", ...params}     Authorization: Bearer {RAEA_GEN_TOKEN}
```

Any server that implements those two routes works: a local one on your Mac, or a function on a serverless host that
forwards to the speech model you use. raea caches every result by prompt, model and seed, so a render asks once.
(`/v1/gen/music` serves `lyria()`, which still works but isn't what raea's guides use: music is written as Tone.js
instruments.)

## Local audio models

Two models make sound on your Mac, free, with no account. Each installs itself the first time a composition or a
command asks for it (a few minutes, once), or ahead of time with `raea-motion setup-audio`.

| Setting | What |
|---|---|
| `RAEA_TOKENSYNTH` | Where TokenSynth lives (default `~/.cache/raea-tokensynth`, about 6 GB): an instrument described in words, one note at a time, for a Tone.js `Sampler` |
| `RAEA_SAO` | Where Stable Audio Open Small lives (default `~/.cache/raea-sao`, about 2 GB, Core ML): foley and sound effects |
| `RAEA_AUDIO_AUTOSETUP=0` | Don't install on first use; a missing model's error names the command (`tools/sao/setup.sh`, `tools/tokensynth/setup.sh`) |

From the command line, `raea-motion sao "PROMPT" -o hit.wav` and `raea-motion tokensynth "TIMBRE" --notes C3,C4 -o
notes/` make sounds through the same cache a composition's render uses, and `raea-motion sounds` searches open sample
libraries (pianos, General MIDI instruments, orchestral and percussion samples, each with its licence).

## Checks and guides (optional)

| Setting | What |
|---|---|
| `RAEA_CHECK_PACKS` | The folder of check packs (default `~/.raea/checks/packs`) |
| `RAEA_CHECKS_API` | A check service to run packs remotely |
| `CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_API_TOKEN` | The checks' judge model (clef-omni on Workers AI) |
| `RAEA_GUIDES` | A folder of craft references for the agent's `guide` tool (`captions.md`, `motion-rules.md`, ...) |
