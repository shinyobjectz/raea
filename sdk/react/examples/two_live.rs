//! P2 probe (void.feature: a space is the interface): two live compositions in one thread, each painted to a texture
//! of its own, as a space's panel and its object would be. Run from raea/react:
//! cargo run --release --example two_live -- PANEL.tsx OBJECT.tsx OUT_DIR
use raea_react::live::Live;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (panel, object, out) = (&a[1], &a[2], std::path::PathBuf::from(&a[3]));
    std::fs::create_dir_all(&out).unwrap();
    let t = std::time::Instant::now();
    let mut p = Live::open_with(panel.as_ref(), (260, 600), true).expect("panel");
    let mut o = Live::open_with(object.as_ref(), (1280, 720), true).expect("object");
    println!("both open in {:.0} ms: panel {}x{} responsive {}, object {}x{}", t.elapsed().as_secs_f64() * 1e3, p.w, p.h, p.responsive, o.w, o.h);
    for f in 0..120 {
        p.step(f, &[], None).expect("panel step");
        o.step(f, &[], None).expect("object step");
        if f % 60 == 0 {
            for (name, l, id) in [("panel", &mut p, u64::MAX - 100), ("object", &mut o, u64::MAX - 101)] {
                let (w, h) = l.paint_texture_fit(id, (l.w, l.h)).expect("paint");
                let (tw, th, px) = raea::quads::read_texture(id).expect("read");
                let lit = px.chunks(4).filter(|c| c[0] as u32 + c[1] as u32 + c[2] as u32 > 30).count();
                println!("frame {f} {name}: {w}x{h} (texture {tw}x{th}), {:.1}% lit", 100.0 * lit as f64 / (tw * th) as f64);
                image::save_buffer(out.join(format!("{name}-{f}.png")), &px, tw, th, image::ColorType::Rgba8).unwrap();
            }
        }
    }
    println!("120 frames of both in {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    // the same two, each on a thread of its own (void: the stage's thread and the panels' thread), painting at once
    let t = std::time::Instant::now();
    let hs: Vec<_> = [(panel.clone(), (260u32, 600u32), u64::MAX - 102), (object.clone(), (1280, 720), u64::MAX - 103)]
        .into_iter()
        .map(|(f, size, id)| std::thread::spawn(move || {
            let mut l = Live::open_with(f.as_ref(), size, true).expect("open");
            for f in 0..240 {
                l.step(f, &[], None).expect("step");
                l.paint_texture_fit(id, (l.w, l.h)).expect("paint");
            }
            raea::quads::read_texture(id).expect("read")
        }))
        .collect();
    for (i, h) in hs.into_iter().enumerate() {
        let (w, hh, px) = h.join().expect("thread");
        image::save_buffer(out.join(format!("thread-{i}.png")), &px, w, hh, image::ColorType::Rgba8).unwrap();
    }
    println!("240 frames each on two threads, painted every frame, in {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
}
