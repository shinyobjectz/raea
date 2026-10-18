// bun tw.ts COMP.tsx: the CSS for the Tailwind classes a component uses (Tailwind 4.3.3, the host app's kit's version), for
// void's `component` tool to put in a <style> beside it (raea-react's Blitz honours inline <style>, not <link>).
// Class candidates are every whitespace-separated token in the file's string literals; Tailwind ignores the rest.
import { compile } from "tailwindcss";
import { readFileSync } from "fs";
import { dirname, join } from "path";
const src = readFileSync(process.argv[2], "utf8");
const candidates = new Set<string>();
for (const m of src.matchAll(/(["'`])((?:(?!\1)[^\\]|\\.)*)\1/g)) for (const c of m[2].split(/\s+/)) if (c) candidates.add(c);
const pkg = dirname(require.resolve("tailwindcss/package.json"));
const c = await compile('@import "tailwindcss"; @theme { --font-sans: "Inter", sans-serif; }', {
  base: pkg,
  loadStylesheet: async (id: string, base: string) => {
    const file = id === "tailwindcss" ? join(pkg, "index.css") : join(pkg, id.replace(/^tailwindcss\//, ""));
    return { path: file, base: dirname(file), content: readFileSync(file.endsWith(".css") ? file : file + ".css", "utf8") };
  },
});
process.stdout.write(c.build([...candidates]));
