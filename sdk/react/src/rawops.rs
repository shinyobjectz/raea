//! ZeroHeap Z4: React's per-frame `update` ops read where they lie in the batch's JSON, without building a
//! serde_json::Value for each (a map, a String per key and per string value: pong's four small updates a frame were ~64
//! allocations). A batch is split into its ops' raw text; an `update` is read here, and every other op (creates,
//! structure, text) is parsed whole as before. Props are merged into the node's kept props by value: a key or a string
//! is allocated only when it's new or changed.

use std::borrow::Cow;

use serde::de::{Deserializer as _, MapAccess, SeqAccess, Visitor};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

/// One op of a batch: an `update` read in place, or any other op parsed whole
pub(crate) enum Op<'a> {
    Update(Update<'a>),
    Full(Value),
}

/// An `update`: the node, its kind, its changed props (a JSON object), and the props and style fields it unsets
pub(crate) struct Update<'a> {
    pub id: u64,
    pub kind: &'a str,
    pub props: &'a RawValue,
    pub unset: Option<&'a RawValue>,
    pub style_unset: Option<&'a RawValue>,
}

#[derive(serde::Deserialize)]
struct Head<'a> {
    #[serde(borrow)]
    op: &'a str,
    #[serde(default)]
    id: Option<u64>,
    #[serde(borrow, default)]
    kind: Option<&'a str>,
    #[serde(borrow, default)]
    props: Option<&'a RawValue>,
    #[serde(borrow, default)]
    unset: Option<&'a RawValue>,
    #[serde(borrow, default, rename = "styleUnset")]
    style_unset: Option<&'a RawValue>,
}

/// A batch's ops (None: not an array of ops)
pub(crate) fn ops(json: &str) -> Option<Vec<Op<'_>>> {
    let raws: Vec<&RawValue> = serde_json::from_str(json).ok()?;
    Some(raws.into_iter().map(op).collect())
}

fn op(raw: &RawValue) -> Op<'_> {
    if let Ok(h) = serde_json::from_str::<Head>(raw.get()) {
        if h.op == "update" {
            if let (Some(id), Some(kind), Some(props)) = (h.id, h.kind, h.props) {
                if is_object(props) {
                    return Op::Update(Update { id, kind, props, unset: h.unset, style_unset: h.style_unset });
                }
            }
        }
    }
    Op::Full(serde_json::from_str(raw.get()).unwrap_or(Value::Null))
}

pub(crate) fn is_object(raw: &RawValue) -> bool {
    raw.get().trim_start().starts_with('{')
}

/// A key or string as written: borrowed, or owned where it had escapes
struct Str<'a>(Cow<'a, str>);

impl<'de> serde::Deserialize<'de> for Str<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Str<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }
            fn visit_borrowed_str<E>(self, s: &'de str) -> Result<Self::Value, E> {
                Ok(Str(Cow::Borrowed(s)))
            }
            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E> {
                Ok(Str(Cow::Owned(s.to_string())))
            }
            fn visit_string<E>(self, s: String) -> Result<Self::Value, E> {
                Ok(Str(Cow::Owned(s)))
            }
        }
        d.deserialize_str(V)
    }
}

/// Each (key, raw value) of a JSON object, in order, with nothing allocated for keys without escapes
pub(crate) fn each_entry<'a>(obj: &'a RawValue, mut f: impl FnMut(&str, &'a RawValue)) {
    struct V<'f, F>(&'f mut F);
    impl<'de, 'f, F: FnMut(&str, &'de RawValue)> Visitor<'de> for V<'f, F> {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an object")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<(), A::Error> {
            while let Some(k) = m.next_key::<Str<'de>>()? {
                let v: &'de RawValue = m.next_value()?;
                (self.0)(&k.0, v);
            }
            Ok(())
        }
    }
    let mut d = serde_json::Deserializer::from_str(obj.get());
    let _ = d.deserialize_map(V(&mut f));
}

/// Each string of a JSON array of strings, in order
pub(crate) fn each_str(arr: &RawValue, mut f: impl FnMut(&str)) {
    struct V<'f, F>(&'f mut F);
    impl<'de, 'f, F: FnMut(&str)> Visitor<'de> for V<'f, F> {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an array")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<(), A::Error> {
            while let Some(k) = s.next_element::<Str<'de>>()? {
                (self.0)(&k.0);
            }
            Ok(())
        }
    }
    let mut d = serde_json::Deserializer::from_str(arr.get());
    let _ = d.deserialize_seq(V(&mut f));
}

