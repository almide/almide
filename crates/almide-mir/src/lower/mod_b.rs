
/// Does the BRIDGED module-side type agree with the main-side REFERENCE entry's
/// type, treating the reference side's `Unknown` as a wildcard STRUCTURALLY
/// (`Option[Unknown]` agrees with `Option[Cfg]` — the un-inferred synthesized
/// ref vs the refined module truth)? A concrete mismatch still refuses (the
/// honest unbound wall, never a wrong-typed alias).
fn bridged_ref_ty_agrees(bridged: &Ty, reference: &Ty) -> bool {
    match (bridged, reference) {
        (_, Ty::Unknown) => true,
        (Ty::Applied(a, xs), Ty::Applied(b, ys)) if a == b && xs.len() == ys.len() => {
            xs.iter().zip(ys).all(|(x, y)| bridged_ref_ty_agrees(x, y))
        }
        _ => bridged == reference,
    }
}

/// Refine an UNANNOTATED top-let's Unknown(-payload) type from its OPTION-ctor
/// initializer: `let MAYBE = some(Cfg { .. })` leaves the declared ty `Unknown`
/// (or the checker's partial `Option[Unknown]`) while the ctor's PAYLOAD expr
/// carries its real inferred type — `Option[payload.ty]` is the structural
/// truth, never a guess. Any other shape returns `None` (untouched).
// Named (codopsy cc) — a pure classification, no logic change.
fn is_unknown_option_payload_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Unknown => true,
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 && matches!(a[0], Ty::Unknown) => {
            true
        }
        _ => false,
    }
}

pub fn refine_option_toplet_ty(ty: &Ty, init: &almide_ir::IrExpr) -> Option<Ty> {
    if !is_unknown_option_payload_ty(ty) {
        return None;
    }
    let almide_ir::IrExprKind::OptionSome { expr } = &init.kind else { return None };
    if matches!(expr.ty, Ty::Unknown) {
        return None;
    }
    Some(Ty::option(expr.ty.clone()))
}

pub fn build_variant_layouts(type_decls: &[almide_ir::IrTypeDecl]) -> VariantLayouts {
    // Fold-with-append-only-accumulator split (codopsy8 complexity sweep): each `decl` is
    // processed independently and only APPENDS to `out` (never reads back an earlier
    // decl's contribution) — the established "fold is safer than router" pattern
    // (round5/round7/round8). Pure text-move, no logic change.
    let mut out = VariantLayouts::default();
    for decl in type_decls {
        build_variant_layouts_for_decl(decl, &mut out);
    }
    out
}

/// Extracted from `build_variant_layouts` (codopsy8 complexity sweep, per-decl phase):
/// a plain RECORD's field defaults ride the same map, keyed by the record TYPE name
/// (`AllDefault()` — the paren-empty ctor fills them in `try_lower_record_construct`; a
/// variant record-ctor keys by CTOR name in [`build_variant_layouts_variant_decl`]).
/// Verbatim.
fn build_variant_layouts_for_decl(decl: &almide_ir::IrTypeDecl, out: &mut VariantLayouts) {
    use almide_ir::IrTypeDeclKind;
    if let IrTypeDeclKind::Record { fields } = &decl.kind {
        out.records.insert(decl.name.as_str().to_string());
        for f in fields {
            if let Some(d) = &f.default {
                out.ctor_field_defaults
                    .entry(decl.name.as_str().to_string())
                    .or_default()
                    .insert(f.name.as_str().to_string(), d.clone());
            }
        }
        return;
    }
    let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else {
        return;
    };
    build_variant_layouts_variant_decl(decl, cases, out);
}

/// Extracted from `build_variant_layouts` (codopsy8 complexity sweep, the `Variant` arm of
/// the per-decl phase): builds one type's [`VariantLayout`] (all its constructor cases +
/// the shared slot count) and registers each ctor's owning type. Verbatim.
fn build_variant_layouts_variant_decl(
    decl: &almide_ir::IrTypeDecl,
    cases: &[almide_ir::IrVariantDecl],
    out: &mut VariantLayouts,
) {
    let generics =
        decl.generics.as_ref().map(|gs| gs.iter().map(|g| g.name).collect()).unwrap_or_default();
    let type_name = decl.name.as_str().to_string();
    let mut case_layouts = Vec::with_capacity(cases.len());
    let mut max_arity = 0usize;
    for (tag, case) in cases.iter().enumerate() {
        let fields = build_variant_layouts_case_fields(case, out);
        max_arity = max_arity.max(fields.len());
        out.ctor_to_type.insert(case.name.as_str().to_string(), type_name.clone());
        case_layouts.push(VariantCaseLayout { ctor: case.name, tag: tag as u32, fields });
    }
    out.by_type.insert(
        type_name,
        VariantLayout {
            generics,
            cases: case_layouts,
            // slot 0 is the tag; slots 1.. are the widest constructor's fields, so all
            // constructors of the type share one block size (uniform alloc + sound `==`).
            slot_count: 1 + max_arity,
        },
    );
}

