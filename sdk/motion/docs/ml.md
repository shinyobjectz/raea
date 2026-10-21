# Models in compositions and games (@raea/motion/ml)

A composition or a game can run an ONNX model on the engine's ONNX Runtime (the `raea-ml` crate, `ml/`). The model is
loaded once per process and shared by every caller; a game may run it inside its step and fold the result into its
state, because a CPU run gives the same outputs for the same inputs every time. Speech (`@raea/motion/speech`, see
[speech.md](speech.md)) is built on the same runtime.

## Adding a model to a game

1. **Put the model in the composition's media folder** (`media/policy.onnx`), or in raea's model cache
   (`~/.cache/raea-void/models/<name>/<file>.onnx`, `RAEA_VOID_MODELS` moves it), which void's voice models share.
2. **Load it where it's used.** `loadModel` returns the same `Model` for the same source and options, so calling it
   from a step or a component every frame costs a map lookup:

   ```tsx
   import { loadModel, tensor } from "@raea/motion/ml";

   function chase(s: S, target: [number, number]): S {
     const net = loadModel(staticFile("policy.onnx"));
     const obs = tensor([(target[0] - s.pos[0]) / W, (target[1] - s.pos[1]) / H, s.vel[0] / 10, s.vel[1] / 10], [1, 4]);
     const a = net.run({ obs }).action.data as Float32Array;
     // ... fold a[0], a[1] into the state
   }
   ```

   Sources: `staticFile("x.onnx")` or `"/media/x.onnx"` (the media folder), `"cache:name/file.onnx"` (the model cache),
   or an absolute path.
3. **Check its inputs** with `net.inputs` / `net.outputs` (`{ name, type, shape }`, -1 for a dynamic dimension), and
   build tensors with `tensor(values, shape, type?)`; plain numbers become float32. `numbers(t)` reads any tensor
   (int64 included) back as plain numbers.
4. **Heavy models: ask ahead.** `net.run` waits for the run. For a model that takes longer than a frame, start it
   with `const job = net.request(feeds)` and join it a few frames later with `job.result()`. Keep the job's id (a
   string) in the state; `new Job(id).result()` works from any frame.
5. **Check it plays as it renders**: log a play's input and render the logged play (`sdk/react/bench/ml/check.sh` does this
   for the chaser, pixel for pixel).

The worked example is `sdk/react/bench/ml/chaser/`: a 487-byte two-layer policy net (`tools/make_policy.py` writes the
ONNX protobuf directly, with no `onnx` package) steering a dot after the arrow-key target ("chaser", a game), and the
same net chasing a target on a fixed path, one model run per frame ("orbit", a video, `orbit.tsx`).

## What is deterministic and what isn't

| Call | Pure (may go into game state) | Why |
|---|---|---|
| `Model.run(feeds)` on CPU | yes | the same inputs give the same bits on the same build |
| `Job.result()` | yes | a job is named by its model and inputs; joining it waits, so the answer is the same on whatever frame it's joined |
| `Job.poll()` | **no**, display only | *when* a job finishes depends on the machine |
| any call on a Core ML model (`provider: "coreml"`) | **no**, display only | Core ML isn't bit-exact across runs or machines; `model.deterministic` is false |
| `loadedModels()` | **no** | process statistics |

A Core ML model is fine for effects that don't feed back into the game (a style filter on the picture, a level meter).
Note that on the M5 Pro, SigLIP gives wrong answers on Core ML's GPU path; check a model's outputs against the CPU
before trusting Core ML for it.

## Options and lifetime

`loadModel(src, { provider, threads, optimization })`: `provider` is `"cpu"` (the default) or `"coreml"`; `threads` is
the intra-op thread count (default: the machine's cores, at most 4); `optimization` is ONNX Runtime's graph
optimisation level 0-3 (default 3).

The engine keeps one session per model file and options per process. `model.release()` lets a hold go; the session is
freed 30 s after its last release (`release(true)`: at once), and `loadModel` loads it again after that.

## Under the hood

- `ai/ml/src/lib.rs`: the session builder (`raea_ml::session`, shared with void's voice), the registry (holds, release
  timer), seeded jobs on a worker thread, tensor conversion.
- `ai/ml/src/models.rs`: the model cache and its Hugging Face downloads (`ensure`).
- `sdk/react/src/ml.rs`: the `ml.*` native calls (`ml.load`, `ml.run`, `ml.request`, `ml.result`, `ml.poll`,
  `ml.release`, `ml.loaded`). Tensors cross as `{type, shape, data}` with `data` the base64 of the little-endian bytes.
- `sdk/motion/src/ml.ts`: the SDK.
- `ai/ml/tests/runtime.rs`: the conformance test (the chaser's policy run twice to the same bits; Silero VAD when it's in
  the model cache).

raea-react's `ml` feature (on by default) carries all of this; void builds raea-react without it, and its voice uses
raea-ml directly.

## A small language model, later

The pieces for a local language model writing lines at run time: sessions and seeded jobs exist. Still missing are a
tokenizer call for TypeScript (Rust already has `tokenizers`) and a decoding loop that keeps its KV cache between
steps. Run it as a job (`request`, then `result` a few frames later) with fixed sampling and it stays deterministic on
CPU. Hand the narrator each sentence as it's finished, so speech starts before the whole reply is written.
