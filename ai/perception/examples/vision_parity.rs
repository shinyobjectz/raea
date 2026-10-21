// What each Vision backend gives perception's callers, per frame, as JSON lines: `vision_parity apple|portable PNG...`
// (the portable-perception measurement in host docs/checks.feature: layout, person, framing, subject, attention, room)
use raea_perception::vision::Vision;
use serde_json::json;

fn grid(m: &[u8], mw: usize, mh: usize, n: usize) -> String {
    (0..n * n).map(|i| { let (x, y) = (i % n * mw / n, i / n * mh / n); if mw > 0 && m[y * mw + x] > 127 { '1' } else { '0' } }).collect()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let v: Box<dyn Vision> = match args[0].as_str() {
        "apple" => Box::new(raea_perception::vision::AppleVision::default()),
        _ => Box::new(raea_perception::portable::PortableVision::new()?),
    };
    for path in &args[1..] {
        let img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let px = img.as_raw();
        let layout = raea_perception::vision::layout(v.as_ref(), px, w, h, &[])?;
        let p = v.people(px, w, h, false)?;
        let r = v.read(px, w, h, false)?;
        let att = v.attention(px, w, h)?.map(|(m, mw, mh)| {
            let total: f32 = m.iter().map(|x| x.max(0.0)).sum();
            let (mut cx, mut cy) = (0.0, 0.0);
            let mut best = (0, f32::MIN);
            for (i, x) in m.iter().enumerate() {
                let q = x.max(0.0) / total.max(1e-9);
                cx += q * ((i % mw) as f32 + 0.5) / mw as f32;
                cy += q * ((i / mw) as f32 + 0.5) / mh as f32;
                if *x > best.1 { best = (i, *x); }
            }
            json!({"centre": [cx, cy], "peak": [((best.0 % mw) as f32 + 0.5) / mw as f32, ((best.0 / mw) as f32 + 0.5) / mh as f32]})
        });
        let room = v.room(px, w, h)?;
        println!("{}", json!({"file": path, "layout": layout, "person": p.person.map(|b| [b.x, b.y, b.w, b.h]), "mask": grid(&p.mask, p.mw, p.mh, 48),
            "framing": r.framing, "subject": r.subject, "attention": att, "room": room}));
    }
    Ok(())
}
