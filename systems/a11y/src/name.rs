//! What a Ctx works out per element: hidden, role, name, description, states and values.

use super::*;

impl<K: Copy + Eq + std::hash::Hash> Ctx<'_, K> {
    pub(crate) fn el(&self, e: K) -> Option<&El<K>> {
        self.els.get(&e)
    }

    pub(crate) fn ancestors(&self, e: K) -> impl Iterator<Item = &El<K>> {
        let mut at = self.el(e).and_then(|x| x.parent);
        std::iter::from_fn(move || {
            let p = at?;
            let el = self.el(p)?;
            at = el.parent;
            Some(el)
        })
    }

    pub(crate) fn by_id(&self, e: K, id: &str) -> Option<K> {
        let root = *self.roots.get(&e)?;
        self.ids.get(&(root, id.to_string())).copied()
    }

    pub(crate) fn in_option_context(&self, e: K) -> bool {
        let mut at = self.el(e).and_then(|el| el.parent);
        while let Some(x) = at {
            let Some(el) = self.el(x) else { return false };
            match self.role(x) {
                Role::ListBox | Role::ComboBox | Role::EditableComboBox => return true,
                Role::Group => {}
                Role::GenericContainer if s(el.p("role")).is_none() => {}
                _ => return false,
            }
            at = el.parent;
        }
        false
    }

    pub(crate) fn role(&self, e: K) -> Role {
        let Some(el) = self.el(e) else { return Role::GenericContainer };
        if el.text.is_some() {
            return Role::Label;
        }
        if let Some(r) = s(el.p("role")) {
            for word in r.split_whitespace() {
                if let Some(role) = aria_role(word, el) {
                    // an option outside a listbox (or a combobox's popup, or a group in one) is out of its context:
                    // Chrome leaves it a plain container
                    if role == Role::ListBoxOption && !self.in_option_context(e) {
                        return Role::GenericContainer;
                    }
                    return role;
                }
            }
        }
        let named = el.p("aria-label").is_some_and(|v| s(Some(v)).is_some_and(|t| !t.trim().is_empty())) || el.p("aria-labelledby").is_some();
        match el.tag.as_str() {
            "button" => Role::Button,
            "a" | "area" => {
                if el.p("href").is_some() {
                    Role::Link
                } else {
                    Role::GenericContainer
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Role::Heading,
            "p" => Role::Paragraph,
            "ul" | "ol" | "menu" => Role::List,
            "li" => Role::ListItem,
            "nav" => Role::Navigation,
            "main" => Role::Main,
            "header" | "footer" => {
                let scoped = self.ancestors(e).any(|a| SECTIONING.contains(&a.tag.as_str()) || matches!(s(a.p("role")).as_deref(), Some("article" | "complementary" | "main" | "navigation" | "region")));
                if scoped {
                    Role::GenericContainer
                } else if el.tag == "header" {
                    Role::Banner
                } else {
                    Role::ContentInfo
                }
            }
            "aside" => Role::Complementary,
            "section" => {
                if named {
                    Role::Region
                } else {
                    Role::GenericContainer
                }
            }
            "article" => Role::Article,
            "form" => Role::Form,
            "search" => Role::Search,
            "table" => Role::Table,
            "tr" => Role::Row,
            "th" => {
                if s(el.p("scope")).as_deref() == Some("row") {
                    Role::RowHeader
                } else {
                    Role::ColumnHeader
                }
            }
            "td" => Role::Cell,
            "caption" => Role::Caption,
            "img" => {
                if el.p("alt").and_then(|v| v.as_str()) == Some("") {
                    Role::GenericContainer
                } else {
                    Role::Image
                }
            }
            // an svg is a graphic (Chrome: role image), named or not
            "svg" => Role::Image,
            "input" => match el.ty().as_str() {
                "checkbox" => Role::CheckBox,
                "radio" => Role::RadioButton,
                "range" => Role::Slider,
                "number" => Role::SpinButton,
                "search" => Role::SearchInput,
                "button" | "submit" | "reset" | "image" => Role::Button,
                "email" => Role::EmailInput,
                "password" => Role::PasswordInput,
                "tel" => Role::PhoneNumberInput,
                "url" => Role::UrlInput,
                "color" => Role::ColorWell,
                _ => Role::TextInput,
            },
            "textarea" => Role::MultilineTextInput,
            "select" => {
                if truthy(el.p("multiple")) || num(el.p("size")).is_some_and(|n| n > 1.0) {
                    Role::ListBox
                } else {
                    Role::ComboBox
                }
            }
            "option" => Role::ListBoxOption,
            "progress" => Role::ProgressIndicator,
            "meter" => Role::Meter,
            "hr" => Role::Splitter,
            "dialog" => Role::Dialog,
            "details" | "fieldset" => Role::Group,
            "summary" => Role::DisclosureTriangle,
            "figure" => Role::Figure,
            "blockquote" => Role::Blockquote,
            "label" => {
                // a label around its control is only its control's name
                if self.descendants(e).any(|d| self.el(d).is_some_and(|x| LABELABLE.contains(&x.tag.as_str()))) {
                    Role::GenericContainer
                } else {
                    Role::Label
                }
            }
            "video" => Role::Video,
            "audio" => Role::Audio,
            "canvas" => Role::Canvas,
            "time" => Role::Time,
            _ => Role::GenericContainer,
        }
    }

    pub(crate) fn descendants(&self, e: K) -> impl Iterator<Item = K> + '_ {
        let mut stack: Vec<K> = self.el(e).map(|x| x.kids.iter().rev().copied().collect()).unwrap_or_default();
        std::iter::from_fn(move || {
            let d = stack.pop()?;
            if let Some(x) = self.el(d) {
                stack.extend(x.kids.iter().rev().copied());
            }
            Some(d)
        })
    }

    /// Out of the tree: display none, visibility hidden, the hidden attribute, aria-hidden (itself or an ancestor).
    pub(crate) fn hidden(&self, e: K) -> bool {
        let mut at = Some(e);
        while let Some(x) = at {
            let Some(el) = self.el(x) else { break };
            if truthy(el.p("hidden")) || s(el.p("aria-hidden")).as_deref() == Some("true") {
                return true;
            }
            if self.facts.display_none(x) {
                return true;
            }
            at = el.parent;
        }
        self.facts.invisible(e)
    }

    pub(crate) fn disabled(&self, e: K) -> bool {
        let Some(el) = self.el(e) else { return false };
        truthy(el.p("disabled")) || s(el.p("aria-disabled")).as_deref() == Some("true") || self.facts.natively_disabled(e)
    }

    /// A field's text (its editor's), a select's chosen option's text, a range's value.
    pub(crate) fn value(&self, e: K, role: Role) -> Option<String> {
        let el = self.el(e)?;
        if let Some(v) = self.facts.editor_value(e) {
            return Some(v);
        }
        match role {
            Role::ComboBox if el.tag == "select" => {
                let want = s(el.p("value")).or_else(|| s(el.p("defaultValue")));
                let opts: Vec<K> = self.descendants(e).filter(|d| self.el(*d).is_some_and(|x| x.tag == "option")).collect();
                let pick = opts.iter().copied().find(|o| want.is_some() && s(self.el(*o).and_then(|x| x.p("value"))) == want).or_else(|| opts.iter().copied().find(|o| truthy(self.el(*o).and_then(|x| x.p("selected"))))).or(opts.first().copied());
                pick.map(|o| collapse(&self.content(o, &mut vec![])))
            }
            _ => s(el.p("value")).or_else(|| s(el.p("defaultValue"))),
        }
    }

    /// The text of what's inside an element, as the name computation reads it (each part's own text alternative).
    pub(crate) fn content(&self, e: K, seen: &mut Vec<K>) -> String {
        let Some(el) = self.el(e) else { return String::new() };
        let mut out = String::new();
        for &k in &el.kids {
            let Some(kel) = self.el(k) else { continue };
            if let Some(t) = &kel.text {
                out.push_str(t);
                continue;
            }
            if self.hidden(k) {
                continue;
            }
            let part = self.alt(k, true, seen);
            let block = self.facts.block(k).unwrap_or_else(|| !matches!(kel.tag.as_str(), "span" | "a" | "b" | "i" | "em" | "strong" | "code" | "small" | "abbr" | "kbd" | "label" | "img" | "svg" | "text" | "sub" | "sup" | "mark" | "s" | "u" | "q" | "time"));
            if block {
                out.push(' ');
            }
            out.push_str(&part);
            if block {
                out.push(' ');
            }
        }
        out
    }

    /// accname: an element's text alternative (`inside`: as part of another's name).
    pub(crate) fn alt(&self, e: K, inside: bool, seen: &mut Vec<K>) -> String {
        if seen.contains(&e) {
            return String::new();
        }
        seen.push(e);
        let Some(el) = self.el(e) else { return String::new() };
        if let Some(t) = &el.text {
            return t.clone();
        }
        let role = self.role(e);
        // aria-labelledby (not followed again from inside another's)
        if !inside && let Some(ids) = s(el.p("aria-labelledby")) {
            let parts: Vec<String> = ids.split_whitespace().filter_map(|id| self.by_id(e, id)).map(|r| collapse(&self.alt_ref(r, seen))).filter(|t| !t.is_empty()).collect();
            if !parts.is_empty() {
                return parts.join(" ");
            }
        }
        if let Some(l) = s(el.p("aria-label")).filter(|l| !l.trim().is_empty()) {
            return l;
        }
        // an embedded control inside another's name: its value
        if inside && is_field(role) {
            return self.value(e, role).unwrap_or_default();
        }
        // the element's own
        match el.tag.as_str() {
            "img" | "area" => {
                if let Some(a) = s(el.p("alt")) {
                    return a;
                }
            }
            "input" if matches!(el.ty().as_str(), "button" | "submit" | "reset") => {
                return s(el.p("value")).unwrap_or_else(|| match el.ty().as_str() {
                    "submit" => "Submit".into(),
                    "reset" => "Reset".into(),
                    _ => String::new(),
                });
            }
            "svg" => {
                if let Some(t) = el.title.clone().filter(|t| !t.trim().is_empty()) {
                    return t;
                }
            }
            _ => {}
        }
        if LABELABLE.contains(&el.tag.as_str()) && !inside {
            let mut labels = vec![];
            if let Some(id) = s(el.p("id")) {
                let root = self.roots.get(&e).copied();
                for (x, xel) in &self.els {
                    if xel.tag == "label" && self.roots.get(x).copied() == root && s(xel.p("htmlFor")).or_else(|| s(xel.p("for"))).as_deref() == Some(id.as_str()) {
                        labels.push(*x);
                    }
                }
            }
            if let Some(a) = self.ancestor_entity(e, |x| x.tag == "label") {
                labels.push(a);
            }
            let text: Vec<String> = labels.into_iter().map(|l| collapse(&self.content(l, seen))).filter(|t| !t.is_empty()).collect();
            if !text.is_empty() {
                return text.join(" ");
            }
        }
        if from_content(role) || inside {
            let t = collapse(&self.content(e, seen));
            if !t.is_empty() {
                return t;
            }
        }
        if !inside {
            if let Some(t) = s(el.p("title")).filter(|t| !t.trim().is_empty()) {
                return t;
            }
            if is_field(role) && let Some(p) = s(el.p("placeholder")).or_else(|| s(el.p("aria-placeholder"))) {
                return p;
            }
        }
        String::new()
    }

    /// A referenced element's text alternative (aria-labelledby): its aria-label or its content, hidden or not.
    pub(crate) fn alt_ref(&self, e: K, seen: &mut Vec<K>) -> String {
        let Some(el) = self.el(e) else { return String::new() };
        if let Some(l) = s(el.p("aria-label")).filter(|l| !l.trim().is_empty()) {
            return l;
        }
        self.content(e, seen)
    }

    pub(crate) fn ancestor_entity(&self, e: K, f: impl Fn(&El<K>) -> bool) -> Option<K> {
        let mut at = self.el(e)?.parent;
        while let Some(p) = at {
            let el = self.el(p)?;
            if f(el) {
                return Some(p);
            }
            at = el.parent;
        }
        None
    }
}
