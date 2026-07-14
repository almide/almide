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
                } else if matches!(ty, Ty::String) {
                    frees.push_str(&format!(
                        "        prim.rc_dec(prim.load64(h + {off}))\n"
                    ));
                } else if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 && !is_heap_ty(&a[0]))
                {
                    // A List[scalar] ctor field — a FLAT block, one rc_dec is its full free.
                    frees.push_str(&format!(
                        "        prim.rc_dec(prim.load64(h + {off}))
"
                    ));
                } else if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
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
                } else if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a)
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
                } else if let Some(ev) = list_rich_variant_elem(ty, &rec_variant_names) {
                    // A `List[<rich variant>]` ctor field (`Block(_, List[Instr])`): each element is a
                    // recursive-drop variant block, freed per-element by the generated `$__drop_list_<ev>`
                    // (→ `$__drop_<ev>`). A flat `rc_dec` of the list block would leak every element.
                    let ev_fn = drop_fn_ident(&ev);
                    frees.push_str(&format!(
                        "        let f{idx}: List[{ev}] = prim.load_handle(h + {off})\n        __drop_list_{ev_fn}(f{idx})\n"
                    ));
                    idx += 1;
                } else if let Ty::Named(rn, _) = ty {
                    if all_record_names.contains(rn.as_str()) {
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
pub fn discover_generic_variant_list_instantiations(
    ir: &almide_ir::IrProgram,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Vec<(String, String, Vec<Ty>)> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    use almide_ir::{IrExpr, IrExprKind};
    use almide_lang::types::constructor::TypeConstructorId;

    struct Scan<'a> {
        variant_layouts: &'a crate::lower::VariantLayouts,
        found: std::collections::BTreeMap<String, (String, Vec<Ty>)>,
    }
    impl IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::List { .. }) {
                if let Ty::Applied(TypeConstructorId::List, a) = &e.ty {
                    if a.len() == 1 {
                        if let Some((name, args)) = crate::lower::VariantLayouts::variant_name_and_args(&a[0]) {
                            if !args.is_empty() {
                                if let Some(layout) = self.variant_layouts.by_type.get(name) {
                                    if !layout.generics.is_empty() {
                                        if let Some(inst) =
                                            crate::lower::generic_variant_instantiation_name(name, args)
                                        {
                                            self.found
                                                .entry(inst.clone())
                                                .or_insert_with(|| (name.to_string(), args.to_vec()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    let mut scan = Scan { variant_layouts, found: Default::default() };
    for f in &ir.functions {
        scan.visit_expr(&f.body);
    }
    for tl in &ir.top_lets {
        scan.visit_expr(&tl.value);
    }
    for m in &ir.modules {
        for f in &m.functions {
            scan.visit_expr(&f.body);
        }
        for tl in &m.top_lets {
            scan.visit_expr(&tl.value);
        }
    }
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
pub fn generate_generic_variant_instantiation_type_decls(
    instantiations: &[(String, String, Vec<Ty>)],
    variant_layouts: &crate::lower::VariantLayouts,
) -> (String, Vec<almide_ir::IrTypeDecl>) {
    use almide_lang::intern::sym;

    let mut type_decl_src = String::new();
    let mut synthetic_decls: Vec<almide_ir::IrTypeDecl> = Vec::new();
    for (base, inst_name, args) in instantiations {
        let Some(layout) = variant_layouts.by_type.get(base) else { continue };
        let subst: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
            layout.generics.iter().copied().zip(args.iter().cloned()).collect();
        let mut cases: Vec<almide_ir::IrVariantDecl> = Vec::with_capacity(layout.cases.len());
        let mut case_src_parts: Vec<String> = Vec::with_capacity(layout.cases.len());
        let mut ok = true;
        for c in &layout.cases {
            let mut field_tys: Vec<Ty> = Vec::with_capacity(c.fields.len());
            let mut field_src_parts: Vec<String> = Vec::with_capacity(c.fields.len());
            for (_, fty) in &c.fields {
                let sub = substitute_generic_ty(fty, &subst);
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
                let Some(rendered) = rendered.filter(|s| !s.is_empty()) else {
                    ok = false;
                    break;
                };
                field_src_parts.push(rendered);
                field_tys.push(sub);
            }
            if !ok {
                break;
            }
            let ctor_name = format!("__{inst_name}_c{}", c.tag);
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
        if !ok || cases.is_empty() {
            continue;
        }
        type_decl_src.push_str(&format!("type {inst_name} = {}\n", case_src_parts.join(" | ")));
        synthetic_decls.push(almide_ir::IrTypeDecl {
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
        });
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

/// The set of RECORD type names whose drop must be the recursive `$__drop_<R>` (any field
/// [`record_field_needs_recursive_drop`]). A scalar/String-only record keeps the flat masked
/// `DropListStr`. Mirrors [`variant_needs_recursive_drop`] for records.
pub fn recursive_record_drop_names(
    type_decls: &[almide_ir::IrTypeDecl],
) -> std::collections::HashSet<String> {
    use almide_ir::IrTypeDeclKind;
    type_decls
        .iter()
        .filter_map(|d| match &d.kind {
            IrTypeDeclKind::Record { fields }
                if fields.iter().any(|f| record_field_needs_recursive_drop(&f.ty)) =>
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

/// A structural string tag for a field type, fine enough that two anon records with DIFFERENT field
/// types (hence different drop bodies) get different names, recursing into nested aggregates so a
/// `{ st: A }` and `{ st: B }` never collide. Only the drop-relevant structure matters.
fn ty_shape_tag(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Named(n, _) => format!("N{}", n.as_str()),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => format!("N{n}"),
        Ty::Applied(c, a) => {
            let inner: Vec<String> = a.iter().map(ty_shape_tag).collect();
            format!("A{c:?}[{}]", inner.join(","))
        }
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let inner: Vec<String> =
                fields.iter().map(|(k, t)| format!("{}:{}", k.as_str(), ty_shape_tag(t))).collect();
            format!("R{{{}}}", inner.join(","))
        }
        Ty::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(ty_shape_tag).collect();
            format!("T({})", inner.join(","))
        }
        other => format!("{other:?}"),
    }
}

/// Does an ANONYMOUS record (`Ty::Record`) need a SYNTHESIZED recursive `$__drop_<hash>`? It does iff
/// ANY field needs a recursive drop ([`record_field_needs_recursive_drop`]) — EXACTLY the predicate
/// `recursive_record_drop_names` uses for NAMED records, since the slot layout is identical. A flat
/// one-level mask `rc_dec`s only each field's HANDLE: that fully frees a flat-heap field (Bytes /
/// String — a single buffer) but only frees the BLOCK of a field that itself holds heap handles (a
/// nested record / Value / Map / `List[heap]`), leaking what's inside. So an anon record that owns
/// any heap field at all needs the synthesized recursive drop (the body flat-frees the
/// single-buffer fields and recurses into the handle-holding ones via `record_drop_field_frees`).
/// `record_field_needs_recursive_drop` is structural and host-independent.
pub(crate) fn anon_record_needs_recursive_drop(fields: &[(almide_lang::intern::Sym, Ty)]) -> bool {
    fields.iter().any(|(_, t)| record_field_needs_recursive_drop(t))
}

/// The per-field FREE statements of a record's recursive `$__drop` body (shared by the named-record
/// and the synthesized anon-record generators — the SINGLE source of truth for record field drops,
/// so the two can never drift). Each field at `slot_offset(i)` is freed by its CONCRETE type:
/// `String → rc_dec`, `Map[String,String] → __drop_map_ss`, `List[String] → __drop_list_str`,
/// `List[<recursive record>] → __drop_list_<R>`, a recursive record (named or anon) → `__drop_<R>`,
/// a `Value → __drop_value`, a scalar-only nested aggregate / `List[scalar]` → flat `rc_dec`, a
/// scalar → skip. Records the needed shared-helper flags into the caller's accumulators so they are
/// emitted once at the end.
#[allow(clippy::too_many_arguments)]
fn record_drop_field_frees(
    field_tys: &[Ty],
    rec_names: &std::collections::HashSet<String>,
    flat_variant_names: &std::collections::HashSet<String>,
    rec_variant_names: &std::collections::HashSet<String>,
    list_drops: &mut std::collections::BTreeSet<String>,
    need_map_ss: &mut bool,
    need_list_str: &mut bool,
    need_matrix: &mut bool,
    need_list_matrix: &mut bool,
) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    let mut frees = String::new();
    for (i, ty) in field_tys.iter().enumerate() {
        let off = layout::slot_offset(i);
        match ty {
            Ty::String => {
                frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
            }
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                if let Some(rn) = recursive_aggregate_name(&a[0], rec_names) {
                    list_drops.insert(rn.clone());
                    let rn_fn = drop_fn_ident(&rn);
                    // The BINDING type must be valid Almide source: a NAMED element renders
                    // its name, an ANONYMOUS element its STRUCTURAL `{ k: T, … }` form — the
                    // synthesized `anonrec_<hash>` is a drop-fn identity, NOT a type name
                    // (writing `List[anonrec_…]` type-errored the whole generated batch:
                    // "undefined variable 'f0'" after the rejected let).
                    let src = aggregate_source_ty(&a[0]);
                    frees.push_str(&format!(
                        "    let f{i}: List[{src}] = prim.load_handle(h + {off})\n    __drop_list_{rn_fn}(f{i})\n"
                    ));
                } else if let Some(ev) = list_rich_variant_elem(ty, rec_variant_names) {
                    // `List[<rich variant>]` (`Global.init: List[Instr]`): each element is a
                    // recursive-drop variant block, freed per-element by `$__drop_list_<ev>` (→
                    // `$__drop_<ev>`, generated by `generate_variant_drop_sources`). A flat `rc_dec`
                    // of the list block would leak every element's nested children (its own List[Instr]).
                    let ev_fn = drop_fn_ident(&ev);
                    frees.push_str(&format!(
                        "    let f{i}: List[{ev}] = prim.load_handle(h + {off})\n    __drop_list_{ev_fn}(f{i})\n"
                    ));
                } else if matches!(&a[0], Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _)) {
                    // `List[Matrix]` — each element is a matrix block whose slots hold owned
                    // row blocks: sweep TWO levels via `__drop_list_matrix` (each element
                    // through `__drop_matrix`, then the list). A flat `rc_dec` would leak
                    // every matrix AND its rows.
                    *need_matrix = true;
                    *need_list_matrix = true;
                    frees.push_str(&format!(
                        "    let f{i}: List[Matrix] = prim.load_handle(h + {off})\n    __drop_list_matrix(f{i})\n"
                    ));
                } else if matches!(&a[0],
                    Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && !is_heap_ty(&b[0]))
                {
                    // A matrix-shaped STRUCTURAL field (`List[List[scalar]]`): its slots hold
                    // owned flat row blocks — `__drop_matrix`'s per-row `rc_dec` sweep is its
                    // exact free (a flat `rc_dec` frees only the outer block, leaking rows).
                    *need_matrix = true;
                    frees.push_str(&format!(
                        "    let f{i}: Matrix = prim.load_handle(h + {off})\n    __drop_matrix(f{i})\n"
                    ));
                } else if matches!(a[0], Ty::String) || is_flat_variant_elem(&a[0], flat_variant_names) {
                    // `List[String]` OR `List[flat-variant]` (a nullary/scalar-only enum like
                    // `Capability`): each element is a single FLAT block, so `__drop_list_str` frees
                    // them per-element (`rc_dec` of each element handle + the list block). The flat
                    // variant element holds no inner handle, so the byte-identical String-list drop is
                    // its full free — a flat `rc_dec` of just the list block would LEAK each element.
                    *need_list_str = true;
                    frees.push_str(&format!(
                        "    let f{i}: List[String] = prim.load_handle(h + {off})\n    __drop_list_str(f{i})\n"
                    ));
                } else {
                    // List[scalar] or List[non-recursive heap]: flat free the block.
                    frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
                }
            }
            // A `Matrix` field (the v1 value model: a List[List[Float]] block whose slots
            // hold owned flat row blocks — nn WhisperWeights.conv1_w): free each row + the
            // block via `__drop_matrix`. The previous flat `rc_dec` fallback leaked every row.
            Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _) => {
                *need_matrix = true;
                frees.push_str(&format!(
                    "    let f{i}: Matrix = prim.load_handle(h + {off})\n    __drop_matrix(f{i})\n"
                ));
            }
            Ty::Applied(TypeConstructorId::Map, a)
                if a.len() == 2 && matches!(a[0], Ty::String) && matches!(a[1], Ty::String) =>
            {
                *need_map_ss = true;
                frees.push_str(&format!(
                    "    let f{i}: Map[String, String] = prim.load_handle(h + {off})\n    __drop_map_ss(f{i})\n"
                ));
            }
            t if is_value_ty(t) => {
                frees.push_str(&format!(
                    "    let f{i}: Value = prim.load_handle(h + {off})\n    __drop_value(f{i})\n"
                ));
            }
            // A CLOSURE field (`Handler.run: (String) -> String`): the slot holds a
            // self-describing closure block ([fnidx][nh|nc<<16][env…]) whose captured heap
            // env a flat rc_dec would LEAK — free it via the generated `__drop_closure`
            // (the SAME recursive routine every closure drop site uses; CLOSURE_DROP_SRC
            // is linked whenever the program creates closures, which a populated Fn field
            // requires). The binding type is the closure block's List[Int] rep.
            Ty::Fn { .. } => {
                frees.push_str(&format!(
                    "    let f{i}: List[Int] = prim.load_handle(h + {off})\n    __drop_closure(f{i})\n"
                ));
            }
            t => {
                if let Some(rn) = recursive_aggregate_name(t, rec_names) {
                    let src = aggregate_source_ty(t);
                    let rn_fn = drop_fn_ident(&rn);
                    frees.push_str(&format!(
                        "    let f{i}: {src} = prim.load_handle(h + {off})\n    __drop_{rn_fn}(f{i})\n"
                    ));
                } else if is_heap_ty(t) {
                    // a non-recursive heap field (scalar-only nested record, Bytes, scalar map) — flat.
                    frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
                }
                // a scalar field — skip (no free).
            }
        }
    }
    frees
}

/// Is `ty` a FLAT custom variant (in `flat_variant_names`) — a `List[ty]` element that frees as a
/// single block? `Named`/`UserDefined` only; `List`/`Map`/`Value`/record types never qualify.
fn is_flat_variant_elem(ty: &Ty, flat_variant_names: &std::collections::HashSet<String>) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let n = match ty {
        Ty::Named(n, _) => n.as_str(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.as_str(),
        _ => return false,
    };
    flat_variant_names.contains(n)
}

/// If `ty` is `List[V]` where `V` is a RICH (recursive-drop) variant (in `rec_variant_names`), return
/// `V`'s name — the element drop `$__drop_<V>` that `$__drop_list_<V>` calls per element. `None` for a
/// non-list, a scalar/String/flat-variant element list, or a record-element list (those route
/// elsewhere). Used by the variant-ctor field generator AND `record_drop_field_frees` so a `List[Instr]`
/// field (`Global.init`, `Block`'s payload) is freed recursively instead of leaking.
fn list_rich_variant_elem(
    ty: &Ty,
    rec_variant_names: &std::collections::HashSet<String>,
) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let Ty::Applied(TypeConstructorId::List, a) = ty else { return None };
    if a.len() != 1 {
        return None;
    }
    let n = match &a[0] {
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        _ => return None,
    };
    rec_variant_names.contains(&n).then_some(n)
}

/// The ALMIDE SOURCE TYPE for a recursive-aggregate field (the `let fN: <ty> =` binding type in a
/// drop body). A NAMED aggregate renders to its name; an ANONYMOUS record renders to its structural
/// `{ k: T, … }` form (so a heap-nested anon-record field binds + recurses through `__drop_<hash>`).
fn aggregate_source_ty(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        Ty::Record { fields } | Ty::OpenRecord { fields } => anon_record_source_ty(fields),
        _ => field_source_ty(ty),
    }
}

/// The ALMIDE SOURCE rendering of an anonymous record TYPE — `{ k0: T0, k1: T1 }` — used as the
/// synthesized `__drop_<hash>` parameter type and a nested anon-record field binding type. Field
/// types render via [`field_source_ty`] (the drop-relevant subset: Bytes/String/Int/.../named
/// records / `List[..]` / `Map[..]` / `Value` / nested anon records).
fn anon_record_source_ty(fields: &[(almide_lang::intern::Sym, Ty)]) -> String {
    let inner: Vec<String> = fields
        .iter()
        .map(|(k, t)| format!("{}: {}", k.as_str(), field_source_ty(t)))
        .collect();
    format!("{{ {} }}", inner.join(", "))
}

/// Render a record FIELD type back to Almide source for a drop binding/param. Total over the field
/// types a recursive-drop record can carry; an unhandled exotic type falls back to `Bytes` (a flat
/// heap block) ONLY as a defensive default — discovery (`anon_record_needs_recursive_drop`) never
/// synthesizes a drop for a shape whose fields it cannot classify, so this fallback is unreachable
/// for the registered shapes.
fn field_source_ty(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Int | Ty::Int64 => "Int".to_string(),
        Ty::Int8 => "Int8".to_string(),
        Ty::Int16 => "Int16".to_string(),
        Ty::Int32 => "Int32".to_string(),
        Ty::UInt8 => "UInt8".to_string(),
        Ty::UInt16 => "UInt16".to_string(),
        Ty::UInt32 => "UInt32".to_string(),
        Ty::UInt64 => "UInt64".to_string(),
        Ty::Float | Ty::Float64 => "Float".to_string(),
        Ty::Float32 => "Float32".to_string(),
        Ty::Bool => "Bool".to_string(),
        Ty::String => "String".to_string(),
        Ty::Bytes => "Bytes".to_string(),
        Ty::Named(n, _) => n.as_str().to_string(),
        Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
            format!("List[{}]", field_source_ty(&a[0]))
        }
        Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2 => {
            format!("Map[{}, {}]", field_source_ty(&a[0]), field_source_ty(&a[1]))
        }
        t if is_value_ty(t) => "Value".to_string(),
        Ty::Record { fields } | Ty::OpenRecord { fields } => anon_record_source_ty(fields),
        Ty::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(field_source_ty).collect();
            format!("({})", inner.join(", "))
        }
        // Defensive: a shape the synthesizer never registers (see doc). Bytes = a flat heap block.
        _ => "Bytes".to_string(),
    }
}

/// Walk the IR (every function's signature + body-expr types, every type decl's record fields) and
/// COLLECT the distinct ANONYMOUS record shapes that need a synthesized recursive drop — the input
/// to [`generate_record_drop_sources`]'s anon-drop loop. A shape qualifies iff at least one field
/// frees NON-flat (the flat one-level mask would leak — `anon_record_needs_recursive_drop`), where a
/// NAMED field record's recursiveness is resolved through the program's `rec_names`. Deduped by the
/// content-hash drop name. This is the discovery half; the generation half is the anon loop in
/// `generate_record_drop_sources`.
pub fn collect_recursive_anon_records(
    program: &almide_ir::IrProgram,
) -> Vec<Vec<(almide_lang::intern::Sym, Ty)>> {
    let mut all_decls: Vec<almide_ir::IrTypeDecl> = program.type_decls.clone();
    // A visitor that inspects every expression's type (every IrExpr carries its `ty`), collecting
    // the distinct anon record shapes that need a synthesized recursive drop (deduped by drop name).
    // RECURSES into a qualifying anon record's own anon-record FIELDS so a nested anon shape
    // (`{ st: { iv: Bytes } }`) gets its inner `__drop_anonrec_<hash>` generated too (the outer drop
    // body `let f: { iv: Bytes } = …; __drop_anonrec_<inner>(f)` would otherwise call a missing fn).
    struct TyCollector {
        seen: std::collections::HashSet<String>,
        out: Vec<Vec<(almide_lang::intern::Sym, Ty)>>,
    }
    impl TyCollector {
        fn consider(&mut self, ty: &Ty) {
            use almide_lang::types::constructor::TypeConstructorId;
            match ty {
                Ty::Record { fields } if anon_record_needs_recursive_drop(fields) => {
                    let name = anon_record_drop_name(fields);
                    if self.seen.insert(name) {
                        self.out.push(fields.clone());
                    }
                    // Recurse into field types so a nested anon record / a `List[anon]` element
                    // also registers its drop.
                    for (_, fty) in fields {
                        self.consider(fty);
                    }
                }
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => self.consider(&a[0]),
                Ty::Tuple(elems) => {
                    for e in elems {
                        self.consider(e);
                    }
                }
                _ => {}
            }
        }
    }
    impl almide_ir::visit::IrVisitor for TyCollector {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            self.consider(&expr.ty);
            almide_ir::visit::walk_expr(self, expr);
        }
    }

    let mut collector = TyCollector { seen: std::collections::HashSet::new(), out: Vec::new() };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        collector.consider(&f.ret_ty);
        let param_tys: Vec<Ty> = f.params.iter().map(|p| p.ty.clone()).collect();
        for ty in &param_tys {
            collector.consider(ty);
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut collector, &f.body);
    }
    collector.out
}

/// The ALMIDE SOURCE of `$__drop_list_lenlist` — the recursive release of a list whose
/// elements are LEN-AS-TAG Option/Result blocks with OWNED handle slots (`List[Option[
/// String]]`, `List[Result[Int, String]]`, `List[Result[String, String]]`): at the list's
/// last ref, for each element (an owned wrapper handle) free its first `len` owned slots
/// (Ok-scalar = len 0 → nothing; Some/Err/cap-as-tag = len 1 → the payload) at the
/// ELEMENT's last ref, then the element block, then the list. The rc==1 masks mirror
/// `$__drop_closure`. Like every generated `$__drop_*`, a trusted prim-only routine
/// (outside the witness surface), pinned by the list leak-loop coverage.
/// The ALMIDE SOURCE of `__drop_list_str` — the generic per-element release of a
/// `List[String]` (each slot an OWNED String handle: `rc_dec` every element, then the
/// list block). SHARED by both `generate_record_drop_sources` (a record field) and
/// `generate_variant_drop_sources` (a variant ctor field) — emitted ONCE here, gated by
/// [`program_uses_list_str_drop_field`], so two generators referencing the same helper
/// name never double-define it (a duplicate-fn compile error) when a single program has
/// both a record AND a variant with a `List[String]` field.
pub const LIST_STR_DROP_SRC: &str = "\
fn __drop_list_str(xs: List[String]) -> Unit = {
  let h = prim.handle(xs)
  if prim.load32(h + 0) == 1 then __drop_list_str_loop(h, prim.load32(h + 4), 0) else ()
  prim.rc_dec(h)
}
fn __drop_list_str_loop(h: Int, n: Int, i: Int) -> Unit =
  if i >= n then ()
  else { prim.rc_dec(prim.load64(h + 12 + i * 8))
         __drop_list_str_loop(h, n, i + 1) }
";

/// Is `t` a `List[String]` (or `List[<flat variant>]`) — the shape whose scope-end drop
/// routes to the shared `__drop_list_str`? Shared by [`program_uses_list_str_drop_field`]
/// (named type decls) and [`program_uses_anon_list_str_record`] (anonymous record shapes,
/// which never appear in `type_decls`).
fn is_list_str_field(t: &Ty, flat_names: &std::collections::HashSet<String>) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(t, Ty::Applied(TypeConstructorId::List, a)
        if a.len() == 1
            && (matches!(a[0], Ty::String) || is_flat_variant_elem(&a[0], flat_names)))
}

/// Does the program's type decls carry a record OR variant field that routes to the
/// shared `__drop_list_str` — a `List[String]` field, or a `List[<flat variant>]` field
/// (record side only; the variant generator does not admit that shape yet)? Gates
/// [`LIST_STR_DROP_SRC`]'s single emission in the pipeline (a program never touching
/// this shape pays no dead drop routine).
pub fn program_uses_list_str_drop_field(type_decls: &[almide_ir::IrTypeDecl]) -> bool {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let flat_names = flat_variant_type_names(type_decls);
    type_decls.iter().any(|d| match &d.kind {
        IrTypeDeclKind::Record { fields } => fields.iter().any(|f| is_list_str_field(&f.ty, &flat_names)),
        IrTypeDeclKind::Variant { cases, .. } => cases.iter().any(|c| {
            let tys: Vec<&Ty> = match &c.kind {
                IrVariantKind::Unit => vec![],
                IrVariantKind::Tuple { fields } => fields.iter().collect(),
                IrVariantKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
            };
            tys.iter().any(|t| is_list_str_field(t, &flat_names))
        }),
        _ => false,
    })
}

/// Does the program carry an ANONYMOUS record shape (`{ out: List[String], flag: Bool }` —
/// never declared via `type X = {...}`, so it never appears in `type_decls`) with a
/// `List[String]`-ish field? These route to the SAME shared `__drop_list_str` as a named
/// record's field (`anon_record_drop_name`'s generated `$__drop_<anonrec_...>` frees such a
/// field via the identical flat routine) — but `program_uses_list_str_drop_field` only scans
/// `type_decls`, so a program whose ONLY such field lives in an anonymous record (a `list.fold`
/// accumulator record, a plain record literal/param never given a name) rendered a call to
/// `__drop_list_str` that was never emitted (a dangling `(call $__drop_list_str)` — invalid
/// wasm the render step's own type-check catches, not a silent wrong-bytes risk, but a hard
/// failure on an otherwise-lowerable program). Whole-program scan (every expr's `.ty`, plus
/// every function's param/return types) for a `Record`/`OpenRecord` field of this shape,
/// mirroring `program_uses_closures`'s visitor.
pub fn program_uses_anon_list_str_record(
    program: &almide_ir::IrProgram,
    type_decls: &[almide_ir::IrTypeDecl],
) -> bool {
    let flat_names = flat_variant_type_names(type_decls);
    let has_field = |fields: &[(almide_lang::intern::Sym, Ty)]| {
        fields.iter().any(|(_, t)| is_list_str_field(t, &flat_names))
    };
    let ty_matches = |t: &Ty| matches!(t, Ty::Record { fields } | Ty::OpenRecord { fields } if has_field(fields));
    struct Finder<'a> {
        found: bool,
        ty_matches: &'a dyn Fn(&Ty) -> bool,
    }
    impl almide_ir::visit::IrVisitor for Finder<'_> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if (self.ty_matches)(&expr.ty) {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false, ty_matches: &ty_matches };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if ty_matches(&f.ret_ty) || f.params.iter().any(|p| ty_matches(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

pub const LENLIST_DROP_SRC: &str = "\
fn __drop_list_lenlist(xs: List[Int]) -> Unit = {
  let h = prim.handle(xs)
  if prim.load32(h + 0) == 1 then {
    let n = prim.load32(h + 4)
    __drop_list_lenlist_loop(h, n, 0)
  } else ()
  prim.rc_dec(h)
}
fn __drop_list_lenlist_loop(h: Int, n: Int, i: Int) -> Unit =
  if i >= n then ()
  else {
    let e = prim.load64(h + 12 + i * 8)
    if prim.load32(e + 0) == 1 then {
      let k = prim.load32(e + 4)
      __drop_lenlist_slots(e, k, 0)
    } else ()
    prim.rc_dec(e)
    __drop_list_lenlist_loop(h, n, i + 1)
  }
