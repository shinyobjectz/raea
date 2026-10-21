# perception

Reading footage the way an editor does, before it is used: shots and how each begins, framing, faces, the subject,
on-screen text, speech, music and silence, camera motion, and the person's performance over time. It writes a fact
sheet (`facts.json`) beside the footage, and on request mattes and object tracks.

- `Perception.swift`: the original implementation on Apple's Vision and Core ML. It stays the reference.
- `src/` (`raea-perception`): the Rust port, module by module, each checked against the Swift. It runs models through
  Core ML on Apple devices and ONNX Runtime elsewhere. Binaries: `raea-perceive` and `raea-focus`.
- `seg/`: a small overlay segmenter trained in Rust (burn) that marks which pixels of a rendered frame are overlays.

[`ai/meaning/`](../meaning) depends on it. Status: in progress; not every part of the Swift is ported.
