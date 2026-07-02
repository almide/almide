// Free helper functions for `infer.rs` — block result-match collection, ident
// collection, if-arm fix hints, and collection-type validation. Module-scope
// (not `impl Checker`); `include!`d into `infer.rs`, so imports come from there.

/// Collect Ident names that appear as match subjects with Ok/Err
/// patterns inside a block. Used to suppress auto-unwrap of Result on
/// the corresponding `let` bindings — the user wants to inspect the
/// Result, so the Bind must keep its Result type.
pub(crate) fn collect_block_result_match_vars(stmts: &[ast::Stmt], tail: Option<&ast::Expr>) -> std::collections::HashSet<Sym> {
    let mut out = std::collections::HashSet::new();
    for s in stmts { collect_in_stmt(s, &mut out); }
    if let Some(e) = tail { collect_in_expr(e, &mut out); }
    out
}

fn collect_in_stmt(stmt: &ast::Stmt, out: &mut std::collections::HashSet<Sym>) {
    match stmt {
        ast::Stmt::Let { value, .. } | ast::Stmt::Var { value, .. }
        | ast::Stmt::LetDestructure { value, .. }
        | ast::Stmt::Assign { value, .. }
        | ast::Stmt::FieldAssign { value, .. } => collect_in_expr(value, out),
        ast::Stmt::IndexAssign { index, value, .. } => {
            collect_in_expr(index, out);
            collect_in_expr(value, out);
        }
        ast::Stmt::Expr { expr, .. } => collect_in_expr(expr, out),
        ast::Stmt::Guard { cond, else_, .. } => { collect_in_expr(cond, out); collect_in_expr(else_, out); }
        ast::Stmt::GuardLet { scrutinee, else_, .. } => { collect_in_expr(scrutinee, out); collect_in_expr(else_, out); }
        _ => {}
    }
}

// Traversal must reach a `match r { ok/err }` through EVERY value-carrying
// wrapper — a missed form means the binding of `r` gets auto-unwrapped out
// from under the match (porta mcp: the match sat inside an `ok(...)` tail,
// the old Match/Block/If-only walk never saw it, and the Bind lost its
// Result → invalid native Rust).
fn collect_in_expr(expr: &ast::Expr, out: &mut std::collections::HashSet<Sym>) {
    match &expr.kind {
        ExprKind::Match { subject, arms, .. } => {
            let arms_match_result = arms.iter().any(|a| matches!(
                &a.pattern,
                ast::Pattern::Ok { .. } | ast::Pattern::Err { .. }
            ));
            if arms_match_result {
                if let ExprKind::Ident { name, .. } = &subject.kind {
                    out.insert(*name);
                }
            }
            collect_in_expr(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard { collect_in_expr(g, out); }
                collect_in_expr(&arm.body, out);
            }
        }
        ExprKind::Block { stmts, expr: tail, .. } => {
            for s in stmts { collect_in_stmt(s, out); }
            if let Some(t) = tail { collect_in_expr(t, out); }
        }
        ExprKind::If { cond, then, else_, .. } => {
            collect_in_expr(cond, out);
            collect_in_expr(then, out);
            collect_in_expr(else_, out);
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            collect_in_expr(scrutinee, out);
            collect_in_expr(then, out);
            collect_in_expr(else_, out);
        }
        ExprKind::ForIn { iterable, body, .. } => {
            collect_in_expr(iterable, out);
            for s in body { collect_in_stmt(s, out); }
        }
        ExprKind::While { cond, body, .. } => {
            collect_in_expr(cond, out);
            for s in body { collect_in_stmt(s, out); }
        }
        ExprKind::Paren { expr: inner, .. } | ExprKind::Some { expr: inner, .. }
        | ExprKind::Ok { expr: inner, .. } | ExprKind::Err { expr: inner, .. }
        | ExprKind::Try { expr: inner, .. } | ExprKind::Unwrap { expr: inner, .. }
        | ExprKind::ToOption { expr: inner, .. } | ExprKind::Await { expr: inner, .. }
        | ExprKind::TypeAscription { expr: inner, .. }
        | ExprKind::OptionalChain { expr: inner, .. } => collect_in_expr(inner, out),
        ExprKind::UnwrapOr { expr: inner, fallback, .. } => {
            collect_in_expr(inner, out);
            collect_in_expr(fallback, out);
        }
        ExprKind::Unary { operand, .. } => collect_in_expr(operand, out),
        ExprKind::Binary { left, right, .. } | ExprKind::Pipe { left, right, .. }
        | ExprKind::Compose { left, right, .. } => {
            collect_in_expr(left, out);
            collect_in_expr(right, out);
        }
        ExprKind::Range { start, end, .. } => {
            collect_in_expr(start, out);
            collect_in_expr(end, out);
        }
        ExprKind::Call { callee, args, named_args, .. } => {
            collect_in_expr(callee, out);
            for a in args { collect_in_expr(a, out); }
            for (_, a) in named_args { collect_in_expr(a, out); }
        }
        ExprKind::Member { object, .. } | ExprKind::TupleIndex { object, .. } => collect_in_expr(object, out),
        ExprKind::IndexAccess { object, index, .. } => {
            collect_in_expr(object, out);
            collect_in_expr(index, out);
        }
        ExprKind::List { elements, .. } | ExprKind::Tuple { elements, .. } => {
            for e in elements { collect_in_expr(e, out); }
        }
        ExprKind::Fan { exprs, .. } => {
            for e in exprs { collect_in_expr(e, out); }
        }
        ExprKind::MapLiteral { entries, .. } => {
            for (k, v) in entries { collect_in_expr(k, out); collect_in_expr(v, out); }
        }
        ExprKind::Record { fields, .. } => {
            for f in fields { collect_in_expr(&f.value, out); }
        }
        ExprKind::SpreadRecord { base, fields, .. } => {
            collect_in_expr(base, out);
            for f in fields { collect_in_expr(&f.value, out); }
        }
        ExprKind::Lambda { body, .. } => collect_in_expr(body, out),
        ExprKind::InterpolatedString { parts, .. } => {
            for p in parts {
                if let ast::StringPart::Expr { expr } = p { collect_in_expr(expr, out); }
            }
        }
        _ => {}
    }
}

