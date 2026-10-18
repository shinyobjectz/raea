//! The window (Dioxus native on the RAEA engine: blitz-shell and winit): everything the app asks of its window goes
//! through here (its size and scale, a drag, the NSWindow for the native stage, test clicks and drops over MCP).

mod imp {
    use std::sync::{Arc, Mutex};

    use dioxus_native::winit::window::Window;

    /// The window, kept by the app's root as it mounts (`use_window` is a hook).
    static WINDOW: Mutex<Option<Arc<dyn Window>>> = Mutex::new(None);

    pub fn keep(w: Arc<dyn Window>) {
        *WINDOW.lock().unwrap() = Some(w);
    }
    /// The window, called with the lock let go: winit sends some calls (request_redraw from the redraw thread) to the
    /// main thread and waits, and the main thread asking for the scale under the lock then waited on that thread forever.
    fn with<T>(f: impl FnOnce(&dyn Window) -> T, or: T) -> T {
        let w = WINDOW.lock().unwrap().clone();
        w.as_deref().map(f).unwrap_or(or)
    }
    pub fn size() -> (f64, f64) {
        with(|w| { let s = w.surface_size(); let k = w.scale_factor(); (s.width as f64 / k, s.height as f64 / k) }, (1200.0, 800.0))
    }
    pub fn scale() -> f64 {
        with(|w| w.scale_factor(), 1.0)
    }
    pub fn drag() {
        with(|w| { let _ = w.drag_window(); }, ());
    }
    /// A frame asked for (the composited stage painted one), from any thread without waiting: the stage calls this while
    /// it holds the shared GPU device, and winit's request_redraw can wait on the main thread, which may be waiting on
    /// that device; a one-slot channel and a thread of its own keep the stage from ever blocking on it.
    pub fn redraw() {
        use std::sync::mpsc::{sync_channel, SyncSender};
        static TICK: std::sync::OnceLock<SyncSender<()>> = std::sync::OnceLock::new();
        let tx = TICK.get_or_init(|| {
            let (tx, rx) = sync_channel::<()>(1);
            std::thread::spawn(move || {
                while rx.recv().is_ok() {
                    with(|w| w.request_redraw(), ());
                }
            });
            tx
        });
        let _ = tx.try_send(());
    }
    /// A click at (x, y) in the window's content, in points, posted to void's own event queue (left or right button):
    /// what a person's click does, through AppKit, winit and Blitz, without moving the pointer or making void the
    /// active app. For tests over MCP (act "click" / "right_click").
    pub fn post_click(x: f64, y: f64, right: bool) -> Result<(), String> {
        use objc2::encode::{Encode, Encoding};
        use objc2::runtime::AnyObject;
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Point { x: f64, y: f64 }
        unsafe impl Encode for Point {
            const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
        }
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGEventCreateCopy(event: *const std::ffi::c_void) -> *mut std::ffi::c_void;
            fn CGEventSetIntegerValueField(event: *mut std::ffi::c_void, field: u32, value: i64);
            fn CFRelease(cf: *const std::ffi::c_void);
        }
        let ns = ns_window();
        if ns.is_null() {
            return Err("no window".into());
        }
        let (_, h) = size();
        // AppKit's window coordinates start at the bottom left
        let at = Point { x, y: h - y };
        let w = unsafe { &*(ns as *const AnyObject) };
        let number: isize = unsafe { objc2::msg_send![w, windowNumber] };
        // NSEventTypeLeftMouseDown 1, Up 2, RightMouseDown 3, Up 4, MouseMoved 5
        let (down, up) = if right { (3usize, 4usize) } else { (1, 2) };
        unsafe {
            let app: *mut AnyObject = objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
            for (kind, clicks, pressure) in [(5usize, 0isize, 0.0f32), (down, 1, 1.0), (up, 1, 0.0)] {
                let mut e: *mut AnyObject = objc2::msg_send![objc2::class!(NSEvent),
                    mouseEventWithType: kind, location: at, modifierFlags: 0usize, timestamp: 0.0f64,
                    windowNumber: number, context: std::ptr::null_mut::<AnyObject>(), eventNumber: 0isize,
                    clickCount: clicks, pressure: pressure];
                if e.is_null() {
                    return Err("AppKit made no event".into());
                }
                // AppKit's event says button 0 whatever its type, and winit reads the button from it: the right button
                // set on its CGEvent (kCGMouseEventButtonNumber, field 3)
                if right && kind != 5 {
                    let cg: *const std::ffi::c_void = objc2::msg_send![e, CGEvent];
                    let copy = CGEventCreateCopy(cg);
                    CGEventSetIntegerValueField(copy, 3, 1);
                    e = objc2::msg_send![objc2::class!(NSEvent), eventWithCGEvent: copy];
                    CFRelease(copy);
                    if e.is_null() {
                        return Err("AppKit made no event".into());
                    }
                }
                let _: () = objc2::msg_send![app, postEvent: e, atStart: false];
            }
        }
        Ok(())
    }
    /// One step of a drop as Finder makes one, sent to the window's dragging destination (winit's window delegate):
    /// 0 entered, 1 updated (answers the operation the window takes: 0 is a refusal), 2 perform, 3 conclude. The
    /// dragging info is an object of our own with a pasteboard holding the files' URLs. For tests over MCP (act "drop").
    pub fn drop_step(paths: &[String], x: f64, y: f64, step: u8) -> Result<usize, String> {
        use objc2::encode::{Encode, Encoding};
        use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
        use objc2::{msg_send, sel};
        use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Point { x: f64, y: f64 }
        unsafe impl Encode for Point {
            const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
        }
        static PASTEBOARD: AtomicUsize = AtomicUsize::new(0);
        static AT: Mutex<(f64, f64)> = Mutex::new((0.0, 0.0));
        static INFO: AtomicUsize = AtomicUsize::new(0);
        extern "C-unwind" fn pasteboard(_: &AnyObject, _: Sel) -> *mut AnyObject { PASTEBOARD.load(Relaxed) as *mut AnyObject }
        extern "C-unwind" fn location(_: &AnyObject, _: Sel) -> Point { let (x, y) = *AT.lock().unwrap(); Point { x, y } }
        extern "C-unwind" fn operations(_: &AnyObject, _: Sel) -> usize { 1 | 2 | 16 } // copy, link, move
        extern "C-unwind" fn sequence(_: &AnyObject, _: Sel) -> isize { 7341 }
        let ns = ns_window();
        if ns.is_null() {
            return Err("no window".into());
        }
        let w = unsafe { &*(ns as *const AnyObject) };
        unsafe {
            if INFO.load(Relaxed) == 0 {
                let cls = match AnyClass::get(c"VoidTestDraggingInfo") {
                    Some(c) => c,
                    None => {
                        let mut b = ClassBuilder::new(c"VoidTestDraggingInfo", objc2::class!(NSObject)).ok_or("no class")?;
                        b.add_method(sel!(draggingPasteboard), pasteboard as extern "C-unwind" fn(_, _) -> _);
                        b.add_method(sel!(draggingLocation), location as extern "C-unwind" fn(_, _) -> _);
                        b.add_method(sel!(draggingSourceOperationMask), operations as extern "C-unwind" fn(_, _) -> _);
                        b.add_method(sel!(draggingSequenceNumber), sequence as extern "C-unwind" fn(_, _) -> _);
                        b.register()
                    }
                };
                let info: *mut AnyObject = msg_send![cls, new];
                INFO.store(info as usize, Relaxed);
            }
            if step == 0 {
                let pb: *mut AnyObject = msg_send![objc2::class!(NSPasteboard), pasteboardWithUniqueName];
                let _: isize = msg_send![pb, clearContents];
                let urls: *mut AnyObject = msg_send![objc2::class!(NSMutableArray), array];
                for p in paths {
                    let c = std::ffi::CString::new(p.as_str()).map_err(|e| e.to_string())?;
                    let s: *mut AnyObject = msg_send![objc2::class!(NSString), stringWithUTF8String: c.as_ptr()];
                    let u: *mut AnyObject = msg_send![objc2::class!(NSURL), fileURLWithPath: s];
                    let _: () = msg_send![urls, addObject: u];
                }
                let ok: bool = msg_send![pb, writeObjects: urls];
                if !ok {
                    return Err("the pasteboard took no files".into());
                }
                let _: *mut AnyObject = msg_send![pb, retain];
                PASTEBOARD.store(pb as usize, Relaxed);
                // the window's coordinates start at the bottom left
                *AT.lock().unwrap() = (x, size().1 - y);
            }
            let info = &*(INFO.load(Relaxed) as *const AnyObject);
            let delegate: *mut AnyObject = msg_send![w, delegate];
            let delegate = &*delegate;
            Ok(match step {
                0 => msg_send![delegate, draggingEntered: info],
                1 => msg_send![delegate, draggingUpdated: info],
                2 => { let ok: bool = msg_send![delegate, performDragOperation: info]; ok as usize }
                _ => { let _: () = msg_send![delegate, concludeDragOperation: info]; 0 }
            })
        }
    }
    /// The NSWindow behind winit's view, for the native stage.
    pub fn ns_window() -> *mut std::ffi::c_void {
        use dioxus_native::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        with(|w| match w.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::AppKit(h)) => unsafe {
                let view = h.ns_view.as_ptr() as *const objc2::runtime::AnyObject;
                let win: *mut objc2::runtime::AnyObject = objc2::msg_send![&*view, window];
                win as *mut std::ffi::c_void
            },
            _ => std::ptr::null_mut(),
        }, std::ptr::null_mut())
    }
}

pub use imp::*;

/// Whether macOS shows any of the window (NSWindow occlusionState): a covered window isn't drawn, so its screenshots go stale.
pub fn visible() -> bool {
    let ns = ns_window();
    if ns.is_null() {
        return false;
    }
    let occ: usize = unsafe { objc2::msg_send![&*(ns as *const objc2::runtime::AnyObject), occlusionState] };
    occ & 2 != 0
}
