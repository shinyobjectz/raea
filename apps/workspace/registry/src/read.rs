//! What one .tsx or .ts file declares, read from its source without running it (docs/projects.feature PJ8): the
//! compositions its registerRoot registers, each <Composition>'s id, component, size, fps and length (a value written
//! out, or the expression that computes it); its default export; and what it imports. A prop written as a top-level
//! constant, or simple arithmetic on them (`const FPS = 30; … durationInFrames={15 * FPS}`), is read as its value.
use oxc::allocator::Allocator;
use oxc::ast::ast::*;
use oxc::ast_visit::{Visit, walk};
use oxc::parser::Parser;
use oxc::span::{GetSpan, SourceType};
use oxc::syntax::operator::{BinaryOperator, UnaryOperator};
use serde_json::{Value, json};
use std::collections::HashMap;

/// A <Composition> prop as written.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    Num(f64),
    Text(String),
    /// an expression only the render knows the value of
    Computed(String),
}

impl Prop {
    pub fn json(&self) -> Value {
        match self {
            Prop::Num(n) => json!(n),
            Prop::Text(t) => json!(t),
            Prop::Computed(e) => json!({ "computed": e }),
        }
    }
}

/// One registered <Composition>.
#[derive(Debug, Clone, Default)]
pub struct Registered {
    pub line: usize,
    pub id: Option<Prop>,
    pub component: Option<String>,
    pub width: Option<Prop>,
    pub height: Option<Prop>,
    pub fps: Option<Prop>,
    pub frames: Option<Prop>,
}

#[derive(Debug, Default)]
pub struct Facts {
    pub registered: Vec<Registered>,
    /// the default export, with its name when it has one (`export default function Promo`)
    pub default: Option<Option<String>>,
    /// each import or re-export's module, at its line
    pub imports: Vec<(String, usize)>,
    /// the names it imports from "raea" (a fix that registers its default export adds only what's missing)
    pub raea_names: Vec<String>,
    /// why it didn't parse (the first diagnostic), when it didn't
    pub broken: Option<String>,
    /// it calls registerRoot (its compositions may come from a helper the scan doesn't follow)
    pub roots: bool,
}

/// The file's facts; `tsx` for .tsx (JSX on), else plain TypeScript.
pub fn read(source: &str, tsx: bool) -> Facts {
    let alloc = Allocator::default();
    let st = if tsx { SourceType::tsx() } else { SourceType::ts() };
    let parsed = Parser::new(&alloc, source, st).parse();
    // 1-based: the newlines before it, plus one
    let line = |at: u32| source.as_bytes()[..at as usize].iter().filter(|&&b| b == b'\n').count() + 1;
    let mut f = Facts::default();
    if let Some(d) = parsed.diagnostics.first() {
        f.broken = Some(d.to_string());
        return f;
    }
    for s in &parsed.program.body {
        match s {
            Statement::ImportDeclaration(d) => {
                f.imports.push((d.source.value.to_string(), line(d.span.start)));
                if d.source.value == "raea" {
                    f.raea_names.extend(d.specifiers.iter().flatten().map(|sp| sp.local().name.to_string()));
                }
            }
            Statement::ExportFromDeclaration(d) => f.imports.push((d.source.value.to_string(), line(d.span.start))),
            Statement::ExportAllDeclaration(d) => f.imports.push((d.source.value.to_string(), line(d.span.start))),
            Statement::ExportNamedDeclaration(d) => {
                if let Some(sp) = d.specifiers.iter().find(|sp| sp.exported.name() == "default") {
                    f.default = Some(Some(sp.local.name().to_string()));
                }
            }
            Statement::ExportDefaultDeclaration(d) => {
                f.default = Some(match &d.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(x) => x.id.as_ref().map(|i| i.name.to_string()),
                    ExportDefaultDeclarationKind::ClassDeclaration(x) => x.id.as_ref().map(|i| i.name.to_string()),
                    ExportDefaultDeclarationKind::Identifier(x) => Some(x.name.to_string()),
                    _ => None,
                });
            }
            _ => {}
        }
    }
    let mut v = Compositions { src: source, depth: 0, roots: false, found: Vec::new(), consts: consts(&parsed.program) };
    v.visit_program(&parsed.program);
    f.roots = v.roots;
    f.registered = v.found.into_iter().map(|(at, mut r)| {
        r.line = line(at);
        r
    }).collect();
    f
}

/// The file's top-level numeric constants, in order: `const FPS = 30`, `const DUR = 15 * FPS`, and an object constant's
/// numeric properties as `NAME.prop` (`const edit = { fps: 30 }`).
fn consts(p: &Program) -> HashMap<String, f64> {
    let mut k = HashMap::new();
    for s in &p.body {
        let v = match s {
            Statement::VariableDeclaration(v) => v,
            Statement::ExportDeclaration(d) => match &d.declaration {
                Declaration::VariableDeclaration(v) => v,
                _ => continue,
            },
            _ => continue,
        };
        if v.kind != VariableDeclarationKind::Const {
            continue;
        }
        for d in &v.declarations {
            let (BindingPattern::BindingIdentifier(id), Some(init)) = (&d.id, &d.init) else { continue };
            match unwrap(init) {
                Expression::ObjectExpression(o) => {
                    for p in &o.properties {
                        if let ObjectPropertyKind::ObjectProperty(p) = p
                            && let (Some(key), Some(n)) = (p.key.static_name(), number(&p.value, &k))
                        {
                            k.insert(format!("{}.{key}", id.name), n);
                        }
                    }
                }
                e => {
                    if let Some(n) = number(e, &k) {
                        k.insert(id.name.to_string(), n);
                    }
                }
            }
        }
    }
    k
}

