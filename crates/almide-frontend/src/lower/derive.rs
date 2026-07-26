// ── Auto-derive ─────────────────────────────────────────────────

use almide_ir::*;
use crate::types::Ty;
use almide_base::intern::{Sym, sym};
use super::LowerCtx;
use super::derive_codec::{
    auto_derive_encode, auto_derive_decode,
    auto_derive_variant_encode, auto_derive_variant_decode,
    derive_container_helpers,
};

/// Generate IR functions for conventions declared via `deriving` but without custom implementation.
pub(super) fn generate_auto_derives(ctx: &mut LowerCtx, type_decls: &[IrTypeDecl], existing_fns: &[IrFunction]) -> Vec<IrFunction> {
    let fn_names: std::collections::HashSet<&str> = existing_fns.iter().map(|f| &*f.name).collect();
    let mut auto = Vec::new();

    for td in type_decls {
        let derives = match &td.deriving {
            Some(d) => d,
            None => continue,
        };
        let type_ty = Ty::Named(td.name, vec![]);
        let fields = match &td.kind {
            IrTypeDeclKind::Record { fields } => Some(fields.clone()),
            _ => None,
        };

        for conv in derives {
            let fn_name = format!("{}.{}", td.name, conv.to_lowercase());
            if fn_names.contains(fn_name.as_str()) { continue; }
            auto.extend(derive_one(ctx, td, &type_ty, fields.as_deref(), conv, &fn_names));
        }
    }
    auto
}

/// Generate the IR functions one `deriving` entry asks for.
///
/// `fields` is `Some` only for a record declaration; a variant reaches the
/// variant-shaped derive instead, and a type that is neither derives nothing.
/// An unknown convention name derives nothing rather than erroring: `Ord` and
/// `Hash` are real conventions that Rust's own `#[derive]` still covers, so the
/// empty result is the correct answer, not a gap.
fn derive_one(
    ctx: &mut LowerCtx,
    td: &IrTypeDecl,
    type_ty: &Ty,
    fields: Option<&[IrFieldDecl]>,
    conv: &str,
    fn_names: &std::collections::HashSet<&str>,
) -> Vec<IrFunction> {
    match conv {
        "Repr" => fields
            .map(|f| vec![auto_derive_repr(&mut ctx.var_table, &td.name, type_ty, f)])
            .unwrap_or_default(),
        "Eq" => derive_eq(ctx, td, type_ty, fields),
        "Codec" => derive_codec(ctx, td, type_ty, fields, fn_names),
        _ => Vec::new(),
    }
}

/// Structural equality: field-by-field for a record, tag-and-payload for a
/// variant.
fn derive_eq(
    ctx: &mut LowerCtx,
    td: &IrTypeDecl,
    type_ty: &Ty,
    fields: Option<&[IrFieldDecl]>,
) -> Vec<IrFunction> {
    if let Some(fields) = fields {
        return vec![auto_derive_eq(&mut ctx.var_table, &td.name, type_ty, fields)];
    }
    if matches!(&td.kind, IrTypeDeclKind::Variant { .. }) {
        return vec![auto_derive_variant_eq(&mut ctx.var_table, &td.name, type_ty)];
    }
    Vec::new()
}

/// `encode` / `decode`, plus the container helpers every Codec type provides.
///
/// `encode` and `decode` are checked for individually rather than as a pair: a
/// type may hand-write one and derive the other.
fn derive_codec(
    ctx: &mut LowerCtx,
    td: &IrTypeDecl,
    type_ty: &Ty,
    fields: Option<&[IrFieldDecl]>,
    fn_names: &std::collections::HashSet<&str>,
) -> Vec<IrFunction> {
    let mut out = Vec::new();
    let wants = |suffix: &str| !fn_names.contains(format!("{}.{}", td.name, suffix).as_str());
    match (fields, &td.kind) {
        (Some(fields), _) => {
            if wants("encode") {
                out.push(auto_derive_encode(&mut ctx.var_table, &td.name, type_ty, fields));
            }
            if wants("decode") {
                out.push(auto_derive_decode(&mut ctx.var_table, &td.name, type_ty, fields));
            }
        }
        (None, IrTypeDeclKind::Variant { cases, .. }) => {
            if wants("encode") {
                out.push(auto_derive_variant_encode(&mut ctx.var_table, &td.name, type_ty, cases));
            }
            if wants("decode") {
                out.push(auto_derive_variant_decode(&mut ctx.var_table, &td.name, type_ty, cases));
            }
        }
        (None, _) => {}
    }
    // The four container helpers (`__{en,de}code_{list,option}_T`, #790 piece 1)
    // every Codec type provides — real bodies the v1 leg links; on v0 the
    // BuiltinLowering call-rewrite keeps them unused (DCE'd).
    if !fn_names.contains(format!("__encode_list_{}", td.name).as_str()) {
        out.extend(derive_container_helpers(&mut ctx.var_table, &td.name, type_ty));
    }
    out
}

