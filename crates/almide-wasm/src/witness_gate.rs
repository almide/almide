//! The structural witness's SUBSET GATE (#1696 step 4): which function
//! bodies the recorder (witness.rs) may certify, decided before emission.
//! A body is admitted only when every RC-affecting site its lowering can
//! reach is a recorder hook; anything else declines with a counted reason
//! (`!decline:<reason>`, the histogram in golden/witness-declines.txt).

use almide_ir::{IrExpr, IrExprKind, IrStmtKind};

/// The inlined-callback rules (split for the file budget).
#[path = "witness_gate_callbacks.rs"]
mod callbacks;
use callbacks::{inline_callback_subset, is_self_hosted_hof};

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

/// A write statement's value (and index). `x = v` (#2757): the old
/// occupant's release and the new one's binding are the Assign hook's (a var
/// bound outside the loop it is assigned in is loop-carried). #2755: `xs[i] =
/// v` — the copy-on-write judge rebinds the var (`witness_mut_rebind`), the
/// value moves into the slot (`witness_store`), an out-of-range index aborts;
/// `h.f = v` — the copy-on-write field write rebinds the root var
/// (`witness_field_rebind`), the value moves into the copy's slot
/// (`witness_field_value`). #2758: `m[k] = v` — the in-place window
/// (map_inplace.rs) lowers the key and the value through `lower_arg`
/// (`Retain`) and rebinds the var (`witness_mut_rebind`), exactly the
/// `map.insert(m, k, v)` route; its functional fallback declines at emission.
fn write_subset(s: &IrStmtKind) -> Option<String> {
    match s {
        IrStmtKind::Assign { value, .. } => value_subset(value).map(|w| w.at("assign")),
        IrStmtKind::IndexAssign { index, value, .. } => {
            value_subset(index).or_else(|| value_subset(value)).map(|w| w.at("index-assign"))
        }
        IrStmtKind::FieldAssign { value, .. } => value_subset(value).map(|w| w.at("field-assign")),
        IrStmtKind::MapInsert { key, value, .. } => value_subset(key).or_else(|| value_subset(value)).map(|w| w.at("map-insert")),
        _ => None,
    }
}

