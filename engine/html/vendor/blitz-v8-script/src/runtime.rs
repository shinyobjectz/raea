//! The script runtime: owns the JavaScriptCore context, registers the DOM globals and dispatches
//! events / timers into JavaScript. Ported from blitz-vibey-script's Boa runtime (by way of V8,
//! retired: raea runs on the system's engine); the DOM natives, event dispatch and the JS bootstrap
//! are shared or copied, the engine glue is new.

use std::cell::RefCell;
use std::rc::Rc;

use blitz_dom::{BaseDocument, NodeId};
use blitz_traits::events::{DomEvent, DomEventData, EventState};
use crate::js::object::{JsObject, ObjectInitializer};
use crate::js::property::Attribute;
use crate::js::value::JsValue;
use crate::js::{Context, CtxGuard, JsError, JsNativeError, JsResult, JsString, NativeFunction, js_string};
use url::Url;
use web_time::{Duration, Instant};

use crate::dom::event::{EventRef, create_event, create_event_for_dom_event};
use crate::dom::{
    NodeRef, dom_ctx, js_str, node_id_of_value, node_wrapper, to_rust_string, wrap_style_object,
};
use crate::fetch::ScriptFetcher;
use crate::state::{DomCtx, Listener, ReadyState};

/// The shared JS bootstrap (DOM interfaces, CSSOM, fetch, history, ...), from blitz-vibey-script
const BOOTSTRAP_JS: &str = include_str!("js/bootstrap.js");

/// Geometry Interfaces (DOMPoint, DOMRect, DOMQuad, DOMMatrix), defined in JS
const GEOMETRY_JS: &str = include_str!("js/geometry.js");

/// Web platform APIs the React ecosystem calls, defined in JS over the native DOM (raea)
const WEB_JS: &str = include_str!("js/web.js");
/// local change (raea, jetway #1): the Streams API (web-streams-polyfill 4.3.0, MIT: js/streams.LICENSE), evaluated
/// by web.js the first time a page reaches for a stream class (__blitz_streams_source), so pages that don't pay nothing
const STREAMS_JS: &str = include_str!("js/streams.js");

/// What boa_runtime supplied (console, URL, TextEncoder, atob/btoa, structuredClone) and Date on
/// the document clock
const V8_PRELUDE_JS: &str = include_str!("v8_prelude.js");

#[link(name = "JavaScriptCore", kind = "framework")]
unsafe extern "C" {
    fn JSGlobalContextCreate(global_class: crate::js::Class) -> crate::js::GCtx;
    fn JSGlobalContextRelease(ctx: crate::js::GCtx);
    fn JSValueUnprotect(ctx: crate::js::Ctx, v: crate::js::Val);
    fn JSGlobalContextSetUnhandledRejectionCallback(ctx: crate::js::GCtx, function: *mut std::ffi::c_void, exception: *mut crate::js::Val);
}

/// [`ScriptDocument::take_js_errors`](crate::ScriptDocument::take_js_errors),
/// then fire an `error` event at the window (the HTML "report an exception"
/// steps) so that `window.onerror` / `addEventListener("error")` handlers
/// observe it.
fn report_js_error(ctx: &DomCtx, context: &mut Context, what: &str, error: &JsError) {
    #[cfg(feature = "tracing")]
    tracing::error!("Uncaught JS error in {what}: {error}");
    let already_dispatching = {
        let mut state = ctx.state.borrow_mut();
        state.record_error(format!("Uncaught JS error in {what}: {error}"));
        std::mem::replace(&mut state.dispatching_error_event, true)
    };
    if already_dispatching {
        return;
    }
    dispatch_error_event(ctx, context, error);
    ctx.state.borrow_mut().dispatching_error_event = false;
}

/// Fire an `ErrorEvent`-shaped `error` event at the window for an uncaught
/// exception. Exceptions thrown by the handlers themselves are recorded (via
/// [`report_js_error`]) but do not fire further `error` events.
fn dispatch_error_event(ctx: &DomCtx, context: &mut Context, error: &JsError) {
    let listeners: Vec<Listener> = {
        let mut state = ctx.state.borrow_mut();
        match state.window_listeners.get_mut("error") {
            Some(listeners) => {
                let cloned = listeners.clone();
                listeners.retain(|l| !l.once);
                cloned
            }
            None => Vec::new(),
        }
    };
    let onerror = context
        .global_object()
        .get(js_string!("onerror"), context)
        .ok()
        .and_then(|handler| handler.as_object().filter(|h| h.is_callable()));
    if listeners.is_empty() && onerror.is_none() {
        return;
    }

    let global: JsValue = context.global_object().into();
    let event_obj = create_event(ctx, "error", false, true, &global, context);
    crate::dom::define_value(&event_obj, "currentTarget", global.clone(), context);
    let error_value = error
        .clone()
        .into_opaque(context)
        .unwrap_or_else(|_| js_str(&error.to_string()));
    let message = error_value
        .as_object()
        .and_then(|obj| obj.get(js_string!("message"), context).ok())
        .filter(|message| message.is_string())
        .unwrap_or_else(|| js_str(&error.to_string()));
    crate::dom::define_value(&event_obj, "message", message.clone(), context);
    crate::dom::define_value(&event_obj, "filename", js_str(""), context);
    crate::dom::define_value(&event_obj, "lineno", JsValue::from(0), context);
    crate::dom::define_value(&event_obj, "colno", JsValue::from(0), context);
    crate::dom::define_value(&event_obj, "error", error_value.clone(), context);

    for listener in listeners {
        if let Err(error) = listener
            .callback
            .call(&global, &[event_obj.clone().into()], context)
        {
            report_js_error(ctx, context, "error event listener", &error);
        }
    }
    // `window.onerror(message, source, lineno, colno, error)`
    if let Some(handler) = onerror {
        let args = [
            message,
            js_str(""),
            JsValue::from(0),
            JsValue::from(0),
            error_value,
        ];
        if let Err(error) = handler.call(&global, &args, context) {
            report_js_error(ctx, context, "error event listener", &error);
        }
    }
}

/// ES module loading: JavaScriptCore resolves each import against its importer's URL and asks for
/// the source at the absolute URL, fetched synchronously through the document's [`ScriptFetcher`];
/// bare names (import map keys) are rewritten to URLs in each module's text first, as
/// JavaScriptCore refuses them unresolved.
struct ModuleLoader {
    fetcher: Rc<RefCell<Box<dyn ScriptFetcher>>>,
    base_url: Option<Url>,
    /// raea: import map entries (specifier or prefix ending in `/` -> URL), longest first.
    import_map: Vec<(String, Url)>,
    /// inline module scripts so far (each gets a URL of its own, under the document's)
    inline: usize,
}