/// Auto-derive Repr: `fn Dog.repr(d: Dog) -> String = "Dog { name: ..., breed: ... }"`
fn auto_derive_repr(vt: &mut VarTable, type_name: &str, type_ty: &Ty, fields: &[IrFieldDecl]) -> IrFunction {
    let var = vt.alloc(sym("_v"), type_ty.clone(), Mutability::Let, None);

    // Build string interp: "TypeName { field1: ..., field2: ... }"
    let mut parts = vec![IrStringPart::Lit { value: format!("{} {{ ", type_name) }];
    for (i, f) in fields.iter().enumerate() {
        if i > 0 { parts.push(IrStringPart::Lit { value: ", ".to_string() }); }
        parts.push(IrStringPart::Lit { value: format!("{}: ", f.name) });
        let field_access = IrExpr {
            kind: IrExprKind::Member { object: Box::new(IrExpr { kind: IrExprKind::Var { id: var }, ty: type_ty.clone(), span: None, def_id: None }), field: f.name },
            ty: f.ty.clone(), span: None, def_id: None,
        };
        parts.push(IrStringPart::Expr { expr: field_access });
    }
    parts.push(IrStringPart::Lit { value: " }".to_string() });

    IrFunction {
        name: sym(&format!("{}.repr", type_name)),
        params: vec![IrParam { var, ty: type_ty.clone(), name: sym("_v"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }],
        ret_ty: Ty::String,
        body: IrExpr { kind: IrExprKind::StringInterp { parts }, ty: Ty::String, span: None, def_id: None },
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}

/// Auto-derive Eq for variant types: `fn Color.eq(a: Color, b: Color) -> Bool = a == b`
/// Variant types get `#[derive(PartialEq)]` in Rust, so direct == comparison works.
fn auto_derive_variant_eq(vt: &mut VarTable, type_name: &str, type_ty: &Ty) -> IrFunction {
    let var_a = vt.alloc(sym("_a"), type_ty.clone(), Mutability::Let, None);
    let var_b = vt.alloc(sym("_b"), type_ty.clone(), Mutability::Let, None);

    let body = IrExpr {
        kind: IrExprKind::BinOp {
            op: BinOp::Eq,
            left: Box::new(IrExpr { kind: IrExprKind::Var { id: var_a }, ty: type_ty.clone(), span: None, def_id: None }),
            right: Box::new(IrExpr { kind: IrExprKind::Var { id: var_b }, ty: type_ty.clone(), span: None, def_id: None }),
        },
        ty: Ty::Bool,
        span: None, def_id: None,
    };

    IrFunction {
        name: sym(&format!("{}.eq", type_name)),
        params: vec![
            IrParam { var: var_a, ty: type_ty.clone(), name: sym("_a"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] },
            IrParam { var: var_b, ty: type_ty.clone(), name: sym("_b"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] },
        ],
        ret_ty: Ty::Bool,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}

/// Auto-derive Eq: `fn Dog.eq(a: Dog, b: Dog) -> Bool = a.f1 == b.f1 and a.f2 == b.f2 and ...`
fn auto_derive_eq(vt: &mut VarTable, type_name: &str, type_ty: &Ty, fields: &[IrFieldDecl]) -> IrFunction {
    let var_a = vt.alloc(sym("_a"), type_ty.clone(), Mutability::Let, None);
    let var_b = vt.alloc(sym("_b"), type_ty.clone(), Mutability::Let, None);

    let mk_var = |id: VarId, ty: &Ty| IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span: None, def_id: None };
    let mk_field = |var: VarId, field: Sym, ty: &Ty| IrExpr {
        kind: IrExprKind::Member { object: Box::new(mk_var(var, type_ty)), field },
        ty: ty.clone(), span: None, def_id: None,
    };

    // Build: a.f1 == b.f1 and a.f2 == b.f2 and ...
    let body = fields.iter()
        .map(|f| IrExpr {
            kind: IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(mk_field(var_a, f.name, &f.ty)), right: Box::new(mk_field(var_b, f.name, &f.ty)) },
            ty: Ty::Bool, span: None, def_id: None,
        })
        .reduce(|prev, cmp| IrExpr {
            kind: IrExprKind::BinOp { op: BinOp::And, left: Box::new(prev), right: Box::new(cmp) },
            ty: Ty::Bool, span: None, def_id: None,
        });

    IrFunction {
        name: sym(&format!("{}.eq", type_name)),
        params: vec![
            IrParam { var: var_a, ty: type_ty.clone(), name: sym("_a"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] },
            IrParam { var: var_b, ty: type_ty.clone(), name: sym("_b"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] },
        ],
        ret_ty: Ty::Bool,
        body: body.unwrap_or(IrExpr { kind: IrExprKind::LitBool { value: true }, ty: Ty::Bool, span: None, def_id: None }),
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}