fn __drop_lenlist_slots(e: Int, k: Int, j: Int) -> Unit =
  if j >= k then ()
  else {
    prim.rc_dec(prim.load64(e + 12 + j * 8))
    __drop_lenlist_slots(e, k, j + 1)
  }
";

/// The ALMIDE SOURCE of `$__drop_res_ilsl` — the TAG-AWARE release of a
/// `Result[List[Int], List[String]]` (the result.collect return): tag@16 = Err(1)
/// → the @12 payload is a `List[String]` whose Strings free at ITS last ref
/// (recursive); tag = Ok(0) → the @12 `List[Int]` block frees FLAT (freeing it
/// recursively would rc_dec raw int values as handles — unsound). This is the
/// "exact drop" the collect family was pending. Like every generated `$__drop_*`,
/// a trusted prim-only routine (outside the witness surface).
pub const RES_ILSL_DROP_SRC: &str = "\
fn __drop_res_ilsl(r: List[Int]) -> Unit = {
  let h = prim.handle(r)
  if prim.load32(h + 0) == 1 then {
    let inner = prim.load32(h + 12)
    if prim.load32(h + 16) == 1 then {
      if prim.load32(inner + 0) == 1 then {
        let n = prim.load32(inner + 4)
        __drop_res_ilsl_strs(inner, n, 0)
      } else ()
    } else ()
    prim.rc_dec(inner)
  } else ()
  prim.rc_dec(h)
}
fn __drop_res_ilsl_strs(e: Int, n: Int, i: Int) -> Unit =
  if i >= n then ()
  else {
    prim.rc_dec(prim.load64(e + 12 + i * 8))
    __drop_res_ilsl_strs(e, n, i + 1)
  }
