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
    pub(crate) fn lift_lambda(
        &mut self,
        params: &[(VarId, Ty)],
        body: &IrExpr,
    ) -> Option<ValueId> {
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
        let one_level_exact = |ty: &Ty| -> bool {
            matches!(ty, Ty::String)
                || matches!(ty, Ty::Applied(TypeConstructorId::List, a)
                    if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Float))
        };
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
        let mut heap_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut nested_heap_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut cellmap_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut scalar_caps: Vec<(VarId, Ty)> = Vec::new();
        for (v, ty) in cc.out {
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
                match cell_class_of(&ty) {
                    Some(CellClass::Scalar) => heap_caps.push((v, ty)),
                    Some(CellClass::FlatHeap) => nested_heap_caps.push((v, ty)),
                    Some(CellClass::MapSkv) => cellmap_caps.push((v, ty)),
                    None => {
                        if std::env::var("ALMIDE_DBG_ANF").is_ok() {
                            eprintln!("[lift] {}: cell capture {v:?} class unadmitted ({ty:?})", self.fn_name);
                        }
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
            if matches!(ty, Ty::Int | Ty::Bool) {
                scalar_caps.push((v, ty));
                continue;
            }
            if std::env::var("ALMIDE_DBG_ANF").is_ok() {
                eprintln!("[lift] {}: capture {v:?} outside the class slice ({ty:?})", self.fn_name);
            }
            return None;
        }
        let n_closure = closure_caps.len();
        let n_heap = heap_caps.len();
        let n_nested_heap = nested_heap_caps.len();
        let n_cellmap = cellmap_caps.len();
        // ENV LAYOUT ORDER must match `$__drop_closure`'s class walk EXACTLY:
        // [closures][NESTED][FLAT][cell-map][scalars]. The chain previously placed
        // FLAT before NESTED while the walker frees NESTED before FLAT — a LATENT
        // mis-free whenever one closure captured BOTH classes at once (the nested
        // walk over a flat block reads raw i64 slots as handles; the flat dec of a
        // nested block leaks its elements). No corpus shape co-captured both until
        // the cell classes made it reachable (`var count` + `var acc` mutated
        // through one stored closure).
        let captures: Vec<(VarId, Ty)> = closure_caps
            .into_iter()
            .chain(nested_heap_caps)
            .chain(heap_caps)
            .chain(cellmap_caps)
            .chain(scalar_caps)
            .collect();
        // Every capture must resolve to a lowered local value HERE (a capture of a
        // deferred/opaque binding has no readable value). A captured Fn var must be a
        // KNOWN closure block (closure_values), or its slot would hold a non-block.
        let mut cap_vals: Vec<ValueId> = Vec::new();
        for (i, (v, _)) in captures.iter().enumerate() {
            // A cell capture resolves to its CELL BLOCK (shared storage), not a value.
            let cv = match self.cell_of.get(v) {
                Some(&c) => c,
                None => *self.value_of.get(v)?,
            };
            if i < n_closure && !self.closure_values.contains(&cv) {
                return None;
            }
            cap_vals.push(cv);
        }
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
        for (i, (v, ty)) in captures.iter().enumerate() {
            let val = sub.fresh_value();
            if i < n_closure + n_heap + n_nested_heap + n_cellmap {
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
                if i < n_closure {
                    sub.closure_values.insert(val);
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
                if std::env::var("ALMIDE_DBG_ANF").is_ok() {
                    eprintln!("[lift] body lower failed for {name}: {e:?}");
                }
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
        if n_closure == 0 && n_heap == 0 && n_nested_heap == 0 && n_cellmap == 0 {
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
            return Some(blk);
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
            value: (n_heap as i64)
                | ((n_nested_heap as i64) << 16)
                | ((n_closure as i64) << 32)
                | ((n_cellmap as i64) << 48),
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
            if cap_index >= 0 && (cap_index as usize) < n_closure + n_heap + n_nested_heap + n_cellmap {
                let owned = self.fresh_value();
                self.ops.push(Op::Dup { dst: owned, src: v });
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
        Some(blk)
    }
}

include!("binds_b.rs");
