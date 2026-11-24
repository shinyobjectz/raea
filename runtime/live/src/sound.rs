//! Live play's sound: the master, the mixer and each frame's cues.

use super::*;

impl Live {
    /// A timeline's sound mastered as its render's is, built on a thread from now (a few seconds: React stepped
    /// through every frame, no pages painted); until it's ready the elements play their own sounds, unmastered.
    pub fn master_sound(&mut self) {
        if self.master.is_some() || self.master_wav.is_some() {
            return;
        }
        // kept in the temp folder by what makes it (the bundle, and every file in the project: its path, size and
        // time), so opening it again plays it at once (each open made a new 3.8 MB file, kept forever, ~10 s after the
        // open for an edit on an external drive)
        // keyed by the core's mastering version and the build: a track an earlier core mastered isn't this one's
        let engine = format!("fw{}-{:x}-", raea_sys_mix::MASTER_VERSION, build_id());
        let wav = std::env::temp_dir().join(format!("raea-master-{engine}{:016x}.wav", sound_key(&self.js, &self.dir)));
        if wav.exists() {
            self.master_wav = Some(wav);
            return;
        }
        // the 40 newest tracks kept
        if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
            let mut old: Vec<(std::time::SystemTime, PathBuf)> = rd.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("raea-master-") && e.file_name().to_string_lossy().ends_with(".wav")).filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
            old.sort();
            while old.len() > 40 {
                let _ = std::fs::remove_file(old.remove(0).1);
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (js, dir) = (self.js.clone(), self.dir.clone());
        let spawned = std::thread::Builder::new().name("raea live master".into()).spawn(move || {
            let part = wav.with_extension("part.wav");
            let r = raea_runtime_export::sound_only(&js, &dir).and_then(|a| match a {
                Some(a) => raea_sys_mix::write_wav(&a, &part).and_then(|_| std::fs::rename(&part, &wav).map_err(|e| e.to_string())).map(|_| Some(wav)),
                None => Ok(None),
            });
            let _ = tx.send(r);
        });
        if spawned.is_ok() {
            self.master = Some(rx);
        }
    }

