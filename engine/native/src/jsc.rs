//! JavaScriptCore, the system's JS engine on macOS and iOS, through its C API: a context, global functions backed by
//! Rust, scripts evaluated, globals called. What raea runs its TypeScript on in place of V8 (no engine shipped in the
//! binary, Apple's own GC; jitless on iOS as Apple requires). `__raea_call` and the web shims install as on V8.

use std::ffi::{c_char, c_void};
use std::ptr::null_mut;
use std::time::Duration;

type Ctx = *const c_void;
type GCtx = *mut c_void;
type Obj = *mut c_void;
type Val = *const c_void;
type Str = *mut c_void;

/// A global function's body: (the context, its arguments, where to put an exception) -> its value.
pub type Callback = unsafe extern "C" fn(ctx: Ctx, function: Obj, this: Obj, argc: usize, argv: *const Val, exception: *mut Val) -> Val;

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source_handled: u8) -> i32;
}

/// This thread's run loop run once, without waiting: JavaScriptCore schedules its garbage collections (full and
/// eden, by its activity callbacks) as timers on the thread's run loop, so a JS thread that never runs one only ever
/// collects when allocation forces it, and its old generation grew (a live pong at 120 fps: 29 MB a minute). A JS
/// thread's loop calls this between events.
pub fn pump() {
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.0, 0);
    }
}

#[link(name = "JavaScriptCore", kind = "framework")]
unsafe extern "C" {
    fn JSGlobalContextCreate(global_class: *mut c_void) -> GCtx;
    fn JSGlobalContextRelease(ctx: GCtx);
    fn JSGlobalContextRetain(ctx: GCtx) -> GCtx;
    fn JSContextGetGlobalObject(ctx: Ctx) -> Obj;
    fn JSStringCreateWithUTF8CString(s: *const c_char) -> Str;
    fn JSStringRelease(s: Str);
    fn JSStringGetMaximumUTF8CStringSize(s: Str) -> usize;
    fn JSStringGetUTF8CString(s: Str, buf: *mut c_char, size: usize) -> usize;
    fn JSEvaluateScript(ctx: Ctx, script: Str, this: Obj, source_url: Str, line: i32, exception: *mut Val) -> Val;
    fn JSObjectMakeFunctionWithCallback(ctx: Ctx, name: Str, f: Callback) -> Obj;
    fn JSObjectSetProperty(ctx: Ctx, obj: Obj, name: Str, value: Val, attrs: u32, exception: *mut Val);
    fn JSObjectGetProperty(ctx: Ctx, obj: Obj, name: Str, exception: *mut Val) -> Val;
    fn JSObjectCallAsFunction(ctx: Ctx, f: Obj, this: Obj, argc: usize, argv: *const Val, exception: *mut Val) -> Val;
    fn JSValueIsObject(ctx: Ctx, v: Val) -> bool;
    fn JSValueIsUndefined(ctx: Ctx, v: Val) -> bool;
    fn JSValueIsNull(ctx: Ctx, v: Val) -> bool;
    fn JSValueToStringCopy(ctx: Ctx, v: Val, exception: *mut Val) -> Str;
    fn JSValueToNumber(ctx: Ctx, v: Val, exception: *mut Val) -> f64;
    fn JSValueMakeNumber(ctx: Ctx, n: f64) -> Val;
    fn JSValueMakeString(ctx: Ctx, s: Str) -> Val;
    fn JSValueMakeNull(ctx: Ctx) -> Val;
    fn JSValueMakeUndefined(ctx: Ctx) -> Val;
    fn JSObjectMakeError(ctx: Ctx, argc: usize, argv: *const Val, exception: *mut Val) -> Obj;
    fn JSGarbageCollect(ctx: Ctx);
    fn JSContextGetGroup(ctx: Ctx) -> *const c_void;
    fn JSValueUnprotect(ctx: Ctx, v: Val);
}

