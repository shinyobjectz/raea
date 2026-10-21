//! The Blitz backend: agent-browser's words answered by raea's own engine, without Chrome (so on the iPhone too, and
//! deterministically: a virtual clock, no network unless allowed). A page is a ScriptDocument (Blitz + Stylo + the
//! engine's V8 page scripts); `snapshot` reads the DOM as an accessibility tree in agent-browser's format and numbering
//! (interactive roles always get a ref, named content roles too, a select's options after everything else, as Chrome's
//! tree has them); `click` dispatches a synthetic click through Blitz's event driver (the page's listeners and Blitz's
//! default actions run), `fill` and `type` set a control's value and fire input and change, `get`, `eval` and
//! `screenshot` read the page.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use blitz_dom::{BaseDocument, Document as _, DocumentConfig, LocalName, Node, NodeId};
use blitz_traits::events::DomEvent;
use keyboard_types::Modifiers;
use blitz_traits::shell::{ColorScheme, Viewport};
use blitz_v8_script::ScriptDocument;
use serde_json::{Value, json};

type R<T> = Result<T, String>;

const PAGE_PRELUDE: &str = r#"(function () {
  try {
    document.querySelectorAll('*').forEach(function (el) {
      (el.getAttributeNames ? el.getAttributeNames() : []).forEach(function (n) {
        if (n.length > 2 && n.slice(0, 2) === 'on') {
          var fn; try { fn = new Function('event', el.getAttribute(n)); } catch (e) { return; }
          el.addEventListener(n.slice(2), function (e) { return fn.call(el, e); });
        }
      });
    });
  } catch (e) {}
  try {
    var any = document.documentElement && Object.getPrototypeOf(document.documentElement);
    var d = any && Object.getOwnPropertyDescriptor(any, 'value');
    var opts = function (el) { return Array.prototype.slice.call(el.querySelectorAll('option')); };
    var chosen = function (el) { var o = opts(el); return o.find(function (x) { return x.__raeaSelected; }) || o.find(function (x) { return x.hasAttribute('selected'); }) || o[0] || null; };
    var optValue = function (o) { return o.hasAttribute('value') ? o.getAttribute('value') : (o.textContent || '').trim(); };
    Object.defineProperty(any, 'value', { configurable: true,
      get: function () { if (this.tagName === 'SELECT') { var c = chosen(this); return c ? optValue(c) : ''; } if (this.tagName === 'OPTION') return optValue(this); return d && d.get ? d.get.call(this) : this.getAttribute('value'); },
      set: function (v) { if (this.tagName === 'SELECT') { opts(this).forEach(function (o) { o.__raeaSelected = optValue(o) === String(v); }); return; } if (d && d.set) d.set.call(this, v); else this.setAttribute('value', v); } });
    Object.defineProperty(any, 'selectedIndex', { configurable: true, get: function () { if (this.tagName !== 'SELECT') return -1; var c = chosen(this); return c ? opts(this).indexOf(c) : -1; } });
    Object.defineProperty(any, 'options', { configurable: true, get: function () { return this.tagName === 'SELECT' ? opts(this) : undefined; } });
    Object.defineProperty(any, 'selected', { configurable: true, get: function () { if (this.tagName !== 'OPTION') return undefined; var s = this.closest ? this.closest('select') : null; return s ? chosen(s) === this : this.hasAttribute('selected'); } });
    Object.defineProperty(any, 'text', { configurable: true, get: function () { return (this.textContent || '').trim(); } });
  } catch (e) {}
})();"#;

pub struct Page {
    doc: ScriptDocument,
    url: String,
    width: u32,
    height: u32,
    refs: HashMap<String, NodeId>,
    result: Rc<RefCell<Option<String>>>,
}

thread_local! {
    static PAGES: RefCell<HashMap<String, Page>> = RefCell::new(HashMap::new());
}

const SKIP: &[&str] = &["script", "style", "head", "template", "noscript", "meta", "link", "title", "base"];
const INTERACTIVE: &[&str] = &["button", "link", "textbox", "checkbox", "radio", "combobox", "listbox", "menuitem", "menuitemcheckbox", "menuitemradio", "option", "searchbox", "slider", "spinbutton", "switch", "tab", "treeitem"];
const CONTENT: &[&str] = &["heading", "cell", "gridcell", "columnheader", "rowheader", "listitem", "article", "region", "main", "navigation"];

fn attr<'a>(n: &'a Node, name: &str) -> Option<&'a str> {
    n.element_data()?.attr(LocalName::from(name))
}
fn tag(n: &Node) -> String {
    n.element_data().map(|e| e.name.local.to_string()).unwrap_or_default()
}
fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whitespace runs as one space, ends kept: a label's text as Chrome names its control (" Done" after a checkbox).
fn collapse_runs(s: &str) -> String {
    let mut out = String::new();
    let mut ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !ws {
                out.push(' ');
            }
            ws = true;
        } else {
            out.push(c);
            ws = false;
        }
    }
    out.trim_end().to_string()
}

