// raea's own commands inside the agent's shell, each a native call: rdb and raea-motion run in process (their own
// command lines, output captured), and solid, ecs and media are small command lines over the engine's Manifold
// solids, ECS worlds and coders. Paths are the shell's: an argument that names a project path ("/a", "./a", "../a", or
// a file that's there) is given to the tool as the real file under the project's root, and the root never shows in what
// comes back. Big outputs stay in files (renders, transcodes); what's printed is what a person would read.
import { defineCommand, type CustomCommand } from "just-bash/browser";
import { native } from "@raea/motion/native";
import type { RaeaFs } from "./fs";

type Ctx = { cwd: string; stdin: unknown; fs: { exists(p: string): Promise<boolean> } };
const ok = (stdout: string, exitCode = 0, stderr = "") => ({ stdout, stderr, exitCode });

/** A shell path made absolute and clean ("/" is the project's root). */
export function vpath(cwd: string, p: string): string {
  const parts: string[] = [];
  for (const x of (p.startsWith("/") ? p : `${cwd}/${p}`).split("/")) {
    if (!x || x === ".") continue;
    if (x === "..") parts.pop();
    else parts.push(x);
  }
  return "/" + parts.join("/");
}

function realOf(root: string, v: string): string {
  return v === "/" ? root : root + v;
}

/** The arguments with project paths made real: an absolute path is the project's, and a relative one that climbs
 * ("../x") is clamped to the project's root; other relative paths work as they are (the tool runs in the shell's
 * folder). A --flag=/path keeps its flag. */
function mapArgs(root: string, ctx: Ctx, args: string[]): string[] {
  return args.map((a) => {
    const m = /^(--?[\w-]+=)(.*)$/.exec(a);
    const [pre, p] = m ? [m[1], m[2]] : ["", a];
    const climbs = p.split("/").includes("..");
    return p.startsWith("/") || climbs ? pre + realOf(root, vpath(ctx.cwd, p)) : a;
  });
}

/** The project's root taken out of a tool's output, so paths read as the shell's. */
const unroot = (root: string, s: string) => s.split(root + "/").join("/").split(root).join("/");

/** Every argument that could name a project path, kept as it is now: the tool writes outside RaeaFs, so its row (and
 * undo) gets the files from here. A path that isn't there is kept as absent (an output the tool makes). */
function touchArgs(fs: RaeaFs, cwd: string, args: string[]) {
  for (const a of args) {
    const p = a.replace(/^--?[\w-]+=/, "");
    if (!p || p.startsWith("-") || /\s/.test(p) || (!p.includes(".") && !p.includes("/"))) continue;
    try {
      native("fs.touch", { id: fs.id, path: vpath(cwd, p) });
    } catch {
      // not a path in the project (a folder that isn't there, a URL): nothing to keep
    }
  }
}

function tool(name: "rdb" | "raea-motion", fs: RaeaFs): CustomCommand {
  return defineCommand(name, async (args, ctx) => {
    const c = ctx as unknown as Ctx;
    touchArgs(fs, c.cwd, args);
    const r = native(`cli.${name}` as "cli.rdb", { args: mapArgs(fs.root, c, args), cwd: realOf(fs.root, vpath(c.cwd, ".")) });
    return ok(unroot(fs.root, r.stdout), r.exit, unroot(fs.root, r.stderr));
  });
}

