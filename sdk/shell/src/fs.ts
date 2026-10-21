// The agent's project folder as just-bash's filesystem: every call goes to raea-native's fs.* (Rust), which keeps the
// shell inside the folder, records each command as one action row (files read and written, before and after, by
// content hash) and can undo and redo any command. Paths are the shell's ("/" is the project's root).
import type { BufferEncoding, CpOptions, DirentEntry, FileContent, FsStat, IFileSystem, MkdirOptions, ReadFileOptions, RmOptions, WriteFileOptions } from "just-bash/browser";
import { native as rawNative } from "@raea/motion/native";

/** A native fs call whose failure is a Node-style error ("ENOENT: no such file or directory, open '/x'", with .code),
 * as just-bash expects of a filesystem: then a refused write fails only its own command, as in bash. */
const native: typeof rawNative = ((name: string, args: unknown) => {
  try {
    return (rawNative as (n: string, a: unknown) => unknown)(name, args);
  } catch (e) {
    const msg = String((e as Error)?.message ?? e).replace(/^[a-z]+\.[a-z_]+: /, "");
    const err = new Error(msg) as Error & { code?: string };
    const code = /^([A-Z]+):/.exec(msg)?.[1];
    if (code) err.code = code;
    throw err;
  }
}) as typeof rawNative;

const B64 = (b: Uint8Array): string => {
  let s = "";
  for (let i = 0; i < b.length; i += 8192) s += String.fromCharCode.apply(null, Array.from(b.subarray(i, i + 8192)));
  return btoa(s);
};
const UNB64 = (s: string): Uint8Array => {
  const t = atob(s);
  const out = new Uint8Array(t.length);
  for (let i = 0; i < t.length; i++) out[i] = t.charCodeAt(i);
  return out;
};
const latin1 = (b: Uint8Array): string => {
  let s = "";
  for (let i = 0; i < b.length; i += 8192) s += String.fromCharCode.apply(null, Array.from(b.subarray(i, i + 8192)));
  return s;
};
const encName = (o?: ReadFileOptions | WriteFileOptions | BufferEncoding | null): BufferEncoding | null | undefined =>
  typeof o === "string" ? o : (o as { encoding?: BufferEncoding | null } | undefined)?.encoding;

function encode(content: FileContent, enc?: BufferEncoding | null): Uint8Array {
  if (content instanceof Uint8Array) return content;
  switch (enc) {
    case "base64":
      return UNB64(content);
    case "hex": {
      const out = new Uint8Array(content.length >> 1);
      for (let i = 0; i < out.length; i++) out[i] = parseInt(content.substr(i * 2, 2), 16);
      return out;
    }
    case "binary":
    case "latin1":
    case "ascii": {
      const out = new Uint8Array(content.length);
      for (let i = 0; i < content.length; i++) out[i] = content.charCodeAt(i) & 255;
      return out;
    }
    default:
      return new TextEncoder().encode(content);
  }
}
function decode(b: Uint8Array, enc?: BufferEncoding | null): string {
  switch (enc) {
    case "base64":
      return B64(b);
    case "hex":
      return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
    case "binary":
    case "latin1":
    case "ascii":
      return latin1(b);
    default:
      return new TextDecoder().decode(b);
  }
}
// dev and ino are the file's stable identity: just-bash's cp and mv compare them to tell two existing files apart (with
// neither, `mv a.txt b.txt` onto an existing b.txt refused as "cannot safely determine whether ... are the same file")
const toStat = (s: { isFile: boolean; isDirectory: boolean; isSymbolicLink: boolean; mode: number; size: number; mtimeMs: number; dev?: number; ino?: number }): FsStat => ({
  isFile: s.isFile,
  isDirectory: s.isDirectory,
  isSymbolicLink: s.isSymbolicLink,
  mode: s.mode,
  size: s.size,
  mtime: new Date(s.mtimeMs),
  ...(s.ino ? { dev: s.dev, ino: s.ino } : {}),
});
const join = (base: string, p: string): string => {
  const parts: string[] = [];
  for (const x of (p.startsWith("/") ? p : `${base}/${p}`).split("/")) {
    if (!x || x === ".") continue;
    if (x === "..") parts.pop();
    else parts.push(x);
  }
  return "/" + parts.join("/");
};

const DEVICES = new Set(["/dev/null", "/dev/stdin", "/dev/stdout", "/dev/stderr", "/dev/tty"]);
const DEVICE_STAT = (): FsStat => ({ isFile: true, isDirectory: false, isSymbolicLink: false, mode: 0o666, size: 0, mtime: new Date(0) });

export interface RaeaFsOptions {
  /** the project folder (made if missing) */
  root: string;
}

