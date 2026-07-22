
/// The NAMED-RECORD half of [`generate_variant_repr_sources`]: `__repr_rec_<R>` +
/// the `__repr_list_rec_<R>` element loops, appended to `out`. Verbatim text move.
fn generate_record_repr_sources_into(
    out: &mut String,
    type_decls: &[almide_ir::IrTypeDecl],
    interp_anon_recs: &[Vec<(almide_lang::intern::Sym, Ty)>],
    interp_containers: &InterpReprContainers,
    names: &std::collections::HashSet<String>,
    emittable: &std::collections::HashSet<String>,
) {
    use almide_ir::IrTypeDeclKind;
    // ── NAMED-RECORD reprs (`__repr_rec_<R>` + the `__repr_list_rec_<R>` element loop) ──
    // The record sibling: `Node {{ val: 1, kids: [Node {{ … }}] }}` (v0's brace Display,
    // declared field order). The record fixpoint admits Int/Bool/String fields, an emittable
    // nested variant/record, and `List[<emittable record>]` (the recursion that makes the
    // compound_repr recursive/mutually-recursive shapes renderable). Fields at slot_offset(i)
    // (records carry NO tag). Anonymous records stay unhandled (compound.to_string wall).
    let record_decls: Vec<(&str, Vec<(String, Ty)>)> = type_decls
        .iter()
        .filter_map(|d| match &d.kind {
            IrTypeDeclKind::Record { fields } => Some((
                d.name.as_str(),
                fields
                    .iter()
                    .map(|f| (f.name.as_str().to_string(), f.ty.clone()))
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    let rec_names: std::collections::HashSet<String> =
        record_decls.iter().map(|(n, _)| n.to_string()).collect();
    let record_field_of = |ty: &Ty| -> Option<String> {
        match ty {
            Ty::Named(n, _) if rec_names.contains(n.as_str()) => Some(n.as_str().to_string()),
            _ => None,
        }
    };
    let list_record_field_of = |ty: &Ty| -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                match &a[0] {
                    Ty::Named(n, _) if rec_names.contains(n.as_str()) => {
                        Some(n.as_str().to_string())
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    };
    let mut rec_emittable: std::collections::HashSet<String> = rec_names.clone();
    loop {
        let mut removed = false;
        for (tname, fields) in &record_decls {
            if !rec_emittable.contains(*tname) {
                continue;
            }
            let ok = fields.iter().all(|(_, ty)| {
                repr_int_field(ty)
                    || matches!(ty, Ty::Bool | Ty::String)
                    // A `Value` field (`Tool { params: Value }` — C-060) renders via
                    // value_core's JSON serializer (`value_stringify`, the SAME routine
                    // native's `almide_rt_value_stringify` mirrors).
                    || crate::lower::is_value_ty(ty)
                    || variant_field_name(ty, &names)
                        .map(|fv| emittable.contains(&fv))
                        .unwrap_or(false)
                    || record_field_of(ty).map(|r| rec_emittable.contains(&r)).unwrap_or(false)
                    || list_record_field_of(ty)
                        .map(|r| rec_emittable.contains(&r))
                        .unwrap_or(false)
            });
            if !ok {
                rec_emittable.remove(*tname);
                removed = true;
            }
        }
        if !removed {
            break;
        }
    }
    let mut rec_sorted: Vec<&(&str, Vec<(String, Ty)>)> = record_decls
        .iter()
        .filter(|(n, _)| rec_emittable.contains(*n))
        .collect();
    rec_sorted.sort_by_key(|(n, _)| *n);
    // Which emittable records need the LIST loop (referenced as a List[R] field anywhere)?
    let mut need_list: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (n, fields) in &record_decls {
        if !rec_emittable.contains(*n) {
            continue;
        }
        for (_, ty) in fields {
            if let Some(r) = list_record_field_of(ty) {
                if rec_emittable.contains(&r) {
                    need_list.insert(r);
                }
            }
        }
    }
    // ...or referenced as a `${List[R]}` INTERP PART anywhere (compound_repr_records'
    // `points=${pts}` — the container display composes the same element loop).
    for r in &interp_containers.rec_lists {
        if rec_emittable.contains(r) {
            need_list.insert(r.clone());
        }
    }
    for (tname, fields) in rec_sorted.iter() {
        let fname = drop_fn_ident(tname);
        out.push_str(&format!("fn __repr_rec_{fname}(e: {tname}) -> String = {{
"));
        out.push_str("  let h = prim.handle(e)
");
        let mut concat = format!("\"{tname} {{ \"");
        for (i, (fld, ty)) in fields.iter().enumerate() {
            let off = layout::slot_offset(i);
            if i > 0 {
                concat.push_str(" + \", \"");
            }
            concat.push_str(&format!(" + \"{fld}: \""));
            match ty {
                t if repr_int_field(t) => out.push_str(&format!(
                    "  let f{i} = int.to_string(prim.load64(h + {off}))
"
                )),
                Ty::Bool => out.push_str(&format!(
                    "  let f{i} = if prim.load64(h + {off}) == 1 then \"true\" else \"false\"
"
                )),
                Ty::String => out.push_str(&format!(
                    "  let f{i} = __repr_quote(prim.load_str(h + {off}))
"
                )),
                // A `Value` field — borrow the handle and JSON-serialize (C-060).
                t if crate::lower::is_value_ty(t) => out.push_str(&format!(
                    "  let v{i}: Value = prim.load_handle(h + {off})
  let f{i} = value_stringify(v{i})
"
                )),
                _ => {
                    if let Some(fv) = variant_field_name(ty, &names) {
                        let fv_fn = drop_fn_ident(&fv);
                        out.push_str(&format!(
                            "  let v{i}: {fv} = prim.load_handle(h + {off})
  let f{i} = __repr_{fv_fn}(v{i})
"
                        ));
                    } else if let Some(r) = record_field_of(ty) {
                        let r_fn = drop_fn_ident(&r);
                        out.push_str(&format!(
                            "  let v{i}: {r} = prim.load_handle(h + {off})
  let f{i} = __repr_rec_{r_fn}(v{i})
"
                        ));
                    } else {
                        let r = list_record_field_of(ty).expect("fixpoint-admitted");
                        let r_fn = drop_fn_ident(&r);
                        out.push_str(&format!(
                            "  let v{i}: List[{r}] = prim.load_handle(h + {off})
  let f{i} = __repr_list_rec_{r_fn}(v{i})
"
                        ));
                    }
                }
            }
            concat.push_str(&format!(" + f{i}"));
        }
        concat.push_str(" + \" }\"");
        out.push_str(&format!("  {concat}
}}
"));
    }
    for r in &need_list {
        let r_fn = drop_fn_ident(r);
        out.push_str(&format!(
            "fn __repr_list_rec_{r_fn}_go(h: Int, n: Int, i: Int, acc: String) -> String =
                 if i >= n then acc + \"]\"
                 else {{
                     let v: {r} = prim.load_handle(h + 12 + i * 8)
                     let s = __repr_rec_{r_fn}(v)
                     let acc2 = if i == 0 then acc + s else acc + \", \" + s
                     __repr_list_rec_{r_fn}_go(h, n, i + 1, acc2)
  }}
             fn __repr_list_rec_{r_fn}(xs: List[{r}]) -> String = {{
                 let h = prim.handle(xs)
                 let n = prim.load32(h + 4)
                 __repr_list_rec_{r_fn}_go(h, n, 0, \"[\")
}}
"
        ));
    }
    // ── `${Option[<record>]}` interp parts (`opt_rec=${op}`) — `some(<repr>)` / `none` ──
    for r in &interp_containers.rec_opts {
        if !rec_emittable.contains(r) {
            continue;
        }
        let r_fn = drop_fn_ident(r);
        out.push_str(&format!(
            "fn __repr_opt_rec_{r_fn}(o: Option[{r}]) -> String = {{
                 let h = prim.handle(o)
                 if prim.load32(h + 4) == 0 then \"none\"
                 else {{
                     let v: {r} = prim.load_handle(h + 12)
                     \"some(\" + __repr_rec_{r_fn}(v) + \")\"
  }}
}}
"
        ));
    }
    // ── `${Option[<variant>]}` interp parts (`opt_tree=${opt_tree}` — C-009) ──
    for v in &interp_containers.var_opts {
        if !emittable.contains(v) {
            continue;
        }
        let v_fn = drop_fn_ident(v);
        out.push_str(&format!(
            "fn __repr_opt_{v_fn}(o: Option[{v}]) -> String = {{
                 let h = prim.handle(o)
                 if prim.load32(h + 4) == 0 then \"none\"
                 else {{
                     let v: {v} = prim.load_handle(h + 12)
                     \"some(\" + __repr_{v_fn}(v) + \")\"
  }}
}}
"
        ));
    }
    // ── `${List[<variant>]}` interp parts (`shapes=${shapes}`) — the variant element loop ──
    for v in &interp_containers.var_lists {
        if !emittable.contains(v) {
            continue;
        }
        let v_fn = drop_fn_ident(v);
        out.push_str(&format!(
            "fn __repr_list_{v_fn}_go(h: Int, n: Int, i: Int, acc: String) -> String =
                 if i >= n then acc + \"]\"
                 else {{
                     let e: {v} = prim.load_handle(h + 12 + i * 8)
                     let s = __repr_{v_fn}(e)
                     let acc2 = if i == 0 then acc + s else acc + \", \" + s
                     __repr_list_{v_fn}_go(h, n, i + 1, acc2)
  }}
             fn __repr_list_{v_fn}(xs: List[{v}]) -> String = {{
                 let h = prim.handle(xs)
                 let n = prim.load32(h + 4)
                 __repr_list_{v_fn}_go(h, n, 0, \"[\")
}}
"
        ));
    }
    // ── `${Map[String, <record>]}` / `${Map[String, <variant>]}` interp parts — the
    // interleaved [k,v,…] paired-slot map (map_str layout: @4 = 2n slots, key@12+i*8,
    // value at the next slot); keys render QUOTED (the map_to_string_ss form), values
    // through the element repr. Empty renders `[:]`.
    let mut map_repr = |elem: &str, elem_call: &str| {
        let e_fn = drop_fn_ident(elem);
        // map_hobj's SPLIT layout: @4 = entry count n; key i @ 12+i*8, value i @ 12+(n+i)*8.
        out.push_str(&format!(
            "fn __repr_map_{e_fn}_go(h: Int, n: Int, i: Int, acc: String) -> String =
                 if i >= n then acc + \"]\"
                 else {{
                     let k = prim.load_str(h + 12 + i * 8)
                     let v: {elem} = prim.load_handle(h + 12 + (n + i) * 8)
                     let piece = __repr_quote(k) + \": \" + {elem_call}(v)
                     let acc2 = if i == 0 then acc + piece else acc + \", \" + piece
                     __repr_map_{e_fn}_go(h, n, i + 1, acc2)
  }}
             fn __repr_map_{e_fn}(m: Map[String, {elem}]) -> String = {{
                 let h = prim.handle(m)
                 let n = prim.load32(h + 4)
                 if n == 0 then \"[:]\" else __repr_map_{e_fn}_go(h, n, 0, \"[\")
}}
"
        ));
    };
    for r in &interp_containers.rec_maps {
        if rec_emittable.contains(r) {
            map_repr(r, &format!("__repr_rec_{}", drop_fn_ident(r)));
        }
    }
    for v in &interp_containers.var_maps {
        if emittable.contains(v) {
            map_repr(v, &format!("__repr_{}", drop_fn_ident(v)));
        }
    }
    // ── ANONYMOUS-record reprs (`__repr_anonrec_<hash>`) ──
    // v0 renders an anon record `{ apple: 2, mango: 3, zebra: 1 }` with fields SORTED BY
    // NAME while the v1 BLOCK lays fields in SOURCE order — so each generated body reads
    // slots at the SOURCE index but concatenates in sorted-name order. No type-name prefix.
    // Scalar/String fields only (a nested payload keeps the compound.to_string wall).
    //
    // NOMINAL resolution (#627/C-072): an INFERRED record literal keeps its STRUCTURAL
    // type, but native reprs it NOMINALLY when its sorted field-NAME set matches exactly
    // ONE declared record — `{ zeta: 1, alpha: 2, mid: 3 }` prints `Rec { zeta: 1,
    // alpha: 2, mid: 3 }` in DECLARATION order. Mirror that here: the body still reads
    // each field at its SOURCE slot (the block layout), only the PREFIX and the print
    // ORDER switch to the declaration's. No match (or an ambiguous one) keeps the
    // sorted anonymous render.
    let nominal_decls: Vec<(String, Vec<String>)> = type_decls
        .iter()
        .filter_map(|d| match &d.kind {
            IrTypeDeclKind::Record { fields }
                if d.generics.as_ref().map_or(true, |g| g.is_empty()) =>
            {
                Some((
                    d.name.as_str().to_string(),
                    fields.iter().map(|f| f.name.as_str().to_string()).collect(),
                ))
            }
            _ => None,
        })
        .collect();
    let resolve_nominal = |fields: &[(almide_lang::intern::Sym, Ty)]| -> Option<(String, Vec<String>)> {
        let mut key: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
        key.sort_unstable();
        let mut found: Option<&(String, Vec<String>)> = None;
        for decl in &nominal_decls {
            if decl.1.len() != key.len() {
                continue;
            }
            let mut dk: Vec<&str> = decl.1.iter().map(|s| s.as_str()).collect();
            dk.sort_unstable();
            if dk == key {
                if found.is_some() {
                    return None; // ambiguous — keep the anonymous render
                }
                found = Some(decl);
            }
        }
        found.cloned()
    };
    let mut anon_sorted: Vec<&Vec<(almide_lang::intern::Sym, Ty)>> =
        interp_anon_recs.iter().collect();
    anon_sorted.sort_by_key(|f| anon_record_drop_name(f));
    anon_sorted.dedup_by_key(|f| anon_record_drop_name(f));
    for fields in anon_sorted {
        if fields.is_empty()
            || !fields
                .iter()
                .all(|(_, ty)| repr_int_field(ty) || matches!(ty, Ty::Bool | Ty::String))
        {
            continue;
        }
        let name = anon_record_drop_name(fields);
        let param_ty = anon_record_source_ty(fields);
        out.push_str(&format!("fn __repr_{name}(e: {param_ty}) -> String = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        for (i, (_, ty)) in fields.iter().enumerate() {
            let off = layout::slot_offset(i);
            match ty {
                t if repr_int_field(t) => out.push_str(&format!(
                    "  let f{i} = int.to_string(prim.load64(h + {off}))\n"
                )),
                Ty::Bool => out.push_str(&format!(
                    "  let f{i} = if prim.load64(h + {off}) == 1 then \"true\" else \"false\"\n"
                )),
                _ => out.push_str(&format!(
                    "  let f{i} = __repr_quote(prim.load_str(h + {off}))\n"
                )),
            }
        }
        // Print order + prefix: DECLARED (nominal) when the shape resolves, sorted
        // anonymous otherwise.
        let (prefix, order): (String, Vec<usize>) = match resolve_nominal(fields) {
            Some((decl_name, decl_order)) => (
                format!("{decl_name} {{ "),
                decl_order
                    .iter()
                    .map(|dn| fields.iter().position(|(n, _)| n.as_str() == dn.as_str()).unwrap())
                    .collect(),
            ),
            None => {
                let mut order: Vec<usize> = (0..fields.len()).collect();
                order.sort_by_key(|&i| fields[i].0.as_str());
                ("{ ".to_string(), order)
            }
        };
        let mut concat = format!("\"{prefix}\"");
        for (k, &i) in order.iter().enumerate() {
            if k > 0 {
                concat.push_str(" + \", \"");
            }
            concat.push_str(&format!(" + \"{}: \" + f{i}", fields[i].0.as_str()));
        }
        concat.push_str(" + \" }\"");
        out.push_str(&format!("  {concat}\n}}\n"));
        // The `${[a]}` LIST walker over this element shape (`__repr_list_anonrec_<h>`):
        // borrowed element handles, each through the element repr above, joined `, `
        // in `[` … `]`. Emitted for every admitted shape (a dead walker is inert).
        out.push_str(&format!(
            "fn __repr_list_{name}_go(hh: Int, n: Int, i: Int, acc: String) -> String =\n  \
               if i >= n then acc\n  \
               else {{\n    \
                 let e: {param_ty} = prim.load_handle(hh + 12 + i * 8)\n    \
                 let s = __repr_{name}(e)\n    \
                 let acc2 = if i == 0 then acc + s else acc + \", \" + s\n    \
                 __repr_list_{name}_go(hh, n, i + 1, acc2)\n  }}\n\
             fn __repr_list_{name}(xs: List[{param_ty}]) -> String = {{\n  \
               let hh = prim.handle(xs)\n  \
               \"[\" + __repr_list_{name}_go(hh, prim.load32(hh + 4), 0, \"\") + \"]\"\n}}\n"
        ));
    }

    // ── SCALAR-component tuple CONTAINER reprs (`__repr_tup_<key>` + walkers) ──
    // `${list.sort(tups)}` over `List[(Int, String)]` / `${list.min(tups)}` over
    // `Option[(Bool, Bool)]` (the list_total_order C-053 class). The tuple block is
    // uniform i64 slots (component i at 12 + 8i); components render Int/Bool/String
    // only (the collector's tuple_repr_ident gate). The bare element repr is shared;
    // the list walker borrows element handles; the option walker reads the len-tag
    // (`none` at 0) and the borrowed payload handle at slot 0.
    let tup_spelling = |ts: &[Ty]| -> String {
        let one = |t: &Ty| match t {
            Ty::Int => "Int",
            Ty::Bool => "Bool",
            _ => "String",
        };
        format!("({})", ts.iter().map(one).collect::<Vec<_>>().join(", "))
    };
    let mut all_tups: Vec<&Vec<Ty>> =
        interp_containers.tup_lists.iter().chain(interp_containers.tup_opts.iter()).collect();
    all_tups.sort_by_key(|ts| tuple_repr_ident(ts));
    all_tups.dedup_by_key(|ts| tuple_repr_ident(ts));
    for ts in &all_tups {
        let Some(key) = tuple_repr_ident(ts) else { continue };
        let spell = tup_spelling(ts);
        out.push_str(&format!("fn __repr_tup_{key}(e: {spell}) -> String = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        for (i, t) in ts.iter().enumerate() {
            let off = layout::slot_offset(i);
            match t {
                Ty::Int => out.push_str(&format!(
                    "  let f{i} = int.to_string(prim.load64(h + {off}))\n"
                )),
                Ty::Bool => out.push_str(&format!(
                    "  let f{i} = if prim.load64(h + {off}) == 1 then \"true\" else \"false\"\n"
                )),
                _ => out.push_str(&format!(
                    "  let f{i} = __repr_quote(prim.load_str(h + {off}))\n"
                )),
            }
        }
        let body = (0..ts.len()).map(|i| format!("f{i}")).collect::<Vec<_>>().join(" + \", \" + ");
        out.push_str(&format!("  \"(\" + {body} + \")\"\n}}\n"));
    }
    for ts in &interp_containers.tup_lists {
        let Some(key) = tuple_repr_ident(ts) else { continue };
        let spell = tup_spelling(ts);
        out.push_str(&format!(
            "fn __repr_list_tup_{key}_go(hh: Int, n: Int, i: Int, acc: String) -> String =\n  \
               if i >= n then acc\n  \
               else {{\n    \
                 let e: {spell} = prim.load_handle(hh + 12 + i * 8)\n    \
                 let s = __repr_tup_{key}(e)\n    \
                 let acc2 = if i == 0 then acc + s else acc + \", \" + s\n    \
                 __repr_list_tup_{key}_go(hh, n, i + 1, acc2)\n  }}\n\
             fn __repr_list_tup_{key}(xs: List[{spell}]) -> String = {{\n  \
               let hh = prim.handle(xs)\n  \
               \"[\" + __repr_list_tup_{key}_go(hh, prim.load32(hh + 4), 0, \"\") + \"]\"\n}}\n"
        ));
    }
    for ts in &interp_containers.tup_opts {
        let Some(key) = tuple_repr_ident(ts) else { continue };
        let spell = tup_spelling(ts);
        out.push_str(&format!(
            "fn __repr_opt_tup_{key}(o: Option[{spell}]) -> String = {{\n  \
               let h = prim.handle(o)\n  \
               if prim.load32(h + 4) == 0 then \"none\"\n  \
               else {{\n    \
                 let e: {spell} = prim.load_handle(h + 12)\n    \
                 \"some(\" + __repr_tup_{key}(e) + \")\"\n  }}\n}}\n"
        ));
    }

    // ── `${obj}` over a bare `Value` (C-060) ── the interp routes to `__repr_Value`;
    // it is value_core's JSON serializer verbatim (native's almide_rt_value_stringify
    // twin, so the two targets share one text).
    if interp_containers.value_parts {
        out.push_str("fn __repr_Value(v: Value) -> String = value_stringify(v)\n");
    }
}