// ---- solid: Manifold from an expression ----------------------------------------------------------------------------
const SOLID_HELP = `solid EXPR [volume|bounds|triangles|empty|slice Z]...
  a Manifold solid from an expression (box(w,h,d), rounded_box, sphere, cylinder, cone, torus, text3d("T", size, depth),
  union/minus/intersect/hull(a, b, ...), move/rotate/scale(s, x, y, z)), and what's asked of it, one answer a line
  (all of volume, bounds and triangles when nothing's asked)
`;
function solidCommand(): CustomCommand {
  return defineCommand("solid", async (args) => {
    if (!args.length || args[0] === "--help") return ok(SOLID_HELP, args.length ? 0 : 2);
    let made: { id: number; expr: string };
    try {
      made = native("solid.expr", { expr: args[0] });
    } catch (e) {
      return ok("", 1, `solid: ${(e as Error).message}\n`);
    }
    const asks = args.slice(1).length ? args.slice(1) : ["volume", "bounds", "triangles"];
    const lines: string[] = [];
    try {
      for (let i = 0; i < asks.length; i++) {
        const what = asks[i];
        if (what === "slice") {
          const z = Number(asks[++i]);
          const s = native("solid.query", { id: made.id, what: "slice", z }) as { area: number; contours: number };
          lines.push(`slice ${z}: area ${s.area.toFixed(6)}, ${s.contours} contours`);
        } else if (["volume", "bounds", "triangles", "empty"].includes(what)) {
          const v = native("solid.query", { id: made.id, what: what as "volume" });
          lines.push(`${what} ${typeof v === "number" ? +v.toFixed(6) : JSON.stringify(v)}`);
        } else return ok(lines.join("\n") + (lines.length ? "\n" : ""), 2, `solid: no question "${what}" (volume, bounds, triangles, empty, slice Z)\n`);
      }
    } finally {
      native("solid.free", { id: made.id });
    }
    return ok(lines.join("\n") + "\n");
  });
}

// ---- ecs: a world kept in a JSON file -------------------------------------------------------------------------------
const ECS_HELP = `ecs WORLD.json spawn KIND col=value ...     an entity (numbers as numbers), its id printed
ecs WORLD.json set ID col=value ...
ecs WORLD.json despawn ID
ecs WORLD.json query KIND ['col op value' ...]   ids passing every condition (ops == != < <= > >=)
ecs WORLD.json count KIND | rows | column KIND COL
  the world's tables live in the engine while a command runs, and in WORLD.json (its rows) between commands
`;
function parseValue(v: string): number | string | boolean {
  if (v === "true" || v === "false") return v === "true";
  const n = Number(v);
  return v.trim() !== "" && Number.isFinite(n) ? n : v;
}
function ecsCommand(fs: RaeaFs): CustomCommand {
  return defineCommand("ecs", async (args, ctx) => {
    if (args.length < 2 || args[0] === "--help") return ok(ECS_HELP, args[0] === "--help" ? 0 : 2);
    const file = vpath((ctx as unknown as Ctx).cwd, args[0]);
    const [verb, ...rest] = args.slice(1);
    const w = native("ecs.new", {}).world;
    try {
      // load: rows as written by `rows`, ids kept by spawning in order (gaps filled and removed)
      let rows: { id: number; kind: string; [k: string]: unknown }[] = [];
      if (await fs.exists(file)) rows = JSON.parse(await fs.readFile(file));
      const map = new Map<number, number>();
      const maxId = rows.reduce((m, r) => Math.max(m, r.id), 0);
      const byId = new Map(rows.map((r) => [r.id, r]));
      for (let id = 1; id <= maxId; id++) {
        const r = byId.get(id);
        const { id: _i, kind, ...cols } = r ?? { id, kind: "__gap" };
        const got = native("ecs.spawn", { world: w, kind: kind as string, cols: cols as Record<string, number> }).id;
        map.set(id, got);
        if (!r) native("ecs.despawn", { world: w, id: got });
      }
      const cols = (kv: string[]) => Object.fromEntries(kv.map((x) => x.split("=")).map(([k, ...v]) => [k, parseValue(v.join("="))]));
      let out = "";
      let changed = false;
      switch (verb) {
        case "spawn":
          out = `${native("ecs.spawn", { world: w, kind: rest[0], cols: cols(rest.slice(1)) }).id}\n`;
          changed = true;
          break;
        case "set":
          native("ecs.set", { world: w, id: Number(rest[0]), cols: cols(rest.slice(1)) });
          changed = true;
          break;
        case "despawn":
          native("ecs.despawn", { world: w, id: Number(rest[0]) });
          changed = true;
          break;
        case "query": {
          const where = rest.slice(1).map((c) => {
            const m = /^\s*(\S+)\s*(==|!=|<=|>=|<|>)\s*(.+?)\s*$/.exec(c);
            if (!m) throw new Error(`ecs: a condition is "col op value", got "${c}"`);
            return [m[1], m[2], parseValue(m[3])] as [string, "==", number];
          });
          out = native("ecs.query", { world: w, kind: rest[0], where }).ids.join("\n") + "\n";
          break;
        }
        case "count":
          out = `${native("ecs.count", { world: w, kind: rest[0] })}\n`;
          break;
        case "column":
          out = native("ecs.column", { world: w, kind: rest[0], col: rest[1] }).map((x) => (x === null ? "" : String(x))).join("\n") + "\n";
          break;
        case "rows":
          out = JSON.stringify(native("ecs.rows", { world: w })) + "\n";
          break;
        default:
          return ok("", 2, `ecs: no verb "${verb}"\n${ECS_HELP}`);
      }
      if (changed) {
        const all = native("ecs.rows", { world: w }).filter((r) => r.kind !== "__gap");
        await fs.writeFile(file, JSON.stringify(all, null, 1) + "\n");
      }
      return ok(out);
    } catch (e) {
      return ok("", 1, `ecs: ${(e as Error).message}\n`);
    } finally {
      native("ecs.free", { world: w });
    }
  });
}