/// Through `x as T`, `x satisfies T` and parentheses.
fn unwrap<'a, 'b>(e: &'b Expression<'a>) -> &'b Expression<'a> {
    match e {
        Expression::ParenthesizedExpression(p) => unwrap(&p.expression),
        Expression::TSAsExpression(x) => unwrap(&x.expression),
        Expression::TSSatisfiesExpression(x) => unwrap(&x.expression),
        e => e,
    }
}

/// An expression's value when it's numbers and known constants: + - * /, unary minus, Math.round, floor and ceil.
fn number(e: &Expression, k: &HashMap<String, f64>) -> Option<f64> {
    match unwrap(e) {
        Expression::NumericLiteral(n) => Some(n.value),
        Expression::Identifier(id) => k.get(id.name.as_str()).copied(),
        Expression::StaticMemberExpression(m) => match &m.object {
            Expression::Identifier(o) => k.get(&format!("{}.{}", o.name, m.property.name)).copied(),
            _ => None,
        },
        Expression::UnaryExpression(u) if u.operator == UnaryOperator::UnaryNegation => number(&u.argument, k).map(|n| -n),
        Expression::BinaryExpression(b) => {
            let (l, r) = (number(&b.left, k)?, number(&b.right, k)?);
            match b.operator {
                BinaryOperator::Addition => Some(l + r),
                BinaryOperator::Subtraction => Some(l - r),
                BinaryOperator::Multiplication => Some(l * r),
                BinaryOperator::Division if r != 0.0 => Some(l / r),
                _ => None,
            }
        }
        Expression::CallExpression(c) => {
            let Expression::StaticMemberExpression(m) = &c.callee else { return None };
            let Expression::Identifier(o) = &m.object else { return None };
            let x = number(c.arguments.first()?.as_expression()?, k)?;
            match (o.name.as_str(), m.property.name.as_str()) {
                ("Math", "round") => Some(x.round()),
                ("Math", "floor") => Some(x.floor()),
                ("Math", "ceil") => Some(x.ceil()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Every <Composition> inside a registerRoot(...) call's argument.
struct Compositions<'s> {
    src: &'s str,
    depth: usize,
    /// it calls registerRoot, whether or not a <Composition> is written inside it
    roots: bool,
    found: Vec<(u32, Registered)>,
    consts: HashMap<String, f64>,
}

impl<'a> Visit<'a> for Compositions<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let root = matches!(&it.callee, Expression::Identifier(id) if id.name == "registerRoot");
        self.roots |= root;
        self.depth += root as usize;
        walk::walk_call_expression(self, it);
        self.depth -= root as usize;
    }

    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        let open = &it.opening_element;
        if self.depth > 0 && matches!(&open.name, JSXElementName::IdentifierReference(r) if r.name == "Composition") {
            let mut r = Registered::default();
            for a in &open.attributes {
                let JSXAttributeItem::Attribute(x) = a else { continue };
                let JSXAttributeName::Identifier(n) = &x.name else { continue };
                let value = x.value.as_ref().map(|v| self.prop(v));
                match n.name.as_str() {
                    "id" => r.id = value,
                    "width" => r.width = value,
                    "height" => r.height = value,
                    "fps" => r.fps = value,
                    "durationInFrames" => r.frames = value,
                    "component" => {
                        r.component = match value {
                            Some(Prop::Computed(e)) => Some(e),
                            Some(Prop::Text(t)) => Some(t),
                            _ => None,
                        }
                    }
                    _ => {}
                }
            }
            self.found.push((it.span.start, r));
        }
        walk::walk_jsx_element(self, it);
    }
}

impl Compositions<'_> {
    fn prop(&self, v: &JSXAttributeValue) -> Prop {
        match v {
            JSXAttributeValue::StringLiteral(l) => Prop::Text(l.value.to_string()),
            JSXAttributeValue::ExpressionContainer(c) => match c.expression.as_expression() {
                Some(Expression::NumericLiteral(n)) => Prop::Num(n.value),
                Some(Expression::StringLiteral(s)) => Prop::Text(s.value.to_string()),
                Some(Expression::TemplateLiteral(t)) if t.expressions.is_empty() && t.quasis.len() == 1 => Prop::Text(t.quasis[0].value.cooked.map(|c| c.to_string()).unwrap_or_default()),
                Some(e) if number(e, &self.consts).is_some() => Prop::Num(number(e, &self.consts).unwrap_or_default()),
                Some(e) => Prop::Computed(self.src[e.span().start as usize..e.span().end as usize].to_string()),
                None => Prop::Computed(String::new()),
            },
            other => Prop::Computed(self.src[other.span().start as usize..other.span().end as usize].to_string()),
        }
    }
}
