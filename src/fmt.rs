/// Almide code formatter: AST → formatted source code.
///
/// Top-level comments (before module, imports, and declarations) are preserved
/// via the `comment_map` field on `Program`. Inline comments within function
/// bodies are not yet preserved.

use crate::ast::*;

const INDENT: &str = "  ";

pub fn format_program(program: &Program) -> String {
    let mut out = String::new();
    let cm = &program.comment_map;
    let mut cm_idx = 0;

    // Helper: emit comments at current index
    let emit_comments = |out: &mut String, idx: &mut usize| {
        if let Some(comments) = cm.get(*idx) {
            for c in comments {
                out.push_str(c);
                out.push('\n');
            }
        }
        *idx += 1;
    };

    // Imports
    if !program.imports.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        for imp in &program.imports {
            emit_comments(&mut out, &mut cm_idx);
            if let Decl::Import { path, names, alias, .. } = imp {
                out.push_str("import ");
                out.push_str(&path.join("."));
                if let Some(names) = names {
                    out.push_str(&format!(" ({})", names.join(", ")));
                }
                if let Some(a) = alias {
                    out.push_str(&format!(" as {}", a));
                }
                out.push('\n');
            }
        }
    }

    // Declarations
    for decl in &program.decls {
        out.push('\n');
        emit_comments(&mut out, &mut cm_idx);
        format_decl(&mut out, decl, 0);
        out.push('\n');
    }

    // Trailing comments
    if let Some(comments) = cm.get(cm_idx) {
        if !comments.is_empty() {
            out.push('\n');
            for c in comments {
                out.push_str(c);
                out.push('\n');
            }
        }
    }

    out
}

fn format_decl(out: &mut String, decl: &Decl, depth: usize) {
    let ind = indent(depth);
    match decl {
        Decl::Module { path, .. } => {
            out.push_str(&format!("{}module {}", ind, path.join(".")));
        }
        Decl::Import { path, names, alias, .. } => {
            out.push_str(&format!("{}import {}", ind, path.join(".")));
            if let Some(names) = names {
                out.push_str(&format!(" ({})", names.join(", ")));
            }
            if let Some(a) = alias {
                out.push_str(&format!(" as {}", a));
            }
        }
        Decl::Strict { mode, .. } => {
            out.push_str(&format!("{}strict \"{}\"", ind, mode));
        }
        Decl::Type { name, ty, deriving, visibility, .. } => {
            out.push_str(&ind);
            match visibility {
                Visibility::Local => out.push_str("local "),
                Visibility::Mod => out.push_str("mod "),
                Visibility::Public => {}
            }
            out.push_str(&format!("type {} = ", name));
            format_type_expr(out, ty, depth);
            if let Some(derives) = deriving {
                if !derives.is_empty() {
                    out.push_str(&format!(" deriving {}", derives.join(", ")));
                }
            }
        }
        Decl::Fn { name, effect, r#async, visibility, params, return_type, body, extern_attrs, .. } => {
            // Emit @extern annotations
            for attr in extern_attrs {
                out.push_str(&ind);
                out.push_str(&format!("@extern({}, \"{}\", \"{}\")\n", attr.target, attr.module, attr.function));
            }
            out.push_str(&ind);
            match visibility {
                Visibility::Local => out.push_str("local "),
                Visibility::Mod => out.push_str("mod "),
                Visibility::Public => {}
            }
            if matches!(effect, Some(true)) {
                out.push_str("effect ");
            }
            if matches!(r#async, Some(true)) {
                out.push_str("async ");
            }
            out.push_str("fn ");
            out.push_str(name);
            out.push('(');
            for (i, p) in params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&p.name);
                out.push_str(": ");
                format_type_expr(out, &p.ty, depth);
            }
            out.push_str(") -> ");
            format_type_expr(out, return_type, depth);
            if let Some(body) = body {
                out.push_str(" = ");
                format_expr(out, body, depth);
            }
        }
        Decl::Test { name, body, .. } => {
            out.push_str(&format!("{}test \"{}\" ", ind, name));
            format_expr(out, body, depth);
        }
        Decl::Trait { name, .. } => {
            out.push_str(&format!("{}trait {}", ind, name));
        }
        Decl::Impl { trait_, for_, methods, .. } => {
            out.push_str(&format!("{}impl {} for {} {{\n", ind, trait_, for_));
            for m in methods {
                format_decl(out, m, depth + 1);
                out.push('\n');
            }
            out.push_str(&format!("{}}}", ind));
        }
    }
}