// ---- media: probe and the heavy jobs, natively ----------------------------------------------------------------------
const MEDIA_HELP = `media transcode IN OUT [--from S] [--to S] [--height H] [--kbps K]   trim, scale, encode with sound (MP4), natively
media size FILE                                                       bytes
  (rdb media probe|trim|proxy|... has the rest)
`;
function mediaCommand(fs: RaeaFs): CustomCommand {
  return defineCommand("media", async (args, ctx) => {
    const c = ctx as unknown as Ctx;
    const real = (p: string) => realOf(fs.root, vpath(c.cwd, p));
    const flag = (n: string) => {
      const i = args.indexOf(n);
      return i >= 0 ? Number(args[i + 1]) : undefined;
    };
    try {
      switch (args[0]) {
        case "transcode": {
          const r = native("media.transcode", { path: real(args[1]), out: real(args[2]), from: flag("--from"), to: flag("--to"), height: flag("--height"), kbps: flag("--kbps") });
          return ok(`${args[2]}: ${r.frames} frames, ${r.width}x${r.height} at ${r.fps} fps, ${r.kbps} kbps, ${r.bytes} bytes\n`);
        }
        case "size":
          return ok(`${native("media.size", { path: real(args[1]) })}\n`);
        default:
          return ok(MEDIA_HELP, args[0] === "--help" ? 0 : 2);
      }
    } catch (e) {
      return ok("", 1, `media: ${unroot(fs.root, (e as Error).message)}\n`);
    }
  });
}