/// Extracted from `build_variant_layouts_variant_decl` (codopsy8 complexity sweep): one
/// constructor CASE's field list, by ctor kind. A `Record` case ALSO registers its field
/// defaults (keyed by CTOR name, unlike the plain-record arm above which keys by TYPE
/// name). Verbatim.
fn build_variant_layouts_case_fields(
    case: &almide_ir::IrVariantDecl,
    out: &mut VariantLayouts,
) -> Vec<(almide_lang::intern::Sym, Ty)> {
    use almide_ir::IrVariantKind;
    match &case.kind {
        IrVariantKind::Unit => Vec::new(),
        // A tuple constructor's positional fields get the same `_0`, `_1`, …
        // synthetic names v0 assigns, so field identity is shared across backends.
        IrVariantKind::Tuple { fields } => fields
            .iter()
            .enumerate()
            .map(|(i, ty)| (almide_lang::intern::sym(&format!("_{i}")), ty.clone()))
            .collect(),
        IrVariantKind::Record { fields } => {
            for f in fields {
                if let Some(d) = &f.default {
                    out.ctor_field_defaults
                        .entry(case.name.as_str().to_string())
                        .or_default()
                        .insert(f.name.as_str().to_string(), d.clone());
                }
            }
            fields.iter().map(|f| (f.name, f.ty.clone())).collect()
        }
    }
}

/// The `__drop_<T>` FUNCTION IDENTIFIER for a (possibly module-prefixed) type name. A cross-module
/// type carries its module prefix in the IR (`self.types.RunResult` → `Ty::Named("types.RunResult")`);
/// a dot is illegal in an Almide function name, so the generated drop fn / its call sites / the
/// rendered `(call $__drop_…)` all spell it as its `qualified_ident` — the SAME mangling v0 codegen
/// applies (`almide_rt_types_RunResult`); injective, so `a.b.T` and `a_b.T` stay two (#3338). For a single-file (dot-free) type this is the identity, so
/// the v0 corpus / spec fixtures render byte-identically. The `Op::DropVariant` renderer applies the
/// IDENTICAL transform, keeping the call site and the definition in lockstep.
pub fn drop_fn_ident(type_name: &str) -> String {
    almide_base::names::qualified_ident(type_name)
}

/// [`lower_function_all`] WITH the program's record-layout registry threaded in —
/// the entry the real pipeline (render_program) uses so a `Ty::Named` record
/// resolves its fields (and `r.x` materializes). The plain [`lower_function_all`]
/// passes an empty registry (the structurally-typed `Ty::Record`/`Ty::Tuple`
/// paths still work; a `Ty::Named` aggregate stays walled without it). Delegates to
/// [`lower_function_all_with_layouts`] with an empty VARIANT registry — so a custom
/// variant stays walled (the ADT bricks call `_with_layouts` to admit it).
pub fn lower_function_all_with_types(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    record_layouts: &RecordLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    lower_function_all_with_layouts(func, globals, record_layouts, &VariantLayouts::default())
}

/// [`lower_function_all_with_layouts`] WITH the module-level globals' INITIALIZERS threaded
/// in, so a HEAP global reference materializes its real const value (the base64 alphabet /
/// aes S-box) instead of walling. The `_with_layouts` entry delegates here with empty inits
/// (every heap-global reference there still walls, as before — no regression).
pub fn lower_function_all_with_globals(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    global_inits: &HashMap<VarId, IrExpr>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    let out = lower_function_all_impl(func, globals, global_inits, record_layouts, variant_layouts);
    // Debug aid (wasm-leg parity with the native rungs' ALMIDE_DUMP_MIR):
    // `ALMIDE_DUMP_WMIR=<substr>` prints the lowered op stream of matching fns.
    if let (Some(pat), Ok(ms)) = (almide_base::env::var("ALMIDE_DUMP_WMIR"), &out) {
        if func.name.as_str().contains(&pat) {
            for m in ms {
                eprintln!("== WMIR {} ==", m.name);
                for op in &m.ops {
                    eprintln!("  {op:?}");
                }
            }
        }
    }
    out
}

/// [`lower_function_all_with_types`] WITH the program's VARIANT-layout registry threaded in
/// too — the entry the real pipeline uses once custom ADTs participate in the value model
/// (the construct / `match` / drop bricks consult [`LowerCtx::variant_layouts`]). The
/// record-only entry above delegates here with an empty variant registry.
pub fn lower_function_all_with_layouts(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    lower_function_all_impl(func, globals, &HashMap::new(), record_layouts, variant_layouts)
}


include!("mod_b_tail.rs");
