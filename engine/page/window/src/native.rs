//! What a Mac app does that a page can't (docs/onboarding.feature OB-N): the clipboard, links opened in the browser,
//! notifications, the dock badge, and raea:// links coming back to the app. The page asks through host calls:
//!
//!   clipboard.write {text}        clipboard.read -> text
//!   app.open {url}                an http(s) or mailto link, in the person's browser
//!   notify {title, body}          a macOS notification (asked for once, the first time: notify.permission)
//!   notify.permission -> bool     macOS's prompt, or its answer if already given
//!   dock.badge {text}             the Dock icon's badge, on Windows the taskbar button's overlay ("" clears it)
//!   app.urls -> string[]          raea:// links opened since the last ask, the app brought to the front for each
//!   app.drops -> {over, dropped}  files dragged from the Finder: whether some are over the window, the paths dropped since the last ask
//!   app.share {paths, x, y}       macOS's share sheet for those files (absolute paths), beside the point (x, y) in the
//!                                 window from its top left (Shane 11-14: right-click any image or video to share it)
//!   native.log -> {...}           what was done, for tests
//!
//! Live only on screen (--window) and without RAEA_NO_OPEN=1: otherwise every call is recorded and answered as if done,
//! so headless runs never touch the person's clipboard, browser or notifications. Notifications and raea:// links need
//! the app's identity: the window's program in raea.app's Contents/MacOS, which makes raea.app its main bundle
//! (tools/raea-app/launch.sh starts it there as its child, never exec'd: an exec left the app without a pid in AppKit,
//! #163); elsewhere notify answers an error that says so.
//! AppKit is called on the main thread: the calls queue here and `drain` (a NonSend system) runs them.

use std::sync::Mutex;

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use raea_runtime_js::calls::{Answer, HostCalls};
use serde_json::{Value, json};

/// Whether calls reach macOS, or are only recorded.
#[derive(Resource, Clone, Copy)]
pub struct Live(pub bool);

/// What was done, live or recorded: read by native.log.
#[derive(Default)]
struct Log {
    clipboard: String,
    opened: Vec<String>,
    notified: Vec<Value>,
    badge: String,
    urls: Vec<String>,
    shared: Vec<String>,
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);
/// raea:// links that arrived and the page hasn't read yet (filled by the Apple event handler, on the main thread).
static URLS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Files dragged onto the window: whether some are over it now, and the paths dropped the page hasn't read yet (drops).
static DROPS: Mutex<(bool, Vec<String>)> = Mutex::new((false, Vec::new()));

fn log<T>(f: impl FnOnce(&mut Log) -> T) -> T {
    f(LOG.lock().unwrap().get_or_insert_with(Log::default))
}

/// A call waiting for the main thread.
struct Job(String, Value, Answer);

#[derive(Resource, Default)]
struct Queue(Mutex<Vec<Job>>);

/// The calls, live or recorded. Call once, after HostCalls exists.
pub struct NativePlugin {
    pub live: bool,
}

impl Plugin for NativePlugin {
    fn build(&self, app: &mut App) {
        let live = self.live && std::env::var("RAEA_NO_OPEN").as_deref() != Ok("1");
        app.insert_resource(Live(live)).init_resource::<Queue>().init_resource::<HostCalls>().add_systems(Startup, (calls, start)).add_systems(Update, (drops, drain).chain());
    }
}

fn calls(mut calls: ResMut<HostCalls>) {
    for name in ["clipboard.write", "clipboard.read", "app.open", "notify", "notify.permission", "dock.badge", "app.urls", "app.drops", "app.share", "native.log"] {
        calls.on(name, move |w, args, answer| w.resource::<Queue>().0.lock().unwrap().push(Job(name.to_string(), args, answer)));
    }
}

/// On screen: raea:// links are handed to the app from here on.
fn start(live: Res<Live>, _main: NonSendMarker) {
    if live.0 {
        mac::listen_for_urls();
    }
}