/// The text a person sees in a subtree (scripts and hidden parts left out).
fn text_of(doc: &BaseDocument, id: NodeId) -> String {
    let Some(n) = doc.get_node(id) else { return String::new() };
    if n.is_text_node() {
        return n.text_content();
    }
    if SKIP.contains(&tag(n).as_str()) || hidden(n) {
        return String::new();
    }
    n.children.iter().map(|c| text_of(doc, *c)).collect::<Vec<_>>().join(if matches!(tag(n).as_str(), "br" | "p" | "div" | "li") { " " } else { "" })
}

fn hidden(n: &Node) -> bool {
    if attr(n, "hidden").is_some() || attr(n, "aria-hidden") == Some("true") || (tag(n) == "input" && attr(n, "type") == Some("hidden")) {
        return true;
    }
    // a select's options are drawn by the select (display: none in the page), yet they're in the tree
    tag(n) != "option" && n.primary_styles().is_some_and(|s| s.get_box().display.is_none())
}

/// A select's chosen option: the live one when the page said, else the first marked `selected`, else the first.
fn chosen_option(doc: &BaseDocument, select: NodeId, values: &HashMap<NodeId, String>) -> Option<NodeId> {
    let opts: Vec<NodeId> = all_under(doc, select).into_iter().filter(|id| doc.get_node(*id).is_some_and(|n| tag(n) == "option")).collect();
    opts.iter().copied().find(|id| values.get(id).is_some_and(|v| v == "selected"))
        .or_else(|| opts.iter().copied().find(|id| doc.get_node(*id).is_some_and(|n| attr(n, "selected").is_some())))
        .or_else(|| opts.first().copied())
}

fn all_under(doc: &BaseDocument, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(x) = stack.pop() {
        out.push(x);
        if let Some(n) = doc.get_node(x) {
            stack.extend(n.children.iter().rev());
        }
    }
    out
}

struct Ax {
    role: String,
    name: String,
    level: Option<u32>,
    checked: Option<bool>,
    expanded: Option<bool>,
    selected: bool,
    disabled: bool,
    required: bool,
    value: Option<String>,
    node: NodeId,
    children: Vec<usize>,
    ref_id: Option<String>,
    named_by_content: bool,
}

