//! The tree-walking evaluator: `IrExpr` / `IrStmt` / `IrPattern` → `Value`.
//!
//! Every eval step burns one unit of fuel. Codegen-inserted node kinds (Clone,
//! Borrow, IterChain, ClosureCreate, …) are unreachable at the pre-codegen cut
//! point and panic with an explanatory message to document the boundary.

use std::rc::Rc;

use almide_base::intern::Sym;
use almide_ir::{
    BinOp, CallTarget, IrExpr, IrExprKind, IrFieldPattern, IrMatchArm, IrPattern, IrStmt,
    IrStmtKind, IrStringPart, UnOp, VarId,
};
use almide_lang::types::Ty;

use crate::env::Scope;
use crate::value::{Closure, Value, VariantPayload};
use crate::{Flow, Interpreter};

/// Helper: short-circuit a `Flow` that is not a plain value out of an
/// expression evaluator. Returns the inner `Value`, or propagates the signal.
///
/// Usable from a fn returning either `Flow` or `Option<Flow>`: the propagated
/// signal goes through `From`, and core's blanket `impl<T> From<T> for Option<T>`
/// wraps it as `Some`, so the `eval_expr_*` group helpers share the arm bodies
/// verbatim. `None` stays reserved for "not my group".
macro_rules! val {
    ($flow:expr) => {
        match $flow {
            Flow::Value(v) => v,
            other => return ::core::convert::From::from(other),
        }
    };
}

impl<'a> Interpreter<'a> {
    pub(crate) fn eval_expr(&mut self, expr: &IrExpr, scope: &Scope) -> Flow {
        if let Err(f) = self.step() {
            return f;
        }
        if let Some(flow) = self.eval_expr_literal(expr, scope) { return flow; }
        if let Some(flow) = self.eval_expr_operator(expr, scope) { return flow; }
        if let Some(flow) = self.eval_expr_control(expr, scope) { return flow; }
        if let Some(flow) = self.eval_expr_call(expr, scope) { return flow; }
        if let Some(flow) = self.eval_expr_collection(expr, scope) { return flow; }
        if let Some(flow) = self.eval_expr_function(expr, scope) { return flow; }
        if let Some(flow) = self.eval_expr_variant(expr, scope) { return flow; }
        if let Some(flow) = self.eval_expr_misc(expr, scope) { return flow; }
        // No group claimed the node. ABORT rather than invent a value: this
        // interpreter is the third cross-target oracle, and an abstention is a
        // recorded hole in the executable spec while a wrong answer is a wrong
        // verdict against the other two legs.
        Flow::Abort(format!("interp: no evaluation rule for {:?}", std::mem::discriminant(&expr.kind)))
    }

    /// Literals and variable/function references.
    ///
    /// Extracted from `eval_expr` (name-router split): `None` means "not my
    /// group". The groups are the comment sections the function already carried,
    /// so the partition is the one a reader was already using. A dropped arm here
    /// SHRINKS the executable spec rather than failing loudly, so the router's
    /// final fallback aborts with the node kind instead of silently returning a
    /// value — the interp is the third cross-target oracle and a wrong answer is
    /// worse than an abstention.
    fn eval_expr_literal(&mut self, expr: &IrExpr, scope: &Scope) -> Option<Flow> {
        Some(match &expr.kind {
            // ── Literals ──
            IrExprKind::LitInt { value } => Flow::val(Value::Int(*value)),
            // A Float32-typed literal narrows AT BIRTH to the value an f32 can hold,
            // exactly as both backends do (native emits the literal as f32, wasm folds
            // to f32.const) — the widened-carrier convention (bridge.rs "f2f32") is
            // about the CARRIER, not which value it carries. Without this the interp
            // read the f64 spelling of `let p: Float32 = 123456789.12345679` and cast
            // a wrong third vote against two agreeing backends (Wave 4 L3).
            IrExprKind::LitFloat { value } => Flow::val(Value::Float(
                if matches!(expr.ty, almide_lang::types::Ty::Float32) {
                    *value as f32 as f64
                } else {
                    *value
                },
            )),
            IrExprKind::LitStr { value } => Flow::val(Value::str(value.clone())),
            IrExprKind::LitBool { value } => Flow::val(Value::Bool(*value)),
            IrExprKind::Unit => Flow::val(Value::Unit),
            // ── Variables ──
            IrExprKind::Var { id } => match scope.get(*id) {
                Some(v) => Flow::val(v),
                None => Flow::Abort(format!(
                    "internal: unbound variable {:?} ({})",
                    id,
                    self.var_name(*id)
                )),
            },
            // A named function used as a value: wrap it as a closure-like value
            // by capturing its name; application looks it up. We model it as a
            // closure whose body re-dispatches to the named fn.
            IrExprKind::FnRef { name } => self.fn_ref_value(*name, scope),
            _ => return None,
        })
    }

