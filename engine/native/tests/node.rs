//! Node's core on JavaScriptCore (js::run_node): a TypeScript script using fs, path, os, crypto, child_process,
//! process and Buffer, an ES module package resolved through package.json's exports, and a CommonJS package (imported,
//! and required from inside another), each result what Node gives.
#![cfg(feature = "modules")]
use std::time::Duration;

#[test]
fn runs_a_tool_as_node_would() {
    let d = std::env::temp_dir().join(format!("raea-node-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let w = |p: &str, s: &str| {
        let f = d.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    };
    w("node_modules/esm-pkg/package.json", r#"{ "name": "esm-pkg", "type": "module", "exports": { ".": { "import": "./lib/index.js", "require": "./nope.cjs" }, "./extra": "./lib/extra.js" } }"#);
    w("node_modules/esm-pkg/lib/index.js", "export const twice = (x) => x * 2;\nexport default 'esm-default';\n");
    w("node_modules/esm-pkg/lib/extra.js", "export const extra = 'extra';\n");
    w("node_modules/cjs-pkg/package.json", r#"{ "name": "cjs-pkg", "main": "main.js" }"#);
    w("node_modules/cjs-pkg/main.js", "const { join } = require('path');\nconst helper = require('./helper');\nmodule.exports = { joined: join('a', 'b'), helper: helper.name, self: typeof __dirname };\n");
    w("node_modules/cjs-pkg/helper.js", "exports.name = 'helper';\n");
    w("data.json", r#"{ "n": 3 }"#);
    w("tool.ts", r#"
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync, appendFileSync, rmSync } from "fs";
import { join, dirname, basename, relative, extname } from "path";
import { tmpdir } from "os";
import { createHash } from "crypto";
import { spawnSync } from "child_process";
import esm, { twice } from "esm-pkg";
import { extra } from "esm-pkg/extra";
import cjs from "cjs-pkg";
import data from "./data.json";
import { createRequire } from "module";
const out: Record<string, unknown> = {};
const dir = join(process.argv[2], "work");
mkdirSync(join(dir, "deep/er"), { recursive: true });
writeFileSync(join(dir, "a.txt"), "héllo");
appendFileSync(join(dir, "a.txt"), "!");
writeFileSync(join(dir, "b.bin"), new Uint8Array([0, 255, 7]));
out.text = readFileSync(join(dir, "a.txt"), "utf8");
out.bytes = Array.from(readFileSync(join(dir, "b.bin")));
out.isBuffer = Buffer.isBuffer(readFileSync(join(dir, "b.bin")));
out.list = readdirSync(dir).sort();
out.types = readdirSync(dir, { withFileTypes: true }).map((e) => e.name + ":" + (e.isDirectory() ? "d" : "f")).sort();
out.size = statSync(join(dir, "a.txt")).size;
out.mtime = statSync(join(dir, "a.txt")).mtimeMs > 0;
rmSync(join(dir, "deep"), { recursive: true, force: true });
out.gone = existsSync(join(dir, "deep"));
let missing = "";
try { readFileSync(join(dir, "nope")); } catch (e) { missing = (e as any).code; }
out.missing = missing;
out.path = [dirname("/a/b/c.ts"), basename("/a/b/c.ts", ".ts"), extname("x.tar.gz"), relative("/a/b", "/a/c/d"), join("a", "..", "b", "./c")];
out.tmp = tmpdir().length > 0;
out.sha = createHash("sha256").update("abc").digest("hex");
out.sha1 = createHash("sha1").update(Buffer.from("abc")).digest("base64");
const echo = spawnSync("/bin/sh", ["-c", "printf out; printf err >&2; exit 3"]);
out.spawn = [echo.status, echo.stdout.toString(), echo.stderr.toString()];
out.b64 = Buffer.from("héllo").toString("base64") + "|" + Buffer.from("aMOpbGxv", "base64").toString();
out.pkgs = [esm, twice(21), extra, cjs.joined, cjs.helper, cjs.self];
out.json = data.n;
out.argv = process.argv.slice(3);
out.env = typeof process.env.HOME;
out.req = createRequire(import.meta.url)("cjs-pkg").helper;
process.stdout.write(JSON.stringify(out));
process.exit(5);
"#);
    let r = raea_native::js::run_node(&d.join("tool.ts"), &[d.display().to_string(), "x".into(), "y".into()], String::new(), Duration::from_secs(20));
    let _ = std::fs::remove_dir_all(&d);
    assert_eq!(r.exit, 5, "process.exit(5); stderr: {}", r.stderr);
    let got: serde_json::Value = serde_json::from_str(&r.stdout).unwrap_or_else(|e| panic!("{e}: {} / {}", r.stdout, r.stderr));
    // what Node 22 gives for the same script
    let want = serde_json::json!({
        "text": "héllo!", "bytes": [0, 255, 7], "isBuffer": true, "list": ["a.txt", "b.bin", "deep"], "types": ["a.txt:f", "b.bin:f", "deep:d"],
        "size": 7, "mtime": true, "gone": false, "missing": "ENOENT",
        "path": ["/a/b", "c", ".gz", "../c/d", "b/c"], "tmp": true,
        "sha": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "sha1": "qZk+NkcGgWq6PiVxeFDCbJzQ2J0=",
        "spawn": [3, "out", "err"], "b64": "aMOpbGxv|héllo", "pkgs": ["esm-default", 42, "extra", "a/b", "helper", "string"],
        "json": 3, "argv": ["x", "y"], "env": "string", "req": "helper"
    });
    assert_eq!(got, want);
}
