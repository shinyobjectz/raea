//! B2 spike: React (bevy-react's reconciler and bridge) on jitless V8 in a headless Bevy app; prints the entity tree
//! React's commits built. Run from raea/react: cargo run --release --example spike [bundle.js]
use bevy::prelude::*;
use raea_react::{ReactEntities, ReactKind, ReactNode, ReactOutbox, ReactPlugin, dump};

fn main() {
    let bundle = std::env::args().nth(1).unwrap_or_else(|| "js/dist/bundle.js".into());
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(bundle));
    let t0 = std::time::Instant::now();
    let mut last = (0, 0);
    let mut quiet = 0;
    // frames until React's commits stop arriving (or 5 s)
    while t0.elapsed().as_secs_f64() < 5.0 {
        app.update();
        let e = app.world().resource::<ReactEntities>();
        let now = (e.commits, e.ops);
        quiet = if now == last && now.0 > 0 { quiet + 1 } else { 0 };
        last = now;
        if quiet > 20 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let e = app.world().resource::<ReactEntities>();
    println!("{} commits, {} ops, {} entities in {:.0} ms", e.commits, e.ops, e.by_id.len(), t0.elapsed().as_secs_f64() * 1e3);
    print!("{}", dump(app.world_mut()));
    // Bevy -> React: click the button; its onClick sets state, React re-renders, the update comes back as ops
    let mut q = app.world_mut().query::<(&ReactNode, &ReactKind)>();
    let button = q.iter(app.world()).find(|(_, k)| k.0 == "button").map(|(n, _)| *n);
    if let Some(b) = button {
        let before = app.world().resource::<ReactEntities>().commits;
        let t1 = std::time::Instant::now();
        app.world().resource::<ReactOutbox>().click(b);
        while t1.elapsed().as_secs_f64() < 3.0 && app.world().resource::<ReactEntities>().commits == before {
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let e = app.world().resource::<ReactEntities>();
        println!("after a click: {} commits (+{}), round trip {:.1} ms", e.commits, e.commits - before, t1.elapsed().as_secs_f64() * 1e3);
        print!("{}", dump(app.world_mut()));
    }
}
