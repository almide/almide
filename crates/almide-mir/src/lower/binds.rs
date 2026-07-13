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
        let mut closure_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut heap_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut nested_heap_caps: Vec<(VarId, Ty)> = Vec::new();
        let mut scalar_caps: Vec<(VarId, Ty)> = Vec::new();
        for (v, ty) in cc.out {
            if matches!(ty, Ty::Fn { .. }) {
                closure_caps.push((v, ty));
            } else if one_level_exact(&ty) {
                heap_caps.push((v, ty));
            } else if is_nested_list_str(&ty) {
                nested_heap_caps.push((v, ty));
            } else if matches!(ty, Ty::Int | Ty::Bool) {
                scalar_caps.push((v, ty));
            } else {
                return None;
            }
        }
        let n_closure = closure_caps.len();
        let n_heap = heap_caps.len();
        let n_nested_heap = nested_heap_caps.len();
        let captures: Vec<(VarId, Ty)> = closure_caps
            .into_iter()
            .chain(heap_caps)
            .chain(nested_heap_caps)
            .chain(scalar_caps)
            .collect();
        // Every capture must resolve to a lowered local value HERE (a capture of a
        // deferred/opaque binding has no readable value). A captured Fn var must be a
        // KNOWN closure block (closure_values), or its slot would hold a non-block.
        let mut cap_vals: Vec<ValueId> = Vec::new();
        for (i, (v, _)) in captures.iter().enumerate() {
            let cv = *self.value_of.get(v)?;
            if i < n_closure && !self.closure_values.contains(&cv) {
                return None;
            }
            cap_vals.push(cv);
        }
        // Lower the body in a FRESH sub-context sharing only the globals (its own value
        // space + params). A failure (a body outside the subset) aborts the lift cleanly —
        // nothing is emitted into `self`, so the caller's deferred fallback stays sound.
        let mut sub = LowerCtx {
            globals: self.globals.clone(),
            fn_name: self.fn_name.clone(),
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
        for (i, (v, _)) in captures.iter().enumerate() {
            let h = sub.fresh_value();
            sub.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![env_pv] });
            let off = sub.fresh_value();
            sub.ops
                .push(Op::ConstInt { dst: off, value: layout::slot_offset(2 + i) as i64 });
            let addr = sub.fresh_value();
            sub.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            let val = sub.fresh_value();
            if i < n_closure + n_heap + n_nested_heap {
                sub.ops.push(Op::Prim {
                    kind: PrimKind::LoadHandle,
                    dst: Some(val),
                    args: vec![addr],
                });
                sub.param_values.insert(val);
                if i < n_closure {
                    sub.closure_values.insert(val);
                }
            } else {
                sub.ops.push(Op::Prim {
                    kind: PrimKind::Load { width: 8 },
                    dst: Some(val),
                    args: vec![addr],
                });
            }
            sub.value_of.insert(*v, val);
        }
        let ret = sub.lower_body_into(body).ok()?;
        let name = format!("__lambda_{}_{}", self.fn_name, self.lifted.len());
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
            value: (n_heap as i64) | ((n_nested_heap as i64) << 16) | ((n_closure as i64) << 32),
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
            if cap_index >= 0 && (cap_index as usize) < n_closure + n_heap + n_nested_heap {
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

    /// Lower a `List[String]` LITERAL to an alloc_list_str + per-element move-in. Each element is
    /// stored into a nested-ownership `DynListStr` (freed recursively via `DropListStr` at scope end,
    /// cert `i`+`d`). Element ownership by kind:
    /// - a LitStr / ConcatStr is a FRESH owned String (cert `i`), MOVED in (store handle + `Consume`);
    /// - a `Var` binds a STILL-LIVE owned String the list must not steal — it gets its OWN reference
    ///   via `Dup` (cert `a`/+1), then that fresh handle is `Consume`d into the list. The original var
    ///   keeps its reference (its scope-end drop stays balanced), and the list owns a distinct one
    ///   (DropListStr releases it) — no double-free, no leak. (`[e0, e1]` of `string.repeat` results,
    ///   `[a, a]` of a computed `a` — the same var may appear twice, each occurrence its own `Dup`.)
    /// GATED to those element kinds; any other (a heap-returning call as a bare element, a member
    /// access) defers. Gate-first so no partial emission.
    pub(crate) fn try_lower_str_list_literal(&mut self, value: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::List { elements } = &value.kind else {
            return None;
        };
        // A `List[String]` OR a `List[ScalarAggregate]` whose every element is a SCALAR-only
        // record OR tuple literal — all are NESTED-OWNERSHIP lists (i64 slots holding owned heap
        // handles, recursively freed via `DropListStr`'s per-slot `rc_dec`). A scalar record/tuple
        // is a FLAT block (no inner heap slots), so `rc_dec` frees it correctly (no String-specific
        // recursion). This is what makes `[Point{..}, Point{..}]` (then `list.map(…, p => p.x)`)
        // AND `[(1, 100), (127, 300)]` (then a `for (x, y) in …`) materialize as a REAL list of the
        // right length. A `List` of any OTHER heap element (a heap-field record/tuple, a List, a
        // call result) defers — a heap-field aggregate needs the masked recursive drop this builder
        // (a flat per-slot `rc_dec`) does not emit, so its inner heap would leak; gated out by
        // `scalar_slots` (which is `None` for any non-scalar field).
        let elem_str = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String));
        let elem_scalar_aggregate = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && self.aggregate_field_tys(&a[0])
                    .and_then(|(_, tys)| layout::scalar_slots(&tys)).is_some());
        // A `List[Value]` (`[value.int(1), value.str("a")]`) — its slots hold OWNED dynamic Values,
        // each freed RECURSIVELY at scope end via `Op::DropListValue` (`$__drop_value` per element),
        // so a Str/Array element's nested payload is reclaimed. Elements are fresh-owned ctor CALLS
        // (Module `value.*` / a Named `Value`-returning fn) or a tracked Value Var (Dup'd).
        let elem_value = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_value_ty(&a[0]));
        // A `List[(String, Value)]` (`[(key, val)]` — the yaml `pairs + [(k,v)]` append) — each element is a
        // HEAP-FIELD (String, Value) tuple, materialized via `try_lower_tuple_construct` (rc-owning both
        // fields) and reclaimed RECURSIVELY at scope end via `Op::DropListStrValue` (per tuple: rc_dec the
        // String + `$__drop_value` the Value). A flat `DropListStr` would leak each tuple's payloads.
        let elem_str_value = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && is_value_ty(&tys[1])));
        // A `List[List[String]]` (`[cur]` — the csv `rows + [cur]` singleton) — each element is an
        // owned `List[String]` block (Dup'd in, the new list co-owns each row), reclaimed RECURSIVELY
        // at scope end via `Op::DropListListStr` (the nested cell + row free). A flat `DropListStr`
        // would only rc_dec each row HANDLE, leaking the cell Strings.
        let elem_list_str = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && matches!(b[0], Ty::String)));
        // A `List[List[scalar]]` literal (`[[1.0, 2.0], [3.0]]` — the nn Matrix seed):
        // each inner list is a FLAT scalar block whose rc_dec IS its full free, so the
        // outer list's per-slot DropListStr reclaims everything — same ownership shape
        // as List[String].
        let elem_list_scalar = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && !is_heap_ty(&b[0])));
        // A `List[List[List[scalar]]]` / `List[Matrix]` literal (`[matrix.to_lists(a),
        // matrix.to_lists(b)]` — the nn concat_rows flatten argument): each element is a
        // TWO-LEVEL block (a matrix: its slots hold owned flat row blocks), so the list's
        // scope-end drop must be the nested `DropListListStr` (rc_dec each element's row
        // slots + the element block, then the list — `list_list_str_lists`). Elements are
        // fresh-owned matrix.* / to_lists CALLS moved in, or tracked Vars Dup'd in.
        let elem_list_flat = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Matrix
                | Ty::Applied(TypeConstructorId::Matrix, _))
        ) || matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && matches!(&b[0],
                    Ty::Applied(TypeConstructorId::List, c) if c.len() == 1 && !is_heap_ty(&c[0]))));
        // A `List[(<scalar>, String)]` (`[(i, line)]` — list.enumerate; `[(true, "yes")]` —
        // the bool-key map literal): each element is a (scalar @12, String @20) tuple,
        // materialized via `try_lower_tuple_construct` and reclaimed RECURSIVELY at scope end
        // via `$__drop_list_int_str` (per tuple: rc_dec the String only — correct for ANY
        // scalar key, the slot layout is identical). A flat `DropListStr` would leak each
        // tuple's String.
        let elem_int_str = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Tuple(tys) if tys.len() == 2 && !is_heap_ty(&tys[0]) && matches!(tys[1], Ty::String)));
        // A `(String, Int)` TUPLE element (`[("alpha", 1), …]` — the tokenizer vocab
        // pairs) — `DropListStrInt` rc_decs each tuple's String slot0 only.
        let elem_str_int = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::Int)));
        // A HEAP-FIELD record element with a generated recursive drop (`[{key: p, val: "1"}]`
        // — porta's List[EnvVar] literal): each element materializes via the full record
        // builder (rc-owning its heap fields) and the list drops via `$__drop_list_<R>`
        // (each element → `$__drop_<R>`), exactly the concat-side `record_elem` route.
        let elem_recdrop = match &value.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                self.record_drop_type_name(&a[0])
            }
            _ => None,
        };
        // A `List[V]` whose element `V` is a FLAT custom variant (every ctor scalar-only — a nullary
        // enum like `Capability = | CapIO | CapProcess`, or a scalar-payload variant): each element is
        // a fresh OWNED tag-block (`try_lower_variant_ctor`, cert `i`) moved into the slot; the list's
        // scope-end `DropListStr` `rc_dec`s each element + the block (a flat variant block owns no inner
        // handle, so `rc_dec` is its full free — the SAME proven `List[String]` cert). A variant with a
        // `String`/nested/`List` field is NOT flat (`is_flat_variant_ty` = false) and stays walled.
        let elem_flat_variant = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && self.variant_layouts.is_flat_variant_ty(&a[0]));
        // A RICH (recursive-drop) variant element (`[instr_r.val]` — the `acc + [instr_r.val]` singleton
        // operand, `instr_r.val: Instr`). Each element is a fresh OWNED ref (Dup of the borrowed field /
        // Var); the list's `$__drop_list_<V>` frees each RECURSIVELY via `$__drop_<V>` — a flat per-slot
        // `rc_dec` (`DropListStr`) would leak each element's nested `List[Instr]`.
        let elem_rich_variant: Option<String> = match &value.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                self.variant_layouts.is_rich_variant_ty(&a[0])
            }
            _ => None,
        };
        // An EMPTY literal of an admitted class builds the same zero-length block
        // (`ok([])` — the tail-duplicated else-arm's Result payload, porta
        // resolve_env). The element loop below is vacuous; DropListStr over a
        // len-0 block frees just the block.
        if !elem_str && !elem_scalar_aggregate && !elem_value && !elem_str_value && !elem_list_str
            && !elem_int_str && !elem_str_int && !elem_flat_variant && elem_rich_variant.is_none()
            && elem_recdrop.is_none() && !elem_list_scalar && !elem_list_flat
        {
            return None;
        }
        // Gate: every element must be a fresh-owned String (LitStr/ConcatStr) OR a tracked
        // heap Var (so we can Dup it) OR — for an aggregate list — a scalar-only record/tuple
        // LITERAL we can materialize OR — for a value list — a fresh-owned Value CALL. A Var whose
        // value isn't tracked here cannot be Dup'd → defer.
        let all_lowerable = elements.iter().all(|e| match &e.kind {
            IrExprKind::LitStr { .. } | IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => true,
            // A `${...}` interpolation element (`["", "[[${emit_path(...)}]]"]` — the toml
            // emit_sections shape): a fresh owned String, moved into the slot exactly like a concat.
            IrExprKind::StringInterp { .. } => elem_str,
            IrExprKind::Var { id } => self.value_of.contains_key(id),
            // An inner scalar-list LITERAL (`[1.0, 2.0]` in a List[List[Float]]) —
            // buildable iff every inner element is a lowerable scalar (the flat
            // builder itself re-checks; a non-scalar inner defers there).
            IrExprKind::List { .. } => elem_list_scalar,
            // A RECORD-CTOR VARIANT element (`[Click { x, y }, KeyPress { key }, …]` — the
            // event-list shape): a `Record` literal whose NAME is a registered constructor is
            // a TAGGED variant value, materialized via `try_lower_variant_ctor` below (NOT the
            // plain-record path). Gated on the list being a flat/rich variant list.
            IrExprKind::Record { name: Some(n), .. }
                if (elem_flat_variant || elem_rich_variant.is_some())
                    && self.variant_layouts.ctor_to_type.contains_key(n.as_str()) =>
            {
                true
            }
            IrExprKind::Record { .. } => elem_scalar_aggregate || elem_recdrop.is_some(),
            IrExprKind::Tuple { .. } => elem_scalar_aggregate || elem_str_value || elem_int_str || elem_str_int,
            // A FLAT-variant CONSTRUCTOR element (`[CapIO, CapProcess]`) — a Named call whose name is a
            // registered constructor, materialized via `try_lower_variant_ctor` below.
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
                if (elem_flat_variant || elem_rich_variant.is_some())
                    && self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
            {
                true
            }
            // A flat-variant FIELD EXTRACTION element (`acc + [r.val]`) — admitted ONLY when the field
            // BORROW resolves (a tracked/param heap-aggregate container), so the build loop's `?` never
            // fails mid-build (which would leak partial ops). Mirrors `try_lower_heap_field_borrow`'s
            // container gate so the two never disagree.
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
                if (elem_flat_variant || elem_rich_variant.is_some() || elem_str || elem_value)
                    && is_heap_ty(&e.ty) =>
            {
                self.heap_field_container_tracked(object)
            }
            IrExprKind::Call { target: CallTarget::Named { .. } | CallTarget::Module { .. }, .. } => {
                // A Value-returning ctor call (elem_value), OR — for a List[String] — a String-returning
                // call element (`[string.slice(s,a,b)]` in `acc + [string.slice(…)]`, the dominant yaml
                // append shape): a fresh owned String, moved into the slot. `e.ty` is String here.
                // A matrix-shaped call element (`[matrix.to_lists(a), …]` — elem_list_flat) is the
                // same fresh-owned move-in; the list's DropListListStr reclaims it two levels deep.
                (elem_value && is_value_ty(&e.ty)) || elem_str || elem_list_flat
            }
            _ => false,
        });
        if !all_lowerable {
            return None;
        }
        let ptr = crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT };
        let n = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: n, value: elements.len() as i64 });
        let list = self.fresh_value();
        self.ops.push(Op::Alloc { dst: list, repr: ptr, init: Init::DynListStr { len: n } });
        // A `List[Value]` drops via `Op::DropListValue` (recursive `$__drop_value` per element); a
        // String/aggregate list via the flat-element `Op::DropListStr`. Marking the right set is what
        // makes the scope-end drop reclaim each element's nested payload (no leak).
        if elem_value {
            self.value_elem_lists.insert(list);
        } else if elem_str_value {
            self.str_value_elem_lists.insert(list);
        } else if elem_int_str {
            self.variant_drop_handles.insert(list, "list_int_str".to_string());
        } else if elem_str_int {
            self.variant_drop_handles.insert(list, "list_str_int".to_string());
        } else if elem_list_str || elem_list_flat {
            // elem_list_flat: each element is a matrix-shaped two-level block — the SAME
            // DropListListStr sweep (rc_dec each element's flat sub-blocks + the element,
            // then the list) is its exact recursive free.
            self.list_list_str_lists.insert(list);
        } else if let Some(vname) = &elem_rich_variant {
            // RECURSIVE per-element drop via the generated `$__drop_list_<V>`.
            self.variant_drop_handles.insert(list, format!("list_{vname}"));
        } else if let Some(rname) = &elem_recdrop {
            self.variant_drop_handles.insert(list, format!("list_{rname}"));
        } else {
            self.heap_elem_lists.insert(list);
        }
        self.materialized_lists.insert(list);
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![list] });
        for (i, elem) in elements.iter().enumerate() {
            let ev = match &elem.kind {
                IrExprKind::LitStr { value: s } => {
                    let obj = self.fresh_value();
                    self.ops.push(Op::Alloc { dst: obj, repr: ptr, init: Init::Str(s.clone()) });
                    obj
                }
                // A Var element: acquire a fresh owned reference (Dup) the list will own; the original
                // binding keeps its own reference. The dup is then Consume'd (moved) into the slot.
                IrExprKind::Var { id } => {
                    let src = *self.value_of.get(id)?;
                    let dup = self.fresh_value();
                    self.ops.push(Op::Dup { dst: dup, src });
                    dup
                }
                // A RECORD-CTOR VARIANT element (`Click { x, y }` where `Click` is a registered
                // ctor) — materialize the tagged variant block via `try_lower_variant_ctor` (NOT
                // the plain-record path), moved into the slot; the list's `$__drop_list_<V>` frees
                // each element recursively. Checked BEFORE the plain-record arms.
                IrExprKind::Record { name: Some(n), .. }
                    if (elem_flat_variant || elem_rich_variant.is_some())
                        && self.variant_layouts.ctor_to_type.contains_key(n.as_str()) =>
                {
                    self.try_lower_variant_ctor(elem)?
                }
                // A scalar-only record literal element — materialize a fresh OWNED record
                // block (`try_lower_scalar_record_construct`, cert `i`), moved into the slot.
                IrExprKind::Record { .. } if elem_recdrop.is_some() => {
                    self.try_lower_record_construct(elem)?
                }
                // An inner scalar-list LITERAL element (`[1.0, 2.0]` inside a
                // List[List[Float]] literal) — the flat scalar-slot builder yields a
                // fresh owned block moved into the outer slot.
                IrExprKind::List { elements: inner } if elem_list_scalar => {
                    self.try_lower_scalar_list_slots(inner)?
                }
                IrExprKind::Record { .. } => self.try_lower_scalar_record_construct(elem)?,
                // A scalar-only tuple literal element (`(1, 100)`) — materialize a fresh OWNED
                // flat 2-slot block (`try_lower_scalar_tuple_construct`, cert `i`), moved into the
                // slot. The SAME flat shape as a scalar record, so the list's per-slot `rc_dec`
                // frees it correctly.
                // A HEAP-FIELD `(String, Value)` tuple element (`(key, val)`) — materialize a fresh OWNED
                // mixed 2-slot block (`try_lower_tuple_construct`, rc-owning both fields), moved into the
                // slot; the list's `DropListStrValue` reclaims each tuple recursively.
                IrExprKind::Tuple { elements: tup_elems } if elem_str_value || elem_int_str || elem_str_int => {
                    self.try_lower_tuple_construct(tup_elems)?
                }
                IrExprKind::Tuple { .. } => self.try_lower_scalar_tuple_construct_for_elem(elem)?,
                // A heap-returning CALL element — a fresh OWNED value MOVED into the slot. A `Value`
                // ctor (`value.int(1)`) for a List[Value]; a String-returning call (`string.slice(s,a,b)`
                // — the dominant yaml `acc + [string.slice(…)]` append) for a List[String]. Module via
                // the pure-call path (→ a registered CallFn like `string.slice`), Named via CallFn. The
                // list's recursive drop (DropListValue / DropListStr) frees each at scope end.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if elem_value || elem_str || elem_list_flat =>
                {
                    self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, &elem.ty).ok()?
                }
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                    if elem_value || elem_str || elem_list_flat =>
                {
                    let lowered = self.lower_call_args(args).ok()?;
                    let obj = self.fresh_value();
                    let repr = repr_of(&elem.ty).ok()?;
                    self.ops.push(Op::CallFn {
                        dst: Some(obj),
                        name: name.as_str().to_string(),
                        args: lowered,
                        result: Some(repr),
                    });
                    obj
                }
                // A FLAT-variant CONSTRUCTOR element (`CapIO`) — materialize the fresh OWNED tag-block
                // (`try_lower_variant_ctor`, cert `i`) and move it into the slot. The block owns no
                // inner handle (flat), so the list's `DropListStr` `rc_dec` is its full free.
                IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if (elem_flat_variant || elem_rich_variant.is_some())
                    && self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
                {
                    self.try_lower_variant_ctor(elem)?
                }
                // A flat-variant FIELD EXTRACTION element (`acc + [r.val]`, `r.val: ValType` — the
                // wasm-binary `read_vec_valtype_acc` recursive accumulator, where the unwrapped result
                // record `r` carries >1 heap field so `r.val` stays a `Member` rather than folding to a
                // `Var`). BORROW the field handle (the container keeps owning it, freed by its own
                // recursive drop) and `Dup` a fresh OWNED reference to MOVE into the slot. The list
                // co-owns the rc-inc'd block; its `DropListStr` `rc_dec` balances the `Dup`, and a flat
                // variant block owns no inner handle so `rc_dec` is its full free — no double-free
                // (rc-aware). Cert-identical to the `Var` element case (the extra `LoadHandle` is a
                // cert-neutral prim load): `Dup` = `a`, `Consume` = the move into the list `i…m`.
                // A heap-FIELD String/Value element (`opts.profile` in `args + ["--profile",
                // opts.profile]`, the porta serialize_opts shape) — BORROW the field handle (the
                // container keeps owning it, freed by its own drop) and `Dup` a fresh OWNED reference
                // to MOVE into the slot. The list co-owns the rc-inc'd String/Value; its DropListStr /
                // DropListValue `rc_dec`s it, balancing the `Dup` — no double-free (rc-aware), no leak.
                // Cert-identical to the `Var` element case (the `Dup` = `a`, the move into the list =
                // `m`); the extra `LoadHandle` is a cert-neutral prim load.
                IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                    if elem_flat_variant || elem_rich_variant.is_some() || elem_str || elem_value =>
                {
                    let borrowed = self.try_lower_heap_field_borrow(elem)?;
                    let dup = self.fresh_value();
                    self.ops.push(Op::Dup { dst: dup, src: borrowed });
                    dup
                }
                // A `${...}` interpolation element → a fresh owned String via the interp concat chain.
                IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts)?,
                _ => self.try_lower_concat_str(elem)?,
            };
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
            let slot = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: slot, op: crate::IntOp::Add, a: h, b: off });
            let eh = self.fresh_value();
            self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(eh), args: vec![ev] });
            self.ops.push(Op::Prim { kind: crate::PrimKind::Store { width: 8 }, dst: None, args: vec![slot, eh] });
            self.ops.push(Op::Consume { v: ev });
        }
        Some(list)
    }
}

include!("binds_p2.rs");
include!("binds_p3.rs");
include!("binds_p4.rs");
