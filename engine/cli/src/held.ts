// A render that still holds an error isn't shown (docs/stage-guards.feature SG15). raea render exits 1 on an error and
// keeps its findings in the project's record (core/facts/src/record.rs: one file a render, in the project's own .raea
// folder), but the file it wrote is still there to post. raea show looks each file up: the newest render that wrote
// it, and that render's errors less those it was let through with (raea render --despite) and those raea show
// --despite CHECK "why" names. What's left refuses the show, each error named; the agent fixes it, or says why it goes
// anyway, and the person reads the why on the item.

/** A check let through, and why. */
export interface Despite {
  check: string;
  why: string;
}

interface Entry {
  at?: number;
  by?: string;
  out?: string | null;
  findings?: { check?: string; level?: string; says?: string }[];
  despite?: Despite[];
}

/** What it reads the record through (app.ts's Here): a file's text, whether a path exists, a folder's names. */
export interface Records {
  exists: (path: string) => boolean;
  read: (path: string) => string;
  list?: (dir: string) => string[];
}

const slashed = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");
const up = (p: string) => p.slice(0, Math.max(0, p.lastIndexOf("/")));
const rooted = (p: string) => p.startsWith("/") || /^[A-Za-z]:\//.test(p);

/** The nearest folder at or above `from` holding .raea/project (as the record's own up() finds it). */
function projectAbove(from: string, h: Records): string | undefined {
  for (let d = slashed(from); d; d = up(d)) if (h.exists(`${d}/.raea/project`)) return d;
  return undefined;
}

function entries(project: string, h: Records): Entry[] {
  // the project's own .raea, never ~/.raea
  const own = `${project}/.raea`;
  const dir = `${own}/record`;
  if (!h.list || !h.exists(dir)) return [];
  return h.list(dir).filter((n) => n.endsWith(".json")).flatMap((n) => {
    try {
      return [JSON.parse(h.read(`${dir}/${n}`)) as Entry];
    } catch {
      return []; // a half-written entry is no render's
    }
  });
}

/** Each file's open errors, as "FILE: CHECK: what it says": none for a file no render in its project wrote. */
export function openErrors(files: string[], cwd: string, despite: Despite[], h: Records): string[] {
  const out: string[] = [];
  const read = new Map<string, Entry[]>();
  for (const file of files.map(slashed)) {
    const project = projectAbove(up(file), h) ?? projectAbove(cwd, h);
    if (!project) continue;
    if (!read.has(project)) read.set(project, entries(project, h));
    const wrote = (e: Entry) => {
      if (e.by !== "raea render" || !e.out) return false;
      const o = slashed(rooted(slashed(e.out)) ? e.out : `${project}/${e.out}`);
      return file === o || file.startsWith(`${o}/`);
    };
    const last = read.get(project)!.filter(wrote).sort((a, b) => (b.at ?? 0) - (a.at ?? 0))[0];
    if (!last) continue;
    const let_ = new Set([...(last.despite ?? []), ...despite].map((d) => d.check));
    const name = file.slice(up(file).length + 1);
    for (const f of last.findings ?? []) if (f.level === "error" && f.check && !let_.has(f.check)) out.push(`${name}: ${f.check}: ${f.says ?? ""}`);
  }
  return out;
}

/** The refusal, or nothing when every file is clear. */
export function held(files: string[], cwd: string, despite: Despite[], h: Records): string | undefined {
  const open = openErrors(files, cwd, despite, h);
  if (!open.length) return undefined;
  const checks = [...new Set(open.map((l) => l.split(": ")[1]))];
  return `not shown: the render still holds ${open.length === 1 ? "an error" : `${open.length} errors`} (raea findings --open):\n  ${open.slice(0, 12).join("\n  ")}${open.length > 12 ? `\n  and ${open.length - 12} more` : ""}\nfix ${open.length === 1 ? "it" : "them"} and render again, or say why it goes anyway: ${checks.map((c) => `--despite ${c} "why"`).join(" ")} (the person reads the why on the item)`;
}

/** The ask line the person reads, with what was let through and why. */
export const askWith = (ask: string | undefined, despite: Despite[]) =>
  [ask, ...despite.map((d) => `let through: ${d.check}, ${d.why}`)].filter(Boolean).join(" · ") || undefined;
