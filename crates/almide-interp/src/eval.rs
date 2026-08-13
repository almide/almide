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

    // ── `?` / `!` / `?.field` ───────────────────────────────────

    /// `Try`/`Unwrap` — short-circuit the enclosing fn on Err/None.
    /// `node_ty` is the marker NODE's own type: when it is `Option[...]`, the
    /// checker resolved this `!` as the effect-RESULT-layer strip on a
    /// declared-Option effect call (`f(..)! : Option[T]` — #1125, C-216). The
    /// interp's effect convention returns the raw Option, so the marker is
    /// the identity there — pass the Option through, do NOT unwrap some/none.
    fn eval_try_unwrap(&mut self, expr: &IrExpr, node_ty: &Ty, scope: &Scope) -> Flow {
        let v = val!(self.eval_expr(expr, scope));
        self.try_unwrap_value(v, node_ty)
    }

    /// The value half of [`Self::eval_try_unwrap`] — the `!`/`?` marker's
    /// normalization on an ALREADY-evaluated operand. Split out so the
    /// tail-call trampoline can fold the same normalization over a chain's
    /// final value (`run_callable`'s pending list) instead of re-implementing
    /// it: one instrument, two call sites.
    pub(crate) fn try_unwrap_value(&mut self, v: Value, node_ty: &Ty) -> Flow {
        if matches!(node_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) if a.len() == 1)
        {
            if let Value::Option(_) = v {
                return Flow::val(v);
            }
        }
        match v {
            Value::Result(Ok(inner)) => Flow::val(*inner),
            Value::Result(Err(e)) => Flow::Return(Value::Result(Err(e))),
            Value::Option(Some(inner)) => Flow::val(*inner),
            // #556: `expr!` on a None propagates an Err whose message is
            // "none" on BOTH backends (the codegen lowers Option `!` to
            // `ok_or("none")?`). Returning a bare Option(None) made the
            // main-error path print the Rust-internal "called
            // Option::unwrap() on a None value" — a wrong third vote
            // against the native==wasm "Error: none".
            Value::Option(None) => {
                Flow::Return(Value::Result(Err(Box::new(Value::str("none".to_string())))))
            }
            other => Flow::val(other),
        }
    }

    fn eval_optional_chain(&mut self, expr: &IrExpr, field: Sym, scope: &Scope) -> Flow {
        let v = val!(self.eval_expr(expr, scope));
        match v {
            Value::Option(None) => Flow::val(Value::Option(None)),
            Value::Option(Some(inner)) => match self.eval_member(*inner, field) {
                Flow::Value(m) => Flow::val(Value::Option(Some(Box::new(m)))),
                other => other,
            },
            other => match self.eval_member(other, field) {
                Flow::Value(m) => Flow::val(Value::Option(Some(Box::new(m)))),
                other => other,
            },
        }
    }

    // ── Record literal / spread ────────────────────────────────

    fn eval_record_literal(
        &mut self,
        name: &Option<Sym>,
        fields: &[(Sym, IrExpr)],
        ty: &Ty,
        scope: &Scope,
    ) -> Flow {
        let mut out = Vec::with_capacity(fields.len());
        for (k, v) in fields {
            out.push((*k, val!(self.eval_expr(v, scope))));
        }
        // A record-shaped node whose `name` is a registered
        // record-variant constructor builds a `Variant` (so it
        // equality- / pattern-matches as a variant). A plain record
        // type stays a `Record`. Empirically (probe /tmp/repr_probe),
        // both display identically as `Name { f: v }`.
        if let Some(n) = name {
            if let Some((ty_name, crate::dispatch::CtorKind::Record)) = self.variant_ctor(*n) {
                return Flow::val(Value::Variant {
                    ty: Some(ty_name),
                    ctor: *n,
                    payload: VariantPayload::Record(out),
                });
            }
        }
        // Recover the displayed shape exactly as the codegen walker does
        // (walker/expressions.rs:511-530, walker/types.rs:111). A record
        // LITERAL carries no inline `name` when its nominal type comes
        // from an annotation/inference — the name must be recovered from
        // the expression's type. Three cases, in the walker's order:
        //   1. `expr.ty == Ty::Named(n, _)`  → the nominal name `n`,
        //      fields in literal (declaration) order.
        //   2. `expr.ty == Ty::Record/OpenRecord` whose field-name set
        //      matches a registered NAMED record type (e.g. a nested
        //      list element `[{ val: 2, kids: [] }]` whose element type
        //      was inferred structurally) → that type's name, fields
        //      reordered to the type's DECLARATION order.
        //   3. A genuinely ANONYMOUS record → no name; the native
        //      synthesized struct stores fields in SORTED name order, so
        //      sort here to match the backends' repr.
        let resolved_name;
        if let Some(n) = name {
            resolved_name = Some(*n);
        } else {
            match ty {
                Ty::Named(n, _) => resolved_name = Some(*n),
                Ty::Record { .. } | Ty::OpenRecord { .. } => {
                    let mut key: Vec<Sym> = out.iter().map(|(k, _)| *k).collect();
                    key.sort();
                    if let Some((ty_name, decl_order)) = self.named_records.get(&key).cloned() {
                        // Case 2: reorder fields to declaration order.
                        let mut reordered = Vec::with_capacity(out.len());
                        for field in &decl_order {
                            if let Some(pos) = out.iter().position(|(k, _)| k == field) {
                                reordered.push(out.swap_remove(pos));
                            }
                        }
                        reordered.extend(out.drain(..));
                        out = reordered;
                        resolved_name = Some(ty_name);
                    } else {
                        // Case 3: true anonymous record → sorted fields.
                        out.sort_by(|a, b| a.0.cmp(&b.0));
                        resolved_name = None;
                    }
                }
                _ => resolved_name = None,
            }
        }
        Flow::val(Value::Record { name: resolved_name, fields: Rc::new(out) })
    }

    fn eval_spread_record(&mut self, base: &IrExpr, fields: &[(Sym, IrExpr)], scope: &Scope) -> Flow {
        let base_v = val!(self.eval_expr(base, scope));
        let (name, mut merged) = match base_v {
            Value::Record { name, fields } => (name, (*fields).clone()),
            other => {
                return Flow::Abort(format!(
                    "internal: spread base is {} not Record",
                    other.type_name()
                ))
            }
        };
        for (k, v) in fields {
            let vv = val!(self.eval_expr(v, scope));
            if let Some(slot) = merged.iter_mut().find(|(fk, _)| fk == k) {
                slot.1 = vv;
            } else {
                merged.push((*k, vv));
            }
        }
        Flow::val(Value::Record { name, fields: Rc::new(merged) })
    }

    // ── Member / index access ──────────────────────────────────

    fn eval_member(&mut self, object: Value, field: Sym) -> Flow {
        match object {
            Value::Record { fields, .. } => {
                match fields.iter().find(|(k, _)| *k == field) {
                    Some((_, v)) => Flow::val(v.clone()),
                    None => Flow::Abort(format!("internal: no field `{}` on record", field)),
                }
            }
            Value::Variant { payload: VariantPayload::Record(fields), .. } => {
                match fields.iter().find(|(k, _)| *k == field) {
                    Some((_, v)) => Flow::val(v.clone()),
                    None => Flow::Abort(format!("internal: no field `{}` on variant", field)),
                }
            }
            other => Flow::Abort(format!(
                "internal: member access `.{}` on {}",
                field,
                other.type_name()
            )),
        }
    }

    fn eval_index(&mut self, object: Value, index: Value) -> Flow {
        let i = match index {
            Value::Int(i) => i,
            other => {
                return Flow::Abort(format!(
                    "internal: list index is {} not Int",
                    other.type_name()
                ))
            }
        };
        match object {
            Value::List(xs) => {
                if i < 0 || (i as usize) >= xs.len() {
                    // Matches the codegen OOB contract: abort + exit 1.
                    Flow::Abort("index out of bounds".into())
                } else {
                    Flow::val(xs[i as usize].clone())
                }
            }
            // A Range indexes like the list it stands for: `(0..<5)[2] == 2`. Both
            // backends materialize a `let`-bound range that is indexed (only the
            // head-ONLY case skips materialization, #1400), so the interp must
            // agree rather than dissent — it just computes the element instead of
            // building the block. Bounds match `Value::List` above: the codegen OOB
            // contract is abort + exit 1.
            Value::Range { start, end, inclusive } => {
                let len = if inclusive { end - start + 1 } else { end - start };
                if i < 0 || len <= 0 || i >= len {
                    Flow::Abort("index out of bounds".into())
                } else {
                    Flow::val(Value::Int(start + i))
                }
            }
            Value::Str(s) => {
                // String indexing returns the byte? Almide indexes strings via
                // string.* fns; a bare index on a String is unusual. Treat as
                // unsupported to avoid a wrong third vote.
                let _ = s;
                Flow::Unsupported("string index access".into())
            }
            other => Flow::Abort(format!(
                "internal: index access on {}",
                other.type_name()
            )),
        }
    }

    // ── String interpolation ───────────────────────────────────

    fn eval_string_interp(&mut self, parts: &[IrStringPart], scope: &Scope) -> Flow {
        let mut out = String::new();
        for part in parts {
            match part {
                IrStringPart::Lit { value } => out.push_str(value),
                IrStringPart::Expr { expr } => {
                    let v = val!(self.eval_expr(expr, scope));
                    // A bare top-level String stays raw; everything else routes
                    // through the bare-display path (which for compounds is
                    // `almide_repr`, for scalars is plain Display).
                    out.push_str(&v.display_bare());
                }
            }
        }
        Flow::val(Value::str(out))
    }

    // ── Blocks ──────────────────────────────────────────────────

    fn eval_block(
        &mut self,
        stmts: &[IrStmt],
        tail: Option<&IrExpr>,
        scope: &Scope,
    ) -> Flow {
        // A block introduces a new lexical frame.
        let frame = scope.child();
        for stmt in stmts {
            if let Err(f) = self.exec_stmt(stmt, &frame) {
                return f;
            }
        }
        match tail {
            Some(e) => self.eval_expr(e, &frame),
            None => Flow::val(Value::Unit),
        }
    }

    // ── The tail-call trampoline's spine walker ─────────────────

    /// Walk a function/closure body's TAIL SPINE iteratively — Block tails,
    /// If branches, the effect `Try{Call}` wrapper — and report whether it
    /// ends in a transferable call ([`crate::SpineOutcome::Transfer`]) or in
    /// anything else (evaluated normally, [`crate::SpineOutcome::Done`]).
    /// The engine looping on this is [`crate::Interpreter::run_callable`];
    /// everything NOT on the spine evaluates through the ordinary recursive
    /// `eval_expr`, so semantics change nowhere except stack growth.
    pub(crate) fn eval_body_spine(
        &mut self,
        body: &IrExpr,
        frame: &Scope,
    ) -> crate::SpineOutcome<'a> {
        use crate::SpineOutcome;
        let mut scope = frame.clone();
        let mut cur = body;
        loop {
            match &cur.kind {
                IrExprKind::Block { stmts, expr: Some(tail) } => {
                    let child = scope.child();
                    for s in stmts {
                        if let Err(f) = self.exec_stmt(s, &child) {
                            return SpineOutcome::Done(f);
                        }
                    }
                    scope = child;
                    cur = tail;
                }
                IrExprKind::If { cond, then, else_ } => match self.eval_expr(cond, &scope) {
                    Flow::Value(Value::Bool(true)) => cur = then,
                    Flow::Value(Value::Bool(false)) => cur = else_,
                    Flow::Value(other) => {
                        return SpineOutcome::Done(Flow::Abort(format!(
                            "internal: if-condition is {} not Bool",
                            other.type_name()
                        )))
                    }
                    other => return SpineOutcome::Done(other),
                },
                // The effect wrapper `f(..)!` in tail position: transfer the
                // call and let the engine fold the marker's normalization
                // over the chain's final value (the pending list).
                IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner } => {
                    return match self.spine_tail_call(inner, &scope) {
                        Some(SpineOutcome::Transfer { next, next_args, try_marker: None }) => {
                            SpineOutcome::Transfer {
                                next,
                                next_args,
                                try_marker: Some(cur.ty.clone()),
                            }
                        }
                        Some(out) => out,
                        None => SpineOutcome::Done(self.eval_expr(cur, &scope)),
                    };
                }
                IrExprKind::Call { .. } => {
                    return match self.spine_tail_call(cur, &scope) {
                        Some(out) => out,
                        None => SpineOutcome::Done(self.eval_expr(cur, &scope)),
                    };
                }
                _ => return SpineOutcome::Done(self.eval_expr(cur, &scope)),
            }
        }
    }

    /// The call half of the spine walk. `None` = "not transferable, and
    /// NOTHING has been evaluated yet" — the caller re-evaluates the node
    /// through the ordinary path. `Some(Done)` = a terminal tier (builtin,
    /// non-Value arg flow) resolved it here. `Some(Transfer)` = tail call to
    /// a lowered fn / closure with its arguments already evaluated.
    ///
    /// Resolution order mirrors `eval_named_call` exactly: builtins first
    /// (they may evaluate args, so once consulted this fn must complete),
    /// then variant ctors / Endian (pure lookups — declined to the normal
    /// path), then the fn table. A `mut`-param callee declines the transfer:
    /// copy-out needs the caller's lvalue, which a transferred frame no
    /// longer has (#1022).
    fn spine_tail_call(
        &mut self,
        expr: &IrExpr,
        scope: &Scope,
    ) -> Option<crate::SpineOutcome<'a>> {
        use crate::{SpineOutcome, TailCallee};
        let IrExprKind::Call { target, args, .. } = &expr.kind else {
            return None;
        };
        match target {
            CallTarget::Named { name } => {
                if let Some(flow) = self.eval_builtin_call(name.as_str(), args, scope) {
                    return Some(SpineOutcome::Done(flow));
                }
                if self.variant_ctor(*name).is_some()
                    || (args.is_empty()
                        && matches!(name.as_str(), "LittleEndian" | "BigEndian"))
                {
                    return None;
                }
                let func = self.fns.get(name).copied()?;
                if func.params.iter().any(|p| p.is_mut) {
                    return None;
                }
                let mut evaled = Vec::with_capacity(args.len());
                for a in args {
                    match self.eval_expr(a, scope) {
                        Flow::Value(v) => evaled.push(v),
                        other => return Some(SpineOutcome::Done(other)),
                    }
                }
                Some(SpineOutcome::Transfer {
                    next: TailCallee::Fn(func),
                    next_args: evaled,
                    try_marker: None,
                })
            }
            CallTarget::Computed { callee } => {
                let f = match self.eval_expr(callee, scope) {
                    Flow::Value(v) => v,
                    other => return Some(SpineOutcome::Done(other)),
                };
                let Value::Closure(clo) = f else {
                    return Some(SpineOutcome::Done(Flow::Abort(format!(
                        "internal: call of non-closure {}",
                        f.type_name()
                    ))));
                };
                let mut evaled = Vec::with_capacity(args.len());
                for a in args {
                    match self.eval_expr(a, scope) {
                        Flow::Value(v) => evaled.push(v),
                        other => return Some(SpineOutcome::Done(other)),
                    }
                }
                Some(SpineOutcome::Transfer {
                    next: TailCallee::Clo(clo),
                    next_args: evaled,
                    try_marker: None,
                })
            }
            // Module / Method tails keep the ordinary path: their dispatch
            // has value-keyed pre-body tiers (container ops, prims, bridge)
            // whose claim order cannot be probed without evaluating.
            _ => None,
        }
    }
}

include!("eval_loops_stmts.rs");
include!("eval_match.rs");

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
