//! Z3 (docs/zeroheap.feature, lightspeed R1/R2): a live bundle's TSX compiled so a frame that changes nothing in a JSX
//! element allocates nothing for it. The Rust port of what was react/js/lite/compile.ts (removed with bun), on oxc instead of Bun.Transpiler and
//! acorn, for the in-process bundle (A3); what it writes is the same program, site for site.
//!
//! Each element site becomes a per-site builder (made at its first use, kept) and a call that hands the runtime the
//! site's dynamic values one by one: the runtime keeps, per component instance and occurrence (key, or order), the
//! values and the element last built from them, and hands back that same element while every value is the same
//! (sdk/react/js/lite/index.ts: __at, __v, __e). Constant parts (literal props, a style's constant fields, string
//! children) live in the builder, so they're never made again; a template literal's parts are values, so its string is
//! built only when one changes. Sites the compile can't read as plain props (a spread, a computed key, a method or
//! accessor) stay createElement calls, so every program means what it did; only how often an element is made changes.
use oxc::allocator::Allocator;
use oxc::ast::ast::*;
use oxc::ast_visit::{walk, Visit};
use oxc::codegen::Codegen;
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::{GetSpan, SourceType};
use oxc::transformer::{JsxRuntime, TransformOptions, Transformer};
use std::collections::HashSet;
use std::path::Path;

const FACTORY: &str = "__h";
const FRAGMENT: &str = "__F";
/// motion's pure functions (they read their array and object arguments and keep none of them)
const PURE: &[&str] = &["interpolate", "interpolateColors", "spring", "measureSpring"];

/// motion's modules, as an edit imports them (the SDK by name, or rewritten to its files) and as they import each other:
/// compile.ts's SDK pattern (the TypeScript original)
fn is_sdk(source: &str) -> bool {
    if source.starts_with("@raea/motion") || source == "motion" || source.starts_with("motion/") || source.contains("/motion/") || source.ends_with("/motion") {
        return true;
    }
    let Some(rest) = source.strip_prefix("./") else { return false };
    let stem = rest.strip_suffix(".tsx").or_else(|| rest.strip_suffix(".ts")).unwrap_or(rest);
    matches!(stem, "index" | "interpolate" | "spring" | "easing" | "color" | "colors")
}

/// `source` (TSX, or JSX when `jsx`) compiled: types stripped, JSX made `__h` calls, then each site compiled.
pub fn compile(source: &str, jsx: bool) -> Result<String, String> {
    let js = transpile(source, jsx)?;
    if !js.contains(&format!("{FACTORY}(")) {
        return Ok(js);
    }
    let alloc = Allocator::default();
    let parsed = Parser::new(&alloc, &js, SourceType::mjs()).parse();
    if let Some(e) = first_error(&parsed.diagnostics) {
        return Err(format!("the compiled module doesn't parse: {e}"));
    }
    let mut pure = HashSet::new();
    for s in &parsed.program.body {
        let Statement::ImportDeclaration(d) = s else { continue };
        if !is_sdk(&d.source.value) {
            continue;
        }
        for sp in d.specifiers.iter().flatten() {
            if let ImportDeclarationSpecifier::ImportSpecifier(sp) = sp {
                if PURE.contains(&sp.imported.name().as_str()) {
                    pure.insert(sp.local.name.to_string());
                }
            }
        }
    }
    let c = Compile { js: &js, pure };
    let body = c.gen(Node::Program(&parsed.program));
    Ok(format!("import {{ __h, __F, __at, __atk, __v, __e, __o }} from \"react\";\nconst __S = {{}};\n{body}"))
}

/// TSX to JS as Bun.Transpiler made it for compile.ts: classic JSX with `__h` and `__F`
fn transpile(source: &str, jsx: bool) -> Result<String, String> {
    let alloc = Allocator::default();
    let st = if jsx { SourceType::jsx() } else { SourceType::tsx() };
    let parsed = Parser::new(&alloc, source, st).parse();
    if let Some(e) = first_error(&parsed.diagnostics) {
        return Err(e.to_string());
    }
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let mut opts = TransformOptions::default();
    opts.jsx.runtime = JsxRuntime::Classic;
    opts.jsx.pragma = Some(FACTORY.into());
    opts.jsx.pragma_frag = Some(FRAGMENT.into());
    opts.jsx.pure = false;
    opts.jsx.development = false;
    let out = Transformer::new(&alloc, Path::new(if jsx { "module.jsx" } else { "module.tsx" }), &opts).build_with_scoping(scoping, &mut program);
    if let Some(e) = first_error(&out.diagnostics) {
        return Err(e.to_string());
    }
    Ok(Codegen::new().build(&program).code)
}

