// node:path on raea's V8: the POSIX rules (raea's paths are POSIX on the Mac and the iPhone), written out here.
const norm = (p: string, abs: boolean): string => {
  const out: string[] = [];
  for (const s of p.split("/")) {
    if (!s || s === ".") continue;
    if (s === "..") {
      if (out.length && out[out.length - 1] !== "..") out.pop();
      else if (!abs) out.push("..");
    } else out.push(s);
  }
  return out.join("/");
};
export const sep = "/";
export const delimiter = ":";
export function normalize(p: string): string {
  if (p === "") return ".";
  const abs = p.startsWith("/");
  const trail = p.endsWith("/");
  let r = norm(p, abs);
  if (!r && !abs) r = ".";
  if (r && trail) r += "/";
  return (abs ? "/" : "") + r;
}
export function join(...parts: string[]): string {
  const j = parts.filter((x) => x !== "").join("/");
  return j ? normalize(j) : ".";
}
export function resolve(...parts: string[]): string {
  let r = "";
  for (let i = parts.length - 1; i >= 0 && !r.startsWith("/"); i--) if (parts[i]) r = parts[i] + (r ? "/" + r : "");
  if (!r.startsWith("/")) r = "/" + r;
  return "/" + norm(r, true);
}
export const isAbsolute = (p: string) => p.startsWith("/");
export function dirname(p: string): string {
  if (!p) return ".";
  const t = p.replace(/\/+$/, "") || "/";
  const i = t.lastIndexOf("/");
  return i < 0 ? "." : i === 0 ? "/" : t.slice(0, i);
}
export function basename(p: string, ext?: string): string {
  const t = p.replace(/\/+$/, "");
  let b = t.slice(t.lastIndexOf("/") + 1);
  if (ext && b.endsWith(ext) && b !== ext) b = b.slice(0, -ext.length);
  return b;
}
export function extname(p: string): string {
  const b = basename(p);
  const i = b.lastIndexOf(".");
  return i <= 0 ? "" : b.slice(i);
}
export function relative(from: string, to: string): string {
  const a = resolve(from).split("/").filter(Boolean), b = resolve(to).split("/").filter(Boolean);
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  return [...a.slice(i).map(() => ".."), ...b.slice(i)].join("/");
}
export function parse(p: string) {
  const root = p.startsWith("/") ? "/" : "";
  const base = basename(p);
  const ext = extname(p);
  return { root, dir: dirname(p) === "." && !p.includes("/") ? "" : dirname(p), base, ext, name: ext ? base.slice(0, -ext.length) : base };
}
export function format(o: { root?: string; dir?: string; base?: string; name?: string; ext?: string }): string {
  const base = o.base ?? (o.name ?? "") + (o.ext ?? "");
  return o.dir ? (o.dir === o.root ? o.dir + base : o.dir + "/" + base) : (o.root ?? "") + base;
}
export const toNamespacedPath = (p: string) => p;
const path = { sep, delimiter, normalize, join, resolve, isAbsolute, dirname, basename, extname, relative, parse, format, toNamespacedPath } as Record<string, unknown>;
path.posix = path;
path.win32 = path;
export const posix = path;
export const win32 = path;
export default path;
