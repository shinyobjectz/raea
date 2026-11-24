//! raea: what a page asked for and didn't get (#34): the author CSS stylo dropped as it parsed (a property or value it
//! doesn't know, a rule it can't read), and the loads the net provider couldn't serve. Stylo drops what it can't parse
//! silently unless a `ParseErrorReporter` is passed, so a render printed APPLIED over a picture missing some of its
//! code. Each is kept once per property and selector (a style attribute is parsed again on every frame it changes), in
//! the order first seen, for the render to print once at its end and write into its manifest (`take`).
//! User-agent sheets get no reporter: what they drop isn't the page's.

use std::sync::Mutex;
use style::error_reporting::{ContextualParseError, ParseErrorReporter};
use style::stylesheets::UrlExtraData;
use cssparser::ToCss as _;

static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The reporter passed where author CSS is parsed (net.rs, document.rs, node/element.rs, cssom.rs).
pub struct Reporter;
static REPORTER: Reporter = Reporter;

/// The reporter for an author parse, or None with RAEA_NOT_APPLIED=0 (the parse as before: for measuring what it costs).
pub fn reporter() -> Option<&'static dyn ParseErrorReporter> {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    ON.get_or_init(|| std::env::var_os("RAEA_NOT_APPLIED").is_none_or(|v| v != "0")).then_some(&REPORTER as &dyn ParseErrorReporter)
}

impl ParseErrorReporter for Reporter {
    fn report_error(&self, _url: &UrlExtraData, _at: cssparser::SourceLocation, error: ContextualParseError) {
        if let Some(what) = describe(&error) {
            note(what);
        }
    }
}

/// One thing the page didn't get, in words (kept once).
pub fn note(what: String) {
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.contains(&what) {
        seen.push(what);
    }
}