/// `guard c else …`: a one-arm site. `else break` / `else continue` leaves
/// the iteration; any other else (#2755) settles its value like a tail and
/// leaves the frame through the exit plan (stmts.rs `lower_stmt_guard`).
fn guard_subset(cond: &IrExpr, else_: &IrExpr) -> Option<Why> {
    value_subset(cond).map(|w| w.inside("guard-cond")).or_else(|| {
        if ends_in_loop_ctl(else_) {
            stmt_body_subset(else_)
        } else {
            value_subset(else_)
        }
    })
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
                        | IrExprKind::Block { .. }
                        | IrExprKind::Try { .. }
                        | IrExprKind::Unwrap { .. }
                ) =>
            {
                if let Some(w) = stmt_body_subset(expr) {
                    return Some(w.at("stmt:Expr"));
                }
            }
            IrStmtKind::Expr { expr } => return Some(format!("stmt:Expr:{}", expr_tag(expr))),
            IrStmtKind::Assign { .. }
            | IrStmtKind::IndexAssign { .. }
            | IrStmtKind::FieldAssign { .. }
            | IrStmtKind::MapInsert { .. } => {
                if let Some(why) = write_subset(&s.kind) {
                    return Some(why);
                }
            }
            IrStmtKind::Guard { cond, else_ } => {
                if let Some(w) = guard_subset(cond, else_) {
                    return Some(w.at("stmt:Guard"));
                }
            }
            // `let (a, b) = t`: arg_temps.rs names every droppable subject
            // first (`let t = …` is the Bind hook's, the exit plan releases
            // it), so the subject is a Var and each binder is a VIEW of one
            // of its slots (patterns.rs, `local.set`, no share, no release) —
            // exactly a match arm's binders. A named list rest is a fresh
            // block no owner releases (#2971).
            IrStmtKind::BindDestructure { pattern, value } => {
                if pattern_has_named_rest(pattern) {
                    return Some("pattern:list-rest".into());
                }
                let subject = crate::rc_ownership::rc_tail(value);
                if !matches!(subject.kind, IrExprKind::Var { .. }) && !scalar_ty(&subject.ty) {
                    return Some(format!("destructure-subject:{}", tag(&subject.kind)));
                }
                if let Some(w) = value_subset(value) {
                    return Some(w.at("destructure-subject"));
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
        | IrExprKind::OptionNone
        // #2758: a Fn value is a fresh env block (or a pool static when it
        // captures nothing); each capture's share into the env is the
        // capture hook's. The lambda's body is a frame of its own.
        | IrExprKind::Lambda { .. }
        | IrExprKind::FnRef { .. } => None,
        // A list literal: the spine is fresh, each element store is
        // `witness_store` exactly like a constructor payload's.
        IrExprKind::List { elements } => elements.iter().find_map(|x| value_subset(x).map(|w| w.inside("list-elem"))),
        // A tuple / record literal: the same shape as a list literal — a
        // fresh block (the enclosing site's `i` / `im`), each slot store is
        // `witness_store` after the share guard (data.rs). A record field
        // the literal omits is its declaration default, lowered at emission
        // where the gate cannot see it: `witness_record_default` declines
        // one that is not a literal.
        IrExprKind::Tuple { elements } => elements.iter().find_map(|x| value_subset(x).map(|w| w.inside("tuple-elem"))),
        IrExprKind::EmptyMap | IrExprKind::MapLiteral { .. } | IrExprKind::Range { .. } | IrExprKind::ToOption { .. } => {
            built_value_subset(e)
        }
        IrExprKind::Record { fields, .. } => {
            fields.iter().find_map(|(_, x)| value_subset(x).map(|w| w.inside("field")))
        }
        // An interpolation builds in the line buffer and captures the text
        // as a fresh block (emitter.rs), or prints it with no block at all
        // (`println`): the build reads each part and spends no credit, so a
        // part obeys the operator-operand rule — a heap part is a Var or a
        // pool static (arg_temps.rs binds every produced part first).
        IrExprKind::StringInterp { parts } => parts.iter().find_map(|p| match p {
            almide_ir::IrStringPart::Expr { expr } => read_operand(expr, "interp-part"),
            almide_ir::IrStringPart::Lit { .. } => None,
        }),
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
        // #2758: `!` / `?` — a one-arm branch whose arm propagates (the exit
        // plan's releases, the err block's move-out; witness_unwrap.rs).
        // The operand is a bound carrier (arg_temps.rs parks every
        // non-tail `f(x)!`; the payload is then a view) or, in tail
        // position, the call itself (an owned carrier). A call typed with
        // its raw payload (a move-mode effect call, mut_param.rs) still
        // returns its Result block at the ABI: an owned carrier the same
        // route takes (#2758, `extraction_or_rt_subset`).
        IrExprKind::UnwrapOr { .. } | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } | IrExprKind::RuntimeCall { .. } => {
            extraction_or_rt_subset(e)
        }
        IrExprKind::IndexAccess { object, index } => index_subset(object, index),
        // #2755: `m[k]` is exactly `map.get(m, k)` (emitter.rs): its arguments
        // are the arm's argument hooks, its owned Option cell the consumer's.
        IrExprKind::MapAccess { object, key } => {
            [object, key].into_iter().find_map(|a| value_subset(a).map(|w| w.inside("call-arg")))
        }
        // #2755: `r.f` / `t.0` over a bound block (or a chain of such reads):
        // a VIEW of the slot, like an element read, with no abort edge.
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => slot_subset(e, object),
        other => Some(Why::Here(tag(other))),
    }
}