/// Collect all Ident names referenced in an expression (shallow, for var capture check).
fn collect_idents(expr: &ast::Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Ident { name, .. } => out.push(name.to_string()),
        ExprKind::Call { callee, args, .. } => {
            collect_idents(callee, out);
            for a in args { collect_idents(a, out); }
        }
        ExprKind::Member { object, .. } | ExprKind::TupleIndex { object, .. }
        | ExprKind::IndexAccess { object, .. } => collect_idents(object, out),
        ExprKind::Binary { left, right, .. } | ExprKind::Pipe { left, right, .. } | ExprKind::Compose { left, right, .. } => {
            collect_idents(left, out); collect_idents(right, out);
        }
        ExprKind::Unary { operand, .. } | ExprKind::Paren { expr: operand, .. }
        | ExprKind::Some { expr: operand, .. } | ExprKind::Ok { expr: operand, .. }
        | ExprKind::Err { expr: operand, .. } | ExprKind::Try { expr: operand, .. } => {
            collect_idents(operand, out);
        }
        ExprKind::If { cond, then, else_, .. } => {
            collect_idents(cond, out); collect_idents(then, out); collect_idents(else_, out);
        }
        ExprKind::List { elements, .. } | ExprKind::Tuple { elements, .. } => {
            for e in elements { collect_idents(e, out); }
        }
        ExprKind::Lambda { body, .. } => collect_idents(body, out),
        ExprKind::InterpolatedString { parts, .. } => {
            for p in parts { if let ast::StringPart::Expr { expr } = p { collect_idents(expr, out); } }
        }
        ExprKind::Record { fields, .. } => { for f in fields { collect_idents(&f.value, out); } }
        _ => {} // literals, none, unit, etc.
    }
}

/// If an `if/else` arm is a statement-only block that assigns to a variable
/// (e.g. `{ high = mid - 1 }`), return its target name. This is the dojo
/// binary-search / matrix-ops pattern: an arm does a side-effect instead of
/// producing a value, so the whole if-expr types as Unit.
fn arm_assign_target(expr: &ast::Expr) -> Option<String> {
    let ExprKind::Block { stmts, expr: tail } = &expr.kind else { return None };
    if tail.is_some() { return None; }
    // Only single-statement blocks are unambiguous: `{ x = v }`. Multi-stmt
    // blocks might legitimately produce a value via a trailing stmt we
    // don't pattern-match, so skip to avoid false claims.
    if stmts.len() != 1 { return None; }
    match &stmts[0] {
        ast::Stmt::Assign { name, .. } => Some(name.to_string()),
        ast::Stmt::Let { name, .. } | ast::Stmt::Var { name, .. } => Some(name.to_string()),
        _ => None,
    }
}

