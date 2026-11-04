// ── The generated repr fns, include!-spliced at the end of mod_c_tail.rs ──
//
// The interpolation lowering routes a value it does not expand inline to a
// GENERATED repr fn by name: `${t}` over a custom variant `V` calls
// `__repr_<V>`, over a named record `R` outside the inline-expand subset
// `__repr_rec_<R>` (mod_p4_b.rs, `interp_part_leaf_*`). The generators that
// once emitted those bodies died with the incumbent wasm pipeline (#2935,
// #2950), so the call walled. Here the native render synthesizes each one as
// an ordinary IR fn, lowered, verified and certified like a user fn.
//
// The body is v0's `AlmideRepr` (crates/almide-codegen/src/walker/
// declarations.rs, `render_repr_variant_arm`), case by case:
//   - a unit case is its name: `Dot`;
//   - a tuple case is `Name(r0, r1)`;
//   - a record case is `Name { f: r0, g: r1 }`, fields in declaration order;
//   - a record is `Name { f: r0, g: r1 }` (`display_aggregate`);
// where each `ri` is the field's container repr: a nested variant by its own
// generated fn, everything else by the Display builder the inline expansion
// uses (`display_value`: a quoted String, a compound Float, a list by its
// element-keyed formatter). A field type with no such builder makes no fn:
// the call stays unresolved and walls, as before.

/// The repr fn `name` names, over the program's declarations, when it is one
/// this module generates: `(fn, the type it renders)`.
pub(crate) fn synthesize_repr_fn(
    name: &str,
    type_decls: &[almide_ir::IrTypeDecl],
    var_table: &mut almide_ir::VarTable,
    registry: &RecordLayouts,
) -> Option<IrFunction> {
    use almide_ir::{IrTypeDeclKind, IrVisibility, Mutability, ParamBorrow};
    let (decl, is_record) = type_decls.iter().find_map(|d| {
        if d.generics.as_ref().is_some_and(|g| !g.is_empty()) {
            return None;
        }
        let ident = drop_fn_ident(d.name.as_str());
        match &d.kind {
            IrTypeDeclKind::Variant { is_generic: false, .. } if name == format!("__repr_{ident}") => Some((d, false)),
            IrTypeDeclKind::Record { .. } if name == format!("__repr_rec_{ident}") => Some((d, true)),
            _ => None,
        }
    })?;
    let ty = Ty::Named(decl.name, Default::default());
    let p = var_table.alloc(sym("v"), ty.clone(), Mutability::Let, None);
    let subject = IrExpr { kind: IrExprKind::Var { id: p }, ty: ty.clone(), span: None, def_id: None };
    let body = if is_record {
        display_aggregate(&subject, &ty, registry)?
    } else {
        let IrTypeDeclKind::Variant { cases, .. } = &decl.kind else { return None };
        let arms = cases
            .iter()
            .map(|case| repr_case_arm(case, var_table, registry))
            .collect::<Option<Vec<_>>>()?;
        IrExpr { kind: IrExprKind::Match { subject: Box::new(subject), arms }, ty: Ty::String, span: None, def_id: None }
    };
    Some(IrFunction {
        name: sym(name),
        params: vec![almide_ir::IrParam {
            var: p,
            ty,
            name: sym("v"),
            borrow: ParamBorrow::Own,
            is_mut: false,
            open_record: None,
            default: None,
            attrs: vec![],
        }],
        ret_ty: Ty::String,
        body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![],
        module_origin: None,
    })
}

/// One case of a variant's repr: its pattern and the text it renders.
fn repr_case_arm(
    case: &almide_ir::IrVariantDecl,
    var_table: &mut almide_ir::VarTable,
    registry: &RecordLayouts,
) -> Option<almide_ir::IrMatchArm> {
    use almide_ir::{IrFieldPattern, IrMatchArm, IrPattern, IrVariantKind, Mutability};
    let name = case.name.as_str();
    let mut bind = |label: &str, t: &Ty| {
        let v = var_table.alloc(sym(label), t.clone(), Mutability::Let, None);
        (IrPattern::Bind { var: v, ty: t.clone() }, IrExpr { kind: IrExprKind::Var { id: v }, ty: t.clone(), span: None, def_id: None })
    };
    let (pattern, body) = match &case.kind {
        IrVariantKind::Unit => (IrPattern::Constructor { name: name.to_string(), args: vec![] }, lit_str(name)),
        IrVariantKind::Tuple { fields } => {
            let mut args = Vec::new();
            let mut parts = vec![lit_str(&format!("{name}("))];
            for (i, t) in fields.iter().enumerate() {
                let (pat, read) = bind(&format!("f{i}"), t);
                args.push(pat);
                if i > 0 {
                    parts.push(lit_str(", "));
                }
                parts.push(repr_field(&read, registry)?);
            }
            parts.push(lit_str(")"));
            (IrPattern::Constructor { name: name.to_string(), args }, concat_all(parts))
        }
        IrVariantKind::Record { fields } => {
            let mut pats = Vec::new();
            let mut parts = vec![lit_str(&format!("{name} {{ "))];
            for (i, f) in fields.iter().enumerate() {
                let (pat, read) = bind(f.name.as_str(), &f.ty);
                pats.push(IrFieldPattern { name: f.name.to_string(), pattern: Some(pat) });
                parts.push(lit_str(&format!("{}{}: ", if i > 0 { ", " } else { "" }, f.name)));
                parts.push(repr_field(&read, registry)?);
            }
            parts.push(lit_str(" }"));
            (IrPattern::RecordPattern { name: name.to_string(), fields: pats, rest: false }, concat_all(parts))
        }
    };
    Some(IrMatchArm { pattern, guard: None, body })
}

/// A payload field's container repr: a non-generic custom variant by its own
/// generated fn, any other value by the inline expansion's Display builder.
fn repr_field(read: &IrExpr, registry: &RecordLayouts) -> Option<IrExpr> {
    if let Ty::Named(n, args) = &read.ty {
        if args.is_empty() && resolve_aggregate(&read.ty, registry).is_none() {
            return Some(IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: sym(&format!("__repr_{}", drop_fn_ident(n.as_str()))) },
                    args: vec![read.clone()],
                    type_args: Vec::new(),
                },
                ty: Ty::String,
                span: None,
                def_id: None,
            });
        }
    }
    display_value(read, registry)
}
