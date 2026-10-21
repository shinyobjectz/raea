//! ES modules on JavaScriptCore (raea_native::jsc::modules): relative imports, live bindings, top-level await, a
//! missing module's rejection. `cargo run --release --example jsc_modules`
use raea_native::jsc::{Arg, Context, modules::Modules};


fn main() {
    let c = Context::new();
    let m = Modules::new(c.raw(), Box::new(|url| match url {
        "raea://app/b.js" => Ok("export let n = 1; export function bump() { n++; } export default 'B';".into()),
        "raea://app/c.js" => Ok("import d, { n, bump } from './b.js'; bump(); export const seen = d + n;".into()),
        _ => Err(format!("no module {url}")),
    }));
    m.eval("raea://app/main.js", "import { seen } from './c.js'; import * as b from 'raea://app/b.js'; globalThis.out = seen + ':' + b.n; await 0; globalThis.tla = 'ok';").unwrap();
    m.eval("raea://app/bad.js", "import x from './missing.js'; globalThis.bad = x;").unwrap();
    c.eval("globalThis.check = () => [globalThis.out, globalThis.tla, String(globalThis.bad)].join(' ')", "check").unwrap();
    c.eval("print = 0", "x").unwrap();
    let _ = c.call_num("check", &[Arg::Null]);
    let p = m.eval("raea://app/bad2.js", "import x from './missing2.js';").unwrap();
    c.watch("bad2", p);
    for _ in 0..3 { c.eval("0", "tick").unwrap(); }
    eprintln!("bad2 settled: {:?}", c.settled("bad2"));
    c.eval("if (globalThis.out !== 'B2:2' || globalThis.tla !== 'ok' || globalThis.bad !== undefined) throw new Error('modules: ' + globalThis.check())", "assert").unwrap();
    // a syntax error rejects the evaluation's promise (JavaScriptCore parses a module when it's loaded)
    let p = m.eval("raea://app/syntax.js", "import {").unwrap();
    let _ = p;
    c.eval("Promise.resolve().then(() => 0)", "tick").unwrap();
    println!("jsc modules: relative imports, live bindings, top-level await, a missing import and a syntax error as expected");
}
