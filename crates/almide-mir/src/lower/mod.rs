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

use crate::{Init, MirFunction, MirParam, Op, Repr, ValueId, PLACEHOLDER_LAYOUT};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrFunction, IrParam, IrStmt, IrStmtKind, VarId,
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
pub fn lower_function(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
) -> Result<MirFunction, LowerError> {
    // The main function only; any lambda-lifted auxiliaries are dropped (callers that
    // need them — render/verify paths — use `lower_function_all`). Sound while no lambda
    // lifting is wired (lifted is empty); when it is, those paths verify the auxiliaries.
    let mut all = lower_function_all(func, globals)?;
    Ok(all.remove(0))
}

/// Lower a function to its MIR plus any lambda-lifted auxiliary functions (index 0 is the
/// main function). The closures machinery lifts `let f = (x) => …` bodies into fresh
/// functions accumulated in `LowerCtx::lifted`; this returns them so the program assembler
/// can table + verify them. With no lifting wired the result is just `[main]`.
pub fn lower_function_all(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
) -> Result<Vec<MirFunction>, LowerError> {
    let mut ctx = LowerCtx {
        globals: globals.clone(),
        fn_name: func.name.as_str().to_string(),
        ..Default::default()
    };
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
    let lifted = std::mem::take(&mut ctx.lifted);
    let main = MirFunction {
        name: func.name.as_str().to_string(),
        params,
        ops: ctx.ops,
        ret,
        declared_caps,
    };
    let mut all = vec![main];
    all.extend(lifted);
    Ok(all)
}

/// Lower a function body expression to MIR (the param-free testable core;
/// `lower_function` is the wrapper that seeds parameters first).
pub fn lower_body(body: &IrExpr, name: &str) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx::default();
    let ret = ctx.lower_body_into(body)?;
    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() })
}

/// Like [`lower_body`] but returns the main function PLUS any lambda-lifted auxiliaries
/// the body produced (index 0 is the main). The plain [`lower_body`] discards the lifted
/// set, so a test that lifts a closure must use this to see (and verify) the lifted
/// function where the closure's body — and its captured calls — now live.
#[cfg(test)]
pub(crate) fn lower_body_all(body: &IrExpr, name: &str) -> Result<Vec<MirFunction>, LowerError> {
    let mut ctx = LowerCtx { fn_name: name.to_string(), ..Default::default() };
    let ret = ctx.lower_body_into(body)?;
    let lifted = std::mem::take(&mut ctx.lifted);
    let mut all =
        vec![MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() }];
    all.extend(lifted);
    Ok(all)
}

/// Like [`lower_body`] but seeds the declared GLOBAL set (top-level `let`s) so a
/// reference to one is admitted by `value_or_global` instead of walled. Test/diagnostic
/// entry — `lower_function` builds the same context for real programs.
#[cfg(test)]
pub(crate) fn lower_body_with_globals(
    body: &IrExpr,
    name: &str,
    globals: HashMap<VarId, Ty>,
) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx { globals, ..Default::default() };
    let ret = ctx.lower_body_into(body)?;
    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() })
}