    /// A game's live voices mastered as its render masters them: the gain that takes its sound (its scripted run, every
    /// frame) to the <Mix>'s loudness, and the look-ahead limiter at its ceiling, worked out on a thread from now. Until
    /// it's in, and without a <Mix>, the voices play at their own gain under the -1 dBFS ceiling.
    pub fn game_master(&mut self) {
        if self.game_master.is_some() || self.game_gain.is_some() {
            return;
        }
        // known already: kept in the temp folder by the bundle's content (the same game, the same master)
        let key = std::fs::read(&self.js).map(|b| twox_hash::XxHash3_64::oneshot(&b)).unwrap_or(0);
        // v2: the gain before the master's limiter (v1 held the loudness change after it, counting the limiter twice)
        // with the core's mastering version: a master an earlier core worked out isn't this one's
        let engine = format!("fw{}-{:x}-", raea_sys_mix::MASTER_VERSION, build_id());
        let cache = std::env::temp_dir().join(format!("raea-master-gain-v2-{engine}{key:016x}.json"));
        let parse = |v: &Value| v.get("gain_db").and_then(Value::as_f64).zip(v.get("ceiling_dbtp").and_then(Value::as_f64));
        if let Some(v) = std::fs::read_to_string(&cache).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
            self.game_gain = parse(&v);
            if v.get("none").is_none() || self.game_gain.is_some() {
                return;
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (js, dir) = (self.js.clone(), self.dir.clone());
        if std::thread::Builder::new().name("raea live game master".into()).spawn(move || {
            // in a child process (raea master-gain): stepping the game through every frame in a second React,
            // and mixing its whole sound, took ~260 MB in the game's own process. In process only without the binary.
            let bin = std::env::var_os("RAEA_BIN").map(PathBuf::from).or_else(|| std::env::current_exe().ok().map(|e| e.with_file_name("raea"))).filter(|b| b.exists());
            let got: R<Option<(f64, f64)>> = match bin {
                Some(bin) => std::process::Command::new(bin).arg("master-gain").arg(&js).arg(&dir).output().map_err(|e| e.to_string()).and_then(|o| {
                    let text = String::from_utf8_lossy(&o.stdout);
                    let v: Value = text.lines().rev().find_map(|l| serde_json::from_str(l).ok()).ok_or_else(|| format!("master-gain said: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("")))?;
                    let _ = std::fs::write(&cache, v.to_string());
                    Ok(parse(&v))
                }),
                None => raea_runtime_export::master_gain(&js, &dir),
            };
            let _ = tx.send(got);
        }).is_ok() {
            self.game_master = Some(rx);
        }
    }

    /// The mixer opened now, before the play's clock starts. Opened at the first sound instead, that frame was stepped
    /// late by the open (the one mixer core waits ~0.1 s for its device's clock to run), and its sounds skipped in by
    /// that much, so the play wasn't the bounce.
    pub fn open_mixer(&mut self) {
        if self.mixer.is_some() || self.mixer_failed {
            return;
        }
        match raea_runtime_sound::Mixer::open() {
            Ok(mut m) => {
                if let Some((gain, ceiling)) = self.game_gain {
                    m.master(gain, ceiling);
                }
                self.mixer = Some(m);
            }
            Err(e) => {
                eprintln!("raea live: no sound: {e}");
                self.mixer_failed = true;
            }
        }
    }

    /// The master waited for (at most `secs`) before the play's clock starts, so its first frames are mastered as the
    /// render's are (worked out on a thread, it came in seconds into the play: those seconds weren't the bounce).
    pub fn wait_master(&mut self, secs: f64) {
        if let Some(rx) = self.game_master.take() {
            match rx.recv_timeout(std::time::Duration::from_secs_f64(secs)) {
                Ok(Ok(g)) => {
                    self.game_gain = g;
                    if let (Some(m), Some((gain, ceiling))) = (self.mixer.as_mut(), g) {
                        m.master(gain, ceiling);
                    }
                }
                Ok(Err(e)) => eprintln!("raea live: the master failed, the voices play at their own gain: {e}"),
                // not in yet: play starts without it and the frame it lands swaps it in (step's try_recv)
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => self.game_master = Some(rx),
                Err(_) => {}
            }
        }
    }

    /// The mixer's clock past the device's startup, before the play's clock starts (raea_sys_mix's settle_clock: free
    /// when the device opened beside the edit's open).
    pub fn settle_mixer(&mut self) {
        if let Some(m) = self.mixer.as_mut() {
            m.settle();
        }
    }

    /// The game's master, once worked out: (gain dB, ceiling dBTP).
    pub fn game_gain(&self) -> Option<(f64, f64)> {
        self.game_gain
    }

    /// The frame's sounds to the mixer (opened the first time there are any).
    /// The mixer opened and the current frame's sound sources loading, nothing started (open_baked's frame 0).
    pub(crate) fn warm_sound(&mut self) {
        let world = self.app.world_mut();
        let mut q = world.query::<(&raea_runtime_react::ReactKind, Option<&raea_runtime_react::ReactProps>, Option<&raea_runtime_react::ReactNode>)>();
        let items: Vec<(&str, Option<&serde_json::Map<String, Value>>, Option<u64>)> = q.iter(world).filter(|(k, _, _)| k.0 == "audio" || k.0 == "video").map(|(k, p, n)| (k.0.as_str(), p.map(|p| &p.0), n.map(|n| n.0))).collect();
        let sounds = raea_runtime_sound::sounds_of(&items, &self.dir, raea_runtime_react::scene::media_path, &mut self.made);
        if sounds.is_empty() {
            return;
        }
        if self.mixer.is_none() && !self.mixer_failed {
            match raea_runtime_sound::Mixer::open() {
                Ok(mut m) => {
                    if let Some((gain, ceiling)) = self.game_gain {
                        m.master(gain, ceiling);
                    }
                    self.mixer = Some(m)
                }
                Err(e) => {
                    eprintln!("raea live: no sound: {e}");
                    self.mixer_failed = true;
                }
            }
        }
        if let Some(m) = self.mixer.as_mut() {
            for s in &sounds {
                m.preload(&s.file);
            }
        }
    }

    pub(crate) fn sound(&mut self, f: i64) {
        if let Some(rx) = &self.master {
            match rx.try_recv() {
                Ok(Ok(wav)) => {
                    if raea_io_trace::stat("RAEA_LIVE_TRACE") {
                        eprintln!("raea live: the mastered sound is in at frame {f}");
                    }
                    self.master_wav = wav;
                    self.master = None;
                }
                Ok(Err(e)) => {
                    eprintln!("raea live: the mastered sound failed, the elements play their own: {e}");
                    self.master = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.master = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        // the frame's sounds, in the list kept between frames (ZeroHeap)
        let mut sounds = std::mem::take(&mut self.sound_buf);
        sounds.clear();
        match &self.master_wav {
            // the render's own track, placed at the frame's time
            Some(wav) => sounds.push(raea_runtime_sound::Sound { key: "master".into(), file: wav.as_path().into(), at: f as f64 / self.fps, volume: 1.0, rate: 1.0, fade_in: -1.0, fade_out: 0.0, bus: None }),
            None => {
                let (dir, made) = (&self.dir, &mut self.made);
                raea_runtime_react::scene::with_kinds(self.app.world_mut(), |world, q| {
                    let items = q.0.iter(world).filter(|(k, _, _)| k.0 == "audio" || k.0 == "video").map(|(k, p, n)| (k.0.as_str(), p.map(|p| &p.0), n.map(|n| n.0)));
                    raea_runtime_sound::sounds_into(items, dir, raea_runtime_react::scene::media_path, made, &mut sounds)
                })
            }
        }
        // a baked timeline past its last frame holds that frame's picture, not its sound: the held <Video> said the same
        // source time every frame and the mixer put its voice back each one (the 80 ms "drift" past a 4K edit's end).
        // Played unbaked, the composition past its end shows no <Video> either, so both fall silent there.
        if self.baked.as_ref().is_some_and(|b| f >= b.frames) {
            sounds.clear();
        }
        // the edit's Tone (raea/tone)
        let tone = if self.baked.as_ref().is_some_and(|b| f >= b.frames) { raea_sys_mix::ToneCue::default() } else { raea_runtime_sound::tone_at(self.app.world_mut(), f, self.fps, &self.dir, raea_runtime_react::scene::media_path, false) };
        if raea_io_trace::stat("RAEA_SOUND_TRACE") && f % 30 == 0 {
            eprintln!("raea live: frame {f} sounds {:?}", sounds.iter().map(|s| (s.key.clone(), s.file.file_name().map(|n| n.to_string_lossy().into_owned()), (s.at * 100.0).round() / 100.0, s.volume)).collect::<Vec<_>>());
        }
        if sounds.is_empty() && tone.is_empty() && self.mixer.is_none() {
            self.sound_buf = sounds;
            return;
        }
        if self.mixer.is_none() && !self.mixer_failed {
            match raea_runtime_sound::Mixer::open() {
                Ok(mut m) => {
                    if let Some((gain, ceiling)) = self.game_gain {
                        m.master(gain, ceiling);
                    }
                    self.mixer = Some(m)
                }
                Err(e) => {
                    eprintln!("raea live: no sound: {e}");
                    self.mixer_failed = true;
                }
            }
        }
        if let Some(rx) = &self.game_master {
            match rx.try_recv() {
                Ok(Ok(g)) => {
                    if raea_io_trace::stat("RAEA_LIVE_TRACE") {
                        eprintln!("raea live: the game's master is in at frame {f}: {g:?}");
                    }
                    self.game_gain = g;
                    self.game_master = None;
                    if let (Some(m), Some((gain, ceiling))) = (self.mixer.as_mut(), g) {
                        m.master(gain, ceiling);
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("raea live: the game's master failed, its voices play at their own gain: {e}");
                    self.game_master = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.game_master = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(m) = self.mixer.as_mut() {
            // the mastered track passes the core's master as it is (it was mastered by the same core)
            m.premastered(self.master_wav.is_some());
            // the lag as of now: settling and placing the videos took time since it was measured
            m.sync_frame(f, self.fps, &sounds, &tone, self.lag.0 + self.lag.1.elapsed().as_secs_f64());
        }
        self.sound_buf = sounds;
    }
}

/// CoreAudio's output device found and its format read on a thread, once a process, so the HAL's start-up (~95 ms) is
/// done by the time the mixer opens, after the object's open and before the clock (lightspeed F5). Called as an open
/// starts, and earlier by whoever knows sound will come (raea play, void's launch). CoreAudio only: on Windows, cpal
/// keeps one device enumerator a process (a OnceLock) made on the thread that first asks, and that thread's COM is torn
/// down when it ends, so the warm thread left the mixer an enumerator whose MMDevApi.dll was unloaded where nothing
/// else held it, and every `raea play` on a machine with no audio device (GitHub's runners) crashed with an access
/// violation as its mixer opened
pub fn warm_audio() {
    if !cfg!(target_vendor = "apple") {
        return;
    }
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = std::thread::Builder::new().name("raea audio warm".into()).spawn(|| {
            use cpal::traits::{DeviceTrait, HostTrait};
            let t = std::time::Instant::now();
            let ok = cpal::default_host().default_output_device().map(|d| d.default_output_config().is_ok()).unwrap_or(false);
            raea_runtime_react::startup_mark(&format!("audio device warm ({ok}, {:.0} ms)", t.elapsed().as_secs_f64() * 1e3));
        });
    });
}
