"""Build the cat logo kit from a traced bitmap (grid.json, 1 = black cell). Deterministic; no image generation."""
import json, math, sys, os
G = json.load(open(sys.argv[1])); OUT = sys.argv[2]
H, W = len(G), len(G[0])

def small_grid(G):
    # 2x2 majority (>=2 of 4), columns paired (2k-1, 2k), then the eyes punched back where they fall
    P = [[0] + r + [0] * (32 - len(r) - 1) for r in G]
    D = [[1 if sum(P[2*j+a][2*i+b] for a in (0, 1) for b in (0, 1)) >= 2 else 0 for i in range(16)] for j in range(16)]
    for x in (9, 12):
        for y in (5, 6): D[y][x] = 0
    cols = [i for i in range(16) if any(D[j][i] for j in range(16))]
    return [r[cols[0]:cols[-1] + 1] for r in D]

def trace(G):
    """Boundary loops on cell corners, filled cells kept on the right (clockwise outer, counter-clockwise holes)."""
    h, w = len(G), len(G[0])
    f = lambda x, y: 0 <= x < w and 0 <= y < h and G[y][x]
    edges = {}
    for y in range(h):
        for x in range(w):
            if not f(x, y): continue
            if not f(x, y-1): edges.setdefault((x, y), []).append((x+1, y))
            if not f(x+1, y): edges.setdefault((x+1, y), []).append((x+1, y+1))
            if not f(x, y+1): edges.setdefault((x+1, y+1), []).append((x, y+1))
            if not f(x-1, y): edges.setdefault((x, y+1), []).append((x, y))
    loops = []
    while edges:
        start = next(iter(edges)); loop = [start]; cur = start; prev_d = None
        while True:
            outs = edges[cur]
            if len(outs) > 1 and prev_d:  # saddle: turn right, keeping diagonal cells apart
                rd = (-prev_d[1], prev_d[0])
                nxt = next((o for o in outs if (o[0]-cur[0], o[1]-cur[1]) == rd), outs[0])
            else: nxt = outs[0]
            outs.remove(nxt)
            if not outs: del edges[cur]
            prev_d = (nxt[0]-cur[0], nxt[1]-cur[1]); cur = nxt
            if cur == start: break
            loop.append(cur)
        # keep corners only
        n = len(loop)
        loops.append([p for i, p in enumerate(loop)
                      if (loop[i-1][0] == p[0]) != (p[0] == loop[(i+1) % n][0])])
    return loops

def fmt(v):
    s = f"{v:.3f}".rstrip('0').rstrip('.'); return s if s != '-0' else '0'

def path_d(loops, ox=0, oy=0, k=1):
    out = []
    for L in loops:
        x0, y0 = L[0]; s = f"M{fmt(ox+x0*k)} {fmt(oy+y0*k)}"; px, py = x0, y0
        for (x, y) in L[1:]:
            s += f"h{fmt((x-px)*k)}" if y == py else f"v{fmt((y-py)*k)}"; px, py = x, y
        out.append(s + "z")
    return "".join(out)

def simplify(pts, eps):
    """Douglas-Peucker on a closed loop."""
    def dp(p):
        if len(p) < 3: return p
        (x1, y1), (x2, y2) = p[0], p[-1]
        L = math.hypot(x2-x1, y2-y1) or 1e-9
        d = [abs((x2-x1)*(y1-y)-(x1-x)*(y2-y1))/L for (x, y) in p[1:-1]]
        i = max(range(len(d)), key=d.__getitem__)
        if d[i] > eps: return dp(p[:i+2])[:-1] + dp(p[i+1:])
        return [p[0], p[-1]]
    # split at the two farthest-apart points
    a = 0; b = max(range(len(pts)), key=lambda i: math.dist(pts[0], pts[i]))
    return dp(pts[a:b+1])[:-1] + dp(pts[b:] + [pts[0]])[:-1]

def area(L): return sum(L[i][0]*L[(i+1) % len(L)][1] - L[(i+1) % len(L)][0]*L[i][1] for i in range(len(L))) / 2

loops = trace(G)
outer = max(loops, key=lambda L: abs(area(L)))
holes = [L for L in loops if L is not outer]
S_G = small_grid(G); s_loops = trace(S_G)
json.dump(S_G, open(os.path.join(os.path.dirname(sys.argv[1]), 'grid-small.json'), 'w'))

HDR = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" width="{w}" height="{h}"{extra}>'
def svg(vb, w, h, body, extra=''):
    return HDR.format(vb=vb, w=w, h=h, extra=extra) + body + '</svg>\n'
def write(name, text):
    # the cat SVGs go in OUT; the icon tiles they're rendered from go in OUT/src
    d = OUT if name.startswith('cat-') else os.path.join(OUT, 'src')
    open(os.path.join(d, name), 'w').write(text)