#[derive(Default)]
pub(crate) struct LowerCtx {
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
    /// Depth of enclosing control-flow FRAMES (branch arms / loop bodies). A heap
    /// reassignment at depth > 0 must NOT rebind `value_of` — the new handle would
    /// be frame-local (dropped at the frame's end), yet the var is read on the next
    /// iteration or after the branch merges, dereferencing a freed handle (a UAF the
    /// flat fold cannot see). Inside a frame such a reassignment is DEFERRED: the var
    /// keeps its still-live handle and the new value is carried like every `Opaque`.
    in_frame: u32,
    /// Depth of enclosing SCALAR-STATE loops being lowered with real markers
    /// (`LoopStart`/`LoopBreakUnless`/`LoopEnd`). When > 0, a scalar `Assign` reassigns
    /// the var's STABLE local via [`Op::SetLocal`] (the loop-carried state) instead of
    /// rebinding `value_of` to a fresh value (which a loop back-edge could not see), and a
    /// HEAP reassignment ERRORS — that aborts the scalar-loop attempt so `lower_while`
    /// falls back to its sound model-one-iteration form (a heap accumulator is deferred,
    /// not run, exactly as before).
    scalar_loop_depth: u32,
    /// The module's top-level `let` bindings (VarId → declared Ty). A reference to one
    /// of these resolves to no FUNCTION-local `value_of` entry; this DECLARED set lets
    /// `value_or_global` distinguish a legitimate global reference (materialize a fresh
    /// external value) from a genuine lowering gap (a local that should have been bound
    /// — still WALLED). Confirming against the declared set, not merely a `value_of`
    /// miss, is what keeps the boundary a wall instead of a silent hole.
    globals: HashMap<VarId, Ty>,
    /// MIR values KNOWN to be MATERIALIZED Options (the 0-or-1-element-list layout:
    /// `Some(x)` = `Init::OptSome` len=1, `None` = `Init::Opaque` len=0). A variant
    /// `match` may EXECUTE (read `len` as the tag, extract `data[0]`) ONLY over a
    /// subject in this set — every other Option (a closure/range/deferred `Opaque`, a
    /// non-self-host Option-returning call) is `Opaque` with len=0 and would MISREAD as
    /// `None`, so it keeps the sound LINEARIZED match. This is the gate that makes the
    /// len-as-tag execution safe without any global materialization invariant.
    materialized_options: HashSet<ValueId>,
    /// MIR values KNOWN to be MATERIALIZED Results (the DynListStr len-as-tag layout: `Ok(int)` =
    /// len 0 with the value in slot 0, `Err(string)` = len 1 owning the message). An `Ok`/`Err`
    /// `match` may EXECUTE (read `len` as the tag — len 0 → Ok, len != 0 → Err — and extract slot
    /// 0) ONLY over a subject in this set; any other Result is a deferred `Opaque` (len 0 → MISREADS
    /// as Ok) and keeps the sound LINEARIZED match. The Result analogue of `materialized_options`.
    materialized_results: HashSet<ValueId>,
    /// Lambda-lifted auxiliary functions produced while lowering this function's body
    /// (a non-capturing `let f = (x) => …` or a lambda call-argument lifts its body to a
    /// fresh MirFunction here, bound via `Op::FuncRef`). `lower_function_all` returns these
    /// alongside the main function so the program assembler tables + verifies them.
    lifted: Vec<crate::MirFunction>,
    /// The enclosing source function's name — the file-unique prefix for lifted lambda
    /// names (`__lambda_<fn_name>_<n>`). The corpus harness keys the in-profile map by name
    /// within a file, so two source functions each lifting `__lambda_0` would COLLIDE
    /// without this prefix (one lambda's certificate silently lost). Set by
    /// `lower_function_all`; empty for the param-free testable `lower_body` entry.
    fn_name: String,
    /// MIR values that denote a lifted lambda's table slot (an `Op::FuncRef` dst). A later
    /// call whose callee is one of these (`f(args)` where `f` bound a lifted lambda) lowers
    /// to `Op::CallIndirect` through it instead of deferring — the closure EXECUTES.
    funcref_values: HashSet<ValueId>,
    /// MIR values that are `List[String]` (NESTED-OWNERSHIP lists — their i64 slots hold OWNED
    /// String handles). A scope-end drop of one emits [`Op::DropListStr`] (recursive free),
    /// not a flat [`Op::Drop`] — so the element Strings are reclaimed. Populated when an
    /// `alloc_list_str` result or a `List[String]`-typed bind is created (Machinery 2).
    heap_elem_lists: HashSet<ValueId>,
    /// MIR values of the dynamic `Value` type (the Codec data model). A scope-end drop emits
    /// [`Op::DropValue`] (runtime-tag-dispatched: a Str/Array/Object Value frees its one heap
    /// payload, a scalar Value just frees the block) instead of a flat [`Op::Drop`]. Populated
    /// when a `Value`-typed bind is created.
    value_handles: HashSet<ValueId>,
}

