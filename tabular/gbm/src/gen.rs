//! J2b's training renders: short edits over raea's fixture footage or a rendered 3D scene (a third of them), each with random overlays (boxes and badges, images
//! as stickers, titles on pills, stroked captions; still, sliding or fading, sometimes rotated), rendered by rdb twice:
//! as written, and with every overlay at opacity 0. Each pair is a perfect overlay mask for every frame.

use std::path::{Path, PathBuf};

struct Rng(u64);
impl Rng {
    fn u(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn f(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * (self.u() % 10_000) as f64 / 10_000.0
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[(self.u() as usize) % v.len()]
    }
}

const WORDS: [&str; 16] = ["New drop", "50% off", "Link in bio", "Day 14", "Before", "After", "Try it", "Why it works", "Step 2", "Real results",
    "No filter", "My routine", "Shop now", "Wait for it", "Part 3", "Honest review"];
const COLOURS: [&str; 10] = ["#ffffff", "#111111", "#ffd84d", "#ff5a5f", "#2ec4b6", "#3a86ff", "#8338ec", "#fb5607", "#06d6a0", "rgba(0,0,0,0.55)"];

/// The footage under the overlays: raea's fixtures and the host app's takes (21 on this Mac), each with its length.
fn sources(raea: &Path) -> Vec<(PathBuf, f64)> {
    let f = raea.join("media/fixtures");
    let mut v = vec![(f.join("take-h264.mp4"), 25.0), (f.join("broll-rot.mov"), 7.5), (f.join("broll-screen.mov"), 3.5)];
    // the host app's takes made the real renders' scores worse (busier backgrounds, 2026-10-15): only with RAEA_GEN_TAKES
    if std::env::var_os("RAEA_GEN_TAKES").is_none() {
        return v;
    }
    let takes = raea.parent().map(|h| h.join("host/takes")).unwrap_or_default();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&takes).map(|rd| rd.flatten().map(|e| e.path().join("video.mov")).filter(|p| p.exists()).collect()).unwrap_or_default();
    paths.sort();
    if let Ok(be) = raea_media::backend("apple") {
        for p in paths {
            if let Ok(d) = be.open_video(&p) {
                let len = d.info().duration;
                if len > 3.0 {
                    v.push((p, len - 0.5));
                }
            }
        }
    }
    v
}

fn stickers(raea: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(raea.join("engine/react/probe")).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "png")).collect()).unwrap_or_default();
    v.sort();
    v
}