D = path_d(loops, 3, 2)            # 36-cell square, cat at (3,2): 3 left / 4 right, 2 top / 2 bottom
CRISP = ' shape-rendering="crispEdges"'
write('cat-black.svg', svg('0 0 36 36', 360, 360, f'<path fill="#000" fill-rule="evenodd" d="{D}"/>', CRISP))
write('cat-white.svg', svg('0 0 36 36', 360, 360, f'<path fill="#fff" fill-rule="evenodd" d="{D}"/>', CRISP))
# UI glyphs: tight square box (32 tall, 29 wide centred), take the text colour
DG = path_d(loops)
write('cat-glyph.svg', svg('-1.5 0 32 32', 24, 24, f'<path fill="currentColor" fill-rule="evenodd" d="{DG}"/>'))
write('cat-outline.svg', svg('-1.5 0 32 32', 24, 24,
      f'<path fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="miter" vector-effect="non-scaling-stroke" d="{DG}"/>'))
# small (15 x 16 cells): pixel-exact at 16 px; at 32, 48... too
DS = path_d(s_loops, 0.5, 0)
write('cat-glyph-16.svg', svg('0 0 16 16', 16, 16, f'<path fill="currentColor" fill-rule="evenodd" d="{DS}"/>', CRISP))

# simplified outline for 16-24 px UI: the traced silhouette, Douglas-Peucker'd, in a 24 box; eyes as two short strokes
eps = float(os.environ.get('EPS', '1.3'))
simp = simplify(outer, eps)
k = 22 / 32; ox = 12 - 29 * k / 2; oy = 1
pts = " ".join(f"{fmt(ox+x*k)},{fmt(oy+y*k)}" for x, y in simp)
eyes = ""
for L in holes:
    xs = [p[0] for p in L]; ys = [p[1] for p in L]
    cx = (min(xs)+max(xs))/2; eyes += f'<path vector-effect="non-scaling-stroke" d="M{fmt(ox+cx*k)} {fmt(oy+min(ys)*k+0.3)}v{fmt((max(ys)-min(ys))*k-0.6)}"/>'
write('cat-outline-simple.svg', svg('0 0 24 24', 24, 24,
      f'<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round">'
      f'<polygon vector-effect="non-scaling-stroke" points="{pts}"/>{eyes}</g>'))
print('vertices', len(outer), '->', len(simp), 'holes', len(holes), 'small loops', len(s_loops))

# ── app icons ──
def squircle(x, y, s, n=5, steps=96):
    r = s / 2; cx, cy = x + r, y + r; P = []
    for i in range(steps):
        t = 2 * math.pi * i / steps; c, sn = math.cos(t), math.sin(t)
        P.append(f"{fmt(cx + r * math.copysign(abs(c) ** (2/n), c))},{fmt(cy + r * math.copysign(abs(sn) ** (2/n), sn))}")
    return "M" + "L".join(P) + "Z"
BG, INK = '#0e0e10', '#f4f4f1'
tile = squircle(100, 100, 824)
def mac_tile(cat_d):
    return svg('0 0 1024 1024', 1024, 1024,
        f'<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#18181b"/><stop offset="1" stop-color="{BG}"/></linearGradient></defs>'
        f'<path d="{tile}" fill="url(#g)"/><path d="{tile}" fill="none" stroke="#fff" stroke-opacity=".09" stroke-width="4"/>'
        f'<path fill="{INK}" fill-rule="evenodd" shape-rendering="crispEdges" d="{cat_d}"/>')
write('appicon-macos.svg', mac_tile(path_d(loops, 272, 240, 16)))   # cell 16: whole pixels at 1024/512/256/128/64
# small-size macOS tile: the 15x16 grid, cell 32 on 1024 → 1 px at 32
write('appicon-macos-small.svg', mac_tile(path_d(s_loops, 272, 256, 32)))
# iOS: full bleed, opaque, the system masks the corners. cell 20 → 580 x 640
write('appicon-ios.svg', svg('0 0 1024 1024', 1024, 1024,
      f'<rect width="1024" height="1024" fill="{BG}"/><path fill="{INK}" fill-rule="evenodd" shape-rendering="crispEdges" d="{path_d(loops, 222, 180, 20)}"/>'))
# apple-touch 180: opaque, cell 4 → 116 x 128
write('apple-touch.svg', svg('0 0 180 180', 180, 180,
      f'<rect width="180" height="180" fill="{BG}"/><path fill="{INK}" fill-rule="evenodd" shape-rendering="crispEdges" d="{path_d(loops, 32, 24, 4)}"/>'))
# favicon tiles: rounded dark square, small grid at 1 px (16) / 2 px (32); full grid at 1 px (48 → 48 box)
write('favicon-16.svg', svg('0 0 16 16', 16, 16,
      f'<rect width="16" height="16" rx="3" fill="{BG}"/><path fill="{INK}" fill-rule="evenodd" shape-rendering="crispEdges" d="{path_d(s_loops, 0.5, 0)}"/>'))
write('favicon-32.svg', svg('0 0 32 32', 32, 32,
      f'<rect width="32" height="32" rx="6" fill="{BG}"/><path fill="{INK}" fill-rule="evenodd" shape-rendering="crispEdges" d="{path_d(s_loops, 1, 0, 2)}"/>'))
write('favicon-48.svg', svg('0 0 48 48', 48, 48,
      f'<rect width="48" height="48" rx="9" fill="{BG}"/><path fill="{INK}" fill-rule="evenodd" shape-rendering="crispEdges" d="{path_d(loops, 9, 8)}"/>'))