/// What the page didn't get since the last take, in the order first seen.
pub fn take() -> Vec<String> {
    std::mem::take(&mut *SEEN.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Properties a page commonly sets (Tailwind's preflight, CSS resets) that change nothing in a rendered picture, under
/// any vendor prefix: dropping them is no loss, and saying so on every render would be noise again.
const UNSEEN: &[&str] = &["text-size-adjust", "tap-highlight-color", "font-smoothing", "osx-font-smoothing", "appearance", "user-select", "touch-callout", "overflow-scrolling", "user-drag", "tab-size", "focus-ring-color"];

/// A short line for a drop: its property (or rule) and where, never its value, so one property dropped in one rule
/// is one line however often it's parsed.
fn describe(e: &ContextualParseError) -> Option<String> {
    let short = |s: &str| {
        let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if s.chars().count() > 60 { format!("{}…", s.chars().take(60).collect::<String>()) } else { s }
    };
    let name = |decl: &str| decl.split(':').next().unwrap_or(decl).trim().to_string();
    Some(match e {
        ContextualParseError::UnsupportedPropertyDeclaration(decl, _, _) if UNSEEN.iter().any(|p| name(decl).trim_start_matches(['-', '_']).ends_with(p)) || name(decl).starts_with('_') => return None,
        ContextualParseError::UnsupportedPropertyDeclaration(decl, _, selectors) => {
            let at = selectors.last().map_or("a style attribute".to_string(), |s| s.to_css_string());
            format!("{} in {}", name(decl), short(&at))
        }
        ContextualParseError::UnsupportedFontFaceDescriptor(decl, _) => format!("{} in @font-face", name(decl)),
        ContextualParseError::InvalidRule(rule, _) | ContextualParseError::UnsupportedRule(rule, _) => format!("the rule {}", short(rule.split('{').next().unwrap_or(rule))),
        ContextualParseError::InvalidKeyframeRule(rule, _) => format!("the keyframe {}", short(rule)),
        ContextualParseError::InvalidMediaRule(rule, _) => format!("@media {}", short(rule)),
        _ => return None,
    })
}

/// What the page didn't get so far, left in place (the render's manifest, `not_applied`).
pub fn seen() -> Vec<String> {
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The note a render ends with, once (the list taken): what its page didn't apply, or None when it applied it all.
pub fn said() -> Option<String> {
    let all = take();
    let n = all.len();
    let more = if n > 20 { format!("\n  and {} more", n - 20) } else { String::new() };
    let list: String = all.iter().take(20).map(|w| format!("\n  {w}")).collect();
    (n > 0).then(|| format!("NOTE  the page didn't apply {n} thing{}, and is drawn without {}:{list}{more}", if n == 1 { "" } else { "s" }, if n == 1 { "it" } else { "them" }))
}

// ---- fonts (#174) ----------------------------------------------------------------------------------------------------
// A font-family list whose first family isn't installed or loaded draws in the next family that is (the browser's
// serif, Times, when none is), and nothing said so: a web font that didn't load turned a whole video into Times. The
// first named family of each list a text layout uses is asked about as the style is made (`ask_family`) and checked
// against the fonts once that layout is built (`check_families`: the layout's builder holds the font context until
// then). Each miss is kept once as `font "X" not found: drawn in Y` for the render to say as a warning
// (`fonts_missing`). A family that turns up later (a web font still loading at the first layout) takes its line back.
// The platform names for the system face that CSS writes first for other browsers (-apple-system and
// BlinkMacSystemFont off Apple, ui-sans-serif and the other ui- names) aren't checked, and neither is a list bevy_text
// made (it ends in "raea-bevy fallback", and falls to the collection's fallbacks by design).

enum Asked {
    Named(String),
    Generic(parley::GenericFamily),
}

thread_local! {
    /// the lists asked about on this thread since its last layout was built, one a first family
    static ASKED: std::cell::RefCell<Vec<Vec<Asked>>> = const { std::cell::RefCell::new(Vec::new()) };
    /// first families found already: never asked about again
    static FOUND: std::cell::RefCell<std::collections::HashSet<String>> = std::cell::RefCell::new(std::collections::HashSet::new());
}

static FONTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A name CSS writes for the system face that this platform's fonts don't carry (the face is the next in the list).
fn platform_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("ui-") || n == "-apple-system" || n == "blinkmacsystemfont"
}

/// A text style's family list (stylo_to_parley's), its first family asked about when it's named.
pub(crate) fn ask_family(families: &[parley::FontFamilyName<'_>]) {
    let Some(parley::FontFamilyName::Named(first)) = families.first() else { return };
    if platform_name(first) || families.iter().any(|f| matches!(f, parley::FontFamilyName::Named(n) if n.as_ref() == "raea-bevy fallback")) {
        return;
    }
    if FOUND.with(|f| f.borrow().contains(first.as_ref())) {
        return;
    }
    ASKED.with(|a| {
        let mut a = a.borrow_mut();
        if a.len() < 64 && !a.iter().any(|l| matches!(l.first(), Some(Asked::Named(n)) if n.as_str() == first.as_ref())) {
            a.push(families.iter().map(|f| match f {
                parley::FontFamilyName::Named(n) => Asked::Named(n.to_string()),
                parley::FontFamilyName::Generic(g) => Asked::Generic(*g),
            }).collect());
        }
    });
}

/// The lists asked about on this thread, checked against `fcx`'s fonts (after a layout is built).
pub(crate) fn check_families(fcx: &mut parley::FontContext) {
    let asked = ASKED.with(|a| std::mem::take(&mut *a.borrow_mut()));
    for list in asked {
        let Some(Asked::Named(first)) = list.first() else { continue };
        let missing = format!("font \"{first}\" not found");
        if fcx.collection.family_by_name(first).is_some() {
            FOUND.with(|f| f.borrow_mut().insert(first.clone()));
            FONTS.lock().unwrap_or_else(|e| e.into_inner()).retain(|l| !l.starts_with(&missing));
            continue;
        }
        let drawn = list[1..].iter().find_map(|f| match f {
            Asked::Named(n) => fcx.collection.family_by_name(n).map(|i| i.name().to_string()),
            Asked::Generic(g) => {
                let id = fcx.collection.generic_families(*g).next()?;
                fcx.collection.family_name(id).map(str::to_string)
            }
        });
        let line = format!("{missing}: drawn in {}", drawn.as_deref().unwrap_or("the platform's fallback"));
        let mut fonts = FONTS.lock().unwrap_or_else(|e| e.into_inner());
        if !fonts.contains(&line) {
            fonts.push(line);
        }
    }
}

/// The fonts a page asked for and didn't have, taken: `font "X" not found: drawn in Y`, in the order first met.
pub fn fonts_missing() -> Vec<String> {
    std::mem::take(&mut *FONTS.lock().unwrap_or_else(|e| e.into_inner()))
}
