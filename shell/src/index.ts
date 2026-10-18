// @raea/shell: the agent's shell. just-bash (Apache-2.0, used unmodified as a dependency) runs the bash: parsing, pipes,
// redirections, functions and its text tools, in TypeScript on raea's V8. Under it, the filesystem is the project
// folder through raea-native (RaeaFs: nothing outside the folder, every command an action row, any command undone or
// redone), and raea's own tools (rdb, raea-motion, solid, ecs, media) are commands that call the engine natively.
import { Bash, defineCommand, getCommandNames, getNetworkCommandNames, type CustomCommand } from "just-bash/browser";
import { raeaHandler, type Host } from "../../cli/src/index";
import { nativeFetch, raeaCommands, wasiCommands } from "./commands";
import { native } from "@raea/motion/native";
import { RaeaFs } from "./fs";

export { RaeaFs } from "./fs";
export { nativeFetch, raeaCommands, vpath, wasiCommands } from "./commands";

export interface ShellOptions {
  /** the project folder: the shell's "/" */
  root: string;
  /** the starting folder in the shell (default "/") */
  cwd?: string;
  env?: Record<string, string>;
  /** more commands, beside raea's */
  commands?: CustomCommand[];
  /** the host's ops for `raea` (@raea/cli): perception, renders, checks; void answers them over shell-serve */
  host?: Host;
  /** the network curl may reach (none when absent): URL prefixes and methods, and headers injected at the boundary;
   * a secret is named by the host's environment variable (secretHeaders), so its value never enters JavaScript */
  network?: { allow: { prefix: string; methods?: string[]; headers?: Record<string, string>; secretHeaders?: Record<string, string> }[]; timeoutMs?: number; maxRedirects?: number; maxBytes?: number };
}

export interface ShellResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  /** the command's action row (null when the filesystem wasn't involved at all) */
  row: Record<string, unknown> | null;
}

/** One agent's shell over one project folder. */
export class Shell {
  readonly fs: RaeaFs;
  readonly bash: Bash;
  constructor(opts: ShellOptions) {
    this.fs = new RaeaFs({ root: opts.root });
    const own = [...raeaCommands(this.fs), defineCommand("raea", raeaHandler(opts.host) as never), ...(opts.commands ?? [])];
    // just-bash's own command names, so a WASI tool never shadows one
    const builtin = new Set<string>([...getCommandNames(), ...getNetworkCommandNames()]);
    const taken = new Set<string>([...builtin, ...own.map((c) => c.name)]);
    this.bash = new Bash({
      fs: this.fs,
      cwd: opts.cwd ?? "/",
      env: { HOME: "/", PWD: opts.cwd ?? "/", ...(opts.env ?? {}) },
      customCommands: [...own, ...wasiCommands(this.fs, taken)],
      ...(opts.network ? { fetch: nativeFetch(native("net.policy", opts.network).id) as never } : {}),
    });
  }
  /** the folder and exported variables a command left, which the next command starts from */
  private state: { cwd: string; env: Record<string, string> } | null = null;
  /** Runs a command line; what it did to the project is one action row. Like one terminal, a `cd` or an `export`
   * carries over to the next command (just-bash resets them per exec; this shell keeps them). */
  async exec(cmd: string, opts?: { stdin?: string; cwd?: string; env?: Record<string, string>; signal?: AbortSignal }): Promise<ShellResult> {
    this.fs.begin(cmd);
    let r: { stdout: string; stderr: string; exitCode: number; env?: Record<string, string> };
    try {
      const carried = this.state ? { cwd: this.state.cwd, env: this.state.env, replaceEnv: true } : {};
      r = await this.bash.exec(cmd, { ...carried, ...(opts ?? {}), env: { ...(this.state?.env ?? {}), ...(opts?.env ?? {}) } } as never);
      if (r.env) this.state = { cwd: r.env.PWD || this.state?.cwd || "/", env: r.env };
    } catch (e) {
      // an error the interpreter didn't turn into stderr (a refused write through a link it can only read): the
      // command fails with it, as a shell would say it
      const msg = String((e as Error)?.message ?? e).replace(/^fs\.[a-z_]+: /, "");
      const row = this.fs.end(1, msg);
      return { stdout: "", stderr: `bash: ${msg}\n`, exitCode: 1, row };
    }
    const row = this.fs.end(r.exitCode, r.exitCode ? r.stderr.split("\n")[0] || undefined : undefined);
    return { stdout: r.stdout, stderr: r.stderr, exitCode: r.exitCode, row };
  }
  /** The last command that changed the project, taken back; its row (null when there's none). */
  undo() {
    return this.fs.undo();
  }
  /** The last undone command made again. */
  redo() {
    return this.fs.redo();
  }
  /** The action rows (the last n). */
  log(last?: number) {
    return this.fs.log(last);
  }
}