fn first_error(d: &[oxc::diagnostics::OxcDiagnostic]) -> Option<&oxc::diagnostics::OxcDiagnostic> {
    d.iter().find(|d| d.severity == oxc::diagnostics::Severity::Error)
}

/// What gen compiles: the program, an expression, or a call's argument
#[derive(Clone, Copy)]
enum Node<'n, 'a> {
    Program(&'n Program<'a>),
    Expr(&'n Expression<'a>),
    Arg(&'n Argument<'a>),
}

impl Node<'_, '_> {
    fn span(&self) -> oxc::span::Span {
        match self {
            Node::Program(p) => p.span,
            Node::Expr(e) => e.span(),
            Node::Arg(a) => a.span(),
        }
    }
}

struct Compile<'s> {
    js: &'s str,
    pure: HashSet<String>,
}

/// Finds the outermost sites under a node and writes each one's replacement as it finds it
struct Find<'c, 's> {
    c: &'c Compile<'s>,
    out: Vec<(u32, u32, String)>,
}

impl<'a> Visit<'a> for Find<'_, '_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if self.c.is_site(it) {
            self.out.push((it.span.start, it.span.end, self.c.site(it)));
        } else if self.c.is_pure(it) {
            self.out.push((it.span.start, it.span.end, self.c.pure_call(it)));
        } else {
            walk::walk_call_expression(self, it);
        }
    }
}