impl ModuleLoader {
    fn resolve_specifier(&self, referrer: Option<&Url>, specifier: &str) -> Option<Url> {
        for (key, target) in self.import_map.iter() {
            if specifier == key {
                return Some(target.clone());
            }
            if key.ends_with('/') && specifier.starts_with(key.as_str()) {
                if let Ok(url) = target.join(&specifier[key.len()..]) {
                    return Some(url);
                }
            }
        }
        if let Ok(url) = Url::parse(specifier) {
            return Some(url);
        }
        let base = referrer.cloned().or_else(|| self.base_url.clone())?;
        base.join(specifier).ok()
    }

    /// A module's text with its imports resolved to absolute URLs (the import map applied).
    fn rewrite(&self, url: &Url, code: &str) -> String {
        raea_modules::rewrite_specifiers(url.as_str(), code, |spec| self.resolve_specifier(Some(url), spec).map(|u| u.to_string())).unwrap_or_else(|_| code.to_string())
    }
}

type LoaderSlot = Rc<RefCell<ModuleLoader>>;

/// Promise rejections nobody handled (JavaScriptCore reports them after the microtasks that could
/// have handled them ran), reported as "Uncaught (in promise)", like a browser console
#[derive(Default)]
struct Rejections(Vec<String>);

type RejectionSlot = Rc<RefCell<Rejections>>;

thread_local! {
    /// The rejection list of the runtime whose context is active (JavaScriptCore's callback is a JS function)
    static REJECTIONS: RefCell<Option<RejectionSlot>> = const { RefCell::new(None) };
}

fn unhandled_rejection(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let reason = args.get(1).cloned().unwrap_or_default();
    let text = JsError::from_opaque(reason).to_string();
    let _ = context;
    REJECTIONS.with(|r| {
        if let Some(r) = r.borrow().as_ref() {
            r.borrow_mut().0.push(text);
        }
    });
    Ok(JsValue::undefined())
}

/// Console output goes to the `log` crate at `debug` level (target `js_console`)
fn console(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let message = to_rust_string(args.get(1).unwrap_or(&JsValue::undefined()), context)?;
    log::debug!(target: "js_console", "{message}");
    Ok(JsValue::undefined())
}

/// `__blitz_date_now()`: milliseconds since the epoch on the document clock (virtual or real)
fn date_now(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let millis = ctx.state.borrow().date_clock.as_ref().map(|c| c.now_millis()).unwrap_or(0.0);
    Ok(JsValue::from(millis))
}

fn url_fields(url: &Url, context: &mut Context) -> JsValue {
    let host = match (url.host_str(), url.port()) {
        (Some(h), Some(p)) => format!("{h}:{p}"),
        (Some(h), None) => h.to_string(),
        _ => String::new(),
    };
    let search = match url.query() {
        Some(q) if !q.is_empty() => format!("?{q}"),
        _ => String::new(),
    };
    let hash = match url.fragment() {
        Some(f) if !f.is_empty() => format!("#{f}"),
        _ => String::new(),
    };
    let fields = [
        url.as_str().to_string(),
        format!("{}:", url.scheme()),
        url.username().to_string(),
        url.password().unwrap_or("").to_string(),
        host,
        url.host_str().unwrap_or("").to_string(),
        url.port().map(|p| p.to_string()).unwrap_or_default(),
        url.path().to_string(),
        search,
        hash,
        url.origin().ascii_serialization(),
    ];
    crate::js::object::builtins::JsArray::from_iter(fields.iter().map(|f| js_str(f)), context).into()
}

/// `__blitz_url_parse(input, base)`: the URL's fields (see v8_prelude.js), or null if invalid
fn url_parse(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let input = to_rust_string(args.first().unwrap_or(&JsValue::undefined()), context)?;
    let base = match args.get(1) {
        Some(b) if !b.is_null_or_undefined() => Some(to_rust_string(b, context)?),
        _ => None,
    };
    let parsed = match base {
        Some(base) => Url::parse(&base).and_then(|b| b.join(&input)),
        None => Url::parse(&input),
    };
    Ok(match parsed {
        Ok(url) => url_fields(&url, context),
        Err(_) => JsValue::null(),
    })
}

/// `__blitz_url_set(href, field, value)`: the href after setting one component, or null
fn url_set(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let href = to_rust_string(args.first().unwrap_or(&JsValue::undefined()), context)?;
    let field = to_rust_string(args.get(1).unwrap_or(&JsValue::undefined()), context)?;
    let value = to_rust_string(args.get(2).unwrap_or(&JsValue::undefined()), context)?;
    let Ok(mut url) = Url::parse(&href) else {
        return Ok(JsValue::null());
    };
    let ok = match field.as_str() {
        "href" => match Url::parse(&value) {
            Ok(u) => {
                url = u;
                true
            }
            Err(_) => false,
        },
        "protocol" => url.set_scheme(value.trim_end_matches(':')).is_ok(),
        "username" => url.set_username(&value).is_ok(),
        "password" => url.set_password(Some(&value)).is_ok(),
        "host" => url.set_host(Some(&value)).is_ok(),
        "hostname" => url.set_host(Some(&value)).is_ok(),
        "port" => url.set_port(value.parse().ok()).is_ok(),
        "pathname" => {
            url.set_path(&value);
            true
        }
        "search" => {
            let q = value.trim_start_matches('?');
            url.set_query(if q.is_empty() { None } else { Some(q) });
            true
        }
        "hash" => {
            let f = value.trim_start_matches('#');
            url.set_fragment(if f.is_empty() { None } else { Some(f) });
            true
        }
        _ => false,
    };
    Ok(if ok { js_str(url.as_str()) } else { JsValue::null() })
}

pub(crate) struct ScriptRuntime {
    /// Boa-shaped code reaches the active context through this token
    pub context: Context,
    pub ctx: DomCtx,
    loader: LoaderSlot,
    rejections: RejectionSlot,
    modules: raea_native::jsc::modules::Modules,
    /// the JavaScriptCore context, released when the runtime goes (after everything above)
    gctx: crate::js::GCtx,
}

impl Drop for ScriptRuntime {
    fn drop(&mut self) {
        // the values Rust still holds go with the context (the module loader's JSContext lets it go last)
        crate::js::drop_host(self.gctx);
        unsafe { JSGlobalContextRelease(self.gctx) };
    }
}

