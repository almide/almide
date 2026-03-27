/// Almide code formatter: AST → formatted Almide source code.
///
/// Owns:     indentation, spacing, line breaks, comment preservation
/// Does NOT: parsing, type checking

use std::fmt::Write;
use crate::ast::*;
use crate::intern::Sym;

fn join_syms(syms: &[Sym], sep: &str) -> String {
    syms.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(sep)
}

/// Infallible write to String — suppresses unwrap() on write!/writeln!
macro_rules! w {
    ($dst:expr, $($arg:tt)*) => {{ let _ = write!($dst, $($arg)*); }};
}
macro_rules! wln {
    ($dst:expr, $($arg:tt)*) => {{ let _ = writeln!($dst, $($arg)*); }};
    ($dst:expr) => {{ let _ = writeln!($dst); }};
}

const INDENT: &str = "  ";

fn ind(depth: usize) -> String { INDENT.repeat(depth) }

fn is_short(expr: &Expr) -> bool {
    match expr {
        Expr::Int { .. } | Expr::Float { .. } | Expr::Bool { .. }
        | Expr::Unit { .. } | Expr::None { .. } | Expr::Hole { .. } | Expr::Placeholder { .. }
        | Expr::Ident { .. } | Expr::TypeName { .. } => true,
        Expr::String { value, .. } => value.len() < 40,
        Expr::Some { expr, .. } | Expr::Ok { expr, .. } | Expr::Err { expr, .. }
        | Expr::Paren { expr, .. } => is_short(expr),
        Expr::Tuple { elements, .. } => elements.len() <= 4 && elements.iter().all(is_short),
        Expr::Call { args, .. } => args.len() <= 2 && args.iter().all(is_short),
        Expr::IndexAccess { object, index, .. } => is_short(object) && is_short(index),
        Expr::Binary { left, right, .. } => is_short(left) && is_short(right),
        Expr::Unary { operand, .. } => is_short(operand),
        _ => false,
    }
}

fn comma_sep<T>(out: &mut String, items: &[T], f: impl Fn(&mut String, &T)) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 { out.push_str(", "); }
        f(out, item);
    }
}

pub fn format_program(program: &Program) -> String {
    let mut out = String::new();
    let cm = &program.comment_map;
    let mut ci = 0;
    let emit_comments = |out: &mut String, idx: &mut usize| {
        if let Some(comments) = cm.get(*idx) {
            for c in comments { wln!(out, "{c}"); }
        }
        *idx += 1;
    };
    for imp in &program.imports {
        if !out.is_empty() && ci == 0 { out.push('\n'); }
        emit_comments(&mut out, &mut ci);
        fmt_decl(&mut out, imp, 0);
        out.push('\n');
    }
    for decl in &program.decls {
        out.push('\n');
        emit_comments(&mut out, &mut ci);
        fmt_decl(&mut out, decl, 0);
        out.push('\n');
    }
    if let Some(comments) = cm.get(ci) {
        if !comments.is_empty() {
            out.push('\n');
            for c in comments { wln!(out, "{c}"); }
        }
    }
    out
}

fn fmt_vis(out: &mut String, vis: &Visibility) {
    match vis {
        Visibility::Local => out.push_str("local "),
        Visibility::Mod => out.push_str("mod "),
        Visibility::Public => {}
    }
}

fn fmt_generics(out: &mut String, params: &[GenericParam]) {
    out.push('[');
    comma_sep(out, params, |out, gp| {
        out.push_str(&gp.name);
        if let Some(ref sb) = gp.structural_bound {
            out.push_str(": "); fmt_type(out, sb, 0);
        } else if let Some(ref bounds) = gp.bounds {
            if !bounds.is_empty() { w!(out, ": {}", join_syms(bounds, " + ")); }
        }
    });
    out.push(']');
}

fn maybe_generics(out: &mut String, generics: &Option<Vec<GenericParam>>) {
    if let Some(gps) = generics { if !gps.is_empty() { fmt_generics(out, gps); } }
}

