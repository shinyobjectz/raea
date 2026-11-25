// The manual's CLI reference (docs/manual.feature DM3, raea.io/docs/reference/cli/): a page a verb, its usage, where it
// runs and its flags written from `raea help --json`, so the page says what the binary takes. What the binary can't say
// is the page's own: its prose above and below the generated block, and each flag's description in the block's table,
// which a rewrite keeps by the flag's name. `check` fails on a block that's out of date, a flag without a description
// and a verb missing from GROUPS.
//   bun tools/manual-ref/cli.ts gen [DIR]     rewrite the pages in DIR (docs/manual/reference/cli)
//   bun tools/manual-ref/cli.ts check [DIR]   the pages as gen would write them, every flag described
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs"
import path from "node:path"

const ROOT = path.resolve(import.meta.dir, "../..")
const [mode = "check", dirArg] = process.argv.slice(2)
const DIR = path.resolve(dirArg ?? path.join(ROOT, "docs/manual/reference/cli"))
const BIN = process.env.RAEA_BIN ?? path.join(ROOT, "target/release/raea")
if (mode !== "gen" && mode !== "check") throw new Error("usage: cli.ts gen|check [DIR]")

type Flag = { flag: string; values: number }
type Verb = { verb: string; usage: string[]; flags: Flag[] | null; args: number | null; access: string; own_parser: boolean }

function run(args: string[]): string {
  const p = Bun.spawnSync([BIN, ...args], { env: { ...process.env, NO_COLOR: "1" } })
  return (p.stdout.toString() + p.stderr.toString()).trim()
}
const verbs: Verb[] = JSON.parse(run(["help", "--json"])).verbs

// the index's groups, in the order a reader meets them; every verb is in one
const GROUPS: [string, string, string[]][] = [
  ["Make", "Render, play and package what you wrote.", ["render", "play", "export-game", "export", "bundle", "app", "run"]],
  ["Projects", "The workspace, its projects and what a project holds.", ["init", "workspace", "project", "spec", "component", "part", "add", "brand", "library", "index"]],
  ["Check", "What's wrong before anyone watches it.", ["check", "judge", "make", "fingerprint", "videoform", "findings", "scene", "light", "bench", "world", "solid", "next", "doctor", "hook", "traces"]],
  ["Footage", "Takes, cuts and the camera, from your own media.", ["media", "cut", "frame", "track", "splat", "mezz", "neural"]],
  ["Sound", "Music read, foley and instruments made on this machine.", ["song", "sounds", "sao", "tokensynth", "setup-audio", "voice"]],
  ["Generate and 3D tools", "Generated media, physics, light and weathering bakes, clothes and motion for SOMA's body, Blender and Godot.", ["gen", "physics", "bake", "wear", "clothes", "motion", "blender", "godot"]],
  ["Agents and setup", "What an agent reads and shows the person, and the tools raea installs.", ["guide", "fix", "context", "notes", "show", "skill", "skills", "setup", "bug", "ask"]],
]
/** A verb's page: VERB.md, but index.md is this section's own page, so raea index's is indexer.md */
const pageOf = (verb: string) => (verb === "index" ? "indexer.md" : `${verb}.md`)
const groupOf = new Map(GROUPS.flatMap(([g, , vs]) => vs.map((v, i) => [v, { g, i }] as const)))

// a verb with its own parser lists its forms itself: that list is its usage
const OWN_HELP: Record<string, string[]> = { media: ["media", "help"], gen: ["gen"], blender: ["blender"], godot: ["godot"], physics: ["physics"], wear: ["wear"] }
function ownHelp(v: string): string {
  return run(OWN_HELP[v] ?? [v]).replace(/^REFUSED\s+/, "").replace(/^usage:\s*/, "").split("\n").map((l) => l.replace(/^\s{7}(raea )/, "$1").trimEnd()).join("\n")
}