";

/// Is `ty` exactly `Result[List[Int], List[String]]` (the result.collect return —
/// the tag-aware `$__drop_res_ilsl` class)?
pub fn is_res_intlist_strlist_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    matches!(ty, Ty::Applied(TC::Result, a) if a.len() == 2
        && matches!(&a[0], Ty::Applied(TC::List, i) if i.len() == 1 && matches!(i[0], Ty::Int))
        && matches!(&a[1], Ty::Applied(TC::List, s) if s.len() == 1 && matches!(s[0], Ty::String)))
}

/// Does the program mention `Result[List[Int], List[String]]` anywhere a drop could
/// fire (fn sigs, binds, exprs)? Gates the `$__drop_res_ilsl` source injection.
pub fn program_uses_res_intlist_strlist(program: &almide_ir::IrProgram) -> bool {
    struct C(bool);
    impl almide_ir::visit::IrVisitor for C {
        fn visit_expr(&mut self, e: &IrExpr) {
            if is_res_intlist_strlist_ty(&e.ty) {
                self.0 = true;
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut c = C(false);
    for f in program.functions.iter().chain(program.modules.iter().flat_map(|m| m.functions.iter())) {
        if f.params.iter().any(|p| is_res_intlist_strlist_ty(&p.ty))
            || is_res_intlist_strlist_ty(&f.ret_ty)
        {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut c, &f.body);
        if c.0 {
            return true;
        }
    }
    false
}

/// The ALMIDE SOURCE of the UNIFORM closure-block release `$__drop_closure` (the closures
/// machinery — injected by the render pipeline whenever the program carries first-class
/// function values). A closure block is SELF-DESCRIBING: slot 0 = fnidx (a table index —
/// NEVER dereferenced here), slot 1 = n_heap | (n_closure << 16), slots 2.. = captured
/// closures (freed by RECURSING into this very routine — the `compose` shape), then
/// captured heap values (each freed by ONE `rc_dec` — the lowering's capture gate admits
/// only one-level-exact kinds), then scalars (untouched). Any drop site can free any
/// closure value without knowing its captures (a call-result closure's layout is
/// unknowable at the caller). Like every generated `$__drop_*`, a trusted prim-only
/// routine (outside the witness surface), pinned by the closure leak-loop test.
/// The ALMIDE SOURCE of `__drop_list_closure` — the per-element release of a
/// `List[<Fn>]` (each slot an OWNED closure block): recurse into `__drop_closure`
/// per element (the uniform, self-describing closure free — correct whether or not
/// each element captures anything), then free the list block. Requires
/// `CLOSURE_DROP_SRC` in scope (the program already carries it whenever any closure
/// value exists, which a `List[<Fn>]` literal necessarily does). A blind per-element
/// `rc_dec` (the plain masked `DropListStr`) would be unsound for a CAPTURING
/// closure element (it would decrement the block's own refcount without recursively
/// freeing its captured heap slots) — `__drop_closure` is required, not optional,
/// even though this session's only current caller (`call_closure_lambda_param`)
/// happens to use only non-capturing lambdas.
pub const LIST_CLOSURE_DROP_SRC: &str = "\
fn __drop_list_closure(xs: List[List[Int]]) -> Unit = {
  let h = prim.handle(xs)
  if prim.load32(h + 0) == 1 then __drop_list_closure_loop(h, prim.load32(h + 4), 0) else ()
  prim.rc_dec(h)
}
fn __drop_list_closure_loop(h: Int, n: Int, i: Int) -> Unit =
  if i >= n then ()
  else {
    let e: List[Int] = prim.load_handle(h + 12 + i * 8)
    __drop_closure(e)
    __drop_list_closure_loop(h, n, i + 1)
  }
";

/// The ALMIDE SOURCE of `__drop_opt_str_int` — the recursive release of an
/// `Option[(String, Int)]` (`map.find`'s predicate-search result: `Some((key,
/// value))` on a hit). Wrapper `[rc][len@4=0-or-1 (Option's tag)][cap@8][@12
/// payload]`: at the wrapper's last ref (rc==1), IFF len==1 (Some) the @12
/// payload is the `(String, Int)` tuple's handle — at the TUPLE's own last ref,
/// `rc_dec` its String slot0 @12 (the Int slot1 @20 is scalar), then the tuple
/// block; len==0 (None) frees nothing at the payload. THEN the wrapper block,
/// always. A blind flat `rc_dec` of the @12 payload slot (the generic
/// `heap_elem_lists`/`DropListStr` route every OTHER self-host Option call
/// uses) would only decrement the TUPLE's own refcount, leaking its String —
/// the exact class of bug this session's `_str`-dispatch fix caught elsewhere.
/// Named for the (String, Int) shape specifically; a (String, Bool)/(String,
/// Float) `map.find` result reuses the SAME generated fn (the render never
/// reads the Int slot's bits, only rc_decs the String slot — the established
/// type-stand-in convention, e.g. `list_hshare.almd`'s `List[Int]` stand-in).
pub const OPT_STR_INT_DROP_SRC: &str = "\
fn __drop_opt_str_int(o: List[Int]) -> Unit = {
  let h = prim.handle(o)
  if prim.load32(h + 0) == 1 then {
    if prim.load32(h + 4) == 1 then {
      let th = prim.load64(h + 12)
      if prim.load32(th + 0) == 1 then prim.rc_dec(prim.load64(th + 12)) else ()
      prim.rc_dec(th)
    } else ()
  } else ()
  prim.rc_dec(h)
}
";

/// Header layout: `n_heap | (n_nested_heap << 16) | (n_closure << 32)` — three
/// 16-bit counts (ample for any realistic capture count). Widened from the
/// original 2-field `n_heap | (n_closure << 16)` to add the `n_nested_heap`
/// class (a `List[String]` capture — each element itself owned heap, freed via
/// `__drop_list_str`, NOT the flat `rc_dec` a one-level-exact heap capture
/// gets — the same class of bug this session's `_str`-dispatch fix and the
/// `map.find` near-miss both found, caught here BEFORE it shipped).
pub const CLOSURE_DROP_SRC: &str = "\
fn __drop_closure(c: List[Int]) -> Unit = {
  let h = prim.handle(c)
  if prim.load32(h + 0) == 1 then {
    let hdr = prim.load64(h + 20)
    let nc = hdr / 4294967296
    let rem1 = hdr - nc * 4294967296
    let nnh = rem1 / 65536
    let nh = rem1 - nnh * 65536
    __drop_closure_loop(h, nc, nnh, nh, 0)
  } else ()
  prim.rc_dec(h)
}
fn __drop_closure_loop(h: Int, nc: Int, nnh: Int, nh: Int, i: Int) -> Unit =
  if i >= nc + nnh + nh then ()
  else {
    if i < nc then {
      let q: List[Int] = prim.load_handle(h + 28 + i * 8)
      __drop_closure(q)
    } else if i < nc + nnh then {
      let ls: List[String] = prim.load_handle(h + 28 + i * 8)
      __drop_list_str(ls)
    } else {
      prim.rc_dec(prim.load64(h + 28 + i * 8))
    }
    __drop_closure_loop(h, nc, nnh, nh, i + 1)
  }