fn fmt_decl(out: &mut String, decl: &Decl, depth: usize) {
    let i = ind(depth);
    match decl {
        Decl::Module { path, .. } => { w!(out, "{i}module {}", join_syms(path, ".")); }
        Decl::Import { path, names, alias, .. } => {
            w!(out, "{i}import {}", join_syms(path, "."));
            if let Some(n) = names { w!(out, " ({})", join_syms(n, ", ")); }
            if let Some(a) = alias { w!(out, " as {a}"); }
        }
        Decl::Strict { mode, .. } => w!(out, "{i}strict \"{mode}\""),
        Decl::Type { name, ty, deriving, visibility, generics, .. } => {
            out.push_str(&i); fmt_vis(out, visibility);
            w!(out, "type {name}");
            maybe_generics(out, generics);
            if let Some(d) = deriving { if !d.is_empty() { w!(out, ": {}", join_syms(d, ", ")); } }
            out.push_str(" = "); fmt_type(out, ty, depth);
        }
        Decl::TopLet { name, ty, value, visibility, .. } => {
            out.push_str(&i); fmt_vis(out, visibility);
            w!(out, "let {name}");
            if let Some(te) = ty { out.push_str(": "); fmt_type(out, te, depth); }
            out.push_str(" = "); fmt_expr(out, value, depth);
        }
        Decl::Fn { name, effect, r#async, visibility, params, return_type, body, extern_attrs, generics, .. } => {
            for a in extern_attrs { wln!(out, "{i}@extern({}, \"{}\", \"{}\")", a.target, a.module, a.function); }
            out.push_str(&i); fmt_vis(out, visibility);
            if matches!(effect, Some(true)) { out.push_str("effect "); }
            if matches!(r#async, Some(true)) { out.push_str("async "); }
            w!(out, "fn {name}");
            maybe_generics(out, generics);
            out.push('(');
            comma_sep(out, params, |out, p| {
                w!(out, "{}: ", p.name); fmt_type(out, &p.ty, depth);
                if let Some(ref d) = p.default { out.push_str(" = "); fmt_expr(out, d, depth); }
            });
            out.push_str(") -> "); fmt_type(out, return_type, depth);
            if let Some(b) = body { out.push_str(" = "); fmt_expr(out, b, depth); }
        }
        Decl::Test { name, body, .. } => { w!(out, "{i}test \"{name}\" "); fmt_expr(out, body, depth); }
        Decl::Protocol { name, methods, .. } => {
            wln!(out, "{i}protocol {name} {{");
            let inner = "  ".repeat(depth + 1);
            for m in methods {
                let effect = if m.effect { "effect " } else { "" };
                let mut params_str = String::new();
                for (j, p) in m.params.iter().enumerate() {
                    if j > 0 { params_str.push_str(", "); }
                    params_str.push_str(&p.name);
                    params_str.push_str(": ");
                    fmt_type(&mut params_str, &p.ty, 0);
                }
                let mut ret_str = String::new();
                fmt_type(&mut ret_str, &m.return_type, 0);
                wln!(out, "{inner}{effect}fn {name}({params_str}) -> {ret_str}", name = m.name);
            }
            w!(out, "{i}}}");
        }
        Decl::Impl { trait_, for_, methods, .. } => {
            wln!(out, "{i}impl {trait_} for {for_} {{");
            for m in methods { fmt_decl(out, m, depth + 1); out.push('\n'); }
            w!(out, "{i}}}");
        }
    }
}

fn fmt_type(out: &mut String, ty: &TypeExpr, depth: usize) {
    match ty {
        TypeExpr::Simple { name } => out.push_str(name),
        TypeExpr::Generic { name, args } => {
            out.push_str(name); out.push('[');
            comma_sep(out, args, |out, a| fmt_type(out, a, depth));
            out.push(']');
        }
        TypeExpr::Record { fields } | TypeExpr::OpenRecord { fields } => {
            let open = matches!(ty, TypeExpr::OpenRecord { .. });
            out.push_str("{ ");
            comma_sep(out, fields, |out, f| { w!(out, "{}: ", f.name); fmt_type(out, &f.ty, depth); });
            if open { if !fields.is_empty() { out.push_str(", "); } out.push_str(".. "); }
            else { out.push(' '); }
            out.push('}');
        }
        TypeExpr::Fn { params, ret } => {
            out.push_str("fn(");
            comma_sep(out, params, |out, p| fmt_type(out, p, depth));
            out.push_str(") -> "); fmt_type(out, ret, depth);
        }
        TypeExpr::Tuple { elements } => {
            out.push('('); comma_sep(out, elements, |out, e| fmt_type(out, e, depth)); out.push(')');
        }
        TypeExpr::Union { members } => {
            for (i, m) in members.iter().enumerate() {
                if i > 0 { out.push_str(" | "); }
                fmt_type(out, m, depth);
            }
        }
        TypeExpr::Variant { cases } => {
            for (i, case) in cases.iter().enumerate() {
                if i > 0 { out.push_str(" | "); } else { out.push_str("| "); }
                match case {
                    VariantCase::Unit { name } => out.push_str(name),
                    VariantCase::Tuple { name, fields } => {
                        out.push_str(name); out.push('(');
                        comma_sep(out, fields, |out, f| fmt_type(out, f, depth));
                        out.push(')');
                    }
                    VariantCase::Record { name, fields } => {
                        w!(out, "{name} {{ ");
                        comma_sep(out, fields, |out, f| {
                            w!(out, "{}: ", f.name); fmt_type(out, &f.ty, depth);
                            if let Some(ref d) = f.default { out.push_str(" = "); fmt_expr(out, d, depth); }
                        });
                        out.push_str(" }");
                    }
                }
            }
        }
    }
}

fn fmt_expr(out: &mut String, expr: &Expr, depth: usize) {
    match expr {
        Expr::Int { raw, .. } => out.push_str(raw),
        Expr::Float { value, .. } => { let s = format!("{value}"); if s.contains('.') { out.push_str(&s); } else { out.push_str(&s); out.push_str(".0"); } }
        Expr::String { value, .. } => {
            let has_dquote = value.contains('"');
            let has_squote = value.contains('\'');
            let use_single = has_dquote && !has_squote;
            let quote = if use_single { '\'' } else { '"' };
            out.push(quote);
            let chars: Vec<char> = value.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\n' { out.push_str("\\n"); }
                else if ch == '\t' { out.push_str("\\t"); }
                else if ch == '\r' { out.push_str("\\r"); }
                else if ch == '\\' { out.push_str("\\\\"); }
                else if ch == quote { out.push('\\'); out.push(ch); }
                else if !use_single && ch == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                    // Only escape ${ in double-quote strings (single-quote has no interpolation)
                    out.push_str("\\${");
                    i += 2; continue;
                }
                else { out.push(ch); }
                i += 1;
            }
            out.push(quote);
        }
        Expr::InterpolatedString { parts, .. } => fmt_istring_parts(out, parts, depth),
        Expr::Bool { value, .. } => out.push_str(if *value { "true" } else { "false" }),
        Expr::Unit { .. } => out.push_str("()"),
        Expr::None { .. } => out.push_str("none"),
        Expr::Hole { .. } | Expr::Placeholder { .. } => out.push('_'),
        Expr::Error { .. } => out.push_str("/* error */"),
        Expr::Todo { message, .. } => if message.is_empty() { out.push_str("todo"); } else { w!(out, "todo(\"{message}\")"); },
        Expr::Some { expr: e, .. } => { out.push_str("some("); fmt_expr(out, e, depth); out.push(')'); }
        Expr::Ok { expr: e, .. } => { out.push_str("ok("); fmt_expr(out, e, depth); out.push(')'); }
        Expr::Err { expr: e, .. } => { out.push_str("err("); fmt_expr(out, e, depth); out.push(')'); }
        Expr::Ident { name, .. } | Expr::TypeName { name, .. } => out.push_str(name),
        Expr::Paren { expr: e, .. } => { out.push('('); fmt_expr(out, e, depth); out.push(')'); }
        Expr::Tuple { elements, .. } => { out.push('('); comma_sep(out, elements, |out, e| fmt_expr(out, e, depth)); out.push(')'); }
        Expr::List { elements, .. } => fmt_list(out, elements, depth),
        Expr::EmptyMap { .. } => out.push_str("[:]"),
        Expr::MapLiteral { entries, .. } => fmt_map(out, entries, depth),
        Expr::Record { name, fields, .. } => {
            if let Some(n) = name { w!(out, "{n} "); }
            if fields.is_empty() { out.push_str("{}"); }
            else { out.push_str("{ "); comma_sep(out, fields, |out, f| { w!(out, "{}: ", f.name); fmt_expr(out, &f.value, depth); }); out.push_str(" }"); }
        }
        Expr::SpreadRecord { base, fields, .. } => {
            out.push_str("{ ..."); fmt_expr(out, base, depth);
            for f in fields { w!(out, ", {}: ", f.name); fmt_expr(out, &f.value, depth); }
            out.push_str(" }");
        }
        Expr::Call { callee, args, type_args, named_args, .. } => {
            fmt_expr(out, callee, depth);
            if let Some(ta) = type_args { out.push('['); comma_sep(out, ta, |out, t| fmt_type(out, t, depth)); out.push(']'); }
            out.push('(');
            comma_sep(out, args, |out, a| fmt_expr(out, a, depth));
            if !named_args.is_empty() {
                if !args.is_empty() { out.push_str(", "); }
                comma_sep(out, named_args, |out, (name, expr)| {
                    w!(out, "{name}: ");
                    fmt_expr(out, expr, depth);
                });
            }
            out.push(')');
        }
        Expr::Member { object, field, .. } => { fmt_expr(out, object, depth); w!(out, ".{field}"); }
        Expr::TupleIndex { object, index, .. } => { fmt_expr(out, object, depth); w!(out, ".{index}"); }
        Expr::IndexAccess { object, index, .. } => { fmt_expr(out, object, depth); out.push('['); fmt_expr(out, index, depth); out.push(']'); }
        Expr::Pipe { left, right, .. } => { fmt_expr(out, left, depth); out.push_str(" |> "); fmt_expr(out, right, depth); }
        Expr::Compose { left, right, .. } => { fmt_expr(out, left, depth); out.push_str(" >> "); fmt_expr(out, right, depth); }
        Expr::Binary { op, left, right, .. } => { fmt_expr(out, left, depth); w!(out, " {op} "); fmt_expr(out, right, depth); }
        Expr::Unary { op, operand, .. } => { out.push_str(op); if op == "not" { out.push(' '); } fmt_expr(out, operand, depth); }
        Expr::Break { .. } => out.push_str("break"),
        Expr::Continue { .. } => out.push_str("continue"),
        Expr::Try { expr: e, .. } => { out.push_str("try "); fmt_expr(out, e, depth); }
        Expr::Unwrap { expr: e, .. } => { fmt_expr(out, e, depth); out.push('!'); }
        Expr::UnwrapOr { expr: e, fallback, .. } => { fmt_expr(out, e, depth); out.push_str(" ?? "); fmt_expr(out, fallback, depth); }
        Expr::ToOption { expr: e, .. } => { fmt_expr(out, e, depth); out.push('?'); }
        Expr::OptionalChain { expr: e, field, .. } => { fmt_expr(out, e, depth); out.push_str("?."); out.push_str(field); }
        Expr::Await { expr: e, .. } => { out.push_str("await "); fmt_expr(out, e, depth); }
        Expr::If { cond, then, else_, .. } => {
            out.push_str("if "); fmt_expr(out, cond, depth); out.push_str(" then "); fmt_expr(out, then, depth);
            if is_short(then) && is_short(else_) { out.push(' '); }
            else if out.ends_with('}') { out.push(' '); }
            else { out.push('\n'); out.push_str(&ind(depth)); }
            out.push_str("else "); fmt_expr(out, else_, depth);
        }
        Expr::Match { subject, arms, .. } => {
            out.push_str("match "); fmt_expr(out, subject, depth); out.push_str(" {\n");
            let ai = ind(depth + 1);
            for arm in arms {
                for c in &arm.comments { wln!(out, "{ai}{c}"); }
                out.push_str(&ai); fmt_pattern(out, &arm.pattern);
                if let Some(ref g) = arm.guard { out.push_str(" if "); fmt_expr(out, g, depth + 1); }
                out.push_str(" => "); fmt_expr(out, &arm.body, depth + 1);
                if arms.len() > 1 { out.push(','); }
                out.push('\n');
            }
            w!(out, "{}}}", ind(depth));
        }
        Expr::Block { stmts, expr, .. } => {
            if stmts.is_empty() { if let Some(e) = expr { if is_short(e) && depth > 0 { out.push_str("{ "); fmt_expr(out, e, depth); out.push_str(" }"); return; } } }
            fmt_block(out, stmts, expr, depth);
        }

        Expr::Fan { exprs, .. } => {
            out.push_str("fan {\n");
            for e in exprs {
                out.push_str(&ind(depth + 1)); fmt_expr(out, e, depth + 1); out.push('\n');
            }
            out.push_str(&ind(depth)); out.push('}');
        }
        Expr::Range { start, end, inclusive, .. } => { fmt_expr(out, start, depth); out.push_str(if *inclusive { "..=" } else { ".." }); fmt_expr(out, end, depth); }
        Expr::ForIn { var, var_tuple, iterable, body, .. } => {
            out.push_str("for ");
            if let Some(n) = var_tuple { w!(out, "({})", join_syms(n, ", ")); } else { out.push_str(var); }
            out.push_str(" in "); fmt_expr(out, iterable, depth); out.push_str(" {\n");
            for s in body { fmt_stmt(out, s, depth + 1); }
            w!(out, "{}}}", ind(depth));
        }
        Expr::While { cond, body, .. } => {
            out.push_str("while "); fmt_expr(out, cond, depth); out.push_str(" {\n");
            for s in body { fmt_stmt(out, s, depth + 1); }
            w!(out, "{}}}", ind(depth));
        }
        Expr::Lambda { params, body, .. } => {
            out.push('(');
            comma_sep(out, params, |out, p| {
                if let Some(n) = &p.tuple_names { w!(out, "({})", join_syms(n, ", ")); } else { out.push_str(&p.name); }
                if let Some(ref ty) = p.ty { out.push_str(": "); fmt_type(out, ty, depth); }
            });
            out.push_str(") => "); fmt_expr(out, body, depth);
        }
    }
}

