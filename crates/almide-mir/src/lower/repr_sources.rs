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
                // Guard-clause flattening (`continue` re-targets this `for p in parts` loop,
                // matching the former "nothing else runs for this `p`" fallthrough at every
                // failed condition — the plain-Record check above stays a bare `if let` since
                // it runs unconditionally either way, exactly as before). No behavior change —
                // see docs/roadmap/active/code-health-codopsy.md.
                for p in parts {
                    let almide_ir::IrStringPart::Expr { expr } = p else {
                        continue;
                    };
                    if let Ty::Record { fields } = &expr.ty {
                        let key = anon_record_drop_name(fields);
                        if self.seen.insert(key) {
                            self.out.push(fields.clone());
                        }
                    }
                    // `${[a]}` — a List[<structural record>] part: collect the
                    // ELEMENT shape so the generator emits its `__repr_anonrec_<h>`
                    // plus the `__repr_list_anonrec_<h>` walker the interp routes to.
                    let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a) =
                        &expr.ty
                    else {
                        continue;
                    };
                    if a.len() != 1 {
                        continue;
                    }
                    let Ty::Record { fields } = &a[0] else {
                        continue;
                    };
                    let key = anon_record_drop_name(fields);
                    if self.seen.insert(key) {
                        self.out.push(fields.clone());
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
    /// `${Option[<non-generic variant>]}` parts (`opt_tree=${opt_tree}`) — the
    /// generator emits `__repr_opt_<V>` for each emittable one.
    pub var_opts: std::collections::BTreeSet<String>,
    /// `${List[<generic-variant instantiation>]}` / `${Option[<instantiation>]}`
    /// parts (`forest=${forest}` over `List[Tree[Int]]`) — the generator emits the
    /// instantiation-keyed walkers (`__repr_list_<key>` / `__repr_opt_<key>`).
    pub var_inst_lists: Vec<(String, Vec<Ty>)>,
    pub var_inst_opts: Vec<(String, Vec<Ty>)>,
    pub rec_maps: std::collections::BTreeSet<String>,
    pub var_maps: std::collections::BTreeSet<String>,
    /// GENERIC-variant interp instantiations (`${l}` over `ReprEither[Int, String]`)
    /// — the (name, args) pairs the generator emits an instantiation-keyed
    /// `__repr_<key>` for (deduped + sorted at generation).
    pub var_insts: Vec<(String, Vec<Ty>)>,
    /// SCALAR-component tuple CONTAINER interp shapes (`${list.sort(tups)}` over
    /// `List[(Int, String)]`, `${list.min(tups)}` over `Option[(Bool, Bool)]`) —
    /// component-type lists keyed by [`tuple_repr_ident`]; the generator emits
    /// `__repr_tup_<key>` + the `__repr_list_tup_<key>` / `__repr_opt_tup_<key>`
    /// walkers (Int/Bool/String components only; others keep the honest wall).
    pub tup_lists: Vec<Vec<Ty>>,
    pub tup_opts: Vec<Vec<Ty>>,
    /// A bare `${obj}` over a `Value` part exists somewhere — the generator emits
    /// the `__repr_Value` wrapper over the value_core JSON serializer (C-060).
    pub value_parts: bool,
}

