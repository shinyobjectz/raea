//! Accessibility (docs/ts-native.feature P16): every element of a page is an AccessKit node, with the role, name, states and
//! properties Chrome gives the same markup (HTML-AAM and the accessible-name computation, the parts React UI uses).
//!
//! - Role: an explicit `role` (ARIA's), else the element's own (button, a with href, h1-h6, ul/ol/li, nav, main,
//!   header, footer, aside, section with a name, article, form, table rows and cells, input by type, textarea,
//!   select, option, img, svg, progress, meter, hr, dialog, details, summary, label). A div or span is a generic
//!   container; role none/presentation, img alt="" and a without href are too.
//! - Name: aria-labelledby, aria-label, the element's own (img alt, a field's label (for= or around it), an input
//!   button's value, an svg's title), its content for the roles named by their content (button, link, heading, tab,
//!   option, checkbox, radio, menuitem, cell, ...), then title, then a field's placeholder.
//! - States: checked (a checkbox's `Checked`, aria-checked), pressed, expanded, selected, disabled (and
//!   InteractionDisabled), focusable (a TabIndex), focused (InputFocus), level, the value and range of a slider,
//!   spinbutton, progressbar or field, required, read-only, invalid, modal, multiselectable, haspopup.
//! - Hidden: display none, visibility hidden, `hidden`, aria-hidden (each with what's inside it).
//!
//! `records` builds the tree for a page's elements (raea-page's Blitz page answers `Facts`) and reads it back through
//! accesskit_consumer (AccessKit's own reading of a tree, what its platform adapters use) as the records the parity
//! harness compares with Chrome's.

mod name;
mod roles;

pub use roles::chrome_role;
use roles::*;

use std::collections::HashMap;

use accesskit::{Action, HasPopup, Invalid, Node as AkNode, NodeId, Role, Toggled, Tree, TreeId, TreeUpdate};
use serde_json::{Map, Value, json};

