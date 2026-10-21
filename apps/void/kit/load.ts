// A space's layout, read from its space.tsx: `bun apps/void/kit/load.ts SPACE_FOLDER` prints it as JSON (void's
// space_ui.rs reads it); a terminal's, from its terminal.tsx, with `--terminal FOLDER` (shell_ui.rs). Nothing in the
// file is rendered: its default export is called once and the elements it returns are walked.
import { resolve, join } from "node:path";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import * as kit from "../../../sdk/space/src/layout";
import * as term from "../../../sdk/terminal/src/layout";
import { Fragment } from "./jsx";

const here = import.meta.dir;
const spaceKit = join(here, "../../../sdk/space/src/layout.ts");
const terminalKit = join(here, "../../../sdk/terminal/src/layout.ts");

type El = { type: unknown; props: Record<string, any> };
const fail = (e: string) => {
  console.log(JSON.stringify({ error: e }));
  process.exit(0);
};
// a panel's actions (`export const actions = { name: { about, args, run } }`): read with @raea/motion (and the SDKs on
// it, @raea/space and @raea/terminal) stood in for by a module of no-ops (only the names, descriptions and argument shapes are wanted here; void runs an action by handing
// it to the panel live, as input)
const motionStub = (src: string) => {
  const names = new Set<string>();
  for (const m of src.matchAll(/import\s*(?:type\s*)?\{([^}]*)\}\s*from\s*["']@raea\/(?:motion|space|terminal)[^"']*["']/g))
    for (const n of m[1].split(",")) {
      const name = n.trim().replace(/^type\s+/, "").split(/\s+as\s+/).pop()!.trim();
      if (name) names.add(name);
    }
  return [...names].map((n) => `export const ${n} = (..._: unknown[]) => null;`).join("\n") + "\nexport default {};";
};
async function actionsOf(file: string): Promise<{ name: string; about: string; args: unknown }[]> {
  const src = await Bun.file(file).text();
  if (!/export\s+const\s+actions\b/.test(src)) return [];
  const dir = mkdtempSync(join(tmpdir(), "raea-panel-"));
  const b = await Bun.build({
    entrypoints: [file],
    outdir: dir,
    target: "bun",
    plugins: [{
      name: "raea-motion-stub",
      setup(b) {
        b.onResolve({ filter: /^@raea\/(motion|space|terminal)/ }, () => ({ path: "motion", namespace: "stub" }));
        b.onLoad({ filter: /.*/, namespace: "stub" }, () => ({ contents: motionStub(src), loader: "ts" }));
        b.onResolve({ filter: /^react(\/.*)?$/ }, () => ({ path: join(here, "jsx.ts") }));
      },
    }],
  });
  if (!b.success) fail(`${file}: ${b.logs.map((l) => l.message).join("; ")}`);
  const m = await import(b.outputs[0].path).catch((e) => fail(`${file}: ${e?.message ?? e}`));
  rmSync(dir, { recursive: true, force: true });
  return Object.entries((m.actions ?? {}) as Record<string, any>).map(([name, a]) => ({ name, about: a?.about ?? "", args: a?.args ?? {} }));
}

// `bun load.ts --actions FILE...`: only those panels' actions, as { FILE: [...] } (the shell's dock and its controls,
// which void lays out itself: apps/void/src/shell_ui.rs)
if (process.argv[2] === "--actions") {
  const all: Record<string, unknown> = {};
  for (const f of process.argv.slice(3)) all[f] = await actionsOf(resolve(f));
  console.log(JSON.stringify(all));
  process.exit(0);
}

// A file's default export called once, bundled with the layout SDKs and the JSX runtime as this process's own modules
// (the same instances `kit` and `term` are), so the elements it returns can be told apart by their type.
async function elementsOf(file: string, what: string): Promise<El> {
  const tmp = mkdtempSync(join(tmpdir(), "raea-layout-"));
  const built = await Bun.build({
    entrypoints: [file],
    outdir: tmp,
    target: "bun",
    external: [spaceKit, terminalKit, join(here, "jsx.ts")],
    plugins: [{
      name: "raea-layout",
      setup(b) {
        b.onResolve({ filter: /^@raea\/(void|space)(\/layout)?$/ }, () => ({ path: spaceKit, external: true }));
        b.onResolve({ filter: /^@raea\/terminal(\/layout)?$/ }, () => ({ path: terminalKit, external: true }));
        b.onResolve({ filter: /^react\/jsx(-dev)?-runtime$/ }, () => ({ path: join(here, "jsx.ts"), external: true }));
      },
    }],
  }).catch((e) => fail(`${what}: ${e?.message ?? e}`));
  if (!built.success) fail(`${what}: ${built.logs.map((l) => l.message).join("; ")}`);
  const mod = await import(built.outputs[0].path).catch((e) => fail(`${what}: ${e?.message ?? e}`));
  rmSync(tmp, { recursive: true, force: true });
  if (typeof mod.default !== "function") fail(`${what}: its default export isn't a component`);
  return mod.default({});
}