/// The generated-repr key for a scalar-component tuple shape — one tag per
/// component (`(Int, String)` → `i_s`). Derived IDENTICALLY at the interp call
/// site (mod_p4) and in the generator, so the call links by construction.
/// `None` for any component outside Int/Bool/String (the honest-wall gate).
pub(crate) fn tuple_repr_ident(tys: &[Ty]) -> Option<String> {
    let tag = |t: &Ty| -> Option<&'static str> {
        match t {
            Ty::Int => Some("i"),
            Ty::Bool => Some("b"),
            Ty::String => Some("s"),
            _ => None,
        }
    };
    Some(tys.iter().map(tag).collect::<Option<Vec<_>>>()?.join("_"))
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
                            if let Ty::Named(n, args) = &a[0] {
                                self.track_list_named(*n, args);
                            }
                            // `${List[(Int, String)]}` — a scalar-component tuple
                            // element: the generator emits its `__repr_list_tup_<key>`.
                            if let Ty::Tuple(ts) = &a[0] {
                                self.track_list_tuple(ts);
                            }
                        }
                        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
                            if let Ty::Named(n, args) = &a[0] {
                                self.track_option_named(*n, args);
                            }
                            // `${Option[(Bool, Bool)]}` (a list.min/max result) — the
                            // generator emits its `__repr_opt_tup_<key>`.
                            if let Ty::Tuple(ts) = &a[0] {
                                self.track_option_tuple(ts);
                            }
                        }
                        Ty::Applied(TypeConstructorId::Map, a)
                            if a.len() == 2 && matches!(a[0], Ty::String) =>
                        {
                            if let Ty::Named(n, _) = &a[1] {
                                self.track_map_named(*n);
                            }
                        }
                        // A GENERIC-variant instance part (`${l}` over
                        // `ReprEither[Int, String]`) — record the instantiation so the
                        // generator emits its keyed repr.
                        Ty::Named(n, args)
                            if !args.is_empty() && self.var_names.contains(n.as_str()) =>
                        {
                            self.out.var_insts.push((n.as_str().to_string(), args.clone()));
                        }
                        // A bare `${obj}` over a `Value` — the generator emits the
                        // `__repr_Value` wrapper (value_core's JSON serializer, C-060).
                        t if crate::lower::is_value_ty(t) => {
                            self.out.value_parts = true;
                        }
                        _ => {}
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    impl C {
        /// The `List[<Named>]` NAMED-element container-tracking for the `${l}` string-interp
        /// part scan above. Verbatim extraction (guard-clause flattening) of the former inline
        /// if-else-if chain, no behavior change — see
        /// docs/roadmap/active/code-health-codopsy.md.
        fn track_list_named(&mut self, n: almide_lang::intern::Sym, args: &[Ty]) {
            if self.rec_names.contains(n.as_str()) {
                self.out.rec_lists.insert(n.as_str().to_string());
                return;
            }
            if !self.var_names.contains(n.as_str()) {
                return;
            }
            if args.is_empty() {
                self.out.var_lists.insert(n.as_str().to_string());
                return;
            }
            // A generic-variant INSTANTIATION element (`${forest}` over List[Tree[Int]]): the
            // walker needs the element's instantiation-keyed repr too.
            self.out.var_inst_lists.push((n.as_str().to_string(), args.to_vec()));
            self.out.var_insts.push((n.as_str().to_string(), args.to_vec()));
        }

        /// The `Option[<Named>]` sibling of [`Self::track_list_named`]. Verbatim extraction,
        /// no behavior change.
        fn track_option_named(&mut self, n: almide_lang::intern::Sym, args: &[Ty]) {
            if self.rec_names.contains(n.as_str()) {
                self.out.rec_opts.insert(n.as_str().to_string());
                return;
            }
            if !self.var_names.contains(n.as_str()) {
                return;
            }
            if args.is_empty() {
                self.out.var_opts.insert(n.as_str().to_string());
                return;
            }
            self.out.var_inst_opts.push((n.as_str().to_string(), args.to_vec()));
            self.out.var_insts.push((n.as_str().to_string(), args.to_vec()));
        }

        /// `${List[(scalar…)]}` — dedup-push the tuple-component shape into `tup_lists`.
        /// Verbatim extraction, no behavior change.
        fn track_list_tuple(&mut self, ts: &[Ty]) {
            let Some(key) = tuple_repr_ident(ts) else {
                return;
            };
            if self.out.tup_lists.iter().any(|e| tuple_repr_ident(e).as_deref() == Some(&key)) {
                return;
            }
            self.out.tup_lists.push(ts.to_vec());
        }

        /// The `Option[(scalar…)]` sibling of [`Self::track_list_tuple`]. Verbatim
        /// extraction, no behavior change.
        fn track_option_tuple(&mut self, ts: &[Ty]) {
            let Some(key) = tuple_repr_ident(ts) else {
                return;
            };
            if self.out.tup_opts.iter().any(|e| tuple_repr_ident(e).as_deref() == Some(&key)) {
                return;
            }
            self.out.tup_opts.push(ts.to_vec());
        }

        /// `${Map[String, <Named>]}` NAMED-value container-tracking. Verbatim extraction
        /// (guard-clause flattening) of the former inline if-else-if, no behavior change.
        fn track_map_named(&mut self, n: almide_lang::intern::Sym) {
            if self.rec_names.contains(n.as_str()) {
                self.out.rec_maps.insert(n.as_str().to_string());
                return;
            }
            if self.var_names.contains(n.as_str()) {
                self.out.var_maps.insert(n.as_str().to_string());
            }
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

/// The instantiation-keyed repr ident (`ReprEither[Int, String]` →
/// `ReprEither_Int_String`) — derived IDENTICALLY at the interp call site
/// (mod_p4's variant part) and in the generator, so the call links by
/// construction. Args spell via their `Debug` form, sanitized to identifier
/// chars (the `instantiate_variant_layout` key discipline).
pub(crate) fn repr_inst_ident(name: &str, args: &[Ty]) -> String {
    let sane = |s: String| -> String {
        s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
    };
    format!(
        "{}_{}",
        drop_fn_ident(name),
        args.iter().map(|a| sane(format!("{a:?}"))).collect::<Vec<_>>().join("_")
    )
}

/// The Almide SPELLING of a generic-instantiation arg admitted by the
/// instantiation brick — scalar/String leaves only, so the generated fn's param
/// annotation (`e: ReprEither[Int, String]`) type-checks.
fn repr_ty_spelling(ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    Some(match ty {
        Ty::Int => "Int".to_string(),
        Ty::Bool => "Bool".to_string(),
        Ty::String => "String".to_string(),
        Ty::Float => "Float".to_string(),
        // A `List[Int]` instantiation arg (`Tree[List[Int]]` — the nested
        // recursive-generic C-010 shape).
        Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::Int) =>
        {
            "List[Int]".to_string()
        }
        _ => return None,
    })
}

/// Flatten decl cases to `(ctor name, (field-name?, ty))` rows — a record-kind
/// case carries `Some(name)` so the emitter picks the brace form. `subst`
/// (generic instantiation) substitutes each field type via the value model's
/// `subst_type_var` (bare `Named(T, [])` params included).
fn flatten_variant_cases(
    cases: &[almide_ir::IrVariantDecl],
    subst: Option<&std::collections::HashMap<almide_lang::intern::Sym, Ty>>,
) -> Vec<(String, Vec<(Option<String>, Ty)>)> {
    use almide_ir::IrVariantKind;
    let apply = |t: &Ty| -> Ty {
        match subst {
            Some(s) => calls::subst_type_var(t, s),
            None => t.clone(),
        }
    };
    cases
        .iter()
        .map(|case| match &case.kind {
            IrVariantKind::Unit => (case.name.as_str().to_string(), vec![]),
            IrVariantKind::Tuple { fields } => (
                case.name.as_str().to_string(),
                fields.iter().map(|t| (None, apply(t))).collect(),
            ),
            IrVariantKind::Record { fields } => (
                case.name.as_str().to_string(),
                fields
                    .iter()
                    .map(|f| (Some(f.name.as_str().to_string()), apply(&f.ty)))
                    .collect(),
            ),
        })
        .collect()
}

/// Emit ONE `fn __repr_<fname>(e: <tspell>) -> String` body over pre-flattened
/// cases — shared by the DECL loop (raw fields) and the INSTANTIATION loop
/// (type-param fields substituted with the use-site args). A RECORD-variant case
/// renders v0's `Tag { name: "hi", n: 3 }` (field names, brace form); a tuple
/// case renders `Pair(3, true)`; a nullary case its bare name.
fn emit_variant_repr_body(
    out: &mut String,
    fname: &str,
    tspell: &str,
    cases: &[(String, Vec<(Option<String>, Ty)>)],
    scalar_rec_names: &std::collections::HashSet<String>,
    names: &std::collections::HashSet<String>,
) {
    out.push_str(&format!("fn __repr_{fname}(e: {tspell}) -> String = {{\n"));
    out.push_str("  let h = prim.handle(e)\n");
    out.push_str(&format!("  let t = prim.load64(h + {})\n", layout::slot_offset(0)));
    let mut first = true;
    for (tag, (cname, fields)) in cases.iter().enumerate() {
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
            if let Some(fld) = &fields[i].0 {
                concat.push_str(&format!(" + \"{fld}: \""));
            }
            match ty {
                t if repr_int_field(t) => {
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
                // An ANONYMOUS-record payload (`Circle({ r: Int })` — #628/C-079):
                // borrow the payload block and render via its `__repr_anonrec_<hash>`
                // (the record half emits one per variant-payload shape).
                Ty::Record { fields: rf } => {
                    let hash = anon_record_drop_name(rf);
                    let spell = anon_record_source_ty(rf);
                    out.push_str(&format!(
                        "    let v{i}: {spell} = prim.load_handle(h + {off})\n    let f{i} = __repr_{hash}(v{i})\n"
                    ));
                }
                // A `List[Int]` payload (`Leaf([1, 2])` in `Tree[List[Int]]` — C-010):
                // borrow the block, render via the generated `__repr_list_int` helper
                // (the dispatch name `list.to_string` is self-host-only, unknown to
                // the checker — the string.cmp lesson).
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 && matches!(a[0], Ty::Int) =>
                {
                    out.push_str(&format!(
                        "    let v{i}: List[Int] = prim.load_handle(h + {off})\n    let f{i} = __repr_list_int(v{i})\n"
                    ));
                }
                // A generic-variant INSTANTIATION field (`Node(Tree[Int], Tree[Int])`
                // after substitution — the recursive-generic C-010 class): recurse
                // through the SAME instantiation-keyed fn (terminates on the finite
                // value; the admissibility gate admits only the exact self-reference).
                Ty::Named(n, args) if !args.is_empty() => {
                    let key = repr_inst_ident(n.as_str(), args);
                    let spell = format!(
                        "{}[{}]",
                        n.as_str(),
                        args.iter()
                            .map(|a| repr_ty_spelling(a).unwrap_or_else(|| "Int".to_string()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    out.push_str(&format!(
                        "    let v{i}: {spell} = prim.load_handle(h + {off})\n    let f{i} = __repr_{key}(v{i})\n"
                    ));
                }
                _ => {
                    // an emittable nested variant (the fixpoint admitted it)
                    let fv = variant_field_name(ty, names).expect("fixpoint-admitted");
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

/// The Float display helpers (`__repr_float` — shortest-round-trip with the
/// integral `.0` dropped): emitted once, on demand, by either the decl-scan gate
/// or the instantiation loop (function order in the generated source is free).
fn emit_float_helpers(out: &mut String) {
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

/// A ctor/record field whose slot renders as a plain signed decimal via
/// `int.to_string` of the i64-uniform slot value: `Int` and every SMALL-INT
/// class (the value model stores them sign/zero-extended, so the widened value
/// IS the display — v0's per-width Display prints the same digits). `UInt64`
/// is excluded: a value above i64::MAX would print negative (the honest wall).
fn repr_int_field(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Int
            | Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
    )
}
