//! The structural witness's SUBSET GATE (#1696 step 4): which function
//! bodies the recorder (witness.rs) may certify, decided before emission.
//! A body is admitted only when every RC-affecting site its lowering can
//! reach is a recorder hook; anything else declines with a counted reason
//! (`!decline:<reason>`, the histogram in golden/witness-declines.txt).

use almide_ir::{IrExpr, IrExprKind, IrStmtKind};

/// The phase-A/B1 subset gate: `None` = the body is straight-line and
/// every RC-affecting site is covered by the recorder hooks (bind,
/// call-argument, store, tail, epilogue / tail-release); `Some(reason)` =
/// out of subset, do not record. Deliberately conservative — admitting a
/// shape here without auditing its RC sites would let the witness
/// under-count real events, which is the one dishonesty the recorder
/// exists to rule out.
///
/// #2755 (step 4, the flat alphabet on temporaries): the value forms are
/// one recursive predicate, [`value_subset`]. A nested call argument, a
/// binary operator, a constructor (`some` / `ok` / `err` / a variant
/// case) and a `{ let …; v }` block (what arg_temps.rs makes of a call
/// operand) are admitted wherever a value is, because every RC site they
/// reach is already a hook: an inner call's arguments are the call
/// hooks', its owned result is the temporary the enclosing site records
/// (`im` into an owned param or a payload slot, `id` when parked for a
/// borrowed one), a payload store is `witness_store`, a nested bind is the
/// Bind hook, and a concat reads its operands without a credit.
pub fn straightline_subset(body: &IrExpr, ret_is_heap: bool) -> Option<String> {
    // `fn f(x) = expr` lowers exactly like `{ expr }`: a bare body is the
    // empty-statement block with that tail (B1: the tail-call and
    // literal-tail fns are almost all written this way).
    let (stmts, expr): (&[almide_ir::IrStmt], Option<&IrExpr>) = match &body.kind {
        IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
        _ => (&[], Some(body)),
    };
    if let Some(r) = stmts_subset(stmts) {
        return Some(r);
    }
    match expr.map(|t| &t.kind) {
        // A heap return is admitted only as a plain bound Var (the
        // ret-inc + move-out pair the func.rs hook records) or an OWNED
        // value (its one credit moves out); any other heap tail has
        // unrecorded RC sites.
        None | Some(IrExprKind::Unit) if !ret_is_heap => None,
        None => Some("tail:Unit-heap".into()),
        // A SELF tail call is loop-converted (tco.rs) — and certified as
        // what it means (#2757): the next activation of this frame. The
        // exit plan releases the params, the arguments move into the next
        // activation's params (the callee-owned convention), and each owner
        // local the loop-back carries is released by its next rebind or
        // the epilogue (`WitnessRecorder::loop_back`). It is an ordinary
        // tail call to the gate.
        // A scalar literal is no heap tail at all.
        Some(IrExprKind::LitInt { .. } | IrExprKind::LitBool { .. } | IrExprKind::LitFloat { .. })
            if ret_is_heap =>
        {
            Some("tail:scalar-lit-heap".into())
        }
        // A block tail: its statements join the frame's straight line, its
        // value is the tail's (rc_tail — the func.rs hooks read through it).
        Some(IrExprKind::Block { .. }) => straightline_subset(expr?, ret_is_heap),
        // #2756: a branch in tail position — each arm is a tail of its own
        // (a self call in an arm is the loop form, declined there).
        Some(IrExprKind::If { cond, then, else_ }) => value_subset(cond)
            .map(|w| w.at("if-cond"))
            .or_else(|| straightline_subset(then, ret_is_heap))
            .or_else(|| straightline_subset(else_, ret_is_heap)),
        Some(IrExprKind::Match { subject, arms }) => match_head_subset(subject, arms).or_else(|| {
            arms.iter().find_map(|a| straightline_subset(&a.body, ret_is_heap))
        }),
        // A Unit body ending in a loop lowers it as a statement (#2757).
        Some(IrExprKind::While { .. } | IrExprKind::ForIn { .. }) if !ret_is_heap => {
            stmt_body_subset(expr?).map(|w| w.at("tail"))
        }
        Some(_) => value_subset(expr?).map(|w| w.at("tail")),
    }
}

/// Why a value is out of subset: the node ITSELF (`Here(tag)`, reported
/// under the position it stands in — `rhs:If`, `call-arg:Lambda`), or a
/// node somewhere inside it (`Deep(reason)`, already reported under ITS
/// innermost position). The histogram thus names the shape to admit next
/// and the slot it sits in, never the path to it.
enum Why {
    Here(String),
    Deep(String),
}

