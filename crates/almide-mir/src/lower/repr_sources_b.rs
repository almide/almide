
pub fn generate_variant_repr_sources(
    type_decls: &[almide_ir::IrTypeDecl],
    interp_anon_recs: &[Vec<(almide_lang::intern::Sym, Ty)>],
    interp_containers: &InterpReprContainers,
) -> String {
    // Sequential-phase split (codopsy round-2 sweep #852): each phase below is a verbatim
    // text move out of this function — same computations, same emission order,
    // byte-identical output.
    let names = variant_type_names(type_decls);
    let scalar_rec_names = repr_scalar_record_names(type_decls);
    let emittable = repr_emittable_variant_fixpoint(type_decls, &scalar_rec_names, &names);
    if repr_sources_have_no_emitters(&emittable, type_decls, interp_anon_recs, interp_containers)
    {
        return String::new();
    }
    let mut out = repr_quote_helper_source();
    let need_float = variant_needs_float_display(type_decls, &emittable);
    if need_float {
        emit_float_helpers(&mut out);
    }
    emit_emittable_variant_bodies(&mut out, type_decls, &emittable, &scalar_rec_names, &names);
    let (emitted_insts, inst_needs_float, inst_needs_list_int) =
        emit_variant_inst_reprs(&mut out, type_decls, interp_containers, &scalar_rec_names, &names);
    emit_inst_container_walkers(&mut out, interp_containers, &emitted_insts);
    if inst_needs_float && !need_float {
        emit_float_helpers(&mut out);
    }
    if inst_needs_list_int {
        out.push_str(
            "fn __repr_li_go(h: Int, n: Int, i: Int, acc: String) -> String =\n  \
               if i >= n then acc + \"]\"\n  \
               else {\n    \
                 let s = int.to_string(prim.load64(h + 12 + i * 8))\n    \
                 let acc2 = if i == 0 then acc + s else acc + \", \" + s\n    \
                 __repr_li_go(h, n, i + 1, acc2)\n  }\n\
             fn __repr_list_int(v: List[Int]) -> String = {\n  \
               let h = prim.handle(v)\n  \
               __repr_li_go(h, prim.load32(h + 4), 0, \"[\")\n}\n",
        );
    }
    // Decomposed (#781, cog 137): the NAMED-RECORD repr generation is a verbatim
    // text move into `generate_record_repr_sources_into`.
    let all_anon_recs = collect_variant_payload_anon_recs(type_decls, &emittable, interp_anon_recs);
    generate_record_repr_sources_into(
        &mut out,
        type_decls,
        &all_anon_recs,
        interp_containers,
        &names,
        &emittable,
    );

    out
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852): the OWNED
/// ctor-payload type row (Unit/Tuple/Record → `Vec<Ty>`) that the emittability fixpoint, the
/// Float-display gate, and the variant-payload anon-record collector each repeated verbatim —
/// one copy keeps the three scans reading the SAME field list. Distinct from mod_b.rs's
/// borrowing `variant_case_field_tys` (`Vec<&Ty>`): these call sites clone.
fn variant_case_owned_field_tys(case: &almide_ir::IrVariantDecl) -> Vec<Ty> {
    use almide_ir::IrVariantKind;
    match &case.kind {
        IrVariantKind::Unit => vec![],
        IrVariantKind::Tuple { fields } => fields.clone(),
        IrVariantKind::Record { fields } => fields.iter().map(|f| f.ty.clone()).collect(),
    }
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852, phase 1 of 9).
/// Records whose every field is Int/Bool/String — admissible as a VARIANT ctor repr field
/// (`Label { at: Point }`): the record section below emits `__repr_rec_<R>` for them
/// unconditionally (they trivially pass its fixpoint), so the variant body's call links.
/// Computed BEFORE the variant fixpoint to break the variant↔record cycle one-directionally.
fn repr_scalar_record_names(
    type_decls: &[almide_ir::IrTypeDecl],
) -> std::collections::HashSet<String> {
    use almide_ir::IrTypeDeclKind;
    type_decls
        .iter()
        .filter_map(|d| match &d.kind {
            IrTypeDeclKind::Record { fields }
                if fields
                    .iter()
                    .all(|f| repr_int_field(&f.ty) || matches!(f.ty, Ty::Bool | Ty::String)) =>
            {
                Some(d.name.as_str().to_string())
            }
            _ => None,
        })
        .collect()
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852): the per-field
/// decision inside the emittability fixpoint — is this ctor field type renderable by the
/// generated repr body (scalar leaf, Float, scalar record, all-scalar anonymous record, or an
/// emittable variant)?
fn variant_ctor_field_repr_admissible(
    ty: &Ty,
    scalar_rec_names: &std::collections::HashSet<String>,
    names: &std::collections::HashSet<String>,
    emittable: &std::collections::HashSet<String>,
) -> bool {
    repr_int_field(ty)
        || matches!(ty, Ty::Bool | Ty::String)
        // A Float ctor field renders via the compound Display
        // (`float.to_string_compound` — integral drops the `.0`).
        || matches!(ty, Ty::Float)
        // A SCALAR-record ctor field (`Label { at: Point }`) renders via the
        // record section's unconditional `__repr_rec_<R>`.
        || matches!(ty, Ty::Named(n, _) if scalar_rec_names.contains(n.as_str()))
        // An ANONYMOUS-record payload (`Circle({ r: Int })` — #628/C-079)
        // renders via its `__repr_anonrec_<hash>` (emitted for every
        // variant-payload shape by the record half).
        || matches!(ty, Ty::Record { fields }
            if fields.iter().all(|(_, t)|
                repr_int_field(t) || matches!(t, Ty::Bool | Ty::String)))
        || variant_field_name(ty, names)
            .map(|fv| emittable.contains(&fv))
            .unwrap_or(false)
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852, phase 2 of 9).
/// Fixpoint: which variants are repr-EMITTABLE (every ctor field Int/Bool/String
/// or an emittable variant)?
fn repr_emittable_variant_fixpoint(
    type_decls: &[almide_ir::IrTypeDecl],
    scalar_rec_names: &std::collections::HashSet<String>,
    names: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    use almide_ir::IrTypeDeclKind;
    let mut emittable: std::collections::HashSet<String> = type_decls
        .iter()
        .filter(|d| matches!(&d.kind, IrTypeDeclKind::Variant { .. }))
        .map(|d| d.name.as_str().to_string())
        .collect();
    loop {
        let mut removed = false;
        for decl in type_decls {
            let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else { continue };
            let tname = decl.name.as_str();
            if !emittable.contains(tname) {
                continue;
            }
            let ok = cases.iter().all(|case| {
                let tys: Vec<Ty> = variant_case_owned_field_tys(case);
                tys.iter().all(|ty| {
                    variant_ctor_field_repr_admissible(ty, scalar_rec_names, names, &emittable)
                })
            });
            if !ok {
                emittable.remove(tname);
                removed = true;
            }
        }
        if !removed {
            break;
        }
    }
    emittable
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852, phase 3 of 9):
/// the empty-output bail decision.
/// Records also emit through this generator (the section below) — only bail when
/// NEITHER kind has an emittable member.
fn repr_sources_have_no_emitters(
    emittable: &std::collections::HashSet<String>,
    type_decls: &[almide_ir::IrTypeDecl],
    interp_anon_recs: &[Vec<(almide_lang::intern::Sym, Ty)>],
    interp_containers: &InterpReprContainers,
) -> bool {
    use almide_ir::IrTypeDeclKind;
    let any_record = type_decls.iter().any(|d| matches!(&d.kind, IrTypeDeclKind::Record { .. }));
    emittable.is_empty()
        && !any_record
        && interp_anon_recs.is_empty()
        && interp_containers.tup_lists.is_empty()
        && interp_containers.tup_opts.is_empty()
        && !interp_containers.value_parts
        // A GENERIC-variant program can have ZERO bare-emittable variants (every
        // field a type param) yet need its INSTANTIATION-keyed reprs (`Tree[T]`
        // used only as Tree[Int]/Tree[String] — the recursive-generic C-010 class).
        && interp_containers.var_insts.is_empty()
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852, phase 4 of 9):
/// the fixed preamble every non-empty output starts from.
fn repr_quote_helper_source() -> String {
    // The shared QUOTE helper (v0's escape set: \" \\ \n \r \t).
    String::from(
        "fn __repr_is_escaped(b: Int) -> Bool = b == 34 or b == 92 or b == 10 or b == 13 or b == 9\n\
         fn __repr_esc_len(src: Int, slen: Int, i: Int, acc: Int) -> Int =\n  \
           if i >= slen then acc\n  \
           else {\n    let b = prim.load8(src + i)\n    let w = if __repr_is_escaped(b) then 2 else 1\n    __repr_esc_len(src, slen, i + 1, acc + w)\n  }\n\
         fn __repr_esc_char(b: Int) -> Int =\n  \
           if b == 10 then 110\n  \
           else if b == 13 then 114\n  \
           else if b == 9 then 116\n  \
           else b\n\
         fn __repr_fill_esc(src: Int, slen: Int, i: Int, pos: Int) -> Int =\n  \
           if i >= slen then pos\n  \
           else {\n    let b = prim.load8(src + i)\n    \
             let pos1 = if __repr_is_escaped(b) then {\n      prim.store8(pos, 92)\n      prim.store8(pos + 1, __repr_esc_char(b))\n      pos + 2\n    } else {\n      prim.store8(pos, b)\n      pos + 1\n    }\n    \
             __repr_fill_esc(src, slen, i + 1, pos1)\n  }\n\
         fn __repr_quote(s: String) -> String = {\n  \
           let h = prim.handle(s)\n  \
           let n = prim.load32(h + 4)\n  \
           let elen = __repr_esc_len(h + 12, n, 0, 0)\n  \
           let out = prim.alloc_str(elen + 2)\n  \
           let d = prim.handle(out) + 12\n  \
           prim.store8(d, 34)\n  \
           let e = __repr_fill_esc(h + 12, n, 0, d + 1)\n  \
           prim.store8(e, 34)\n  \
           out\n}\n",
    )
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852, phase 5 of 9).
/// The FLOAT display helper links the Dragon4 float.to_string module — emit it ONLY
/// when an emitted variant actually has a Float ctor field (unconditional emission
/// linked Dragon4 into every program and its internal certs into every cert check).
fn variant_needs_float_display(
    type_decls: &[almide_ir::IrTypeDecl],
    emittable: &std::collections::HashSet<String>,
) -> bool {
    use almide_ir::IrTypeDeclKind;
    type_decls.iter().any(|d| {
        let IrTypeDeclKind::Variant { cases, .. } = &d.kind else { return false };
        emittable.contains(d.name.as_str())
            && cases.iter().any(|case| {
                let tys: Vec<Ty> = variant_case_owned_field_tys(case);
                tys.iter().any(|t| matches!(t, Ty::Float))
            })
    })
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852, phase 6 of 9):
/// one `__repr_<T>` body per emittable variant DECL, name-sorted for host-determinism.
fn emit_emittable_variant_bodies(
    out: &mut String,
    type_decls: &[almide_ir::IrTypeDecl],
    emittable: &std::collections::HashSet<String>,
    scalar_rec_names: &std::collections::HashSet<String>,
    names: &std::collections::HashSet<String>,
) {
    use almide_ir::IrTypeDeclKind;
    let mut sorted: Vec<&almide_ir::IrTypeDecl> = type_decls
        .iter()
        .filter(|d| {
            matches!(&d.kind, IrTypeDeclKind::Variant { .. })
                && emittable.contains(d.name.as_str())
        })
        .collect();
    sorted.sort_by_key(|d| d.name.as_str());
    for decl in sorted {
        let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else { continue };
        let tname = decl.name.as_str();
        let fname = drop_fn_ident(tname);
        let flat = flatten_variant_cases(cases, None);
        emit_variant_repr_body(out, &fname, tname, &flat, scalar_rec_names, names);
    }
}

