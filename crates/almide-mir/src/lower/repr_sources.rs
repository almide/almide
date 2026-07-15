/// Generate the ALMIDE SOURCE for each variant type's REPR `__repr_<V>` (the Display
/// counterpart of [`generate_variant_drop_sources`]) — what a string interpolation
/// `"${e}"` over a custom-ADT value prints, byte-matching v0's compound Display:
/// `Overflow("x")` (String fields QUOTED with the \" \\ \n \r \t escapes),
/// `DivZero` (bare nullary), `Pair(3, true)`. Emitted for every variant whose ctor
/// fields are all Int / Bool / String / another emittable variant (a FIXPOINT — so
/// recursive ADTs like `Node(Tree, Tree)` repr themselves); a variant outside that
/// subset gets NO repr fn, so the interp's `__repr_<V>` call stays unlinked and the
/// using function keeps the same honest render wall it had. Like every generated
/// routine: trusted prim-only, injected on the render path, outside the witness
/// surface.
/// Collect every ANONYMOUS-record shape that appears as a STRING-INTERP part anywhere in
/// the program (`"${ { ax: 1, ay: 2 } }"` / a structurally-typed bound var) — the shapes
/// [`generate_variant_repr_sources`] emits `__repr_anonrec_<hash>` for. Order-sensitive
/// (the hash keys the SOURCE field order — the block layout); dedup'd by hash.
pub fn collect_interp_anon_records(
    program: &almide_ir::IrProgram,
) -> Vec<Vec<(almide_lang::intern::Sym, Ty)>> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct C {
        out: Vec<Vec<(almide_lang::intern::Sym, Ty)>>,
        seen: std::collections::HashSet<String>,
    }
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if let almide_ir::IrExprKind::StringInterp { parts } = &e.kind {
                for p in parts {
                    if let almide_ir::IrStringPart::Expr { expr } = p {
                        if let Ty::Record { fields } = &expr.ty {
                            let key = anon_record_drop_name(fields);
                            if self.seen.insert(key) {
                                self.out.push(fields.clone());
                            }
                        }
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    let mut c = C { out: Vec::new(), seen: std::collections::HashSet::new() };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        almide_ir::visit::IrVisitor::visit_expr(&mut c, &f.body);
    }
    c.out
}

/// Collect the RECORD/VARIANT names appearing inside a CONTAINER string-interp part anywhere
/// in the program — `"${pts}"` over `List[Point]` / `Option[Point]` / `List[Shape]` — the shapes
/// the generator must emit `__repr_list_rec_<R>` / `__repr_opt_rec_<R>` / `__repr_list_<V>` for
/// (the bare `${Point{..}}` part either inline-expands or takes `__repr_rec_<R>`, which the
/// record section already emits unconditionally for every emittable record).
#[derive(Default)]
pub struct InterpReprContainers {
    pub rec_lists: std::collections::BTreeSet<String>,
    pub rec_opts: std::collections::BTreeSet<String>,
    pub var_lists: std::collections::BTreeSet<String>,
    pub rec_maps: std::collections::BTreeSet<String>,
    pub var_maps: std::collections::BTreeSet<String>,
}
pub fn collect_interp_repr_containers(program: &almide_ir::IrProgram) -> InterpReprContainers {
    use almide_ir::visit::{walk_expr, IrVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    struct C {
        out: InterpReprContainers,
        rec_names: std::collections::HashSet<String>,
        var_names: std::collections::HashSet<String>,
    }
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if let almide_ir::IrExprKind::StringInterp { parts } = &e.kind {
                for p in parts {
                    let almide_ir::IrStringPart::Expr { expr } = p else { continue };
                    match &expr.ty {
                        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                            if let Ty::Named(n, _) = &a[0] {
                                if self.rec_names.contains(n.as_str()) {
                                    self.out.rec_lists.insert(n.as_str().to_string());
                                } else if self.var_names.contains(n.as_str()) {
                                    self.out.var_lists.insert(n.as_str().to_string());
                                }
                            }
                        }
                        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
                            if let Ty::Named(n, _) = &a[0] {
                                if self.rec_names.contains(n.as_str()) {
                                    self.out.rec_opts.insert(n.as_str().to_string());
                                }
                            }
                        }
                        Ty::Applied(TypeConstructorId::Map, a)
                            if a.len() == 2 && matches!(a[0], Ty::String) =>
                        {
                            if let Ty::Named(n, _) = &a[1] {
                                if self.rec_names.contains(n.as_str()) {
                                    self.out.rec_maps.insert(n.as_str().to_string());
                                } else if self.var_names.contains(n.as_str()) {
                                    self.out.var_maps.insert(n.as_str().to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    use almide_ir::IrTypeDeclKind;
    let mut c = C {
        out: InterpReprContainers::default(),
        rec_names: program
            .type_decls
            .iter()
            .filter(|d| matches!(&d.kind, IrTypeDeclKind::Record { .. }))
            .map(|d| d.name.as_str().to_string())
            .collect(),
        var_names: program
            .type_decls
            .iter()
            .filter(|d| matches!(&d.kind, IrTypeDeclKind::Variant { .. }))
            .map(|d| d.name.as_str().to_string())
            .collect(),
    };
    for f in &program.functions {
        c.visit_expr(&f.body);
    }
    c.out
}

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
                if fields.iter().all(|f| matches!(f.ty, Ty::Int | Ty::Bool | Ty::String)) =>
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
                    matches!(ty, Ty::Int | Ty::Bool | Ty::String)
                        // A Float ctor field renders via the compound Display
                        // (`float.to_string_compound` — integral drops the `.0`).
                        || matches!(ty, Ty::Float)
                        // A SCALAR-record ctor field (`Label { at: Point }`) renders via the
                        // record section's unconditional `__repr_rec_<R>`.
                        || matches!(ty, Ty::Named(n, _) if scalar_rec_names.contains(n.as_str()))
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
    if emittable.is_empty() && !any_record && interp_anon_recs.is_empty() {
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
        out.push_str(
        "fn __repr_float_ends_dot0(src: Int, n: Int) -> Bool =\n  \
           if n < 2 then false\n  \
           else if prim.load8(src + n - 2) != 46 then false\n  \
           else prim.load8(src + n - 1) == 48\n\
         fn __repr_float_copy(src: Int, dst: Int, n: Int) -> Int =\n  \
           if n <= 0 then dst\n  \
           else {\n    prim.store8(dst, prim.load8(src))\n    __repr_float_copy(src + 1, dst + 1, n - 1)\n  }\n\
         fn __repr_float(x: Float) -> String = {\n  \
           let s = float.to_string(x)\n  \
           let sh = prim.handle(s)\n  \
           let n = prim.load32(sh + 4)\n  \
           if __repr_float_ends_dot0(sh + 12, n) then {\n    \
             let out = prim.alloc_str(n - 2)\n    \
             let e = __repr_float_copy(sh + 12, prim.handle(out) + 12, n - 2)\n    \
             out\n  } else s\n}\n",
        );
    }
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
        out.push_str(&format!("fn __repr_{fname}(e: {tname}) -> String = {{\n"));
        out.push_str("  let h = prim.handle(e)\n");
        out.push_str(&format!("  let t = prim.load64(h + {})\n", layout::slot_offset(0)));
        let mut first = true;
        for (tag, case) in cases.iter().enumerate() {
            // A RECORD-variant case renders v0's `Tag {{ name: "hi", n: 3 }}` (field names,
            // brace form); a tuple case renders `Pair(3, true)`. Field names ride as
            // `Some(name)` so the concat seed/separators/closer pick the right form.
            let (cname, fields): (&str, Vec<(Option<String>, Ty)>) = match &case.kind {
                IrVariantKind::Unit => (case.name.as_str(), vec![]),
                IrVariantKind::Tuple { fields } => {
                    (case.name.as_str(), fields.iter().map(|t| (None, t.clone())).collect())
                }
                IrVariantKind::Record { fields } => (
                    case.name.as_str(),
                    fields
                        .iter()
                        .map(|f| (Some(f.name.as_str().to_string()), f.ty.clone()))
                        .collect(),
                ),
            };
            let is_record = fields.iter().any(|(n, _)| n.is_some());
            let tys: Vec<Ty> = fields.iter().map(|(_, t)| t.clone()).collect();
            let kw = if first { "if" } else { "  else if" };
            first = false;
            if tys.is_empty() {
                out.push_str(&format!("  {kw} t == {tag} then \"{cname}\"\n"));
                continue;
            }
            out.push_str(&format!("  {kw} t == {tag} then {{\n"));
            let mut concat = if is_record {
                format!("\"{cname} {{ \"")
            } else {
                format!("\"{cname}(\"")
            };
            for (i, ty) in tys.iter().enumerate() {
                let off = layout::slot_offset(1 + i);
                if i > 0 {
                    concat.push_str(" + \", \"");
                }
                if let Some(fname) = &fields[i].0 {
                    concat.push_str(&format!(" + \"{fname}: \""));
                }
                match ty {
                    Ty::Int => {
                        out.push_str(&format!(
                            "    let f{i} = int.to_string(prim.load64(h + {off}))\n"
                        ));
                    }
                    Ty::Bool => {
                        out.push_str(&format!(
                            "    let f{i} = if prim.load64(h + {off}) == 1 then \"true\" else \"false\"\n"
                        ));
                    }
                    Ty::String => {
                        out.push_str(&format!(
                            "    let f{i} = __repr_quote(prim.load_str(h + {off}))\n"
                        ));
                    }
                    Ty::Float => {
                        // The slot holds the f64 BIT pattern (the scalar ctor stored raw bits);
                        // reinterpret then render with the compound Display (drops integral `.0`).
                        out.push_str(&format!(
                            "    let f{i} = __repr_float(prim.ffrombits(prim.load64(h + {off})))\n"
                        ));
                    }
                    Ty::Named(rn, _) if scalar_rec_names.contains(rn.as_str()) => {
                        // A scalar-record ctor field — compose the record's own generated repr.
                        let rn_s = rn.as_str();
                        let rn_fn = drop_fn_ident(rn_s);
                        out.push_str(&format!(
                            "    let v{i}: {rn_s} = prim.load_handle(h + {off})\n    let f{i} = __repr_rec_{rn_fn}(v{i})\n"
                        ));
                    }
                    _ => {
                        // an emittable nested variant (the fixpoint admitted it)
                        let fv = variant_field_name(ty, &names).expect("fixpoint-admitted");
                        let fv_fn = drop_fn_ident(&fv);
                        out.push_str(&format!(
                            "    let v{i}: {fv} = prim.load_handle(h + {off})\n    let f{i} = __repr_{fv_fn}(v{i})\n"
                        ));
                    }
                }
                concat.push_str(&format!(" + f{i}"));
            }
            concat.push_str(if is_record { " + \" }\"" } else { " + \")\"" });
            out.push_str(&format!("    {concat}\n  }}\n"));
        }
        out.push_str("  else \"\"\n}\n");
    }
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
                matches!(ty, Ty::Int | Ty::Bool | Ty::String)
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
                Ty::Int => out.push_str(&format!(
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
    let mut anon_sorted: Vec<&Vec<(almide_lang::intern::Sym, Ty)>> =
        interp_anon_recs.iter().collect();
    anon_sorted.sort_by_key(|f| anon_record_drop_name(f));
    anon_sorted.dedup_by_key(|f| anon_record_drop_name(f));
    for fields in anon_sorted {
        if fields.is_empty()
            || !fields.iter().all(|(_, ty)| matches!(ty, Ty::Int | Ty::Bool | Ty::String))
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
        let mut order: Vec<usize> = (0..fields.len()).collect();
        order.sort_by_key(|&i| fields[i].0.as_str());
        let mut concat = String::from("\"{ \"");
        for (k, &i) in order.iter().enumerate() {
            if k > 0 {
                concat.push_str(" + \", \"");
            }
            concat.push_str(&format!(" + \"{}: \" + f{i}", fields[i].0.as_str()));
        }
        concat.push_str(" + \" }\"");
        out.push_str(&format!("  {concat}\n}}\n"));
    }

    out
}

/// Does the program reference the `Result[Option[String], String]` shape anywhere (a function
/// signature or an expression type)? Gates `$__drop_opt_str` emission in
/// [`generate_record_drop_sources`] — the recursive-drop leaf `try_lower_result_option_scalar_str_ctor`
/// routes an `ok(some(<string>))` / `ok(none)` `Result[Option[String], String]` through
/// (`resrec:opt_str`). Only that shape needs the generated fn; a scalar Option leaf frees flat. Scans
/// the SAME positions as [`collect_recursive_anon_records`] (ret/param/body-expr types).
pub fn program_uses_result_option_str(program: &almide_ir::IrProgram) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    fn is_result_opt_str(ty: &Ty) -> bool {
        let Ty::Applied(TypeConstructorId::Result, a) = ty else { return false };
        if a.len() != 2 || !matches!(a[1], Ty::String) {
            return false;
        }
        matches!(&a[0], Ty::Applied(TypeConstructorId::Option, oa)
            if oa.len() == 1 && matches!(oa[0], Ty::String))
    }
    struct Finder {
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if is_result_opt_str(&expr.ty) {
                self.found = true;
            }
            almide_ir::visit::walk_expr(self, expr);
        }
    }
    let mut finder = Finder { found: false };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_result_opt_str(&f.ret_ty) || f.params.iter().any(|p| is_result_opt_str(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program create or carry FIRST-CLASS FUNCTION values (a `Lambda` expr or a
/// `Ty::Fn`-typed value anywhere)? Gates the injection of [`CLOSURE_DROP_SRC`] — a program
/// with no closures pays neither the second lowering pass nor the dead drop routine.
pub fn program_uses_closures(program: &almide_ir::IrProgram) -> bool {
    struct Finder {
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if matches!(expr.kind, almide_ir::IrExprKind::Lambda { .. })
                || matches!(expr.ty, Ty::Fn { .. })
            {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if matches!(f.ret_ty, Ty::Fn { .. }) || f.params.iter().any(|p| matches!(p.ty, Ty::Fn { .. }))
        {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program carry a `List[<Fn>]` LITERAL anywhere (a bind/return/call-arg type) —
/// gates `LIST_CLOSURE_DROP_SRC`'s injection (a program with closures but no closure LIST
/// pays no dead drop routine, unlike the broader `program_uses_closures` gate).
pub fn program_uses_closure_list(program: &almide_ir::IrProgram) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let is_closure_list = |ty: &Ty| {
        matches!(ty, Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::Fn { .. }))
    };
    struct Finder<'a> {
        found: bool,
        pred: &'a dyn Fn(&Ty) -> bool,
    }
    impl almide_ir::visit::IrVisitor for Finder<'_> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if (self.pred)(&expr.ty) {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false, pred: &is_closure_list };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_closure_list(&f.ret_ty) || f.params.iter().any(|p| is_closure_list(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// Does the program call `map.find` anywhere (a `Some((key,value))` predicate-search result
/// — the Option-tuple payload `$__drop_opt_str_int` frees) — gates `OPT_STR_INT_DROP_SRC`'s
/// injection. Conservative on the call NAME alone (not the key/value TYPE, which would need
/// re-deriving `map.find`'s exact concrete instantiation here) — a program calling
/// `map.find` over a non-`(String,scalar)`-keyed map would pay one unused generated
/// routine, never a missing one.
pub fn program_calls_map_find(program: &almide_ir::IrProgram) -> bool {
    struct Finder {
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if let almide_ir::IrExprKind::Call {
                target: almide_ir::CallTarget::Module { module, func, .. },
                ..
            } = &expr.kind
            {
                if module.as_str() == "map" && func.as_str() == "find" {
                    self.found = true;
                }
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}

/// The element-drop class a `List[Option/Result]` LITERAL's elements take — the SINGLE
/// classifier the injection pre-scan ([`program_uses_lenlist_elem_lists`]) and the literal
/// builder (`try_lower_record_list_literal_as`) BOTH consult, so `$__drop_list_lenlist` is
/// emitted exactly when a list routes to it (the `field_displayable` agree-by-construction
/// precedent).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CtorElemClass {
    /// The element block owns NO heap (`Option[Int/Bool/Float]` — a scalar payload at
    /// data\[0\] under len-as-tag): the flat per-element `rc_dec` (`DropListStr` via
    /// `heap_elem_lists`) frees it EXACTLY.
    Flat,
    /// The element block's first `len` slots are OWNED handles (`Option[String]` Some =
    /// len 1 + payload; `Result[scalar, String]` Ok = len 0 / Err = len 1 + message;
    /// `Result[String, String]` = the cap-as-tag 1-slot form, len 1 either way): the
    /// len-loop `$__drop_list_lenlist` frees each element's owned slots then the element.
    LenLoop,
}

/// Classify a list-literal ELEMENT type as ctor-materializable, or `None` (the caller keeps
/// the record/tuple/wall paths). Only payload types whose OWN drop is one-level-exact are
/// admitted — an `Option[<heap-field record>]` element would leak its record's fields under
/// the len-loop (its wrapper needs `DropWrapperRec`), so it stays walled.
pub fn lenlist_elem_class(elem_ty: &Ty) -> Option<CtorElemClass> {
    use almide_lang::types::constructor::TypeConstructorId;
    // A one-level-exact HEAP payload: freeing it with ONE rc_dec is exact (no owned interior).
    let flat_heap = |t: &Ty| {
        matches!(t, Ty::String)
            || matches!(t, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && !is_heap_ty(&a[0]))
    };
    match elem_ty {
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
            if !is_heap_ty(&a[0]) {
                Some(CtorElemClass::Flat)
            } else if flat_heap(&a[0]) {
                Some(CtorElemClass::LenLoop)
            } else {
                None
            }
        }
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
            let ok_admits = !is_heap_ty(&a[0]) || flat_heap(&a[0]);
            let err_admits = flat_heap(&a[1]);
            if ok_admits && err_admits {
                Some(CtorElemClass::LenLoop)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Is `ty` a `List` whose ELEMENT type routes to the len-loop drop ([`lenlist_elem_class`]
/// = `LenLoop`) — the TYPE-driven registration the call-result / merged-bind sites consult
/// (a value of this type must free via `$__drop_list_lenlist`, never the flat
/// `heap_elem_lists` `DropListStr` that would leak each element's owned slots).
pub fn is_lenlist_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::List, a)
        if a.len() == 1 && lenlist_elem_class(&a[0]) == Some(CtorElemClass::LenLoop))
}

/// Does the program CARRY a len-loop list type anywhere (a literal, a call result, a
/// param/return — any expression's type)? Gates the injection of [`LENLIST_DROP_SRC`] — a
/// program never touching such a type pays no dead drop routine. (A `Flat` element list
/// reuses `DropListStr` and needs no generated source.) Type-based (not literal-based) so a
/// CALLER that only binds a callee's returned list still gets the drop routine linked.
pub fn program_uses_lenlist_elem_lists(program: &almide_ir::IrProgram) -> bool {
    struct Finder {
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if is_lenlist_list_ty(&expr.ty) {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut finder = Finder { found: false };
    let funcs = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()));
    for f in funcs {
        if is_lenlist_list_ty(&f.ret_ty) || f.params.iter().any(|p| is_lenlist_list_ty(&p.ty)) {
            return true;
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut finder, &f.body);
        if finder.found {
            return true;
        }
    }
    false
}