/// Is `ty` the dynamic `Value` type (the Codec data model)? Its scope-end drop is the
/// runtime-tag-dispatched [`Op::DropValue`], since a heap-payload Value (Str/Array/Object) owns a
/// handle the flat `Drop` would leak.
pub(crate) fn is_value_ty(ty: &Ty) -> bool {
    match ty {
        Ty::Named(name, _) => name.as_str() == "Value",
        Ty::Variant { name, .. } => name.as_str() == "Value",
        _ => false,
    }
}

/// Is `ty` a `List[T]` / `Option[T]` whose element `T` is itself a HEAP type (e.g. `List[String]`,
/// `Option[String]`)? Such a container OWNS its element(s) — it needs the recursive
/// [`Op::DropListStr`], not a flat drop. An `Option[String]` is physically a 0-or-1-element
/// `List[String]` (Machinery 2), so the SAME recursive free applies (len 0 frees nothing, len 1
/// frees the one element + the block).
pub(crate) fn is_heap_elem_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        // `List[heap]` / `Option[heap]` / `Set[heap]` — heap element slots (DynListStr nested
        // ownership). A `Set[heap]` is physically a `List[heap]` of unique elements, so the SAME
        // recursive free applies (each owned element + the block).
        Ty::Applied(TypeConstructorId::List | TypeConstructorId::Option | TypeConstructorId::Set, args)
            if args.len() == 1 && is_heap_ty(&args[0]) =>
        {
            true
        }
        // `Result[_, heap-Err]` is physically the SAME DynListStr (the Ok/Err materialization reuses
        // it): `Err` owns the heap Err payload in slot 0 (len 1 → DropListStr frees it), `Ok` is
        // len 0 (frees nothing). So a Result value is dropped recursively, exactly like Option[heap].
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 && is_heap_ty(&args[1]) => {
            true
        }
        // `Map[heap, heap]` (e.g. `Map[String, String]`) — a DynListStr of INTERLEAVED key+value
        // String handles [k0,v0,k1,v1,...]; EVERY slot is a heap handle, so the uniform recursive
        // DropListStr frees all keys and values. (`len` = the slot count; map.len reads len/2.)
        Ty::Applied(TypeConstructorId::Map, args)
            if args.len() == 2 && is_heap_ty(&args[0]) && is_heap_ty(&args[1]) =>
        {
            true
        }
        _ => false,
    }
}

