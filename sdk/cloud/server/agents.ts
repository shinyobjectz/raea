// The ACP agents a raea cloud runs (docs/cloud.feature CL2, CL3): any agent the ACP registry lists
// (github.com/agentclientprotocol/registry), installed from its registry entry when the image is built. An npx entry
// is installed with npm at the registry's version; a binary entry is fetched from its linux archive and checked by
// sha256; a uvx entry is refused (the image has no Python). Each install leaves AGENTS/ID/spawn.json (how to start it)
// and AGENTS/ID/entry.json (the registry entry it came from).
//
//   node agents.ts install ID...   install agents (the Dockerfile's AGENTS)
//   node agents.ts skills          the raea skill names for the installed agents (entrypoint.sh)
//   node agents.ts list            the installed agents, one per line

import { sh } from "./shell-os.ts"
import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { chmodSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import path from "node:path"
import { pathToFileURL } from "node:url"

export const REGISTRY_URL = process.env.RAEA_CLOUD_REGISTRY ?? "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json"
export const AGENTS_DIR = process.env.RAEA_CLOUD_AGENTS ?? "/opt/raea-cloud/agents"

interface Binary {
  archive: string
  cmd: string
  args?: string[]
  env?: Record<string, string>
  sha256?: string
}
interface Entry {
  id: string
  name: string
  version: string
  distribution: { binary?: Record<string, Binary>; npx?: { package: string; args?: string[]; env?: Record<string, string> }; uvx?: unknown }
}

/** How to start one agent: a command, its arguments and the environment it adds. */
export interface Spawn {
  id: string
  version: string
  command: string
  args: string[]
  env: Record<string, string>
}

/** raea's skill folder names (`raea skills install --agent`) for the registry ids that have one. */
const SKILLS: Record<string, string> = { "kimi-code": "kimi", kimi: "kimi", "claude-acp": "claude", "codex-acp": "codex", gemini: "gemini" }

const platform = () => `linux-${process.arch === "arm64" ? "aarch64" : "x86_64"}`

async function registry(): Promise<Entry[]> {
  if (!/^https?:/.test(REGISTRY_URL)) return JSON.parse(readFileSync(REGISTRY_URL, "utf8")).agents
  const res = await fetch(REGISTRY_URL)
  if (!res.ok) throw new Error(`ACP registry: ${res.status} from ${REGISTRY_URL}`)
  return ((await res.json()) as { agents: Entry[] }).agents
}

/** The command an npm package installs: its one bin, or the one named like the package. */
function binOf(dir: string, pkg: string): string {
  const name = pkg.replace(/@[^@/]+$/, "")
  const manifest = JSON.parse(readFileSync(path.join(dir, "node_modules", name, "package.json"), "utf8")) as { bin?: string | Record<string, string> }
  const short = name.split("/").pop()!
  if (typeof manifest.bin === "string") return short
  const bins = Object.keys(manifest.bin ?? {})
  const bin = bins.length === 1 ? bins[0] : (bins.find((b) => b === short) ?? bins[0])
  if (!bin) throw new Error(`${name} installs no command`)
  return bin
}

async function fetchArchive(b: Binary, dir: string): Promise<void> {
  const res = await fetch(b.archive)
  if (!res.ok) throw new Error(`${b.archive}: ${res.status}`)
  const bytes = Buffer.from(await res.arrayBuffer())
  const sum = createHash("sha256").update(bytes).digest("hex")
  if (b.sha256 && sum !== b.sha256) throw new Error(`${b.archive}: sha256 ${sum}, the registry says ${b.sha256}`)
  const file = path.join(dir, path.basename(new URL(b.archive).pathname))
  writeFileSync(file, bytes)
  if (/\.(tar\.gz|tgz)$/.test(file)) execFileSync("tar", ["-xzf", file, "-C", dir])
  else if (/\.tar\.xz$/.test(file)) execFileSync("tar", ["-xJf", file, "-C", dir])
  else if (/\.tar\.bz2$/.test(file)) execFileSync("tar", ["-xjf", file, "-C", dir])
  else if (/\.zip$/.test(file)) execFileSync("unzip", ["-q", "-o", file, "-d", dir])
  else return // a bare program: the download is the command
  rmSync(file)
}

/** Installs one registry agent under AGENTS_DIR/ID and says how to start it. */
export async function install(id: string, agents?: Entry[]): Promise<Spawn> {
  const e = (agents ?? (await registry())).find((a) => a.id === id)
  if (!e) throw new Error(`${id} isn't in the ACP registry (${REGISTRY_URL})`)
  const dir = path.join(AGENTS_DIR, id)
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  const d = e.distribution
  const bin = d.binary?.[platform()]
  let spawn: Spawn
  // npx first when an agent has both: node is in the image, and the npm package is the one its authors run
  if (d.npx) {
    execFileSync("npm", ["install", "--prefix", dir, "--no-fund", "--no-audit", "--omit=dev", "--loglevel=error", d.npx.package], { stdio: "inherit" })
    spawn = { id, version: e.version, command: path.join(dir, "node_modules/.bin", binOf(dir, d.npx.package)), args: d.npx.args ?? [], env: d.npx.env ?? {} }
  } else if (bin) {
    await fetchArchive(bin, dir)
    const command = path.resolve(dir, bin.cmd)
    if (!existsSync(command)) throw new Error(`${id}: ${bin.cmd} isn't in ${bin.archive}`)
    chmodSync(command, 0o755)
    spawn = { id, version: e.version, command, args: bin.args ?? [], env: bin.env ?? {} }
  } else if (d.uvx) throw new Error(`${id} installs with uvx, and this image has no Python: build an image that adds it, or run it by RAEA_AGENT_CMD`)
  else throw new Error(`${id} has no ${platform()} build in the registry`)
  writeFileSync(path.join(dir, "spawn.json"), JSON.stringify(spawn, null, 2))
  writeFileSync(path.join(dir, "entry.json"), JSON.stringify(e, null, 2))
  return spawn
}

/** The agents installed in this image. */
export function installed(): Spawn[] {
  if (!existsSync(AGENTS_DIR)) return []
  return readdirSync(AGENTS_DIR)
    .map((id) => path.join(AGENTS_DIR, id, "spawn.json"))
    .filter((f) => existsSync(f))
    .map((f) => JSON.parse(readFileSync(f, "utf8")) as Spawn)
}

/** How to start agent ID (the default agent when none is named). RAEA_AGENT_CMD, a shell command line, runs an agent
 *  that isn't installed from the registry (one of your own, or one mounted into the container). */
export function spawnFor(id?: string): Spawn {
  const want = id || process.env.RAEA_AGENT || ""
  const cmd = process.env.RAEA_AGENT_CMD
  if (cmd && (!want || want === "custom")) return { id: "custom", version: "", command: sh(), args: ["-c", cmd], env: {} }
  const all = installed()
  const s = want ? all.find((a) => a.id === want) : all[0]
  if (!s) throw new Error(want ? `${want} isn't installed in this image (installed: ${all.map((a) => a.id).join(", ") || "none"})` : "no agent is installed: build with AGENTS=..., or set RAEA_AGENT_CMD")
  return s
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [cmd, ...ids] = process.argv.slice(2)
  if (cmd === "install") {
    const agents = await registry()
    for (const id of ids.flatMap((s) => s.split(/[\s,]+/)).filter(Boolean)) {
      const s = await install(id, agents)
      console.log(`raea-cloud: ${id} ${s.version} -> ${s.command} ${s.args.join(" ")}`)
    }
  } else if (cmd === "skills") console.log([...new Set(installed().flatMap((a) => SKILLS[a.id] ?? []))].join("\n"))
  else if (cmd === "list") console.log(installed().map((a) => `${a.id} ${a.version}`).join("\n"))
  else {
    console.error("usage: node agents.ts install ID... | skills | list")
    process.exit(2)
  }
}
