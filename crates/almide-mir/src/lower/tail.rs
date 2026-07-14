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
        // A PRECISE heap-field read (`b.label`, `t.1` with a String element) of a
        // MATERIALIZED record/tuple block: load the field's OWNED handle from its layout
        // slot as a BORROW (the container still owns it — freed by the container's masked
        // recursive drop). This is the field-VALUE (not the container-grain Dup): the read
        // returns the String at the slot, byte-correct. Returns the borrowed value (recorded
        // in `param_values`, NOT in `live_heap_handles` — a borrow, no second owner). Falls
        // through to the container-grain Dup below when the slot is not resolvable.
        //
        // SKIP the borrow for a MUTABLE (`var`) binding: a `var b = r.items` may be COW-mutated
        // (`b[0] = 99`), which the value-model refuses on a borrowed field handle. The
        // container-grain `Dup` below gives an OWNED, in-place-mutable copy (the pre-materialized
        // behavior), so the COW case keeps lowering instead of walling.
        if !self.binding_is_mutable {
            if let Some(borrowed) = self.try_lower_heap_field_borrow(expr) {
                return Ok(borrowed);
            }
        } else if let Some(borrowed) = self.try_lower_heap_field_borrow(expr) {
            // A MUTABLE var bound to a heap FIELD ACCESS (`var iv = state.iv`, `var b = box.items`):
            // the container-grain `Dup` below would bind the WHOLE CONTAINER (so a later
            // `bytes.len(iv)` reads the record header = a silent miscompile). Instead, resolve the
            // PRECISE field borrow and `Dup` it into a fresh OWNED, independently-mutable copy (cert
            // `a` + scope-end `d`) — the value-correct owning copy a mutable var needs. `borrowed`
            // is a `param_values` borrow (the container still owns the slot); the Dup'd `owned` is a
            // distinct reference NOT in `param_values`, so the caller adds it to the scope-end drop
            // set (balanced). Falls through to the container-grain Dup only when the field borrow
            // doesn't resolve (a non-aggregate container).
            //
            // LOOP-REASSIGNED (`var iv = state.iv` then `iv = concat(iv, …)` in a `for`/`while` — the
            // aes cfb8 shape): this is the PROVEN loop-carried slot `[Inc; Loop[FDec;FInc]; MoveOut]`
            // (OwnershipLoop.v). The owned `Dup` here is the slot's initial acquire; the in-loop
            // `bytes.concat` heap-result feeds the slot (`certificate.rs loop_carried_slots`), and the
            // cert recognizes this Dup-into-a-slot as the slot's `i` (not a bare `a`) so the stream
            // folds to the proven `i(id)m`. (Previously walled as "unproven coordination" — it is in
            // fact the proven Loop cert; the wall only kept the lowering from reaching the slot
            // machinery. corpus-wall ownership + the aes NIST vectors gate it.)
            let owned = self.fresh_value();
            self.ops.push(Op::Dup { dst: owned, src: borrowed });
            return Ok(owned);
        }
        let container = extraction_container(expr).ok_or_else(|| {
            LowerError::Unsupported(format!(
                "{} is not a field/element extraction",
                kind_name(&expr.kind)
            ))
        })?;
        let src = match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => self.value_or_global(*id)?,
            // ANF-LIFT a CALL-result container (`f(x).field`, `f(x)[i]`, `f(x).0` — the aes
            // `cfb8_encrypt(state, plain).data` / dojo `classify(r).0` shape). The container is a
            // Call producing a FRESH OWNED heap value, not a let-bound var, so neither the
            // container-grain `Dup` (no source value to alias) nor the precise field borrow (keyed
            // on a tracked Var) can fire. MATERIALIZE the call to a fresh synthetic temp by reusing
            // the exact `lower_bind` path a `let tmp = f(x)` takes — it emits the `CallFn`, tracks
            // the result in `live_heap_handles` for a single scope-end (recursive) drop, and seeds
            // its READ shape (`materialized_aggregates` + masks / `seed_variant_param`). Then RE-RUN
            // this extraction over a synthetic `Var` denoting the temp: the precise field/element
            // borrow now resolves exactly as it does for a source `let tmp = f(x); tmp.field`. The
            // borrowed field is alive for the whole expression (the temp outlives it, dropped at
            // scope end), so it is a sound lifetime — identical cert to the proven let-bound form.
            IrExprKind::Call { .. } if is_heap_ty(&container.ty) => {
                let tmp = self.fresh_synth_var();
                self.lower_bind(tmp, &container.ty, container)?;
                let synth_container = IrExpr {
                    kind: IrExprKind::Var { id: tmp },
                    ty: container.ty.clone(),
                    span: container.span,
                    def_id: None,
                };
                let synth_extraction = rebuild_extraction(expr, synth_container);
                return self.lower_heap_extraction(&synth_extraction);
            }
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

    /// Read a HEAP field/element (`b.label`, `t.1`) of a MATERIALIZED record/tuple block as a
    /// BORROW: `LoadHandle` the OWNED handle from the field's i64 slot. The container still
    /// OWNS the field (its masked recursive `DropListStr` frees it), so the read is NOT a
    /// second owner — the result is recorded in `param_values` (BORROWED, like an `Option`
    /// payload) and is NOT added to the scope-end drop set. Returns `None` (the caller then
    /// uses the container-grain fallback) unless the container is a tracked heap VAR whose
    /// block this brick materialized AND the field type is heap.
    pub(crate) fn try_lower_heap_field_borrow(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use crate::PrimKind;
        if !is_heap_ty(&expr.ty) {
            return None;
        }
        // A HEAP-element list index `xs[i]` (`xs: List[String]`) — LoadHandle the element's OWNED
        // handle at the bounds-checked `$elem_addr(list, i)` as a BORROW (the list still owns it,
        // freed by its DropListStr; the read is not a second owner → `param_values`). Without this
        // a heap `xs[i]` fell through to the container-grain `Dup` (the WHOLE list), which a String
        // consumer then read as a String = the list HEADER bytes (the `$ ` garbage). Gated to a
        // tracked/materialized list var so `$elem_addr` reads a real populated block (else defer).
        if let IrExprKind::IndexAccess { object, index } = &expr.kind {
            // GATE: the container must be a `List[heap]` (a nested-ownership list whose slots hold
            // owned element HANDLES). A scalar `List[Int]` slot holds an i64 VALUE, not a handle —
            // LoadHandle'ing it would borrow a non-handle (a use-after-free in the cert), so defer.
            if !crate::lower::is_heap_elem_list_ty(&object.ty) {
                return None;
            }
            let list = match &object.kind {
                IrExprKind::Var { id } if is_heap_ty(&object.ty) => {
                    let v = self.value_or_global(*id).ok()?;
                    if !self.materialized_lists.contains(&v) && !self.param_values.contains(&v) {
                        return None;
                    }
                    v
                }
                // `node.children[i]` — the list is ITSELF a heap FIELD of a materialized
                // aggregate (the ceangal layout class): recurse through this same borrow
                // (gated on materialization at each level; the result is a borrowed real
                // block in `param_values`, exactly what the `$elem_addr` read needs).
                IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                    if is_heap_ty(&object.ty) =>
                {
                    self.try_lower_heap_field_borrow(object)?
                }
                _ => return None,
            };
            let idx = self.lower_scalar_value(index)?;
            let h = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list] });
            let addr = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::ElemAddr, dst: Some(addr), args: vec![h, idx] });
            let dst = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(dst), args: vec![addr] });
            self.param_values.insert(dst);
            return Some(dst);
        }
        let (container, offset) = match &expr.kind {
            IrExprKind::Member { object, field } => {
                (object, self.aggregate_field_offset_any(&object.ty, field.as_str())?)
            }
            IrExprKind::TupleIndex { object, index } => {
                (object, self.aggregate_index_offset_any(&object.ty, *index)?)
            }
            _ => return None,
        };
        // The container must be MATERIALIZED with the uniform slot layout — a tracked heap VAR
        // (its block built by `try_lower_*_construct`, a param-bound aggregate), OR a NESTED
        // aggregate field whose inner block is itself materialized. A DEREFERENCING heap-field
        // read of a DEFERRED `Alloc{Opaque}` aggregate (garbage slots) would load a junk handle
        // and TRAP at `rc_dec`, so the resolver returns `None` for it (the caller falls back to
        // the safe container-grain Dup). The Var case gates on `materialized_aggregates`; the
        // nested case recurses through this same borrow (gated at each level).
        let h = match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => {
                let src = self.value_or_global(*id).ok()?;
                // The container must be a REAL block with the uniform slot layout: a locally
                // materialized aggregate, OR a BORROWED aggregate handle owned elsewhere
                // (`param_values` — a function param, a destructure/match-bound payload handle).
                // A `param_values` aggregate handle ALWAYS points at a real block (a deferred
                // `Init::Opaque` aggregate is owned-and-tracked in `live_heap_handles`, never
                // borrowed), so dereferencing its slot is sound — EXACTLY as the sibling
                // `try_lower_{tuple,record}_destructure` already trust `param_values` for the
                // identical `LoadHandle(container + offset)`. This closes the asymmetry that left a
                // match-bound record payload's String field (`match o { some(r) => r.k }`) falling to
                // the container-grain `Dup` (which read the record HEADER as a String = garbage).
                if !self.materialized_aggregates.contains(&src) && !self.param_values.contains(&src) {
                    return None;
                }
                let h = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![src] });
                h
            }
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                if is_heap_ty(&container.ty) =>
            {
                let inner = self.try_lower_heap_field_borrow(container)?;
                let h = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![inner] });
                h
            }
            _ => return None,
        };
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset as i64 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: h, b: off });
        let dst = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(dst), args: vec![addr] });
        // BORROWED: the container owns the field; the read is not a second owner.
        self.param_values.insert(dst);
        Some(dst)
    }

    /// Read-only predicate mirroring [`Self::try_lower_heap_field_borrow`]'s container gate: does a
    /// field/index extraction over `container` resolve to a MATERIALIZED/param heap-aggregate block
    /// (so a borrow would succeed)? Used by the list-literal builder's `all_lowerable` pre-check to
    /// admit a `Member`/`TupleIndex` element ONLY when the loop's borrow `?` will succeed — keeping the
    /// pre-check and the build loop in lockstep (no partial-ops leak on a mid-build decline).
    pub(crate) fn heap_field_container_tracked(&self, container: &IrExpr) -> bool {
        match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => self
                .value_for(*id)
                .map(|src| {
                    self.materialized_aggregates.contains(&src) || self.param_values.contains(&src)
                })
                .unwrap_or(false),
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
                if is_heap_ty(&container.ty) =>
            {
                self.heap_field_container_tracked(object)
            }
            _ => false,
        }
    }

    /// A single-arm tuple-destructure `match t { (…, x, …) => x }` that returns ONE bound element is
    /// semantically the tuple-index extraction `t.<i>` — the wasm-bindgen `match pair { (offs, _) =>
    /// offs }` / `(_, n) => n` shape that follows a tuple-accumulator `fold`. Detect that shape and
    /// return the index of the extracted component + its type, so the caller can lower it through the
    /// EXISTING `TupleIndex` extraction machinery (a heap component is a `param_values` BORROW; a
    /// scalar one a value load) instead of walling the heap-result match. `None` for any other match
    /// (a guard, a non-tuple pattern, a body that is not exactly the bound var, more than one bound
    /// element, a non-Var/non-Wildcard sub-pattern) — the caller keeps its existing routing.
    pub(crate) fn tuple_extract_match_index(
        &self,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> Option<(usize, Ty)> {
        use almide_ir::IrPattern;
        // The subject must be a TUPLE value (the accumulator result) — a non-tuple subject is some
        // other match this helper must not claim.
        if !matches!(subject.ty, Ty::Tuple(_)) {
            return None;
        }
        if arms.len() != 1 || arms[0].guard.is_some() {
            return None;
        }
        let elements = match &arms[0].pattern {
            IrPattern::Tuple { elements } => elements,
            _ => return None,
        };
        // Exactly ONE `Bind` element, every other a `Wildcard`. Record (index, var, ty).
        let mut bound: Option<(usize, almide_ir::VarId, Ty)> = None;
        for (i, p) in elements.iter().enumerate() {
            match p {
                IrPattern::Bind { var, ty } => {
                    if bound.is_some() {
                        return None; // more than one bound element — not a single extraction
                    }
                    bound = Some((i, *var, ty.clone()));
                }
                IrPattern::Wildcard => {}
                _ => return None,
            }
        }
        let (idx, var, ty) = bound?;
        // The arm body must be EXACTLY the bound var (`=> x`). A computed body is a real match this
        // helper must not flatten to a projection.
        match &arms[0].body.kind {
            IrExprKind::Var { id } if *id == var => Some((idx, ty)),
            _ => None,
        }
    }

    /// Build a synthetic `t.<index>` (`TupleIndex`) IrExpr over `subject` for the
    /// [`Self::tuple_extract_match_index`] projection, typed `elem_ty`. Reused by the tail / bind /
    /// value-match routing so a `match t { (x, _) => x }` lowers through the proven field-extraction
    /// paths.
    pub(crate) fn synth_tuple_index(subject: &IrExpr, index: usize, elem_ty: Ty) -> IrExpr {
        IrExpr {
            kind: IrExprKind::TupleIndex { object: Box::new(subject.clone()), index },
            ty: elem_ty,
            span: subject.span,
            def_id: None,
        }
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
                // A Unit `if` tail EXECUTES (only the taken arm's effects run) when the
                // cond is a scalar; else it linearizes.
                IrExprKind::If { cond, then, else_ }
                    if self.try_lower_unit_if(cond, then, else_) =>
                {
                    Ok(None)
                }
                // A Unit `match` tail over Int literal patterns EXECUTES: desugar to a
                // nested if and run only the matched arm; non-literal patterns linearize.
                IrExprKind::Match { subject, arms } => {
                    if let Some(if_expr) = self.desugar_match_to_if(subject, arms, &Ty::Unit) {
                        if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                            if self.try_lower_unit_if(cond, then, else_) {
                                return Ok(None);
                            }
                        }
                    }
                    self.lower_branch(tail)?;
                    Ok(None)
                }
                IrExprKind::If { .. } => {
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
                // A Unit-typed TAIL `g()` / `g()!` (effect-fn call propagating its
                // `Result[Unit, _]`): the frontend wraps it in `Try`/`Unwrap` for the
                // auto-`?`. The Result is the function's own Unit return, so strip the
                // wrapper and lower the inner effect call — exactly the heap-`Unwrap`
                // tail rule (line below), but for a discarded-Unit result.
                IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => {
                    self.lower_tail(Some(expr))
                }
                other => Err(LowerError::Unsupported(format!(
                    "Unit-typed tail {} not in this brick",
                    kind_name(other)
                ))),
            };
        }
        // A tail of type `Result[Unit, _]` is the return of an `effect fn … -> Unit`
        // (its auto-`?` effect Result). The v1 pipeline lowers such a fn to a VOID wasm
        // function, so its tail must produce NO return value — an EFFECT call (`effect fn
        // main() = loop(xs)`), or a `Try`/`Unwrap` over one (`= loop(xs)?` / `!`). The
        // `Result[Unit, _]` type is `is_heap_ty` (a `Ty::Applied`), so WITHOUT this it
        // fell into the heap branch's `Call` arm, which emits `(local.set $r (call $f …))`
        // expecting an i32 the void callee never returns — invalid wasm. Stripping a
        // `Try`/`Unwrap` recurses to the inner call, which lands back here (still
        // `Result[Unit, _]`) and lowers as the effect call.
        // …UNLESS this function's DECLARED return is an explicit `Result`/`Option` (e.g. `fs.write
        // -> Result[Unit, String]`): then the `Result[Unit, _]` tail is the function's REAL heap
        // return value the caller `match`es on, so it must flow to the HEAP path below (produce the
        // owned Result block) — voiding it would emit a void `$fs.write` while the call site does
        // `(local.set $r (call $fs.write …))`, a type mismatch (invalid wasm). The voiding stays in
        // force for the SYNTHETIC `Result[Unit, _]` of a declared-`Unit` effect fn (flag false).
        if is_unit_result_ty(&tail.ty) && !self.decl_ret_is_result {
            match &tail.kind {
                IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => {
                    return self.lower_tail(Some(expr));
                }
                IrExprKind::Call { .. } => {
                    self.lower_effect_call(tail)?;
                    return Ok(None);
                }
                _ => {}
            }
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
                // A TAIL `e!` (Unwrap — effect-fn error propagation in return position):
                // `f() = g()!` PROPAGATES g's Result unchanged (Ok→Ok, Err→Err), i.e. it IS
                // `f() = g()` at the effect-Result level. So strip the `!` and lower `e` as the
                // tail (return its Result directly). This unblocks the `parse_mapping =
                // collect_map(..)!` shape (a tail call result propagated).
                IrExprKind::Unwrap { expr } => return self.lower_tail(Some(expr)),
                // A lambda RETURNED (`fn mk() -> (Int) -> Int = (x) => x + 1`, `fn adder(n)
                // = (x) => x + n`) — LIFT it to a CLOSURE BLOCK (fnidx + captured scalars)
                // and MOVE the block out as the return (a fresh owned heap value — removed
                // from the scope-end set; cert `im`). The caller tracks the bound result in
                // `closure_values` (binds_p2) so a later `f(args)` dispatches through it.
                IrExprKind::Lambda { params, body, .. } => {
                    return match self.lift_lambda(params, body) {
                        Some(blk) => {
                            self.live_heap_handles.retain(|h| *h != blk);
                            Ok(Some(blk))
                        }
                        None => Err(LowerError::Unsupported(
                            "lambda outside the liftable subset returned (heap/Float captures \
                             are a later ratchet) — cannot be faithfully materialized"
                                .into(),
                        )),
                    };
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
                | IrExprKind::UnwrapOr { .. }
                | IrExprKind::ToOption { .. }
                | IrExprKind::OptionalChain { .. }
                // A CAPTURING CLOSURE value returned is a fresh heap env; a RANGE is a fresh value —
                // both `Alloc{Opaque}`, moved out. (A non-capturing `Lambda` is lifted above.)
                | IrExprKind::ClosureCreate { .. }
                | IrExprKind::Range { .. }
                | IrExprKind::RuntimeCall { .. } => {
                    // A RECORD literal RETURNED (`fn mk(a) = P { x: a, y: a * 2 }`) — build the
                    // real layout block (scalar fields stored, heap fields moved in) and MOVE
                    // IT OUT as the return (NOT tracked → the caller owns it, no scope-end
                    // drop). Same cert as the heap-literal return: alloc(i) + move-out(m).
                    if let IrExprKind::Record { .. } = &tail.kind {
                        if let Some(dst) = self
                            .try_lower_record_construct(tail)
                            .or_else(|| self.try_lower_scalar_record_construct(tail))
                        {
                            return Ok(Some(dst));
                        }
                    }
                    // A SPREAD record RETURNED (`fn attr(e, k, v) = { ...e, attrs: map.set(…) }` —
                    // the svg element-builder shape): build a fresh same-layout block COPYING each
                    // non-overridden field from the materialized base (scalar Load, heap-handle Dup so
                    // base keeps its own ref) + storing the overrides, then MOVE IT OUT as the return
                    // (the caller owns it, no scope-end drop). Same `i…m` cert as the Record return.
                    if let IrExprKind::SpreadRecord { .. } = &tail.kind {
                        if let Some(dst) = self.try_lower_spread_record_construct(tail) {
                            return Ok(Some(dst));
                        }
                    }
                    // A TUPLE literal RETURNED (`fn pair(s) = (s, 5)`, `(parse_inline(t), pos + 1)`
                    // — the dominant yaml `(Value, Int)` parser shape): build the real block (scalar
                    // slots stored, heap elements moved in via `lower_owned_heap_field`) and MOVE IT
                    // OUT as the return (the block is `record_masks`-tracked but NOT in
                    // `live_heap_handles`, so it is the moved-out result — the caller owns it, no
                    // scope-end drop). Same cert as the Record return: alloc(i) + per-element moves +
                    // move-out(m). The caller's destructure reads it precisely (it's a masked aggregate).
                    if let IrExprKind::Tuple { elements } = &tail.kind {
                        if let Some(dst) = self
                            .try_lower_scalar_tuple_construct(elements)
                            .or_else(|| self.try_lower_tuple_construct(elements))
                        {
                            return Ok(Some(dst));
                        }
                    }
                    // A heap-ELEMENT list literal RETURNED — a `List[(String, String)]`
                    // (`fn keyword_aliases() = [("Ok", "ok"), …]`) or a `List[Record]`
                    // (`fn keyword_groups() = [KeywordGroup { … }, …]`, `fn precedence_table() =
                    // [PrecLevel { … }, …]`). Build the real nested-ownership block (each element
                    // moved in, the recursive per-element drop registered), MOVED OUT as the return
                    // (NOT tracked → no scope-end drop; the caller owns it). Without this the literal
                    // fell through `try_lower_str_list_literal` (which returns None for these heap
                    // elements) to the Opaque alloc = an empty len-0 list (a silent miscompile).
                    if let Some(dst) = self.try_lower_record_list_literal_tail(tail) {
                        return Ok(Some(dst));
                    }
                    // A `List[String]` literal RETURNED (`fn make() = [e0, e1]`) — build a real
                    // nested-ownership DynListStr (each element moved/Dup'd in), moved out as the
                    // return (NOT tracked, so no scope-end DropListStr — the caller owns it). Without
                    // this the literal fell to the Opaque alloc = an empty len-0 list.
                    if let Some(dst) = self.try_lower_str_list_literal(tail) {
                        return Ok(Some(dst));
                    }
                    // A scalar `List[Int/Float/Bool]` literal RETURNED with computed elements —
                    // build + store each slot, moved out (an all-literal list is the Opaque/IntList
                    // path below). Without this a `[a, a]` of computed scalars returned an empty list.
                    if let Some(dst) = self.try_lower_scalar_list_construct(tail) {
                        return Ok(Some(dst));
                    }
                    // A string concat RETURNED (`fn greet(n) = "Hi, " + n`) — a fresh owned String
                    // (via __str_concat), moved out as the return (cert CallFn-result i + ret m).
                    if let Some(dst) = self.try_lower_concat_str(tail) {
                        return Ok(Some(dst));
                    }
                    // A SCALAR-element list concat RETURNED (`fn pair(xs) = xs + [7]`) — a fresh owned
                    // list (via __list_concat), moved out as the return (cert CallFn-result i + ret m).
                    // A heap-element list concat returns None and falls through to the deferred Opaque.
                    if let Some(dst) = self.try_lower_concat_list(tail) {
                        return Ok(Some(dst));
                    }
                    // A STRING INTERPOLATION RETURNED (`fn greet(n) = "Hi, ${n}"`) over the
                    // executable subset — a fresh owned String (via the __str_concat chain),
                    // moved out as the return. A compound/call-operand interp falls through to
                    // the deferred Opaque below.
                    if let IrExprKind::StringInterp { parts } = &tail.kind {
                        if let Some(dst) = self.try_lower_string_interp(parts) {
                            return Ok(Some(dst));
                        }
                    }
                    // A `Some(scalar)`/`None` RETURNED (`fn some_int(x) = Some(x)`) is
                    // MATERIALIZED so the caller receives a real 0-or-1-element-list
                    // Option (len-correct) it can `match` — the self-host Option fns
                    // (list.get/first/last) return through such helpers. Moved out (NOT
                    // pushed to live_heap_handles), cert = Alloc i + move-out m.
                    // `ok(Pair(_e0, _e1))` / `ok(Plain)` / `err(msg)` for `Result[<user variant>, String]`
                    // (derived variant decode) — materialize the variant Ok, recursive `$__drop_<V>` drop.
                    // BEFORE the generic `try_lower_option_ctor` heap-Ok path, which would emit a dangling
                    // `CallFn "Pair"` for the variant ctor.
                    if let Some(dst) = self.try_lower_result_variant_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `err(Overflow(msg))` RETURNED for `Result[T_scalar, <user variant>]`
                    // (the structured-error class): the len-as-tag Err wrapper, moved out.
                    if self.is_scalar_ok_variant_err_result(&tail.ty) {
                        if let Some(dst) = self.try_lower_result_err_variant_ctor(tail, &tail.ty) {
                            self.live_heap_handles.retain(|h| *h != dst);
                            return Ok(Some(dst));
                        }
                    }
                    if let Some(dst) = self.try_lower_option_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `fn f() -> String = opt ?? "d"` — a heap-String `??` RETURNED. Executes via the
                    // self-host `option.unwrap_or_str` call (try_lower_option_unwrap_or's heap branch),
                    // MOVED OUT as the return (track_result=false — the caller owns it; tracking it
                    // would double-free). Closes the tail-position heap-`??` silent-Opaque hole.
                    if let IrExprKind::UnwrapOr { expr, fallback } = &tail.kind {
                        if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, false) {
                            return Ok(Some(dst));
                        }
                    }
                    // `ok({val, next})` / `err(msg)` RETURNED for a `Result[heap-record, String]` (porta
                    // read_valtype): materialize the record-Ok / String-Err block, MOVED OUT as the
                    // return (the recursive `Op::DropWrapperRec` frees it via `$__drop_<R>` at the
                    // caller's scope end). Checked before the generic ctor paths below.
                    if let Some(dst) = self.try_lower_result_record_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `ok(none)` / `ok(<Option[record]>)` / `err(msg)` RETURNED for `Result[Option[record],
                    // String]` (porta read_message): recursive `$__drop_opt_<R>` via `resrec:opt_<R>`.
                    if let Some(dst) = self.try_lower_result_option_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `ok(some(x))` / `ok(none)` / `err(msg)` RETURNED for `Result[Option[T], String]` with a
                    // STRING / SCALAR leaf (the derived-Codec `__decode_option_T`): flat `DropListStr` for a
                    // scalar Option, recursive `$__drop_opt_str` for a String Option. Checked AFTER the
                    // record ctor (which claims the record-Option shape) — the leaf gate keeps them disjoint.
                    if let Some(dst) = self.try_lower_result_option_scalar_str_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `ok(value.array(...))` / `err(msg)` RETURNED for a `Result[Value, String]` (csv
                    // `parse`): materialize the Value-Ok / String-Err Result block, MOVED OUT as the
                    // return (the recursive `Op::DropResultValue` frees it at the caller's scope end).
                    if let Some(dst) = self.try_lower_result_value_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `ok((slice, pos))` / `err(msg)` RETURNED for a `Result[(String, Int), String]`
                    // (toml `parse_key_part`): materialize the (String,Int)-Ok / String-Err block,
                    // MOVED OUT (the recursive `Op::DropResultStrInt` frees it at the caller's scope end).
                    if let Some(dst) = self.try_lower_result_str_int_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `ok((value.…, pos))` / `err(msg)` RETURNED for `Result[(Value, Int), String]`
                    // (toml parse_val): materialize the (Value,Int)-Ok / String-Err block, MOVED OUT
                    // (the recursive `Op::DropResultValueInt` frees it at the caller's scope end).
                    if let Some(dst) = self.try_lower_result_value_int_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `ok((keys, pos))` / `err(msg)` RETURNED for `Result[(List[String], Int), String]`
                    // (toml parse_key / parse_table_key): the recursive `Op::DropResultListStrInt`.
                    if let Some(dst) = self.try_lower_result_list_str_int_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `ok((items, np))` / `err` for `Result[(List[Value], Int), String]` (collect_array_items).
                    if let Some(dst) = self.try_lower_result_list_value_int_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `ok(())` / `ok(<scalar>)` RETURNED for a `Result[<non-heap>, String]` (porta
                    // `run_foreground` / `ensure_porta_dir` `ok(())`): materialize the flat len-0 Ok
                    // block, MOVED OUT as the return (its scope-end `DropListStr` frees just the block —
                    // no nested heap). The heap-Ok cases (record/value/tuple/String) were intercepted
                    // by the ctors above, so reaching here is exactly the scalar/Unit Ok the arm path
                    // already lowers — only the TAIL position was missing it (this closed that gap).
                    if let Some(dst) = self.try_lower_result_scalar_ok_ctor(tail, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // A SPREAD-record (`{ ...n, gap_main: v }` — ceangal's with_* rebuilders) or a
                    // plain RECORD literal RETURNED as the fn tail: the SAME construct machinery the
                    // heap-result ARM position already uses (base slots copied via Dup — a borrowed
                    // param base stays valid; overrides moved in), then MOVED OUT exactly per the arm
                    // precedent (`Consume` + per-frame temp drops; the caller frees the return by its
                    // type). A non-materialized base / out-of-subset field returns None → the honest
                    // Opaque wall below.
                    if matches!(&tail.kind, IrExprKind::SpreadRecord { .. }) {
                        let mark = self.live_heap_handles.len();
                        if let Some(dst) = self.try_lower_spread_record_construct(tail) {
                            self.ops.push(Op::Consume { v: dst });
                            self.drop_arm_locals(mark);
                            return Ok(Some(dst));
                        }
                    }
                    if matches!(&tail.kind, IrExprKind::Record { .. }) {
                        let mark = self.live_heap_handles.len();
                        if let Some(dst) = self
                            .try_lower_record_construct(tail)
                            .or_else(|| self.try_lower_scalar_record_construct(tail))
                        {
                            self.ops.push(Op::Consume { v: dst });
                            self.drop_arm_locals(mark);
                            return Ok(Some(dst));
                        }
                    }
                    let repr = repr_of(&tail.ty)?;
                    let init = alloc_init(tail);
                    // `alloc_init` faithfully materializes a string literal and a scalar-
                    // literal list/tuple (handled together with the faithful attempts above);
                    // every other constructor (Map/Record/Result/Option/closure/range, a
                    // non-foldable list) yields `Init::Opaque` — an EMPTY heap value the caller
                    // would observe as the return = a SILENT MISCOMPILE. Reject the unfaithful
                    // case explicitly.
                    if matches!(init, Init::Opaque) {
                        return Err(LowerError::Unsupported(format!(
                            "heap-result {} cannot be faithfully returned in this brick \
                             (would move out an empty deferred heap value)",
                            kind_name(&tail.kind)
                        )));
                    }
                    let dst = self.fresh_value();
                    self.ops.push(Op::Alloc { dst, repr, init });
                    self.record_elided_calls(tail);
                    Ok(Some(dst))
                }
                // A VARIANT CONSTRUCTOR call returned DIRECTLY (`fn make(x) -> Boxed =
                // Wrap(x)` — a bare ctor with no enclosing `let`/match; also reached via
                // `lift_lambda`'s body-lowering for a synthesized `(x) => Wrap(x)` wrapper,
                // the `list.map(Wrap)` desugar's exact shape). A constructor is NOT a real
                // top-level wasm function — it has no `Op::FuncRef` target, `try_lower_
                // variant_ctor` inlines its block construction at each call site — so the
                // GENERIC Named-call arm below (a plain `Op::CallFn` by name) would reference
                // a symbol that is NEVER linked (an "unlinked call" render wall). Checked
                // BEFORE the generic arm. `try_lower_variant_ctor` does not itself track its
                // result in `live_heap_handles` (the caller decides) — returning it directly
                // IS the move-out tail position needs, no extra bookkeeping.
                IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
                {
                    match self.try_lower_variant_ctor(tail) {
                        Some(dst) => Ok(Some(dst)),
                        None => Err(LowerError::Unsupported(
                            "variant constructor returned directly cannot be faithfully \
                             materialized in this brick (a heap/recursive field outside the \
                             executable subset)"
                                .into(),
                        )),
                    }
                }
                // A function-call result returned directly (`fn f() = g(xs)`): the
                // callee's heap result is a FRESH OWNED value (its return-mode
                // signature), moved out — NOT added to live_heap_handles. Cert:
                // CallFn-result + move-out, identical to the already-verified
                // `var x = g(xs); x`, so the gate covers it by the same evidence
                // (the runtime correspondence is exact — the callee returns rc 1).
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                    let mark = self.live_heap_handles.len();
                    let lowered = self.lower_call_args(args)?;
                    let dst = self.fresh_value();
                    let repr = repr_of(&tail.ty)?;
                    self.ops.push(Op::CallFn {
                        dst: Some(dst),
                        name: name.as_str().to_string(),
                        args: lowered,
                        result: Some(repr),
                    });
                    // Free any OWNED-temp arg the call materialized (`f(string.replace(s,…), s)` — the
                    // yaml `parse_number(string.replace(s,"_",""), s)` shape). A heap-result tail returns
                    // `dst` directly (moved out, NOT in live_heap_handles), bypassing the function's
                    // scope-end drops — so the materialized arg temp would LEAK (a parse loop OOMs).
                    self.drop_arm_locals(mark);
                    Ok(Some(dst))
                }
                // `fn f() = string.trim(s)` — a stdlib MODULE call result returned
                // directly. Admitted only when first-order + pure; the fresh owned
                // result is moved out (NOT added to live_heap_handles), like the
                // `Named` case above.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                    let mark = self.live_heap_handles.len();
                    let dst = self.lower_pure_module_value_call(
                        module.as_str(),
                        func.as_str(),
                        args,
                        &tail.ty,
                    )?;
                    // Free any owned-temp arg materialized for the call — a heap-result tail moves out
                    // `dst` and bypasses scope-end drops (see the Named case above), so the temp leaks.
                    // `dst` is moved out (not in live_heap_handles) so it is never among the dropped.
                    self.live_heap_handles.retain(|h| *h != dst);
                    self.drop_arm_locals(mark);
                    Ok(Some(dst))
                }
                // `fn f(r) = r.x` — a HEAP extraction returned directly: alias the
                // container (`Op::Dup`) and move it out (cert `a` + `m`). A non-var
                // container (`f().x`, nested) cannot be aliased, so a failed extraction
                // would move out a deferred Opaque EMPTY value the caller observes = a
                // SILENT MISCOMPILE. Reject explicitly instead.
                IrExprKind::Member { .. }
                | IrExprKind::IndexAccess { .. }
                | IrExprKind::MapAccess { .. }
                | IrExprKind::TupleIndex { .. } => {
                    let dst = self.lower_heap_extraction(tail)?;
                    // A PRECISE field BORROW (`fn f(r) = r.name` over a materialized/param
                    // record — the loaded slot handle is in `param_values`, the CONTAINER still
                    // owns it) cannot be moved out as-is: the caller would drop it while the
                    // container also drops it = a double-free. AUTO-ACQUIRE an OWNED reference
                    // first (`Op::Dup` cert `a`, then move out cert `m` — exactly `let q = r.name;
                    // q`), so the returned `am` is independent of the container's reference. A
                    // container-grain `Dup` result (NOT a borrow — `lower_heap_extraction`'s
                    // fallback already acquired its own reference) is moved out directly.
                    if self.param_values.contains(&dst) {
                        let owned = self.fresh_value();
                        self.ops.push(Op::Dup { dst: owned, src: dst });
                        return Ok(Some(owned)); // moved out, NOT tracked (no double-drop)
                    }
                    Ok(Some(dst))
                }
                // `fn f() = if c then "a" else "b"` — a heap-result branch RETURNED. A
                // literal-armed `if` EXECUTES (only the taken arm allocates, returned rc=1)
                // via per-arm Alloc+Consume balance; otherwise LINEARIZE the arms and move
                // out ONE fresh `Alloc{Opaque}` (the deferred merged result slot, NOT added
                // to live_heap_handles — it is the return value). See `lower_branch`.
                IrExprKind::If { cond, then, else_ } => {
                    if let Some(dst) = self.try_lower_heap_result_if(cond, then, else_, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // Outside the executable heap-result-if subset, the arms would linearize
                    // and the RETURN value would be one deferred Opaque EMPTY heap object the
                    // caller observes = a SILENT MISCOMPILE. Reject explicitly.
                    Err(LowerError::Unsupported(
                        "heap-result `if` outside the executable subset cannot be faithfully \
                         returned in this brick (would move out an empty deferred heap value)"
                            .into(),
                    ))
                }
                // A heap-result `match` over Int literal patterns with string-literal arms
                // EXECUTES: desugar to a nested heap-result `if` and run only the matched
                // arm; otherwise LINEARIZE to one deferred `Alloc{Opaque}`.
                IrExprKind::Match { subject, arms } => {
                    // A single-arm tuple-destructure `match t { (offs, _) => offs }` extracting ONE
                    // component — semantically `t.<i>` (the wasm-bindgen post-`fold` extraction).
                    // Re-route through the proven `TupleIndex` tail extraction (a heap component is a
                    // borrow auto-acquired into an owned move-out; a scalar one a value read).
                    if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
                        let synth = Self::synth_tuple_index(subject, idx, elem_ty);
                        return self.lower_tail(Some(&synth));
                    }
                    // A CUSTOM variant (user ADT) subject with a HEAP result — tag@slot0 dispatch
                    // with heap-result arms (ADT brick 4, e.g. recursive `to_string`).
                    if let Some(dst) =
                        self.try_lower_custom_variant_match(subject, arms, &tail.ty)
                    {
                        return Ok(Some(dst));
                    }
                    // A heap-result VARIANT (Option/Result) match (`match scan_quote(..) {
                    // some(p) => "..", none => ".." }`) over a SCALAR payload — the
                    // subject-drop-before-arms desugar (cert-clean, scalar-payload only; a heap
                    // payload self-gates back to None here = the true Camp-4 frontier).
                    if is_variant_ty(&subject.ty) {
                        if let Some(dst) =
                            self.try_lower_variant_value_match(subject, arms, &tail.ty)
                        {
                            return Ok(Some(dst));
                        }
                    }
                    // A len-as-tag RESULT subject with HEAP-result arms — the merge-based
                    // value match (the Camp-4 `compute` opener; borrowed payload binds, the
                    // subject temp freed by the scope epilogue after the merge move-out).
                    if let Some(dst) = self.try_lower_result_match_value(subject, arms, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // An `Option[<heap>]` subject with HEAP-result arms — the Option twin
                    // (is_balanced's fold step: `match acc { none => none, some(stack) => … }`).
                    if let Some(dst) = self.try_lower_option_match_value(subject, arms, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // A LIST subject (`match xs { [] => .., ys => .. }`) with HEAP-result
                    // arms — the len-tag twin of the Result opener (a bind-all arm aliases
                    // the owned subject temp; release parity covers an arm move-out).
                    if let Some(dst) = self.try_lower_list_match_value(subject, arms, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // `desugar_match_to_if` wraps its OUTPUT in a `Block` (hoisted `let`s
                    // preceding the `If`) whenever the subject isn't one of `subject_pure`'s
                    // freely-substitutable kinds (`Var`/`LitInt`/`LitBool`/`LitFloat` —
                    // notably missing `LitStr`: a single-use `let s = "hello world"` subject
                    // gets constant-propagated to a bare `LitStr` upstream, same gap B52
                    // fixed for the call-argument consumer in `calls_p2.rs`). Unwrap it
                    // generically here too — lower the hoisted `let`s via `self.lower_stmt`,
                    // then extract the inner `If` — rather than widening `subject_pure`
                    // itself (a general fix, not LitStr-specific: ANY subject needing the
                    // hoist now works in this tail position too).
                    let lifted = self.desugar_match_to_if(subject, arms, &tail.ty).and_then(|e| {
                        let (stmts, if_expr) = match e.kind {
                            IrExprKind::If { .. } => (Vec::new(), e),
                            IrExprKind::Block { stmts, expr: Some(t) } => (stmts, *t),
                            _ => return None,
                        };
                        let IrExprKind::If { cond, then, else_ } = &if_expr.kind else { return None };
                        for s in &stmts {
                            self.lower_stmt(s).ok()?;
                        }
                        self.try_lower_heap_result_if(cond, then, else_, &tail.ty)
                    });
                    if let Some(dst) = lifted {
                        return Ok(Some(dst));
                    }
                    // Outside the executable heap-result-match subset, the RETURN value would
                    // be one deferred Opaque EMPTY heap object the caller observes = a SILENT
                    // MISCOMPILE. Reject explicitly.
                    Err(LowerError::Unsupported(
                        "heap-result `match` outside the executable subset cannot be faithfully \
                         returned in this brick (would move out an empty deferred heap value)"
                            .into(),
                    ))
                }
                // `fn apply(g, x) = g(x)` — a heap-result call through a KNOWN funcref (a lifted
                // lambda / a function-typed param bound to a table slot). EXECUTE it via
                // `Op::CallIndirect` and move the fresh owned result out, exactly like the Named /
                // Module arms above (its var-bind sibling is `binds_p2`'s heap-result CallIndirect).
                // This opens higher-order functions RETURNING a heap value (Result/List/String) — the
                // foundation for a self-hosted `fan.map` / traverse. An UNKNOWN callee stays walled.
                IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. }
                    if self.closure_value_of(callee).is_some() =>
                {
                    let mark = self.live_heap_handles.len();
                    let blk = self.closure_value_of(callee).unwrap();
                    let lowered = self.lower_call_args(args)?;
                    let dst = self.fresh_value();
                    let repr = repr_of(&tail.ty)?;
                    self.emit_closure_call(blk, Some(dst), lowered, Some(repr));
                    self.drop_arm_locals(mark);
                    Ok(Some(dst))
                }
                // `fn f(o) = o.method()` / `(g)()` returned — an UNRESOLVABLE
                // `Method`/`Computed` callee (the `Named`/`Module` arms are above).
                // Moving out a deferred Opaque EMPTY value the caller observes is a SILENT
                // MISCOMPILE, so reject explicitly.
                IrExprKind::Call { .. } => {
                    Err(LowerError::Unsupported(
                        "heap-result method/computed call cannot be faithfully returned in this \
                         brick (would move out an empty deferred heap value)"
                            .into(),
                    ))
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
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
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
            // A FLOAT literal returned directly (`fn pi() = 3.14159`) materializes its REAL f64
            // BITS as a `ConstInt` (the i64-uniform Float repr), so the fn returns the constant,
            // not the deferred-`Const` zero — the same materialization `lower_scalar_value` does
            // for a LitFloat operand. (The frontend folds `{ let p = 3.14; p }` to this form.)
            IrExprKind::LitFloat { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: value.to_bits() as i64 });
                Ok(Some(dst))
            }
            // A BOOL literal returned directly (`(x) => true` — a constant/param-ignoring predicate
            // for list.all/any/count, or `fn t() = true`) materializes its 0/1 as a `ConstInt`, NOT
            // the deferred-`Const` ZERO it used to fall into below (which made `(x) => true` return
            // FALSE — a silent miscompile of every constant-true predicate). Bool is an i64 0/1, the
            // same materialization lower_scalar_value does for a LitBool operand.
            IrExprKind::LitBool { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: *value as i64 });
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
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A SCALAR field / tuple element / list element TAIL (`(p) => p.x`, `fn fst(t) = t.0`,
            // `fn at(xs, i) = xs[i]`) — LOAD the real value from the materialized aggregate's layout
            // slot (the VALUE MODEL read side, what makes `list.map(points, (p)=>p.x)` return the
            // real field); `xs[i]` is the bounds-checked `$elem_addr` load. `lower_scalar_value`
            // dispatches each. Outside the materialized subset it rolls back to the deferred `Const`
            // (its container's calls elided), exactly as before.
            IrExprKind::Member { .. }
            | IrExprKind::TupleIndex { .. }
            | IrExprKind::IndexAccess { .. } => {
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(tail) {
                    return Ok(Some(dst));
                }
                self.ops.truncate(mark);
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A scalar UNARY op RETURNED directly (`fn ineg(n) = -n`, `fn flip(b) = not b`,
            // `fn fneg(x) = -x`) computes its REAL value via `lower_scalar_value` (the
            // UnOp arm: int neg `0 - x`, float neg the `f64.neg` prim, bool `not` `1 - b`)
            // — NOT the deferred-`Const` zero this used to fall into. This is the TAIL-
            // position twin of the value-position UnOp fix: a function whose body IS a
            // `UnOp` is a value position, so it must compute, not read 0. Outside the
            // scalar subset (a non-lowerable operand) it rolls back to the Const below,
            // exactly like the `BinOp` tail arm.
            IrExprKind::UnOp { .. } if !is_heap_ty(&tail.ty) => {
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(tail) {
                    return Ok(Some(dst));
                }
                self.ops.truncate(mark);
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A SCALAR map extraction is an unambiguous COPY (a scalar is never
            // reference-counted), so it is a `Const` — its container carries its own
            // ownership. (A HEAP extraction is an ALIAS / share — it needs a layout-aware
            // field-access op with `Dup` semantics and stays walled until that brick.)
            IrExprKind::MapAccess { .. }
            // A SCALAR error-operator result (`x?.f` yielding a scalar) is
            // likewise a fresh `Const`; the operator's value + early-return are deferred.
            | IrExprKind::Try { .. }
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. }
            // A RANGE returned: a fresh `Const` (no ownership); any analyzable callee
            // inside it is captured for caps by `record_elided_calls`. (A scalar-result
            // CALL is handled by its own arm above — a real executable `CallFn` when
            // resolvable, else the same deferred `Const` + elided marker.)
            | IrExprKind::Range { .. } => {
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A TAIL `e!` (Unwrap — effect-fn error propagation): `f() = g()!` propagates g's
            // Result unchanged, i.e. it IS `f() = g()`. Strip the `!` and lower `e` as the tail.
            IrExprKind::Unwrap { expr } => self.lower_tail(Some(expr)),
            // A SCALAR tail `??` (`fn parse_or_zero(s) = int.parse(s) ?? 0`, the canonical
            // form) EXECUTES the unwrap (tag read + payload-or-fallback) — it was a deferred
            // `Const` 0 here (a silent wrong value, neither payload nor fallback). Outside the
            // executable subset a `??` over a VARIANT operand WALLs (a Const-0 would be wrong);
            // a non-variant operand keeps the deferred `Const`.
            IrExprKind::UnwrapOr { expr, fallback } => {
                if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, false) {
                    return Ok(Some(dst));
                }
                if is_variant_ty(&expr.ty) {
                    return Err(LowerError::Unsupported(
                        "?? over an Option/Result operand in tail position outside the \
                         executable subset cannot be faithfully computed (a Const-0 would be \
                         a wrong value) not in this brick"
                            .into(),
                    ));
                }
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A scalar `if` tail EXECUTES (only the taken arm runs) via try_lower_scalar_if
            // — the IfThen/Else/EndIf markers — when the cond + both arms are in the
            // scalar subset; otherwise it falls back to the deferred linearize + Const.
            IrExprKind::If { cond, then, else_ } => {
                if let Some(dst) = self.try_lower_scalar_if(cond, then, else_, &tail.ty) {
                    return Ok(Some(dst));
                }
                self.lower_branch(tail)?;
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                Ok(Some(dst))
            }
            // A scalar-result `match` over INT literal patterns EXECUTES: desugar to a
            // nested `if subject == lit then arm else …` and lower it via the scalar-if
            // machinery (only the matched arm runs). Non-literal patterns / guards / a
            // non-scalar subject fall back to the deferred linearize + merged `Const`.
            IrExprKind::Match { subject, arms } => {
                // A single-arm tuple-destructure `match t { (_, n) => n }` extracting ONE SCALAR
                // component — semantically `t.<i>` (the tuple-accumulator `fold` cursor extraction).
                // Lower the synthetic `TupleIndex` via the scalar value model (a real slot load).
                if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
                    if !is_heap_ty(&elem_ty) {
                        let synth = Self::synth_tuple_index(subject, idx, elem_ty);
                        let mark = self.ops.len();
                        if let Some(dst) = self.lower_scalar_value(&synth) {
                            return Ok(Some(dst));
                        }
                        self.ops.truncate(mark);
                    }
                }
                // A CUSTOM variant (user ADT) subject — tag@slot0 dispatch (ADT brick 3).
                // `fn val(t: Tok) -> Int = match t { Num(n) => n, … }`.
                if let Some(dst) =
                    self.try_lower_custom_variant_match(subject, arms, &tail.ty)
                {
                    return Ok(Some(dst));
                }
                // A VARIANT (Option/Result) subject returned by a function — execute the
                // tag-read value-match (only the taken arm runs, the scalar payload bound);
                // `fn pick(o) = match o { Some(x) => x, None => -1 }` is the canonical form.
                // A ctor pattern is not `subj == lit`, so it can't reach `desugar_match_to_if`.
                if is_variant_ty(&subject.ty) {
                    if let Some(dst) = self.try_lower_variant_value_match(subject, arms, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // A UNIT-result tail variant match (`match write_summary(..) { ok(p) =>
                    // {…effects…}, err(e) => {…effects…} }` — the run_all_finish shape): the arms
                    // produce no VALUE, only effects, so there is nothing to "pick" — this is
                    // exactly the statement/Unit-position dispatch `lower_branch` already executes
                    // (track the Result subject → `try_lower_result_match` reads the tag and runs
                    // ONLY the taken arm; an untrackable subject linearizes both arms, the
                    // existing caps-union-sound fallback). DELEGATE to it rather than wall — the
                    // function's Unit return is the merged `Const` below (no value escapes the
                    // branch). The same proven machinery every non-tail Unit match uses; gated to
                    // `Unit` so a SCALAR/HEAP-result variant match (whose value DOES matter) keeps
                    // walling here (`lower_branch` would discard its value = a silent miscompile).
                    if matches!(tail.ty, Ty::Unit) {
                        self.lower_branch(tail)?;
                        let dst = self.fresh_value();
                        if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                        return Ok(Some(dst));
                    }
                    return Err(LowerError::Unsupported(
                        "variant (Option/Result) match in tail position outside the \
                         executable subset cannot be faithfully computed (a Const-0 would \
                         silently pick a wrong arm) not in this brick"
                            .into(),
                    ));
                }
                if let Some(if_expr) = self.desugar_match_to_if(subject, arms, &tail.ty) {
                    // `If` (literal arms) OR `Block` (`{ let x = subj; if … }` for a
                    // binder/guarded arm) — `lower_scalar_arm` runs both; roll back on a miss.
                    let mark = self.ops.len();
                    let lhh = self.live_heap_handles.len();
                    if let Some(dst) = self.lower_scalar_arm(&if_expr) {
                        return Ok(Some(dst));
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh);
                }
                self.lower_branch(tail)?;
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
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