impl ScriptRuntime {
    pub fn new(
        doc: Rc<RefCell<BaseDocument>>,
        base_url: Option<&Url>,
        fetcher: Rc<RefCell<Box<dyn ScriptFetcher>>>,
    ) -> Self {
        // a null class: JavaScriptCore's own API global, the one its module loader serves
        raea_native::jsc::options();
        let gctx = unsafe { JSGlobalContextCreate(std::ptr::null_mut()) };
        crate::js::new_host(gctx);
        let loader: LoaderSlot = Rc::new(RefCell::new(ModuleLoader {
            fetcher: Rc::clone(&fetcher),
            base_url: base_url.cloned(),
            import_map: Vec::new(),
            inline: 0,
        }));
        let rejections: RejectionSlot = Rc::new(RefCell::new(Rejections::default()));
        // modules: each fetched through the document's fetcher, its bare imports resolved first
        let modules = {
            let loader = loader.clone();
            raea_native::jsc::modules::Modules::new(gctx as *mut std::ffi::c_void, Box::new(move |url| {
                let url = Url::parse(url).map_err(|e| format!("{url}: {e}"))?;
                let l = loader.borrow();
                let code = l.fetcher.borrow().fetch(&url).map_err(|e| format!("failed to fetch module {url}: {e}"))?;
                Ok(l.rewrite(&url, &code))
            }))
        };

        let ctx = DomCtx::new(doc);
        {
            let mut state = ctx.state.borrow_mut();
            state.base_url = base_url.cloned();
            state.fetcher = Some(fetcher);
            state.date_clock = Some(crate::clock::DateClock::new(state.clock.clone()));
        }

        let mut runtime = Self {
            context: Context::current(),
            ctx,
            loader,
            rejections,
            modules,
            gctx,
        };
        runtime.enter(|rt| {
            rt.install_globals(base_url);
            // unhandled rejections come to a function of ours
            let f = NativeFunction::from_fn_ptr(unhandled_rejection).to_function("__blitz_unhandled_rejection", 2);
            unsafe { JSGlobalContextSetUnhandledRejectionCallback(rt.gctx, f.raw(), std::ptr::null_mut()) };
        });
        runtime.install_native();
        runtime
    }

    /// (raea) raea-native's `__raea_call` (every registered native call) and its web shims, which fill only what the
    /// page's globals lack: composition scripts reach the same calls as @raea/motion does in raea-react and the apps.
    fn install_native(&mut self) {
        if let Err(e) = raea_native::jsc::install_into(self.gctx as *mut std::ffi::c_void) {
            eprintln!("blitz-v8-script: raea-native: {e}");
        }
    }