fn fmt_block(out: &mut String, stmts: &[Stmt], expr: &Option<Box<Expr>>, depth: usize) {
    out.push_str("{\n");
    for s in stmts { fmt_stmt(out, s, depth + 1); }
    if let Some(e) = expr { out.push_str(&ind(depth + 1)); fmt_expr(out, e, depth + 1); out.push('\n'); }
    w!(out, "{}}}", ind(depth));
}

fn fmt_list(out: &mut String, elements: &[Expr], depth: usize) {
    if elements.is_empty() { out.push_str("[]"); return; }
    if elements.len() <= 5 && elements.iter().all(is_short) {
        out.push('['); comma_sep(out, elements, |out, e| fmt_expr(out, e, depth)); out.push(']');
    } else {
        out.push_str("[\n");
        for (i, e) in elements.iter().enumerate() {
            out.push_str(&ind(depth + 1)); fmt_expr(out, e, depth + 1);
            if i < elements.len() - 1 { out.push(','); } out.push('\n');
        }
        w!(out, "{}]", ind(depth));
    }
}

fn fmt_map(out: &mut String, entries: &[(Expr, Expr)], depth: usize) {
    let short = entries.len() <= 3 && entries.iter().all(|(k, v)| is_short(k) && is_short(v));
    let (open, close, d) = if short { ("[", "]", depth) } else { ("[\n", "]", depth + 1) };
    out.push_str(open);
    for (i, (k, v)) in entries.iter().enumerate() {
        if short { if i > 0 { out.push_str(", "); } }
        else { out.push_str(&ind(d)); }
        fmt_expr(out, k, d); out.push_str(": "); fmt_expr(out, v, d);
        if !short { if i < entries.len() - 1 { out.push(','); } out.push('\n'); }
    }
    if !short { out.push_str(&ind(depth)); }
    out.push_str(close);
}

