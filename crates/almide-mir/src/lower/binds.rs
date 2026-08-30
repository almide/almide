//! `LowerCtx` methods: binds (extracted from lower/mod.rs).

use super::*;
use crate::{CallArg, Init, IntOp, Op, PrimKind, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrPattern, VarId,
};
use almide_lang::types::Ty;

impl LowerCtx {

    /// Lift a lambda `(params) => body` into a fresh top-level MIR function (the closures
    /// machinery) and materialize its CLOSURE BLOCK — a heap `[rc][len][cap][fnidx]
    /// [captured…]` value (a plain DynList: slot 0 holds the `Op::FuncRef` table index,
    /// slots 1… hold the captured locals by VALUE). The block is the UNIFORM first-class
    /// function representation: a call through it loads the fnidx from slot 0 and passes
    /// the block as the leading (borrowed) ENV argument (`emit_closure_call`); the lifted
    /// body reads its captures back out of that env param in a prologue. A NON-capturing
    /// lambda is the k = 0 degenerate block. Returns `None` for a capture outside the
    /// slice (a heap or non-i64-scalar capture — a later ratchet) or a body outside the
    /// lowering subset; the caller then keeps the deferred `Opaque` model.
    ///
    /// OWNERSHIP: the block is a fresh owned heap object (cert `i`, scope-end `d` — pushed
    /// to `live_heap_handles` here; a tail return moves it out instead). Captured scalars
    /// are COPIED into the block at creation (value semantics — matching v0's move-closure
    /// copy), so the env owns no nested handles and the flat drop frees it exactly.
    ///
    /// SOUNDNESS: the lifted body is lowered by the SAME `lower_body_into` as any function,
    /// so it carries its own ownership / name-totality / capability certificate that the
    /// proven checker re-verifies; its env param is BORROWED (the caller's block outlives
    /// the call — the call-mode agreement the CallModes witness pins). Its capabilities
    /// reach THIS function through the `Op::FuncRef` edge — folded at closure CREATION
    /// (coverage-free; see `certificate::reachable_caps` / `reachable_caps_or_tainted`), so
    /// a printing lambda can never be silently caps-verified regardless of how/whether it
    /// is later invoked. The lambda is named `__lambda_<fn_name>_<n>` — file-unique (the
    /// harness keys the in-profile map by name), with nested lifts flattened into this
    /// function's set.
    /// Lift an inline lambda to a top-level MIR function plus a closure BLOCK, in the
    /// five sequential phases below — each a named decider so the one that declines is
    /// the one the trace names. `None` at ANY phase is an honest defer: nothing has been
    /// emitted into `self` yet (the sub-context is separate, and the block is only
    /// materialized once every phase has succeeded), so the caller's deferred fallback
    /// stays sound. Extracted from the one body (codopsy round-3 sweep, #852); every
    /// phase moved verbatim.
    pub(crate) fn lift_lambda(
        &mut self,
        params: &[(VarId, Ty)],
        body: &IrExpr,
    ) -> Option<ValueId> {
        let captured = self.collect_lambda_captures(params, body)?;
        let layout = self.partition_capture_drop_classes(captured)?;
        let cap_vals = self.resolve_capture_values(&layout)?;
        let name = self.lower_lifted_lambda_body(params, body, &layout)?;
        Some(self.materialize_closure_block(name, &layout, cap_vals))
    }

