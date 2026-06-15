//! `LowerCtx` methods: binds (extracted from lower/mod.rs).

use super::*;
use crate::{CallArg, Init, Op, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrPattern, VarId,
};
use almide_lang::types::Ty;

impl LowerCtx {

    pub(crate) fn lower_bind(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        if !is_heap_ty(ty) {
            // Scalar binding: a Copy value, no ownership accounting. A RESOLVABLE
            // scalar call (`let n = add(2, 3)`, `let m = string.len(s)`) is lowered to
            // a real executable `CallFn` (args materialized, the scalar result bound)
            // so it RUNS. Any other scalar value — arithmetic, a literal, an
            // unresolvable Method/Computed call — keeps the deferred `Const` + elided-
            // caps marker: its CONTENT is carried by a later brick, its calls still
            // folded for capabilities (`var n = obj.m()` elided ⇒ honest caps taint).
            if let Some(dst) = self.try_lower_scalar_call(value, ty) {
                self.value_of.insert(var, dst);
                return Ok(());
            }
            // An INT literal carries its real value (`ConstInt` → `(i64.const v)`),
            // the scalar-value foundation; other scalars stay the deferred `Const`.
            if let IrExprKind::LitInt { value } = &value.kind {
                let dst = self.fresh_value();
                self.value_of.insert(var, dst);
                self.ops.push(Op::ConstInt { dst, value: *value });
                return Ok(());
            }
            // A scalar Int Add/Sub/Mul computes its real value (IntBinOp), and a
            // scalar prim-floor call (`let n = prim.load32(a)`) becomes an Op::Prim —
            // both via lower_scalar_value; outside the subset it rolls back to `Const`.
            if let IrExprKind::BinOp { .. } | IrExprKind::RuntimeCall { .. } = &value.kind {
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(value) {
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
                self.ops.truncate(mark);
            }
            // A scalar `if`/`match` VALUE (`let step = if c then 0 else 1`) EXECUTES — only
            // the taken arm runs — via the if-marker machinery; a non-literal `match` or a
            // non-scalar subject falls through to the deferred `Const`.
            if let IrExprKind::If { cond, then, else_ } = &value.kind {
                if let Some(dst) = self.try_lower_scalar_if(cond, then, else_, ty) {
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
            }
            if let IrExprKind::Match { subject, arms } = &value.kind {
                if let Some(if_expr) = self.desugar_match_to_if(subject, arms, ty) {
                    if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                        if let Some(dst) = self.try_lower_scalar_if(cond, then, else_, ty) {
                            self.value_of.insert(var, dst);
                            return Ok(());
                        }
                    }
                }
            }
            let dst = self.fresh_value();
            self.value_of.insert(var, dst);
            self.ops.push(Op::Const { dst });
            self.record_elided_calls(value);
            return Ok(());
        }
        match &value.kind {
            // Alias: `var b = a` — b is a NEW handle denoting the SAME heap
            // object as a, acquiring its own owned reference (the single
            // fresh-vs-alias decision).
            IrExprKind::Var { id } => {
                let src = self.value_for(*id)?;
                let dst = self.fresh_value();
                self.value_of.insert(var, dst);
                self.ops.push(Op::Dup { dst, src });
                self.live_heap_handles.push(dst);
                Ok(())
            }
            // A fresh heap value (literal container / string / Option·Result
            // variant). Constructors lower like a container literal: a fresh
            // `Alloc` (value-semantics — the payload is copied, not consumed), the
            // proven-sound convention the corpus already verifies for List/Record.
            // An ERROR OPERATOR (`e!`/`e?`/`e ?? d`/`e?.f`) likewise yields a FRESH
            // value (the unwrapped/defaulted/mapped result, deferred like every
            // Opaque); its operand's calls are captured by `record_elided_calls`.
            // (Almide has NO try/catch: `e?` is `Result → Option`, `e ?? d` is
            // unwrap-or-default, `e?.f` is optional chaining — all TOTAL value maps, no
            // control flow. Only `e!` (`Unwrap`, effect-fn) PROPAGATES an error — an
            // early return that is DEFERRED here: the always-continue path is self-
            // consistent (each handle still drops exactly once, so memory-safe); error
            // propagation is functional, not a safety property.)
            IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. }
            | IrExprKind::EmptyMap
            | IrExprKind::Record { .. }
            | IrExprKind::SpreadRecord { .. }
            | IrExprKind::Tuple { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::StringInterp { .. }
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. }
            | IrExprKind::OptionSome { .. }
            | IrExprKind::OptionNone
            | IrExprKind::BinOp { .. }
            | IrExprKind::UnOp { .. }
            | IrExprKind::Try { .. }
            | IrExprKind::Unwrap { .. }
            | IrExprKind::UnwrapOr { .. }
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. }
            // A CLOSURE value (`var f = (x) => …`) is a fresh heap env, and a RANGE is
            // a fresh value — both `Alloc{Opaque}`. The closure is NOT invoked here, so
            // its body's calls are elided ⇒ the gate taints the function caps-unverified
            // honestly (the closure's invocation capabilities are unknown).
            | IrExprKind::Lambda { .. }
            | IrExprKind::ClosureCreate { .. }
            | IrExprKind::Range { .. }
            // A RUNTIME CALL result is a fresh value (its call is elided ⇒ the gate
            // taints the function honestly, like Method/Computed).
            | IrExprKind::RuntimeCall { .. } => {
                let dst = self.fresh_value();
                let repr = repr_of(ty)?;
                let init = alloc_init(value);
                self.value_of.insert(var, dst);
                self.ops.push(Op::Alloc { dst, repr, init });
                self.live_heap_handles.push(dst);
                self.record_elided_calls(value);
                Ok(())
            }
            // `var v = r.x` / `xs[i]` — a HEAP extraction: alias the container
            // (`Op::Dup`), bound here and dropped at scope end (cert `a` + `d`). When
            // the container is NOT a tracked var (`f().x`, nested `a.b.c`), there is no
            // single `src` to `Dup`, so fall back to a deferred fresh `Alloc{Opaque}`
            // (the extracted value deferred, its container's calls captured) — never a
            // wall (totality), always memory-safe (a clean fresh alloc).
            IrExprKind::Member { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::TupleIndex { .. } => {
                let dst = match self.lower_heap_extraction(value) {
                    Ok(dst) => dst,
                    Err(_) => {
                        let dst = self.fresh_value();
                        let repr = repr_of(ty)?;
                        self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                        self.record_elided_calls(value);
                        dst
                    }
                };
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                Ok(())
            }
            // `var x = f(...)` — a USER call returning a heap value. The result is
            // a FRESH OWNED heap value (the callee's return-mode signature, read
            // from the bind's heap type — the checker need not open the callee).
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args)?;
                let dst = self.fresh_value();
                let repr = repr_of(ty)?;
                self.value_of.insert(var, dst);
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                self.live_heap_handles.push(dst);
                Ok(())
            }
            // `var x = string.trim(s)` — a stdlib MODULE call returning a heap
            // value. Admitted only when first-order + pure (else walled); the
            // fresh owned result is bound and dropped at scope end, exactly like
            // the `Named` case above.
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let dst =
                    self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, ty)?;
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                Ok(())
            }
            // `var x = obj.method(args)` / `var x = (g)(args)` — an UNRESOLVABLE
            // `Method`/`Computed` callee bound to a heap var. Model the result as ONE
            // deferred fresh `Alloc{Opaque}` (its receiver's/args' calls captured by
            // `record_elided_calls`; the method/computed call itself is elided, so the
            // `ir_calls > mir_calls` gate taints the function caps-unverified — honest).
            IrExprKind::Call { .. } => {
                let dst = self.fresh_value();
                let repr = repr_of(ty)?;
                self.value_of.insert(var, dst);
                self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                self.live_heap_handles.push(dst);
                self.record_elided_calls(value);
                Ok(())
            }
            // `var x = if c then … else …` — a heap-result branch. LINEARIZE the arms
            // (each per-arm balanced, its value deferred), then bind `x` to ONE fresh
            // `Alloc{Opaque}` — the merged result slot. Memory-safe by construction
            // (the arms balance; the result is a clean fresh alloc dropped at scope
            // end); which arm's value it equals is functional, deferred like every
            // `Opaque`. The same WALLS as statement position still apply per arm.
            IrExprKind::If { .. } | IrExprKind::Match { .. } => {
                self.lower_branch(value)?;
                let dst = self.fresh_value();
                let repr = repr_of(ty)?;
                self.value_of.insert(var, dst);
                self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                self.live_heap_handles.push(dst);
                Ok(())
            }
            // `var x = { stmts; tail }` — a heap BLOCK value. Lower the block's
            // statements (their locals ride to the enclosing scope and are dropped at
            // scope end), then bind `x` to the block's heap TAIL via `lower_bind` (a var
            // alias / fresh literal / call result / nested branch — all proven shapes).
            // A tail-less block is never heap-typed, so it falls through to the wall.
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                self.lower_bind(var, ty, tail)
            }
            other => Err(LowerError::Unsupported(format!(
                "heap bind from {} not in this brick",
                kind_name(other)
            ))),
        }
    }

    /// `let (a, b) = …` — a TUPLE destructuring bind. Two sound shapes:
    ///
    /// 1. From a tuple LITERAL `(x, y)` of the same arity — lowered COMPONENT-WISE
    ///    as ordinary binds (`lower_bind` reused: a `Var` is an alias `Dup`, a
    ///    literal an `Alloc`/`Const`, a call a real `CallFn` whose caps are
    ///    captured, NOT elided). The tuple is never materialized.
    /// 2. From a tracked heap VAR `t` — each HEAP component aliases the WHOLE
    ///    container `t` (an `Op::Dup`, the container-grain field access of the
    ///    field-access op), each SCALAR component is a `Const` copy. Aliasing the
    ///    container keeps it alive for each component's lifetime (a conservative
    ///    lifetime widening, never a UAF); component-PRECISE identity (`a == t.0`)
    ///    is deferred to the layout brick.
    ///
    /// A `Wildcard` component is ignored. Anything else — a non-tuple/nested/
    /// constructor/record pattern, or a value that is neither a matching tuple
    /// literal nor a tracked heap var — stays an explicit `Unsupported` (totality).
    pub(crate) fn lower_destructure(&mut self, pattern: &IrPattern, value: &IrExpr) -> Result<(), LowerError> {
        // Shape 1: component-wise from a same-arity tuple LITERAL — each component is
        // bound to the ACTUAL element (a fresh value / alias, not a container alias),
        // the most precise lowering. The element's call caps are captured, not elided.
        if let (IrPattern::Tuple { elements: pats }, IrExprKind::Tuple { elements: vals }) =
            (pattern, &value.kind)
        {
            if pats.len() == vals.len() {
                for (p, v) in pats.iter().zip(vals) {
                    match p {
                        IrPattern::Bind { var, ty } => self.lower_bind(*var, ty, v)?,
                        IrPattern::Wildcard => {}
                        // A NESTED tuple sub-pattern `(b, c)` binds against the
                        // corresponding element value `v` — recurse (the same two sound
                        // shapes: a same-arity tuple literal binds component-wise, a
                        // tracked heap var aliases the container).
                        IrPattern::Tuple { .. } => self.lower_destructure(p, v)?,
                        _ => {
                            return Err(LowerError::Unsupported(
                                "destructure sub-pattern (only a bound var, `_`, or nested tuple) not in this brick"
                                    .into(),
                            ))
                        }
                    }
                }
                return Ok(());
            }
        }
        // Shape 2 (general): materialize/borrow the value as a SUBJECT (a tracked heap
        // var is borrowed, a fresh heap value is materialized + dropped at scope end),
        // then bind the pattern CONTAINER-GRAIN (each heap binding aliases the whole
        // subject — `bind_pattern`). Handles tuple-from-var, constructor, record, and
        // option/result destructuring; the bound vars drop at scope end.
        let subject: Option<ValueId> = if is_heap_ty(&value.ty) {
            match self.lower_call_args(std::slice::from_ref(value))?.into_iter().next() {
                Some(CallArg::Handle(v)) => Some(v),
                _ => None,
            }
        } else {
            self.record_elided_calls(value);
            None
        };
        self.bind_pattern(pattern, subject)
    }

    /// Introduce the variables a destructuring `pattern` binds, CONTAINER-GRAIN: a
    /// HEAP payload/field/element aliases the WHOLE `subject` (`Op::Dup`), a SCALAR one
    /// is a `Const`. Aliasing the container keeps it (and thus the bound value within
    /// it) alive for the binding's lifetime — a conservative lifetime WIDENING that
    /// can never shorten a lifetime, so never a use-after-free; and it reuses the
    /// proven `a`/`Op::Dup` event, so the Coq checker and the `#a == #Dup` backing gate
    /// are UNCHANGED. HONEST SCOPE (value-content, NOT safety): a bound var denotes "a
    /// reference to the SUBJECT", not "the payload's value" — payload/field-PRECISE
    /// aliasing needs the layout brick (offsets + per-field heap-ness) and is deferred,
    /// exactly like `Init::Opaque` content. WALLED: a `RecordPattern` shorthand field
    /// (`{ name }` — no bound `VarId` to thread) and a heap binding over a non-heap
    /// subject (the container has no handle to `Dup`).
    pub(crate) fn bind_pattern(
        &mut self,
        pattern: &IrPattern,
        subject: Option<ValueId>,
    ) -> Result<(), LowerError> {
        match pattern {
            IrPattern::Wildcard | IrPattern::None | IrPattern::Literal { .. } => Ok(()),
            IrPattern::Bind { var, ty } => {
                let dst = self.fresh_value();
                if is_heap_ty(ty) {
                    let src = subject.ok_or_else(|| {
                        LowerError::Unsupported(
                            "heap pattern binding over a non-heap subject (no container to alias) not in this brick".into(),
                        )
                    })?;
                    self.ops.push(Op::Dup { dst, src });
                    self.live_heap_handles.push(dst);
                } else {
                    self.ops.push(Op::Const { dst });
                }
                self.value_of.insert(*var, dst);
                Ok(())
            }
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
                self.bind_pattern(inner, subject)
            }
            IrPattern::Constructor { args, .. } => {
                for p in args {
                    self.bind_pattern(p, subject)?;
                }
                Ok(())
            }
            IrPattern::Tuple { elements } | IrPattern::List { elements } => {
                for p in elements {
                    self.bind_pattern(p, subject)?;
                }
                Ok(())
            }
            IrPattern::RecordPattern { fields, .. } => {
                for f in fields {
                    match &f.pattern {
                        Some(p) => self.bind_pattern(p, subject)?,
                        None => {
                            return Err(LowerError::Unsupported(
                                "record pattern shorthand field (no bound VarId) not in this brick".into(),
                            ))
                        }
                    }
                }
                Ok(())
            }
        }
    }
}
