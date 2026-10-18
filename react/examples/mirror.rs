//! B3: React (bevy-react) commits become entities, mirrored into a raea::dom page; style (Stylo), layout and text
//! (Taffy, Parley) and raster (Vello) run as Bevy systems on the frames React changed it. Writes the picture to a PNG, to compare with the same App through react-dom in today's engine.
//! Run from raea/react: cargo run --release --example mirror -- out.png [bundle.js] [--click]
use bevy::prelude::*;
use raea_react::{DomFrame, DomNode, DomPlugin, ReactEntities, ReactKind, ReactNode, ReactOutbox, ReactPlugin};

fn settle(app: &mut App, secs: f64) {
    let t0 = std::time::Instant::now();
    let (mut last, mut quiet) = ((0, 0), 0);
    while t0.elapsed().as_secs_f64() < secs {
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
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().cloned().unwrap_or_else(|| "mirror.png".into());
    let bundle = args.get(1).filter(|a| !a.starts_with("--")).cloned().unwrap_or_else(|| "js/dist/bundle.js".into());
    let (w, h) = (540, 960);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(bundle));
    app.add_plugins(DomPlugin { width: w, height: h, dir: ".".into() });
    settle(&mut app, 5.0);
    let report = |app: &mut App, what: &str| {
        let tagged = app.world_mut().query::<&DomNode>().iter(app.world()).count();
        let f = app.world().resource::<DomFrame>();
        println!("{what}: style {:.2} ms, layout {:.2} ms, raster {:.2} ms (painted {} times; {tagged} entities with a page node)", f.style_ms, f.layout_ms, f.raster_ms, f.painted);
    };
    report(&mut app, "first frame");
    if args.iter().any(|a| a == "--click") {
        let mut q = app.world_mut().query::<(&ReactNode, &ReactKind)>();
        if let Some(b) = q.iter(app.world()).find(|(_, k)| k.0 == "button").map(|(n, _)| *n) {
            app.world().resource::<ReactOutbox>().click(b);
            settle(&mut app, 3.0);
            report(&mut app, "after a click");
        }
    }
    let f = app.world().resource::<DomFrame>();
    image::save_buffer(&out, &f.rgba, w, h, image::ExtendedColorType::Rgba8).expect("png");
    let e = app.world().resource::<ReactEntities>();
    println!("{} commits, {} ops, {} entities -> {out}", e.commits, e.ops, e.by_id.len());
}