    /// Phase 1 of [`Self::lift_lambda`]: the lambda's CAPTURES, with their types, in
    /// first-occurrence order — and the two honest-wall gates that refuse a lift whose
    /// captures are MUTATED without a shared cell. Verbatim.
    fn collect_lambda_captures(
        &self,
        params: &[(VarId, Ty)],
        body: &IrExpr,
    ) -> Option<Vec<(VarId, Ty)>> {
        // free_vars over the lambda's own params reports exactly its captures (a `Var` node
        // denotes only locals). Collect them WITH their types (from the body's Var nodes) in
        // first-occurrence order — the deterministic env slot layout both sides share.
        let mut bound: std::collections::HashSet<VarId> = std::collections::HashSet::new();
        for (v, _) in params {
            bound.insert(*v);
        }
        let free = almide_ir::free_vars::free_vars(body, &bound);
        // A MODULE-LEVEL global (a top `let` or a mutable `var`) is NOT a capture:
        // the lambda body reads/writes it through the GLOBAL SLOT machinery
        // (`value_or_global` / the `__mg_take`+Store assign), which the lifted
        // sub-context carries (`globals` cloned; `mutable_global_info` is
        // program-static). The slot IS the shared cell, so a `var` global mutated
        // through a closure keeps native's shared semantics — capturing it as an
        // env VALUE COPY both broke the lift (a global has no `value_of` entry to
        // read the capture from) and would have frozen a stale snapshot.
        let free: Vec<VarId> = free
            .into_iter()
            .filter(|v| {
                !self.globals.contains_key(v) && crate::lower::mutable_global_info(*v).is_none()
            })
            .collect();
        struct CapCollect<'a> {
            free: &'a [VarId],
            out: Vec<(VarId, Ty)>,
        }
        impl almide_ir::visit::IrVisitor for CapCollect<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Var { id } = &e.kind {
                    if self.free.contains(id) && !self.out.iter().any(|(v, _)| v == id) {
                        self.out.push((*id, e.ty.clone()));
                    }
                }
                almide_ir::visit::walk_expr(self, e);
            }
        }
        let mut cc = CapCollect { free: &free, out: Vec::new() };
        almide_ir::visit::IrVisitor::visit_expr(&mut cc, body);
        // HONEST-WALL GATE — a MUTATED capture without a SHARED CELL: env slots are
        // VALUE COPIES / co-owns, so writing the copy silently LOSES the mutation
        // (sort_by_call_count printed calls=0; the closure-mutation wasm_runtime
        // cells printed stale values — all bisect-confirmed). A var the cell pre-scan
        // promoted (`cell_of`) is EXEMPT — its capture is the cell handle and every
        // read/write goes through the shared slot. Two layers:
        //   (a) the pre-scan verdict: a captured var in `cell_vars` (mutated ANYWHERE
        //       — including the enclosing scope after capture, the STALE-READ
        //       direction a body-only scan cannot see) that did NOT get a cell (an
        //       unadmitted inner class) refuses the lift;
        //   (b) the body-local MutScan (kept for entry paths that skip the pre-scan),
        //       now also catching IN-PLACE MUTATOR CALLS (`list.push(acc, 1)`) — the
        //       exact shape the rebind desugar turns into an Assign only later, which
        //       an Assign-only scan missed (the s2/s4 container-closure miscompile).
        if free.iter().any(|v| self.cell_vars.contains(v) && !self.cell_of.contains_key(v)) {
            return None;
        }
        {
            struct MutScan<'a> {
                free: &'a [VarId],
                hit: bool,
            }
            impl almide_ir::visit::IrVisitor for MutScan<'_> {
                fn visit_stmt(&mut self, s: &almide_ir::IrStmt) {
                    match &s.kind {
                        IrStmtKind::Assign { var, .. } if self.free.contains(var) => {
                            self.hit = true;
                        }
                        IrStmtKind::IndexAssign { target, .. }
                        | IrStmtKind::FieldAssign { target, .. }
                        | IrStmtKind::MapInsert { target, .. } => {
                            if self.free.contains(target) {
                                self.hit = true;
                            }
                        }
                        _ => {}
                    }
                    almide_ir::visit::walk_stmt(self, s);
                }
                fn visit_expr(&mut self, e: &IrExpr) {
                    if let Some(v) = crate::lower::inplace_mutated_receiver(e) {
                        if self.free.contains(&v) {
                            self.hit = true;
                        }
                    }
                    almide_ir::visit::walk_expr(self, e);
                }
            }
            let ms_free: Vec<VarId> =
                free.iter().copied().filter(|v| !self.cell_of.contains_key(v)).collect();
            let mut ms = MutScan { free: &ms_free, hit: false };
            almide_ir::visit::IrVisitor::visit_expr(&mut ms, body);
            if ms.hit {
                return None;
            }
        }
        Some(cc.out)
    }

    /// Phase 2 of [`Self::lift_lambda`]: partition the captures by DROP CLASS and lay
    /// them out in the slot order `$__drop_closure` walks. `None` = a capture outside
    /// the admitted classes (the honest defer). Verbatim.
    fn partition_capture_drop_classes(
        &self,
        captured: Vec<(VarId, Ty)>,
    ) -> Option<CaptureLayout> {
        // Partition the captures by DROP CLASS — the env layout is self-describing so the
        // uniform `$__drop_closure` runtime can free ANY closure block without lowering-time
        // mask knowledge (a call-result closure's captures are unknowable at the drop site):
        //   slot 0            = fnidx (SCALAR — the drop must never touch it)
        //   slot 1            = header: n_heap | (n_nested_heap << 16) | (n_closure << 32)
        //   slots 2..         = closure captures (freed by recursive $__drop_closure),
        //                       then FLAT heap captures (freed by one flat $rc_dec each),
        //                       then NESTED-heap captures (freed by the type-specific
        //                       recursive $__drop_list_str — a List[String] element),
        //                       then scalar captures (untouched).
        // Flat heap captures are ONE-LEVEL-EXACT kinds (String, List[Int], List[Float] — a
        // single rc_dec frees them completely). `List[String]` is NESTED (each element is
        // itself owned heap — a flat rc_dec of just the list block would leak every String,
        // the exact class of bug this session's `_str`-dispatch fix + the map.find near-miss
        // both found) — freed via the generic `__drop_list_str` (B33) instead. A `Value` /
        // variant / heap-field-record capture (or a `Float`, f64↔i64 reinterpret not in the
        // prim vocabulary) still defers — honest wall, recorded in the goal file.
        use almide_lang::types::constructor::TypeConstructorId;
        // `is_flat_scalar_block_ty` IS this predicate — the crate's canonical name for
        // "every slot is a raw i64, so one rc_dec is the full free", which is exactly
        // what the FLAT env class promises. This used to be spelled out again here, and
        // more narrowly (`List[Int|Float]` only), so shapes with identical physics were
        // refused: an all-scalar TUPLE, a `List[Bool]` or `List[Int32]`, an
        // `Option[<scalar>]`. A refused capture is not merely a missing feature — it
        // leaves the lambda deferred, and `map.fold(m, acc, (a, k, v) => acc)` over a
        // `(Int, Bool)` accumulator walls on exactly that (#905). Reading the one
        // definition instead of restating a subset of it keeps the two from drifting
        // again. `String` stays a separate clause: it is flat under rc_dec too, but it
        // is a byte buffer rather than a slot block, so it is not that predicate's job.
        let one_level_exact =
            |ty: &Ty| -> bool { matches!(ty, Ty::String) || crate::lower::is_flat_scalar_block_ty(ty) };
        let is_nested_list_str = |ty: &Ty| -> bool {
            matches!(ty, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && matches!(a[0], Ty::String))
        };
        // A String-err Result capture (`(v) => s1` — the or_else recovery shape,
        // fuzz B-198) shares the DynListStr layout family: len-as-tag
        // `Result[scalar, String]` (Ok = len 0, nothing to free; Err = len 1
        // owning the message) and cap-as-tag `Result[String, String]` (len 1,
        // String payload @slot 0 either arm) are both freed EXACTLY by the
        // nested `__drop_list_str` walk — so they ride the nested-heap env
        // class. Any other Result instantiation keeps the honest-wall defer.
        let is_nested_result_str = |ty: &Ty| -> bool {
            matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2 && matches!(a[1], Ty::String)
                    && (!is_heap_ty(&a[0]) || matches!(a[0], Ty::String)))
        };
        let mut closure_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut rich_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut heap_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut nested_heap_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut cellmap_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut scalar_caps: Vec<(VarId, Ty)> = Vec::new();
        for (v, ty) in captured {
            // A SHARED-CELL capture (cells.rs): the env slot holds the CELL handle,
            // not a value copy — reads/writes inside the body go through the shared
            // slot (`sub.cell_of`, seeded in the prologue). Drop-class placement
            // rides the existing self-describing header: a SCALAR-inner cell is a
            // FLAT block (one rc_dec frees it; the raw inner slot is untouched); a
            // FLAT-HEAP-inner cell is physically a 1-slot DynListStr (the nested
            // walk decs slot 0 — a full free for a flat inner — then frees the cell);
            // a MAP-inner cell (`Map[String, scalar]`) takes the 4th header class
            // (`$__drop_closure` sweeps the inner map's key slots, then the map, then
            // the cell — a flat/nested dec would leak every key String).
            if self.cell_of.contains_key(&v) {
                match self.cell_class_of_ctx(&ty) {
                    Some(CellClass::Scalar) => heap_caps.push((v, ty)),
                    Some(CellClass::FlatHeap) => nested_heap_caps.push((v, ty)),
                    Some(CellClass::LenOwnedSlots) => cellmap_caps.push((v, ty)),
                    // A ROUTED cell (`Map[String, <record/variant>]`, #1143):
                    // rides the RICH env class behind a `cell:`-tagged wrapper —
                    // `__drop_env_rich`'s cell arms free the co-owned cell via
                    // the inner's type-specific generated map sweep.
                    Some(CellClass::Routed) => rich_caps.push((v, ty)),
                    None => {
                        crate::trace::trace("ALMIDE_DBG_ANF", || format!(
                            "[lift] {}: cell capture {v:?} class unadmitted ({ty:?})", self.fn_name));
                        return None;
                    }
                }
                continue;
            }
            if matches!(ty, Ty::Fn { .. }) {
                closure_caps.push((v, ty));
                continue;
            }
            if one_level_exact(&ty) {
                heap_caps.push((v, ty));
                continue;
            }
            if is_nested_list_str(&ty) || is_nested_result_str(&ty) {
                nested_heap_caps.push((v, ty));
                continue;
            }
            // #1547 shapes 2/3 — the port/adapter capture: a `List[<rich variant>]` /
            // `List[<recursive-drop record>]` (the repository `db` a returned record's
            // closure field closes over) rides the RICH env class. Its slot holds a
            // `[tag@12][list-handle@20]` wrapper block; the uniform `$__drop_closure`
            // walk hands it to the generated `__drop_env_rich`, whose tag arm recurses
            // via the element type's own `$__drop_list_<V>` / `$__drop_caplist_<R>` —
            // a flat or `__drop_list_str` free would leak every element's tree.
            // Structural admission (the same per-name mirrors the drop generators use);
            // the tag is `rich_env_tag(<element type name>)` — no registry to sync.
            if self.rich_capture_elem_name(&ty).is_some() {
                rich_caps.push((v, ty));
                continue;
            }
            // `!is_heap_ty` IS this bucket's promise — the crate's canonical name for
            // "one raw i64 slot, no refcount", which is exactly what the scalar region
            // of the env stores and what `ListGetScalar` reads back. This used to
            // restate a SUBSET of it (`Int | Bool`), so every other scalar was refused
            // for no physical reason: a `Float` capture in particular, on the belief
            // that its slot needed an f64↔i64 reinterpret the prim vocabulary lacked.
            // It does not — a MIR float local ALREADY holds its bits in an i64 and
            // every float op reinterprets around itself, so the bits round-trip through
            // the slot untouched. `(s) => d` over `let d: Float` walled on that
            // misreading (#954, surfaced by the nightly fuzz), and so did captures of
            // every sized int width. Reading the one definition instead of restating
            // part of it is the same fix `one_level_exact` above already took.
            if !is_heap_ty(&ty) {
                scalar_caps.push((v, ty));
                continue;
            }
            crate::trace::trace("ALMIDE_DBG_ANF", || format!(
                "[lift] {}: capture {v:?} outside the class slice ({ty:?})", self.fn_name));
            return None;
        }
        let n_closure = closure_caps.len();
        let n_rich = rich_caps.len();
        let n_heap = heap_caps.len();
        let n_nested_heap = nested_heap_caps.len();
        let n_cellmap = cellmap_caps.len();
        // ENV LAYOUT ORDER must match `$__drop_closure`'s class walk EXACTLY:
        // [closures][RICH][NESTED][FLAT][cell-map][scalars]. The chain previously
        // placed FLAT before NESTED while the walker frees NESTED before FLAT — a
        // LATENT mis-free whenever one closure captured BOTH classes at once (the
        // nested walk over a flat block reads raw i64 slots as handles; the flat dec
        // of a nested block leaks its elements). No corpus shape co-captured both
        // until the cell classes made it reachable (`var count` + `var acc` mutated
        // through one stored closure).
        let captures: Vec<(VarId, Ty)> = closure_caps
            .into_iter()
            .chain(rich_caps)
            .chain(nested_heap_caps)
            .chain(heap_caps)
            .chain(cellmap_caps)
            .chain(scalar_caps)
            .collect();
        Some(CaptureLayout { captures, n_closure, n_rich, n_heap, n_nested_heap, n_cellmap })
    }

    /// The RICH-capture admission mirror (#1547 shapes 2/3): `Some(<element type
    /// name>)` iff `ty` is a `List[<non-generic named elem>]` whose element the drop
    /// generators give a per-element recursive free — a RICH variant (`$__drop_list_
    /// <V>`, unconditional) or a recursive-drop record (`$__drop_<R>` unconditional;
    /// the capture side's `$__drop_caplist_<R>` loop rides it). The name feeds
    /// [`crate::lower::rich_env_tag`] — the SAME function `generate_closure_env_rich_
    /// sources` derives its dispatcher arm tags with, so admission ⊆ generation holds
    /// per name with no registry to keep in sync. Everything else declines (an
    /// honest wall, exactly as before).
    fn rich_capture_elem_name(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::List, a) = ty else { return None };
        if a.len() != 1 {
            return None;
        }
        let elem = &a[0];
        if !matches!(elem, Ty::Named(_, args) if args.is_empty()) {
            return None;
        }
        if let Some(vn) = self.variant_layouts.is_rich_variant_ty(elem, &|rn| {
            crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
        }) {
            return Some(vn);
        }
        self.record_drop_type_name(elem)
    }

    /// Phase 3 of [`Self::lift_lambda`]: resolve every capture to a lowered local value
    /// (a cell capture to its CELL BLOCK). Verbatim.
    fn resolve_capture_values(&self, layout: &CaptureLayout) -> Option<Vec<ValueId>> {
        // Every capture must resolve to a lowered local value HERE (a capture of a
        // deferred/opaque binding has no readable value). A captured Fn var must be a
        // KNOWN closure block (closure_values), or its slot would hold a non-block.
        let mut cap_vals: Vec<ValueId> = Vec::new();
        for (i, (v, _)) in layout.captures.iter().enumerate() {
            // A cell capture resolves to its CELL BLOCK (shared storage), not a value.
            let cv = match self.cell_of.get(v) {
                Some(&c) => c,
                None => *self.value_of.get(v)?,
            };
            if i < layout.n_closure && !self.closure_values.contains(&cv) {
                return None;
            }
            cap_vals.push(cv);
        }
        Some(cap_vals)
    }

    /// Phase 4 of [`Self::lift_lambda`]: lower the body in a FRESH sub-context (its own
    /// value space, the globals/layout registries shared), emit the env prologue that
    /// reads each capture back out of the block, and push the lifted fn — returning its
    /// name for the block's funcref slot. Verbatim.
    fn lower_lifted_lambda_body(
        &mut self,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        layout: &CaptureLayout,
    ) -> Option<String> {
        // Lower the body in a FRESH sub-context sharing only the globals (its own value
        // space + params). A failure (a body outside the subset) aborts the lift cleanly —
        // nothing is emitted into `self`, so the caller's deferred fallback stays sound.
        // The lifted fn's NAME is precomputed and seeded as the sub-context's fn_name:
        // a NESTED lift inside this body then names itself `__lambda_<THIS lambda>_<k>`
        // — unique. Inheriting the parent fn_name made the inner lambda collide with the
        // parent's own `__lambda_<fn>_0` (sub.lifted starts empty), and the by-NAME
        // FuncRef resolution dispatched the WRONG lambda (hof_closure_string_tail's
        // nested bench ran the alpha body). `self.lifted` is untouched while `sub`
        // lowers (nested lifts land in `sub.lifted`), so the index is stable here.
        let name = format!("__lambda_{}_{}", self.fn_name, self.lifted.len());
        let mut sub = LowerCtx {
            globals: self.globals.clone(),
            fn_name: name.clone(),
            // The lifted body may access a record/tuple field (`(p) => p.x`), so it needs
            // the VALUE-MODEL field registry too.
            record_layouts: self.record_layouts.clone(),
            // …and the VARIANT registry: a custom-ADT `match` inside the lambda
            // (`list.filter((t) => match t { Empty => false, _ => true })`) resolved
            // against an EMPTY by_type without it, fell past the executable variant
            // match, and linearized to a deferred Const-0 — every element filtered
            // out (the closures_and_variants silent miscompile, 2026-07-03).
            variant_layouts: self.variant_layouts.clone(),
            // …and the module-global initializers, so a lambda referencing a
            // top-level `let` materializes its real value exactly like the
            // enclosing fn does.
            global_inits: self.global_inits.clone(),
            // …and the SHARED-CELL var set, so a NESTED lift inside this body
            // re-captures a cell as a cell (its own `cell_of` seeds below).
            cell_vars: self.cell_vars.clone(),
            // A lambda whose body types `Result[…]` returns a REAL carrier — a
            // fallible callback's `(n) => record_seen(n)` hands the block to the
            // `__fallible_*` twin, which matches on it. A lambda has no effect-fn
            // synthetic `Result[Unit,_]` (the voided-tail convention this flag
            // gates), so leaving it false VOIDED a `Result[Unit,String]` body:
            // the lifted fn returned nothing while the twin's `call_indirect`
            // expects the carrier — `indirect call type mismatch` at runtime
            // (the fallible-each trap, latent while the file's other walls kept
            // it on the native leg; #1134 Shape 2 exposed it).
            decl_ret_is_result: matches!(
                &body.ty,
                Ty::Applied(
                    almide_lang::types::constructor::TypeConstructorId::Result
                        | almide_lang::types::constructor::TypeConstructorId::Option,
                    _
                )
            ),
            // A FALLIBLE lambda (ADR-0009: fallibility-inferred mini fallible
            // fn) reaches the lift with its carrier ALREADY in `body.ty`
            // (`Result[B, String]` — L3/L5 ran upstream), so the sub-context
            // classifies it exactly like a declared-Result fn: the one `!`
            // rule then owns its propagation instead of declining fn-family.
            // A bare-typed body (an L9 test-block lambda — deliberately
            // channel-less) stays None and keeps its honest wall.
            decl_ret_family: match &body.ty {
                t @ Ty::Applied(
                    almide_lang::types::constructor::TypeConstructorId::Result,
                    _,
                ) => Some(crate::lower::result_family(t)),
                _ => None,
            },
            decl_fn_err: match &body.ty {
                Ty::Applied(
                    almide_lang::types::constructor::TypeConstructorId::Result,
                    a,
                ) if a.len() == 2 => Some(a[1].clone()),
                _ => None,
            },
            ..Default::default()
        };
        // The leading ENV param: the closure block itself, BORROWED (the caller owns it
        // and keeps it live across the call — the v1 heap-param convention).
        let env_pv = sub.fresh_value();
        sub.param_values.insert(env_pv);
        let mut mir_params =
            vec![crate::MirParam { value: env_pv, repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT } }];
        for (v, ty) in params {
            let pv = sub.fresh_value();
            sub.value_of.insert(*v, pv);
            // A FUNCTION-typed PARAM (`list.map(fns, (f) => f(10))` — `f`'s own type is
            // `(Int)->Int`, NOT a capture): mirrors `bind_params`'s IDENTICAL Fn-param arm
            // exactly — the caller passes a closure block, and `f(x)` inside the lifted
            // body must lower to `Op::CallIndirect` through it. `lift_lambda`'s param loop
            // never had this (only a CAPTURED closure got `closure_values.insert`, at the
            // prologue loop below) — a lifted lambda whose OWN parameter is itself callable
            // (the `list.map` over a `List[Closure]` shape) fell to `lower_body_into`
            // declining `f(10)` as a call through an unknown target, so `lift_lambda`
            // returned `None` and the whole HOF call walled.
            if matches!(ty, Ty::Fn { .. }) {
                sub.closure_values.insert(pv);
            }
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
        // PROLOGUE: read each capture back out of the env block (slot 2 + i — slot 0 is
        // the fnidx, slot 1 the drop header). A closure/heap capture loads its HANDLE
        // (`LoadHandle`) and is BORROWED inside the body (the env owns it — the param
        // discipline: a body that consumes/returns it must Dup first); a captured Fn
        // handle also joins the sub-context's `closure_values` so `g(x)` inside the body
        // dispatches (the `compose` shape). A scalar capture is a raw 64-bit load. All
        // Prim reads — no ownership events (the block is the caller's).
        for (i, (v, ty)) in layout.captures.iter().enumerate() {
            let val = sub.fresh_value();
            if i < layout.handle_slots() {
                let h = sub.fresh_value();
                sub.ops
                    .push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![env_pv] });
                let off = sub.fresh_value();
                sub.ops
                    .push(Op::ConstInt { dst: off, value: layout::slot_offset(2 + i) as i64 });
                let addr = sub.fresh_value();
                sub.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
                sub.ops.push(Op::Prim {
                    kind: PrimKind::LoadHandle,
                    dst: Some(val),
                    args: vec![addr],
                });
                sub.param_values.insert(val);
                if i < layout.n_closure {
                    sub.closure_values.insert(val);
                }
                // A RICH capture (#1547 shapes 2/3): `val` is the `[tag@12][payload@20]`
                // WRAPPER — the body wants the PAYLOAD. One more deref binds it,
                // borrowed exactly like every other env-owned capture. A plain rich
                // LIST capture binds `value_of` with the read-shape seed `bind_params`
                // gives a List param; a ROUTED CELL capture (#1143) binds the loaded
                // CELL into `sub.cell_of` instead, so body reads load the shared slot
                // fresh and body writes store through it.
                if layout.is_rich(i) {
                    let iwh = sub.fresh_value();
                    sub.ops
                        .push(Op::Prim { kind: PrimKind::Handle, dst: Some(iwh), args: vec![val] });
                    let o20 = sub.fresh_value();
                    sub.ops.push(Op::ConstInt { dst: o20, value: 20 });
                    let laddr = sub.fresh_value();
                    sub.ops.push(Op::IntBinOp { dst: laddr, op: IntOp::Add, a: iwh, b: o20 });
                    let lv = sub.fresh_value();
                    sub.ops.push(Op::Prim {
                        kind: PrimKind::LoadHandle,
                        dst: Some(lv),
                        args: vec![laddr],
                    });
                    sub.param_values.insert(lv);
                    if self.cell_of.contains_key(v) {
                        sub.cell_of.insert(*v, lv);
                        sub.var_decl_tys.insert(*v, ty.clone());
                    } else {
                        sub.seed_variant_param(lv, ty);
                        sub.value_of.insert(*v, lv);
                    }
                    continue;
                }
                // A SHARED-CELL capture: the loaded handle IS the cell block — map the
                // var into the sub-context's `cell_of` (NOT `value_of`), so body reads
                // load the slot fresh and body assigns store through it. The inner
                // type rides along for the read/write class dispatch.
                if self.cell_of.contains_key(v) {
                    sub.cell_of.insert(*v, val);
                    sub.var_decl_tys.insert(*v, ty.clone());
                    continue;
                }
            } else {
                // Rung-5 closures slab: a SCALAR capture reads its slot through the
                // TARGET-NEUTRAL `Op::ListGetScalar` on the env block (wasm renders the
                // bounds-checked element load; native `env[slot]`) — the same pattern as
                // record fields and variant payloads. Heap/closure captures keep the
                // h-based `LoadHandle` above (native walls them honestly).
                let idx = sub.fresh_value();
                sub.ops.push(Op::ConstInt { dst: idx, value: (2 + i) as i64 });
                sub.ops.push(Op::ListGetScalar { dst: val, list: env_pv, idx });
            }
            sub.value_of.insert(*v, val);
        }
        let ret = match sub.lower_body_into(body) {
            Ok(r) => r,
            Err(e) => {
                crate::trace::trace("ALMIDE_DBG_ANF", || format!("[lift] body lower failed for {name}: {e:?}"));
                return None;
            }
        };
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
        Some(name)
    }

    /// Phase 5 of [`Self::lift_lambda`]: materialize the CLOSURE BLOCK itself — the
    /// all-scalar-capture `ListLit` fast path, else the prim alloc + per-capture
    /// Dup/Store/Consume co-own. Verbatim.
    fn materialize_closure_block(
        &mut self,
        name: String,
        layout: &CaptureLayout,
        cap_vals: Vec<ValueId>,
    ) -> ValueId {
        // Materialize the CLOSURE BLOCK: a DynList of 2 + k slots — slot 0 the funcref
        // table index, slot 1 the SELF-DESCRIBING drop header (n_heap | n_nested_heap<<16
        // | n_closure<<32 — three 16-bit counts, what lets the uniform `$__drop_closure`
        // free any closure block at any drop site without lowering-time mask knowledge),
        // then the captures (closure, flat heap, nested heap, scalar).
        // Rung-5 closures slab: an ALL-SCALAR-capture env block is a plain slot list
        // ([fnidx, drop-header=0, scalars…]), so the TARGET-NEUTRAL `Op::ListLit`
        // builds it on both legs — same cert `i`, same block bytes on wasm, a
        // `Vec<i64>` on native. Heap/closure captures keep the prim path below
        // (their Dup/Consume co-own dance needs the address stores).
        if layout.handle_slots() == 0 {
            let fr = self.fresh_value();
            self.ops.push(Op::FuncRef { dst: fr, name });
            let hdr = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: hdr, value: 0 });
            let mut elems: Vec<ValueId> = Vec::with_capacity(2 + cap_vals.len());
            elems.push(fr);
            elems.push(hdr);
            elems.extend(cap_vals.iter().copied());
            let blk = self.fresh_value();
            self.ops.push(Op::ListLit { dst: blk, elems });
            // EXACT tracking mirror of the prim path below.
            self.live_heap_handles.push(blk);
            self.closure_values.insert(blk);
            return blk;
        }
        // RICH captures (#1547 shapes 2/3): wrap each captured list in a fresh
        // 2-slot `[tag@12][list-handle@20]` block FIRST — the env slot then holds
        // the wrapper, and `$__drop_closure`'s rich arm hands it to the generated
        // `__drop_env_rich` (tag → the element type's recursive list free). The
        // wrapper co-owns the list (`Dup` + move-in, the original var's scope-end
        // drop untouched) and is itself MOVED into the env below (no Dup — it
        // exists solely for this block).
        let mut cap_vals = cap_vals;
        let mut rich_wrappers: std::collections::HashSet<ValueId> = std::collections::HashSet::new();
        for i in 0..layout.captures.len() {
            if !layout.is_rich(i) {
                continue;
            }
            // A ROUTED cell capture tags `cell:<map drop name>` (the dispatcher's
            // cell arms free the co-owned CELL — wrapper @20 holds the cell, whose
            // @12 holds the map); a plain rich list capture tags its element name.
            let (cap_var, cap_ty) = &layout.captures[i];
            let tag_name = if self.cell_of.contains_key(cap_var) {
                self.map_named_value_drop(cap_ty)
                    .map(|n| format!("cell:{n}"))
                    .expect("Routed cell admitted only via map_named_value_drop")
            } else {
                self.rich_capture_elem_name(cap_ty)
                    .expect("rich class admitted only via rich_capture_elem_name")
            };
            let tag_val = crate::lower::rich_env_tag(&tag_name);
            let two = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: two, value: 2 });
            let w = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst: w,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: Init::DynList { len: two },
            });
            let wh = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(wh), args: vec![w] });
            let tv = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: tv, value: tag_val });
            let o12 = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: o12, value: 12 });
            let s0 = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: s0, op: IntOp::Add, a: wh, b: o12 });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s0, tv] });
            let owned = self.fresh_value();
            self.ops.push(Op::Dup { dst: owned, src: cap_vals[i] });
            let lh = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(lh), args: vec![owned] });
            let o20 = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: o20, value: 20 });
            let s1 = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: s1, op: IntOp::Add, a: wh, b: o20 });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s1, lh] });
            self.ops.push(Op::Consume { v: owned });
            self.live_heap_handles.retain(|x| *x != owned);
            cap_vals[i] = w;
            rich_wrappers.insert(w);
        }
        let len_c = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len_c, value: (2 + cap_vals.len()) as i64 });
        let blk = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: blk,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: Init::DynList { len: len_c },
        });
        let fr = self.fresh_value();
        self.ops.push(Op::FuncRef { dst: fr, name });
        let hdr = self.fresh_value();
        self.ops.push(Op::ConstInt {
            dst: hdr,
            value: (layout.n_heap as i64)
                | ((layout.n_nested_heap as i64) << 16)
                | ((layout.n_closure as i64) << 32)
                | ((layout.n_cellmap as i64) << 48)
                | ((layout.n_rich as i64) << 56),
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![blk] });
        for (i, v) in [fr, hdr].into_iter().chain(cap_vals.iter().copied()).enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(i) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            // A closure/heap/nested-heap capture: the closure CO-OWNS it — `Dup` a fresh
            // reference (CowSafety makes the share value-semantics-safe: any later in-place
            // mutation clones-on-shared), store its handle, `Consume` the fresh ref into
            // the block (cert `a` + `m`; the original var's scope-end drop is untouched).
            // The fnidx/header/scalar slots store the raw value.
            let cap_index = i as i64 - 2; // captures start at slot 2
            if cap_index >= 0 && (cap_index as usize) < layout.handle_slots() {
                // A RICH wrapper is MOVED in (built above solely for this env slot —
                // a Dup would strand one reference); every other handle capture keeps
                // the Dup co-own.
                let owned = if rich_wrappers.contains(&v) {
                    v
                } else {
                    let owned = self.fresh_value();
                    self.ops.push(Op::Dup { dst: owned, src: v });
                    owned
                };
                let handle = self.fresh_value();
                self.ops
                    .push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![owned] });
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![addr, handle],
                });
                self.ops.push(Op::Consume { v: owned });
                self.live_heap_handles.retain(|x| *x != owned);
            } else {
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![addr, v],
                });
            }
        }
        // A fresh owned heap value: dropped at scope end unless a consumer moves it out
        // (a tail return removes it from the live set). `closure_values` routes its drop
        // to the recursive `$__drop_closure` (`drop_op_for`).
        self.live_heap_handles.push(blk);
        self.closure_values.insert(blk);
        blk
    }
}

