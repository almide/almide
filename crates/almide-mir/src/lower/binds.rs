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
        // A `List[String]` OR a `List[Record]` whose every element is a SCALAR-only
        // record literal — both are NESTED-OWNERSHIP lists (i64 slots holding owned heap
        // handles, recursively freed via `DropListStr`'s per-slot `rc_dec`). A scalar
        // record is a flat block, so `rc_dec` frees it correctly (no String-specific
        // recursion). This is what makes `[Point{..}, Point{..}]` (then `list.map(…, p =>
        // p.x)`) materialize. A `List` of any OTHER heap element (a heap-field record, a
        // List, a call result) defers.
        let elem_str = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String));
        let elem_scalar_record = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && self.aggregate_field_tys(&a[0])
                    .and_then(|(_, tys)| layout::scalar_slots(&tys)).is_some());
        if (!elem_str && !elem_scalar_record) || elements.is_empty() {
            return None;
        }
        // Gate: every element must be a fresh-owned String (LitStr/ConcatStr) OR a tracked
        // heap Var (so we can Dup it) OR — for a record list — a scalar-only record LITERAL
        // we can materialize. A Var whose value isn't tracked here cannot be Dup'd → defer.
        let all_lowerable = elements.iter().all(|e| match &e.kind {
            IrExprKind::LitStr { .. } | IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => true,
            IrExprKind::Var { id } => self.value_of.contains_key(id),
            IrExprKind::Record { .. } => elem_scalar_record,
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
        self.heap_elem_lists.insert(list);
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
            if let IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } = &value.kind {
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
                if let Some(if_expr) = self.desugar_match_to_if(subject, arms, ty) {
                    if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                        if let Some(dst) = self.try_lower_scalar_if(cond, then, else_, ty) {
                            self.value_of.insert(var, dst);
                            return Ok(());
                        }
                    }
                }
            }
            // `let idx = string.index_of(s, x) ?? -1` — a `??` over a materialized Option
            // EXECUTES to a scalar (tag read + payload/fallback), unwrapping the self-host
            // Option[Int] fns; outside the subset it falls through to the deferred `Const`.
            if let IrExprKind::UnwrapOr { expr, fallback } = &value.kind {
                if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, true) {
                    self.value_of.insert(var, dst);
                    return Ok(());
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
            // `let d = r.x` / `let d = t.0` — a SCALAR field projection LOADS the real
            // value from the materialized aggregate's layout slot (the VALUE MODEL).
            // Outside the materialized subset it rolls back to the deferred `Const`.
            if let IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } = &value.kind {
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_field_access(value) {
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
                if let Some(dst) = self.lift_lambda(params, body) {
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
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
                // A scalar `List[Int/Float/Bool]` literal with COMPUTED elements (`[1.0, inf, 0.5]`,
                // `[a, a]`) — build the block + store each slot (an all-literal list is the IntList
                // path in `alloc_init` below; a computed element can't fold to a constant).
                if let Some(dst) = self.try_lower_scalar_list_construct(value) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
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
            // `var x = if c then … else …` / `let x = match … { … }` — a heap-result
            // branch in a NON-TAIL, let-bound position. LINEARIZE the arms (each per-arm
            // balanced, its value deferred), then bind `x` to ONE fresh `Alloc{Opaque}` —
            // the merged result slot. Memory-safe by construction (the arms balance; the
            // result is a clean fresh alloc dropped at scope end); which arm's value it
            // equals is functional, deferred like every `Opaque`. The same WALLS as
            // statement position still apply per arm.
            //
            // Unlike TAIL position (`try_lower_heap_result_if`), a let-bound heap-result
            // value is NOT moved out — it is bound and dropped at scope end. That single
            // scope-end `Drop` on the MERGED `IfThen` dst has NO sound encoding in the FLAT
            // ownership certificate: the per-arm `"im"` balance models each arm's value
            // being moved OUT (the tail-return transfer), so a trailing `Drop` of the dst
            // would release a moved-out object — the checker REJECTS the resulting
            // `im·im·d` (empirically verified: an executable let-bound heap-result if/match
            // makes corpus ownership REJECT, accept⟹safe violated). The flat cert cannot
            // attribute ONE scope-end drop to exactly-one-of-two arm allocs without a
            // checker/Coq change (out of scope). So a let-bound heap-result if/match stays
            // the sound deferred `Opaque` here until the certificate can express it.
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
        // PRECISE tuple field extraction (the layout brick, scalar-field slice): a tuple value is a
        // block [rc][len][cap][f0@12, f1@20, ...]; an ALL-SCALAR-field destructure (`let (a, b) = t`)
        // loads each field at its slot instead of the container-grain alias. The tuple still drops
        // at scope end (scalar fields move nothing). Heap-field tuples fall back to bind_pattern.
        if let IrPattern::Tuple { elements } = pattern {
            if let Some(subj) = subject {
                if self.try_lower_scalar_tuple(elements, subj) {
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

    fn try_lower_scalar_tuple_construct(&mut self, elements: &[IrExpr]) -> Option<ValueId> {
        if elements.iter().any(|e| is_heap_ty(&e.ty)) {
            return None; // heap-element tuple → the masked `try_lower_tuple_construct` path.
        }
        let dst = self.try_lower_scalar_list_slots(elements)?;
        // A scalar tuple is built with the uniform slot layout, so `t.0` / a `${tuple}` Display
        // reads its real slots. No heap slots → only the SAFE scalar reads are enabled.
        self.materialized_aggregates.insert(dst);
        Some(dst)
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
        Some(dst)
    }

    /// Extract each SCALAR field of a tuple `subject` (a heap block) into its bound var via a
    /// `Prim::Load` at the field's slot. Returns `false` (caller falls back to `bind_pattern`) if
    /// any field is heap or a non-`Bind`/`Wildcard` pattern (precise heap-field move is deferred —
    /// the all-heap borrow path traps inside loops, TODO the while-loop interaction).
    fn try_lower_scalar_tuple(&mut self, pats: &[IrPattern], subject: ValueId) -> bool {
        use crate::{IntOp, PrimKind};
        for p in pats {
            match p {
                IrPattern::Bind { ty, .. } if !is_heap_ty(ty) => {}
                IrPattern::Wildcard => {}
                _ => return false,
            }
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subject] });
        for (i, p) in pats.iter().enumerate() {
            if let IrPattern::Bind { var, .. } = p {
                let off = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
                let addr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
                let v = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v), args: vec![addr] });
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
