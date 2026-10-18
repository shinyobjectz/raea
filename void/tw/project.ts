// bun project.ts SPACE THEME.css: the CSS for a raea project (`raea render`): the brand kit's variables (SPACE/brand/
// brand.css, its @imports dropped: Blitz loads no remote CSS) then Tailwind 4.3.3 for every class any .tsx in the project
// uses, with the brand theme (cli/template/theme.css: class names onto --brand-* variables). Blitz honours an inline
// <style>, so the edit renders this as one (styles.gen.ts).
import { compile } from "tailwindcss";
import { existsSync, readdirSync, readFileSync, statSync } from "fs";
import { dirname, join } from "path";

const [space, themeFile] = process.argv.slice(2);
const files: string[] = [];
const walk = (d: string) => {
  for (const f of readdirSync(d)) {
    if (f.startsWith(".") || f === "node_modules" || f === "media") continue;
    const p = join(d, f);
    if (statSync(p).isDirectory()) walk(p);
    else if (f.endsWith(".tsx") || f.endsWith(".ts")) files.push(p);
  }
};
walk(space);
const candidates = new Set<string>();
for (const f of files) {
  const src = readFileSync(f, "utf8");
  for (const m of src.matchAll(/(["'`])((?:(?!\1)[^\\]|\\.)*)\1/g)) for (const c of m[2].split(/\s+/)) if (c) candidates.add(c);
}
const pkg = dirname(require.resolve("tailwindcss/package.json"));
const theme = readFileSync(themeFile, "utf8");
const c = await compile(`@import "tailwindcss";\n${theme}`, {
  base: pkg,
  loadStylesheet: async (id: string) => {
    const file = id === "tailwindcss" ? join(pkg, "index.css") : join(pkg, id.replace(/^tailwindcss\//, ""));
    return { path: file, base: dirname(file), content: readFileSync(file.endsWith(".css") ? file : file + ".css", "utf8") };
  },
});
const brandFile = join(space, "brand/brand.css");
const brand = existsSync(brandFile) ? readFileSync(brandFile, "utf8").split("\n").filter((l) => !l.trim().startsWith("@import")).join("\n") : "";
process.stdout.write(brand + "\n" + c.build([...candidates]));
