#![allow(dead_code, private_interfaces)]
//! The Boa-shaped API (see js.rs) over JavaScriptCore, the system engine on macOS and iOS: the same vocabulary
//! (`JsValue`, `JsObject`, `Context`, `JsResult`, `JsNativeError`, `JsArray`, `ObjectInitializer`, ...) so the DOM
//! bindings run unchanged on either engine (the `jsc` feature picks this one).
//!
//! How it maps:
//! - Primitives live in Rust; objects (and symbols, bigints) are JSValueRefs kept alive with JSValueProtect while a
//!   Rust handle holds them, released when the last one goes.
//! - The active context is a thread-local, set by the runtime as it enters (and inside every native call).
//! - Native functions are objects of a callable class whose private data is the Rust function; their prototype is
//!   Function.prototype, so `.call`, `.apply` and `.bind` work on them as on any function.
//! - Native data (`JsObject::from_proto_and_data`) is the private data of an object of a data class, freed by the
//!   class's finalizer when JavaScriptCore collects the object.
//! - A failure comes back through JavaScriptCore's exception out-parameter.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::ffi::{CString, c_char, c_void};
use std::fmt;
use std::ptr::{null, null_mut};
use std::rc::Rc;

pub(crate) type Ctx = *const c_void;
pub(crate) type GCtx = *mut c_void;
type Obj = *mut c_void;
pub(crate) type Val = *const c_void;
type Str = *mut c_void;
pub(crate) type Class = *mut c_void;

#[repr(C)]
struct ClassDefinition {
    version: i32,
    attributes: u32,
    class_name: *const c_char,
    parent_class: Class,
    static_values: *const c_void,
    static_functions: *const c_void,
    initialize: Option<unsafe extern "C" fn(Ctx, Obj)>,
    finalize: Option<unsafe extern "C" fn(Obj)>,
    has_property: *const c_void,
    get_property: *const c_void,
    set_property: *const c_void,
    delete_property: *const c_void,
    get_property_names: *const c_void,
    call_as_function: Option<unsafe extern "C" fn(Ctx, Obj, Obj, usize, *const Val, *mut Val) -> Val>,
    call_as_constructor: *const c_void,
    has_instance: *const c_void,
    convert_to_type: *const c_void,
}

const TYPE_UNDEFINED: i32 = 0;
const TYPE_NULL: i32 = 1;
const TYPE_BOOLEAN: i32 = 2;
const TYPE_NUMBER: i32 = 3;
const TYPE_STRING: i32 = 4;
const TYPE_OBJECT: i32 = 5;

#[link(name = "JavaScriptCore", kind = "framework")]
unsafe extern "C" {
    fn JSClassCreate(def: *const ClassDefinition) -> Class;
    fn JSObjectMake(ctx: Ctx, class: Class, data: *mut c_void) -> Obj;
    fn JSObjectGetPrivate(obj: Obj) -> *mut c_void;
    fn JSValueIsObjectOfClass(ctx: Ctx, v: Val, class: Class) -> bool;
    fn JSContextGetGlobalObject(ctx: Ctx) -> Obj;
    fn JSStringCreateWithUTF8CString(s: *const c_char) -> Str;
    fn JSStringRelease(s: Str);
    fn JSStringGetMaximumUTF8CStringSize(s: Str) -> usize;
    fn JSStringGetUTF8CString(s: Str, buf: *mut c_char, size: usize) -> usize;
    fn JSEvaluateScript(ctx: Ctx, script: Str, this: Obj, url: Str, line: i32, exception: *mut Val) -> Val;
    fn JSObjectGetProperty(ctx: Ctx, obj: Obj, name: Str, exception: *mut Val) -> Val;
    fn JSObjectSetProperty(ctx: Ctx, obj: Obj, name: Str, value: Val, attrs: u32, exception: *mut Val);
    fn JSObjectGetPropertyAtIndex(ctx: Ctx, obj: Obj, index: u32, exception: *mut Val) -> Val;
    fn JSObjectSetPropertyAtIndex(ctx: Ctx, obj: Obj, index: u32, value: Val, exception: *mut Val);
    fn JSObjectHasProperty(ctx: Ctx, obj: Obj, name: Str) -> bool;
    fn JSObjectCallAsFunction(ctx: Ctx, f: Obj, this: Obj, argc: usize, argv: *const Val, exception: *mut Val) -> Val;
    fn JSObjectCallAsConstructor(ctx: Ctx, f: Obj, argc: usize, argv: *const Val, exception: *mut Val) -> Obj;
    fn JSObjectIsFunction(ctx: Ctx, obj: Obj) -> bool;
    fn JSObjectGetPrototype(ctx: Ctx, obj: Obj) -> Val;
    fn JSObjectSetPrototype(ctx: Ctx, obj: Obj, proto: Val);
    fn JSObjectMakeArray(ctx: Ctx, argc: usize, argv: *const Val, exception: *mut Val) -> Obj;
    fn JSValueGetType(ctx: Ctx, v: Val) -> i32;
    fn JSValueIsArray(ctx: Ctx, v: Val) -> bool;
    fn JSValueToBoolean(ctx: Ctx, v: Val) -> bool;
    fn JSValueToNumber(ctx: Ctx, v: Val, exception: *mut Val) -> f64;
    fn JSValueToStringCopy(ctx: Ctx, v: Val, exception: *mut Val) -> Str;
    fn JSValueToObject(ctx: Ctx, v: Val, exception: *mut Val) -> Obj;
    fn JSValueMakeUndefined(ctx: Ctx) -> Val;
    fn JSValueMakeNull(ctx: Ctx) -> Val;
    fn JSValueMakeBoolean(ctx: Ctx, b: bool) -> Val;
    fn JSValueMakeNumber(ctx: Ctx, n: f64) -> Val;
    fn JSValueMakeString(ctx: Ctx, s: Str) -> Val;
    fn JSValueIsStrictEqual(ctx: Ctx, a: Val, b: Val) -> bool;
    fn JSValueProtect(ctx: Ctx, v: Val);
    fn JSValueUnprotect(ctx: Ctx, v: Val);
}

