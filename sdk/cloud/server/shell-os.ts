// The box's shell on any OS (main.ts /v1/shell, agents.ts RAEA_AGENT_CMD). In raea cloud's image it's Linux's bash and
// /bin/sh; on a person's own computer (raea.app's local cloud, apps/app host/local-cloud/box.ts) it can be Windows,
// where there's no /bin/sh: RAEA_CLOUD_BASH names a bash (the one beside Git for Windows, or raea's own Git's), and a
// run is stopped with its children by taskkill, as Windows has no process groups to signal.
import { spawn, type ChildProcess } from "node:child_process"
import path from "node:path"

const win = process.platform === "win32"

/** the bash that runs a command line: RAEA_CLOUD_BASH, else bash on PATH */
export const bash = () => process.env.RAEA_CLOUD_BASH || "bash"

/** the POSIX shell an agent's own command line runs in (RAEA_AGENT_CMD): /bin/sh, or that bash on Windows */
export const sh = () => (win ? bash() : "/bin/sh")

/** the environment that shell runs with: on Windows RAEA_CLOUD_BASH's own folder first on PATH, where its cat, ls and
 *  grep are (raea's own Git has a bare sh.exe there; Git for Windows' bash adds its own). PATH by whatever case Windows
 *  gave its name in. */
export function shellEnv(env: NodeJS.ProcessEnv, on = win, shell = bash()): NodeJS.ProcessEnv {
  if (!on || !/[\\/]/.test(shell)) return env
  const key = Object.keys(env).find((k) => k.toUpperCase() === "PATH") ?? "PATH"
  return { ...env, [key]: `${path.win32.dirname(shell)};${env[key] ?? ""}` }
}

/** spawn options for a run that's stopped with everything it started: its own process group (not on Windows, where a
 *  detached child gets a console window of its own) */
export const groupOpts = () => (win ? { windowsHide: true } : { detached: true })

/** the run and what it started stopped */
export function stopTree(child: ChildProcess) {
  if (!child.pid || child.exitCode !== null) return
  try {
    if (win) spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { windowsHide: true, stdio: "ignore" })
    else process.kill(-child.pid, "SIGTERM")
  } catch {}
}