/// Extracted from `emit_variant_inst_reprs` (codopsy round-2 sweep #852).
/// Admissibility: every INSTANTIATED field a plain int/Bool/String/Float leaf,
/// a `List[Int]` payload (the `Tree[List[Int]]` shape — rendered via
/// list.to_string), or an EXACT SELF-reference (`Node(Tree[T], Tree[T])` after
/// substitution — the recursive-generic C-010 class; the body recurses through
/// the SAME instantiation-keyed fn, terminating on the finite value). Anything
/// else keeps the honest unlinked wall.
fn inst_fields_repr_admissible(
    flat: &[(String, Vec<(Option<String>, Ty)>)],
    iname: &str,
    iargs: &[Ty],
) -> bool {
    let self_ref = |t: &Ty| {
        matches!(t, Ty::Named(n, a) if n.as_str() == iname && a == iargs)
    };
    flat.iter().all(|(_, fs)| {
        fs.iter().all(|(_, t)| {
            repr_int_field(t)
                || matches!(t, Ty::Bool | Ty::String | Ty::Float)
                || matches!(t,
                    Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                        if a.len() == 1 && matches!(a[0], Ty::Int))
                || self_ref(t)
        })
    })
}

/// Extracted from `emit_variant_inst_reprs` (codopsy round-2 sweep #852): does an admitted
/// instantiation carry a Float field (gates the Dragon4 float display helpers)?
fn inst_fields_contain_float(flat: &[(String, Vec<(Option<String>, Ty)>)]) -> bool {
    flat.iter().any(|(_, fs)| fs.iter().any(|(_, t)| matches!(t, Ty::Float)))
}