/// #2755: the values a route BUILDS from its operands. `[]` of a map is a
/// fresh empty block; `["k": v, …]` lowers as `map.from_list` over a fresh
/// pairs list (emitter_values.rs) — a borrowed temporary of the arm (`id`)
/// whose tuple slots are `witness_store`s; a range is a fresh Int list over
/// its bounds (ranges.rs), whose overflow abort is a recorded terminal; `r?`
/// converts its carrier (data.rs `witness_to_option`).
fn built_value_subset(e: &IrExpr) -> Option<Why> {
    match &e.kind {
        IrExprKind::MapLiteral { entries } => entries
            .iter()
            .find_map(|(k, v)| value_subset(k).or_else(|| value_subset(v)).map(|w| w.inside("map-entry"))),
        IrExprKind::Range { start, end, .. } => {
            value_subset(start).or_else(|| value_subset(end)).map(|w| w.inside("range-bound"))
        }
        IrExprKind::ToOption { expr } => value_subset(expr).map(|w| w.inside("to-option")),
        _ => None,
    }
}

/// #2755: `xs[i]` over a bound list (arg_temps.rs names a produced object
/// first): the element is a VIEW of the list's slot — a consumer that keeps
/// it shares it (`is_extraction_view`), a reader spends nothing — and an
/// out-of-bounds index aborts after the frame's owners are released, a
/// recorded exit arm. A Bytes index has its own unrecorded abort.
fn index_subset(object: &IrExpr, index: &IrExpr) -> Option<Why> {
    let core = crate::rc_ownership::rc_tail(object);
    if !crate::witness_unwrap::slot_read_of_var(object) {
        return Some(Why::Here(format!("IndexAccess-object:{}", tag(&core.kind))));
    }
    if !matches!(&core.ty, almide_types::types::Ty::Applied(almide_types::types::constructor::TypeConstructorId::List, _)) {
        return Some(Why::Here("IndexAccess:non-list".into()));
    }
    value_subset(object).or_else(|| value_subset(index).map(|w| w.inside("index")))
}

/// `r.f` / `t.0`: the slot load of a bound block (data.rs `lower_record`) —
/// no RC site, a VIEW its consumer shares when it keeps it.
fn slot_subset(e: &IrExpr, object: &IrExpr) -> Option<Why> {
    if !crate::witness_unwrap::slot_read_of_var(e) {
        let core = crate::rc_ownership::rc_tail(object);
        return Some(Why::Here(format!("{}-object:{}", tag(&e.kind), tag(&core.kind))));
    }
    value_subset(object)
}

/// The runtime prims `lower_budget_prim` lowers (fuel.rs): the RC-free
/// quartet of the deterministic meter and the wall-deadline trio.
const METER_PRIMS: &[&str] = &[
    "almide_rt_prim_budget_enter",
    "almide_rt_prim_budget_exit",
    "almide_rt_prim_budget_exhausted",
    "almide_rt_prim_budget_spend",
    "almide_rt_prim_timeout_enter",
    "almide_rt_prim_timeout_exit",
    "almide_rt_prim_timeout_hit",
];

