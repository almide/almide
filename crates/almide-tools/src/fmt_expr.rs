thread_local! {
    /// #1404: the expression-comment bindings for the program being formatted.
    ///
    /// A thread_local rather than a parameter because `fmt_expr` has ~70 call
    /// sites across this file and `fmt.rs`, and threading a map through all of
    /// them is exactly the kind of mechanical edit where ONE missed site drops
    /// a comment silently — the failure this feature exists to prevent. Scoped
    /// by `with_expr_comments` so it can never outlive one format run, and the
    /// crate already uses this idiom (`bundled_borrow_at`'s per-fn cache).
    static EXPR_COMMENTS: std::cell::RefCell<std::collections::HashMap<ExprId, ExprComments>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Run `f` with `map` installed as the active expression-comment table, then
/// restore. Nested calls are not expected, but restoring rather than clearing
/// keeps one format run from blanking another's table if they ever are.
pub(crate) fn with_expr_comments<R>(map: &std::collections::HashMap<ExprId, ExprComments>, f: impl FnOnce() -> R) -> R {
    let prev = EXPR_COMMENTS.with(|c| c.replace(map.clone()));
    let r = f();
    EXPR_COMMENTS.with(|c| *c.borrow_mut() = prev);
    r
}

/// The comments bound to `id`, if any.
fn comments_for(id: ExprId) -> Option<ExprComments> {
    EXPR_COMMENTS.with(|c| c.borrow().get(&id).cloned())
}

fn fmt_expr(out: &mut String, expr: &Expr, depth: usize) {
    // #1404: a bound comment brackets its node's rendering, on the side it was
    // written. Both go inline — a leading one before the node, a trailing one
    // after — because that is where the author put them and fmt is
    // idempotent-by-contract: re-reading this output must re-attach them to the
    // same node.
    let attached = comments_for(expr.id);
    if let Some(a) = &attached {
        for c in &a.leading {
            out.push_str(c);
            out.push(' ');
        }
    }
    fmt_expr_inner(out, expr, depth);
    if let Some(a) = &attached {
        for c in &a.trailing {
            out.push(' ');
            out.push_str(c);
        }
    }
}

/// Render an expression.
///
/// Split into five EXHAUSTIVE groups by shape — leaf, wrapper (a fixed prefix or
/// suffix around one child), infix, the value-shaped compounds, and the
/// block-shaped forms that already have their own helpers. Each group returns
/// `bool` (handled / not mine) instead of `Option`, so the compiler still
/// cannot warn about a dropped arm; instead the `debug_assert` below fails
/// loudly the first time a NEW `ExprKind` is added without a rendering, which
/// is the property the original single match had by exhaustiveness. Splitting
/// it any other way (a wildcard `_` in each group) would have silently shrunk
/// the formatter's coverage — the one thing this function must not do, since a
/// missing arm means source that fmt drops.
fn fmt_expr_inner(out: &mut String, expr: &Expr, depth: usize) {
    let handled = fmt_expr_leaf(out, expr)
        || fmt_expr_wrapper(out, expr, depth)
        || fmt_expr_infix(out, expr, depth)
        || fmt_expr_compound(out, expr, depth)
        || fmt_expr_blocklike(out, expr, depth);
    debug_assert!(handled, "fmt_expr: no rendering for {:?}", std::mem::discriminant(&expr.kind));
    if !handled {
        // Release builds must still emit SOMETHING parseable rather than silently
        // dropping the expression from the formatted output.
        out.push_str("/* unformatted */");
    }
}

/// Leaves: no child expressions.
fn fmt_expr_leaf(out: &mut String, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Int { raw, .. } => out.push_str(raw),
        // A literal that CAME FROM SOURCE reprints its own spelling (#1261,
        // #1263). Reprinting from the value normalized `1e10` to
        // `10000000000.0`, dropped the `_` from `1_000.25`, turned
        // `"\u{3042}"` into a bare `あ`, collapsed heredocs to one quoted
        // line — and rendered `1e999` as `inf.0`, which does not parse, so
        // fmt turned a valid file into an invalid one. The value is
        // untouched: this is a printing change only.
        ExprKind::Float { value, raw } => match raw {
            Some(r) => out.push_str(r),
            None => fmt_expr_float(out, *value),
        },
        ExprKind::String { value, raw } => match raw {
            Some(r) => out.push_str(r),
            None => fmt_expr_string(out, value),
        },
        ExprKind::Bool { value, .. } => out.push_str(if *value { "true" } else { "false" }),
        ExprKind::Unit => out.push_str("()"),
        ExprKind::None => out.push_str("none"),
        ExprKind::Hole | ExprKind::Placeholder => out.push('_'),
        ExprKind::Error => out.push_str("/* error */"),
        ExprKind::EmptyMap => out.push_str("[:]"),
        ExprKind::Break => out.push_str("break"),
        ExprKind::Continue => out.push_str("continue"),
        ExprKind::Ident { name, .. } | ExprKind::TypeName { name, .. } => out.push_str(name),
        ExprKind::Todo { message, .. } => fmt_expr_todo(out, message),
        _ => return false,
    }
    true
}