fn implicit_role(doc: &BaseDocument, n: &Node) -> String {
    if let Some(r) = attr(n, "role") {
        return r.split_whitespace().next().unwrap_or("").to_string();
    }
    let t = tag(n);
    match t.as_str() {
        "a" | "area" if attr(n, "href").is_some() => "link".into(),
        "button" => "button".into(),
        "input" => match attr(n, "type").unwrap_or("text").to_ascii_lowercase().as_str() {
            "button" | "submit" | "reset" | "image" => "button".into(),
            "checkbox" => "checkbox".into(),
            "radio" => "radio".into(),
            "range" => "slider".into(),
            "number" => "spinbutton".into(),
            "search" => "searchbox".into(),
            _ => "textbox".into(),
        },
        "textarea" => "textbox".into(),
        "select" => if attr(n, "multiple").is_some() || attr(n, "size").and_then(|s| s.parse::<u32>().ok()).is_some_and(|s| s > 1) { "listbox".into() } else { "combobox".into() },
        "option" => "option".into(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading".into(),
        "img" => if attr(n, "alt").is_some_and(|a| a.is_empty()) { String::new() } else { "img".into() },
        "ul" | "ol" | "menu" => "list".into(),
        "li" => "listitem".into(),
        "nav" => "navigation".into(),
        "main" => "main".into(),
        "article" => "article".into(),
        "aside" => "complementary".into(),
        "section" if attr(n, "aria-label").is_some() || attr(n, "aria-labelledby").is_some() => "region".into(),
        "form" if attr(n, "aria-label").is_some() || attr(n, "name").is_some() => "form".into(),
        "table" => "table".into(),
        "tr" => "row".into(),
        "td" => "cell".into(),
        "th" => "columnheader".into(),
        "p" => "paragraph".into(),
        "label" => "LabelText".into(),
        "dialog" => "dialog".into(),
        "fieldset" => "group".into(),
        "progress" => "progressbar".into(),
        "header" if !inside(doc, n, &["article", "section", "main"]) => "banner".into(),
        "footer" if !inside(doc, n, &["article", "section", "main"]) => "contentinfo".into(),
        _ => String::new(),
    }
}

fn inside(doc: &BaseDocument, n: &Node, tags: &[&str]) -> bool {
    let mut p = n.parent;
    while let Some(id) = p {
        let Some(pn) = doc.get_node(id) else { break };
        if tags.contains(&tag(pn).as_str()) {
            return true;
        }
        p = pn.parent;
    }
    false
}

/// The accessible name (aria-label, aria-labelledby, a label, alt, the content for roles named by it, value,
/// placeholder, title), and whether it came from the content.
fn name_of(doc: &BaseDocument, n: &Node, role: &str) -> (String, bool) {
    if let Some(l) = attr(n, "aria-label").filter(|l| !l.trim().is_empty()) {
        return (collapse(l), false);
    }
    if let Some(ids) = attr(n, "aria-labelledby") {
        let t: Vec<String> = ids.split_whitespace().filter_map(|i| doc.get_element_by_id(i)).map(|id| collapse(&text_of(doc, id))).collect();
        if !t.is_empty() {
            return (t.join(" "), false);
        }
    }
    let t = tag(n);
    if matches!(t.as_str(), "input" | "select" | "textarea") {
        if matches!(attr(n, "type"), Some("button" | "submit" | "reset")) {
            return (attr(n, "value").map(collapse).unwrap_or_else(|| if attr(n, "type") == Some("submit") { "Submit".into() } else { String::new() }), false);
        }
        if let Some(id) = attr(n, "id") {
            if let Some(l) = find_label(doc, id) {
                return (l, false);
            }
        }
        let mut p = n.parent;
        while let Some(pid) = p {
            let Some(pn) = doc.get_node(pid) else { break };
            if tag(pn) == "label" {
                return (collapse_runs(&text_of(doc, pid)), false);
            }
            p = pn.parent;
        }
        if let Some(ph) = attr(n, "placeholder").filter(|p| !p.is_empty()) {
            return (collapse(ph), false);
        }
    }
    if t == "img" {
        if let Some(a) = attr(n, "alt") {
            return (collapse(a), false);
        }
    }
    if matches!(role, "button" | "link" | "heading" | "option" | "cell" | "columnheader" | "rowheader" | "gridcell" | "menuitem" | "tab" | "treeitem" | "checkbox" | "radio" | "switch" | "LabelText") {
        let c = collapse(&text_of(doc, n.id));
        if !c.is_empty() {
            return (c, true);
        }
    }
    (attr(n, "title").map(collapse).unwrap_or_default(), false)
}

fn find_label(doc: &BaseDocument, id: &str) -> Option<String> {
    let mut stack = vec![doc.root_node().id];
    while let Some(x) = stack.pop() {
        let Some(n) = doc.get_node(x) else { continue };
        if tag(n) == "label" && attr(n, "for") == Some(id) {
            return Some(collapse(&text_of(doc, x)));
        }
        stack.extend(n.children.iter().rev());
    }
    None
}

fn build(doc: &BaseDocument, id: NodeId, out: &mut Vec<Ax>, parent: Option<usize>, values: &HashMap<NodeId, String>) {
    let Some(n) = doc.get_node(id) else { return };
    if n.is_text_node() {
        let t = collapse(&n.text_content());
        if !t.is_empty() {
            let me = out.len();
            out.push(Ax { role: "StaticText".into(), name: t, level: None, checked: None, expanded: None, selected: false, disabled: false, required: false, value: None, node: id, children: Vec::new(), ref_id: None, named_by_content: false });
            if let Some(p) = parent {
                out[p].children.push(me);
            }
        }
        return;
    }
    if n.element_data().is_some() && (SKIP.contains(&tag(n).as_str()) || hidden(n)) {
        return;
    }
    let role = if n.element_data().is_some() { implicit_role(doc, n) } else { String::new() };
    let me = if role.is_empty() {
        parent
    } else {
        let (name, by_content) = name_of(doc, n, &role);
        let t = tag(n);
        let level = if role == "heading" { attr(n, "aria-level").and_then(|l| l.parse().ok()).or_else(|| t.strip_prefix('h').and_then(|d| d.parse().ok())) } else { None };
        let checked = if matches!(role.as_str(), "checkbox" | "radio" | "switch") { Some(values.get(&id).map(|v| v == "true").unwrap_or(attr(n, "checked").is_some())) } else { None };
        let value = match role.as_str() {
            "textbox" | "searchbox" | "spinbutton" => values.get(&id).cloned().or_else(|| attr(n, "value").map(str::to_string)).filter(|v| !v.is_empty()),
            "combobox" => chosen_option(doc, id, values).map(|o| collapse(&text_of(doc, o))),
            _ => None,
        };
        let chosen = if role == "option" { n.parent.and_then(|p| { let mut q = Some(p); while let Some(x) = q { if doc.get_node(x).is_some_and(|pn| tag(pn) == "select") { return Some(x); } q = doc.get_node(x).and_then(|pn| pn.parent); } None }).and_then(|sel| chosen_option(doc, sel, values)) == Some(id) } else { false };
        let name = if role == "LabelText" { String::new() } else { name };
        let o = out.len();
        out.push(Ax {
            role: role.clone(),
            name,
            level,
            checked,
            expanded: if role == "combobox" { Some(false) } else { None },
            selected: chosen,
            disabled: attr(n, "disabled").is_some(),
            required: attr(n, "required").is_some(),
            value,
            node: id,
            children: Vec::new(),
            ref_id: None,
            named_by_content: by_content,
        });
        if let Some(p) = parent {
            out[p].children.push(o);
        }
        Some(o)
    };
    // a select's options sit in a MenuListPopup, as Chrome's tree has them
    if me.is_some_and(|m| out[m].role == "combobox") {
        let popup = out.len();
        out.push(Ax { role: "MenuListPopup".into(), name: String::new(), level: None, checked: None, expanded: None, selected: false, disabled: false, required: false, value: None, node: id, children: Vec::new(), ref_id: None, named_by_content: false });
        out[me.unwrap()].children.push(popup);
        for c in &n.children {
            build(doc, *c, out, Some(popup), values);
        }
        return;
    }
    // a node named by its content doesn't list that text again
    if me.is_some_and(|m| out[m].named_by_content && out[m].role != "LabelText") {
        for c in &n.children {
            if doc.get_node(*c).is_some_and(|cn| cn.element_data().is_some()) {
                build(doc, *c, out, me, values);
            }
        }
        return;
    }
    for c in &n.children {
        build(doc, *c, out, me, values);
    }
}

fn render(nodes: &[Ax], idx: usize, indent: usize, interactive: bool, out: &mut String) {
    let n = &nodes[idx];
    if interactive && n.ref_id.is_none() {
        for c in &n.children {
            render(nodes, *c, indent, interactive, out);
        }
        return;
    }
    let mut line = format!("{}- {}", "  ".repeat(indent), n.role);
    if !n.name.is_empty() {
        line += &format!(" {}", serde_json::to_string(&n.name).unwrap_or_default());
    }
    let mut attrs = Vec::new();
    if let Some(l) = n.level {
        attrs.push(format!("level={l}"));
    }
    if let Some(c) = n.checked {
        attrs.push(format!("checked={c}"));
    }
    if let Some(e) = n.expanded {
        attrs.push(format!("expanded={e}"));
    }
    if n.selected {
        attrs.push("selected".into());
    }
    if n.disabled {
        attrs.push("disabled".into());
    }
    if n.required {
        attrs.push("required".into());
    }
    if let Some(r) = &n.ref_id {
        attrs.push(format!("ref={r}"));
    }
    if !attrs.is_empty() {
        line += &format!(" [{}]", attrs.join(", "));
    }
    if let Some(v) = &n.value {
        line += &format!(": {v}");
    }
    out.push_str(&line);
    out.push('\n');
    for c in &n.children {
        render(nodes, *c, indent + 1, interactive, out);
    }
}

impl Page {
    fn open(url: &str, allow_network: bool, size: (u32, u32)) -> R<Page> {
        let (html, base) = if let Some(data) = url.strip_prefix("data:text/html,") {
            (urlencoding_decode(data), url::Url::parse("about:blank").ok())
        } else {
            let u = url::Url::parse(url).or_else(|_| url::Url::from_file_path(std::path::absolute(url).map_err(|e| e.to_string())?).map_err(|_| format!("{url}: not a URL or a path")))?;
            let net = raea::net::SyncNet::new(std::path::Path::new("."), allow_network);
            let bytes = net.load(&u).map_err(|e| format!("{url}: {e}"))?;
            (String::from_utf8_lossy(&bytes).into_owned(), Some(u))
        };
        let net = Arc::new(raea::net::SyncNet::new(std::path::Path::new("."), allow_network));
        let config = DocumentConfig {
            base_url: base.as_ref().map(|u| u.to_string()),
            net_provider: Some(net.clone() as _),
            html_parser_provider: Some(Arc::new(blitz_html::HtmlProvider)),
            viewport: Some(Viewport::new(size.0, size.1, 1.0, ColorScheme::Light)),
            ..Default::default()
        };
        let mut doc = BaseDocument::new(config);
        {
            let mut m = doc.mutate();
            blitz_html::DocumentHtmlParser::parse_into_mutator(&mut m, &html);
        }
        let result: Rc<RefCell<Option<String>>> = Default::default();
        let slot = result.clone();
        let mut sdoc = ScriptDocument::from_base_document(doc).without_timer_thread().with_virtual_time().with_native("raea_result", move |s| {
            *slot.borrow_mut() = Some(s.to_string());
            Ok(String::new())
        });
        // what a browser gives a page that the engine's page runtime leaves out: inline on* attributes bound as
        // listeners, and a select's value and selectedIndex from its options
        sdoc.eval(PAGE_PRELUDE);
        sdoc.execute_scripts();
        // an event fired the way the page's runtime can make one
        sdoc.eval("window.__raeaFire = function (el, type) { var e; try { e = new Event(type, { bubbles: true }); } catch (_) { try { e = document.createEvent('Event'); e.initEvent(type, true, true); } catch (__) { e = null; } } if (e) el.dispatchEvent(e); };");
        let mut page = Page { doc: sdoc, url: base.map(|u| u.to_string()).unwrap_or_else(|| url.to_string()), width: size.0, height: size.1, refs: HashMap::new(), result };
        page.settle(50);
        Ok(page)
    }

    /// Due timers run on the virtual clock, up to `ms` ahead, then style and layout.
    fn settle(&mut self, ms: u64) {
        let end = self.doc.clock_now() + std::time::Duration::from_millis(ms);
        for _ in 0..1000 {
            match self.doc.next_timer_deadline() {
                Some(d) if d <= end => {
                    self.doc.advance_clock_to(d);
                    self.doc.poll(None);
                }
                _ => break,
            }
        }
        self.doc.advance_clock_to(end);
        let mut inner = self.doc.inner_mut();
        inner.resolve(0.0);
    }

    /// JS run in the page; what it hands to `raea_result` (JSON) comes back.
    fn js(&mut self, code: &str) -> R<Value> {
        *self.result.borrow_mut() = None;
        self.doc.eval(&format!("try {{ __blitz_native('raea_result', JSON.stringify({{ ok: (function() {{ {code} }})() }})) }} catch (e) {{ __blitz_native('raea_result', JSON.stringify({{ err: String(e && e.message || e) }})) }}"));
        let errs = self.doc.take_js_errors();
        let r = self.result.borrow_mut().take();
        match r.and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
            Some(v) if v.get("err").is_some() => Err(v["err"].as_str().unwrap_or("error").to_string()),
            Some(v) => Ok(v.get("ok").cloned().unwrap_or(Value::Null)),
            None => Err(errs.first().cloned().unwrap_or_else(|| "the script didn't answer".into())),
        }
    }

    /// The node a target names: @eN / eN from the last snapshot, or a CSS selector.
    fn target(&mut self, t: &str) -> R<NodeId> {
        let r = t.trim_start_matches('@');
        if let Some(id) = self.refs.get(r) {
            return Ok(*id);
        }
        if r.starts_with('e') && r[1..].chars().all(|c| c.is_ascii_digit()) && !r[1..].is_empty() {
            return Err(format!("Element \"{t}\" not found: take a fresh snapshot"));
        }
        let sel = serde_json::to_string(t).unwrap();
        self.js(&format!("const el = document.querySelector({sel}); if (!el) throw new Error('Element \"' + {sel} + '\" not found'); el.setAttribute('data-raea-target', ''); return 1;"))?;
        let doc = self.doc.inner();
        let found = all_nodes(&doc).into_iter().find(|&id| doc.get_node(id).is_some_and(|n| attr(n, "data-raea-target").is_some()));
        drop(doc);
        self.js("const el = document.querySelector('[data-raea-target]'); if (el) el.removeAttribute('data-raea-target'); return 1;")?;
        found.ok_or_else(|| format!("Element \"{t}\" not found"))
    }

    /// JS with `el` bound to the node.
    fn on(&mut self, node: NodeId, code: &str) -> R<Value> {
        {
            let mut d = self.doc.inner_mut();
            d.mutate().set_attribute(node, blitz_dom::QualName::new(None, blitz_dom::ns!(), blitz_dom::LocalName::from("data-raea-target")), "");
        }
        let r = self.js(&format!("const el = document.querySelector('[data-raea-target]'); el.removeAttribute('data-raea-target'); {code}"));
        r
    }

    fn values(&mut self) -> HashMap<NodeId, String> {
        // current values of controls, read from the page (Blitz keeps a form control's live value in JS)
        let _ = self.js("document.querySelectorAll('input,textarea,select,option').forEach((el, i) => el.setAttribute('data-raea-v', String(i))); return 1;");
        let v = self.js("return Array.from(document.querySelectorAll('input,textarea,select,option')).map((el) => el.tagName === 'OPTION' ? (el.selected ? 'selected' : '') : el.tagName === 'SELECT' ? (el.options && el.options[el.selectedIndex] ? el.options[el.selectedIndex].text : '') : (el.type === 'checkbox' || el.type === 'radio') ? String(!!el.checked) : String(el.value == null ? '' : el.value));").unwrap_or(Value::Null);
        let doc = self.doc.inner();
        let mut out = HashMap::new();
        if let Some(list) = v.as_array() {
            for id in all_nodes(&doc) {
                if let Some(i) = doc.get_node(id).and_then(|n| attr(n, "data-raea-v")).and_then(|s| s.parse::<usize>().ok()) {
                    if let Some(s) = list.get(i).and_then(Value::as_str) {
                        out.insert(id, s.to_string());
                    }
                }
            }
        }
        drop(doc);
        let _ = self.js("document.querySelectorAll('[data-raea-v]').forEach((el) => el.removeAttribute('data-raea-v')); return 1;");
        out
    }

    fn snapshot(&mut self, interactive: bool) -> Value {
        let values = self.values();
        let doc = self.doc.inner();
        let mut nodes: Vec<Ax> = Vec::new();
        let root = doc.root_node().id;
        nodes.push(Ax { role: "RootWebArea".into(), name: String::new(), level: None, checked: None, expanded: None, selected: false, disabled: false, required: false, value: None, node: root, children: Vec::new(), ref_id: None, named_by_content: false });
        for c in doc.root_node().children.clone() {
            build(&doc, c, &mut nodes, Some(0), &values);
        }
        // a select with no option chosen shows its first
        let mut refs = serde_json::Map::new();
        self.refs.clear();
        let mut k = 0;
        let mut options = Vec::new();
        for i in 0..nodes.len() {
            let n = &nodes[i];
            let wants = INTERACTIVE.contains(&n.role.as_str()) || (CONTENT.contains(&n.role.as_str()) && !n.name.is_empty());
            if !wants {
                continue;
            }
            if n.role == "option" {
                options.push(i);
                continue;
            }
            k += 1;
            nodes[i].ref_id = Some(format!("e{k}"));
        }
        for i in options {
            k += 1;
            nodes[i].ref_id = Some(format!("e{k}"));
        }
        for n in &nodes {
            if let Some(r) = &n.ref_id {
                refs.insert(r.clone(), json!({"role": n.role, "name": n.name}));
                self.refs.insert(r.clone(), n.node);
            }
        }
        drop(doc);
        let mut text = String::new();
        for c in nodes[0].children.clone() {
            render(&nodes, c, 0, interactive, &mut text);
        }
        json!({"snapshot": text.trim_end(), "refs": refs, "origin": self.url})
    }

    fn click(&mut self, node: NodeId) -> R<()> {
        let ev = {
            let d = self.doc.inner();
            let n = d.get_node(node).ok_or("that element is gone")?;
            n.synthetic_click_event(Modifiers::empty())
        };
        self.doc.dispatch_dom_event(DomEvent::new(node, ev));
        self.settle(50);
        Ok(())
    }

    fn screenshot(&mut self, path: &str, full: bool) -> R<()> {
        let h = if full {
            let d = self.doc.inner();
            (d.root_element().final_layout().size.height.ceil() as u32).max(self.height)
        } else {
            self.height
        };
        if full {
            let mut d = self.doc.inner_mut();
            d.set_viewport(Viewport::new(self.width, h, 1.0, ColorScheme::Light));
            d.resolve(0.0);
        }
        let mut rgba = Vec::new();
        {
            let mut d = self.doc.inner_mut();
            raea::row_render::paint_document(&mut d, self.width, h, 1.0, false, &mut rgba);
        }
        for px in rgba.chunks_exact_mut(4) {
            let k = 255 - px[3] as u16;
            for c in &mut px[..3] {
                *c = (*c as u16 + k).min(255) as u8;
            }
            px[3] = 255;
        }
        if let Some(dir) = std::path::Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        image::save_buffer(path, &rgba, self.width, h, image::ExtendedColorType::Rgba8).map_err(|e| format!("{path}: {e}"))
    }
}

