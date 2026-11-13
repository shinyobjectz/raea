//! What the page's tests build documents with.
use blitz_dom::{LocalName, QualName, ns};

/// An HTML element's name.
pub fn q(local: &str) -> QualName {
    QualName { prefix: None, ns: ns!(html), local: LocalName::from(local) }
}

/// An attribute, as `name="v"`.
pub fn attr(local: &str, v: &str) -> blitz_dom::Attribute {
    blitz_dom::Attribute { name: QualName { prefix: None, ns: ns!(), local: LocalName::from(local) }, value: v.into() }
}
