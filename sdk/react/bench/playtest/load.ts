// Loads a game's edit.tsx headlessly in Bun (JavaScriptCore) and returns its `playtest` export (sdk/motion/src/games.ts).
// @raea/motion resolves to the SDK's source and React to the SDK's copy, as the engine's bundle points them; nothing
// renders: the bot only calls the pure fold. Step arithmetic is IEEE doubles in any engine, and the engine's render of
// the bot's input log is the check that the two agree (run.ts --render).
import path from "node:path";
import os from "node:os";
import fs from "node:fs";
import type { Playtest } from "../../../motion/src/games";

const MOTION = path.resolve(import.meta.dir, "../../../motion");
const alias: import("bun").BunPlugin = {
  name: "raea-motion",
  setup(b) {
    b.onResolve({ filter: /^@raea\/motion$/ }, () => ({ path: path.join(MOTION, "src/index.tsx") }));
    b.onResolve({ filter: /^@raea\/motion\/(.+)$/ }, (a) => {
      const base = path.join(MOTION, "src", a.path.slice("@raea/motion/".length));
      return { path: fs.existsSync(base + ".ts") ? base + ".ts" : base + ".tsx" };
    });
    b.onResolve({ filter: /^react(-dom)?(\/.*)?$/ }, (a) => ({ path: require.resolve(a.path, { paths: [MOTION] }) }));
  },
};

/** A TypeScript module bundled with @raea/motion resolved to the SDK's source, then imported. */
export async function loadModule(file: string): Promise<Record<string, any>> {
  const out = fs.mkdtempSync(path.join(os.tmpdir(), "raea-playtest-"));
  const built = await Bun.build({ entrypoints: [path.resolve(file)], outdir: out, target: "bun", format: "esm", plugins: [alias] });
  if (!built.success) throw new Error(`${file}: bun couldn't bundle it: ${built.logs.map(String).join(" | ")}`);
  const module = await import(built.outputs[0].path);
  fs.rmSync(out, { recursive: true, force: true });
  return module;
}

/** The game's frame rate, from its edit: `--fps N` on the command line, else the Composition's `fps={N}` (or
 *  `fps={NAME}` with `const NAME = N` in the file), else 120 (LEDGE's, which the bench was written for). The bots' holds
 *  are in frames either way; this sets seconds in the report and how fast a view's clip plays. */
export function fpsOf(edit: string, argv: string[] = process.argv): number {
  const i = argv.indexOf("--fps");
  if (i > 0 && Number(argv[i + 1]) > 0) return Number(argv[i + 1]);
  const text = fs.existsSync(edit) ? fs.readFileSync(edit, "utf8") : "";
  const lit = text.match(/fps=\{\s*(\d+)\s*\}/);
  if (lit) return Number(lit[1]);
  const named = text.match(/fps=\{\s*([A-Za-z_]\w*)\s*\}/);
  if (!named) return 120;
  const decl = new RegExp(`const\\s+${named[1]}\\s*(?::\\s*number\\s*)?=\\s*(\\d+)`);
  // the constant in the edit, or in a module beside it that the edit imports it from (an agent's sim.ts)
  const files = [text, ...fs.readdirSync(path.dirname(edit)).filter((f) => /\.(ts|tsx)$/.test(f)).map((f) => fs.readFileSync(path.join(path.dirname(edit), f), "utf8"))];
  for (const f of files) {
    const value = f.match(decl);
    if (value) return Number(value[1]);
  }
  return 120;
}

export async function loadGame(edit: string): Promise<{ playtest: Playtest<any, any>; module: Record<string, unknown> }> {
  const module = await loadModule(edit);
  const playtest = module.playtest as Playtest<any, any> | undefined;
  if (!playtest || typeof playtest.step !== "function") {
    throw new Error(`${edit}: no \`export const playtest\` (sdk/motion/src/games.ts Playtest); the pixel fallback isn't built yet (sdk/react/docs/playtest.md)`);
  }
  return { playtest, module };
}