/// The extraction forms (`??`, `!`, `?`) and runtime calls, split from
/// [`value_subset`] for the complexity budget.
fn extraction_or_rt_subset(e: &IrExpr) -> Option<Why> {
    match &e.kind {
        // `r ?? fallback`: a two-arm branch site over the carrier (data.rs,
        // `witness_unwrap_or_arm`). The carrier must be a bound local
        // (arg_temps.rs names a produced one), the fallback runs on its arm.
        IrExprKind::UnwrapOr { expr, fallback } => {
            // #2755: a slot of a bound block (`r.f ?? x`) is a carrier view
            // like a local's.
            if !crate::witness_unwrap::slot_read_of_var(expr) {
                return Some(Why::Here(format!("UnwrapOr-carrier:{}", tag(&crate::rc_ownership::rc_tail(expr).kind))));
            }
            value_subset(expr).or_else(|| value_subset(fallback).map(|w| w.inside("fallback")))
        }
        IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => match &expr.kind {
            IrExprKind::Var { .. } => None,
            IrExprKind::Call { .. } if carrier_ty(&expr.ty) => call_subset(expr).map(|w| w.inside("unwrap-operand")),
            // #2758: a MOVE-MODE effect call (C-132, mut_param.rs) is typed
            // with its raw payload, but its wasm value is the effect ABI's
            // one Result block (func.rs `fn_signature`), handed over at rc 1.
            // `lower_try_unwrap` reads it as an owned carrier exactly as for
            // a carrier-typed tail call: born at the site, out on the err
            // arm, released on the ok path (`release_ok_carrier`), its
            // payload's credit moving to the value. A call whose lowered
            // type is no carrier is refused by the lowering itself.
            IrExprKind::Call { .. } => call_subset(expr).map(|w| w.inside("unwrap-operand")),
            // #2758: `err(m)!`, the explicit raise, is a certainly-fresh
            // carrier — the owned-carrier route of the `!` site
            // (witness_unwrap.rs): born at the site, out on the err arm,
            // released on the ok path. Its payload store is the constructor's.
            IrExprKind::ResultErr { .. } => value_subset(expr).map(|w| w.inside("unwrap-operand")),
            IrExprKind::Call { .. } => call_subset(expr).map(|w| w.inside("unwrap-operand")),
            _ => Some(Why::Here(tag(&e.kind))),
        },
        // The deterministic-meter / wall-deadline prims (fuel.rs
        // `lower_budget_prim`): scalar in, scalar out, globals only — no RC
        // site of their own. Their scalar arguments are ordinary values.
        IrExprKind::RuntimeCall { symbol, args } if METER_PRIMS.contains(&symbol.as_str()) => {
            match args.iter().find(|a| !scalar_ty(&a.ty)) {
                Some(a) => Some(Why::Here(format!("RuntimeCall:{symbol}:heap-arg:{}", tag(&a.kind)))),
                None => args.iter().find_map(|a| value_subset(a).map(|w| w.inside("rt-arg"))),
            }
        }
        IrExprKind::RuntimeCall { symbol, .. } => Some(Why::Here(format!("RuntimeCall:{symbol}"))),
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
    if let Some(w) = read_operand(left, "operand") {
        return Some(w);
    }
    // `and` / `or`: the right operand runs on one arm of a branch site the
    // lowering opens for the witness (#2756) — any admissible operand.
    read_operand(right, "operand")
}

/// A value a reader consumes no credit of (an operator operand, an
/// interpolation part): a scalar is any admissible value (reported under
/// `position`), a heap value only a Var or a pool static — a fresh heap
/// value there is an unowned temporary no hook records (`heap-<position>`).
fn read_operand(x: &IrExpr, position: &str) -> Option<Why> {
    if scalar_ty(&x.ty) {
        return value_subset(x).map(|w| w.inside(position));
    }
    match &x.kind {
        IrExprKind::Var { .. } | IrExprKind::LitStr { .. } => None,
        // An element read of a bound list: a view the reader spends nothing of.
        IrExprKind::IndexAccess { .. } | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } => {
            value_subset(x).map(|w| w.inside(position))
        }
        // #2758: `r ?? v` / `r ?? "lit"` over a bound carrier: a join that is
        // never owned (arg_temps.rs `unwrap_or_joins_owned`, the predicate
        // `own_unwrap_or_join` reads), so both arms are views and the arm
        // hook records no site — a read like a slot read's. Any other
        // fallback may own the join, and an owned join must be bound first.
        IrExprKind::UnwrapOr { fallback, .. }
            if matches!(fallback.kind, IrExprKind::Var { .. } | IrExprKind::LitStr { .. }) =>
        {
            value_subset(x).map(|w| w.inside(position))
        }
        // #2758: arg_temps.rs's `{ let t = f(x); read(t) }`: the binds are
        // the Bind hook's, the value read is the tail's, under this rule.
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            stmts_subset(stmts).map(Why::Deep).or_else(|| read_operand(tail, position))
        }
        other => Some(Why::Deep(format!("heap-{position}:{}", tag(other)))),
    }
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
                || name.as_str().starts_with("__proc_")
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
        // A native arm may INLINE a literal callback (list.rs lowers the
        // lambda's body in this frame, once per element). Only the arms whose
        // callback activation is hooked are admitted (`inline_callback_subset`);
        // any other declines by arm name. A Fn value that arrives as a value
        // (a Var, a call result) is an ordinary argument everywhere.
        almide_ir::CallTarget::Module { module, func, .. } => {
            if args.iter().any(|a| matches!(crate::rc_ownership::rc_tail(a).kind, IrExprKind::Lambda { .. }))
                && !is_self_hosted_hof(module.as_str(), func.as_str())
            {
                return inline_callback_subset(module.as_str(), func.as_str(), args);
            }
        }
        // #2758: a closure call — `call_indirect` through the env block the
        // callee value is. The env is lent (a view to the lifted body); a
        // fresh callee is released after the call (`id`); the arguments are
        // callee-owned (the closure convention), each an argument hook's.
        // A field of a bound record (`r.f(x)`, #2758): the Fn value is a
        // VIEW of the slot, lent to the lifted body like a borrowed local —
        // no release (`rc_owned_result` of a field read is false), no share.
        almide_ir::CallTarget::Computed { callee } if matches!(callee.kind, IrExprKind::Member { .. }) => {
            let IrExprKind::Member { object, .. } = &callee.kind else { unreachable!() };
            if !matches!(object.kind, IrExprKind::Var { .. }) {
                return Some(Why::Deep(format!("callee:Member-of:{}", tag(&object.kind))));
            }
        }
        almide_ir::CallTarget::Computed { callee } => {
            if let Some(w) = value_subset(callee) {
                return Some(w.inside("callee"));
            }
        }
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
        // A statement `f(x)!`: the site runs, its payload is discarded
        // (released when the extraction handed the frame its credit).
        IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } => value_subset(e),
        other => Some(Why::Here(tag(other))),
    }
}

