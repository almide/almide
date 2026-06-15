//! `LowerCtx` methods: calls (extracted from lower/mod.rs).

use super::*;
use crate::purity;
use crate::{CallArg, Init, Op, Repr, RtFn, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind,
};
use almide_lang::types::Ty;

impl LowerCtx {

    /// Lower a stdlib `Module` call (`<module>.<func>(args)`) in a VALUE position
    /// (bind or tail) to an `Op::CallFn` named `"<module>.<func>"`, IFF admissible.
    ///
    /// THE GATE: PURE — the callee reaches no host capability of its OWN
    /// ([`purity::is_pure`]). An effectful call lowered as a bare `Op::CallFn` would
    /// silently omit its capability from `used` (the checker derives caps only from
    /// `Op::Call`/the transitive fold over named callees), i.e. accept-but-unsafe.
    /// Walling it keeps `used` complete by construction. (A pure combinator's dotted
    /// name is treated as Stdout-free by the fold — sound because it IS pure; the
    /// capabilities come from the CLOSURE it applies, captured below.)
    ///
    /// HIGHER-ORDER closures are admitted (a pure combinator — `list.map`/`filter`/
    /// `fold` … — INVOKES the closure during the call and DISCARDS it: it never
    /// escapes, so the closure's captures cannot outlive the scope). Each closure
    /// ARGUMENT is handled by its capability, its value DEFERRED:
    /// - a `Lambda` — its body's calls are recorded as effect markers
    ///   ([`Self::record_elided_calls`]), so a printing closure taints HONESTLY and a
    ///   nested higher-order call inside the body is left elided (the `mir <= ir`
    ///   gate then taints — never a FALSE caps-verified);
    /// - a `ClosureCreate`/`FnRef` — its named callee is recorded as a marker so the
    ///   fold reaches its capabilities;
    /// - an OPAQUE function value (a `Fn`-typed `Var`/expr whose callee is unknown
    ///   here) is WALLED — its capabilities are unanalyzable, so admitting it would
    ///   be accept-but-unsafe. The closure's captures are BORROWED (the env is not
    ///   materialized → the rendered code owns nothing extra → memory-safe).
    ///
    /// Non-closure args are lowered normally. A heap result is a FRESH OWNED value
    /// (the return-mode signature), a scalar result carries no ownership. The caller
    /// decides bind (push to live handles) vs tail (move out). Returns the result.
    pub(crate) fn lower_pure_module_value_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Result<ValueId, LowerError> {
        // The primitive floor: `prim.load32(a)` / `prim.handle(s)` / `prim.fd_write(…)`
        // map to an Op::Prim, not a real CallFn (the v1 self-host floor).
        if module == "prim" {
            return self
                .lower_prim_call(func, args)?
                .ok_or_else(|| LowerError::Unsupported(format!("prim.{func} yields no value here")));
        }
        let arg_tys: Vec<Ty> = args.iter().map(|a| a.ty.clone()).collect();
        let lowered = self.lower_pure_module_call_args(module, func, args)?;
        let dst = self.fresh_value();
        let repr = repr_of(result_ty)?;
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: list_heap_call_name(module, func, &arg_tys, result_ty),
            args: lowered,
            result: Some(repr),
        });
        Ok(dst)
    }

    /// Admission + closure-capability capture shared by a stdlib `Module` call in any
    /// position (value or effect). Requires PURITY (the combinator's OWN caps must be
    /// ∅ — an effectful call would omit its capability, accept-but-unsafe). Captures
    /// each closure ARGUMENT's capabilities while DEFERRING its value and BORROWING
    /// its captures: a `Lambda` body's calls become effect markers, a `ClosureCreate`/
    /// `FnRef` named callee a marker; an OPAQUE function value (unanalyzable caps) is
    /// walled. Returns the lowered REGULAR (non-closure) args. The pure combinator
    /// invokes-and-discards the closure, so its captures never escape — see
    /// [`Self::lower_pure_module_value_call`].
    pub(crate) fn lower_pure_module_call_args(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Vec<CallArg>, LowerError> {
        if !purity::is_pure(module, func) {
            return Err(LowerError::Unsupported(format!(
                "effectful/impure stdlib Module call {module}.{func} needs a declared capability not in this brick"
            )));
        }
        let mut out: Vec<CallArg> = Vec::with_capacity(args.len());
        for a in args {
            match &a.kind {
                // A NON-CAPTURING lambda ARGUMENT to a pure combinator (`list.map(xs, (x) =>
                // …)`): LIFT it and PASS its FuncRef table slot BY VALUE, so a SELF-HOSTED
                // combinator (auto-linked `list.map`/`filter`/`fold`) receives a real
                // callable closure and invokes it via CallIndirect. A CAPTURING lambda has no
                // liftable form, so it keeps the builtin-combinator model: its calls are
                // captured for the caps fold and the value is DROPPED (a builtin combinator
                // that is never self-host-linked ignores the extra arg — its name is
                // is_known_free, no body to mismatch). The lifted lambda's caps reach this
                // function through the FuncRef edge (folded at creation), so a printing
                // closure can never be silently caps-verified.
                IrExprKind::Lambda { params, body, .. } => match self.lift_lambda(params, body) {
                    Some(slot) => out.push(CallArg::Scalar(slot)),
                    None => self.record_elided_calls(body),
                },
                IrExprKind::ClosureCreate { func_name, .. } => self.ops.push(Op::CallFn {
                    dst: None,
                    name: func_name.as_str().to_string(),
                    args: Vec::new(),
                    result: None,
                }),
                IrExprKind::FnRef { name } => self.ops.push(Op::CallFn {
                    dst: None,
                    name: name.as_str().to_string(),
                    args: Vec::new(),
                    result: None,
                }),
                _ if matches!(a.ty, Ty::Fn { .. }) => {
                    return Err(LowerError::Unsupported(format!(
                        "Module call {module}.{func} with an opaque function-value argument (capabilities unanalyzable) not in this brick"
                    )))
                }
                // A regular (non-closure) argument — lower it with the same per-arg machinery
                // as any call, preserving argument ORDER among the closure slots.
                _ => out.extend(self.lower_call_args(std::slice::from_ref(a))?),
            }
        }
        Ok(out)
    }

    /// Lower a pure `Module` COMBINATOR applied for its EFFECT (`list.each(xs, f)` in
    /// statement position) — the side effect is the CLOSURE's, captured by
    /// [`Self::lower_pure_module_call_args`]. A Unit/scalar result carries no
    /// ownership; a (rarely) discarded HEAP result is allocated and dropped at scope
    /// end (value semantics — never leaked).
    pub(crate) fn lower_effect_module_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Result<(), LowerError> {
        // A prim-floor STATEMENT (`prim.store32(a, v)`) → Op::Prim (Unit, no result).
        if module == "prim" {
            self.lower_prim_call(func, args)?;
            return Ok(());
        }
        let lowered = self.lower_pure_module_call_args(module, func, args)?;
        if is_heap_ty(result_ty) {
            let dst = self.fresh_value();
            let repr = repr_of(result_ty)?;
            self.ops.push(Op::CallFn {
                dst: Some(dst),
                name: format!("{module}.{func}"),
                args: lowered,
                result: Some(repr),
            });
            self.live_heap_handles.push(dst);
        } else {
            self.ops.push(Op::CallFn {
                dst: None,
                name: format!("{module}.{func}"),
                args: lowered,
                result: None,
            });
        }
        Ok(())
    }

    /// Make the CALLS hidden inside a value whose CONTENT is deferred to
    /// `Init::Opaque` / `Const` VISIBLE to the transitive capability fold. An
    /// Opaque/Const value lowers NONE of its sub-expressions, so a call buried in a
    /// list element, constructor payload, operand, or scalar value (`[f()]`,
    /// `Some(g(x))`, `a ++ h()`, `var n = list.len(xs)`) vanishes from the MIR —
    /// invisible to the caps fold over `Op::CallFn` edges, forcing the corpus gate
    /// to conservatively TAINT the whole function. This appends a bare EFFECT MARKER
    /// `Op::CallFn { dst: None, args: [], result: None }` per such call: the
    /// existing handlers already treat a result-less, dst-less call as a PURE EFFECT
    /// — `ownership_certificate` emits no event (no `+1`/drop), `name_witness`
    /// references nothing (no dangling ref), the `+1`-backing gate ignores it — yet
    /// `reachable_caps_or_tainted` matches it by NAME and folds the callee
    /// transitively. So the EFFECT becomes analyzable while the value CONTENT stays
    /// deferred: the same Opaque deferral, now extended to the capability axis.
    ///
    /// Only calls whose capabilities the fold models SOUNDLY are recorded: a
    /// first-order `Named` call (the fold opens an in-profile callee or honestly
    /// taints an unknown one) and a first-order PURE `Module` call (a dotted name
    /// the gate treats as Stdout-free — sound because it IS pure). A higher-order
    /// call (unmodelled closure caps), an effectful/impure `Module` call (its dotted
    /// name would be WRONGLY treated as free), and a `Method`/`Computed` target are
    /// SKIPPED — left elided, so the `ir_calls > mir_calls` gate keeps the function
    /// tainted (no FALSE de-taint). This never errors and never walls — it only adds
    /// effect markers, so it can never turn an in-profile function `Unsupported`.
    ///
    /// SOUNDNESS BACKSTOP: a marker is recorded ONLY at a wholesale-elided position
    /// (the caller emits one `Opaque`/`Const` op for the whole `value`, lowering
    /// none of its sub-calls), so the MIR call-op count can only rise TOWARD the
    /// IR's, never past it. The corpus gate asserts `mir_calls <= ir_calls` — a
    /// double-count (the one way a marker could mask a real elision and FALSELY
    /// de-taint a function) then fails the gate, structurally impossible to ship.
    pub(crate) fn record_elided_calls(&mut self, value: &IrExpr) {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct Collector {
            names: Vec<String>,
        }
        impl IrVisitor for Collector {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Call { target, args, .. } = &e.kind {
                    if !is_higher_order(args) {
                        match target {
                            CallTarget::Named { name } => {
                                self.names.push(name.as_str().to_string())
                            }
                            CallTarget::Module { module, func, .. }
                                if purity::is_pure(module.as_str(), func.as_str()) =>
                            {
                                self.names.push(format!("{}.{}", module.as_str(), func.as_str()))
                            }
                            _ => {}
                        }
                    }
                }
                walk_expr(self, e);
            }
        }
        let mut c = Collector { names: Vec::new() };
        c.visit_expr(value);
        for name in c.names {
            self.ops.push(Op::CallFn { dst: None, name, args: Vec::new(), result: None });
        }
    }

    /// Lower an EFFECT call (a Unit-typed `Call`) to a runtime [`Op::Call`].
    /// Today the recognized set is `println(s)` for a heap string → [`RtFn::PrintStr`],
    /// which BORROWS the string handle (no refcount change; the value stays live
    /// and is dropped at scope end) and reaches [`crate::Capability::Stdout`] (so a
    /// real printing program's capability witness is derived from real source).
    /// Anything outside the set is an explicit `Unsupported` (totality).
    pub(crate) fn lower_effect_call(&mut self, call: &IrExpr) -> Result<(), LowerError> {
        // A primitive-floor STATEMENT (`prim.store32(...)` / a discarded `prim.*`):
        // `@intrinsic` lowers it to a `RuntimeCall`; map the `almide_rt_prim_*` symbol
        // to an `Op::Prim` (a store is Unit, so the dst is None — nothing to discard).
        if let IrExprKind::RuntimeCall { symbol, args } = &call.kind {
            if let Some(func) = symbol.as_str().strip_prefix("almide_rt_prim_") {
                self.lower_prim_call(func, args)?;
                return Ok(());
            }
        }
        let (target, args) = match &call.kind {
            IrExprKind::Call { target, args, .. } => (target, args),
            other => {
                return Err(LowerError::Unsupported(format!(
                    "effect statement {} is not a call",
                    kind_name(other)
                )))
            }
        };
        let name = match target {
            CallTarget::Named { name } => name.as_str(),
            // A pure Module COMBINATOR applied for side effects (`list.each(xs, f)`):
            // the effect is the CLOSURE's. Capture the closure's capabilities, borrow
            // the regular args, and emit the Unit-result call — exactly the value-
            // position higher-order handling, minus the result. An effectful/impure
            // Module call reaches a host capability of its OWN that the model cannot
            // yet name, so it stays walled (`purity::is_pure` gates inside).
            CallTarget::Module { module, func, .. } => {
                return self.lower_effect_module_call(module.as_str(), func.as_str(), args, &call.ty)
            }
            CallTarget::Method { method, .. } => {
                return Err(LowerError::Unsupported(format!(
                    "effect Method call .{} (unresolved dispatch) not in this brick",
                    method.as_str()
                )))
            }
            // A Computed effect call `(g)()` — the callee is a closure VALUE we cannot
            // name. DEFER it exactly like a Computed VALUE call: the callee's and args'
            // analyzable sub-calls are captured (`record_elided_calls`), the Computed
            // call itself is ELIDED (no nameable `CallFn`). Since `count_ir_calls` counts
            // the Computed `Call` node but the lowering emits no marker for it,
            // `ir_calls > mir_calls` TAINTS the function caps-unverified — honest (the
            // closure's invocation capabilities are unknown), never falsely caps-verified.
            // A discarded HEAP result is a fresh `Alloc{Opaque}` dropped at scope end;
            // a Unit/scalar result carries no ownership.
            CallTarget::Computed { callee } => {
                // A Unit-result call THROUGH a lifted lambda value EXECUTES via CallIndirect
                // (e.g. `let f = (x) => print_it(x); f(3)`). Otherwise — a dynamic closure
                // value we cannot name — DEFER as before (calls captured, the Computed call
                // elided ⇒ honest caps taint).
                if let Some(table_idx) = self.funcref_value_of(callee) {
                    let mark = self.ops.len();
                    let lhh = self.live_heap_handles.len();
                    if let Ok(lowered) = self.lower_call_args(args) {
                        self.ops.push(Op::CallIndirect {
                            dst: None,
                            table_idx,
                            args: lowered,
                            result: None,
                        });
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh);
                }
                self.record_elided_calls(call);
                if is_heap_ty(&call.ty) {
                    let dst = self.fresh_value();
                    let repr = repr_of(&call.ty)?;
                    self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                    self.live_heap_handles.push(dst);
                }
                return Ok(());
            }
        };
        match (name, args.as_slice()) {
            // println(s) — the heap-string argument is BORROWED for a Stdout write.
            // A non-var arg (a literal `println("x")`, a concat `println(a ++ b)`,
            // an interpolation `println("${x}")`, or a call result `println(f())`)
            // is materialized into an owned temp by `lower_call_args` (the same
            // arg machinery as a normal call), then borrowed; the temp is dropped
            // at scope end. The Stdout effect makes the function caps-unverified
            // (it reaches Stdout, which `declared_caps` is empty for) — honest, not
            // claimed caps-safe.
            ("println", [arg]) if is_heap_ty(&arg.ty) => {
                let lowered = self.lower_call_args(std::slice::from_ref(arg))?;
                self.ops.push(Op::Call { dst: None, func: RtFn::PrintStr, args: lowered, result: None });
                Ok(())
            }
            // A USER function call (Unit result, e.g. `beep()`) → Op::CallFn. The
            // call BORROWS its heap-handle args (no refcount change here). The
            // callee's capabilities are accounted for at the CALL SITE against
            // its signature (the per-call-site subset rule), so a program is
            // rejected for a capability a CALLEE reaches — transitively — even
            // with no direct effect (closes the direct-only caps gap).
            (callee, call_args) => {
                let lowered = self.lower_call_args(call_args)?;
                self.ops.push(Op::CallFn {
                    dst: None,
                    name: callee.to_string(),
                    args: lowered,
                result: None });
                Ok(())
            }
        }
    }

    /// Lower call arguments to [`CallArg`]s. A heap var is BORROWED (`Handle`), a
    /// scalar var is a `Scalar`, an int literal is an `Imm`. A nested CALL argument
    /// (`f(g(x))` / `f(string.trim(s))`) is MATERIALIZED: the inner call's result
    /// is computed into a fresh OWNED temp, then BORROWED into the outer call and
    /// dropped at scope end — cert `i` (call-result) + `d` (drop), both backed by
    /// real ops; the temp's capabilities are folded transitively by the corpus gate
    /// (an effectful callee taints the caller honestly). The inner call must itself
    /// be admissible: a `Named` user call, or a first-order pure stdlib `Module`
    /// call. Anything else is an explicit `Unsupported` (totality).
    pub(crate) fn lower_call_args(&mut self, args: &[IrExpr]) -> Result<Vec<CallArg>, LowerError> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let arg = match &a.kind {
                // A FUNCTION-typed var (`f` passed on to `__map_fill(…, f, …)`) is a SCALAR
                // table slot, NOT a borrowed heap handle — pass it by value so the callee can
                // CallIndirect through it. (Its `Ty::Fn` is_heap, so it must precede the heap
                // Var arm.) This threads a closure through nested self-host helpers.
                IrExprKind::Var { id } if matches!(a.ty, Ty::Fn { .. }) => {
                    CallArg::Scalar(self.value_or_global(*id)?)
                }
                IrExprKind::Var { id } if is_heap_ty(&a.ty) => CallArg::Handle(self.value_or_global(*id)?),
                IrExprKind::Var { id } => CallArg::Scalar(self.value_or_global(*id)?),
                IrExprKind::LitInt { value } => CallArg::Imm(*value),
                // A NON-CAPTURING lambda ARGUMENT (`list.map(xs, (x) => x + 1)`): LIFT it to
                // a fresh `__lambda_*` function and pass its `FuncRef` table slot BY VALUE
                // (a scalar i64) — the callee invokes it via `Op::CallIndirect` through its
                // function-typed param. This is the call-site half of higher-order self-host
                // (`list.map`/`filter`/`fold`). A CAPTURING lambda has no liftable form, so
                // it falls through to the deferred Opaque arm below (unchanged).
                IrExprKind::Lambda { params, body, .. } => {
                    match self.lift_lambda(params, body) {
                        Some(slot) => CallArg::Scalar(slot),
                        None => {
                            let dst = self.fresh_value();
                            let repr = repr_of(&a.ty)?;
                            self.ops.push(Op::Alloc { dst, repr, init: alloc_init(a) });
                            self.record_elided_calls(a);
                            self.materialized_call_arg(dst, repr)
                        }
                    }
                }
                // A fresh HEAP literal argument (`f("x")`, `f([1, 2, 3])`):
                // materialized into an owned temp via `Alloc`, borrowed into the
                // call, dropped at scope end — cert `i` (alloc) + `d` (drop), both
                // backed, identical to the verified fresh-heap bind.
                IrExprKind::LitStr { .. }
                | IrExprKind::List { .. }
                | IrExprKind::MapLiteral { .. }
                | IrExprKind::EmptyMap
                | IrExprKind::Record { .. }
                | IrExprKind::SpreadRecord { .. }
                | IrExprKind::Tuple { .. }
                | IrExprKind::StringInterp { .. }
                | IrExprKind::ResultOk { .. }
                | IrExprKind::ResultErr { .. }
                | IrExprKind::OptionSome { .. }
                | IrExprKind::OptionNone
                // A CLOSURE value argument (`register((x) => …)`): a fresh heap env,
                // materialized + borrowed into the call. The callee borrows it per the
                // borrow-by-default convention; its body's calls are elided ⇒ the gate
                // taints the function caps-unverified (invocation caps unknown).
                // (A NON-CAPTURING `Lambda` arg is intercepted BELOW and lifted to a scalar
                // FuncRef slot passed by value — `list.map(xs, (x) => x + 1)`; only a
                // capturing one reaches this deferred Opaque arm.)
                | IrExprKind::ClosureCreate { .. } => {
                    let dst = self.fresh_value();
                    let repr = repr_of(&a.ty)?;
                    let init = alloc_init(a);
                    self.ops.push(Op::Alloc { dst, repr, init });
                    self.record_elided_calls(a);
                    self.materialized_call_arg(dst, repr)
                }
                // A Bool literal argument (`f(true)`): the real value 1/0 (the `if` cond
                // a callee branches on). `LitInt` is already an `Imm` above.
                IrExprKind::LitBool { value } => {
                    let dst = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst, value: if *value { 1 } else { 0 } });
                    CallArg::Scalar(dst)
                }
                // A Float literal arg (`f(2.5)`): the i64-uniform value carries the f64 BITS
                // (the float-floor render reinterprets), so `2.5` materializes as ConstInt.
                IrExprKind::LitFloat { value } => {
                    let dst = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst, value: value.to_bits() as i64 });
                    CallArg::Scalar(dst)
                }
                // A fresh BinOp/UnOp result as an argument (`f(a + b)`, `f(-n)`), or an
                // ERROR OPERATOR result (`f(x!)`, `f(x ?? d)`, `f(x?.field)`): a fresh
                // computed value — a heap result is materialized via `Alloc` (borrowed
                // and dropped), a scalar result is a `Const`. Operands carry their own
                // ownership; the operator's value (and any early-return) is deferred.
                IrExprKind::BinOp { .. }
                | IrExprKind::UnOp { .. }
                | IrExprKind::Try { .. }
                | IrExprKind::Unwrap { .. }
                | IrExprKind::UnwrapOr { .. }
                | IrExprKind::ToOption { .. }
                | IrExprKind::OptionalChain { .. }
                // A RANGE (`f(0..n)`), a RUNTIME CALL, or an `if`/`match` ARGUMENT is a
                // fresh value of the same shape — a deferred `Alloc{Opaque}`/`Const`,
                // its calls (incl. the branch arms' calls) captured by
                // `record_elided_calls`; the arms' values/effects are deferred.
                | IrExprKind::Range { .. }
                | IrExprKind::RuntimeCall { .. }
                | IrExprKind::If { .. }
                | IrExprKind::Match { .. } => {
                    if is_heap_ty(&a.ty) {
                        let dst = self.fresh_value();
                        self.record_elided_calls(a);
                        let repr = repr_of(&a.ty)?;
                        self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                        self.materialized_call_arg(dst, repr)
                    } else {
                        // A scalar Int arithmetic / comparison / prim arg computes its
                        // REAL value (`f(n / 10)` → IntBinOp); outside that subset it
                        // rolls back to the deferred Const + elided caps marker.
                        let mark = self.ops.len();
                        match self.lower_scalar_value(a) {
                            Some(v) => CallArg::Scalar(v),
                            None => {
                                self.ops.truncate(mark);
                                let dst = self.fresh_value();
                                self.record_elided_calls(a);
                                self.ops.push(Op::Const { dst });
                                CallArg::Scalar(dst)
                            }
                        }
                    }
                }
                // A field/element/tuple EXTRACTION argument. A SCALAR result is an
                // unambiguous COPY → `Const`. A HEAP result is an ALIAS/share of
                // the container → `Op::Dup` of the container value (the container-
                // grain field access), borrowed into the call and dropped at scope
                // end. (A nested-container extraction stays walled inside
                // `lower_heap_extraction`.)
                IrExprKind::Member { .. }
                | IrExprKind::IndexAccess { .. }
                | IrExprKind::MapAccess { .. }
                | IrExprKind::TupleIndex { .. } => {
                    if is_heap_ty(&a.ty) {
                        let repr = repr_of(&a.ty)?;
                        let dst = match self.lower_heap_extraction(a) {
                            Ok(dst) => dst,
                            // A non-var container (`f().x`) → deferred fresh Opaque.
                            Err(_) => {
                                let dst = self.fresh_value();
                                self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                                self.record_elided_calls(a);
                                dst
                            }
                        };
                        self.materialized_call_arg(dst, repr)
                    } else {
                        // A SCALAR extraction is a `Const` copy — its container
                        // (which may itself be a call, `g().field`) is elided; record
                        // any call so the caps fold sees it.
                        let dst = self.fresh_value();
                        self.ops.push(Op::Const { dst });
                        self.record_elided_calls(a);
                        CallArg::Scalar(dst)
                    }
                }
                // A Named user-call result, materialized into an owned temp.
                IrExprKind::Call { target: CallTarget::Named { name }, args: inner, .. } => {
                    let inner_args = self.lower_call_args(inner)?;
                    let dst = self.fresh_value();
                    let repr = repr_of(&a.ty)?;
                    self.ops.push(Op::CallFn {
                        dst: Some(dst),
                        name: name.as_str().to_string(),
                        args: inner_args,
                        result: Some(repr),
                    });
                    self.materialized_call_arg(dst, repr)
                }
                // A first-order pure stdlib Module-call result, materialized (the
                // purity + higher-order gates live in `lower_pure_module_value_call`).
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args: inner, .. } => {
                    let dst = self.lower_pure_module_value_call(
                        module.as_str(),
                        func.as_str(),
                        inner,
                        &a.ty,
                    )?;
                    let repr = repr_of(&a.ty)?;
                    self.materialized_call_arg(dst, repr)
                }
                // A `Method`/`Computed`-target call argument (`f(obj.m())`,
                // `f((g)())`): an UNRESOLVABLE callee (dispatch / closure value not
                // known here) — model it as a DEFERRED fresh value (a heap `Alloc`
                // borrowed+dropped, a scalar `Const`). Its receiver's/args' calls are
                // captured by `record_elided_calls`, but the method/computed call
                // itself is NOT (skipped), so the source has MORE call nodes than the
                // MIR ⇒ the `ir_calls > mir_calls` gate TAINTS the function caps-
                // unverified (honest — the callee's capabilities are unknown). The
                // result value is deferred, like every Opaque.
                IrExprKind::Call { .. } => {
                    let dst = self.fresh_value();
                    self.record_elided_calls(a);
                    if is_heap_ty(&a.ty) {
                        let repr = repr_of(&a.ty)?;
                        self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                        self.materialized_call_arg(dst, repr)
                    } else {
                        self.ops.push(Op::Const { dst });
                        CallArg::Scalar(dst)
                    }
                }
                other => {
                    return Err(LowerError::Unsupported(format!(
                        "call argument {} not in this brick",
                        kind_name(other)
                    )))
                }
            };
            out.push(arg);
        }
        Ok(out)
    }

    /// Try to lower a SCALAR-result call to a REAL executable `CallFn` (arguments
    /// materialized via [`Self::lower_call_args`], the scalar result bound to a fresh
    /// `dst`), returning `Some(dst)`. Mirrors the heap Named/pure-`Module` call
    /// lowering MINUS the live-heap-handle — a scalar result carries no ownership
    /// (`Repr::Scalar` is not heap), so it is bound but never dropped.
    ///
    /// Returns `None` for a non-call value, an unresolvable `Method`/`Computed`
    /// callee, or a call whose args / module-purity are not resolvably executable —
    /// the caller then DEFERS it (a `Const` + an elided-caps marker), exactly as
    /// before. A partial-then-failed lowering rolls back its pushed ops/handles, so
    /// the deferred path starts clean. This can NEVER turn an in-profile function
    /// `Unsupported` (the deferral is always available) — the in-profile set and the
    /// caps fold are preserved: a real `CallFn` replaces the elided marker 1:1 (same
    /// callee NAME, so `reachable_caps` is unchanged; same op count, so the
    /// `mir_calls <= ir_calls` gate cannot falsely de-taint).
    /// If `callee` names a local bound to a LIFTED lambda (an `Op::FuncRef` value recorded
    /// in `funcref_values`), return that value — the table slot a `CallIndirect` dispatches
    /// through. Returns `None` for any other computed callee (a dynamic closure param, an
    /// unanalyzable value), so the caller keeps the sound deferred model for those.
    pub(crate) fn funcref_value_of(&self, callee: &IrExpr) -> Option<ValueId> {
        if let IrExprKind::Var { id } = &callee.kind {
            if let Some(v) = self.value_of.get(id) {
                if self.funcref_values.contains(v) {
                    return Some(*v);
                }
            }
        }
        None
    }

    pub(crate) fn try_lower_scalar_call(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        match &value.kind {
            // A scalar call THROUGH a lifted lambda value (`let y = f(5)` where `f` bound a
            // non-capturing lambda ⇒ an `Op::FuncRef`). The callee resolves to a funcref
            // value, so this lowers to `Op::CallIndirect` and the closure EXECUTES — args
            // materialized like any call, the scalar result bound. A Computed callee that is
            // NOT a known funcref returns `None` and DEFERS (the existing model). The MIR
            // CallIndirect is a genuine call (the corpus gate counts it), so it replaces the
            // elided Computed 1:1 — no spurious caps taint, no `mir > ir` breach.
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } => {
                let table_idx = self.funcref_value_of(callee)?;
                let repr = repr_of(ty).ok()?;
                match self.lower_call_args(args) {
                    Ok(lowered) => {
                        let dst = self.fresh_value();
                        self.ops.push(Op::CallIndirect {
                            dst: Some(dst),
                            table_idx,
                            args: lowered,
                            result: Some(repr),
                        });
                        Some(dst)
                    }
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        None
                    }
                }
            }
            // A scalar `Named` user call (`fn f() = g()`, `let n = add(2, 3)`).
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let repr = repr_of(ty).ok()?;
                match self.lower_call_args(args) {
                    Ok(lowered) => {
                        let dst = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(dst),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(repr),
                        });
                        Some(dst)
                    }
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        None
                    }
                }
            }
            // A scalar first-order PURE `Module` call (`let n = string.len(s)`): the
            // purity / higher-order gate is inside `lower_pure_module_value_call`; an
            // impure/HO/unsupported call errors → roll back and defer (no new wall).
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                match self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, ty) {
                    Ok(dst) => Some(dst),
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        None
                    }
                }
            }
            _ => None,
        }
    }

    /// Lower a SCALAR `Int` expression to a `ValueId` holding its REAL value (the
    /// scalar-value foundation): a Var/param, an `Int` literal (`ConstInt`), or an
    /// `Int` Add/Sub/Mul (`IntBinOp` over recursively-lowered operands). Returns
    /// `None` for anything outside this subset (Div/Mod/Pow, comparisons, logic,
    /// Float, calls, …) — the caller then DEFERS the value (`Const`). It pushes only
    /// `ConstInt`/`IntBinOp` (never a heap handle / ownership event), so a caller can
    /// roll back a partial attempt by truncating `self.ops`. The cert is unaffected:
    /// `IntBinOp`/`ConstInt` are no-ops for ownership and already define their `dst` /
    /// use their operands for the name witness.
    pub(crate) fn lower_scalar_value(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        match &expr.kind {
            IrExprKind::Var { id } => self.value_or_global(*id).ok(),
            IrExprKind::LitInt { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: *value });
                Some(dst)
            }
            // A Bool is a scalar int (true = 1, false = 0) — the `if` condition.
            IrExprKind::LitBool { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: if *value { 1 } else { 0 } });
                Some(dst)
            }
            // A FLOAT literal: the i64-uniform value holds the f64 BITS, so `3.5` materializes
            // as `ConstInt(3.5_f64.to_bits())`. The render's float prims reinterpret it back.
            IrExprKind::LitFloat { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: value.to_bits() as i64 });
                Some(dst)
            }
            IrExprKind::BinOp { op, left, right } => {
                let iop = match op {
                    BinOp::AddInt => crate::IntOp::Add,
                    BinOp::SubInt => crate::IntOp::Sub,
                    BinOp::MulInt => crate::IntOp::Mul,
                    BinOp::DivInt => crate::IntOp::Div,
                    BinOp::ModInt => crate::IntOp::Mod,
                    // Comparisons (the `if` condition) — INT operands only (a Float/
                    // String compare needs a different op). Gate on the operand type.
                    BinOp::Lt if matches!(left.ty, Ty::Int) => crate::IntOp::Lt,
                    BinOp::Lte if matches!(left.ty, Ty::Int) => crate::IntOp::Le,
                    BinOp::Gt if matches!(left.ty, Ty::Int) => crate::IntOp::Gt,
                    BinOp::Gte if matches!(left.ty, Ty::Int) => crate::IntOp::Ge,
                    BinOp::Eq if matches!(left.ty, Ty::Int) => crate::IntOp::Eq,
                    BinOp::Neq if matches!(left.ty, Ty::Int) => crate::IntOp::Ne,
                    // Pow, Float, logic, concat, non-Int compares: defer.
                    _ => return None,
                };
                let a = self.lower_scalar_value(left)?;
                let b = self.lower_scalar_value(right)?;
                let dst = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst, op: iop, a, b });
                Some(dst)
            }
            // A scalar-result PRIMITIVE-FLOOR call (`prim.handle`/`prim.load32`/
            // `prim.fd_write`) — `@intrinsic` lowers it to a `RuntimeCall`; we map the
            // `almide_rt_prim_*` symbol to an [`Op::Prim`] (NOT the deferred Const a
            // generic RuntimeCall gets). The self-host floor reaching executable code.
            IrExprKind::RuntimeCall { symbol, args } => {
                let func = symbol.as_str().strip_prefix("almide_rt_prim_")?;
                self.lower_prim_call(func, args).ok().flatten()
            }
            // The same prim floor reached as a MODULE call (`prim.handle(buf)`) in a value
            // position — e.g. an address operand `prim.handle(buf) + LIST_HEADER`. prim
            // calls are pure scalar/handle ops (no ownership), so this is the narrow,
            // sound subset (NOT the general scalar-call-in-operand admission).
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if module.as_str() == "prim" =>
            {
                self.lower_prim_call(func.as_str(), args).ok().flatten()
            }
            _ => None,
        }
    }

    /// Lower a `prim.*` PRIMITIVE-FLOOR call to an [`Op::Prim`] — the v1 self-host
    /// floor (raw memory + the fd_write host call), mapped by name, NOT a real
    /// `CallFn`/runtime symbol. Each arg lowers to a ValueId via
    /// [`Self::lower_scalar_value`] (a handle var / int literal / int-arith). Returns
    /// the result `dst` (load / fd_write / handle) or `None` (a store is Unit).
    pub(crate) fn lower_prim_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        use crate::PrimKind;
        // `prim.alloc_str(byte_len)` allocates a runtime-sized OWNED String — an `Op::Alloc`
        // (cert `i`, a fresh owned object), NOT a scalar prim. The caller fills its bytes
        // via `prim.store8`; the result is moved out / dropped like any heap value.
        // `prim.alloc_str(n)` / `prim.alloc_bytes(n)` BOTH allocate a runtime-sized OWNED byte
        // block (`Init::DynStr`: rc=1, len set, data filled by store8) — physically identical;
        // they differ only in the prim's DECLARED return type (String vs Bytes). A flat heap
        // value (no nested ownership), moved out / dropped like any String.
        if func == "alloc_str" || func == "alloc_bytes" {
            let len_v = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} length is not a lowerable scalar"))
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::DynStr { len: len_v },
            });
            return Ok(Some(dst));
        }
        // `prim.alloc_list(n)` allocates a runtime-sized OWNED `List[Int]` of n i64 slots —
        // an `Op::Alloc` (cert `i`), the list-building sibling of alloc_str. The caller
        // fills it via `prim.store64`; moved out / dropped like any heap value.
        if func == "alloc_list" || func == "alloc_list_f64" || func == "alloc_set" || func == "alloc_map" || func == "alloc_value" {
            let len_v = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.alloc_list length is not a lowerable scalar".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::DynList { len: len_v },
            });
            return Ok(Some(dst));
        }
        // `prim.alloc_list_str(n)` allocates a runtime-sized OWNED `List[String]` (n slots,
        // physically identical to alloc_list) — but the dst is tracked as a NESTED-OWNERSHIP
        // list, so its scope-end drop is a recursive `DropListStr` (frees the owned element
        // Strings) and `prim.store_str` Consumes each String moved into it (Machinery 2).
        if func == "alloc_list_str" || func == "alloc_set_str" {
            let len_v = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.alloc_list_str length is not a lowerable scalar".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::DynListStr { len: len_v },
            });
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.store_str(list, byte_addr_of_slot, piece)` — store the String `piece`'s handle
        // into the list slot at `byte_addr_of_slot` AND CONSUME the piece (its reference is
        // MOVED into the list, which now owns it — cert `m`, removed from the scope drop set).
        // The slot holds the i64-widened handle; `DropListStr` later i32.wrap's it to free.
        if func == "store_str" {
            let addr = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.store_str slot address is not a lowerable scalar".into())
            })?;
            // The piece must be a tracked heap var (so we can Consume it). Its handle value:
            let piece = match &args[1].kind {
                IrExprKind::Var { id } => self.value_for(*id)?,
                _ => {
                    return Err(LowerError::Unsupported(
                        "prim.store_str piece must be a heap variable (to consume)".into(),
                    ))
                }
            };
            // The slot value is the piece's HANDLE (its address as an i64). Op::Prim Handle
            // gives that; store it 8-wide at the slot, then Consume the piece (move-out).
            let handle = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![piece] });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, handle] });
            self.ops.push(Op::Consume { v: piece });
            self.live_heap_handles.retain(|h| *h != piece);
            return Ok(None);
        }
        // Bitwise binary ops lower to a scalar `Op::IntBinOp` (i64 and/or/xor/shl/shr_s),
        // not an `Op::Prim` — the int.band/bor/bxor/bshl/bshr floor. No ownership.
        let bitop = match func {
            "band" => Some(crate::IntOp::And),
            "bor" => Some(crate::IntOp::Or),
            "bxor" => Some(crate::IntOp::Xor),
            "bshl" => Some(crate::IntOp::Shl),
            "bshr" => Some(crate::IntOp::Shr),
            "bshr_u" => Some(crate::IntOp::ShrU),
            _ => None,
        };
        if let Some(op) = bitop {
            let a = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} arg 0 is not a lowerable scalar"))
            })?;
            let b = self.lower_scalar_value(&args[1]).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} arg 1 is not a lowerable scalar"))
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst, op, a, b });
            return Ok(Some(dst));
        }
        let kind = match func {
            "handle" => PrimKind::Handle,
            "load8" => PrimKind::Load { width: 1 },
            "load32" => PrimKind::Load { width: 4 },
            "load64" => PrimKind::Load { width: 8 },
            // Load a 4-byte handle KEEPING Ptr repr — reads a String element out of a list slot
            // (a borrow of the slot's String, for passing to a closure / String fn).
            "load_str" => PrimKind::LoadHandle,
            "store32" => PrimKind::Store { width: 4 },
            "store8" => PrimKind::Store { width: 1 },
            "store64" => PrimKind::Store { width: 8 },
            "fd_write" => PrimKind::FdWrite,
            // The FLOAT floor (the f64 bits live in the i64-uniform value; render reinterprets).
            "fabs" => PrimKind::FloatUn(crate::FUnOp::Abs),
            "fsqrt" => PrimKind::FloatUn(crate::FUnOp::Sqrt),
            "ffloor" => PrimKind::FloatUn(crate::FUnOp::Floor),
            "fceil" => PrimKind::FloatUn(crate::FUnOp::Ceil),
            "fneg" => PrimKind::FloatUn(crate::FUnOp::Neg),
            "fadd" => PrimKind::FloatBin(crate::FBinOp::Add),
            "fsub" => PrimKind::FloatBin(crate::FBinOp::Sub),
            "fmul" => PrimKind::FloatBin(crate::FBinOp::Mul),
            "fdiv" => PrimKind::FloatBin(crate::FBinOp::Div),
            "fmin" => PrimKind::FloatBin(crate::FBinOp::Min),
            "fmax" => PrimKind::FloatBin(crate::FBinOp::Max),
            "fcopysign" => PrimKind::FloatBin(crate::FBinOp::CopySign),
            "flt" => PrimKind::FloatCmp(crate::FCmpOp::Lt),
            "fle" => PrimKind::FloatCmp(crate::FCmpOp::Le),
            "fgt" => PrimKind::FloatCmp(crate::FCmpOp::Gt),
            "fge" => PrimKind::FloatCmp(crate::FCmpOp::Ge),
            "feq" => PrimKind::FloatCmp(crate::FCmpOp::Eq),
            "fne" => PrimKind::FloatCmp(crate::FCmpOp::Ne),
            "f2i" => PrimKind::FloatToInt,
            "i2f" => PrimKind::IntToFloat,
            "fbits" | "ffrombits" => PrimKind::FloatBits,
            _ => return Err(LowerError::Unsupported(format!("unknown primitive prim.{func}"))),
        };
        let mut lowered = Vec::with_capacity(args.len());
        for a in args {
            let v = self.lower_scalar_value(a).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} argument is not a lowerable scalar/handle"))
            })?;
            lowered.push(v);
        }
        let dst = if matches!(kind, PrimKind::Store { .. }) { None } else { Some(self.fresh_value()) };
        // `prim.load_str` (LoadHandle) yields a BORROW of a list slot's String — the list still owns
        // it. Mark the result BORROWED so a `let` binding does not add it to the scope-end drop set
        // (that would double-free with the owning list's DropListStr).
        if matches!(kind, PrimKind::LoadHandle) {
            if let Some(d) = dst {
                self.param_values.insert(d);
            }
        }
        self.ops.push(Op::Prim { kind, dst, args: lowered });
        Ok(dst)
    }

    /// Register a freshly-materialized call-result temp used as a call argument: a
    /// HEAP temp is BORROWED into the call (`Handle`) and added to the scope-end
    /// drop set (it is owned by THIS scope, not moved out, so it is released after
    /// the call returns); a scalar temp is passed by value.
    pub(crate) fn materialized_call_arg(&mut self, dst: ValueId, repr: Repr) -> CallArg {
        if repr.is_heap() {
            self.live_heap_handles.push(dst);
            CallArg::Handle(dst)
        } else {
            CallArg::Scalar(dst)
        }
    }
}

