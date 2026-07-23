//! `LowerCtx` methods: tail (extracted from lower/mod.rs).

use super::*;
use crate::{Init, Op, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind,
};
use almide_lang::types::Ty;

impl LowerCtx {
    /// True when a tail `e!` pass-through would return a Result whose ERR component
    /// differs from this fn's own err type ([`decl_fn_err`]) — a coercion v0 renders
    /// with `.map_err(...)` at the `?` site, so stripping the `!` here would type-pun
    /// the err payload (the `result.collect_map(..)!` List[String]-as-String class).
    /// `None` (a declared-Option fn, a lambda sub-ctx) keeps the pass-through.
    fn unwrap_tail_err_mismatch(&self, inner: &IrExpr) -> bool {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        if let (Ty::Applied(TC::Result, a), Some(fe)) = (&inner.ty, &self.decl_fn_err) {
            a.len() == 2 && &a[1] != fe
        } else {
            false
        }
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
        // C-127: a `match` tail whose RESULT spelling is an UNRESOLVED generic
        // (`Unknown` / a bare type param left by an under-constrained chain link)
        // is judged HEAP and routed down the heap-match leg — but when EVERY arm
        // body's own type is a resolved SCALAR, the arms are the ground truth
        // (native sizes the value the same way): retype the tail from the arms
        // and take the scalar leg.
        if is_heap_ty(&tail.ty) {
            if let IrExprKind::Match { subject, arms } = &tail.kind {
                let arm_tys_scalar = !arms.is_empty()
                    && arms.iter().all(|a| {
                        !is_heap_ty(&a.body.ty) && !matches!(a.body.ty, Ty::Unknown)
                    });
                if arm_tys_scalar {
                    let retyped = IrExpr {
                        kind: IrExprKind::Match {
                            subject: subject.clone(),
                            arms: arms.clone(),
                        },
                        ty: arms[0].body.ty.clone(),
                        span: tail.span.clone(),
                        def_id: tail.def_id,
                    };
                    return self.lower_tail_scalar(&retyped);
                }
            }
        }
        // Decomposed (#781, cog 232): the UNIT / HEAP / SCALAR tails are verbatim
        // text moves into lower_tail_unit / lower_tail_heap / lower_tail_scalar —
        // behavior proven by the classify wall-list + cert byte-identity ladder.
        if matches!(tail.ty, Ty::Unit) {
            return self.lower_tail_unit(tail);
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
            return self.lower_tail_heap(tail);
        }
        self.lower_tail_scalar(tail)
    }

