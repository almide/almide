//! `LowerCtx` methods: tail (extracted from lower/mod.rs).

use super::*;
use crate::{Init, Op, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind,
};
use almide_lang::types::Ty;

impl LowerCtx {

    /// Lower a HEAP field/element/tuple/map EXTRACTION (`r.x`, `xs[i]`, `t.0`,
    /// `m[k]` with a heap result) to an ALIAS of the CONTAINER: `Op::Dup{dst,
    /// src: <container value>}`. The extracted value is modeled as a SECOND HANDLE
    /// on the whole container — the v1 container-grain field access. This is sound:
    /// aliasing the container keeps it (and thus its field) alive for the value's
    /// whole lifetime — a conservative lifetime WIDENING that can never shorten a
    /// lifetime, so never a use-after-free; and it reuses the proven `a`/`Op::Dup`
    /// event, so the Coq checker and the `#a == #Dup` backing gate are UNCHANGED.
    ///
    /// HONEST SCOPE (value-content, NOT safety): `dst` denotes "a reference to the
    /// CONTAINER", not "the field's value" — field-PRECISE aliasing (the value's
    /// own object identity) needs the not-yet-existent layout brick (offsets +
    /// per-field heap-ness) and is deferred, exactly like every heap value's
    /// `Init::Opaque` content. Reading/mutating through `dst` as if it were the
    /// field is the deferred-functional gap, not a memory-safety hole.
    ///
    /// Admitted ONLY when the container is itself a TRACKED heap value (a bound
    /// var) — a nested extraction (`a.b.c`) has no single `src` to `Dup` and stays
    /// walled (totality). The caller decides placement (bind / move-out / borrow).
    pub(crate) fn lower_heap_extraction(&mut self, expr: &IrExpr) -> Result<ValueId, LowerError> {
        let container = extraction_container(expr).ok_or_else(|| {
            LowerError::Unsupported(format!(
                "{} is not a field/element extraction",
                kind_name(&expr.kind)
            ))
        })?;
        let src = match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => self.value_or_global(*id)?,
            other => {
                return Err(LowerError::Unsupported(format!(
                    "heap extraction whose container is {} (not a tracked heap var) not in this brick",
                    kind_name(other)
                )))
            }
        };
        let dst = self.fresh_value();
        self.ops.push(Op::Dup { dst, src });
        Ok(dst)
    }

    /// Lower the body's tail expression to the function's return value.
    /// - heap `Var` tail → MOVE-OUT: the handle is consumed at the boundary
    ///   (returned as `ret`, removed from the live set so it is not also dropped).
    /// - scalar `Var` tail → returned by value (no ownership; `ret` names it).
    /// - scalar literal tail → a fresh `Const`, returned by value.
    /// - `Unit` / absent → a Unit-returning body (no return value).
    /// Anything else is an explicit `Unsupported` (flight-grade totality).
    pub(crate) fn lower_tail(&mut self, tail: Option<&IrExpr>) -> Result<Option<ValueId>, LowerError> {
        // (The tail Try/Unwrap early-return-over-a-live-heap-local wall is LIFTED — the v0
        // wasm codegen now frees live heap locals before the Err `return_`; see lower_stmt.)
        let tail = match tail {
            Some(t) => t,
            None => return Ok(None),
        };
        // A BLOCK tail (`fn f() = { stmts; e }`, or a nested block in tail position):
        // lower its statements (their heap locals ride to the ENCLOSING scope's end —
        // a conservative lifetime extension, dropped exactly once, never a double-free)
        // and recurse on its own tail, which is the value. Any kind of result.
        if let IrExprKind::Block { stmts, expr } = &tail.kind {
            for s in stmts {
                self.lower_stmt(s)?;
            }
            return self.lower_tail(expr.as_deref());
        }
        if matches!(tail.ty, Ty::Unit) {
            return match &tail.kind {
                IrExprKind::Unit => Ok(None),
                // A Unit-typed call tail is an EFFECT call (e.g. `println(s)`):
                // lower it as a statement-effect, no return value.
                IrExprKind::Call { .. } => {
                    self.lower_effect_call(tail)?;
                    Ok(None)
                }
                // A Unit-typed `if`/`match` tail is LINEARIZED control flow.
                IrExprKind::If { .. } | IrExprKind::Match { .. } => {
                    self.lower_branch(tail)?;
                    Ok(None)
                }
                // A Unit-typed `for`/`while` tail is a per-iteration-framed loop.
                IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                    self.lower_for_in(*var, var_tuple, iterable, body)?;
                    Ok(None)
                }
                IrExprKind::While { cond, body } => {
                    self.lower_while(cond, body)?;
                    Ok(None)
                }
                other => Err(LowerError::Unsupported(format!(
                    "Unit-typed tail {} not in this brick",
                    kind_name(other)
                ))),
            };
        }
        if is_heap_ty(&tail.ty) {
            return match &tail.kind {
                IrExprKind::Var { id } => {
                    let v = self.value_or_global(*id)?;
                    if self.param_values.contains(&v) {
                        // Returning a BORROWED param directly would move out a
                        // reference we do not own (the caller's) — a double-free. AUTO-
                        // ACQUIRE one first: `Op::Dup` (cert `a`) then move out the new
                        // handle (cert `m`) — exactly `let q = p; q`. The returned `am`
                        // is an OWNED reference (rc incremented), independent of the
                        // caller's, so no double-free; the proven checker accepts it.
                        let dst = self.fresh_value();
                        self.ops.push(Op::Dup { dst, src: v });
                        return Ok(Some(dst)); // moved out, NOT added to live_heap_handles
                    }
                    self.live_heap_handles.retain(|h| *h != v); // moved out, not dropped
                    Ok(Some(v))
                }
                // A fresh heap literal returned directly (`fn f() = [1, 2, 3]`):
                // allocate it and move it out. It is NOT added to
                // `live_heap_handles`, so it is the return value (consumed at the
                // boundary) and never also dropped. Cert: alloc(i) + move-out(m) =
                // balanced — and the runtime correspondence is exact (a real
                // Alloc, a real move-out), so the gate fully covers it.
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
                // A CLOSURE value returned (`fn mk() = (x) => …`) is a fresh heap env;
                // a RANGE is a fresh value — both `Alloc{Opaque}`, moved out.
                | IrExprKind::Lambda { .. }
                | IrExprKind::ClosureCreate { .. }
                | IrExprKind::Range { .. }
                | IrExprKind::RuntimeCall { .. } => {
                    let dst = self.fresh_value();
                    let repr = repr_of(&tail.ty)?;
                    let init = alloc_init(tail);
                    self.ops.push(Op::Alloc { dst, repr, init });
                    self.record_elided_calls(tail);
                    Ok(Some(dst))
                }
                // A function-call result returned directly (`fn f() = g(xs)`): the
                // callee's heap result is a FRESH OWNED value (its return-mode
                // signature), moved out — NOT added to live_heap_handles. Cert:
                // CallFn-result + move-out, identical to the already-verified
                // `var x = g(xs); x`, so the gate covers it by the same evidence
                // (the runtime correspondence is exact — the callee returns rc 1).
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                    let lowered = self.lower_call_args(args)?;
                    let dst = self.fresh_value();
                    let repr = repr_of(&tail.ty)?;
                    self.ops.push(Op::CallFn {
                        dst: Some(dst),
                        name: name.as_str().to_string(),
                        args: lowered,
                        result: Some(repr),
                    });
                    Ok(Some(dst))
                }
                // `fn f() = string.trim(s)` — a stdlib MODULE call result returned
                // directly. Admitted only when first-order + pure; the fresh owned
                // result is moved out (NOT added to live_heap_handles), like the
                // `Named` case above.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                    let dst = self.lower_pure_module_value_call(
                        module.as_str(),
                        func.as_str(),
                        args,
                        &tail.ty,
                    )?;
                    Ok(Some(dst))
                }
                // `fn f(r) = r.x` — a HEAP extraction returned directly: alias the
                // container (`Op::Dup`) and move it out (cert `a` + `m`). A non-var
                // container (`f().x`, nested) falls back to a deferred fresh Opaque,
                // moved out — never walled.
                IrExprKind::Member { .. }
                | IrExprKind::IndexAccess { .. }
                | IrExprKind::MapAccess { .. }
                | IrExprKind::TupleIndex { .. } => {
                    let dst = match self.lower_heap_extraction(tail) {
                        Ok(dst) => dst,
                        Err(_) => {
                            let dst = self.fresh_value();
                            let repr = repr_of(&tail.ty)?;
                            self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                            self.record_elided_calls(tail);
                            dst
                        }
                    };
                    Ok(Some(dst))
                }
                // `fn f() = if c then … else …` — a heap-result branch RETURNED.
                // LINEARIZE the arms (per-arm balanced, values deferred) and move out
                // ONE fresh `Alloc{Opaque}` — the merged result slot, NOT added to
                // live_heap_handles (it is the return value). See `lower_branch`.
                IrExprKind::If { .. } | IrExprKind::Match { .. } => {
                    self.lower_branch(tail)?;
                    let dst = self.fresh_value();
                    let repr = repr_of(&tail.ty)?;
                    self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                    Ok(Some(dst))
                }
                // `fn f(o) = o.method()` / `(g)()` returned — an UNRESOLVABLE
                // `Method`/`Computed` callee (the `Named`/`Module` arms are above).
                // Move out ONE deferred fresh `Alloc{Opaque}`; the call is elided so
                // the gate taints the function caps-unverified (honest).
                IrExprKind::Call { .. } => {
                    let dst = self.fresh_value();
                    let repr = repr_of(&tail.ty)?;
                    self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                    self.record_elided_calls(tail);
                    Ok(Some(dst))
                }
                other => Err(LowerError::Unsupported(format!(
                    "heap move-out from {} (only a bound var, fresh literal, or call) not in this brick",
                    kind_name(other)
                ))),
            };
        }
        // Scalar return value (Copy — no ownership accounting). A scalar `BinOp`/
        // `UnOp` is a FRESH computed scalar (arithmetic / comparison / logic), so it
        // is a `Const` like a literal — its operands carry their own ownership.
        match &tail.kind {
            IrExprKind::Var { id } => Ok(Some(self.value_or_global(*id)?)),
            // A scalar-result resolvable CALL tail (`fn f() = g()`, `= add(2, 3)`,
            // `= string.len(s)`): a real executable `CallFn` (args materialized, the
            // scalar result returned). An unresolvable Method/Computed callee (or an
            // unsupported arg) falls through to the deferred `Const` + elided-caps
            // marker below — the call is captured for caps, its value deferred.
            IrExprKind::Call { .. } => {
                if let Some(dst) = self.try_lower_scalar_call(tail, &tail.ty) {
                    return Ok(Some(dst));
                }
                let dst = self.fresh_value();
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // An INT literal materializes to a real value (the scalar-value
            // foundation): `ConstInt` renders `(i64.const v)`, so a fn returning a
            // literal returns the right value, not the deferred-`Const` zero. This is
            // what lets a self-hosted runtime fn compute real offsets/lengths.
            IrExprKind::LitInt { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: *value });
                Ok(Some(dst))
            }
            // A scalar Int Add/Sub/Mul computes its REAL value (IntBinOp over
            // recursively-lowered operands), so a fn `add(a, b) = a + b` returns the
            // sum — not the deferred-Const zero. Outside the int-arith subset (Div/
            // Mod/cmp/logic/Float) it rolls back and falls through to the Const below.
            // A scalar Int Add/Sub/Mul OR a scalar prim-floor call (`= prim.load32(a)`)
            // computes a real value via lower_scalar_value (IntBinOp / Op::Prim);
            // outside the subset it rolls back to the deferred Const + elided marker.
            IrExprKind::BinOp { .. } | IrExprKind::RuntimeCall { .. } => {
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(tail) {
                    return Ok(Some(dst));
                }
                self.ops.truncate(mark);
                let dst = self.fresh_value();
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            IrExprKind::LitBool { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::UnOp { .. }
            // A SCALAR field/element/tuple extraction is an unambiguous COPY (a
            // scalar is never reference-counted), so it is a `Const` — its
            // container carries its own ownership. (A HEAP extraction is an ALIAS
            // / share — it needs a layout-aware field-access op with `Dup`
            // semantics and stays walled until that brick.)
            | IrExprKind::Member { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::TupleIndex { .. }
            // A SCALAR error-operator result (`x!`/`x ?? d`/`x?.f` yielding a scalar) is
            // likewise a fresh `Const`; the operator's value + early-return are deferred.
            | IrExprKind::Try { .. }
            | IrExprKind::Unwrap { .. }
            | IrExprKind::UnwrapOr { .. }
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. }
            // A RANGE returned: a fresh `Const` (no ownership); any analyzable callee
            // inside it is captured for caps by `record_elided_calls`. (A scalar-result
            // CALL is handled by its own arm above — a real executable `CallFn` when
            // resolvable, else the same deferred `Const` + elided marker.)
            | IrExprKind::Range { .. } => {
                let dst = self.fresh_value();
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A scalar-result `if`/`match` tail: LINEARIZE the arms (their effects /
            // arm-local ownership lowered, per-arm balanced) and emit ONE `Const` as
            // the merged scalar result — both arms cross by the SAME no-event pattern
            // (a Copy scalar), so nothing per-arm escapes the branch.
            IrExprKind::If { .. } | IrExprKind::Match { .. } => {
                self.lower_branch(tail)?;
                let dst = self.fresh_value();
                self.ops.push(Op::Const { dst });
                Ok(Some(dst))
            }
            other => Err(LowerError::Unsupported(format!(
                "scalar tail {} not in this brick",
                kind_name(other)
            ))),
        }
    }
}
