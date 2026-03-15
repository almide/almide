/// Shared rendering primitives for all JS-family targets.
///
/// Input:    TsIR Expr, Stmt, Pattern, Type, MatchArm
/// Output:   String fragments (appended to &mut String)
/// Owns:     expression/statement/pattern/type → string conversion
/// Does NOT: program-level structure (render_ts/render_js/render_npm own that)
///
/// Every target-specific renderer calls these functions.
/// This file must remain target-agnostic — no js_mode flags.

use super::ts_ir::*;

macro_rules! w {
    ($o:expr, $($arg:tt)*) => { $o.push_str(&format!($($arg)*)) };
}

pub fn ind(d: usize) -> String { "  ".repeat(d) }

pub fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s))
}

// ── Expression rendering ─────────────────────────────────────────

pub fn expr(o: &mut String, e: &Expr, d: usize) {
    match e {
        Expr::Int(v) => w!(o, "{}", v),
        Expr::BigInt(v) => w!(o, "{}n", v),
        Expr::Float(v) => w!(o, "{}", v),
        Expr::Str(s) => o.push_str(&json_string(s)),
        Expr::Bool(v) => o.push_str(if *v { "true" } else { "false" }),
        Expr::Null => o.push_str("null"),
        Expr::Undefined => o.push_str("undefined"),
        Expr::Var(n) => o.push_str(n),

        Expr::BinOp { op, left, right } => { o.push('('); expr(o, left, d); o.push(' '); o.push_str(op); o.push(' '); expr(o, right, d); o.push(')'); }
        Expr::UnOp { op, operand } => { o.push_str(op); o.push('('); expr(o, operand, d); o.push(')'); }

        Expr::Call { func, args } => { expr(o, func, d); o.push('('); comma_exprs(o, args, d); o.push(')'); }
        Expr::MethodCall { recv, method, args } => { expr(o, recv, d); o.push('.'); o.push_str(method); o.push('('); comma_exprs(o, args, d); o.push(')'); }
        Expr::New { class, args } => { o.push_str("new "); o.push_str(class); o.push('('); comma_exprs(o, args, d); o.push(')'); }

        Expr::Ternary { cond, then, else_ } => {
            o.push('('); expr(o, cond, d); o.push_str(" ? "); expr(o, then, d); o.push_str(" : "); expr(o, else_, d); o.push(')');
        }
        Expr::Match { subject, arms, has_err_arm } => render_match(o, subject, arms, *has_err_arm, d),
        Expr::Block { stmts, tail } => {
            o.push_str("{\n");
            for s in stmts { stmt(o, s, d + 1); }
            if let Some(t) = tail { o.push_str(&ind(d + 1)); o.push_str("return "); expr(o, t, d + 1); o.push_str(";\n"); }
            o.push_str(&ind(d)); o.push('}');
        }
        Expr::Iife(inner) => { o.push_str("(() => "); expr(o, inner, d); o.push_str(")()"); }

        Expr::For { binding, iter, body } => {
            o.push_str("for (const "); o.push_str(binding); o.push_str(" of "); expr(o, iter, d); o.push_str(") {\n");
            for s in body { stmt(o, s, d + 1); }
            o.push_str(&ind(d)); o.push('}');
        }
        Expr::ForRange { binding, start, end, inclusive, body } => {
            let cmp = if *inclusive { "<=" } else { "<" };
            o.push_str("for (let "); o.push_str(binding); o.push_str(" = "); expr(o, start, d);
            w!(o, "; {} {} ", binding, cmp); expr(o, end, d);
            w!(o, "; {}++) {{\n", binding);
            for s in body { stmt(o, s, d + 1); }
            o.push_str(&ind(d)); o.push('}');
        }
        Expr::While { cond, body } => {
            o.push_str("while ("); expr(o, cond, d); o.push_str(") {\n");
            for s in body { stmt(o, s, d + 1); }
            o.push_str(&ind(d)); o.push('}');
        }
        Expr::DoLoop { body } => {
            o.push_str("while (true) {\n");
            for s in body { stmt(o, s, d + 1); }
            o.push_str(&ind(d)); o.push('}');
        }
        Expr::Break => o.push_str("break"),
        Expr::Continue => o.push_str("continue"),
        Expr::Return(v) => { o.push_str("return"); if let Some(e) = v { o.push(' '); expr(o, e, d); } }
        Expr::Throw(e) => { o.push_str("throw "); expr(o, e, d); }

        Expr::ResultOk(v) => { o.push_str("{ ok: true, value: "); expr(o, v, d); o.push_str(" }"); }
        Expr::ResultErr(e) => { o.push_str("{ ok: false, error: "); expr(o, e, d); o.push_str(" }"); }
        Expr::ThrowError(msg) => { o.push_str("__throw("); expr(o, msg, d); o.push(')'); }
        Expr::ThrowStructuredError { msg, value } => {
            o.push_str("(() => { const __e = new Error("); expr(o, msg, d);
            o.push_str("); __e.__almd_value = "); expr(o, value, d); o.push_str("; throw __e; })()");
        }

        Expr::Array(elems) => { o.push('['); comma_exprs(o, elems, d); o.push(']'); }
        Expr::MapNew(pairs) => {
            o.push_str("new Map([");
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 { o.push_str(", "); }
                o.push('['); expr(o, k, d); o.push_str(", "); expr(o, v, d); o.push(']');
            }
            o.push_str("])");
        }
        Expr::Object { fields } => { o.push_str("{ "); render_obj_fields(o, fields, d); o.push_str(" }"); }
        Expr::ObjectWithTag { tag, fields } => {
            o.push_str("{ tag: "); o.push_str(&json_string(tag));
            if !fields.is_empty() { o.push_str(", "); render_obj_fields(o, fields, d); }
            o.push_str(" }");
        }
        Expr::Spread { base, fields } => {
            o.push_str("{ ..."); expr(o, base, d);
            if !fields.is_empty() { o.push_str(", "); render_obj_fields(o, fields, d); }
            o.push_str(" }");
        }
        Expr::Tuple(elems) => { o.push('['); comma_exprs(o, elems, d); o.push(']'); }
        Expr::RangeArray { start, end, inclusive } => {
            if *inclusive {
                o.push_str("Array.from({length: ("); expr(o, end, d); o.push_str(") - ("); expr(o, start, d); o.push_str(") + 1}, (_, i) => ("); expr(o, start, d); o.push_str(") + i)");
            } else {
                o.push_str("Array.from({length: ("); expr(o, end, d); o.push_str(") - ("); expr(o, start, d); o.push_str(")}, (_, i) => ("); expr(o, start, d); o.push_str(") + i)");
            }
        }

        Expr::Field(e, f) => { expr(o, e, d); o.push('.'); o.push_str(f); }
        Expr::Index(e, i) => { expr(o, e, d); o.push('['); expr(o, i, d); o.push(']'); }
        Expr::TupleIdx(e, i) => { o.push('('); expr(o, e, d); w!(o, ")[{}]", i); }

        Expr::Arrow { params, body } => {
            o.push_str("(("); o.push_str(&params.join(", ")); o.push_str(") => "); expr(o, body, d); o.push(')');
        }
        Expr::Template { parts } => {
            o.push('`');
            for p in parts {
                match p {
                    TemplatePart::Lit(s) => o.push_str(s),
                    TemplatePart::Expr(e) => { o.push_str("${"); expr(o, e, d); o.push('}'); }
                }
            }
            o.push('`');
        }
        Expr::Await(e) => { o.push_str("await "); expr(o, e, d); }
        Expr::Raw(code) => o.push_str(code),
    }
}

