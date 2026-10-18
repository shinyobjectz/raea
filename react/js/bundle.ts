// raea's bundler for its V8 (run by raea-motion through bun): one IIFE from an entry, with the engine's resolution on
// top of bun's own. `@raea/motion` (and its subpaths) is the SDK's source in motion/src; `bun:test`, `vitest` and
// `@jest/globals` are the engine's test API (shims/test.ts); a bare import a package's own folder can't satisfy falls
// back to raea-react's node_modules (react, mediabunny, bevy-react), so a script anywhere can use them. Node's built-in
// modules aren't in the engine: those raea carries natively (shims/node: zlib) resolve there, any other fails the bundle
// with the importer named, unless `--node-stub` lists it.
//   bun bundle.ts ENTRY OUT [--define K=V ...] [--alias SPEC=PATH ...] [--node-stub fs,path]
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const [entry, out, ...rest] = process.argv.slice(2);
if (!entry || !out) {
  console.error("bun bundle.ts ENTRY OUT [--define K=V ...] [--node-stub a,b]");
  process.exit(2);
}
const define: Record<string, string> = { "process.env.NODE_ENV": '"production"' };
let stubs = new Set<string>();
const aliases: Record<string, string> = {};
for (let i = 0; i < rest.length; i++) {
  if (rest[i] === "--define") {
    const [k, ...v] = rest[++i].split("=");
    define[k] = v.join("=");
  } else if (rest[i] === "--node-stub") for (const n of rest[++i].split(",").filter(Boolean)) stubs.add(n);
  else if (rest[i] === "--alias") {
    const [k, ...v] = rest[++i].split("=");
    aliases[k] = resolve(v.join("="));
  }
}
const here = import.meta.dir;
const motion = resolve(here, "../../motion/src");
const SUBPATHS: Record<string, string> = { "": "index.tsx", test: "test.tsx", "3d": "3d.tsx", ecs: "ecs.ts", native: "native.ts", codecs: "codecs.ts" };
const testShim = join(here, "shims/test.ts");
const fallback = join(here, "node_modules");
const SINGLETONS = new Set(["react", "react-dom", "react-reconciler", "scheduler", "mediabunny", "bevy-react"]);
const NODE = new Set(["assert", "buffer", "child_process", "crypto", "events", "fs", "fs/promises", "os", "path", "process", "stream", "url", "util", "worker_threads", "zlib", "module", "net", "http", "https", "tty", "readline", "string_decoder", "perf_hooks", "async_hooks", "vm"]);

const result = await Bun.build({
  entrypoints: [resolve(entry)],
  format: "iife",
  target: "browser",
  minify: { syntax: true },
  define,
  plugins: [
    {
      name: "raea",
      setup(build) {
        build.onResolve({ filter: /.*/ }, (a) => (aliases[a.path] ? { path: aliases[a.path] } : undefined));
        build.onResolve({ filter: /^(bun:test|vitest|@jest\/globals)$/ }, () => ({ path: testShim }));
        build.onResolve({ filter: /^@raea\/(browser|shell)$/ }, (a) => ({ path: a.path === "@raea/browser" ? join(here, "../../browser/ts/index.ts") : join(here, "../../shell/src/index.ts") }));
        build.onResolve({ filter: /^@raea\/motion(\/.*)?$/ }, (a) => {
          const sub = a.path.slice("@raea/motion".length).replace(/^\//, "");
          const f = SUBPATHS[sub];
          if (!f) throw new Error(`@raea/motion has no subpath "${sub}" (imported by ${a.importer})`);
          return { path: join(motion, f) };
        });
        build.onResolve({ filter: /^(node:)?[a-z_/]+$/ }, (a) => {
          const name = a.path.replace(/^node:/, "");
          if (!a.path.startsWith("node:") && !NODE.has(name)) return undefined;
          if (!NODE.has(name)) return undefined;
          if (existsSync(join(here, "shims/node", `${name}.ts`))) return { path: join(here, "shims/node", `${name}.ts`) };
          if (stubs.has(name)) return { path: name, namespace: "raea-node-stub" };
          throw new Error(`"${a.path}" is a Node built-in, which raea's V8 doesn't have (imported by ${a.importer}); use the package's browser build, or --node-stub ${name}`);
        });
        build.onLoad({ filter: /.*/, namespace: "raea-node-stub" }, (a) => ({
          contents: `const m = new Proxy({}, { get: (_t, k) => k === "__esModule" ? true : k === "default" ? m : () => { throw new Error("node:${a.path}." + String(k) + " isn't available in raea's V8"); } }); export default m;`,
          loader: "js",
        }));
        // packages the engine itself carries (react, mediabunny, bevy-react) resolve to its copies from anywhere, so
        // a bundle has one of each (two Reacts break hooks; two mediabunnys break instanceof between them); other bare
        // imports resolve from the importer's own folder, then fall back to the engine's node_modules
        build.onResolve({ filter: /^[@a-z]/ }, (a) => {
          if (!a.importer || a.path.startsWith("@raea/")) return undefined;
          const pkg = a.path.startsWith("@") ? a.path.split("/").slice(0, 2).join("/") : a.path.split("/")[0];
          if (SINGLETONS.has(pkg) && existsSync(join(fallback, pkg))) return { path: Bun.resolveSync(a.path, fallback) };
          let d = dirname(a.importer);
          while (true) {
            if (existsSync(join(d, "node_modules", pkg))) return undefined;
            const up = dirname(d);
            if (up === d) break;
            d = up;
          }
          if (existsSync(join(fallback, pkg))) return { path: Bun.resolveSync(a.path, fallback) };
          return undefined;
        });
      },
    },
  ],
});
if (!result.success) {
  for (const l of result.logs.slice(0, 8)) console.error(String(l.message ?? l));
  process.exit(1);
}
await Bun.write(out, await result.outputs[0].text());