/// A float literal always prints with a decimal point (`1.0`, not `1`) so it
/// re-parses as a Float.
fn fmt_expr_float(out: &mut String, value: f64) {
    let s = format!("{value}");
    out.push_str(&s);
    if !s.contains('.') { out.push_str(".0"); }
}

fn fmt_expr_todo(out: &mut String, message: &str) {
    if message.is_empty() {
        out.push_str("todo");
    } else {
        w!(out, "todo(\"{}\")", crate::fmt::escape_dquoted(message));
    }
}

/// One child wrapped in a fixed prefix and/or suffix.
fn fmt_expr_wrapper(out: &mut String, expr: &Expr, depth: usize) -> bool {
    let mut around = |pre: &str, e: &Expr, post: &str| {
        out.push_str(pre);
        fmt_expr(out, e, depth);
        out.push_str(post);
    };
    match &expr.kind {
        ExprKind::Some { expr: e, .. } => around("some(", e, ")"),
        ExprKind::Ok { expr: e, .. } => around("ok(", e, ")"),
        ExprKind::Err { expr: e, .. } => around("err(", e, ")"),
        ExprKind::Paren { expr: e, .. } => around("(", e, ")"),
        ExprKind::Try { expr: e, .. } => around("try ", e, ""),
        ExprKind::Unwrap { expr: e, .. } => around("", e, "!"),
        ExprKind::ToOption { expr: e, .. } => around("", e, "?"),
        ExprKind::Unary { op, operand, .. } => {
            out.push_str(op);
            if op == "not" { out.push(' '); }
            fmt_expr(out, operand, depth);
        }
        ExprKind::Member { object, field, .. } => {
            fmt_expr(out, object, depth);
            w!(out, ".{field}");
        }
        ExprKind::TupleIndex { object, index, .. } => {
            fmt_expr(out, object, depth);
            w!(out, ".{index}");
        }
        ExprKind::OptionalChain { expr: e, field, .. } => {
            fmt_expr(out, e, depth);
            out.push_str("?.");
            out.push_str(field);
        }
        ExprKind::IndexAccess { object, index, .. } => {
            fmt_expr(out, object, depth);
            out.push('[');
            fmt_expr(out, index, depth);
            out.push(']');
        }
        _ => return false,
    }
    true
}

/// Two children joined by an operator or separator.
fn fmt_expr_infix(out: &mut String, expr: &Expr, depth: usize) -> bool {
    let mut joined = |l: &Expr, sep: &str, r: &Expr| {
        fmt_expr(out, l, depth);
        out.push_str(sep);
        fmt_expr(out, r, depth);
    };
    match &expr.kind {
        ExprKind::Pipe { left, right, .. } => joined(left, " |> ", right),
        ExprKind::Compose { left, right, .. } => joined(left, " >> ", right),
        ExprKind::UnwrapOr { expr: e, fallback, .. } => joined(e, " ?? ", fallback),
        ExprKind::Range { start, end, inclusive, .. } => {
            joined(start, if *inclusive { "..." } else { "..<" }, end)
        }
        ExprKind::Binary { op, left, right, .. } => {
            fmt_expr(out, left, depth);
            w!(out, " {op} ");
            fmt_expr(out, right, depth);
        }
        _ => return false,
    }
    true
}