";

/// Generate the ALMIDE SOURCE for each RECORD type's recursive drop `$__drop_<R>` (the records
/// counterpart of [`generate_variant_drop_sources`]). Records have NO tag — fields sit at
/// `slot_offset(i)`, freed per CONCRETE field type: `String → rc_dec`, `Map[String,String] →
/// __drop_map_ss`, `List[String] → __drop_list_str`, `List[<recursive record>] → __drop_list_<R>`,
/// a recursive record → `__drop_<R>`, a `Value → __drop_value`, a scalar-only nested aggregate or
/// `List[scalar]` → flat `rc_dec` of the block, a scalar → skip. Emits the needed `__drop_list_<R>`
/// loops + the generic `__drop_map_ss` / `__drop_list_str` helpers. Also emits a synthesized
/// `__drop_anonrec_<hash>` for each ANONYMOUS record shape in `anon_records` that needs the
/// recursive drop (a heap-nested anon record return — aes cfb8). All `__drop_`-prefixed ⇒ on the
/// `prim.rc_dec` whitelist + an empty ownership cert (a trusted free, leak-loop verified).
pub fn generate_record_drop_sources(
    type_decls: &[almide_ir::IrTypeDecl],
    anon_records: &[Vec<(almide_lang::intern::Sym, Ty)>],
    uses_result_opt_str: bool,
) -> String {
    use almide_ir::IrTypeDeclKind;
    let rec_names = recursive_record_drop_names(type_decls);
    let flat_variant_names = flat_variant_type_names(type_decls);
    // The RICH variant names — a record `List[<rich variant>]` field (`Global.init: List[Instr]`) routes
    // to `$__drop_list_<V>` (generated by `generate_variant_drop_sources`, appended to the same program).
    let variant_names = variant_type_names(type_decls);
    let all_record_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| matches!(&d.kind, IrTypeDeclKind::Record { .. }))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let rec_variant_names: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| variant_needs_recursive_drop(d, &variant_names, &all_record_names))
        .map(|d| d.name.as_str().to_string())
        .collect();
    let mut out = String::new();
    let mut list_drops: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut need_map_ss = false;
    let mut need_list_str = false;
    let mut need_matrix = false;
    let mut need_list_matrix = false;
    for decl in type_decls {
        let IrTypeDeclKind::Record { fields } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        let fname = drop_fn_ident(tname);
        let field_tys: Vec<Ty> = fields.iter().map(|f| f.ty.clone()).collect();
        out.push_str(&format!("fn __drop_{fname}(e: {tname}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        out.push_str(&record_drop_field_frees(
            &field_tys,
            &rec_names,
            &flat_variant_names,
            &rec_variant_names,
            &mut list_drops,
            &mut need_map_ss,
            &mut need_list_str,
            &mut need_matrix,
            &mut need_list_matrix,
        ));
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
    // `$__drop_opt_<R>` for each recursive-drop record R — frees an `Option[R]` (the 0-or-1-element
    // layout) used by `Result[Option[R], String]` wrappers (`resrec:opt_<R>`, porta read_message's
    // `ok(none)` / `ok(r)` bases). The `match` drops the bound record `r` at the Some-arm end (routing
    // to `$__drop_<R>`); a None is a no-op; consuming `e` frees the Option block. Same per-R set as the
    // `$__drop_<R>` loop above, so an `$__drop_opt_<R>` is emitted only when its `$__drop_<R>` exists.
    for decl in type_decls {
        let IrTypeDeclKind::Record { .. } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        out.push_str(&format!(
            "fn __drop_opt_{tname}(e: Option[{tname}]) -> Unit = {{\n  match e {{\n    some(r) => (),\n    none => (),\n  }}\n}}\n"
        ));
    }
    // `$__drop_opt_str` — frees an `Option[String]` (the recursive-drop leaf of a `Result[Option[String],
    // String]`, the derived-Codec `__decode_option_string`). The `some(r)` arm binds the inner String
    // whose scope-end `rc_dec` frees it; consuming `e` frees the 0-or-1 Option block. Emitted ONLY when
    // the program constructs that shape (via `try_lower_result_option_scalar_str_ctor`'s `resrec:opt_str`),
    // so a program without it is not perturbed. (The scalar Option leaves — Int/Float/Bool — need no drop
    // fn: their `Result[Option[<scalar>], String]` frees flat via `DropListStr`.)
    if uses_result_opt_str {
        out.push_str(
            "fn __drop_opt_str(e: Option[String]) -> Unit = {\n  match e {\n    some(r) => (),\n    none => (),\n  }\n}\n",
        );
    }
    // `$__drop_tup_int_<R>` for each recursive-drop record R — frees a `(R, Int)` TUPLE
    // block (record handle @12 recursed via `$__drop_<R>`, the Int @20 is scalar), used
    // by `Result[(R, Int), String]` wrappers (`resrec:tup_int_<R>` — the gguf
    // parse_header `ok((GGUFHeader {…}, 24))` shape).
    for decl in type_decls {
        let IrTypeDeclKind::Record { .. } = &decl.kind else { continue };
        if !rec_names.contains(decl.name.as_str()) {
            continue;
        }
        let tname = decl.name.as_str();
        let fname = drop_fn_ident(tname);
        out.push_str(&format!(
            "fn __drop_tup_int_{fname}(e: ({tname}, Int)) -> Unit = {{
                 let h = prim.handle(e)
                 if prim.load32(h + 0) == 1 then {{
                     let r: {tname} = prim.load_handle(h + 12)
                     __drop_{fname}(r)
                 }} else ()
                 prim.rc_dec(h)
}}
"
        ));
    }
    // SYNTHESIZED recursive drops for the ANONYMOUS record return/binding shapes the corpus uses
    // (`{ data: Bytes, state: Cfb8State }` — aes cfb8). An anon record is NOT a `type` decl, so the
    // loop above never names it; it would otherwise drop via the flat one-level mask `DropListStr`,
    // which `rc_dec`s the `state` BLOCK but LEAKS the Bytes INSIDE Cfb8State. Each shape gets a
    // content-hashed `__drop_anonrec_<hash>` (dedup'd) with the SAME per-field-type recursion the
    // named generator emits — so the `state` field is freed through `__drop_Cfb8State`. The param is
    // the structural anon record type in source (`e: { data: Bytes, state: Cfb8State }`). Sorted by
    // name for host-determinism. (The discovery of WHICH anon shapes appear is the caller's; see
    // `generate_anon_record_drop_sources`.)
    let mut anon_sorted: Vec<&Vec<(almide_lang::intern::Sym, Ty)>> = anon_records.iter().collect();
    anon_sorted.sort_by_key(|fields| anon_record_drop_name(fields));
    anon_sorted.dedup_by_key(|fields| anon_record_drop_name(fields));
    for fields in anon_sorted {
        if !anon_record_needs_recursive_drop(fields) {
            continue;
        }
        let name = anon_record_drop_name(fields);
        let field_tys: Vec<Ty> = fields.iter().map(|(_, t)| t.clone()).collect();
        let param_ty = anon_record_source_ty(fields);
        out.push_str(&format!("fn __drop_{name}(e: {param_ty}) -> Unit = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str("  if prim.load32(h + 0) == 1 then {\n");
        out.push_str(&record_drop_field_frees(
            &field_tys,
            &rec_names,
            &flat_variant_names,
            &rec_variant_names,
            &mut list_drops,
            &mut need_map_ss,
            &mut need_list_str,
            &mut need_matrix,
            &mut need_list_matrix,
        ));
        out.push_str("  } else ()\n");
        out.push_str("  prim.rc_dec(h)\n");
        out.push_str("}\n");
    }
    // The SAME per-element list wrapper for each synthesized ANON-record drop — a
    // STRUCTURAL record-list literal (`take([{key: "x", val: "2"}])`, the checker
    // leaves the elements structural) routes to `list_anonrec_<hash>`; without this
    // wrapper the route referenced a missing `$__drop_list_anonrec_<hash>`.
    {
        let mut anon_sorted: Vec<&Vec<(almide_lang::intern::Sym, Ty)>> =
            anon_records.iter().collect();
        anon_sorted.sort_by_key(|fields| anon_record_drop_name(fields));
        anon_sorted.dedup_by_key(|fields| anon_record_drop_name(fields));
        for fields in anon_sorted {
            if !anon_record_needs_recursive_drop(fields) {
                continue;
            }
            let name = anon_record_drop_name(fields);
            let param_ty = anon_record_source_ty(fields);
            out.push_str(&format!(
                "fn __drop_list_{name}(xs: List[{param_ty}]) -> Unit = {{
                     let h = prim.handle(xs)
                     if prim.load32(h + 0) == 1 then __drop_list_{name}_loop(h, prim.load32(h + 4), 0) else ()
                     prim.rc_dec(h)
}}
                 fn __drop_list_{name}_loop(h: Int, n: Int, i: Int) -> Unit =
                     if i >= n then ()
                     else {{ let e: {param_ty} = prim.load_handle(h + 12 + i * 8)
         __drop_{name}(e)
         __drop_list_{name}_loop(h, n, i + 1) }}
"
            ));
        }
    }
    // A per-element-recursive `$__drop_list_<R>` for EVERY recursive-drop record R (not just the
    // field-referenced ones in `list_drops`) — so a standalone `List[R]` LITERAL value (`group([…])`)
    // routes its drop here too. Sorted for host-determinism.
    let _ = &list_drops; // (subsumed by rec_names below)
    let mut list_drop_names: Vec<&String> = rec_names.iter().collect();
    list_drop_names.sort();
    for rn in list_drop_names {
        // fn NAMES sanitize the module prefix; the `List[{rn}]` / `e: {rn}` type annotations keep
        // the dotted module-qualified name (a valid Almide type reference).
        let rn_fn = drop_fn_ident(rn);
        out.push_str(&format!(
            "fn __drop_list_{rn_fn}(xs: List[{rn}]) -> Unit = {{\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_{rn_fn}_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}}\n\
             fn __drop_list_{rn_fn}_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else {{ let e: {rn} = prim.load_handle(h + 12 + i * 8)\n         __drop_{rn_fn}(e)\n         __drop_list_{rn_fn}_loop(h, n, i + 1) }}\n"
        ));
    }
    if need_map_ss {
        // v1's `Map[String,String]` borrows the `map_skv` (String,Int) layout: the n KEYS are the
        // first n slots (`@ 12 + i*8`), DEEP-COPIED + owned by the map (`__skv_store_key` store_str);
        // the n VALUES are the next n slots, stored RAW (`store64`) — NOT owned by the map (the proper
        // owned-value `Map[String,String]` self-host is a separate brick, docs/roadmap v1-records-svg).
        // So the drop frees ONLY the owned key copies (rc_dec the first n slots) — freeing the borrowed
        // values would DOUBLE-FREE. (`n = load32(h+4)` is the entry count.)
        out.push_str(
            "fn __drop_map_ss(m: Map[String, String]) -> Unit = {\n  \
               let h = prim.handle(m)\n  \
               if prim.load32(h + 0) == 1 then __drop_map_ss_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_map_ss_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { prim.rc_dec(prim.load64(h + 12 + i * 8))\n         __drop_map_ss_loop(h, n, i + 1) }\n",
        );
    }
    if need_matrix {
        // The v1 Matrix free: at the block's last ref, `rc_dec` each owned flat row
        // (slot i64-widened handles @12 + i*8, count @4), then the block — the
        // `__drop_list_str` sweep typed over Matrix.
        out.push_str(
            "fn __drop_matrix(m: Matrix) -> Unit = {\n  \
               let h = prim.handle(m)\n  \
               if prim.load32(h + 0) == 1 then __drop_matrix_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_matrix_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { prim.rc_dec(prim.load64(h + 12 + i * 8))\n         __drop_matrix_loop(h, n, i + 1) }\n",
        );
    }
    if need_list_matrix {
        // A `List[Matrix]` field: each element recurses through `__drop_matrix`, then
        // the list block — the two-level sweep `DropListListStr` performs for values.
        out.push_str(
            "fn __drop_list_matrix(xs: List[Matrix]) -> Unit = {\n  \
               let h = prim.handle(xs)\n  \
               if prim.load32(h + 0) == 1 then __drop_list_matrix_loop(h, prim.load32(h + 4), 0) else ()\n  \
               prim.rc_dec(h)\n}\n\
             fn __drop_list_matrix_loop(h: Int, n: Int, i: Int) -> Unit =\n  \
               if i >= n then ()\n  \
               else { let e: Matrix = prim.load_handle(h + 12 + i * 8)\n         __drop_matrix(e)\n         __drop_list_matrix_loop(h, n, i + 1) }\n",
        );
    }
    // `__drop_list_str` itself is no longer emitted HERE — it is now a SHARED source
    // block (`LIST_STR_DROP_SRC`) the pipeline injects once, gated by
    // `program_uses_list_str_drop_field` — the generated variant-drop generator ALSO
    // references this same fn name for its own `List[String]` ctor fields, and two
    // independent inline copies would be a duplicate-fn compile error. `need_list_str`
    // is still computed above (by `record_drop_field_frees`) purely to preserve that
    // function's shared signature with the anon-record caller; its value is unused here.
    let _ = need_list_str;
    out
}
