use super::*;

#[test]
fn a_name_is_its_folder() {
    assert_eq!(slug("Shane's Studio"), "shane-s-studio");
    assert_eq!(slug("  CJ / Reel 2 "), "cj-reel-2");
}

#[test]
fn up_finds_the_nearest_marker() {
    let root = dunce::canonicalize(std::env::temp_dir()).unwrap().join(format!("raea-ws-up-{}", std::process::id()));
    let deep = root.join("projects/launch/media");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::create_dir_all(root.join(".raea")).unwrap();
    std::fs::write(root.join(".raea/workspace"), "").unwrap();
    assert_eq!(up(&deep, ".raea/workspace").as_deref(), Some(root.as_path()));
    assert_eq!(up(&deep, ".raea/project"), None);
    let _ = std::fs::remove_dir_all(&root);
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("raea-ws-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn next_walks_a_project_from_brief_to_show() {
    let d = scratch("next");
    std::fs::create_dir_all(d.join(".raea")).unwrap();
    std::fs::write(d.join(".raea/project"), "").unwrap();
    let shaped = next::Brief { needs: vec![], modes: vec!["tool".into()], approve: vec![] };
    let unshaped = next::Brief { needs: vec!["who it's for (audience)".into()], modes: vec!["pitch".into()], approve: vec![] };
    // a project that approves its script first (the brief's approve), in any mode: the script, then nothing edited till yes
    let approves = next::Brief { needs: vec![], modes: vec!["tool".into()], approve: vec!["script".into()] };
    assert!(next::steps(&d, &approves, 0)[0].starts_with("plan it"));
    std::fs::write(d.join("ad.script.ts"), "").unwrap();
    assert!(next::steps(&d, &approves, 0)[0].starts_with("this project approves its script before anything is edited: show ad.script.ts"));
    assert!(next::steps(&d, &shaped, 0).iter().all(|s| !s.contains("approves")));
    std::fs::remove_file(d.join("ad.script.ts")).unwrap();
    let s = next::steps(&d, &unshaped, 0);
    assert!(s[0].starts_with("shape the brief: who it's for") && s[1].starts_with("plan it") && s[2].starts_with("make the first item"), "{s:?}");
    assert!(next::steps(&d, &shaped, 2)[0].starts_with("raea doctor: 2"));
    std::fs::write(d.join("card.tsx"), "export const Card = () => null;\n").unwrap();
    let s = next::steps(&d, &shaped, 0);
    assert!(s[0].starts_with("connect the project's files: 1 thing(s) aren't connected, first card.tsx:1: isn't a composition"), "{s:?}");
    std::fs::write(d.join("piece.tsx"), "import { Composition, registerRoot } from \"raea\";\nimport { Card } from \"./card\";\nregisterRoot(() => <Composition id=\"piece\" component={Card} width={1080} height={1920} fps={30} durationInFrames={90} />);\n").unwrap();
    assert!(next::steps(&d, &shaped, 0)[0].starts_with("render piece.tsx"), "{:?}", next::steps(&d, &shaped, 0));
    // the rest is read from the project's record (DR3): what each render and check found
    use raea_core_facts::{Finding, Level, record::keep};
    let (piece, video) = (d.join("piece.tsx"), d.join("out/piece.mp4"));
    std::fs::create_dir_all(d.join("out")).unwrap();
    std::fs::write(&video, "").unwrap();
    let clips = Finding::new("clips", Level::Error, "the rider is in the seat", "raea render");
    keep("raea render", &piece, Some(&video), &[clips], vec![], "20261110-160000").unwrap();
    assert!(next::steps(&d, &shaped, 0)[0].starts_with("fix piece.tsx: clips: the rider is in the seat (its render 20261110-160000-piece found it"), "{:?}", next::steps(&d, &shaped, 0));
    keep("raea render", &piece, Some(&video), &[], vec![], "20261110-160001").unwrap();
    assert!(next::steps(&d, &shaped, 0)[0].starts_with("check out/piece.mp4: `raea check out/piece.mp4 -o out/piece.check.json`"));
    keep("raea check", &video, None, &[Finding::new("clipped", Level::Error, "text clipped at 2 s", "raea check")], vec![], "20261110-160002").unwrap();
    assert_eq!(next::steps(&d, &shaped, 0)[0], "polish out/piece.mp4: clipped: text clipped at 2 s (`raea guide polish`)");
    keep("raea check", &video, None, &[Finding::new("reading", Level::Note, "minor", "raea check")], vec![], "20261110-160003").unwrap();
    assert!(next::steps(&d, &shaped, 0)[0].starts_with("show out/piece.mp4"));
    // stills (or a review) since the video don't replace it
    keep("raea render", &piece, Some(&d.join("out")), &[], vec![], "20261110-160004").unwrap();
    assert!(next::steps(&d, &shaped, 0)[0].starts_with("show out/piece.mp4"), "{:?}", next::steps(&d, &shaped, 0));
    // the composition changed after the videos were made (their entries made old), before the stills: the video again
    for id in ["20261110-160000-piece", "20261110-160001-piece"] {
        let rec = d.join(format!(".raea/record/{id}.json"));
        let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&rec).unwrap()).unwrap();
        v["at"] = 1.into();
        std::fs::write(&rec, v.to_string()).unwrap();
    }
    let changed = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100);
    std::fs::File::options().write(true).open(&piece).unwrap().set_modified(changed).unwrap();
    assert_eq!(next::steps(&d, &shaped, 0)[0], "render the video again: piece.tsx changed after 20261110-160001-piece made out/piece.mp4");
    // no video at all: the video next
    for id in ["20261110-160000-piece", "20261110-160001-piece"] {
        std::fs::remove_file(d.join(format!(".raea/record/{id}.json"))).unwrap();
    }
    assert_eq!(next::steps(&d, &shaped, 0)[0], "render the video: `raea render piece.tsx -o out/piece.mp4`");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn doctor_connects_a_projects_files() {
    let d = scratch("doctor-pj8");
    std::fs::create_dir_all(d.join(".raea")).unwrap();
    std::fs::write(d.join(".raea/project"), "").unwrap();
    std::fs::write(d.join("loose.tsx"), "export const Loose = () => null;\n").unwrap();
    std::fs::create_dir_all(d.join("controllers")).unwrap();
    std::fs::write(d.join("controllers/pace.ts"), "import { controller } from \"raea/brief\";\nexport default controller({ title: \"Pace\", fields: {} });\n").unwrap();
    std::fs::write(d.join("brief.ts"), "import { brief } from \"raea/brief\";\nexport default brief({\n  title: \"Old\",\n  stages: [{ name: \"\", tasks: [{ text: \"Write the plan\" }] }],\n});\n").unwrap();
    let found = doctor::registry_findings(&d);
    let says: Vec<&String> = found.iter().map(|f| &f.says).collect();
    assert_eq!(found.len(), 1, "{says:?}");
    assert!(says[0].contains("loose.tsx:1: isn't a composition") && !found[0].fixable(), "{says:?}");
    // a controller from before 11-20 is told where its parts go, and kept
    let out = doctor::doctor(&["--fix".into(), d.display().to_string()], &[], "0.0.1", &|_| Ok(String::new())).unwrap();
    assert!(out.starts_with("APPLIED  2 of 5 fixed") && out.contains("controllers/pace.ts: a controller, which raea no longer reads") && out.contains("taste.ts"), "{out}");
    // a brief's old tasks are said to belong in plan.org (docs/plan.feature PL8a), and kept
    assert!(out.contains("brief.ts: keeps tasks in `stages`") && out.contains("plan.org"), "{out}");
    assert!(d.join("controllers/pace.ts").is_file() && std::fs::read_to_string(d.join("brief.ts")).unwrap().contains("stages"), "nothing of the person's is deleted");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn doctor_finds_old_briefs_and_old_skills() {
    let d = scratch("doctor");
    std::fs::write(d.join("acme.brief"), "title: x").unwrap();
    std::fs::create_dir_all(d.join("skills/raea")).unwrap();
    std::fs::write(d.join("skills/raea/SKILL.md"), "---\nname: raea\nversion: 0.0.1\n---\n").unwrap();
    let skills = [("claude", d.join("skills"))];
    let found = doctor::findings(&d, &skills, "0.0.2");
    assert_eq!(found.len(), 2, "{:?}", found.iter().map(|f| &f.says).collect::<Vec<_>>());
    assert!(found[0].says.contains("acme.brief") && !found[0].fixable());
    assert!(found[1].says.contains("is 0.0.1, this raea is 0.0.2") && found[1].fixable());
    assert!(doctor::findings(&d, &skills, "0.0.1").len() == 1);
    let reinstalled = std::cell::Cell::new(false);
    let out = doctor::doctor(&["--fix".into(), d.display().to_string()], &skills, "0.0.2", &|a| { reinstalled.set(a == "claude"); Ok(String::new()) }).unwrap();
    assert!(reinstalled.get() && out.starts_with("APPLIED  1 of 2 fixed"), "{out}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_root_with_none_set() {
    let home = scratch("home-root");
    assert_eq!(default_root(&home), home.join("Documents/raea"));
    // never ~/Studio, even with a workspace in it (Shane 11-14: "no internal no studio"; raea.root says otherwise)
    std::fs::create_dir_all(home.join("Studio/acme/.raea")).unwrap();
    std::fs::write(home.join("Studio/acme/.raea/workspace"), "").unwrap();
    assert_eq!(default_root(&home), home.join("Documents/raea"));
    // iCloud keeping Documents: ~/raea
    let synced = scratch("home-icloud");
    std::fs::create_dir_all(synced.join("Library/Mobile Documents/com~apple~CloudDocs/Documents")).unwrap();
    assert_eq!(default_root(&synced), synced.join("raea"));
}

#[test]
fn a_workspace_reads_back_through_git() {
    // Windows: canonicalize() gives \\?\C:\..., a path Git for Windows can neither read nor lock, and one that shares no
    // prefix with the C:/... git prints, so the include climbed to the root (../../../../\\?\C:/...) and every git
    // command in the workspace failed; ids were never written and nothing read back (0.1.5, 0.1.6)
    let ws = scratch("plain");
    let made = crate::workspace::workspace(&["adopt".into(), ws.display().to_string(), "--name".into(), "Plain".into()]);
    assert!(made.is_ok(), "{made:?}");
    let git = |dir: &Path, a: &[&str]| {
        let o = std::process::Command::new("git").arg("-C").arg(dir).args(a).output().unwrap();
        (o.status.success(), String::from_utf8_lossy(&o.stdout).trim().to_string(), String::from_utf8_lossy(&o.stderr).trim().to_string())
    };
    let (ok, inc, err) = git(&ws, &["config", "--get-all", "include.path"]);
    assert!(ok && inc == "../.raea/workspace", "include.path {inc:?} {err}");
    let (ok, id, err) = git(&ws, &["config", "workspace.id"]);
    assert!(ok && id.starts_with("ws_"), "workspace.id {id:?} {err}");
    let found = up(&ws.join("projects"), ".raea/workspace").expect("up finds the workspace");
    assert!(!found.to_string_lossy().starts_with(r"\\?\"), "{}", found.display());
    crate::workspace::new_project_in(&found, "Desk", None).unwrap();
    let (ok, pid, err) = git(&ws.join("projects/desk"), &["config", "-f", ".raea/project", "project.id"]);
    assert!(ok && pid.starts_with("pj_"), "project.id {pid:?} {err}");
    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn an_include_path_is_relative_with_slashes() {
    use crate::workspace::relative;
    assert_eq!(relative(Path::new("/w/.git"), Path::new("/w/.raea/workspace")), "../.raea/workspace");
    assert_eq!(relative(Path::new("/w/.git/modules/projects/p"), Path::new("/w/projects/p/.raea/project")), "../../../../projects/p/.raea/project");
    #[cfg(windows)]
    {
        // git's C:/... and a plain C:\... are one root; another drive is named whole, not climbed to
        assert_eq!(relative(Path::new("C:/w/.git"), Path::new(r"C:\w\.raea\workspace")), "../.raea/workspace");
        assert_eq!(relative(Path::new(r"C:\w\.git"), Path::new(r"D:\o\.raea\workspace")), "D:/o/.raea/workspace");
    }
}

#[test]
fn a_new_workspace_and_project_have_the_layout() {
    // docs/layout.feature LY1: a folder per kind in the project and in the workspace's shared/, no media/ brand/ parts/
    let ws = scratch("layout");
    crate::workspace::make_workspace(&ws, "Layout", "monorepo").unwrap();
    for k in crate::workspace::KINDS {
        assert!(ws.join("shared").join(k).join(".gitkeep").is_file(), "shared/{k}");
    }
    assert!(!ws.join("controllers").exists() && !ws.join("artifacts").exists());
    let attrs = std::fs::read_to_string(ws.join(".gitattributes")).unwrap();
    assert!(attrs.contains("**/assets/** filter=lfs"), "{attrs}");
    crate::workspace::new_project_in(&ws, "Desk Test", None).unwrap();
    let p = ws.join("projects/desk-test");
    for k in crate::workspace::KINDS.iter().chain(["out"].iter()) {
        assert!(p.join(k).join(".gitkeep").is_file(), "{k}");
    }
    for gone in ["media", "brand", "parts"] {
        assert!(!p.join(gone).exists(), "{gone}");
    }
    assert!(std::fs::read_to_string(p.join("tsconfig.json")).unwrap().contains("\"@shared/*\": [\"../../shared/*\"]"));
    assert!(std::fs::read_to_string(p.join("AGENTS.md")).unwrap().contains("@shared/components/desk"));
    // committed: the workspace's repo has every kept folder
    let tracked = std::process::Command::new("git").args(["ls-files"]).current_dir(&ws).output().unwrap();
    let tracked = String::from_utf8_lossy(&tracked.stdout);
    assert!(tracked.contains("projects/desk-test/components/.gitkeep") && tracked.contains("shared/assets/.gitkeep"), "{tracked}");
    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn a_verbatim_include_is_dropped_and_nothing_else() {
    // .git/config as raea 0.1.6 left it on Windows (#169, read off the test PC), with an [includeIf] and a good include
    let text = "[core]\n\trepositoryformatversion = 0\n[include]\n\tpath = ../../../../../../../../\\\\\\\\?\\\\C:/\\\\/Users/you/Documents/raea/acme/.raea/workspace\n[includeIf \"gitdir:~/x/\"]\n\tpath = ../keep?\\\\x\n[include]\n\tpath = ../.raea/workspace\n";
    let (fixed, dropped) = crate::repair::without_verbatim(text);
    assert!(dropped);
    assert_eq!(fixed, "[core]\n\trepositoryformatversion = 0\n[include]\n[includeIf \"gitdir:~/x/\"]\n\tpath = ../keep?\\\\x\n[include]\n\tpath = ../.raea/workspace\n");
    assert_eq!(crate::repair::without_verbatim(&fixed).0, fixed);
    assert!(!crate::repair::without_verbatim("[include]\r\n\tpath = ../.raea/workspace\r\n").1);
}

#[test]
fn a_workspace_0_1_6_broke_is_repaired() {
    // updated from 0.1.6: its include names a \\?\ path; doctor says so, and adopt (as init inside it) puts it right
    let ws = scratch("repair");
    crate::workspace::workspace(&["adopt".into(), ws.display().to_string(), "--name".into(), "Repair".into()]).unwrap();
    let cfg = ws.join(".git/config");
    let bad = "\tpath = ../../../../\\\\\\\\?\\\\C:/\\\\/Users/you/Documents/raea/repair/.raea/workspace\n";
    let text = std::fs::read_to_string(&cfg).unwrap().replace("\tpath = ../.raea/workspace\n", bad);
    std::fs::write(&cfg, text).unwrap();
    // on Windows this is #169 itself: Git for Windows refuses the include, so every git command in the repo fails
    let status = || std::process::Command::new("git").arg("-C").arg(&ws).args(["status", "--short"]).output().unwrap();
    if cfg!(windows) {
        assert!(!status().status.success(), "git ran in a repo whose include names a \\\\?\\ path");
    }
    assert!(crate::repair::has_verbatim_include(&ws));
    let said: Vec<String> = crate::doctor::findings(&ws, &[], "test").into_iter().filter(|f| f.fixable()).map(|f| f.says).collect();
    assert!(said.iter().any(|s| s.contains("Git for Windows can't open")), "{said:?}");
    crate::workspace::workspace(&["adopt".into(), ws.display().to_string()]).unwrap();
    assert!(!crate::repair::has_verbatim_include(&ws));
    let o = std::process::Command::new("git").arg("-C").arg(&ws).args(["config", "--get-all", "include.path"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "../.raea/workspace");
    assert!(status().status.success(), "git status after the repair: {}", String::from_utf8_lossy(&status().stderr));
    let _ = std::fs::remove_dir_all(&ws);
}
