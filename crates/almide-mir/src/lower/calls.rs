//! `LowerCtx` methods: calls (extracted from lower/mod.rs).

use super::*;
use crate::purity;
use crate::{CallArg, Init, Op, Repr, RtFn, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrStringPart,
};
use almide_lang::types::Ty;

/// Substitute `Ty::TypeVar(name)` with the supplied concrete type throughout `ty` —
/// the generic-record instantiation used by the VALUE MODEL (`Box[Int]`'s `value: T`
/// becomes `value: Int`). Total over `Ty`; an unmapped `TypeVar` is left as-is (the
/// caller's `scalar_field_width` then rejects it, walling the record).
pub(super) fn subst_type_var(
    ty: &Ty,
    subst: &std::collections::HashMap<almide_lang::intern::Sym, Ty>,
) -> Ty {
    match ty {
        Ty::TypeVar(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::Applied(id, args) => {
            Ty::Applied(id.clone(), args.iter().map(|a| subst_type_var(a, subst)).collect())
        }
        Ty::Record { fields } => Ty::Record {
            fields: fields.iter().map(|(n, t)| (*n, subst_type_var(t, subst))).collect(),
        },
        Ty::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|(n, t)| (*n, subst_type_var(t, subst))).collect(),
        },
        Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| subst_type_var(e, subst)).collect()),
        // A generic PARAMETER of a record decl is stored as a bare `Named(T, [])` (the
        // frontend lowers an uninstantiated type variable to a nullary named type, NOT a
        // `Ty::TypeVar`). When `T` is one of this type's params (it is in `subst`), resolve
        // it to the instantiated arg — this is the #650 "generic field sized by its
        // INSTANTIATED type" fix, the substitution `aggregate_field_tys` relies on so a
        // `Box[Int]` field `value: T` resolves to `Int` (and its heap-ness is decided
        // correctly for the spread-copy / offset paths). A `Named` WITH args is a real
        // applied type — recurse into the args only.
        Ty::Named(name, args) if args.is_empty() && subst.contains_key(name) => {
            subst.get(name).cloned().unwrap_or_else(|| ty.clone())
        }
        Ty::Named(name, args) => {
            Ty::Named(*name, args.iter().map(|a| subst_type_var(a, subst)).collect())
        }
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| subst_type_var(p, subst)).collect(),
            ret: Box::new(subst_type_var(ret, subst)),
        },
        Ty::Union(members) => {
            Ty::Union(members.iter().map(|m| subst_type_var(m, subst)).collect())
        }
        // Scalars, Variant, Const*, Unknown, Never, etc. carry no nested TypeVar this
        // brick substitutes through — returned unchanged.
        other => other.clone(),
    }
}

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
        // C1 DEFUNCTIONALIZATION — a `list.map`/`filter`/`fold` whose closure arg is an
        // INLINE lambda is specialized as a loop at the call site (no runtime closure, no
        // CallIndirect, no lifted fn). This is tried FIRST so a CAPTURING inline lambda
        // (`(x) => x * k`) WORKS via inline rather than walling at the self-host path below
        // (a capturing lambda has no liftable FuncRef). A non-inlinable form (a first-class
        // Var closure, a heap element/result, a side-effecting body) returns `None` and
        // falls through to the existing `lift_lambda` / self-host-combinator routing.
        if module == "list" && matches!(func, "map" | "filter" | "fold") {
            if let Some(dst) = self.try_lower_defunc_list_hof(func, args, result_ty) {
                return Ok(dst);
            }
        }
        let arg_tys: Vec<Ty> = args.iter().map(|a| a.ty.clone()).collect();
        let lowered = self.lower_pure_module_call_args(module, func, args)?;
        let dst = self.fresh_value();
        let repr = repr_of(result_ty)?;
        // `string.slice(s, start)` is the 2-arg overload of `string.slice(s, start, end)` with the
        // implicit `end = string.len(s)` (v0: `s.chars().skip(start)`). The frontend admits the short
        // form (min_params=2) WITHOUT padding it, so the 3-param `string.slice` impl would underflow.
        // Route the 2-arg form to a DEDICATED `string.slice2(s, start)` variant that computes the end
        // itself — this stays ONE CallFn ↔ ONE IR call node (no extra synthetic call, so the corpus
        // `mir == ir` double-count gate is untouched), unlike synthesizing a `string.len` call arg.
        let name = if module == "string" && func == "slice" && args.len() == 2 {
            "string.slice2".to_string()
        } else {
            list_heap_call_name(module, func, &arg_tys, result_ty)
        };
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name,
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
        self.last_call_had_unlifted_closure = false;
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
                    None => {
                        // A capturing / param-invoking lambda — no liftable form. The self-host
                        // combinator runs with a missing closure slot → an empty/garbage result.
                        self.last_call_had_unlifted_closure = true;
                        self.record_elided_calls(body);
                    }
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
        struct Collector<'a> {
            names: Vec<String>,
            registry: &'a crate::lower::RecordLayouts,
        }
        impl IrVisitor for Collector<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                match &e.kind {
                    IrExprKind::Call { target, args, .. } => {
                        if !is_higher_order(args) {
                            match target {
                                CallTarget::Named { name } => {
                                    self.names.push(name.as_str().to_string())
                                }
                                CallTarget::Module { module, func, .. }
                                    if purity::is_pure(module.as_str(), func.as_str()) =>
                                {
                                    self.names
                                        .push(format!("{}.{}", module.as_str(), func.as_str()))
                                }
                                _ => {}
                            }
                        }
                    }
                    // A string `+` OPERATOR (`BinOp::ConcatStr`) lowers, where reachable,
                    // to a real `__str_concat` CallFn (`try_lower_concat_str`); in a
                    // DEFERRED position — a heap-result match/if arm tail, an Opaque
                    // call/branch — it is elided exactly like a call. Surface it as an
                    // elided `__str_concat` marker so the caps gate's `mir_calls` matches
                    // the `ir_calls` ConcatStr count (else the enclosing function falsely
                    // taints caps-unverified — `ir_calls > mir_calls`). SOUND: `__str_concat`
                    // is pure (empty capability witness — an `Op::CallFn` contributes zero
                    // caps), and the marker carries NO value (`dst: None`, no leak). The
                    // marker maps 1:1 to the counted ConcatStr node, so `mir_calls <=
                    // ir_calls` is preserved.
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        self.names.push("__str_concat".to_string());
                    }
                    // A SCALAR-element list `+` OPERATOR (`BinOp::ConcatList` over List[Int/Float/Bool])
                    // lowers, where reachable, to a real `__list_concat` CallFn; in a DEFERRED position
                    // (a statement reassignment `c = c + [10]`, an Opaque branch/arg) it is elided like
                    // a call. Surface a `__list_concat` marker so the caps gate's `mir_calls` matches the
                    // `ir_calls` ConcatList count (the gate counts the SAME scalar-element shape). SOUND:
                    // `__list_concat` is pure (prim memory ops, empty capability witness), the marker
                    // carries no value (`dst: None`). A HEAP-element list concat is NOT counted by the
                    // gate and emits NO marker here (the `is_heap_ty` element guard mirrors the count).
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                        use almide_lang::types::constructor::TypeConstructorId;
                        let scalar_elem = matches!(&e.ty,
                            Ty::Applied(TypeConstructorId::List, a)
                                if a.len() == 1 && !crate::lower::is_heap_ty(&a[0]));
                        if scalar_elem {
                            self.names.push("__list_concat".to_string());
                        }
                    }
                    // A STRING INTERPOLATION in a DEFERRED position — a heap-result match/if
                    // arm where the WHOLE branch fell back to Opaque, or any Opaque value/arg.
                    // `count_ir_calls` credits a desugarable interp the call NODES of its
                    // desugared tree REGARDLESS of position (the gate's visitor walks every
                    // subtree); when the interp does NOT get folded by `try_lower_string_interp`
                    // (its enclosing branch is Opaque), surface the SAME synthetic calls as
                    // elided markers so `mir_calls` keeps pace with `ir_calls` (else the function
                    // falsely taints — the −32 caps regression). Every synthetic callee
                    // (`__str_concat`, `<module>.to_string`) is pure (no Stdout), so the markers
                    // add no capability; a NON-desugarable interp is credited 0 and emits 0
                    // markers here. The SYNTHETIC names are the ConcatStr + to_string wrappers
                    // ONLY — the operands' OWN calls (a `${g(x)}` callee) are reached by the
                    // `walk_expr` below over the ORIGINAL parts, so there is no double-count.
                    IrExprKind::StringInterp { parts } => {
                        for name in crate::lower::interp_synthetic_call_names(parts, self.registry) {
                            self.names.push(name);
                        }
                    }
                    _ => {}
                }
                walk_expr(self, e);
            }
        }
        let names = {
            let mut c = Collector { names: Vec::new(), registry: &self.record_layouts };
            c.visit_expr(value);
            c.names
        };
        for name in names {
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
    /// Lower a `BinOp::ConcatStr` (string `a + b`) to a `CallFn` to the self-host `__str_concat`
    /// (auto-linked) — a FRESH owned String of byte-len(a)+byte-len(b). The operands lower as
    /// borrowed-or-materialized call args (like any heap call); the result is a fresh owned heap
    /// value the CALLER owns (a bind drops it `d`, a tail returns it `m`, an arg materializes +
    /// drops it). OWNERSHIP is the SAME proven shape as any heap-result Named/Module call
    /// (CallFn-heap-result = cert `i`). Nested `a + b + c` recurses (each ConcatStr → one call).
    /// Returns `None` (rolled back) if an operand doesn't lower. The mir↔ir gate counts each
    /// `ConcatStr` node as 1 IR call (classify_corpus.rs) so this synthetic CallFn keeps
    /// `mir_calls <= ir_calls`.
    pub(crate) fn try_lower_concat_str(&mut self, value: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        let IrExprKind::BinOp { op: BinOp::ConcatStr, left, right } = &value.kind else {
            return None;
        };
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let arg_exprs = [(**left).clone(), (**right).clone()];
        let args = match self.lower_call_args(&arg_exprs) {
            Ok(a) => a,
            Err(_) => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let dst = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: "__str_concat".to_string(),
            args,
            result: Some(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        });
        Some(dst)
    }

    /// Lower a `BinOp::ConcatList` (list `a + b`) over a SCALAR-element list (`List[Int/Float/Bool]`)
    /// to a `CallFn` to the self-host `__list_concat` (auto-linked) — a FRESH owned list of
    /// len(a)+len(b) i64 slots, both element ranges byte-copied. The operands lower as borrowed-or-
    /// materialized call args (like any heap call); the result is a fresh owned list the CALLER owns
    /// (a bind drops it `d`, a tail returns it `m`, an arg materializes + drops it). OWNERSHIP is the
    /// SAME proven shape as any heap-result Named/Module call (CallFn-heap-result = cert `i`), exactly
    /// like `try_lower_concat_str`. GATED to a SCALAR element type: a heap-element list (`List[String]`)
    /// has owned String handles in its slots that a copy would ALIAS (double-free on drop), so it
    /// returns `None` (deferred — never wrong bytes). Nested `a + b + c` recurses (each ConcatList →
    /// one call). The mir↔ir gate counts each `ConcatList` node as 1 IR call (classify_corpus.rs) so
    /// this synthetic CallFn keeps `mir_calls <= ir_calls`.
    pub(crate) fn try_lower_concat_list(&mut self, value: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::BinOp { op: BinOp::ConcatList, left, right } = &value.kind else {
            return None;
        };
        let elem_ty = match &value.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
            _ => return None,
        };
        // SCALAR-element (i64 slots: Int/Float/Bool) → byte-copy `__list_concat`. HEAP-element String or
        // Value (OWNED handle slots) → the rc-incrementing `__list_concat_rc` (the new list co-owns each
        // element; the source's recursive drop frees its own refs). A heap-FIELD aggregate element
        // (tuple/record with inner heap) still DEFERS — it needs the masked recursive drop (tuple-heap).
        let scalar_elem = !is_heap_ty(&elem_ty);
        let heap_elem =
            is_heap_ty(&elem_ty) && (matches!(elem_ty, Ty::String) || crate::lower::is_value_ty(&elem_ty));
        // A `(String, Value)` TUPLE element (the yaml `pairs` shape) — `__list_concat_rc` rc-owns each
        // tuple, freed recursively by `Op::DropListStrValue` (rc_dec the String slot + `$__drop_value` the
        // Value slot, per tuple). The two-heap-field aggregate `DropListStr` cannot express.
        let str_value_elem = matches!(&elem_ty,
            Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String)
                && crate::lower::is_value_ty(&tys[1]));
        if !scalar_elem && !heap_elem && !str_value_elem {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let arg_exprs = [(**left).clone(), (**right).clone()];
        let args = match self.lower_call_args(&arg_exprs) {
            Ok(a) => a,
            Err(_) => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let dst = self.fresh_value();
        let name = if scalar_elem { "__list_concat" } else { "__list_concat_rc" };
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: name.to_string(),
            args,
            result: Some(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        });
        // Mark the heap-element result for the correct RECURSIVE drop (DropListValue per `$__drop_value`
        // for Value, DropListStr per-slot rc_dec for String) so scope-end / loop teardown frees each
        // owned element — the leak-safety the cert-invisible per-element rc_inc relies on the drop for.
        if heap_elem {
            if crate::lower::is_value_ty(&elem_ty) {
                self.value_elem_lists.insert(dst);
            } else {
                self.heap_elem_lists.insert(dst);
            }
        } else if str_value_elem {
            self.str_value_elem_lists.insert(dst);
        }
        Some(dst)
    }

    /// Lower a STRING INTERPOLATION `"…${e}…"` to a FRESH owned String, byte-matching
    /// v0 (`emit_string_interp`), via the proven `__str_concat` self-host runtime.
    ///
    /// MODEL: the UNIFORM [`crate::lower::desugar_string_interp`] folds the K parts into
    /// a LEFT-nested `BinOp::ConcatStr` tree seeded by `""`, each part wrapped in its
    /// type's `to_string` (a Lit/String part is a no-call leaf; an Int → `int.to_string`,
    /// a Bool → `bool.to_string`, a Float/compound → `<module>.to_string`). This routine
    /// then lowers that tree through the EXISTING [`Self::try_lower_concat_str`] — the
    /// same path the `+` operator uses. Concatenating with a leading `""` is byte-
    /// identical to v0 (`"" ++ bytes == bytes`), so the rendered String matches v0 in
    /// EVERY position (bind / call-arg / tail / concat-operand / match-arm), and the
    /// caller owns the fresh result exactly like any `try_lower_concat_str` value.
    ///
    /// THE GATE-EXACTNESS INVARIANT (why this never regresses caps): the desugar admits a
    /// part ONLY when its leaf lowers to exactly one `CallFn` (a pure `module.to_string`)
    /// or a no-call passthrough, so `try_lower_concat_str` CANNOT roll back here. The
    /// corpus gate's `count_ir_calls` counts the call NODES of the SAME desugared tree,
    /// so `mir_calls == ir_calls` for the interp's contribution BY CONSTRUCTION — no
    /// `mir > ir` (forbidden), no spurious `ir > mir` taint. A part with no admitted
    /// `to_string` module (a Tuple/Record/variant) makes the desugar return `None`; the
    /// interp then stays the deferred `Alloc{Opaque}` (credited 0 by the gate), fully
    /// memory-safe. A Float/compound part DESUGARS but its `to_string` is UNLINKED, so
    /// the enclosing function emits an unlinked call and the RENDER WALL rejects it — it
    /// is out of profile and cannot be a `count != lower` mismatch.
    pub(crate) fn try_lower_string_interp(&mut self, parts: &[IrStringPart]) -> Option<ValueId> {
        // The desugar decides, per record/tuple part, EXPAND (a STATICALLY-expandable Var — a
        // materialized-aggregate binding with displayable fields → the recursive Display tree,
        // byte-matching v0) vs WRAP (any other aggregate → ONE unlinked `compound.to_string`, so
        // the function walls at render). The SAME static predicate (`aggregate_part_expandable`)
        // drives the corpus gate's `interp_synthetic_call_names`, so the synthetic call COUNT the
        // gate credits equals the one this lowering emits BY CONSTRUCTION.
        //
        // SAFETY GATE: "expandable" is a STATIC over-approximation (a `Var` need not denote a
        // materialized block — e.g. `let p = f()` is an Opaque call result). Reading its fields
        // would print garbage. So when the desugar WOULD expand a part but the var is NOT in
        // `materialized_aggregates` at lowering time, route the WHOLE interp to the compound WALL
        // — padded to the gate's synthetic-call count so `mir == ir` still holds (the extra calls
        // are pure elided markers; the one unlinked `compound.to_string` walls the function).
        if self.first_unmaterialized_expand_part(parts) {
            return Some(self.lower_interp_compound_wall(parts));
        }
        let tree = crate::lower::desugar_string_interp(parts, &self.record_layouts)?;
        self.try_lower_concat_str(&tree)
    }

    /// Is there a record/tuple part the desugar would EXPAND (statically `aggregate_part_expandable`)
    /// but whose Var is NOT actually a materialized aggregate at lowering time — so its field reads
    /// would be garbage? `false` when every would-expand part is genuinely materialized (the fold is
    /// safe). When `true`, the caller routes the whole interp to the count-padded compound wall.
    fn first_unmaterialized_expand_part(&self, parts: &[IrStringPart]) -> bool {
        parts.iter().any(|p| {
            let IrStringPart::Expr { expr } = p else { return false };
            if !crate::lower::aggregate_part_expandable(expr, &self.record_layouts) {
                return false;
            }
            let materialized = match &expr.kind {
                IrExprKind::Var { id } => self
                    .value_of
                    .get(id)
                    .is_some_and(|v| self.materialized_aggregates.contains(v)),
                _ => false,
            };
            !materialized
        })
    }

    /// Lower an interpolation whose statically-expandable record/tuple part is NOT materialized at
    /// runtime: route to ONE unlinked `compound.to_string` (the result — walls the function at
    /// render, so its bytes never run) PLUS `pad` pure elided markers so the MIR call count EQUALS
    /// the gate's `interp_synthetic_call_names` count for this interp (`mir == ir`, no false caps
    /// taint, no forbidden `mir > ir`). The markers (`__str_concat` / dotted `to_string`) reach no
    /// Stdout. The returned `dst` is tracked by the CALLER (like `try_lower_concat_str`).
    fn lower_interp_compound_wall(&mut self, parts: &[IrStringPart]) -> ValueId {
        // The gate counts this interp's synthetic calls assuming the expand happens. We emit ONE
        // real `compound.to_string` + (gate_count - 1) pure markers so the totals match exactly.
        let gate_count = crate::lower::interp_synthetic_call_names(parts, &self.record_layouts).len();
        let mut emitted = 0usize;
        let dst = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: "compound.to_string".to_string(),
            args: Vec::new(),
            result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        });
        emitted += 1;
        while emitted < gate_count {
            self.ops.push(Op::CallFn {
                dst: None,
                name: "__str_concat".to_string(),
                args: Vec::new(),
                result: None,
            });
            emitted += 1;
        }
        dst
    }

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
                        // A CAPTURING lambda has no liftable form, so it would materialize a
                        // deferred `Init::Opaque` (an EMPTY closure env) and pass it to the
                        // callee, which would invoke garbage = a SILENT MISCOMPILE. Reject.
                        None => {
                            return Err(LowerError::Unsupported(
                                "capturing lambda in a call-argument position cannot be lifted \
                                 (would pass an empty deferred closure env)"
                                    .into(),
                            ))
                        }
                    }
                }
                // A STRING INTERPOLATION argument (`println("x=${n}")`, `f("hi ${s}")`)
                // over the executable subset — lowered to a fresh owned String via the
                // __str_concat chain, borrowed into the call and dropped at scope end
                // (cert `i` + `d`, identical to a materialized heap-literal arg). A
                // compound/call-operand interp returns None and falls through to the
                // deferred `Alloc{Opaque}` below (its inner calls recorded as elided),
                // unchanged. (This is the highest-traffic interp position — every
                // `println("…${x}…")` real program uses it.)
                IrExprKind::StringInterp { parts } => {
                    let repr = repr_of(&a.ty)?;
                    match self.try_lower_string_interp(parts) {
                        Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                        // A non-lowerable interp as a call ARGUMENT would materialize a
                        // deferred `Init::Opaque` (an EMPTY String) and BORROW it into the
                        // call — the callee reads zero bytes = a SILENT MISCOMPILE. Reject
                        // explicitly so the enclosing function walls cleanly instead of
                        // emitting wrong output.
                        None => {
                            return Err(LowerError::Unsupported(
                                "non-lowerable string interpolation in a call-argument position \
                                 (would borrow an empty deferred String)"
                                    .into(),
                            ))
                        }
                    }
                }
                // An Option/Result CONSTRUCTOR argument (`f(Some(8))`, `g(Ok(y))`,
                // `h(Err("e"))`, `k(None)`) materializes a REAL tagged block via
                // `try_lower_option_ctor` — the SAME `OptSome`/`OptNone`/DynListStr-Result
                // blocks a `let o = Some(8)` builds (len-as-tag, scalar payload moved in /
                // owned heap Err) — borrowed into the call and dropped at scope end via
                // `materialized_call_arg`: cert `i` (alloc) + `d` (drop), identical to the
                // verified fresh-heap bind. Outside that subset (a heap payload it declines,
                // e.g. a borrowed-param `Some(p)`) it WALLs — never the `Init::Opaque` empty
                // value the grouped arm below would build (which a callee reads as zero
                // bytes = a silent miscompile).
                IrExprKind::OptionSome { .. }
                | IrExprKind::OptionNone
                | IrExprKind::ResultOk { .. }
                | IrExprKind::ResultErr { .. } => {
                    let repr = repr_of(&a.ty)?;
                    match self.try_lower_option_ctor(a, &a.ty) {
                        Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                        None => {
                            return Err(LowerError::Unsupported(format!(
                                "{} argument cannot be faithfully materialized in this brick \
                                 (a heap payload outside the executable subset)",
                                kind_name(&a.kind)
                            )))
                        }
                    }
                }
                // A RECORD literal argument (`f(P { x: 3, y: 4 })`) materializes the real
                // layout block via `try_lower_record_construct` (the SAME block a `let p =
                // P{..}` builds — scalar fields stored, heap fields moved in), borrowed into
                // the call and dropped at scope end via `materialized_call_arg`: cert `i`
                // (alloc) + `d` (drop), identical to the verified fresh-heap bind. Outside the
                // subset (a heap-returning-call field) it WALLs — never an `Init::Opaque` empty.
                IrExprKind::Record { .. } => {
                    let repr = repr_of(&a.ty)?;
                    // heap-field records via `try_lower_record_construct`; all-scalar-field
                    // records (`Point { x, y }`) via `try_lower_scalar_record_construct`.
                    match self
                        .try_lower_record_construct(a)
                        .or_else(|| self.try_lower_scalar_record_construct(a))
                    {
                        Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                        None => {
                            return Err(LowerError::Unsupported(
                                "record argument cannot be faithfully materialized in this \
                                 brick (a field outside the executable subset)"
                                    .into(),
                            ))
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
                | IrExprKind::SpreadRecord { .. }
                | IrExprKind::Tuple { .. }
                // A CLOSURE value argument (`register((x) => …)`): a fresh heap env,
                // materialized + borrowed into the call. The callee borrows it per the
                // borrow-by-default convention; its body's calls are elided ⇒ the gate
                // taints the function caps-unverified (invocation caps unknown).
                // (A NON-CAPTURING `Lambda` arg is intercepted BELOW and lifted to a scalar
                // FuncRef slot passed by value — `list.map(xs, (x) => x + 1)`; only a
                // capturing one reaches this deferred Opaque arm.)
                | IrExprKind::ClosureCreate { .. } => {
                    let repr = repr_of(&a.ty)?;
                    // A NON-EMPTY `List[String]` (or scalar-aggregate-element) LITERAL arg
                    // (`f(["a", "b"])`) materializes the REAL nested-ownership DynListStr via the
                    // same builder the RETURN position uses (each element moved/Dup'd in), borrowed
                    // into the call + dropped at scope end by DropListStr (cert `i` + recursive `d`).
                    // Without this it fell to `alloc_init` → `Init::Opaque` empty list = rejected as
                    // a silent miscompile below. (An empty/`List[Value]`/computed list still defers
                    // to `alloc_init`, unchanged — the foundation for heap-element-list call args.)
                    if matches!(&a.kind, IrExprKind::List { .. }) {
                        if let Some(dst) = self.try_lower_str_list_literal(a) {
                            out.push(self.materialized_call_arg(dst, repr, &a.ty));
                            continue;
                        }
                    }
                    let init = alloc_init(a);
                    // `alloc_init` faithfully materializes a string literal and a scalar-
                    // literal list/tuple; every other constructor (Map/Record/Result/Option/
                    // closure, a computed-element list) yields `Init::Opaque` — an EMPTY heap
                    // value. Borrowing an empty value into the call lets the callee read zero
                    // bytes = a SILENT MISCOMPILE, so reject the unfaithful case explicitly.
                    if matches!(init, Init::Opaque) {
                        return Err(LowerError::Unsupported(format!(
                            "{} argument cannot be faithfully materialized in this brick \
                             (would borrow an empty deferred heap value)",
                            kind_name(&a.kind)
                        )));
                    }
                    let dst = self.fresh_value();
                    self.ops.push(Op::Alloc { dst, repr, init });
                    self.record_elided_calls(a);
                    self.materialized_call_arg(dst, repr, &a.ty)
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
                // `f(a + b)` — a string concat in a CALL-ARG position (also a NESTED `a + b + c`,
                // where `a + b` is the left operand arg). Lower it to the __str_concat call; its
                // fresh owned String is borrowed into the outer call and dropped at scope end.
                IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                    let repr = repr_of(&a.ty)?;
                    match self.try_lower_concat_str(a) {
                        Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                        // A non-lowerable string concat as a call ARGUMENT would borrow an
                        // empty deferred String into the callee = a SILENT MISCOMPILE. Reject.
                        None => {
                            return Err(LowerError::Unsupported(
                                "non-lowerable string concat in a call-argument position \
                                 (would borrow an empty deferred String)"
                                    .into(),
                            ))
                        }
                    }
                }
                // `f(xs + [7])` — a SCALAR-element list concat in a CALL-ARG position. Lower it to
                // the __list_concat call; its fresh owned list is borrowed into the outer call and
                // dropped at scope end. A heap-element list concat (or a non-lowerable operand)
                // returns None and falls to the deferred Opaque.
                IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                    let repr = repr_of(&a.ty)?;
                    match self.try_lower_concat_list(a) {
                        Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                        // A non-lowerable list concat (heap-element / non-lowerable operand) as a
                        // call ARGUMENT would borrow an empty deferred list = a SILENT MISCOMPILE.
                        None => {
                            return Err(LowerError::Unsupported(
                                "non-lowerable list concat in a call-argument position \
                                 (would borrow an empty deferred list)"
                                    .into(),
                            ))
                        }
                    }
                }
                // `f(opt ?? default)` — a `??` over a self-host materialized Option in a CALL-ARG
                // position (`int.to_string(list.get(xs, i) ?? 0)` / `println(list.get(ss, i) ?? "d")`
                // — extremely common). The let-bind path executes this via
                // `try_lower_option_unwrap_or`; the arg position must too, else the Option call
                // deferred to a bare elided-call marker (wrong arity → invalid wasm). A SCALAR result
                // passes by value; a HEAP-String result (`option.unwrap_or_str` — a fresh owned
                // String, tracked for scope-end drop by the helper) passes as a borrowed Handle. A
                // non-String-heap / non-Option operand returns None and defers below.
                IrExprKind::UnwrapOr { expr, fallback } => {
                    let mark = self.ops.len();
                    let lhh_mark = self.live_heap_handles.len();
                    match self.try_lower_option_unwrap_or(expr, fallback, true) {
                        Some(v) if is_heap_ty(&a.ty) => CallArg::Handle(v),
                        Some(v) => CallArg::Scalar(v),
                        None => {
                            self.ops.truncate(mark);
                            self.live_heap_handles.truncate(lhh_mark);
                            if is_heap_ty(&a.ty) {
                                // A non-lowerable `??` with a HEAP result as a call ARGUMENT
                                // would borrow an empty deferred heap value = a SILENT
                                // MISCOMPILE. Reject. (A SCALAR `??` falls to the deferred
                                // `Const` 0 below — the separate silent-zero class, left as-is.)
                                return Err(LowerError::Unsupported(
                                    "non-lowerable `??` with a heap result in a call-argument \
                                     position (would borrow an empty deferred heap value)"
                                        .into(),
                                ));
                            }
                            let dst = self.fresh_value();
                            self.record_elided_calls(a);
                            self.ops.push(Op::Const { dst });
                            CallArg::Scalar(dst)
                        }
                    }
                }
                // A scalar-result `match` over a HEAP subject must EXECUTE: a VARIANT
                // (Option/Result) via the tag-read value-match, a scalar-pattern subject via
                // the desugared if-chain. If it falls outside the executable subset (e.g. a
                // `match s { "a" => 1, _ => 9 }` over a String — string equality is not yet
                // lowered) a Const-0 fallback would SILENTLY pick a wrong arm, so WALL it. The
                // executing forms (`match o`/`match list.get(..)`/`match n { 1 => .. }`)
                // return a real `CallArg::Scalar` here.
                IrExprKind::Match { subject, .. }
                    if !is_heap_ty(&a.ty) && is_heap_ty(&subject.ty) =>
                {
                    let mark = self.ops.len();
                    match self.lower_scalar_value(a) {
                        Some(v) => CallArg::Scalar(v),
                        None => {
                            self.ops.truncate(mark);
                            return Err(LowerError::Unsupported(
                                "scalar-result match over a heap subject in a call-argument \
                                 position outside the executable subset cannot be faithfully \
                                 computed (a Const-0 would silently pick a wrong arm) not in \
                                 this brick"
                                    .into(),
                            ));
                        }
                    }
                }
                // A fresh BinOp/UnOp result as an argument (`f(a + b)`, `f(-n)`), or an
                // ERROR OPERATOR result (`f(x!)`, `f(x ?? d)`, `f(x?.field)`): a fresh
                // computed value — a heap result is materialized via `Alloc` (borrowed
                // and dropped), a scalar result is a `Const`. Operands carry their own
                // ownership; the operator's value (and any early-return) is deferred.
                // An `f(x!)` (Unwrap — effect-fn error propagation) as a call ARGUMENT was a
                // deferred `Const`/`Opaque` = a SILENT MISCOMPILE (`f(int.parse(s)!)` passed 0).
                // The faithful lowering needs early-return-on-Err (a later brick); until then
                // WALL it — NEVER pass a silently-wrong value (the ② cardinal rule).
                IrExprKind::Unwrap { .. } => {
                    return Err(LowerError::Unsupported(
                        "unwrap `!` in a call-argument position cannot be faithfully computed \
                         (needs early-return propagation; a Const/Opaque would be a silently \
                         wrong value) not in this brick"
                            .into(),
                    ));
                }
                IrExprKind::BinOp { .. }
                | IrExprKind::UnOp { .. }
                | IrExprKind::Try { .. }
                // (UnwrapOr is handled in full — scalar + heap — by the dedicated arm above.)
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
                        // A heap-result operator / branch as a call ARGUMENT (`f(a ++ b)`
                        // unlowered, `f(if c then "a" else "b")`, `f(0..n)`) would borrow an
                        // empty deferred heap value into the callee = a SILENT MISCOMPILE.
                        return Err(LowerError::Unsupported(format!(
                            "heap-result {} in a call-argument position cannot be faithfully \
                             computed in this brick (would borrow an empty deferred heap value)",
                            kind_name(&a.kind)
                        )));
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
                        // A non-var container (`f().x`) cannot be aliased (no single `src` to
                        // `Dup`); the deferred Opaque empty value borrowed into the callee is a
                        // SILENT MISCOMPILE, so a failed extraction rejects here.
                        let dst = self.lower_heap_extraction(a)?;
                        // A precise heap-field BORROW (`b.label`) is in `param_values` — the
                        // container owns it, so it is passed by Handle WITHOUT joining the
                        // scope-end drop set (no second owner, no double-free). A container-
                        // grain Dup / deferred Opaque is a fresh owned temp → tracked normally.
                        if self.param_values.contains(&dst) {
                            CallArg::Handle(dst)
                        } else {
                            self.materialized_call_arg(dst, repr, &a.ty)
                        }
                    } else {
                        // A SCALAR extraction (`r.x`, `t.0`, `xs[i]`) — load the REAL field /
                        // element value from the block's layout slot when the container is a
                        // materialized scalar aggregate / a tracked list (the VALUE MODEL).
                        // `lower_scalar_value` dispatches Member/TupleIndex to the field load and
                        // IndexAccess to the bounds-checked `$elem_addr` load. Outside that subset
                        // (a non-var / heap-field-aggregate container, or a computed container
                        // `g().field`) it rolls back to a deferred `Const` copy with the
                        // container's calls elided (the caps fold then sees them), as before.
                        let mark = self.ops.len();
                        match self.lower_scalar_value(a) {
                            Some(v) => CallArg::Scalar(v),
                            None => {
                                self.ops.truncate(mark);
                                // A scalar field access on a COMPUTED CALL result (`mk(5).x`)
                                // — the call result is not a tracked aggregate, so a Const-0
                                // reads a WRONG value (and the record-returning callee now
                                // renders, making it observable). WALL it. A tracked-Var
                                // container (`r.x`) lowered above and never reaches here; other
                                // computed containers keep the deferred Const (unchanged).
                                if let IrExprKind::Member { object, .. } = &a.kind {
                                    if matches!(object.kind, IrExprKind::Call { .. }) {
                                        return Err(LowerError::Unsupported(
                                            "scalar field access on a computed call result \
                                             cannot be faithfully computed in this brick (a \
                                             Const-0 would read a wrong value) not in this brick"
                                                .into(),
                                        ));
                                    }
                                }
                                let dst = self.fresh_value();
                                self.ops.push(Op::Const { dst });
                                self.record_elided_calls(a);
                                CallArg::Scalar(dst)
                            }
                        }
                    }
                }
                // A custom-variant CONSTRUCTOR argument (`val(Num(7))`, `f(Eof)`) — NOT a
                // function call: materialize the tagged value-model block (tag@slot0 + scalar
                // fields@slot1..) via `try_lower_variant_ctor`, borrowed into the call and
                // dropped at scope end (cert `i` + `d`, like the record-literal arg above).
                // Must PRECEDE the generic Named-call arm, which would emit a dangling
                // `CallFn "Num"` (an unlinked call = invalid wasm). Outside the subset (a
                // heap/recursive ctor field — ADT brick 5) it WALLs, never a wrong-bytes block.
                IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
                {
                    let repr = repr_of(&a.ty)?;
                    match self.try_lower_variant_ctor(a) {
                        Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                        None => {
                            return Err(LowerError::Unsupported(format!(
                                "variant constructor `{}` argument cannot be faithfully \
                                 materialized in this brick (a heap/recursive field — ADT brick 5)",
                                name.as_str()
                            )))
                        }
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
                    let arg = self.materialized_call_arg(dst, repr, &a.ty);
                    // A user function returning Option/Result yields a REAL same-layout variant
                    // block (an in-profile `-> Option[T]` callee returns `OptSome`/`OptNone`,
                    // a `-> Result[..]` the DynListStr — the v1 calling convention, the SAME
                    // evidence as a variant PARAM). Seed the READ-shape so a `match`/`??` over
                    // this owned call result EXECUTES (reads the tag) instead of WALLing/deferring.
                    // Ownership is unchanged — `materialized_call_arg` already registered the
                    // scope-end drop; `seed_variant_param` adds only layout knowledge.
                    if is_variant_ty(&a.ty) {
                        self.seed_variant_param(dst, &a.ty);
                    }
                    arg
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
                    self.materialized_call_arg(dst, repr, &a.ty)
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
                    if is_heap_ty(&a.ty) {
                        // An unresolvable `Method`/`Computed` call with a HEAP result as a
                        // call ARGUMENT (`f(obj.m())`, `f((g)())`) would borrow an empty
                        // deferred heap value into the callee = a SILENT MISCOMPILE. Reject.
                        // (A SCALAR result still defers to `Const` 0 below — silent-zero class.)
                        return Err(LowerError::Unsupported(
                            "unresolvable method/computed call with a heap result in a \
                             call-argument position (would borrow an empty deferred heap value)"
                                .into(),
                        ));
                    }
                    // C1 DIRECT-CALL INLINE: a SCALAR-result `Computed` call `f(x)` whose callee
                    // is a statically-known let-bound INLINE lambda is DEFUNCTIONALIZED to its
                    // inlined body (`try_lower_scalar_call`'s Computed arm). This EXECUTES
                    // `int.to_string(f(1))` (= 3 for `let f = (x) => string.len(s) + x`) instead
                    // of the deferred `Const 0` silent-zero below. `try_lower_scalar_call` is
                    // rollback-safe (restores ops + handles on a miss), so a non-inlinable
                    // Method/Computed callee falls through to the deferred `Const` exactly as
                    // before — the caps fold still tags it via `record_elided_calls`.
                    let mark = self.ops.len();
                    if let Some(v) = self.try_lower_scalar_call(a, &a.ty) {
                        CallArg::Scalar(v)
                    } else {
                        self.ops.truncate(mark);
                        let dst = self.fresh_value();
                        self.record_elided_calls(a);
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
                // C1 DIRECT-CALL INLINE: a `f(args)` whose callee `f` is a statically-known
                // let-bound INLINE lambda is DEFUNCTIONALIZED — the body is lowered inline with
                // params bound to args, captures resolved through `value_of`. Tried FIRST (a
                // capturing lambda has no FuncRef slot; even a liftable one prefers inline — a
                // direct call edge is more sound for caps than a CallIndirect). Returns None →
                // the CallIndirect / defer path below.
                if !is_heap_ty(ty) {
                    if let Some(v) = self.try_inline_direct_lambda_call(callee, args, ty) {
                        return Some(v);
                    }
                    // The inline attempt rolls itself back on failure (its own marks), so the
                    // op stream is clean here for the CallIndirect fallback.
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                }
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

    /// C1 DIRECT-CALL INLINE — defunctionalize a `f(args)` whose callee `f` is a
    /// statically-known let-bound INLINE lambda (`let f = (x) => body`). The body is
    /// lowered INLINE with each param bound to its lowered argument value; the lambda's
    /// CAPTURES (free vars like the `s` in `(x) => string.len(s) + x`) resolve through the
    /// EXISTING `value_of` map — they are in scope at the call site, so no env block and no
    /// substitution are needed. NO runtime closure, NO `CallIndirect`, NO lifted function:
    /// a static call graph, the inlined body's calls are REAL IR call nodes the caps fold
    /// and `count_ir_calls` see in place.
    ///
    /// SCALAR result only (this slice): the body lowers via `lower_scalar_value` (a
    /// Var/literal/arith/scalar-call/`string.len`-style pure-module call), which is
    /// rollback-safe (it restores `ops` + `live_heap_handles` on a partial miss). A heap
    /// result, or a body the scalar subset cannot lower (a side effect, a heap op), returns
    /// `None` and the caller keeps the existing CallIndirect / deferred path. Each ARGUMENT
    /// is lowered as a scalar value (a literal/Var/arith); a non-scalar-lowerable arg →
    /// `None` (defer). Self-contained marks make a partial attempt fully reversible.
    ///
    /// SOUNDNESS: param binding is `value_of[param] = arg_value` (a pure local rebind, no
    /// ownership event — a SCALAR arg carries none); the body lowers exactly as if its
    /// statements/expr were written at the call site. The captures are BORROWED through
    /// `value_of` (no new owner — the enclosing binding still owns `s`, dropped once at its
    /// own scope end), so no double-free. NB: a parameter VarId is UNIQUE per lambda (the
    /// frontend assigns fresh VarIds), so binding it cannot clobber a live caller local.
    fn try_inline_direct_lambda_call(
        &mut self,
        callee: &IrExpr,
        args: &[IrExpr],
        ty: &Ty,
    ) -> Option<ValueId> {
        // The callee must be a Var statically bound to a recorded inline lambda.
        let callee_var = match &callee.kind {
            IrExprKind::Var { id } => *id,
            _ => return None,
        };
        let (params, body) = self.lambda_bindings.get(&callee_var)?.clone();
        if params.len() != args.len() {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // Lower each ARGUMENT to a scalar value, then bind the param to it. (A heap arg is
        // out of this slice — it would need owned/borrow tracking; defer.)
        for ((pvar, pty), arg) in params.iter().zip(args.iter()) {
            if is_heap_ty(pty) {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
            match self.lower_scalar_value(arg) {
                Some(v) => {
                    self.value_of.insert(*pvar, v);
                }
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            }
        }
        // Lower the lambda BODY inline as a scalar value (captures resolve through value_of).
        match self.lower_scalar_value(&body) {
            Some(v) if !is_heap_ty(ty) => Some(v),
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
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
    /// Lower a SCALAR field/element PROJECTION (`r.x`, `t.0`) to a real `Prim::Load`
    /// at the field's layout slot — the v1 VALUE MODEL read side. Returns the loaded
    /// scalar `dst`, or `None` (defer/wall) when the projection is not in the
    /// materialized subset:
    ///   - the container is not a TRACKED heap var (`f().x`, a nested `a.b.c` — no
    ///     single block to load from),
    ///   - the container's type is not a SCALAR-only record/tuple (a heap-field
    ///     aggregate is constructed as a deferred `Opaque`, whose slots are NOT the
    ///     layout offsets, so loading would read garbage — walled instead),
    ///   - the field is heap-typed (a String field — handled by the container-grain
    ///     `lower_heap_extraction`, not a scalar load).
    ///
    /// SOUNDNESS: a pure `Prim::Load` reads a copy of the scalar — no ownership event
    /// (the container keeps its single reference, dropped once at scope end). The gate
    /// on a MATERIALIZED scalar-aggregate container is what makes the offset correct:
    /// a deferred `Opaque` record never reaches here (its type would still be a
    /// scalar-aggregate, but it was never built with field stores — see below).
    /// The DECLARATION-ordered scalar field types of an aggregate container type, for
    /// the VALUE MODEL: a `Ty::Record`/`Ty::Tuple` is structural (used directly), a
    /// `Ty::Named(name, args)` is resolved via the [`LowerCtx::record_layouts`] registry,
    /// substituting the declared generic params with `args` (so a `Box[Int]` field
    /// `value: T` is sized as `Int` — the #650 instantiated-layout concern). Returns
    /// `None` for a non-aggregate / unregistered / arity-mismatched type (the caller
    /// then walls). The field NAMES are returned alongside so a `.field` access can find
    /// its index; a tuple has positional "fields" so its names are empty.
    pub(crate) fn aggregate_field_tys(&self, ty: &Ty) -> Option<(Vec<almide_lang::intern::Sym>, Vec<Ty>)> {
        match ty {
            Ty::Record { fields } => {
                Some((fields.iter().map(|(n, _)| *n).collect(), fields.iter().map(|(_, t)| t.clone()).collect()))
            }
            Ty::Tuple(elems) => Some((Vec::new(), elems.clone())),
            Ty::Named(name, args) => {
                let (generics, decl_fields) = self.record_layouts.get(name.as_str())?;
                // Substitute the declared generic params (`T`, `A`, …) with the concrete
                // `args` from the instantiated type. A param with no supplied arg (arity
                // mismatch) is a resolution failure → wall.
                let mut subst: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
                    std::collections::HashMap::new();
                for (g, a) in generics.iter().zip(args.iter()) {
                    subst.insert(*g, a.clone());
                }
                let names = decl_fields.iter().map(|(n, _)| *n).collect();
                let tys = decl_fields
                    .iter()
                    .map(|(_, t)| subst_type_var(t, &subst))
                    .collect();
                Some((names, tys))
            }
            _ => None,
        }
    }

    /// The uniform-slot BYTE OFFSET of a named field, resolving the concrete field types
    /// first — NOT walling a heap-field aggregate (the layout is one i64 slot per field
    /// regardless of field-ness, so a heap field's slot is at the same
    /// `BLOCK_HEADER + idx*SLOT_SIZE` a scalar field's is). A SCALAR read at this offset
    /// (`r.n` of `{name: String, n: Int}`) loads its value; a heap read (`b.label`) loads
    /// the slot's owned handle. `None` if `ty` is unresolvable or has no such field.
    pub(crate) fn aggregate_field_offset_any(&self, ty: &Ty, field: &str) -> Option<u32> {
        let (names, _tys) = self.aggregate_field_tys(ty)?;
        let idx = names.iter().position(|n| n.as_str() == field)?;
        Some(layout::slot_offset(idx))
    }

    /// The uniform-slot BYTE OFFSET of a tuple element by index, NOT walling a heap-element
    /// tuple (the tuple sibling of [`Self::aggregate_field_offset_any`]).
    pub(crate) fn aggregate_index_offset_any(&self, ty: &Ty, index: usize) -> Option<u32> {
        if !matches!(ty, Ty::Tuple(_)) {
            return None;
        }
        let (_, tys) = self.aggregate_field_tys(ty)?;
        if index >= tys.len() {
            return None;
        }
        Some(layout::slot_offset(index))
    }

    /// Resolve an aggregate CONTAINER expression to the i64 BYTE-ADDRESS of its block (the base
    /// for a `base + slot_offset` field load). A `Var` bound to a tracked heap aggregate (or a
    /// param-bound aggregate) is `Prim::Handle`'d directly. A NESTED aggregate field (`o.p` in
    /// `o.p.x`) is borrowed via `try_lower_heap_field_borrow` (the loaded inner-block handle) then
    /// `Prim::Handle`'d — so field access composes to arbitrary depth over materialized blocks.
    /// `None` for a non-resolvable container (`f().x`, a non-materialized var) → the caller defers.
    pub(crate) fn resolve_aggregate_container_handle(&mut self, container: &IrExpr) -> Option<ValueId> {
        use crate::PrimKind;
        let block = match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => self.value_or_global(*id).ok()?,
            // A nested aggregate field — borrow its loaded inner-block handle. Gated on the
            // OUTER container being materialized (inside `try_lower_heap_field_borrow`), so a
            // garbage slot is never dereferenced.
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                if is_heap_ty(&container.ty) =>
            {
                self.try_lower_heap_field_borrow(container)?
            }
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![block] });
        Some(h)
    }

    pub(crate) fn lower_scalar_field_access(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        // Scalar result only (the caller's contract; a heap field defers to the
        // container-grain extraction).
        if is_heap_ty(&expr.ty) {
            return None;
        }
        // Use the NON-WALLING offset: a SCALAR field of a MIXED heap-field record/tuple is
        // at the same uniform slot a scalar-only record's is (one i64 slot per field), so
        // `R { name: String, n: Int }.n` reads slot 1 correctly. The result is scalar
        // (guarded above), so loading it (load64) is right regardless of the OTHER fields'
        // heap-ness; the only requirement is the container is materialized with this layout
        // (the tracked-heap-var guard below), which a heap-field record now is.
        let (container, offset) = match &expr.kind {
            IrExprKind::Member { object, field } => {
                (object, self.aggregate_field_offset_any(&object.ty, field.as_str())?)
            }
            IrExprKind::TupleIndex { object, index } => {
                (object, self.aggregate_index_offset_any(&object.ty, *index)?)
            }
            _ => return None,
        };
        // Resolve the container to a block handle: a TRACKED heap var (a `try_lower_*_construct`
        // block or a param-bound aggregate), OR a NESTED aggregate field (`o.p` of `o.p.x`) whose
        // borrowed handle points to the inner block. A non-resolvable container (`f().x`) → defer.
        let h = self.resolve_aggregate_container_handle(container)?;
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset as i64 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
        let dst = self.fresh_value();
        // Uniform i64 slot — `Load { width: 8 }`. The stored scalar (any width) round-
        // trips losslessly: a narrow Int8 stored as the full i64 value reads back exact.
        self.ops.push(Op::Prim {
            kind: PrimKind::Load { width: 8 },
            dst: Some(dst),
            args: vec![addr],
        });
        Some(dst)
    }

    /// Lower a SCALAR direct index `xs[i]` (`xs: List[Int/Float/Bool]`, scalar i64 element slots)
    /// to a bounds-checked element load: `prim.handle(xs)` → `$elem_addr(list, i)` (the preamble
    /// helper that TRAPs on a negative / `>= cap` index — v0's `a[i]` likewise halts on OOB) →
    /// `Load { width: 8 }` of the i64 slot. The element round-trips losslessly (a narrow Int8 / a
    /// Float's f64 bits read back exact). GATED to a SCALAR result element AND a resolvable heap
    /// container var (a tracked List) AND a lowerable scalar index; a heap-element list (an
    /// i32-handle slot) or an unresolvable container defers to the caller's safe fallback. The
    /// container is BORROWED (read-only handle), no ownership — `lower_scalar_value`'s contract
    /// (only rollback-safe value ops, never an ownership event) holds.
    pub(crate) fn lower_scalar_index_access(
        &mut self,
        object: &IrExpr,
        index: &IrExpr,
        elem_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // Scalar element only — a heap element (List[String]) needs a borrowing LoadHandle path,
        // handled by the heap-extraction lowering, not here.
        if is_heap_ty(elem_ty) {
            return None;
        }
        // The container must be a tracked heap list VAR that is a REAL, POPULATED block (in
        // `materialized_lists` — a literal / heap param / fully-lifted self-host list result) OR a
        // borrowed heap PARAM (the caller passes a genuine list). An Opaque/deferred list (a
        // `list.map` whose param-invoking lambda could not lift → an empty block, cap 0) is NOT
        // admitted: a bounds-checked `$elem_addr` load would TRAP at `xs[0]` (cap 0), a new runtime
        // crash. Such a list defers to the caller's safe `Const 0` fallback (mis-valued, never a trap).
        let list = match &object.kind {
            IrExprKind::Var { id } if is_heap_ty(&object.ty) => {
                let v = self.value_or_global(*id).ok()?;
                if !self.materialized_lists.contains(&v) && !self.param_values.contains(&v) {
                    return None;
                }
                v
            }
            _ => return None,
        };
        let idx = self.lower_scalar_value(index)?;
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list] });
        // $elem_addr(list, idx) — bounds-checked i64 slot address (traps OOB).
        let addr = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::ElemAddr, dst: Some(addr), args: vec![h, idx] });
        let dst = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(dst), args: vec![addr] });
        Some(dst)
    }

    /// Lower a SCALAR (Int/Bool/Float) value expression to a `ValueId` holding its REAL
    /// value, or `None` (the caller then DEFERS to `Const`). SELF-ROLLBACK contract: on a
    /// `None` return this restores BOTH `self.ops` AND `self.live_heap_handles` to their
    /// entry length, so the function leaves NO net side effect when it fails — a caller may
    /// roll back with an `ops`-only truncate (the historic discipline) and still be correct
    /// even though a sub-lowering (a scalar CALL OPERAND, `5 + string.len("abc")`) may
    /// MATERIALIZE a fresh heap argument temp (an `Alloc` registered for a scope-end drop).
    /// On SUCCESS, any such temp stays tracked (it is a genuine value to free at scope end),
    /// exactly as a direct `let _ = string.len("abc")` bind tracks it. The actual lowering
    /// is [`Self::lower_scalar_value_inner`].
    pub(crate) fn lower_scalar_value(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        match self.lower_scalar_value_inner(expr) {
            Some(v) => Some(v),
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
        }
    }

    /// Lower a scalar operand of an EAGER `UnOp` (`not e`) or logical `And`/`Or`, FREEING any
    /// transient heap temp the operand materializes WITHIN a local frame. The canonical case is
    /// `c == "'"` (→ `string.eq(c, "'")`): the `"'"` literal is a fresh owned String that is dead
    /// the instant the `Bool` is computed, so it is `Alloc`'d (cert `i`) and `Drop`'d (cert `d`)
    /// LOCALLY here — the operand is internally balanced and registers NO temp in the enclosing
    /// frame. This is SOUND precisely because `and`/`or`/`not` are EAGER in v0 (both operands /
    /// the operand always evaluate, NO short-circuit), so the `Drop` always runs on the same path
    /// as the `Alloc`; the scalar `Bool` result survives the frame teardown (it is not a heap
    /// handle). Returns `None` (fully rolled back) if the operand is not scalar-lowerable. (Before,
    /// a heap-materializing operand was GATED OUT to `None` → the caller fell back to a silent
    /// `Const 0` / a `WALL` — the `not (c == "'" or c == "\"")` miscompile.)
    fn lower_scalar_operand(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let frame = self.live_heap_handles.len();
        match self.lower_scalar_value_inner(expr) {
            Some(v) => {
                // Free any transient temp the operand allocated (e.g. a string-eq literal),
                // keeping the operand internally `i…d`-balanced — the scalar `v` is not among them.
                self.drop_arm_locals(frame);
                Some(v)
            }
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(frame);
                None
            }
        }
    }

    fn lower_scalar_value_inner(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        match &expr.kind {
            IrExprKind::Var { id } => self.value_or_global(*id).ok(),
            // A SCALAR record field / tuple element (`r.x`, `t.0`) — load from the
            // block's layout slot. Defers (→ None) for a non-materialized container.
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } => {
                self.lower_scalar_field_access(expr)
            }
            // A scalar list element `xs[i]` (`xs: List[Int/Float/Bool]`) — a bounds-checked
            // element load. Defers (→ None) for a heap element (an i32-handle slot) or a
            // non-resolvable container.
            IrExprKind::IndexAccess { object, index } => {
                self.lower_scalar_index_access(object, index, &expr.ty)
            }
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
                // The `**` OPERATOR has no single hardware instruction — it desugars to a CALL into
                // the self-hosted pow stdlib, exactly as if the user wrote `math.fpow(a, b)` /
                // `math.pow(a, b)`. `PowFloat` → `math.fpow` (the bit-exact libm transcription),
                // `PowInt` → `math.pow` (exponentiation-by-squaring). Both callees live in a
                // PURE_MODULES module, so the synthesized `Op::CallFn` carries an EMPTY capability
                // witness (sound), and the corpus `count_ir_calls` credits the operator node 1:1 so
                // `mir_calls <= ir_calls` holds BY CONSTRUCTION (no elision-masking over-count).
                let pow_callee = match op {
                    BinOp::PowFloat => Some("math.fpow"),
                    BinOp::PowInt => Some("math.pow"),
                    _ => None,
                };
                if let Some(callee) = pow_callee {
                    let a = self.lower_scalar_value(left)?;
                    let b = self.lower_scalar_value(right)?;
                    let repr = repr_of(&expr.ty).ok()?;
                    let dst = self.fresh_value();
                    self.ops.push(Op::CallFn {
                        dst: Some(dst),
                        name: callee.to_string(),
                        args: vec![CallArg::Scalar(a), CallArg::Scalar(b)],
                        result: Some(repr),
                    });
                    return Some(dst);
                }
                // FLOAT arithmetic + comparison operators → the prim float floor (Op::Prim). The
                // operands are Float (the i64-uniform f64 bits); the prim reinterprets around the
                // wasm f64 op. Pure scalar — no ownership (cert untouched). This makes float-heavy
                // self-host (libm / dtoa) write `a * b` instead of `prim.fmul(a, b)`.
                let fkind = match op {
                    BinOp::AddFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Add)),
                    BinOp::SubFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Sub)),
                    BinOp::MulFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Mul)),
                    BinOp::DivFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Div)),
                    BinOp::Lt if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Lt)),
                    BinOp::Lte if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Le)),
                    BinOp::Gt if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Gt)),
                    BinOp::Gte if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Ge)),
                    BinOp::Eq if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Eq)),
                    BinOp::Neq if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Ne)),
                    _ => None,
                };
                if let Some(kind) = fkind {
                    let a = self.lower_scalar_value(left)?;
                    let b = self.lower_scalar_value(right)?;
                    let dst = self.fresh_value();
                    self.ops.push(Op::Prim { kind, dst: Some(dst), args: vec![a, b] });
                    return Some(dst);
                }
                // STRING equality (`c == ":"` / `a != b` over String) → the self-host
                // `string.eq` byte-compare call (→ scalar Bool). Both operands are BORROWED
                // heap String handles (the call reads + copies; no ownership event). `!=` is
                // `1 - eq`. This is the dominant real-parser condition; without it the cond
                // silently lowered to 0 (false) — the yaml/char-scan miscompile.
                if matches!(op, BinOp::Eq | BinOp::Neq) && matches!(left.ty, Ty::String) {
                    let args = [(**left).clone(), (**right).clone()];
                    let eq = self
                        .lower_pure_module_value_call("string", "eq", &args, &Ty::Bool)
                        .ok()?;
                    if matches!(op, BinOp::Eq) {
                        return Some(eq);
                    }
                    let one = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: one, value: 1 });
                    let dst = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: one, b: eq });
                    return Some(dst);
                }
                let iop = match op {
                    BinOp::AddInt => crate::IntOp::Add,
                    BinOp::SubInt => crate::IntOp::Sub,
                    BinOp::MulInt => crate::IntOp::Mul,
                    BinOp::DivInt => crate::IntOp::Div,
                    BinOp::ModInt => crate::IntOp::Mod,
                    // Ordering comparisons (the `if` condition) — INT operands only (a
                    // Float compare uses the prim float floor above; a String compare needs
                    // a different op). Gate on the operand type.
                    BinOp::Lt if matches!(left.ty, Ty::Int) => crate::IntOp::Lt,
                    BinOp::Lte if matches!(left.ty, Ty::Int) => crate::IntOp::Le,
                    BinOp::Gt if matches!(left.ty, Ty::Int) => crate::IntOp::Gt,
                    BinOp::Gte if matches!(left.ty, Ty::Int) => crate::IntOp::Ge,
                    // Equality — INT or BOOL operands. A `Bool` is an i64 0/1 (a Var loads
                    // its 0/1, a `LitBool` materializes `ConstInt 0/1` above), so the SAME
                    // `IntOp::Eq`/`Ne` render is bit-exact for `b == false` / `b1 != b2` as
                    // for `n == 0`. (Ordering on Bool is undefined in v0, so it is NOT
                    // admitted; a Float/String/compound `==` still needs a distinct op.)
                    BinOp::Eq if matches!(left.ty, Ty::Int | Ty::Bool) => crate::IntOp::Eq,
                    BinOp::Neq if matches!(left.ty, Ty::Int | Ty::Bool) => crate::IntOp::Ne,
                    // Logical `and`/`or` on Bool operands → EAGER `i64.and`/`i64.or` of the
                    // two lowered Bools (each an i64 0/1: a `LitBool` materializes ConstInt
                    // 0/1, a Var loads its 0/1, a nested compare yields 0/1). This is
                    // BIT-EXACT with v0, which itself evaluates BOTH operands unconditionally
                    // (`emit(left); emit(right); i32.and/i32.or` — NO short-circuit) — so
                    // eager `and`/`or` is the faithful transcription, not an approximation.
                    // 0/1 ∧ 0/1 (resp. ∨) stays in {0,1}, so the result is a valid Bool the
                    // `if` condition / `to_string` reads uniformly. The SOUNDNESS subtlety
                    // (v0 is eager so there is no observable to short-circuit) is moot for a
                    // pure operand; a SIDE-EFFECTING operand (a printing call) would still be
                    // executed once by v0's eager emit, but to keep the cert/effect reasoning
                    // simple we only admit operands that `lower_scalar_value` accepts as a
                    // pure scalar predicate below — a non-lowerable operand returns None
                    // (WALL), never both-arms / never 0.
                    BinOp::And if matches!(left.ty, Ty::Bool) => crate::IntOp::And,
                    BinOp::Or if matches!(left.ty, Ty::Bool) => crate::IntOp::Or,
                    // Pow, Float, concat, non-Int/Bool compares: defer.
                    _ => return None,
                };
                // `and`/`or` admit only PURE operands. v0's eager emit evaluates BOTH
                // unconditionally, so an effect-free operand is bit-exact; but a
                // heap-materializing operand (`is_empty(x) and contains(y, "@")`) would
                // register an owned temp whose consume escapes the enclosing per-arm
                // frame (a dangling `m`). Gate it out → WALL to the sound prior lowering.
                // The arithmetic/comparison ops keep the plain `lower_scalar_value`: by
                // type their operands are Int/Float/Bool scalars that never materialize a
                // heap temp, so the pure-gate would be a no-op there.
                let is_logic = matches!(iop, crate::IntOp::And | crate::IntOp::Or);
                let (a, b) = if is_logic {
                    (self.lower_scalar_operand(left)?, self.lower_scalar_operand(right)?)
                } else {
                    (self.lower_scalar_value(left)?, self.lower_scalar_value(right)?)
                };
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
            // A scalar `if`/`match` as an OPERAND (`a + (if c then 1 else 2)`,
            // `n + match k { 0 => x, _ => y }`): EXECUTE it to a scalar via the same
            // `try_lower_scalar_if` the let-bind path uses — only the taken arm runs. The
            // helper is self-contained: it marks BOTH `ops` and `live_heap_handles`, drops
            // every per-arm heap temp WITHIN its arm (so on success `live_heap_handles` is
            // exactly at entry — no net ownership event), and fully rolls back on a miss. So
            // it honors `lower_scalar_value`'s contract and a caller's `ops`-only truncate
            // stays correct. A heap-RESULT if/match is NOT this path (string `+` is ConcatStr,
            // and a let-bound heap if is the separate escalated-cert path) — it defers.
            IrExprKind::If { cond, then, else_ } if !is_heap_ty(&expr.ty) => {
                self.try_lower_scalar_if(cond, then, else_, &expr.ty)
            }
            IrExprKind::Match { subject, arms } if !is_heap_ty(&expr.ty) => {
                // A CUSTOM variant (user ADT) subject — tag@slot0 dispatch (ADT brick 3).
                if let Some(dst) = self.try_lower_custom_variant_match(subject, arms, &expr.ty) {
                    return Some(dst);
                }
                // A VARIANT (Option/Result) subject — execute via the tag-read value-match
                // (ctor patterns are not `subj == lit`, so `desugar_match_to_if` can't reach
                // them; the result would stay an unset 0 = a silent miscompile).
                if is_heap_ty(&subject.ty) {
                    return self.try_lower_variant_value_match(subject, arms, &expr.ty);
                }
                // The desugared chain may be an `If` (literal arms) OR a `Block` (`{ let x =
                // subj; if … }` for a binder/guarded-binder arm) — `lower_scalar_arm` handles
                // both (its tail-`if`/`match` recursion runs the scalar-if machinery).
                let if_expr = self.desugar_match_to_if(subject, arms, &expr.ty)?;
                self.lower_scalar_arm(&if_expr)
            }
            // A scalar user/stdlib CALL as an OPERAND (`5 + string.len(s)`, `5 +
            // string.len("abc")` after the optimizer inlines a `let s = "abc"`, `g(a) +
            // h(b)`, `string.len(s) > 0`, a nested `f(g(x))`): EXECUTE it via the same
            // `try_lower_scalar_call` the direct-bind path uses. Its argument lowering
            // (`lower_call_args`) materializes/borrows heap args exactly as a bound `let k =
            // call` already does — a heap `Var` is BORROWED (`CallArg::Handle`, no ownership
            // event), a FRESH heap literal is `Alloc`'d into an owned temp released at scope
            // end. The latter pushes to `live_heap_handles`, but the SELF-ROLLBACK wrapper
            // (see `lower_scalar_value`) restores both `ops` and `live_heap_handles` if this
            // (or a sibling operand) later fails, so the materialize is rollback-safe. A
            // Method/Computed/impure-Module callee returns `None` from `try_lower_scalar_call`
            // (rolled back) and DEFERS — honest, the caps fold tags the elided callee. A heap
            // RESULT operand is NOT this path (string `+` is ConcatStr; a let-bound heap if is
            // the separate escalated-cert path) — it is gated out by `!is_heap_ty`.
            IrExprKind::Call { .. } if !is_heap_ty(&expr.ty) => {
                self.try_lower_scalar_call(expr, &expr.ty)
            }
            // A scalar UNARY op (`-a`, `not x`). The operand lowers via the SAME scalar
            // value path (a Var load, a literal, a nested compare/arith) — if it is not
            // scalar-lowerable we return None (WALL/defer), never a silent 0. Previously
            // there was NO UnOp arm here, so EVERY `-a` / `not x` in a value position fell
            // through to `_ => None` → the caller's Const-0 materialization, reading 0; and
            // in an `if` CONDITION the un-lowered cond made `try_lower_scalar_if` /
            // `try_lower_unit_if` run BOTH arms. This arm closes both failures.
            IrExprKind::UnOp { op, operand } => {
                use almide_ir::UnOp;
                // The operand is EAGER (always evaluated), so a `not (c == "'")` whose
                // `string.eq` materializes the `"'"` literal is lowered with that temp
                // `Alloc`'d + `Drop`'d LOCALLY (`lower_scalar_operand`'s frame) — internally
                // `i…d`-balanced, no temp escaping to the enclosing per-arm heap frame. A
                // non-scalar-lowerable operand still WALLS (→ None).
                let x = self.lower_scalar_operand(operand)?;
                let dst = self.fresh_value();
                match op {
                    // Integer negation: `0 - x` (no dedicated wasm i64 negate op; the
                    // IntBinOp Sub renders `i64.sub` of a ConstInt 0 and x, matching v0's
                    // `0i64 - x`). i64::MIN negation overflows identically to v0's wrapping.
                    UnOp::NegInt => {
                        let zero = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
                        self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: zero, b: x });
                        Some(dst)
                    }
                    // Float negation: the existing `f64.neg` prim (the i64-uniform value
                    // holds the f64 bits; the prim reinterprets around `f64.neg` — sign-bit
                    // flip, so `-0.0` and NaN behave exactly as v0's `f64::neg`).
                    UnOp::NegFloat => {
                        self.ops.push(Op::Prim {
                            kind: crate::PrimKind::FloatUn(crate::FUnOp::Neg),
                            dst: Some(dst),
                            args: vec![x],
                        });
                        Some(dst)
                    }
                    // Boolean `not`: a Bool is an i64 0/1, so `1 - b` flips it (b∈{0,1} →
                    // 1-b∈{1,0}). Renders `i64.sub` of ConstInt 1 and b; the result stays in
                    // {0,1}, a valid Bool the `if` condition reads uniformly.
                    UnOp::Not => {
                        let one = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: one, value: 1 });
                        self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: one, b: x });
                        Some(dst)
                    }
                }
            }
            // A SCALAR `??` in a value/operand position (`(int.parse(s) ?? 0) - 48`,
            // `(codepoint(ch) ?? 0)` fed to arithmetic) — execute the unwrap (tag read +
            // payload-or-fallback) via the same machinery the tail/let positions use. Without this
            // arm a `??` operand fell to `_ => None` → the caller's `Const 0`, so the WHOLE BinOp
            // silently read 0 (`(x ?? 0) - 48` → 0, not x-48). Scalar result only — a heap-String
            // `??` is not a scalar value operand. `try_lower_option_unwrap_or` is rollback-safe and
            // emits its own balanced Option materialize/drop, exactly like the scalar-Call arm above.
            IrExprKind::UnwrapOr { expr, fallback } if !is_heap_ty(&fallback.ty) => {
                self.try_lower_option_unwrap_or(expr, fallback, false)
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
        if func == "alloc_list_str" || func == "alloc_set_str" || func == "alloc_map_str" || func == "alloc_map_skv" {
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
            // Generic typed `load_handle[A]` — the same i32-handle-keeping load as `load_str`, for
            // reading a `List[Value]`/`Value` payload out of a Value's slot (the Value model floor).
            "load_handle" => PrimKind::LoadHandle,
            "store32" => PrimKind::Store { width: 4 },
            "store8" => PrimKind::Store { width: 1 },
            "store64" => PrimKind::Store { width: 8 },
            // Raw refcount free/acquire — the Value drop/copy mechanism. GATED to the value-model
            // self-host fns (the trusted recursive-free / shallow-copy, like the inline DropListStr):
            // an UNTRACKED free exposed to arbitrary code would let any fn double-free outside the
            // ownership cert's sight, so only the value-model drop/copy routines may name it: the
            // recursive drop (`__drop_value`, rc_dec), the array shallow-copy (`__varr_copy`, rc_inc),
            // the as_array element-list fill (`__vfill`, rc_inc), and the heap-element list-concat copy
            // (`__lc_copy_rc`, rc_inc — the new list co-owns each appended element, balanced by the
            // source's recursive DropListStr/DropListValue). See docs/roadmap/active/v1-value-model.md.
            "rc_dec" | "rc_inc"
                if matches!(
                    self.fn_name.as_str(),
                    "__drop_value"
                        | "__drop_list_value"
                        | "__svdrop_list"
                        | "__drop_list_str_value"
                        | "__drop_result_lv"
                        | "__varr_copy"
                        | "__vfill"
                        | "__lc_copy_rc"
                ) || self.fn_name.starts_with("__drop_") =>
            {
                // `__drop_*` also covers the GENERATED per-type custom-variant recursive drops
                // (`__drop_Expr`, ADT brick 5b) — the same trusted prim-only free routine.
                if func == "rc_dec" { PrimKind::RcDec } else { PrimKind::RcInc }
            }
            "rc_dec" | "rc_inc" => {
                return Err(LowerError::Unsupported(format!(
                    "prim.{func} is restricted to the value-model drop/copy routines (untracked free)"
                )))
            }
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
            // f32 narrowing/widening (f32 value = its 32-bit pattern in the low half of the i64).
            "f2f32" => PrimKind::F32Demote,
            // `f32_2f` (Float32→Float) and `bits_to_f32` (raw 32-bit pattern→Float) are the SAME
            // f64.promote_f32 over a low-32 f32 pattern.
            "f32_2f" | "bits_to_f32" => PrimKind::F32Promote,
            "i2f32" => PrimKind::IntToF32,
            "f32bits" => PrimKind::F32Bits,
            _ => return Err(LowerError::Unsupported(format!("unknown primitive prim.{func}"))),
        };
        let mut lowered = Vec::with_capacity(args.len());
        for a in args {
            let v = self.lower_scalar_value(a).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} argument is not a lowerable scalar/handle"))
            })?;
            lowered.push(v);
        }
        let dst = if matches!(kind, PrimKind::Store { .. } | PrimKind::RcDec | PrimKind::RcInc) {
            None
        } else {
            Some(self.fresh_value())
        };
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
    /// the call returns); a scalar temp is passed by value. A NESTED-OWNERSHIP temp
    /// (a `List[String]` from `set.from_list(string.split(…))`, etc.) is ALSO recorded
    /// in `heap_elem_lists` so its scope-end drop is the recursive `DropListStr` that
    /// frees the owned element Strings — a flat `Drop` would free only the block and
    /// LEAK the elements (per-iteration in a loop → OOM). Cert is unchanged: one `i`
    /// (alloc) + one `d` (drop) for the temp; DropListStr vs Drop is the runtime
    /// realization of that same single `d`.
    pub(crate) fn materialized_call_arg(&mut self, dst: ValueId, repr: Repr, ty: &Ty) -> CallArg {
        if repr.is_heap() {
            self.live_heap_handles.push(dst);
            if crate::lower::is_heap_elem_list_ty(ty) {
                self.heap_elem_lists.insert(dst);
            }
            // A `Value` call-argument temp (`f(value.array([…]))`, `f(value.str(s))`) drops via the
            // runtime-tag-dispatched `Op::DropValue` (recursive — an Array frees its element Values, a
            // Str its String), NOT a flat `Op::Drop` (which would leak the nested payload). Without
            // this a tag-5 Array / tag-4 Str passed as an argument leaks at the call-site scope end.
            if crate::lower::is_value_ty(ty) {
                self.value_handles.insert(dst);
            }
            CallArg::Handle(dst)
        } else {
            CallArg::Scalar(dst)
        }
    }
}

