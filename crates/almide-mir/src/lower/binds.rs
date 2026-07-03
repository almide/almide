//! `LowerCtx` methods: binds (extracted from lower/mod.rs).

use super::*;
use crate::{CallArg, Init, Op, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrPattern, VarId,
};
use almide_lang::types::Ty;

impl LowerCtx {

    /// Lift a NON-CAPTURING lambda `(params) => body` into a fresh top-level MIR function
    /// (the closures machinery) and emit an `Op::FuncRef` binding its table slot, returning
    /// that scalar value (recorded in `funcref_values` so a later call through it lowers to
    /// `Op::CallIndirect`). Returns `None` for a CAPTURING lambda (its body references an
    /// enclosing local — a real closure environment the proven model cannot represent) or a
    /// body outside the lowering subset; the caller then keeps the deferred `Opaque` model.
    ///
    /// SOUNDNESS: the lifted body is lowered by the SAME `lower_body_into` as any function,
    /// so it carries its own ownership / name-totality / capability certificate that the
    /// proven checker re-verifies. Its capabilities reach THIS function through the
    /// `Op::FuncRef` edge — folded at closure CREATION (coverage-free; see
    /// `certificate::reachable_caps` / `reachable_caps_or_tainted`), so a printing lambda
    /// can never be silently caps-verified regardless of how/whether it is later invoked.
    /// The lambda is named `__lambda_<fn_name>_<n>` — file-unique (the harness keys the
    /// in-profile map by name), with nested lifts flattened into this function's set.
    pub(crate) fn lift_lambda(
        &mut self,
        params: &[(VarId, Ty)],
        body: &IrExpr,
    ) -> Option<ValueId> {
        // free_vars over the lambda's own params reports exactly its captures (a `Var` node
        // denotes only locals). A non-empty set ⇒ a real environment ⇒ not liftable here.
        let mut bound: std::collections::HashSet<VarId> = std::collections::HashSet::new();
        for (v, _) in params {
            bound.insert(*v);
        }
        if !almide_ir::free_vars::free_vars(body, &bound).is_empty() {
            return None;
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
        let mut mir_params = Vec::new();
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
        let dst = self.fresh_value();
        self.ops.push(Op::FuncRef { dst, name });
        self.funcref_values.insert(dst);
        Some(dst)
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
        // A `List[(Int, String)]` (`[(i, line)]` — the list.enumerate append) — each element is a
        // (Int @12 scalar, String @20 heap) tuple, materialized via `try_lower_tuple_construct` and
        // reclaimed RECURSIVELY at scope end via `$__drop_list_int_str` (per tuple: rc_dec the String
        // only). A flat `DropListStr` would leak each tuple's String.
        let elem_int_str = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String)));
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
        if (!elem_str && !elem_scalar_aggregate && !elem_value && !elem_str_value && !elem_list_str
            && !elem_int_str && !elem_flat_variant && elem_rich_variant.is_none())
            || elements.is_empty()
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
            IrExprKind::Record { .. } => elem_scalar_aggregate,
            IrExprKind::Tuple { .. } => elem_scalar_aggregate || elem_str_value || elem_int_str,
            // A FLAT-variant CONSTRUCTOR element (`[CapIO, CapProcess]`) — a Named call whose name is a
            // registered constructor, materialized via `try_lower_variant_ctor` below.
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
                if elem_flat_variant && self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
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
                (elem_value && is_value_ty(&e.ty)) || elem_str
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
        } else if elem_list_str {
            self.list_list_str_lists.insert(list);
        } else if let Some(vname) = &elem_rich_variant {
            // RECURSIVE per-element drop via the generated `$__drop_list_<V>`.
            self.variant_drop_handles.insert(list, format!("list_{vname}"));
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
                // A scalar-only record literal element — materialize a fresh OWNED record
                // block (`try_lower_scalar_record_construct`, cert `i`), moved into the slot.
                IrExprKind::Record { .. } => self.try_lower_scalar_record_construct(elem)?,
                // A scalar-only tuple literal element (`(1, 100)`) — materialize a fresh OWNED
                // flat 2-slot block (`try_lower_scalar_tuple_construct`, cert `i`), moved into the
                // slot. The SAME flat shape as a scalar record, so the list's per-slot `rc_dec`
                // frees it correctly.
                // A HEAP-FIELD `(String, Value)` tuple element (`(key, val)`) — materialize a fresh OWNED
                // mixed 2-slot block (`try_lower_tuple_construct`, rc-owning both fields), moved into the
                // slot; the list's `DropListStrValue` reclaims each tuple recursively.
                IrExprKind::Tuple { elements: tup_elems } if elem_str_value || elem_int_str => {
                    self.try_lower_tuple_construct(tup_elems)?
                }
                IrExprKind::Tuple { .. } => self.try_lower_scalar_tuple_construct_for_elem(elem)?,
                // A heap-returning CALL element — a fresh OWNED value MOVED into the slot. A `Value`
                // ctor (`value.int(1)`) for a List[Value]; a String-returning call (`string.slice(s,a,b)`
                // — the dominant yaml `acc + [string.slice(…)]` append) for a List[String]. Module via
                // the pure-call path (→ a registered CallFn like `string.slice`), Named via CallFn. The
                // list's recursive drop (DropListValue / DropListStr) frees each at scope end.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if elem_value || elem_str =>
                {
                    self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, &elem.ty).ok()?
                }
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                    if elem_value || elem_str =>
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
                    if elem_flat_variant && self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
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