impl LowerCtx {
    pub(crate) fn fresh_value(&mut self) -> ValueId {
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
    pub(crate) fn bind_params(&mut self, params: &[IrParam]) -> Result<Vec<MirParam>, LowerError> {
        let mut out = Vec::new();
        for p in params {
            let v = self.fresh_value();
            self.value_of.insert(p.var, v);
            // A FUNCTION-typed param (`f: (Int) -> Int`, the closures machinery) is a SCALAR
            // table slot (an i64 index into the module function table), NOT a heap value:
            // the caller passes the lifted lambda's `FuncRef` value. So it gets a scalar
            // Repr and joins `funcref_values` — a `f(x)` call in the body then lowers to
            // `Op::CallIndirect` through it (the dynamic-closure path; cap_witness taints
            // it conservatively, so a higher-order function stays honestly caps-unverified).
            // This is what lets `list.map`/`filter`/`fold` be self-hosted in Almide.
            let repr = if matches!(p.ty, Ty::Fn { .. }) {
                let r = Repr::Scalar { width: crate::ScalarWidth::Double };
                self.funcref_values.insert(v);
                r
            } else {
                repr_of(&p.ty)? // Ptr (heap) / Scalar; Unsupported if Unknown or non-value
            };
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
    pub(crate) fn lower_body_into(&mut self, body: &IrExpr) -> Result<Option<ValueId>, LowerError> {
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

    pub(crate) fn lower_stmt(&mut self, stmt: &IrStmt) -> Result<(), LowerError> {
        // (The Try/Unwrap early-return-over-a-live-heap-local wall is LIFTED: the v0 wasm
        // codegen now frees the live heap locals before the Err-path `return_`
        // [emit_wasm: emit_early_return_decs], so the deferred-continue cert is faithful
        // on both targets — no leak. See docs/roadmap/active/v0-unwrap-early-return-leak.md.)
        match &stmt.kind {
            IrStmtKind::Bind { var, ty, value, .. } => self.lower_bind(*var, ty, value),
            // `x = value` — reassignment.
            //
            // At function TOP LEVEL: REBIND `x` to the new value (reusing
            // `lower_bind`). The OLD binding's handle stays in `live_heap_handles`
            // and is dropped at scope end — a conservative lifetime EXTENSION
            // (memory-safe, never a double-free: the old object is dropped exactly
            // once, at scope end, instead of at the reassignment). A read of the
            // old `x` inside `value` (e.g. `x = f(x)`) lowers BEFORE the rebind
            // overwrites `value_of[x]`, so it borrows the still-live old handle —
            // never a use-after-free.
            //
            // Inside a control-flow FRAME (`in_frame > 0`): a HEAP rebind would
            // repoint `value_of[x]` to a frame-local handle the per-iteration / per-arm
            // teardown drops, while `x` is read on the next iteration or after the
            // branch merges → UAF. So DEFER it — `x` keeps its still-live handle (the
            // loop/branch accumulator stays memory-safe), and the new value is carried
            // like every `Opaque`; capture its calls so the caps fold stays honest. A
            // SCALAR reassignment (`i = i + 1`) rebinds to a Copy `Const` with no handle
            // to dangle, so it is admitted unchanged (e.g. a loop counter).
            IrStmtKind::Assign { var, value } => {
                // Inside a scalar-marker loop, a reassignment mutates the var's STABLE
                // local (the loop-carried state) — `SetLocal`, not a fresh rebind. A heap
                // reassignment cannot run this way (the accumulator would need real heap
                // merge): ERROR to abort the attempt → `lower_while` falls back to its
                // sound model-one-iteration form.
                if self.scalar_loop_depth > 0 {
                    if is_heap_ty(&value.ty) {
                        return Err(LowerError::Unsupported(
                            "heap reassignment in a scalar loop body".into(),
                        ));
                    }
                    let local = *self.value_of.get(var).ok_or_else(|| {
                        LowerError::Unsupported("scalar loop reassigns an unbound var".into())
                    })?;
                    let src = self.lower_scalar_value(value).ok_or_else(|| {
                        LowerError::Unsupported("non-scalar value in a scalar loop reassignment".into())
                    })?;
                    self.ops.push(Op::SetLocal { local, src });
                    return Ok(());
                }
                if self.in_frame > 0 && is_heap_ty(&value.ty) {
                    self.record_elided_calls(value);
                    Ok(())
                } else {
                    self.lower_bind(*var, &value.ty, value)
                }
            }
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
            // `m[k] = v` — map insertion/update, in-place on the buffer. Like
            // `IndexAssign` it requires the map to be UNIQUELY owned (copy-on-write) →
            // `MakeUnique`. The key and value are deferred — record their calls so the
            // caps fold is not blind to their effects.
            IrStmtKind::MapInsert { target, key, value } => {
                self.lower_place_mutation(*target)?;
                self.record_elided_calls(key);
                self.record_elided_calls(value);
                Ok(())
            }
            // A bare expression statement: an `if`/`match` in statement position is
            // LINEARIZED (control flow), an EFFECT call (`println(s)`) is lowered as a
            // runtime effect. Other non-call expr statements stay Unsupported (the
            // lower_effect_call guard rejects them — flight-grade totality).
            IrStmtKind::Expr { expr } => match &expr.kind {
                // A Unit `if` statement EXECUTES (only the taken arm's effects run) when
                // its cond is a scalar; otherwise it falls back to the linearization.
                IrExprKind::If { cond, then, else_ }
                    if self.try_lower_unit_if(cond, then, else_) =>
                {
                    Ok(())
                }
                // A Unit `match` over INT literal patterns EXECUTES: desugar to a nested
                // `if subject == lit then arm else …` and run it via try_lower_unit_if
                // (only the matched arm's effects run). Non-literal patterns / guards / a
                // non-scalar subject fall back to the linearization below.
                IrExprKind::Match { subject, arms } => {
                    if let Some(if_expr) = self.desugar_match_to_if(subject, arms, &Ty::Unit) {
                        if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                            if self.try_lower_unit_if(cond, then, else_) {
                                return Ok(());
                            }
                        }
                    }
                    self.lower_branch(expr)
                }
                IrExprKind::If { .. } => self.lower_branch(expr),
                IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                    self.lower_for_in(*var, var_tuple, iterable, body)
                }
                IrExprKind::While { cond, body } => self.lower_while(cond, body),
                // A BLOCK expression statement (`{ stmts; e }` for its effect): lower
                // its statements (locals ride to the enclosing scope), then its tail —
                // a Unit effect call, a nested branch, or a deferred value whose calls
                // we capture (its value is discarded in statement position).
                IrExprKind::Block { stmts, expr: tail } => {
                    for s in stmts {
                        self.lower_stmt(s)?;
                    }
                    if let Some(t) = tail {
                        match &t.kind {
                            IrExprKind::Call { .. } if matches!(t.ty, Ty::Unit) => {
                                self.lower_effect_call(t)?
                            }
                            IrExprKind::If { .. } | IrExprKind::Match { .. } => {
                                self.lower_branch(t)?
                            }
                            _ => self.record_elided_calls(t),
                        }
                    }
                    Ok(())
                }
                // `break` / `continue` — a Unit-typed, value-less, label-less early exit
                // (Almide has no `break x`, no labels, no `return`). It adds NO ownership
                // op: the cert models the loop running to completion, with the
                // per-iteration frame's Drops intact. This is leak-safe ONLY when the
                // frame holds no heap handle a real early exit could skip — the loop
                // lowerers enforce that with a post-lowering frame check (a heap-frame
                // loop with break/continue is WALLED, because the v0 wasm backend frees
                // AFTER the break branch target and would leak).
                IrExprKind::Break | IrExprKind::Continue => Ok(()),
                _ => self.lower_effect_call(expr),
            },
            // A source comment carries no ownership — skip it (it is not a
            // "silent drop": Comment is a no-op by definition, not an unhandled op).
            IrStmtKind::Comment { .. } => Ok(()),
            // `guard cond else { body }` — a CONDITIONAL early exit. The guard adds NO
            // ownership: the model takes the always-CONTINUE path (success), which is
            // self-consistent and memory-safe; the failure path's early exit and the
            // `else` body's effects are DEFERRED, like every Opaque (the guard's job is
            // functional, not a safety property). Capture the caps of any call in the
            // condition or the else body so a printing/effectful guard taints honestly.
            IrStmtKind::Guard { cond, else_ } => {
                self.record_elided_calls(cond);
                self.record_elided_calls(else_);
                Ok(())
            }
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
    pub(crate) fn lower_place_mutation(&mut self, target: VarId) -> Result<(), LowerError> {
        let v = self.value_for(target)?;
        if self.param_values.contains(&v) {
            return Err(LowerError::Unsupported(
                "in-place mutation of a borrowed param not in this brick".into(),
            ));
        }
        self.ops.push(Op::MakeUnique { v });
        Ok(())
    }

    pub(crate) fn value_for(&self, var: VarId) -> Result<ValueId, LowerError> {
        self.value_of
            .get(&var)
            .copied()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))
    }

    /// Resolve a value-position variable reference, admitting a reference to a
    /// module-level `let` GLOBAL. A function-local var is in `value_of`. A miss is a
    /// global IFF it is in the DECLARED global set (`self.globals`) — the frontend
    /// guarantees every non-global reference is bound by a preceding local form, so a
    /// miss that is NOT a declared global is a genuine lowering gap and stays WALLED.
    ///
    /// A confirmed global is bound ONCE (cached in `value_of`, so repeated references
    /// reuse the one handle) as a fresh EXTERNAL value: a scalar global is a Copy
    /// `Const`; a heap global is a fresh owned `Alloc{Opaque}` dropped at scope end —
    /// we model an owned COPY rather than an alias of the module's object, which is
    /// memory-safe by construction (alloc once / drop once, the real global untouched)
    /// and its content deferred like every `Opaque`. Referencing a global does NOT
    /// re-run its initializer, so this adds no call/cap obligation.
    pub(crate) fn value_or_global(&mut self, var: VarId) -> Result<ValueId, LowerError> {
        if let Some(&v) = self.value_of.get(&var) {
            return Ok(v);
        }
        let ty = self
            .globals
            .get(&var)
            .cloned()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))?;
        let dst = self.fresh_value();
        if is_heap_ty(&ty) {
            let repr = repr_of(&ty)?;
            self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
            self.live_heap_handles.push(dst);
        } else {
            self.ops.push(Op::Const { dst });
        }
        self.value_of.insert(var, dst);
        Ok(dst)
    }

