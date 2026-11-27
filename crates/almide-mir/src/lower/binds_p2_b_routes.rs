// ── part of binds_p2_b.rs, include!-spliced (split for the 800-line budget) ──

impl LowerCtx {
    /// Extracted from `Self::seed_call_named_heap_drop_route` (third-round split, cog
    /// reduction): the second half of the mutually-exclusive `if/else if` chain, verbatim
    /// (only reached when the first half's chain did not match).
    fn seed_call_named_heap_drop_route_b(&mut self, dst: ValueId, ty: &Ty) {
        // Guard-clause flattening (codopsy7 max-depth sweep, same rationale as `_a` above):
        // independent `if COND { ...; return; }` guards in the SAME order as the original
        // `if/else if` chain — first-match-wins semantics preserved exactly.
        if crate::lower::is_map_mlo_ty(ty) {
            // `Map[String, List[Option[Int]]]` — `$__drop_map_mlo` sweeps each
            // last-ref value list's Option slots (a flat rc_dec would leak them).
            self.value_drops.entry(dst).or_default().named_route = Some("map_mlo".to_string());
            return;
        }
        if let Some(rname) = match ty {
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 =>
            {
                self.record_or_anon_drop_type_name(&a[0])
            }
            _ => None,
        }  {
            // A `List[<recursive-drop record>]` result (`list.unique` over a
            // String-field record via the `__krec_*` twins): route to the generated
            // `$__drop_list_<R>` (emitted for EVERY recursive-drop record) — the
            // flat per-slot dec freed each element block but LEAKED its String
            // fields (the krec-unique residue).
            self.value_drops.entry(dst).or_default().named_route = Some(format!("list_{rname}"));
            return;
        }
        if crate::lower::is_lenlist_list_ty(ty) {
            // `List[Result[_, String]]`/`List[Option[String]]` — the len-loop drop; the
            // flat DropListStr would leak each element's owned payload slots.
            self.value_drops.entry(dst).or_default().named_route = Some("list_lenlist".to_string());
            return;
        }
        if crate::lower::is_opt_list_str_ty(ty) {
            // `Option[List[String]]` (the heap-acc fold value) — physically a 0/1-element
            // List[List[String]]; the nested DropListListStr sweep is its exact free (the
            // flat DropListStr would leak the stack Strings).
            self.value_drops.entry(dst).or_default().list_list_str = true;
            return;
        }
        if matches!(ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]))
        {
            // `Map[String, <scalar>]` (split layout, @4 = n): the DropListStr sweep
            // rc_decs exactly the n deep-copied key Strings (scalar value slots
            // untouched) — the bare flat rc_dec LEAKED every key copy per bind (a
            // latent leak the map.fold heap-acc loop made observable at a 4MB cap).
            self.value_drops.entry(dst).or_default().flat_elems = true;
            return;
        }
        if is_heap_elem_list_ty(ty) {
            self.value_drops.entry(dst).or_default().flat_elems = true;
            return;
        }
        if is_scalar_elem_list_ty(ty) {
            // A user fn returning `List[scalar]` yields a REAL, POPULATED list
            // block (the v1 calling convention — the same argument as the
            // variant/record seeds below; a callee that cannot build one WALLS,
            // and the render rejects the program). Admit a direct `xs[i]`
            // bounds-checked load over the bound result — the C-132 move-mode
            // write-back binds the returned buffer exactly here (`__mp_buf =
            // add_item(data, 1)` then `data = __mp_buf; data[0]`).
            self.materialized_lists.insert(dst);
        }
    }

    /// Extracted from `Self::lower_bind_heap_call_named` (second-round split, cog
    /// reduction): the independent (non-mutually-exclusive) read-shape tracking checks
    /// for a Named-call's fresh heap result, verbatim.
    pub(crate) fn seed_call_named_heap_read_shape(&mut self, dst: ValueId, ty: &Ty) {
        // A user fn returning `List[heap]` (`build_nested() -> List[List[Int]]`) is
        // likewise a REAL, POPULATED nested-ownership block, so admit the element
        // borrow `nested[i]` (LoadHandle at `$elem_addr`) over the bound var — the
        // exact mirror of the Module-call bind's `string.split` registration. Without
        // it, `nested[10]` fell to the container-grain Dup of the WHOLE list and a
        // consumer read the outer block (`list.len(mid)` = the OUTER len — the
        // rc_alloc_stress silent miscompile). Narrowed to `List[heap]` (NOT the
        // broader Option/Result/Map `is_heap_elem_list_ty` also matches) — only a
        // real list is `[i]`-indexable here.
        if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]))
        {
            self.materialized_lists.insert(dst);
        }
        // A `Value` result from a user fn (`let v = parse_number(c, raw)`) drops via the
        // runtime-tag-dispatched `DropValue` — the SAME marking the Module-call bind path does
        // (was missing here, so a let-bound Named-call Value leaked: a parse loop OOMs).
        if crate::lower::is_value_ty(ty) {
            self.value_handles.insert(dst);
        }
        // A user fn RETURNING a function value (`let f = mk()` / `let f = adder(3)`)
        // yields a CLOSURE BLOCK — a fresh owned heap value (already in the scope-end
        // set like any heap result): track it so a later `f(args)` dispatches through
        // `Op::CallIndirect` via `emit_closure_call`.
        if matches!(ty, Ty::Fn { .. }) {
            self.closure_values.insert(dst);
        }
        // A user function returning Option/Result yields a REAL same-layout variant block
        // (the v1 calling convention — `seed_variant_param`'s contract). SEED its READ-shape
        // so a later `match x { … }` / `x ?? d` over the LET-BOUND var EXECUTES (reads the
        // tag) exactly as the direct-call-arg position already does (`lower_call_args`'s
        // Named arm). Adds ONLY layout knowledge — `dst` is already an owned heap value
        // dropped at scope end, so no ownership/cert change. This is what made
        // `let parsed = parse_oct(d); match parsed { … }` (num_signed_base, after the
        // let-bound-heap-`if` tail-duplication) lower instead of wall.
        if is_variant_ty(ty) {
            self.seed_variant_param(dst, ty);
        } else if let Some((_, tys)) = self.aggregate_field_tys(ty) {
            // A user function returning a RECORD/TUPLE yields a REAL same-layout block (the
            // callee built it via try_lower_record_construct). Seed its READ-shape
            // (materialized_aggregates) so a field read `p.y` loads the real slot instead of
            // falling back to the container-grain Dup (which returns the whole record — the
            // `mk(5).y` empty-string miscompile), AND its heap-slot MASK (record_masks) so the
            // OWNED scope-end drop frees exactly the heap fields (no leak, no double-free).
            let heap_slots: Vec<usize> =
                (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
            self.materialized_aggregates.insert(dst);
            self.record_masks.insert(dst, heap_slots);
            // A record with a Map/List[heap]/record/Value field drops RECURSIVELY ($__drop_<R>),
            // not the flat masked DropListStr (which would leak the nested heap) — route it. An
            // ANONYMOUS record return whose flat one-level mask would leak a nested heap field
            // (`{ data: Bytes, state: Cfb8State }` — aes cfb8) routes to its synthesized
            // `__drop_anonrec_<hash>` (so `state` is freed through `__drop_Cfb8State`).
            if let Some(name) = self.record_or_anon_drop_type_name(ty) {
                self.value_drops.entry(dst).or_default().named_route = Some(name);
            }
        }
    }
}
