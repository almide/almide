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
            // An ERROR OPERATOR (`e!`/`e?`/`e ?? d`/`e?.f`) likewise yields a FRESH
            // value (the unwrapped/defaulted/chained result, deferred like every
            // Opaque); its operand's calls are captured by `record_elided_calls`. The
            // EARLY-RETURN of `Try`/`Unwrap` is deferred (the always-continue path is
            // self-consistent — each handle still drops exactly once, so memory-safe;
            // error PROPAGATION is functional, not a safety property).
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
            | IrExprKind::UnOp { .. }
            | IrExprKind::Try { .. }
            | IrExprKind::Unwrap { .. }
            | IrExprKind::UnwrapOr { .. }
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. } => {
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
    fn lower_pure_module_value_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Result<ValueId, LowerError> {
        let lowered = self.lower_pure_module_call_args(module, func, args)?;
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

    /// Admission + closure-capability capture shared by a stdlib `Module` call in any
    /// position (value or effect). Requires PURITY (the combinator's OWN caps must be
    /// ∅ — an effectful call would omit its capability, accept-but-unsafe). Captures
    /// each closure ARGUMENT's capabilities while DEFERRING its value and BORROWING
    /// its captures: a `Lambda` body's calls become effect markers, a `ClosureCreate`/
    /// `FnRef` named callee a marker; an OPAQUE function value (unanalyzable caps) is
    /// walled. Returns the lowered REGULAR (non-closure) args. The pure combinator
    /// invokes-and-discards the closure, so its captures never escape — see
    /// [`Self::lower_pure_module_value_call`].
    fn lower_pure_module_call_args(
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
        let mut regular: Vec<IrExpr> = Vec::new();
        for a in args {
            match &a.kind {
                IrExprKind::Lambda { body, .. } => self.record_elided_calls(body),
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
                _ => regular.push(a.clone()),
            }
        }
        self.lower_call_args(&regular)
    }

    /// Lower a pure `Module` COMBINATOR applied for its EFFECT (`list.each(xs, f)` in
    /// statement position) — the side effect is the CLOSURE's, captured by
    /// [`Self::lower_pure_module_call_args`]. A Unit/scalar result carries no
    /// ownership; a (rarely) discarded HEAP result is allocated and dropped at scope
    /// end (value semantics — never leaked).
    fn lower_effect_module_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Result<(), LowerError> {
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
                | IrExprKind::UnOp { .. }
                | IrExprKind::Try { .. }
                | IrExprKind::Unwrap { .. }
                | IrExprKind::UnwrapOr { .. }
                | IrExprKind::ToOption { .. }
                | IrExprKind::OptionalChain { .. } => {
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
            | IrExprKind::TupleIndex { .. }
            // A SCALAR error-operator result (`x!`/`x ?? d`/`x?.f` yielding a scalar) is
            // likewise a fresh `Const`; the operator's value + early-return are deferred.
            | IrExprKind::Try { .. }
            | IrExprKind::Unwrap { .. }
            | IrExprKind::UnwrapOr { .. }
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. } => {
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
            CallTarget::Computed { .. } => {
                return Err(LowerError::Unsupported(
                    "effect Computed call (closure-value callee) not in this brick".into(),
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
    /// A heap `match` SUBJECT is materialized (a fresh value into an owned temp dropped
    /// at the outer scope, a tracked var borrowed) so its `Alloc` is never elided.
    /// WALLED (each an explicit `Unsupported`, never a silent miscompile): a
    /// payload-BINDING `match` pattern (extracting a field needs the layout brick), a
    /// `match` arm GUARD, and an arm that REASSIGNS a variable (a path-dependent
    /// `value_of` rebind the flat fold cannot see → UAF).
    fn lower_branch(&mut self, expr: &IrExpr) -> Result<(), LowerError> {
        match &expr.kind {
            IrExprKind::If { cond, then, else_ } => {
                // The condition is evaluated ONCE before the branch — it is scalar
                // (Bool), so no ownership, but capture the caps of any call in it.
                self.record_elided_calls(cond);
                self.lower_branch_arm(None, then)?;
                self.lower_branch_arm(None, else_)?;
                Ok(())
            }
            IrExprKind::Match { subject, arms } => {
                // The subject is inspected once. A heap subject goes through
                // `lower_call_args` — an already-tracked `Var` is BORROWED, a FRESH
                // heap value (a call/literal result) is MATERIALIZED into an owned temp
                // dropped at the OUTER scope (never leaked — eliding its `Alloc` would
                // be accept-but-unsafe). A scalar subject carries no ownership; capture
                // any call in it for caps. Its ValueId (when heap) is the container a
                // payload-binding pattern aliases per arm.
                let subject_value: Option<ValueId> = if is_heap_ty(&subject.ty) {
                    match self.lower_call_args(std::slice::from_ref(subject))?.into_iter().next() {
                        Some(CallArg::Handle(v)) => Some(v),
                        _ => None,
                    }
                } else {
                    self.record_elided_calls(subject);
                    None
                };
                for arm in arms {
                    // A guard is a deferred heap-touching sub-computation — walled.
                    if arm.guard.is_some() {
                        return Err(LowerError::Unsupported(
                            "match arm guard not in this control-flow slice".into(),
                        ));
                    }
                    self.lower_branch_arm(Some((&arm.pattern, subject_value)), &arm.body)?;
                }
                Ok(())
            }
            other => Err(LowerError::Unsupported(format!(
                "lower_branch on a non-branch {}",
                kind_name(other)
            ))),
        }
    }

    /// Lower ONE branch arm into the flat op stream with a PER-ARM SCOPE FRAME:
    /// snapshot the live-handle count, lower the arm, then DROP every handle the arm
    /// added (so the arm is internally balanced, and vacuous when the other arm runs).
    /// The arm's result is DISCARDED (Unit/statement) or a SCALAR the caller merges
    /// into one `Const`; a heap result is walled. See [`Self::lower_branch`].
    ///
    /// For a `match` arm, `pattern` is `Some((pat, subject))` — the pattern's bound
    /// variables are introduced at the START of the frame (so they drop with the arm):
    /// a HEAP payload aliases the whole SUBJECT (`Op::Dup` — container-grain, like a
    /// field extraction; element/payload-PRECISE identity needs the layout brick),
    /// a SCALAR payload is a `Const`. See [`Self::bind_pattern`].
    fn lower_branch_arm(
        &mut self,
        pattern: Option<(&IrPattern, Option<ValueId>)>,
        body: &IrExpr,
    ) -> Result<(), LowerError> {
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
        if let Some((pat, subject)) = pattern {
            self.bind_pattern(pat, subject)?;
        }
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
    fn bind_pattern(
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
                | IrExprKind::OptionalChain { .. } => {
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
mod tests;
