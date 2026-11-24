// The raea app's host, as the native `raea` reaches it (native.ts): where the running app listens, and the verbs that
// post to it: `raea show` (and its --on tasks), `raea library`, and the agent harness a show names as its poster. On a
// Mac, Windows or Linux (#169: on Windows `raea show` never found the running app, since it asked `kill`, and said it
// had opened the files "in the Mac's own viewer", which it hadn't).
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { type Despite, held } from "./held";
import { HARNESSES } from "./show";

/** What these verbs ask of the computer they run on (a test gives its own: conformance/app.test.ts). */
export interface Here {
  platform: string;
  run: (cmd: string, args: string[]) => { status: number | null; stdout: string };
  exists: (path: string) => boolean;
  read: (path: string) => string;
  /** a folder's names: the project's record, which a show reads first (held.ts) */
  list?: (dir: string) => string[];
  env: Record<string, string | undefined>;
  home: string;
}

export const HERE: Here = {
  platform: process.platform,
  run: (cmd, args) => {
    const r = spawnSync(cmd, args, { encoding: "utf8" });
    return { status: r.status, stdout: r.stdout ?? "" };
  },
  exists: existsSync,
  read: (p) => readFileSync(p, "utf8"),
  list: (d) => readdirSync(d),
  env: process.env,
  home: homedir(),
};

/** This computer as raea names it, and the viewer files open in without the app. */
export function computer(h: Here = HERE): { name: string; viewer: string } {
  if (h.platform === "darwin") return { name: "this Mac", viewer: "the Mac's own viewer" };
  if (h.platform === "win32") return { name: "this PC", viewer: "Windows' own viewer" };
  return { name: "this computer", viewer: "the desktop's own viewer" };
}

/** Whether process `pid` is running: `kill -0`, or on Windows (which has no kill) tasklist. */
function alive(pid: number, h: Here): boolean {
  if (h.platform !== "win32") return h.run("kill", ["-0", String(pid)]).status === 0;
  return h.run("tasklist", ["/FI", `PID eq ${pid}`, "/NH", "/FO", "CSV"]).stdout.includes(`"${pid}"`);
}

/** The running raea app's host: RAEA_FEED_HOST for an agent the app started, else the address the installed app leaves
 *  in ~/.raea/app-host.json while it runs (apps/app host/host-file.ts), if its process is still there. */
export function appHost(h: Here = HERE): string | null {
  if (h.env.RAEA_FEED_HOST) return h.env.RAEA_FEED_HOST;
  const file = h.env.RAEA_HOST_FILE || `${h.home}/.raea/app-host.json`;
  try {
    const x = JSON.parse(h.read(file)) as { url?: string; pid?: number };
    if (x.url && x.pid && alive(x.pid, h)) return x.url;
  } catch {
    // no app running, or a file left by one that stopped
  }
  return null;
}

/** The installed raea app, RAEA_APP first: on a Mac raea.app in Applications; on Windows raea.exe in the folder its
 *  installer put it in (the person picks it, #178; Settings > Apps keeps it as InstallLocation), else the default; on
 *  Linux the launch.sh that linux/install.sh's raea.desktop runs. */
export function installedApp(h: Here = HERE): string | undefined {
  const at: (string | undefined)[] = [h.env.RAEA_APP];
  if (h.platform === "darwin") at.push("/Applications/raea.app", `${h.home}/Applications/raea.app`);
  else if (h.platform === "win32") {
    const q = h.run("reg", ["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\raea", "/v", "InstallLocation"]).stdout;
    const dir = /InstallLocation\s+REG_\w+\s+(.+?)\s*$/m.exec(q)?.[1];
    if (dir) at.push(`${dir.replace(/[\\/]$/, "")}\\raea.exe`);
    if (h.env.LOCALAPPDATA) at.push(`${h.env.LOCALAPPDATA}\\Programs\\raea\\raea.exe`);
  } else {
    try {
      const entry = h.read(`${h.env.XDG_DATA_HOME || `${h.home}/.local/share`}/applications/raea.desktop`);
      at.push(/^Exec="?([^"\n]+?)"?\s*$/m.exec(entry)?.[1]);
    } catch {
      // not in the desktop's app list
    }
  }
  return at.find((p) => p && h.exists(p));
}