/// Extracted from `emit_variant_inst_reprs` (codopsy round-2 sweep #852): does an admitted
/// instantiation carry a `List[Int]` field (gates the `__repr_list_int` helper)?
fn inst_fields_contain_list_int(flat: &[(String, Vec<(Option<String>, Ty)>)]) -> bool {
    flat.iter().any(|(_, fs)| {
        fs.iter().any(|(_, t)| matches!(t,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && matches!(a[0], Ty::Int)))
    })
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852, phase 7 of 9).
/// ── GENERIC-variant INSTANTIATION reprs (`__repr_ReprEither_Int_String`) ──
/// A `${l}` over `ReprEither[Int, String]` calls the INSTANTIATION-KEYED repr
/// (the interp call site derives the same key via `repr_inst_ident`): the
/// decl's type-param fields (bare `Named(L, [])` — the frontend's spelling of
/// an uninstantiated param) are substituted with the use-site args and the
/// body emitted like any variant. SCALAR/String args + fields only in this
/// brick (a nested/heap payload keeps the honest unlinked wall). Sorted +
/// deduped by key for host-determinism.
fn emit_variant_inst_reprs(
    out: &mut String,
    type_decls: &[almide_ir::IrTypeDecl],
    interp_containers: &InterpReprContainers,
    scalar_rec_names: &std::collections::HashSet<String>,
    names: &std::collections::HashSet<String>,
) -> (Vec<(String, Vec<Ty>, String)>, bool, bool) {
    use almide_ir::IrTypeDeclKind;
    // Instantiation-keyed repr bookkeeping (filled by the inst loop below): which
    // (name, args) actually EMITTED (walkers gate on it), and whether any inst
    // field needs the Float display helpers.
    let mut emitted_insts: Vec<(String, Vec<Ty>, String)> = Vec::new();
    let mut inst_needs_float = false;
    let mut inst_needs_list_int = false;
    let mut inst_sorted: Vec<&(String, Vec<Ty>)> = interp_containers.var_insts.iter().collect();
    inst_sorted.sort_by_key(|(n, a)| repr_inst_ident(n, a));
    inst_sorted.dedup_by_key(|(n, a)| repr_inst_ident(n, a));
    for (iname, iargs) in inst_sorted {
        let Some(decl) = type_decls.iter().find(|d| {
            d.name.as_str() == iname.as_str() && matches!(&d.kind, IrTypeDeclKind::Variant { .. })
        }) else {
            continue;
        };
        let Some(gps) = decl.generics.as_ref() else { continue };
        if gps.is_empty() || gps.len() != iargs.len() {
            continue;
        }
        let Some(spells) = iargs.iter().map(repr_ty_spelling).collect::<Option<Vec<String>>>()
        else {
            continue;
        };
        let subst: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
            gps.iter().map(|g| g.name).zip(iargs.iter().cloned()).collect();
        let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else { continue };
        let flat = flatten_variant_cases(cases, Some(&subst));
        if !inst_fields_repr_admissible(&flat, iname, iargs) {
            continue;
        }
        if inst_fields_contain_float(&flat) {
            inst_needs_float = true;
        }
        if inst_fields_contain_list_int(&flat) {
            inst_needs_list_int = true;
        }
        let key = repr_inst_ident(iname, iargs);
        let tspell = format!("{}[{}]", iname, spells.join(", "));
        emit_variant_repr_body(out, &key, &tspell, &flat, scalar_rec_names, names);
        emitted_insts.push((iname.clone(), iargs.clone(), tspell));
    }
    (emitted_insts, inst_needs_float, inst_needs_list_int)
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852, phase 8 of 9).
/// The instantiation-keyed CONTAINER walkers (`${forest}` over `List[Tree[Int]]`,
/// `${opt}` over `Option[Tree[String]]`) — same loops as the non-generic variant
/// walkers, keyed + typed by the instantiation.
fn emit_inst_container_walkers(
    out: &mut String,
    interp_containers: &InterpReprContainers,
    emitted_insts: &[(String, Vec<Ty>, String)],
) {
    for (iname, iargs) in &interp_containers.var_inst_lists {
        let Some((_, _, tspell)) = emitted_insts
            .iter()
            .find(|(n, a, _)| n == iname && a == iargs)
        else {
            continue;
        };
        let key = repr_inst_ident(iname, iargs);
        out.push_str(&format!(
            "fn __repr_list_{key}_go(h: Int, n: Int, i: Int, acc: String) -> String =\n  \
               if i >= n then acc + \"]\"\n  \
               else {{\n    \
                 let e: {tspell} = prim.load_handle(h + 12 + i * 8)\n    \
                 let s = __repr_{key}(e)\n    \
                 let acc2 = if i == 0 then acc + s else acc + \", \" + s\n    \
                 __repr_list_{key}_go(h, n, i + 1, acc2)\n  }}\n\
             fn __repr_list_{key}(xs: List[{tspell}]) -> String = {{\n  \
               let h = prim.handle(xs)\n  \
               __repr_list_{key}_go(h, prim.load32(h + 4), 0, \"[\")\n}}\n"
        ));
    }
    for (iname, iargs) in &interp_containers.var_inst_opts {
        let Some((_, _, tspell)) = emitted_insts
            .iter()
            .find(|(n, a, _)| n == iname && a == iargs)
        else {
            continue;
        };
        let key = repr_inst_ident(iname, iargs);
        out.push_str(&format!(
            "fn __repr_opt_{key}(o: Option[{tspell}]) -> String = {{\n  \
               let h = prim.handle(o)\n  \
               if prim.load32(h + 4) == 0 then \"none\"\n  \
               else {{\n    \
                 let v: {tspell} = prim.load_handle(h + 12)\n    \
                 \"some(\" + __repr_{key}(v) + \")\"\n  }}\n}}\n"
        ));
    }
}