    /// Run `f` with this runtime's context active (for the facade). (raea) Several pages can share a thread (the
    /// node API, tests): the active context is this one for the call, the previous one after.
    pub(crate) fn enter<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let _guard = CtxGuard::enter(self.gctx);
        let previous = REJECTIONS.with(|r| r.replace(Some(self.rejections.clone())));
        struct Restore(Option<RejectionSlot>);
        impl Drop for Restore {
            fn drop(&mut self) {
                REJECTIONS.with(|r| *r.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(previous);
        f(self)
    }

    fn install_globals(&mut self, base_url: Option<&Url>) {
        let ctx = self.ctx.clone();
        let context = &mut self.context;
        context.insert_data(ctx.clone());

        register_global_fn(context, "__blitz_console", 2, console);
        register_global_fn(context, "__blitz_date_now", 0, date_now);
        register_global_fn(context, "__blitz_url_parse", 2, url_parse);
        register_global_fn(context, "__blitz_url_set", 3, url_set);

        crate::dom::init_protos(&ctx, context);

        // `document`
        let root_id = ctx.doc.borrow().root_node().id;
        let document_wrapper = node_wrapper(&ctx, root_id, context);
        register_global(context, "document", document_wrapper.into());

        // `window` and friends (aliases for the global object)
        let global: JsValue = context.global_object().into();
        register_global(context, "window", global.clone());
        register_global(context, "self", global.clone());
        // There is only ever a single frame, so `parent` and `top` refer to the
        // window itself and `opener` is null
        register_global(context, "parent", global.clone());
        register_global(context, "top", global);
        register_global(context, "opener", JsValue::null());

        // `location`
        let location = build_location(base_url, context);
        register_global(context, "location", location);

        // `navigator`
        let navigator = ObjectInitializer::new(context)
            .property(
                js_string!("userAgent"),
                js_string!("Mozilla/5.0 (compatible; Blitz)"),
                Attribute::all(),
            )
            .build();
        register_global(context, "navigator", navigator.into());

        // Timers and window event listeners
        register_global_fn(context, "setTimeout", 2, set_timeout);
        register_global_fn(context, "clearTimeout", 1, clear_timer);
        register_global_fn(context, "setInterval", 2, set_interval);
        register_global_fn(context, "clearInterval", 1, clear_timer);
        register_global_fn(context, "requestAnimationFrame", 1, request_animation_frame);
        register_global_fn(context, "cancelAnimationFrame", 1, clear_timer);
        register_global_fn(context, "addEventListener", 2, window_add_event_listener);
        register_global_fn(context, "removeEventListener", 2, window_remove_event_listener);

        // Embedder message channel (see `ScriptDocument::take_messages`)
        register_global_fn(context, "__blitz_send_message", 1, send_message);
        register_global_fn(context, "__blitz_streams_source", 0, streams_source);

        // Synchronous resource fetch backing the bootstrap's `fetch()`
        register_global_fn(context, "__blitz_fetch_sync", 1, fetch_sync);

        // Embedder native bindings (local change, raea)
        register_global_fn(context, "__blitz_native", 2, call_native);

        // CSS property support check, used by the style Proxy's `has` trap
        register_global_fn(context, "__blitz_css_property_supported", 1, css_property_supported);

        // `getComputedStyle`
        register_global_fn(context, "getComputedStyle", 1, get_computed_style);
        register_global_fn(context, "getSelection", 0, crate::dom::selection::get_selection);
        register_global_fn(context, "__blitz_parse_transform", 1, parse_transform);

        // CSSOM stylesheet natives (`__blitz_sheet_*`), used by the bootstrap's
        // `CSSStyleSheet` / `CSSRule` implementation
        crate::dom::stylesheet::register(context);

        // Viewport dimensions
        register_global_accessor(context, "innerWidth", inner_width);
        register_global_accessor(context, "innerHeight", inner_height);
        register_global_accessor(context, "outerWidth", inner_width);
        register_global_accessor(context, "outerHeight", inner_height);
        register_global_accessor(context, "devicePixelRatio", device_pixel_ratio);

        // Viewport scrolling
        register_global_accessor(context, "scrollX", scroll_x);
        register_global_accessor(context, "scrollY", scroll_y);
        register_global_accessor(context, "pageXOffset", scroll_x);
        register_global_accessor(context, "pageYOffset", scroll_y);
        register_global_fn(context, "scroll", 2, window_scroll_to);
        register_global_fn(context, "scrollTo", 2, window_scroll_to);
        register_global_fn(context, "scrollBy", 2, window_scroll_by);

        // The `CSS` namespace object. `CSS.escape` is defined in the JS bootstrap.
        let css_namespace = ObjectInitializer::new(context)
            .function(NativeFunction::from_fn_ptr(css_supports), js_string!("supports"), 1)
            .function(NativeFunction::from_fn_ptr(css_register_property), js_string!("registerProperty"), 1)
            .build();
        register_global(context, "CSS", css_namespace.into());

        self.eval_internal(V8_PRELUDE_JS, "<blitz-v8-prelude>");
        self.eval_internal(BOOTSTRAP_JS, "<blitz-bootstrap>");
        self.eval_internal(GEOMETRY_JS, "<blitz-geometry>");
        self.eval_internal(WEB_JS, "<blitz-web>");
    }

    /// Evaluate a script, logging (but not propagating) any uncaught errors,
    /// then drain the microtask queue.
    pub fn eval(&mut self, code: &str, description: &str) {
        self.eval_internal(code, description);
        self.run_jobs(description);
    }

    fn eval_internal(&mut self, code: &str, description: &str) {
        if let Err(error) = self.context.eval(code, description) {
            report_js_error(&self.ctx, &mut self.context, description, &error);
        }
    }

    /// Run pending promise jobs (microtasks), then report rejections nobody handled
    pub fn run_jobs(&mut self, description: &str) {
        if let Err(error) = self.context.run_jobs() {
            report_js_error(&self.ctx, &mut self.context, description, &error);
        }
        let unhandled = std::mem::take(&mut self.rejections.borrow_mut().0);
        for reason in unhandled {
            self.ctx
                .state
                .borrow_mut()
                .record_error(format!("Uncaught (in promise) in {description}: {reason}"));
        }
    }

    /// Evaluate an ES module script: compile it, load its imports (via the module
    /// loader), then instantiate and evaluate it. Uncaught errors are logged (but not
    /// propagated), matching [`eval`](Self::eval).
    pub fn eval_module(&mut self, code: &str, url: Option<&Url>) {
        let description = url.map(Url::as_str).unwrap_or("<inline module>").to_string();
        // an inline module gets a URL of its own under the document's, so its relative imports resolve against it
        let url = match url {
            Some(u) => u.clone(),
            None => {
                let mut l = self.loader.borrow_mut();
                l.inline += 1;
                let base = l.base_url.clone().unwrap_or_else(|| Url::parse("about:blank").unwrap());
                let mut u = if base.cannot_be_a_base() { Url::parse("raea-inline:///index.html").unwrap() } else { base };
                u.set_fragment(Some(&format!("module-{}", l.inline)));
                u
            }
        };
        let code = self.loader.borrow().rewrite(&url, code);
        match self.modules.eval(url.as_str(), &code) {
            Err(e) => report_js_error(&self.ctx, &mut self.context, &description, &JsError::from(JsNativeError::syntax().with_message(e))),
            Ok(p) => {
                let c = crate::js::ctx();
                let promise = JsValue::from_jsc(c, p);
                unsafe { JSValueUnprotect(c, p) };
                // the evaluation's failure (a syntax error, a failed import, a throw) reported when it settles
                let ctx = self.ctx.clone();
                let what = description.clone();
                let on_reject = NativeFunction::from_closure(move |_, args, context| {
                    let reason = args.first().cloned().unwrap_or_default();
                    report_js_error(&ctx, context, &what, &JsError::from_opaque(reason));
                    Ok(JsValue::undefined())
                })
                .to_function("", 1);
                if let Some(then) = promise.as_object().and_then(|p| p.get(js_string!("then"), &mut self.context).ok()).and_then(|t| t.as_object()) {
                    let _ = then.call(&promise, &[JsValue::undefined(), on_reject.into()], &mut self.context);
                }
                self.run_jobs(&description);
            }
        }
    }

    /// raea: add an import map's `imports` (targets resolve against `base`).
    pub(crate) fn add_import_map(&self, json: &str, base: Option<&Url>) {
        let mut loader = self.loader.borrow_mut();
        for (key, value) in import_map_imports(json) {
            let target = Url::parse(&value).ok().or_else(|| base.and_then(|b| b.join(&value).ok()));
            if let Some(target) = target {
                loader.import_map.push((key, target));
            }
        }
        loader.import_map.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    }

    /// The deadline of the soonest pending timer (if any)
    pub fn next_timer_deadline(&self) -> Option<Instant> {
        self.ctx.state.borrow().timers.next_deadline()
    }

    /// Set the value exposed as `document.readyState`
    pub fn set_ready_state(&mut self, ready_state: ReadyState) {
        self.ctx.state.borrow_mut().ready_state = ready_state;
    }

    /// Install the `<body onload="...">` attribute (if any) as the window's
    /// `load` event handler, per the HTML spec (event handler attributes on
    /// `<body>` apply to the window). Handlers assigned to `window.onload` by
    /// scripts take precedence.
    pub fn install_body_onload_attribute(&mut self) {
        let code: Option<String> = {
            let doc = self.ctx.doc.borrow();
            let mut stack = vec![doc.root_node().id];
            let mut code = None;
            while let Some(node_id) = stack.pop() {
                let Some(node) = doc.get_node(node_id) else {
                    continue;
                };
                if let Some(element) = node.element_data() {
                    if element.name.local == blitz_dom::local_name!("body") {
                        code = element
                            .attr(blitz_dom::local_name!("onload"))
                            .map(str::to_string);
                        break;
                    }
                }
                stack.extend(node.children.iter().rev().copied());
            }
            code
        };
        let Some(code) = code else { return };

        // Don't override a handler assigned via `window.onload = ...`
        let already_set = self
            .context
            .global_object()
            .get(js_string!("onload"), &mut self.context)
            .ok()
            .and_then(|value| value.as_object())
            .is_some_and(|obj| obj.is_callable());
        if already_set {
            return;
        }

        let script = format!("window.onload = function (event) {{\n{code}\n}};");
        self.eval_internal(&script, "<body onload attribute>");
    }

    /// HTML named element access: expose elements with an `id` attribute as
    /// properties of the global (`window`) object, without overriding existing
    /// globals. Should be called before evaluating scripts so that newly
    /// parsed/created ids are visible.
    pub fn sync_named_element_globals(&mut self) {
        use crate::js::property::{PropertyDescriptor, PropertyKey};

        let ids: Vec<(String, NodeId)> = {
            let doc = self.ctx.doc.borrow();
            let mut ids = Vec::new();
            let mut stack = vec![doc.root_node().id];
            while let Some(node_id) = stack.pop() {
                let Some(node) = doc.get_node(node_id) else {
                    continue;
                };
                if let Some(id) = node
                    .element_data()
                    .and_then(|element| element.attr(blitz_dom::local_name!("id")))
                {
                    if !id.is_empty() {
                        ids.push((id.to_string(), node_id));
                    }
                }
                stack.extend(node.children.iter().rev().copied());
            }
            ids
        };

        let global = self.context.global_object();
        for (id, node_id) in ids {
            let key = PropertyKey::from(JsString::from(id));
            let already_defined = global
                .has_own_property(key.clone(), &mut self.context)
                .unwrap_or(true);
            if already_defined {
                continue;
            }
            let wrapper = node_wrapper(&self.ctx, node_id, &mut self.context);
            let _ = global.define_property_or_throw(
                key,
                PropertyDescriptor::builder()
                    .value(wrapper)
                    .writable(true)
                    .enumerable(false)
                    .configurable(true)
                    .build(),
                &mut self.context,
            );
        }
    }

    /// Run all timers that are currently due. Returns `true` if any JavaScript was run.
    pub fn run_due_timers(&mut self) -> bool {
        let due = {
            let mut state = self.ctx.state.borrow_mut();
            let now = state.clock.now();
            state.timers.take_due(now)
        };
        if due.is_empty() {
            return false;
        }
        for timer in due {
            if let Err(error) =
                timer
                    .callback
                    .call(&JsValue::undefined(), &timer.args, &mut self.context)
            {
                report_js_error(&self.ctx, &mut self.context, "timer callback", &error);
            }
        }
        self.run_jobs("timer microtasks");
        true
    }

    /// Dispatch a Blitz DOM event to JavaScript event listeners registered on
    /// the nodes in `chain` (which is ordered target-first).
    ///
    /// Returns `true` if any listener was invoked.
    pub fn dispatch_dom_event(
        &mut self,
        chain: &[NodeId],
        event: &DomEvent,
        event_state: &mut EventState,
    ) -> bool {
        let name = event.name().to_string();
        let mut any_called = self.dispatch_event_inner(
            chain,
            &name,
            event.bubbles,
            |ctx, target, context| {
                create_event_for_dom_event(
                    ctx,
                    &event.data,
                    event.bubbles,
                    event.cancelable,
                    target,
                    context,
                )
            },
            event_state,
        );

        // Browsers fire a `change` event after `input` events on checkbox/radio
        // inputs. Blitz only generates `input` events, so synthesise the `change`
        // event here.
        if matches!(event.data, DomEventData::Input(_))
            && self.target_is_checkbox_or_radio(event.target)
        {
            let mut change_state = EventState::default();
            any_called |= self.dispatch_event_inner(
                chain,
                "change",
                true,
                |ctx, target, context| create_event(ctx, "change", true, false, target, context),
                &mut change_state,
            );
            if change_state.redraw_is_requested() {
                event_state.request_redraw();
            }
        }

        if any_called {
            self.run_jobs("event microtasks");
        }

        any_called
    }

    fn target_is_checkbox_or_radio(&self, node_id: NodeId) -> bool {
        let doc = self.ctx.doc.borrow();
        doc.get_node(node_id)
            .and_then(|node| node.element_data())
            .is_some_and(|element| {
                element.name.local == blitz_dom::local_name!("input")
                    && matches!(
                        element.attr(blitz_dom::local_name!("type")),
                        Some("checkbox") | Some("radio")
                    )
            })
    }

    /// Dispatch an event named `name` along `chain`, using `make_event` to lazily
    /// construct the JS event object. Returns `true` if any listener was invoked.
    fn dispatch_event_inner(
        &mut self,
        chain: &[NodeId],
        name: &str,
        bubbles: bool,
        make_event: impl FnOnce(&DomCtx, &JsValue, &mut Context) -> JsObject,
        event_state: &mut EventState,
    ) -> bool {
        let ctx = self.ctx.clone();
        let context = &mut self.context;
        let on_name = JsString::from(format!("on{name}"));

        // Fast path: bail if no listener of this type could possibly be registered
        let may_have_listeners = {
            let state = ctx.state.borrow();
            let registry_hit = chain.iter().any(|node_id| {
                state
                    .node_listeners
                    .get(node_id)
                    .and_then(|map| map.get(name))
                    .is_some_and(|listeners| !listeners.is_empty())
            }) || state
                .window_listeners
                .get(name)
                .is_some_and(|listeners| !listeners.is_empty());
            // `on<event>` handlers can only exist on nodes that script has touched
            // (i.e. nodes with a cached wrapper)
            let wrapper_hit = chain
                .iter()
                .any(|node_id| state.node_wrappers.contains_key(node_id));
            registry_hit || wrapper_hit
        };
        if !may_have_listeners {
            return false;
        }

        let target: JsValue = node_wrapper(&ctx, chain[0], context).into();
        let event_obj = make_event(&ctx, &target, context);
        let event_ref = |event_obj: &JsObject, f: &dyn Fn(&EventRef) -> bool| -> bool {
            event_obj
                .downcast_ref::<EventRef>()
                .map(|event| f(&event))
                .unwrap_or(false)
        };

        let mut any_called = false;

        // local change (raea, jetway #10): the DOM's three phases. Capture from the root down to the target's parent
        // (capture listeners), the target (all its listeners, in the order they were added), then bubbling back up
        // (the rest, and `on<event>` handlers) for events that bubble, each with its eventPhase. It used to run every
        // listener target-first in one pass with eventPhase 2: React's capture listeners on its root ran after the
        // target's.
        // `(node, phase)`: 1 capturing, 2 at the target, 3 bubbling
        let mut steps: Vec<(NodeId, u8)> = chain.iter().skip(1).rev().map(|&n| (n, 1)).collect();
        steps.push((chain[0], 2));
        if bubbles {
            steps.extend(chain.iter().skip(1).map(|&n| (n, 3)));
        }
        let mut last_node = None;
        for (node_id, phase) in steps {
            if last_node.is_some() && last_node != Some(node_id) && event_ref(&event_obj, &|event| event.stopped.get()) {
                break;
            }
            last_node = Some(node_id);
            // this node's listeners for this phase: `addEventListener` listeners plus, outside capture, an
            // `on<event>` property handler (if any); a `once` listener is removed as it's taken
            let mut callbacks: Vec<JsObject> = Vec::new();
            {
                let mut state = ctx.state.borrow_mut();
                if let Some(listeners) = state
                    .node_listeners
                    .get_mut(&node_id)
                    .and_then(|map| map.get_mut(name))
                {
                    let wanted = |l: &Listener| match phase {
                        1 => l.capture,
                        3 => !l.capture,
                        _ => true,
                    };
                    callbacks.extend(listeners.iter().filter(|l| wanted(l)).map(|l| l.callback.clone()));
                    listeners.retain(|l| !(l.once && wanted(l)));
                }
            }
            if phase != 1 {
                let wrapper = ctx.state.borrow().node_wrappers.get(&node_id).cloned();
                if let Some(wrapper) = wrapper {
                    if let Ok(handler) = wrapper.get(on_name.clone(), context) {
                        if let Some(handler) = handler.as_object() {
                            if handler.is_callable() {
                                callbacks.push(handler);
                            }
                        }
                    }
                }
            }
            if callbacks.is_empty() {
                continue;
            }

            let current_target: JsValue = node_wrapper(&ctx, node_id, context).into();
            crate::dom::define_value(&event_obj, "currentTarget", current_target.clone(), context);
            crate::dom::define_value(&event_obj, "eventPhase", JsValue::from(phase as i32), context);

            for callback in callbacks {
                any_called = true;
                if let Err(error) =
                    callback.call(&current_target, &[event_obj.clone().into()], context)
                {
                    report_js_error(&ctx, context, "event listener", &error);
                }
                if event_ref(&event_obj, &|event| event.stopped_immediate.get()) {
                    break;
                }
            }
            if event_ref(&event_obj, &|event| event.stopped_immediate.get()) {
                break;
            }
        }

        // Window-level listeners
        if bubbles && !event_ref(&event_obj, &|event| event.stopped.get()) {
            let listeners: Vec<Listener> = {
                let mut state = ctx.state.borrow_mut();
                match state.window_listeners.get_mut(name) {
                    Some(listeners) => {
                        let cloned = listeners.clone();
                        listeners.retain(|l| !l.once);
                        cloned
                    }
                    None => Vec::new(),
                }
            };
            if !listeners.is_empty() {
                let global: JsValue = context.global_object().into();
                crate::dom::define_value(&event_obj, "currentTarget", global.clone(), context);
                for listener in listeners {
                    any_called = true;
                    if let Err(error) =
                        listener
                            .callback
                            .call(&global, &[event_obj.clone().into()], context)
                    {
                        report_js_error(&ctx, context, "event listener", &error);
                    }
                    if event_ref(&event_obj, &|event| event.stopped_immediate.get()) {
                        break;
                    }
                }
            }
        }

        crate::dom::define_value(&event_obj, "currentTarget", JsValue::null(), context);

        // Feed `preventDefault` / `stopPropagation` back into Blitz
        if event_ref(&event_obj, &|event| event.prevented.get()) {
            event_state.prevent_default();
        }
        if event_ref(&event_obj, &|event| event.stopped.get()) {
            event_state.stop_propagation();
        }
        if any_called {
            event_state.request_redraw();
        }

        any_called
    }

    /// Dispatch a simple event (e.g. `DOMContentLoaded`) targeting the document node
    pub fn dispatch_document_event(&mut self, name: &str) -> bool {
        let root_id = self.ctx.doc.borrow().root_node().id;
        let mut event_state = EventState::default();
        let ran = self.dispatch_event_inner(
            &[root_id],
            name,
            true,
            |ctx, target, context| create_event(ctx, name, true, false, target, context),
            &mut event_state,
        );
        if ran {
            self.run_jobs("event microtasks");
        }
        ran
    }

    /// Dispatch a simple event (e.g. `load`) targeting the window
    pub fn dispatch_window_event(&mut self, name: &str) -> bool {
        let ctx = self.ctx.clone();
        let context = &mut self.context;

        let listeners: Vec<Listener> = {
            let mut state = ctx.state.borrow_mut();
            match state.window_listeners.get_mut(name) {
                Some(listeners) => {
                    let cloned = listeners.clone();
                    listeners.retain(|l| !l.once);
                    cloned
                }
                None => Vec::new(),
            }
        };

        let global: JsValue = context.global_object().into();
        let event_obj = create_event(&ctx, name, false, false, &global, context);
        crate::dom::define_value(&event_obj, "currentTarget", global.clone(), context);

        let mut any_called = false;
        for listener in listeners {
            any_called = true;
            if let Err(error) =
                listener
                    .callback
                    .call(&global, &[event_obj.clone().into()], context)
            {
                report_js_error(&ctx, context, "event listener", &error);
            }
        }

        // `window.onload = ...` style handler
        let on_name = JsString::from(format!("on{name}"));
        if let Ok(handler) = context.global_object().get(on_name, context) {
            if let Some(handler) = handler.as_object() {
                if handler.is_callable() {
                    any_called = true;
                    if let Err(error) = handler.call(&global, &[event_obj.into()], context) {
                        report_js_error(&ctx, context, "event listener", &error);
                    }
                }
            }
        }

        if any_called {
            self.run_jobs("event microtasks");
        }
        any_called
    }
}


/// raea: the string pairs of an import map's top-level `"imports"` object (`scopes` are ignored).
fn import_map_imports(json: &str) -> Vec<(String, String)> {
    let Some(at) = json.find("\"imports\"") else {
        return Vec::new();
    };
    let rest = &json[at + "\"imports\"".len()..];
    let Some(open) = rest.find('{') else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    let mut chars = rest[open + 1..].chars();
    while let Some(c) = chars.next() {
        if c == '}' {
            break;
        }
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(n) = chars.next() {
                        s.push(n);
                    }
                }
                '"' => break,
                _ => s.push(c),
            }
        }
        match pending.take() {
            None => pending = Some(s),
            Some(key) => out.push((key, s)),
        }
    }
    out
}