fn format_type_expr(out: &mut String, ty: &TypeExpr, _depth: usize) {
    match ty {
        TypeExpr::Simple { name } => out.push_str(name),
        TypeExpr::Generic { name, args } => {
            out.push_str(name);
            out.push('[');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                format_type_expr(out, a, _depth);
            }
            out.push(']');
        }
        TypeExpr::Record { fields } => {
            out.push_str("{ ");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&f.name);
                out.push_str(": ");
                format_type_expr(out, &f.ty, _depth);
            }
            out.push_str(" }");
        }
        TypeExpr::Fn { params, ret } => {
            out.push_str("(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                format_type_expr(out, p, _depth);
            }
            out.push_str(") -> ");
            format_type_expr(out, ret, _depth);
        }
        TypeExpr::Tuple { elements } => {
            out.push('(');
            for (i, e) in elements.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                format_type_expr(out, e, _depth);
            }
            out.push(')');
        }
        TypeExpr::Newtype { inner } => {
            format_type_expr(out, inner, _depth);
        }
        TypeExpr::Variant { cases } => {
            for (i, case) in cases.iter().enumerate() {
                if i > 0 { out.push_str(" | "); } else { out.push_str("| "); }
                match case {
                    VariantCase::Unit { name } => out.push_str(name),
                    VariantCase::Tuple { name, fields } => {
                        out.push_str(name);
                        out.push('(');
                        for (j, f) in fields.iter().enumerate() {
                            if j > 0 { out.push_str(", "); }
                            format_type_expr(out, f, _depth);
                        }
                        out.push(')');
                    }
                    VariantCase::Record { name, fields } => {
                        out.push_str(name);
                        out.push_str(" { ");
                        for (j, f) in fields.iter().enumerate() {
                            if j > 0 { out.push_str(", "); }
                            out.push_str(&f.name);
                            out.push_str(": ");
                            format_type_expr(out, &f.ty, _depth);
                        }
                        out.push_str(" }");
                    }
                }
            }
        }
    }
}