impl Why {
    fn at(self, position: &str) -> String {
        match self {
            Why::Here(t) => format!("{position}:{t}"),
            Why::Deep(r) => r,
        }
    }

    /// The node is inside `position`: a `Here` becomes `Deep` there.
    fn inside(self, position: &str) -> Why {
        Why::Deep(self.at(position))
    }
}

/// The statement rules of a straight line: a Bind of an admissible value,
/// a statement-position call (its owned droppable result is released by
/// the discard route, `id`).
fn stmts_subset(stmts: &[almide_ir::IrStmt]) -> Option<String> {
    for s in stmts {
        match &s.kind {
            IrStmtKind::Bind { value, .. } => {
                if let Some(w) = value_subset(value) {
                    return Some(w.at("rhs"));
                }
            }
            IrStmtKind::Expr { expr } if matches!(expr.kind, IrExprKind::Call { .. }) => {
                if let Some(w) = call_subset(expr) {
                    return Some(w.at("stmt:Expr"));
                }
            }
            // #2756: a statement-position branch; its arms are statement
            // bodies.
            // #2757: loops and their jumps — also statement bodies.
            IrStmtKind::Expr { expr }
                if matches!(
                    expr.kind,
                    IrExprKind::If { .. }
                        | IrExprKind::Match { .. }
                        | IrExprKind::While { .. }
                        | IrExprKind::ForIn { .. }
                        | IrExprKind::Break
                        | IrExprKind::Continue
                ) =>
            {
                if let Some(w) = stmt_body_subset(expr) {
                    return Some(w.at("stmt:Expr"));
                }
            }
            IrStmtKind::Expr { expr } => return Some(format!("stmt:Expr:{}", expr_tag(expr))),
            // #2757: an assignment — the old occupant's release and the new
            // one's binding are the Assign hook's (a var bound outside the
            // loop it is assigned in declines at emission).
            IrStmtKind::Assign { value, .. } => {
                if let Some(w) = value_subset(value) {
                    return Some(w.at("assign"));
                }
            }
            // `guard c else break` / `else continue`: a one-arm branch that
            // leaves the iteration (the loop-control form only; a guard that
            // returns is an exit edge, not recorded yet).
            IrStmtKind::Guard { cond, else_ } if ends_in_loop_ctl(else_) => {
                if let Some(w) = value_subset(cond).map(|w| w.inside("guard-cond")).or_else(|| stmt_body_subset(else_))
                {
                    return Some(w.at("stmt:Guard"));
                }
            }
            // A source comment emits nothing.
            IrStmtKind::Comment { .. } => {}
            other => return Some(format!("stmt:{}", tag(other))),
        }
    }
    None
}

/// A space-free tag of an IR node's variant, for the decline histogram
/// (`grep -o '^!decline:[^ ]*' | sort | uniq -c` over the floor dump).
fn tag<T: std::fmt::Debug>(v: &T) -> String {
    format!("{v:?}").chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect()
}

/// A statement-position expression's tag, one level deeper for a call
/// (its target family is what the next increment chooses by).
fn expr_tag(e: &IrExpr) -> String {
    match &e.kind {
        IrExprKind::Call { target, .. } => format!("Call:{}", tag(target)),
        other => tag(other),
    }
}

/// Is the value a scalar (no block, no RC site of its own)?
fn scalar_ty(t: &almide_types::types::Ty) -> bool {
    use almide_types::types::Ty;
    matches!(
        t,
        Ty::Int
            | Ty::Float
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float32
            | Ty::Float64
            | Ty::Bool
            | Ty::Unit
    )
}