// what follows a flag in its usage: `--frame N`, `--quality draft|high|master|0..1`, `--snapshot T FILE`
function valuesOf(v: Verb, f: Flag): string {
  if (!f.values) return ""
  const esc = f.flag.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
  for (const line of v.usage) {
    const m = new RegExp(`(?:^|[\\s\\[|])${esc}(?:\\s+([^\\s\\[\\]]+))${f.values > 1 ? "(?:\\s+([^\\s\\[\\]]+))" : ""}`).exec(line.split("   (")[0])
    if (m) return [m[1], m[2]].filter(Boolean).map((t) => t.replace(/[\]|,;]+$/, "")).filter((t) => !t.startsWith("-")).join(" ")
  }
  return f.values === 1 ? "VALUE" : Array.from({ length: f.values }, (_, i) => `V${i + 1}`).join(" ")
}

const ACCESS: Record<string, string> = {
  HEADLESS: "Headless: it needs nothing running and opens no window.",
  HOST: "Needs the raea app's host running on this machine.",
  PERSON: "Waits for a person's click in the raea app.",
}
const accessLine = (a: string) => ACCESS[a] ?? a.split(/, then /).map((p, i) => (i ? "then " : "") + (i ? (ACCESS[p.split(" ")[0]] ?? p).replace(/^W/, "w") : ACCESS[p.split(" ")[0]] ?? p).replace(/\.$/, "") + (p.includes(" to ") ? ` (${p.slice(p.indexOf(" to ") + 1)})` : "")).join("; ") + "."
const argsLine = (v: Verb) => (v.own_parser ? "Its arguments go to a parser of its own, whose forms are listed above." : v.args === null ? "Takes any number of arguments besides its flags." : v.args === 0 ? "Takes no arguments besides its flags." : `Takes up to ${v.args} argument${v.args === 1 ? "" : "s"} besides its flags.`)

const BEGIN = "<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->"
const END = "<!-- generated:end -->"
const cell = (s: string) => s.replace(/\|/g, "\\|")
const uncell = (s: string) => s.replace(/\\\|/g, "|").trim()

type Page = { head: string; tail: string; desc: Map<string, string>; summary: string }
function readPage(file: string): Page | null {
  if (!existsSync(file)) return null
  const text = readFileSync(file, "utf8")
  const b = text.indexOf("<!-- generated:begin"), e = text.indexOf(END)
  if (b < 0 || e < b) throw new Error(`${path.relative(ROOT, file)}: no generated block (${BEGIN} … ${END})`)
  const desc = new Map<string, string>()
  for (const m of text.slice(b, e).matchAll(/^\| `([^`]+)` \|.*\| (.*) \|$/gm)) desc.set(m[1], uncell(m[2]) === "–" ? "" : uncell(m[2]))
  const summary = /^summary:\s*(.*)$/m.exec(text)?.[1] ?? ""
  return { head: text.slice(0, b), tail: text.slice(e + END.length), desc, summary: summary ? JSON.parse(summary) : "" }
}

function block(v: Verb, desc: Map<string, string>): string {
  const usage = v.own_parser ? ownHelp(v.verb) : v.usage.join("\n")
  const md = [BEGIN, "", "## Usage", "", "```sh", usage, "```", "", `${accessLine(v.access)} ${argsLine(v)}`, ""]
  const flags = v.flags ?? []
  if (flags.length) {
    md.push("## Flags", "", "| Flag | Takes | What it does |", "| --- | --- | --- |")
    for (const f of flags) md.push(`| \`${f.flag}\` | ${cell(valuesOf(v, f)) ? "`" + cell(valuesOf(v, f)) + "`" : "–"} | ${cell(desc.get(f.flag) || "–")} |`)
    md.push("")
  }
  md.push(END)
  return md.join("\n")
}

const note = (v: Verb) => (v.usage.map((u) => /\s{3}\((.*)\)\s*$/.exec(u)?.[1] ?? "").find((n) => n && !n.startsWith("also:")) ?? "").replace(/;.*$/, "")
function newHead(v: Verb): string {
  const s = note(v) || `What raea ${v.verb} does.`
  const sum = s.charAt(0).toUpperCase() + s.slice(1).replace(/\.?$/, ".")
  return ["---", `title: ${JSON.stringify("raea " + v.verb)}`, `summary: ${JSON.stringify(sum)}`, `order: 0`, `sources: ${JSON.stringify(["engine/cli/src/cli.rs"])}`, "---", "", sum, "", ""].join("\n")
}