/// Collections, interpolation, calls and lambdas — the value-shaped compound
/// forms. (The statement-shaped forms are `fmt_expr_blocklike`'s group; the
/// two together are the original compound group, split along the same
/// shape-axis as leaf/wrapper/infix.)
fn fmt_expr_compound(out: &mut String, expr: &Expr, depth: usize) -> bool {
    match &expr.kind {
        // Same source-spelling rule as the plain String leaf. An interpolated
        // heredoc reprints as a heredoc, and `"\u{3042}${x}"` keeps its
        // escape. Any tool that REWRITES an expression inside a `${…}` hole
        // must call `ast::strip_literal_raw` first — otherwise the verbatim
        // reprint would drop the rewrite.
        ExprKind::InterpolatedString { parts, raw } => match raw {
            Some(r) => fmt_istring_raw(out, r, parts, depth),
            None => fmt_istring_parts(out, parts, depth),
        },
        ExprKind::Tuple { elements, .. } => {
            out.push('(');
            comma_sep(out, elements, |out, e| fmt_expr(out, e, depth));
            // A 1-tuple's trailing comma is load-bearing: `(e,)` is a tuple,
            // `(e)` is grouping — dropping it changes the program (#1265).
            if elements.len() == 1 {
                out.push(',');
            }
            out.push(')');
        }
        ExprKind::List { elements, .. } => fmt_list(out, elements, depth),
        ExprKind::MapLiteral { entries, .. } => fmt_map(out, entries, depth),
        ExprKind::Record { .. } => fmt_expr_record(out, expr, depth),
        ExprKind::SpreadRecord { .. } => fmt_expr_spread_record(out, expr, depth),
        ExprKind::Call { .. } => fmt_expr_call(out, expr, depth),
        ExprKind::Lambda { .. } => fmt_expr_lambda(out, expr, depth),
        ExprKind::TypeAscription { .. } => fmt_expr_type_ascription(out, expr, depth),
        _ => return false,
    }
    true
}

/// The block-shaped / control-flow forms — each helper owns its line breaking.
fn fmt_expr_blocklike(out: &mut String, expr: &Expr, depth: usize) -> bool {
    match &expr.kind {
        ExprKind::If { .. } => fmt_expr_if(out, expr, depth),
        ExprKind::IfLet { .. } => fmt_expr_iflet(out, expr, depth),
        ExprKind::Match { .. } => fmt_expr_match(out, expr, depth),
        ExprKind::Block { .. } => fmt_expr_block(out, expr, depth),
        ExprKind::Fan { .. } => fmt_expr_fan(out, expr, depth),
        ExprKind::FanBounded { .. } => fmt_expr_fan_bounded(out, expr, depth),
        ExprKind::FanRace { .. } => fmt_expr_fan_race(out, expr, depth),
        ExprKind::FanRaceMap { .. } => fmt_expr_fan_race_map(out, expr, depth),
        ExprKind::FanSettle { .. } => fmt_expr_fan_settle(out, expr, depth),
        ExprKind::FanTimeout { .. } => fmt_expr_fan_timeout(out, expr, depth),
        ExprKind::ForIn { .. } => fmt_expr_forin(out, expr, depth),
        ExprKind::While { .. } => fmt_expr_while(out, expr, depth),
        _ => return false,
    }
    true
}

fn fmt_expr_string(out: &mut String, value: &str) {
    let has_dquote = value.contains('"');
    let has_squote = value.contains('\'');
    let use_single = has_dquote && !has_squote;
    let quote = if use_single { '\'' } else { '"' };
    out.push(quote);
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        i += push_escaped_char(out, &chars, i, quote, use_single);
    }
    out.push(quote);
}