    /// The UNIT-typed tail of [`Self::lower_tail`] (effects run, no value).
    /// Verbatim text move.
    fn lower_tail_unit(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        return match &tail.kind {
            IrExprKind::Unit => Ok(None),
            // A Unit-typed call tail is an EFFECT call (e.g. `println(s)`): lower it
            // through the STATEMENT dispatcher — the same one-dispatcher discipline
            // as the branch arm-tail — so the functional-rebind group (list.push /
            // map.insert / bytes.push / clear) fires here too. A lifted LAMBDA whose
            // body is `{ list.push(g, 7) }` reaches its push as this unit TAIL; the
            // direct lower_effect_call bypassed the rebind and emitted a bare
            // unlinked `list.push` (the closure-over-global wall class).
            IrExprKind::Call { .. } => {
                self.lower_stmt_expr(tail)?;
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
                // A TUPLE subject in Unit-tail position — the heap-branch tail
                // duplication turns `let s = match (…) {…}; use(s)` into exactly
                // this shape (the whole body IS the match): the refinement chain's
                // unit sibling runs only the taken arm's effects.
                if self.try_lower_tuple_refinement_unit_match(subject, arms) {
                    return Ok(None);
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
                if self.unwrap_tail_err_mismatch(expr) {
                    return Err(LowerError::Unsupported(
                        "tail `!` propagates a Result whose err type differs from the fn's \
                         (v0 map_err-coerces it) — the pass-through would type-pun the err \
                         payload not in this brick"
                            .into(),
                    ));
                }
                self.lower_tail(Some(expr))
            }
            other => Err(LowerError::Unsupported(format!(
                "Unit-typed tail {} not in this brick",
                kind_name(other)
            ))),
        };
    }

    /// The HEAP-typed tail of [`Self::lower_tail`] (the moved-out owned return).
    /// Verbatim text move.
    fn lower_tail_heap(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        return match &tail.kind {
            IrExprKind::Var { .. } => self.lower_tail_heap_var(tail),
            // A TAIL `e!` (Unwrap — effect-fn error propagation in return position):
            // `f() = g()!` PROPAGATES g's Result unchanged (Ok→Ok, Err→Err), i.e. it IS
            // `f() = g()` at the effect-Result level. So strip the `!` and lower `e` as the
            // tail (return its Result directly). This unblocks the `parse_mapping =
            // collect_map(..)!` shape (a tail call result propagated). Sound ONLY when the
            // err components match — a mismatch (v0 map_err-coerces at the `?` site) would
            // type-pun the propagated err payload, so it walls (the collect_map! class).
            IrExprKind::Unwrap { .. } => self.lower_tail_heap_unwrap(tail),
            // A lambda RETURNED (`fn mk() -> (Int) -> Int = (x) => x + 1`, `fn adder(n)
            // = (x) => x + n`) — LIFT it to a CLOSURE BLOCK (fnidx + captured scalars)
            // and MOVE the block out as the return (a fresh owned heap value — removed
            // from the scope-end set; cert `im`). The caller tracks the bound result in
            // `closure_values` (binds_p2) so a later `f(args)` dispatches through it.
            IrExprKind::Lambda { .. } => self.lower_tail_heap_lambda(tail),
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
            | IrExprKind::RuntimeCall { .. } => self.lower_tail_heap_fresh(tail),
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
                self.lower_tail_heap_call_named_ctor(tail)
            }
            // A function-call result returned directly (`fn f() = g(xs)`): the
            // callee's heap result is a FRESH OWNED value (its return-mode
            // signature), moved out — NOT added to live_heap_handles. Cert:
            // CallFn-result + move-out, identical to the already-verified
            // `var x = g(xs); x`, so the gate covers it by the same evidence
            // (the runtime correspondence is exact — the callee returns rc 1).
            IrExprKind::Call { target: CallTarget::Named { .. }, .. } => {
                self.lower_tail_heap_call_named(tail)
            }
            // `fn f() = string.trim(s)` — a stdlib MODULE call result returned
            // directly. Admitted only when first-order + pure; the fresh owned
            // result is moved out (NOT added to live_heap_handles), like the
            // `Named` case above.
            IrExprKind::Call { target: CallTarget::Module { .. }, .. } => {
                self.lower_tail_heap_call_module(tail)
            }
            // `fn f(r) = r.x` — a HEAP extraction returned directly: alias the
            // container (`Op::Dup`) and move it out (cert `a` + `m`). A non-var
            // container (`f().x`, nested) cannot be aliased, so a failed extraction
            // would move out a deferred Opaque EMPTY value the caller observes = a
            // SILENT MISCOMPILE. Reject explicitly instead.
            IrExprKind::Member { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::TupleIndex { .. } => self.lower_tail_heap_extraction(tail),
            // `fn f() = if c then "a" else "b"` — a heap-result branch RETURNED. A
            // literal-armed `if` EXECUTES (only the taken arm allocates, returned rc=1)
            // via per-arm Alloc+Consume balance; otherwise LINEARIZE the arms and move
            // out ONE fresh `Alloc{Opaque}` (the deferred merged result slot, NOT added
            // to live_heap_handles — it is the return value). See `lower_branch`.
            IrExprKind::If { .. } => self.lower_tail_heap_if(tail),
            // A heap-result `match` over Int literal patterns with string-literal arms
            // EXECUTES: desugar to a nested heap-result `if` and run only the matched
            // arm; otherwise LINEARIZE to one deferred `Alloc{Opaque}`.
            IrExprKind::Match { .. } => self.lower_tail_heap_match(tail),
            // `fn apply(g, x) = g(x)` — a heap-result call through a KNOWN funcref (a lifted
            // lambda / a function-typed param bound to a table slot). EXECUTE it via
            // `Op::CallIndirect` and move the fresh owned result out, exactly like the Named /
            // Module arms above (its var-bind sibling is `binds_p2`'s heap-result CallIndirect).
            // This opens higher-order functions RETURNING a heap value (Result/List/String) — the
            // foundation for a self-hosted `fan.map` / traverse. An UNKNOWN callee stays walled.
            IrExprKind::Call { target: CallTarget::Computed { callee }, .. }
                if self.closure_value_of(callee).is_some() =>
            {
                self.lower_tail_heap_call_computed(tail)
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

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// Var arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_var(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::Var { id } = &tail.kind else { unreachable!() };
        let v = self.value_or_global(*id)?;
        // F2 pass-2 consumer gate (#790): RETURNING a deferred Opaque bind hands
        // the CALLER an empty block it reads executably. Strict mode refuses.
        if crate::lower::strict_values() && self.deferred_opaque_binds.contains(&v) {
            return Err(LowerError::Unsupported(
                "deferred (Opaque) value returned — the caller would read an \
                 empty block not in this brick"
                    .into(),
            ));
        }
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

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// Unwrap arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_unwrap(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::Unwrap { expr } = &tail.kind else { unreachable!() };
        if self.unwrap_tail_err_mismatch(expr) {
            return Err(LowerError::Unsupported(
                "tail `!` propagates a Result whose err type differs from the fn's \
                 (v0 map_err-coerces it) — the pass-through would type-pun the err \
                 payload not in this brick"
                    .into(),
            ));
        }
        self.lower_tail(Some(expr))
    }

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// Lambda arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_lambda(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::Lambda { params, body, .. } = &tail.kind else { unreachable!() };
        match self.lift_lambda(params, body) {
            Some(blk) => {
                self.live_heap_handles.retain(|h| *h != blk);
                Ok(Some(blk))
            }
            None => Err(LowerError::Unsupported(
                "lambda outside the liftable subset returned (heap/Float captures \
                 are a later ratchet) — cannot be faithfully materialized"
                    .into(),
            )),
        }
    }

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// big multi-pattern "fresh heap literal returned directly" arm body, verbatim (the
    /// arm never destructured `tail.kind` beyond the top-level match, so this helper
    /// doesn't either — see also `Self::lower_bind_heap_fresh`, the arm-position twin).
    fn lower_tail_heap_fresh(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        if let Some(dst) = self.lower_tail_heap_fresh_literals(tail)? {
            return Ok(Some(dst));
        }
        self.lower_tail_heap_fresh_ctors_and_opaque(tail)
    }

    /// Extracted from `Self::lower_tail_heap_fresh` (fifth-round split, cog reduction):
    /// the container-literal try-in-order chain, verbatim.
    fn lower_tail_heap_fresh_literals(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        if let Some(dst) = self.lower_tail_heap_fresh_record_tuple(tail)? {
            return Ok(Some(dst));
        }
        self.lower_tail_heap_fresh_list_concat_interp(tail)
    }

    /// Extracted from `Self::lower_tail_heap_fresh_literals` (sixth-round split, cog
    /// reduction): the Record/SpreadRecord/Tuple construct sub-chain, verbatim.
    fn lower_tail_heap_fresh_record_tuple(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
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
        Ok(None)
    }
}

include!("tail_b.rs");