// ── Statement rendering ──────────────────────────────────────────

pub fn stmt(o: &mut String, s: &Stmt, d: usize) {
    let i = ind(d);
    o.push_str(&i);
    match s {
        Stmt::Var { name, value } => { o.push_str("var "); o.push_str(name); o.push_str(" = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::Let { name, value } => { o.push_str("let "); o.push_str(name); o.push_str(" = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::Const { name, value } => { o.push_str("const "); o.push_str(name); o.push_str(" = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::VarDestructure { pattern, value } => { o.push_str("var "); o.push_str(pattern); o.push_str(" = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::Assign { target, value } => { o.push_str(target); o.push_str(" = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::FieldAssign { target, field, value } => { o.push_str(target); o.push('.'); o.push_str(field); o.push_str(" = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::IndexAssign { target, index, value } => { o.push_str(target); o.push('['); expr(o, index, d); o.push_str("] = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::MapSet { target, key, value } => { o.push_str(target); o.push_str(".set("); expr(o, key, d); o.push_str(", "); expr(o, value, d); o.push_str(");\n"); }
        Stmt::If { cond, body } => {
            o.push_str("if ("); expr(o, cond, d); o.push_str(") {\n");
            for s in body { stmt(o, s, d + 1); }
            o.push_str(&i); o.push_str("}\n");
        }
        Stmt::Expr(e) => { expr(o, e, d); o.push_str(";\n"); }
        Stmt::Comment(text) => { o.push_str(text); o.push('\n'); }
        Stmt::ResultUnwrapBind { name, value } => {
            w!(o, "const __r_{0} = ", name); expr(o, value, d);
            w!(o, "; if (!__r_{0}.ok) return __r_{0}; const {0} = __r_{0}.value;\n", name);
        }
        Stmt::TryCatchBind { name, value } => {
            w!(o, "var {}; try {{ {} = ", name, name); expr(o, value, d);
            w!(o, "; }} catch (__e) {{ {} = new __Err(__e instanceof Error ? __e.message : String(__e), __e.__almd_value); }}\n", name);
        }
        Stmt::ErrPropagate { name } => {
            w!(o, "if ({name} instanceof __Err) {{ const __re = new Error({name}.message); __re.__almd_value = {name}.value; throw __re; }}\n");
        }
    }
}

// ── Match rendering ──────────────────────────────────────────────

fn render_match(o: &mut String, subject: &Expr, arms: &[MatchArm], has_err_arm: bool, d: usize) {
    if has_err_arm { render_match_with_err(o, subject, arms, d); }
    else { render_match_normal(o, subject, arms, d); }
}

fn render_match_normal(o: &mut String, subject: &Expr, arms: &[MatchArm], d: usize) {
    o.push_str("((__m) => {\n");
    for arm in arms { render_arm(o, "__m", arm, d + 1); }
    if !arms.last().map_or(false, |a| a.guard.is_none() && is_unconditional(&a.pattern)) {
        o.push_str(&ind(d + 1)); o.push_str("throw new Error(\"match exhausted\");\n");
    }
    o.push_str(&ind(d)); o.push_str("})("); expr(o, subject, d); o.push(')');
}

fn render_match_with_err(o: &mut String, subject: &Expr, arms: &[MatchArm], d: usize) {
    let ok_arms: Vec<&MatchArm> = arms.iter().filter(|a| !matches!(&a.pattern, Pattern::Err(_))).collect();
    let err_arms: Vec<&MatchArm> = arms.iter().filter(|a| matches!(&a.pattern, Pattern::Err(_))).collect();
    // Result object: { ok: true, value } | { ok: false, error }
    o.push_str("(() => { const __m = "); expr(o, subject, d);
    o.push_str("; if (!__m.ok) { ");
    for arm in &err_arms {
        if let Pattern::Err(inner) = &arm.pattern { render_result_err_arm(o, inner, &arm.body, d); }
    }
    o.push_str("}\n");
    // ok arms: bind __m.value
    for arm in &ok_arms {
        let rewritten = MatchArm {
            pattern: if let Pattern::Ok(inner) = &arm.pattern { *inner.clone() } else { arm.pattern.clone() },
            guard: arm.guard.clone(),
            body: arm.body.clone(),
        };
        render_arm(o, "__m.value", &rewritten, d + 1);
    }
    if !ok_arms.last().map_or(false, |a| a.guard.is_none() && is_unconditional(&a.pattern)) {
        o.push_str(&ind(d + 1)); o.push_str("throw new Error(\"match exhausted\");\n");
    }
    o.push_str(&ind(d)); o.push_str("})()");
}

fn render_result_err_arm(o: &mut String, inner: &Pattern, body: &Expr, d: usize) {
    match inner {
        Pattern::Wild => { o.push_str("return "); expr(o, body, d); o.push_str("; "); }
        Pattern::Bind(name) => { w!(o, "{{ const {} = __m.error; return ", name); expr(o, body, d); o.push_str("; } "); }
        _ => { o.push_str("return "); expr(o, body, d); o.push_str("; "); }
    }
}

fn render_err_arm_inline(o: &mut String, inner: &Pattern, body: &Expr, d: usize) {
    match inner {
        Pattern::Wild => { o.push_str("return "); expr(o, body, d); o.push_str("; "); }
        Pattern::Bind(name) => { w!(o, "{{ const {} = __m.value; return ", name); expr(o, body, d); o.push_str("; } "); }
        _ => {
            let (cond, binds) = pattern_cond("__m.value", inner);
            let bs: String = binds.iter().map(|(n, v)| format!("const {} = {};", n, v)).collect::<Vec<_>>().join(" ");
            if cond == "true" {
                if bs.is_empty() { o.push_str("return "); expr(o, body, d); o.push_str("; "); }
                else { w!(o, "{{ {} return ", bs); expr(o, body, d); o.push_str("; } "); }
            } else { w!(o, "if ({}) {{ {} return ", cond, bs); expr(o, body, d); o.push_str("; } "); }
        }
    }
}

fn render_arm(o: &mut String, tmp: &str, arm: &MatchArm, d: usize) {
    let i = ind(d);
    let (cond, binds) = pattern_cond(tmp, &arm.pattern);
    let bs: String = binds.iter().map(|(n, v)| format!("const {} = {};", n, v)).collect::<Vec<_>>().join(" ");
    let mut body_str = String::new();
    expr(&mut body_str, &arm.body, d);

    if let Some(guard) = &arm.guard {
        let mut gs = String::new(); expr(&mut gs, guard, d);
        if !bs.is_empty() { w!(o, "{}{{ {} if ({} && {}) return {}; }}\n", i, bs, cond, gs, body_str); }
        else { w!(o, "{}if ({} && {}) return {};\n", i, cond, gs, body_str); }
    } else if cond == "true" && bs.is_empty() { w!(o, "{}return {};\n", i, body_str); }
    else if cond == "true" { w!(o, "{}{{ {} return {}; }}\n", i, bs, body_str); }
    else if !bs.is_empty() { w!(o, "{}if ({}) {{ {} return {}; }}\n", i, cond, bs, body_str); }
    else { w!(o, "{}if ({}) return {};\n", i, cond, body_str); }
}

pub fn pattern_cond(e: &str, pat: &Pattern) -> (String, Vec<(String, String)>) {
    match pat {
        Pattern::Wild => ("true".into(), vec![]),
        Pattern::Bind(n) => ("true".into(), vec![(n.clone(), e.into())]),
        Pattern::Literal(lit) => { let mut s = String::new(); expr(&mut s, lit, 0); (format!("{} === {}", e, s), vec![]) }
        Pattern::None => (format!("{} === null", e), vec![]),
        Pattern::Some(inner) => {
            let (ic, ib) = pattern_cond(e, inner);
            (if ic == "true" { format!("{} !== null", e) } else { format!("{} !== null && {}", e, ic) }, ib)
        }
        Pattern::Ok(inner) => pattern_cond(e, inner),
        Pattern::Err(_) => ("false".into(), vec![]),
        Pattern::Ctor { tag, args } => {
            if args.is_empty() { return (format!("{}?.tag === {}", e, json_string(tag)), vec![]); }
            let mut conds = vec![format!("{}?.tag === {}", e, json_string(tag))];
            let mut binds = vec![];
            for (field, pat) in args { let sub = format!("{}.{}", e, field); let (sc, sb) = pattern_cond(&sub, pat); if sc != "true" { conds.push(sc); } binds.extend(sb); }
            (conds.join(" && "), binds)
        }
        Pattern::RecordCtor { tag, fields } => {
            let mut conds = vec![format!("{}?.tag === {}", e, json_string(tag))];
            let mut binds = vec![];
            for (name, pat) in fields { let sub = format!("{}.{}", e, name); if let Some(p) = pat { let (sc, sb) = pattern_cond(&sub, p); if sc != "true" { conds.push(sc); } binds.extend(sb); } else { binds.push((name.clone(), sub)); } }
            (conds.join(" && "), binds)
        }
        Pattern::Tuple(elems) => {
            let mut conds = vec![]; let mut binds = vec![];
            for (i, p) in elems.iter().enumerate() { let sub = format!("{}[{}]", e, i); let (sc, sb) = pattern_cond(&sub, p); if sc != "true" { conds.push(sc); } binds.extend(sb); }
            (if conds.is_empty() { "true".into() } else { conds.join(" && ") }, binds)
        }
    }
}

pub fn is_unconditional(pat: &Pattern) -> bool {
    matches!(pat, Pattern::Wild | Pattern::Bind(_))
        || matches!(pat, Pattern::Ok(inner) if is_unconditional(inner))
}

// ── Type rendering ───────────────────────────────────────────────

pub fn ty(o: &mut String, t: &Type) {
    match t {
        Type::Number => o.push_str("number"),
        Type::String => o.push_str("string"),
        Type::Boolean => o.push_str("boolean"),
        Type::Void => o.push_str("void"),
        Type::Any => o.push_str("any"),
        Type::Null => o.push_str("null"),
        Type::Array(inner) => { ty(o, inner); o.push_str("[]"); }
        Type::Map(k, v) => { o.push_str("Map<"); ty(o, k); o.push_str(", "); ty(o, v); o.push('>'); }
        Type::Tuple(elems) => { o.push('['); for (i, t) in elems.iter().enumerate() { if i > 0 { o.push_str(", "); } ty(o, t); } o.push(']'); }
        Type::Object(fields) => { o.push_str("{ "); for (i, (n, t)) in fields.iter().enumerate() { if i > 0 { o.push_str(", "); } o.push_str(n); o.push_str(": "); ty(o, t); } o.push_str(" }"); }
        Type::Union(members) => { for (i, m) in members.iter().enumerate() { if i > 0 { o.push_str(" | "); } ty(o, m); } }
        Type::Fn { params, ret } => { o.push('('); for (i, (n, t)) in params.iter().enumerate() { if i > 0 { o.push_str(", "); } o.push_str(n); o.push_str(": "); ty(o, t); } o.push_str(") => "); ty(o, ret); }
        Type::Named(n) => o.push_str(n),
        Type::Nullable(inner) => { ty(o, inner); o.push_str(" | null"); }
    }
}

// ── Shared utilities ─────────────────────────────────────────────

pub fn comma_exprs(o: &mut String, exprs: &[Expr], d: usize) {
    for (i, e) in exprs.iter().enumerate() { if i > 0 { o.push_str(", "); } expr(o, e, d); }
}

pub fn render_obj_fields(o: &mut String, fields: &[(String, Expr)], d: usize) {
    for (i, (n, v)) in fields.iter().enumerate() { if i > 0 { o.push_str(", "); } o.push_str(n); o.push_str(": "); expr(o, v, d); }
}

// ── Shared top-level helpers ─────────────────────────────────────

pub fn function(o: &mut String, f: &Function, d: usize, with_types: bool) {
    let i = ind(d);
    o.push_str(&i);
    if f.is_async { o.push_str("async "); }
    o.push_str("function "); o.push_str(&f.name);
    o.push('(');
    for (idx, p) in f.params.iter().enumerate() {
        if idx > 0 { o.push_str(", "); }
        o.push_str(&p.name);
        if with_types { if let Some(t) = &p.ty { o.push_str(": "); ty(o, t); } }
    }
    o.push(')');
    if with_types { if let Some(t) = &f.ret { o.push_str(": "); ty(o, t); } }
    match &f.body {
        FnBody::Block { stmts, tail } => {
            o.push_str(" {\n");
            for s in stmts { stmt(o, s, d + 1); }
            if let Some(t) = tail { o.push_str(&ind(d + 1)); o.push_str("return "); expr(o, t, d + 1); o.push_str(";\n"); }
            o.push_str(&i); o.push('}');
        }
        FnBody::Expr(e) => {
            o.push_str(" {\n"); o.push_str(&ind(d + 1)); o.push_str("return "); expr(o, e, d + 1); o.push_str(";\n");
            o.push_str(&i); o.push('}');
        }
    }
}

pub fn module(o: &mut String, m: &Module, with_types: bool) {
    w!(o, "// module: {}\n", m.name);
    if m.name.contains('.') { w!(o, "{} = (() => {{\n", m.name); }
    else { w!(o, "const {} = (() => {{\n", m.name); }
    for td in &m.type_decls { type_decl(o, td, with_types); o.push('\n'); }
    for f in &m.functions { function(o, f, 0, with_types); o.push('\n'); }
    w!(o, "  return {{ {} }};\n", m.exports.join(", "));
    o.push_str("})();\n\n");
}

pub fn type_decl(o: &mut String, td: &TypeDecl, with_types: bool) {
    match td {
        TypeDecl::Interface { name, generics, fields } => {
            if !with_types { return; }
            let g = if generics.is_empty() { String::new() } else { format!("<{}>", generics.join(", ")) };
            w!(o, "interface {}{} {{\n", name, g);
            for (n, t) in fields { o.push_str("  "); o.push_str(n); o.push_str(": "); ty(o, t); o.push_str(";\n"); }
            o.push('}');
        }
        TypeDecl::TypeAlias { name, generics, target } => {
            if !with_types { return; }
            let g = if generics.is_empty() { String::new() } else { format!("<{}>", generics.join(", ")) };
            w!(o, "type {}{} = ", name, g); ty(o, target); o.push(';');
        }
        TypeDecl::VariantCtors(ctors) => {
            if !ctors.is_empty() { o.push_str("// variant constructors\n"); }
            for c in ctors { variant_ctor(o, c); o.push('\n'); }
        }
    }
}

pub fn variant_ctor(o: &mut String, c: &VariantCtor) {
    let tag = json_string(&c.name);
    match &c.kind {
        VariantCtorKind::Const => w!(o, "const {} = {{ tag: {} }};", c.name, tag),
        VariantCtorKind::GenericUnit => w!(o, "function {}() {{ return {{ tag: {} }}; }}", c.name, tag),
        VariantCtorKind::TupleCtor { arity } => {
            let params: Vec<String> = (0..*arity).map(|i| format!("_{}", i)).collect();
            let fields: Vec<String> = (0..*arity).map(|i| format!("_{i}: _{i}")).collect();
            w!(o, "function {}({}) {{ return {{ tag: {}, {} }}; }}", c.name, params.join(", "), tag, fields.join(", "));
        }
        VariantCtorKind::RecordCtor { fields } => {
            let fi: Vec<String> = fields.iter().map(|f| format!("{f}: {f}")).collect();
            w!(o, "function {}({}) {{ return {{ tag: {}, {} }}; }}", c.name, fields.join(", "), tag, fi.join(", "));
        }
    }
}

pub fn namespace_decls(o: &mut String, decls: &[String]) {
    for ns in decls {
        if ns.contains('.') { w!(o, "{} = {{}};\n", ns); }
        else { w!(o, "const {} = {{}};\n", ns); }
    }
}