    pub(crate) fn emit_scope_end_drops(&mut self) {
        // Reverse binding order (LIFO scope teardown). A `List[String]` value is released by a
        // RECURSIVE `DropListStr` (frees its owned element Strings); every other heap value by
        // a flat `Drop`.
        let drops: Vec<Op> = self
            .live_heap_handles
            .iter()
            .rev()
            .map(|v| {
                if self.heap_elem_lists.contains(v) {
                    Op::DropListStr { v: *v }
                } else if self.value_handles.contains(v) {
                    Op::DropValue { v: *v }
                } else {
                    Op::Drop { v: *v }
                }
            })
            .collect();
        self.ops.extend(drops);
    }
}

mod binds;
mod tail;
mod control;
mod calls;


/// Does a statement list contain a `break`/`continue` that targets THIS loop — i.e.
/// not nested inside another loop (which captures its own)? Used to wall a loop body
/// whose early-exit path would skip the per-iteration frame's drops (a leak).
pub(crate) fn body_breaks_or_continues(stmts: &[IrStmt]) -> bool {
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
pub(crate) fn find_var_ty(stmts: &[IrStmt], var: VarId) -> Option<Ty> {
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
/// Does the stdlib `module.func` call return a real MATERIALIZED 0-or-1-element-list
/// Option (a self-host Option fn whose impl returns through tail-materialized `Some`/
/// `None`)? Its result may be tracked in `materialized_options` so a `match` over it
/// EXECUTES. The SINGLE SOURCE for both the bound-var path (binds.rs) and the direct-
/// subject path (control.rs) — keep them in sync to avoid tracking a non-materialized
/// call (which would misread as `None`). Add a name only when its self-host impl lands.
pub(crate) fn is_self_host_option_module_fn(module: &str, func: &str) -> bool {
    match module {
        "list" => {
            matches!(func, "get" | "first" | "last" | "index_of" | "binary_search" | "max" | "min" | "find" | "find_index" | "reduce" | "get_str" | "first_str" | "last_str")
        }
        "string" => matches!(func, "index_of" | "last_index_of" | "codepoint" | "first" | "last" | "get" | "strip_prefix" | "strip_suffix"),
        "bytes" => matches!(func, "get" | "index_of"),
        // result.to_option builds a materialized Option[Int] from a Result's len-tag (Ok → Some,
        // Err → None); option.map rebuilds a materialized Option (Some(f(x)) / None) — a `match`
        // over either result EXECUTES.
        "result" => matches!(func, "to_option" | "to_err_option"),
        "option" => matches!(func, "map" | "filter" | "flat_map" | "or_else" | "flatten"),
        // map.get(m, k) builds a materialized Option[Int] (Some(value) when the key is found via
        // the paired-slot scan, None otherwise) — a `match` over it EXECUTES.
        "map" => matches!(func, "get"),
        _ => false,
    }
}

/// Does `module.func` return a real MATERIALIZED `Result[Int, String]` (the DynListStr len-as-tag
/// layout)? Its result may be tracked in `materialized_results` so an `Ok`/`Err` `match` over it
/// EXECUTES. NARROW to fns actually self-hosted — any other Result is a deferred `Opaque` (len 0,
/// would misread as `Ok`). `int.parse` is the canonical for string.to_int/to_integer/parse_int.
/// The CallFn name for a stdlib `module.func` call, routing the REPR-POLYMORPHIC list combinators
/// to their `_str` variant when the RESULT is a `List[heap]` (e.g. `list.map` over a `List[String]`
/// → `list.map_str`, a DynListStr-result impl). The element repr (i64 vs i32 handle) demands a
/// separate variant; the variant reads/writes via the heap-aware prim ops. Scalar-result lists keep
/// the plain name. `module.func` is unchanged for everything else.
pub(crate) fn list_heap_call_name(module: &str, func: &str, arg_tys: &[Ty], result_ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    if module == "list" {
        // List[heap]-RETURNING combinators (the result is a new heap-element list).
        if matches!(func, "map" | "filter" | "reverse" | "take" | "drop" | "unique" | "dedup") {
            if let Ty::Applied(TypeConstructorId::List, args) = result_ty {
                if args.len() == 1 && is_heap_ty(&args[0]) {
                    return format!("list.{func}_str");
                }
            }
        }
        // Element-RETURNING accessors / search over a List[heap] (the result is an Option[heap]):
        // get/first/last (positional) + find (predicate higher-order).
        if matches!(func, "get" | "first" | "last" | "find") {
            if let Ty::Applied(TypeConstructorId::Option, args) = result_ty {
                if args.len() == 1 && is_heap_ty(&args[0]) {
                    return format!("list.{func}_str");
                }
            }
        }
        // SUBJECT-keyed (arg 0) over a List[heap], where the result is scalar (Bool/Int/Option[Int])
        // so it can't be keyed on the result type: search (contains/index_of) + the predicate
        // higher-order all/any/count.
        if matches!(func, "contains" | "index_of" | "all" | "any" | "count" | "fold") {
            if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
                if a.len() == 1 && is_heap_ty(&a[0]) {
                    return format!("list.{func}_str");
                }
            }
        }
    }
    if module == "set" {
        // `Set[heap]`-RETURNING constructors key on the RESULT element type; `set.to_list` over a
        // `Set[heap]` returns a `List[heap]`; the predicate `set.contains` keys on its SUBJECT
        // (arg 0) element type (its result is Bool). Each routes to the heap-element `_str` variant.
        let result_is_heap_container = matches!(
            result_ty,
            Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0])
        );
        // RESULT-keyed: constructors / Set-returning algebra over heap elements.
        if matches!(
            func,
            "from_list" | "to_list" | "union" | "intersection" | "difference"
                | "new" | "insert" | "remove" | "symmetric_difference" | "filter"
        ) && result_is_heap_container
        {
            return format!("set.{func}_str");
        }
        // ARG-keyed: a Bool/scalar-returning fn over a `Set[heap]` subject (arg 0).
        let arg0_is_heap_set = matches!(
            arg_tys.first(),
            Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && is_heap_ty(&a[0])
        );
        if matches!(func, "contains" | "is_subset" | "is_disjoint" | "all" | "any" | "fold")
            && arg0_is_heap_set
        {
            return format!("set.{func}_str");
        }
    }
    if module == "map" {
        // A `Map[heap, heap]` (e.g. Map[String,String]) routes to the `_str` variant. new/set
        // RETURN a Map[heap,heap]; get returns Option[heap]; len/is_empty/contains read a
        // Map[heap,heap] SUBJECT (arg 0, scalar result).
        let result_is_heap_map = matches!(
            result_ty,
            Ty::Applied(TypeConstructorId::Map, a)
                if a.len() == 2 && is_heap_ty(&a[0]) && is_heap_ty(&a[1])
        );
        if matches!(func, "new" | "set") && result_is_heap_map {
            return format!("map.{func}_str");
        }
        if func == "get" {
            if let Ty::Applied(TypeConstructorId::Option, a) = result_ty {
                if a.len() == 1 && is_heap_ty(&a[0]) {
                    return "map.get_str".to_string();
                }
            }
        }
        let arg0_is_heap_map = matches!(
            arg_tys.first(),
            Some(Ty::Applied(TypeConstructorId::Map, a))
                if a.len() == 2 && is_heap_ty(&a[0]) && is_heap_ty(&a[1])
        );
        if matches!(func, "len" | "is_empty" | "contains") && arg0_is_heap_map {
            return format!("map.{func}_str");
        }
    }
    format!("{module}.{func}")
}

