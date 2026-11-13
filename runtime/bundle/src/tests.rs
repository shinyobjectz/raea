#![cfg(test)]
use super::*;

#[test]
fn rewrite_into_points_imports_and_re_exports_at_the_copies() {
    // the SDK's names resolve through the tree's sdk/tsconfig.base.json
    let m = &raea_io_home::home().join("sdk/raea/src");
    let src = "import { a } from \"raea\";\nimport { useCurrentFrame } from \"raea/clock\";\nimport { Clock } from \"@raea/app/clock\";\nexport { sao, lyria } from \"raea/asset\";\nexport * from \"raea/video\";\nexport type { T } from \"raea/native-calls\";\nexport default 1\nimport b from \"raea\";\nexport function f() { return \"from\"; }\n";
    let out = rewrite_into(src, Path::new("/nowhere/app"), m, "../motion", None).unwrap();
    assert!(out.contains("import { a } from \"../motion/prelude\""));
    // a module whose code moved into a package is that package's copy, beside motion/
    assert!(out.contains("import { useCurrentFrame } from \"../app/clock\""));
    assert!(out.contains("import { Clock } from \"../app/clock\""));
    assert!(out.contains("export { sao, lyria } from \"../motion/asset\""));
    assert!(out.contains("export * from \"../motion/video\""));
    assert!(out.contains("export type { T } from \"../motion/native-calls\""));
    // an `export default` with no semicolon isn't swallowed into the import after it
    assert!(out.contains("export default 1\nimport b from \"../motion/prelude\""));
    assert!(out.contains("export function f() { return \"from\"; }"));
}

#[test]
fn an_import_written_in_a_string_template_or_comment_is_left_as_text() {
    // a code chip's text (a launch film types `import { RigidBody } from "raea/plugins/avian"`) isn't an import
    let m = &raea_io_home::home().join("sdk/raea/src");
    let src = "import { a } from \"raea\";\nconst chip = `import { b } from \"raea/clock\"; ${ {x: `import c from \"raea\"`}.x }`;\n// import d from \"raea\";\n/* import e from \"raea\"; */\nconst s = 'import f from \"raea\"';\nconst r = /import g from \"raea\"/;\nimport { h } from \"raea/clock\";\n";
    let out = rewrite_into(src, Path::new("/nowhere/app"), m, "../motion", None).unwrap();
    assert!(out.starts_with("import { a } from \"../motion/prelude\""), "{out}");
    assert!(out.ends_with("import { h } from \"../app/clock\";\n"), "{out}");
    for kept in ["`import { b } from \"raea/clock\"; ${", "`import c from \"raea\"`", "// import d from \"raea\";", "/* import e from \"raea\"; */", "'import f from \"raea\"'", "/import g from \"raea\"/"] {
        assert!(out.contains(kept), "{kept} rewritten in {out}");
    }
}

#[test]
fn point_sdks_climbs_from_a_nested_file() {
    let src = "import { useClockState } from \"@raea/app/clock\";\nimport { x } from \"@raea/nope\";\n";
    assert_eq!(point_sdks(src, "../", &sdks()), src.replace("@raea/app/clock", "../app/clock"));
    assert_eq!(point_sdks(src, "../../", &sdks()), src.replace("@raea/app/clock", "../../app/clock"));
    assert_eq!(point_sdks("import { C } from \"@raea/ui/canvas\";", "../", &sdks()), "import { C } from \"../ui/canvas\";");
}

#[test]
fn outside_copies_what_the_edit_imports_beyond_its_folder() {
    let t = std::env::temp_dir().join(format!("raea-outside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&t);
    let w = |p: &str, s: &str| {
        let f = t.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    };
    w("proj/edit/a.tsx", "import { b } from \"../lib/b\";\nimport { x } from \"./x\";\nimport take from \"./take.mp4\";\n");
    w("proj/edit/x.ts", "export const x = 1;\n");
    // b and c import each other: each copied once
    w("proj/lib/b.tsx", "import { c } from \"./c\";\nimport { x } from \"../edit/x\";\nimport { a } from \"raea\";\nexport const b = 1;\n");
    w("proj/lib/c.ts", "export { b } from \"./b\";\nexport const c = 2;\n");
    let (root, dir) = (t.join("proj"), t.join("proj/edit"));
    let roots = vec![("project", root.clone())];
    let outs = outside_in(&dir, &roots).unwrap();
    assert_eq!(outs.iter().map(|o| o.copy.clone()).collect::<Vec<_>>(), [PathBuf::from("ext/project/lib/b.tsx"), PathBuf::from("ext/project/lib/c.ts")]);
    // the edit's file points at the copy; its own and its media imports stay
    let a = point_outside(&std::fs::read_to_string(dir.join("a.tsx")).unwrap(), &dir, Path::new("src"), &dir, &outs);
    assert!(a.contains("from \"../ext/project/lib/b.tsx\"") && a.contains("from \"./x\"") && a.contains("from \"./take.mp4\""), "{a}");
    // the copy points at its neighbour's copy and back into src/; the SDK's name is rewrite_into's
    let b = point_outside(&std::fs::read_to_string(root.join("lib/b.tsx")).unwrap(), &root.join("lib"), Path::new("ext/project/lib"), &dir, &outs);
    assert!(b.contains("from \"./c.ts\"") && b.contains("from \"../../../src/x.ts\"") && b.contains("from \"raea\""), "{b}");
    // beyond the roots: refused, its importer named
    w("proj/lib/c.ts", "import { y } from \"../../y\";\n");
    w("y.ts", "export const y = 0;\n");
    let e = outside_in(&dir, &roots).unwrap_err();
    assert!(e.contains("lib/c.ts") && e.contains("outside raea's home"), "{e}");
    std::fs::remove_dir_all(&t).unwrap();
}

#[test]
fn a_project_s_own_tsconfig_is_not_in_the_work_copy() {
    // raea writes a tsconfig.json into every new project: copied with the sources, it shadowed the work folder's
    let d = std::env::temp_dir().join(format!("raea-bundle-tsconfig-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("from/components")).unwrap();
    for f in ["from/tsconfig.json", "from/components/jsconfig.json", "from/a.tsx", "from/components/data.json"] {
        std::fs::write(d.join(f), "{}").unwrap();
    }
    copy_dir(&d.join("from"), &d.join("to"), 0).unwrap();
    drop_tsconfigs(&d.join("to"), 0);
    assert!(!d.join("to/tsconfig.json").exists() && !d.join("to/components/jsconfig.json").exists());
    assert!(d.join("to/a.tsx").exists() && d.join("to/components/data.json").exists());
    // the project's own files are untouched
    assert!(d.join("from/tsconfig.json").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_edit_s_sources_leave_out_hidden_folders_and_packages() {
    let d = std::env::temp_dir().join(format!("raea-bundle-sources-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for f in ["a.tsx", "parts/b.ts", ".git/hooks/c.js", "node_modules/pkg/d.js", "target/e.ts", "out/f.json"] {
        std::fs::create_dir_all(d.join(f).parent().unwrap()).unwrap();
        std::fs::write(d.join(f), "").unwrap();
    }
    let mut got = sources(&d).unwrap();
    got.sort();
    assert_eq!(got, vec![PathBuf::from("a.tsx"), PathBuf::from("parts/b.ts")]);
    let _ = std::fs::remove_dir_all(&d);
}