fn format_expr(out: &mut String, expr: &Expr, depth: usize) {
    match expr {
        Expr::Int { raw, .. } => out.push_str(raw),
        Expr::Float { value, .. } => {
            let s = format!("{}", value);
            if s.contains('.') {
                out.push_str(&s);
            } else {
                out.push_str(&format!("{}.0", s));
            }
        }
        Expr::String { value, .. } => out.push_str(&format!("{:?}", value)),
        Expr::InterpolatedString { value, .. } => {
            out.push('"');
            for ch in value.chars() {
                match ch {
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    other => out.push(other),
                }
            }
            out.push('"');
        }
        Expr::Bool { value, .. } => out.push_str(if *value { "true" } else { "false" }),
        Expr::Unit { .. } => out.push_str("()"),
        Expr::None { .. } => out.push_str("none"),
        Expr::Hole { .. } => out.push_str("_"),
        Expr::Placeholder { .. } => out.push_str("_"),
        Expr::Todo { message, .. } => {
            if message.is_empty() {
                out.push_str("todo");
            } else {
                out.push_str(&format!("todo(\"{}\")", message));
            }
        }
        Expr::Some { expr: inner, .. } => {
            out.push_str("some(");
            format_expr(out, inner, depth);
            out.push(')');
        }
        Expr::Ok { expr: inner, .. } => {
            out.push_str("ok(");
            format_expr(out, inner, depth);
            out.push(')');
        }
        Expr::Err { expr: inner, .. } => {
            out.push_str("err(");
            format_expr(out, inner, depth);
            out.push(')');
        }
        Expr::Ident { name, .. } => out.push_str(name),
        Expr::TypeName { name, .. } => out.push_str(name),
        Expr::Paren { expr: inner, .. } => {
            out.push('(');
            format_expr(out, inner, depth);
            out.push(')');
        }
        Expr::Tuple { elements, .. } => {
            out.push('(');
            for (i, e) in elements.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                format_expr(out, e, depth);
            }
            out.push(')');
        }

        Expr::List { elements, .. } => {
            if elements.is_empty() {
                out.push_str("[]");
            } else if is_short_list(elements) {
                out.push('[');
                for (i, e) in elements.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    format_expr(out, e, depth);
                }
                out.push(']');
            } else {
                out.push_str("[\n");
                for (i, e) in elements.iter().enumerate() {
                    out.push_str(&indent(depth + 1));
                    format_expr(out, e, depth + 1);
                    if i < elements.len() - 1 { out.push(','); }
                    out.push('\n');
                }
                out.push_str(&indent(depth));
                out.push(']');
            }
        }

        Expr::Record { name, fields, .. } => {
            if let Some(n) = name {
                out.push_str(n);
                out.push(' ');
            }
            if fields.is_empty() {
                out.push_str("{}");
            } else {
                out.push_str("{ ");
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    out.push_str(&f.name);
                    out.push_str(": ");
                    format_expr(out, &f.value, depth);
                }
                out.push_str(" }");
            }
        }

        Expr::SpreadRecord { base, fields, .. } => {
            out.push_str("{ ...");
            format_expr(out, base, depth);
            for f in fields {
                out.push_str(", ");
                out.push_str(&f.name);
                out.push_str(": ");
                format_expr(out, &f.value, depth);
            }
            out.push_str(" }");
        }

        Expr::Call { callee, args, type_args, .. } => {
            format_expr(out, callee, depth);
            if let Some(ta) = type_args {
                out.push('[');
                for (i, t) in ta.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    format_type_expr(out, t, depth);
                }
                out.push(']');
            }
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                format_expr(out, a, depth);
            }
            out.push(')');
        }

        Expr::Member { object, field, .. } => {
            format_expr(out, object, depth);
            out.push('.');
            out.push_str(field);
        }

        Expr::TupleIndex { object, index, .. } => {
            format_expr(out, object, depth);
            out.push('.');
            out.push_str(&index.to_string());
        }

        Expr::Pipe { left, right, .. } => {
            format_expr(out, left, depth);
            out.push_str(" |> ");
            format_expr(out, right, depth);
        }

        Expr::Binary { op, left, right, .. } => {
            format_expr(out, left, depth);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            format_expr(out, right, depth);
        }

        Expr::Unary { op, operand, .. } => {
            out.push_str(op);
            if op == "not" { out.push(' '); }
            format_expr(out, operand, depth);
        }

        Expr::Break { .. } => {
            out.push_str("break");
        }

        Expr::Continue { .. } => {
            out.push_str("continue");
        }

        Expr::Try { expr: inner, .. } => {
            out.push_str("try ");
            format_expr(out, inner, depth);
        }

        Expr::Await { expr: inner, .. } => {
            out.push_str("await ");
            format_expr(out, inner, depth);
        }

        Expr::If { cond, then, else_, .. } => {
            let inline = is_short_expr(then) && is_short_expr(else_);
            if inline {
                out.push_str("if ");
                format_expr(out, cond, depth);
                out.push_str(" then ");
                format_expr(out, then, depth);
                out.push_str(" else ");
                format_expr(out, else_, depth);
            } else {
                out.push_str("if ");
                format_expr(out, cond, depth);
                out.push_str(" then ");
                format_expr(out, then, depth);
                // Keep else on same line after closing brace
                if out.ends_with('}') {
                    out.push(' ');
                } else {
                    out.push('\n');
                    out.push_str(&indent(depth));
                }
                out.push_str("else ");
                format_expr(out, else_, depth);
            }
        }

        Expr::Match { subject, arms, .. } => {
            out.push_str("match ");
            format_expr(out, subject, depth);
            out.push_str(" {\n");
            for arm in arms {
                for comment in &arm.comments {
                    out.push_str(&indent(depth + 1));
                    out.push_str(comment);
                    out.push('\n');
                }
                out.push_str(&indent(depth + 1));
                format_pattern(out, &arm.pattern);
                if let Some(ref guard) = arm.guard {
                    out.push_str(" if ");
                    format_expr(out, guard, depth + 1);
                }
                out.push_str(" => ");
                format_expr(out, &arm.body, depth + 1);
                if arms.len() > 1 { out.push(','); }
                out.push('\n');
            }
            out.push_str(&indent(depth));
            out.push('}');
        }

        Expr::Block { stmts, expr, .. } => {
            if stmts.is_empty() && expr.is_some() {
                // Single-expression block: might be inline
                let inner = expr.as_ref().expect("guarded by is_some()");
                if is_short_expr(inner) && depth > 0 {
                    out.push_str("{ ");
                    format_expr(out, inner, depth);
                    out.push_str(" }");
                    return;
                }
            }
            out.push_str("{\n");
            for s in stmts {
                format_stmt(out, s, depth + 1);
            }
            if let Some(e) = expr {
                out.push_str(&indent(depth + 1));
                format_expr(out, e, depth + 1);
                out.push('\n');
            }
            out.push_str(&indent(depth));
            out.push('}');
        }

        Expr::DoBlock { stmts, expr, .. } => {
            out.push_str("do {\n");
            for s in stmts {
                format_stmt(out, s, depth + 1);
            }
            if let Some(e) = expr {
                out.push_str(&indent(depth + 1));
                format_expr(out, e, depth + 1);
                out.push('\n');
            }
            out.push_str(&indent(depth));
            out.push('}');
        }

        Expr::Range { start, end, inclusive, .. } => {
            format_expr(out, start, depth);
            if *inclusive {
                out.push_str("..=");
            } else {
                out.push_str("..");
            }
            format_expr(out, end, depth);
        }

        Expr::ForIn { var, var_tuple, iterable, body, .. } => {
            out.push_str("for ");
            if let Some(names) = var_tuple {
                out.push('(');
                out.push_str(&names.join(", "));
                out.push(')');
            } else {
                out.push_str(var);
            }
            out.push_str(" in ");
            format_expr(out, iterable, depth);
            out.push_str(" {\n");
            for s in body {
                format_stmt(out, s, depth + 1);
            }
            out.push_str(&indent(depth));
            out.push('}');
        }

        Expr::Lambda { params, body, .. } => {
            out.push_str("fn(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                if let Some(names) = &p.tuple_names {
                    out.push('(');
                    out.push_str(&names.join(", "));
                    out.push(')');
                } else {
                    out.push_str(&p.name);
                }
                if let Some(ref ty) = p.ty {
                    out.push_str(": ");
                    format_type_expr(out, ty, depth);
                }
            }
            out.push_str(") => ");
            format_expr(out, body, depth);
        }
    }
}