pub(crate) fn is_self_host_result_module_fn(module: &str, func: &str) -> bool {
    matches!(
        (module, func),
        ("int", "parse")
            | ("int", "from_hex")
            | ("option", "to_result")
            | ("result", "map")
            | ("result", "flat_map")
            | ("result", "map_err")
            // value.as_int/as_bool/as_float build a materialized Result[T, String] (Ok(payload)
            // on a tag match, else Err("expected T")) — a `match` over the result EXECUTES.
            | ("value", "as_int")
            | ("value", "as_bool")
            | ("value", "as_float")
    )
}

pub(crate) fn alloc_init(value: &IrExpr) -> Init {
    if let IrExprKind::LitStr { value } = &value.kind {
        return Init::Str(value.clone());
    }
    if let IrExprKind::List { elements } = &value.kind {
        // A list of scalar literals materializes its slots: an Int element stores its value, a
        // Float element stores its f64 BITS (the i64-uniform Float repr — a `List[Float]` slot
        // is read back via load64 + ffrombits). A mixed/non-literal list stays Opaque.
        let ints: Option<Vec<i64>> = elements
            .iter()
            .map(|e| match &e.kind {
                IrExprKind::LitInt { value } => Some(*value),
                IrExprKind::LitFloat { value } => Some(value.to_bits() as i64),
                _ => None,
            })
            .collect();
        if let Some(ints) = ints {
            return Init::IntList(ints);
        }
    }
    Init::Opaque
}

pub(crate) fn stmt_kind_name(k: &IrStmtKind) -> &'static str {
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
pub(crate) fn extraction_container(expr: &IrExpr) -> Option<&IrExpr> {
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
pub(crate) fn is_higher_order(args: &[IrExpr]) -> bool {
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
pub(crate) fn call_target_kind(t: &CallTarget) -> &'static str {
    match t {
        CallTarget::Named { .. } => "Named",
        CallTarget::Module { .. } => "Module",
        CallTarget::Method { .. } => "Method",
        CallTarget::Computed { .. } => "Computed",
    }
}

pub(crate) fn kind_name(k: &IrExprKind) -> &'static str {
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
        IrExprKind::Await { .. } => "Await",
        IrExprKind::Clone { .. } => "Clone",
        IrExprKind::Deref { .. } => "Deref",
        IrExprKind::Borrow { .. } => "Borrow",
        IrExprKind::ToVec { .. } => "ToVec",
        IrExprKind::BoxNew { .. } => "BoxNew",
        IrExprKind::SpreadRecord { .. } => "SpreadRecord",
        _ => "<other>",
    }
}

#[cfg(test)]
mod tests;