fn js_str(s: &str) -> Str {
    // written NUL-terminated (NULs dropped) into a buffer kept per thread (ZeroHeap: a String and a CString a call)
    thread_local! {
        static BUF: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    BUF.with_borrow_mut(|b| {
        b.clear();
        b.extend(s.bytes().filter(|&c| c != 0));
        b.push(0);
        unsafe { JSStringCreateWithUTF8CString(b.as_ptr() as *const c_char) }
    })
}

fn rust_str(s: Str) -> String {
    unsafe {
        let n = JSStringGetMaximumUTF8CStringSize(s);
        let mut buf = vec![0u8; n.max(1)];
        let len = JSStringGetUTF8CString(s, buf.as_mut_ptr() as *mut c_char, n);
        buf.truncate(len.saturating_sub(1));
        String::from_utf8_lossy(&buf).into_owned()
    }
}

/// A value as a string (JS's String(v)).
///
/// # Safety
/// `v` must belong to `ctx`.
pub unsafe fn to_string(ctx: Ctx, v: Val) -> String {
    unsafe {
        let s = JSValueToStringCopy(ctx, v, null_mut());
        if s.is_null() {
            return String::new();
        }
        let out = rust_str(s);
        JSStringRelease(s);
        out
    }
}

/// Argument `i` of a callback as a string ("" when missing).
///
/// # Safety
/// `argv` must hold `argc` values of `ctx`.
pub unsafe fn arg(ctx: Ctx, argc: usize, argv: *const Val, i: usize) -> String {
    if i >= argc {
        return String::new();
    }
    unsafe { to_string(ctx, *argv.add(i)) }
}

/// A string value of `ctx`.
pub fn string(ctx: Ctx, s: &str) -> Val {
    let j = js_str(s);
    let v = unsafe { JSValueMakeString(ctx, j) };
    unsafe { JSStringRelease(j) };
    v
}

pub fn number(ctx: Ctx, n: f64) -> Val {
    unsafe { JSValueMakeNumber(ctx, n) }
}

pub fn undefined(ctx: Ctx) -> Val {
    unsafe { JSValueMakeUndefined(ctx) }
}

/// An Error with `msg`, put in `exception` (a callback's way to throw).
///
/// # Safety
/// `exception` must be the callback's.
pub unsafe fn throw(ctx: Ctx, exception: *mut Val, msg: &str) -> Val {
    unsafe {
        let m = string(ctx, msg);
        let e = JSObjectMakeError(ctx, 1, &m, null_mut());
        if !exception.is_null() {
            *exception = e as Val;
        }
        JSValueMakeUndefined(ctx)
    }
}

/// JavaScriptCore's options for raea, set before its first VM (they're read once per process); every host that makes a
/// context calls this first.
/// - FTL, the top tier, off (JSC_useFTLJIT=true brings it back): its compiler took 75 MB at once ~90 s into a live game
///   and kept it, while the baseline and DFG tiers run raea's JS as fast here.
/// - Polling traps: the DFG tier's code checks for termination at loop heads, so a time limit (js-exec's timeout)
///   stops a loop it compiled; with signal traps such a loop ran on past the limit.
pub fn options() {
    static OPTIONS: std::sync::Once = std::sync::Once::new();
    OPTIONS.call_once(|| {
        for (k, v) in [("JSC_useFTLJIT", "false"), ("JSC_usePollingTraps", "true")] {
            if std::env::var_os(k).is_none() {
                unsafe { std::env::set_var(k, v) };
            }
        }
    });
}

/// JavaScriptCore's allocator gives its free pages back to the system now (WTF::releaseFastMallocFreeMemory, where the
/// system exports it). Freed memory stays resident in it otherwise, returned lazily: a live game's resident size crept
/// ~1 MB a minute with the bytes it held flat. Returns the call's time in ms, or None without it.
pub fn release_free_memory() -> Option<f64> {
    type Release = unsafe extern "C" fn();
    static RELEASE: std::sync::OnceLock<Option<Release>> = std::sync::OnceLock::new();
    let f = RELEASE.get_or_init(|| unsafe {
        let p = dlsym(RTLD_DEFAULT, c"_ZN3WTF27releaseFastMallocFreeMemoryEv".as_ptr());
        (!p.is_null()).then(|| std::mem::transmute::<*mut c_void, Release>(p))
    });
    let f = (*f)?;
    let t = std::time::Instant::now();
    unsafe { f() };
    Some(t.elapsed().as_secs_f64() * 1e3)
}

/// An argument to `Context::call`.
pub enum Arg<'a> {
    Num(f64),
    Str(&'a str),
    Null,
}

/// A JS context of its own (one global object), used from the thread that made it.
pub struct Context {
    ctx: GCtx,
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    pub fn new() -> Context {
        options();
        Context { ctx: unsafe { JSGlobalContextCreate(null_mut()) } }
    }

    fn c(&self) -> Ctx {
        self.ctx as Ctx
    }

    /// The JSGlobalContextRef (for raw use alongside this, as `modules::Modules` does).
    pub fn raw(&self) -> *mut c_void {
        self.ctx
    }

    /// `name` on the global object, a function backed by `f`.
    pub fn set_fn(&self, name: &str, f: Callback) {
        unsafe {
            let n = js_str(name);
            let func = JSObjectMakeFunctionWithCallback(self.c(), n, f);
            JSObjectSetProperty(self.c(), JSContextGetGlobalObject(self.c()), n, func as Val, 0, null_mut());
            JSStringRelease(n);
        }
    }

    /// `src` run as a script (`name` for its stack traces); Err carries what it threw. Promise jobs it queued run before
    /// this returns (JSC drains them as the outermost API call ends).
    pub fn eval(&self, src: &str, name: &str) -> Result<(), String> {
        unsafe {
            let (s, u) = (js_str(src), js_str(name));
            let mut exc: Val = std::ptr::null();
            JSEvaluateScript(self.c(), s, null_mut(), u, 1, &mut exc);
            JSStringRelease(s);
            JSStringRelease(u);
            if !exc.is_null() { Err(self.describe(exc)) } else { Ok(()) }
        }
    }

    /// The global function `name` called with `args`: its value as a number (NaN when it isn't one), or what it threw.
    /// None when there's no such function.
    pub fn call_num(&self, name: &str, args: &[Arg]) -> Option<Result<f64, String>> {
        unsafe {
            let n = js_str(name);
            let f = JSObjectGetProperty(self.c(), JSContextGetGlobalObject(self.c()), n, null_mut());
            JSStringRelease(n);
            if f.is_null() || !JSValueIsObject(self.c(), f) {
                return None;
            }
            let val = |a: &Arg| match a {
                Arg::Num(x) => JSValueMakeNumber(self.c(), *x),
                Arg::Str(s) => string(self.c(), s),
                Arg::Null => JSValueMakeNull(self.c()),
            };
            // up to eight arguments on the stack (ZeroHeap: a Vec a call)
            let mut fixed: [Val; 8] = [std::ptr::null(); 8];
            let heap: Vec<Val>;
            let argv: &[Val] = if args.len() <= fixed.len() {
                for (slot, a) in fixed.iter_mut().zip(args) {
                    *slot = val(a);
                }
                &fixed[..args.len()]
            } else {
                heap = args.iter().map(val).collect();
                &heap
            };
            let mut exc: Val = std::ptr::null();
            let v = JSObjectCallAsFunction(self.c(), f as Obj, null_mut(), argv.len(), argv.as_ptr(), &mut exc);
            if !exc.is_null() {
                return Some(Err(self.describe(exc)));
            }
            if v.is_null() || JSValueIsUndefined(self.c(), v) || JSValueIsNull(self.c(), v) {
                return Some(Ok(f64::NAN));
            }
            Some(Ok(JSValueToNumber(self.c(), v, null_mut())))
        }
    }

    /// A script running longer than `limit` at a time is terminated (JavaScriptCore's watchdog, where the system
    /// exports it): js-exec's timeout for a script that never returns.
    pub fn time_limit(&self, limit: Duration) {
        type SetLimit = unsafe extern "C" fn(*const c_void, f64, Option<unsafe extern "C" fn(Ctx, *mut c_void) -> bool>, *mut c_void);
        unsafe extern "C" fn stop(_: Ctx, _: *mut c_void) -> bool {
            true
        }
        unsafe {
            let p = dlsym(RTLD_DEFAULT, c"JSContextGroupSetExecutionTimeLimit".as_ptr());
            if !p.is_null() {
                let f = std::mem::transmute::<*mut c_void, SetLimit>(p);
                f(JSContextGetGroup(self.c()), limit.as_secs_f64(), Some(stop), null_mut());
            }
        }
    }

    /// `promise` (a protected JSValueRef, unprotected here) kept as the global `name`, with what it settles to noted
    /// beside it for `settled`.
    pub fn watch(&self, name: &str, promise: Val) {
        unsafe {
            let n = js_str(name);
            JSObjectSetProperty(self.c(), JSContextGetGlobalObject(self.c()), n, promise, 0, null_mut());
            JSStringRelease(n);
            JSValueUnprotect(self.c(), promise);
        }
        let _ = self.eval(&format!("Promise.resolve(globalThis.{name}).then(() => {{ globalThis.{name}_state = 1; }}, (e) => {{ globalThis.{name}_state = 2; globalThis.{name}_reason = e && e.stack ? String(e) + '\\n' + e.stack : String(e); }});"), "watch");
    }

    /// What a watched promise settled to: None while pending, Ok when fulfilled, Err(its reason) when rejected.
    pub fn settled(&self, name: &str) -> Option<Result<(), String>> {
        let state = self.global_string(&format!("{name}_state"));
        match state.as_str() {
            "1" => Some(Ok(())),
            "2" => Some(Err(self.global_string(&format!("{name}_reason")))),
            _ => None,
        }
    }

    fn global_string(&self, name: &str) -> String {
        unsafe {
            let n = js_str(name);
            let v = JSObjectGetProperty(self.c(), JSContextGetGlobalObject(self.c()), n, null_mut());
            JSStringRelease(n);
            if v.is_null() || JSValueIsUndefined(self.c(), v) { String::new() } else { to_string(self.c(), v) }
        }
    }

    /// A young-generation (eden) collection now, where the system exports one (JSSynchronousEdenCollectForDebugging):
    /// about a millisecond, as only what was made since the last collection is looked at. Run after each frame, a
    /// frame's garbage dies young instead of being promoted, so the old generation (and its full collections, ~30 ms
    /// stops in a live game) grows only with what lives. False where there's no such call.
    pub fn collect_young(&self) -> bool {
        type Eden = unsafe extern "C" fn(Ctx);
        static EDEN: std::sync::OnceLock<Option<Eden>> = std::sync::OnceLock::new();
        let f = EDEN.get_or_init(|| unsafe {
            let p = dlsym(RTLD_DEFAULT, c"JSSynchronousEdenCollectForDebugging".as_ptr());
            (!p.is_null()).then(|| std::mem::transmute::<*mut c_void, Eden>(p))
        });
        match f {
            Some(f) => {
                unsafe { f(self.c()) };
                true
            }
            None => false,
        }
    }

    fn describe(&self, exc: Val) -> String {
        unsafe {
            let msg = to_string(self.c(), exc);
            // the stack, when the thrown value has one
            if JSValueIsObject(self.c(), exc) {
                let k = js_str("stack");
                let st = JSObjectGetProperty(self.c(), exc as Obj, k, null_mut());
                JSStringRelease(k);
                if !st.is_null() && !JSValueIsUndefined(self.c(), st) {
                    return format!("{msg}\n{}", to_string(self.c(), st));
                }
            }
            msg
        }
    }

    /// A collection now (between frames, as V8's posted GC jobs ran): JavaScriptCore's synchronous full collection
    /// where the system exports it (JSSynchronousGarbageCollectForDebugging), else a request (JSGarbageCollect, which
    /// only schedules one).
    pub fn collect(&self) {
        type Sync = unsafe extern "C" fn(Ctx);
        static SYNC: std::sync::OnceLock<Option<Sync>> = std::sync::OnceLock::new();
        let f = SYNC.get_or_init(|| unsafe {
            let p = dlsym(RTLD_DEFAULT, c"JSSynchronousGarbageCollectForDebugging".as_ptr());
            (!p.is_null()).then(|| std::mem::transmute::<*mut c_void, Sync>(p))
        });
        unsafe {
            match f {
                Some(f) => f(self.c()),
                None => JSGarbageCollect(self.c()),
            }
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { JSGlobalContextRelease(self.ctx) }
    }
}

unsafe extern "C" fn native_call(ctx: Ctx, _f: Obj, _this: Obj, argc: usize, argv: *const Val, exception: *mut Val) -> Val {
    unsafe {
        let name = arg(ctx, argc, argv, 0);
        let json = arg(ctx, argc, argv, 1);
        if !crate::allowed(&name) {
            return throw(ctx, exception, &format!("{name}: not allowed here"));
        }
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or(serde_json::Value::Null);
        match crate::call(&name, &v) {
            Ok(out) => string(ctx, &out.to_string()),
            Err(e) => throw(ctx, exception, &format!("{name}: {e}")),
        }
    }
}

/// `__raea_call` on the context's global, then the web shims run in it (as v8::install).
pub fn install(c: &Context) -> Result<(), String> {
    c.set_fn("__raea_call", native_call);
    c.eval(crate::WEB_JS, "web.js").map_err(|e| format!("web.js: {e}"))
}

/// `install` into a context made elsewhere (a JSGlobalContextRef its owner keeps).
pub fn install_into(gctx: *mut c_void) -> Result<(), String> {
    let c = Context { ctx: unsafe { JSGlobalContextRetain(gctx) } };
    install(&c)
}

/// ES modules on a JavaScriptCore context, through the system's Objective-C module loader (JSScript and a
/// JSModuleLoaderDelegate; macOS 10.15, iOS 13): JavaScriptCore resolves each import against its importer's URL and
/// asks `fetch` for the source at the absolute URL, once per URL (its registry keeps the rest: cycles, live bindings,
/// top-level await all as the spec has them). Bare names never reach `fetch`: the host rewrites them to URLs first
/// (raea-modules' rewrite_specifiers).
pub mod modules {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::{CStr, CString, c_char, c_void};

    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{AnyThread, DefinedClass, define_class, msg_send};

    /// The source of the module at an absolute URL, or why there's none.
    pub type Fetch = Box<dyn Fn(&str) -> Result<String, String>>;

    pub struct State {
        fetch: Fetch,
        /// entry modules being evaluated: JavaScriptCore fetches an entry's own URL too
        entries: RefCell<HashMap<String, String>>,
    }

    fn ns(s: &str) -> Retained<AnyObject> {
        let c = CString::new(s.replace('\0', "")).unwrap();
        unsafe { msg_send![objc2::class!(NSString), stringWithUTF8String: c.as_ptr()] }
    }

    fn rs(o: &AnyObject) -> String {
        unsafe {
            let p: *const c_char = msg_send![o, UTF8String];
            if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
        }
    }

    /// A module script for `url`, or JavaScriptCore's reason it isn't one (a syntax error).
    fn script(vm: &AnyObject, url: &str, source: &str) -> Result<Retained<AnyObject>, String> {
        unsafe {
            let u: Option<Retained<AnyObject>> = msg_send![objc2::class!(NSURL), URLWithString: &*ns(url)];
            let Some(u) = u else { return Err(format!("{url}: not a URL")) };
            let mut err: *mut AnyObject = std::ptr::null_mut();
            let s: Option<Retained<AnyObject>> = msg_send![objc2::class!(JSScript), scriptOfType: 1isize, withSource: &*ns(source), andSourceURL: &*u, andBytecodeCache: std::ptr::null::<AnyObject>(), inVirtualMachine: vm, error: &mut err];
            match s {
                Some(s) => Ok(s),
                None if !err.is_null() => {
                    let d: Retained<AnyObject> = msg_send![&*err, localizedDescription];
                    Err(format!("{url}: {}", rs(&d)))
                }
                None => Err(format!("{url}: not a module")),
            }
        }
    }

    fn call1(f: &AnyObject, arg: &AnyObject) {
        unsafe {
            let arr: Retained<AnyObject> = msg_send![objc2::class!(NSArray), arrayWithObject: arg];
            let _: Option<Retained<AnyObject>> = msg_send![f, callWithArguments: &*arr];
        }
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "RaeaJSCModuleLoader"]
        #[ivars = State]
        struct Loader;

        impl Loader {
            #[unsafe(method(context:fetchModuleForIdentifier:withResolveHandler:andRejectHandler:))]
            fn fetch_module(&self, ctx: &AnyObject, id: &AnyObject, resolve: &AnyObject, reject: &AnyObject) {
                let url = unsafe {
                    let s: Retained<AnyObject> = msg_send![id, toString];
                    rs(&s)
                };
                let st = self.ivars();
                let source = match st.entries.borrow().get(&url) {
                    Some(s) => Ok(s.clone()),
                    None => (st.fetch)(&url),
                };
                let vm: Retained<AnyObject> = unsafe { msg_send![ctx, virtualMachine] };
                match source.and_then(|s| script(&vm, &url, &s)) {
                    Ok(s) => call1(resolve, &s),
                    Err(e) => unsafe {
                        // an Error, so the import's rejection reads as one
                        let err: Retained<AnyObject> = msg_send![objc2::class!(JSValue), valueWithNewErrorFromMessage: &*ns(&e), inContext: ctx];
                        call1(reject, &err);
                    },
                }
            }
        }
    );

    /// A context's module loader: keep it as long as the context runs modules (JavaScriptCore holds its delegate weakly).
    pub struct Modules {
        ctx: Retained<AnyObject>,
        loader: Retained<Loader>,
    }

    impl Modules {
        /// Modules for the context `gctx` (a JSGlobalContextRef), their sources from `fetch`.
        pub fn new(gctx: *mut c_void, fetch: Fetch) -> Modules {
            unsafe {
                let ctx: Retained<AnyObject> = msg_send![objc2::class!(JSContext), contextWithJSGlobalContextRef: gctx];
                let loader = Loader::alloc().set_ivars(State { fetch, entries: RefCell::new(HashMap::new()) });
                let loader: Retained<Loader> = msg_send![super(loader), init];
                let _: () = msg_send![&*ctx, setModuleLoaderDelegate: &*loader];
                Modules { ctx, loader }
            }
        }

        /// `source` evaluated as the module at `url` (its imports fetched as they're reached): the evaluation's promise
        /// (a JSValueRef protected for the caller, who unprotects it), or why it couldn't start (a syntax error).
        pub fn eval(&self, url: &str, source: &str) -> Result<*const c_void, String> {
            unsafe {
                let vm: Retained<AnyObject> = msg_send![&*self.ctx, virtualMachine];
                let s = script(&vm, url, source)?;
                self.loader.ivars().entries.borrow_mut().insert(url.to_string(), source.to_string());
                let p: Option<Retained<AnyObject>> = msg_send![&*self.ctx, evaluateJSScript: &*s];
                self.loader.ivars().entries.borrow_mut().remove(url);
                let Some(p) = p else { return Err(format!("{url}: not evaluated")) };
                let v: *const c_void = msg_send![&*p, JSValueRef];
                let g: *mut c_void = msg_send![&*self.ctx, JSGlobalContextRef];
                JSValueProtect(g as *const c_void, v);
                Ok(v)
            }
        }
    }

    #[link(name = "JavaScriptCore", kind = "framework")]
    unsafe extern "C" {
        fn JSValueProtect(ctx: *const c_void, v: *const c_void);
    }
}
