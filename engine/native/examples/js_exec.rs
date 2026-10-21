//! js-exec on JavaScriptCore: TypeScript, project imports, timers, process.exit, a rejection, and a run past its time.
//! `cargo run --release --features modules --example js_exec`
use raea_native::js::{Entry, run};
use std::time::Duration;

fn main() {
    let dir = std::env::temp_dir().join(format!("raea-js-exec-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    let dir = dir.canonicalize().unwrap();
    std::fs::write(dir.join("lib/add.ts"), "export const add = (a: number, b: number): number => a + b;\nexport let count = 0;\nexport function bump() { count++; }\n").unwrap();
    std::fs::write(dir.join("main.ts"), "import { add, count, bump } from './lib/add';\nbump();\nconsole.log('sum', add(2, 3), count);\nawait new Promise((r) => setTimeout(r, 30));\nconsole.error('late');\n").unwrap();
    let go = |e: Entry, ms: u64| run(&dir, &dir, e, vec!["a".into()], Default::default(), String::new(), vec![], Duration::from_millis(ms));
    let r = go(Entry::File(dir.join("main.ts")), 5000);
    println!("file: exit {} out {:?} err {:?}", r.exit, r.stdout, r.stderr);
    assert_eq!((r.exit, r.stdout.as_str(), r.stderr.as_str()), (0, "sum 5 1\n", "late\n"));
    let r = go(Entry::Code { text: "console.log(process.argv[0]); process.exit(3);".into(), ts: true, dir: dir.clone() }, 5000);
    println!("exit: {} {:?}", r.exit, r.stdout);
    assert_eq!((r.exit, r.stdout.as_str()), (3, "a\n"));
    let r = go(Entry::Code { text: "await Promise.reject(new Error('nope'));".into(), ts: true, dir: dir.clone() }, 5000);
    println!("reject: {} {:?}", r.exit, r.stderr.lines().next());
    assert!(r.exit == 1 && r.stderr.contains("nope"));
    let r = go(Entry::Code { text: "import x from './missing'; console.log(x);".into(), ts: true, dir: dir.clone() }, 5000);
    println!("missing: {} {:?}", r.exit, r.stderr.lines().next());
    assert_eq!(r.exit, 1);
    let t = std::time::Instant::now();
    let r = go(Entry::Code { text: "while (true) {}".into(), ts: false, dir: dir.clone() }, 300);
    println!("loop: {} {:?} in {} ms", r.exit, r.stderr, t.elapsed().as_millis());
    assert_eq!(r.exit, 124);
    let r = go(Entry::Code { text: "setInterval(() => {}, 10);".into(), ts: false, dir: dir.clone() }, 300);
    println!("interval: {} {:?}", r.exit, r.stderr);
    assert_eq!(r.exit, 124);
    std::fs::remove_dir_all(&dir).ok();
    println!("js-exec on JavaScriptCore: all as expected");
}