/// Extracted from `generate_variant_repr_sources` (codopsy round-2 sweep #852, phase 9 of 9).
/// A VARIANT-PAYLOAD anonymous record (`Circle({ r: Int })` — #628/C-079) needs its
/// `__repr_anonrec_<hash>` even when no interp part carries the bare shape: the
/// emitted variant body above calls it. Extend the interp-collected shapes with
/// every all-scalar Record payload of an EMITTED variant (dedup by hash below).
fn collect_variant_payload_anon_recs(
    type_decls: &[almide_ir::IrTypeDecl],
    emittable: &std::collections::HashSet<String>,
    interp_anon_recs: &[Vec<(almide_lang::intern::Sym, Ty)>],
) -> Vec<Vec<(almide_lang::intern::Sym, Ty)>> {
    use almide_ir::IrTypeDeclKind;
    let mut all_anon_recs: Vec<Vec<(almide_lang::intern::Sym, Ty)>> = interp_anon_recs.to_vec();
    for decl in type_decls {
        let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else { continue };
        if !emittable.contains(decl.name.as_str()) {
            continue;
        }
        for case in cases {
            let tys: Vec<Ty> = variant_case_owned_field_tys(case);
            for ty in tys {
                if let Ty::Record { fields } = &ty {
                    if fields.iter().all(|(_, t)| {
                        repr_int_field(t) || matches!(t, Ty::Bool | Ty::String)
                    }) {
                        all_anon_recs.push(fields.clone());
                    }
                }
            }
        }
    }
    all_anon_recs
}