/// A value whose every RC site is a recorder hook (#2755). `Some(tag)` names
/// the innermost shape that is not — the reason the histogram counts, under
/// the caller's position prefix (`rhs:`, `tail:`, `call-arg:` …).
fn value_subset(e: &IrExpr) -> Option<Why> {
    match &e.kind {
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::Unit
        | IrExprKind::Var { .. }
        // `none` is NULL_ADDR: no block, no site.
        | IrExprKind::OptionNone => None,
        // A list literal: the spine is fresh, each element store is
        // `witness_store` exactly like a constructor payload's.
        IrExprKind::List { elements } => elements.iter().find_map(|x| value_subset(x).map(|w| w.inside("list-elem"))),
        // A call: its arguments are the call hooks' (recursively), its
        // droppable result is a received credit (#1986) the enclosing
        // site records.
        IrExprKind::Call { .. } => call_subset(e),
        // A one-slot constructor: the block is fresh (the enclosing site's
        // `i` / `im`), its payload store is `witness_store` — a Var shares
        // and moves in (`am`), an owned temporary moves in (`im`).
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => {
            value_subset(expr).map(|w| w.inside("payload"))
        }
        IrExprKind::BinOp { left, right, .. } => binop_subset(left, right),
        // A scalar negation / `not`: no site of its own.
        IrExprKind::UnOp { operand, .. } => value_subset(operand).map(|w| w.inside("operand")),
        // `{ let t = f(x); op(t) }` — arg_temps.rs's shape: the binds are the
        // Bind hook's (the frame's exit plan releases them), the value is
        // the tail's (`rc_owned_result` and the hooks read through blocks).
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            stmts_subset(stmts).map(Why::Deep).or_else(|| value_subset(tail))
        }
        // #2756: a value-position branch — one site, each arm hands the
        // join one credit (`witness_arm_value`); the condition runs first.
        IrExprKind::If { cond, then, else_ } => value_subset(cond)
            .map(|w| w.inside("if-cond"))
            .or_else(|| value_subset(then))
            .or_else(|| value_subset(else_)),
        IrExprKind::Match { subject, arms } => match_head_subset(subject, arms)
            .map(Why::Deep)
            .or_else(|| arms.iter().find_map(|a| value_subset(&a.body))),
        other => Some(Why::Here(tag(other))),
    }
}

/// A binary operator. The operators read their operands and spend no
/// credit (`$concat` copies, a comparison reads), so a HEAP operand is
/// admitted only as a Var or a pool-static literal: a fresh heap operand
/// would be an unowned temporary no hook records (arg_temps.rs binds every
/// such operand first, so this is the shape the emitter actually sees). A
/// SCALAR operand is any admissible value.
fn binop_subset(left: &IrExpr, right: &IrExpr) -> Option<Why> {
    let operand = |x: &IrExpr| -> Option<Why> {
        if scalar_ty(&x.ty) {
            return value_subset(x).map(|w| w.inside("operand"));
        }
        match &x.kind {
            IrExprKind::Var { .. } | IrExprKind::LitStr { .. } => None,
            other => Some(Why::Deep(format!("heap-operand:{}", tag(other)))),
        }
    };
    if let Some(w) = operand(left) {
        return Some(w);
    }
    // `and` / `or`: the right operand runs on one arm of a branch site the
    // lowering opens for the witness (#2756) — any admissible operand.
    operand(right)
}

/// A value with no RC site anywhere inside (Vars, literals, operators over
/// them): safe to evaluate conditionally inside a straight-line frame.
fn rc_free(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::Var { .. } => true,
        IrExprKind::BinOp { left, right, .. } => rc_free(left) && rc_free(right),
        IrExprKind::UnOp { operand, .. } => rc_free(operand),
        _ => false,
    }
}

/// A call the hooks cover: a Named user fn or variant constructor (the
/// builtin `some`/`ok`/`err` are IR kinds, not calls) or, since step 4, a
/// Module call (the native arms' declared modes are recorded at
/// `lower_arg`; the registry route consults the callee's param_owned
/// table like the Named route), over admissible arguments (#2755: nested
/// calls, operators, constructors — each argument's own sites are hooks,
/// and its owned result is the temporary the argument hook records).
fn call_subset(e: &IrExpr) -> Option<Why> {
    let IrExprKind::Call { target, args, .. } = &e.kind else {
        return Some(Why::Deep("call:not-a-call".into()));
    };
    match target {
        almide_ir::CallTarget::Named { name } => {
            // The http_framed host-op leaves (calls.rs) intercept before
            // resolution and lower their args outside every hook.
            if name.as_str().starts_with("__http_framed_")
                || name.as_str().starts_with("__http_call_")
                || name.as_str().starts_with("__http_serve_")
            {
                return Some(Why::Deep("call:host-splice".into()));
            }
            // `__is_null` reads the Value tag of its lowered argument, and
            // `panic` concatenates its message into a line it never binds:
            // no argument hook fires for either, so only an RC-free
            // argument is honest.
            if matches!(name.as_str(), "__is_null" | "panic") && !args.iter().all(rc_free) {
                return Some(Why::Deep(format!("call:{name}-arg")));
            }
        }
        almide_ir::CallTarget::Module { .. } => {}
        other => return Some(Why::Deep(format!("call:target:{}", tag(other)))),
    }
    for a in args {
        if let Some(w) = value_subset(a) {
            return Some(w.inside("call-arg"));
        }
    }
    None
}