/// `map[k] = raw`, as merge's put does it with the parsed value; true when that changed the map. The value already
/// there is updated in place where it has the same shape (scalars read without allocating, a string's buffer reused,
/// arrays element by element, objects key by key); a new key, or a different shape, is parsed whole.
pub(crate) fn put_raw(map: &mut Map<String, Value>, k: &str, raw: &RawValue) -> bool {
    match map.get_mut(k) {
        Some(slot) => match update_value(slot, raw) {
            Some(changed) => changed,
            None => {
                let v: Value = serde_json::from_str(raw.get()).unwrap_or(Value::Null);
                if *slot == v {
                    false
                } else {
                    *slot = v;
                    true
                }
            }
        },
        None => {
            map.insert(k.to_string(), serde_json::from_str(raw.get()).unwrap_or(Value::Null));
            true
        }
    }
}

/// `slot = raw` in place where the shape is the same: Some(changed); None where it isn't (the caller replaces it whole;
/// what was updated before that is overwritten then)
fn update_value(slot: &mut Value, raw: &RawValue) -> Option<bool> {
    let t = raw.get().trim_start();
    match t.as_bytes().first()? {
        b'"' => {
            // a backslash means escapes (a failed borrowed parse allocates its error, so it isn't tried)
            let plain = if t.contains('\\') { None } else { serde_json::from_str::<&str>(t).ok() };
            let s: Cow<str> = match plain {
                Some(s) => Cow::Borrowed(s),
                // escaped (a JSON document carried as a string: 3D parts' data-props): compared as it's decoded, and
                // decoded into the old string's buffer only when it differs
                None => {
                    let lit = literal(t)?;
                    return match slot {
                        Value::String(old) if JsonChars::new(lit).eq(old.chars()) && JsonChars::new(lit).ok() => Some(false),
                        Value::String(old) => {
                            old.clear();
                            old.extend(JsonChars::new(lit));
                            Some(true)
                        }
                        _ => {
                            *slot = Value::String(serde_json::from_str::<String>(t).ok()?);
                            Some(true)
                        }
                    };
                }
            };
            match slot {
                Value::String(old) if old.as_str() == s.as_ref() => Some(false),
                Value::String(old) => {
                    old.clear();
                    old.push_str(&s);
                    Some(true)
                }
                _ => {
                    *slot = Value::String(s.into_owned());
                    Some(true)
                }
            }
        }
        b'{' => {
            let Value::Object(old) = slot else { return None };
            let (mut changed, mut n, mut ok) = (false, 0usize, true);
            each_entry(raw, |k, v| {
                if !ok {
                    return;
                }
                n += 1;
                match old.get_mut(k) {
                    Some(s) => match update_value(s, v) {
                        Some(c) => changed |= c,
                        None => ok = false,
                    },
                    None => {
                        old.insert(k.to_string(), serde_json::from_str(v.get()).unwrap_or(Value::Null));
                        changed = true;
                    }
                }
            });
            // keys the old object has that the new one doesn't: not the same shape
            (ok && old.len() == n).then_some(changed)
        }
        b'[' => {
            let Value::Array(old) = slot else { return None };
            let (mut changed, mut n, mut ok) = (false, 0usize, true);
            each_raw(raw, |v| {
                if !ok {
                    return;
                }
                match old.get_mut(n) {
                    Some(s) => match update_value(s, v) {
                        Some(c) => changed |= c,
                        None => ok = false,
                    },
                    None => ok = false,
                }
                n += 1;
            });
            (ok && old.len() == n).then_some(changed)
        }
        b't' | b'f' | b'n' | b'-' | b'0'..=b'9' => {
            let v = match t.as_bytes()[0] {
                b't' => Value::Bool(true),
                b'f' => Value::Bool(false),
                b'n' => Value::Null,
                _ => Value::Number(serde_json::from_str::<serde_json::Number>(t).ok()?),
            };
            if *slot == v {
                Some(false)
            } else {
                *slot = v;
                Some(true)
            }
        }
        _ => None,
    }
}

/// A JSON string literal's body (between its quotes), when `t` is one
fn literal(t: &str) -> Option<&str> {
    let t = t.trim_end();
    (t.len() >= 2 && t.starts_with('"') && t.ends_with('"')).then(|| &t[1..t.len() - 1])
}

/// The characters a JSON string literal's body decodes to (its escapes undone), without allocating
struct JsonChars<'a> {
    s: std::str::Chars<'a>,
    bad: bool,
}