/** A project folder as a just-bash filesystem, every change recorded and undoable. */
export class RaeaFs implements IFileSystem {
  readonly id: number;
  readonly root: string;
  constructor(opts: RaeaFsOptions) {
    const o = native("fs.open", { root: opts.root });
    this.id = o.id;
    this.root = o.root;
  }
  // ---- commands and history
  /** A command starts: what the filesystem does until end() is its action row. */
  begin(cmd: string): void {
    native("fs.begin", { id: this.id, cmd });
  }
  /** The command ends; its action row. */
  end(exit = 0, error?: string): Record<string, unknown> | null {
    return native("fs.end", error === undefined ? { id: this.id, exit } : { id: this.id, exit, error });
  }
  /** The last command's changes taken back; its row, or null when there's nothing to undo. */
  undo(): Record<string, unknown> | null {
    return native("fs.undo", { id: this.id });
  }
  redo(): Record<string, unknown> | null {
    return native("fs.redo", { id: this.id });
  }
  /** Keeps a path as it is now in the open command's row, before something outside this filesystem changes it (a
   * host op writing files: `raea restore`), so the row has its before and after and undo takes it back. */
  touch(path: string): void {
    native("fs.touch", { id: this.id, path });
  }
  /** The action rows (the last n). */
  log(last?: number): Record<string, unknown>[] {
    return native("fs.log", last === undefined ? { id: this.id } : { id: this.id, last });
  }
  // ---- IFileSystem (/dev/null and /dev/std* are the shell's devices, not project files: written to, nothing is kept)
  async readFile(path: string, options?: ReadFileOptions | BufferEncoding): Promise<string> {
    return decode(await this.readFileBuffer(path), encName(options) ?? "utf8");
  }
  async readFileBuffer(path: string): Promise<Uint8Array> {
    if (DEVICES.has(join("/", path))) return new Uint8Array(0);
    return UNB64(native("fs.read", { id: this.id, path }));
  }
  async writeFile(path: string, content: FileContent, options?: WriteFileOptions | BufferEncoding): Promise<void> {
    if (DEVICES.has(join("/", path))) return;
    native("fs.write", { id: this.id, path, data: B64(encode(content, encName(options))) });
  }
  async appendFile(path: string, content: FileContent, options?: WriteFileOptions | BufferEncoding): Promise<void> {
    if (DEVICES.has(join("/", path))) return;
    native("fs.write", { id: this.id, path, data: B64(encode(content, encName(options))), append: true });
  }
  async exists(path: string): Promise<boolean> {
    if (DEVICES.has(join("/", path)) || join("/", path) === "/dev") return true;
    return native("fs.exists", { id: this.id, path });
  }
  async stat(path: string): Promise<FsStat> {
    if (DEVICES.has(join("/", path))) return DEVICE_STAT();
    return toStat(native("fs.stat", { id: this.id, path }));
  }
  async lstat(path: string): Promise<FsStat> {
    if (DEVICES.has(join("/", path))) return DEVICE_STAT();
    return toStat(native("fs.lstat", { id: this.id, path }));
  }
  async mkdir(path: string, options?: MkdirOptions): Promise<void> {
    native("fs.mkdir", { id: this.id, path, recursive: !!options?.recursive });
  }
  async createExclusive(path: string, options: { mode: number; directory?: boolean }): Promise<void> {
    if (options.directory) native("fs.mkdir", { id: this.id, path, exclusive: true, mode: options.mode });
    else native("fs.write", { id: this.id, path, data: "", exclusive: true, mode: options.mode });
  }
  async readdir(path: string): Promise<string[]> {
    return native("fs.readdir", { id: this.id, path }).map((e) => e.name);
  }
  async readdirWithFileTypes(path: string): Promise<DirentEntry[]> {
    return native("fs.readdir", { id: this.id, path });
  }
  async rm(path: string, options?: RmOptions): Promise<void> {
    native("fs.rm", { id: this.id, path, recursive: !!options?.recursive, force: !!options?.force });
  }
  async cp(src: string, dest: string, options?: CpOptions): Promise<void> {
    native("fs.cp", { id: this.id, src, dest, recursive: !!options?.recursive });
  }
  async mv(src: string, dest: string): Promise<void> {
    native("fs.mv", { id: this.id, src, dest });
  }
  resolvePath(base: string, path: string): string {
    return join(base, path);
  }
  getAllPaths(): string[] {
    return native("fs.paths", { id: this.id });
  }
  async chmod(path: string, mode: number): Promise<void> {
    native("fs.chmod", { id: this.id, path, mode });
  }
  async symlink(target: string, linkPath: string): Promise<void> {
    native("fs.symlink", { id: this.id, target, path: linkPath });
  }
  async link(existingPath: string, newPath: string): Promise<void> {
    native("fs.link", { id: this.id, existing: existingPath, path: newPath });
  }
  async readlink(path: string): Promise<string> {
    return native("fs.readlink", { id: this.id, path });
  }
  async realpath(path: string): Promise<string> {
    return native("fs.realpath", { id: this.id, path });
  }
  async utimes(path: string, _atime: Date, mtime: Date): Promise<void> {
    native("fs.utimes", { id: this.id, path, mtimeMs: mtime.getTime() });
  }
}
