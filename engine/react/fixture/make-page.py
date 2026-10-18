# make-page.py N OUT [BUNDLE] — a self-contained page: board CSS, task data, a React bundle inline
# (default dist/client.js).
import json, sys
n, out = int(sys.argv[1]), sys.argv[2]
bundle = sys.argv[3] if len(sys.argv) > 3 else "dist/client.js"
S, P, T = ["Todo", "Doing", "Review", "Done"], ["Ana", "Ben", "Chi", "Dev", "Eli"], ["bug", "design", "infra"]
W = ["Ship onboarding", "Fix sync bug", "Draft brief", "Review copy", "Set up backups"]
tasks = [{"id": i, "title": f"Task {i}: {W[i % 5]}", "status": S[(i * 7 + i // 3) % 4], "assignee": P[i % 5],
          "due": f"Oct {1 + i % 28}", "tags": [t for k, t in enumerate(T) if (i >> k) & 1]} for i in range(n)]
css, js = open("board.css").read(), open(bundle).read().replace("</script", "<\\/script")
open(out, "w").write(f"""<!doctype html><html><head><meta charset="utf-8"><style>{css}</style></head>
<body><div id="root"></div><script>window.__TASKS__ = {json.dumps(tasks)};</script><script>{js}</script></body></html>""")
