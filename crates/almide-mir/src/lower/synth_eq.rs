// ─────────── recursive-eq: the synthesized per-type eq fn (the #791 brick) ───────────
//
// A `==` over a RECURSIVE variant (a payload that reaches the type itself —
// `Node(Tree, Tree)` directly, or `BeginEnd { patterns: List[Pat] }` through a
// list) cannot inline through `typed_slot_eq` (the static expansion never
// terminates; the depth cap walled it). The executable form is a SYNTHESIZED
// per-type helper the eq site CALLS:
//
//   fn __eq_<T>__<parent>(a: Ptr, b: Ptr) -> Bool     tag eq + tag-dispatched
//                                                     per-field compares; a
//                                                     self-typed field CALLS
//                                                     the helper (recursion)
//   fn __eq_list_<T>__<parent>(a: Ptr, b: Ptr) -> Bool  branchless length+fold:
//                                                     `res = len_eq; n = la *
//                                                     len_eq; loop { res &=
//                                                     __eq_T(a[i], b[i]) }`
//
// The helpers ride the `lifted` rail (extra MirFunctions in the parent's
// cluster) and are named per-PARENT so two functions' helpers never collide in
// the assembled module. Ownership certs are trivially empty (params are
// borrowed, every op is a scalar prim / a Bool-returning CallFn); the caps
// fold sees the helper calls as unknown callees and taints honestly.
//
// The classify counter mirrors this via [`variant_needs_eq_helper`] +
// [`eq_helper_call_count`] (exported below) — ONE predicate + ONE count shared
// by the engine and the gate, so `mir == ir` holds by construction for the
// single-eq-site shapes (a second site over the same type re-counts the helper
// bodies on the IR side and only ever lands on the conservative `ir > mir`
// taint, never the `mir > ir` breach).

impl LowerCtx {
    /// Does `tyname`'s layout reach ITSELF through any payload position the eq
    /// engine descends (a direct variant field, a `List[T]` element, an
    /// `Option[T]`/`Result[..]` payload)? Such a type cannot inline and takes
    /// the synthesized-helper route. Pure over the layout registry — the
    /// classify counter consults the same walk via `variant_layout_recursive`.
    pub(crate) fn variant_needs_eq_helper(&self, tyname: &str) -> bool {
        variant_layout_recursive(&self.variant_layouts, tyname)
    }

    fn eq_helper_name(&self, tyname: &str) -> String {
        format!("__eq_{}__{}", sanitize_ty_ident(tyname), sanitize_ty_ident(&self.fn_name))
    }

    fn list_eq_helper_name(&self, tyname: &str) -> String {
        format!("__eq_list_{}__{}", sanitize_ty_ident(tyname), sanitize_ty_ident(&self.fn_name))
    }