/// Append the escaped form of `chars[i]` and return how many chars it consumed.
/// Only `${` in a DOUBLE-quoted string consumes two (a single-quoted string has no
/// interpolation, so `$` is literal there). Extracted so the escape table is a flat
/// chain instead of a `while`-nested one — the nesting, not the arm count, was what
/// tripped the depth limit.
fn push_escaped_char(out: &mut String, chars: &[char], i: usize, quote: char, use_single: bool) -> usize {
    let ch = chars[i];
    let interp_open = !use_single && ch == '$' && chars.get(i + 1) == Some(&'{');
    if interp_open {
        out.push_str("\\${");
        return 2;
    }
    match ch {
        '\n' => out.push_str("\\n"),
        '\t' => out.push_str("\\t"),
        '\r' => out.push_str("\\r"),
        '\\' => out.push_str("\\\\"),
        c if c == quote => { out.push('\\'); out.push(c); }
        c => out.push(c),
    }
    1
}

fn fmt_expr_record(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::Record { name, fields, .. } = &expr.kind else { unreachable!() };
    if let Some(n) = name { w!(out, "{n} "); }
    if fields.is_empty() { out.push_str("{}"); }
    else { out.push_str("{ "); comma_sep(out, fields, |out, f| { w!(out, "{}: ", f.name); fmt_expr(out, &f.value, depth); }); out.push_str(" }"); }
}

fn fmt_expr_spread_record(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::SpreadRecord { base, fields, .. } = &expr.kind else { unreachable!() };
    out.push_str("{ ..."); fmt_expr(out, base, depth);
    for f in fields { w!(out, ", {}: ", f.name); fmt_expr(out, &f.value, depth); }
    out.push_str(" }");
}

