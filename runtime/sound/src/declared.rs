//! Whether an edit's bundle writes an <Audio> that always plays (raea-runtime-export: a render with no sound although it
//! does is an error, not a silent file).

/// Whether a sound's file is there, saying so once when it isn't (the caller remembers each file): a render dropped
/// it without a word and then refused as silent, with nothing naming the file (0.1.9: three reports in a day, each
/// read as a Windows bug; the files were in the project's assets/, which an edit in compositions/ didn't search).
pub fn there(file: &std::path::Path, src: &str) -> bool {
    let there = file.exists();
    if !there {
        eprintln!("raea: no sound file {} (src \"{src}\"), so it's silent: staticFile looks in the project's media/, assets/ and public/", file.display());
    }
    there
}

/// True when the bundle's JSX has an <Audio> call, `(…jsx)(Audio, {`, that isn't behind a condition. One behind `?` or
/// `&&` is silent when its condition is false, as a sound-only cut's in the starter edit is before there are any cuts
/// (`c.kind === "audio" ? <Audio … /> : <Video … />`), and that render is meant to be silent. One in a wrapper's children
/// (`children: (…jsx)(Audio, {`) always plays, so a wrapper that drops its children (a <Mix> did, 2026-10-18) is still
/// caught. What the bundle holds is what the edit renders (not minified, what isn't used left out).
pub fn always_plays_audio(bundle: &str) -> bool {
    bundle.match_indices(")(Audio").any(|(i, m)| {
        let rest = bundle[i + m.len()..].trim_start_matches(|c: char| c.is_ascii_digit());
        if !(rest.starts_with(", {") || rest.starts_with(",{")) {
            return false;
        }
        // back over the call's own `(0, import_jsx_runtime.jsx` and esbuild's `/* @__PURE__ */`
        let before = bundle[..i].rfind('(').map_or("", |j| &bundle[..j]);
        let before = before.trim_end().trim_end_matches("/* @__PURE__ */").trim_end();
        !(before.ends_with('?') || before.ends_with("&&"))
    })
}

#[cfg(test)]
mod tests {
    use super::always_plays_audio;

    #[test]
    fn an_audio_behind_a_condition_isnt_one_that_always_plays() {
        let starter = r#"children: c.kind === "audio" ? /* @__PURE__ */ (0, import_jsx_runtime.jsx)(Audio, { src: c.src }) : /* @__PURE__ */ (0, import_jsx_runtime.jsx)(Video, { src: c.src })"#;
        assert!(!always_plays_audio(starter));
        assert!(!always_plays_audio(r#"music && /* @__PURE__ */ (0, import_jsx_runtime.jsx)(Audio2, { src: music })"#));
        // a wrapper's child, and a call on its own: both always play
        assert!(always_plays_audio(r#"(0, import_jsx_runtime.jsx)(Mix, { children: /* @__PURE__ */ (0, import_jsx_runtime.jsx)(Audio, { src: "bed.wav" }) })"#));
        assert!(always_plays_audio(r#"return /* @__PURE__ */ (0, import_jsx_runtime.jsxs)(AbsoluteFill, { children: [(0, import_jsx_runtime.jsx)(Audio, { src: "a.wav" })] })"#));
        // the component's own definition and a type check aren't calls of it
        assert!(!always_plays_audio("function Audio(props) {}\nif (c.type === Audio) {}"));
    }
}