/** Files opened as the system opens them (a double-click's app): open, Windows' Explorer, xdg-open. */
export function openFiles(files: string[], h: Here = HERE) {
  if (h.platform === "darwin") h.run("open", files);
  // Explorer, not cmd's start: start reads its line as cmd does, so a folder named with & or ^ breaks it
  else if (h.platform === "win32") for (const f of files) h.run("explorer.exe", [f.replace(/\//g, "\\")]);
  else for (const f of files) h.run("xdg-open", [f]);
}

/** The installed app started: open -a on a Mac, Explorer on Windows, its launch.sh in the background on Linux. */
export function startApp(app: string, h: Here = HERE) {
  if (h.platform === "darwin") h.run("open", ["-a", app]);
  else if (h.platform === "win32") h.run("explorer.exe", [app]);
  else h.run("sh", ["-c", 'nohup "$0" >/dev/null 2>&1 &', app]);
}

/** A quarter of a second between looks for the app's host (Windows has no sleep command: PowerShell's). */
function pause(h: Here) {
  if (h.platform === "win32") h.run("powershell", ["-NoProfile", "-Command", "Start-Sleep -Milliseconds 250"]);
  else h.run("/bin/sleep", ["0.25"]);
}

/** POST a host route of the raea app at `at`, answering its JSON; its refusal is thrown as it says it. */
function postHost(at: string, route: string, a: Record<string, unknown>): any {
  const r = spawnSync("curl", ["-sS", "--max-time", "120", "-w", "\n%{http_code}", "-H", "content-type: application/json", "--data-binary", "@-", `${at.replace(/\/$/, "")}${route}`], { input: JSON.stringify(a), encoding: "utf8", maxBuffer: 64 << 20 });
  const text = r.stdout ?? "";
  const cut = text.lastIndexOf("\n");
  const [body, code] = [text.slice(0, cut), Number(text.slice(cut + 1))];
  if (r.status !== 0) throw new Error(`the raea app at ${at} didn't answer: ${(r.stderr ?? "").trim()}`);
  if (!(code >= 200 && code < 300)) throw Object.assign(new Error(body.trim() || `the raea app said ${code}`), { code });
  return JSON.parse(body);
}

/** `raea show` (show.ts): the item made by the raea app's host (apps/app host/show.ts), the app started first when it's
 *  installed and not running. The window is brought up on it by its raea:// link only with --open or when the app was
 *  just started (on a Mac or Linux: Windows' app has no raea:// link, so there the post lands as its notice); otherwise
 *  the page finds it and says so. Without the app, the files open in the system's own viewer. */
export function show(a: Record<string, unknown>, h: Here = HERE): unknown {
  // a render that still holds an error isn't shown, here or in a viewer (held.ts, docs/stage-guards.feature SG15)
  const refused = held((a.files as string[]) ?? [], String(a.cwd ?? ""), (a.despite as Despite[]) ?? [], h);
  if (refused) throw new Error(refused);
  let at = appHost(h);
  let launched = false;
  if (!at) {
    const app = installedApp(h);
    const files = (a.files as string[]) ?? [];
    if (!app) {
      if (a.question || a.questions) throw new Error(`a question needs the raea app, and it isn't on ${computer(h).name} (raea.io)`);
      openFiles(files, h);
      return { opened: files, ...computer(h) };
    }
    if (h.platform === "darwin" && h.run("pgrep", ["-f", `${app}/Contents/MacOS/raea-window`]).status === 0)
      throw new Error("the raea app running here is older than raea show (it leaves no ~/.raea/app-host.json); quit it and open it again to update, then show it again");
    startApp(app, h);
    launched = true;
    // the window's host writes its address once it listens: a minute at most
    const until = Date.now() + 60_000;
    while (!(at = appHost(h)) && Date.now() < until) pause(h);
    if (!at) throw new Error(`the raea app didn't start (no ~/.raea/app-host.json after a minute); open ${app} and show it again`);
  }
  let shown: { url: string };
  try {
    shown = postHost(at, "/host/show", a);
  } catch (e) {
    // the host's refusal names the verb, as the shell's report does too
    throw new Error(String((e as Error).message).replace(/^raea show: /, ""));
  }
  // the window comes up on it only when asked (--open) or when it was just started for it: a post otherwise lands
  // quietly, the page saying so in a notice (Shane 11-10: "any time you make an update to the feed it takes me out of
  // the thing im currently doing"); RAEA_NO_OPEN: a test's, which mustn't bring up the person's window
  // an app from before quiet posts (no /host/shown) never finds an item it isn't sent to: there it comes up, as before
  const finds = (() => {
    try {
      postHost(at!, "/host/shown", {});
      return true;
    } catch {
      return false;
    }
  })();
  const brought = !h.env.RAEA_NO_OPEN && h.platform !== "win32" && (a.open === true || launched || !finds);
  if (brought) h.run(h.platform === "darwin" ? "open" : "xdg-open", [shown.url]);
  return { ...shown, brought };
}

/** A process's parent and command line: ps, or on Windows every process's at once from PowerShell (one call, not one
 *  per step up: each takes a good part of a second there). */
function processes(h: Here): { start: number; of: (pid: number) => { ppid: number; args: string } | undefined } {
  if (h.platform !== "win32") {
    return {
      start: process.ppid,
      of: (pid) => {
        const m = /^\s*(\d+)\s+(.*)$/.exec(h.run("ps", ["-o", "ppid=,args=", "-p", String(pid)]).stdout);
        return m ? { ppid: Number(m[1]), args: m[2] } : undefined;
      },
    };
  }
  const all = new Map<number, { ppid: number; args: string }>();
  const list = h.run("powershell", ["-NoProfile", "-NonInteractive", "-Command", "Get-CimInstance Win32_Process | ForEach-Object { '{0}`t{1}`t{2}' -f $_.ProcessId, $_.ParentProcessId, $_.CommandLine }"]).stdout;
  for (const line of list.split(/\r?\n/)) {
    const [pid, ppid, ...args] = line.split("\t");
    if (Number(pid) > 0) all.set(Number(pid), { ppid: Number(ppid), args: args.join("\t") });
  }
  return { start: all.get(process.pid)?.ppid ?? 0, of: (pid) => all.get(pid) };
}

/** The agent harness that started this command (show.ts HARNESSES), found up its parent processes: a program's own
 *  name (claude, or on Windows claude.exe), or the script a runtime runs (`node .../gemini`). Nothing when no harness
 *  is up there. (Until 0.1.8 raea's engine had no process.ppid, so this found none anywhere.) */
export function harness(h: Here = HERE): { name: string; pid: number } | undefined {
  const up = processes(h);
  let pid = up.start;
  for (let i = 0; i < 16 && pid > 1; i++) {
    const p = up.of(pid);
    if (!p) return undefined;
    // a Windows command line quotes a path with spaces in it: "C:\Program Files\nodejs\node.exe" ...
    const words = (p.args.match(/"[^"]*"|\S+/g) ?? []).slice(0, 2).map((w) => w.replace(/^"|"$/g, ""));
    for (const w of words.map((w) => w.split(/[\\/]/).pop()!.toLowerCase().replace(/\.(exe|cmd|bat|ps1)$/, ""))) {
      const hit = HARNESSES.find(([re]) => re.test(w));
      if (hit) return { name: hit[1], pid };
    }
    pid = p.ppid;
  }
  return undefined;
}

/** `raea show --on ITEM --working ID` and `--done ID` (show.ts): the person's comments marked on their item by the raea
 *  app's host (apps/app host/tasks.ts), which the window shows as who's on each and which are done. */
export function task(a: Record<string, unknown>): unknown {
  const at = appHost();
  if (!at) throw new Error("the raea app isn't running here: open it, then mark the comments again");
  try {
    return postHost(at, "/host/task", a);
  } catch (e) {
    const old = (e as { code?: number }).code === 404 && !/^raea show: /.test((e as Error).message); // an app from before --on: its host's bare "not found"
    throw new Error(old ? "the raea app running here is older than raea show --on; quit it and open it again to update, then mark the comments again" : String((e as Error).message).replace(/^raea show: /, ""));
  }
}

/** An op of the feed host's libraries (apps/app host/library/service.ts): POST <the app's host>/host/library. */
export function library(a: Record<string, unknown>): unknown {
  const at = appHost();
  if (!at) throw new Error(`libraries live in the raea app: open it on ${computer().name} (or set RAEA_FEED_HOST to its address)`);
  return postHost(at, "/host/library", a);
}

