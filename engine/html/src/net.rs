//! Synchronous resource loading for Blitz: project files, data: URLs, and http(s) assets
//! (web fonts, CDN images) cached on disk so repeated renders are offline and deterministic.

use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use url::Url;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:60.0) Gecko/20100101 Firefox/81.0";
/// Google Fonts serves different files per browser; Chrome's (and the composition format's own fetches) are
/// what its renders use.
pub const CHROME_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub struct SyncNet {
    pub cache_dir: Option<PathBuf>,
    pub allow_network: bool,
    pub warnings: Mutex<Vec<String>>,
    /// Every load: the URL and its size, or the error (an html node reports these in its info)
    pub fetched: Mutex<Vec<(String, Result<usize, String>)>>,
    /// Once sealed, loads fail: an html node resolves everything at open (moonsplice P6), so a fetch
    /// during render is an error, listed in `fetched`, never a late network call.
    pub sealed: std::sync::atomic::AtomicBool,
}

/// `<caches>/raea`. A cache from before the rename (`<caches>/runicorn`) is moved there once, so
/// fonts and decoded video frames aren't fetched again.
fn cache_home(caches: &Path) -> PathBuf {
    let dir = caches.join("raea");
    let old = caches.join("runicorn");
    if !dir.exists() && old.is_dir() {
        let _ = std::fs::rename(&old, &dir);
    }
    dir
}

impl SyncNet {
    pub fn new(project_dir: &Path, allow_network: bool) -> SyncNet {
        let cache_dir = crate::env_os("CACHE")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| cache_home(&PathBuf::from(h).join("Library/Caches"))));
        let _ = project_dir;
        SyncNet {
            cache_dir,
            allow_network,
            warnings: Mutex::new(Vec::new()),
            fetched: Mutex::new(Vec::new()),
            sealed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn warn(&self, msg: String) {
        let mut w = self.warnings.lock().unwrap();
        if !w.contains(&msg) {
            w.push(msg);
        }
    }

    pub fn load(&self, url: &Url) -> Result<Vec<u8>, String> {
        let result = if self.sealed.load(std::sync::atomic::Ordering::Relaxed) {
            Err("fetched after the node was opened (resources resolve at open)".to_string())
        } else {
            self.load_unrecorded(url)
        };
        let record = result.as_ref().map(|b| b.len()).map_err(|e| e.clone());
        self.fetched.lock().unwrap().push((url.to_string(), record));
        result
    }

    fn load_unrecorded(&self, url: &Url) -> Result<Vec<u8>, String> {
        match url.scheme() {
            "data" => {
                let data = data_url::DataUrl::process(url.as_str()).map_err(|e| format!("{e:?}"))?;
                let (body, _) = data.decode_to_vec().map_err(|e| format!("{e:?}"))?;
                Ok(body)
            }
            "file" => {
                let path = url.to_file_path().map_err(|_| format!("bad file url {url}"))?;
                std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
            }
            "http" | "https" => self.load_remote(url),
            other => Err(format!("unsupported scheme {other}")),
        }
    }

    fn cache_path(&self, url: &Url) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        let mut h: u64 = 0xcbf29ce484222325;
        for b in url.as_str().bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        Some(dir.join(format!("{h:016x}")))
    }

    fn load_remote(&self, url: &Url) -> Result<Vec<u8>, String> {
        // a page's own Google Fonts stylesheet loads as Chrome would load it
        if url.host_str() == Some("fonts.googleapis.com") {
            return self.load_remote_as(url, CHROME_USER_AGENT);
        }
        self.load_remote_as(url, USER_AGENT)
    }

    /// An http(s) load with a specific User-Agent (Google Fonts serves different CSS per browser).
    pub fn load_with_agent(&self, url: &Url, agent: &str) -> Result<Vec<u8>, String> {
        self.load_remote_as(url, agent)
    }

    fn load_remote_as(&self, url: &Url, agent: &str) -> Result<Vec<u8>, String> {
        let cached = if agent == USER_AGENT {
            self.cache_path(url)
        } else {
            self.cache_path(&Url::parse(&format!("{url}#ua={}", agent.len())).unwrap_or_else(|_| url.clone()))
        };
        if let Some(p) = &cached {
            if let Ok(bytes) = std::fs::read(p) {
                return Ok(bytes);
            }
        }
        if !self.allow_network {
            return Err(format!("{url}: network disabled and not cached"));
        }
        let bytes = http_get(url, agent)?;
        if let Some(p) = &cached {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let tmp = p.with_extension("part");
            if std::fs::write(&tmp, &bytes).is_ok() {
                let _ = std::fs::rename(&tmp, p);
            }
        }
        Ok(bytes)
    }
}

#[cfg(feature = "network")]
fn http_get(url: &Url, agent: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url.as_str())
        .set("User-Agent", agent)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut bytes).map_err(|e| format!("{url}: {e}"))?;
    Ok(bytes)
}

/// Built without the `network` feature (iOS): only the on-disk cache serves http(s) URLs.
#[cfg(not(feature = "network"))]
fn http_get(url: &Url, _agent: &str) -> Result<Vec<u8>, String> {
    Err(format!("{url}: not cached, and this build has no network support (the `network` feature is off)"))
}

impl NetProvider for SyncNet {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        match self.load(&request.url) {
            Ok(bytes) => handler.bytes(request.url.to_string(), Bytes::from(bytes)),
            Err(e) => {
                self.warn(format!("could not load {}: {e}", short(&request.url)));
                // Answer anyway: Blitz holds all rendering until every render-blocking request
                // (a <link rel=stylesheet>) responds, so an unanswered failure blanked the whole
                // frame (offline, or a cold cache with --no-network). Empty bytes complete it
                // as an empty stylesheet / a failed image or font.
                handler.bytes(request.url.to_string(), Bytes::new());
            }
        }
    }
}

fn short(url: &Url) -> String {
    let s = url.as_str();
    if s.len() > 140 { format!("{}…", &s[..140]) } else { s.to_string() }
}
