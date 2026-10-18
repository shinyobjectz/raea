// The shell for a host outside raea's V8 (void's agent, in Rust): one Shell kept for a space while the host sends
// JSON lines on stdin, one reply line on stdout per request, so cwd, env, the browser's page and its refs survive
// between calls and V8 and Chrome start once.
//   request:  {"id", "op": "exec" | "undo" | "redo" | "log", "cmd"?, "timeout_ms"?, "last"?}
//   reply:    {"id", "stdout", "stderr", "exitCode", "row"}  (undo/redo: "row"; log: "rows"; a bad request: "error")
// While a command runs, `raea` may ask the host: {"host": {"op", "args"}} on stdout, answered by one line on stdin,
// {"host_reply": VALUE} or {"host_error": "..."}, before the command's own reply.
// stdout and stderr are cut at 64 KB each, saying so: the model's context is the limit.
import { native } from "@raea/motion/native";
import { Shell, type ShellOptions } from "./index";

const CAP = 64 * 1024;
const cut = (s: string) => (s.length > CAP ? s.slice(0, CAP) + `\n[cut: ${s.length - CAP} more characters; write the output to a file and read it in parts]\n` : s);

/** An AbortSignal that aborts itself once its deadline has passed, checked whenever the interpreter looks at it (no
 * timer needed while a command runs). */
function deadline(ms: number): AbortSignal {
  const sig = new AbortController().signal as AbortSignal & { _abort?: (r: unknown) => void };
  const end = Date.now() + ms;
  let real = false;
  Object.defineProperty(sig, "aborted", {
    configurable: true,
    get() {
      if (!real && Date.now() > end) {
        real = true;
        Object.defineProperty(sig, "aborted", { value: false, writable: true, configurable: true });
        sig._abort?.(new DOMException(`timed out after ${ms} ms`, "TimeoutError"));
      }
      return real;
    },
  });
  return sig;
}

export interface Request {
  id?: unknown;
  op?: string;
  cmd?: string;
  timeout_ms?: number;
  last?: number;
}

export async function handle(sh: Shell, req: Request): Promise<Record<string, unknown>> {
  const id = req.id ?? null;
  switch (req.op ?? "exec") {
    case "exec": {
      if (typeof req.cmd !== "string") return { id, error: "exec needs cmd" };
      const t0 = Date.now();
      const ms = req.timeout_ms && req.timeout_ms > 0 ? req.timeout_ms : 0;
      try {
        const r = await sh.exec(req.cmd, ms ? { signal: deadline(ms) } : undefined);
        const timedOut = ms > 0 && Date.now() - t0 > ms;
        return { id, stdout: cut(r.stdout), stderr: cut(r.stderr + (timedOut ? `\n[stopped after ${ms} ms]\n` : "")), exitCode: timedOut ? 124 : r.exitCode, row: r.row };
      } catch (e) {
        const timedOut = ms > 0 && Date.now() - t0 > ms;
        return { id, stdout: "", stderr: cut(String((e as Error).message ?? e) + "\n"), exitCode: timedOut ? 124 : 1, row: (e as { row?: unknown }).row ?? null };
      }
    }
    case "undo":
      return { id, row: sh.undo() };
    case "redo":
      return { id, row: sh.redo() };
    case "log":
      return { id, rows: sh.log(req.last) };
    default:
      return { id, error: `no op "${req.op}" (exec, undo, redo, log)` };
  }
}

/** The server: requests from stdin until it closes. */
export async function serve(opts: ShellOptions): Promise<void> {
  const write = (o: unknown) => native("host.write", { text: JSON.stringify(o) + "\n" });
  const host = (op: string, args: Record<string, unknown>) => {
    write({ host: { op, args } });
    for (;;) {
      const line = native("host.readline", {});
      if (line === null) throw new Error("the host went away");
      if (!line.trim()) continue;
      const v = JSON.parse(line);
      if (typeof v.host_error === "string") throw new Error(v.host_error);
      if ("host_reply" in v) return v.host_reply;
    }
  };
  const sh = new Shell({ ...opts, host });
  write({ ready: true, root: sh.fs.root });
  for (;;) {
    const line = native("host.readline", {});
    if (line === null) break;
    if (!line.trim()) continue;
    let req: Request;
    try {
      req = JSON.parse(line);
    } catch {
      write({ id: null, error: "not a JSON line" });
      continue;
    }
    write(await handle(sh, req));
  }
}

/** One command, one reply (the CLI). */
export async function once(opts: ShellOptions, cmd: string, timeout_ms?: number): Promise<void> {
  const sh = new Shell(opts);
  native("host.write", { text: JSON.stringify(await handle(sh, { id: 0, op: "exec", cmd, timeout_ms })) + "\n" });
}