fn all_nodes(doc: &BaseDocument) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![doc.root_node().id];
    while let Some(x) = stack.pop() {
        out.push(x);
        if let Some(n) = doc.get_node(x) {
            stack.extend(n.children.iter().rev());
        }
    }
    out
}

fn urlencoding_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// agent-browser's words on the Blitz backend: the same response shape and printed text for the verbs it has. Pages
/// live on one thread of their own (each page has its own V8 isolate, which mustn't be entered inside a caller's), and
/// each request waits for its answer.
pub fn run(args: &[String]) -> R<Value> {
    type Req = (Vec<String>, std::sync::mpsc::Sender<R<Value>>);
    static TX: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<Req>>> = std::sync::OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Req>();
        std::thread::Builder::new()
            .name("raea-browser-blitz".into())
            .stack_size(16 << 20)
            .spawn(move || {
                for (args, reply) in rx {
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_here(&args))).unwrap_or_else(|_| Err("the Blitz page panicked".into()));
                    let _ = reply.send(r);
                }
            })
            .expect("the Blitz thread");
        std::sync::Mutex::new(tx)
    });
    let (rtx, rrx) = std::sync::mpsc::channel();
    tx.lock().unwrap().send((args.to_vec(), rtx)).map_err(|e| e.to_string())?;
    rrx.recv().map_err(|e| e.to_string())?
}