fn format_stmt(out: &mut String, stmt: &Stmt, depth: usize) {
    let ind = indent(depth);
    match stmt {
        Stmt::Let { name, ty, value, .. } => {
            out.push_str(&ind);
            out.push_str("let ");
            out.push_str(name);
            if let Some(ty) = ty {
                out.push_str(": ");
                format_type_expr(out, ty, depth);
            }
            out.push_str(" = ");
            format_expr(out, value, depth);
        }
        Stmt::LetDestructure { pattern, value, .. } => {
            out.push_str(&ind);
            out.push_str("let ");
            format_destructure_pattern(out, pattern);
            out.push_str(" = ");
            format_expr(out, value, depth);
        }
        Stmt::Var { name, ty, value, .. } => {
            out.push_str(&ind);
            out.push_str("var ");
            out.push_str(name);
            if let Some(ty) = ty {
                out.push_str(": ");
                format_type_expr(out, ty, depth);
            }
            out.push_str(" = ");
            format_expr(out, value, depth);
        }
        Stmt::Assign { name, value, .. } => {
            out.push_str(&ind);
            out.push_str(name);
            out.push_str(" = ");
            format_expr(out, value, depth);
        }
        Stmt::IndexAssign { target, index, value, .. } => {
            out.push_str(&ind);
            out.push_str(target);
            out.push('[');
            format_expr(out, index, depth);
            out.push_str("] = ");
            format_expr(out, value, depth);
        }
        Stmt::FieldAssign { target, field, value, .. } => {
            out.push_str(&ind);
            out.push_str(target);
            out.push('.');
            out.push_str(field);
            out.push_str(" = ");
            format_expr(out, value, depth);
        }
        Stmt::Guard { cond, else_, .. } => {
            out.push_str(&ind);
            out.push_str("guard ");
            format_expr(out, cond, depth);
            out.push_str(" else ");
            format_expr(out, else_, depth);
        }
        Stmt::Expr { expr, .. } => {
            out.push_str(&ind);
            format_expr(out, expr, depth);
        }
        Stmt::Comment { text } => {
            out.push_str(&ind);
            out.push_str(text);
            out.push('\n');
            return;
        }
    }
    out.push_str(";\n");
}