// === The active context ===

thread_local! {
    static CTX: Cell<GCtx> = const { Cell::new(null_mut()) };
}

/// Makes `ctx` the active context until dropped (restoring the previous one).
pub(crate) struct CtxGuard(GCtx);

impl CtxGuard {
    pub(crate) fn enter(ctx: GCtx) -> Self {
        CtxGuard(CTX.with(|c| c.replace(ctx)))
    }
}

impl Drop for CtxGuard {
    fn drop(&mut self) {
        CTX.with(|c| c.set(self.0));
    }
}

/// The active context. Panics outside of an entry point.
pub(crate) fn ctx() -> Ctx {
    let c = CTX.with(|c| c.get());
    assert!(!c.is_null(), "blitz-v8-script (jsc): no active JavaScriptCore context");
    c as Ctx
}

fn js_str(s: &str) -> Str {
    let c = CString::new(s.replace('\0', "")).unwrap();
    unsafe { JSStringCreateWithUTF8CString(c.as_ptr()) }
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

fn val_to_string(c: Ctx, v: Val, exc: *mut Val) -> Option<String> {
    unsafe {
        let s = JSValueToStringCopy(c, v, exc);
        if s.is_null() {
            return None;
        }
        let out = rust_str(s);
        JSStringRelease(s);
        Some(out)
    }
}

/// A protected JSValueRef: alive while this is (JSValueProtect/Unprotect), unprotected through its own context (another
/// page's may be active when it goes), and not at all once that context is torn down (`drop_host`).
struct Protected(Val, GCtx);

impl Protected {
    fn new(v: Val) -> Protected {
        let c = ctx();
        unsafe { JSValueProtect(c, v) };
        Protected(v, c as GCtx)
    }
}

impl Drop for Protected {
    fn drop(&mut self) {
        if !DEAD.with(|d| d.borrow().contains(&(self.1 as usize))) {
            unsafe { JSValueUnprotect(self.1 as Ctx, self.0) };
        }
    }
}

thread_local! {
    /// contexts torn down: the values they protected went with them
    static DEAD: RefCell<std::collections::HashSet<usize>> = RefCell::new(std::collections::HashSet::new());
}

/// A context made (its address may be a torn-down one's).
pub(crate) fn new_host(g: GCtx) {
    DEAD.with(|d| d.borrow_mut().remove(&(g as usize)));
}

/// A context about to be torn down: its host data goes, and handles to its values stop unprotecting them.
pub(crate) fn drop_host(g: GCtx) {
    DEAD.with(|d| d.borrow_mut().insert(g as usize));
    let data = HOST.with(|h| h.borrow_mut().remove(&(g as usize)));
    drop(data);
}

// === Strings ===

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct JsString(Rc<str>);

impl JsString {
    pub fn to_std_string_lossy(&self) -> String {
        self.0.to_string()
    }
    pub fn to_std_string_escaped(&self) -> String {
        self.0.to_string()
    }
    pub fn to_std_string(&self) -> Result<String, ()> {
        Ok(self.0.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&str> for JsString {
    fn from(s: &str) -> Self {
        JsString(Rc::from(s))
    }
}
impl From<String> for JsString {
    fn from(s: String) -> Self {
        JsString(Rc::from(s))
    }
}
impl From<&String> for JsString {
    fn from(s: &String) -> Self {
        JsString(Rc::from(s.as_str()))
    }
}
impl fmt::Display for JsString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[macro_export]
macro_rules! js_string {
    () => {
        $crate::js::JsString::from("")
    };
    ($s:expr) => {
        $crate::js::JsString::from($s)
    };
}
pub use crate::js_string;

// === Values ===

#[derive(Clone, Default)]
pub enum JsValue {
    #[default]
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(JsString),
    Object(JsObject),
    /// Symbols and BigInts, carried through untouched
    Other(Rc<Protected>),
}

impl JsValue {
    pub fn undefined() -> Self {
        JsValue::Undefined
    }
    pub fn null() -> Self {
        JsValue::Null
    }
    pub fn is_undefined(&self) -> bool {
        matches!(self, JsValue::Undefined)
    }
    pub fn is_null(&self) -> bool {
        matches!(self, JsValue::Null)
    }
    pub fn is_null_or_undefined(&self) -> bool {
        matches!(self, JsValue::Undefined | JsValue::Null)
    }
    pub fn is_object(&self) -> bool {
        matches!(self, JsValue::Object(_))
    }
    pub fn is_string(&self) -> bool {
        matches!(self, JsValue::String(_))
    }
    pub fn is_number(&self) -> bool {
        matches!(self, JsValue::Number(_))
    }
    pub fn is_boolean(&self) -> bool {
        matches!(self, JsValue::Boolean(_))
    }
    pub fn is_callable(&self) -> bool {
        matches!(self, JsValue::Object(o) if o.is_callable())
    }
    pub fn as_object(&self) -> Option<JsObject> {
        match self {
            JsValue::Object(o) => Some(o.clone()),
            _ => None,
        }
    }
    pub fn as_string(&self) -> Option<&JsString> {
        match self {
            JsValue::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_number(&self) -> Option<f64> {
        match self {
            JsValue::Number(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            JsValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }
    pub fn to_boolean(&self) -> bool {
        match self {
            JsValue::Undefined | JsValue::Null => false,
            JsValue::Boolean(b) => *b,
            JsValue::Number(n) => !(*n == 0.0 || n.is_nan()),
            JsValue::String(s) => !s.is_empty(),
            JsValue::Object(_) => true,
            JsValue::Other(p) => unsafe { JSValueToBoolean(ctx(), p.0) },
        }
    }

    /// ECMAScript ToNumber
    pub fn to_number(&self, _: &mut Context) -> JsResult<f64> {
        match self {
            JsValue::Number(n) => Ok(*n),
            JsValue::Boolean(b) => Ok(if *b { 1.0 } else { 0.0 }),
            JsValue::Undefined => Ok(f64::NAN),
            JsValue::Null => Ok(0.0),
            _ => with_exception(|c, exc| {
                let v = self.to_jsc(c);
                Some(unsafe { JSValueToNumber(c, v, exc) })
            }),
        }
    }

    /// ECMAScript ToString
    pub fn to_string(&self, _: &mut Context) -> JsResult<JsString> {
        match self {
            JsValue::String(s) => Ok(s.clone()),
            JsValue::Undefined => Ok(JsString::from("undefined")),
            JsValue::Null => Ok(JsString::from("null")),
            JsValue::Boolean(b) => Ok(JsString::from(if *b { "true" } else { "false" })),
            _ => with_exception(|c, exc| {
                let v = self.to_jsc(c);
                val_to_string(c, v, exc).map(JsString::from)
            }),
        }
    }

    pub fn to_u32(&self, context: &mut Context) -> JsResult<u32> {
        let n = self.to_number(context)?;
        if !n.is_finite() {
            return Ok(0);
        }
        Ok((n.trunc() as i64 as u64 & 0xffff_ffff) as u32)
    }

    pub fn to_i32(&self, context: &mut Context) -> JsResult<i32> {
        Ok(self.to_u32(context)? as i32)
    }

    pub(crate) fn from_jsc(c: Ctx, v: Val) -> JsValue {
        if v.is_null() {
            return JsValue::Undefined;
        }
        unsafe {
            match JSValueGetType(c, v) {
                TYPE_UNDEFINED => JsValue::Undefined,
                TYPE_NULL => JsValue::Null,
                TYPE_BOOLEAN => JsValue::Boolean(JSValueToBoolean(c, v)),
                TYPE_NUMBER => JsValue::Number(JSValueToNumber(c, v, null_mut())),
                TYPE_STRING => JsValue::String(JsString::from(val_to_string(c, v, null_mut()).unwrap_or_default())),
                TYPE_OBJECT => JsValue::Object(JsObject::from_val(c, v)),
                _ => JsValue::Other(Rc::new(Protected::new(v))),
            }
        }
    }

    pub(crate) fn to_jsc(&self, c: Ctx) -> Val {
        unsafe {
            match self {
                JsValue::Undefined => JSValueMakeUndefined(c),
                JsValue::Null => JSValueMakeNull(c),
                JsValue::Boolean(b) => JSValueMakeBoolean(c, *b),
                JsValue::Number(n) => JSValueMakeNumber(c, *n),
                JsValue::String(s) => {
                    let j = js_str(s.as_str());
                    let v = JSValueMakeString(c, j);
                    JSStringRelease(j);
                    v
                }
                JsValue::Object(o) => o.0.v.0,
                JsValue::Other(p) => p.0,
            }
        }
    }
}

impl fmt::Debug for JsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsValue::Undefined => write!(f, "undefined"),
            JsValue::Null => write!(f, "null"),
            JsValue::Boolean(b) => write!(f, "{b}"),
            JsValue::Number(n) => write!(f, "{n}"),
            JsValue::String(s) => write!(f, "{:?}", s.as_str()),
            JsValue::Object(_) => write!(f, "[object]"),
            JsValue::Other(_) => write!(f, "[symbol/bigint]"),
        }
    }
}

macro_rules! from_number {
    ($($t:ty),*) => {$(
        impl From<$t> for JsValue {
            fn from(n: $t) -> Self { JsValue::Number(n as f64) }
        }
    )*};
}
from_number!(f64, f32, i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);

impl From<bool> for JsValue {
    fn from(b: bool) -> Self {
        JsValue::Boolean(b)
    }
}
impl From<JsString> for JsValue {
    fn from(s: JsString) -> Self {
        JsValue::String(s)
    }
}
impl From<JsObject> for JsValue {
    fn from(o: JsObject) -> Self {
        JsValue::Object(o)
    }
}
impl From<&str> for JsValue {
    fn from(s: &str) -> Self {
        JsValue::String(JsString::from(s))
    }
}
impl From<String> for JsValue {
    fn from(s: String) -> Self {
        JsValue::String(JsString::from(s))
    }
}
impl From<()> for JsValue {
    fn from(_: ()) -> Self {
        JsValue::Undefined
    }
}

// === Objects ===

struct ObjInner {
    v: Protected,
    callable: bool,
}

#[derive(Clone)]
pub struct JsObject(Rc<ObjInner>);

pub type JsFunction = JsObject;

/// A global's value (Object, Function.prototype, TypeError, ...) looked up on the active context's global object.
fn global_value(c: Ctx, name: &str) -> Val {
    unsafe {
        let n = js_str(name);
        let v = JSObjectGetProperty(c, JSContextGetGlobalObject(c), n, null_mut());
        JSStringRelease(n);
        v
    }
}

fn property(c: Ctx, obj: Val, name: &str) -> Val {
    unsafe {
        let n = js_str(name);
        let v = JSObjectGetProperty(c, obj as Obj, n, null_mut());
        JSStringRelease(n);
        v
    }
}

impl JsObject {
    pub(crate) fn from_val(c: Ctx, v: Val) -> JsObject {
        let callable = unsafe { JSObjectIsFunction(c, v as Obj) };
        JsObject(Rc::new(ObjInner { v: Protected::new(v), callable }))
    }

    pub(crate) fn raw(&self) -> Obj {
        self.0.v.0 as Obj
    }

    pub fn is_callable(&self) -> bool {
        self.0.callable
    }
    pub fn is_constructor(&self) -> bool {
        self.0.callable
    }

    pub fn equals(a: &JsObject, b: &JsObject) -> bool {
        Rc::ptr_eq(&a.0, &b.0) || unsafe { JSValueIsStrictEqual(ctx(), a.0.v.0, b.0.v.0) }
    }

    /// A new plain object (`Object.prototype`)
    pub fn with_object_proto(_: Intrinsics) -> JsObject {
        let c = ctx();
        let o = unsafe { JSObjectMake(c, null_mut(), null_mut()) };
        JsObject::from_val(c, o as Val)
    }

    /// A new object with `proto` and native `data` (read back with [`downcast_ref`](Self::downcast_ref)).
    pub fn from_proto_and_data<T: Any>(proto: Option<JsObject>, data: T) -> JsObject {
        let c = ctx();
        let boxed: Box<Rc<dyn Any>> = Box::new(Rc::new(data));
        let o = unsafe { JSObjectMake(c, data_class(), Box::into_raw(boxed) as *mut c_void) };
        if let Some(p) = proto {
            unsafe { JSObjectSetPrototype(c, o, p.0.v.0) };
        }
        JsObject::from_val(c, o as Val)
    }

    pub fn downcast_ref<T: Any>(&self) -> Option<Rc<T>> {
        let c = ctx();
        unsafe {
            if !JSValueIsObjectOfClass(c, self.0.v.0, data_class()) {
                return None;
            }
            let p = JSObjectGetPrivate(self.raw()) as *const Rc<dyn Any>;
            if p.is_null() {
                return None;
            }
            (*p).clone().downcast::<T>().ok()
        }
    }

    pub fn get<K: Into<PropertyKey>>(&self, key: K, _: &mut Context) -> JsResult<JsValue> {
        let key = key.into();
        with_exception(|c, exc| {
            let v = unsafe {
                match &key {
                    PropertyKey::Index(i) => JSObjectGetPropertyAtIndex(c, self.raw(), *i, exc),
                    PropertyKey::String(s) => {
                        let n = js_str(s.as_str());
                        let v = JSObjectGetProperty(c, self.raw(), n, exc);
                        JSStringRelease(n);
                        v
                    }
                }
            };
            Some(JsValue::from_jsc(c, v))
        })
    }

    pub fn set<K: Into<PropertyKey>, V: Into<JsValue>>(&self, key: K, value: V, _throw: bool, _: &mut Context) -> JsResult<bool> {
        let (key, value) = (key.into(), value.into());
        with_exception(|c, exc| {
            let v = value.to_jsc(c);
            unsafe {
                match &key {
                    PropertyKey::Index(i) => JSObjectSetPropertyAtIndex(c, self.raw(), *i, v, exc),
                    PropertyKey::String(s) => {
                        let n = js_str(s.as_str());
                        JSObjectSetProperty(c, self.raw(), n, v, 0, exc);
                        JSStringRelease(n);
                    }
                }
            }
            Some(true)
        })
    }

    pub fn has_own_property<K: Into<PropertyKey>>(&self, key: K, _: &mut Context) -> JsResult<bool> {
        let key = key.into();
        with_exception(|c, exc| {
            // Object.prototype.hasOwnProperty.call(obj, key)
            let has = property(c, property(c, global_value(c, "Object"), "prototype"), "hasOwnProperty");
            let k = key.to_value().to_jsc(c);
            let r = unsafe { JSObjectCallAsFunction(c, has as Obj, self.raw(), 1, &k, exc) };
            Some(unsafe { JSValueToBoolean(c, r) })
        })
    }

    pub fn has_property<K: Into<PropertyKey>>(&self, key: K, _: &mut Context) -> JsResult<bool> {
        let key = key.into();
        let c = ctx();
        let n = js_str(&key.to_value().to_string(&mut Context::current())?.to_std_string_lossy());
        let has = unsafe { JSObjectHasProperty(c, self.raw(), n) };
        unsafe { JSStringRelease(n) };
        Ok(has)
    }

    pub fn define_property_or_throw<K: Into<PropertyKey>>(&self, key: K, desc: PropertyDescriptor, _: &mut Context) -> JsResult<bool> {
        let key = key.into();
        with_exception(|c, exc| {
            // Object.defineProperty(obj, key, {...}): the C API has no accessor properties of its own
            let d = unsafe { JSObjectMake(c, null_mut(), null_mut()) };
            let set = |name: &str, v: Val| unsafe {
                let n = js_str(name);
                JSObjectSetProperty(c, d, n, v, 0, null_mut());
                JSStringRelease(n);
            };
            let b = |x: bool| unsafe { JSValueMakeBoolean(c, x) };
            match (&desc.get, &desc.set) {
                (None, None) => {
                    set("value", desc.value.clone().unwrap_or_default().to_jsc(c));
                    set("writable", b(desc.writable.unwrap_or(false)));
                }
                (g, s) => {
                    if let Some(g) = g {
                        set("get", g.0.v.0);
                    }
                    if let Some(s) = s {
                        set("set", s.0.v.0);
                    }
                }
            }
            set("enumerable", b(desc.enumerable.unwrap_or(false)));
            set("configurable", b(desc.configurable.unwrap_or(false)));
            let define = property(c, global_value(c, "Object"), "defineProperty");
            let args = [self.0.v.0, key.to_value().to_jsc(c), d as Val];
            unsafe { JSObjectCallAsFunction(c, define as Obj, null_mut(), 3, args.as_ptr(), exc) };
            Some(true)
        })
    }

    pub fn call(&self, this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        with_exception(|c, exc| {
            let this = match this {
                JsValue::Object(o) => o.raw(),
                JsValue::Undefined | JsValue::Null => null_mut(),
                other => unsafe { JSValueToObject(c, other.to_jsc(c), null_mut()) },
            };
            let argv: Vec<Val> = args.iter().map(|a| a.to_jsc(c)).collect();
            let v = unsafe { JSObjectCallAsFunction(c, self.raw(), this, argv.len(), argv.as_ptr(), exc) };
            if v.is_null() { None } else { Some(JsValue::from_jsc(c, v)) }
        })
    }

    pub fn construct(&self, args: &[JsValue], _new_target: Option<&JsObject>, _: &mut Context) -> JsResult<JsObject> {
        with_exception(|c, exc| {
            let argv: Vec<Val> = args.iter().map(|a| a.to_jsc(c)).collect();
            let o = unsafe { JSObjectCallAsConstructor(c, self.raw(), argv.len(), argv.as_ptr(), exc) };
            if o.is_null() { None } else { Some(JsObject::from_val(c, o as Val)) }
        })
    }

    pub fn set_prototype(&self, proto: Option<JsObject>) -> bool {
        let c = ctx();
        let p = match proto {
            Some(p) => p.0.v.0,
            None => unsafe { JSValueMakeNull(c) },
        };
        unsafe { JSObjectSetPrototype(c, self.raw(), p) };
        true
    }

    pub fn prototype(&self) -> Option<JsObject> {
        let c = ctx();
        let p = unsafe { JSObjectGetPrototype(c, self.raw()) };
        match JsValue::from_jsc(c, p) {
            JsValue::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Does this object carry native data of type `T`?
    pub fn is<T: Any>(&self) -> bool {
        self.downcast_ref::<T>().is_some()
    }

    pub fn create_data_property_or_throw<K: Into<PropertyKey>, V: Into<JsValue>>(&self, key: K, value: V, context: &mut Context) -> JsResult<bool> {
        self.define_property_or_throw(key, descriptor_for(value.into(), Attribute::all()), context)
    }

    pub fn is_array(&self) -> bool {
        unsafe { JSValueIsArray(ctx(), self.0.v.0) }
    }
}

impl PartialEq for JsObject {
    fn eq(&self, other: &Self) -> bool {
        JsObject::equals(self, other)
    }
}

/// The class of objects carrying native data: its private data a leaked `Box<Rc<dyn Any>>`, freed when the object is.
fn data_class() -> Class {
    unsafe extern "C" fn finalize(o: Obj) {
        let p = unsafe { JSObjectGetPrivate(o) } as *mut Rc<dyn Any>;
        if !p.is_null() {
            drop(unsafe { Box::from_raw(p) });
        }
    }
    thread_local! {
        static CLASS: Class = {
            let def = ClassDefinition { version: 0, attributes: 0, class_name: c"RaeaData".as_ptr(), parent_class: null_mut(), static_values: null(), static_functions: null(), initialize: None, finalize: Some(finalize), has_property: null(), get_property: null(), set_property: null(), delete_property: null(), get_property_names: null(), call_as_function: None, call_as_constructor: null(), has_instance: null(), convert_to_type: null() };
            unsafe { JSClassCreate(&def) }
        };
    }
    CLASS.with(|c| *c)
}

/// Run `f` with the active context and an exception slot: a thrown value becomes `Err(JsError::Opaque(value))`.
pub(crate) fn with_exception<T>(f: impl FnOnce(Ctx, *mut Val) -> Option<T>) -> JsResult<T> {
    let c = ctx();
    let mut exc: Val = null();
    let r = f(c, &mut exc);
    if !exc.is_null() {
        return Err(JsError::Opaque(JsValue::from_jsc(c, exc)));
    }
    r.ok_or_else(|| JsNativeError::typ().with_message("not a callable function").into())
}

// === Property keys and descriptors ===

#[derive(Clone, Debug)]
pub enum PropertyKey {
    String(JsString),
    Index(u32),
}

impl PropertyKey {
    fn to_value(&self) -> JsValue {
        match self {
            PropertyKey::String(s) => JsValue::String(s.clone()),
            PropertyKey::Index(i) => JsValue::Number(*i as f64),
        }
    }
}

impl From<JsString> for PropertyKey {
    fn from(s: JsString) -> Self {
        PropertyKey::String(s)
    }
}
impl From<&JsString> for PropertyKey {
    fn from(s: &JsString) -> Self {
        PropertyKey::String(s.clone())
    }
}
impl From<&str> for PropertyKey {
    fn from(s: &str) -> Self {
        PropertyKey::String(JsString::from(s))
    }
}
impl From<String> for PropertyKey {
    fn from(s: String) -> Self {
        PropertyKey::String(JsString::from(s))
    }
}
impl From<u32> for PropertyKey {
    fn from(i: u32) -> Self {
        PropertyKey::Index(i)
    }
}
impl From<usize> for PropertyKey {
    fn from(i: usize) -> Self {
        PropertyKey::Index(i as u32)
    }
}
impl From<u64> for PropertyKey {
    fn from(i: u64) -> Self {
        PropertyKey::Index(i as u32)
    }
}
impl From<i32> for PropertyKey {
    fn from(i: i32) -> Self {
        PropertyKey::Index(i as u32)
    }
}

#[derive(Clone, Default)]
pub struct PropertyDescriptor {
    value: Option<JsValue>,
    get: Option<JsObject>,
    set: Option<JsObject>,
    writable: Option<bool>,
    enumerable: Option<bool>,
    configurable: Option<bool>,
}

impl PropertyDescriptor {
    pub fn builder() -> PropertyDescriptorBuilder {
        PropertyDescriptorBuilder(PropertyDescriptor::default())
    }
}

pub struct PropertyDescriptorBuilder(PropertyDescriptor);

impl PropertyDescriptorBuilder {
    pub fn value<V: Into<JsValue>>(mut self, v: V) -> Self {
        self.0.value = Some(v.into());
        self
    }
    pub fn get(mut self, f: JsObject) -> Self {
        self.0.get = Some(f);
        self
    }
    pub fn set(mut self, f: JsObject) -> Self {
        self.0.set = Some(f);
        self
    }
    pub fn writable(mut self, b: bool) -> Self {
        self.0.writable = Some(b);
        self
    }
    pub fn enumerable(mut self, b: bool) -> Self {
        self.0.enumerable = Some(b);
        self
    }
    pub fn configurable(mut self, b: bool) -> Self {
        self.0.configurable = Some(b);
        self
    }
    pub fn build(self) -> PropertyDescriptor {
        self.0
    }
}

/// Property attribute flags (`Attribute::all()`, `WRITABLE | CONFIGURABLE`, ...)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attribute(u8);

impl Attribute {
    pub const WRITABLE: Attribute = Attribute(1);
    pub const ENUMERABLE: Attribute = Attribute(2);
    pub const CONFIGURABLE: Attribute = Attribute(4);
    pub const READONLY: Attribute = Attribute(0);
    pub fn all() -> Self {
        Attribute(7)
    }
    pub fn empty() -> Self {
        Attribute(0)
    }
    pub fn union(self, other: Attribute) -> Self {
        Attribute(self.0 | other.0)
    }
    pub fn contains(self, other: Attribute) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Attribute {
    type Output = Attribute;
    fn bitor(self, other: Attribute) -> Attribute {
        self.union(other)
    }
}

fn descriptor_for(value: JsValue, attr: Attribute) -> PropertyDescriptor {
    PropertyDescriptor::builder()
        .value(value)
        .writable(attr.contains(Attribute::WRITABLE))
        .enumerable(attr.contains(Attribute::ENUMERABLE))
        .configurable(attr.contains(Attribute::CONFIGURABLE))
        .build()
}

// === Errors ===

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Error,
    Type,
    Syntax,
    Range,
    Reference,
}

#[derive(Clone, Debug)]
pub struct JsNativeError {
    kind: ErrorKind,
    message: String,
}

impl JsNativeError {
    fn of(kind: ErrorKind) -> Self {
        JsNativeError { kind, message: String::new() }
    }
    pub fn typ() -> Self {
        Self::of(ErrorKind::Type)
    }
    pub fn syntax() -> Self {
        Self::of(ErrorKind::Syntax)
    }
    pub fn error() -> Self {
        Self::of(ErrorKind::Error)
    }
    pub fn range() -> Self {
        Self::of(ErrorKind::Range)
    }
    pub fn reference() -> Self {
        Self::of(ErrorKind::Reference)
    }
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    fn to_jsc(&self, c: Ctx) -> Val {
        let name = match self.kind {
            ErrorKind::Error => "Error",
            ErrorKind::Type => "TypeError",
            ErrorKind::Syntax => "SyntaxError",
            ErrorKind::Range => "RangeError",
            ErrorKind::Reference => "ReferenceError",
        };
        let ctor = global_value(c, name);
        let msg = JsValue::from(self.message.as_str()).to_jsc(c);
        unsafe { JSObjectCallAsConstructor(c, ctor as Obj, 1, &msg, null_mut()) as Val }
    }
}

impl fmt::Display for JsNativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.kind {
            ErrorKind::Error => "Error",
            ErrorKind::Type => "TypeError",
            ErrorKind::Syntax => "SyntaxError",
            ErrorKind::Range => "RangeError",
            ErrorKind::Reference => "ReferenceError",
        };
        write!(f, "{name}: {}", self.message)
    }
}

#[derive(Clone, Debug)]
pub enum JsError {
    Native(JsNativeError),
    Opaque(JsValue),
}

impl JsError {
    pub fn from_opaque(value: JsValue) -> Self {
        JsError::Opaque(value)
    }

    /// The thrown JS value (native errors become real Error objects)
    pub fn into_opaque(self, _: &mut Context) -> JsResult<JsValue> {
        match self {
            JsError::Opaque(v) => Ok(v),
            JsError::Native(e) => {
                let c = ctx();
                Ok(JsValue::from_jsc(c, e.to_jsc(c)))
            }
        }
    }

    pub(crate) fn to_jsc(&self, c: Ctx) -> Val {
        match self {
            JsError::Opaque(v) => v.to_jsc(c),
            JsError::Native(e) => e.to_jsc(c),
        }
    }
}

impl From<JsNativeError> for JsError {
    fn from(e: JsNativeError) -> Self {
        JsError::Native(e)
    }
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsError::Native(e) => write!(f, "{e}"),
            JsError::Opaque(JsValue::String(s)) => write!(f, "{s}"),
            JsError::Opaque(JsValue::Object(o)) => {
                let c = ctx();
                write!(f, "{}", val_to_string(c, o.0.v.0, null_mut()).unwrap_or_else(|| "[object]".into()))
            }
            JsError::Opaque(v) => write!(f, "{v:?}"),
        }
    }
}

impl std::error::Error for JsError {}

pub type JsResult<T> = Result<T, JsError>;

// === Native functions ===

pub type NativeFnPtr = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;
type NativeClosure = Rc<dyn Fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>>;

#[derive(Clone)]
pub enum NativeFunction {
    Ptr(NativeFnPtr),
    Closure(NativeClosure),
}

impl NativeFunction {
    pub fn from_fn_ptr(f: NativeFnPtr) -> Self {
        NativeFunction::Ptr(f)
    }
    pub fn from_copy_closure<F>(f: F) -> Self
    where
        F: Fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue> + Copy + 'static,
    {
        NativeFunction::Closure(Rc::new(f))
    }
    pub fn from_closure<F>(f: F) -> Self
    where
        F: Fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue> + 'static,
    {
        NativeFunction::Closure(Rc::new(f))
    }

    fn call(&self, this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        match self {
            NativeFunction::Ptr(f) => f(this, args, context),
            NativeFunction::Closure(f) => f(this, args, context),
        }
    }

    /// A JS function calling this native: an object of the callable class whose private data is the native (freed with
    /// the function), with Function.prototype for `.call`/`.apply`/`.bind`, and its name and length set.
    pub(crate) fn to_function(self, name: &str, length: usize) -> JsObject {
        let c = ctx();
        let boxed = Box::into_raw(Box::new(self)) as *mut c_void;
        let f = unsafe { JSObjectMake(c, function_class(), boxed) };
        let proto = property(c, global_value(c, "Function"), "prototype");
        unsafe { JSObjectSetPrototype(c, f, proto) };
        let obj = JsObject::from_val(c, f as Val);
        let mut cx = Context::current();
        let fixed = |v: JsValue| PropertyDescriptor::builder().value(v).writable(false).enumerable(false).configurable(true).build();
        let _ = obj.define_property_or_throw("name", fixed(JsValue::from(name)), &mut cx);
        let _ = obj.define_property_or_throw("length", fixed(JsValue::from(length)), &mut cx);
        obj
    }
}

fn function_class() -> Class {
    unsafe extern "C" fn finalize(o: Obj) {
        let p = unsafe { JSObjectGetPrivate(o) } as *mut NativeFunction;
        if !p.is_null() {
            drop(unsafe { Box::from_raw(p) });
        }
    }
    thread_local! {
        static CLASS: Class = {
            let def = ClassDefinition { version: 0, attributes: 0, class_name: c"Function".as_ptr(), parent_class: null_mut(), static_values: null(), static_functions: null(), initialize: None, finalize: Some(finalize), has_property: null(), get_property: null(), set_property: null(), delete_property: null(), get_property_names: null(), call_as_function: Some(trampoline), call_as_constructor: null(), has_instance: null(), convert_to_type: null() };
            unsafe { JSClassCreate(&def) }
        };
    }
    CLASS.with(|c| *c)
}

unsafe extern "C" fn trampoline(c: Ctx, f: Obj, this: Obj, argc: usize, argv: *const Val, exception: *mut Val) -> Val {
    let native = unsafe { &*(JSObjectGetPrivate(f) as *const NativeFunction) };
    let _guard = CtxGuard::enter(c as GCtx);
    let this = if this.is_null() { JsValue::Undefined } else { JsValue::from_jsc(c, this as Val) };
    let args: Vec<JsValue> = (0..argc).map(|i| JsValue::from_jsc(c, unsafe { *argv.add(i) })).collect();
    match native.call(&this, &args, &mut Context::current()) {
        Ok(v) => v.to_jsc(c),
        Err(e) => {
            if !exception.is_null() {
                unsafe { *exception = e.to_jsc(c) };
            }
            unsafe { JSValueMakeUndefined(c) }
        }
    }
}

pub struct Realm;

pub struct FunctionObjectBuilder {
    native: NativeFunction,
    name: String,
    length: usize,
}

impl FunctionObjectBuilder {
    pub fn new(_: Realm, native: NativeFunction) -> Self {
        FunctionObjectBuilder { native, name: String::new(), length: 0 }
    }
    pub fn name(mut self, name: JsString) -> Self {
        self.name = name.to_std_string_lossy();
        self
    }
    pub fn length(mut self, length: usize) -> Self {
        self.length = length;
        self
    }
    pub fn build(self) -> JsObject {
        self.native.to_function(&self.name, self.length)
    }
}

pub struct ObjectInitializer {
    obj: JsObject,
}

impl ObjectInitializer {
    pub fn new(_: &mut Context) -> Self {
        ObjectInitializer { obj: JsObject::with_object_proto(Intrinsics) }
    }
    pub fn property<K: Into<PropertyKey>, V: Into<JsValue>>(&mut self, key: K, value: V, attr: Attribute) -> &mut Self {
        let _ = self.obj.define_property_or_throw(key, descriptor_for(value.into(), attr), &mut Context::current());
        self
    }
    pub fn function(&mut self, native: NativeFunction, name: JsString, length: usize) -> &mut Self {
        let f = native.to_function(name.as_str(), length);
        let _ = self.obj.define_property_or_throw(name, PropertyDescriptor::builder().value(f).writable(true).enumerable(false).configurable(true).build(), &mut Context::current());
        self
    }
    pub fn build(&mut self) -> JsObject {
        self.obj.clone()
    }
}

// === Arrays ===

#[derive(Clone)]
pub struct JsArray(JsObject);

impl JsArray {
    pub fn from_iter<I: IntoIterator<Item = JsValue>>(items: I, _: &mut Context) -> JsArray {
        let c = ctx();
        let items: Vec<Val> = items.into_iter().map(|v| v.to_jsc(c)).collect();
        let a = unsafe { JSObjectMakeArray(c, items.len(), items.as_ptr(), null_mut()) };
        JsArray(JsObject::from_val(c, a as Val))
    }
    pub fn new(context: &mut Context) -> JsArray {
        Self::from_iter(std::iter::empty(), context)
    }
    pub fn from_object(obj: JsObject) -> JsResult<JsArray> {
        if obj.is_array() { Ok(JsArray(obj)) } else { Err(JsNativeError::typ().with_message("object is not an Array").into()) }
    }
    pub fn length(&self, context: &mut Context) -> JsResult<u64> {
        Ok(self.0.get("length", context)?.to_number(context)? as u64)
    }
    pub fn get<K: Into<PropertyKey>>(&self, key: K, context: &mut Context) -> JsResult<JsValue> {
        self.0.get(key, context)
    }
    pub fn at(&self, index: i64, context: &mut Context) -> JsResult<JsValue> {
        let len = self.length(context)? as i64;
        let i = if index < 0 { len + index } else { index };
        if i < 0 || i >= len {
            return Ok(JsValue::Undefined);
        }
        self.0.get(i as u32, context)
    }
    pub fn push<V: Into<JsValue>>(&self, value: V, context: &mut Context) -> JsResult<JsValue> {
        let len = self.length(context)? as u32;
        self.0.set(len, value, true, context)?;
        Ok(JsValue::from(len + 1))
    }
}

impl std::ops::Deref for JsArray {
    type Target = JsObject;
    fn deref(&self) -> &JsObject {
        &self.0
    }
}
impl From<JsArray> for JsValue {
    fn from(a: JsArray) -> Self {
        JsValue::Object(a.0)
    }
}
impl From<JsArray> for JsObject {
    fn from(a: JsArray) -> Self {
        a.0
    }
}

// === Context ===

/// The token for "the active context" that Boa-shaped code passes around as `&mut Context`.
pub struct Context {
    _private: (),
}

#[derive(Clone, Copy)]
pub struct Intrinsics;
pub struct Constructors;
pub struct ObjectConstructor;

impl Intrinsics {
    pub fn constructors(&self) -> Constructors {
        Constructors
    }
}
impl Constructors {
    pub fn object(&self) -> ObjectConstructor {
        ObjectConstructor
    }
}
impl ObjectConstructor {
    pub fn prototype(&self) -> JsObject {
        let c = ctx();
        JsObject::from_val(c, property(c, global_value(c, "Object"), "prototype"))
    }
}

thread_local! {
    /// Host data (Boa's `Context::insert_data`), per context (several pages can share a thread)
    static HOST: RefCell<std::collections::HashMap<usize, Box<dyn Any>>> = RefCell::new(std::collections::HashMap::new());
}

impl Context {
    pub(crate) fn current() -> Context {
        Context { _private: () }
    }

    pub fn global_object(&mut self) -> JsObject {
        let c = ctx();
        JsObject::from_val(c, unsafe { JSContextGetGlobalObject(c) } as Val)
    }

    pub fn intrinsics(&self) -> Intrinsics {
        Intrinsics
    }

    pub fn realm(&self) -> Realm {
        Realm
    }

    pub fn insert_data<T: Any>(&mut self, data: T) {
        let old = HOST.with(|h| h.borrow_mut().insert(ctx() as usize, Box::new(data)));
        drop(old);
    }

    pub fn get_data<T: Any>(&self) -> Option<&T> {
        HOST.with(|h| {
            let b = h.borrow();
            let d: &T = b.get(&(ctx() as usize))?.downcast_ref::<T>()?;
            // SAFETY: the host data lives until the thread's runtime replaces or drops it, past every Context token
            Some(unsafe { &*(d as *const T) })
        })
    }

    pub fn register_global_property<V: Into<JsValue>>(&mut self, name: JsString, value: V, attr: Attribute) -> JsResult<()> {
        let global = self.global_object();
        global.define_property_or_throw(name, descriptor_for(value.into(), attr), self)?;
        Ok(())
    }

    pub fn register_global_callable(&mut self, name: JsString, length: usize, native: NativeFunction) -> JsResult<()> {
        let f = native.to_function(name.as_str(), length);
        self.register_global_property(name, f, Attribute::WRITABLE | Attribute::CONFIGURABLE)
    }

    /// Run pending promise jobs: JavaScriptCore runs them as each outermost API call returns, so there's nothing left
    pub fn run_jobs(&mut self) -> JsResult<()> {
        Ok(())
    }

    /// Evaluate a classic script
    pub fn eval(&mut self, source: &str, name: &str) -> JsResult<JsValue> {
        with_exception(|c, exc| unsafe {
            let (s, u) = (js_str(source), js_str(name));
            let v = JSEvaluateScript(c, s, null_mut(), u, 1, exc);
            JSStringRelease(s);
            JSStringRelease(u);
            if v.is_null() { None } else { Some(JsValue::from_jsc(c, v)) }
        })
    }
}

// === Boa's module paths ===

pub mod object {
    pub use super::{FunctionObjectBuilder, JsObject, ObjectInitializer};
    pub mod builtins {
        pub use super::super::JsArray;
    }
}
pub mod property {
    pub use super::{Attribute, PropertyDescriptor, PropertyKey};
}
pub mod value {
    pub use super::JsValue;
}