    /// Emit the Bool-returning helper call at an eq SITE (both operands are
    /// heap slot values — i64 addresses or i32 loaded handles; the wasm render
    /// wraps address-repr'd Handle args).
    pub(crate) fn emit_eq_helper_call(&mut self, name: String, lv: ValueId, rv: ValueId) -> ValueId {
        let dst = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name,
            args: vec![crate::CallArg::Handle(lv), crate::CallArg::Handle(rv)],
            result: Some(crate::Repr::Scalar { width: crate::ScalarWidth::Double }),
        });
        dst
    }

    /// Generate (once per parent fn) the tag-dispatch eq helper for variant
    /// `tyname`. Returns `false` when a payload is outside the engine (the eq
    /// site then walls — never wrong bytes). Recursion terminates because the
    /// type is put IN `synth_eq_types` before its body lowers: a self-typed
    /// field inside emits the helper CALL instead of regenerating.
    pub(crate) fn ensure_variant_eq_helper(
        &mut self,
        tyname: &str,
        layout: &crate::lower::VariantLayout,
    ) -> bool {
        let name = self.eq_helper_name(tyname);
        if self.synth_eq_fns.iter().any(|f| f.name == name)
            || self.synth_eq_types.contains(tyname)
        {
            return true;
        }
        self.synth_eq_types.insert(tyname.to_string());
        let mut sub = LowerCtx {
            variant_layouts: self.variant_layouts.clone(),
            record_layouts: self.record_layouts.clone(),
            fn_name: self.fn_name.clone(),
            next_value: 2,
            synth_eq_types: self.synth_eq_types.clone(),
            ..Default::default()
        };
        sub.param_values.insert(ValueId(0));
        sub.param_values.insert(ValueId(1));
        let ha = sub.handle_of(ValueId(0));
        let hb = sub.handle_of(ValueId(1));
        let res = sub.variant_eq_from_handles(ha, hb, layout, 0);
        // Absorb the nested helpers/types the sub generated (a field's List[T]
        // helper, a mutually-recursive variant) whichever way this body went.
        for f in std::mem::take(&mut sub.synth_eq_fns) {
            if !self.synth_eq_fns.iter().any(|g| g.name == f.name) {
                self.synth_eq_fns.push(f);
            }
        }
        for t in sub.synth_eq_types {
            self.synth_eq_types.insert(t);
        }
        match res {
            Some(r) => {
                let ptr = crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT };
                self.synth_eq_fns.push(MirFunction {
                    name,
                    params: vec![
                        MirParam { value: ValueId(0), repr: ptr },
                        MirParam { value: ValueId(1), repr: ptr },
                    ],
                    ops: sub.ops,
                    ret: Some(r),
                    declared_caps: Vec::new(),
                    heap_slot_masks: Default::default(),
                });
                true
            }
            None => {
                self.synth_eq_types.remove(tyname);
                false
            }
        }
    }

    /// Generate (once per parent fn) the `List[T]` eq helper for a variant
    /// element type: branchless `res = (la == lb); n = la * res; for i < n {
    /// res &= __eq_T(a[i], b[i]) }` — a length mismatch runs zero iterations
    /// and returns the false length-eq; matching lengths fold every element
    /// (eq is pure, so the no-short-circuit fold is observation-equal).
    pub(crate) fn ensure_list_eq_helper(
        &mut self,
        elem_tyname: &str,
        elem_layout: &crate::lower::VariantLayout,
    ) -> bool {
        let name = self.list_eq_helper_name(elem_tyname);
        if self.synth_eq_fns.iter().any(|f| f.name == name) {
            return true;
        }
        if !self.ensure_variant_eq_helper(elem_tyname, elem_layout) {
            return false;
        }
        let elem_call = self.eq_helper_name(elem_tyname);
        let mut sub = LowerCtx { next_value: 2, ..Default::default() };
        let ha = sub.handle_of(ValueId(0));
        let hb = sub.handle_of(ValueId(1));
        let la = sub.load_at_offset(ha, 4, crate::PrimKind::Load { width: 4 });
        let lb = sub.load_at_offset(hb, 4, crate::PrimKind::Load { width: 4 });
        // res (loop-carried accumulator local) seeded with the length eq.
        let res = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: res, op: crate::IntOp::Eq, a: la, b: lb });
        // n = la * res — 0 iterations on a length mismatch (res = 0).
        let n = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: n, op: crate::IntOp::Mul, a: la, b: res });
        let i = sub.fresh_value();
        sub.ops.push(Op::ConstInt { dst: i, value: 0 });
        sub.ops.push(Op::LoopStart);
        let cond = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: cond, op: crate::IntOp::Lt, a: i, b: n });
        sub.ops.push(Op::LoopBreakUnless { cond });
        let c8 = sub.fresh_value();
        sub.ops.push(Op::ConstInt { dst: c8, value: 8 });
        let c12 = sub.fresh_value();
        sub.ops.push(Op::ConstInt { dst: c12, value: 12 });
        let off = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: off, op: crate::IntOp::Mul, a: i, b: c8 });
        let base_a = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: base_a, op: crate::IntOp::Add, a: ha, b: c12 });
        let addr_a = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: addr_a, op: crate::IntOp::Add, a: base_a, b: off });
        let ea = sub.fresh_value();
        sub.ops.push(Op::Prim {
            kind: crate::PrimKind::LoadHandle,
            dst: Some(ea),
            args: vec![addr_a],
        });
        let base_b = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: base_b, op: crate::IntOp::Add, a: hb, b: c12 });
        let addr_b = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: addr_b, op: crate::IntOp::Add, a: base_b, b: off });
        let eb = sub.fresh_value();
        sub.ops.push(Op::Prim {
            kind: crate::PrimKind::LoadHandle,
            dst: Some(eb),
            args: vec![addr_b],
        });
        let e = sub.emit_eq_helper_call(elem_call, ea, eb);
        let and = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: and, op: crate::IntOp::And, a: res, b: e });
        sub.ops.push(Op::SetLocal { local: res, src: and });
        let one = sub.fresh_value();
        sub.ops.push(Op::ConstInt { dst: one, value: 1 });
        let i2 = sub.fresh_value();
        sub.ops.push(Op::IntBinOp { dst: i2, op: crate::IntOp::Add, a: i, b: one });
        sub.ops.push(Op::SetLocal { local: i, src: i2 });
        sub.ops.push(Op::LoopEnd);
        let ptr = crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT };
        self.synth_eq_fns.push(MirFunction {
            name,
            params: vec![
                MirParam { value: ValueId(0), repr: ptr },
                MirParam { value: ValueId(1), repr: ptr },
            ],
            ops: sub.ops,
            ret: Some(res),
            declared_caps: Vec::new(),
            heap_slot_masks: Default::default(),
        });
        true
    }
}

