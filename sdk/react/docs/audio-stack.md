# The audio stack

What reaches a speaker or a file is PCM from one mixer core. That core runs live in the device's callback and offline
as a render's bounce, so a render and a play of the same frames give the same samples by construction. Neural models
never sit in that path. Generated sound is rendered once, cached as PCM keyed by its inputs, and enters as an ordinary
clip. Separately, frozen encoders turn sound into symbols and bits for the meaning lane (the HDC sandwich, docs/the roadmap
Track K).

This document covers:
- the two lanes;
- where each component sits;
- a verdict on each component evaluated (Firewheel, bevy_seedling, Tone.js and @tonejs/midi, TokenSynth, DAC and CLAP);
- what the spikes measured;
- the recommended path.

Written 2026-10-18 on the M5 Pro.

## The two lanes

**The output lane (deterministic DSP).**
- A frame's sounds (each `<Audio>`/`<Video>` element's source at its source time, gain, rate and fades) and its notes
  (`<audio data-note>` from @raea/motion/music) become voice events.
- One rule makes them (`engine/mixer/src/score.rs`). An element whose source time moved on by its rate is the same voice. A
  new element, or one that jumped (a cut, a seek, a loop), starts a voice. One that's gone stops its voice. A changed
  gain ramps over one frame.
- Every event lands on its frame's sample of the timeline, `round(f * rate / fps)`. A note lands on its own sample.
- The events drive one Firewheel graph: a voices node (sample voices and instrument notes), then a master node (gain,
  then a look-ahead true-peak limiter), then out.
- Live, the graph is pulled from a cpal callback. Offline, it is pulled into a buffer. Both use the same code, the
  same events and the same nodes.

**The meaning lane (frozen encoders).**
- CLAP embeddings and DAC tokens are candidates for the sandwich's audio symbols.
- Nothing in this lane produces what anyone hears.
- A generative model (TokenSynth, Lyria, SAO) renders a stem once, the stem is cached as PCM, and the output lane
  plays it as a clip.

```
TSX frame ──► sounds + notes ──► Score (one rule) ──► events on timeline samples
                                                         │
                         live: origin + t on the device clock │ offline: t
                                                         ▼
                             Firewheel graph: voices ─► master (gain, true-peak limiter)
                                   │                                   │
                       cpal 0.15 callback (play)          processor pulled into a Vec (render's bounce)
```

## Where each component sits

| Component | Lane | Role | Where it lives |
|---|---|---|---|
| Firewheel 0.12 | output | the graph: scheduled events, the voices and master nodes, the processor pulled live and offline | `mixer/` (raea-mixer) |
| cpal 0.15 | output | the device; the callback pulls Firewheel's processor | `engine/mixer/src/live.rs` |
| Score | output | the one rule from frames to events, shared by play and render | `engine/mixer/src/score.rs` |
| @raea/motion/tone | authoring | Tone.js's API (instruments, effects, channels, Transport, Loop/Part/Sequence/Pattern) compiled to a graph and events | `sdk/motion/src/tone.tsx` |
| Tone graph on the core | output | the edit's Tone graph as Firewheel nodes: instruments (tone.rs), effects and channels with ducking (fx.rs) | `engine/mixer/src/tonegraph.rs` |
| @raea/motion/music | authoring | the shorthand: Part and Sequence on named instruments, `<Music>`, MIDI in and out, played through Tone | `sdk/motion/src/music.tsx` |
| bevy_seedling 0.8 | output | a game's samples, pools and effect chains (raea's Tone effects and limiter registered as seedling nodes) in seedling's graph stepped beside the game, a chunk a frame into the core | `seedling/` (raea-seedling), `<Seedling>` in @raea/motion/seedling |
| TokenSynth | generation (offline) | a Sampler's root notes from a timbre prompt, made once and cached as PCM | rdb assets `tokensynth`, tools/tokensynth |
| DAC 44 kHz | generation and meaning | TokenSynth's codec; 9 x 86 Hz tokens as symbols | phase 4 and 5 |
| CLAP (htsat-unfused; music checkpoint) | meaning (and generation) | 512-d sound embeddings; TokenSynth's timbre | phase 5 (and 4) |
| Tone.js (the library) | none | its runtime needs Web Audio; its API is @raea/motion/tone | not used |

## Verdicts

Sources were opened on 2026-10-18: crates.io, the GitHub repositories, docs.rs and the crate sources, npm, PyPI, the
Hugging Face model cards, and arXiv. The Firewheel API was read from the 0.12.0 and 0.14.0 crate sources.

### Firewheel: shippable, and the one mixer core for everything

