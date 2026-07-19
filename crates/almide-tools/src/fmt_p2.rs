fn fmt_expr(out: &mut String, expr: &Expr, depth: usize) {
    match &expr.kind {
        ExprKind::Int { raw, .. } => out.push_str(raw),
        ExprKind::Float { value, .. } => { let s = format!("{value}"); if s.contains('.') { out.push_str(&s); } else { out.push_str(&s); out.push_str(".0"); } }
        ExprKind::String { value, .. } => {
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
        ExprKind::InterpolatedString { parts, .. } => fmt_istring_parts(out, parts, depth),
        ExprKind::Bool { value, .. } => out.push_str(if *value { "true" } else { "false" }),
        ExprKind::Unit => out.push_str("()"),
        ExprKind::None => out.push_str("none"),
        ExprKind::Hole | ExprKind::Placeholder => out.push('_'),
        ExprKind::Error => out.push_str("/* error */"),
        ExprKind::Todo { message, .. } => if message.is_empty() { out.push_str("todo"); } else { w!(out, "todo(\"{}\")", crate::fmt::escape_dquoted(message)); },
        ExprKind::Some { expr: e, .. } => { out.push_str("some("); fmt_expr(out, e, depth); out.push(')'); }
        ExprKind::Ok { expr: e, .. } => { out.push_str("ok("); fmt_expr(out, e, depth); out.push(')'); }
        ExprKind::Err { expr: e, .. } => { out.push_str("err("); fmt_expr(out, e, depth); out.push(')'); }
        ExprKind::Ident { name, .. } | ExprKind::TypeName { name, .. } => out.push_str(name),
        ExprKind::Paren { expr: e, .. } => { out.push('('); fmt_expr(out, e, depth); out.push(')'); }
        ExprKind::Tuple { elements, .. } => { out.push('('); comma_sep(out, elements, |out, e| fmt_expr(out, e, depth)); out.push(')'); }
        ExprKind::List { elements, .. } => fmt_list(out, elements, depth),
        ExprKind::EmptyMap => out.push_str("[:]"),
        ExprKind::MapLiteral { entries, .. } => fmt_map(out, entries, depth),
        ExprKind::Record { name, fields, .. } => {
            if let Some(n) = name { w!(out, "{n} "); }
            if fields.is_empty() { out.push_str("{}"); }
            else { out.push_str("{ "); comma_sep(out, fields, |out, f| { w!(out, "{}: ", f.name); fmt_expr(out, &f.value, depth); }); out.push_str(" }"); }
        }
        ExprKind::SpreadRecord { base, fields, .. } => {
            out.push_str("{ ..."); fmt_expr(out, base, depth);
            for f in fields { w!(out, ", {}: ", f.name); fmt_expr(out, &f.value, depth); }
            out.push_str(" }");
        }
        ExprKind::Call { callee, args, type_args, named_args, .. } => {
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
        ExprKind::Member { object, field, .. } => { fmt_expr(out, object, depth); w!(out, ".{field}"); }
        ExprKind::TupleIndex { object, index, .. } => { fmt_expr(out, object, depth); w!(out, ".{index}"); }
        ExprKind::IndexAccess { object, index, .. } => { fmt_expr(out, object, depth); out.push('['); fmt_expr(out, index, depth); out.push(']'); }
        ExprKind::Pipe { left, right, .. } => { fmt_expr(out, left, depth); out.push_str(" |> "); fmt_expr(out, right, depth); }
        ExprKind::Compose { left, right, .. } => { fmt_expr(out, left, depth); out.push_str(" >> "); fmt_expr(out, right, depth); }
        ExprKind::Binary { op, left, right, .. } => { fmt_expr(out, left, depth); w!(out, " {op} "); fmt_expr(out, right, depth); }
        ExprKind::Unary { op, operand, .. } => { out.push_str(op); if op == "not" { out.push(' '); } fmt_expr(out, operand, depth); }
        ExprKind::Break => out.push_str("break"),
        ExprKind::Continue => out.push_str("continue"),
        ExprKind::Try { expr: e, .. } => { out.push_str("try "); fmt_expr(out, e, depth); }
        ExprKind::Unwrap { expr: e, .. } => { fmt_expr(out, e, depth); out.push('!'); }
        ExprKind::UnwrapOr { expr: e, fallback, .. } => { fmt_expr(out, e, depth); out.push_str(" ?? "); fmt_expr(out, fallback, depth); }
        ExprKind::ToOption { expr: e, .. } => { fmt_expr(out, e, depth); out.push('?'); }
        ExprKind::OptionalChain { expr: e, field, .. } => { fmt_expr(out, e, depth); out.push_str("?."); out.push_str(field); }
        ExprKind::Await { expr: e, .. } => { out.push_str("await "); fmt_expr(out, e, depth); }
        ExprKind::If { cond, then, else_, .. } => {
            out.push_str("if "); fmt_expr(out, cond, depth); out.push_str(" then "); fmt_expr(out, then, depth);
            if is_short(then) && is_short(else_) { out.push(' '); }
            else if out.ends_with('}') { out.push(' '); }
            else { out.push('\n'); out.push_str(&ind(depth)); }
            out.push_str("else "); fmt_expr(out, else_, depth);
        }
        ExprKind::IfLet { name, scrutinee, then, else_ } => {
            out.push_str("if let "); out.push_str(name.as_str());
            out.push_str(" = "); fmt_expr(out, scrutinee, depth);
            out.push(' '); fmt_expr(out, then, depth);
            out.push_str(" else "); fmt_expr(out, else_, depth);
        }
        ExprKind::Match { subject, arms, .. } => {
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
        ExprKind::Block { stmts, expr, .. } => {
            if stmts.is_empty() { if let Some(e) = expr { if is_short(e) && depth > 0 { out.push_str("{ "); fmt_expr(out, e, depth); out.push_str(" }"); return; } } }
            fmt_block(out, stmts, expr, depth);
        }

        ExprKind::Fan { exprs, .. } => {
            out.push_str("fan {\n");
            for e in exprs {
                out.push_str(&ind(depth + 1)); fmt_expr(out, e, depth + 1); out.push('\n');
            }
            out.push_str(&ind(depth)); out.push('}');
        }
        ExprKind::Range { start, end, inclusive, .. } => { fmt_expr(out, start, depth); out.push_str(if *inclusive { "..=" } else { ".." }); fmt_expr(out, end, depth); }
        ExprKind::ForIn { var, var_tuple, iterable, body, .. } => {
            out.push_str("for ");
            if let Some(n) = var_tuple { w!(out, "({})", join_syms(n, ", ")); } else { out.push_str(var); }
            out.push_str(" in "); fmt_expr(out, iterable, depth); out.push_str(" {\n");
            for s in body { fmt_stmt(out, s, depth + 1); }
            w!(out, "{}}}", ind(depth));
        }
        ExprKind::While { cond, body, .. } => {
            out.push_str("while "); fmt_expr(out, cond, depth); out.push_str(" {\n");
            for s in body { fmt_stmt(out, s, depth + 1); }
            w!(out, "{}}}", ind(depth));
        }
        ExprKind::Lambda { params, body, .. } => {
            out.push('(');
            comma_sep(out, params, |out, p| {
                if let Some(n) = &p.tuple_names { w!(out, "({})", join_syms(n, ", ")); } else { out.push_str(&p.name); }
                if let Some(ref ty) = p.ty { out.push_str(": "); fmt_type(out, ty, depth); }
            });
            out.push_str(") => "); fmt_expr(out, body, depth);
        }
        ExprKind::TypeAscription { expr, ty } => {
            // Parenthesize so the ascription re-parses in EVERY position, not just
            // as a bare call argument: `([]: List[String])` is valid as a record-
            // field value / `let` initializer, while the bare `[]: List[String]`
            // there is a parse error (the `:` is unexpected). `(expr: Type)` parses
            // anywhere an expression does, so this is safe + idempotent (#437).
            out.push('(');
            fmt_expr(out, expr, depth);
            out.push_str(": ");
            fmt_type(out, ty, depth);
            out.push(')');
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
        Stmt::GuardLet { name, scrutinee, else_, .. } => { out.push_str(&i); out.push_str("guard let "); out.push_str(name.as_str()); out.push_str(" = "); fmt_expr(out, scrutinee, depth); out.push_str(" else "); fmt_expr(out, else_, depth); }
        Stmt::Expr { expr, .. } => { out.push_str(&i); fmt_expr(out, expr, depth); }
        Stmt::Comment { text } => { wln!(out, "{i}{text}"); return; }
        Stmt::Error { .. } => return,
    }
    out.push_str(";\n");
}

/// One test `where` clause, matching the parsed grammar:
/// `where name = expr` / `where path.to = expr` /
/// `where target(pats) => expr` / `"case" [name = expr, ...]` (inside `where [ ]`).
fn fmt_test_where(out: &mut String, wc: &TestWhere, depth: usize) {
    if !matches!(wc, TestWhere::Case { .. }) { out.push_str("where "); }
    fmt_test_where_bare(out, wc, depth);
}

/// The clause WITHOUT the `where` keyword — the form used inside a case's
/// `[...]` binding list (`"times 10" [double(x) => x * 10, input = 5]`).
fn fmt_test_where_bare(out: &mut String, wc: &TestWhere, depth: usize) {
    match wc {
        TestWhere::Bind { name, value } => {
            w!(out, "{} = ", name);
            fmt_expr(out, value, depth);
        }
        TestWhere::Override { path, value } => {
            w!(out, "{} = ", join_syms(path, "."));
            fmt_expr(out, value, depth);
        }
        TestWhere::CallResponse { target, params, response } => {
            w!(out, "{}(", join_syms(target, "."));
            comma_sep(out, params, |out, p| fmt_pattern(out, p));
            out.push_str(") => ");
            fmt_expr(out, response, depth);
        }
        TestWhere::Case { name, bindings } => {
            w!(out, "\"{}\" [", crate::fmt::escape_dquoted(name));
            comma_sep(out, bindings, |out, b| fmt_test_where_bare(out, b, depth));
            out.push(']');
        }
    }
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
        Pattern::List { elements } => { out.push('['); comma_sep(out, elements, |out, e| fmt_pattern(out, e)); out.push(']'); }
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