fn if_arm_fix_hint(then: &ast::Expr, else_: &ast::Expr) -> Option<FixHint> {
    let then_tgt = arm_assign_target(then);
    let else_tgt = arm_assign_target(else_);
    match (then_tgt, else_tgt) {
        (Some(t), Some(e)) => Some(FixHint::IfArmsAssign {
            then_var: Some(t), else_var: Some(e),
        }),
        (Some(t), None) => Some(FixHint::IfArmAssign {
            arm: IfArm::Then, var_name: t,
        }),
        (None, Some(e)) => Some(FixHint::IfArmAssign {
            arm: IfArm::Else, var_name: e,
        }),
        (None, None) => None,
    }
}

/// True if `ty` mentions a function type anywhere (directly or nested in a
/// container/tuple/record). Such a type can't be hashed or compared.
fn ty_mentions_fn(ty: &Ty) -> bool {
    match ty {
        Ty::Fn { .. } => true,
        Ty::Tuple(ts) => ts.iter().any(ty_mentions_fn),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| ty_mentions_fn(t)),
        Ty::Applied(_, args) | Ty::Named(_, args) => args.iter().any(ty_mentions_fn),
        _ => false,
    }
}

/// `Some((message, hint))` if `ty` (or a nested type) uses a function where a
/// comparable/hashable type is required: a `Set` element or a `Map` key.
/// Closures are allowed as `Map` values, so only the key (arg 0) is checked.
fn invalid_collection_type(ty: &Ty) -> Option<(&'static str, &'static str)> {
    match ty {
        Ty::Applied(TypeConstructorId::Set, args) if args.len() == 1 && ty_mentions_fn(&args[0]) => {
            return Some((
                "a `Set` element type cannot contain a function — closures have no equality or hashing",
                "Closures can't be deduplicated. Keep them in a `List`, or build the set from a comparable key.",
            ));
        }
        Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 && ty_mentions_fn(&args[0]) => {
            return Some((
                "a `Map` key type cannot contain a function — closures have no equality or hashing",
                "Closures are fine as `Map` values; only the key must be comparable.",
            ));
        }
        _ => {}
    }
    let children: Vec<&Ty> = match ty {
        Ty::Tuple(ts) => ts.iter().collect(),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().map(|(_, t)| t).collect(),
        Ty::Applied(_, args) | Ty::Named(_, args) => args.iter().collect(),
        Ty::Fn { params, ret } => params.iter().chain(std::iter::once(ret.as_ref())).collect(),
        _ => Vec::new(),
    };
    for c in children {
        if let Some(e) = invalid_collection_type(c) { return Some(e); }
    }
    None
}

/// If `callee` names a generic collection constructor whose element type NO
/// argument can constrain — `set.new()` (`Set[A]`) or `list.with_capacity(n)`
/// (`List[A]`, the only arg being the capacity) — return its
/// [`EmptyCollectionKind`] so the call is registered for the undecidable-empty
/// check (E018). Any other callee returns `None`. Matched structurally on the
/// `module.func` member form the parser produces for stdlib calls.
fn empty_collection_ctor_kind(callee: &ast::Expr) -> Option<super::EmptyCollectionKind> {
    let ExprKind::Member { object, field } = &callee.kind else { return None; };
    let ExprKind::Ident { name: module } = &object.kind else { return None; };
    match (module.as_str(), field.as_str()) {
        ("set", "new") => Some(super::EmptyCollectionKind::SetNew),
        ("list", "with_capacity") => Some(super::EmptyCollectionKind::ListWithCapacity),
        // `map.new()` is the constructor analogue of the empty `[:]` literal —
        // a generic `Map[K, V]` with no key/value-bearing argument. Same fix
        // family as the map literal (annotate the key/value types).
        ("map", "new") => Some(super::EmptyCollectionKind::MapLiteral),
        _ => None,
    }
}
