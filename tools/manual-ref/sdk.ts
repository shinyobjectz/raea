// The manual's SDK reference (docs/manual.feature DM3, raea.io/docs/reference/sdk/): one page an import path, written
// from the SDK itself by the TypeScript compiler, so it says what the code says. Each export with its doc comment and
// its Bevy name (@bevy), a component's props with their types and docs, and the Bevy parity ports and manual examples
// that use it, which are checked against Bevy and rendered in CI, so they work as examples.
// A page's prose above and below its generated block is its own, kept by a rewrite as the CLI pages' is (cli.ts).
//   bun tools/manual-ref/sdk.ts gen [DIR]     rewrite the pages in DIR (docs/manual/reference/sdk)
//   bun tools/manual-ref/sdk.ts check [DIR]   the pages as gen would write them, every import path in GROUPS, every
//                                             prop and field they table described
import { readdirSync, readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs"
import path from "node:path"
import { createRequire } from "node:module"

const ROOT = path.resolve(import.meta.dir, "../..")
const ts = createRequire(path.join(ROOT, "sdk/shell/package.json"))("typescript") as typeof import("typescript")
const SRC = path.join(ROOT, "sdk/raea/src")
const REPO = "https://github.com/shinyobjectz/raea/blob/main"
const SKIP = /(\.gen\.ts|^test\.tsx|^native-calls\.ts|^prelude\.ts|^pure\.ts|^frame-globals\.ts|^canvas2d\.ts|^random-colors\.ts|^page-layout\.ts|^dom-[a-z-]+\.ts)$/
const [mode = "check", dirArg] = process.argv.slice(2)
if (mode !== "gen" && mode !== "check") throw new Error("usage: sdk.ts gen|check [DIR]")
const out = path.resolve(dirArg ?? path.join(ROOT, "docs/manual/reference/sdk"))
const problems: string[] = []

// the index's groups, in the order a reader meets them; every import path is in one
const GROUPS: [string, string, string[]][] = [
  ["Start here", "The composition, the app, and the specs and notes an agent writes.", ["raea", "app", "brief", "note"]],
  ["Time and motion", "Frames, animation, springs, states and input, and holding a frame until it's ready.", ["time", "animation", "math", "state", "input", "hold"]],
  ["Video, sound and text", "Footage, pictures, sound, words and colour, and the timeline they are edited on.", ["video", "audio", "text", "image", "asset", "color", "editor"]],
  ["3D", "Bevy's 3D: cameras, lights, materials, meshes and the frame after it's drawn.", ["3d", "camera", "light", "pbr", "mesh", "transform", "post_process", "picking", "gizmos"]],
  ["2D and UI", "Sprites and flat shapes, bevy_ui and Bevy's widgets.", ["sprite", "ui", "feathers"]],
  ["Tools", "Checks, lints and a headless App you can drive.", ["check", "fingerprint", "dev_tools", "remote", "ask"]],
]
const groupAt = new Map(GROUPS.flatMap(([, , ps], g) => ps.map((p, i) => [p, g * 20 + i + 2] as const)))
const BEGIN = "<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->"
const END = "<!-- generated:end -->"
/** a page's own prose: what's above its generated block and below it */
function own(file: string): { head: string; tail: string } | null {
  if (!existsSync(file)) return null
  const t = readFileSync(file, "utf8")
  const b = t.indexOf("<!-- generated:begin"), e = t.indexOf(END)
  if (b < 0 || e < b) { problems.push(`${path.relative(ROOT, file)}: no generated block`); return null }
  return { head: t.slice(0, b), tail: t.slice(e + END.length) }
}
function emit(file: string, text: string) {
  const now = existsSync(file) ? readFileSync(file, "utf8") : ""
  if (mode === "gen") writeFileSync(file, text)
  else if (now !== text) problems.push(`${path.relative(ROOT, file)}: ${now ? "out of date" : "missing"} (bun tools/manual-ref/sdk.ts gen)`)
}
const summaryOf = (head: string) => { const m = /^summary:\s*(.*)$/m.exec(head); return m ? (JSON.parse(m[1]) as string) : "" }
const join = (head: string, block: string, tail: string) => (head.trimEnd() + "\n\n" + block + "\n" + tail.replace(/^\n*/, "\n")).replace(/\n{3,}/g, "\n\n").trimEnd() + "\n"

const entries = readdirSync(SRC).filter((f) => /\.tsx?$/.test(f) && !SKIP.test(f)).sort()
const opts = ts.getParsedCommandLineOfConfigFile(path.join(ROOT, "sdk/raea/tsconfig.json"), {}, { ...ts.sys, onUnRecoverableConfigFileDiagnostic: () => {} })!.options
const program = ts.createProgram(entries.map((f) => path.join(SRC, f)), { ...opts, noEmit: true })
const checker = program.getTypeChecker()

const real = (s: import("typescript").Symbol) => (s.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(s) : s)
const entryName = (f: string) => (f.startsWith("index.") ? "raea" : "raea/" + f.replace(/\.tsx?$/, ""))
const pageName = (entry: string) => (entry === "raea" ? "raea" : entry.slice(5))
const oneLine = (s: string) => s.replace(/\s*\n\s*/g, " ").trim()
const cell = (s: string) => oneLine(s).replace(/\|/g, "\\|")
// raea's plan ids and feature files mean nothing to a reader of the docs: "(S3c)", "(docs/x.feature B5)", "(M1)"
const plain = (s: string) => s.replace(/\s*\((?:[^()]*\.feature[^()]*|[A-Z]{1,3}\d+[a-z]?(?:[-, ][A-Z]{0,3}\d+[a-z]?)*)\)/g, "").replace(/\s+([.,;:])/g, "$1")
const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`

// Bevy's own docs for the types the SDK names (bevy-docs.ts): by @bevy path, or "name:<Name>" for a mirrored name
type BevyDoc = { doc: string; fields: Record<string, string>; from?: string }
const BEVY: Record<string, BevyDoc> = JSON.parse(readFileSync(path.join(import.meta.dir, "bevy-docs.json"), "utf8"))
function bevyOf(name: string, tags: string[]): { path: string; doc: BevyDoc } | null {
  for (const t of tags) if (BEVY[t]) return { path: t, doc: BEVY[t] }
  const n = BEVY[`name:${name}`]
  return n ? { path: n.from ?? name, doc: n } : null
}
/** a prop's doc: the SDK's own, else Bevy's field of that name, marked as Bevy's */
function propDoc(s: import("typescript").Symbol, bevy: BevyDoc | undefined) {
  const own = docOf(s).text
  if (own) return own
  // the field's Bevy doc: from the type it was declared in (NodeProps is bevy_ui's Node), else the component's own
  const p = s.declarations?.[0]?.parent
  if (p && (ts.isInterfaceDeclaration(p) || ts.isTypeAliasDeclaration(p)) && p.name) {
    const sym = checker.getSymbolAtLocation(p.name)
    const tags = sym ? docOf(sym).bevy : []
    for (const t of tags) if (BEVY[t]?.fields[s.name]) return `${ownRust(t) ? "" : "Bevy: "}${BEVY[t].fields[s.name]}`
    const n = BEVY[`name:${p.name.text}`]
    if (n?.fields[s.name]) return `Bevy: ${n.fields[s.name]}`
  }
  return bevy?.fields[s.name] ? `Bevy: ${bevy.fields[s.name]}` : ""
}

/** a type of raea's own Rust (a raea_* crate), as a type raea-bevy-sdk generates names it: its docs are raea's, not Bevy's */
const ownRust = (path: string) => path.startsWith("raea_")
const RUST_PATH = /^[a-z_0-9]+(?:::[A-Za-z_0-9]+)+$/
/** the doc comment and @bevy tags of a symbol; a generated type's doc, its Rust path alone, is a tag, and the Rust
 *  type's own doc stands for it */
function docOf(sym: import("typescript").Symbol) {
  let text = ts.displayPartsToString(sym.getDocumentationComment(checker)).trim()
  const bevy = sym.getJsDocTags(checker).filter((t) => t.name === "bevy").map((t) => ts.displayPartsToString(t.text).trim())
  if (RUST_PATH.test(text)) { bevy.push(text); text = BEVY[text]?.doc ?? "" }
  return { text, bevy }
}

/** a wrapped component's doc (`export const X = memo(XImpl)`, `Object.assign(memo(XImpl), {...})`): the wrapped
 *  function's, when the export has none of its own */
function wrappedDoc(sym: import("typescript").Symbol): { text: string; bevy: string[] } {
  const d = sym.valueDeclaration
  if (!d || !ts.isVariableDeclaration(d)) return { text: "", bevy: [] }
  let found: { text: string; bevy: string[] } = { text: "", bevy: [] }
  // `export const rise: typeof riseImpl = ...`: the function it is typed as
  if (d.type && ts.isTypeQueryNode(d.type)) {
    const s = checker.getSymbolAtLocation(d.type.exprName)
    if (s) found = docOf(real(s))
  }
  if (found.text || !d.initializer) return found
  const visit = (n: import("typescript").Node): void => {
    if (found.text) return
    if (ts.isIdentifier(n)) {
      const s = checker.getSymbolAtLocation(n)
      const r = s ? real(s) : undefined
      if (r && r !== sym && r.valueDeclaration && (ts.isFunctionDeclaration(r.valueDeclaration) || ts.isVariableDeclaration(r.valueDeclaration))) {
        const doc = docOf(r)
        if (doc.text) found = doc
      }
    }
    if (ts.isCallExpression(n)) n.arguments.forEach(visit)
  }
  visit(d.initializer)
  return found
}

/** a prop's type as written where it's declared (shorter and closer to the docs than the checker's expansion) */
function typeText(sym: import("typescript").Symbol): string {
  const d = sym.valueDeclaration ?? sym.declarations?.[0]
  const written = d && "type" in d && (d as { type?: import("typescript").Node }).type
  // a method (`build(app: App): void`) shows as its signature, not its return type
  const method = d && (ts.isMethodSignature(d) || ts.isMethodDeclaration(d))
  let s = method ? checker.typeToString(checker.getTypeOfSymbolAtLocation(sym, d)) : written ? written.getText() : d ? checker.typeToString(checker.getTypeOfSymbolAtLocation(sym, d)) : "unknown"
  s = oneLine(s)
  return s.length > 90 ? s.slice(0, 87) + "…" : s
}

/** where a declaration's name was declared: an interface or type alias's name */
const ownerOf = (sym: import("typescript").Symbol) => {
  const p = sym.declarations?.[0]?.parent
  return p && (ts.isInterfaceDeclaration(p) || ts.isTypeAliasDeclaration(p) || ts.isClassDeclaration(p)) && p.name ? p.name.text : ""
}

/** a function component's props type: its first parameter's, when it takes one */
function propsOf(sym: import("typescript").Symbol) {
  const d = sym.valueDeclaration
  if (!d) return null
  const t = checker.getTypeOfSymbolAtLocation(sym, d)
  const sig = t.getCallSignatures()[0]
  const param = sig?.getParameters()[0]
  if (!param || !param.valueDeclaration) return null
  const pt = checker.getTypeOfSymbolAtLocation(param, param.valueDeclaration)
  const name = param.valueDeclaration && ts.isParameter(param.valueDeclaration) && param.valueDeclaration.type ? param.valueDeclaration.type.getText() : ""
  const props = checker.getPropertiesOfType(pt)
  return props.length ? { name, props } : null
}

type Kind = "component" | "hook" | "function" | "class" | "value" | "type"
function kindOf(name: string, sym: import("typescript").Symbol): Kind {
  const f = sym.flags
  if (f & ts.SymbolFlags.Class) return "class"
  const value = f & (ts.SymbolFlags.Function | ts.SymbolFlags.Variable | ts.SymbolFlags.Enum)
  if (!value) return "type"
  const d = sym.valueDeclaration
  const calls = d ? checker.getTypeOfSymbolAtLocation(sym, d).getCallSignatures().length > 0 : false
  if (calls && /^use[A-Z]/.test(name)) return "hook"
  if (calls && /^[A-Z]/.test(name)) return "component"
  return calls ? "function" : "value"
}

// the files that show each name in use: Bevy parity ports (checked against Bevy) and the manual's examples (rendered in CI)
const examples: { file: string; text: string }[] = []
for (const dir of ["sdk/raea/bench/bevy-parity", "docs/manual/examples"]) {
  const walk = (d: string): void => {
    for (const n of readdirSync(path.join(ROOT, d), { withFileTypes: true })) {
      const p = path.join(d, n.name)
      if (n.isDirectory()) { if (n.name !== "assets" && n.name !== "helpers") walk(p) }
      else if (/\.tsx?$/.test(n.name)) examples.push({ file: p, text: readFileSync(path.join(ROOT, p), "utf8") })
    }
  }
  try { walk(dir) } catch { /* a folder this checkout doesn't have */ }
}
function usedIn(entry: string, name: string) {
  const from = new RegExp(`import\\s*(?:type\\s*)?\\{[^}]*\\b${name}\\b[^}]*\\}\\s*from\\s*"${entry.replace("/", "\\/")}"`)
  return examples.filter((e) => from.test(e.text)).map((e) => e.file)
}
const exampleLink = (f: string) => {
  const label = f.replace(/^sdk\/raea\/bench\/bevy-parity\//, "").replace(/^docs\/manual\/examples\//, "manual: ").replace(/\.tsx?$/, "")
  return `[${label}](${REPO}/${f})`
}

/** the entry file's own header: its "raea/x (bevy_x): what" line and the lines after it, as prose */
function headerOf(file: string, entry: string) {
  const lines = readFileSync(path.join(SRC, file), "utf8").split("\n")
  const head: string[] = []
  for (const l of lines) { if (!l.startsWith("//")) break; head.push(l.replace(/^\/\/\s?/, "")) }
  const at = head.findIndex((l) => l.startsWith(entry + " ") || l.startsWith(entry + ":"))
  const text = oneLine((at >= 0 ? head.slice(at) : head.filter((l) => !/^docs\/sdk\.feature/.test(l))).join("\n"))
  const m = /^raea(?:\/[a-z_0-9]+)?\s*(?:\(([^)]*)\))?:\s*(.*)$/.exec(text)
  return { bevy: m?.[1] ?? "", what: (m?.[2] ?? text).replace(/^\s+/, "") }
}

const SECTION: Record<Kind, string> = { component: "Components", hook: "Hooks", function: "Functions", class: "Classes", value: "Values", type: "Types" }
const ONE: Record<Kind, string> = { component: "component", hook: "hook", function: "function", class: "class", value: "value", type: "type" }
const ORDER: Kind[] = ["component", "hook", "function", "class", "value", "type"]
if (mode === "gen") mkdirSync(out, { recursive: true })
const index: { entry: string; page: string; what: string; counts: string }[] = []

// every exported type's page (an entry of its own before the root "raea", which re-exports), and how many components take
// each props type: one shared by several (NodeProps) is tabled once, on its type's entry, and linked from each
const home = new Map<string, string>()
const uses = new Map<string, number>()
for (const file of [...entries].sort((a, b) => Number(a.startsWith("index.")) - Number(b.startsWith("index.")))) {
  const mod = checker.getSymbolAtLocation(program.getSourceFile(path.join(SRC, file))!)
  if (!mod) continue
  for (const ex of checker.getExportsOfModule(mod)) {
    const s = real(ex)
    const k = kindOf(ex.name, s)
    if (k === "type" && !home.has(ex.name)) home.set(ex.name, pageName(entryName(file)))
    if (k === "component" && file.startsWith("index.") === false) { const p = propsOf(s)?.name.replace(/<.*$/, ""); if (p) uses.set(p, (uses.get(p) ?? 0) + 1) }
  }
}
const typeLink = (name: string, from: string) => (home.has(name) ? `[\`${name}\`](${home.get(name) === from ? "" : home.get(name) + ".md"}#${name.toLowerCase()})` : "`" + name + "`")
const shared = (name: string) => (uses.get(name) ?? 0) > 1 && home.has(name)
// every prop and field the reference tables has a description (its own, or Bevy's): one problem a declaration, however many
// pages table it
const undescribed = new Set<string>()
function described(s: import("typescript").Symbol, bevy: BevyDoc | undefined): string {
  const d = plain(propDoc(s, bevy))
  const decl = s.declarations?.[0]
  if (!d && decl) {
    const sf = decl.getSourceFile()
    const at = `${path.relative(ROOT, sf.fileName)}:${sf.getLineAndCharacterOfPosition(decl.getStart()).line + 1}`
    if (!undescribed.has(at + s.name)) { undescribed.add(at + s.name); problems.push(`${at}: ${s.name} has no doc comment`) }
  }
  return d
}
/** a constructor's doc comment, a problem without one */
function ctorDoc(c: import("typescript").Signature, cls: string): string {
  const decl = c.getDeclaration()
  const d = decl ? plain(ts.displayPartsToString(c.getDocumentationComment(checker)).trim()) : ""
  if (!d && decl) {
    const sf = decl.getSourceFile()
    const at = `${path.relative(ROOT, sf.fileName)}:${sf.getLineAndCharacterOfPosition(decl.getStart()).line + 1}`
    if (!undescribed.has(at)) { undescribed.add(at); problems.push(`${at}: new ${cls} has no doc comment`) }
  }
  return d
}