/// One edit's SQL: a clip of footage and 1-4 overlays; `bare` turns every overlay kind's opacity to 0.
fn edit_sql(rng: &mut Rng, raea: &Path, srcs: &[(PathBuf, f64)], frames: usize) -> (String, String) {
    let mut sql = format!("INSERT INTO edit VALUES (30, 1080, 1920, {frames});\n");
    if rng.u() % 3 == 0 {
        // a rendered 3D scene behind (the hook's kind: a flat backdrop, a floor, moving shapes), its colours varied
        let bgs = ["#1b1f3a", "#f4efe6", "#0d0d0d", "#2b4c7e", "#f7d6e0"];
        sql += &format!("INSERT INTO camera (id, x, y, z, look_y, bg) VALUES ('cam', {:.1}, 2.2, {:.1}, 1, '{}');\nINSERT INTO light (id, dir_x, dir_y, dir_z, lux) VALUES ('sun', -0.5, -1, -0.4, 9000);\n\
            INSERT INTO plane (id, size, color) VALUES ('floor', 30, '{}');\nINSERT INTO cube (id, size, color, y, rough) VALUES ('hero', {:.1}, '{}', 1.4, 0.3);\n\
            INSERT INTO sphere (id, class, size, color, y, rough, metal) SELECT 'ring' || i, 'ring', 0.38, '{}', 0.6, 0.2, 0.3 FROM range(10) t(i);\n\
            UPDATE cube SET rot_y = frame * 2 * pi() / 180 WHERE id = 'hero';\n\
            UPDATE sphere SET x = 2.4 * cos(2 * pi() * (row_number() OVER () / count(*) OVER () + frame * 0.004)), z = 2.4 * sin(2 * pi() * (row_number() OVER () / count(*) OVER () + frame * 0.004)) WHERE class = 'ring';\n",
            rng.f(-1.0, 1.0), rng.f(7.0, 11.0), rng.pick(&bgs), rng.pick(&bgs), rng.f(1.0, 2.2), rng.pick(&COLOURS[2..9]), rng.pick(&COLOURS[..3]));
    } else {
        let (src, len) = rng.pick(srcs).clone();
        let start = rng.f(0.0, (len - frames as f64 / 30.0).max(0.0));
        sql += &format!("INSERT INTO clip (id, src, src_in, \"from\", \"to\", zoom) VALUES ('bg', '{}', {start:.2}, 0, {frames}, {:.2});\n", src.display(), rng.f(1.0, 1.3));
    }
    let stick = stickers(raea);
    let n = 1 + (rng.u() % 4) as usize;
    let mut kinds = Vec::new();
    for i in 0..n {
        let kind = *rng.pick(&["box", "image", "title", "caption"]);
        let (x, y) = (rng.f(40.0, 760.0), rng.f(120.0, 1650.0));
        let id = format!("o{i}");
        let (from, to) = (if rng.u() % 3 == 0 { (rng.u() % 30) as usize } else { 0 }, frames);
        match kind {
            "box" => {
                let (w, h) = (rng.f(80.0, 420.0), rng.f(40.0, 260.0));
                let border = if rng.u() % 2 == 0 { format!(", '{}px solid {}'", (rng.u() % 6) + 1, rng.pick(&COLOURS)) } else { ", ''".into() };
                sql += &format!("INSERT INTO box (id, x, y, w, h, bg, radius, border, \"from\", \"to\") VALUES ('{id}', {x:.0}, {y:.0}, {w:.0}, {h:.0}, '{}', {:.0}{border}, {from}, {to});\n",
                    rng.pick(&COLOURS), rng.f(0.0, 60.0));
            }
            "image" if !stick.is_empty() => {
                let s = rng.f(120.0, 380.0);
                sql += &format!("INSERT INTO image (id, src, x, y, width, height, radius, \"from\", \"to\") VALUES ('{id}', '{}', {x:.0}, {y:.0}, {s:.0}, {:.0}, {:.0}, {from}, {to});\n",
                    rng.pick(&stick).display(), s * rng.f(0.6, 1.4), rng.f(0.0, 40.0));
            }
            "title" => {
                // mostly where editors put them: the top band
                let y = if rng.u() % 4 != 0 { rng.f(160.0, 420.0) } else { y };
                sql += &format!("INSERT INTO title (id, text, font, weight, size, color, bg, padding, radius, x, y, w, align, \"from\", \"to\") VALUES ('{id}', '{}', 'Avenir Next', 800, {:.0}, '{}', '{}', {:.0}, {:.0}, {x:.0}, {y:.0}, {:.0}, 'center', {from}, {to});\n",
                    rng.pick(&WORDS), rng.f(40.0, 96.0), rng.pick(&COLOURS), rng.pick(&COLOURS), rng.f(8.0, 30.0), rng.f(0.0, 30.0), rng.f(300.0, 900.0));
            }
            _ => {
                // as editors caption: 2-4 short chunks one after another, 0.5-1.3 s each, centred in the lower third,
                // stroked and usually shadowed
                let shadow = if rng.u() % 4 != 0 { format!("0 {:.0}px {:.0}px rgba(0,0,0,{:.2})", rng.f(2.0, 6.0), rng.f(8.0, 24.0), rng.f(0.3, 0.7)) } else { String::new() };
                let (cy, size, colour, stroke) = (rng.f(1200.0, 1500.0), rng.f(56.0, 84.0), *rng.pick(&["#ffffff", "#ffd84d", "#ffffff"]), rng.u() % 4);
                let mut at = from;
                let mut c = 0;
                while at < to && c < 4 {
                    let len = rng.f(15.0, 40.0) as usize;
                    sql += &format!("INSERT INTO caption (id, text, font, weight, size, color, x, y, w, align, stroke, text_shadow, \"from\", \"to\") VALUES ('{id}c{c}', '{}', 'Avenir Next', 800, {size:.0}, '{colour}', 60, {cy:.0}, 960, 'center', '{stroke}px #000', '{shadow}', {at}, {});\n",
                        rng.pick(&WORDS), (at + len).min(to));
                    at += len;
                    c += 1;
                }
                kinds.push("caption");
                continue;
            }
        }
        let k = match kind { "image" if stick.is_empty() => continue, k => k };
        kinds.push(k);
        // motion: still, a slide in, a fade in, a rotation
        match rng.u() % 4 {
            1 => sql += &format!("INSERT INTO keys VALUES ('{id}', 'x', {from}, {:.0}, '{}'), ('{id}', 'x', {}, {x:.0}, '{}');\n", x - rng.f(150.0, 500.0), rng.pick(&["out", "inout", "linear"]), from + 12, rng.pick(&["out", "inout"])),
            2 => sql += &format!("INSERT INTO keys VALUES ('{id}', 'opacity', {from}, 0, 'linear'), ('{id}', 'opacity', {}, 1, 'linear');\n", from + 10),
            3 => sql += &format!("UPDATE {k} SET rotate = {:.0} WHERE id = '{id}';\n", rng.f(-12.0, 12.0)),
            _ => {}
        }
    }
    kinds.sort();
    kinds.dedup();
    let bare = format!("{sql}{}", kinds.iter().map(|k| format!("UPDATE {k} SET opacity = 0;\n")).collect::<String>());
    (sql, bare)
}

/// `n` render pairs in `dir` (rendered by rdb), as (full, bare) paths; pairs already rendered are kept.
pub fn generate(n: usize, frames: usize, dir: &Path) -> Result<Vec<(String, String)>, String> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let raea = home.join("raea");
    let rdb = raea.join("rdb/target/release/rdb");
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut rng = Rng(11);
    let srcs = sources(&raea);
    eprintln!("{} footage sources", srcs.len());
    let mut out = Vec::new();
    for i in 0..n {
        let (sql, bare) = edit_sql(&mut rng, &raea, &srcs, frames);
        let (fs, bs) = (dir.join(format!("e{i:03}.sql")), dir.join(format!("e{i:03}-bare.sql")));
        let (fm, bm) = (dir.join(format!("e{i:03}.mp4")), dir.join(format!("e{i:03}-bare.mp4")));
        if !(fm.exists() && bm.exists()) {
            std::fs::write(&fs, &sql).map_err(|e| e.to_string())?;
            std::fs::write(&bs, &bare).map_err(|e| e.to_string())?;
            for (s, m) in [(&fs, &fm), (&bs, &bm)] {
                let r = std::process::Command::new(&rdb).arg("render").arg(s).arg("-o").arg(m).output().map_err(|e| e.to_string())?;
                if !r.status.success() {
                    eprintln!("e{i:03}: {}", String::from_utf8_lossy(&r.stderr).lines().last().unwrap_or(""));
                }
            }
        }
        if fm.exists() && bm.exists() {
            out.push((fm.display().to_string(), bm.display().to_string()));
        }
    }
    Ok(out)
}
