//! Core-IR → MIR lowering — the single ownership+layout DECISION pass (§3.1).
//!
//! This is the v1 thesis made real: ONE pass decides, per binding, the
//! ownership (fresh `Alloc` / alias `Dup` / scope-end `Drop` / mutate
//! `MakeUnique`) and the layout ([`Repr`]) — replacing the five scattered
//! codegen passes (`pass_perceus`/`pass_clone`/`pass_borrow_inference`/
//! `pass_capture_clone`/`pass_box_deref`) with a single source the renderers
//! only translate. The produced MIR is checked by [`crate::verify_ownership`].
//!
//! Build order (§6, risk-first): it consumes the EXISTING frontend IR
//! (`almide_ir`) as a temporary feeder so the novel core is validated before
//! the frontend is greenfielded.
//!
//! # Scope of this brick
//! The value-semantics subset, on a LINEAR function body: `Bind` of a fresh
//! heap value (list/record/string literal) or an alias (`var b = a`) or a
//! scalar; `IndexAssign` (copy-on-write `MakeUnique`); scope-end `Drop`s.
//! Anything outside the subset (control flow, calls, …) returns
//! [`LowerError::Unsupported`] — never a silent drop (flight-grade totality).

use crate::purity;
use crate::{CallArg, Init, MirFunction, MirParam, Op, Repr, RtFn, ValueId, PLACEHOLDER_LAYOUT};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrFunction, IrParam, IrPattern, IrStmt, IrStmtKind, VarId,
};
use almide_lang::types::Ty;
use std::collections::{HashMap, HashSet};

/// A lowering could not proceed because the input is outside this brick's
/// subset (or violates a precondition such as concrete types). Carrying the
/// reason keeps the pass TOTAL — no case is silently skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    Unsupported(String),
}

/// Heap-managed types (need refcount: `Alloc`/`Dup`/`Drop`) vs `Copy` scalars.
/// Mirrors the old `pass_perceus::is_heap_type` / `emit_wasm` copy — but here it
/// is the SINGLE definition both renderers will read off the MIR.
pub fn is_heap_ty(ty: &Ty) -> bool {
    !matches!(
        ty,
        Ty::Int
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float
            | Ty::Float32
            | Ty::Float64
            | Ty::Bool
            | Ty::Unit
            | Ty::Never
            | Ty::RawPtr
            | Ty::ConstParam { .. }
            | Ty::ConstValue { .. }
    )
}

/// The [`Repr`] of a value of type `ty` — the LAYOUT decision, made once here.
/// Heap types get `Ptr` with a placeholder [`LayoutId`] (the layout pass, a
/// later brick, assigns real ids); scalars get their named byte width.
pub fn repr_of(ty: &Ty) -> Result<Repr, LowerError> {
    if matches!(ty, Ty::Unknown) {
        // Repr demands concrete types — the AllTypesConcrete precondition (§4).
        return Err(LowerError::Unsupported(
            "Unknown type reached MIR lowering (AllTypesConcrete precondition violated)".into(),
        ));
    }
    if is_heap_ty(ty) {
        return Ok(Repr::Ptr { layout: PLACEHOLDER_LAYOUT });
    }
    use crate::ScalarWidth;
    let w = match ty {
        Ty::Int | Ty::Int64 | Ty::UInt64 | Ty::Float | Ty::Float64 => ScalarWidth::Double,
        Ty::Int32 | Ty::UInt32 | Ty::Float32 => ScalarWidth::Word,
        Ty::Int16 | Ty::UInt16 => ScalarWidth::Half,
        Ty::Int8 | Ty::UInt8 => ScalarWidth::Byte,
        Ty::Bool => ScalarWidth::Word, // Bool ABI slot is 4 bytes
        // Unit/Never/RawPtr/Const* are not values that get a scalar slot here.
        other => {
            return Err(LowerError::Unsupported(format!(
                "no scalar Repr for {other:?}"
            )))
        }
    };
    Ok(Repr::Scalar { width: w })
}

/// Lower one function to MIR. Parameters are seeded first (the v1 borrow-by-
/// default calling convention — see [`LowerCtx::bind_params`]), then the body.
pub fn lower_function(func: &IrFunction) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx::default();
    let params = ctx.bind_params(&func.params)?;
    let ret = ctx.lower_body_into(&func.body)?;
    // The function's EFFECT SIGNATURE → its declared capability bound. The v1 model
    // has one capability (Stdout); an `effect fn` declares it may reach the host, so
    // it admits the only modeled cap. A pure `fn` declares ∅ — so if it reached
    // Stdout (forbidden by the effect system) the proven `used ⊆ declared` checker
    // would REJECT it. The capability gate verifies `reachable ⊆ declared`, not just
    // "reaches nothing" — so an effectful function is now caps-VERIFIED against its
    // own declared bound, not merely excluded.
    let declared_caps = if func.is_effect {
        vec![crate::Capability::Stdout]
    } else {
        Vec::new()
    };
    Ok(MirFunction {
        name: func.name.as_str().to_string(),
        params,
        ops: ctx.ops,
        ret,
        declared_caps,
    })
}

/// Lower a function body expression to MIR (the param-free testable core;
/// `lower_function` is the wrapper that seeds parameters first).
pub fn lower_body(body: &IrExpr, name: &str) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx::default();
    let ret = ctx.lower_body_into(body)?;
    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() })
}

#[derive(Default)]
struct LowerCtx {
    ops: Vec<Op>,
    /// VarId → the MIR value it denotes. Aliases map to the SAME ValueId.
    value_of: HashMap<VarId, ValueId>,
    /// Heap handles in binding order, for scope-end drops (one Drop per handle).
    live_heap_handles: Vec<ValueId>,
    /// The MIR values that are BORROWED heap parameters (the v1 calling
    /// convention): the caller owns the reference. A direct move-out/return or
    /// in-place mutation of one needs an explicit acquire (`Dup`) the body does
    /// not perform, so it is walled — never lowered to an unbacked cert event.
    param_values: HashSet<ValueId>,
    next_value: u32,
}

impl LowerCtx {
    fn fresh_value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    /// Seed the parameters: each param's VarId maps to a fresh MIR value (so uses
    /// in the body resolve) and becomes a [`MirParam`] carrying its [`Repr`] (so
    /// the name-totality witness counts it as DEFINED — every param use must have
    /// a defining param). A HEAP param is BORROWED (the caller owns the reference
    /// — it contributes no owned `+1` to the ownership certificate; the cert and
    /// verifier guard on `repr.is_heap()`) and is recorded in `param_values` so a
    /// later move-out/mutation of a bare borrowed param is walled, not faked. A
    /// scalar param carries no ownership but is still a defined value.
    fn bind_params(&mut self, params: &[IrParam]) -> Result<Vec<MirParam>, LowerError> {
        let mut out = Vec::new();
        for p in params {
            let v = self.fresh_value();
            self.value_of.insert(p.var, v);
            let repr = repr_of(&p.ty)?; // Ptr (heap) / Scalar; Unsupported if Unknown or non-value
            if repr.is_heap() {
                self.param_values.insert(v);
            }
            out.push(MirParam { value: v, repr });
        }
        Ok(out)
    }

    /// Lower a function body (statements + tail + scope-end drops) into `self` —
    /// the shared core of `lower_function` (params pre-seeded) and `lower_body`.
    ///
    /// An expression-bodied function (`fn f() = expr`) is the SAME value-semantics
    /// subset as a block body — just an empty statement list whose tail IS the
    /// expression. The tail lowering walls anything outside the subset, so the
    /// wrapping never weakens the boundary (control-flow / unsupported tails still
    /// become an explicit `Unsupported`).
    fn lower_body_into(&mut self, body: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let (stmts, tail): (&[IrStmt], Option<&IrExpr>) = match &body.kind {
            IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
            _ => (&[], Some(body)),
        };
        for stmt in stmts {
            self.lower_stmt(stmt)?;
        }
        // The tail expression is the function's return value. A HEAP tail is MOVED
        // OUT to the caller (recorded as `ret`, not dropped at scope end); a scalar
        // tail carries no ownership; a Unit/absent tail is a Unit-returning body.
        let ret = self.lower_tail(tail)?;
        // Scope end: release every still-live heap handle (the moved-out return is
        // already removed). Aliases share a ValueId, so one Drop per HANDLE
        // balances the Alloc(+1) and each aliasing Dup(+1).
        self.emit_scope_end_drops();
        Ok(ret)
    }

