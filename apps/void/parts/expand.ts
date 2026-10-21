// bun parts/expand.ts EDIT.rrl --words WORDS.json [--parts parts/parts.tsx] --o OUT.rrl
// An edit places parts like any row (`statcard s1 {big: "2%", from: 251, to: 390}`); this replaces each such line with
// the rows its React component renders, in place, and leaves every other line (clips, keys, rules, overrides) as is.
// Words are mapped through the edit's clips onto its frames so parts can cue on phrases.
import { createElement } from "react";
import { render, toRrl } from "./rows";
import { EditCtx, type Word, type Theme } from "./kit";

const args = process.argv.slice(2);
const flag = (n: string, d?: string) => { const i = args.indexOf("--" + n); return i >= 0 ? args[i + 1] : d; };
const editPath = args[0];
const partsPath = flag("parts", decodeURIComponent(new URL("./parts.tsx", import.meta.url).pathname))!;
const { registry } = await import(partsPath);
const src = await Bun.file(editPath).text();

// rrl column lists: {a: 1, b: "x", c: #fff, d: name}
function cols(s: string): Record<string, any> {
  const out: Record<string, any> = {};
  const re = /\s*([a-z_][a-z0-9_]*)\s*:\s*("(?:[^"\\]|\\.)*"|#[0-9a-fA-F]+|-?[0-9.]+|[A-Za-z_][\w]*)\s*,?/gy;
  let m: RegExpExecArray | null;
  while ((m = re.exec(s))) {
    const v = m[2];
    out[m[1].replace(/_([a-z])/g, (_, c) => c.toUpperCase())] =
      v.startsWith('"') ? JSON.parse(v) : /^-?[0-9.]+$/.test(v) ? Number(v) : v;
  }
  return out;
}

const fps = Number(/edit\s*\{[^}]*fps:\s*(\d+)/.exec(src)?.[1] ?? 30);
const width = Number(/edit\s*\{[^}]*width:\s*(\d+)/.exec(src)?.[1] ?? 1080);
const height = Number(/edit\s*\{[^}]*height:\s*(\d+)/.exec(src)?.[1] ?? 1920);
const clips = [...src.matchAll(/^clip\s+\w+\s*\{(.*)\}\s*$/gm)].map((m) => cols(m[1]));
const raw = flag("words") ? await Bun.file(flag("words")!).json() : [];
const list: any[] = Array.isArray(raw) ? raw : raw.words;
const toFrame = (s: number) => {
  for (const c of clips) {
    const len = (c.to - c.from) / fps;
    if (s >= c.srcIn - 1e-6 && s < c.srcIn + len) return c.from + Math.round((s - c.srcIn) * fps);
  }
};
const words: Word[] = list.flatMap((w) => {
  const ws = toFrame(w.start_ms / 1000);
  if (ws === undefined) return [];
  const we = toFrame(Math.max(w.start_ms, w.end_ms - 1) / 1000) ?? ws + 6;
  return [{ text: w.text, ws, we }];
});

// The brand's theme: the space's brand/brand.json (the brand tool writes it; the space is the edit's folder, or the
// parts folder's parent), then any theme columns on the edit row (`edit {..., accent: #cf202f, display_font: "Oswald"}`).
// A part's own props win over both. No brand, no theme: parts that need a colour or face ask for it.
const dirOf = (p: string) => p.replace(/\/[^/]*$/, "") || ".";
let theme: Theme = {};
for (const d of [dirOf(editPath), dirOf(dirOf(partsPath))]) {
  const f = Bun.file(`${d}/brand/brand.json`);
  if (!(await f.exists())) continue;
  const b = await f.json();
  const num = (v: any) => (typeof v === "number" ? v : undefined);
  theme = {
    accent: b.palette?.accent ?? undefined, surface: b.palette?.surface ?? undefined,
    textColor: b.palette?.text_on_surface ?? undefined, accentText: b.palette?.text_on_accent ?? undefined,
    displayFont: b.fonts?.heading?.kit ?? undefined, textFont: b.fonts?.body?.kit ?? undefined,
    scale: Object.fromEntries(Object.entries(b.scale ?? {}).map(([k, l]: any) => [k, { size: num(l?.size), weight: num(l?.weight), line: num(l?.line), tracking: num(l?.tracking) }])),
    button: b.button ? { bg: b.button.bg ?? undefined, color: b.button.color ?? undefined, font: b.button.font ?? undefined, size: num(b.button.size), weight: num(b.button.weight), radius: num(b.button.radius) } : undefined,
    radius: num(b.radius), shadow: b.shadow ?? undefined,
  };
  break;
}
const editRow = cols(/^edit\s*\{(.*)\}\s*$/m.exec(src)?.[1] ?? "");
for (const k of ["accent", "surface", "textColor", "accentText", "displayFont", "textFont", "typeScale"] as const)
  if (editRow[k] != null) (theme as any)[k] = editRow[k];

const ctx = { fps, width, height, words, theme };
const seen = new Set<string>();
const t0 = performance.now();
let parts = 0, made = 0;
const out = src.split("\n").flatMap((line) => {
  const m = /^([a-z]+)\s+([A-Za-z_]\w*)\s*\{(.*)\}\s*$/.exec(line);
  const Comp = m && registry[m[1]];
  if (!Comp) return [line];
  parts++;
  const props = { id: m![2], ...cols(m![3]) };
  let rows: string[];
  try { rows = toRrl(render(createElement(EditCtx.Provider, { value: ctx }, createElement(Comp, props))), seen); }
  catch (e) { console.error(`${m![1]} ${m![2]}: ${(e as Error).message ?? e}`); process.exit(1); }
  made += rows.length;
  return [`-- ${m![1]} ${m![2]} (React: ${rows.length} lines)`, ...rows];
});
await Bun.write(flag("o", editPath.replace(/\.rrl$/, ".out.rrl"))!, out.join("\n"));
console.error(`expanded ${parts} parts into ${made} rrl lines in ${(performance.now() - t0).toFixed(0)} ms`);
