/// Generate the ALMIDE SOURCE for each variant type's recursive drop fn `__drop_<T>` (ADT brick
/// 5b) — the `$__drop_value` shape: at the last ref (rc==1) read the tag, recursively
/// `__drop_<V>` each nested-variant field + `prim.rc_dec` each leaf `String` field, then release
/// the block. Returns the concatenated source to APPEND to the program (so the `type` decls it
/// references are in scope); only types that `variant_needs_recursive_drop` get a fn. The fn is
/// `prim`-only ⇒ empty ownership cert (a trusted routine — its leak/double-free correctness is
/// the create+drop LEAK LOOP's burden, exactly like `__drop_value`). The slot offsets match the
/// v1 construct (`[rc@0][len@4][cap@8][tag=slot0@12][field i @ 12+(1+i)*8]`).
pub fn generate_variant_drop_sources(type_decls: &[almide_ir::IrTypeDecl]) -> String {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let names = variant_type_names(type_decls);
    // A variant FIELD that is itself a FLAT variant (e.g. `BlockType.BlockVal(ValType)`) is a single
    // owned tag-block with no inner handle: it must be freed by a flat `rc_dec`, NOT a recursive
    // `__drop_<flatvariant>` (which is never generated for a flat variant — it has no heap field — and
    // would render a DANGLING call). Mirrors the record-drop generator's `is_flat_variant_elem` treatment.
    let flat_names = flat_variant_type_names(type_decls);
    // The RICH (recursive-drop) variant type names — those for which `$__drop_<V>` is generated below.
    // A `List[<rich variant>]` ctor field (the wasm `Instr.Block(BlockType, List[Instr])` shape) is
    // freed RECURSIVELY via `$__drop_list_<V>` (each element → `$__drop_<V>`, mutually recursive); a
    // flat one-level `rc_dec` of the list block would leak every element's nested children.
    // A ctor field that is itself a RECORD: freed via `$__drop_<R>` (recursive-drop record — a
    // nested String/heap field) or a flat `rc_dec` (scalar-only record). `all_record_names` gates the
    // detection + the `needs_recursive_drop` widening, `rec_record_names` selects the free.
    let all_record_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| matches!(&d.kind, IrTypeDeclKind::Record { .. }))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let rec_record_names = recursive_record_drop_names(type_decls);
    let rec_variant_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| variant_needs_recursive_drop(d, &names, &all_record_names))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let mut out = String::new();
    for decl in type_decls {
        if !variant_needs_recursive_drop(decl, &names, &all_record_names) {
            continue;
        }
        let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else { continue };
        let tname = decl.name.as_str();
        // The fn NAME sanitizes the module prefix (`types.RunResult` → `types_RunResult`); the param
        // TYPE annotation keeps the dotted module-qualified name (a valid Almide type reference).
        let fname = drop_fn_ident(tname);
        out.push_str(&format!("fn __drop_{fname}(e: {tname}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        out.push_str(&format!("    let t = prim.load64(h + {})\n", layout::slot_offset(0)));
        // One tag branch per ctor that has a heap field; chained `if t == k then {..} else ..`.
        let mut branch = String::new();
        let mut first = true;
        for (tag, case) in cases.iter().enumerate() {
            let tys: Vec<Ty> = match &case.kind {
                IrVariantKind::Unit => vec![],
                IrVariantKind::Tuple { fields } => fields.clone(),
                IrVariantKind::Record { fields } => fields.iter().map(|f| f.ty.clone()).collect(),
            };
            // Per-field free statements (variant → recurse, String → rc_dec, scalar → skip).
            let mut frees = String::new();
            let mut idx = 0usize;
            for (i, ty) in tys.iter().enumerate() {
                let off = layout::slot_offset(1 + i);
                if let Some(fv) = variant_field_name(ty, &names) {
                    if flat_names.contains(&fv) {
                        // A flat-variant field — a single owned block, freed by one `rc_dec` (no
                        // recursive `__drop_<fv>` exists for a flat variant). No `let` binding needed.
                        frees.push_str(&format!(
                            "        prim.rc_dec(prim.load64(h + {off}))\n"
                        ));
                    } else {
                        let fv_fn = drop_fn_ident(&fv);
                        frees.push_str(&format!(
                            "        let f{idx}: {fv} = prim.load_handle(h + {off})\n        __drop_{fv_fn}(f{idx})\n"
                        ));
                        idx += 1;
                    }
                    continue;
                }
                if matches!(ty, Ty::String) {
                    frees.push_str(&format!(
                        "        prim.rc_dec(prim.load64(h + {off}))\n"
                    ));
                    continue;
                }
                if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 && !is_heap_ty(&a[0]))
                {
                    // A List[scalar] ctor field — a FLAT block, one rc_dec is its full free.
                    frees.push_str(&format!(
                        "        prim.rc_dec(prim.load64(h + {off}))
"
                    ));
                    continue;
                }
                if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 && matches!(a[0], Ty::String))
                {
                    // A `List[String]` ctor field (`Node(String, List[String])`): each element is
                    // an OWNED String handle — the generic `__drop_list_str` (shared with the
                    // record-drop generator via `LIST_STR_DROP_SRC`, gated once at the pipeline
                    // top level so both generators' identical references never double-define it)
                    // frees every element then the list block. A flat `rc_dec` of just the list
                    // block would leak each String.
                    frees.push_str(&format!(
                        "        let f{idx}: List[String] = prim.load_handle(h + {off})\n        __drop_list_str(f{idx})\n"
                    ));
                    idx += 1;
                    continue;
                }
                if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 && is_flat_variant_elem(&a[0], &flat_names))
                {
                    // A `List[<flat variant>]` ctor field (`Wrapped(List[Policy])` — #484): each
                    // element is a single owned FLAT block (no inner handles), so `__drop_list_str`'s
                    // per-element `rc_dec` sweep is its exact free — the record-drop generator's
                    // List[flat-variant] precedent mirrored (incl. its `List[String]` binding type,
                    // the handle-level reinterpretation that precedent already uses).
                    frees.push_str(&format!(
                        "        let f{idx}: List[String] = prim.load_handle(h + {off})\n        __drop_list_str(f{idx})\n"
                    ));
                    idx += 1;
                    continue;
                }
                if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a)
                    if a.len() == 1 && !is_heap_ty(&a[0]))
                {
                    // An Option[scalar] ctor field (`Box(Option[Int])`) — the 0-or-1-element
                    // len-tag block owns NO children (a Some payload is a scalar slot), so one
                    // rc_dec is its full free. Mirrored in BOTH `needs_recursive_drop` gates and
                    // `try_lower_variant_ctor`'s field admission — construction and drop agree.
                    frees.push_str(&format!(
                        "        prim.rc_dec(prim.load64(h + {off}))
"
                    ));
                    continue;
                }
                if matches!(ty, Ty::Fn { .. }) {
                    // A CLOSURE ctor field (`Run(() -> Unit)` — the variant-stored closure
                    // class): the slot holds a self-describing closure block whose captured
                    // heap env a flat rc_dec would LEAK — free it via `__drop_closure`, the
                    // SAME routine the record-drop generator's Fn arm uses (CLOSURE_DROP_SRC
                    // is linked whenever the program creates closures, which a populated Fn
                    // payload requires). The binding type is the block's List[Int] rep.
                    frees.push_str(&format!(
                        "        let f{idx}: List[Int] = prim.load_handle(h + {off})\n        __drop_closure(f{idx})\n"
                    ));
                    idx += 1;
                    continue;
                }
                if let Some(ev) = list_rich_variant_elem(ty, &rec_variant_names) {
                    // A `List[<rich variant>]` ctor field (`Block(_, List[Instr])`): each element is a
                    // recursive-drop variant block, freed per-element by the generated `$__drop_list_<ev>`
                    // (→ `$__drop_<ev>`). A flat `rc_dec` of the list block would leak every element.
                    let ev_fn = drop_fn_ident(&ev);
                    frees.push_str(&format!(
                        "        let f{idx}: List[{ev}] = prim.load_handle(h + {off})\n        __drop_list_{ev_fn}(f{idx})\n"
                    ));
                    idx += 1;
                    continue;
                }
                let Ty::Named(rn, _) = ty else {
                    continue;
                };
                if !all_record_names.contains(rn.as_str()) {
                    continue;
                }
                // A RECORD-type ctor field (`Wrap(Color)` / `Box(Inner)`). A recursive-drop
                // record (a String / nested-heap field) recurses via `$__drop_<R>`; a
                // scalar-only record block is a single owned allocation, one `rc_dec` its full
                // free. Either way the ctor stored its HANDLE at this slot.
                if rec_record_names.contains(rn.as_str()) {
                    let rn_fn = drop_fn_ident(rn.as_str());
                    let rn_s = rn.as_str();
                    frees.push_str(&format!(
                        "        let f{idx}: {rn_s} = prim.load_handle(h + {off})\n        __drop_{rn_fn}(f{idx})\n"
                    ));
                    idx += 1;
                } else {
                    frees.push_str(&format!(
                        "        prim.rc_dec(prim.load64(h + {off}))\n"
                    ));
                }
            }
            if frees.is_empty() {
                continue; // scalar/Unit ctor — nothing to free
            }
            let kw = if first { "if" } else { "else if" };
            branch.push_str(&format!("    {kw} t == {tag} then {{\n{frees}      }}\n"));
            first = false;
        }
        if branch.is_empty() {
            // No heap-field ctor (shouldn't happen — needs_recursive_drop was true), guard anyway.
            out.push_str("    ()\n");
        } else {
            out.push_str(&branch);
            out.push_str("    else ()\n");
        }
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
    // A per-element-recursive `$__drop_list_<V>` for EVERY rich variant V — so a `List[V]` value (the
    // wasm `read_instrs` accumulator) AND a `List[V]` FIELD of a record (`Global.init`, freed via
    // `record_drop_field_frees` → `__drop_list_<V>`) reclaim each element through `$__drop_<V>`. Mirrors
    // the record list-drop loop in `generate_record_drop_sources` (the variant is the element drop). The
    // recursion `$__drop_<V> ↔ $__drop_list_<V>` terminates on a finite (parsed) tree; both are trusted
    // prim-only routines (empty cert), verified by the create+drop leak loop. Sorted for host-determinism.
    let mut list_drop_names: Vec<&String> = rec_variant_names.iter().collect();
    list_drop_names.sort();
    for vn in list_drop_names {
        let vn_fn = drop_fn_ident(vn);
        out.push_str(&format!(
            "fn __drop_list_{vn_fn}(xs: List[{vn}]) -> Unit = {{\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_{vn_fn}_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}}\n\
             fn __drop_list_{vn_fn}_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else {{ let e: {vn} = prim.load_handle(h + 12 + i * 8)\n         __drop_{vn_fn}(e)\n         __drop_list_{vn_fn}_loop(h, n, i + 1) }}\n"
        ));
        // The LEN-AS-TAG Result wrapper holding a rich-variant ERR payload
        // (`Result[T_scalar, V]` — the structured-error class `err(Overflow(msg))`):
        // at the wrapper's last ref, an Err (len == 1) recurses into the slot-0
        // variant via `$__drop_<V>`, then the block frees. An Ok (len == 0) has no
        // payload. Same trusted prim-only class as every generated drop.
        out.push_str(&format!(
            "fn __drop_res_{vn_fn}(e: List[Int]) -> Unit = {{\n  \
               let h = prim.handle(e)\n  \
               if prim.load32(h + 0) == 1 and prim.load32(h + 4) == 1 then {{\n    \
                 let q: {vn} = prim.load_handle(h + 12)\n    __drop_{vn_fn}(q)\n  }} else ()\n  \
               prim.rc_dec(h)\n}}\n"
        ));
        // `List[(String, <rich variant V>)]` — a TUPLE-wrapped rich-variant Err/value
        // payload (`generic_chain_unwrap_or`'s `List[(String, V)]` metadata pairs,
        // `type V = ValInt(Int) | ValStr(String)`): each list element is its OWN
        // separately-refcounted tuple block (`[rc][len][cap][String@12][V-handle@20]`,
        // the SAME layout `DropListStrInt` walks) — but unlike `DropListStrInt` (whose
        // render never reads slot1, sound only when slot1 is scalar), slot1 here is a
        // RICH variant that owns further heap (a `ValStr` payload's String) and must
        // recurse via the variant's own `$__drop_<V>` — a flat `rc_dec` of the tuple
        // block alone would LEAK every `ValStr` element's String. Mirrors the
        // per-element-then-per-slot walk `__drop_list_map_hval` (map_hval.almd) uses
        // for a Map-valued list element, specialized to a 2-slot tuple's own rc check.
        out.push_str(&format!(
            "fn __drop_list_str_{vn_fn}(xs: List[(String, {vn})]) -> Unit = {{\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_str_{vn_fn}_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}}\n\
             fn __drop_list_str_{vn_fn}_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else {{\n    \
                 let th = prim.load64(h + 12 + i * 8)\n    \
                 if prim.load32(th + 0) == 1 then {{\n      \
                   prim.rc_dec(prim.load64(th + 12))\n      \
                   let v: {vn} = prim.load_handle(th + 20)\n      \
                   __drop_{vn_fn}(v)\n    \
                 }} else ()\n    \
                 prim.rc_dec(th)\n    \
                 __drop_list_str_{vn_fn}_loop(h, n, i + 1)\n  \
               }}\n"
        ));
        // (moved below — the map-value sweep now covers ALL variants, split layout)
    }
    // A `Map[String, <variant>]` value (`["a": Circle(3.0)]` — the shape_map class), the
    // map_hobj SPLIT layout (@4 = n entries; keys 0..n-1, values n..2n-1): the exact free
    // is `rc_dec` of each deep-copied key + the value free — RECURSIVE `$__drop_<V>` for a
    // recursive-drop variant, a flat `rc_dec` for a flat one (its block owns no children).
    // Generated for EVERY variant so the bind-side `map_<V>` admission never outruns
    // generation. Records with all-scalar fields get the same sweep with a flat value
    // `rc_dec` (`__drop_map_rec_<R>`).
    {
        let mut all_variant_names: Vec<&str> = type_decls
            .iter()
            .filter(|d| matches!(&d.kind, IrTypeDeclKind::Variant { .. }))
            .map(|d| d.name.as_str())
            .collect();
        all_variant_names.sort_unstable();
        for vn in all_variant_names {
            let vn_fn = drop_fn_ident(vn);
            let free_v = if rec_variant_names.contains(vn) {
                format!("let v: {vn} = prim.load_handle(h + 12 + (n + i) * 8)\n    __drop_{vn_fn}(v)")
            } else {
                "prim.rc_dec(prim.load64(h + 12 + (n + i) * 8))".to_string()
            };
            out.push_str(&format!(
                "fn __drop_map_{vn_fn}_go(h: Int, n: Int, i: Int) -> Unit =\n  \
                   if i >= n then ()\n  \
                   else {{\n    \
                     prim.rc_dec(prim.load64(h + 12 + i * 8))\n    \
                     {free_v}\n    \
                     __drop_map_{vn_fn}_go(h, n, i + 1)\n  \
                   }}\n\
                 fn __drop_map_{vn_fn}(m: Map[String, {vn}]) -> Unit = {{\n  \
                   let h = prim.handle(m)\n  \
                   if prim.load32(h + 0) == 1 then __drop_map_{vn_fn}_go(h, prim.load32(h + 4), 0) else ()\n  \
                   prim.rc_dec(h)\n}}\n"
            ));
        }
        let mut scalar_recs: Vec<&str> = type_decls
            .iter()
            .filter_map(|d| match &d.kind {
                IrTypeDeclKind::Record { fields }
                    if fields.iter().all(|f| !is_heap_ty(&f.ty)) =>
                {
                    Some(d.name.as_str())
                }
                _ => None,
            })
            .collect();
        scalar_recs.sort_unstable();
        for rn in scalar_recs {
            let rn_fn = drop_fn_ident(rn);
            out.push_str(&format!(
                "fn __drop_map_rec_{rn_fn}_go(h: Int, n: Int, i: Int) -> Unit =\n  \
                   if i >= n then ()\n  \
                   else {{\n    \
                     prim.rc_dec(prim.load64(h + 12 + i * 8))\n    \
                     prim.rc_dec(prim.load64(h + 12 + (n + i) * 8))\n    \
                     __drop_map_rec_{rn_fn}_go(h, n, i + 1)\n  \
                   }}\n\
                 fn __drop_map_rec_{rn_fn}(m: Map[String, {rn}]) -> Unit = {{\n  \
                   let h = prim.handle(m)\n  \
                   if prim.load32(h + 0) == 1 then __drop_map_rec_{rn_fn}_go(h, prim.load32(h + 4), 0) else ()\n  \
                   prim.rc_dec(h)\n}}\n"
            ));
        }
    }
    out
}