fn run_here(args: &[String]) -> R<Value> {
    // --session NAME picks the page; --engine blitz and other flags are taken out
    let mut session = "default".to_string();
    let mut allow_network = false;
    let mut size = (1280u32, 720u32);
    let mut words: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--session" => {
                session = args.get(i + 1).cloned().unwrap_or_default();
                i += 2;
            }
            "--engine" | "--screenshot-format" | "--screenshot-quality" | "--cdp" => i += 2,
            "--allow-network" => {
                allow_network = true;
                i += 1;
            }
            "--viewport" => {
                if let Some((w, h)) = args.get(i + 1).and_then(|v| v.split_once('x')) {
                    size = (w.parse().unwrap_or(1280), h.parse().unwrap_or(720));
                }
                i += 2;
            }
            _ => {
                words.push(args[i].clone());
                i += 1;
            }
        }
    }
    let verb = words.first().map(String::as_str).unwrap_or("");
    let ok = |data: Value, text: String| Ok(json!({"success": true, "data": data, "error": null, "code": null, "text": text}));
    let fail = |e: String| Ok(json!({"success": false, "data": null, "error": e.clone(), "code": null, "text": format!("✗ {e}\n")}));
    if verb == "open" || verb == "goto" || verb == "navigate" {
        let Some(url) = words.get(1) else { return ok(json!({"url": "about:blank"}), "✓ about:blank\n".into()) };
        return match Page::open(url, allow_network, size) {
            Ok(p) => {
                let title = p_title(&p);
                let u = p.url.clone();
                PAGES.with(|ps| {
                    if let Some(old) = ps.borrow_mut().insert(session.clone(), p) {
                        retire(old);
                    }
                });
                ok(json!({"url": u, "title": title}), format!("✓ {title}\n  {u}\n"))
            }
            Err(e) => fail(e),
        };
    }
    if verb == "close" {
        PAGES.with(|ps| {
            if let Some(old) = ps.borrow_mut().remove(&session) {
                retire(old);
            }
        });
        return ok(json!({"closed": true}), "✓ Browser closed\n".into());
    }
    if verb == "batch" {
        return Err("batch is answered by the caller".into());
    }
    PAGES.with(|ps| {
        let mut ps = ps.borrow_mut();
        let Some(p) = ps.get_mut(&session) else { return fail("no page is open (open URL first)".into()) };
        let r: R<(Value, String)> = (|| match verb {
            "snapshot" => {
                let interactive = words.iter().any(|w| w == "-i" || w == "--interactive");
                let s = p.snapshot(interactive);
                let text = format!("{}\n", s["snapshot"].as_str().unwrap_or(""));
                Ok((s, text))
            }
            "click" | "dblclick" => {
                let t = words.get(1).ok_or("click needs a target")?;
                let n = p.target(t)?;
                p.click(n)?;
                Ok((json!({"clicked": t}), "✓ Done\n".into()))
            }
            "fill" | "type" => {
                let t = words.get(1).ok_or("fill needs a target and text")?;
                let text = words.get(2).cloned().unwrap_or_default();
                let n = p.target(t)?;
                let s = serde_json::to_string(&text).unwrap();
                let set = if verb == "fill" { format!("el.value = {s};") } else { format!("el.value = String(el.value || '') + {s};") };
                let value = p.on(n, &format!("if (el.focus) el.focus(); {set} return String(el.value);"))?;
                // the input event through Blitz's driver, so the page's input listeners see it
                p.doc.dispatch_dom_event(DomEvent::new(n, blitz_traits::events::DomEventData::Input(blitz_traits::events::BlitzInputEvent { value: value.as_str().unwrap_or("").to_string() })));
                p.settle(20);
                Ok((json!({"filled": t}), "✓ Done\n".into()))
            }
            "select" => {
                let t = words.get(1).ok_or("select needs a target and a value")?;
                let v = serde_json::to_string(&words.get(2).cloned().unwrap_or_default()).unwrap();
                let n = p.target(t)?;
                p.on(n, &format!("const o = Array.from(el.options).find((o) => o.value === {v} || o.text === {v}); if (!o) throw new Error('no option ' + {v}); el.value = o.value; return 1;"))?;
                p.settle(20);
                Ok((json!({"selected": t}), "✓ Done\n".into()))
            }
            "check" | "uncheck" => {
                let t = words.get(1).ok_or("check needs a target")?;
                let n = p.target(t)?;
                p.on(n, &format!("el.checked = {}; __raeaFire(el, 'change'); return 1;", verb == "check"))?;
                Ok((json!({"checked": verb == "check"}), "✓ Done\n".into()))
            }
            "press" | "key" => {
                let key = serde_json::to_string(words.get(1).ok_or("press needs a key")?).unwrap();
                p.js(&format!("const t = document.activeElement || document.body; for (const type of ['keydown', 'keyup']) t.dispatchEvent(new KeyboardEvent(type, {{ key: {key}, bubbles: true }})); if ({key} === 'Enter' && t.form) {{ const b = t.form.querySelector('[type=submit],button:not([type])'); if (b) b.click(); }} return 1;"))?;
                p.settle(20);
                Ok((json!({"pressed": words[1]}), "✓ Done\n".into()))
            }
            "get" => {
                let what = words.get(1).map(String::as_str).ok_or("get needs what (text, value, html, title, url, attr, count)")?;
                let v = match what {
                    "title" => json!({"title": p_title(p)}),
                    "url" => json!({"url": p.url}),
                    "text" | "value" | "html" | "attr" => {
                        let t = words.get(2).ok_or("get needs a target")?;
                        let n = p.target(t)?;
                        let field = match what {
                            "text" => "return (el.innerText || el.textContent || '').replace(/\\s+/g, ' ').trim();".to_string(),
                            "value" => "return String(el.value == null ? '' : el.value);".to_string(),
                            "html" => "return el.innerHTML;".to_string(),
                            _ => format!("return el.getAttribute({});", serde_json::to_string(words.get(3).ok_or("get attr needs a name")?).unwrap()),
                        };
                        let r = p.on(n, &field)?;
                        json!({ what: r })
                    }
                    "count" => {
                        let sel = serde_json::to_string(words.get(2).ok_or("get count needs a selector")?).unwrap();
                        json!({"count": p.js(&format!("return document.querySelectorAll({sel}).length;"))?})
                    }
                    w => return Err(format!("get {w} isn't on the Blitz backend yet")),
                };
                let text = v.as_object().and_then(|o| o.values().next()).map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())).unwrap_or_default();
                Ok((v, format!("{text}\n")))
            }
            "eval" => {
                let code = words[1..].join(" ");
                let r = p.js(&format!("return (0, eval)({});", serde_json::to_string(&code).unwrap()))?;
                p.settle(20);
                let text = r.as_str().map(str::to_string).unwrap_or_else(|| r.to_string());
                Ok((json!({"result": r}), format!("{text}\n")))
            }
            "screenshot" => {
                let path = words.iter().skip(1).find(|w| !w.starts_with('-')).cloned().unwrap_or_else(|| std::env::temp_dir().join(format!("screenshot-{}.png", std::process::id())).display().to_string());
                p.screenshot(&path, words.iter().any(|w| w == "--full" || w == "-f"))?;
                Ok((json!({"path": path}), format!("✓ Screenshot saved to {path}\n")))
            }
            "wait" => {
                let ms = words.get(1).and_then(|w| w.parse::<u64>().ok()).unwrap_or(0);
                p.settle(ms);
                Ok((json!({"waited": ms}), "✓ Done\n".into()))
            }
            "reload" => {
                let url = p.url.clone();
                let fresh = Page::open(&url, allow_network, size)?;
                retire(std::mem::replace(p, fresh));
                Ok((json!({"url": url}), "✓ Done\n".into()))
            }
            v => Err(format!("{v} isn't on the Blitz backend yet (open, snapshot, click, fill, type, select, check, press, get, eval, screenshot, wait, reload, close)")),
        })();
        match r {
            Ok((d, t)) => ok(d, t),
            Err(e) => fail(e),
        }
    })
}

/// A page no longer used: V8 isolates must be dropped newest first, and pages come and go in any order, so a page's
/// isolate is kept (its memory with it) for the life of the Blitz thread rather than dropped out of order.
fn retire(p: Page) {
    std::mem::forget(p);
}

fn p_title(p: &Page) -> String {
    let d = p.doc.inner();
    all_nodes(&d).into_iter().find_map(|id| d.get_node(id).filter(|n| tag(n) == "title").map(|n| collapse(&n.text_content()))).unwrap_or_default()
}

