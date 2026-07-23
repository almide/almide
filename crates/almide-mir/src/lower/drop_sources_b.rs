
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
    generic_decls: &GenericRecordDecls,
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
            // Guard-clause flattening (`continue` re-targets the enclosing `for` loop, exactly
            // matching this arm's former "fall out of the if-else-if chain, the match arm — and
            // so this loop iteration — ends" behavior for every branch including the last). No
            // behavior change — see docs/roadmap/active/code-health-codopsy.md.
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                if let Some((rn, src)) = recursive_aggregate_route(&a[0], rec_names, generic_decls) {
                    list_drops.insert(rn.clone());
                    let rn_fn = drop_fn_ident(&rn);
                    // The BINDING type must be valid Almide source: a NAMED element renders
                    // its name, an ANONYMOUS element (or a generic INSTANTIATION) its
                    // STRUCTURAL `{ k: T, … }` form — the synthesized `anonrec_<hash>` is a
                    // drop-fn identity, NOT a type name (writing `List[anonrec_…]`
                    // type-errored the whole generated batch: "undefined variable 'f0'"
                    // after the rejected let).
                    frees.push_str(&format!(
                        "    let f{i}: List[{src}] = prim.load_handle(h + {off})\n    __drop_list_{rn_fn}(f{i})\n"
                    ));
                    continue;
                }
                if let Some(ev) = list_rich_variant_elem(ty, rec_variant_names) {
                    // `List[<rich variant>]` (`Global.init: List[Instr]`): each element is a
                    // recursive-drop variant block, freed per-element by `$__drop_list_<ev>` (→
                    // `$__drop_<ev>`, generated by `generate_variant_drop_sources`). A flat `rc_dec`
                    // of the list block would leak every element's nested children (its own List[Instr]).
                    let ev_fn = drop_fn_ident(&ev);
                    frees.push_str(&format!(
                        "    let f{i}: List[{ev}] = prim.load_handle(h + {off})\n    __drop_list_{ev_fn}(f{i})\n"
                    ));
                    continue;
                }
                if matches!(&a[0], Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _)) {
                    // `List[Matrix]` — each element is a matrix block whose slots hold owned
                    // row blocks: sweep TWO levels via `__drop_list_matrix` (each element
                    // through `__drop_matrix`, then the list). A flat `rc_dec` would leak
                    // every matrix AND its rows.
                    *need_matrix = true;
                    *need_list_matrix = true;
                    frees.push_str(&format!(
                        "    let f{i}: List[Matrix] = prim.load_handle(h + {off})\n    __drop_list_matrix(f{i})\n"
                    ));
                    continue;
                }
                if matches!(&a[0],
                    Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && !is_heap_ty(&b[0]))
                {
                    // A matrix-shaped STRUCTURAL field (`List[List[scalar]]`): its slots hold
                    // owned flat row blocks — `__drop_matrix`'s per-row `rc_dec` sweep is its
                    // exact free (a flat `rc_dec` frees only the outer block, leaking rows).
                    *need_matrix = true;
                    frees.push_str(&format!(
                        "    let f{i}: Matrix = prim.load_handle(h + {off})\n    __drop_matrix(f{i})\n"
                    ));
                    continue;
                }
                if matches!(a[0], Ty::String) || is_flat_variant_elem(&a[0], flat_variant_names) {
                    // `List[String]` OR `List[flat-variant]` (a nullary/scalar-only enum like
                    // `Capability`): each element is a single FLAT block, so `__drop_list_str` frees
                    // them per-element (`rc_dec` of each element handle + the list block). The flat
                    // variant element holds no inner handle, so the byte-identical String-list drop is
                    // its full free — a flat `rc_dec` of just the list block would LEAK each element.
                    *need_list_str = true;
                    frees.push_str(&format!(
                        "    let f{i}: List[String] = prim.load_handle(h + {off})\n    __drop_list_str(f{i})\n"
                    ));
                    continue;
                }
                // List[scalar] or List[non-recursive heap]: flat free the block.
                frees.push_str(&format!("    prim.rc_dec(prim.load64(h + {off}))\n"));
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
                if let Some((rn, src)) = recursive_aggregate_route(t, rec_names, generic_decls) {
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
    let all_decls: Vec<almide_ir::IrTypeDecl> = program.type_decls.clone();
    // A visitor that inspects every expression's type (every IrExpr carries its `ty`), collecting
    // the distinct anon record shapes that need a synthesized recursive drop (deduped by drop name).
    // RECURSES into a qualifying anon record's own anon-record FIELDS so a nested anon shape
    // (`{ st: { iv: Bytes } }`) gets its inner `__drop_anonrec_<hash>` generated too (the outer drop
    // body `let f: { iv: Bytes } = …; __drop_anonrec_<inner>(f)` would otherwise call a missing fn).
    // An INSTANTIATED GENERIC record (`Pair[Int, String]`) registers its SUBSTITUTED field
    // shape the same way — its drop is the per-shape `__drop_anonrec_<hash>`, never a shared
    // `__drop_<R>` (the heap mask differs per instantiation; see `generic_record_decls`).
    struct TyCollector {
        seen: std::collections::HashSet<String>,
        out: Vec<Vec<(almide_lang::intern::Sym, Ty)>>,
        generic_decls: GenericRecordDecls,
    }
    impl TyCollector {
        fn consider(&mut self, ty: &Ty) {
            use almide_lang::types::constructor::TypeConstructorId;
            if let Some(pairs) = instantiated_generic_record_fields(ty, &self.generic_decls) {
                if anon_record_needs_recursive_drop(&pairs) {
                    let name = anon_record_drop_name(&pairs);
                    if self.seen.insert(name) {
                        self.out.push(pairs.clone());
                    }
                    for (_, fty) in &pairs {
                        self.consider(fty);
                    }
                }
                return;
            }
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

    let mut collector = TyCollector {
        seen: std::collections::HashSet::new(),
        out: Vec::new(),
        generic_decls: generic_record_decls(&all_decls),
    };
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
