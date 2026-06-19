//! `LowerCtx` methods: binds (extracted from lower/mod.rs).

use super::*;
use crate::{CallArg, Init, Op, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrPattern, VarId,
};
use almide_lang::types::Ty;

impl LowerCtx {

    /// Lift a NON-CAPTURING lambda `(params) => body` into a fresh top-level MIR function
    /// (the closures machinery) and emit an `Op::FuncRef` binding its table slot, returning
    /// that scalar value (recorded in `funcref_values` so a later call through it lowers to
    /// `Op::CallIndirect`). Returns `None` for a CAPTURING lambda (its body references an
    /// enclosing local — a real closure environment the proven model cannot represent) or a
    /// body outside the lowering subset; the caller then keeps the deferred `Opaque` model.
    ///
    /// SOUNDNESS: the lifted body is lowered by the SAME `lower_body_into` as any function,
    /// so it carries its own ownership / name-totality / capability certificate that the
    /// proven checker re-verifies. Its capabilities reach THIS function through the
    /// `Op::FuncRef` edge — folded at closure CREATION (coverage-free; see
    /// `certificate::reachable_caps` / `reachable_caps_or_tainted`), so a printing lambda
    /// can never be silently caps-verified regardless of how/whether it is later invoked.
    /// The lambda is named `__lambda_<fn_name>_<n>` — file-unique (the harness keys the
    /// in-profile map by name), with nested lifts flattened into this function's set.
    pub(crate) fn lift_lambda(
        &mut self,
        params: &[(VarId, Ty)],
        body: &IrExpr,
    ) -> Option<ValueId> {
        // free_vars over the lambda's own params reports exactly its captures (a `Var` node
        // denotes only locals). A non-empty set ⇒ a real environment ⇒ not liftable here.
        let mut bound: std::collections::HashSet<VarId> = std::collections::HashSet::new();
        for (v, _) in params {
            bound.insert(*v);
        }
        if !almide_ir::free_vars::free_vars(body, &bound).is_empty() {
            return None;
        }
        // Lower the body in a FRESH sub-context sharing only the globals (its own value
        // space + params). A failure (a body outside the subset) aborts the lift cleanly —
        // nothing is emitted into `self`, so the caller's deferred fallback stays sound.
        let mut sub = LowerCtx {
            globals: self.globals.clone(),
            fn_name: self.fn_name.clone(),
            // The lifted body may access a record/tuple field (`(p) => p.x`), so it needs
            // the VALUE-MODEL field registry too.
            record_layouts: self.record_layouts.clone(),
            ..Default::default()
        };
        let mut mir_params = Vec::new();
        for (v, ty) in params {
            let pv = sub.fresh_value();
            sub.value_of.insert(*v, pv);
            let repr = repr_of(ty).ok()?;
            if repr.is_heap() {
                sub.param_values.insert(pv);
                // SEED the param's variant/aggregate read-shape — IDENTICAL to `bind_params`.
                // A closure over a record/tuple param (`(r) => r.name`, `(r) => r.v` — the
                // List[R] map/sort_by key fns) needs `r` in `materialized_aggregates` so its
                // field read borrows the real slot; an Option/Result param needs its variant
                // tracking so a `match` inside the closure executes. Without this the lifted
                // body read an EMPTY deferred value (the silent-empty List[R] map bug).
                sub.seed_variant_param(pv, ty);
            }
            mir_params.push(crate::MirParam { value: pv, repr });
        }
        let ret = sub.lower_body_into(body).ok()?;
        let name = format!("__lambda_{}_{}", self.fn_name, self.lifted.len());
        let mut nested = std::mem::take(&mut sub.lifted);
        // A lifted lambda is pure-by-default (declared ∅): an effectful one is NOT silently
        // accepted — its own caps witness (Stdout used ⊄ ∅ declared) faults the subset
        // checker, and the FuncRef edge propagates that to every holder. (A lambda carries
        // no `is_effect` flag in the IR; ∅ is the conservative, never-over-accepting bound.)
        let lifted_fn = crate::MirFunction {
            name: name.clone(),
            params: mir_params,
            ops: sub.ops,
            ret,
            declared_caps: Vec::new(),
            heap_slot_masks: sub.record_masks.iter().map(|(v, m)| (*v, m.clone())).collect(),
        };
        self.lifted.push(lifted_fn);
        self.lifted.append(&mut nested);
        let dst = self.fresh_value();
        self.ops.push(Op::FuncRef { dst, name });
        self.funcref_values.insert(dst);
        Some(dst)
    }

