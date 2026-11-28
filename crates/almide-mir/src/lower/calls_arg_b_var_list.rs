// ── part of calls_arg_b.rs, include!-spliced (split for the 800-line budget) ──

impl LowerCtx {
    pub(crate) fn try_lower_heap_var_list_literal(&mut self, a: &IrExpr) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::List { elements } = &a.kind else { return None };
        if elements.is_empty() {
            return None;
        }
        let elem_ty = match &a.ty {
            Ty::Applied(TypeConstructorId::List, ts) if ts.len() == 1 => &ts[0],
            _ => return None,
        };
        let flat_content = matches!(elem_ty, Ty::String | Ty::Bytes)
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::Bytes, _))
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::List, inner)
                    if inner.len() == 1
                        && (!is_heap_ty(&inner[0])
                            || self.aggregate_field_tys(&inner[0])
                                .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
                                .is_some()))
            // An all-scalar aggregate element itself (`[c_add(e, t)]` — a (Float, Float)
            // Complex): its block holds only inline scalars, rc_dec IS its full free.
            || self
                .aggregate_field_tys(elem_ty)
                .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
                .is_some();
        // Beyond the flat classes: a ROUTABLE-MAP element (#1527's List-argument
        // top family — `list.get([m0, m1], i) ?? d` over Map values). The view
        // itself never reads element bytes (slots are borrowed handles) and the
        // hshare accessor family the router sends these callees to is likewise
        // handle-level; what must be EXACT is the scope-end drop story of any
        // element temp and of the shared-out result — and for every map class
        // below that is precisely the type-keyed route `materialized_call_arg`'s
        // seeding ladder already installs (named-value sweep / hval / ivh / msv /
        // mlo / the skv key sweep / the interleaved Map[heap,heap] DropListStr).
        // The FAMILY is the parameter (the same router classes the map ops use),
        // never a hardcoded key type.
        // ONE-LEVEL-EXACT under a per-slot dec: a scalar side is untouched, a
        // String/Bytes/flat-block/Flat-ctor side decs clean. Shared by the
        // map-layout arm below and the Result-element arm.
        let one_exact = |t: &Ty| !is_heap_ty(t)
            || matches!(t, Ty::String | Ty::Bytes)
            || crate::lower::is_flat_scalar_block_ty(t)
            || crate::lower::lenlist_elem_class(t)
                == Some(crate::lower::CtorElemClass::Flat);
        let routable_map = self.map_named_value_drop(elem_ty).is_some()
            || crate::lower::is_map_hval_ty(elem_ty)
            || crate::lower::is_map_ivh_ty(elem_ty)
            || crate::lower::is_map_msv_ty(elem_ty)
            || crate::lower::is_map_mlo_ty(elem_ty)
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::Map, a)
                    if a.len() == 2 && one_exact(&a[0]) && one_exact(&a[1]))
            // A `Result[<one-level-exact payload>, String]` element
            // (`list.get([acc0, n1], 5) ?? x2` over Result[Option[Float],
            // String] — the #1527 List-argument Result bucket's flat half):
            // the element and the shared-out result both ride the DynListStr
            // family's per-slot sweep (`is_heap_elem_list_ty` -> flat_elems),
            // which is exact precisely when the Ok payload is one-level-exact
            // (the Err String always is). A NESTED payload (Result[Result[..]],
            // Result[Map[..]], Option[String]) stays declined — its slot-0 dec
            // would leak the payload's interior.
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::Result, a)
                    if a.len() == 2 && matches!(a[1], Ty::String) && one_exact(&a[0]))
            ;
        if !flat_content && !routable_map {
            return None;
        }
        let ops_mark = self.ops.len();
        // Each element is a tracked heap Var (borrowed — Dup'd into the slot below) or a
        // heap-returning NAMED call (`[c_add(e, t)]` — fft's concat element): the call's
        // fresh OWNED result is moved into the slot directly (no Dup).
        let lhh_mark = self.live_heap_handles.len();
        let mut handles: Vec<ValueId> = Vec::with_capacity(elements.len());
        for e in elements {
            let Some(src) = self.heap_view_slot_handle(e) else {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            };
            handles.push(src);
        }
        let n = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: n, value: elements.len() as i64 });
        let list = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: list,
            repr: Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: Init::DynListStr { len: n },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![list] });
        for (i, src) in handles.into_iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: h, b: off });
            let oh = self.fresh_value();
            self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(oh), args: vec![src] });
            self.ops.push(Op::Prim {
                kind: crate::PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, oh],
            });
        }
        // The view block itself: tracked with NO element set → plain block-only Drop.
        self.live_heap_handles.push(list);
        Some(list)
    }
}
