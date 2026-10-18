# meaning

`raea-meaning`: what footage shows, as columns. Each frame of a video (or N a second) is embedded once with SigLIP 2
and stored as a column of 768 floats (`COL.f32` plus `COL.json`). Everything after that is K kernels over the column,
with no model call:

    raea-meaning ingest VIDEO -o COL [--fps N] [--int8 | --coreml]
    raea-meaning dist COL                 distance from each frame to the one before
    raea-meaning search COL "text"        the frames most like a phrase
    raea-meaning find LIBDIR "text"       the best frame in each column of a footage library

- `embed/` (`raea-embed`): SigLIP 2 on ONNX Runtime, the same vectors on the Mac, the iPhone and a server. Models are
  read from `RAEA_MODELS`.
- `src/k.rs`: the kernels, also compiled into [`rdb/`](../rdb).
- `ios-hdc/`: a probe that times the hypervector kernels on an iPhone.

Depends on [`perception/`](../perception) for Core ML on Apple devices.

Status: working; experimental.