impl Compile<'_> {
    fn text(&self, s: oxc::span::Span) -> &str {
        &self.js[s.start as usize..s.end as usize]
    }

    /// The source of `x` with each outermost site under it compiled.
    fn gen(&self, x: Node<'_, '_>) -> String {
        let mut f = Find { c: self, out: Vec::new() };
        match x {
            Node::Program(p) => f.visit_program(p),
            Node::Expr(e) => f.visit_expression(e),
            Node::Arg(a) => f.visit_argument(a),
        }
        let span = x.span();
        f.out.sort_by_key(|s| s.0);
        let mut out = String::new();
        let mut at = span.start;
        for (start, end, text) in f.out {
            out.push_str(&self.js[at as usize..start as usize]);
            out.push_str(&text);
            at = end;
        }
        out.push_str(&self.js[at as usize..span.end as usize]);
        out
    }

    fn is_site(&self, c: &CallExpression) -> bool {
        matches!(&c.callee, Expression::Identifier(id) if id.name == FACTORY)
    }

    fn is_pure(&self, c: &CallExpression) -> bool {
        matches!(&c.callee, Expression::Identifier(id) if self.pure.contains(id.name.as_str())) && c.arguments.iter().any(|a| a.as_expression().is_some_and(cacheable))
    }

    /// A builder's text for a value: constants inline, plain objects, arrays and template literals taken apart, the
    /// rest a value (pushed to `values`, read back as r.v[i]).
    fn leaf(&self, values: &mut Vec<String>, x: &Expression) -> String {
        if constant(x) {
            return self.text(x.span()).to_string();
        }
        if let Expression::ObjectExpression(o) = x {
            if plain(o) {
                let fields: Vec<String> = o.properties.iter().map(|p| {
                    let ObjectPropertyKind::ObjectProperty(p) = p else { unreachable!("plain") };
                    format!("{}: {}", json(&key_name(&p.key)), self.leaf(values, &p.value))
                }).collect();
                return format!("{{{}}}", fields.join(", "));
            }
        }
        if let Expression::TemplateLiteral(t) = x {
            let mut s = String::from("`");
            for (i, q) in t.quasis.iter().enumerate() {
                s.push_str(&q.value.raw);
                if let Some(e) = t.expressions.get(i) {
                    s.push_str("${");
                    s.push_str(&self.leaf(values, e));
                    s.push('}');
                }
            }
            return s + "`";
        }
        values.push(self.gen(Node::Expr(x)));
        format!("r.v[{}]", values.len() - 1)
    }

    /// A pure call whose array or object literal arguments are each kept per occurrence: the same one again while its
    /// values are the same (interpolate(age, [0, life], [a, b], { extrapolateLeft: "clamp" }) made three a call).
    fn pure_call(&self, call: &CallExpression) -> String {
        let args: Vec<String> = call.arguments.iter().map(|a| {
            let Some(e) = a.as_expression().filter(|e| cacheable(e)) else { return self.gen(Node::Arg(a)) };
            let mut values = Vec::new();
            let shape = match e {
                Expression::ArrayExpression(arr) => {
                    let items: Vec<String> = arr.elements.iter().map(|el| self.leaf(&mut values, el.as_expression().expect("cacheable"))).collect();
                    format!("[{}]", items.join(", "))
                }
                _ => self.leaf(&mut values, e),
            };
            let b = builder(&format!("(r) => ({shape})"));
            format!("__o({})", chain(&format!("__at({b}, null)"), &values))
        }).collect();
        format!("{}({})", self.gen(Node::Expr(&call.callee)), args.join(", "))
    }

    fn site(&self, call: &CallExpression) -> String {
        let args = &call.arguments;
        let ty = args.first();
        let props = args.get(1).map(|a| a.as_expression());
        let kids = args.iter().skip(2);
        let ok_props = match props {
            None => true,
            Some(Some(Expression::NullLiteral(_))) => true,
            Some(Some(Expression::ObjectExpression(o))) => plain(o),
            _ => false,
        };
        let spread = |a: &Argument| matches!(a, Argument::SpreadElement(_));
        if ty.is_none_or(spread) || !ok_props || kids.clone().any(spread) {
            let all: Vec<String> = args.iter().map(|a| self.gen(Node::Arg(a))).collect();
            return format!("{FACTORY}({})", all.join(", "));
        }
        let mut values = Vec::new();
        let mut key_at = String::new();
        let mut fields = Vec::new();
        if let Some(Some(Expression::ObjectExpression(o))) = props {
            for p in &o.properties {
                let ObjectPropertyKind::ObjectProperty(p) = p else { unreachable!("plain") };
                let k = key_name(&p.key);
                if k == "key" {
                    key_at = self.key_of(&p.value);
                } else {
                    let v = self.leaf(&mut values, &p.value);
                    fields.push(format!("{}: {v}", json(&k)));
                }
            }
        }
        let kids: Vec<&Expression> = kids.map(|a| a.as_expression().expect("not spread")).collect();
        if kids.len() == 1 {
            let v = self.leaf(&mut values, kids[0]);
            fields.push(format!("\"children\": {v}"));
        } else if kids.len() > 1 {
            let vs: Vec<String> = kids.iter().map(|k| self.leaf(&mut values, k)).collect();
            fields.push(format!("\"children\": [{}]", vs.join(", ")));
        }
        let b = builder(&format!("(r) => ({{{}}})", fields.join(", ")));
        let key = if key_at.is_empty() { format!("__at({b}, null)") } else { key_at.replacen("$B", &b, 1) };
        format!("__e({}, {})", chain(&key, &values), self.gen(Node::Arg(ty.expect("checked"))))
    }

    /// A key's lookup: a literal prefix and a value ("s" + i, `s${i}`) looked up by the value, the string made only
    /// when the element is built; anything else as it is.
    fn key_of(&self, x: &Expression) -> String {
        let lit = |y: &Expression| matches!(y, Expression::StringLiteral(_));
        if let Expression::BinaryExpression(b) = x {
            if b.operator == BinaryOperator::Addition && lit(&b.left) && !lit(&b.right) {
                let Expression::StringLiteral(l) = &b.left else { unreachable!() };
                return format!("__atk($B, {}, {})", json(&l.value), self.gen(Node::Expr(&b.right)));
            }
        }
        if let Expression::TemplateLiteral(t) = x {
            if t.expressions.len() == 1 && t.quasis.get(1).is_some_and(|q| q.value.cooked.as_deref() == Some("")) {
                let first = t.quasis[0].value.cooked.as_deref().map(json).unwrap_or_else(|| "null".into());
                return format!("__atk($B, {first}, {})", self.gen(Node::Expr(&t.expressions[0])));
            }
        }
        format!("__at($B, {})", self.gen(Node::Expr(x)))
    }
}

