# brief

`raea-brief`: the `.brief` format and its tools. A `.brief` is a plain-text plan for one video, written in a
Fountain-style screenplay form with one reading for every line: scenes, speakers and the exact words of a take, open
beats to be found in the footage, parts timed to lines (captions, titles, end cards), variables, and checks the
finished video must pass. [format.md](format.md) is the reference.

Binaries:

- `raea-brief`: `parse` a brief into its intent graph (JSON), `fmt` it, `convert` a Fountain script into it,
  `compile` it to an `@raea/motion` composition (through [`ai/score/`](../../ai/score)), and `check` the render against its
  rules (through [`sdk/data/`](../data)). `ink` and `lower` turn a brief into an ink story and back.
- `raea-beats`: split a free-text brief into beats with a model, and check that every beat quotes the brief and every
  line of it is used or set aside.

Status: in use by void's agent; the format is still changing.