    fn lower_stmt(&mut self, stmt: &IrStmt) -> Result<(), LowerError> {
        match &stmt.kind {
            IrStmtKind::Bind { var, ty, value, .. } => self.lower_bind(*var, ty, value),
            // `x = value` — reassignment. REBIND `x` to the new value (reusing
            // `lower_bind`). The OLD binding's handle stays in `live_heap_handles`
            // and is dropped at scope end — a conservative lifetime EXTENSION
            // (memory-safe, never a double-free: the old object is dropped exactly
            // once, at scope end, instead of at the reassignment). A read of the
            // old `x` inside `value` (e.g. `x = f(x)`) lowers BEFORE the rebind
            // overwrites `value_of[x]`, so it borrows the still-live old handle —
            // never a use-after-free.
            IrStmtKind::Assign { var, value } => self.lower_bind(*var, &value.ty, value),
            // `let (a, b) = (x, y)` — a TUPLE destructuring bind.
            IrStmtKind::BindDestructure { pattern, value } => {
                self.lower_destructure(pattern, value)
            }
            // In-place mutation of a place: `xs[i] = v` and `r.field = v` both
            // require the buffer to be UNIQUELY owned (copy-on-write) → `MakeUnique`.
            // The written value (and an index expression) are deferred — record any
            // call inside them so the caps fold is not blind to their effects.
            IrStmtKind::IndexAssign { target, index, value } => {
                self.lower_place_mutation(*target)?;
                self.record_elided_calls(index);
                self.record_elided_calls(value);
                Ok(())
            }
            IrStmtKind::FieldAssign { target, value, .. } => {
                self.lower_place_mutation(*target)?;
                self.record_elided_calls(value);
                Ok(())
            }
            // A bare expression statement: an `if`/`match` in statement position is
            // LINEARIZED (control flow), an EFFECT call (`println(s)`) is lowered as a
            // runtime effect. Other non-call expr statements stay Unsupported (the
            // lower_effect_call guard rejects them — flight-grade totality).
            IrStmtKind::Expr { expr } => match &expr.kind {
                IrExprKind::If { .. } | IrExprKind::Match { .. } => self.lower_branch(expr),
                IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                    self.lower_for_in(*var, var_tuple, iterable, body)
                }
                IrExprKind::While { cond, body } => self.lower_while(cond, body),
                _ => self.lower_effect_call(expr),
            },
            // A source comment carries no ownership — skip it (it is not a
            // "silent drop": Comment is a no-op by definition, not an unhandled op).
            IrStmtKind::Comment { .. } => Ok(()),
            other => Err(LowerError::Unsupported(format!(
                "statement {} not in the value-semantics subset",
                stmt_kind_name(other)
            ))),
        }
    }

    /// In-place mutation of a place (`xs[i] = v` / `r.field = v`): the write must
    /// land on a UNIQUELY-owned buffer, so emit `Op::MakeUnique` (copy-on-write if
    /// the buffer is shared). The written value is copied (value semantics; its
    /// content is deferred, and any call in it is caps-tainted by the elided-call
    /// gate, not silently dropped). A borrowed-param target is walled — mutating
    /// the caller's data needs the move-mode calling convention.
    fn lower_place_mutation(&mut self, target: VarId) -> Result<(), LowerError> {
        let v = self.value_for(target)?;
        if self.param_values.contains(&v) {
            return Err(LowerError::Unsupported(
                "in-place mutation of a borrowed param not in this brick".into(),
            ));
        }
        self.ops.push(Op::MakeUnique { v });
        Ok(())
    }

    fn lower_bind(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        if !is_heap_ty(ty) {
            // Scalar binding: define a Copy value, no ownership accounting. The
            // value's CONTENT is deferred (a single `Const`), so any call inside it
            // (`var n = list.len(xs)`) is elided — record those calls as effect
            // markers so the capability fold still sees their effects.
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
            IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. }
            | IrExprKind::EmptyMap
            | IrExprKind::Record { .. }
            | IrExprKind::Tuple { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::StringInterp { .. }
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. }
            | IrExprKind::OptionSome { .. }
            | IrExprKind::OptionNone
            | IrExprKind::BinOp { .. }
            | IrExprKind::UnOp { .. } => {
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
            // (`Op::Dup`), bound here and dropped at scope end (cert `a` + `d`).
            IrExprKind::Member { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::TupleIndex { .. } => {
                let dst = self.lower_heap_extraction(value)?;
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
            IrExprKind::Call { target, .. } => Err(LowerError::Unsupported(format!(
                "heap bind from Call[{}] not in this brick",
                call_target_kind(target)
            ))),
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
            other => Err(LowerError::Unsupported(format!(
                "heap bind from {} not in this brick",
                kind_name(other)
            ))),
        }
    }

    /// Lower a stdlib `Module` call (`<module>.<func>(args)`) in a VALUE position
    /// (bind or tail) to an `Op::CallFn` named `"<module>.<func>"`, IFF it is
    /// admissible. Two gates, in order:
    /// 1. FIRST-ORDER — no argument is a closure/function (`is_higher_order`): the
    ///    stdlib callee would invoke user code whose capabilities/ownership are
    ///    unmodelled in this slice.
    /// 2. PURE — the callee reaches no host capability ([`purity::is_pure`]): an
    ///    effectful call lowered as a bare `Op::CallFn` would silently omit its
    ///    capability from the witness `used` set (the checker derives caps only
    ///    from `Op::Call`), i.e. accept-but-unsafe. Walling it keeps `used`
    ///    complete by construction (no certificate is emitted at all).
    /// When both pass it lowers exactly like the verified `Named`-call arm: a heap
    /// result is a FRESH OWNED value (the return-mode signature), a scalar result
    /// carries no ownership. The caller decides bind (push to live handles) vs tail
    /// (move out). Returns the result's [`ValueId`].
    fn lower_pure_module_value_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Result<ValueId, LowerError> {
        if is_higher_order(args) {
            return Err(LowerError::Unsupported(format!(
                "Module call {module}.{func} with a function-typed argument (closure capabilities unmodelled) not in this brick"
            )));
        }
        if !purity::is_pure(module, func) {
            return Err(LowerError::Unsupported(format!(
                "effectful/impure stdlib Module call {module}.{func} needs a declared capability not in this brick"
            )));
        }
        let lowered = self.lower_call_args(args)?;
        let dst = self.fresh_value();
        let repr = repr_of(result_ty)?;
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: format!("{module}.{func}"),
            args: lowered,
            result: Some(repr),
        });
        Ok(dst)
    }

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
    fn lower_heap_extraction(&mut self, expr: &IrExpr) -> Result<ValueId, LowerError> {
        let container = extraction_container(expr).ok_or_else(|| {
            LowerError::Unsupported(format!(
                "{} is not a field/element extraction",
                kind_name(&expr.kind)
            ))
        })?;
        let src = match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => self.value_for(*id)?,
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
    fn record_elided_calls(&mut self, value: &IrExpr) {
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
    fn lower_destructure(&mut self, pattern: &IrPattern, value: &IrExpr) -> Result<(), LowerError> {
        let pats = match pattern {
            IrPattern::Tuple { elements } => elements,
            _ => {
                return Err(LowerError::Unsupported(
                    "destructure of a non-tuple pattern not in this brick".into(),
                ))
            }
        };
        // Shape 1: component-wise from a same-arity tuple literal.
        if let IrExprKind::Tuple { elements: vals } = &value.kind {
            if pats.len() == vals.len() {
                for (p, v) in pats.iter().zip(vals) {
                    match p {
                        IrPattern::Bind { var, ty } => self.lower_bind(*var, ty, v)?,
                        IrPattern::Wildcard => {}
                        _ => {
                            return Err(LowerError::Unsupported(
                                "destructure sub-pattern (only a bound var or `_`) not in this brick"
                                    .into(),
                            ))
                        }
                    }
                }
                return Ok(());
            }
        }
        // Shape 2: alias the whole container `t` per heap component.
        if let IrExprKind::Var { id } = &value.kind {
            if is_heap_ty(&value.ty) {
                let container = self.value_for(*id)?;
                for p in pats {
                    match p {
                        IrPattern::Wildcard => {}
                        IrPattern::Bind { var, ty } => {
                            let dst = self.fresh_value();
                            if is_heap_ty(ty) {
                                self.ops.push(Op::Dup { dst, src: container });
                                self.live_heap_handles.push(dst);
                            } else {
                                self.ops.push(Op::Const { dst });
                            }
                            self.value_of.insert(*var, dst);
                        }
                        _ => {
                            return Err(LowerError::Unsupported(
                                "destructure sub-pattern (only a bound var or `_`) not in this brick"
                                    .into(),
                            ))
                        }
                    }
                }
                return Ok(());
            }
        }
        Err(LowerError::Unsupported(
            "destructure (only a tuple literal or a tracked heap var) not in this brick".into(),
        ))
    }

    fn value_for(&self, var: VarId) -> Result<ValueId, LowerError> {
        self.value_of
            .get(&var)
            .copied()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))
    }

    /// Lower the body's tail expression to the function's return value.
    /// - heap `Var` tail → MOVE-OUT: the handle is consumed at the boundary
    ///   (returned as `ret`, removed from the live set so it is not also dropped).
    /// - scalar `Var` tail → returned by value (no ownership; `ret` names it).
    /// - scalar literal tail → a fresh `Const`, returned by value.
    /// - `Unit` / absent → a Unit-returning body (no return value).
    /// Anything else is an explicit `Unsupported` (flight-grade totality).
    fn lower_tail(&mut self, tail: Option<&IrExpr>) -> Result<Option<ValueId>, LowerError> {
        let tail = match tail {
            Some(t) => t,
            None => return Ok(None),
        };
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
                    let v = self.value_for(*id)?;
                    if self.param_values.contains(&v) {
                        // Returning a BORROWED param directly would move out a
                        // reference we do not own (the caller's) — a double-free.
                        // The sound form is `let q = p; q` (an alias `Dup` first),
                        // which lowers fine. Wall the bare-param return (totality);
                        // the cert would otherwise be `m` at rc 0 → checker fault.
                        return Err(LowerError::Unsupported(
                            "returning a borrowed param directly (needs an explicit acquire) not in this brick".into(),
                        ));
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
                | IrExprKind::Tuple { .. }
                | IrExprKind::LitStr { .. }
                | IrExprKind::StringInterp { .. }
                | IrExprKind::ResultOk { .. }
                | IrExprKind::ResultErr { .. }
                | IrExprKind::OptionSome { .. }
                | IrExprKind::OptionNone
                | IrExprKind::BinOp { .. }
                | IrExprKind::UnOp { .. } => {
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
                // container (`Op::Dup`) and move it out (cert `a` + `m`).
                IrExprKind::Member { .. }
                | IrExprKind::IndexAccess { .. }
                | IrExprKind::MapAccess { .. }
                | IrExprKind::TupleIndex { .. } => {
                    let dst = self.lower_heap_extraction(tail)?;
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
            IrExprKind::Var { id } => Ok(Some(self.value_for(*id)?)),
            IrExprKind::LitInt { .. }
            | IrExprKind::LitBool { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::BinOp { .. }
            | IrExprKind::UnOp { .. }
            // A SCALAR field/element/tuple extraction is an unambiguous COPY (a
            // scalar is never reference-counted), so it is a `Const` — its
            // container carries its own ownership. (A HEAP extraction is an ALIAS
            // / share — it needs a layout-aware field-access op with `Dup`
            // semantics and stays walled until that brick.)
            | IrExprKind::Member { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::TupleIndex { .. } => {
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

    /// Lower an EFFECT call (a Unit-typed `Call`) to a runtime [`Op::Call`].
    /// Today the recognized set is `println(s)` for a heap string → [`RtFn::PrintStr`],
    /// which BORROWS the string handle (no refcount change; the value stays live
    /// and is dropped at scope end) and reaches [`crate::Capability::Stdout`] (so a
    /// real printing program's capability witness is derived from real source).
    /// Anything outside the set is an explicit `Unsupported` (totality).
    fn lower_effect_call(&mut self, call: &IrExpr) -> Result<(), LowerError> {
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
            _ => {
                return Err(LowerError::Unsupported(
                    "only Named effect calls in this brick".into(),
                ))
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

    /// Lower an `if`/`match` in STATEMENT or scalar-/Unit-TAIL position by
    /// LINEARIZING its arms into the flat op stream — NO `Branch` op. A branch op
    /// would force the certificate fold (and `exec`/`verify`) to RECURSE a control-
    /// flow graph; the v1 checker must stay a flat fold (the certificate-format-v1
    /// tripwire: the instant the checker walks a CFG, the shape is broken). So the
    /// branch discipline lives ENTIRELY here in the untrusted lowering, and the cert
    /// the checker sees is a flat sequence.
    ///
    /// SOUNDNESS over a runtime where only ONE arm executes: each arm is lowered with
    /// a PER-ARM SCOPE FRAME ([`Self::lower_branch_arm`]) so every heap object it
    /// allocates is balanced WITHIN the arm (`i…d`). Such an object is therefore safe
    /// on EVERY path — the arm that allocates it runs its balanced `i…d`; on the
    /// other path it is simply never allocated (its `i…d` is vacuous). A handle that
    /// READS a pre-branch object (`var w = z`) is a balanced `a…d` PAIR inside the
    /// arm, removable on the other path, so the shared object stays balanced too. No
    /// arm value ESCAPES the branch: the RESULT is emitted by the CALLER as ONE
    /// merged slot — DISCARDED (statement / Unit position), a `Const` (scalar), or a
    /// fresh `Alloc{Opaque}` (heap). So no per-arm `i`/`a` crosses the branch and the
    /// flat cert is sound on both paths. The fresh-`Opaque` heap result is the same
    /// value-CONTENT deferral as every other heap value (which arm's value it equals
    /// is functional, not a safety property — `守るのは安全性であって機能の正しさで
    /// はない`); it is memory-safe BY CONSTRUCTION (a clean fresh alloc), so it needs
    /// no result-phi merge and bypasses no soundness check (a borrowed-param arm
    /// result is simply not moved out — the function returns the fresh `Opaque`).
    ///
    /// CAPS: both arms are lowered, so the witness captures the UNION of their
    /// capabilities — a conservative over-approximation (the path actually taken
    /// reaches a SUBSET), hence `actual ⊆ union ⊆ declared` stays sound. Const-ing a
    /// scalar branch instead (dropping the arms) would MISS an arm's `println` =
    /// caps-unsound, so the arms MUST be lowered even for a scalar result.
    ///
    /// WALLED (each an explicit `Unsupported`, never a silent miscompile): a fresh
    /// heap SUBJECT (eliding its `Alloc` would be an accept-but-unsafe leak), a
    /// payload-BINDING `match` pattern (extracting a field needs the layout brick), a
    /// `match` arm GUARD, and an arm that REASSIGNS a variable (a path-dependent
    /// `value_of` rebind the flat fold cannot see → UAF).
    fn lower_branch(&mut self, expr: &IrExpr) -> Result<(), LowerError> {
        let arms: Vec<&IrExpr> = match &expr.kind {
            IrExprKind::If { cond, then, else_ } => {
                // The condition is evaluated ONCE before the branch — it is scalar
                // (Bool), so no ownership, but capture the caps of any call in it.
                self.record_elided_calls(cond);
                vec![then, else_]
            }
            IrExprKind::Match { subject, arms } => {
                // The subject is inspected once. Only a SCALAR or an already-tracked
                // heap var (borrowed, dropped at the OUTER scope) is admitted: a fresh
                // heap subject (a call/literal result) would need materialize-and-drop
                // — eliding it would leave its `Alloc` unmodelled = accept-but-unsafe
                // leak. A `Var` is borrowed for the inspection (no ownership change).
                if is_heap_ty(&subject.ty)
                    && !matches!(subject.kind, IrExprKind::Var { .. })
                {
                    return Err(LowerError::Unsupported(
                        "match on a fresh heap subject (needs materialize-and-drop) not in this control-flow slice".into(),
                    ));
                }
                self.record_elided_calls(subject);
                let mut bodies = Vec::with_capacity(arms.len());
                for arm in arms {
                    // A pattern that BINDS any value extracts a payload/field/element
                    // — its object identity needs the layout brick (#54). A guard is a
                    // deferred heap-touching sub-computation. Both are walled.
                    if !pattern_binds_nothing(&arm.pattern) {
                        return Err(LowerError::Unsupported(
                            "match arm pattern binds a value (needs the layout brick) not in this control-flow slice".into(),
                        ));
                    }
                    if arm.guard.is_some() {
                        return Err(LowerError::Unsupported(
                            "match arm guard not in this control-flow slice".into(),
                        ));
                    }
                    bodies.push(&arm.body);
                }
                bodies
            }
            other => {
                return Err(LowerError::Unsupported(format!(
                    "lower_branch on a non-branch {}",
                    kind_name(other)
                )))
            }
        };
        for body in arms {
            self.lower_branch_arm(body)?;
        }
        Ok(())
    }

    /// Lower ONE branch arm into the flat op stream with a PER-ARM SCOPE FRAME:
    /// snapshot the live-handle count, lower the arm, then DROP every handle the arm
    /// added (so the arm is internally balanced, and vacuous when the other arm runs).
    /// The arm's result is DISCARDED (Unit/statement) or a SCALAR the caller merges
    /// into one `Const`; a heap result is walled. See [`Self::lower_branch`].
    fn lower_branch_arm(&mut self, body: &IrExpr) -> Result<(), LowerError> {
        let (stmts, tail): (&[IrStmt], Option<&IrExpr>) = match &body.kind {
            IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
            _ => (&[], Some(body)),
        };
        // A HEAP reassignment inside the arm would rebind a var's `value_of`
        // PATH-DEPENDENTLY: a post-branch read then dereferences a handle this arm
        // dropped (a UAF the flat fold cannot see). Wall a top-level heap Assign (a
        // nested one is re-checked when its control flow recurses). A SCALAR reassign
        // is harmless — it rebinds to a Copy `Const` (no handle to dangle), so it is
        // admitted (e.g. a loop counter inside a branch).
        if stmts.iter().any(stmt_is_heap_reassign) {
            return Err(LowerError::Unsupported(
                "branch arm reassigns a heap variable (path-dependent rebind) not in this control-flow slice".into(),
            ));
        }
        let mark = self.live_heap_handles.len();
        for stmt in stmts {
            self.lower_stmt(stmt)?;
        }
        if let Some(tail) = tail {
            // The arm's tail VALUE never escapes the arm — the branch RESULT is one
            // fresh `Alloc{Opaque}` the CALLER emits (a heap result) or a `Const` (a
            // scalar). So a Unit-call tail is lowered as an EFFECT (`println`, so its
            // Stdout reaches the witness); a nested branch recurses (its own arms get
            // per-arm frames); ANY OTHER tail — scalar or HEAP — is a deferred value
            // whose calls we capture as effect markers (its content, like every
            // `Opaque`, is carried by the merged result, not modelled per-arm).
            match &tail.kind {
                IrExprKind::Call { .. } if matches!(tail.ty, Ty::Unit) => {
                    self.lower_effect_call(tail)?
                }
                IrExprKind::If { .. } | IrExprKind::Match { .. } => self.lower_branch(tail)?,
                _ => self.record_elided_calls(tail),
            }
        }
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Drop every heap handle the current scope frame added beyond `mark` (LIFO),
    /// restoring `live_heap_handles` to its pre-frame length — the per-arm teardown.
    fn drop_arm_locals(&mut self, mark: usize) {
        for v in self.live_heap_handles.split_off(mark).into_iter().rev() {
            self.ops.push(Op::Drop { v });
        }
    }

    /// Lower a `for v in iterable { body }` by modeling ONE iteration with a
    /// PER-ITERATION SCOPE FRAME. Each iteration is internally balanced (its loop
    /// variable + body locals are all dropped at iteration end), so N runtime
    /// iterations are N balanced episodes — no cross-iteration leak or double-free,
    /// and the flat cert (one iteration) is sound for any N (including 0: every op is
    /// in a balanced frame). NO loop op — the iteration discipline lives entirely in
    /// the lowering, the checker stays a flat fold.
    ///
    /// The ITERABLE is evaluated once: a heap iterable is lowered by `lower_call_args`
    /// — an already-tracked `Var` is BORROWED, a FRESH heap value (a call/literal
    /// result) is MATERIALIZED into an owned temp released at the OUTER scope; a scalar
    /// iterable (a `Range`) carries no ownership. The LOOP VARIABLE binds one element per
    /// iteration: a HEAP element ALIASES the whole container (`Op::Dup`, container-
    /// grain like field extraction — it keeps the container alive for the iteration,
    /// dropped at its end; element-precise identity needs the layout brick), a SCALAR
    /// element is a `Const`. WALLED: a `break`/`continue` (the early-exit path would
    /// skip the frame's drops = a leak), and a HEAP reassignment of a pre-loop var (an
    /// iteration-dependent rebind → UAF). A scalar reassignment (`i = i + 1`) is a
    /// Copy `Const`, harmless, admitted.
    fn lower_for_in(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> Result<(), LowerError> {
        // The iterable is evaluated ONCE before the loop. A heap iterable goes through
        // `lower_call_args` — an already-tracked `Var` is borrowed (no new ownership),
        // a fresh heap value is materialized into an owned temp dropped at the OUTER
        // scope (its caps captured by the lowering). A scalar iterable (a `Range`)
        // carries no ownership; capture any call in it for caps.
        let container: Option<ValueId> = if is_heap_ty(&iterable.ty) {
            match self.lower_call_args(std::slice::from_ref(iterable))?.into_iter().next() {
                Some(CallArg::Handle(v)) => Some(v),
                _ => None,
            }
        } else {
            self.record_elided_calls(iterable);
            None
        };
        self.guard_loop_body(body, "for-in")?;
        let mark = self.live_heap_handles.len();
        let vars: Vec<VarId> = match var_tuple {
            Some(vs) => vs.clone(),
            None => vec![var],
        };
        for v in vars {
            // A heap element aliases the whole container; a scalar element is a Const.
            let elem_heap = find_var_ty(body, v).map(|t| is_heap_ty(&t)).unwrap_or(false);
            if elem_heap {
                let src = container.ok_or_else(|| {
                    LowerError::Unsupported(
                        "for-in heap loop variable over a non-container iterable not in this brick".into(),
                    )
                })?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                self.value_of.insert(v, dst);
                self.live_heap_handles.push(dst);
            } else {
                let dst = self.fresh_value();
                self.ops.push(Op::Const { dst });
                self.value_of.insert(v, dst);
            }
        }
        for stmt in body {
            self.lower_stmt(stmt)?;
        }
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Lower a `while cond { body }` like a `for-in` body — a PER-ITERATION SCOPE
    /// FRAME makes one modeled iteration balanced, sound for any N. The condition is
    /// evaluated each iteration (its caps captured); the body's locals are dropped at
    /// iteration end. Same WALLS as `for-in` (`break`/`continue`, heap reassignment).
    fn lower_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> Result<(), LowerError> {
        self.record_elided_calls(cond);
        self.guard_loop_body(body, "while")?;
        let mark = self.live_heap_handles.len();
        for stmt in body {
            self.lower_stmt(stmt)?;
        }
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Shared loop-body admission: a `break`/`continue` (the early-exit path would
    /// skip the per-iteration frame's drops = a leak) and a HEAP reassignment of a
    /// pre-loop variable (an iteration-dependent `value_of` rebind → UAF) are walled.
    fn guard_loop_body(&self, body: &[IrStmt], what: &str) -> Result<(), LowerError> {
        if body_breaks_or_continues(body) {
            return Err(LowerError::Unsupported(format!(
                "{what} body with break/continue (early exit would skip per-iteration drops) not in this brick"
            )));
        }
        if body.iter().any(stmt_is_heap_reassign) {
            return Err(LowerError::Unsupported(format!(
                "{what} body reassigns a heap variable (iteration-dependent rebind) not in this brick"
            )));
        }
        Ok(())
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
    fn lower_call_args(&mut self, args: &[IrExpr]) -> Result<Vec<CallArg>, LowerError> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let arg = match &a.kind {
                IrExprKind::Var { id } if is_heap_ty(&a.ty) => CallArg::Handle(self.value_for(*id)?),
                IrExprKind::Var { id } => CallArg::Scalar(self.value_for(*id)?),
                IrExprKind::LitInt { value } => CallArg::Imm(*value),
                // A fresh HEAP literal argument (`f("x")`, `f([1, 2, 3])`):
                // materialized into an owned temp via `Alloc`, borrowed into the
                // call, dropped at scope end — cert `i` (alloc) + `d` (drop), both
                // backed, identical to the verified fresh-heap bind.
                IrExprKind::LitStr { .. }
                | IrExprKind::List { .. }
                | IrExprKind::MapLiteral { .. }
                | IrExprKind::EmptyMap
                | IrExprKind::Record { .. }
                | IrExprKind::Tuple { .. }
                | IrExprKind::StringInterp { .. }
                | IrExprKind::ResultOk { .. }
                | IrExprKind::ResultErr { .. }
                | IrExprKind::OptionSome { .. }
                | IrExprKind::OptionNone => {
                    let dst = self.fresh_value();
                    let repr = repr_of(&a.ty)?;
                    let init = alloc_init(a);
                    self.ops.push(Op::Alloc { dst, repr, init });
                    self.record_elided_calls(a);
                    self.materialized_call_arg(dst, repr)
                }
                // A scalar literal argument (`f(3.14)`, `f(true)`): a fresh `Const`,
                // passed by value (no ownership). `LitInt` is already an `Imm` above.
                IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. } => {
                    let dst = self.fresh_value();
                    self.ops.push(Op::Const { dst });
                    CallArg::Scalar(dst)
                }
                // A fresh BinOp/UnOp result as an argument (`f(a + b)`, `f(-n)`): a
                // heap result (string concat) is materialized via `Alloc`, borrowed
                // and dropped; a scalar result is a `Const`. Operands carry their own
                // ownership (value semantics — the result is fresh, never an alias).
                IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. } => {
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
                        let dst = self.lower_heap_extraction(a)?;
                        let repr = repr_of(&a.ty)?;
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
                IrExprKind::Call { target, .. } => {
                    return Err(LowerError::Unsupported(format!(
                        "call argument Call[{}] not in this brick",
                        call_target_kind(target)
                    )))
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

    /// Register a freshly-materialized call-result temp used as a call argument: a
    /// HEAP temp is BORROWED into the call (`Handle`) and added to the scope-end
    /// drop set (it is owned by THIS scope, not moved out, so it is released after
    /// the call returns); a scalar temp is passed by value.
    fn materialized_call_arg(&mut self, dst: ValueId, repr: Repr) -> CallArg {
        if repr.is_heap() {
            self.live_heap_handles.push(dst);
            CallArg::Handle(dst)
        } else {
            CallArg::Scalar(dst)
        }
    }

    fn emit_scope_end_drops(&mut self) {
        // Reverse binding order (LIFO scope teardown).
        for v in self.live_heap_handles.iter().rev() {
            self.ops.push(Op::Drop { v: *v });
        }
    }
}

/// Is a statement a HEAP reassignment (`x = <heap value>`)? Such a rebind inside a
/// branch arm or loop body changes a var's `value_of` in a path/iteration-dependent
/// way the flat fold cannot see (→ UAF), so it is walled. A SCALAR reassignment is a
/// Copy `Const` with no handle to dangle, so it is NOT flagged (admitted).
fn stmt_is_heap_reassign(s: &IrStmt) -> bool {
    matches!(&s.kind, IrStmtKind::Assign { value, .. } if is_heap_ty(&value.ty))
}

/// Does a statement list contain a `break`/`continue` that targets THIS loop — i.e.
/// not nested inside another loop (which captures its own)? Used to wall a loop body
/// whose early-exit path would skip the per-iteration frame's drops (a leak).
fn body_breaks_or_continues(stmts: &[IrStmt]) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Scan {
        found: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Break | IrExprKind::Continue => self.found = true,
                // A nested loop captures its OWN break/continue — do not descend.
                IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {}
                _ => walk_expr(self, e),
            }
        }
    }
    let mut s = Scan { found: false };
    for stmt in stmts {
        s.visit_stmt(stmt);
    }
    s.found
}

/// Find the type a variable is USED at in a body (its first reference's `ty`) — for
/// a `for-in` loop variable, this is its element type (the `ForIn` node carries no
/// explicit element type). `None` if the variable is unused (then its heap-ness does
/// not matter — nothing references it to manage).
fn find_var_ty(stmts: &[IrStmt], var: VarId) -> Option<Ty> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Find {
        var: VarId,
        ty: Option<Ty>,
    }
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.ty.is_some() {
                return;
            }
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    self.ty = Some(e.ty.clone());
                    return;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut f = Find { var, ty: None };
    for stmt in stmts {
        if f.ty.is_some() {
            break;
        }
        f.visit_stmt(stmt);
    }
    f.ty
}

/// Does a `match` pattern BIND no value — i.e. only inspect tags/shape, never
/// extract a payload/field/element into a variable? A binding pattern needs the
/// layout brick (#54) to know WHERE the bound value lives (offset + heap-ness), so
/// the control-flow slice admits only non-binding patterns: a `Wildcard`, a literal
/// match, `None`, or a constructor/tuple/record/list whose sub-patterns ALL bind
/// nothing (`Some(_)`, `Ok(0)`, `(_, _)`). A `Bind` (even scalar — its position in
/// the subject is unknown without layout) makes the whole pattern binding.
fn pattern_binds_nothing(pat: &IrPattern) -> bool {
    match pat {
        IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => true,
        IrPattern::Bind { .. } => false,
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            pattern_binds_nothing(inner)
        }
        IrPattern::Constructor { args, .. } => args.iter().all(pattern_binds_nothing),
        IrPattern::Tuple { elements } | IrPattern::List { elements } => {
            elements.iter().all(pattern_binds_nothing)
        }
        IrPattern::RecordPattern { fields, .. } => {
            // A field with `pattern: None` is the shorthand `{ name }` — it BINDS the
            // field to `name`, so it is NOT a non-binding pattern.
            fields
                .iter()
                .all(|f| f.pattern.as_ref().is_some_and(pattern_binds_nothing))
        }
    }
}

/// Extract a concrete initializer from a fresh-heap bind value. A `List[Int]`
/// literal yields [`Init::IntList`]; everything else is [`Init::Opaque`] (the
/// computation is carried by a later brick).
fn alloc_init(value: &IrExpr) -> Init {
    if let IrExprKind::List { elements } = &value.kind {
        let ints: Option<Vec<i64>> = elements
            .iter()
            .map(|e| match &e.kind {
                IrExprKind::LitInt { value } => Some(*value),
                _ => None,
            })
            .collect();
        if let Some(ints) = ints {
            return Init::IntList(ints);
        }
    }
    Init::Opaque
}

fn stmt_kind_name(k: &IrStmtKind) -> &'static str {
    match k {
        IrStmtKind::Bind { .. } => "Bind",
        IrStmtKind::BindDestructure { .. } => "BindDestructure",
        IrStmtKind::Assign { .. } => "Assign",
        IrStmtKind::IndexAssign { .. } => "IndexAssign",
        IrStmtKind::MapInsert { .. } => "MapInsert",
        IrStmtKind::FieldAssign { .. } => "FieldAssign",
        IrStmtKind::Guard { .. } => "Guard",
        IrStmtKind::Expr { .. } => "Expr",
        IrStmtKind::Comment { .. } => "Comment",
        IrStmtKind::RcInc { .. } => "RcInc",
        IrStmtKind::RcDec { .. } => "RcDec",
        IrStmtKind::ListSwap { .. } => "ListSwap",
        IrStmtKind::ListReverse { .. } => "ListReverse",
        IrStmtKind::ListRotateLeft { .. } => "ListRotateLeft",
        IrStmtKind::ListCopySlice { .. } => "ListCopySlice",
    }
}

/// The CONTAINER expression of a field/element/tuple/map extraction, if `expr`
/// is one — the source whose object the extracted value aliases (the
/// container-grain field access, see [`LowerCtx::lower_heap_extraction`]).
fn extraction_container(expr: &IrExpr) -> Option<&IrExpr> {
    match &expr.kind {
        IrExprKind::Member { object, .. }
        | IrExprKind::IndexAccess { object, .. }
        | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::MapAccess { object, .. } => Some(object),
        _ => None,
    }
}

/// True if any argument is a FUNCTION-typed value (a closure / lambda / fn-ref).
/// A stdlib call with such an argument invokes USER code, so its effective
/// capabilities are its-own ∪ the closure's — unmodelled in the pure-only Module
/// slice — and a captured-heap closure carries ownership this brick does not
/// track. Such calls are walled. The TYPE test catches every form (a lambda
/// literal, a fn-ref, OR a variable of function type) under the AllTypesConcrete
/// precondition; the kind test is a belt-and-suspenders for any arg whose type
/// was not concretized.
fn is_higher_order(args: &[IrExpr]) -> bool {
    args.iter().any(|a| {
        matches!(a.ty, Ty::Fn { .. })
            || matches!(
                a.kind,
                IrExprKind::Lambda { .. }
                    | IrExprKind::ClosureCreate { .. }
                    | IrExprKind::FnRef { .. }
            )
    })
}

/// The kind of a call's resolved target — used to make a walled `Call`'s reason
/// precise (the histogram then names which call SHAPE to admit next: a free
/// `Named` call vs a stdlib `Module` dispatch vs an unresolved `Method` vs a
/// `Computed` callee), so the coverage roadmap is evidence-based, not guessed.
fn call_target_kind(t: &CallTarget) -> &'static str {
    match t {
        CallTarget::Named { .. } => "Named",
        CallTarget::Module { .. } => "Module",
        CallTarget::Method { .. } => "Method",
        CallTarget::Computed { .. } => "Computed",
    }
}

fn kind_name(k: &IrExprKind) -> &'static str {
    // Named precisely so the corpus-wall `<other>` buckets break down into the
    // exact expression forms still to admit (an evidence-based roadmap, the same
    // discipline as `call_target_kind`). Unnamed kinds remain `<other>`.
    match k {
        IrExprKind::LitInt { .. } => "LitInt",
        IrExprKind::LitFloat { .. } => "LitFloat",
        IrExprKind::LitStr { .. } => "LitStr",
        IrExprKind::LitBool { .. } => "LitBool",
        IrExprKind::Unit => "Unit",
        IrExprKind::Var { .. } => "Var",
        IrExprKind::List { .. } => "List",
        IrExprKind::Record { .. } => "Record",
        IrExprKind::Tuple { .. } => "Tuple",
        IrExprKind::Block { .. } => "Block",
        IrExprKind::Call { .. } => "Call",
        IrExprKind::RuntimeCall { .. } => "RuntimeCall",
        IrExprKind::BinOp { .. } => "BinOp",
        IrExprKind::UnOp { .. } => "UnOp",
        IrExprKind::If { .. } => "If",
        IrExprKind::Match { .. } => "Match",
        IrExprKind::Member { .. } => "Member",
        IrExprKind::TupleIndex { .. } => "TupleIndex",
        IrExprKind::IndexAccess { .. } => "IndexAccess",
        IrExprKind::MapAccess { .. } => "MapAccess",
        IrExprKind::Range { .. } => "Range",
        IrExprKind::MapLiteral { .. } => "MapLiteral",
        IrExprKind::EmptyMap => "EmptyMap",
        IrExprKind::StringInterp { .. } => "StringInterp",
        IrExprKind::Lambda { .. } => "Lambda",
        IrExprKind::ClosureCreate { .. } => "ClosureCreate",
        IrExprKind::FnRef { .. } => "FnRef",
        IrExprKind::ResultOk { .. } => "ResultOk",
        IrExprKind::ResultErr { .. } => "ResultErr",
        IrExprKind::OptionSome { .. } => "OptionSome",
        IrExprKind::OptionNone => "OptionNone",
        IrExprKind::Try { .. } => "Try",
        IrExprKind::Unwrap { .. } => "Unwrap",
        IrExprKind::UnwrapOr { .. } => "UnwrapOr",
        IrExprKind::ForIn { .. } => "ForIn",
        IrExprKind::While { .. } => "While",
        IrExprKind::Fan { .. } => "Fan",
        IrExprKind::Break => "Break",
        IrExprKind::Continue => "Continue",
        IrExprKind::TailCall { .. } => "TailCall",
        IrExprKind::IterChain { .. } => "IterChain",
        _ => "<other>",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify_ownership, ViolationKind};
    use almide_lang::types::constructor::TypeConstructorId;

    fn ir_expr(kind: IrExprKind, ty: Ty) -> IrExpr {
        IrExpr { kind, ty, span: None, def_id: None }
    }
    fn stmt(kind: IrStmtKind) -> IrStmt {
        IrStmt { kind, span: None }
    }
    fn list_int() -> Ty {
        // Any Applied/heap type works for the ownership logic; List[Int] is the
        // value-semantics shape under test.
        Ty::Applied(TypeConstructorId::List, vec![Ty::Int])
    }
    fn bind(var: u32, ty: Ty, value: IrExpr) -> IrStmt {
        stmt(IrStmtKind::Bind {
            var: VarId(var),
            mutability: almide_ir::Mutability::Var,
            ty,
            value,
        })
    }
    /// Build a Unit-returning body block (avoids constructing a full IrFunction).
    fn body(stmts: Vec<IrStmt>) -> IrExpr {
        ir_expr(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
    }

    #[test]
    fn alias_then_cow_lowers_to_balanced_mir() {
        // var a = [1,2,3]; var b = a; a[0] = 9
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(1, list_int(), ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
            stmt(IrStmtKind::IndexAssign {
                target: VarId(0),
                index: ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int),
                value: ir_expr(IrExprKind::LitInt { value: 9 }, Ty::Int),
            }),
        ]);
        let mir = lower_body(&b, "main").expect("lowers");

        // Expect: Alloc(a=V0), Dup(b=V1 from V0), MakeUnique(a=V0), Drop, Drop.
        assert!(matches!(mir.ops[0], Op::Alloc { dst: ValueId(0), .. }));
        assert!(matches!(mir.ops[1], Op::Dup { dst: ValueId(1), src: ValueId(0) }));
        assert!(matches!(mir.ops[2], Op::MakeUnique { v: ValueId(0) }));
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(drops, 2, "two handles (a, b) → two scope-end drops");

        // The single ownership decision must be balanced by construction.
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn heap_return_is_a_balanced_move_out() {
        // fn build() -> List[Int] = { var a = [1,2,3]; a }
        // The tail `a` is a heap move-out: Alloc(+1), returned/consumed(−1), and
        // NOT dropped at scope end. Ownership witness `id` → balanced.
        let tail = ir_expr(IrExprKind::Var { id: VarId(0) }, list_int());
        let b = ir_expr(
            IrExprKind::Block {
                stmts: vec![bind(
                    0,
                    list_int(),
                    ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
                )],
                expr: Some(Box::new(tail)),
            },
            list_int(),
        );
        let mir = lower_body(&b, "build").expect("lowers");
        assert!(matches!(mir.ops[0], Op::Alloc { dst: ValueId(0), .. }));
        // moved out, so NO scope-end Drop of the returned handle.
        assert!(!mir.ops.iter().any(|o| matches!(o, Op::Drop { .. })));
        assert_eq!(mir.ret, Some(ValueId(0)));
        // The move-out balances the Alloc — the verifier accepts.
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn scalar_bind_needs_no_ownership() {
        // let n = 5
        let b = body(vec![bind(
            0,
            Ty::Int,
            ir_expr(IrExprKind::LitInt { value: 5 }, Ty::Int),
        )]);
        let mir = lower_body(&b, "main").expect("lowers");
        assert_eq!(mir.ops, vec![Op::Const { dst: ValueId(0) }]);
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn fresh_heap_bind_allocs_and_drops() {
        // var s = "hi"
        let b = body(vec![bind(
            0,
            Ty::String,
            ir_expr(IrExprKind::LitStr { value: "hi".into() }, Ty::String),
        )]);
        let mir = lower_body(&b, "main").expect("lowers");
        assert!(matches!(mir.ops[0], Op::Alloc { .. }));
        assert!(matches!(mir.ops[1], Op::Drop { .. }));
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn out_of_subset_is_an_explicit_error_not_silent() {
        // A bare expression statement is outside this brick → explicit Unsupported.
        let b = body(vec![stmt(IrStmtKind::Expr {
            expr: ir_expr(IrExprKind::LitInt { value: 1 }, Ty::Int),
        })]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_is_rejected_at_repr() {
        assert!(matches!(repr_of(&Ty::Unknown), Err(LowerError::Unsupported(_))));
    }

    #[test]
    fn use_after_free_caught_if_decision_were_wrong() {
        // Sanity that the verifier guards the lowering: a hand-broken MIR with a
        // missing alias Dup would leave the alias' Drop unbalanced.
        let broken = MirFunction {
            name: "broken".into(),
            ops: vec![
                Op::Alloc { dst: ValueId(0), repr: Repr::Ptr { layout: PLACEHOLDER_LAYOUT }, init: Init::Opaque },
                Op::Drop { v: ValueId(0) },
                Op::Drop { v: ValueId(0) }, // second drop with no Dup → double free
            ],
            ..Default::default()
        };
        let errs = verify_ownership(&broken).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::DoubleFree));
    }

    // ── stdlib Module-call lowering (brick #47) ──

    fn module_call(module: &str, func: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
        use almide_lang::intern::sym;
        ir_expr(
            IrExprKind::Call {
                target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None },
                args,
                type_args: vec![],
            },
            ty,
        )
    }

    #[test]
    fn is_higher_order_detects_function_typed_args() {
        let fn_ty = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) };
        let plain = ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String);
        let closure = ir_expr(IrExprKind::Var { id: VarId(1) }, fn_ty);
        assert!(!is_higher_order(std::slice::from_ref(&plain)));
        assert!(is_higher_order(&[plain, closure]));
    }

    #[test]
    fn pure_first_order_module_call_lowers() {
        // var s = "x"; var t = string.trim(s)  — first-order + pure → admitted.
        let b = body(vec![
            bind(0, Ty::String, ir_expr(IrExprKind::LitStr { value: "x".into() }, Ty::String)),
            bind(
                1,
                Ty::String,
                module_call(
                    "string",
                    "trim",
                    vec![ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String)],
                    Ty::String,
                ),
            ),
        ]);
        let mir = lower_body(&b, "main").expect("pure first-order Module call lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "string.trim")),
            "expected an Op::CallFn named string.trim, got {:?}",
            mir.ops
        );
        // A fresh owned heap result, balanced by a scope-end drop.
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn higher_order_module_call_is_walled() {
        // var ys = list.map(xs, f)  with f : (Int) -> Int  → walled (closure arg).
        let fn_ty = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) };
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(
                1,
                list_int(),
                module_call(
                    "list",
                    "map",
                    vec![
                        ir_expr(IrExprKind::Var { id: VarId(0) }, list_int()),
                        ir_expr(IrExprKind::Var { id: VarId(2) }, fn_ty),
                    ],
                    list_int(),
                ),
            ),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(m)) => {
                assert!(m.contains("function-typed argument"), "got: {m}")
            }
            other => panic!("expected a higher-order wall, got {other:?}"),
        }
    }

    #[test]
    fn effectful_module_call_is_walled() {
        // var x = fs.read_text(p)  → walled (fs is effectful; its capability cannot
        // yet be charged into the witness, so admitting it would be accept-but-unsafe).
        let b = body(vec![
            bind(0, Ty::String, ir_expr(IrExprKind::LitStr { value: "p".into() }, Ty::String)),
            bind(
                1,
                Ty::String,
                module_call(
                    "fs",
                    "read_text",
                    vec![ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String)],
                    Ty::String,
                ),
            ),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(m)) => assert!(m.contains("effectful/impure"), "got: {m}"),
            other => panic!("expected an effectful wall, got {other:?}"),
        }
    }

    #[test]
    fn nested_call_arg_materializes_into_owned_temp() {
        use almide_lang::intern::sym;
        // var x = outer(inner())  — inner()'s heap result is materialized into an
        // owned temp, borrowed into outer, and dropped at scope end; outer's result
        // is bound and dropped. Two CallFns emitted, in evaluation order.
        let named = |n: &str, args: Vec<IrExpr>| {
            ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym(n) },
                    args,
                    type_args: vec![],
                },
                list_int(),
            )
        };
        let b = body(vec![bind(0, list_int(), named("outer", vec![named("inner", vec![])]))]);
        let mir = lower_body(&b, "main").expect("nested call arg lowers");
        let callfns: Vec<&str> = mir
            .ops
            .iter()
            .filter_map(|o| match o {
                Op::CallFn { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(callfns, vec!["inner", "outer"], "inner materialized before outer");
        // The materialized temp + the outer result are both balanced (each `i`
        // matched by a scope-end `d`).
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn literal_call_arg_materializes_and_drops() {
        use almide_lang::intern::sym;
        // f("hello")  — the string literal argument is materialized via `Alloc`,
        // borrowed into the call, and dropped at scope end (cert `i` + `d`).
        let call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("f") },
                args: vec![ir_expr(IrExprKind::LitStr { value: "hello".into() }, Ty::String)],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: call })]);
        let mir = lower_body(&b, "main").expect("literal call arg lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::Alloc { .. })),
            "the literal is materialized via Alloc: {:?}",
            mir.ops
        );
        assert!(mir.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "f")));
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn elided_calls_in_an_opaque_value_emit_cert_neutral_effect_markers() {
        use almide_lang::intern::sym;
        let named = |n: &str, args: Vec<IrExpr>| {
            ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym(n) },
                    args,
                    type_args: vec![],
                },
                list_int(),
            )
        };
        // var xs = [helper(), other()]  — the list literal lowers to ONE Opaque
        // `Alloc`, ELIDING its element calls. `record_elided_calls` surfaces each as
        // a bare EFFECT MARKER `CallFn{dst:None, args:[], result:None}` so the caps
        // fold can see them, while the value content stays deferred.
        let elements = vec![named("helper", vec![]), named("other", vec![])];
        let b = body(vec![bind(0, list_int(), ir_expr(IrExprKind::List { elements }, list_int()))]);
        let mir = lower_body(&b, "main").expect("lowers");

        let markers: Vec<&str> = mir
            .ops
            .iter()
            .filter_map(|o| match o {
                Op::CallFn { dst: None, name, args, result: None } if args.is_empty() => {
                    Some(name.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(markers, vec!["helper", "other"], "one marker per elided call");

        // CERT-NEUTRAL: ownership is just the list Alloc (+1) and its scope-end Drop
        // (−1) — a marker injects no `+1`/drop. NAMES-NEUTRAL: a dst-less, arg-less
        // marker references nothing, so it cannot dangle.
        assert_eq!(verify_ownership(&mir), Ok(()));
        let cert = crate::certificate::ownership_certificate(&mir);
        assert_eq!(cert.matches('i').count(), 1, "only the list Alloc is a +1, not the markers");
        let nw = crate::certificate::name_witness(&mir);
        assert!(nw.used.iter().all(|u| nw.defined.contains(u)), "no dangling MIR reference");

        // A HIGHER-ORDER call is SKIPPED (unmodelled closure caps): no marker, so the
        // `ir_calls > mir_calls` gate keeps such a function honestly tainted.
        let fn_ty = Ty::Fn { params: vec![], ret: Box::new(Ty::Int) };
        let ho = body(vec![bind(
            1,
            list_int(),
            ir_expr(
                IrExprKind::List {
                    elements: vec![named(
                        "apply",
                        vec![ir_expr(IrExprKind::Var { id: VarId(2) }, fn_ty)],
                    )],
                },
                list_int(),
            ),
        )]);
        let mir2 = lower_body(&ho, "main").expect("lowers");
        assert!(
            !mir2.ops.iter().any(|o| matches!(o, Op::CallFn { dst: None, .. })),
            "a higher-order call is not recorded as a marker"
        );
    }

    fn bool_var() -> IrExpr {
        ir_expr(IrExprKind::Var { id: VarId(5) }, Ty::Bool)
    }
    fn unit_block(stmts: Vec<IrStmt>) -> IrExpr {
        ir_expr(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
    }
    fn iff(then: IrExpr, els: IrExpr, ty: Ty) -> IrExpr {
        ir_expr(
            IrExprKind::If { cond: Box::new(bool_var()), then: Box::new(then), else_: Box::new(els) },
            ty,
        )
    }

    #[test]
    fn for_in_heap_element_aliases_container_per_iteration() {
        use almide_lang::intern::sym;
        // var xs = []; for s in xs { println(s) }  — the heap loop var `s` aliases the
        // whole container `xs` (Op::Dup) for the iteration and is dropped at iteration
        // end; the println borrows it. Per-iteration frame balanced.
        let prn_s = stmt(IrStmtKind::Expr {
            expr: ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym("println") },
                    args: vec![ir_expr(IrExprKind::Var { id: VarId(1) }, Ty::String)],
                    type_args: vec![],
                },
                Ty::Unit,
            ),
        });
        let forin = ir_expr(
            IrExprKind::ForIn {
                var: VarId(1),
                var_tuple: None,
                iterable: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
                body: vec![prn_s],
            },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: forin }),
        ]);
        let mir = lower_body(&b, "main").expect("for-in lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::Dup { src: ValueId(0), .. })),
            "loop var aliases the container: {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()), "per-iteration frame balanced");
    }

    #[test]
    fn while_with_scalar_counter_reassign_lowers() {
        // var i = 0; while c { i = 5 }  — a SCALAR reassign is a Copy `Const` (no
        // handle), admitted; the body has no heap, so the loop lowers balanced.
        let inc = stmt(IrStmtKind::Assign {
            var: VarId(0),
            value: ir_expr(IrExprKind::LitInt { value: 5 }, Ty::Int),
        });
        let w = ir_expr(
            IrExprKind::While { cond: Box::new(bool_var()), body: vec![inc] },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, Ty::Int, ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int)),
            stmt(IrStmtKind::Expr { expr: w }),
        ]);
        let mir = lower_body(&b, "main").expect("while with scalar reassign lowers");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn loop_with_break_is_walled() {
        // while c { break }  — the early-exit path would skip the per-iteration frame's
        // drops (a leak). Must WALL.
        let w = ir_expr(
            IrExprKind::While {
                cond: Box::new(bool_var()),
                body: vec![stmt(IrStmtKind::Expr { expr: ir_expr(IrExprKind::Break, Ty::Unit) })],
            },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: w })]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("break/continue"), "got: {r}"),
            other => panic!("expected a break/continue wall, got {other:?}"),
        }
    }

    #[test]
    fn loop_body_heap_reassign_is_walled() {
        // var acc = []; while c { acc = [] }  — a HEAP reassign of a pre-loop var is an
        // iteration-dependent value_of rebind (→ UAF). Must WALL. (A scalar reassign
        // would be admitted — see `while_with_scalar_counter_reassign_lowers`.)
        let reassign = stmt(IrStmtKind::Assign {
            var: VarId(0),
            value: ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
        });
        let w = ir_expr(
            IrExprKind::While { cond: Box::new(bool_var()), body: vec![reassign] },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: w }),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("reassigns a heap"), "got: {r}"),
            other => panic!("expected a heap-reassign wall, got {other:?}"),
        }
    }

    #[test]
    fn unit_if_with_effect_arms_linearizes_balanced() {
        use almide_lang::intern::sym;
        // if c then println("a") else println("b")  — each arm is a Unit effect call;
        // its string arg is materialized into an arm-local temp and dropped by the
        // per-arm frame. BOTH printlns lower (caps union); ownership balanced.
        let prn = |s: &str| {
            ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym("println") },
                    args: vec![ir_expr(IrExprKind::LitStr { value: s.into() }, Ty::String)],
                    type_args: vec![],
                },
                Ty::Unit,
            )
        };
        let b = body(vec![stmt(IrStmtKind::Expr { expr: iff(prn("a"), prn("b"), Ty::Unit) })]);
        let mir = lower_body(&b, "main").expect("unit if lowers");
        let prints = mir.ops.iter().filter(|o| matches!(o, Op::Call { .. })).count();
        assert_eq!(prints, 2, "both arms' println are lowered (caps union, not Const-skipped)");
        assert_eq!(verify_ownership(&mir), Ok(()));
        let allocs = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(allocs, drops, "every arm-local alloc has its per-arm drop (balanced)");
    }

    #[test]
    fn if_arm_local_alloc_is_dropped_within_the_arm() {
        // if c then { var w = [1,2,3] } else { }  — w is an arm-local heap value,
        // dropped by the per-arm frame (vacuous on the else path). Cert balanced.
        let then = unit_block(vec![bind(
            0,
            list_int(),
            ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
        )]);
        let b = body(vec![stmt(IrStmtKind::Expr {
            expr: iff(then, unit_block(vec![]), Ty::Unit),
        })]);
        let mir = lower_body(&b, "main").expect("lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Alloc { .. })), "arm-local alloc");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Drop { .. })), "arm-local drop");
        assert_eq!(verify_ownership(&mir), Ok(()), "arm balanced by construction");
    }

    #[test]
    fn scalar_if_tail_linearizes_arms_and_const_merges() {
        // fn f() = if c then 1 else 2  — arms lowered (for caps), result is ONE Const.
        let b = ir_expr(
            IrExprKind::Block {
                stmts: vec![],
                expr: Some(Box::new(iff(
                    ir_expr(IrExprKind::LitInt { value: 1 }, Ty::Int),
                    ir_expr(IrExprKind::LitInt { value: 2 }, Ty::Int),
                    Ty::Int,
                ))),
            },
            Ty::Int,
        );
        let mir = lower_body(&b, "f").expect("scalar if tail lowers");
        assert!(matches!(mir.ops.last(), Some(Op::Const { .. })), "merged scalar result is a Const");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn heap_result_if_yields_one_fresh_opaque_merged_slot() {
        use almide_lang::intern::sym;
        // fn f() = if c then make() else [9]  — a HEAP-result branch. Arms are
        // linearized (each per-arm balanced; the make() call's caps captured, its
        // value deferred), and the result is ONE fresh `Alloc{Opaque}` MOVED OUT (the
        // merged slot) — never per-arm phi-merged. Balanced + moved out by the cert.
        let then = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("make") },
                args: vec![],
                type_args: vec![],
            },
            list_int(),
        );
        let els = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let b = ir_expr(
            IrExprKind::Block { stmts: vec![], expr: Some(Box::new(iff(then, els, list_int()))) },
            list_int(),
        );
        let mir = lower_body(&b, "f").expect("heap if tail lowers");
        // The make() call's caps are captured as an effect marker (deferred value).
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { dst: None, name, .. } if name == "make")),
            "the arm call's caps are captured: {:?}",
            mir.ops
        );
        // The merged result is the LAST op: a fresh Opaque Alloc, MOVED OUT (returned,
        // so NOT dropped at scope end).
        assert!(matches!(mir.ops.last(), Some(Op::Alloc { init: Init::Opaque, .. })));
        assert!(mir.ret.is_some(), "the fresh merged result is the return value");
        assert!(!mir.ops.iter().any(|o| matches!(o, Op::Drop { .. })), "moved out, not dropped");
        assert_eq!(verify_ownership(&mir), Ok(()), "fresh result + balanced arms");
    }

    #[test]
    fn heap_result_if_bind_drops_the_merged_slot_at_scope_end() {
        // var x = if c then [1] else [2]  (Unit body) — the merged fresh Opaque is
        // BOUND and dropped at scope end (cert i + d, balanced).
        let then = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let els = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let b = body(vec![bind(0, list_int(), iff(then, els, list_int()))]);
        let mir = lower_body(&b, "main").expect("heap if bind lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Alloc { init: Init::Opaque, .. })));
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Drop { .. })), "bound slot dropped at scope end");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn branch_arm_reassigning_a_variable_is_walled() {
        // var z = []; if c then { z = [9] } else { }  — the arm reassigns pre-branch z
        // (a path-dependent value_of rebind → UAF the flat fold can't see). Must WALL.
        let then = unit_block(vec![stmt(IrStmtKind::Assign {
            var: VarId(0),
            value: ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
        })]);
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: iff(then, unit_block(vec![]), Ty::Unit) }),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("reassigns"), "got: {r}"),
            other => panic!("expected a reassign wall, got {other:?}"),
        }
    }

    #[test]
    fn match_arm_binding_pattern_is_walled() {
        // match n { x => () }  — a Bind pattern extracts a value whose position needs
        // the layout brick (#54). Must WALL (no silent miscompile).
        let arm = almide_ir::IrMatchArm {
            pattern: IrPattern::Bind { var: VarId(1), ty: Ty::Int },
            guard: None,
            body: ir_expr(IrExprKind::Unit, Ty::Unit),
        };
        let m = ir_expr(
            IrExprKind::Match {
                subject: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::Int)),
                arms: vec![arm],
            },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, Ty::Int, ir_expr(IrExprKind::LitInt { value: 3 }, Ty::Int)),
            stmt(IrStmtKind::Expr { expr: m }),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("binds a value"), "got: {r}"),
            other => panic!("expected a pattern-binding wall, got {other:?}"),
        }
        // A NON-binding pattern (Some(_) / None / _) is admitted by the gate.
        assert!(pattern_binds_nothing(&IrPattern::Wildcard));
        assert!(pattern_binds_nothing(&IrPattern::Some { inner: Box::new(IrPattern::Wildcard) }));
        assert!(!pattern_binds_nothing(&IrPattern::Some {
            inner: Box::new(IrPattern::Bind { var: VarId(1), ty: Ty::Int })
        }));
    }

    #[test]
    fn match_on_a_fresh_heap_subject_is_walled() {
        use almide_lang::intern::sym;
        // match make() { _ => () }  — a fresh heap subject would need materialize-and-
        // drop; eliding its Alloc would be an accept-but-unsafe leak. Must WALL.
        let subject = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("make") },
                args: vec![],
                type_args: vec![],
            },
            list_int(),
        );
        let arm = almide_ir::IrMatchArm {
            pattern: IrPattern::Wildcard,
            guard: None,
            body: ir_expr(IrExprKind::Unit, Ty::Unit),
        };
        let m = ir_expr(
            IrExprKind::Match { subject: Box::new(subject), arms: vec![arm] },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: m })]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("fresh heap subject"), "got: {r}"),
            other => panic!("expected a fresh-heap-subject wall, got {other:?}"),
        }
    }

    #[test]
    fn option_result_constructor_lowers_like_a_literal() {
        // var x = Some("hi")  — a heap Option variant is materialized via `Alloc`
        // (value semantics: the payload is copied, the shell owned + dropped),
        // exactly like a container literal. `list_int()` stands in as a heap type;
        // the lowering keys on the expression KIND + `is_heap_ty`, not the payload.
        let some = ir_expr(
            IrExprKind::OptionSome {
                expr: Box::new(ir_expr(IrExprKind::LitStr { value: "hi".into() }, Ty::String)),
            },
            list_int(),
        );
        let b = body(vec![bind(0, list_int(), some)]);
        let mir = lower_body(&b, "main").expect("Option constructor lowers");
        assert!(
            matches!(mir.ops[0], Op::Alloc { .. }),
            "the constructor is materialized via Alloc: {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn binop_value_materializes_scalar_const_and_heap_alloc() {
        use almide_ir::BinOp;
        use almide_lang::intern::sym;
        let binop = |op, ty, l: IrExpr, r: IrExpr| {
            ir_expr(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) }, ty)
        };
        let v = |id| ir_expr(IrExprKind::Var { id: VarId(id) }, Ty::Int);
        // f(a + b)  — a scalar BinOp argument is a fresh `Const` (no ownership).
        let scalar = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("f") },
                args: vec![binop(BinOp::AddInt, Ty::Int, v(0), v(1))],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let mir = lower_body(&body(vec![stmt(IrStmtKind::Expr { expr: scalar })]), "main")
            .expect("scalar BinOp arg lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Const { .. })), "scalar BinOp is Const: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()));

        // var s = a ++ b  — a heap (string-concat) BinOp is a fresh `Alloc`, dropped.
        let sv = |id| ir_expr(IrExprKind::Var { id: VarId(id) }, Ty::String);
        let concat = binop(BinOp::ConcatStr, Ty::String, sv(0), sv(1));
        let mir2 = lower_body(&body(vec![bind(2, Ty::String, concat)]), "main")
            .expect("heap concat bind lowers");
        assert!(matches!(mir2.ops[0], Op::Alloc { .. }), "heap BinOp is Alloc: {:?}", mir2.ops);
        assert_eq!(verify_ownership(&mir2), Ok(()));
    }

    #[test]
    fn scalar_extraction_is_const_heap_extraction_aliases_container() {
        use almide_lang::intern::sym;
        let idx = |obj: IrExpr, ty: Ty| {
            ir_expr(
                IrExprKind::IndexAccess {
                    object: Box::new(obj),
                    index: Box::new(ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int)),
                },
                ty,
            )
        };
        let c = || ir_expr(IrExprKind::Var { id: VarId(0) }, list_int());

        // fn f() = xs[i]  with a SCALAR element type → a `Const` copy (no ownership).
        let scalar = idx(c(), Ty::Int);
        let mir = lower_body(
            &ir_expr(IrExprKind::Block { stmts: vec![], expr: Some(Box::new(scalar)) }, Ty::Int),
            "main",
        )
        .expect("scalar extraction lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Const { .. })), "scalar extraction is Const: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()));

        // var xs = [..]; f(xs[0])  with a HEAP element → ALIAS the container (Op::Dup),
        // borrowed into the call and dropped at scope end (cert `a` + `d`).
        let heap_call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("f") },
                args: vec![idx(c(), Ty::String)],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: heap_call }),
        ]);
        let mir2 = lower_body(&b, "main").expect("heap extraction aliases the container");
        assert!(mir2.ops.iter().any(|o| matches!(o, Op::Dup { .. })), "heap extraction is a container Dup: {:?}", mir2.ops);
        assert_eq!(verify_ownership(&mir2), Ok(()));

        // A NESTED-container extraction (the immediate container is itself an
        // extraction, not a tracked var) stays walled — there is no `src` to Dup.
        let nested = ir_expr(
            IrExprKind::Member { object: Box::new(idx(c(), list_int())), field: sym("x") },
            Ty::String,
        );
        let nested_call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("g") },
                args: vec![nested],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b2 = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: nested_call }),
        ]);
        match lower_body(&b2, "main") {
            Err(LowerError::Unsupported(m)) => assert!(m.contains("not a tracked heap var"), "got: {m}"),
            other => panic!("expected a nested-container wall, got {other:?}"),
        }
    }

    #[test]
    fn reassignment_rebinds_and_old_rides_to_scope_end() {
        use almide_lang::intern::sym;
        // var x = [..]; x = [..]  — old + new both allocated, both dropped (the old
        // rides to scope-end, dropped exactly once; never a double-free).
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Assign {
                var: VarId(0),
                value: ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
            }),
        ]);
        let mir = lower_body(&b, "main").expect("reassignment lowers");
        let allocs = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(allocs, 2, "old + new both allocated: {:?}", mir.ops);
        assert_eq!(drops, 2, "old + new both dropped: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()));

        // var x = [..]; x = f(x)  — reading the old x in the new value borrows the
        // still-live old handle (the read lowers before the rebind), NOT a UAF.
        let b2 = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Assign {
                var: VarId(0),
                value: ir_expr(
                    IrExprKind::Call {
                        target: CallTarget::Named { name: sym("f") },
                        args: vec![ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())],
                        type_args: vec![],
                    },
                    list_int(),
                ),
            }),
        ]);
        let mir2 = lower_body(&b2, "main").expect("reassign reading old x lowers");
        assert_eq!(verify_ownership(&mir2), Ok(()), "no UAF reading old x: {:?}", mir2.ops);
    }

    #[test]
    fn tuple_destructure_aliases_components() {
        let heap_binds = || {
            IrPattern::Tuple {
                elements: vec![
                    IrPattern::Bind { var: VarId(2), ty: list_int() },
                    IrPattern::Bind { var: VarId(3), ty: list_int() },
                ],
            }
        };
        // var x; var y; let (a, b) = (x, y)  — component-wise: a aliases x, b aliases y.
        let tup_lit = ir_expr(
            IrExprKind::Tuple {
                elements: vec![
                    ir_expr(IrExprKind::Var { id: VarId(0) }, list_int()),
                    ir_expr(IrExprKind::Var { id: VarId(1) }, list_int()),
                ],
            },
            list_int(),
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(1, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::BindDestructure { pattern: heap_binds(), value: tup_lit }),
        ]);
        let mir = lower_body(&b, "main").expect("tuple-literal destructure lowers");
        assert_eq!(
            mir.ops.iter().filter(|o| matches!(o, Op::Dup { .. })).count(),
            2,
            "a aliases x, b aliases y: {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()));

        // var t; let (a, b) = t  — each heap component aliases the container t.
        let b2 = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::BindDestructure {
                pattern: heap_binds(),
                value: ir_expr(IrExprKind::Var { id: VarId(0) }, list_int()),
            }),
        ]);
        let mir2 = lower_body(&b2, "main").expect("container-var destructure lowers");
        assert_eq!(
            mir2.ops.iter().filter(|o| matches!(o, Op::Dup { .. })).count(),
            2,
            "both components alias t: {:?}",
            mir2.ops
        );
        assert_eq!(verify_ownership(&mir2), Ok(()));
    }
}