entries.forEach((file, i) => {
  const entry = entryName(file)
  const sf = program.getSourceFile(path.join(SRC, file))!
  const mod = checker.getSymbolAtLocation(sf)
  if (!mod) return
  const exps = checker.getExportsOfModule(mod).filter((s) => s.name !== "default").sort((a, b) => a.name.localeCompare(b.name))
  if (!exps.length) return
  const { bevy, what } = headerOf(file, entry)
  const groups = new Map<Kind, string[]>()
  // a component's props type is tabled with the component: the type's own entry just points there
  const propsFor = new Map<string, string>()
  for (const ex of exps) { const s = real(ex); if (kindOf(ex.name, s) === "component") { const p = propsOf(s); if (p?.name && !shared(p.name.replace(/<.*$/, ""))) propsFor.set(p.name.replace(/<.*$/, ""), ex.name) } }
  for (const ex of exps) {
    const sym = real(ex)
    const kind = kindOf(ex.name, sym)
    const doc0 = docOf(sym)
    const { text, bevy: tags } = doc0.text ? doc0 : wrappedDoc(sym).text ? { text: wrappedDoc(sym).text, bevy: [...doc0.bevy, ...wrappedDoc(sym).bevy] } : doc0
    const bv = bevyOf(ex.name, tags)
    // a props type (StandardMaterialProps) is its component's Bevy type's fields: the component's tags stand for it
    const of = /Props$/.test(ex.name) ? exps.find((e) => e.name === ex.name.slice(0, -5)) : undefined
    const bvFields = bv ?? (of ? bevyOf(of.name, [...docOf(real(of)).bevy, ...wrappedDoc(real(of)).bevy]) : null)
    const md: string[] = [`### \`${ex.name}\``, ""]
    if (text) md.push(plain(text), "")
    else if (bv?.doc.doc) md.push(`*From ${ownRust(bv.path) ? "raea's Rust" : "Bevy's"} \`${bv.path}\`:* ${bv.doc.doc}`, "")
    const bevyTags = tags.filter((t) => !ownRust(t)), rustTags = tags.filter(ownRust)
    if (bevyTags.length) md.push(`Bevy: ${bevyTags.map((t) => "`" + t + "`").join(", ")}.`, "")
    else if (bv && text && !rustTags.length) md.push(`Bevy: \`${bv.path}\`.`, "")
    if (rustTags.length) md.push(`Rust: ${rustTags.map((t) => "`" + t + "`").join(", ")}.`, "")
    // a function's or hook's signatures, as the checker prints them (the first three overloads)
    if ((kind === "function" || kind === "hook") && sym.valueDeclaration) {
      const sigs = checker.getTypeOfSymbolAtLocation(sym, sym.valueDeclaration).getCallSignatures().slice(0, 3)
      const f = ts.TypeFormatFlags.NoTruncation | ts.TypeFormatFlags.WriteArrowStyleSignature
      const lines = sigs.map((s) => { const t = oneLine(checker.signatureToString(s, undefined, f & ~ts.TypeFormatFlags.WriteArrowStyleSignature)).replace(/(\?: [^;,()]+?) \| undefined/g, "$1"); return `function ${ex.name}${t.length > 400 ? t.slice(0, 397) + "…" : t}` })
      if (lines.length) md.push("```ts", ...lines, "```", "")
    }
    // a class's public members: its statics, then what an instance has, each with its signature or type and its doc
    if (kind === "class" && sym.valueDeclaration) {
      const rows: string[] = []
      const fmt = ts.TypeFormatFlags.NoTruncation
      const member = (m: import("typescript").Symbol, on: string) => {
        if (m.name.startsWith("_") || m.name.startsWith("#") || m.name === "prototype") return
        const d = m.valueDeclaration ?? m.declarations?.[0]
        if (!d || (ts.getCombinedModifierFlags(d as import("typescript").Declaration) & (ts.ModifierFlags.Private | ts.ModifierFlags.Protected))) return
        const t = checker.getTypeOfSymbolAtLocation(m, d)
        const sig = t.getCallSignatures()[0]
        const shown = sig ? `${on}${m.name}${oneLine(checker.signatureToString(sig, undefined, fmt))}` : `${on}${m.name}: ${oneLine(checker.typeToString(t, undefined, fmt))}`
        rows.push(`| \`${cell(shown.length > 160 ? shown.slice(0, 157) + "…" : shown)}\` | ${cell(described(m, bv?.doc)) || "–"} |`)
      }
      const statics = checker.getTypeOfSymbolAtLocation(sym, sym.valueDeclaration)
      for (const c of statics.getConstructSignatures().slice(0, 2)) if (c.getParameters().length && !(c.getDeclaration() && ts.getCombinedModifierFlags(c.getDeclaration() as import("typescript").Declaration) & ts.ModifierFlags.Private)) rows.push(`| \`${cell(`new ${ex.name}${oneLine(checker.signatureToString(c, undefined, fmt)).replace(/:\s*[^:)]*$/, "")}`)}\` | ${cell(ctorDoc(c, ex.name)) || "–"} |`)
      for (const m of checker.getPropertiesOfType(statics)) member(m, `${ex.name}.`)
      for (const m of checker.getPropertiesOfType(checker.getDeclaredTypeOfSymbol(sym))) member(m, ".")
      if (rows.length) md.push("| Member | What it does |", "| --- | --- |", ...rows.slice(0, 60), "")
      if (rows.length > 60) md.push(`And ${rows.length - 60} more members; see the source.`, "")
    }
    if (kind === "component") {
      const p = propsOf(sym)
      if (p) {
        const pname = p.name.replace(/<.*$/, "")
        if (shared(pname)) md.push(`Takes ${typeLink(pname, pageName(entry))}'s props.`, "")
        const own = p.props.filter((s) => !ownerOf(s) || ownerOf(s) === p.name.replace(/<.*$/, ""))
        const inherited = new Map<string, string[]>()
        for (const s of p.props) if (!own.includes(s)) inherited.set(ownerOf(s), [...(inherited.get(ownerOf(s)) ?? []), s.name])
        if (own.length && !shared(pname)) {
          md.push("| Prop | Type | What it does |", "| --- | --- | --- |")
          for (const s of own) {
            const opt = s.flags & ts.SymbolFlags.Optional ? "" : " (required)"
            md.push(`| \`${s.name}\`${opt} | \`${cell(typeText(s))}\` | ${cell(described(s, bv?.doc)) || "–"} |`)
          }
          md.push("")
        }
        const named = shared(pname) ? [] : [...inherited].filter(([o]) => o), loose = shared(pname) ? [] : inherited.get("") ?? []
        if (named.length) md.push(`Also takes the props of ${named.map(([o, n]) => `${typeLink(o, pageName(entry))}${home.has(o) && n.length > 6 ? "" : ` (${n.map((x) => "`" + x + "`").join(", ")})`}`).join("; ")}.`, "")
        if (loose.length) md.push(`Also takes: ${loose.map((n) => "`" + n + "`").join(", ")}.`, "")
      }
    }
    if (kind === "type" && propsFor.has(ex.name)) md.push(`The props of [\`${propsFor.get(ex.name)}\`](#${propsFor.get(ex.name)!.toLowerCase()}), tabled there.`, "")
    else     if (kind === "type") {
      const d = sym.declarations?.[0]
      if (d && (ts.isInterfaceDeclaration(d) || ts.isTypeAliasDeclaration(d))) {
        const fields = ts.isInterfaceDeclaration(d) ? checker.getPropertiesOfType(checker.getDeclaredTypeOfSymbol(sym)) : []
        if (fields.length && fields.length <= 40) {
          md.push("| Field | Type | What it is |", "| --- | --- | --- |")
          for (const s of fields) md.push(`| \`${s.name}\`${s.flags & ts.SymbolFlags.Optional ? "" : " (required)"} | \`${cell(typeText(s))}\` | ${cell(described(s, bvFields?.doc)) || "–"} |`)
          md.push("")
        } else if (ts.isTypeAliasDeclaration(d)) {
          const t = oneLine(d.type.getText())
          md.push("```ts", `type ${ex.name} = ${t.length > 300 ? t.slice(0, 297) + "…" : t}`, "```", "")
        }
      }
    }
    const uses = kind === "type" ? [] : usedIn(entry, ex.name)
    if (uses.length) md.push(`Used in: ${uses.slice(0, 8).map(exampleLink).join(", ")}${uses.length > 8 ? `, and ${uses.length - 8} more` : ""}.`, "")
    groups.set(kind, [...(groups.get(kind) ?? []), md.join("\n")])
  }
  const page = pageName(entry)
  const counts = ORDER.filter((k) => groups.has(k)).map((k) => plural(groups.get(k)!.length, ONE[k], SECTION[k].toLowerCase())).join(", ")
  const at = groupAt.get(page)
  if (at === undefined) problems.push(`${entry}: in no group (GROUPS in tools/manual-ref/sdk.ts)`)
  const pageFile = path.join(out, page + ".md")
  const kept = own(pageFile)
  // a new page's head: its title, and the summary its file's header gives, to be rewritten by hand
  const said = what ? plain(what).replace(/\.$/, "") + "." : `What \`${entry}\` exports.`
  const head = (kept?.head ?? ["---", `title: ${JSON.stringify(entry)}`, `summary: ${JSON.stringify(oneLine(said))}`, "order: 0", `sources: ${JSON.stringify(["sdk/raea/src/" + file])}`, "---", "", oneLine(said), ""].join("\n")).replace(/^order: .*$/m, `order: ${at ?? 99}`)
  const values = exps.filter((e) => real(e).flags & ts.SymbolFlags.Value)
  const block = [
    BEGIN,
    "",
    "```ts",
    `import { ${values.slice(0, 4).map((e) => e.name).join(", ")} } from "${entry}"`,
    "```",
    "",
    `\`${entry}\` exports ${counts}.${bevy ? ` Bevy's counterpart: \`${bevy}\`.` : ""} Source: [sdk/raea/src/${file}](${REPO}/sdk/raea/src/${file}).`,
    "",
    ...ORDER.filter((k) => groups.has(k)).flatMap((k) => [`## ${SECTION[k]}`, "", ...groups.get(k)!]),
    END,
  ].join("\n").replace(/\n{3,}/g, "\n\n")
  emit(pageFile, join(head, block, kept?.tail ?? ""))
  index.push({ entry, page, what: summaryOf(head), counts })
})

// the index: its own prose above, the paths by group below
const indexFile = path.join(out, "index.md")
const ikept = own(indexFile)
const ihead = ikept?.head ?? ["---", 'title: "SDK"', 'summary: "Every raea import path and everything it exports, generated from the SDK."', "order: 1", 'sources: ["sdk/raea/src"]', "---", ""].join("\n")
const list = [BEGIN, ""]
for (const [g, what, ps] of GROUPS) {
  list.push(`## ${g}`, "", what, "", "| Import | What it's for | Exports |", "| --- | --- | --- |")
  for (const p of ps) {
    const x = index.find((r) => r.page === p)
    if (x) list.push(`| [\`${x.entry}\`](${x.page}.md) | ${cell(x.what)} | ${x.counts} |`)
    else problems.push(`${p}: in GROUPS but sdk/raea/src has no such entry`)
  }
  list.push("")
}
list.push(END)
emit(indexFile, join(ihead, list.join("\n"), ikept?.tail ?? ""))
if (problems.length) {
  console.error(problems.map((p) => "sdk-ref: " + p).join("\n"))
  if (mode === "check") process.exit(1)
}
console.log(`sdk-ref: ${index.length} import paths, ${problems.length} problem(s)`)