fn fmt_istring_parts(out: &mut String, parts: &[StringPart], depth: usize) {
    let has_interp = parts.iter().any(|p| matches!(p, StringPart::Expr { .. }));
    // Single quotes don't support interpolation, so only use them for pure-literal strings
    let has_dquote = parts.iter().any(|p| matches!(p, StringPart::Lit { value } if value.contains('"')));
    let has_squote = parts.iter().any(|p| matches!(p, StringPart::Lit { value } if value.contains('\'')));
    let use_single = !has_interp && has_dquote && !has_squote;

    let quote = if use_single { '\'' } else { '"' };
    out.push(quote);
    for part in parts {
        match part {
            StringPart::Lit { value } => {
                for ch in value.chars() {
                    if ch == '\n' { out.push_str("\\n"); }
                    else if ch == '\t' { out.push_str("\\t"); }
                    else if ch == '\\' { out.push_str("\\\\"); }
                    else if ch == quote { out.push('\\'); out.push(ch); }
                    else { out.push(ch); }
                }
            }
            StringPart::Expr { expr } => {
                out.push_str("${");
                fmt_expr(out, expr, depth);
                out.push('}');
            }
        }
    }
    out.push(quote);
}

fn fmt_stmt(out: &mut String, stmt: &Stmt, depth: usize) {
    let i = ind(depth);
    match stmt {
        Stmt::Let { name, ty, value, .. } => {
            w!(out, "{i}let {name}");
            if let Some(t) = ty { out.push_str(": "); fmt_type(out, t, depth); }
            out.push_str(" = "); fmt_expr(out, value, depth);
        }
        Stmt::LetDestructure { pattern, value, .. } => { out.push_str(&i); out.push_str("let "); fmt_dpat(out, pattern); out.push_str(" = "); fmt_expr(out, value, depth); }
        Stmt::Var { name, ty, value, .. } => {
            w!(out, "{i}var {name}");
            if let Some(t) = ty { out.push_str(": "); fmt_type(out, t, depth); }
            out.push_str(" = "); fmt_expr(out, value, depth);
        }
        Stmt::Assign { name, value, .. } => { w!(out, "{i}{name} = "); fmt_expr(out, value, depth); }
        Stmt::IndexAssign { target, index, value, .. } => { w!(out, "{i}{target}["); fmt_expr(out, index, depth); out.push_str("] = "); fmt_expr(out, value, depth); }
        Stmt::FieldAssign { target, field, value, .. } => { w!(out, "{i}{target}.{field} = "); fmt_expr(out, value, depth); }
        Stmt::Guard { cond, else_, .. } => { out.push_str(&i); out.push_str("guard "); fmt_expr(out, cond, depth); out.push_str(" else "); fmt_expr(out, else_, depth); }
        Stmt::Expr { expr, .. } => { out.push_str(&i); fmt_expr(out, expr, depth); }
        Stmt::Comment { text } => { wln!(out, "{i}{text}"); return; }
        Stmt::Error { .. } => return,
    }
    out.push_str(";\n");
}