const problems: string[] = []
const rows = new Map<string, { verb: string; summary: string }[]>()
if (mode === "gen") mkdirSync(DIR, { recursive: true })
for (const v of verbs) {
  const at = groupOf.get(v.verb)
  if (!at) { problems.push(`${v.verb}: in no group (GROUPS in tools/manual-ref/cli.ts)`); continue }
  const file = path.join(DIR, pageOf(v.verb))
  const old = readPage(file)
  const desc = old?.desc ?? new Map()
  let head = old?.head ?? newHead(v)
  const order = GROUPS.findIndex(([g]) => g === at.g) * 20 + at.i + 2
  head = head.replace(/^order: .*$/m, `order: ${order}`)
  const text = (head.trimEnd() + "\n\n" + block(v, desc) + "\n" + (old?.tail ?? "").replace(/^\n*/, "\n")).replace(/\n{3,}/g, "\n\n").trimEnd() + "\n"
  for (const f of v.flags ?? []) if (!desc.get(f.flag)) problems.push(`${v.verb}: ${f.flag} has no description`)
  const now = existsSync(file) ? readFileSync(file, "utf8") : ""
  if (mode === "gen") writeFileSync(file, text)
  else if (now !== text) problems.push(`${v.verb}: ${now ? "out of date" : "no page"} (bun tools/manual-ref/cli.ts gen)`)
  rows.set(at.g, [...(rows.get(at.g) ?? []), { verb: v.verb, summary: readPage(file)?.summary ?? old?.summary ?? "" }])
}
for (const [, , vs] of GROUPS) for (const v of vs) if (!verbs.some((x) => x.verb === v)) problems.push(`${v}: in GROUPS but raea help doesn't list it`)

// the index: its own prose above, the verbs by group below
const indexFile = path.join(DIR, "index.md")
const idx = existsSync(indexFile) ? readFileSync(indexFile, "utf8") : ["---", 'title: "CLI"', 'summary: "Every raea verb: what it does, its forms and its flags."', "order: 1", "---", "", ""].join("\n")
const ib = idx.indexOf("<!-- generated:begin"), ie = idx.indexOf(END)
const ihead = ib < 0 ? idx : idx.slice(0, ib), itail = ib < 0 ? "" : idx.slice(ie + END.length)
const list = [BEGIN.replace("; edit only the flag descriptions", ""), ""]
for (const [g, what, vs] of GROUPS) {
  list.push(`## ${g}`, "", what, "", "| Verb | What it does |", "| --- | --- |")
  for (const v of vs) { const r = rows.get(g)?.find((x) => x.verb === v); if (r) list.push(`| [\`raea ${v}\`](${pageOf(v)}) | ${cell(r.summary)} |`) }
  list.push("")
}
list.push(END)
const itext = (ihead.trimEnd() + "\n\n" + list.join("\n") + "\n" + itail.replace(/^\n*/, "\n")).replace(/\n{3,}/g, "\n\n").trimEnd() + "\n"
if (mode === "gen") writeFileSync(indexFile, itext)
else if (!existsSync(indexFile) || readFileSync(indexFile, "utf8") !== itext) problems.push("index: out of date (bun tools/manual-ref/cli.ts gen)")

const flagCount = verbs.reduce((n, v) => n + (v.flags?.length ?? 0), 0)
if (problems.length) {
  console.error(problems.map((p) => "cli-ref: " + p).join("\n"))
  console.error(`cli-ref: ${problems.length} problem(s), ${verbs.length} verbs, ${flagCount} flags`)
  if (mode === "check") process.exit(1)
} else console.log(`cli-ref: ${verbs.length} verbs, ${flagCount} flags, every flag described`)
