// @raea/data/props: each component's props as its TypeScript says (the type of its default export's first parameter),
// for raea-brief to check a brief's `[[part props]]` against: a prop's kind (string, number, boolean, array, object),
// the values a string union allows, and whether it's required.
//
//   bun ~/raea/data/src/props.ts SPACE     -> JSON on stdout: {component: {prop: {kind, values?, required}}}
//                                            (cached in SPACE/.score/props.json while no component changes)

// TypeScript 5: the last with the JavaScript compiler API (7, the native port, has none); tsc 7 still type-checks the package
import ts from "typescript5";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "fs";
import { join } from "path";

export interface Prop { kind: "string" | "number" | "boolean" | "array" | "object" | "any"; values?: string[]; required: boolean }

function kindOf(t: ts.Type, checker: ts.TypeChecker): Pick<Prop, "kind" | "values"> {
  const parts = t.isUnion() ? t.types.filter((x) => !(x.flags & (ts.TypeFlags.Undefined | ts.TypeFlags.Null))) : [t];
  if (parts.length && parts.every((x) => x.isStringLiteral())) return { kind: "string", values: parts.map((x) => (x as ts.StringLiteralType).value) };
  if (parts.length && parts.every((x) => x.flags & ts.TypeFlags.BooleanLiteral)) return { kind: "boolean" }; // boolean is true | false
  const one = parts.length === 1 ? parts[0] : null;
  if (!one) return { kind: "any" };
  if (one.flags & (ts.TypeFlags.String | ts.TypeFlags.StringLiteral)) return { kind: "string" };
  if (one.flags & (ts.TypeFlags.Number | ts.TypeFlags.NumberLiteral)) return { kind: "number" };
  if (one.flags & (ts.TypeFlags.Boolean | ts.TypeFlags.BooleanLiteral)) return { kind: "boolean" };
  if (checker.isArrayType(one)) return { kind: "array" };
  if (one.flags & ts.TypeFlags.Object) return { kind: "object" };
  return { kind: "any" };
}

/** The props of every components/*.tsx in a space (the `_tw` wrappers the compiler writes left out). */
export function componentProps(space: string): Record<string, Record<string, Prop>> {
  const dir = join(space, "components");
  if (!existsSync(dir)) return {};
  const files = readdirSync(dir).filter((f) => f.endsWith(".tsx") && !f.endsWith("_tw.tsx")).map((f) => join(dir, f));
  const program = ts.createProgram(files, { jsx: ts.JsxEmit.ReactJSX, strict: true, noEmit: true, skipLibCheck: true, allowJs: false, target: ts.ScriptTarget.ES2022, moduleResolution: ts.ModuleResolutionKind.Bundler, module: ts.ModuleKind.ESNext });
  const checker = program.getTypeChecker();
  const out: Record<string, Record<string, Prop>> = {};
  for (const f of files) {
    const sf = program.getSourceFile(f);
    if (!sf) continue;
    const mod = checker.getSymbolAtLocation(sf);
    const def = mod && checker.getExportsOfModule(mod).find((s) => s.escapedName === "default");
    if (!def) continue;
    const decl = def.valueDeclaration ?? def.declarations?.[0];
    if (!decl) continue;
    const fnType = checker.getTypeOfSymbolAtLocation(def, decl);
    const sig = fnType.getCallSignatures()[0];
    const param = sig?.getParameters()[0];
    const props: Record<string, Prop> = {};
    if (param) {
      const pt = checker.getTypeOfSymbolAtLocation(param, decl);
      for (const p of pt.getProperties()) {
        const t = checker.getTypeOfSymbolAtLocation(p, decl);
        props[p.name] = { ...kindOf(t, checker), required: !(p.flags & ts.SymbolFlags.Optional) };
      }
    }
    out[f.split("/").pop()!.replace(/\.tsx$/, "")] = props;
  }
  return out;
}

if (import.meta.main) {
  const space = process.argv[2];
  const cache = join(space, ".score", "props.json");
  const dir = join(space, "components");
  const newest = existsSync(dir) ? Math.max(0, ...readdirSync(dir).map((f) => statSync(join(dir, f)).mtimeMs)) : 0;
  if (existsSync(cache) && statSync(cache).mtimeMs > newest) {
    process.stdout.write(readFileSync(cache, "utf8"));
  } else {
    const json = JSON.stringify(componentProps(space));
    mkdirSync(join(space, ".score"), { recursive: true });
    writeFileSync(cache, json);
    process.stdout.write(json);
  }
}