/// A statement body of a branch arm (#2756) or a loop (#2757): a call, a
/// block of admitted statements, a nested branch or loop, a jump, or nothing.
fn stmt_body_subset(e: &IrExpr) -> Option<Why> {
    match &e.kind {
        IrExprKind::Unit | IrExprKind::Break | IrExprKind::Continue => None,
        // A while condition runs at the head of every iteration, the last
        // one being a check that leaves (lower_while records it as such).
        IrExprKind::While { cond, body } => value_subset(cond)
            .map(|w| w.inside("while-cond"))
            .or_else(|| stmts_subset(body).map(Why::Deep)),
        // A map walk shares its subject for the cursor and releases it
        // after the loop — sites the recorder does not hook yet.
        IrExprKind::ForIn { iterable, .. }
            if matches!(
                &iterable.ty,
                almide_types::types::Ty::Applied(almide_types::types::constructor::TypeConstructorId::Map, _)
            ) =>
        {
            Some(Why::Deep("forin-map".into()))
        }
        // A range head is a counting loop over its bounds — no list.
        IrExprKind::ForIn { iterable, body, .. } => match &iterable.kind {
            IrExprKind::Range { start, end, .. } => value_subset(start).or_else(|| value_subset(end)),
            _ => value_subset(iterable),
        }
        .map(|w| w.inside("forin-iter"))
        .or_else(|| stmts_subset(body).map(Why::Deep)),
        IrExprKind::Call { .. } => call_subset(e),
        IrExprKind::Block { stmts, expr } => stmts_subset(stmts)
            .map(Why::Deep)
            .or_else(|| expr.as_deref().and_then(stmt_body_subset)),
        IrExprKind::If { cond, then, else_ } => value_subset(cond)
            .map(|w| w.inside("if-cond"))
            .or_else(|| stmt_body_subset(then))
            .or_else(|| stmt_body_subset(else_)),
        IrExprKind::Match { subject, arms } => match_head_subset(subject, arms)
            .map(Why::Deep)
            .or_else(|| arms.iter().find_map(|a| stmt_body_subset(&a.body))),
        other => Some(Why::Here(tag(other))),
    }
}

/// A match's subject and arm heads (#2756): the subject is evaluated once,
/// before the site; a pattern binds VIEWS of it (patterns.rs, no share, no
/// release) — except a named list rest, a fresh block no owner releases
/// (#2971), declined; a guard runs between two arms' tests, so it must be
/// RC-free.
fn match_head_subset(subject: &IrExpr, arms: &[almide_ir::IrMatchArm]) -> Option<String> {
    if let Some(w) = value_subset(subject) {
        return Some(w.at("match-subject"));
    }
    for a in arms {
        if pattern_has_named_rest(&a.pattern) {
            return Some("pattern:list-rest".into());
        }
        if a.guard.as_ref().is_some_and(|g| !rc_free(g)) {
            return Some("match-guard".into());
        }
    }
    None
}

fn pattern_has_named_rest(p: &almide_ir::IrPattern) -> bool {
    use almide_ir::IrPattern as P;
    match p {
        P::List { elements, rest } => {
            rest.as_deref().is_some_and(|r| !matches!(r, P::Wildcard)) || elements.iter().any(pattern_has_named_rest)
        }
        P::As { inner, .. } | P::Some { inner } | P::Ok { inner } | P::Err { inner } => pattern_has_named_rest(inner),
        P::Constructor { args: ps, .. } | P::Tuple { elements: ps } => ps.iter().any(pattern_has_named_rest),
        P::RecordPattern { fields, .. } => fields.iter().filter_map(|f| f.pattern.as_ref()).any(pattern_has_named_rest),
        P::Bind { .. } | P::Wildcard | P::Literal { .. } | P::None => false,
    }
}

/// Does this guard else leave the enclosing LOOP body (`break` / `continue`,
/// possibly after statements)? The mirror of stmts_loop.rs's predicate.
fn ends_in_loop_ctl(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Break | IrExprKind::Continue => true,
        IrExprKind::Block { expr: Some(tail), .. } => ends_in_loop_ctl(tail),
        IrExprKind::Block { stmts, expr: None } => matches!(
            stmts.last().map(|s| &s.kind),
            Some(IrStmtKind::Expr { expr }) if ends_in_loop_ctl(expr)
        ),
        _ => false,
    }
}
