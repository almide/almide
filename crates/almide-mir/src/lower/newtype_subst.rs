// The Ty-substitution core + type-decl erasure walk of the transparent-newtype
// pass: `subst` and its named/fields/variant legs, and the in-place type_decls
// erasure. Split out of newtype_erase.rs (max-lines, #852); moved verbatim.

/// The `Ty`-substitution core [`erase_transparent_newtypes`] and
/// [`erase_newtypes_in_type_decls`] both recurse with — module-level (was a
/// nested fn; hoisted so the type_decls-erasure loop below can share it
/// without either duplicating it or threading it as a closure param).
fn subst(ty: &almide_lang::types::Ty, map: &std::collections::HashMap<String, almide_lang::types::Ty>) -> almide_lang::types::Ty {
    use almide_lang::types::Ty;
    match ty {
        Ty::Named(name, args) => subst_named(name, args, map),
        Ty::Applied(id, args) => {
            Ty::Applied(id.clone(), args.iter().map(|a| subst(a, map)).collect())
        }
        Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|a| subst(a, map)).collect()),
        Ty::Union(ts) => Ty::Union(ts.iter().map(|a| subst(a, map)).collect()),
        Ty::Record { fields } => Ty::Record { fields: subst_ty_fields(fields, map) },
        Ty::OpenRecord { fields } => Ty::OpenRecord { fields: subst_ty_fields(fields, map) },
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|a| subst(a, map)).collect(),
            ret: Box::new(subst(ret, map)),
        },
        Ty::Variant { name, cases } => subst_variant(*name, cases, map),
        Ty::ConstParam { name, ty } => {
            Ty::ConstParam { name: *name, ty: Box::new(subst(ty, map)) }
        }
        Ty::ConstValue { ty, value } => {
            Ty::ConstValue { ty: Box::new(subst(ty, map)), value: *value }
        }
        _ => ty.clone(),
    }
}

/// The `Ty::Named` arm of [`subst`] — extracted (uniform, self-contained
/// match arms, no shared state; same as [`erase_newtypes_in_type_decls`]'s
/// split above). The nominal replaces itself wholesale when it's a
/// zero-arg alias target; otherwise recurse into its type args.
fn subst_named(
    name: &almide_lang::intern::Sym,
    args: &[almide_lang::types::Ty],
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) -> almide_lang::types::Ty {
    use almide_lang::types::Ty;
    if args.is_empty() {
        if let Some(t) = map.get(name.as_str()) {
            return t.clone();
        }
    }
    Ty::Named(*name, args.iter().map(|a| subst(a, map)).collect())
}

/// The `(Sym, Ty)` field-list substitution shared by `Ty::Record` and
/// `Ty::OpenRecord` in [`subst`] — was duplicated inline in both arms.
fn subst_ty_fields(
    fields: &[(almide_lang::intern::Sym, almide_lang::types::Ty)],
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) -> Vec<(almide_lang::intern::Sym, almide_lang::types::Ty)> {
    fields.iter().map(|(n, t)| (*n, subst(t, map))).collect()
}

/// The `Ty::Variant` arm of [`subst`] — extracted for the same reason as
/// [`subst_named`].
fn subst_variant(
    name: almide_lang::intern::Sym,
    cases: &[almide_lang::types::VariantCase],
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) -> almide_lang::types::Ty {
    use almide_lang::types::Ty;
    Ty::Variant {
        name,
        cases: cases.iter().map(|c| subst_variant_case(c, map)).collect(),
    }
}

/// The per-case payload substitution inside [`subst_variant`] — a uniform,
/// self-contained match over `VariantPayload` (each arm reads only its own
/// payload, no cross-arm state).
fn subst_variant_case(
    c: &almide_lang::types::VariantCase,
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) -> almide_lang::types::VariantCase {
    use almide_lang::types::VariantPayload;
    almide_lang::types::VariantCase {
        name: c.name,
        payload: match &c.payload {
            VariantPayload::Unit => VariantPayload::Unit,
            VariantPayload::Tuple(ts) => {
                VariantPayload::Tuple(ts.iter().map(|a| subst(a, map)).collect())
            }
            VariantPayload::Record(fs) => VariantPayload::Record(subst_ty_fields(fs, map)),
        },
    }
}

/// Other type decls may carry alias-typed fields (a record holding a
/// SafeHtml) — the tail loop of [`erase_transparent_newtypes`], verbatim move.
fn erase_newtypes_in_type_decls(
    type_decls: &mut [almide_ir::IrTypeDecl],
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) {
    for td in type_decls.iter_mut() {
        erase_newtypes_in_one_type_decl(td, map);
    }
}

/// One `td.kind` arm of [`erase_newtypes_in_type_decls`] — extracted, each
/// arm is self-contained (reads only its own `td`, writes only its own
/// fields), so this is a pure name-router split, no behavior change.
fn erase_newtypes_in_one_type_decl(
    td: &mut almide_ir::IrTypeDecl,
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) {
    match &mut td.kind {
        almide_ir::IrTypeDeclKind::Record { fields } => {
            for f in fields.iter_mut() {
                f.ty = subst(&f.ty, map);
            }
        }
        almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
            for c in cases.iter_mut() {
                erase_newtypes_in_variant_case(c, map);
            }
        }
        almide_ir::IrTypeDeclKind::Alias { .. } => {}
    }
}

/// The `c.kind` arm of [`erase_newtypes_in_one_type_decl`]'s `Variant` case —
/// extracted for the same reason (uniform, self-contained match arms).
fn erase_newtypes_in_variant_case(
    c: &mut almide_ir::IrVariantDecl,
    map: &std::collections::HashMap<String, almide_lang::types::Ty>,
) {
    match &mut c.kind {
        almide_ir::IrVariantKind::Unit => {}
        almide_ir::IrVariantKind::Tuple { fields } => {
            for t in fields.iter_mut() {
                *t = subst(t, map);
            }
        }
        almide_ir::IrVariantKind::Record { fields } => {
            for f in fields.iter_mut() {
                f.ty = subst(&f.ty, map);
            }
        }
    }
}
