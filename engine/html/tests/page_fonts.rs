//! system-ui text asks CoreText for no font by a private name: Apple's UI faces (.SFNS-Regular, .LucidaGrandeUI and
//! the .SF faces for other scripts) can't be opened by name, and CoreText answered each request with Times New Roman
//! and the note "Client requested name ..., it will get TimesNewRomanPSMT" (jetway #11, rendering Jetflow's app).
#![cfg(target_os = "macos")]

const PAGE: &str = r#"<!doctype html><html><body style="margin:0">
<div id="root" data-composition-id="t" data-duration="1" data-width="400" data-height="120" style="width:400px;height:120px;background:#fff;font-size:22px">
<div style="font-family: system-ui">system-ui: ⌘ ⌥ ⇧ ✓ → ● ★ € ¶ שלום مرحبا</div>
<div style="font-family: system-ui; font-style: italic; font-weight: 700">italic bold: Quick brown fox</div>
<div style="font-family: BlinkMacSystemFont, sans-serif">BlinkMacSystemFont</div>
</div></body></html>"#;

#[test]
fn system_ui_asks_coretext_for_no_private_names() {
    let dir = std::env::temp_dir().join(format!("raea-page-fonts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_raea"))
        .args(["snapshot", html.to_str().unwrap(), "--at", "0", "-o", dir.join("f.png").to_str().unwrap(), "--profile", "web"])
        .output()
        .expect("raea runs");
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "the snapshot failed: {err}");
    let notes: Vec<&str> = err.lines().filter(|l| l.contains("CoreText note") || l.contains("TimesNewRoman")).collect();
    assert!(notes.is_empty(), "CoreText substituted Times:\n{}", notes.join("\n"));
}
