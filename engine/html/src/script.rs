//! The page's script engine: blitz-v8-script with the `scripts` feature (on in raea-motion, rdb, void and the browser),
//! else a stand-in with the same surface that runs nothing. An exported game's player is TSX on the lite runtime and
//! never runs an HTML composition's <script>s, so it goes without (lightspeed R7: blitz-v8-script and the TypeScript
//! toolchain under it, swc and deno_ast, were ~3 MB of its code).

#[cfg(feature = "scripts")]
pub use blitz_v8_script::{jit_on, FetchError, ScriptDocument, ScriptFetcher};

#[cfg(not(feature = "scripts"))]
pub use stand_in::*;

#[cfg(not(feature = "scripts"))]
mod stand_in {
    use blitz_dom::{BaseDocument, DocGuard, DocGuardMut, Document};
    use std::time::Instant;
    use url::Url;

    /// Why a script fetch failed (the stand-in never fetches).
    #[derive(Debug)]
    pub enum FetchError {
        UnsupportedScheme(String),
        Io(std::io::Error),
        InvalidData(String),
    }

    impl std::fmt::Display for FetchError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                FetchError::UnsupportedScheme(s) => write!(f, "unsupported scheme: {s}"),
                FetchError::Io(e) => write!(f, "{e}"),
                FetchError::InvalidData(s) => write!(f, "invalid data: {s}"),
            }
        }
    }

    /// Where a page's scripts would load from (kept so callers build the same with or without scripts).
    pub trait ScriptFetcher: 'static {
        fn fetch(&self, url: &Url) -> Result<String, FetchError>;
    }

    /// The page with no script engine: its document as parsed, its scripts never run, its clock virtual.
    pub struct ScriptDocument {
        doc: BaseDocument,
        clock: Instant,
    }

    impl ScriptDocument {
        pub fn from_base_document(doc: BaseDocument) -> Self {
            ScriptDocument { doc, clock: Instant::now() }
        }
        pub fn without_timer_thread(self) -> Self {
            self
        }
        pub fn with_virtual_time(self) -> Self {
            self
        }
        pub fn with_fetcher(self, _fetcher: impl ScriptFetcher) -> Self {
            self
        }
        pub fn with_native(self, _name: &str, _f: impl Fn(&str) -> Result<String, String> + 'static) -> Self {
            self
        }
        pub fn clock_now(&self) -> Instant {
            self.clock
        }
        pub fn advance_clock_to(&mut self, deadline: Instant) {
            if deadline > self.clock {
                self.clock = deadline;
            }
        }
        pub fn execute_scripts(&mut self) {}
        pub fn eval(&mut self, _code: &str) {}
        pub fn take_messages(&mut self) -> Vec<String> {
            Vec::new()
        }
        pub fn take_js_errors(&mut self) -> Vec<String> {
            Vec::new()
        }
        pub fn next_timer_deadline(&self) -> Option<Instant> {
            None
        }
    }

    impl Document for ScriptDocument {
        fn inner(&self) -> DocGuard<'_> {
            DocGuard::Ref(&self.doc)
        }
        fn inner_mut(&mut self) -> DocGuardMut<'_> {
            DocGuardMut::Ref(&mut self.doc)
        }
    }

    pub fn jit_on() -> bool {
        false
    }
}