    /// Unary and binary operators.
    ///
    /// Extracted from `eval_expr` (name-router split): `None` means "not my
    /// group". The groups are the comment sections the function already carried,
    /// so the partition is the one a reader was already using. A dropped arm here
    /// SHRINKS the executable spec rather than failing loudly, so the router's
    /// final fallback aborts with the node kind instead of silently returning a
    /// value — the interp is the third cross-target oracle and a wrong answer is
    /// worse than an abstention.
    fn eval_expr_operator(&mut self, expr: &IrExpr, scope: &Scope) -> Option<Flow> {
        Some(match &expr.kind {
            // ── Operators ──
            IrExprKind::BinOp { op, left, right } => self.eval_binop(*op, left, right, scope),
            IrExprKind::UnOp { op, operand } => {
                let v = val!(self.eval_expr(operand, scope));
                self.eval_unop(*op, v)
            }
            _ => return None,
        })
    }

    /// Control flow and loops.
    ///
    /// Extracted from `eval_expr` (name-router split): `None` means "not my
    /// group". The groups are the comment sections the function already carried,
    /// so the partition is the one a reader was already using. A dropped arm here
    /// SHRINKS the executable spec rather than failing loudly, so the router's
    /// final fallback aborts with the node kind instead of silently returning a
    /// value — the interp is the third cross-target oracle and a wrong answer is
    /// worse than an abstention.
    fn eval_expr_control(&mut self, expr: &IrExpr, scope: &Scope) -> Option<Flow> {
        Some(match &expr.kind {
            // ── Control flow ──
            IrExprKind::If { cond, then, else_ } => self.eval_if(cond, then, else_, scope),
            IrExprKind::Match { subject, arms } => self.eval_match(subject, arms, scope),
            IrExprKind::Block { stmts, expr } => self.eval_block(stmts, expr.as_deref(), scope),
            // Fan block: evaluate each expr SEQUENTIALLY in source order — the
            // deterministic mode both backends collapse to (WASM has no threads;
            // native's `fan_effect`/`fan_expr` join in handle order). Both
            // backends materialize the results as a TUPLE, not a list:
            //   - native walker `render_fan` joins as `(j0, j1, ...)` for >1 expr,
            //     and a bare `j0` for exactly one expr;
            //   - WASM `emit_wasm/expressions.rs::Fan` builds a tuple (`>1`) or
            //     emits the single value bare (`==1`).
            // Each Result-typed spawn body auto-unwraps with `?` semantics at the
            // join (`handle.join().unwrap()?`): on Ok the inner value is taken, on
            // Err the enclosing fn short-circuits. At THIS pre-codegen cut point
            // the auto-`?` is still an explicit `Try`/`Unwrap` node wrapping each
            // Result-typed fan expr (the `strip_fan_auto_try` codegen pass that
            // removes it runs post-cut), so evaluating the expr already performs
            // the unwrap and propagates an `Err` as `Flow::Return` — exactly the
            // backends' join-point `?`. We therefore just evaluate and collect.
            IrExprKind::Fan { exprs } => self.eval_fan(exprs, scope),
            // ── Loops ──
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.eval_for_in(*var, var_tuple.as_deref(), iterable, body, scope)
            }
            IrExprKind::While { cond, body } => self.eval_while(cond, body, scope),
            IrExprKind::Break => Flow::Break,
            IrExprKind::Continue => Flow::Continue,
            _ => return None,
        })
    }

    /// `If` — the condition must be a Bool; anything else is an internal
    /// error, not a truthiness rule.
    fn eval_if(&mut self, cond: &IrExpr, then: &IrExpr, else_: &IrExpr, scope: &Scope) -> Flow {
        let c = val!(self.eval_expr(cond, scope));
        match c {
            Value::Bool(true) => self.eval_expr(then, scope),
            Value::Bool(false) => self.eval_expr(else_, scope),
            other => Flow::Abort(format!(
                "internal: if-condition is {} not Bool",
                other.type_name()
            )),
        }
    }

    /// `Fan` — the deterministic-data-parallelism model (docs/roadmap/active/
    /// concurrency-stance.md) defines a `fan` block's observable behaviour as
    /// sequential evaluation in LIST ORDER, so the interpreter models it exactly:
    /// evaluate every arm in order (JOINING all of them — there is no
    /// cancellation, C-199), then fail with the FIRST `Err` in list order.
    ///
    /// The arms are Result-typed and the block's type is the unwrapped payload —
    /// `infer_expr_g3_fan` does that unwrap in the checker. Without mirroring it
    /// here the tuple carried Results into arithmetic and the interpreter aborted
    /// with `internal: int op on Result and Int`, which is a WRONG VOTE into the
    /// 3-way oracle rather than an honest skip. C-199's fixture caught it.
    fn eval_fan(&mut self, exprs: &[IrExpr], scope: &Scope) -> Flow {
        let mut out = Vec::with_capacity(exprs.len());
        let mut first_err: Option<String> = None;
        for e in exprs {
            let v = val!(self.eval_expr(e, scope));
            match v {
                Value::Result(Ok(payload)) => out.push(*payload),
                Value::Result(Err(payload)) => {
                    if first_err.is_none() {
                        first_err = Some(payload.display_bare());
                    }
                    // Keep the arity right for the tuple below; the value is
                    // unreachable because `first_err` aborts before it is read.
                    out.push(Value::Unit);
                }
                other => out.push(other),
            }
        }
        if let Some(msg) = first_err {
            return Flow::Abort(msg);
        }
        // A single-expr fan is the bare value (no 1-tuple), matching both
        // backends; `into_iter` is destructured rather than unwrapped so the
        // 1-element case is proved by the pattern instead of by a length check.
        let mut it = out.into_iter();
        match (it.next(), it.next()) {
            (Some(only), None) => Flow::val(only),
            (first, second) => Flow::val(Value::tuple(
                first.into_iter().chain(second).chain(it).collect(),
            )),
        }
    }

    /// Direct and tail calls.
    ///
    /// Extracted from `eval_expr` (name-router split): `None` means "not my
    /// group". The groups are the comment sections the function already carried,
    /// so the partition is the one a reader was already using. A dropped arm here
    /// SHRINKS the executable spec rather than failing loudly, so the router's
    /// final fallback aborts with the node kind instead of silently returning a
    /// value — the interp is the third cross-target oracle and a wrong answer is
    /// worse than an abstention.
    fn eval_expr_call(&mut self, expr: &IrExpr, scope: &Scope) -> Option<Flow> {
        Some(match &expr.kind {
            // ── Calls ──
            IrExprKind::Call { target, args, .. } => {
                let flow = self.eval_call(target, args, scope);
                lift_to_declared_carrier(&expr.ty, flow)
            }
            // TailCall is codegen-inserted (TailCallMarkPass, post-cut) but we
            // treat it == Call defensively.
            IrExprKind::TailCall { target, args } => {
                let flow = self.eval_call(target, args, scope);
                lift_to_declared_carrier(&expr.ty, flow)
            }
            _ => return None,
        })
    }

    /// Collection construction, ranges, and element/field access.
    ///
    /// Extracted from `eval_expr` (name-router split): `None` means "not my
    /// group". Kept as ONE function: an arm-table halving cut the `Range` arm's
    /// inner `match (s, e)` in half and silently dropped its Int case, so the
    /// interpreter aborted on every range. Splitting a match whose arms contain
    /// their own matches is not a mechanical operation.
    fn eval_expr_collection(&mut self, expr: &IrExpr, scope: &Scope) -> Option<Flow> {
        Some(match &expr.kind {
            // ── Collections ──
            IrExprKind::List { elements } => self.eval_seq_literal(elements, Value::list, scope),
            IrExprKind::Tuple { elements } => self.eval_seq_literal(elements, Value::tuple, scope),
            IrExprKind::MapLiteral { entries } => self.eval_map_literal(entries, scope),
            IrExprKind::EmptyMap => Flow::val(Value::Map(Rc::new(Vec::new()))),
            IrExprKind::Record { name, fields } => {
                self.eval_record_literal(name, fields, &expr.ty, scope)
            }
            IrExprKind::SpreadRecord { base, fields } => {
                self.eval_spread_record(base, fields, scope)
            }
            IrExprKind::Range { start, end, inclusive } => {
                self.eval_range(start, end, *inclusive, scope)
            }

            // ── Access ──
            IrExprKind::Member { object, field } => {
                let o = val!(self.eval_expr(object, scope));
                self.eval_member(o, *field)
            }
            IrExprKind::TupleIndex { object, index } => {
                let o = val!(self.eval_expr(object, scope));
                tuple_index(o, *index)
            }
            IrExprKind::IndexAccess { object, index } => {
                let o = val!(self.eval_expr(object, scope));
                let i = val!(self.eval_expr(index, scope));
                self.eval_index(o, i)
            }
            IrExprKind::MapAccess { object, key } => {
                let o = val!(self.eval_expr(object, scope));
                let k = val!(self.eval_expr(key, scope));
                map_lookup(o, k)
            }

            _ => return None,
        })
    }

    /// `List` / `Tuple`: evaluate every element left to right, then hand the
    /// vector to the value constructor for this literal's shape.
    fn eval_seq_literal(
        &mut self,
        elements: &[IrExpr],
        build: fn(Vec<Value>) -> Value,
        scope: &Scope,
    ) -> Flow {
        let mut out = Vec::with_capacity(elements.len());
        for e in elements {
            out.push(val!(self.eval_expr(e, scope)));
        }
        Flow::val(build(out))
    }

    /// `MapLiteral`: key then value per entry, inserted in source order so a
    /// duplicate key keeps the LAST binding, as both backends do.
    fn eval_map_literal(&mut self, entries: &[(IrExpr, IrExpr)], scope: &Scope) -> Flow {
        let mut out: Vec<(Value, Value)> = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            let kv = val!(self.eval_expr(k, scope));
            let vv = val!(self.eval_expr(v, scope));
            map_insert(&mut out, kv, vv);
        }
        Flow::val(Value::Map(Rc::new(out)))
    }

    /// Lambdas and string interpolation.
    ///
    /// Extracted from `eval_expr` (name-router split): `None` means "not my
    /// group". The groups are the comment sections the function already carried,
    /// so the partition is the one a reader was already using. A dropped arm here
    /// SHRINKS the executable spec rather than failing loudly, so the router's
    /// final fallback aborts with the node kind instead of silently returning a
    /// value — the interp is the third cross-target oracle and a wrong answer is
    /// worse than an abstention.
    fn eval_expr_function(&mut self, expr: &IrExpr, scope: &Scope) -> Option<Flow> {
        Some(match &expr.kind {
            // ── Functions ──
            IrExprKind::Lambda { params, body, .. } => {
                let clo = Closure {
                    params: params.iter().map(|(v, _)| *v).collect(),
                    body: Rc::new((**body).clone()),
                    captured: scope.clone(),
                    ret_ty: match &expr.ty {
                        Ty::Fn { ret, .. } => Some((**ret).clone()),
                        _ => None,
                    },
                };
                Flow::val(Value::Closure(Rc::new(clo)))
            }
            // ── Strings ──
            IrExprKind::StringInterp { parts } => self.eval_string_interp(parts, scope),
            _ => return None,
        })
    }

    /// `Result`/`Option` construction, unwrap, and the optional chain.
    ///
    /// Extracted from `eval_expr` (name-router split): `None` means "not my
    /// group". Kept as ONE function for the same reason as
    /// `eval_expr_collection` — several arms carry their own `match`, and cutting
    /// the table in half cut those too (the `UnwrapOr` arm lost every real case
    /// and answered `None`, which the router then read as "unhandled").
    fn eval_expr_variant(&mut self, expr: &IrExpr, scope: &Scope) -> Option<Flow> {
        Some(match &expr.kind {
            // ── Result / Option ──
            IrExprKind::ResultOk { expr } => {
                let v = val!(self.eval_expr(expr, scope));
                Flow::val(Value::Result(Ok(Box::new(v))))
            }
            IrExprKind::ResultErr { expr } => {
                let v = val!(self.eval_expr(expr, scope));
                Flow::val(Value::Result(Err(Box::new(v))))
            }
            IrExprKind::OptionSome { expr } => {
                let v = val!(self.eval_expr(expr, scope));
                Flow::val(Value::Option(Some(Box::new(v))))
            }
            IrExprKind::OptionNone => Flow::val(Value::Option(None)),
            // `?` / `!` — short-circuit the enclosing fn on Err/None.
            IrExprKind::Try { expr: op } | IrExprKind::Unwrap { expr: op } => {
                self.eval_try_unwrap(op, &expr.ty, scope)
            }
            // `??` — unwrap with a fallback value.
            IrExprKind::UnwrapOr { expr, fallback } => {
                self.eval_unwrap_or(expr, fallback, scope)
            }
            // `?` Result→Option (identity for Option).
            IrExprKind::ToOption { expr } => {
                let v = val!(self.eval_expr(expr, scope));
                match v {
                    Value::Result(Ok(inner)) => Flow::val(Value::Option(Some(inner))),
                    Value::Result(Err(_)) => Flow::val(Value::Option(None)),
                    opt @ Value::Option(_) => Flow::val(opt),
                    other => Flow::val(other),
                }
            }
            // `?.field` — optional chaining.
            IrExprKind::OptionalChain { expr, field } => {
                self.eval_optional_chain(expr, *field, scope)
            }
            // The interp is synchronous: await is identity over the value.

            _ => return None,
        })
    }

    /// `start..end` / `start..=end` — both bounds must evaluate to `Int`.
    /// Extracted so its inner `match (s, e)` does not nest inside the group's
    /// arm table (an earlier mechanical halving cut exactly this match and
    /// dropped its only real case, aborting every range).
    fn eval_range(&mut self, start: &IrExpr, end: &IrExpr, inclusive: bool, scope: &Scope) -> Flow {
        let s = val!(self.eval_expr(start, scope));
        let e = val!(self.eval_expr(end, scope));
        match (s, e) {
            (Value::Int(s), Value::Int(e)) => {
                Flow::val(Value::Range { start: s, end: e, inclusive })
            }
            _ => Flow::Abort("internal: range bounds must be Int".into()),
        }
    }

    /// `x ?? fallback` — the fallback is taken for `none` and for `err(_)`; any
    /// other value passes through unchanged. Extracted for the same nesting
    /// reason as `eval_range`.
    fn eval_unwrap_or(&mut self, expr: &IrExpr, fallback: &IrExpr, scope: &Scope) -> Flow {
        let v = val!(self.eval_expr(expr, scope));
        match v {
            Value::Option(Some(inner)) => Flow::val(*inner),
            Value::Option(None) => self.eval_expr(fallback, scope),
            Value::Result(Ok(inner)) => Flow::val(*inner),
            Value::Result(Err(_)) => self.eval_expr(fallback, scope),
            other => Flow::val(other),
        }
    }

    /// Holes, `todo`, and the codegen-inserted nodes that are UNREACHABLE at
    /// the pre-codegen cut point this interpreter runs at.
    ///
    /// Extracted from `eval_expr` (name-router split): `None` means "not my
    /// group". The groups are the comment sections the function already carried,
    /// so the partition is the one a reader was already using. A dropped arm here
    /// SHRINKS the executable spec rather than failing loudly, so the router's
    /// final fallback aborts with the node kind instead of silently returning a
    /// value — the interp is the third cross-target oracle and a wrong answer is
    /// worse than an abstention.
    fn eval_expr_misc(&mut self, expr: &IrExpr, scope: &Scope) -> Option<Flow> {
        Some(match &expr.kind {
            // ── Misc ──
            // `_` and `todo(..)` ABORT the native leg as a Rust panic — the
            // message on stderr and exit **101** (ALS-E30). `Flow::Abort` is
            // this interpreter's model of the R1 abort, which is `Error: <msg>`
            // and exit **1** — a different observable. Voting Abort here would
            // therefore cast a WRONG third vote on every hole, and a wrong vote
            // is worse than an honest skip (crate rule). The interpreter has no
            // panic model, so it abstains by name.
            //
            // The same node is also the synthesized body of an intrinsic stub
            // (`lower/mod.rs`), which `dispatch.rs` filters out before it can be
            // evaluated. Both routes are named below, since an abstain that
            // appears without a hole in the source means that filter broke.
            IrExprKind::Hole => Flow::Unsupported(
                "typed hole `_` (native panics with exit 101; no panic model here) \
                 — or an intrinsic-stub body that escaped dispatch's filter"
                    .into(),
            ),
            IrExprKind::Todo { message } => Flow::Unsupported(format!(
                "todo({message:?}) (native panics with exit 101; no panic model here)"
            )),
            // ── The ONE pre-codegen RuntimeCall family: the budget prims.
            // The fan.bounded/race frontend lowering emits these symbols
            // directly (they are the deterministic tier's floor on every
            // leg), so they are legitimately reachable at the cut point.
            // Everything else stays unreachable below.
            IrExprKind::RuntimeCall { symbol, args }
                if symbol.as_str().starts_with("almide_rt_prim_budget_")
                    || symbol.as_str().starts_with("almide_rt_prim_timeout_") =>
            {
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(val!(self.eval_expr(a, scope)));
                }
                self.budget_prim_rt(symbol.as_str(), &vals)
            }
            // ── Codegen-inserted: UNREACHABLE at the pre-codegen cut point ──
            IrExprKind::RuntimeCall { .. } | IrExprKind::Clone { .. }
            | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
            | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
            | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
            | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
            | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
            | IrExprKind::IterChain { .. } => unreachable_post_cut(&expr.kind),
            _ => return None,
        })
    }
}