    /// Lower a `List[String]` LITERAL to an alloc_list_str + per-element move-in. Each element is
    /// stored into a nested-ownership `DynListStr` (freed recursively via `DropListStr` at scope end,
    /// cert `i`+`d`). Element ownership by kind:
    /// - a LitStr / ConcatStr is a FRESH owned String (cert `i`), MOVED in (store handle + `Consume`);
    /// - a `Var` binds a STILL-LIVE owned String the list must not steal — it gets its OWN reference
    ///   via `Dup` (cert `a`/+1), then that fresh handle is `Consume`d into the list. The original var
    ///   keeps its reference (its scope-end drop stays balanced), and the list owns a distinct one
    ///   (DropListStr releases it) — no double-free, no leak. (`[e0, e1]` of `string.repeat` results,
    ///   `[a, a]` of a computed `a` — the same var may appear twice, each occurrence its own `Dup`.)
    /// GATED to those element kinds; any other (a heap-returning call as a bare element, a member
    /// access) defers. Gate-first so no partial emission.
    pub(crate) fn try_lower_str_list_literal(&mut self, value: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::List { elements } = &value.kind else {
            return None;
        };
        // A `List[String]` OR a `List[ScalarAggregate]` whose every element is a SCALAR-only
        // record OR tuple literal — all are NESTED-OWNERSHIP lists (i64 slots holding owned heap
        // handles, recursively freed via `DropListStr`'s per-slot `rc_dec`). A scalar record/tuple
        // is a FLAT block (no inner heap slots), so `rc_dec` frees it correctly (no String-specific
        // recursion). This is what makes `[Point{..}, Point{..}]` (then `list.map(…, p => p.x)`)
        // AND `[(1, 100), (127, 300)]` (then a `for (x, y) in …`) materialize as a REAL list of the
        // right length. A `List` of any OTHER heap element (a heap-field record/tuple, a List, a
        // call result) defers — a heap-field aggregate needs the masked recursive drop this builder
        // (a flat per-slot `rc_dec`) does not emit, so its inner heap would leak; gated out by
        // `scalar_slots` (which is `None` for any non-scalar field).
        let elem_str = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String));
        let elem_scalar_aggregate = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && self.aggregate_field_tys(&a[0])
                    .and_then(|(_, tys)| layout::scalar_slots(&tys)).is_some());
        // A `List[Value]` (`[value.int(1), value.str("a")]`) — its slots hold OWNED dynamic Values,
        // each freed RECURSIVELY at scope end via `Op::DropListValue` (`$__drop_value` per element),
        // so a Str/Array element's nested payload is reclaimed. Elements are fresh-owned ctor CALLS
        // (Module `value.*` / a Named `Value`-returning fn) or a tracked Value Var (Dup'd).
        let elem_value = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_value_ty(&a[0]));
        if (!elem_str && !elem_scalar_aggregate && !elem_value) || elements.is_empty() {
            return None;
        }
        // Gate: every element must be a fresh-owned String (LitStr/ConcatStr) OR a tracked
        // heap Var (so we can Dup it) OR — for an aggregate list — a scalar-only record/tuple
        // LITERAL we can materialize OR — for a value list — a fresh-owned Value CALL. A Var whose
        // value isn't tracked here cannot be Dup'd → defer.
        let all_lowerable = elements.iter().all(|e| match &e.kind {
            IrExprKind::LitStr { .. } | IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => true,
            IrExprKind::Var { id } => self.value_of.contains_key(id),
            IrExprKind::Record { .. } | IrExprKind::Tuple { .. } => elem_scalar_aggregate,
            IrExprKind::Call { target: CallTarget::Named { .. } | CallTarget::Module { .. }, .. } => {
                elem_value && is_value_ty(&e.ty)
            }
            _ => false,
        });
        if !all_lowerable {
            return None;
        }
        let ptr = crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT };
        let n = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: n, value: elements.len() as i64 });
        let list = self.fresh_value();
        self.ops.push(Op::Alloc { dst: list, repr: ptr, init: Init::DynListStr { len: n } });
        // A `List[Value]` drops via `Op::DropListValue` (recursive `$__drop_value` per element); a
        // String/aggregate list via the flat-element `Op::DropListStr`. Marking the right set is what
        // makes the scope-end drop reclaim each element's nested payload (no leak).
        if elem_value {
            self.value_elem_lists.insert(list);
        } else {
            self.heap_elem_lists.insert(list);
        }
        self.materialized_lists.insert(list);
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![list] });
        for (i, elem) in elements.iter().enumerate() {
            let ev = match &elem.kind {
                IrExprKind::LitStr { value: s } => {
                    let obj = self.fresh_value();
                    self.ops.push(Op::Alloc { dst: obj, repr: ptr, init: Init::Str(s.clone()) });
                    obj
                }
                // A Var element: acquire a fresh owned reference (Dup) the list will own; the original
                // binding keeps its own reference. The dup is then Consume'd (moved) into the slot.
                IrExprKind::Var { id } => {
                    let src = *self.value_of.get(id)?;
                    let dup = self.fresh_value();
                    self.ops.push(Op::Dup { dst: dup, src });
                    dup
                }
                // A scalar-only record literal element — materialize a fresh OWNED record
                // block (`try_lower_scalar_record_construct`, cert `i`), moved into the slot.
                IrExprKind::Record { .. } => self.try_lower_scalar_record_construct(elem)?,
                // A scalar-only tuple literal element (`(1, 100)`) — materialize a fresh OWNED
                // flat 2-slot block (`try_lower_scalar_tuple_construct`, cert `i`), moved into the
                // slot. The SAME flat shape as a scalar record, so the list's per-slot `rc_dec`
                // frees it correctly.
                IrExprKind::Tuple { .. } => self.try_lower_scalar_tuple_construct_for_elem(elem)?,
                // A `Value` ctor CALL element (`value.int(1)`, `value.str(s)`) — a fresh OWNED Value
                // (Module via the pure-call path, Named via CallFn), MOVED into the slot. The list's
                // `Op::DropListValue` frees each recursively at scope end.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if elem_value =>
                {
                    self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, &elem.ty).ok()?
                }
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if elem_value => {
                    let lowered = self.lower_call_args(args).ok()?;
                    let obj = self.fresh_value();
                    let repr = repr_of(&elem.ty).ok()?;
                    self.ops.push(Op::CallFn {
                        dst: Some(obj),
                        name: name.as_str().to_string(),
                        args: lowered,
                        result: Some(repr),
                    });
                    obj
                }
                _ => self.try_lower_concat_str(elem)?,
            };
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
            let slot = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: slot, op: crate::IntOp::Add, a: h, b: off });
            let eh = self.fresh_value();
            self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(eh), args: vec![ev] });
            self.ops.push(Op::Prim { kind: crate::PrimKind::Store { width: 8 }, dst: None, args: vec![slot, eh] });
            self.ops.push(Op::Consume { v: ev });
        }
        Some(list)
    }

    pub(crate) fn lower_bind(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        // `let r = e!` (Unwrap — effect-fn error propagation) bound to a let/var was a deferred
        // `Const`/`Alloc{Opaque}` = a SILENT MISCOMPILE (`int.parse(s)!` bound 0, `g()!` empty).
        // The faithful lowering needs early-return-on-Err (a later brick); until then WALL it —
        // NEVER bind a silently-wrong value (the ② cardinal rule). Both scalar + heap paths.
        if matches!(&value.kind, IrExprKind::Unwrap { .. }) {
            return Err(LowerError::Unsupported(
                "unwrap `!` bound to a let/var cannot be faithfully computed (needs early-return \
                 propagation; a Const/Opaque would be a silently wrong value) not in this brick"
                    .into(),
            ));
        }
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
            // the scalar-value foundation; other scalars stay the deferred `Const`. A FLOAT
            // literal carries its f64 BITS the same way (the float-floor render reinterprets).
            // A BOOL literal materializes to ConstInt 0/1 (else `var b=true` stays a deferred
            // Const 0, and `if b` / `match b { true=>.. }` always takes the false arm).
            if let IrExprKind::LitInt { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::LitBool { .. } = &value.kind
            {
                if let Some(dst) = self.lower_scalar_value(value) {
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
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
                // A VARIANT (Option/Result) subject — execute the tag-read value-match
                // (only the taken arm runs, the scalar payload bound). A ctor pattern is not
                // `subj == lit`, so it can't reach `desugar_match_to_if`; without this the
                // result stayed an unset deferred Const (a silent 0).
                if is_variant_ty(&subject.ty) {
                    if let Some(dst) = self.try_lower_variant_value_match(subject, arms, ty) {
                        self.value_of.insert(var, dst);
                        return Ok(());
                    }
                    // Outside the executable subset a Const-0 would silently pick a wrong
                    // arm — WALL (the discipline: an unfaithful variant match rejects, never
                    // emits a deferred 0).
                    return Err(LowerError::Unsupported(
                        "variant (Option/Result) match bound to a let/var outside the \
                         executable subset cannot be faithfully computed (a Const-0 would \
                         silently pick a wrong arm) not in this brick"
                            .into(),
                    ));
                }
                if let Some(if_expr) = self.desugar_match_to_if(subject, arms, ty) {
                    // `If` (literal arms) OR `Block` (`{ let x = subj; if … }` for a
                    // binder/guarded arm) — `lower_scalar_arm` runs both; roll back on a miss.
                    let mark = self.ops.len();
                    let lhh = self.live_heap_handles.len();
                    if let Some(dst) = self.lower_scalar_arm(&if_expr) {
                        self.value_of.insert(var, dst);
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh);
                }
            }
            // `let idx = string.index_of(s, x) ?? -1` — a `??` over a materialized Option
            // EXECUTES to a scalar (tag read + payload/fallback), unwrapping the self-host
            // Option[Int] fns; outside the subset a `??` over a VARIANT operand can't read
            // the tag (e.g. a USER-function Option/Result result not yet tracked as
            // materialized) — a Const-0 would be a wrong value, so WALL (never silently 0).
            if let IrExprKind::UnwrapOr { expr, fallback } = &value.kind {
                if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, true) {
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
                if is_variant_ty(&expr.ty) {
                    return Err(LowerError::Unsupported(
                        "?? over an Option/Result operand outside the executable subset (e.g. a \
                         user-function result not tracked as materialized) cannot be faithfully \
                         computed (a Const-0 would be a wrong value) not in this brick"
                            .into(),
                    ));
                }
            }
            // `let v = w` aliasing a SCALAR var — v denotes the SAME value (a scalar is freely
            // duplicable: no copy, no ownership). Without this, a bare-Var scalar RHS fell to the
            // deferred `Const` below and silently became 0 (the param-alias zeroing trap).
            if let IrExprKind::Var { id } = &value.kind {
                if let Ok(src) = self.value_for(*id) {
                    self.value_of.insert(var, src);
                    return Ok(());
                }
            }
            // `let d = r.x` / `let d = t.0` / `let d = xs[i]` — a SCALAR field / element
            // projection LOADS the real value from the materialized aggregate's layout slot
            // (the VALUE MODEL); `xs[i]` is a bounds-checked `$elem_addr` load. Outside the
            // materialized subset it rolls back to the deferred `Const`.
            if let IrExprKind::Member { .. }
            | IrExprKind::TupleIndex { .. }
            | IrExprKind::IndexAccess { .. } = &value.kind
            {
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(value) {
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
                self.ops.truncate(mark);
            }
            let dst = self.fresh_value();
            self.value_of.insert(var, dst);
            self.ops.push(Op::Const { dst });
            self.record_elided_calls(value);
            return Ok(());
        }
        // `let s = opt ?? "default"` — a HEAP-String `??` over a materialized Option[String]
        // EXECUTES via the self-host `option.unwrap_or_str` CALL (try_lower_option_unwrap_or's heap
        // branch): a fresh owned String, bound + dropped like any heap value. This CLOSES the
        // silent-empty `Alloc{Opaque}` hole the deferred arm below leaves for heap `??` (the
        // `list.get(xs,i) ?? "d"` / `json.as_string(v) ?? "d"` miscompile). Outside the subset
        // (a non-String heap payload, a non-materialized operand) it falls through to the deferred
        // `Alloc{Opaque}` arm below — unchanged, the existing memory-safe incompleteness.
        if let IrExprKind::UnwrapOr { expr, fallback } = &value.kind {
            if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, true) {
                self.value_of.insert(var, dst);
                return Ok(());
            }
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
            // A `let f = (params) => body` lambda. A NON-CAPTURING one LIFTS to a fresh
            // top-level function bound via `Op::FuncRef` (a scalar table slot) — so a later
            // `f(args)` lowers to `Op::CallIndirect` and the closure EXECUTES. A CAPTURING
            // lambda (its body references an enclosing local) needs an environment the
            // proven model has no representation for, so it falls through to the deferred
            // `Alloc{Opaque}` (its calls elided ⇒ honest caps taint), unchanged.
            IrExprKind::Lambda { params, body, .. } => {
                // C1 DIRECT-CALL INLINE: record the lambda (params + body) so a later DIRECT
                // call `f(args)` to this `var` is DEFUNCTIONALIZED (the body inlined with the
                // params bound to the args, captures resolved through `value_of`). Recorded
                // for BOTH the liftable and the capturing case — the call site prefers inline.
                self.lambda_bindings.insert(var, (params.clone(), (**body).clone()));
                if let Some(dst) = self.lift_lambda(params, body) {
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
                // A CAPTURING / non-liftable lambda — NO `Op::FuncRef` slot exists, but the
                // direct-call inline above can still EXECUTE a `f(args)`. Bind a placeholder
                // value so `f` is in `value_of` (a lone `f` never invoked carries no
                // observable, and a captured-`f`-passed-to-a-HOF is the C2 first-class case
                // that WALLS at that HOF). The deferred Opaque keeps the value memory-safe.
                let dst = self.fresh_value();
                let repr = repr_of(ty)?;
                let init = alloc_init(value);
                self.value_of.insert(var, dst);
                self.ops.push(Op::Alloc { dst, repr, init });
                self.live_heap_handles.push(dst);
                self.record_elided_calls(value);
                Ok(())
            }
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
            // (A NON-CAPTURING `Lambda` is intercepted ABOVE and LIFTED to a FuncRef; only
            // a capturing one — a real environment — reaches this deferred Opaque arm.)
            | IrExprKind::ClosureCreate { .. }
            | IrExprKind::Range { .. }
            // A RUNTIME CALL result is a fresh value (its call is elided ⇒ the gate
            // taints the function honestly, like Method/Computed).
            | IrExprKind::RuntimeCall { .. } => {
                // `let s = a + b` — a string concat EXECUTES to a fresh owned String (via the
                // self-host __str_concat), held by the binding and dropped at scope end.
                if let Some(dst) = self.try_lower_concat_str(value) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
                // `let ys = xs + [7]` — a SCALAR-element list concat EXECUTES to a fresh owned list
                // (via the self-host __list_concat), held by the binding and dropped at scope end.
                // The result is a REAL, POPULATED block (len(a)+len(b) copied slots), so a later
                // `ys[i]` may index it directly. (A heap-element list concat returns None and falls
                // through to the deferred Opaque.)
                if let Some(dst) = self.try_lower_concat_list(value) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    self.materialized_lists.insert(dst);
                    return Ok(());
                }
                // `let s = "x=${n} y=${t}"` — a STRING INTERPOLATION over the executable
                // subset (Lit / String Var/LitStr / Int Var/LitInt parts) EXECUTES to a
                // fresh owned String via the same __str_concat chain, byte-matching v0;
                // held by the binding and dropped at scope end. An interp with a compound
                // (`${list}`) or call (`${f()}`) operand falls through to the deferred
                // `Alloc{Opaque}` below, unchanged.
                if let IrExprKind::StringInterp { parts } = &value.kind {
                    if let Some(dst) = self.try_lower_string_interp(parts) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                }
                // `let xs = ["a" + "b", "c"]` — a List[String] literal with fresh-owned elements
                // (the heap-container-element concat position; the −214 caps recovery).
                if let Some(dst) = self.try_lower_str_list_literal(value) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
                // An Option ctor in the executable subset (`Some(scalar)` / `None`) is
                // MATERIALIZED + tracked so a later `match` over the bound var executes;
                // everything else is the deferred fresh `Alloc` (value-semantics).
                if let Some(dst) = self.try_lower_option_ctor(value, ty) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
                // A scalar-field tuple `(a, b)` of NON-LITERAL fields (vars / scalar exprs) — a
                // literal `(3, 7)` is already an `Init::IntList` below. Construct the 2-slot block
                // and store each field's computed value (the tuple-machinery construction sibling
                // of the precise destructure). A heap-field tuple falls through to the Opaque alloc.
                if let IrExprKind::Tuple { elements } = &value.kind {
                    if let Some(dst) = self.try_lower_scalar_tuple_construct(elements) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    // A HEAP-element tuple (`(1, "a")`, `(p, 9)`) — materialize the mixed block
                    // + track its heap-slot mask, so `t.0`/`${tuple}` execute and the block (with
                    // its owned heap elements) is reclaimed by a masked recursive drop. Rolls back
                    // on a non-lowerable element (then Opaque → the Display walls).
                    let mark = self.ops.len();
                    let lhh_mark = self.live_heap_handles.len();
                    if let Some(dst) = self.try_lower_tuple_construct(elements) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh_mark);
                }
                // A SCALAR-only record `R { x: 3, y: 4 }` — build the tight-packed,
                // width-aware block + store each field at its layout slot (the VALUE
                // MODEL: `r.x`/`r.y` read back exactly what was stored). A HEAP-field
                // record (a String/List field) needs an ownership-aware recursive drop
                // this brick does not have, so it falls through to the deferred Opaque
                // (which the field-access path then WALLS rather than mis-reads).
                if let IrExprKind::Record { .. } = &value.kind {
                    if let Some(dst) = self.try_lower_scalar_record_construct(value) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    // A record with one or more HEAP fields (`R { name: "x", n: i }`) —
                    // materialize the mixed scalar+heap block + track its heap-slot mask, so a
                    // `r.n` scalar read AND a `r.name` heap read execute and the block (with its
                    // owned heap fields) is reclaimed by a masked recursive drop. Rolls back on
                    // a partially-lowered out-of-subset field (a heap-returning-call field).
                    let mark = self.ops.len();
                    let lhh_mark = self.live_heap_handles.len();
                    if let Some(dst) = self.try_lower_record_construct(value) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh_mark);
                }
                // A SPREAD record `R { ...base, f: override }` — build a fresh block of the
                // same layout, COPYING each non-overridden field from `base` (a scalar load,
                // a heap-handle Dup so both records own a distinct reference) and storing the
                // overrides. So `let b2 = Box { ...b, value: 8 }` reads `b2.value=8
                // b2.label=old` while `b.label` still reads `old`. Rolls back to the deferred
                // Opaque (whose field reads WALL) on a non-materialized base / out-of-subset
                // override — never wrong bytes.
                if let IrExprKind::SpreadRecord { .. } = &value.kind {
                    let mark = self.ops.len();
                    let lhh_mark = self.live_heap_handles.len();
                    if let Some(dst) = self.try_lower_spread_record_construct(value) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh_mark);
                }
                // A scalar `List[Int/Float/Bool]` literal with COMPUTED elements (`[1.0, inf, 0.5]`,
                // `[a, a]`) — build the block + store each slot (an all-literal list is the IntList
                // path in `alloc_init` below; a computed element can't fold to a constant).
                if let Some(dst) = self.try_lower_scalar_list_construct(value) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
                // A NON-EMPTY `List[heap]` LITERAL that NONE of the materialization paths above
                // could build — a list of heap-FIELD records/tuples (`[R{name:String,…}, …]`), a
                // list of lists, a list of heap call-results. The flat `Init::Opaque` fallback
                // below would emit an EMPTY len-0 block (`list_new(0, …)`); a later `list.map` /
                // `list.sort_by` / `xs[i]` over it then silently reads NOTHING = wrong/empty bytes.
                // (A heap-field-record element needs a TWO-LEVEL recursive drop — the list frees
                // each record, each record frees its String fields — which the single-level
                // `DropListStr` cannot express without a new ownership op; that is the
                // nested-ownership frontier, out of this brick.) WALL the function cleanly instead
                // of mis-valuing it — the render discards it (no invalid wasm, no empty output).
                // GATED to a NON-EMPTY heap-element `List` LITERAL (an empty `[]`, a scalar list,
                // and a `List[String]`/scalar-aggregate list are all handled above), so this only
                // rejects the genuinely-unmaterializable case.
                if let IrExprKind::List { elements } = &value.kind {
                    use almide_lang::types::constructor::TypeConstructorId;
                    let heap_elem_list = matches!(ty,
                        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_heap_ty(&a[0]));
                    if heap_elem_list && !elements.is_empty() {
                        return Err(LowerError::Unsupported(
                            "non-empty List[heap] literal with nested-ownership elements \
                             (a heap-field record/tuple, a list, a call result) cannot be \
                             faithfully materialized in this brick (walled, not emitted empty)"
                                .into(),
                        ));
                    }
                }
                let dst = self.fresh_value();
                let repr = repr_of(ty)?;
                let init = alloc_init(value);
                // An all-literal `Init::IntList` is a REAL, POPULATED block (every slot a constant) —
                // admit a direct `xs[i]` bounds-checked load over it. An `Init::Opaque` (a deferred /
                // unsupported value) is NOT tracked: indexing it would trap on cap 0.
                let real_list = matches!(init, Init::IntList(_));
                self.value_of.insert(var, dst);
                self.ops.push(Op::Alloc { dst, repr, init });
                self.live_heap_handles.push(dst);
                if real_list {
                    self.materialized_lists.insert(dst);
                }
                self.record_elided_calls(value);
                Ok(())
            }
            // `var v = r.x` / `xs[i]` — a HEAP extraction: alias the container
            // (`Op::Dup`), bound here and dropped at scope end (cert `a` + `d`). When
            // the container is NOT a tracked var (`f().x`, nested `a.b.c`), there is no
            // single `src` to `Dup`; the deferred Opaque EMPTY value the binding would
            // hold is observed by any later read of `v` = a SILENT MISCOMPILE, so a failed
            // extraction rejects here.
            IrExprKind::Member { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::TupleIndex { .. } => {
                let dst = self.lower_heap_extraction(value)?;
                self.value_of.insert(var, dst);
                // A precise heap-field BORROW (a `LoadHandle` of a slot in a still-owning
                // container) is in `param_values` — it is NOT a second owner, so it must NOT
                // join the scope-end drop set (the container's masked drop frees the field).
                if !self.param_values.contains(&dst) {
                    self.live_heap_handles.push(dst);
                }
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
                if is_heap_elem_list_ty(ty) {
                    self.heap_elem_lists.insert(dst);
                }
                // A user function returning Option/Result yields a REAL same-layout variant block
                // (the v1 calling convention — `seed_variant_param`'s contract). SEED its READ-shape
                // so a later `match x { … }` / `x ?? d` over the LET-BOUND var EXECUTES (reads the
                // tag) exactly as the direct-call-arg position already does (`lower_call_args`'s
                // Named arm). Adds ONLY layout knowledge — `dst` is already an owned heap value
                // dropped at scope end, so no ownership/cert change. This is what made
                // `let parsed = parse_oct(d); match parsed { … }` (num_signed_base, after the
                // let-bound-heap-`if` tail-duplication) lower instead of wall.
                if is_variant_ty(ty) {
                    self.seed_variant_param(dst, ty);
                }
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
                // A SCALAR-element `List[Int/Float/Bool]` result from a self-host list call is a REAL,
                // POPULATED block — admit a direct `xs[i]` — ONLY when the call is FAITHFULLY executable:
                //  (1) every closure arg LIFTED (an unlifted `list.map(fns, (f) => f(10))` runs the
                //      combinator with a missing slot → empty/garbage), AND
                //  (2) no DATA argument carries a function type (`list.map(fns, …)` over `fns:
                //      List[(Int)->Int]` — a list of closures the v1 model cannot represent →
                //      empty/garbage). The combinator's OWN closure arg (a `Lambda`/`FnRef`, function-
                //      typed by construction) is EXCLUDED — it is handled by (1), and `(p) => p.x` over
                //      `points: List[Point]` is the faithful case that must stay tracked.
                // Otherwise the result is unmaterialized and a `xs[i]` over it would TRAP on cap 0, so
                // it is left deferring to `Const 0` (mis-valued, never a new runtime crash).
                let data_arg_has_fn = args.iter().any(|a| {
                    let is_closure_arg = matches!(
                        &a.kind,
                        IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. } | IrExprKind::ClosureCreate { .. }
                    );
                    !is_closure_arg && crate::lower::ty_contains_fn(&a.ty)
                });
                let faithful = !self.last_call_had_unlifted_closure && !data_arg_has_fn;
                // WALL the UNFAITHFUL higher-order combinator instead of silently
                // mis-valuing it. A HOF call (`list.map`/`filter`/`fold`…) over a
                // CAPTURING/param-invoking lambda (no liftable slot) or a fn-typed DATA
                // arg (`list.map(fns, (f) => f(10))` over `fns: List[(Int)->Int]` — a
                // list of closures the v1 model cannot represent) runs the self-host
                // combinator with a missing/garbage closure and produces a zero-filled
                // result. Leaving the result deferred (a `Const 0` `xs[i]`) emits WRONG
                // BYTES — a silent miscompile. Walling the whole function here is the
                // honest outcome (render discards it cleanly; no invalid wasm, no wrong
                // output). The FAITHFUL case (every closure lifted, no fn-typed data —
                // `list.map(xs, (x) => x + 1)`, `(p) => p.x` over `List[Point]`) is
                // UNTOUCHED, so the in-scope HOF byte-matches stay materialized.
                if crate::lower::is_higher_order(args) && !faithful {
                    return Err(LowerError::Unsupported(format!(
                        "{}.{} with an unliftable/closure-list higher-order argument \
                         cannot execute faithfully in this brick (walled, not mis-valued)",
                        module.as_str(),
                        func.as_str()
                    )));
                }
                if is_scalar_elem_list_ty(ty) && faithful {
                    self.materialized_lists.insert(dst);
                }
                // A BORROW result (`prim.load_str` of a list slot — the list still owns it) is NOT
                // added to the scope-end drop set; everything else is a fresh owned value.
                if !self.param_values.contains(&dst) {
                    self.live_heap_handles.push(dst);
                }
                // A self-host Option fn (`list.get`) returns a real materialized Option —
                // track the bound result so a later `match` over the var EXECUTES.
                if is_self_host_option_module_fn(module.as_str(), func.as_str()) {
                    self.materialized_options.insert(dst);
                }
                // A self-host Result fn (`int.parse`) returns a real materialized Result — track it
                // so a later `match r { Ok(v) => …, Err(e) => … }` over the var EXECUTES.
                if is_self_host_result_module_fn(module.as_str(), func.as_str()) {
                    self.materialized_results.insert(dst);
                }
                // A self-host HEAP-Ok Result fn (`value.as_string` → Result[String, String]) — track
                // it in the cap-as-tag set so a `match` reads cap@8 + binds slot-0 String.
                if crate::lower::is_self_host_result_str_module_fn(module.as_str(), func.as_str()) {
                    self.materialized_results_str.insert(dst);
                }
                // A `List[String]` result (string.split / a List[String] combinator) is a
                // nested-ownership list — its scope-end drop must recursively free elements.
                if is_heap_elem_list_ty(ty) {
                    self.heap_elem_lists.insert(dst);
                }
                // A `Value` result (value.str/int/… or a Value-returning combinator) drops via the
                // runtime-tag-dispatched DropValue (a heap-payload Value owns one handle).
                if crate::lower::is_value_ty(ty) {
                    self.value_handles.insert(dst);
                }
                Ok(())
            }
            // `var o = f(x)` where `f` is a lifted lambda / function-typed param returning a
            // HEAP value (`(Int) -> Option[Int]` / `-> List[Int]`): EXECUTE the closure via a
            // heap-result `Op::CallIndirect`. The result is a FRESH OWNED value (the closure
            // moves it out — cert `i`, dropped at scope end — the foundation for filter_map /
            // flat_map). A Computed callee that is NOT a known funcref falls through to the
            // deferred Opaque below.
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. }
                if self.funcref_value_of(callee).is_some() =>
            {
                let table_idx = self.funcref_value_of(callee).unwrap();
                let repr = repr_of(ty)?;
                let lowered = self.lower_call_args(args)?;
                let dst = self.fresh_value();
                self.ops.push(Op::CallIndirect {
                    dst: Some(dst),
                    table_idx,
                    args: lowered,
                    result: Some(repr),
                });
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                Ok(())
            }
            // `var x = obj.method(args)` / `var x = (g)(args)` — an UNRESOLVABLE
            // `Method`/`Computed` callee bound to a heap var. The deferred Opaque EMPTY
            // value the binding would hold is observed by any later read of `x` = a SILENT
            // MISCOMPILE, so reject explicitly.
            IrExprKind::Call { .. } => {
                Err(LowerError::Unsupported(
                    "heap-result method/computed call bound to a var cannot be faithfully \
                     computed in this brick (would bind an empty deferred heap value)"
                        .into(),
                ))
            }
            // `let s = if c then "A" else "B"; …` / `let x = match … { … }` — a heap-result
            // branch in a NON-TAIL, let-bound position. There is NO faithful executable
            // encoding here: a tail heap-result `if` moves each arm's value OUT (the
            // per-arm `"im"` balance), but a LET-BOUND value is held and dropped at scope
            // end — a trailing `Drop` of the merged `IfThen` dst would release a moved-out
            // object (the checker REJECTS the resulting `im·im·d` — accept⟹safe violated),
            // and attributing ONE scope-end drop to exactly-one-of-two arm allocs needs a
            // checker/Coq change (out of scope). The OLD fallback bound `x` to a deferred
            // `Init::Opaque` — an EMPTY heap value — so `println(s)` printed EMPTY instead
            // of "A"/"B": a SILENT MISCOMPILE. Reject explicitly so the function walls
            // cleanly instead of emitting wrong bytes.
            IrExprKind::If { .. } | IrExprKind::Match { .. } => {
                Err(LowerError::Unsupported(
                    "heap-result `if`/`match` bound to a let/var cannot be faithfully \
                     computed in this brick (would bind an empty deferred heap value); \
                     the merged result has no sound scope-end drop in the flat certificate"
                        .into(),
                ))
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
        // PRECISE tuple field extraction (the layout brick): a tuple value is a block
        // [rc][len][cap][f0@12, f1@20, ...]; a destructure (`let (a, b) = t`) loads each field at
        // its OWN slot instead of the container-grain alias. A SCALAR field is a value COPY; a HEAP
        // field (`let (inner, z) = n` over `((Int,Int), Int)`) is the BORROWED slot handle (the
        // tuple keeps ownership through its masked scope-end drop). Without this, `bind_pattern`
        // aliased the WHOLE container for a heap field and emitted `Const 0` for a scalar field
        // alongside it = the `8192:2000:0` miscompile.
        if let IrPattern::Tuple { elements } = pattern {
            if let Some(subj) = subject {
                // A CALL-RESULT tuple (`let (v, n) = dispatch(..)`) is a real OWNED block the
                // callee built (the `lower_tail` Tuple materialize) but `materialized_call_arg`
                // tracked it only flatly (a plain Drop would LEAK its heap slot, and it is not a
                // `materialized_aggregate` so the precise destructure below bails to the `Const 0`
                // container-alias garbage). SEED it as a masked aggregate: record the heap-slot
                // mask (so the scope-end drop is the recursive `DropListStr` that frees the owned
                // String/Value slot) + mark it `materialized_aggregates` (so per-slot borrow reads
                // execute). Only for an owned, still-live result (in `live_heap_handles`) — a
                // borrowed param/var already carries its own tracking.
                if !self.materialized_aggregates.contains(&subj)
                    && self.live_heap_handles.contains(&subj)
                {
                    if let Ty::Tuple(tys) = &value.ty {
                        let heap_slots: Vec<usize> =
                            (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
                        if !heap_slots.is_empty() {
                            self.record_masks.insert(subj, heap_slots);
                            self.materialized_aggregates.insert(subj);
                        }
                    }
                }
                if self.try_lower_tuple_destructure(elements, subj) {
                    return Ok(());
                }
            }
        }
        self.bind_pattern(pattern, subject)
    }

    /// Construct a SCALAR-field tuple `(a, b, …)`: alloc an n-slot block (Init::DynList) and store
    /// each field's computed scalar value at its slot via `Prim::Store`. Returns `None` (caller
    /// falls back to the Opaque alloc) if any field is heap or not a lowerable scalar.
    /// A scalar `List[Int]`/`List[Float]`/`List[Bool]` LITERAL with NON-literal elements (`[1.0, inf,
    /// 0.5]`, `[a, a]`, `[f(x), g(y)]`) — an all-literal list is already an `Init::IntList`, but a
    /// computed element can't be folded to a constant, so build the block explicitly: alloc `n` i64
    /// slots and `store64` each element's lowered scalar value (a Float's f64 bits, an Int's value).
    /// Scalar elements own no heap, so a flat `DynList` (drops as a flat block) is correct — no nested
    /// ownership. Returns None (defer to the Opaque alloc) if any element is heap or non-scalar-
    /// lowerable. The list-shaped sibling of [`Self::try_lower_scalar_tuple_construct`].
    pub(crate) fn try_lower_scalar_list_construct(&mut self, value: &IrExpr) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::List { elements } = &value.kind else {
            return None;
        };
        // Only SCALAR-element lists (List[Int]/Float/Bool). A heap-element list is the str path above.
        let scalar_list = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]));
        if !scalar_list || elements.is_empty() {
            return None;
        }
        self.try_lower_scalar_list_slots(elements)
    }

    pub(crate) fn try_lower_scalar_tuple_construct(&mut self, elements: &[IrExpr]) -> Option<ValueId> {
        if elements.iter().any(|e| is_heap_ty(&e.ty)) {
            return None; // heap-element tuple → the masked `try_lower_tuple_construct` path.
        }
        let dst = self.try_lower_scalar_list_slots(elements)?;
        // A scalar tuple is built with the uniform slot layout, so `t.0` / a `${tuple}` Display
        // reads its real slots. No heap slots → only the SAFE scalar reads are enabled.
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// Materialize a scalar-only tuple LITERAL element of a `List[(scalar, …)]` (`(1, 100)` in
    /// `[(1, 100), (127, 300)]`). Takes the tuple `IrExpr`, builds the fresh OWNED flat block, and
    /// returns its handle for the list-slot store. `None` (the list defers) on a non-tuple or a
    /// heap-field tuple element. The element does NOT join `materialized_aggregates` (the FOR-loop
    /// var binding tracks its own per-iteration handle); it is just the owned slot value moved in.
    fn try_lower_scalar_tuple_construct_for_elem(&mut self, elem: &IrExpr) -> Option<ValueId> {
        let IrExprKind::Tuple { elements } = &elem.kind else {
            return None;
        };
        self.try_lower_scalar_tuple_construct(elements)
    }

    /// Construct a TUPLE `(e0, e1, …)` with one or more HEAP ELEMENTS (a String/List/nested
    /// aggregate alongside scalars) — the positional analogue of [`Self::try_lower_record_construct`].
    /// Same `[rc][len][cap]` + uniform-i64-slot block; each heap element is a fresh OWNED handle
    /// MOVED into its slot (cert `m`), tracked in `record_masks` so the drop frees exactly the heap
    /// slots then the block (a masked `DropListStr`, cert = the single `d`). Returns `None` (defer)
    /// for an element value not lowerable to an owned heap handle / scalar — then the tuple falls
    /// back to Opaque and a `${tuple}` Display WALLS (never wrong bytes). SOUND by the SAME argument
    /// as the record path (each heap element `i…m`, the block `i…d` — the balanced List[String] shape).
    pub(crate) fn try_lower_tuple_construct(&mut self, elements: &[IrExpr]) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        if elements.is_empty() {
            return None;
        }
        let n = elements.len();
        let heap_slots: Vec<usize> =
            (0..n).filter(|&i| is_heap_ty(&elements[i].ty)).collect();
        if heap_slots.is_empty() {
            return None; // all-scalar → `try_lower_scalar_tuple_construct` owns it.
        }
        // Lower every element first (before the alloc), as (slot-value, is-heap).
        let mut slots: Vec<(ValueId, bool)> = Vec::with_capacity(n);
        for e in elements {
            if is_heap_ty(&e.ty) {
                let obj = self.lower_owned_heap_field(e)?;
                slots.push((obj, true));
            } else {
                let v = self.lower_scalar_value(e)?;
                slots.push((v, false));
            }
        }
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: n as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        for (idx, (v, is_heap)) in slots.into_iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(idx) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            let store_val = if is_heap {
                let handle = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![v] });
                handle
            } else {
                v
            };
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, store_val],
            });
            if is_heap {
                self.ops.push(Op::Consume { v });
                self.live_heap_handles.retain(|x| *x != v);
            }
        }
        self.record_masks.insert(dst, heap_slots);
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// Construct a SCALAR-only RECORD `R { f0: e0, f1: e1, … }`: alloc a block laid out
    /// by [`Self::aggregate_field_tys`] + [`layout::field_slots`] (per-field TIGHT-PACKED
    /// at width-aware offsets after the `[rc][len][cap]` header) and `Prim::Store` each
    /// field's computed scalar at its own (offset, width). Unlike
    /// [`Self::try_lower_scalar_list_slots`] (uniform 8-byte slots), this honours each
    /// field's DECLARED width (Int8 → width 1, Bool/Int32 → 4, Int/Float → 8), so a
    /// `{ b: Int8, n: Int }` round-trips through `r.b`/`r.n` byte-exactly.
    ///
    /// The field order + concrete widths come from the record's TYPE (resolved via the
    /// layout registry, substituting generic params with the instantiated args — so a
    /// `Box[Int]` field `value: T` is sized as `Int`, the #650 concern), NOT the literal's
    /// field order: construction and `r.x` projection consult the SAME declaration-ordered
    /// slot list, so they cannot desync even if the literal lists fields out of order.
    ///
    /// Returns `None` (defer/wall) for a non-record / unresolvable type, a HEAP field
    /// (needs an ownership-aware recursive drop — out of this value-model brick), an
    /// unsupported scalar width, or a field whose value is not a lowerable scalar.
    ///
    /// SOUNDNESS: a scalar-only record owns NO nested heap, so the block is a FLAT
    /// `DynList` — its scope-end drop is the ordinary single `Drop` (cert `i`+`d`), no
    /// new ownership op or certificate event. The fields are pure `Prim::Store`s (no
    /// ownership), exactly like the scalar-tuple / IntList path: one i64 slot per field,
    /// `12 + idx*8`, `store64`. A narrow Int8 value round-trips losslessly through its
    /// i64 slot, so a uniform slot is correct for the observable output.
    pub(crate) fn try_lower_scalar_record_construct(
        &mut self,
        value: &IrExpr,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        // Only an explicit `Record` literal reaches here (a `SpreadRecord` defers).
        let IrExprKind::Record { fields, .. } = &value.kind else {
            return None;
        };
        // The CANONICAL declaration-ordered (name, concrete-type) field list. A heap
        // field / unresolvable type ⇒ `None` (via `scalar_slots`) ⇒ wall.
        let (names, tys) = self.aggregate_field_tys(&value.ty)?;
        let n = layout::scalar_slots(&tys)?;
        if names.len() != n {
            return None;
        }
        // Lower every supplied field value FIRST (before the alloc) so a field expr that
        // itself allocates does not interleave with our store sequence. Map each literal
        // field to its DECLARED index (the literal may list fields out of declaration
        // order — the slot offset follows the declaration, not the literal). A record may
        // OMIT a field (default) — the fresh block's slot stays zero, never garbage.
        let mut field_vals: Vec<(usize, ValueId)> = Vec::with_capacity(fields.len());
        for (name, expr) in fields {
            let idx = names.iter().position(|n| n == name)?;
            // A field whose VALUE is heap is out of the scalar value-model — wall the
            // whole record (never a partial wrong-bytes block).
            if is_heap_ty(&expr.ty) {
                return None;
            }
            let v = self.lower_scalar_value(expr)?;
            field_vals.push((idx, v));
        }
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: n as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        for (idx, v) in field_vals {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(idx) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, v],
            });
        }
        // Built with the uniform slot layout, so a `${record}` Display (and a heap-field
        // borrow, were a later field heap) may read its real slots. A scalar-only record has
        // no heap slots, so this only enables the SAFE field reads — never a garbage deref.
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// Construct a record/tuple with one or more HEAP FIELDS (a `String`/`List`/nested
    /// aggregate field alongside scalar fields) — `R { name: "x", n: i }`. The block is the
    /// SAME `[rc][len][cap]` + uniform-i64-slot layout as the scalar path, but each HEAP
    /// field is a fresh OWNED handle MOVED into its slot (cert `m`), and the value is tracked
    /// in `record_masks` so its drop frees exactly the heap slots then the block (an
    /// [`Op::DropListStr`] with the per-value mask — cert = the SAME single `d`).
    ///
    /// SOUNDNESS (no new op / no certificate change): this is byte-identical to the
    /// `List[String]` machinery applied to a mixed slot set. A heap field's owned handle is
    /// `Consume`d into the slot (cert `m` — moved in, like `prim.store_str`), so each heap
    /// field is `i…m` (alloc/dup then move-in) and the BLOCK is `i…d` (alloc then the
    /// recursive `DropListStr`), exactly the balanced shape the proven checker already
    /// accepts for a list of Strings. A scalar field is a pure `Prim::Store` (no ownership).
    /// The recursive free at drop touches ONLY the heap slots (the mask) — a scalar slot is
    /// never `rc_dec`'d. Returns `None` (defer) for an unresolvable type, an omitted heap
    /// field (a defaulted heap slot would be a garbage handle the drop frees — unsound), or
    /// a field value not lowerable to an owned handle / scalar.
    pub(crate) fn try_lower_record_construct(&mut self, value: &IrExpr) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        let IrExprKind::Record { fields, .. } = &value.kind else {
            return None;
        };
        let (names, tys) = self.aggregate_field_tys(&value.ty)?;
        if tys.is_empty() {
            return None;
        }
        let n = tys.len();
        // Per-slot heap-ness from the SUPPLIED field's CONCRETE type (`expr.ty`), NOT the
        // declared field type — a generic field (`first: A` in `Pair[A,B]`) may leave the
        // DECLARED type an unresolved param that `is_heap_ty` would mis-classify as heap; the
        // literal's value carries the concrete instantiated type. `None` for an unsupplied
        // (defaulted) slot — its concrete heap-ness is unknown here.
        let mut field_heap: Vec<Option<bool>> = vec![None; n];
        for (name, expr) in fields {
            let idx = names.iter().position(|n| n == name)?;
            field_heap[idx] = Some(is_heap_ty(&expr.ty));
        }
        // A DEFAULTED (omitted) slot whose DECLARED type is concretely heap (or an unresolved
        // generic we can't prove scalar) would leave a zero handle the masked drop frees — so
        // WALL the whole record (never an unsound partial block). A scalar default (a 0 slot)
        // is fine. (An omitted scalar slot's `field_heap` stays `None` = treated non-heap.)
        for i in 0..n {
            if field_heap[i].is_none() && is_heap_ty(&tys[i]) {
                return None;
            }
        }
        let heap_slots: Vec<usize> =
            (0..n).filter(|&i| field_heap[i] == Some(true)).collect();
        if heap_slots.is_empty() {
            return None; // no heap field — `try_lower_scalar_record_construct` owns it.
        }
        // Lower each supplied field to (declared-index, slot-value, is-heap). Heap fields
        // become a fresh OWNED handle (the same kinds `try_lower_str_list_literal` admits);
        // scalar fields a plain value. All lowered BEFORE the alloc (a field expr that
        // itself allocates must not interleave with our store sequence).
        let mut slots: Vec<(usize, ValueId, bool)> = Vec::with_capacity(fields.len());
        for (name, expr) in fields {
            let idx = names.iter().position(|n| n == name)?;
            let is_heap = is_heap_ty(&expr.ty);
            if is_heap {
                let obj = self.lower_owned_heap_field(expr)?;
                slots.push((idx, obj, true));
            } else {
                let v = self.lower_scalar_value(expr)?;
                slots.push((idx, v, false));
            }
        }
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: n as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        for (idx, v, is_heap) in slots {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(idx) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            // A heap field stores its HANDLE (i64-widened) then is `Consume`d (moved in);
            // a scalar field stores its value directly.
            let store_val = if is_heap {
                let handle = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![v] });
                handle
            } else {
                v
            };
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, store_val],
            });
            if is_heap {
                self.ops.push(Op::Consume { v });
                self.live_heap_handles.retain(|x| *x != v);
            }
        }
        self.record_masks.insert(dst, heap_slots);
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// Construct a SPREAD record `R { ...base, f: override, … }`: a FRESH block of the
    /// SAME uniform-slot layout, where each declared field's slot is either the supplied
    /// OVERRIDE value or COPIED from `base`. The copy preserves value semantics — `base`
    /// is left fully intact (a scalar slot is a `Load` copy; a heap slot is a borrowed
    /// handle `Dup`'d so the new record owns a DISTINCT reference while `base` keeps its
    /// own). This is what makes `let b2 = Box { ...b, value: 8 }` print `b2.value=8
    /// b2.label=old` while `b.label` still reads `old` — both records own the same string
    /// content through independent reference counts.
    ///
    /// GATE: `base` must be a MATERIALIZED aggregate var (its slots are real — a deferred
    /// `Opaque` base would copy garbage), every declared field's CONCRETE type must be
    /// known (resolved from `base.ty`, which carries the instantiated generic args — the
    /// `Pair[Int,String]` concern), and every override value must lower to an owned-handle
    /// (heap) / scalar. Any miss → `None` (the binding falls back to the deferred Opaque,
    /// whose field reads then WALL — never wrong bytes).
    ///
    /// SOUNDNESS (no new op / no certificate change): identical to [`Self::try_lower_record_construct`]'s
    /// shape — the block is `i…d` (alloc then the masked `DropListStr`), each heap slot
    /// holds an OWNED handle that is `Consume`d (moved) into the slot (cert `m`). A copied
    /// heap field's owned handle comes from `Dup`-ing `base`'s borrowed slot handle (cert
    /// `a` then `m` = the balanced shape the checker already accepts for a List[String]
    /// element duplicated from another container). `base` is never consumed, so it remains
    /// the sole owner of its own slots (dropped once at its own scope end).
    pub(crate) fn try_lower_spread_record_construct(&mut self, value: &IrExpr) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        let IrExprKind::SpreadRecord { base, fields } = &value.kind else {
            return None;
        };
        // The CANONICAL declaration-ordered (name, concrete-type) field list. The result's
        // type carries the instantiated generic args, so a `Pair[Int,String]` field `first: A`
        // resolves to `Int`. An unresolvable type ⇒ `None` ⇒ wall.
        let (names, tys) = self.aggregate_field_tys(&value.ty)?;
        let n = tys.len();
        if n == 0 || names.len() != n {
            return None;
        }
        // The base must be a TRACKED, MATERIALIZED aggregate var — its slots are real, so a
        // copy reads the right value (a deferred Opaque base would copy garbage). Resolve its
        // block handle.
        let base_block = match &base.kind {
            IrExprKind::Var { id } if is_heap_ty(&base.ty) => {
                let src = self.value_or_global(*id).ok()?;
                if !self.materialized_aggregates.contains(&src) {
                    return None;
                }
                src
            }
            _ => return None,
        };
        // Per declared slot: the override expr (if the literal supplies it) or `None` (copy
        // from base). A field NOT in the declaration is a type error the checker rejects
        // upstream, so a supplied field always maps to a declared index.
        let mut overrides: Vec<Option<&IrExpr>> = vec![None; n];
        for (name, expr) in fields {
            let idx = names.iter().position(|nm| nm == name)?;
            overrides[idx] = Some(expr);
        }
        // The slot is heap iff the declared CONCRETE type is heap (the base's slot, and the
        // copy/override, follow that). A generic field already substituted to its concrete
        // type by `aggregate_field_tys`, so `is_heap_ty` is decisive here.
        let heap_slots: Vec<usize> = (0..n).filter(|&i| is_heap_ty(&tys[i])).collect();
        // Lower every OVERRIDE value FIRST (before the alloc) so an override expr that itself
        // allocates does not interleave with our store sequence. Copies read from `base` and
        // are emitted inline at store time (a pure Load / a Dup of a borrowed handle — neither
        // allocates a block that could interleave badly). Each entry: (slot-value, is-heap).
        // For a heap OVERRIDE the value is a fresh owned handle to Consume into the slot.
        let mut override_vals: Vec<Option<(ValueId, bool)>> = vec![None; n];
        for (i, ov) in overrides.iter().enumerate() {
            if let Some(expr) = ov {
                if is_heap_ty(&tys[i]) {
                    let obj = self.lower_owned_heap_field(expr)?;
                    override_vals[i] = Some((obj, true));
                } else {
                    let v = self.lower_scalar_value(expr)?;
                    override_vals[i] = Some((v, false));
                }
            }
        }
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: n as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        // The base's block handle, for the per-slot copy loads.
        let bh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(bh), args: vec![base_block] });
        for i in 0..n {
            let is_heap = is_heap_ty(&tys[i]);
            // The destination slot address.
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(i) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            // The value to store: an OVERRIDE's lowered value, or a COPY from base's slot.
            let (slot_val, consume_owned) = match override_vals[i].take() {
                Some((v, true)) => {
                    // A heap override: store its handle, then Consume the owned value (moved in).
                    let handle = self.fresh_value();
                    self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![v] });
                    (handle, Some(v))
                }
                Some((v, false)) => (v, None), // a scalar override: store directly.
                None => {
                    // Copy from base's slot at the same offset.
                    let baddr = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: baddr, op: IntOp::Add, a: bh, b: off });
                    if is_heap {
                        // BORROW base's slot handle, then Dup it: the new record owns a DISTINCT
                        // reference (cert `a`), so base's own slot stays valid and the new block's
                        // masked drop frees only its own reference (no double-free).
                        let borrowed = self.fresh_value();
                        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(borrowed), args: vec![baddr] });
                        let owned = self.fresh_value();
                        self.ops.push(Op::Dup { dst: owned, src: borrowed });
                        self.live_heap_handles.push(owned);
                        let handle = self.fresh_value();
                        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![owned] });
                        (handle, Some(owned))
                    } else {
                        // A scalar copy: a pure value Load (no ownership).
                        let v = self.fresh_value();
                        self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v), args: vec![baddr] });
                        (v, None)
                    }
                }
            };
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, slot_val],
            });
            if let Some(v) = consume_owned {
                self.ops.push(Op::Consume { v });
                self.live_heap_handles.retain(|x| *x != v);
            }
        }
        if !heap_slots.is_empty() {
            self.record_masks.insert(dst, heap_slots);
        }
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// Lower a record/tuple field EXPRESSION whose type is HEAP to a FRESH OWNED handle the
    /// aggregate will own (moved into its slot). The admitted kinds mirror
    /// [`Self::try_lower_str_list_literal`]'s element kinds:
    /// - a `LitStr` is a fresh `Alloc{Str}` (cert `i`);
    /// - a `BinOp::ConcatStr` is the self-host `__str_concat` CallFn (cert `i`);
    /// - a tracked heap `Var` gets its OWN reference via `Dup` (cert `a`) so the original
    ///   binding keeps its reference (no double-free) and the aggregate owns a distinct one.
    /// Any other kind (a heap-returning call, a member access, a nested record literal)
    /// defers — `None`. The returned handle is in `live_heap_handles`; the caller MUST
    /// `Consume` it (the move-in) and remove it from the live set.
    fn lower_owned_heap_field(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        match &expr.kind {
            IrExprKind::LitStr { value: s } => {
                let obj = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: obj,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str(s.clone()),
                });
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => {
                let obj = self.try_lower_concat_str(expr)?;
                // try_lower_concat_str returns a fresh owned String (a CallFn result); track it
                // so the caller's Consume + live-set removal balances it.
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            IrExprKind::Var { id } => {
                let src = *self.value_of.get(id)?;
                let dup = self.fresh_value();
                self.ops.push(Op::Dup { dst: dup, src });
                self.live_heap_handles.push(dup);
                Some(dup)
            }
            // A user-call element (`(parse_inline(after), pos + 1)` — the dominant yaml tuple shape):
            // the callee returns a FRESH owned heap value (CallFn result = cert `i`, rc 1), tracked
            // so the enclosing tuple's per-slot `Consume` (`m`) moves it into the slot — the tuple
            // then owns it (its masked recursive DropListStr frees it). Same `i`/`m` balance as the
            // Var element's Dup. A pure Module-call (`value.array(items)`) returns a fresh Value the
            // same way; an impure/HO callee errors → None → the tuple defers (sound Opaque).
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args).ok()?;
                let repr = repr_of(&expr.ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(obj),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let obj = self
                    .lower_pure_module_value_call(module.as_str(), func.as_str(), args, &expr.ty)
                    .ok()?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            // A `List[Int/Float/Bool]` LITERAL field (`{ items: [1, 2, 3] }`, `{ items: [] }`) —
            // materialize the scalar-element block (flat slots, no nested ownership) as a fresh
            // OWNED list. The aggregate owns it; its masked recursive drop `rc_dec`s the block
            // (sound: scalar elements need no per-element free). An EMPTY scalar list is a valid
            // 0-length block (so `{ items: [] }` materializes, not Opaque-with-garbage). A
            // heap-element list (`List[String]`/`List[Record]`) DEFERS (`None`) — its elements
            // need a per-element recursive free not wired through the single-level mask — so the
            // aggregate falls back to Opaque and the field-read path WALLS (never wrong bytes).
            IrExprKind::List { elements } => {
                use almide_lang::types::constructor::TypeConstructorId;
                let scalar_list = matches!(&expr.ty,
                    Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]));
                if !scalar_list {
                    return None;
                }
                let obj = self.try_lower_scalar_list_slots(elements)?;
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            // A NESTED RECORD LITERAL field (`Outer { p: Point { x: 1, y: 2 }, n: 5 }`) —
            // materialize the inner block as a fresh OWNED aggregate the outer owns. Its own
            // construction (scalar / mixed-heap) registers it in `materialized_aggregates`, so
            // the recursive `${outer}` Display reads the inner's real slots. The outer's masked
            // drop `rc_dec`s the inner block; if the INNER has heap fields of its OWN, those are
            // freed by the inner block's own mask — but the outer mask only `rc_dec`s the inner
            // BLOCK (one level), so a heap-IN-nested field would leak. To stay sound, admit a
            // nested aggregate ONLY when it is SCALAR-only (no nested heap to leak); a nested
            // aggregate with its own heap field defers (`None`) → the outer walls (never wrong
            // bytes, never a leak).
            IrExprKind::Record { .. } | IrExprKind::Tuple { .. } => {
                let scalar_only = self
                    .aggregate_field_tys(&expr.ty)
                    .is_some_and(|(_, tys)| tys.iter().all(|t| !is_heap_ty(t)));
                if !scalar_only {
                    return None;
                }
                let obj = match &expr.kind {
                    IrExprKind::Record { .. } => self.try_lower_scalar_record_construct(expr)?,
                    IrExprKind::Tuple { elements } => self.try_lower_scalar_tuple_construct(elements)?,
                    _ => return None,
                };
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            _ => None,
        }
    }

    /// Shared block-builder for a scalar tuple/list: lower each element to a scalar value, alloc a
    /// `DynList` of `n` i64 slots, `store64` each. Element ownership-free (scalars), flat drop.
    fn try_lower_scalar_list_slots(&mut self, elements: &[IrExpr]) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        if elements.iter().any(|e| is_heap_ty(&e.ty)) {
            return None;
        }
        // Lower each field's scalar value first (before the alloc, so a field expr that itself
        // allocates doesn't interleave with our store sequence).
        let vals: Vec<ValueId> = elements
            .iter()
            .map(|e| self.lower_scalar_value(e))
            .collect::<Option<Vec<_>>>()?;
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: elements.len() as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        for (i, v) in vals.into_iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, v] });
        }
        // A REAL, POPULATED scalar list block — admit a direct `xs[i]` bounds-checked load.
        self.materialized_lists.insert(dst);
        Some(dst)
    }

    /// Extract each field of a tuple `subject` (a heap block) into its bound var via a precise
    /// per-slot `Prim` read: a SCALAR field is a value COPY (`Load width 8`), a HEAP field is the
    /// BORROWED slot handle (`LoadHandle`, recorded in `param_values` — the tuple still OWNS the
    /// element, freed by its masked scope-end drop, so the bound var is NOT a second owner). A heap
    /// field is admitted ONLY when the subject is a TRACKED owning aggregate (`materialized_
    /// aggregates`, with a `record_masks` heap-slot mask) or a borrowed PARAM/element handle
    /// (`param_values` — the caller owns it): in both cases reading the slot is a borrow with a
    /// guaranteed single owner, never a leak/double-free. Otherwise (an untracked heap subject —
    /// no mask to free the borrowed inner block) it returns `false` and the caller falls back to
    /// the container-grain `bind_pattern` (still memory-safe, just imprecise) so we never emit a
    /// dangling borrow. Returns `false` for any non-`Bind`/`Wildcard` sub-pattern (a nested tuple
    /// pattern in ONE statement is deferred — sz4 splits it into two statements, which works).
    fn try_lower_tuple_destructure(&mut self, pats: &[IrPattern], subject: ValueId) -> bool {
        use crate::{IntOp, PrimKind};
        // Does the subject OWN its heap slots (a tracked masked aggregate) OR is it a borrow whose
        // owner is elsewhere (a param / a borrowed element handle)? Either way a per-slot HEAP read
        // is a sound borrow. An untracked owned heap subject would leak the borrowed inner block, so
        // a heap field over it must defer to the container-grain alias.
        let heap_borrow_ok =
            self.materialized_aggregates.contains(&subject) || self.param_values.contains(&subject);
        for p in pats {
            match p {
                IrPattern::Bind { ty, .. } if !is_heap_ty(ty) => {}
                IrPattern::Bind { .. } => {
                    if !heap_borrow_ok {
                        return false;
                    }
                }
                IrPattern::Wildcard => {}
                _ => return false,
            }
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subject] });
        for (i, p) in pats.iter().enumerate() {
            if let IrPattern::Bind { var, ty } = p {
                let off = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
                let addr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
                let v = self.fresh_value();
                if is_heap_ty(ty) {
                    // BORROW the slot's owned handle (an i32 Ptr). The tuple keeps ownership (its
                    // masked drop frees it), so the bound var joins `param_values` (not a second
                    // owner, not in the scope-end drop set). A nested tuple/record handle bound this
                    // way is itself a tracked aggregate iff the subject's mask owns it — record it so
                    // a FURTHER `(ix, iy) = inner` destructure of it can also borrow its heap slots.
                    self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v), args: vec![addr] });
                    self.param_values.insert(v);
                    if matches!(ty, Ty::Tuple(_)) || self.aggregate_field_tys(ty).is_some() {
                        self.materialized_aggregates.insert(v);
                    }
                } else {
                    self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v), args: vec![addr] });
                }
                self.value_of.insert(*var, v);
            }
        }
        true
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

    /// If `value` is an Option CONSTRUCTOR in the executable subset — `Some(scalar)`
    /// or `None` — lower it to a MATERIALIZED 0-or-1-element-list block and TRACK the
    /// resulting `dst` as a materialized Option, so a later variant `match` over it may
    /// EXECUTE (read `len` as the tag, extract `data[0]`). Returns the fresh OWNED heap
    /// handle `dst` (NOT pushed to `live_heap_handles` — the caller does its own
    /// position-specific bookkeeping). Returns `None` when `value` is not a tracked
    /// Option ctor (a heap-payload `Some`, whose payload is not a lowerable scalar,
    /// falls through here too): the caller then takes its normal deferred-`Opaque` path,
    /// and a `match` over THAT value stays soundly LINEARIZED (it is never in the set).
    ///
    /// `Some(x)` is `Init::OptSome` (len=1, `data[0]`=x); `None` is `Init::Opaque`
    /// (len=0) — the SAME render as today, only now its `dst` is tracked. The ownership
    /// cert is one `Alloc` = i either way (init-agnostic), so NO checker change.
    pub(crate) fn try_lower_option_ctor(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            // `Some(heap)` RETURNED / bound directly — a fresh OWNED message/element (a LitStr, a
            // Named-call result, or an OWNED `Var` in `live_heap_handles`, NOT a borrowed param)
            // materializes the 0-or-1-element DynListStr Option (the element MOVED in). Same cert as
            // the heap-result-`if` arm; the owned gate keeps a borrowed `Some(param)` deferred.
            IrExprKind::OptionSome { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        self.value_for(*id).ok()?
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                // materialize_opt_str_some tracks materialized_options + heap_elem_lists.
                Some(self.materialize_opt_str_some(piece, repr))
            }
            IrExprKind::OptionSome { expr } => {
                // SCALAR payload only — `lower_scalar_value` returns `None` for a heap
                // payload, which IS the gate (a heap `Some` aliases its element, a later
                // refinement; it falls through to the deferred `Opaque` path, untracked).
                let payload = self.lower_scalar_value(expr)?;
                let dst = self.fresh_value();
                let repr = repr_of(ty).ok()?;
                self.ops.push(Op::Alloc { dst, repr, init: Init::OptSome { payload } });
                self.materialized_options.insert(dst);
                Some(dst)
            }
            IrExprKind::OptionNone => {
                let dst = self.fresh_value();
                let repr = repr_of(ty).ok()?;
                // `None` is the 0-element Option, sized like `OptSome` (`Init::OptNone`) so the
                // free-list reuses a block between Some/None results; tracked as materialized.
                self.ops.push(Op::Alloc { dst, repr, init: Init::OptNone });
                self.materialized_options.insert(dst);
                Some(dst)
            }
            // A `Result[Int, String]` ctor RETURNED / bound directly (`fn f() = Ok(y)` / `… = Err(
            // msg)`) MATERIALIZES the DynListStr Result (len-as-tag: Ok = len 0 with the scalar in
            // slot 0, Err = len 1 owning the message), tracked so the caller can `match` it. Same
            // cert as the heap-result-`if` arms (reuses `materialize_result_ok` / the Some-string
            // builder) — no new Init. SCALAR Ok payload, heap (Var/LitStr/Named-call) Err payload.
            // HEAP-Ok `Result[String, String]` (`Ok(s)` with a heap payload, both arms heap) RETURNED
            // / bound directly — the 2-SLOT DynListStr (String @slot 0, Ok/Err tag @slot 1, len 1 so
            // `DropListStr` frees only the one String). Same cert as the Err-heap arm (one owned
            // String moved in). Owned-`Var` / LitStr / Named-call piece only (a borrowed param would
            // double-free), else the deferred Opaque.
            IrExprKind::ResultOk { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(ty) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        self.value_for(*id).ok()?
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                Some(self.materialize_result_str(piece, repr, false))
            }
            IrExprKind::ResultOk { expr } if !is_heap_ty(&expr.ty) => {
                let payload = self.lower_scalar_value(expr)?;
                let repr = repr_of(ty).ok()?;
                let dst = self.materialize_result_ok(payload, repr);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            // HEAP-Ok `Result[(Int,Int), String]` etc. — `Err(msg)` RETURNED / bound directly
            // (`fn __rzip_err(..) = Err(copy)`). The Err message goes into the SAME cap-as-tag 1-slot
            // DynListStr as the heap-Ok arm (payload @12, tag @16 = 1), so a `match` reading tag @16
            // sees Err. Without this it would fall to the len-as-tag arm below (a DIFFERENT layout the
            // heap-Ok match misreads). Owned-`Var` / LitStr / Named-call piece only.
            IrExprKind::ResultErr { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(ty) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        self.value_for(*id).ok()?
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                Some(self.materialize_result_str(piece, repr, true))
            }
            IrExprKind::ResultErr { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(ty).ok()?;
                // A FRESH owned message only — a LitStr alloc, a Named-call result, or an OWNED
                // `Var` (one in `live_heap_handles` — a freshly-built/closure-returned String, NOT
                // a BORROWED param). Consuming a borrow into the Err would move out a value the
                // caller still owns (a double-free the checker rejects), so a borrowed `Var` falls
                // through to the sound deferred `Opaque`.
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        self.value_for(*id).ok()?
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                let dst = self.materialize_opt_str_some(piece, repr);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            _ => None,
        }
    }
}