fn fmt_expr_call(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::Call { callee, args, type_args, named_args, .. } = &expr.kind else { unreachable!() };
    if try_fmt_fan_block_resugar(out, callee, args, depth) {
        return;
    }
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

/// Wave 1 block forms: the parser synthesizes `fan.__any_block([() => …])`
/// internally — RE-SUGAR to the surface spelling, or fmt would rewrite the
/// user's block into an internal name that does not parse. True = handled.
fn try_fmt_fan_block_resugar(out: &mut String, callee: &Expr, args: &[Expr], depth: usize) -> bool {
    let ExprKind::Member { object, field } = &callee.kind else { return false };
    let ExprKind::Ident { name, .. } = &object.kind else { return false };
    if name.as_str() != "fan" || !matches!(field.as_str(), "__any_block" | "__settle_block") {
        return false;
    }
    let [arg] = args else { return false };
    let ExprKind::List { elements } = &arg.kind else { return false };
    let head = if field.as_str() == "__any_block" { "any" } else { "settle" };
    w!(out, "fan.{head} {{\n");
    for el in elements {
        let body: &Expr = match &el.kind {
            ExprKind::Lambda { params, body } if params.is_empty() => body,
            _ => el,
        };
        out.push_str(&ind(depth + 1));
        fmt_expr(out, body, depth + 1);
        out.push('\n');
    }
    out.push_str(&ind(depth));
    out.push('}');
    true
}

fn fmt_expr_if(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::If { cond, then, else_, .. } = &expr.kind else { unreachable!() };
    out.push_str("if "); fmt_expr(out, cond, depth); out.push_str(" then "); fmt_expr(out, then, depth);
    if is_short(then) && is_short(else_) { out.push(' '); }
    else if out.ends_with('}') { out.push(' '); }
    else { out.push('\n'); out.push_str(&ind(depth)); }
    out.push_str("else "); fmt_expr(out, else_, depth);
}

fn fmt_expr_iflet(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::IfLet { name, scrutinee, then, else_ } = &expr.kind else { unreachable!() };
    out.push_str("if let "); out.push_str(name.as_str());
    out.push_str(" = "); fmt_expr(out, scrutinee, depth);
    out.push(' '); fmt_expr(out, then, depth);
    out.push_str(" else "); fmt_expr(out, else_, depth);
}

fn fmt_expr_match(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::Match { subject, arms, .. } = &expr.kind else { unreachable!() };
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

fn fmt_expr_block(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::Block { stmts, expr, .. } = &expr.kind else { unreachable!() };
    if stmts.is_empty() { if let Some(e) = expr { if is_short(e) && depth > 0 { out.push_str("{ "); fmt_expr(out, e, depth); out.push_str(" }"); return; } } }
    fmt_block(out, stmts, expr, depth);
}

fn fmt_expr_fan(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::Fan { exprs, .. } = &expr.kind else { unreachable!() };
    out.push_str("fan {\n");
    for e in exprs {
        out.push_str(&ind(depth + 1)); fmt_expr(out, e, depth + 1); out.push('\n');
    }
    out.push_str(&ind(depth)); out.push('}');
}

fn fmt_expr_fan_bounded(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::FanBounded { budget, body } = &expr.kind else { unreachable!() };
    out.push_str("fan.bounded(");
    fmt_expr(out, budget, depth);
    out.push_str(") ");
    // The body parses as a Block (T2-1): a one-expr block prints inline
    // (`{ work(x) }`), a statement block prints as itself — either way the
    // Block arm supplies its own braces.
    match &body.kind {
        ExprKind::Block { .. } => fmt_expr(out, body, depth),
        _ => {
            out.push_str("{ ");
            fmt_expr(out, body, depth);
            out.push_str(" }");
        }
    }
}

fn fmt_expr_fan_race(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::FanRace { budget, arms } = &expr.kind else { unreachable!() };
    out.push_str("fan.race");
    if let Some(b) = budget {
        out.push('(');
        fmt_expr(out, b, depth);
        out.push(')');
    }
    out.push_str(" {
");
    for e in arms {
        out.push_str(&ind(depth + 1)); fmt_expr(out, e, depth + 1); out.push('\n');
    }
    out.push_str(&ind(depth)); out.push('}');
}

fn fmt_expr_fan_race_map(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::FanRaceMap { budget, list, mapper } = &expr.kind else { unreachable!() };
    out.push_str("fan.race(");
    if let Some(b) = budget {
        fmt_expr(out, b, depth);
        out.push_str(", ");
    }
    fmt_expr(out, list, depth);
    out.push_str(", ");
    fmt_expr(out, mapper, depth);
    out.push(')');
}

fn fmt_expr_fan_timeout(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::FanTimeout { deadline, body } = &expr.kind else { unreachable!() };
    out.push_str("fan.timeout(");
    fmt_expr(out, deadline, depth);
    out.push_str(") ");
    match &body.kind {
        ExprKind::Block { .. } => fmt_expr(out, body, depth),
        _ => {
            out.push_str("{ ");
            fmt_expr(out, body, depth);
            out.push_str(" }");
        }
    }
}

fn fmt_expr_fan_settle(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::FanSettle { arms } = &expr.kind else { unreachable!() };
    out.push_str("fan.settle {\n");
    for e in arms {
        out.push_str(&ind(depth + 1)); fmt_expr(out, e, depth + 1); out.push('\n');
    }
    out.push_str(&ind(depth)); out.push('}');
}

fn fmt_expr_forin(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::ForIn { var, var_tuple, iterable, body, .. } = &expr.kind else { unreachable!() };
    out.push_str("for ");
    if let Some(n) = var_tuple { w!(out, "({})", join_syms(n, ", ")); } else { out.push_str(var); }
    out.push_str(" in "); fmt_expr(out, iterable, depth); out.push_str(" {\n");
    for s in body { fmt_stmt(out, s, depth + 1); }
    w!(out, "{}}}", ind(depth));
}

fn fmt_expr_while(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::While { cond, body, .. } = &expr.kind else { unreachable!() };
    out.push_str("while "); fmt_expr(out, cond, depth); out.push_str(" {\n");
    for s in body { fmt_stmt(out, s, depth + 1); }
    w!(out, "{}}}", ind(depth));
}

fn fmt_expr_lambda(out: &mut String, expr: &Expr, depth: usize) {
    let ExprKind::Lambda { params, body, .. } = &expr.kind else { unreachable!() };
    // #1111's parser synthesis, inverted: a bare builtin ctor parses as
    // `(__ctor_arg) => some(__ctor_arg)` — printing that form would leak
    // the internal name into user source (it did: the C-322 fixtures on
    // 2026-08-28), so the exact synthesized shape prints back as the
    // bare ctor. A user-written lambda never carries the reserved
    // `__ctor_arg` spelling, and even if one did, the two forms are
    // semantically identical.
    if let [p] = params.as_slice()
        && p.name.as_str() == "__ctor_arg"
        && p.ty.is_none()
        && p.tuple_names.is_none()
    {
        let inner = match &body.kind {
            ExprKind::Some { expr } => Some(("some", expr)),
            ExprKind::Ok { expr } => Some(("ok", expr)),
            ExprKind::Err { expr } => Some(("err", expr)),
            _ => None,
        };
        if let Some((ctor, inner)) = inner
            && matches!(&inner.kind, ExprKind::Ident { name } if name.as_str() == "__ctor_arg")
        {
            out.push_str(ctor);
            return;
        }
    }
    out.push('(');
    comma_sep(out, params, |out, p| {
        if let Some(n) = &p.tuple_names { w!(out, "({})", join_syms(n, ", ")); } else { out.push_str(&p.name); }
        if let Some(ref ty) = p.ty { out.push_str(": "); fmt_type(out, ty, depth); }
    });
    out.push_str(") => "); fmt_expr(out, body, depth);
}

fn fmt_expr_type_ascription(out: &mut String, expr: &Expr, depth: usize) {
    // Parenthesize so the ascription re-parses in EVERY position, not just
    // as a bare call argument: `([]: List[String])` is valid as a record-
    // field value / `let` initializer, while the bare `[]: List[String]`
    // there is a parse error (the `:` is unexpected). `(expr: Type)` parses
    // anywhere an expression does, so this is safe + idempotent (#437).
    let ExprKind::TypeAscription { expr, ty } = &expr.kind else { unreachable!() };
    out.push('(');
    fmt_expr(out, expr, depth);
    out.push_str(": ");
    fmt_type(out, ty, depth);
    out.push(')');
}

fn fmt_block(out: &mut String, stmts: &[Stmt], expr: &Option<Box<Expr>>, depth: usize) {
    out.push_str("{\n");
    for s in stmts { fmt_stmt(out, s, depth + 1); }
    if let Some(e) = expr { out.push_str(&ind(depth + 1)); fmt_expr(out, e, depth + 1); out.push('\n'); }
    w!(out, "{}}}", ind(depth));
}

/// Does `id` carry LEADING comments (the #1714 own-line element-introducer
/// shape)? Decides multi-line forcing for list/map literals: an own-line `//`
/// comment has nowhere to go on one line — the record-type precedent
/// (`fmt_record_type`).
fn has_leading_comments(id: ExprId) -> bool {
    comments_for(id).is_some_and(|a| !a.leading.is_empty())
}

/// Emit `id`'s leading comments on their own lines at `depth` — the element
/// position their author wrote them in. The caller then renders the node via
/// [`fmt_expr_sans_leading`] so the generic inline-leading printer in
/// [`fmt_expr`] does not print them a second time.
fn emit_leading_comment_lines(out: &mut String, id: ExprId, depth: usize) {
    if let Some(a) = comments_for(id) {
        for c in &a.leading {
            out.push_str(&ind(depth));
            out.push_str(c);
            out.push('\n');
        }
    }
}

/// Render `expr` with its LEADING comments already emitted own-line by the
/// caller; trailing stays inline exactly as [`fmt_expr`] prints it.
fn fmt_expr_sans_leading(out: &mut String, expr: &Expr, depth: usize) {
    fmt_expr_inner(out, expr, depth);
    if let Some(a) = comments_for(expr.id) {
        for c in &a.trailing {
            out.push(' ');
            out.push_str(c);
        }
    }
}

fn fmt_list(out: &mut String, elements: &[Expr], depth: usize) {
    if elements.is_empty() { out.push_str("[]"); return; }
    let any_leading = elements.iter().any(|e| has_leading_comments(e.id));
    if !any_leading && elements.len() <= 5 && elements.iter().all(is_short) {
        out.push('['); comma_sep(out, elements, |out, e| fmt_expr(out, e, depth)); out.push(']');
    } else {
        out.push_str("[\n");
        for (i, e) in elements.iter().enumerate() {
            emit_leading_comment_lines(out, e.id, depth + 1);
            out.push_str(&ind(depth + 1)); fmt_expr_sans_leading(out, e, depth + 1);
            if i < elements.len() - 1 { out.push(','); } out.push('\n');
        }
        w!(out, "{}]", ind(depth));
    }
}

fn fmt_map(out: &mut String, entries: &[(Expr, Expr)], depth: usize) {
    let any_leading = entries.iter().any(|(k, _)| has_leading_comments(k.id));
    let short = !any_leading && entries.len() <= 3 && entries.iter().all(|(k, v)| is_short(k) && is_short(v));
    let (open, close, d) = if short { ("[", "]", depth) } else { ("[\n", "]", depth + 1) };
    out.push_str(open);
    for (i, (k, v)) in entries.iter().enumerate() {
        if short { if i > 0 { out.push_str(", "); } }
        else {
            emit_leading_comment_lines(out, k.id, d);
            out.push_str(&ind(d));
        }
        if short { fmt_expr(out, k, d); } else { fmt_expr_sans_leading(out, k, d); }
        out.push_str(": "); fmt_expr(out, v, d);
        if !short { if i < entries.len() - 1 { out.push(','); } out.push('\n'); }
    }
    if !short { out.push_str(&ind(depth)); }
    out.push_str(close);
}

/// Escape an interpolated string's LITERAL run. Note the escape set differs from
/// [`push_escaped_char`]'s deliberately: a `${` inside an interpolated literal is
/// already a real interpolation boundary the parser consumed, so `$` stays literal
/// here. Extracted to flatten the for-inside-match-inside-for nesting.
fn push_escaped_lit(out: &mut String, value: &str, quote: char) {
    let mut it = value.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            // A literal `${` in a double-quoted string must not round-trip
            // into a live interpolation hole (#1076). Single quotes never
            // interpolate, and their lexer keeps `\$` as two characters, so
            // the escape is double-quote-only.
            '$' if quote == '"' && it.peek() == Some(&'{') => out.push_str("\\$"),
            c if c == quote => { out.push('\\'); out.push(c); }
            c => out.push(c),
        }
    }
}

/// Reprint an interpolated literal from its verbatim source spelling, but
/// RE-FORMAT the code inside each `${…}` hole.
///
/// The literal runs must survive byte-for-byte — that is the whole point of
/// `raw` (quote style, heredoc form, `\u{3042}` escapes: #1261/#1263). A hole
/// is not a literal run though, it is CODE, and a formatter that stops
/// normalizing code the moment it sits inside a string has a blind spot
/// exactly where interpolation-heavy Almide lives (`"${ a  +  b }"` would
/// stay crooked forever). So the raw is copied verbatim everywhere except at
/// the holes, which are re-rendered through `fmt_expr`.
///
/// **Heredoc and raw-string forms are copied whole.** A heredoc's value is
/// computed by stripping the COMMON leading indent of its lines, so a hole
/// that re-renders across lines can change the strip amount — i.e. change the
/// string's value. Not re-formatting is a cosmetic loss; changing a value is a
/// miscompile, so the conservative direction wins for those two forms.
///
/// The hole scan mirrors `parse_interpolation_parts` exactly: `\\` and `\$`
/// arrive as undecoded pairs and never open a hole, and a nested string
/// literal inside a hole is consumed atomically by the lexer's own scanner
/// (shared, so the two walks can never disagree on where a literal ends). If
/// the counts still disagree — the parse-error recovery path turns a bad hole
/// into a `Lit` — the whole raw is copied verbatim: losing a re-format is
/// safe, splicing an expression into the wrong hole is not.
fn fmt_istring_raw(out: &mut String, raw: &str, parts: &[StringPart], depth: usize) {
    if raw.starts_with("\"\"\"") || raw.starts_with('r') {
        out.push_str(raw);
        return;
    }
    let exprs: Vec<&Expr> = parts
        .iter()
        .filter_map(|p| match p {
            StringPart::Expr { expr } => Some(&**expr),
            StringPart::Lit { .. } => None,
        })
        .collect();
    let chars: Vec<char> = raw.chars().collect();
    let mut buf = String::new();
    let mut i = 0usize;
    let mut next = 0usize;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() && (chars[i + 1] == '\\' || chars[i + 1] == '$') {
            buf.push(chars[i]);
            buf.push(chars[i + 1]);
            i += 2;
        } else if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
            let Some(expr) = exprs.get(next) else { out.push_str(raw); return };
            next += 1;
            i = skip_interpolation_hole(&chars, i);
            buf.push_str("${");
            fmt_expr(&mut buf, expr, depth);
            buf.push('}');
        } else {
            buf.push(chars[i]);
            i += 1;
        }
    }
    if next == exprs.len() { out.push_str(&buf); } else { out.push_str(raw); }
}