include!("eval_loops_stmts.rs");
include!("eval_match.rs");
include!("eval_access.rs");
include!("eval_spine.rs");

/// `TupleIndex` on an already-evaluated receiver. Lists share the arm: a
/// destructured list binding is a `Value::List` at this cut point.
fn tuple_index(o: Value, index: usize) -> Flow {
    let (Value::Tuple(items) | Value::List(items)) = o else {
        return Flow::Abort(format!("internal: tuple-index on {} ", o.type_name()));
    };
    match items.get(index) {
        Some(v) => Flow::val(v.clone()),
        None => Flow::Abort("index out of bounds".into()),
    }
}

/// `MapAccess` on an already-evaluated receiver — a miss is `None`, not an
/// abort (the node's type is `Option[V]`).
fn map_lookup(o: Value, k: Value) -> Flow {
    let Value::Map(entries) = o else {
        return Flow::Abort(format!("internal: map-access on {}", o.type_name()));
    };
    let found = entries.iter().find(|(ek, _)| ek == &k);
    Flow::val(Value::Option(found.map(|(_, v)| Box::new(v.clone()))))
}

/// Every node kind an `almide-codegen` target pass inserts, paired with the
/// pass that inserts it. Reaching one means the pre-codegen cut point moved —
/// fix the boundary rather than handling the node here (see the crate docs).
fn unreachable_post_cut(kind: &IrExprKind) -> ! {
    let (node, pass) = match kind {
        IrExprKind::RuntimeCall { .. } => ("RuntimeCall", "IntrinsicLowering"),
        IrExprKind::Clone { .. } => ("Clone", "CloneInsertion"),
        IrExprKind::Deref { .. } => ("Deref", "BoxDeref"),
        IrExprKind::Borrow { .. } => ("Borrow", "BorrowInsertion"),
        IrExprKind::BoxNew { .. } => ("BoxNew", "BoxDeref"),
        IrExprKind::RcWrap { .. } => ("RcWrap", "ClosureConversion"),
        IrExprKind::RustMacro { .. } => ("RustMacro", "BuiltinLowering"),
        IrExprKind::ToVec { .. } => ("ToVec", "StdlibLowering"),
        IrExprKind::RenderedCall { .. } => ("RenderedCall", "StdlibLowering"),
        IrExprKind::InlineRust { .. } => ("InlineRust", "StdlibLowering"),
        IrExprKind::ClosureCreate { .. } => ("ClosureCreate", "ClosureConversion"),
        IrExprKind::EnvLoad { .. } => ("EnvLoad", "ClosureConversion"),
        IrExprKind::IterChain { .. } => ("IterChain", "StdlibLowering"),
        other => unreachable!("{:?} is not a codegen-inserted node", std::mem::discriminant(other)),
    };
    unreachable!("{} is codegen-inserted ({}); interp runs pre-codegen", node, pass)
}