fn s(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s != "false",
        Some(Value::Number(_)) => true,
        _ => false,
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn collapse(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An element as the computation sees it, by its key `K` (bevy_ui's entity; another page's element id).
pub struct El<K> {
    /// the HTML tag (a text field's `editableText` is its input or textarea)
    pub tag: String,
    pub props: Map<String, Value>,
    pub parent: Option<K>,
    pub kids: Vec<K>,
    /// a text node's string
    pub text: Option<String>,
    /// an svg's <title>
    pub title: Option<String>,
}

impl<K> El<K> {
    fn p(&self, k: &str) -> Option<&Value> {
        self.props.get(k)
    }
    /// a text node of only white space
    fn blank(&self) -> bool {
        matches!(&self.text, Some(t) if t.trim().is_empty())
    }
    fn ty(&self) -> String {
        s(self.p("type")).unwrap_or_default().to_ascii_lowercase()
    }
}

/// What the computation asks of the page an element is on, beyond its props: what its layout and its controls' state
/// say (bevy_ui's components here; a Blitz document's computed style and element state on the Blitz page).
pub trait Facts<K> {
    /// display: none on the element itself
    fn display_none(&self, k: K) -> bool;
    /// not drawn by visibility (hidden or collapse, its own or inherited)
    fn invisible(&self, k: K) -> bool;
    /// disabled by its control's own state (beyond its disabled prop)
    fn natively_disabled(&self, k: K) -> bool;
    /// a text field's text as its editor has it
    fn editor_value(&self, k: K) -> Option<String>;
    /// in the tab order by the page's own rule (beyond a tabIndex prop)
    fn tabbable(&self, k: K) -> bool;
    /// a checkbox's or radio's checked state
    fn checked(&self, k: K) -> bool;
    /// its data-testid, the harness's name for it
    fn test_id(&self, k: K) -> Option<String>;
    /// laid out as a block (its computed display's outside is block: block, flex, grid, list-item, table), when the
    /// host knows; a name from its content puts a space around it, as Chrome does. None: by its tag
    fn block(&self, _k: K) -> Option<bool> {
        None
    }
}

struct Ctx<'w, K> {
    facts: &'w dyn Facts<K>,
    els: HashMap<K, El<K>>,
    /// elements by their `id`, per root
    ids: HashMap<(K, String), K>,
    roots: HashMap<K, K>,
}


fn node_of<K: Copy + Eq + std::hash::Hash>(cx: &Ctx<K>, e: K, el: &El<K>, focus: Option<K>) -> AkNode {
    let role = cx.role(e);
    let mut n = AkNode::new(role);
    if el.text.is_some() {
        n.set_value(el.text.clone().unwrap_or_default());
        return n;
    }
    if cx.hidden(e) {
        n.set_hidden();
        return n;
    }
    if !unnamed(role) {
        let name = collapse(&cx.alt(e, false, &mut vec![]));
        if !name.is_empty() {
            n.set_label(name);
        }
    }
    let p = |k: &str| el.p(k);
    let disabled = cx.disabled(e);
    if disabled {
        n.set_disabled();
    }
    // aria-disabled says disabled but leaves it focusable; only a native disabled takes it out of the tab order
    let natively = truthy(p("disabled")) || cx.facts.natively_disabled(e);
    let focusable = !natively && (p("tabIndex").is_some() || cx.facts.tabbable(e));
    if focusable {
        n.add_action(Action::Focus);
    }
    if focus == Some(e) {
        n.add_action(Action::Focus);
    }
    // a control a press works: what VoiceOver's and an AX client's press asks for (macOS sends Click only to a node
    // that has it; raea.app's window took no press on its links or buttons without it, 11-23)
    if !disabled
        && matches!(
            role,
            Role::Button | Role::Link | Role::CheckBox | Role::RadioButton | Role::Switch | Role::MenuItem | Role::MenuItemCheckBox | Role::MenuItemRadio | Role::Tab | Role::ListBoxOption | Role::TreeItem | Role::DisclosureTriangle | Role::ComboBox
        )
    {
        n.add_action(Action::Click);
    }
    // a field typed in takes a value (macOS sends setAccessibilityValue only to a node that has it, or text ranges,
    // which the tree doesn't carry): an AX client's or Voice Control's text, typed over what the field held (ax.rs)
    if !disabled
        && !(truthy(p("readOnly")) || truthy(p("readonly")) || s(p("aria-readonly")).as_deref() == Some("true"))
        && matches!(role, Role::TextInput | Role::MultilineTextInput | Role::SearchInput | Role::EmailInput | Role::PasswordInput | Role::PhoneNumberInput | Role::UrlInput | Role::NumberInput | Role::SpinButton)
    {
        n.add_action(Action::SetValue);
    }
    // checked: a checkbox or radio (its Checked), or aria-checked; pressed: aria-pressed on a button
    let checkable = matches!(role, Role::CheckBox | Role::RadioButton | Role::Switch | Role::MenuItemCheckBox | Role::MenuItemRadio);
    let tri = |v: Option<String>| match v.as_deref() {
        Some("mixed") => Some(Toggled::Mixed),
        Some("true") => Some(Toggled::True),
        Some("false") => Some(Toggled::False),
        _ => None,
    };
    if checkable {
        let native = el.tag == "input" && matches!(el.ty().as_str(), "checkbox" | "radio");
        let t = if native { Some(Toggled::from(cx.facts.checked(e))) } else { tri(s(p("aria-checked"))).or(Some(Toggled::False)) };
        if let Some(t) = t {
            n.set_toggled(t);
        }
    } else if role == Role::Button && let Some(t) = tri(s(p("aria-pressed"))) {
        n.set_toggled(t);
    }
    if let Some(x) = s(p("aria-expanded")).and_then(|v| v.parse::<bool>().ok()) {
        n.set_expanded(x);
    } else if role == Role::ComboBox && el.tag == "select" {
        n.set_expanded(false);
    }
    if role != Role::GenericContainer && let Some(x) = s(p("aria-selected")).and_then(|v| v.parse::<bool>().ok()) {
        n.set_selected(x);
    }
    let level = match el.tag.as_str() {
        _ if role != Role::Heading && role != Role::ListItem => None,
        t if role == Role::Heading && s(p("aria-level")).is_none() && t.len() == 2 && t.starts_with('h') => t[1..].parse::<usize>().ok(),
        _ if role == Role::Heading => num(p("aria-level")).map(|x| x as usize).or(Some(2)),
        _ => Some(cx.ancestors(e).filter(|a| matches!(a.tag.as_str(), "ul" | "ol" | "menu") || s(a.p("role")).as_deref() == Some("list")).count().max(1)),
    };
    if let Some(l) = level {
        n.set_level(l);
    }
    if let Some(h) = s(p("aria-haspopup")).filter(|h| h != "false") {
        n.set_has_popup(match h.as_str() {
            "listbox" => HasPopup::Listbox,
            "tree" => HasPopup::Tree,
            "grid" => HasPopup::Grid,
            "dialog" => HasPopup::Dialog,
            _ => HasPopup::Menu,
        });
    } else if role == Role::ComboBox && el.tag == "select" {
        n.set_has_popup(HasPopup::Menu);
    }
    if truthy(p("aria-modal")) || (el.tag == "dialog" && truthy(p("open")) && truthy(p("data-modal"))) {
        n.set_modal();
    }
    if truthy(p("required")) || s(p("aria-required")).as_deref() == Some("true") {
        n.set_required();
    }
    if (is_field(role) && (truthy(p("readOnly")) || truthy(p("readonly")))) || s(p("aria-readonly")).as_deref() == Some("true") {
        n.set_read_only();
    }
    if s(p("aria-invalid")).is_some_and(|v| v != "false") {
        n.set_invalid(Invalid::True);
    }
    if truthy(p("aria-multiselectable")) || (role == Role::ListBox && truthy(p("multiple"))) {
        n.set_multiselectable();
    }
    // ranges: their value and bounds
    match role {
        Role::Slider | Role::ScrollBar | Role::ProgressIndicator | Role::Meter => {
            let native = matches!(el.tag.as_str(), "input" | "progress" | "meter");
            let (lo, hi) = if el.tag == "progress" { (0.0, num(p("max")).unwrap_or(1.0)) } else { (0.0, 100.0) };
            let min = if native { num(p("min")) } else { num(p("aria-valuemin")) }.unwrap_or(lo);
            let max = if native { num(p("max")) } else { num(p("aria-valuemax")) }.unwrap_or(hi);
            n.set_min_numeric_value(min);
            n.set_max_numeric_value(max);
            let v = if native { num(p("value")).or_else(|| num(p("defaultValue"))) } else { num(p("aria-valuenow")) };
            let v = v.or_else(|| (el.tag == "input").then_some(min + (max - min) / 2.0));
            if let Some(v) = v {
                n.set_numeric_value(v);
            }
        }
        _ => {}
    }
    if (is_field(role) || role == Role::ComboBox) && let Some(v) = cx.value(e, role) {
        n.set_value(v);
    }
    if let Some(t) = cx.facts.test_id(e) {
        n.set_author_id(t);
    }
    n
}

/// A page's elements as one AccessKit tree under `window` (its root, a Role::Window node): each element a node with its
/// children, its box where `bounds` gives one ([x0, y0, x1, y1] in the root's space), and the elements with a test id
/// beside it. raea's window publishes it to the OS (VoiceOver, macOS accessibility: raea-page-host's ax.rs); `records`
/// reads it back for the parity harness. `id_of` must never give `window`.
#[allow(clippy::too_many_arguments)] // each is its own input: the page, its facts, focus, ids, the window and boxes
pub fn tree<K: Copy + Eq + std::hash::Hash>(els: HashMap<K, El<K>>, roots: HashMap<K, K>, facts: &dyn Facts<K>, focus: Option<K>, id_of: impl Fn(K) -> u64, window: NodeId, label: Option<&str>, bounds: impl Fn(K) -> Option<[f64; 4]>) -> (TreeUpdate, Vec<(String, NodeId)>) {
    let mut ids = HashMap::new();
    for (k, el) in &els {
        if let (Some(id), Some(r)) = (el.props.get("id").and_then(|v| v.as_str()), roots.get(k)) {
            ids.insert((*r, id.to_string()), *k);
        }
    }
    let cx = Ctx { facts, els, ids, roots };
    let mut nodes = vec![];
    let mut top = vec![];
    let mut tested = vec![];
    // a text node of only white space (the markup's line breaks and indents) is no node, as in Chrome's tree: a screen
    // reader would stop on each
    for (&k, el) in &cx.els {
        if el.blank() {
            continue;
        }
        let mut n = node_of(&cx, k, el, focus);
        for c in &el.kids {
            if let Some(kid) = cx.els.get(c)
                && !kid.blank()
            {
                n.push_child(NodeId(id_of(*c)));
            }
        }
        if let Some([x0, y0, x1, y1]) = bounds(k) {
            n.set_bounds(accesskit::Rect { x0, y0, x1, y1 });
        }
        if el.parent.is_none_or(|p| !cx.els.contains_key(&p)) {
            top.push(NodeId(id_of(k)));
        }
        if let Some(t) = facts.test_id(k) {
            tested.push((t, NodeId(id_of(k))));
        }
        nodes.push((NodeId(id_of(k)), n));
    }
    let mut root = AkNode::new(Role::Window);
    root.set_children(top);
    if let Some(l) = label {
        root.set_label(l);
    }
    nodes.insert(0, (window, root));
    let focus_id = focus.filter(|f| cx.els.contains_key(f)).map(|f| NodeId(id_of(f))).unwrap_or(window);
    (TreeUpdate { nodes, tree: Some(Tree::new(window)), tree_id: TreeId::ROOT, focus: focus_id }, tested)
}

/// The update without its hidden nodes (each with what's in it), as a platform's tree from Chrome has none: AccessKit's
/// filtered walk (accesskit_consumer 0.38's next_filtered_sibling) stops for good at a hidden node that follows an
/// empty container (a layer with nothing in it, then raea cloud's closed side panel), and everything after them in
/// their parent was gone from what macOS lists: raea.app's welcome and every sheet (11-23). Focus on what's gone is
/// the window's.
pub fn unhide(update: &mut TreeUpdate, window: NodeId) {
    let kids: HashMap<NodeId, Vec<NodeId>> = update.nodes.iter().map(|(id, n)| (*id, n.children().to_vec())).collect();
    let mut gone = std::collections::HashSet::new();
    let mut stack: Vec<NodeId> = update.nodes.iter().filter(|(_, n)| n.is_hidden()).map(|(id, _)| *id).collect();
    while let Some(id) = stack.pop() {
        if gone.insert(id) {
            stack.extend(kids.get(&id).into_iter().flatten().copied());
        }
    }
    if gone.is_empty() {
        return;
    }
    update.nodes.retain(|(id, _)| !gone.contains(id));
    for (_, n) in &mut update.nodes {
        if n.children().iter().any(|c| gone.contains(c)) {
            let left: Vec<NodeId> = n.children().iter().copied().filter(|c| !gone.contains(c)).collect();
            n.set_children(left);
        }
    }
    if gone.contains(&update.focus) {
        update.focus = window;
    }
}

/// A page's elements (the Blitz page's, raea-page's host) as the parity harness's records, by test id, read through
/// accesskit_consumer. `roots` is each element's root (whose `id`s it looks up), `id_of` its AccessKit id (not
/// u64::MAX, the window's).
pub fn records<K: Copy + Eq + std::hash::Hash>(els: HashMap<K, El<K>>, roots: HashMap<K, K>, facts: &dyn Facts<K>, focus: Option<K>, id_of: impl Fn(K) -> u64) -> Value {
    let (update, tested) = tree(els, roots, facts, focus, id_of, NodeId(u64::MAX), None, |_| None);
    let tree = accesskit_consumer::Tree::new(update, true);
    let state = tree.state();
    let mut out = Map::new();
    for (id, node_id) in tested {
        let Some(node) = state.node_by_tree_local_id(node_id, TreeId::ROOT) else { continue };
        out.insert(id, record(&node));
    }
    json!({"nodes": out})
}

/// A consumer node as the harness's record.
fn record(n: &accesskit_consumer::Node) -> Value {
    let mut at = Some(*n);
    while let Some(x) = at {
        if x.is_hidden() {
            return json!({"role": "hidden"});
        }
        at = x.parent();
    }
    let role = chrome_role(n.role());
    let mut r = Map::new();
    let yes = |r: &mut Map<String, Value>, k: &str, b: bool| {
        if b {
            r.insert(k.into(), json!(true));
        }
    };
    yes(&mut r, "disabled", n.is_disabled());
    yes(&mut r, "focusable", n.is_focusable(&|_| accesskit_consumer::FilterResult::Include));
    yes(&mut r, "focused", n.is_focused());
    yes(&mut r, "modal", n.is_modal());
    yes(&mut r, "required", n.is_required());
    yes(&mut r, "readonly", n.data().is_read_only());
    yes(&mut r, "multiselectable", n.is_multiselectable());
    yes(&mut r, "invalid", n.data().invalid().is_some());
    if let Some(t) = n.toggled() {
        let v = match t {
            Toggled::True => "true",
            Toggled::False => "false",
            Toggled::Mixed => "mixed",
        };
        r.insert(if role == "button" { "pressed" } else { "checked" }.into(), json!(v));
    }
    if let Some(x) = n.data().is_expanded() {
        r.insert("expanded".into(), json!(x));
    }
    if let Some(x) = n.is_selected() {
        r.insert("selected".into(), json!(x));
    }
    if let Some(l) = n.level() {
        r.insert("level".into(), json!(l as f64));
    }
    if let Some(h) = n.has_popup() {
        let v = match h {
            HasPopup::Menu => "menu",
            HasPopup::Listbox => "listbox",
            HasPopup::Tree => "tree",
            HasPopup::Grid => "grid",
            HasPopup::Dialog => "dialog",
        };
        r.insert("haspopup".into(), json!(v));
    }
    if matches!(role, "slider" | "progressbar" | "scrollbar" | "meter") {
        if let Some(v) = n.min_numeric_value() {
            r.insert("valuemin".into(), json!(v));
        }
        if let Some(v) = n.max_numeric_value() {
            r.insert("valuemax".into(), json!(v));
        }
        if let Some(v) = n.numeric_value() {
            r.insert("value".into(), json!(fmt_num(v)));
        }
    }
    if role == "textbox" {
        r.insert("multiline".into(), json!(n.role() == Role::MultilineTextInput));
    }
    if matches!(role, "spinbutton" | "textbox" | "searchbox" | "combobox") && let Some(v) = n.data().value().filter(|v| !v.is_empty()) {
        r.insert("value".into(), json!(v));
    }
    if role == "generic" && r.is_empty() {
        return json!({"role": "generic"});
    }
    let name = if role == "generic" { String::new() } else { collapse(&n.label().unwrap_or_default()) };
    r.insert("role".into(), json!(role));
    r.insert("name".into(), json!(name));
    Value::Object(r)
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

#[cfg(test)]
mod tests;
