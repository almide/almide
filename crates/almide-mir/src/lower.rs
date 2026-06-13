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
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrParam, IrStmt, IrStmtKind, VarId};
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
    Ok(MirFunction {
        name: func.name.as_str().to_string(),
        params,
        ops: ctx.ops,
        ret,
        ..Default::default()
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
            IrStmtKind::IndexAssign { target, .. } => {
                // Copy-on-write: the write must land on a uniquely-owned buffer.
                let v = self.value_for(*target)?;
                if self.param_values.contains(&v) {
                    // Mutating a borrowed param (the caller's data) is outside the
                    // borrow-by-default first brick — it needs the move-mode /
                    // unique-acquire calling convention. Wall it (totality).
                    return Err(LowerError::Unsupported(
                        "in-place mutation of a borrowed param not in this brick".into(),
                    ));
                }
                self.ops.push(Op::MakeUnique { v });
                Ok(())
            }
            // A bare expression statement that is an EFFECT call (`println(s)`).
            // Non-call expr statements stay Unsupported (the lower_effect_call
            // guard rejects them — flight-grade totality).
            IrStmtKind::Expr { expr } => self.lower_effect_call(expr),
            // A source comment carries no ownership — skip it (it is not a
            // "silent drop": Comment is a no-op by definition, not an unhandled op).
            IrStmtKind::Comment { .. } => Ok(()),
            other => Err(LowerError::Unsupported(format!(
                "statement {} not in the value-semantics subset",
                stmt_kind_name(other)
            ))),
        }
    }

    fn lower_bind(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        if !is_heap_ty(ty) {
            // Scalar binding: define a Copy value, no ownership accounting.
            let dst = self.fresh_value();
            self.value_of.insert(var, dst);
            self.ops.push(Op::Const { dst });
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
            // A fresh heap value (literal container / string).
            IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. }
            | IrExprKind::EmptyMap
            | IrExprKind::Record { .. }
            | IrExprKind::Tuple { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::StringInterp { .. } => {
                let dst = self.fresh_value();
                let repr = repr_of(ty)?;
                let init = alloc_init(value);
                self.value_of.insert(var, dst);
                self.ops.push(Op::Alloc { dst, repr, init });
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
                | IrExprKind::StringInterp { .. } => {
                    let dst = self.fresh_value();
                    let repr = repr_of(&tail.ty)?;
                    let init = alloc_init(tail);
                    self.ops.push(Op::Alloc { dst, repr, init });
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
                other => Err(LowerError::Unsupported(format!(
                    "heap move-out from {} (only a bound var, fresh literal, or call) not in this brick",
                    kind_name(other)
                ))),
            };
        }
        // Scalar return value (Copy — no ownership accounting).
        match &tail.kind {
            IrExprKind::Var { id } => Ok(Some(self.value_for(*id)?)),
            IrExprKind::LitInt { .. } | IrExprKind::LitBool { .. } | IrExprKind::LitFloat { .. } => {
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
            // println(s) — s a bound heap string: borrow it for a Stdout write.
            ("println", [arg]) if is_heap_ty(&arg.ty) => match &arg.kind {
                IrExprKind::Var { id } => {
                    let v = self.value_for(*id)?;
                    self.ops.push(Op::Call {
                        dst: None,
                        func: RtFn::PrintStr,
                        args: vec![CallArg::Handle(v)],
                    result: None });
                    Ok(())
                }
                other => Err(LowerError::Unsupported(format!(
                    "println of {} (only a bound heap var) not in this brick",
                    kind_name(other)
                ))),
            },
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
                | IrExprKind::StringInterp { .. } => {
                    let dst = self.fresh_value();
                    let repr = repr_of(&a.ty)?;
                    let init = alloc_init(a);
                    self.ops.push(Op::Alloc { dst, repr, init });
                    self.materialized_call_arg(dst, repr)
                }
                // A scalar literal argument (`f(3.14)`, `f(true)`): a fresh `Const`,
                // passed by value (no ownership). `LitInt` is already an `Imm` above.
                IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. } => {
                    let dst = self.fresh_value();
                    self.ops.push(Op::Const { dst });
                    CallArg::Scalar(dst)
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
}