// ---- sqlite3: SQLite natively -----------------------------------------------------------------------------------------
type Cell = number | string | null | { blob: string };
const SQL_HELP = `sqlite3 [OPTIONS] [DATABASE] [SQL]
  SQL by SQLite (natively) on DATABASE (a project file; :memory: when none), from the argument or stdin
  -list -csv -json -line -column -table -box -markdown -tabs -quote -html -ascii   output mode (list by default)
  -header | -noheader   -separator SEP   -newline SEP   -nullvalue TEXT   -readonly   -bail   -echo   -cmd SQL   -version
`;
function cellText(c: Cell, nul: string): string {
  if (c === null) return nul;
  if (typeof c === "object") {
    const b = atob(c.blob);
    return b;
  }
  if (typeof c === "number" && Number.isFinite(c) && !Number.isInteger(c)) return String(c);
  return String(c);
}
function quoteCell(c: Cell): string {
  if (c === null) return "NULL";
  if (typeof c === "number") return String(c);
  if (typeof c === "object") return "X'" + Array.from(atob(c.blob), (ch) => ch.charCodeAt(0).toString(16).padStart(2, "0")).join("").toUpperCase() + "'";
  return "'" + c.replace(/'/g, "''") + "'";
}
function csvCell(t: string): string {
  return /[",\n\r]/.test(t) || t !== t.trim() ? '"' + t.replace(/"/g, '""') + '"' : t;
}
function htmlEsc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}
interface SqlOpts {
  mode: string;
  header: boolean | undefined;
  sep: string | undefined;
  nl: string;
  nul: string;
}
function formatSet(cols: string[], rows: Cell[][], o: SqlOpts): string {
  const t = (c: Cell) => cellText(c, o.nul);
  const width = (i: number) => Math.max(cols[i].length, ...rows.map((r) => t(r[i]).length));
  const nl = o.nl;
  switch (o.mode) {
    case "csv": {
      const sep = o.sep ?? ",";
      const lines = rows.map((r) => r.map((c) => (c === null ? o.nul : csvCell(t(c)))).join(sep));
      return (o.header ? [cols.map(csvCell).join(sep), ...lines] : lines).map((l) => l + "\r\n").join("");
    }
    case "tabs":
      return (o.header ? [cols.join("\t")] : []).concat(rows.map((r) => r.map(t).join("\t"))).map((l) => l + nl).join("");
    case "json":
      return rows.length ? "[" + rows.map((r) => "{" + cols.map((c, i) => JSON.stringify(c) + ":" + (r[i] === null ? "null" : typeof r[i] === "number" ? String(r[i]) : JSON.stringify(t(r[i])))).join(",") + "}").join(",\n") + "]\n" : "";
    case "line": {
      const w = Math.max(...cols.map((c) => c.length));
      return rows.map((r) => cols.map((c, i) => c.padStart(w) + " = " + t(r[i])).join("\n") + "\n").join("\n");
    }
    case "quote":
      return (o.header ? [cols.map((c) => "'" + c + "'").join(",")] : []).concat(rows.map((r) => r.map(quoteCell).join(","))).map((l) => l + nl).join("");
    case "html":
      return (o.header ? ["<TR>" + cols.map((c) => "<TH>" + htmlEsc(c) + "</TH>").join("\n") + "\n</TR>"] : []).concat(rows.map((r) => "<TR>" + r.map((c) => "<TD>" + htmlEsc(t(c)) + "</TD>").join("\n") + "\n</TR>")).map((l) => l + "\n").join("");
    case "ascii":
      return (o.header ? [cols.join("\x1f")] : []).concat(rows.map((r) => r.map(t).join("\x1f"))).map((l) => l + "\x1e").join("");
    case "column": {
      const ws = cols.map((_, i) => width(i));
      const line = (cells: string[]) => cells.map((c, i) => (i === cells.length - 1 ? c : c.padEnd(ws[i]))).join("  ").trimEnd();
      const head = o.header !== false ? [line(cols), ws.map((w) => "-".repeat(w)).join("  ")] : [];
      return head.concat(rows.map((r) => line(r.map(t)))).map((l) => l + "\n").join("");
    }
    case "markdown": {
      const ws = cols.map((_, i) => width(i));
      const line = (cells: string[]) => "| " + cells.map((c, i) => c.padEnd(ws[i])).join(" | ") + " |";
      return [line(cols), "|" + ws.map((w) => "-".repeat(w + 2)).join("|") + "|", ...rows.map((r) => line(r.map(t)))].map((l) => l + "\n").join("");
    }
    case "table":
    case "box": {
      const ws = cols.map((_, i) => width(i));
      const box = o.mode === "box";
      const [h, v, tl, tm, tr, ml, mm, mr, bl, bm, br] = box ? ["─", "│", "┌", "┬", "┐", "├", "┼", "┤", "└", "┴", "┘"] : ["-", "|", "+", "+", "+", "+", "+", "+", "+", "+", "+"];
      const rule = (l: string, m: string, r: string) => l + ws.map((w) => h.repeat(w + 2)).join(m) + r;
      const line = (cells: string[], num: boolean[]) => v + cells.map((c, i) => " " + (num[i] ? c.padStart(ws[i]) : c.padEnd(ws[i])) + " ").join(v) + v;
      const isNum = (r: Cell[]) => r.map((c) => typeof c === "number");
      return [rule(tl, tm, tr), line(cols, cols.map(() => false)), rule(ml, mm, mr), ...rows.map((r) => line(r.map(t), isNum(r))), rule(bl, bm, br)].map((l) => l + "\n").join("");
    }
    default: {
      const sep = o.sep ?? "|";
      return (o.header ? [cols.join(sep)] : []).concat(rows.map((r) => r.map(t).join(sep))).map((l) => l + nl).join("");
    }
  }
}
function sqliteCommand(fs: RaeaFs): CustomCommand {
  return defineCommand("sqlite3", async (args, ctx) => {
    const c = ctx as unknown as Ctx & { stdin: string };
    const o: SqlOpts = { mode: "list", header: undefined, sep: undefined, nl: "\n", nul: "" };
    let readonly = false, bail = false, echo = false;
    const cmds: string[] = [];
    const pos: string[] = [];
    for (let i = 0; i < args.length; i++) {
      const a = args[i];
      const mode = /^-(list|csv|json|line|column|table|markdown|tabs|box|quote|html|ascii)$/.exec(a);
      if (a === "--") {
        pos.push(...args.slice(i + 1));
        break;
      } else if (a === "-version") return ok("3 (raea, SQLite)\n");
      else if (a === "--help" || a === "-help") return ok(SQL_HELP);
      else if (mode) o.mode = mode[1];
      else if (a === "-header") o.header = true;
      else if (a === "-noheader") o.header = false;
      else if (a === "-readonly") readonly = true;
      else if (a === "-bail") bail = true;
      else if (a === "-echo") echo = true;
      else if (a === "-separator") o.sep = args[++i];
      else if (a === "-newline") o.nl = args[++i];
      else if (a === "-nullvalue") o.nul = args[++i];
      else if (a === "-cmd") cmds.push(args[++i]);
      else if (a.startsWith("-")) return ok("", 1, `sqlite3: Error: unknown option: ${a}\nUse -help for a list of options.\n`);
      else pos.push(a);
    }
    if (["column", "table", "box", "markdown"].includes(o.mode) && o.header === undefined) o.header = true;
    const db = pos[0] ?? ":memory:";
    const stdinText = typeof c.stdin === "string" ? c.stdin : "";
    const sql = [...cmds, pos.length > 1 ? pos.slice(1).join(" ") : stdinText].join(";\n");
    const path = db === ":memory:" ? db : realOf(fs.root, vpath(c.cwd, db));
    if (db !== ":memory:" && !readonly) touchArgs(fs, c.cwd, [db]);
    let r: { sets: { columns: string[]; rows: Cell[][] }[]; error: string | null };
    try {
      r = native("sqlite.exec", { path, sql, readonly });
    } catch (e) {
      return ok("", 1, unroot(fs.root, (e as Error).message) + "\n");
    }
    let out = echo ? sql + "\n" : "";
    for (const set of r.sets) out += formatSet(set.columns, set.rows, o);
    void bail;
    return r.error ? ok(out, 1, unroot(fs.root, r.error) + "\n") : ok(out);
  });
}

// ---- js-exec: JavaScript and TypeScript on raea's V8 ----------------------------------------------------------------
const JS_HELP = `js-exec [-c CODE | FILE] [ARGS...]   JavaScript or TypeScript (FILE: .ts .tsx .js .mjs; -c: TypeScript) as an ES module
  on raea's V8 in its own isolate: imports only from the project, console to stdout and stderr, process.argv/env/exit,
  no filesystem or network calls; --timeout MS (default 30000)
`;
function jsCommand(fs: RaeaFs): CustomCommand {
  return defineCommand("js-exec", async (args, ctx) => {
    const c = ctx as unknown as Ctx & { stdin: string; env: Map<string, string> };
    if (!args.length || args[0] === "--help") return ok(JS_HELP, args.length ? 0 : 2);
    let timeoutMs = 30000;
    const rest = [...args];
    const t = rest.indexOf("--timeout");
    if (t >= 0) {
      timeoutMs = Number(rest[t + 1]) || timeoutMs;
      rest.splice(t, 2);
    }
    const isCode = rest[0] === "-c" || rest[0] === "-e";
    const env: Record<string, string> = {};
    for (const [k, v] of c.env ?? []) env[k] = v;
    const cwd = realOf(fs.root, vpath(c.cwd, "."));
    const base = { root: fs.root, cwd, argv: ["js-exec", ...rest.slice(isCode ? 2 : 1)], env, stdin: typeof c.stdin === "string" ? c.stdin : "", timeoutMs };
    const r = isCode ? native("js.run", { ...base, code: rest[1] ?? "", ts: true }) : native("js.run", { ...base, file: realOf(fs.root, vpath(c.cwd, rest[0])) });
    return ok(r.stdout, r.exit, unroot(fs.root, r.stderr));
  });
}

// ---- WASI tools: what's compiled to WASI runs in the sandbox (wsx) ---------------------------------------------------
/** The sandbox's WASI tools as commands, except names the shell already has (just-bash's text tools and raea's own):
 * just-bash is the shell and its tools, wsx runs anything compiled to WASI (Wasmtime, Pulley on the iPhone) with only
 * the project folder visible and its own fuel and memory limits. */
export function wasiCommands(fs: RaeaFs, taken: Set<string>): CustomCommand[] {
  let tools: string[] = [];
  try {
    tools = native("wsx.tools", {});
  } catch {
    return [];
  }
  return tools
    .filter((t) => !taken.has(t))
    .map((t) =>
      defineCommand(t, async (args, ctx) => {
        const c = ctx as unknown as Ctx;
        // the tool sees the project as "."; shell paths become relative to it
        const rel = (p: string) => "." + vpath(c.cwd, p);
        const mapped = args.map((a) => (a.startsWith("/") || a.split("/").includes("..") ? rel(a) : c.cwd === "/" || a.startsWith("-") ? a : /[\/.]/.test(a) ? rel(a) : a));
        touchArgs(fs, c.cwd, args);
        const r = native("wsx.run", { tool: t, args: mapped, project: fs.root });
        return ok(r.stdout, r.exit, r.stderr + (r.stopped ? r.stopped + "\n" : ""));
      }),
    );
}

// ---- browser: agent-browser's words, in process ------------------------------------------------------------------
/** `browser WORDS...`: agent-browser's command line (open, snapshot -i, click @e2, fill @e3 TEXT, get, screenshot,
 * eval, diff, batch, ...) on the host's browser, where it has one (the Mac: Chrome over CDP). A file argument
 * (screenshot, pdf, diff baselines, state files) is the project's. */
function browserCommand(fs: RaeaFs): CustomCommand[] {
  if (!native("host.has", { name: "browser.run" })) return [];
  return [
    defineCommand("browser", async (args, ctx) => {
      const c = ctx as unknown as Ctx;
      if (!args.length) return ok("", 2, "browser: give agent-browser's words (open URL, snapshot -i, click @e1, ...)\n");
      const fileArg = (a: string) => /\.(png|jpe?g|pdf|json|txt|html|webm|mp4|zip)$/i.test(a) && !/^[a-z]+:\/\//i.test(a);
      // an output's folder made first (agent-browser writes into existing folders only), through RaeaFs so it's
      // recorded; then the files kept as they are, so the row has them
      for (const a of args.filter(fileArg)) {
        const dir = vpath(c.cwd, a).replace(/\/[^/]*$/, "") || "/";
        if (dir !== "/") await fs.mkdir(dir, { recursive: true });
      }
      touchArgs(fs, c.cwd, args.filter(fileArg));
      const words = args.map((a) => (fileArg(a) ? realOf(fs.root, vpath(c.cwd, a)) : a));
      const r = native("browser.run" as never, { args: words } as never) as { success: boolean; text: string; error: string | null };
      const text = unroot(fs.root, r.text);
      return r.success ? ok(text) : ok("", 1, text || `browser: ${r.error ?? "failed"}\n`);
    }),
  ];
}

// ---- tar and html-to-markdown, natively -----------------------------------------------------------------------------
function tarCommand(fs: RaeaFs): CustomCommand {
  return defineCommand("tar", async (args, ctx) => {
    const c = ctx as unknown as Ctx;
    const o = { create: false, extract: false, list: false, gzip: false, verbose: false, file: "", dir: undefined as string | undefined, members: [] as string[] };
    for (let i = 0; i < args.length; i++) {
      const a = args[i];
      if (a === "-f" || a === "--file") o.file = args[++i] ?? "";
      else if (a === "-C" || a === "--directory") o.dir = args[++i];
      else if (a.startsWith("--file=")) o.file = a.slice(7);
      else if (a === "--create") o.create = true;
      else if (a === "--extract" || a === "--get") o.extract = true;
      else if (a === "--list") o.list = true;
      else if (a === "--gzip" || a === "--gunzip") o.gzip = true;
      else if (a === "--verbose") o.verbose = true;
      else if (/^-?[cxtzvf]+$/.test(a) && (a.startsWith("-") || i === 0)) {
        for (const ch of a.replace(/^-/, "")) {
          if (ch === "c") o.create = true;
          else if (ch === "x") o.extract = true;
          else if (ch === "t") o.list = true;
          else if (ch === "z") o.gzip = true;
          else if (ch === "v") o.verbose = true;
          else if (ch === "f") o.file = args[++i] ?? "";
        }
      } else o.members.push(a);
    }
    if (!o.file) return ok("", 2, "tar: give the archive with -f FILE\n");
    if ([o.create, o.extract, o.list].filter(Boolean).length !== 1) return ok("", 2, "tar: one of -c, -x or -t\n");
    const cwdReal = realOf(fs.root, vpath(c.cwd, "."));
    // what it writes is recorded: the archive when creating, the extraction folder when extracting
    touchArgs(fs, c.cwd, o.create ? [o.file] : o.extract ? [o.dir ?? "."].concat(o.members) : []);
    if (o.extract) native("fs.touch", { id: fs.id, path: vpath(c.cwd, o.dir ?? ".") });
    try {
      const r = native("tar.run", { root: fs.root, cwd: cwdReal, ...o, file: o.file.startsWith("/") ? fs.root + o.file : o.file, dir: o.dir && o.dir.startsWith("/") ? fs.root + o.dir : o.dir });
      return ok(r.stdout);
    } catch (e) {
      return ok("", 2, unroot(fs.root, (e as Error).message.replace(/^tar\.run: /, "")) + "\n");
    }
  });
}
function markdownCommand(): CustomCommand {
  return defineCommand("html-to-markdown", async (args, ctx) => {
    const c = ctx as unknown as Ctx & { stdin: string; fs: { readFile(p: string): Promise<string> } };
    let html: string;
    if (args[0] && args[0] !== "-") html = await c.fs.readFile(vpath(c.cwd, args[0]));
    else {
      // the stdin a shell hands over is bytes as latin1 text; as UTF-8 again
      const raw = typeof c.stdin === "string" ? c.stdin : "";
      html = new TextDecoder().decode(Uint8Array.from(raw, (ch) => ch.charCodeAt(0) & 255));
    }
    try {
      const md = native("html.markdown", { html }).trim();
      return ok(md ? md + "\n" : "");
    } catch (e) {
      return ok("", 1, `html-to-markdown: ${(e as Error).message}\n`);
    }
  });
}

/** raea's commands for a shell over `fs`. */
export function raeaCommands(fs: RaeaFs): CustomCommand[] {
  return [tool("rdb", fs), tool("raea-motion", fs), solidCommand(), ecsCommand(fs), mediaCommand(fs), sqliteCommand(fs), jsCommand(fs), tarCommand(fs), markdownCommand(), ...browserCommand(fs)];
}

/** A just-bash fetch over the native network under a policy (net.policy): the allow-list and credentials live in Rust. */
export function nativeFetch(policy: number) {
  return async (url: string, o: { method?: string; headers?: Headers | Record<string, string>; body?: string | Uint8Array; followRedirects?: boolean; timeoutMs?: number; maxRedirects?: number } = {}) => {
    const headers: Record<string, string> = {};
    if (o.headers && typeof (o.headers as Headers).forEach === "function" && !(o.headers instanceof Object && !("forEach" in o.headers))) (o.headers as Headers).forEach((v, k) => (headers[k] = v));
    else Object.assign(headers, o.headers ?? {});
    const body = o.body === undefined ? undefined : typeof o.body === "string" ? btoa(unescape(encodeURIComponent(o.body))) : btoa(Array.from(o.body, (b) => String.fromCharCode(b)).join(""));
    const r = native("net.fetch", { policy, url, method: o.method, headers, body, followRedirects: o.followRedirects, timeoutMs: o.timeoutMs, maxRedirects: o.maxRedirects });
    const bin = atob(r.body);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return { status: r.status, statusText: r.statusText, headers: r.headers, body: bytes, url: r.url, redirectChain: r.redirectChain };
  };
}