/// A match's subject and arm heads (#2756): the subject is evaluated once,
/// before the site; a pattern binds VIEWS of it (patterns.rs, no share, no
/// release) — except a named list rest, a fresh block no owner releases
/// (#2971), declined. A guard runs between two arms' tests (patterns.rs
/// `lower_arm_chain`) and is recorded on its own arm's path, where the
/// state before it is the one every path through it starts from. Its value
/// is a scalar Bool, so each credit it takes inside is settled inside it (a
/// temporary born and released, a share moved into a callee): the path that
/// falls through to the next arm leaves it in that same state, and the
/// events checked on the guard's own arm are the ones that path ran. A guard
/// that BINDS a local declines — the local outlives the guard.
fn match_head_subset(subject: &IrExpr, arms: &[almide_ir::IrMatchArm]) -> Option<String> {
    // A tuple / record literal subject is a fresh block the match only
    // reads: no route owns or releases it (arg_temps.rs names a produced or
    // concatenated subject, not a literal one), so no hook has its `i`.
    if matches!(
        crate::rc_ownership::rc_tail(subject).kind,
        IrExprKind::Tuple { .. } | IrExprKind::Record { .. } | IrExprKind::StringInterp { .. }
    ) {
        return Some(format!("match-subject:{}", tag(&crate::rc_ownership::rc_tail(subject).kind)));
    }
    if let Some(w) = value_subset(subject) {
        return Some(w.at("match-subject"));
    }
    // A subject the match BUILDS (`match (1, (4, 5), 3)`, `match some(7)`)
    // is a fresh block no hook records and nothing released (#2971: three
    // fixtures certified every frame and still ended with it live).
    // arg_temps.rs names it first, so the site reads a local; one that
    // reaches here unnamed declines rather than certifies.
    let core = crate::rc_ownership::rc_tail(subject);
    let builds = matches!(
        core.kind,
        IrExprKind::Tuple { .. }
            | IrExprKind::Record { .. }
            | IrExprKind::SpreadRecord { .. }
            | IrExprKind::OptionSome { .. }
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. }
            | IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. }
            | IrExprKind::StringInterp { .. }
    );
    if builds && crate::arg_temps::bindable_ty(&core.ty) {
        return Some("match-subject:fresh".into());
    }
    for a in arms {
        if pattern_has_named_rest(&a.pattern) {
            return Some("pattern:list-rest".into());
        }
        if let Some(g) = a.guard.as_ref().filter(|g| !rc_free(g)) {
            if binds_a_local(g) {
                return Some("match-guard:binds".into());
            }
            if let Some(w) = value_subset(g) {
                return Some(w.at("match-guard"));
            }
        }
    }
    None
}