/// Put a call's value into the carrier the CHECKER says the call site has.
///
/// An `effect fn f() -> T` has ABI return type `Result[T, String]`, and both
/// backends materialize that carrier; since ADR-0008 removed implicit
/// propagation, a call in any position yields a Result value. The interpreter
/// handed the success value back BARE while modelling the failure channel as
/// `Result(Err(..))` — so `match <effect call> { ok(v) => .., err(e) => .. }`,
/// one of the sanctioned consumption spellings, saw a plain scalar, matched no
/// arm, and aborted. A wrong third vote against two agreeing backends, which
/// this crate rates worse than an honest skip (#1366).
///
/// **Driven by the call NODE's type, not by the callee's `is_effect` flag.**
/// The first attempt keyed off the callee and wrapped inside
/// `call_function_keeping_frame`; that also caught the lowered stdlib module
/// bodies, whose call sites want the bare value — seven fixtures started
/// dissenting with `list.len on non-list` and `if-condition is Result not Bool`.
/// The checker's type at the site is the authority on what shape belongs there,
/// so consult it: a site typed `Result[..]` gets the carrier, everything else
/// is untouched. A value that is ALREADY a Result is the failure the body
/// propagated with `!`, and must not be buried inside an `Ok`.
fn lift_to_declared_carrier(ty: &Ty, flow: Flow) -> Flow {
    if !ty.is_result() {
        return flow;
    }
    match flow {
        Flow::Value(Value::Result(r)) => Flow::Value(Value::Result(r)),
        Flow::Value(v) => Flow::Value(Value::Result(Ok(Box::new(v)))),
        other => other,
    }
}

/// The C-216 marker fact (see `try_unwrap_value_flag`): a `Try`/`Unwrap`
/// node TYPED `Option[_]` is the effect-RESULT-layer strip on a
/// declared-Option effect call — identity on Option values.
pub(crate) fn marker_is_option_identity(node_ty: &Ty) -> bool {
    matches!(node_ty,
        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) if a.len() == 1)
}