impl<'a> JsonChars<'a> {
    fn new(body: &'a str) -> JsonChars<'a> {
        JsonChars { s: body.chars(), bad: false }
    }
    /// The whole body decodes (consumes the iterator)
    fn ok(mut self) -> bool {
        while self.next().is_some() {}
        !self.bad
    }
    fn hex4(&mut self) -> Option<u32> {
        let mut v = 0;
        for _ in 0..4 {
            v = v * 16 + self.s.next()?.to_digit(16)?;
        }
        Some(v)
    }
}

impl Iterator for JsonChars<'_> {
    type Item = char;
    fn next(&mut self) -> Option<char> {
        let c = self.s.next()?;
        if c != '\\' {
            return Some(c);
        }
        let out = match self.s.next() {
            Some('"') => Some('"'),
            Some('\\') => Some('\\'),
            Some('/') => Some('/'),
            Some('b') => Some('\u{8}'),
            Some('f') => Some('\u{c}'),
            Some('n') => Some('\n'),
            Some('r') => Some('\r'),
            Some('t') => Some('\t'),
            Some('u') => self.hex4().and_then(|hi| {
                if (0xD800..0xDC00).contains(&hi) {
                    // a surrogate pair: the low half follows as its own escape
                    if self.s.next() != Some('\\') || self.s.next() != Some('u') {
                        return None;
                    }
                    let lo = self.hex4()?;
                    char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo.checked_sub(0xDC00)?))
                } else {
                    char::from_u32(hi)
                }
            }),
            _ => None,
        };
        if out.is_none() {
            self.bad = true;
        }
        out
    }
}

/// Each raw element of a JSON array, in order
fn each_raw<'a>(arr: &'a RawValue, mut f: impl FnMut(&'a RawValue)) {
    struct V<'f, F>(&'f mut F);
    impl<'de, 'f, F: FnMut(&'de RawValue)> Visitor<'de> for V<'f, F> {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an array")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<(), A::Error> {
            while let Some(v) = s.next_element::<&'de RawValue>()? {
                (self.0)(v);
            }
            Ok(())
        }
    }
    let mut d = serde_json::Deserializer::from_str(arr.get());
    let _ = d.deserialize_seq(V(&mut f));
}

/// merge, with the update read in place: its props into `props` (a style object field by field), then what it unsets
pub(crate) fn merge_update(props: &mut Map<String, Value>, u: &Update) {
    each_entry(u.props, |k, v| {
        if k == "style" && is_object(v) {
            if let Some(old) = props.get_mut("style").and_then(|s| s.as_object_mut()) {
                each_entry(v, |sk, sv| {
                    put_raw(old, sk, sv);
                });
                return;
            }
        }
        put_raw(props, k, v);
    });
    if let Some(un) = u.unset {
        each_str(un, |k| {
            props.remove(k);
        });
    }
    if let Some(gone) = u.style_unset {
        if let Some(style) = props.get_mut("style").and_then(|s| s.as_object_mut()) {
            each_str(gone, |k| {
                style.remove(k);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JsonChars decodes as serde_json does
    #[test]
    fn json_chars_match_serde() {
        for lit in [r#""plain""#, r#""a\"b\\c\/d""#, r#""\n\t\r\b\f""#, r#""\u00e9\u4e2d""#, r#""\ud83d\ude00 x""#, r#""{\"position\":[0,7.5]}""#] {
            let want: String = serde_json::from_str(lit).unwrap();
            let got: String = JsonChars::new(literal(lit).unwrap()).collect();
            assert_eq!(got, want, "{lit}");
            assert!(JsonChars::new(literal(lit).unwrap()).ok());
        }
    }

    /// An update read in place leaves the props as merge leaves them with the parsed op
    #[test]
    fn merge_update_matches_merge() {
        let start: Value = serde_json::json!({"style": {"left": 1, "top": "2px", "color": "red"}, "data-x": "a", "esc": "{\"p\":[1,2]}", "esc2": "{\"p\":[1,2]}", "n": 3, "gone": true, "pos": [1, 2, "y"], "obj": {"a": 1, "b": {"c": [2]}}, "shrink": [1, 2], "grow": {"a": 1}});
        let ops = r#"[{"op":"update","id":7,"kind":"div","props":{"style":{"left":5,"top":"2px","opacity":0.5},"data-x":"b\"q","esc":"{\"p\":[1,2]}","esc2":"{\"p\":[1,3]}","n":3.25,"new":null,"arr":[1,2],"pos":[4,5.5,"x"],"obj":{"a":1,"b":{"c":[1]}},"shrink":[1],"grow":{"a":1,"z":2}},"unset":["gone"],"styleUnset":["color"]}]"#;
        let parsed: Value = serde_json::from_str(ops).unwrap();
        let op = &parsed[0];
        let mut a = start.as_object().unwrap().clone();
        crate::merge(&mut a, &op["props"], op.get("unset"));
        if let (Some(style), Some(gone)) = (a.get_mut("style").and_then(|s| s.as_object_mut()), op["styleUnset"].as_array()) {
            for k in gone.iter().filter_map(|k| k.as_str()) {
                style.remove(k);
            }
        }
        let mut b = start.as_object().unwrap().clone();
        let read = super::ops(ops).unwrap();
        let Op::Update(u) = &read[0] else { panic!("not read in place") };
        assert_eq!((u.id, u.kind), (7, "div"));
        merge_update(&mut b, u);
        assert_eq!(a, b);
    }
}