fn register_global(context: &mut Context, name: &str, value: JsValue) {
    context
        .register_global_property(
            JsString::from(name),
            value,
            Attribute::WRITABLE.union(Attribute::CONFIGURABLE),
        )
        .expect("failed to register global");
}

fn register_global_accessor(
    context: &mut Context,
    name: &str,
    getter: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
) {
    use crate::js::object::FunctionObjectBuilder;
    use crate::js::property::{PropertyDescriptor, PropertyKey};

    let getter_fn =
        FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(getter))
            .name(JsString::from(format!("get {name}")))
            .length(0)
            .build();
    context
        .global_object()
        .define_property_or_throw(
            PropertyKey::from(JsString::from(name)),
            PropertyDescriptor::builder()
                .get(getter_fn)
                .enumerable(false)
                .configurable(true)
                .build(),
            context,
        )
        .expect("failed to register global accessor");
}

fn register_global_fn(
    context: &mut Context,
    name: &str,
    length: usize,
    body: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
) {
    context
        .register_global_callable(
            JsString::from(name),
            length,
            NativeFunction::from_fn_ptr(body),
        )
        .expect("failed to register global function");
}

fn build_location(base_url: Option<&Url>, context: &mut Context) -> JsValue {
    let (href, protocol, host, pathname, search, hash, origin) = match base_url {
        Some(url) => (
            url.to_string(),
            format!("{}:", url.scheme()),
            url.host_str().unwrap_or_default().to_string(),
            url.path().to_string(),
            url.query().map(|q| format!("?{q}")).unwrap_or_default(),
            url.fragment().map(|f| format!("#{f}")).unwrap_or_default(),
            url.origin().ascii_serialization(),
        ),
        None => (
            "about:blank".to_string(),
            "about:".to_string(),
            String::new(),
            "blank".to_string(),
            String::new(),
            String::new(),
            "null".to_string(),
        ),
    };
    ObjectInitializer::new(context)
        .property(js_string!("href"), JsString::from(href), Attribute::all())
        .property(
            js_string!("protocol"),
            JsString::from(protocol),
            Attribute::all(),
        )
        .property(
            js_string!("host"),
            JsString::from(host.clone()),
            Attribute::all(),
        )
        .property(
            js_string!("hostname"),
            JsString::from(host),
            Attribute::all(),
        )
        .property(
            js_string!("pathname"),
            JsString::from(pathname),
            Attribute::all(),
        )
        .property(
            js_string!("search"),
            JsString::from(search),
            Attribute::all(),
        )
        .property(js_string!("hash"), JsString::from(hash), Attribute::all())
        .property(
            js_string!("origin"),
            JsString::from(origin),
            Attribute::all(),
        )
        .build()
        .into()
}