/// Bevy's drag-and-drop messages (winit's), kept for app.drops: live or not, a file dropped on the window is the
/// person's own act, never something a call does to them.
fn drops(mut evs: MessageReader<bevy::window::FileDragAndDrop>) {
    use bevy::window::FileDragAndDrop as D;
    for e in evs.read() {
        let mut d = DROPS.lock().unwrap();
        match e {
            D::HoveredFile { .. } => d.0 = true,
            D::HoveredFileCanceled { .. } => d.0 = false,
            D::DroppedFile { path_buf, .. } => {
                d.0 = false;
                d.1.push(path_buf.to_string_lossy().into_owned());
            }
        }
    }
}

fn drain(world: &mut World) {
    let jobs = std::mem::take(&mut *world.resource::<Queue>().0.lock().unwrap());
    if jobs.is_empty() {
        return;
    }
    let live = world.resource::<Live>().0;
    for Job(name, args, answer) in jobs {
        run(live, &name, &args, answer);
    }
}

// NonSend: AppKit wants its main thread. An exclusive system with this marker would be enough; drain takes &mut World,
// which Bevy's executors already run on the main thread.

fn text(a: &Value, k: &str) -> String {
    a.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn run(live: bool, name: &str, a: &Value, answer: Answer) {
    match name {
        "clipboard.write" => {
            let t = text(a, "text");
            if live && let Err(e) = mac::clipboard_write(&t) {
                return answer.err(e);
            }
            log(|l| l.clipboard = t);
            answer.ok(json!(true))
        }
        "clipboard.read" => answer.ok(json!(if live { mac::clipboard_read() } else { log(|l| l.clipboard.clone()) })),
        "app.open" => {
            let url = text(a, "url");
            if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")) {
                return answer.err(format!("app.open: {url:?} isn't an http(s) or mailto link"));
            }
            // a link that didn't open says so, so the page can show it to copy rather than a button that does nothing
            if live && let Err(e) = mac::open(&url) {
                return answer.err(e);
            }
            log(|l| l.opened.push(url));
            answer.ok(json!(true))
        }
        "notify" => {
            let (title, body) = (text(a, "title"), text(a, "body"));
            if live && let Err(e) = mac::notify(&title, &body) {
                return answer.err(e);
            }
            log(|l| l.notified.push(json!({"title": title, "body": body})));
            answer.ok(json!(true))
        }
        "notify.permission" => {
            if live {
                mac::ask_permission(answer)
            } else {
                answer.ok(json!(true))
            }
        }
        "dock.badge" => {
            let t = text(a, "text");
            if live {
                mac::badge(&t);
                // the taskbar button's overlay (win.rs)
                #[cfg(all(windows, feature = "window"))]
                crate::win::badge(&t);
            }
            log(|l| l.badge = t);
            answer.ok(json!(true))
        }
        "app.urls" => {
            // a test hands the app a link with {push: url} when nothing is live (a raea:// link opened from outside)
            if !live && let Some(u) = a.get("push").and_then(Value::as_str) {
                URLS.lock().unwrap().push(u.to_string());
            }
            let urls = std::mem::take(&mut *URLS.lock().unwrap());
            log(|l| l.urls.extend(urls.iter().cloned()));
            answer.ok(json!(urls))
        }
        "app.drops" => {
            let mut d = DROPS.lock().unwrap();
            let dropped = std::mem::take(&mut d.1);
            answer.ok(json!({"over": d.0, "dropped": dropped}))
        }
        "app.share" => {
            let paths: Vec<String> = a.get("paths").and_then(Value::as_array).map(|v| v.iter().filter_map(|p| p.as_str().map(String::from)).collect()).unwrap_or_default();
            if paths.is_empty() || paths.iter().any(|p| !std::path::Path::new(p).is_absolute()) {
                return answer.err("app.share: paths are the files' absolute paths");
            }
            let at = |k: &str| a.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            if live && let Err(e) = mac::share(&paths, at("x"), at("y")) {
                return answer.err(e);
            }
            log(|l| l.shared.extend(paths));
            answer.ok(json!(true))
        }
        "native.log" => answer.ok(log(|l| json!({"live": live, "clipboard": l.clipboard, "opened": l.opened, "notified": l.notified, "badge": l.badge, "urls": l.urls, "shared": l.shared}))),
        _ => answer.err(format!("no native call {name}")),
    }
}