fn fmt_pattern(out: &mut String, pat: &Pattern) {
    match pat {
        Pattern::Wildcard => out.push('_'),
        Pattern::Ident { name } => out.push_str(name),
        Pattern::Literal { value } => fmt_expr(out, value, 0),
        Pattern::Constructor { name, args } => {
            out.push_str(name);
            if !args.is_empty() { out.push('('); comma_sep(out, args, |out, a| fmt_pattern(out, a)); out.push(')'); }
        }
        Pattern::RecordPattern { name, fields, rest } => {
            w!(out, "{name} {{ ");
            comma_sep(out, fields, |out, f| { out.push_str(&f.name); if let Some(ref p) = f.pattern { out.push_str(": "); fmt_pattern(out, p); } });
            if *rest { if !fields.is_empty() { out.push_str(", "); } out.push_str(".."); }
            out.push_str(" }");
        }
        Pattern::Tuple { elements } => { out.push('('); comma_sep(out, elements, |out, e| fmt_pattern(out, e)); out.push(')'); }
        Pattern::Some { inner } => { out.push_str("some("); fmt_pattern(out, inner); out.push(')'); }
        Pattern::None => out.push_str("none"),
        Pattern::Ok { inner } => { out.push_str("ok("); fmt_pattern(out, inner); out.push(')'); }
        Pattern::Err { inner } => { out.push_str("err("); fmt_pattern(out, inner); out.push(')'); }
    }
}

fn fmt_dpat(out: &mut String, pat: &Pattern) {
    match pat {
        Pattern::Tuple { elements } => { out.push('('); comma_sep(out, elements, |out, e| fmt_dpat(out, e)); out.push(')'); }
        Pattern::RecordPattern { fields, .. } => { out.push_str("{ "); comma_sep(out, fields, |out, f| out.push_str(&f.name)); out.push_str(" }"); }
        Pattern::Ident { name } => out.push_str(name),
        _ => fmt_pattern(out, pat),
    }
}