/// Identifier-safe spelling of a type/fn name (`varlib.Pigment` → `varlib_Pigment`,
/// test names carry spaces/1+1 — flattened the same way).
fn sanitize_ty_ident(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// A GENERIC variant's per-instantiation (key, layout): substitute the layout
/// generics with the use-site args (`Tree[Int]` → `Leaf(Int) | Node(Tree[Int],
/// Tree[Int])`) and suffix the helper key with the args. A non-generic layout
/// passes through under its bare name. The classify counter runs the SAME
/// substitution (`eq_helper_call_count` takes the full `Ty`).
pub(crate) fn instantiate_variant_layout(
    tyname: &str,
    layout: &crate::lower::VariantLayout,
    ty: &Ty,
) -> (String, crate::lower::VariantLayout) {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let args: &[Ty] = match ty {
        Ty::Named(_, a) => a,
        Ty::Applied(TC::UserDefined(_), a) => a,
        _ => &[],
    };
    if layout.generics.is_empty() || args.is_empty() {
        return (tyname.to_string(), layout.clone());
    }
    let mut subst: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
        std::collections::HashMap::new();
    for (g, a) in layout.generics.iter().zip(args.iter()) {
        subst.insert(*g, a.clone());
    }
    let mut inst = layout.clone();
    for case in &mut inst.cases {
        for (_, fty) in &mut case.fields {
            *fty = calls::subst_type_var(fty, &subst);
        }
    }
    let key = format!(
        "{}_{}",
        tyname,
        args.iter().map(|a| sanitize_ty_ident(&format!("{a:?}"))).collect::<Vec<_>>().join("_")
    );
    (key, inst)
}

/// Does `tyname` reach itself through eq-descended payload positions? (The
/// classify counter consults this exact walk.)
pub fn variant_layout_recursive(layouts: &crate::lower::VariantLayouts, tyname: &str) -> bool {
    fn ty_mentions(
        layouts: &crate::lower::VariantLayouts,
        ty: &Ty,
        target: &str,
        visited: &mut std::collections::HashSet<String>,
    ) -> bool {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let named = match ty {
            Ty::Named(n, _) => Some(n.as_str().to_string()),
            Ty::Variant { name, .. } => Some(name.as_str().to_string()),
            Ty::Applied(TC::UserDefined(n), _) => Some(n.clone()),
            _ => None,
        };
        if let Some(n) = named {
            if n == target {
                return true;
            }
            if let Some(l) = layouts.by_type.get(&n) {
                if visited.insert(n) {
                    return l.cases.iter().flat_map(|c| c.fields.iter()).any(|(_, fty)| {
                        ty_mentions(layouts, fty, target, visited)
                    });
                }
            }
            return false;
        }
        match ty {
            Ty::Applied(TC::List | TC::Option | TC::Result | TC::Set, args) => {
                args.iter().any(|a| ty_mentions(layouts, a, target, visited))
            }
            Ty::Tuple(elems) => elems.iter().any(|a| ty_mentions(layouts, a, target, visited)),
            _ => false,
        }
    }
    let Some(layout) = layouts.by_type.get(tyname) else { return false };
    let mut visited = std::collections::HashSet::new();
    visited.insert(tyname.to_string());
    layout
        .cases
        .iter()
        .flat_map(|c| c.fields.iter())
        .any(|(_, fty)| ty_mentions(layouts, fty, tyname, &mut visited))
}

/// The classify-counter mirror: the STATIC CallFn count a synthesized-eq SITE
/// contributes — 1 (the site's helper call) + every helper body generated for
/// the transitive type set, each counted once. Must match the generator above
/// op-for-op; a drift shows up as a `mir > ir` breach in the corpus gate.
pub fn eq_helper_call_count(layouts: &crate::lower::VariantLayouts, ty: &Ty) -> usize {
    // Collect the transitive helper set — variants reached through
    // helper-routed positions (self-typed fields, List[variant] elements) —
    // over the INSTANTIATED layouts (a generic payload's field types are
    // substituted exactly as the generator does), plus which instantiations
    // need the list helper.
    let mut variant_set: Vec<String> = Vec::new();
    let mut list_set: Vec<String> = Vec::new();
    let mut queue: Vec<Ty> = vec![ty.clone()];
    let mut total = 1; // the eq site's own helper call
    while let Some(t) = queue.pop() {
        let Some(n) = named_variant(layouts, &t) else { continue };
        let Some(layout) = layouts.by_type.get(&n).cloned() else { continue };
        let (key, inst) = instantiate_variant_layout(&n, &layout, &t);
        if variant_set.contains(&key) {
            continue;
        }
        variant_set.push(key);
        for (_, fty) in inst.cases.iter().flat_map(|c| c.fields.iter()) {
            total += field_eq_call_count(layouts, fty);
            collect_helper_types(layouts, fty, &mut queue, &mut list_set);
        }
    }
    total += list_set.len(); // each list helper's ONE element call
    total
}

fn collect_helper_types(
    layouts: &crate::lower::VariantLayouts,
    fty: &Ty,
    queue: &mut Vec<Ty>,
    list_set: &mut Vec<String>,
) {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    if named_variant(layouts, fty).is_some() {
        queue.push(fty.clone());
        return;
    }
    if let Ty::Applied(TC::List, es) = fty {
        if es.len() == 1 {
            if let Some(n) = named_variant(layouts, &es[0]) {
                let layout = layouts.by_type.get(&n).cloned();
                if let Some(layout) = layout {
                    let (key, _) = instantiate_variant_layout(&n, &layout, &es[0]);
                    if !list_set.contains(&key) {
                        list_set.push(key);
                    }
                }
                queue.push(es[0].clone());
            }
        }
    }
}

fn named_variant(layouts: &crate::lower::VariantLayouts, ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let n = match ty {
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Variant { name, .. } => name.as_str().to_string(),
        Ty::Applied(TC::UserDefined(n), _) => n.clone(),
        _ => return None,
    };
    layouts.by_type.contains_key(&n).then_some(n)
}

/// The CallFn count `typed_slot_eq` emits for ONE field compare inside a
/// generated helper body — mirrors the engine's field arms exactly.
fn field_eq_call_count(layouts: &crate::lower::VariantLayouts, fty: &Ty) -> usize {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    if matches!(fty, Ty::String) || crate::lower::is_value_ty(fty) {
        return 1;
    }
    if named_variant(layouts, fty).is_some() {
        return 1; // the helper call (self or mutual — both routed by the in-progress set)
    }
    if let Ty::Applied(TC::List, es) = fty {
        if es.len() == 1 {
            if named_variant(layouts, &es[0]).is_some() {
                return 1; // the list-helper call
            }
            // the module-eq table (list.eq_int / eq_str / … / eq_opt_int / nested)
            let inner_mod_eq = match &es[0] {
                Ty::Int | Ty::String | Ty::Float | Ty::Bool => true,
                t if crate::lower::is_value_ty(t) => true,
                Ty::Applied(TC::List, i2) => {
                    i2.len() == 1 && matches!(i2[0], Ty::Int | Ty::Float | Ty::String)
                }
                Ty::Applied(TC::Option, i2) => {
                    i2.len() == 1 && matches!(i2[0], Ty::Int | Ty::Bool)
                }
                _ => false,
            };
            return usize::from(inner_mod_eq);
        }
        return 0;
    }
    if let Ty::Applied(TC::Option, oa) = fty {
        if oa.len() == 1 {
            return if crate::lower::is_heap_ty(&oa[0]) {
                field_eq_call_count(layouts, &oa[0])
            } else {
                0
            };
        }
    }
    if let Ty::Applied(TC::Result, ra) = fty {
        if ra.len() == 2 {
            return if !crate::lower::is_heap_ty(&ra[0]) && matches!(ra[1], Ty::String) {
                1
            } else {
                field_eq_call_count(layouts, &ra[0]) + field_eq_call_count(layouts, &ra[1])
            };
        }
    }
    if let Ty::Tuple(elems) = fty {
        return elems.iter().map(|t| field_eq_call_count(layouts, t)).sum();
    }
    0
}
