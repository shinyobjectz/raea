"""score: the old entry point, kept so existing calls work. It is scorec.py (the split compiler: front end, intermediate
form, sheet, backends); void's score tool calls scorec.py directly since 0d5de36. The bench's own compiler
(bench/stage2.py) stays as it was, for the ablation's record.

    uv run -q python ~/raea/ai/score/score.py SPACE [SCRIPT] [--o EDIT] [--no-sound]
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import scorec  # noqa: E402

if __name__ == "__main__":
    sys.exit(scorec.main())