- **Facts.** Licence MIT OR Apache-2.0. Version 0.14.0, released 2026-09-05; 0.13.0 came out 2026-08-28 and 0.12.0 on
  2026-07-09. The last commit was 2026-09-05. Its README says it is meant to be upstreamed into Bevy as Bevy's audio
  engine (https://github.com/BillyDM/Firewheel, https://crates.io/crates/firewheel).
- **Sample-accurate scheduling.** It's behind the `scheduled_events` feature, which is off by default. The context
  call is `schedule_event_for(node, event, Some(EventInstant::AtClockSamples(..)))`. The processor splits each block at
  event times and calls the node's `events()` then `process()` for each sub-chunk
  (firewheel-graph `processor/event_scheduler.rs`). Our nodes therefore see every event at its exact sample.
- **Offline rendering.** There is no offline backend, and none is needed. `FirewheelContext::activate(ActivateInfo)`
  returns a `FirewheelProcessor`, and `processor.process(input, output, BackendProcessInfo)` can be pulled by hand into
  any buffer (the crate's own `clap_drop_ordering` test does this). The clock counts processed samples, not wall
  time, so the output is deterministic and runs as fast as it computes. Measured: 10 s of the tones fixture bounces
  in 23 ms, and 8 s of the music fixture in 29 ms.
- **Custom nodes.** An `AudioNode` trait (info and construct_processor) and an `AudioNodeProcessor` trait (events,
  process). Custom events are `NodeEventType::custom(T)`, dropped off the audio thread by Firewheel's collector. The
  voices node and the master node (our limiter) are about 350 lines (`engine/mixer/src/nodes.rs`).
- **CPAL backend.** firewheel-cpal 0.12 requires cpal 0.18. cpal 0.18 can't link in the same dependency graph as
  void's and react's cpal 0.15, because both link `alsa-sys` (Cargo fails to resolve). raea-mixer therefore leaves
  out Firewheel's backend and pulls the processor from its own cpal 0.15 callback (`engine/mixer/src/live.rs`, about 60
  lines). It asks for 48 kHz and 256-frame blocks. Firewheel's default is 1024 frames (21 ms).
- **Why 0.12 and not 0.14.** bevy_seedling 0.8, the Bevy 0.19 release, pins Firewheel 0.12. One Firewheel for raea
  and Bevy matters more than the two point releases.

### bevy_seedling: viable with work, Bevy 0.19 is supported

- **Facts.** Licence MIT OR Apache-2.0 on crates.io; GitHub's detector shows Apache-2.0 only. Version 0.8.0, released
  2026-08-09. Its README's table maps Bevy 0.19 to seedling 0.8 (and 0.18 to 0.7, 0.17 to 0.6). Firewheel is pinned
  at 0.12 with `default-features = false` (https://github.com/CorvusPrudens/bevy_seedling).
- **How it runs on the core (seedling/, raea-seedling, 2026-10-18).** Seedling's graph runs in a Bevy App beside
  the game, with no device of its own. raea's platform plugin activates seedling's Firewheel context and keeps the
  processor. Each frame, after seedling has flushed the frame's changes (its `Last` systems), the processor runs for
  exactly that frame's span of the timeline (round(f * 48000 / fps) to the next frame's). The chunk is a voice of the
  core (`ToneCue::pcm`) from the frame's sample, through the core's master to the device (cpal 0.15) or into the
  bounce, so seedling never needs a backend.
- **Why a render and a play hear the same samples.** Seedling's events reach its processor at the start of their
  frame's chunk. Its clock is the timeline's samples, its Time steps one frame per update
  (`TimeUpdateStrategy::ManualDuration`), it diffs every update (`DiffRate(0)`, where its default waits 10 ms of audio
  time), and its pools start at 32 samplers: a pool growing mid-game recompiles Firewheel's graph, a 40-100 ms frame.
  Samples are decoded whole at 48 kHz before their frame: a shared cache, filled at open on a thread from the edit's
  assets and media, where Bevy's asset server would land a load on whatever frame it lands on. The host is made when
  the game opens (made on frame 0, it took 50 ms and the frames after came too late).
- **Registered nodes.** `ToneFx` (any of raea's Tone effects by Tone's class name and options: Filter, Freeverb,
  Convolver, PitchShift, EQ3, ...) and `CoreLimiter` (the core's true-peak limiter), each wrapping the mixer's own
  processor (`FxNode::processor`, `MasterNode::processor`). Seedling's `SampleEffects` chains use them per player.
- **For TSX games.** `<Seedling src id volume speed loop fx>` (@raea/motion/seedling): a player while the element is on
  the page, started on the frame it first shows, stopped when it's gone. `src` is a media file or a generated sound
  (`sao(...)`) from the asset store. Rust can drive `raea_seedling::Host::app` directly.
- **Dependencies.** bevy_seedling 0.8.0 is vendored (`engine/seedling/vendor/bevy_seedling`, patched from react's and void's
  manifests) with its backends and hrtf removed, its wasm32 Firewheel dependency removed, and Firewheel's `tracing`
  feature off. Cargo's lockfile takes even a weak dependency's package (`firewheel-cpal?/tracing`) and every target's
  dependencies, so any of these brought cpal 0.18, whose alsa-sys can't link beside the cpal 0.15 react and void use.
  The changes are marked "raea:" in its Cargo.toml. react's `seedling` feature (on by default; void turns it on) is
  the way out.
- **What it measured** (sdk/react/bench/games/seedling: pong with every sound played by seedling, its room tone looping
  through raea's Freeverb as a seedling node):
  - Render against play's tap: difference 0.0 over 38.9 s, 0 late events, twice at load 6-9, and once at load 24-31.
    That is with `RAEA_AUDIO_HEAD_MS=70`. A chunk is a new voice every frame, so a frame stepped later than the head
    loses its start. The play loop's own first frames come 25-50 ms late, more under load. Pong's voices hide that,
    because nothing new starts on those frames; seedling's can't. At the default head (19.8 ms at 120 fps) the
    fixture differed only on frames stepped 20-51 ms late.
  - Its frames: render against play SSIM 1.000000 at every checked frame, with keys (replay.sh).
  - Cost at 120 fps: a frame's seedling update and process are about 1-2 ms, under 5 ms at worst (load 17).
  - Before mastering, seedling's default game graph sits about 12.5 dB under the same sounds as plain `<Audio>`
    (its buses' gains); the `<Mix>` master takes both to -14 LUFS.

### Tone.js: its API, yes, on the core (@raea/motion/tone); its Web Audio runtime, no

- **Facts.** tone: MIT, 15.1.22 stable (2025-04-27), with 15.5.57 on `next`; the repo was pushed 2026-10-17.
- **Why it can't run here.** It is coupled to Web Audio throughout. `TransportClass extends ToneWithContext` and
  builds a `Clock` on the context's `tick`, and `Clock` asserts the context is running. Even its offline rendering
  is an OfflineAudioContext (Tone/core/clock/Transport.ts, Clock.ts). JavaScriptCore here has no Web Audio, so none
  of it runs unmodified.
- **What does port: the whole API surface agents know** (see "Tone.js on the core" and its compatibility table).
  Instruments, effects, channels, the Transport and its schedulers compile to an edit's graph and events, which the
  core plays deterministically.
- **@tonejs/midi.** MIT, 2.0.28, released 2022-02-04, repo last pushed 2023-07-19. It is pure JS (midi-file plus
  array-flatten), uses `fetch` only in `fromUrl`, and writes MIDI with `toArray()`. It is dormant. A Standard MIDI File
  reader and writer that keeps its JSON shape (`tracks[].notes[]` with `midi`, `time`, `duration`, `velocity`, `ticks`;
  `header.tempos`, `timeSignatures`, `ppq`) is about 150 lines of TypeScript, so the music module has no dependency.

### TokenSynth: viable with work, as an offline stem renderer only

- **Facts.** Paper: arXiv 2502.08939 (ICASSP 2025), Kim, Koo, Lee, Joung and Lee (SNU). It is a 12-layer, 175M
  parameter decoder-only transformer over DAC 44 kHz tokens, conditioned on CLAP and MIDI tokens. Code (MIT) is at
  https://github.com/KyungsuKim42/tokensynth, last commit 2026-06-24; weights (MIT) are at
  https://huggingface.co/KyungsuKim/TokenSynth.
- **Why it's viable.**
  - It runs locally and deterministically on CPU (0.76 s per second of audio).
  - Its stems cache as PCM and play through the core exactly like any clip.
  - The licences allow it.
- **The work it needs.**
  - A 5 s window per generation (chunk and overlap-add).
  - Velocity in four steps.
  - Hot output that needs normalising.
  - No export (it's Python and torch today).
  - About 4.1 GB of fp32 weights, mostly the 2.35 GB CLAP music checkpoint.
  - A device in the cache key.
- **The verdict.** It never belongs in the live path; the paper itself says it can't run in real time. As the phase 4
  stem renderer it is a reasonable first model once it has an export and fp16 weights. The oscillator instruments stay
  the fallback, so MIDI always plays.

### DAC (Descript Audio Codec): viable for the meaning lane, unmaintained

- Code MIT and weights MIT ("Weights are released … under MIT license",
  https://github.com/descriptinc/descript-audio-codec).
- The 44 kHz model is 306.7 MB fp32, about 76.7M parameters. It encodes at 86.1 Hz with 9 codebooks of 1024 entries
  (hop 512, about 7.75 kbps; conf/final/44khz.yml). There are also 24 kHz and 16 kHz models.
- PyPI 1.0.0 (2023-07-20) is the last release, and there has been no code commit since. It works, but nobody is
  fixing it.
- It is TokenSynth's codec, and its tokens are a candidate symbol stream for the sandwich (9 x 86 Hz integers is
  exactly the kind of discrete code an HDC layer binds).

### CLAP (laion/clap-htsat-unfused): viable for the meaning lane, measured before

- Licence Apache-2.0 on the model card; the LAION-AI/CLAP repo is CC0-1.0. `pytorch_model.bin` is 614.5 MB, about
  153M parameters. The projection is 512-d, and the audio and text towers are 768 wide. transformers supports it
  (`ClapModel.get_audio_features`) (https://huggingface.co/laion/clap-htsat-unfused).
- It's already in this Mac's Hugging Face cache (589 MB on disk).
- perception.feature's H5 test (2026-10-15) found it useless at 1 s windows and right at 5 s on voice, music bed and
  mix. It hears a music bed as music where YAMNet hears noise.
- It is a frozen encoder for the meaning lane (sound-to-description, Foley search, a 512-d vector to binarize for
  the sandwich). TokenSynth conditions on a different CLAP checkpoint (`music_audioset_epoch_15_esc_90.14.pt`,
  2.35 GB), not this one.

## Tone.js on the core (@raea/motion/tone)

Tone.js's own API runs on the one mixer core, with no Web Audio: `import * as Tone from "@raea/motion/tone"`. Snippets
from Tone's docs run unchanged (sdk/motion/conformance/tone.test.tsx runs 18 of them).

**How it works:**
- **TS side (sdk/motion/src/tone.tsx).** Every Tone object registers in the edit's Tone graph (a node id, Tone's kind,
  its options with Tone's names and defaults filled in) and records its connections.
- **Triggers become events.** `triggerAttackRelease`, `start`/`stop` and Param ramps become events, each with its
  key, node, time on the timeline and what it does.
- **The Transport compiles; it doesn't play.** `schedule`, `scheduleRepeat`, `Loop`, `Part`, `Sequence` and `Pattern`
  add callbacks to a queue that runs in time order as far as the frames need, always in the same order whatever
  frames ask. So a render, which steps every frame, and a play, which may skip frames or seek, make the same events.
- **What a frame emits.** The registry renders `ToneOut` in every composition once the module is imported. Each frame
  it emits the graph (`<audio data-tone-graph>`, the same every frame) and the events whose time falls in the last
  three frames (`<audio data-tone-evs>`). Both are ordinary elements: they bake, replay and seek like any other.
- **Core side: the graph** (engine/mixer/src/tonegraph.rs). Each instrument plays from its own pair of the voices node's 32
  stereo outputs. Every other node (effects, Channel, Volume, Gain, Panner, buses, the Destination) is a ToneFx node
  (engine/mixer/src/fx.rs). Tone's connections are Firewheel edges; a ducked channel's key goes to its sidechain input.
  Instruments are mixer/src/tone.rs.
- **Core side: the events** (engine/mixer/src/score.rs `tone`). Each event fires once, by its key, on its own sample:
  `round(t * 48000)`, not the frame's sample. Parameter ramps are sample-exact. LFOs run on the timeline's time (from
  the origin the node is given), so they're the same live and offline and stay right after a seek.

**What's different, and why.** A composition is a function of its frame, so:
- Time 0 is frame 0. `Tone.now()` is 0 at the top level, and the event's time inside a callback.
- The graph is the one the edit has when its first frame renders. A node made later rebuilds the graph (a gap in the
  sound), so build instruments at the top level.
- A game's one-shots go through `<Tone.Sfx when play>`, which keys their events by the frame. A trigger in a render
  body would fire on every re-render.
- Sources are local files or generated assets: there is no network.

**Compatibility.** Covered means it behaves as Tone documents. Partial means it runs, with the difference named.
Missing means it isn't there.

| Area | Covered | Partial | Missing |
|---|---|---|---|
| Instruments | Synth, MonoSynth, FMSynth, AMSynth, MembraneSynth, NoiseSynth, MetalSynth, PluckSynth, PolySynth (maxPolyphony, voice stealing), Sampler (urls, baseUrl, release, attack, add/has), Player (start offset duration, stop, loop, loopStart/End, playbackRate, fadeIn/Out, autostart), Players (player(name), stopAll), Oscillator and Noise with start/stop | DuoSynth (as an FMSynth); MetalSynth (six square operators through a highpass, approximating Tone's); PluckSynth (Karplus-Strong, like Tone's but not its code); FM/AM modulation envelopes; oscillator `fat*` types (count, spread) covered, but `am*`/`fm*` prefixes fall back to the base wave and `partials` is ignored; Player `reverse` ignored; GrainPlayer is a Player; Oscillator and Noise start and stop with 3 ms fades | Envelope or Signal connected to a parameter; an LFO on an instrument's parameter (LFOs reach effects and channels only); custom partials; Sampler cross-fades |
| Methods | triggerAttack, triggerRelease, triggerAttackRelease (per-note durations), releaseAll, set/get, connect, disconnect, toDestination/toMaster, chain, fan, dispose | `set()` after the first frame sends numeric values as events; nested settings need the first frame | `sync()` to the Transport, which is a no-op because everything here is already on the timeline |
| Transport | bpm.value, timeSignature, swing and swingSubdivision, start(time, offset), stop, schedule, scheduleRepeat(interval, start, duration), scheduleOnce, clear, cancel, position, seconds, ticks, nextSubdivision, `loop`/`loopStart`/`loopEnd` and setLoopPoints (an event inside the window comes round every pass, one before it plays once), tempo automation (`bpm.rampTo`, `bpm.linearRampToValueAtTime`, `bpm.value =` later: the grid follows a tempo map, beats integrated over each linear segment) | an exponential bpm ramp is taken as linear; loop settings must be set before `start()`; Transport time strings are read at the starting tempo | `Tone.Draw.schedule` (accepted, never called: read the frame instead) |
| Scheduling | Loop (interval, iterations, probability: seeded), Part ([time, value] or {time}, loop, loopEnd, add/remove/at), Sequence (nested subdivisions, null rests, loops by default), Pattern (up, down, upDown, downUp, alternateUp/Down, random, randomOnce, randomWalk, all seeded) | `humanize` ignored; a Part's `loop` must be set before `start()` | Transport-relative events after a later Transport stop and start (the queue keeps running) |
| Time and units | numbers, "4n", "8t", "2n.", "1m", "1:2:3", "+0.5", "@4n", "192i", "4hz", "a + b"; Time().toSeconds/toTicks/toFrequency/toMilliseconds; Frequency/Midi (toFrequency, toMidi, toNote, transpose, harmonize); mtof, ftom, dbToGain, gainToDb, intervalToFrequencyRatio | `toBarsBeatsSixteenths` at the current tempo | |
| Mixing | Destination (volume, mute), Channel (volume, pan, mute, solo, send, receive), Volume (mute), Panner, Gain, Solo, PanVol | solo affects Channels and Solo nodes only | Merge, Split, Mono, Panner3D, Listener |
| Effects | Reverb, Freeverb, JCReverb, FeedbackDelay, PingPongDelay, Filter (all types, rolloff -12/-24/-48/-96, Q, gain), AutoFilter, Compressor (threshold, ratio, attack, release, knee), Limiter, EQ3 (Tone's three-band split: lowpass at lowFrequency, highpass then lowpass between, highpass at highFrequency, each band's gain), Distortion (Tone's curve), Chorus, BitCrusher, Tremolo, Vibrato, Phaser, AutoPanner, Convolver (an impulse response from a media file or a generated sound, Web Audio's normalisation, no latency: the first 512 taps direct, the rest by partitioned FFT), PitchShift (Tone's two sawtooth-swept delay taps, cross-faded; pitch, windowSize, delayTime, feedback), `wet` on each, `start()` for the LFO ones, a Tone.LFO connected to any effect or channel parameter (sine, square, triangle, sawtooth; min, max, phase, amplitude), exponential ramps (`exponentialRampTo`, `exponentialRampToValueAtTime`: the same ratio each sample) | Reverb is algorithmic (Freeverb's combs, each set for a 60 dB decay over `decay`) rather than Tone's noise-IR convolution, and `preDelay` is ignored; an exponential ramp from or to 0, or across a sign change, is linear; a Convolver reads at most 30 s of its response | FrequencyShifter, StereoWidener, MidSideCompressor, MultibandCompressor, Gate, Follower, Chebyshev, WaveShaper with a custom curve |
| Context | start(), loaded(), now(), immediate(), getContext/context (currentTime, sampleRate 48000), getTransport/Transport, getDestination/Destination/Master | | Offline (it throws, saying why: there is no audio engine in raea's JavaScript; render the composition, whose sound is the core's bounce), UserMedia, Recorder, ToneAudioBuffer editing, Emitter, rawContext, Web MIDI |
| Analysis | Meter, FFT, Waveform, Analyser pass sound through | they read as silence | |

**raea's additions:**
- `buses` (music, sfx, voice, ambience, ui), each a named Channel into the Destination. `<Audio>` and `<Video>`
  take `bus=`.
- `Channel.duck(key, {amount, threshold, attack, release})`: sidechain ducking.
- `<Sfx when play>`: frame-exact one-shots.
- `vary(instrument, {pitch, volume})`: seeded variation per event.
- Generated sources: `sao`, `lyria`, `fish` and `tokensynth(timbre, notes)`.
- `Time(...).toFrames(fps)`: cuts on bars and beats.
- `@raea/motion/music` (Part and Sequence on named instruments, MIDI in and out) is a shorthand. It plays through
  this layer: each instrument name is a Tone preset.

## Generated sources

A Sampler's `urls` and a Player's `url` take a media path, or a generated asset:
- `Tone.tokensynth(timbre, notes, {seconds, velocity, seed, precision})`: one asset per root note, by TokenSynth,
  local CPU. The Sampler pitches between the roots. This is how raea makes music.
- `Tone.sao(prompt, {seconds})`: foley, by Stable Audio Open Small on Core ML, local.
- `Tone.fish(text, {voice})`: narration over the picture, by Fish, paid.
- `Tone.lyria(prompt)`: a music bed, by Lyria, paid. Still wired; the guides no longer use it.
- A file from an open sample library in the media folder: `raea-motion sounds get PACK/INSTRUMENT` (below).

How a source is made and played:
- **Policy** (2026-10-19). Music is Tone.js instruments: TokenSynth notes, an open sample library's notes, or
  synths, in a Sampler playing a Part or Sequence. SAO for foley, Fish for non-diegetic voice. No other generation
  (no images, video or remote sound effects); Lyria stays in the code for older edits. Nothing in this work spent money: the fixtures' sounds come from SAO and TokenSynth (local) and the
  voice from macOS `say` at test time.
- **Resolution.** The graph's sources are resolved by react (audio.rs `tone_of`): media paths through the media
  folder, generated ones through the asset store (engine/rdb/src/assets.rs). A render makes an asset that isn't made yet;
  live play only finds it, and an asset never made is left out (silent, with a note on stderr).
- **Caching.** The store keys a make by everything that decides the bytes: the model, the prompt, the seed, and the
  params with sorted keys and whole numbers written as integers (a Tone graph's `seconds: 1` and an `<Audio>`'s
  `1.0` are one asset; lines indexed before 2026-10-19 under the f64's key are still found). For TokenSynth the
  params are midi, velocity, seconds, seed, device and precision, because CPU and MPS, and each precision, give
  different samples from the same seed. The bytes are kept under their sha256.
- **Normalising.** TokenSynth notes are normalised to -1 dBFS peak by tools/tokensynth/run.py, with a 5 ms fade at
  the end. Its raw output ran hot, peaking at 1.8 in the spike.
- **Playing.** Sources open whole when the graph is built (lib.rs `open_whole`, up to 4 minutes), so a Sampler's
  notes never wait on a block in the audio thread.
- **TokenSynth install.** tools/tokensynth/setup.sh installs into ~/.cache/raea-tokensynth: the pinned venv (torch
  and torchaudio 2.5.1, transformers 4.30.2, scipy 1.14.1), 3.8 GB of weights, the 1.0 GB fp16 runtime and a 1.1 GB
  venv. One fp16 note renders in about 5 s on CPU.
- **SAO install.** tools/sao/setup.sh installs into ~/.cache/raea-sao: the DiT (1.36 GB), VAE decoder (156 MB) and
  number embedder of mlboydaisuke/Stable-Audio-Open-Small-CoreML at a pinned revision, each checked by SHA-256;
  t5-base's tokenizer.json; raea's fp32 T5 encoder (419 MB; the published fp16 one overflows), converted once by
  t5_to_coreml.py in a throwaway uv environment with torch and t5-base's weights, which are deleted after; and a
  94 MB venv (numpy, coremltools, tokenizers). tools/sao/run.py is the 2026-10-15 spike's coreml_run.py without
  torch or stable-audio-tools: the same prompt and seed give byte-identical WAVs (checked 2026-10-19, and with a T5
  converted from scratch). A sound takes about 10 s, 2.2 GB peak.
- **Installed on first use.** rdb's `ensure_installed` runs a model's setup.sh the first time a make asks for it and
  nothing is installed, its progress on stderr; `RAEA_AUDIO_AUTOSETUP=0` makes it an error naming the command
  instead. `$RAEA_SAO_FROM` / `$RAEA_TOKENSYNTH_FROM` name an install to copy from (APFS clones), defaulting to the
  standard folders, so a second install on the same Mac costs neither download nor disk. An install from before
  tools/sao (the spike's coreml_run.py) is still served as it is.
- **From the command line.** `raea-motion sao "PROMPT" [--seconds 2] [--seed 1] -o OUT.wav` and `raea-motion
  tokensynth "TIMBRE" --notes C2,C3,C4 [--seconds 2] [--velocity 100] [--seed 1234] [--precision fp16] -o DIR/` make
  through the asset store beside `--dir`/edit.tsx (default the working folder) with the TypeScript functions'
  defaults, so a sound made there is the one the edit's call finds made. `raea-motion setup-audio [--check]`
  installs or checks both and prints their sizes. void's agent reaches them as `raea sao`, `raea tokensynth` and
  `raea sounds` (the host's `audio` op).
- **Open sample libraries.** `raea-motion sounds [WORDS]` searches sdk/motion/library/sounds/catalog.json
  (tools/sounds/catalog.py writes it): FluidR3 General MIDI (128 instruments, CC-BY 3.0), the Salamander grand
  piano (CC-BY 3.0), tonejs-instruments (20 instruments; public-domain and CC-BY sources) and the Versilian Community
  Sample Library (104 instruments and percussion, CC0). `sounds get PACK/INSTRUMENT [--notes ..]` downloads one take
  per note (the middle dynamic, the first round robin; a drum's first takes), caches it in ~/.cache/raea-sounds,
  copies it into media/sounds/PACK-INSTRUMENT/ with a LICENCE.txt, and prints the Sampler (or Players) that play it.
  Tone's own drum kits are CC BY-NC-SA and left out.

## Channels: buses, ducking and the master

- **Buses.** `Tone.buses.music`, `.sfx`, `.voice`, `.ambience` and `.ui` are Channels into the Destination, created
  on first use. Each has volume, pan, mute, solo, sends and receives. A sound's `bus=` sends it into that bus
  through its own voices port, so it takes the bus's effects and ducking.
- **Ducking.** `buses.music.duck(buses.voice, {amount: -12, threshold: -45, attack: 0.04, release: 0.4})` puts the
  voice bus's output on the music bus's sidechain. A level detector (5 ms attack, 100 ms release on the key) takes
  the channel down by `amount` while the key is over the threshold, with the attack and release times on the gain.
- **The master.** `<Mix lufs ceiling>` is measured on the core's own sound, Tone included (sdk/react/src/motion.rs
  `core_master`):
  1. bounce once without limiting;
  2. take the gain to the target loudness (media's `master_gained`);
  3. bounce again with that gain and the limiter at the ceiling;
  4. on Apple, lower the ceiling by any AAC overshoot and bounce again.

  A live play gets the same gain and ceiling from `raea-motion master-gain`, which runs the same function, and play
  waits for it before its clock starts (live.rs `wait_master`).
- **Games.**
  - `<Tone.Sfx when={...} play={(t, frame) => ...}>` fires on the exact frame `when` is true. Its events are keyed by
    the component and the frame, so a replay of the same input plays the same sound.
  - `vary()` seeds each event's pitch and volume jitter from its key.
  - An instrument's `maxPolyphony` caps its voices; past it, the core steals the oldest with a 5 ms fade.
  - Round-robin over a set of samples is chosen by the frame (`frame % n`), so it is the same every run.
- **Timelines.** Parts sit on the Transport, which starts at frame 0, and `Tone.Time("1m").toFrames(fps)` gives a bar
  in frames, so cuts land on downbeats. Beats found in a music bed (engine/rdb/src/beats.rs, used by the mix lints) aren't
  yet fed to the Transport. A bed that is itself Tone needs no beat tracking.

## What the spikes measured

All runs were on the M5 Pro on 2026-10-18. Timed and audio-timing runs held the shared measurement lock and ran
under `nice -n 15`. Load averages are given with each run.

### Render equals play (the Firewheel spike)

On the core (the default):
- a render writes the core's bounce before the encoder (`RAEA_AUDIO_BOUNCE=OUT.wav`);
- a play writes the core's output before the mute and the device (`RAEA_AUDIO_TAP=OUT.wav`), with a JSON beside it
  that records where the timeline sat on the device clock;
- `raea-motion ci bounce-vs-live` (sdk/react/src/ci.rs) aligns the two by that origin and compares every sample. It also searches
  ±50 ms for the offset where they agree best, as a check on the alignment.

| Piece | Compared | Max difference | Samples over 1e-6 | Best offset | Late events | Load (1 min) |
|---|---|---|---|---|---|---|
| pong (game, live, no input; its master and AAC-held ceiling) | 38.9 s | 0 | 0 | 0 | 0 | 6.65 |
| tones fixture (starts, a cut, an overlap, a gain ramp, stops) | 10.0 s | 0 | 0 | 0 | 0 | 6.65 |
| music fixture (@raea/motion/music: drums, bass, chords) | 8.0 s | 0 | 0 | 0 | 0 | 6.65 |
| k4: the 4K edit (3 cuts on two AAC takes), baked play | 19.0 s | 0 | 0 | 0 | 0 | 4.47 |
| TokenSynth stems as clips (44.1 kHz, resampled in the voices) plus synth drums | 8.0 s | 0 | 0 | 0 | 0 | 27.5 |

- So the play is the render's bounce, sample for sample, before the device. The mixer tests prove the same of the
  graph in isolation (`engine/mixer/src/offline.rs`). Played like a device, with an arbitrary origin, blocks of 1 to 700
  frames and events sent ahead by varying margins, it gives the bounce with a worst difference of 0.0.
- Getting there took six fixes, each found from the tap's report:
  1. The device clock has to run past CoreAudio's startup burst before the timeline is placed. Placed earlier, it
     was placed again 67 ms later and its voices restarted.
  2. The mixer opens before play's clock starts. Opened at the first sound, that frame's events were 75 to 190 ms
     late and skipped in.
  3. A head of two blocks, 5 ms and half a frame (32 ms at 30 fps, 36 ms at 25 fps). A cut's frame, which opens its
     clip, was stepped late, and its events arrived after their sample.
  4. Each sound's current block is decoded before its voice starts. At most 3.5 ms per load (k4, AAC); 2.3 to 10.7
     ms per run in total.
  5. Blocks are kept a second after their last use. Let go at once, a stopping voice read silence: a click 80 ms
     before each stop.
  6. The callback waits for the block lock instead of skipping the block. The lock is held only for a map insert or
     remove.
- **audio.sh with the switch on:**
  - pong played live is -14.9 LUFS against its render's -14.9 (0.0 LU apart). True peak is -2.9 dBTP live and -2.0
    in the AAC file, under the -1.5 ceiling.
  - tones: 0 clicks live, 0 rendered.
  - All three bounce-against-tap checks pass (load 6.65).
- **The 80 ms baked-play drift.** It wasn't a mixer problem. The play ran 1.3 s past the edit's last frame, and the
  held frame's `<Video>` kept saying the same source time, so the classic mixer put its voice back every frame
  (fixed in e5d5f04; `play --seconds end`). Inside the timeline, classic baked play drifts 0.05 ms at most. On the
  core, k4 baked to the end drifts 0.10 ms at most (p99 0.10, 474 frames) with 0 of 475 frames missed. Its sound is
  the render's bounce exactly. The two paths are now one path.
- **Speed.** The core bounces faster than real time: 10 s of tones in 23 ms, 8 s of the music loop in 29 ms, and 19 s
  of k4 in 3.2 s (most of it AAC decoding).

### The export (ts1, warm, under the lock, load 7-10)

On the one-mixer switch, ts1's export grew a 3.09 s sound step after its last frame. All of it was one Firewheel
context dropping before its processor: the context waited its 3 s timeout for a processor nobody was running. The
offline engine now drops the processor first.

The bounce is also streamed and mastered in one pass:
- A fed render sends each frame's cue to a bounce thread as it draws the frame (`offline::Streamer`), so the sound is
  ready when the last frame is.
- The master is the live master node's limiter run over the raw bounce (`offline::master_raw`), equal sample for
  sample to bouncing through it (tested). So mastering and the AAC-held retries are passes over a buffer, not more
  bounces.

| | wall (s) | sound after the last frame (s) |
|---|---|---|
| classic (before the switch) | 2.66, 2.72 (one run 4.08) | 0.105-0.110 |
| core, before the fix | — | 3.09 (a loaded machine) |
| core, after | 2.58, 2.64 | 0.025-0.029 |

Decoding a 4 s AAC block of a ts1 take costs 3.5 ms and a probe 0.3 ms, so a decoded-PCM cache wasn't needed.
Reusing the cached mastered track in a render wasn't either: with the bounce streamed, sound is no longer on the
export's critical path.

### Tone on the core (sdk/react/bench/tone/check.sh, under the lock, load 6.7)

| Piece | What it exercises | Tap against bounce | Loudness, true peak |
|---|---|---|---|
| song (16 s timeline, cut on the bar) | Sequence, Loop, Part (looped), Pattern; MembraneSynth, MetalSynth, NoiseSynth, MonoSynth, PolySynth of fat saws, Synth; FeedbackDelay into Reverb, a Compressor bus, the music bus; `<Mix>` | difference 0 over 16.0 s, baked play | -14.2 LUFS, -1.3 dBTP (-1.0 in the AAC file) |
| game (10 s) | a state machine's landings as `<Sfx>` one-shots (seeded variation, 4 voices), an SAO coin in a Sampler (round robin), a pad and a pluck delay on the music bus, a voice line on the voice bus, ducking | difference 0 over 10.0 s, live play | -14.0 LUFS, -1.8 dBTP |
| gaps (8 s, 2026-10-18, under load 42-62) | an LFO on a filter's frequency, an exponential sweep into a PitchShift a fifth up, the Transport looping a bar while its tempo ramps from 90 to 140 bpm, a Convolver on a generated 1.2 s impulse response, Tone's EQ3 | difference 3e-8 over 8.0 s (best lag 0, 0 late events), live play | -14.1 LUFS, -1.3 dBTP |
| TokenSynth Sampler (11 s) | two TokenSynth roots (C2, C3) made locally and pitched between | difference 0 over 11.0 s, baked play | -14.0 LUFS, -5.2 dBTP, 0 clicks |
| ducking (the game's music bus) | `buses.music.duck(buses.voice, {amount: -12})` | the music bus under the voice: -11.2 dB on average (it lets go in the pauses); before the voice 0.00 dB | |

- The gaps piece, each part switched off in turn against the whole (RMS of the difference, 0.24 for the tempo ramp,
  0.035 for the Convolver's wet, 0.07-0.10 for the rest) changes the sound, and a second render is identical. Its
  conformance snippets (sdk/motion/conformance/tone.test.tsx) run Tone's doc examples unchanged: an LFO on a filter's
  frequency, exponential ramps, the Transport looping a measure, a Loop inside setLoopPoints, `bpm.rampTo` (6 beats
  in the 4 s ramp from 60 to 120 bpm), a Convolver, PitchShift and EQ3, and Offline's error. The Convolver's
  partitioned convolution equals the direct sum within 1e-3 (mixer test).
- How the ducking was measured: on three versions of the game, without `<Mix>` and with the sfx bus muted. The
  music under the voice is A - C + D, with A as written, C with ducking off, and D with ducking off and no voice.
- The click counts in the song and game are the instruments' own attacks (a pluck's noise burst, hats), the same in
  the render and the play.
- The Tone graph played like a device equals its bounce in the mixer's tests too: a PolySynth through a ramped
  Filter and a Reverb, a ducked bus, a MembraneSynth with variation through an AutoFilter, worst difference 0.
- Two bugs found on the way made a play differ from its render, and are fixed:
  - An effect node's coefficients were recomputed on its block's grid, not the timeline's.
  - Its detectors ran on the silence before the timeline's 0. They now don't move before time 0.

### The music API

- @raea/motion/music has Transport, Part, Sequence and `<Music>`, plus MIDI out and in.
- `sdk/motion/conformance/music.test.tsx` passes 5 tests: Tone's time notation at 120 bpm, a Sequence's nested steps and
  loop, a Part's chords and loop, `<Music>` showing a note from the frame its start falls in until it ends, and MIDI
  out and back in keeping every note's time, length, pitch and velocity.
- The fixture's MIDI round trip (sdk/react/bench/games/music) has 92 notes: 36 hats, 18 bass, 18 pad, 12 kick, 8 snare.
- Live play reports `durationInFrames` as 1,000,000,000 (a live composition runs open-ended). `<Music>` therefore
  computes only the notes around its frame, visiting only the loop passes that reach it. Compiling every note of a
  looped part made play time out on frame 0.
- The instruments (`mixer/src/synth.rs`) are polyBLEP oscillators, ADSR, one-pole filters and seeded-noise drums. They
  are deterministic: the same notes give the same samples. The music bounce is -12.8 LUFS with a true peak of -1.1
  dBTP. Its 136 "clicks" are the bass saw's corner each cycle (A1, 18.2 ms apart); that is an instrument edge, not a
  mixer click.
- The tones fixture, which is pure sines, is the click gate. It shows 0.

### TokenSynth (run locally)

- **Input.**
  - MIDI: polyphonic, treated as one instrument. It reads notes in start order and stops at the first note that
    starts or ends at 5.0 s or later, silently dropping everything after (4 tokens a note, 10 ms grid, 450 steps,
    1024-token context). Longer parts must be rendered in chunks under 5 s and overlap-added; the fixture's 8 s parts
    were two 4 s chunks.
  - Velocity: snapped to 25, 50, 100 or 127.
  - Timbre: a 512-d LAION-CLAP embedding, from text or from a reference clip (they can be blended). Text guidance
    needs the unconditional model too.
- **Speed** (wall-clock seconds per second of audio, generation only):
  - CPU: 0.76 (load 4.7 to 7.0); about 1.3 under load 34 to 39.
  - MPS: 4.2 to 5.5, even at load 7. The per-token loop is the likely cause (not profiled).
  - DAC decode runs on CPU only (0.25 to 0.41 s/s). On MPS, a clip over about 1 to 4 s fails with "Output channels
    > 65536 not supported".
  - Model load: 3.2 s on CPU. A whole job for both 9 s stems took 27.5 s.
  - So a 30 s part costs about 23 s of generation plus 8 s of decode on CPU at low load, rendered as eight 4 s chunks.
- **Peak memory.** 5.8 GB RSS on CPU; 7.2 GB RSS and 10.6 GB footprint on MPS. Both are under memguard's 24 GB.
- **Determinism.** The same seed gives bit-identical tokens and samples on the same device (3 MPS and 2 CPU repeats).
  CPU and MPS differ from the first frame. So the cache key must include the device. The spike's key, sha256 of
  {notes, timbre, seed, model}, doesn't yet. It should also include the sampling settings and chunking.
- **Cache.** A second run with the same inputs hit both entries in 0.03 s. The stems play as `<Audio>` clips from the
  cache files, through the core, and the tap equals the bounce exactly (table above).
- **The model's own quirks.**
  - The chords stem comes out hot: peak 1.80, RMS about 0.5. It needs normalising before it mixes; the fixture plays
    it at 0.35.
  - The reference code feeds the unconditional model an always-empty slot after the first guided step (`tokens[:,
    -2:-1]`). Left as is, because it changes the README's behaviour.
  - Its quickstart writes 16 kHz. The spike decoded DAC at its own 44.1 kHz.
- **Core ML.** There is no export of any kind. By reading only: the transformer is a plain GPT and looks exportable
  once its Python-side KV cache becomes explicit inputs. DAC is convs with Snake activations and should convert
  after removing weight_norm. CLAP's HTSAT uses bicubic upsampling, which MPS lacks and Core ML probably lacks too.
- **Size.** These are the files a run needs:

  | File | Size |
  |---|---|
  | `token_synth_aug.pt` (175M parameters) | 712.7 MB |
  | `token_synth_unconditional.pt` (needed for text timbres) | 712.7 MB |
  | `dac_weights_44khz_8kbps_0.0.1.pt` | 306.7 MB |
  | `clap_music_audioset_epoch_15_esc_90.14.pt` | 2,352.5 MB |
  | roberta-base, bert-base-uncased, bart-base (pulled quietly by laion-clap) | 479 MB |
  | the venv | 1.1 GB |

  That is about 4.1 GB of weights in fp32. fp16 would halve it. Caching text-prompt embeddings would drop the CLAP
  text tower from the package, but not the audio tower, which reference timbres need.
- **Licences.** TokenSynth's code and weights are MIT (https://github.com/KyungsuKim42/tokensynth,
  https://huggingface.co/KyungsuKim/TokenSynth). DAC is MIT. laion-clap's metadata contradicts itself (CC0 licence
  text, Apache classifier), and its checkpoint was trained partly on AudioSet, which is YouTube-sourced.
- The spike's scratch use peaked at 6.6 GB. The weights and venv were deleted afterwards.

### TokenSynth smaller and faster (2026-10-18; timings under the lock, load 12.8-13.4)

`tools/tokensynth/run.py --precision fp32|fp16|bf16|int8|coreml`, with a smaller runtime that `convert.py` writes
into the install (`weights-fp16/`: both transformers, DAC's quantizer and decoder, CLAP's text tower alone; `coreml/`:
DAC's decoder and the transformer step). setup.sh runs it. TSX's `tokensynth()` and the CLI ask for fp16, and
since 2026-10-19 rdb's asset store also treats a note with no `precision` as fp16, run.py's default (it was fp32;
nothing writes a note without a precision any more).

| Variant | Weights | Load | Generation (s per s of audio: 1.5 s note, 3 s note) | Per note | Peak RSS | Same top token as fp32, fed fp32's tokens | KL to fp32 |
|---|---|---|---|---|---|---|---|
| fp32 (before) | 4.08 GB | 5.1 s | 2.09, 1.35 | 11.9 s | 5.8 GB | | |
| fp16 (the default) | 1.01 GB | 1.3 s | 0.88, 0.88 | 4.3-5.6 s | 3.4-3.8 GB | 99.9% | 1.3e-5 |
| bf16 | 1.01 GB | 1.4 s | 0.94, 0.90 | 4.6-5.8 s | 3.4-3.9 GB | 99.2% | 7.6e-4 |
| int8 (dynamic, qnnpack) | 1.01 GB | 1.6 s | 2.49, 2.37 | 9.4-12.6 s | 3.8-4.2 GB | 90.2% | 0.10 |
| coreml (GPU) | 1.01 GB + 516 MB | 2.5 s | 0.95, 0.86 | 5.8-6.9 s | 2.8 GB | 99.9% | 2.0e-5 |

- Most of the speed is not precision. TokenSynth almost never emits its end token, so it always ran 450 steps; it
  now stops once the frames cover the note and its release (on the same tokens: CLAP 1.000). CLAP's 2.35 GB file
  is replaced by its 250 MB text tower, and laion_clap's import is skipped.
- Quality against fp32. Sampling diverges within the first frame at any precision (even fp32 compute on fp16-rounded
  weights), so a free-running note at fp16 is another sample of fp32, not a degraded copy. Its CLAP cosine to fp32's
  note is 0.919, while fp32 against itself with another seed gives 0.73-0.77. The precision is therefore judged on
  fp32's own tokens (the table's last two columns) and on the decoders. DAC with fp16-stored weights, fed fp32's
  tokens, gives CLAP 1.000.
- What didn't work: DAC's decoder in fp16 or bf16 convolution on the CPU (over 3 minutes a note: it stays fp32);
  MPS (DAC fails with more than 65536 output channels, and the transformer step was 60 ms against 12 on the CPU);
  int8 (slower and less accurate); the Core ML DAC in fp16 (its noise floor shows in the quiet tail, CLAP 0.82, so it
  computes in fp32); Core ML's per-process load cost eats its decoding win. ONNX wasn't tried: Core ML covered the
  same ground.
- Fixed in passing: both models' KV caches are cleared before each note (upstream carried the unconditional model's
  cache from one note into the next), and the attention mask is the checkpoint's lower-triangular one.

### DAC and CLAP as encoders (10 s clip, torch 2.5.1)

| Encoder | Parameters | Output | CPU | MPS |
|---|---|---|---|---|
| DAC 44 kHz 8 kbps | 76.65M (encoder and quantizer 22.5M, decoder 54.1M) | 9 codebooks x 862 frames (86.1/s) | encode 0.128 s/s, decode 0.233 s/s (load 26.5) | fails over about 1-4 s (channel limit); a 1 s clip encodes at 0.041-0.053 s/s and decodes at 0.080-0.087 s/s |
| CLAP laion/clap-htsat-unfused | 153.5M (audio tower and projection 28.2M) | 512-d | 0.348 s a clip (load 26.5) | 0.133 s a clip, only with `PYTORCH_ENABLE_MPS_FALLBACK=1` (no bicubic upsample on MPS) |

- The load average was 18 to 48 during these runs, so treat the timings as upper bounds.
- The CPU encoder run peaked at 5.66 GB RSS.
- For the meaning lane, either encoder is far faster than real time on CPU. Neither should sit on the live path, and
  neither needs to.

## The classic mixer, retired

The classic mixer (sdk/react/src/audio.rs `Engine::Classic`: a cpal callback of its own with its own voices and limiter,
behind `RAEA_AUDIO_ENGINE=classic` and `raea_mixer::engine_is_firewheel()`) is gone, on Shane's call before CI's
14-night audio streak was reached. The core was already the default for everything. With it went the switch, the
classic branches in react's motion.rs, live.rs and play.rs and in rdb's `sound_tl`, rdb's `FedSound::Rows` fallback
(`render_fed`'s sound is now the core's mastered bounce or nothing), the `audio:streak` CI gate and its script.
The core's master node keeps its own copy of the limiter (engine/mixer/src/nodes.rs `Limiter`, with its tests). The caches
keyed by engine keep their `fw{version}-{build}` prefix, which still versions the core's master.

Still on media's `mix`: rdb's `stems` export and mixline.rs mix lanes with `lanes_tl` and `mastered`. They aren't a
second live mixer, but porting them to `cues_tl` and the core's bounce (a stem is one lane's cues bounced alone) would
leave media's `mix` with nothing to do for output.

## Recommended path

**Done (2026-10-18):**
- **Phase 1, games:** the core plays and renders games (audio.sh and replay.sh pass on it).
- **Phase 2, timelines:**
  - It plays and renders baked and live timelines too, with their mastered track, as well as 3D scenes, decks and
    the page-file and 3D-file renders.
  - The mastered track is the core's own bounce and passes the master untouched; every other play gets the
    render's master.
  - void's stage opens the mixer's device before its clock starts.
- **Tone.js on the core** (@raea/motion/tone): the API, buses, ducking, game one-shots and generated sources.

**Next:**
- **rdb's own renders** take the core (ae5e3a1: `sound_tl` -> `sound_core`). rdb's stems and mix-line exports
  (`stems`, mixline.rs) still mix its lanes with media's `mix`; port them to `cues_tl` and the core (above, "The
  classic mixer, retired").
- **Bevy:** seedling runs on the core (above). Still to do:
  - Spatial sound for 3D games, from seedling's spatial pools fed the 3D scene's listener and emitters.
  - A chunk path that survives a late frame without a bigger head: a lead of a frame for seedling's chunks, the same
    in the render.
  - cpal 0.18 everywhere when convenient, so Firewheel's own backend can replace the 60-line callback.
- **Tone:**
  - Envelopes and Signals connected to parameters, and LFOs on instruments' parameters (LFOs on effects and channels,
    tempo automation, Transport loops, exponential ramps, Convolver, PitchShift and Tone's EQ3 are done: below).
  - A convolution Reverb, if the algorithmic one isn't enough.
  - Feed engine/rdb/src/beats.rs's beats from a music bed to the Transport, for cuts on a generated bed's beats.
- **TokenSynth:** a Core ML or ONNX export, and fp16 weights (about 2 GB fewer). Render a Sampler's roots in one
  process; each note is a process today, mostly model loading.
- **Meaning lane:** CLAP embeddings (512-d, 5 s windows or longer) and DAC tokens into the HDC sandwich, after their
  exports.