/// Format a destructure pattern for `let` statements.
/// Differs from `format_pattern` in that record patterns omit the constructor name.
fn format_destructure_pattern(out: &mut String, pat: &Pattern) {
    match pat {
        Pattern::Tuple { elements } => {
            out.push('(');
            for (i, e) in elements.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                format_destructure_pattern(out, e);
            }
            out.push(')');
        }
        Pattern::RecordPattern { fields, .. } => {
            out.push_str("{ ");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&f.name);
            }
            out.push_str(" }");
        }
        Pattern::Ident { name } => out.push_str(name),
        _ => format_pattern(out, pat),
    }
}

fn format_pattern(out: &mut String, pat: &Pattern) {
    match pat {
        Pattern::Wildcard => out.push('_'),
        Pattern::Ident { name } => out.push_str(name),
        Pattern::Literal { value } => format_expr(out, value, 0),
        Pattern::Constructor { name, args } => {
            out.push_str(name);
            if !args.is_empty() {
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    format_pattern(out, a);
                }
                out.push(')');
            }
        }
        Pattern::RecordPattern { name, fields, rest } => {
            out.push_str(name);
            out.push_str(" { ");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&f.name);
                if let Some(ref p) = f.pattern {
                    out.push_str(": ");
                    format_pattern(out, p);
                }
            }
            if *rest {
                if !fields.is_empty() { out.push_str(", "); }
                out.push_str("..");
            }
            out.push_str(" }");
        }
        Pattern::Tuple { elements } => {
            out.push('(');
            for (i, e) in elements.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                format_pattern(out, e);
            }
            out.push(')');
        }
        Pattern::Some { inner } => {
            out.push_str("some(");
            format_pattern(out, inner);
            out.push(')');
        }
        Pattern::None => out.push_str("none"),
        Pattern::Ok { inner } => {
            out.push_str("ok(");
            format_pattern(out, inner);
            out.push(')');
        }
        Pattern::Err { inner } => {
            out.push_str("err(");
            format_pattern(out, inner);
            out.push(')');
        }
    }
}

fn indent(depth: usize) -> String {
    INDENT.repeat(depth)
}

/// Heuristic: is this expression short enough for inline formatting?
fn is_short_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Int { .. } | Expr::Float { .. } | Expr::Bool { .. }
        | Expr::Unit { .. } | Expr::None { .. } | Expr::Hole { .. } | Expr::Placeholder { .. }
        | Expr::Ident { .. } | Expr::TypeName { .. } => true,
        Expr::String { value, .. } => value.len() < 40,
        Expr::Some { expr, .. } | Expr::Ok { expr, .. } | Expr::Err { expr, .. }
        | Expr::Paren { expr, .. } => is_short_expr(expr),
        Expr::Tuple { elements, .. } => elements.len() <= 4 && elements.iter().all(is_short_expr),
        Expr::Call { args, .. } => args.len() <= 2 && args.iter().all(is_short_expr),
        Expr::Binary { left, right, .. } => is_short_expr(left) && is_short_expr(right),
        Expr::Unary { operand, .. } => is_short_expr(operand),
        _ => false,
    }
}

/// Heuristic: is a list short enough to be on one line?
fn is_short_list(elements: &[Expr]) -> bool {
    elements.len() <= 5 && elements.iter().all(is_short_expr)
}