/// Advance past one `${…}` hole, `start` sitting on the `$`. Brace-depth scan
/// with nested string literals consumed atomically — the same walk the parser
/// runs in `parse_interpolation_expr_part`.
///
/// One difference the parser does not have to care about: it walks the DECODED
/// template, this walks the RAW, so a nested literal may arrive either bare
/// (`"${f("ab")}"`) or backslash-escaped (`"${v ?? \"?\"}"`). An escape pair is
/// therefore skipped WHOLE before the quote test — otherwise the `"` of a `\"`
/// opens a nested-literal scan that runs off the end of the hole and swallows
/// the literal's own closing delimiter.
fn skip_interpolation_hole(chars: &[char], start: usize) -> usize {
    let mut i = start + 2;
    let mut depth = 1usize;
    let mut scratch = String::new();
    while i < chars.len() && depth > 0 {
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 2;
            continue;
        }
        if chars[i] == '"' || chars[i] == '\'' {
            i = almide_lang::lexer::scan_nested_string_literal(chars, i, &mut scratch);
            continue;
        }
        if chars[i] == '{' { depth += 1; }
        if chars[i] == '}' {
            depth -= 1;
            if depth == 0 { break; }
        }
        i += 1;
    }
    i + 1
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
            StringPart::Lit { value } => push_escaped_lit(out, value, quote),
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
    // A NEWLINE alone separates statements — the `;` the parser also accepts is optional
    // (`parse_block` consumes one if present, then keeps going on the newline either way).
    // Emitting it wrote a form that appears in no hand-written Almide, no stdlib module and
    // no doc example, so `almide fmt` rewrote idiomatic source into a style the project does
    // not use — which is why a tree-wide `fmt --check` gate could never be green (#919:
    // 313 of 317 spec/wasm_cross files differed, every diff only this).
    out.push('\n');
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
        Pattern::Or { alts } => {
            for (i, a) in alts.iter().enumerate() {
                if i > 0 { out.push_str(" | "); }
                fmt_pattern(out, a);
            }
        }
        Pattern::Tuple { elements } => { out.push('('); comma_sep(out, elements, |out, e| fmt_pattern(out, e)); out.push(')'); }
        Pattern::List { elements, rest } => {
            out.push('[');
            comma_sep(out, elements, |out, e| fmt_pattern(out, e));
            if let Some(r) = rest {
                if !elements.is_empty() {
                    out.push_str(", ");
                }
                out.push_str("..");
                if let Some(name) = r {
                    out.push_str(name.as_str());
                }
            }
            out.push(']');
        }
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

