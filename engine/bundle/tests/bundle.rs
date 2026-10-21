//! A work folder bundled as raea-motion's bundle_fresh lays it out: tsconfig `paths`, the automatic JSX runtime from
//! node_modules, process.env.NODE_ENV defined, and the same bytes from any folder (lightspeed T3: a bundle that names
//! its work folder changes every function the incremental bake compares).
use std::path::{Path, PathBuf};

fn folder(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("raea-bundle-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let w = |p: &str, s: &str| {
        let f = d.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    };
    w("tsconfig.json", r#"{ "compilerOptions": { "jsx": "react-jsx", "jsxImportSource": "react", "baseUrl": ".", "paths": { "@lib/*": ["./lib/*"] } } }"#);
    w("node_modules/react/package.json", r#"{ "name": "react", "version": "0.0.0", "exports": { "./jsx-runtime": "./jsx-runtime.js" } }"#);
    w("node_modules/react/jsx-runtime.js", "export const jsx = (t, p) => ({ t, p }); export const jsxs = jsx;");
    w("lib/greet.ts", "export const greet = (n: string): string => `hello ${n}`;");
    w("entry.tsx", "import { greet } from \"@lib/greet\";\nconst el = <div title={greet(\"raea\")} />;\n(globalThis as any).out = [el, process.env.NODE_ENV];\n");
    d
}

#[test]
fn bundles_a_work_folder_the_same_from_anywhere() {
    let (a, b) = (folder("a"), folder("b-longer-name"));
    let ja = raea_bundle::bundle(&a.join("entry.tsx")).expect("bundles").js;
    let jb = raea_bundle::bundle(&b.join("entry.tsx")).expect("bundles").js;
    for d in [&a, &b] {
        let _ = std::fs::remove_dir_all(d);
    }
    assert!(ja.contains("hello ${n}"), "the paths alias resolved: {ja}");
    assert!(ja.contains("\"production\""), "NODE_ENV defined: {ja}");
    assert!(ja.contains("({ t, p })") || ja.contains("t,\n") || ja.contains("jsx"), "the automatic runtime from node_modules: {ja}");
    assert!(!ja.contains("<div"), "JSX compiled");
    assert!(!ja.contains(&*std::env::temp_dir().to_string_lossy()) && !ja.contains("raea-bundle-test"), "no folder named: {ja}");
    assert_eq!(ja, jb, "the same bytes from two folders");
}

#[test]
fn reports_a_missing_import() {
    let d = folder("missing");
    std::fs::write(d.join("entry.tsx"), "import { x } from \"./nowhere\";\nconsole.log(x);\n").unwrap();
    let e = raea_bundle::bundle(Path::new(&d.join("entry.tsx"))).expect_err("can't resolve it");
    let _ = std::fs::remove_dir_all(&d);
    assert!(e.contains("nowhere"), "names what it couldn't find: {e}");
}

/// CSS (A2): a global stylesheet with an @import and nesting, and a CSS module with `composes`, imported by the entry
fn css_folder(name: &str) -> PathBuf {
    let d = folder(name);
    let w = |p: &str, s: &str| std::fs::write(d.join(p), s).unwrap();
    w("base.css", "body { margin: 0 }\n");
    w("site.css", "@import \"./base.css\";\n.card { color: red; & .title { font-weight: 700 } }\n");
    w("card.module.css", ".shell { padding: 4px }\n.card { composes: shell; background: oklch(70% 0.1 200) }\n");
    w("entry.tsx", "import \"./site.css\";\nimport s from \"./card.module.css\";\n(globalThis as any).out = s.card;\n");
    d
}

#[test]
fn bundles_css_imports_and_modules() {
    let (a, b) = (css_folder("css-a"), css_folder("css-b-other"));
    let ba = raea_bundle::bundle(&a.join("entry.tsx")).expect("bundles");
    let bb = raea_bundle::bundle(&b.join("entry.tsx")).expect("bundles");
    for d in [&a, &b] {
        let _ = std::fs::remove_dir_all(d);
    }
    let css = &ba.css;
    // the order the modules run: site.css (base.css inlined first), then the module
    let (base, card, shell) = (css.find("margin: 0").unwrap_or_else(|| panic!("@import inlined: {css:?} js: {}", ba.js)), css.find(".card").unwrap(), css.find("shell").unwrap());
    assert!(base < card && card < shell, "in import order: {css}");
    assert!(css.contains(".card .title"), "nesting written out: {css}");
    assert!(!css.contains("oklch"), "oklch() lowered for Stylo: {css}");
    // the module's classes: its file, the class, its CSS's hash; `composes` adds the composed class
    let class = regex_like(&ba.js, "card-module_card_").expect("the module's class map in the script");
    assert!(css.contains(&format!(".{class}")), "the class {class} in the CSS: {css}");
    assert!(ba.js.contains("card-module_shell_"), "composes names the composed class: {}", ba.js);
    assert_eq!(ba, bb, "the same script and CSS from two folders");
}

/// the first token in `s` starting with `prefix`
fn regex_like(s: &str, prefix: &str) -> Option<String> {
    let i = s.find(prefix)?;
    Some(s[i..].chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect())
}

#[test]
fn no_css_is_empty() {
    let d = folder("no-css");
    let b = raea_bundle::bundle(&d.join("entry.tsx")).expect("bundles");
    let _ = std::fs::remove_dir_all(&d);
    assert_eq!(b.css, "");
}

/// A script's bundle with the engine's resolution (script.rs, as react/js/bundle.ts did on bun's)
#[test]
fn bundles_a_script_with_the_engines_resolution() {
    let d = std::env::temp_dir().join(format!("raea-bundle-script-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let w = |p: &str, s: &str| {
        let f = d.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    };
    w("js/shims/test.ts", "export const test = (n: string) => 'TEST_SHIM:' + n;\n");
    w("js/shims/node/zlib.ts", "export const gzip = () => 'ZLIB_SHIM';\n");
    w("js/node_modules/react/package.json", r#"{ "name": "react", "main": "index.js" }"#);
    w("js/node_modules/react/index.js", "export const version = 'ENGINE_REACT';\n");
    w("motion/index.tsx", "export const motion = 'MOTION_INDEX';\n");
    w("motion/ecs.ts", "export const ecs = 'MOTION_ECS';\n");
    w("lib/aliased.ts", "export const aliased = 'ALIASED';\n");
    // the script's own folder carries another react: the engine's must win
    w("work/node_modules/react/package.json", r#"{ "name": "react", "main": "index.js" }"#);
    w("work/node_modules/react/index.js", "export const version = 'OTHER_REACT';\n");
    w("work/entry.ts", r#"
import { motion } from "@raea/motion";
import { ecs } from "@raea/motion/ecs";
import { test } from "bun:test";
import { gzip } from "zlib";
import fs from "node:fs";
import { aliased } from "~lib";
import { version } from "react";
(globalThis as any).out = [motion, ecs, test("x"), gzip(), aliased, version, typeof fs, FLAG];
"#);
    let script = raea_bundle::script::Script {
        js: d.join("js"),
        motion: d.join("motion"),
        browser: d.join("nope.ts"),
        shell: d.join("nope.ts"),
        aliases: [("~lib".to_string(), d.join("lib/aliased.ts"))].into_iter().collect(),
        stubs: vec!["fs".into()],
    };
    let o = raea_bundle::Options { z3: false, script: Some(script.clone()), defines: vec![("FLAG".into(), "\"ON\"".into())], edit: None };
    let b = raea_bundle::bundle_with(&d.join("work/entry.ts"), o).expect("bundles");
    for want in ["MOTION_INDEX", "MOTION_ECS", "TEST_SHIM:", "ZLIB_SHIM", "ALIASED", "ENGINE_REACT", "isn't available in raea's V8", "\"ON\""] {
        assert!(b.js.contains(want), "{want} in the bundle: {}", b.js);
    }
    assert!(!b.js.contains("OTHER_REACT"), "one React: the engine's");
    // a Node built-in raea doesn't carry, not listed as a stub: the bundle fails, naming it
    w("work/bad.ts", "import { createServer } from \"http\";\nconsole.log(createServer);\n");
    let e = raea_bundle::bundle_with(&d.join("work/bad.ts"), raea_bundle::Options { z3: false, script: Some(script), defines: vec![], edit: None }).expect_err("http isn't carried");
    assert!(e.contains("\"http\" is a Node built-in") && e.contains("--node-stub http"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

/// A composition's bundle sees the edit's own packages (edit.rs): its sources are copied to a work folder whose
/// node_modules is the engine's, so a package only the edit installed (motion/react) was missing. The edit's package
/// importing react gets the engine's React, not the edit's own copy.
#[test]
fn bundles_a_composition_with_the_edits_packages() {
    let d = std::env::temp_dir().join(format!("raea-bundle-edit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let w = |p: &str, s: &str| {
        let f = d.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    };
    w("js/node_modules/react/package.json", r#"{ "name": "react", "main": "index.js" }"#);
    w("js/node_modules/react/index.js", "export const version = 'ENGINE_REACT';\n");
    // the edit's folder: its own react, and a package (with a subpath export, as motion/react) that imports react
    w("edit/node_modules/react/package.json", r#"{ "name": "react", "main": "index.js" }"#);
    w("edit/node_modules/react/index.js", "export const version = 'EDIT_REACT';\n");
    w("edit/node_modules/anim/package.json", r#"{ "name": "anim", "exports": { "./react": "./react.js" } }"#);
    w("edit/node_modules/anim/react.js", "import { version } from 'react';\nexport const anim = 'EDIT_ANIM with ' + version;\n");
    // the work folder raea-motion makes: the edit's source copied, node_modules the engine's
    w("work/src/edit.tsx", "import { anim } from 'anim/react';\nimport { version } from 'react';\n(globalThis as any).out = [anim, version];\n");
    w("work/entry.ts", "import './src/edit.tsx';\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink(d.join("js/node_modules"), d.join("work/node_modules")).unwrap();
    let edit = raea_bundle::edit::Edit { dir: d.join("edit"), js: d.join("js"), work: d.join("work") };
    let b = raea_bundle::bundle_with(&d.join("work/entry.ts"), raea_bundle::Options { edit: Some(edit), ..Default::default() }).expect("bundles");
    assert!(b.js.contains("EDIT_ANIM with ") && b.js.contains("ENGINE_REACT"), "the edit's package, the engine's React: {}", b.js);
    assert!(!b.js.contains("EDIT_REACT"), "one React: the engine's");
    // without it, as before: the edit's package isn't found, and Rolldown leaves it an external global (undefined when the
    // composition runs)
    let before = raea_bundle::bundle_with(&d.join("work/entry.ts"), raea_bundle::Options::default()).expect("bundles");
    assert!(!before.js.contains("EDIT_ANIM"), "anim/react only resolves from the edit's folder");
    let _ = std::fs::remove_dir_all(&d);
}
