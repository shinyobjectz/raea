# score

The compiler from a `.brief` to an `@raea/motion` composition, in Python. `raea-brief compile` runs it after parsing
the brief.

- `scorec.py`: the entry point. It reads the brief's intent graph, places it on the project's takes, times the script
  against them, and writes the composition and an exposure sheet (what is timed where, which checks hold, what would
  bring the piece to length).
- `compiler/`: the stages: `front.py` (graph to script), `ir.py` (the target-free script and timeline),
  `moments.py` (open beats bound to footage by perception's facts and SigLIP scores), `sheet.py`, and `sdk.py` (the
  `@raea/motion` backend).
- `cover.py`: checks that the beats several pieces share are each covered by one of them.
- `score.py`: an older entry point kept for existing callers; it runs `scorec.py`.

Run with `uv run python ai/score/scorec.py SPACE [NAME.brief]`. The format is [`sdk/brief/format.md`](../../sdk/brief/format.md).

Status: in use by void's agent.
