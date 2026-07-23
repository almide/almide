
pub fn generate_variant_repr_sources(
    type_decls: &[almide_ir::IrTypeDecl],
    interp_anon_recs: &[Vec<(almide_lang::intern::Sym, Ty)>],
    interp_containers: &InterpReprContainers,
) -> String {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    let names = variant_type_names(type_decls);
    // Records whose every field is Int/Bool/String — admissible as a VARIANT ctor repr field
    // (`Label { at: Point }`): the record section below emits `__repr_rec_<R>` for them
    // unconditionally (they trivially pass its fixpoint), so the variant body's call links.
    // Computed BEFORE the variant fixpoint to break the variant↔record cycle one-directionally.
    let scalar_rec_names: std::collections::HashSet<String> = type_decls
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
        .collect();
    // Fixpoint: which variants are repr-EMITTABLE (every ctor field Int/Bool/String
    // or an emittable variant)?
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
                let tys: Vec<Ty> = match &case.kind {
                    IrVariantKind::Unit => vec![],
                    IrVariantKind::Tuple { fields } => fields.clone(),
                    IrVariantKind::Record { fields } => {
                        fields.iter().map(|f| f.ty.clone()).collect()
                    }
                };
                tys.iter().all(|ty| {
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
                        || variant_field_name(ty, &names)
                            .map(|fv| emittable.contains(&fv))
                            .unwrap_or(false)
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
    // Records also emit through this generator (the section below) — only bail when
    // NEITHER kind has an emittable member.
    let any_record = type_decls.iter().any(|d| matches!(&d.kind, IrTypeDeclKind::Record { .. }));
    if emittable.is_empty()
        && !any_record
        && interp_anon_recs.is_empty()
        && interp_containers.tup_lists.is_empty()
        && interp_containers.tup_opts.is_empty()
        && !interp_containers.value_parts
        // A GENERIC-variant program can have ZERO bare-emittable variants (every
        // field a type param) yet need its INSTANTIATION-keyed reprs (`Tree[T]`
        // used only as Tree[Int]/Tree[String] — the recursive-generic C-010 class).
        && interp_containers.var_insts.is_empty()
    {
        return String::new();
    }
    // The shared QUOTE helper (v0's escape set: \" \\ \n \r \t).
    let mut out = String::from(
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
    );
    // The FLOAT display helper links the Dragon4 float.to_string module — emit it ONLY
    // when an emitted variant actually has a Float ctor field (unconditional emission
    // linked Dragon4 into every program and its internal certs into every cert check).
    let need_float = type_decls.iter().any(|d| {
        let IrTypeDeclKind::Variant { cases, .. } = &d.kind else { return false };
        emittable.contains(d.name.as_str())
            && cases.iter().any(|case| {
                let tys: Vec<Ty> = match &case.kind {
                    IrVariantKind::Unit => vec![],
                    IrVariantKind::Tuple { fields } => fields.clone(),
                    IrVariantKind::Record { fields } => {
                        fields.iter().map(|f| f.ty.clone()).collect()
                    }
                };
                tys.iter().any(|t| matches!(t, Ty::Float))
            })
    });
    if need_float {
        emit_float_helpers(&mut out);
    }
    // Instantiation-keyed repr bookkeeping (filled by the inst loop below): which
    // (name, args) actually EMITTED (walkers gate on it), and whether any inst
    // field needs the Float display helpers.
    let mut emitted_insts: Vec<(String, Vec<Ty>, String)> = Vec::new();
    let mut inst_needs_float = false;
    let mut inst_needs_list_int = false;
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
        emit_variant_repr_body(&mut out, &fname, tname, &flat, &scalar_rec_names, &names);
    }
    // ── GENERIC-variant INSTANTIATION reprs (`__repr_ReprEither_Int_String`) ──
    // A `${l}` over `ReprEither[Int, String]` calls the INSTANTIATION-KEYED repr
    // (the interp call site derives the same key via `repr_inst_ident`): the
    // decl's type-param fields (bare `Named(L, [])` — the frontend's spelling of
    // an uninstantiated param) are substituted with the use-site args and the
    // body emitted like any variant. SCALAR/String args + fields only in this
    // brick (a nested/heap payload keeps the honest unlinked wall). Sorted +
    // deduped by key for host-determinism.
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
        // Admissibility: every INSTANTIATED field a plain int/Bool/String/Float leaf,
        // a `List[Int]` payload (the `Tree[List[Int]]` shape — rendered via
        // list.to_string), or an EXACT SELF-reference (`Node(Tree[T], Tree[T])` after
        // substitution — the recursive-generic C-010 class; the body recurses through
        // the SAME instantiation-keyed fn, terminating on the finite value). Anything
        // else keeps the honest unlinked wall.
        let self_ref = |t: &Ty| {
            matches!(t, Ty::Named(n, a) if n.as_str() == iname.as_str() && a == iargs)
        };
        if !flat.iter().all(|(_, fs)| {
            fs.iter().all(|(_, t)| {
                repr_int_field(t)
                    || matches!(t, Ty::Bool | Ty::String | Ty::Float)
                    || matches!(t,
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                            if a.len() == 1 && matches!(a[0], Ty::Int))
                    || self_ref(t)
            })
        }) {
            continue;
        }
        if flat.iter().any(|(_, fs)| fs.iter().any(|(_, t)| matches!(t, Ty::Float))) {
            inst_needs_float = true;
        }
        if flat.iter().any(|(_, fs)| {
            fs.iter().any(|(_, t)| matches!(t,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 && matches!(a[0], Ty::Int)))
        }) {
            inst_needs_list_int = true;
        }
        let key = repr_inst_ident(iname, iargs);
        let tspell = format!("{}[{}]", iname, spells.join(", "));
        emit_variant_repr_body(&mut out, &key, &tspell, &flat, &scalar_rec_names, &names);
        emitted_insts.push((iname.clone(), iargs.clone(), tspell));
    }
    // The instantiation-keyed CONTAINER walkers (`${forest}` over `List[Tree[Int]]`,
    // `${opt}` over `Option[Tree[String]]`) — same loops as the non-generic variant
    // walkers, keyed + typed by the instantiation.
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
    // A VARIANT-PAYLOAD anonymous record (`Circle({ r: Int })` — #628/C-079) needs its
    // `__repr_anonrec_<hash>` even when no interp part carries the bare shape: the
    // emitted variant body above calls it. Extend the interp-collected shapes with
    // every all-scalar Record payload of an EMITTED variant (dedup by hash below).
    let mut all_anon_recs: Vec<Vec<(almide_lang::intern::Sym, Ty)>> = interp_anon_recs.to_vec();
    for decl in type_decls {
        let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else { continue };
        if !emittable.contains(decl.name.as_str()) {
            continue;
        }
        for case in cases {
            let tys: Vec<Ty> = match &case.kind {
                IrVariantKind::Unit => vec![],
                IrVariantKind::Tuple { fields } => fields.clone(),
                IrVariantKind::Record { fields } => fields.iter().map(|f| f.ty.clone()).collect(),
            };
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