/// Discover every `List[<generic user variant instantiated with concrete args>]` LITERAL
/// element type used anywhere in the program (`Either[Int,String]` in `compound_repr_recursive_
/// interp.almd`'s `List[Either[Int,String]] = [Left(1), Right("y")]`) — the set of instantiations
/// [`generate_generic_variant_instantiation_sources`] needs a shadow `type` + `$__drop_<inst>`/
/// `$__drop_list_<inst>` for. DELIBERATELY scoped to List-literal element position only — not a
/// general generic-instantiation scan — matching the actual corpus shape this closes; a bare
/// `Left(1): Either[Int,String]` construction outside a list, a function parameter, a record
/// field, … are untouched (their existing `try_lower_variant_ctor` construction path is ALREADY
/// correct for a leaf-heap-field-only instantiation via its masked flat-drop fallback — see the
/// `is_rich_variant_ty` doc comment; only the LIST case lacked a category at all). Deduped by
/// instantiation name (a `BTreeMap`, so the output order is a pure function of the program — no
/// HashMap-iteration-order divergence across native/wasm hosts).
// The `'discover` guard cascade's decision — a PURE function of `e` and
// `variant_layouts` (read-only); the scan's `found` map is an OUTPUT written once
// with the result, never read back mid-decision, so this is safe to pull out whole
// (same "no shared decision state" reasoning as the other visitor-body extractions
// in this crate). Top-level (not nested in `discover_generic_variant_list_
// instantiations`, codopsy cc) alongside its `Scan` visitor below.
fn discover_one_generic_variant_list_instantiation(
    e: &almide_ir::IrExpr,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Option<(String, String, Vec<Ty>)> {
    use almide_ir::IrExprKind;
    use almide_lang::types::constructor::TypeConstructorId;
    if !matches!(&e.kind, IrExprKind::List { .. }) {
        return None;
    }
    let Ty::Applied(TypeConstructorId::List, a) = &e.ty else { return None };
    if a.len() != 1 {
        return None;
    }
    let (name, args) = crate::lower::VariantLayouts::variant_name_and_args(&a[0])?;
    if args.is_empty() {
        return None;
    }
    let layout = variant_layouts.by_type.get(name)?;
    if layout.generics.is_empty() {
        return None;
    }
    let inst = crate::lower::generic_variant_instantiation_name(name, args)?;
    Some((inst, name.to_string(), args.to_vec()))
}

struct GenericVariantListInstantiationScan<'a> {
    variant_layouts: &'a crate::lower::VariantLayouts,
    found: std::collections::BTreeMap<String, (String, Vec<Ty>)>,
}
impl almide_ir::visit::IrVisitor for GenericVariantListInstantiationScan<'_> {
    fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
        if let Some((inst, name, args)) =
            discover_one_generic_variant_list_instantiation(e, self.variant_layouts)
        {
            self.found.entry(inst).or_insert_with(|| (name, args));
        }
        almide_ir::visit::walk_expr(self, e);
    }
}