#[cfg(all(target_os = "macos", feature = "window"))]
mod mac {
    use super::{Answer, URLS};
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Bool, NSObject};
    use objc2::{ClassType, define_class, msg_send};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use serde_json::json;
    use std::ffi::CString;
    use std::sync::Mutex;

    #[link(name = "UserNotifications", kind = "framework")]
    unsafe extern "C" {}
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {}

    fn class(name: &str) -> Option<&'static AnyClass> {
        AnyClass::get(&CString::new(name).ok()?)
    }


    fn rust(s: *mut AnyObject) -> String {
        if s.is_null() {
            return String::new();
        }
        let p: *const std::ffi::c_char = unsafe { msg_send![s, UTF8String] };
        if p.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned() }
    }

    fn app() -> *mut AnyObject {
        unsafe { msg_send![class("NSApplication").unwrap(), sharedApplication] }
    }

    /// NSPasteboardTypeString's value
    const PLAIN: &str = "public.utf8-plain-text";

    /// The system clipboard, or (tests) a private pasteboard by name, which never touches the person's clipboard.
    fn pasteboard(name: Option<&str>) -> *mut AnyObject {
        let c = class("NSPasteboard").unwrap();
        match name {
            Some(n) => unsafe { msg_send![c, pasteboardWithName: &*NSString::from_str(n)] },
            None => unsafe { msg_send![c, generalPasteboard] },
        }
    }

    pub fn clipboard_write(t: &str) -> Result<(), String> {
        write_to(None, t);
        Ok(())
    }

    pub fn clipboard_read() -> String {
        read_from(None)
    }

    pub(super) fn write_to(name: Option<&str>, t: &str) {
        unsafe {
            let pb = pasteboard(name);
            let _: isize = msg_send![pb, clearContents];
            let _: Bool = msg_send![pb, setString: &*NSString::from_str(t), forType: &*NSString::from_str(PLAIN)];
        }
    }

    pub(super) fn read_from(name: Option<&str>) -> String {
        unsafe {
            let pb = pasteboard(name);
            rust(msg_send![pb, stringForType: &*NSString::from_str(PLAIN)])
        }
    }

    pub fn open(url: &str) -> Result<(), String> {
        unsafe {
            let u: *mut AnyObject = msg_send![class("NSURL").unwrap(), URLWithString: &*NSString::from_str(url)];
            if u.is_null() {
                return Err(format!("app.open: macOS can't read {url:?} as a link"));
            }
            let ws: *mut AnyObject = msg_send![class("NSWorkspace").unwrap(), sharedWorkspace];
            let opened: Bool = msg_send![ws, openURL: u];
            if opened.as_bool() { Ok(()) } else { Err(format!("app.open: macOS didn't open {url}")) }
        }
    }

    /// NSSharingServicePicker for the files, shown beside the point (x, y), from the top left of the window's content
    /// in points (the page's own pixels). The picker stays up after this returns; it isn't released (one small object
    /// a share).
    pub fn share(paths: &[String], x: f64, y: f64) -> Result<(), String> {
        unsafe {
            let a = app();
            let mut win: *mut AnyObject = msg_send![a, keyWindow];
            if win.is_null() {
                win = msg_send![a, mainWindow];
            }
            if win.is_null() {
                return Err("raea's window isn't open".into());
            }
            let view: *mut AnyObject = msg_send![win, contentView];
            if view.is_null() {
                return Err("raea's window has no view to show the share sheet from".into());
            }
            let items: *mut AnyObject = msg_send![class("NSMutableArray").unwrap(), array];
            for p in paths {
                let url: *mut AnyObject = msg_send![class("NSURL").unwrap(), fileURLWithPath: &*NSString::from_str(p)];
                if !url.is_null() {
                    let _: () = msg_send![items, addObject: url];
                }
            }
            let picker: *mut AnyObject = msg_send![class("NSSharingServicePicker").ok_or("AppKit's share sheet isn't here")?, alloc];
            let picker: *mut AnyObject = msg_send![picker, initWithItems: items];
            if picker.is_null() {
                return Err("the share sheet didn't open".into());
            }
            let bounds: NSRect = msg_send![view, bounds];
            let flipped: Bool = msg_send![view, isFlipped];
            let at = NSRect::new(NSPoint::new(x, if flipped.as_bool() { y } else { bounds.size.height - y }), NSSize::new(1.0, 1.0));
            // NSRectEdgeMinY
            let _: () = msg_send![picker, showRelativeToRect: at, ofView: view, preferredEdge: 1usize];
        }
        Ok(())
    }

    pub fn badge(t: &str) {
        unsafe {
            let tile: *mut AnyObject = msg_send![app(), dockTile];
            let label: *const AnyObject = if t.is_empty() { std::ptr::null() } else { Retained::into_raw(NSString::from_str(t)).cast::<AnyObject>() };
            let _: () = msg_send![tile, setBadgeLabel: label];
        }
    }

    /// UNUserNotificationCenter refuses a process with no bundle identifier (it raises), so that is checked first.
    fn center() -> Result<*mut AnyObject, String> {
        unsafe {
            let bundle: *mut AnyObject = msg_send![class("NSBundle").unwrap(), mainBundle];
            let id: *mut AnyObject = msg_send![bundle, bundleIdentifier];
            if id.is_null() {
                return Err("notifications need raea.app (this window isn't running as the app)".into());
            }
            let c = class("UNUserNotificationCenter").ok_or("UserNotifications isn't loaded")?;
            Ok(msg_send![c, currentNotificationCenter])
        }
    }

    pub fn ask_permission(answer: Answer) {
        let center = match center() {
            Ok(c) => c,
            Err(e) => return answer.err(e),
        };
        let slot = Mutex::new(Some(answer));
        let done = RcBlock::new(move |granted: Bool, _err: *mut AnyObject| {
            if let Some(a) = slot.lock().unwrap().take() {
                a.ok(json!(granted.as_bool()))
            }
        });
        // alert | sound | badge
        let opts: usize = (1 << 2) | (1 << 1) | 1;
        unsafe {
            let _: () = msg_send![center, requestAuthorizationWithOptions: opts, completionHandler: &*done];
        }
    }

    pub fn notify(title: &str, body: &str) -> Result<(), String> {
        let center = center()?;
        unsafe {
            let content: Retained<AnyObject> = msg_send![class("UNMutableNotificationContent").ok_or("no UNMutableNotificationContent")?, new];
            let _: () = msg_send![&*content, setTitle: &*NSString::from_str(title)];
            let _: () = msg_send![&*content, setBody: &*NSString::from_str(body)];
            let id = NSString::from_str(&format!("raea-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
            let none: *const AnyObject = std::ptr::null();
            let req: *mut AnyObject = msg_send![class("UNNotificationRequest").unwrap(), requestWithIdentifier: &*id, content: &*content, trigger: none];
            let _: () = msg_send![center, addNotificationRequest: req, withCompletionHandler: none];
        }
        Ok(())
    }

    define_class!(
        // AppKit's Apple event handler for kAEGetURL: a raea:// link opened anywhere on the Mac
        #[unsafe(super(NSObject))]
        #[name = "RaeaUrlHandler"]
        struct UrlHandler;

        impl UrlHandler {
            #[unsafe(method(handle:reply:))]
            fn handle(&self, event: *mut AnyObject, _reply: *mut AnyObject) {
                // keyDirectObject '----'
                let desc: *mut AnyObject = unsafe { msg_send![event, paramDescriptorForKeyword: 0x2d2d2d2du32] };
                let url = if desc.is_null() { String::new() } else { rust(unsafe { msg_send![desc, stringValue] }) };
                if url.starts_with("raea://") {
                    URLS.lock().unwrap().push(url);
                }
                let _: () = unsafe { msg_send![app(), activateIgnoringOtherApps: Bool::YES] };
            }
        }
    );

    /// A kAEGetURL event carrying `url`, handed to the handler as AppKit would (tests).
    #[cfg(test)]
    pub(super) fn deliver(url: &str) {
        unsafe {
            let cls = class("NSAppleEventDescriptor").unwrap();
            let none: *const AnyObject = std::ptr::null();
            let gurl: u32 = 0x4755_524c;
            let event: *mut AnyObject = msg_send![cls, appleEventWithEventClass: gurl, eventID: gurl, targetDescriptor: none, returnID: -1i16, transactionID: 0i32];
            let s: *mut AnyObject = msg_send![cls, descriptorWithString: &*NSString::from_str(url)];
            let _: () = msg_send![event, setParamDescriptor: s, forKeyword: 0x2d2d2d2du32];
            let h: Retained<UrlHandler> = msg_send![UrlHandler::class(), new];
            let _: () = msg_send![&*h, handle: event, reply: none];
        }
    }

    pub fn listen_for_urls() {
        unsafe {
            // kept for the app's life: the event manager doesn't retain its handler
            let handler: *mut AnyObject = Retained::<UrlHandler>::into_raw(msg_send![UrlHandler::class(), new]) as *mut AnyObject;
            let mgr: *mut AnyObject = msg_send![class("NSAppleEventManager").unwrap(), sharedAppleEventManager];
            // kInternetEventClass and kAEGetURL are both 'GURL'; the handler lives as long as the app
            let gurl: u32 = 0x4755_524c;
            let _: () = msg_send![mgr, setEventHandler: handler, andSelector: objc2::sel!(handle:reply:), forEventClass: gurl, andEventID: gurl];
        }
    }
}

/// Off AppKit: Linux's and Windows' window, through raea-io-os, and no window at all (other.rs).
#[cfg(not(all(target_os = "macos", feature = "window")))]
#[path = "other.rs"]
mod mac;

#[cfg(all(test, target_os = "macos", feature = "window"))]
mod tests {
    use super::{URLS, mac};

    #[test]
    fn pasteboard_round_trip_on_a_private_board() {
        let name = format!("io.raea.test.{}", std::process::id());
        mac::write_to(Some(&name), "RAEA-CODE-42 ✓");
        assert_eq!(mac::read_from(Some(&name)), "RAEA-CODE-42 ✓");
    }

    #[test]
    fn a_raea_link_reaches_the_queue_and_others_dont() {
        mac::deliver("raea://linked?code=ABC123");
        mac::deliver("https://example.com/not-ours");
        let urls = std::mem::take(&mut *URLS.lock().unwrap());
        assert_eq!(urls, vec!["raea://linked?code=ABC123".to_string()]);
    }

    #[test]
    fn notifications_outside_the_app_say_why() {
        let e = mac::notify("t", "b").unwrap_err();
        assert!(e.contains("raea.app"), "{e}");
    }
}

#[cfg(test)]
mod drop_tests {
    use super::{DROPS, drops};
    use bevy::prelude::*;
    use bevy::window::FileDragAndDrop as D;

    /// What app.drops answers: files over the window while they're dragged, then the paths once, as they're dropped.
    #[test]
    fn a_finder_drag_says_over_then_hands_its_paths_once() {
        let mut app = App::new();
        app.add_message::<D>().add_systems(Update, drops);
        let window = Entity::PLACEHOLDER;
        app.world_mut().write_message(D::HoveredFile { window, path_buf: "/tmp/a.mov".into() });
        app.update();
        assert!(DROPS.lock().unwrap().0, "over while dragged");
        app.world_mut().write_message(D::HoveredFileCanceled { window });
        app.update();
        assert!(!DROPS.lock().unwrap().0, "not over once it leaves");
        app.world_mut().write_message(D::DroppedFile { window, path_buf: "/tmp/a.mov".into() });
        app.world_mut().write_message(D::DroppedFile { window, path_buf: "/tmp/b c.png".into() });
        app.update();
        let mut d = DROPS.lock().unwrap();
        assert!(!d.0);
        assert_eq!(std::mem::take(&mut d.1), vec!["/tmp/a.mov".to_string(), "/tmp/b c.png".to_string()]);
    }
}