/// Does `e` bind a local anywhere (a `let` in a block, a lambda param, a
/// loop var, a pattern)? A guard that does keeps that local past the guard.
fn binds_a_local(e: &IrExpr) -> bool {
    struct V(bool);
    impl almide_ir::visit::IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(
                e.kind,
                IrExprKind::Block { .. } | IrExprKind::Lambda { .. } | IrExprKind::ForIn { .. } | IrExprKind::Match { .. }
            ) {
                self.0 = true;
                return;
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut v = V(false);
    almide_ir::visit::IrVisitor::visit_expr(&mut v, e);
    v.0
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

/// #2758: the gate for an EFFECT frame. Its body lowers at the raw ok type
/// and func.rs wraps the result in the ok carrier (both recorded there), so
/// the straight-line rules apply unchanged — except to a constructor of the
/// carrier itself. Inside an effect body, where the raw type is expected:
///
/// - `err(e)` RAISES through the frame's error exit (data.rs
///   `lower_err_raise`): the err block is built (its payload store is
///   `witness_store`), the exit plan's releases are recorded like a `!`
///   propagation's, and the block leaves (`witness_err_raise`). Admitted.
/// - `ok(v)` is transparent — the value IS `v` — while every consumer reads
///   the node as a fresh construction (`rc_owned_result`). That agrees only
///   when `v` is a scalar or certainly fresh itself, so any other `ok(v)`
///   declines as `effect:carrier:ok-borrowed`.
pub fn effect_subset(body: &IrExpr, raw_is_heap: bool) -> Option<String> {
    struct Carrier(bool);
    impl almide_ir::visit::IrVisitor for Carrier {
        fn visit_expr(&mut self, e: &IrExpr) {
            // A call's droppable result is its callee's handed-over credit
            // (#1986); one an arm declares a View declines at emission
            // (data.rs `lower_err_raise`).
            if let IrExprKind::ResultOk { expr } = &e.kind
                && !scalar_ty(&expr.ty)
                && !crate::rc_ownership::rc_certainly_fresh(&expr.kind)
                && !matches!(expr.kind, IrExprKind::Call { .. })
            {
                self.0 = true;
            } else if !self.0 {
                almide_ir::visit::walk_expr(self, e);
            }
        }
    }
    let mut c = Carrier(false);
    almide_ir::visit::IrVisitor::visit_expr(&mut c, body);
    if c.0 {
        return Some("effect:carrier:ok-borrowed".into());
    }
    straightline_subset(body, raw_is_heap)
}

/// Is `t` a Result / Option carrier type?
fn carrier_ty(t: &almide_types::types::Ty) -> bool {
    use almide_types::types::constructor::TypeConstructorId as C;
    matches!(t, almide_types::types::Ty::Applied(C::Result | C::Option, _))
}

/// #2758: a top-let initializer lowered inline in `main`'s prologue: an
/// admissible value (its sites are the hooks'); the store into the global is
/// `witness_top_let`'s.
pub fn top_let_subset(value: &IrExpr) -> Option<String> {
    value_subset(value).map(|w| w.at("top-let"))
}
