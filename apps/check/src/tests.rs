#![cfg(test)]
use super::*;

#[test]
fn raea_help_names_every_flag() {
    let missing: Vec<_> = TAKES.iter().filter(|f| !USAGE.contains(&format!("[{f} "))).collect();
    assert!(missing.is_empty(), "raea check takes flags its usage doesn't name: {missing:?}");
}

#[test]
fn a_placeholder_brand_kit_is_flagged() {
    let dir = std::env::temp_dir().join(format!("raea-check-brand-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("brand")).unwrap();
    let video = dir.join("cut.mp4");
    let rules = |kit: Option<&str>| {
        let _ = std::fs::remove_file(dir.join("brand/brand.json"));
        std::fs::write(dir.join("brand/brand.css"), ":root {}").unwrap();
        if let Some(k) = kit {
            std::fs::write(dir.join("brand/brand.json"), k).unwrap();
        }
        let (mut issues, mut passed) = (Vec::new(), Vec::new());
        brand_kit(&["", video.to_str().unwrap()], &mut issues, &mut passed);
        (issues.iter().map(|i| i.p).collect::<Vec<_>>(), passed.len())
    };
    assert_eq!(rules(Some(r#"{"domain":"acme.test","source":{"thin":["no font-family in its CSS"]}}"#)), (vec![1], 0));
    assert_eq!(rules(Some(r#"{"domain":"acme.test","source":{"thin":["x"],"by_hand":{"palette.accent":"hero button"}}}"#)), (vec![], 1));
    assert_eq!(rules(Some(r#"{"domain":"acme.test","source":{"thin":[]}}"#)), (vec![], 1));
    assert_eq!(rules(None), (vec![2], 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn flat_dark_is_clean_and_stepped_dark_is_blotchy() {
    let (w, h) = (320, 240);
    let flat = vec![12u8; w * h];
    assert_eq!(blotch_score(&flat, w, h), Some(0.0));
    // 8x8 patches stepping between 10 and 12: block-shaped structure where the picture should be flat
    let blotchy: Vec<u8> = (0..w * h).map(|i| if ((i % w) / 8 + (i / w) / 8) % 3 == 0 { 12 } else { 10 }).collect();
    assert!(blotch_score(&blotchy, w, h).unwrap() > BLOTCHY);
    // a real edge (a panel 20 levels brighter) isn't blotching
    let panel: Vec<u8> = (0..w * h).map(|i| if i % w >= w / 2 { 31 } else { 11 }).collect();
    assert_eq!(blotch_score(&panel, w, h), Some(0.0));
    // nor is a faint one the picture draws (a dark panel two levels off its ground), nor gradient bands
    let faint: Vec<u8> = (0..w * h).map(|i| if i % w >= w / 2 && i / w >= h / 3 { 13 } else { 11 }).collect();
    assert_eq!(blotch_score(&faint, w, h), Some(0.0));
    let bands: Vec<u8> = (0..w * h).map(|i| 8 + ((i / w) / 24) as u8 % 6).collect();
    assert_eq!(blotch_score(&bands, w, h), Some(0.0));
    // nor a glow's 8-bit rings, which curve (no straight run) but keep stepping the same way
    let rings: Vec<u8> = (0..w * h).map(|i| {
        let (x, y) = ((i % w) as f64 - w as f64 / 2.0, (i / w) as f64 - h as f64 / 2.0);
        (20.0 - (x * x + y * y).sqrt() / 24.0).max(6.0) as u8
    }).collect();
    assert!(blotch_score(&rings, w, h).unwrap() < BLOTCHY / 2.0);
    assert_eq!(x264_crf(b"..\x05x264 - core 165 r3222 - H.264/MPEG-4 AVC codec - options: cabac=1 rc=crf mbtree=1 crf=12.0 qcomp=0.60"), Some(12.0));
    assert_eq!(x264_crf(b"Lavf61 VideoToolbox"), None);
}

#[test]
fn taste_checks_hold_text_to_the_persons_line() {
    let taste: raea_core_brief::Taste = serde_json::from_value(json!({"title": "Shane's taste", "sections": [{"heading": "Always", "rules": [
        {"text": "Captions sit clear of the bottom.", "check": {"form": "text-above", "value": 62.0}},
        {"text": "No glow"},
        {"text": "Text holds two seconds", "check": {"form": "text-hold", "value": 2.0}},
    ]}]})).unwrap();
    let rules = taste_rules(&[taste]);
    assert_eq!(rules, vec![
        Taste { form: "text-above", value: 62.0, line: "Captions sit clear of the bottom.".into() },
        Taste { form: "text-hold", value: 2.0, line: "Text holds two seconds".into() },
    ]);
    let low = Run { text: "Render anything.".into(), from: 1.0, to: 4.0, box_: [800.0, 800.0, 300.0, 60.0], boxes: vec![[800.0, 800.0, 300.0, 60.0]], color: String::new(), size: 48.0 };
    let high = Run { text: "raea".into(), from: 5.0, to: 6.0, box_: [800.0, 300.0, 300.0, 60.0], boxes: vec![[800.0, 300.0, 300.0, 60.0]], color: String::new(), size: 48.0 };
    let (mut issues, mut passed) = (vec![], vec![]);
    taste_text(&rules, &[low, high], 1920.0, 1080.0, &mut issues, &mut passed);
    let says: Vec<_> = issues.iter().map(|i| (i.rule, i.p, i.at)).collect();
    // the low caption breaks the placement line (860 of 1080 is 80%); the short one breaks the hold
    assert_eq!(says, vec![("text-above", 1, Some(1.0)), ("text-hold", 1, Some(5.0))], "a taste line is the named check of its form (DR4)");
}

/// `RAEA_CHECK_ARGS="film.mp4 --manifest film.render.json --taste DIR" cargo test -p raea-runtime-react --lib
/// check::tests::on_a_real_export -- --ignored --nocapture`: the CLI on real files, without building raea
#[test]
#[ignore]
fn on_a_real_export() {
    let args: Vec<String> = std::env::var("RAEA_CHECK_ARGS").expect("RAEA_CHECK_ARGS").split_whitespace().map(String::from).collect();
    let code = cli_with(&args, Vec::new());
    println!("exit {code}");
}

/// a 440 Hz tone at `db` dBFS for each (from, to) in seconds, at the lint's rate
fn tone(len: f64, parts: &[(f64, f64, f64)]) -> Vec<f32> {
    (0..(len * SR as f64) as usize)
        .map(|i| {
            let t = i as f64 / SR as f64;
            parts.iter().filter(|(a, b, _)| t >= *a && t < *b).map(|(_, _, db)| 10f64.powf(db / 20.0) * (2.0 * std::f64::consts::PI * 440.0 * t).sin()).sum::<f64>() as f32
        })
        .collect()
}

#[test]
fn a_steady_sound_that_stops_dead_is_found_and_a_faded_one_isnt() {
    // a bed at -20 dBFS that stops at 2 s, and one that fades out over 1 s from 2 s
    let (rms, bands) = levels(&tone(4.0, &[(0.0, 2.0, -20.0)]));
    let stops = dead_stops(&rms, &bands);
    assert_eq!(stops.len(), 1, "{stops:?}");
    assert!((stops[0].0 - 2.0).abs() < 0.02, "{stops:?}");
    let faded: Vec<f32> = tone(4.0, &[(0.0, 3.0, -20.0)]).into_iter().enumerate().map(|(i, v)| v * (1.0 - ((i as f32 / SR as f32) - 2.0).clamp(0.0, 1.0))).collect();
    let (rms, bands) = levels(&faded);
    assert!(dead_stops(&rms, &bands).is_empty());
    // a source cut partway through a 10 ms step is as dead
    let cut: Vec<f32> = tone(4.0, &[(0.0, 2.0 + 117.0 / SR as f64, -20.0)]);
    let (rms, bands) = levels(&cut);
    assert_eq!(dead_stops(&rms, &bands).len(), 1);
    // a 100 ms fadeOut is a fade, though its last step falls into silence
    let short: Vec<f32> = tone(4.0, &[(0.0, 2.0, -20.0)]).into_iter().enumerate().map(|(i, v)| v * ((2.0 - i as f32 / SR as f32) / 0.1).clamp(0.0, 1.0)).collect();
    let (rms, bands) = levels(&short);
    assert!(dead_stops(&rms, &bands).is_empty(), "{:?}", dead_stops(&rms, &bands));
}

#[test]
fn bands_read_in_dbfs() {
    // a tone in the middle of its band reads there what it reads in the mix: -20 dBFS peak is -23 dBFS RMS
    let sine = |hz: f64| (0..SR).map(|i| (0.1 * (2.0 * std::f64::consts::PI * hz * i as f64 / SR as f64).sin()) as f32).collect::<Vec<_>>();
    for (hz, band) in [(200.0, 0), (2000.0, 1), (8000.0, 2)] {
        let (rms, bands) = levels(&sine(hz));
        assert!((rms[50] + 23.0).abs() < 0.1, "{}", rms[50]);
        assert!((bands[band][50] - rms[50]).abs() < 0.2, "{hz} Hz: band {} vs the mix {}", bands[band][50], rms[50]);
    }
}

#[test]
fn a_voice_that_ends_a_sentence_isnt_a_dead_stop() {
    // its-a-race at 12-14 s: "AI became a thing." ends at 13.36 s and the speaker pauses. Its lows held within 2.6 dB over
    // the second before and fell 12 dB in one step, so they read as a bed cut dead ("a steady sound at -0 dB", before the
    // bands read in dBFS), but the mix follows the source there and the word's tail comes back up after the first drop
    let tracks: Vec<Vec<f64>> = include_str!("testdata/its-a-race-12s.levels")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split_whitespace().map(|v| v.parse().unwrap()).collect())
        .collect();
    let bands = [tracks[1].clone(), tracks[2].clone(), tracks[3].clone()];
    let stops = dead_stops(&tracks[0], &bands);
    assert!(stops.is_empty(), "{stops:?}");
    // the same voice with a bed under it that's cut at 13.6 s: that one is still found
    let bed = 10f64.powf(-30.0 / 10.0);
    let highs: Vec<f64> = tracks[3].iter().enumerate().map(|(i, v)| 10.0 * (10f64.powf(v / 10.0) + if i < 160 { bed } else { 0.0 }).log10()).collect();
    let found = dead_stops(&tracks[0], &[tracks[1].clone(), tracks[2].clone(), highs]);
    assert!(found.iter().any(|(at, says)| (at - 1.6).abs() < 0.02 && says.starts_with("the highs")), "{found:?}");
}

#[test]
fn a_freeze_still_running_when_the_file_ends_is_found() {
    // what FFmpeg 9.0.2's freezedetect logged on a film held from 3.4 s to its end, after a hold that ended at 3 s: the
    // last freeze has a start and nothing after it, since it logs the duration when the picture moves again
    let log = "[Parsed_freezedetect_0 @ 0000] lavfi.freezedetect.freeze_start: 0
               [Parsed_freezedetect_0 @ 0000] lavfi.freezedetect.freeze_duration: 3
               [Parsed_freezedetect_0 @ 0000] lavfi.freezedetect.freeze_end: 3
               [Parsed_freezedetect_0 @ 0000] lavfi.freezedetect.freeze_start: 3.4
";
    assert_eq!(frozen_stretches(log, 10.0), vec![(0.0, 3.0), (3.4, 6.6)]);
    // the whole file frozen: one start, at zero
    assert_eq!(frozen_stretches("lavfi.freezedetect.freeze_start: 0
", 8.0), vec![(0.0, 8.0)]);
    assert!(frozen_stretches("frame=  300 fps=0.0
", 10.0).is_empty());
}

#[test]
fn a_bed_close_under_the_voice_and_a_loud_stretch_without_it_are_found() {
    // 8 s of voice (-20 dBFS) in phrases with gaps, over a bed at -26 (6 dB under) and then at -40 (20 dB under)
    let words: Vec<Word> = (0..16).map(|k| Word { text: "w".into(), start: k as f64 * 0.5, end: k as f64 * 0.5 + 0.3 }).collect();
    let phrases: Vec<(f64, f64, f64)> = (0..16).map(|k| (k as f64 * 0.5, k as f64 * 0.5 + 0.3, -20.0)).collect();
    let close = [&phrases[..], &[(0.0, 8.0, -26.0)]].concat();
    let (rms, _) = levels(&tone(8.0, &close));
    assert_eq!(bed_up(&rms, &speaking(&words, rms.len())).len(), 1);
    let low = [&phrases[..], &[(0.0, 8.0, -40.0)]].concat();
    let (rms, _) = levels(&tone(8.0, &low));
    assert!(bed_up(&rms, &speaking(&words, rms.len())).is_empty());
    // momentary loudness: the voice at -20 for 3 s, then 2 s with no voice at -12 (1.2 s of it clear of the last word: it ends at 2.9 s, held 0.5 s, and a 400 ms window)
    let (mt, mv): (Vec<f64>, Vec<f64>) = (0..50).map(|k| (k as f64 / 10.0, if k < 30 { -20.0 } else { -12.0 })).unzip();
    let w: Vec<Word> = (0..6).map(|k| Word { text: "w".into(), start: k as f64 * 0.5, end: k as f64 * 0.5 + 0.4 }).collect();
    let (voice, over) = over_voice(&mt, &mv, &speaking(&w, 500)).unwrap();
    assert_eq!(voice, -20.0);
    assert_eq!(over.len(), 1, "{over:?}");
    assert!(over[0].1.starts_with("1.2 s with no voice at up to -12.0 LUFS, 8.0 LU over"), "{}", over[0].1);
}

#[test]
fn the_floor_and_placement_state_the_zones_check_measures() {
    // zones() is where the numbers live; floor.md and placement.md say them in words, held to it here
    let words = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
    let floor = words(include_str!("../../../sdk/guides/craft/floor.md"));
    let placement = words(include_str!("../../../sdk/guides/craft/motion/placement.md"));
    let n = |v: f64| format!("{v:.0}");
    let [t, r, b, l] = zones(1080.0, 1920.0, Some("9:16"), None);
    let said = [
        format!("keep the top {} px, the bottom {}, the right {} and the left {} clear", n(t), n(b), n(r), n(l)),
        ["tiktok", "reels", "shorts"].map(|p| {
            let [t, r, b, _] = zones(1080.0, 1920.0, Some("9:16"), Some(p));
            let name = match p { "tiktok" => "TikTok", "reels" => "Reels", _ => "Shorts" };
            format!("{name}: top {}, bottom {}, right {}", n(t), n(b), n(r))
        }).join("; "),
        { let [t, r, b, l] = zones(1080.0, 1350.0, Some("4:5"), None); assert_eq!(r, l); format!("A 4:5 feed frame needs top {}, bottom {}, sides {}", n(t), n(b), n(r)) },
        { let [t, r, b, l] = zones(1080.0, 1080.0, Some("1:1"), None); assert_eq!(r, l); format!("a 1:1 frame top {}, bottom {}, sides {}", n(t), n(b), n(r)) },
        { let [t, r, b, l] = zones(1920.0, 1080.0, Some("16:9"), None); assert!(t == b && r == l); format!("16:9 top and bottom {}, sides {}", n(t), n(r)) },
    ];
    for s in &said {
        assert!(floor.contains(s.as_str()), "floor.md doesn't say {s:?}");
    }
    let s = format!("that is top {}, bottom {}, right {} and left {} px at 1080 wide", n(t), n(b), n(r), n(l));
    assert!(placement.contains(&s), "placement.md doesn't say {s:?}");
}

#[test]
fn text_over_the_face_is_a_fault_while_that_face_is_up() {
    // a caption over the face for 1 s, a card clear of it; the face moves away at frame 20 (a new clip)
    let m = json!({"fps": 10.0, "frames": (0..30).map(|f| json!({"frame": f, "text": [
        {"text": "on the face", "box": [400, 500, 300, 100], "size": 80},
        {"text": "clear", "box": [100, 1500, 300, 100], "size": 80}
    ]})).collect::<Vec<_>>()});
    let runs = text_runs(&m);
    let faces = faces_of(&json!({"0": {"safe": {"top": 220, "bottom": 1500}, "face": {"x": 350, "y": 400, "w": 400, "h": 400}}, "20": {"safe": {"top": 220, "bottom": 1500}}}), 10.0);
    assert_eq!(faces, vec![(0.0, 2.0, [350.0, 400.0, 400.0, 400.0])]);
    let (mut issues, mut passed) = (Vec::new(), Vec::new());
    face_cover(&runs, &faces, &mut issues, &mut passed);
    assert!(passed.is_empty());
    assert_eq!(issues.len(), 1, "{:?}", issues.iter().map(|i| &i.says).collect::<Vec<_>>());
    assert!(issues[0].says.contains("on the face\u{201d} covers 19% of the face"), "{}", issues[0].says);
    // no face anywhere: nothing to cover, and nothing claimed
    let (mut issues, mut passed2) = (Vec::new(), Vec::new());
    face_cover(&runs, &faces_of(&json!({"0": {"safe": {"top": 220, "bottom": 1500}}}), 10.0), &mut issues, &mut passed2);
    assert!(issues.is_empty() && passed2.is_empty());
}

#[test]
fn a_caption_the_voice_reads_needs_no_reading_time() {
    // two karaoke groups up for 0.5 s each, and a card line up as briefly that the voice doesn't say
    let m = json!({"fps": 10.0, "frames": (0..16).map(|f| json!({"frame": f, "text": [
        {"text": if f < 5 { "so if it" } else if f < 10 { "feels like you're" } else { "share of spend" }, "box": [300, 900, 400, 80], "size": 78}
    ]})).collect::<Vec<_>>()});
    let words: Vec<Word> = [("So", 0.0), ("if", 0.15), ("it", 0.3), ("feels", 0.5), ("like", 0.7), ("you're", 0.85)].map(|(t, s)| Word { text: t.into(), start: s, end: s }).into();
    let (mut issues, mut passed) = (vec![], vec![]);
    text_rules(&text_runs(&m), Some(&words), 1080.0, 1920.0, Some("9:16"), None, &mut issues, &mut passed);
    let brief: Vec<_> = issues.iter().filter(|i| i.rule == "reading").map(|i| i.says.split('\u{201d}').next().unwrap().trim_start_matches('\u{201c}').to_string()).collect();
    assert_eq!(brief, vec!["share of spend"]);
    // without the transcript all three are held to the reading time
    let mut issues = vec![];
    text_rules(&text_runs(&m), None, 1080.0, 1920.0, Some("9:16"), None, &mut issues, &mut passed);
    assert_eq!(issues.iter().filter(|i| i.rule == "reading").count(), 3);
}

#[test]
fn typing_is_one_run_and_cues_find_their_word() {
    let m = json!({"fps": 10.0, "frames": [
        {"frame": 0, "text": [{"text": "Sh", "box": [0, 0, 10, 10]}]},
        {"frame": 1, "text": [{"text": "Shape", "box": [0, 0, 10, 10]}]},
        {"frame": 2, "text": [{"text": "Shape it", "box": [0, 0, 10, 10], "size": 72}]},
        {"frame": 3, "text": []},
    ]});
    let runs = text_runs(&m);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "Shape it");
    let words = vec![Word { text: "shape".into(), start: 0.9, end: 0.9 }];
    let (mut issues, mut passed) = (vec![], vec![]);
    cues(&runs, &words, &mut issues, &mut passed);
    assert_eq!(issues.len(), 1, "on at 0 s, said at 0.9 s: 0.9 s early");
}

#[test]
fn a_caption_is_timed_from_the_first_word_it_shows() {
    let run = |text: &str, from: f64| Run { text: text.into(), from, to: from + 1.0, box_: [0.0; 4], boxes: vec![], color: String::new(), size: 48.0 };
    let words: Vec<Word> = [("half", 12.2), ("of", 12.66), ("the", 12.8), ("spend", 12.96)].iter().map(|(t, s)| Word { text: (*t).into(), start: *s, end: *s }).collect();
    let (mut issues, mut passed) = (vec![], vec![]);
    // on with "of": on time, though "spend" (its first word of weight) is said 0.3 s later
    cues(&[run("of the spend.", 12.66)], &words, &mut issues, &mut passed);
    assert!(issues.is_empty(), "{:?}", issues.iter().map(|i| &i.says).collect::<Vec<_>>());
    // a word the voice doesn't say just before isn't walked back over: still early against "spend"
    cues(&[run("all the spend", 12.5)], &words, &mut issues, &mut passed);
    assert_eq!(issues.len(), 1);
    assert!(issues[0].says.contains("before the voice says \u{201c}the\u{201d}"), "{}", issues[0].says);
}

#[test]
fn a_sign_the_camera_carries_off_isnt_clipped_but_a_title_held_off_is() {
    let m = json!({"fps": 10.0, "frames": [
        {"frame": 0, "text": [{"text": "NOODLES", "box": [900, 556, 304, 112]}, {"text": "THE NIGHT MARKET", "box": [700, 40, 500, 90]}]},
        {"frame": 1, "text": [{"text": "NOODLES", "box": [-100, 556, 304, 112]}, {"text": "THE NIGHT MARKET", "box": [700, 40, 500, 90]}]},
        {"frame": 2, "text": [{"text": "NOODLES", "box": [-924, 556, 304, 112]}, {"text": "WIDE", "box": [-50, 500, 2000, 90]}]},
    ]});
    let (mut issues, mut passed) = (vec![], vec![]);
    text_rules(&text_runs(&m), None, 1920.0, 1080.0, None, None, &mut issues, &mut passed);
    let rules: Vec<_> = issues.iter().map(|i| (i.rule, i.says.split('”').next().unwrap().trim_start_matches('“').to_string())).collect();
    assert_eq!(rules, vec![("safe-area", "THE NIGHT MARKET".to_string()), ("clipped", "WIDE".to_string())]);
}

#[test]
fn a_frame_gets_its_exact_time_from_the_log() {
    // lines FFmpeg 9.0.2 logged with -loglevel level+info
    let base = showinfo_base("[Parsed_showinfo_0 @ 000001] [info] config in time_base: 1/15360, frame_rate: 30/1");
    assert_eq!(base, Some((1, 15360)));
    let line = "[Parsed_showinfo_0 @ 000001] [info] n:  12 pts: 6144 pts_time:0.4 duration: 512 fmt:yuv420p";
    assert_eq!(showinfo_frame(line, base), Some((12, 0.4)));
    // rounded to six significant digits at long times: the ticks stay exact
    let late = "[Parsed_showinfo_0 @ 000001] [info] n:40000 pts:20480512 pts_time:1333.37 duration: 512";
    assert_eq!(showinfo_frame(late, base), Some((40000, 20480512.0 / 15360.0)));
    assert_eq!(showinfo_frame(line, None), None);
    assert_eq!(showinfo_frame("[info] frame=  300 fps=0.0", base), None);
}

#[test]
fn only_a_lines_own_severity_counts_as_an_error() {
    assert_eq!(log_level("[h264 @ 000001] [error] error while decoding MB 3 5"), Some("error"));
    assert_eq!(log_level("[error] Conversion failed!"), Some("error"));
    assert_eq!(log_level("[Parsed_showinfo_0 @ 000001] [info] n:   0 pts:      0"), Some("info"));
    // a file's own metadata, which FFmpeg prints back at info
    assert_eq!(log_level("[info]     title           : [error] harmless title"), Some("info"));
    assert_eq!(log_level("frame=  300 fps=0.0"), None);
}


/// A centred line in a left: 0; right: 0 box isn't in the left zone: its letters are tested where the render recorded
/// them (#89); a left-aligned one is, the finding naming the edge, and a manifest without ink is tested by the box.
#[test]
fn safe_area_tests_the_letters_not_the_box() {
    let safe_area = |ink: Value| {
        let m = json!({"fps": 1.0, "frames": [{"frame": 0, "text": [{"text": "centred", "box": [0, 160, 640, 40], "ink": ink}]}]});
        let (mut issues, mut passed) = (vec![], vec![]);
        text_rules(&text_runs(&m), None, 640.0, 360.0, None, None, &mut issues, &mut passed);
        issues.into_iter().filter(|i| i.rule == "safe-area").map(|i| i.says).collect::<String>()
    };
    assert_eq!(safe_area(json!([250, 160, 140, 40])), "");
    let left = safe_area(json!([0, 160, 140, 40]));
    assert!(left.contains("at the left") && left.contains("its letters 0,160 140x40"), "{left}");
    assert!(safe_area(Value::Null).contains("its box 0,160 640x40"));
}

#[test]
fn a_card_sharing_one_word_with_another_line_isnt_held_to_it() {
    let run = |text: &str, from: f64| Run { text: text.into(), from, to: from + 2.0, box_: [0.0; 4], boxes: vec![], color: String::new(), size: 48.0 };
    let said = |ws: &[(&str, f64)]| ws.iter().map(|(t, s)| Word { text: (*t).into(), start: *s, end: *s }).collect::<Vec<Word>>();
    let words = said(&[("at", 37.9), ("wirestock", 38.2), ("i", 38.9), ("worked", 39.0), ("on", 39.2), ("a", 39.3), ("sora", 39.5), ("campaign", 40.2)]);
    let (mut issues, mut passed) = (vec![], vec![]);
    // one word of three shared with the line, said 1.35 s before: not a caption of it (the film's false P1)
    cues(&[run("Wirestock × OpenAI Sora", 39.55)], &words, &mut issues, &mut passed);
    // one word of four, "campaign": the same
    cues(&[run("Classroom of Tomorrow campaign", 39.55)], &words, &mut issues, &mut passed);
    assert!(issues.is_empty(), "{:?}", issues.iter().map(|i| &i.says).collect::<Vec<_>>());
    // a one-word card is held only to the word said right by it: "wirestock" 1.35 s earlier is another moment
    cues(&[run("Wirestock", 39.55)], &words, &mut issues, &mut passed);
    assert!(issues.is_empty(), "{:?}", issues.iter().map(|i| &i.says).collect::<Vec<_>>());
    // a real caption late by 0.4 s is still found
    cues(&[run("a Sora campaign", 39.7)], &words, &mut issues, &mut passed);
    assert_eq!(issues.len(), 1, "{:?}", issues.iter().map(|i| &i.says).collect::<Vec<_>>());
}

#[test]
fn clipping_is_found_and_a_loud_clean_tone_isnt() {
    // a clean tone near full scale, then the same tone clamped (a limiter's flat tops), then pushed past full scale
    let loud = tone(3.0, &[(0.0, 3.0, -1.0)]);
    let one = |x: &[f32]| clipped(&[x.to_vec()]);
    assert!(one(&loud).is_empty(), "{:?}", one(&loud));
    let clamped: Vec<f32> = loud.iter().map(|v| (v * 10.0).clamp(-0.3, 0.3)).collect();
    assert_eq!(one(&clamped).len(), 1);
    let over: Vec<f32> = loud.iter().map(|v| (v * 2.0).clamp(-1.0, 1.0)).collect();
    assert_eq!(one(&over).len(), 1);
    // one transient hitting full scale once isn't clipping
    let mut once = tone(3.0, &[(0.0, 3.0, -20.0)]);
    once[SR] = 1.0;
    assert!(one(&once).is_empty());
    // clipping is a channel's: one side clipped is found; a centred voice at -1.6 dBFS a side is clean, though the mono
    // fold (L + R) * 0.707 takes it past full scale (11-19: the walkaround's master read as clipping)
    assert_eq!(clipped(&[loud.clone(), over.clone()]).len(), 1);
    let centred: Vec<f32> = loud.iter().map(|v| v * 0.92).collect();
    assert!(clipped(&[centred.clone(), centred.clone()]).is_empty());
    let folded: Vec<f32> = centred.iter().map(|v| (v + v) * 0.7071).collect();
    assert_eq!(one(&folded).len(), 1);
}

#[test]
fn a_dropout_is_found_and_a_faded_pause_isnt() {
    // a bed at -20 dBFS with 0.6 s cut out of it, as the bootstrap's dropout does
    let mut cut = tone(4.0, &[(0.0, 4.0, -20.0)]);
    cut[(1.7 * SR as f64) as usize..(2.3 * SR as f64) as usize].iter_mut().for_each(|v| *v = 0.0);
    let found = dropouts(&cut);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!((found[0].0 - 1.7).abs() < 0.02, "{found:?}");
    // a pause that fades out over 300 ms and back in isn't one, nor a sound that stops and stays stopped
    let gain = |t: f32| ((1.7 - t) / 0.3).max((t - 2.3) / 0.3).clamp(0.0, 1.0);
    let paused: Vec<f32> = tone(4.0, &[(0.0, 4.0, -20.0)]).into_iter().enumerate().map(|(i, v)| v * gain(i as f32 / SR as f32)).collect();
    assert!(dropouts(&paused).is_empty(), "{:?}", dropouts(&paused));
    assert!(dropouts(&tone(4.0, &[(0.0, 2.0, -20.0)])).is_empty());
}

#[test]
fn a_flash_frame_is_found_and_a_cut_or_a_fade_isnt() {
    let mut shot = vec![0.5f32; 60];
    shot[30] = 0.0;
    assert_eq!(flash_frames(&shot, 30.0).len(), 1);
    shot[31] = 0.0;
    assert_eq!(flash_frames(&shot, 30.0).len(), 1, "two frames are one flash");
    let cut: Vec<f32> = (0..60).map(|i| if i < 30 { 0.6 } else { 0.1 }).collect();
    assert!(flash_frames(&cut, 30.0).is_empty());
    let fade: Vec<f32> = (0..60).map(|i| 0.6 * (1.0 - i as f32 / 59.0)).collect();
    assert!(flash_frames(&fade, 30.0).is_empty());
    // a white frame, too
    let mut white = vec![0.3f32; 20];
    white[10] = 1.0;
    assert_eq!(flash_frames(&white, 30.0)[0].0, 10.0 / 30.0);
}

#[test]
fn things_found_are_findings_and_an_unknown_rule_is_not() {
    let p = std::env::temp_dir().join(format!("raea-check-things-{}.json", std::process::id()));
    let found = r#"{"issues": [{"rule": "floating", "p": 2, "at": 1.5, "to": 3.0, "says": "cup floats"}, {"rule": "made-up", "p": 0, "at": 0.0, "says": "x"}]}"#;
    std::fs::write(&p, found).unwrap();
    let (mut issues, mut passed) = (Vec::new(), Vec::new());
    things(p.to_str().unwrap(), &mut issues, &mut passed).unwrap();
    assert_eq!(issues.iter().map(|i| (i.rule, i.p, i.at)).collect::<Vec<_>>(), [("floating", 2, Some(1.5))]);
    std::fs::write(&p, r#"{"issues": []}"#).unwrap();
    things(p.to_str().unwrap(), &mut Vec::new(), &mut passed).unwrap();
    let _ = std::fs::remove_file(&p);
    assert_eq!(passed.len(), 1, "nothing found is a pass");
}

/// #236: a text that moves is read where it is on its middle frame, not where it ends.
#[test]
fn a_moving_text_is_read_where_it_is() {
    let ink = |f: usize| [56.7 + 4.0 * f as f64, 1000.0, 26.7, 40.0];
    let run = Run { text: "E".into(), from: 0.0, to: 8.0, box_: [996.0, 1000.0, 60.0, 40.0], boxes: (0..240).map(ink).collect(), color: String::new(), size: 40.0 };
    let (t, b) = run.middle(30.0);
    assert_eq!(t, 4.0);
    assert!((b[0] - 520.0).abs() < 1e-9 && b[1..] == [1000.0, 60.0, 40.0], "{b:?}");
    let still = Run { boxes: vec![ink(0); 240], box_: [40.0, 1000.0, 60.0, 40.0], ..run };
    assert_eq!(still.middle(30.0), (4.0, [40.0, 1000.0, 60.0, 40.0]));
}
