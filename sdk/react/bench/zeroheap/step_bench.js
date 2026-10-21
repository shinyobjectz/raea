const N = parseInt(typeof arguments !== "undefined" && arguments[1] || "2000"), FRAMES = 7200, mode = arguments[0];
function stats(t) { t.sort((a, b) => a - b); return `p50 ${t[t.length >> 1].toFixed(3)} p99 ${t[Math.floor(t.length * 0.99)].toFixed(3)} max ${t[t.length - 1].toFixed(2)} ms`; }
const times = [];
if (mode === "objects") {
  // today's shape: a pure step returning fresh objects (as a game's fold does)
  let s = { ents: Array.from({ length: N }, (_, i) => ({ x: i, y: i * 2, vx: 1.5, vy: -0.5, hp: 3 })), score: 0 };
  const step = (s, f) => ({ ...s, score: s.score + 1, ents: s.ents.map(e => { const x = e.x + e.vx, y = e.y + e.vy; return { ...e, x: x > 1920 ? 0 : x, y: y < 0 ? 1080 : y }; }) });
  for (let f = 0; f < FRAMES; f++) { const t0 = preciseTime(); s = step(s, f); times.push((preciseTime() - t0) * 1000); }
  print(`objects N=${N}: ${stats(times)} (check ${s.ents[7].x.toFixed(1)})`);
} else {
  // ZeroHeap: fields as typed arrays, read A write B, swap (still pure frame to frame)
  const mk = () => ({ x: new Float64Array(N), y: new Float64Array(N), vx: new Float64Array(N), vy: new Float64Array(N), hp: new Int32Array(N), score: new Int32Array(1) });
  let a = mk(), b = mk();
  for (let i = 0; i < N; i++) { a.x[i] = i; a.y[i] = i * 2; a.vx[i] = 1.5; a.vy[i] = -0.5; a.hp[i] = 3; }
  const step = (a, b) => { b.score[0] = a.score[0] + 1; for (let i = 0; i < N; i++) { const x = a.x[i] + a.vx[i], y = a.y[i] + a.vy[i]; b.x[i] = x > 1920 ? 0 : x; b.y[i] = y < 0 ? 1080 : y; b.vx[i] = a.vx[i]; b.vy[i] = a.vy[i]; b.hp[i] = a.hp[i]; } };
  for (let f = 0; f < FRAMES; f++) { const t0 = preciseTime(); step(a, b); const t = a; a = b; b = t; times.push((preciseTime() - t0) * 1000); }
  print(`buffer  N=${N}: ${stats(times)} (check ${a.x[7].toFixed(1)})`);
}