/// An array or object literal a pure call's argument can be kept as (its values compared, as a site's)
fn cacheable(a: &Expression) -> bool {
    match a {
        Expression::ArrayExpression(arr) => arr.elements.iter().all(|e| !matches!(e, ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_))),
        Expression::ObjectExpression(o) => plain(o),
        _ => false,
    }
}

/// A plain prop object: identifier or string keys, plain values, nothing spread, computed or accessed.
fn plain(o: &ObjectExpression) -> bool {
    o.properties.iter().all(|p| match p {
        ObjectPropertyKind::ObjectProperty(p) => {
            p.kind == PropertyKind::Init && !p.computed && !p.method && matches!(p.key, PropertyKey::StaticIdentifier(_) | PropertyKey::StringLiteral(_))
        }
        ObjectPropertyKind::SpreadProperty(_) => false,
    })
}

fn key_name(k: &PropertyKey) -> String {
    match k {
        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
        PropertyKey::StringLiteral(s) => s.value.to_string(),
        _ => unreachable!("plain keys only"),
    }
}

fn constant(x: &Expression) -> bool {
    match x {
        Expression::StringLiteral(_) | Expression::NumericLiteral(_) | Expression::BooleanLiteral(_) | Expression::NullLiteral(_) | Expression::BigIntLiteral(_) => true,
        Expression::TemplateLiteral(t) => t.expressions.is_empty(),
        Expression::UnaryExpression(u) => matches!(u.operator, UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus) && matches!(u.argument, Expression::NumericLiteral(_)),
        _ => false,
    }
}

/// A site's builder, written where the site is (inside the component), made on its first use and kept in the module's
/// __S under a name from its text (FNV-1a over its UTF-16 code units, base 36, as compile.ts names it)
fn builder(text: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for u in text.encode_utf16() {
        h = (h ^ u as u32).wrapping_mul(0x0100_0193);
    }
    format!("(__S.b{} ??= {text})", base36(h))
}

fn chain(key: &str, values: &[String]) -> String {
    let mut rec = key.to_string();
    for (i, v) in values.iter().enumerate() {
        rec = format!("__v({rec}, {i}, {v})");
    }
    rec
}

fn base36(mut n: u32) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut d = Vec::new();
    while n > 0 {
        d.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(n % 36) as usize]);
        n /= 36;
    }
    d.reverse();
    String::from_utf8(d).unwrap()
}

/// JSON.stringify of a string
fn json(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            '\u{8}' => o.push_str("\\b"),
            '\u{c}' => o.push_str("\\f"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_builders_as_compile_ts() {
        // compile.ts: (__S.b${fnv1a(text).toString(36)} ??= text)
        assert_eq!(base36(0x811c_9dc5), "ztntfp");
        assert_eq!(json("a\"b\n"), "\"a\\\"b\\n\"");
    }

    #[test]
    fn compiles_sites() {
        let out = compile("export function A({ n }: { n: number }) { return <div style={{ opacity: n, color: \"red\" }}>hi {n}</div>; }", false).unwrap();
        assert!(out.starts_with("import { __h, __F, __at, __atk, __v, __e, __o } from \"react\";\nconst __S = {};\n"), "{out}");
        assert!(out.contains("__e(__v(__v(__at((__S.b"), "a site with two values: {out}");
        assert!(out.contains("\"color\": \"red\""), "constants in the builder: {out}");
        assert!(!out.contains(": number"), "types stripped: {out}");
    }

    #[test]
    fn keeps_spread_sites_as_calls() {
        let out = compile("export const B = (p: any) => <div {...p} />;", false).unwrap();
        assert!(out.contains("__h(\"div\""), "{out}");
        assert!(!out.contains("__e("), "{out}");
    }
}
