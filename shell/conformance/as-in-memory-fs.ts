// RaeaFs standing in for just-bash's InMemoryFs, so upstream tests written against it (the IFileSystem contract) run
// on the project-folder filesystem: each instance is a fresh folder under TMPDIR, filled from the same initial files.
import { native } from "@raea/motion/native";
import { RaeaFs } from "../src/fs";

let n = 0;
export class InMemoryFs extends RaeaFs {
  constructor(initial?: Record<string, string | Uint8Array | { content: string | Uint8Array }>) {
    const tmp = (native("host.env", { name: "TMPDIR" }) as string | null) ?? "/tmp";
    super({ root: `${tmp}/raea-shell-contract-${Date.now()}-${++n}-${Math.random().toString(36).slice(2)}` });
    for (const [p, c] of Object.entries(initial ?? {})) {
      const content = typeof c === "string" || c instanceof Uint8Array ? c : c.content;
      const dir = p.slice(0, p.lastIndexOf("/")) || "/";
      native("fs.mkdir", { id: this.id, path: dir, recursive: true });
      void this.writeFile(p, content);
    }
  }
}
