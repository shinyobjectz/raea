// Loads a game's edit.tsx headlessly in Bun (JavaScriptCore) and returns its `playtest` export (motion/src/games.ts).
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

export async function loadGame(edit: string): Promise<{ playtest: Playtest<any, any>; module: Record<string, unknown> }> {
  const module = await loadModule(edit);
  const playtest = module.playtest as Playtest<any, any> | undefined;
  if (!playtest || typeof playtest.step !== "function") {
    throw new Error(`${edit}: no \`export const playtest\` (motion/src/games.ts Playtest); the pixel fallback isn't built yet (react/docs/playtest.md)`);
  }
  return { playtest, module };
}