/// The env-block layout ONE lift decided: the captures in SLOT ORDER
/// (`[closures][nested heap][flat heap][cell-map][scalars]`) plus the per-class counts
/// the self-describing drop header carries. Bundled so the phases after the partition
/// read one value instead of five positional `usize`s that could be transposed — and a
/// transposition here mis-frees the block.
struct CaptureLayout {
    captures: Vec<(VarId, Ty)>,
    n_closure: usize,
    n_rich: usize,
    n_heap: usize,
    n_nested_heap: usize,
    n_cellmap: usize,
}

impl CaptureLayout {
    /// Slots `2..2 + handle_slots()` hold HANDLES (closure, rich-wrapper, nested-heap,
    /// flat-heap and cell-map captures); everything after them is a raw scalar slot.
    /// The prologue's `LoadHandle`-vs-`ListGetScalar` split, the header's zero test and
    /// the block store's co-own test all key on this same boundary.
    fn handle_slots(&self) -> usize {
        self.n_closure + self.n_rich + self.n_heap + self.n_nested_heap + self.n_cellmap
    }

    /// Is capture index `i` in the RICH class ([closures][RICH][…] order)?
    fn is_rich(&self, i: usize) -> bool {
        i >= self.n_closure && i < self.n_closure + self.n_rich
    }
}


include!("binds_b.rs");