/// Every top-level expression (fn bodies + top-let inits) in `ir`, main AND every
/// module, in the SAME order the original 4-loop nest visited them (main functions,
/// main top-lets, then per module: that module's functions, that module's
/// top-lets) — an iterator-chain rewrite of nested `for` loops (codopsy cog: nested
/// `for` costs more cognitive complexity per level than a flat `.chain()`/
/// `.flat_map()` pipeline, which this crate's Op-rendering code already prefers for
/// exactly that reason). A plain iteration helper, no decision logic.
fn for_each_program_expr(ir: &almide_ir::IrProgram, mut visit: impl FnMut(&almide_ir::IrExpr)) {
    let main = ir.functions.iter().map(|f| &f.body).chain(ir.top_lets.iter().map(|tl| &tl.value));
    let modules = ir.modules.iter().flat_map(|m| {
        m.functions.iter().map(|f| &f.body).chain(m.top_lets.iter().map(|tl| &tl.value))
    });
    main.chain(modules).for_each(visit);
}

pub fn discover_generic_variant_list_instantiations(
    ir: &almide_ir::IrProgram,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Vec<(String, String, Vec<Ty>)> {
    use almide_ir::visit::IrVisitor;
    let mut scan = GenericVariantListInstantiationScan { variant_layouts, found: Default::default() };
    for_each_program_expr(ir, |e| scan.visit_expr(e));
    scan.found.into_iter().map(|(inst, (base, args))| (base, inst, args)).collect()
}

/// A SHADOW `type <inst_name> = ...` declaration — one Rust `IrTypeDecl` (to splice into the
/// caller's OWN `type_decls` list, once, BEFORE its single `generate_variant_drop_sources` call —
/// so the caller's existing drop-function generation covers the shadow automatically, with no
/// separate/duplicate `generate_variant_drop_sources` invocation here) PLUS its Almide SOURCE
/// TEXT `type` line (the caller must prepend this to whatever it re-lowers, so the shadow name is
/// actually declared when the drop-function source referencing it gets type-checked) — one pair
/// per instantiation `discover_generic_variant_list_instantiations` found. The shadow type
/// mirrors the REAL generic variant's case/field SHAPE with type-parameters SUBSTITUTED to this
/// instantiation's concrete args, under UNIQUE synthetic ctor names (`__<inst_name>_c<tag>`) so
/// it never collides with the real type's own ctors (`Left`/`Right` stay registered to `Either`,
/// not to this shadow) — the runtime v1 variant repr (tag + uniform i64 slots) is driven purely
/// by TAG NUMBER and FIELD ORDER, never by ctor NAME, so a value built by the REAL
/// `Either[Int,String]` construction and one built against this shadow type are BYTE-IDENTICAL;
/// the shadow exists SOLELY so the two-pass re-lower (`pipeline.rs`'s
/// `source_to_ir_with(&format!("{source}\n{drops}"), ..)`) has a real, type-checkable name to
/// hang `$__drop_<inst_name>`'s parameter type on. An instantiation whose ctor field types (after
/// substitution) aren't in the supported-renderable set (`generic_variant_instantiation_scalar_
/// name` / an already-declared non-generic variant) is silently skipped — `is_rich_variant_ty`
/// (mod_p2.rs) gates on the SAME set before ever admitting an instantiation, so this should never
/// actually skip a name that reached here, but the check is re-verified rather than assumed.
// ONE case's field list, rendered to (subst'd tys, source-text names) or `None` if
// any field's substituted type isn't in the supported-renderable set. The
// innermost loop's `ok`/`break` flag is purely LOCAL to this fn (communicated
// back to the caller as `Option`, never shared state threaded across case
// iterations).
fn render_generic_variant_case_fields(
    fields: &[(almide_lang::intern::Sym, Ty)],
    subst: &std::collections::HashMap<almide_lang::intern::Sym, Ty>,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Option<(Vec<Ty>, Vec<String>)> {
    let mut field_tys: Vec<Ty> = Vec::with_capacity(fields.len());
    let mut field_src_parts: Vec<String> = Vec::with_capacity(fields.len());
    for (_, fty) in fields {
        let sub = substitute_generic_ty(fty, subst);
        let rendered = crate::lower::generic_variant_instantiation_scalar_name(&sub)
            .map(|s| s.to_string())
            .or_else(|| {
                // An already-declared NON-GENERIC user variant field — reference it by
                // its own real name (it needs no shadow, it isn't generic).
                variant_layouts.field_is_variant(&sub).then(|| match &sub {
                    Ty::Named(n, _) => n.as_str().to_string(),
                    _ => String::new(),
                })
            });
        let rendered = rendered.filter(|s| !s.is_empty())?;
        field_src_parts.push(rendered);
        field_tys.push(sub);
    }
    Some((field_tys, field_src_parts))
}

/// ONE instantiation's shadow `type` decl (source line + `IrTypeDecl`), or `None` if
/// the base variant is unknown or any case's fields don't render (the whole
/// instantiation is skipped then — the original's `ok = false` semantics, now just
/// "this fn returns `None`"). Extracted out of the per-instantiation loop below
/// (codopsy cc) — the same "outer loop calls a per-item helper" split used
/// throughout this crate.
fn generate_one_generic_variant_instantiation_type_decl(
    base: &str,
    inst_name: &str,
    args: &[Ty],
    variant_layouts: &crate::lower::VariantLayouts,
) -> Option<(String, almide_ir::IrTypeDecl)> {
    use almide_lang::intern::sym;
    let layout = variant_layouts.by_type.get(base)?;
    let subst: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
        layout.generics.iter().copied().zip(args.iter().cloned()).collect();
    let mut cases: Vec<almide_ir::IrVariantDecl> = Vec::with_capacity(layout.cases.len());
    let mut case_src_parts: Vec<String> = Vec::with_capacity(layout.cases.len());
    for c in &layout.cases {
        let (field_tys, field_src_parts) =
            render_generic_variant_case_fields(&c.fields, &subst, variant_layouts)?;
        // MUST start uppercase — the parser rejects a lowercase/underscore ctor name
        // ("Expected type name"), which silently killed the whole shadow `type` line
        // and cascaded to "unknown type '<inst>'" at every generated reference.
        let ctor_name = format!("C__{inst_name}_{}", c.tag);
        case_src_parts.push(if field_src_parts.is_empty() {
            ctor_name.clone()
        } else {
            format!("{ctor_name}({})", field_src_parts.join(", "))
        });
        cases.push(almide_ir::IrVariantDecl {
            name: sym(&ctor_name),
            kind: if field_tys.is_empty() {
                almide_ir::IrVariantKind::Unit
            } else {
                almide_ir::IrVariantKind::Tuple { fields: field_tys }
            },
        });
    }
    if cases.is_empty() {
        return None;
    }
    let src_line = format!("type {inst_name} = {}\n", case_src_parts.join(" | "));
    let decl = almide_ir::IrTypeDecl {
        name: sym(inst_name),
        kind: almide_ir::IrTypeDeclKind::Variant {
            cases,
            is_generic: false,
            boxed_args: Default::default(),
            boxed_record_fields: Default::default(),
        },
        deriving: None,
        generics: None,
        visibility: almide_ir::IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
    };
    Some((src_line, decl))
}

pub fn generate_generic_variant_instantiation_type_decls(
    instantiations: &[(String, String, Vec<Ty>)],
    variant_layouts: &crate::lower::VariantLayouts,
) -> (String, Vec<almide_ir::IrTypeDecl>) {
    let mut type_decl_src = String::new();
    let mut synthetic_decls: Vec<almide_ir::IrTypeDecl> = Vec::new();
    for (base, inst_name, args) in instantiations {
        if let Some((src_line, decl)) = generate_one_generic_variant_instantiation_type_decl(
            base,
            inst_name,
            args,
            variant_layouts,
        ) {
            type_decl_src.push_str(&src_line);
            synthetic_decls.push(decl);
        }
    }
    (type_decl_src, synthetic_decls)
}

/// Does a record carrying a field of type `ty` need a generated recursive `$__drop_<R>` (rather than
/// a flat one-level `rc_dec` of its block)? ANY heap field does: a flat `rc_dec` of the record block
/// frees only the block, leaking every owned heap SLOT (a `String` handle, a `List`/`Map`/`Value`
/// handle, a nested record). This was historically `false` for `String` / `List[scalar]` because the
/// DIRECT-drop path masks those slots (`record_masks` → `DropListStr`); but a record so classified
/// gets NO `$__drop_<R>`, so when it is NESTED as a field of ANOTHER recursive record the outer's
/// per-field free (`record_drop_field_frees`) has no routine to call and falls back to a flat
/// `rc_dec` that LEAKS the inner slot (the porta `Parser = { bytes: List[Int], pos: Int }` nested in
/// `{ val, next: Parser }` — its `bytes` list leaked). Generating `$__drop_<R>` for every heap-field
/// record closes that: for an already-direct-dropped record the generated body frees the SAME slots
/// as the mask (`String`/`List[scalar]` → one `rc_dec` each), so the output is byte-identical and the
/// ownership cert stays a single `d`; the only delta is that the routine now EXISTS for nesting.
pub fn record_field_needs_recursive_drop(ty: &Ty) -> bool {
    is_heap_ty(ty)
}

/// Record decls WITH type params (`type Pair[A, B] = { fst: A, snd: B }`): name →
/// (params, decl fields). A generic decl gets NO shared `$__drop_<R>` — each
/// INSTANTIATION routes to a per-shape `__drop_anonrec_<hash>` (the anon-record
/// machinery), because the heap MASK differs per instantiation: one `__drop_Pair`
/// generated from the tyvar fields treated BOTH slots as heap, so dropping a
/// `Pair[Int, String]` rc_dec'd the Int 1 as a pointer (trap) and a scalar-only
/// `Pair[Int, Int]` would double-free garbage.
pub(crate) type GenericRecordDecls =
    std::collections::HashMap<String, (Vec<almide_lang::intern::Sym>, Vec<(almide_lang::intern::Sym, Ty)>)>;

pub(crate) fn generic_record_decls(type_decls: &[almide_ir::IrTypeDecl]) -> GenericRecordDecls {
    use almide_ir::IrTypeDeclKind;
    type_decls
        .iter()
        .filter_map(|d| {
            let IrTypeDeclKind::Record { fields } = &d.kind else { return None };
            let gs = d.generics.as_ref()?;
            if gs.is_empty() {
                return None;
            }
            Some((
                d.name.as_str().to_string(),
                (
                    gs.iter().map(|g| g.name).collect(),
                    fields.iter().map(|f| (f.name, f.ty.clone())).collect(),
                ),
            ))
        })
        .collect()
}

/// The SUBSTITUTED field list of an INSTANTIATED generic record type
/// (`Pair[Int, String]` → `[(fst, Int), (snd, String)]`), or `None` when `ty` is not
/// an instantiation of a generic record decl. The (name, ty) pairs are exactly what
/// `anon_record_drop_name` hashes — so routing and generation agree on the identity.
pub(crate) fn instantiated_generic_record_fields(
    ty: &Ty,
    generic_decls: &GenericRecordDecls,
) -> Option<Vec<(almide_lang::intern::Sym, Ty)>> {
    use almide_lang::types::constructor::TypeConstructorId;
    let (name, args) = match ty {
        Ty::Named(n, args) if !args.is_empty() => (n.as_str().to_string(), args),
        Ty::Applied(TypeConstructorId::UserDefined(n), args) if !args.is_empty() => {
            (n.clone(), args)
        }
        _ => return None,
    };
    let keys: std::collections::HashSet<String> = generic_decls.keys().cloned().collect();
    let key = crate::lower::canonical_name_in(&keys, &name)?.to_string();
    let (gs, fields) = generic_decls.get(&key)?;
    let mut subst: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
        std::collections::HashMap::new();
    for (g, a) in gs.iter().zip(args.iter()) {
        subst.insert(*g, a.clone());
    }
    Some(fields.iter().map(|(n, t)| (*n, calls::subst_type_var(t, &subst))).collect())
}

/// The set of RECORD type names whose drop must be the recursive `$__drop_<R>` (any field
/// [`record_field_needs_recursive_drop`]). A scalar/String-only record keeps the flat masked
/// `DropListStr`. Mirrors [`variant_needs_recursive_drop`] for records. GENERIC decls are
/// EXCLUDED — their instantiations route to per-shape `__drop_anonrec_<hash>` drops (see
/// [`generic_record_decls`]).
pub fn recursive_record_drop_names(
    type_decls: &[almide_ir::IrTypeDecl],
) -> std::collections::HashSet<String> {
    use almide_ir::IrTypeDeclKind;
    type_decls
        .iter()
        .filter_map(|d| match &d.kind {
            IrTypeDeclKind::Record { fields }
                if d.generics.as_ref().is_none_or(|g| g.is_empty())
                    && fields.iter().any(|f| record_field_needs_recursive_drop(&f.ty)) =>
            {
                Some(d.name.as_str().to_string())
            }
            _ => None,
        })
        .collect()
}

/// `Some(name)` iff `ty` is a NAMED record/aggregate whose `$__drop_<name>` is generated (it is in
/// `rec_names`) — so a field of that type recurses via `__drop_<name>`. A non-recursive (scalar-only)
/// record is `None`: it is freed by a flat `rc_dec` of its block.
/// Resolve a nested aggregate FIELD to its recursive-drop route: `(drop name, source
/// binding type)`. An INSTANTIATED generic record resolves to its per-shape
/// `anonrec_<hash>` + the STRUCTURAL `{ k: T, … }` binding type (its bare name would
/// not re-instantiate in the generated source); everything else rides
/// [`recursive_aggregate_name`] + [`aggregate_source_ty`].
fn recursive_aggregate_route(
    ty: &Ty,
    rec_names: &std::collections::HashSet<String>,
    generic_decls: &GenericRecordDecls,
) -> Option<(String, String)> {
    if let Some(pairs) = instantiated_generic_record_fields(ty, generic_decls) {
        if anon_record_needs_recursive_drop(&pairs) {
            return Some((anon_record_drop_name(&pairs), anon_record_source_ty(&pairs)));
        }
        return None;
    }
    recursive_aggregate_name(ty, rec_names).map(|rn| {
        let src = aggregate_source_ty(ty);
        (rn, src)
    })
}

fn recursive_aggregate_name(ty: &Ty, rec_names: &std::collections::HashSet<String>) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let n = match ty {
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        // An ANONYMOUS record field that itself needs the recursive drop (a heap-nested anon
        // record, e.g. `{ st: Cfb8State }` inside another anon record) routes to its synthesized
        // `__drop_<anon_hash>` (registered by `anon_record_drop_name`). It is NOT in `type_decls`,
        // so `rec_names` won't carry it — admit it directly here.
        Ty::Record { fields } if anon_record_needs_recursive_drop(fields) => {
            return Some(anon_record_drop_name(fields));
        }
        _ => return None,
    };
    // A cross-module field may be spelled BARE (`Lin`) while `rec_names` carries the
    // QUALIFIED decl name (`types_mod.Lin`) — resolve via the unique-suffix rule so the
    // generated free targets the real `$__drop_<canonical>` (an ambiguous bare name stays
    // unresolved → the field falls to the flat arm, never a wrong-name dangling call).
    canonical_name_in(rec_names, &n).map(|k| k.to_string())
}

/// A DETERMINISTIC, host-independent synthetic type name for an ANONYMOUS record shape, used as the
/// suffix of its synthesized recursive drop `$__drop_<name>` (and the `variant_drop_handles` route).
/// FNV-1a over the ordered `(field-name, field-type-tag)` shape — the SAME shape two structurally
/// equal anon records share, so they dedup to one `__drop`. The `anonrec_` prefix keeps it disjoint
/// from any user type name. Stable across native/wasm hosts (pure arithmetic, no pointer/order deps).
pub(crate) fn anon_record_drop_name(fields: &[(almide_lang::intern::Sym, Ty)]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for (name, ty) in fields {
        mix(name.as_str().as_bytes());
        mix(b"\x00");
        mix(ty_shape_tag(ty).as_bytes());
        mix(b"\x00");
    }
    format!("anonrec_{h:016x}")
}
