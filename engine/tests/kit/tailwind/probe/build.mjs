// T-080: the Tailwind v4 probe (body.html: 170 utilities a component kit leans on, in ten 180 px sections) compiled
// with Tailwind's own compiler into index.html. Re-run after changing body.html, then normalize the fonts as the
// registry sweep does and render Chrome's reference:
//   node build.mjs ~/the host app/admin/node_modules <this folder>
//   node ../../../../bench/hf-normalize-fonts.mjs ~/src/the composition format <this folder>
//   (cd framework && PARITY_ROOT=tests/kit bench/parity-refs.sh tailwind)
import { readFileSync, writeFileSync } from "node:fs";
const tw = await import(process.argv[2] + "/@tailwindcss/node/dist/index.mjs");
const dir = process.argv[3];
const body = readFileSync(dir + "/body.html", "utf8");
const candidates = [...new Set(body.match(/class="([^"]*)"/g).flatMap((m) => m.slice(7, -1).split(/\s+/)))];
const compiler = await tw.compile('@import "tailwindcss";', { base: process.argv[2], onDependency() {} });
const raw = compiler.build(candidates);
const opt = tw.optimize(raw, { minify: false }).code;
for (const [name, css] of [["", raw]]) {
  const sections = body.split("<section").length - 1;
  writeFileSync(`${dir}/index.html`, `<!doctype html><html><head><meta charset="utf-8"><style>${css}</style>
<style>html,body{margin:0;width:1280px;height:2000px;background:#fff} section{height:180px;overflow:hidden;box-sizing:border-box}</style></head>
<body><div id="root" data-composition-id="r" data-width="1280" data-height="2000" data-duration="1">${body}</div></body></html>`);
}
console.log(candidates.length, "candidates;", raw.length, "raw bytes;", opt.length, "optimized");