// === Timer + window listener native functions ===

fn timer_args(
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<Option<(JsObject, Duration, Vec<JsValue>)>> {
    let Some(callback) = args
        .first()
        .and_then(|value| value.as_object())
        .filter(|obj| obj.is_callable())
    else {
        return Ok(None);
    };
    let delay_ms = match args.get(1) {
        Some(value) => value.to_number(context)?,
        None => 0.0,
    };
    let delay_ms = if delay_ms.is_finite() && delay_ms > 0.0 {
        delay_ms
    } else {
        0.0
    };
    let rest: Vec<JsValue> = args.iter().skip(2).cloned().collect();
    Ok(Some((
        callback,
        Duration::from_secs_f64(delay_ms / 1000.0),
        rest,
    )))
}

fn set_timeout(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let Some((callback, delay, rest)) = timer_args(args, context)? else {
        return Ok(JsValue::from(0));
    };
    let mut state = ctx.state.borrow_mut();
    let now = state.clock.now();
    let id = state.timers.add(now, delay, None, callback, rest);
    Ok(JsValue::from(id as f64))
}

fn set_interval(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let Some((callback, delay, rest)) = timer_args(args, context)? else {
        return Ok(JsValue::from(0));
    };
    let mut state = ctx.state.borrow_mut();
    let now = state.clock.now();
    let id = state.timers.add(now, delay, Some(delay), callback, rest);
    Ok(JsValue::from(id as f64))
}

fn request_animation_frame(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let Some(callback) = args
        .first()
        .and_then(|value| value.as_object())
        .filter(|obj| obj.is_callable())
    else {
        return Ok(JsValue::from(0));
    };
    // Approximate the next frame as ~16ms away
    let timestamp = JsValue::from(16.0);
    let mut state = ctx.state.borrow_mut();
    let now = state.clock.now();
    let id = state.timers.add(
        now,
        Duration::from_millis(16),
        None,
        callback,
        vec![timestamp],
    );
    Ok(JsValue::from(id as f64))
}

// === Viewport dimensions ===

fn inner_width(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let doc = ctx.doc.borrow();
    let viewport = doc.viewport();
    Ok(JsValue::from(
        viewport.window_size.0 as f64 / viewport.scale() as f64,
    ))
}

fn inner_height(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let doc = ctx.doc.borrow();
    let viewport = doc.viewport();
    Ok(JsValue::from(
        viewport.window_size.1 as f64 / viewport.scale() as f64,
    ))
}

fn device_pixel_ratio(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let scale = ctx.doc.borrow().viewport().scale();
    Ok(JsValue::from(scale as f64))
}

// === Viewport scrolling ===

fn scroll_x(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    Ok(JsValue::from(ctx.doc.borrow().viewport_scroll().x))
}

fn scroll_y(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    Ok(JsValue::from(ctx.doc.borrow().viewport_scroll().y))
}

/// `window.scrollTo`/`window.scroll`: scroll the viewport, which blitz-dom
/// models as a programmatic scroll of the root element.
fn window_scroll_to(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let parsed = crate::dom::element::parse_scroll_to_args(args, context)?;
    let ctx = dom_ctx(context)?;
    let mut doc = ctx.doc.borrow_mut();
    doc.resolve_at_last();
    let Some(root_id) = doc.try_root_element().map(|root| root.id) else {
        return Ok(JsValue::undefined());
    };
    let current = doc.viewport_scroll();
    doc.scroll_to(
        root_id,
        parsed.left.unwrap_or(current.x),
        parsed.top.unwrap_or(current.y),
        parsed.behavior,
    );
    Ok(JsValue::undefined())
}

fn window_scroll_by(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let parsed = crate::dom::element::parse_scroll_to_args(args, context)?;
    let ctx = dom_ctx(context)?;
    let mut doc = ctx.doc.borrow_mut();
    doc.resolve_at_last();
    let Some(root_id) = doc.try_root_element().map(|root| root.id) else {
        return Ok(JsValue::undefined());
    };
    doc.scroll_by(
        root_id,
        parsed.left.unwrap_or(0.0),
        parsed.top.unwrap_or(0.0),
        parsed.behavior,
    );
    Ok(JsValue::undefined())
}

fn css_property_supported(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = to_rust_string(args.first().unwrap_or(&JsValue::undefined()), context)?;
    Ok(JsValue::from(blitz_dom::css_property_is_supported(&name)))
}

/// `__blitz_parse_transform(string)`: parse a CSS `<transform-list>` into a
/// 4x4 matrix. Returns `[elements, is2D]` (16 column-major elements), or
/// `null` if the string is not a valid transform list.
fn parse_transform(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let value = to_rust_string(args.first().unwrap_or(&JsValue::undefined()), context)?;
    let Some((elements, is_2d)) = blitz_dom::parse_transform_matrix(&value) else {
        return Ok(JsValue::null());
    };
    let elements = crate::js::object::builtins::JsArray::from_iter(
        elements.into_iter().map(JsValue::from),
        context,
    );
    Ok(crate::js::object::builtins::JsArray::from_iter(
        [elements.into(), JsValue::from(is_2d)],
        context,
    )
    .into())
}

/// `CSS.supports()`: the two-argument form checks a property/value declaration,
/// the one-argument form evaluates a `@supports` condition
fn css_supports(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let supported = if args.len() >= 2 {
        let property = to_rust_string(&args[0], context)?;
        let value = to_rust_string(&args[1], context)?;
        ctx.doc.borrow().css_declaration_is_valid(&property, &value)
    } else {
        let condition = to_rust_string(args.first().unwrap_or(&JsValue::undefined()), context)?;
        ctx.doc.borrow().css_supports_condition(&condition)
    };
    Ok(JsValue::from(supported))
}

/// `CSS.registerProperty({ name, syntax = "*", inherits, initialValue })`
/// <https://drafts.css-houdini.org/css-properties-values-api-1/#the-registerproperty-function>
fn css_register_property(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    use blitz_dom::RegisterCustomPropertyResult as Result_;

    let ctx = dom_ctx(context)?;
    let Some(descriptor) = args.first().and_then(JsValue::as_object) else {
        return Err(JsNativeError::typ()
            .with_message("CSS.registerProperty: argument must be a PropertyDefinition dictionary")
            .into());
    };
    let get = |key: &str, context: &mut Context| -> JsResult<Option<String>> {
        let value = descriptor.get(js_string!(key), context)?;
        if value.is_undefined() {
            return Ok(None);
        }
        Ok(Some(to_rust_string(&value, context)?))
    };
    let Some(name) = get("name", context)? else {
        return Err(JsNativeError::typ()
            .with_message("CSS.registerProperty: 'name' is required")
            .into());
    };
    let inherits = descriptor.get(js_string!("inherits"), context)?;
    if inherits.is_undefined() {
        return Err(JsNativeError::typ()
            .with_message("CSS.registerProperty: 'inherits' is required")
            .into());
    }
    let inherits = inherits.to_boolean();
    let syntax = get("syntax", context)?.unwrap_or_else(|| "*".to_string());
    let initial_value = get("initialValue", context)?;

    let result = ctx.doc.borrow_mut().register_custom_property(
        &name,
        &syntax,
        inherits,
        initial_value.as_deref(),
    );
    let error = match result {
        Result_::SuccessfullyRegistered => return Ok(JsValue::undefined()),
        Result_::InvalidName => JsNativeError::syntax().with_message(format!(
            "CSS.registerProperty: '{name}' is not a valid custom property name"
        )),
        Result_::AlreadyRegistered => JsNativeError::error().with_message(format!(
            "CSS.registerProperty: '{name}' is already registered"
        )),
        Result_::InvalidSyntax => JsNativeError::syntax()
            .with_message(format!("CSS.registerProperty: invalid syntax '{syntax}'")),
        Result_::NoInitialValue => JsNativeError::syntax().with_message(
            "CSS.registerProperty: 'initialValue' is required for non-universal syntax",
        ),
        Result_::InvalidInitialValue | Result_::InitialValueNotComputationallyIndependent => {
            JsNativeError::syntax().with_message("CSS.registerProperty: invalid 'initialValue'")
        }
    };
    Err(error.into())
}

fn get_computed_style(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let Some(node_id) = args.first().and_then(node_id_of_value) else {
        return Err(crate::js::JsNativeError::typ()
            .with_message("getComputedStyle: argument is not an Element")
            .into());
    };
    let proto = ctx.state.borrow().protos().computed_style.clone();
    let obj = JsObject::from_proto_and_data(Some(proto), NodeRef { node_id });
    Ok(wrap_style_object(obj, context))
}

fn streams_source(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(crate::dom::js_str(STREAMS_JS))
}

fn send_message(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let message = args
        .first()
        .unwrap_or(&JsValue::undefined())
        .to_string(context)?
        .to_std_string_lossy();
    ctx.state.borrow_mut().outbound_messages.push(message);
    Ok(JsValue::undefined())
}

/// `__blitz_native(name, payload)`: call the embedder's native binding `name` with the payload string
/// and return its string result (local change, raea).
fn call_native(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let name = args.first().unwrap_or(&JsValue::undefined()).to_string(context)?.to_std_string_lossy();
    let payload = match args.get(1) {
        Some(v) if !v.is_undefined() => v.to_string(context)?.to_std_string_lossy(),
        _ => String::new(),
    };
    let f = ctx.state.borrow().natives.get(&name).cloned();
    let f = f.ok_or_else(|| JsNativeError::typ().with_message(format!("no native binding {name}")))?;
    match f(&payload) {
        Ok(out) => Ok(JsString::from(out).into()),
        Err(message) => Err(JsNativeError::typ().with_message(message).into()),
    }
}

/// `__blitz_fetch_sync(url)`: resolve `url` against the document base URL and
/// fetch it via the document's [`ScriptFetcher`]. Returns `[status, url, text]`:
/// a missing resource yields a 404 (like an HTTP server would), any other
/// failure throws a `TypeError` (a network error, in `fetch()` terms).
fn fetch_sync(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    use crate::js::object::builtins::JsArray;

    let ctx = dom_ctx(context)?;
    let input = args
        .first()
        .unwrap_or(&JsValue::undefined())
        .to_string(context)?
        .to_std_string_lossy();

    let (base_url, fetcher) = {
        let state = ctx.state.borrow();
        (state.base_url.clone(), state.fetcher.clone())
    };
    let url = match &base_url {
        Some(base) => base.join(&input),
        None => Url::parse(&input),
    }
    .map_err(|_| JsNativeError::typ().with_message(format!("Failed to parse URL from {input}")))?;
    let fetcher =
        fetcher.ok_or_else(|| JsNativeError::typ().with_message("fetch is unavailable"))?;

    let (status, text) = match fetcher.borrow().fetch(&url) {
        Ok(text) => (200, text),
        Err(crate::fetch::FetchError::Io(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            (404, String::new())
        }
        Err(error) => {
            return Err(JsNativeError::typ()
                .with_message(format!("Failed to fetch {url}: {error}"))
                .into());
        }
    };

    Ok(JsArray::from_iter(
        [
            JsValue::from(status),
            JsString::from(url.as_str()).into(),
            JsString::from(text).into(),
        ],
        context,
    )
    .into())
}

fn clear_timer(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let id = match args.first() {
        Some(value) => value.to_number(context)?,
        None => return Ok(JsValue::undefined()),
    };
    if id.is_finite() && id >= 0.0 {
        ctx.state.borrow_mut().timers.remove(id as u64);
    }
    Ok(JsValue::undefined())
}

fn window_add_event_listener(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let event_type =
        crate::dom::to_rust_string(args.first().unwrap_or(&JsValue::undefined()), context)?;
    let Some(callback) = args
        .get(1)
        .and_then(|value| value.as_object())
        .filter(|obj| obj.is_callable())
    else {
        return Ok(JsValue::undefined());
    };

    let mut state = ctx.state.borrow_mut();
    let listeners = state.window_listeners.entry(event_type).or_default();
    if !listeners
        .iter()
        .any(|l| JsObject::equals(&l.callback, &callback))
    {
        listeners.push(Listener {
            callback,
            capture: false,
            once: false,
        });
    }
    Ok(JsValue::undefined())
}

fn window_remove_event_listener(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let ctx = dom_ctx(context)?;
    let event_type =
        crate::dom::to_rust_string(args.first().unwrap_or(&JsValue::undefined()), context)?;
    let Some(callback) = args.get(1).and_then(|value| value.as_object()) else {
        return Ok(JsValue::undefined());
    };

    let mut state = ctx.state.borrow_mut();
    if let Some(listeners) = state.window_listeners.get_mut(&event_type) {
        listeners.retain(|l| !JsObject::equals(&l.callback, &callback));
    }
    Ok(JsValue::undefined())
}

/// Whether JavaScriptCore runs with its JIT: where the process may map executable memory (macOS with the allow-jit
/// entitlement raea's executables carry), never on an iPhone device (engine/html/docs/js.feature).
pub fn jit_on() -> bool {
    !cfg!(all(target_os = "ios", not(target_abi = "sim")))
}