// `bun load.ts --terminal FOLDER`: a terminal's layout, { name, root }, root a tree of { split: "row" | "column",
// sizes, kids } and { controller, name }: a controller by its slug, or a path made absolute (void finds the slug's
// control.tsx: shell_ui.rs)
if (process.argv[2] === "--terminal") {
  const folder = resolve(process.argv[3] ?? ".");
  const file = join(folder, "terminal.tsx");
  if (!existsSync(file)) fail(`no terminal.tsx in ${folder}`);
  const root = await elementsOf(file, "terminal.tsx");
  if (!root || root.type !== term.Terminal) fail("terminal.tsx: its component must return a <Terminal>");
  const node = (n: unknown): any => {
    if (n == null || typeof n === "boolean") return null;
    if (Array.isArray(n)) {
      const kids = n.map(node).filter(Boolean);
      return kids.length === 1 ? kids[0] : kids.length ? { split: "row", sizes: kids.map(() => 1), kids } : null;
    }
    const e = n as El;
    if (e.type === Fragment) return node(e.props.children);
    const p = e.props;
    if (e.type === term.Controller) {
      if (!p.src) fail("terminal.tsx: a <Controller> needs a src");
      const local = p.src.startsWith(".") || p.src.startsWith("/");
      const src = local ? resolve(folder, p.src) : p.src;
      if (local && !existsSync(join(src, "control.tsx"))) fail(`Controller src ${p.src}: no control.tsx there`);
      return { controller: src, name: p.name ?? null };
    }
    if (e.type === term.Split || e.type === term.Stack) {
      const kids = [p.children].flat(Infinity).map(node).filter(Boolean);
      const sizes = kids.map((_, i) => Math.max(0.01, Number(p.sizes?.[i] ?? 1)));
      return { split: e.type === term.Stack ? "column" : p.dir ?? "row", sizes, kids };
    }
    // the terminal's own component grouping controllers: called, and what it returns read
    if (typeof e.type === "function") return node((e.type as (p: unknown) => unknown)(p));
    return null;
  };
  const tree = node(root.props.children);
  if (!tree) fail("terminal.tsx: a <Terminal> needs a <Controller>");
  console.log(JSON.stringify({ name: root.props.name ?? null, root: tree }));
  process.exit(0);
}

const folder = resolve(process.argv[2] ?? ".");
const file = join(folder, "space.tsx");
if (!existsSync(file)) fail(`no space.tsx in ${folder}`);
const root = await elementsOf(file, "space.tsx");
if (!root || root.type !== kit.Space) fail("space.tsx: its component must return a <Space>");

const out = { name: root.props.name ?? null, agent: root.props.agent ?? {}, regions: [] as any[] };
const walk = (n: unknown) => {
  if (n == null || typeof n === "boolean") return;
  if (Array.isArray(n)) return n.forEach(walk);
  const e = n as El;
  if (e.type === Fragment) return walk(e.props.children);
  if (typeof e.type === "function" && ![kit.Stage, kit.Panel, kit.Toolbar, kit.Chat, kit.Sheet].includes(e.type as any)) {
    // the space's own component grouping regions: called, and what it returns walked
    return walk((e.type as (p: unknown) => unknown)(e.props));
  }
  const p = e.props;
  const src = (s: string) => {
    const abs = resolve(folder, s);
    if (!existsSync(abs)) fail(`${e.type === kit.Panel ? "Panel" : e.type === kit.Chat ? "Chat" : e.type === kit.Sheet ? "Sheet" : "Toolbar"} src ${s}: no such file`);
    return abs;
  };
  if (e.type === kit.Stage) out.regions.push({ kind: "stage" });
  else if (e.type === kit.Panel) out.regions.push({ kind: "panel", src: src(p.src), side: p.side ?? "left", size: p.size ?? 260, name: p.name ?? null });
  else if (e.type === kit.Toolbar) out.regions.push({ kind: "panel", src: src(p.src), side: p.side ?? "top", size: p.size ?? 44, name: p.name ?? null });
  else if (e.type === kit.Chat && p.src) out.regions.push({ kind: "panel", src: src(p.src), side: p.side ?? "right", size: p.size ?? 380, name: p.name ?? "terminal", terminal: true });
  else if (e.type === kit.Chat) out.regions.push({ kind: "terminal", side: p.side ?? null });
  else if (e.type === kit.Sheet) out.regions.push({ kind: "panel", src: src(p.src), side: "sheet", size: p.width ?? 420, height: p.height ?? 480, name: p.name ?? null });
};
walk(root.props.children);

// void's shell around every space (apps/void/shell.feature): the header, the files on the left and the chat on the right,
// after the space's own panels; a workspace's shell/NAME.tsx stands in for void's of the same name
const workspace = process.argv[3];
const shell = (name: string) => {
  const own = workspace ? join(resolve(workspace), "shell", `${name}.tsx`) : "";
  return own && existsSync(own) ? own : join(here, "..", "shell", `${name}.tsx`);
};
if (!process.env.RAEA_VOID_NO_SHELL) {
  out.regions.push({ kind: "panel", src: shell("header"), side: "top", size: 40, name: "shell-header", shell: true });
  out.regions.push({ kind: "panel", src: shell("files"), side: "left", size: 260, name: "shell-files", shell: true });
  out.regions.push({ kind: "panel", src: shell("chat"), side: "right", size: 380, name: "shell-chat", shell: true, terminal: !out.regions.some((r) => r.terminal) });
}

for (const r of out.regions) if (r.kind === "panel") r.actions = await actionsOf(r.src);
if (!out.regions.some((r) => r.kind === "stage")) fail("space.tsx: a <Space> needs a <Stage/>");
console.log(JSON.stringify(out));
