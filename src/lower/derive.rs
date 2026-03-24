// ── Auto-derive ─────────────────────────────────────────────────

use crate::ir::*;
use crate::types::Ty;
use crate::intern::sym;
use super::LowerCtx;
use super::derive_codec::{
    auto_derive_encode, auto_derive_decode,
    auto_derive_variant_encode, auto_derive_variant_decode,
};

/// Generate IR functions for conventions declared via `deriving` but without custom implementation.
pub(super) fn generate_auto_derives(ctx: &mut LowerCtx, type_decls: &[IrTypeDecl], existing_fns: &[IrFunction]) -> Vec<IrFunction> {
    let fn_names: std::collections::HashSet<&str> = existing_fns.iter().map(|f| f.name.as_str()).collect();
    let mut auto = Vec::new();

    for td in type_decls {
        let derives = match &td.deriving {
            Some(d) => d,
            None => continue,
        };
        let type_ty = Ty::Named(sym(&td.name), vec![]);
        let fields = match &td.kind {
            IrTypeDeclKind::Record { fields } => Some(fields.clone()),
            _ => None,
        };

        for conv in derives {
            let fn_name = format!("{}.{}", td.name, conv.to_lowercase());
            if fn_names.contains(fn_name.as_str()) { continue; }

            match conv.as_str() {
                "Repr" => {
                    if let Some(ref fields) = fields {
                        auto.push(auto_derive_repr(&mut ctx.var_table, &td.name, &type_ty, fields));
                    }
                }
                "Eq" => {
                    if let Some(ref fields) = fields {
                        auto.push(auto_derive_eq(&mut ctx.var_table, &td.name, &type_ty, fields));
                    }
                }
                "Codec" => {
                    let encode_name = format!("{}.encode", td.name);
                    let decode_name = format!("{}.decode", td.name);
                    if let Some(ref fields) = fields {
                        if !fn_names.contains(encode_name.as_str()) {
                            auto.push(auto_derive_encode(&mut ctx.var_table, &td.name, &type_ty, fields));
                        }
                        if !fn_names.contains(decode_name.as_str()) {
                            auto.push(auto_derive_decode(&mut ctx.var_table, &td.name, &type_ty, fields));
                        }
                    } else if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                        if !fn_names.contains(encode_name.as_str()) {
                            auto.push(auto_derive_variant_encode(&mut ctx.var_table, &td.name, &type_ty, cases));
                        }
                        if !fn_names.contains(decode_name.as_str()) {
                            auto.push(auto_derive_variant_decode(&mut ctx.var_table, &td.name, &type_ty, cases));
                        }
                    }
                }
                _ => {} // Ord, Hash — Rust #[derive] handles these for now
            }
        }
    }
    auto
}

/// Auto-derive Repr: `fn Dog.repr(d: Dog) -> String = "Dog { name: ..., breed: ... }"`
fn auto_derive_repr(vt: &mut VarTable, type_name: &str, type_ty: &Ty, fields: &[IrFieldDecl]) -> IrFunction {
    let var = vt.alloc("_v".to_string(), type_ty.clone(), Mutability::Let, None);

    // Build string interp: "TypeName { field1: ..., field2: ... }"
    let mut parts = vec![IrStringPart::Lit { value: format!("{} {{ ", type_name) }];
    for (i, f) in fields.iter().enumerate() {
        if i > 0 { parts.push(IrStringPart::Lit { value: ", ".to_string() }); }
        parts.push(IrStringPart::Lit { value: format!("{}: ", f.name) });
        let field_access = IrExpr {
            kind: IrExprKind::Member { object: Box::new(IrExpr { kind: IrExprKind::Var { id: var }, ty: type_ty.clone(), span: None }), field: f.name.clone() },
            ty: f.ty.clone(), span: None,
        };
        parts.push(IrStringPart::Expr { expr: field_access });
    }
    parts.push(IrStringPart::Lit { value: " }".to_string() });

    IrFunction {
        name: format!("{}.repr", type_name),
        params: vec![IrParam { var, ty: type_ty.clone(), name: "_v".to_string(), borrow: ParamBorrow::Own, open_record: None, default: None }],
        ret_ty: Ty::String,
        body: IrExpr { kind: IrExprKind::StringInterp { parts }, ty: Ty::String, span: None },
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], visibility: IrVisibility::Public,
    }
}

/// Auto-derive Eq: `fn Dog.eq(a: Dog, b: Dog) -> Bool = a.f1 == b.f1 and a.f2 == b.f2 and ...`
fn auto_derive_eq(vt: &mut VarTable, type_name: &str, type_ty: &Ty, fields: &[IrFieldDecl]) -> IrFunction {
    let var_a = vt.alloc("_a".to_string(), type_ty.clone(), Mutability::Let, None);
    let var_b = vt.alloc("_b".to_string(), type_ty.clone(), Mutability::Let, None);

    let mk_var = |id: VarId, ty: &Ty| IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span: None };
    let mk_field = |var: VarId, field: &str, ty: &Ty| IrExpr {
        kind: IrExprKind::Member { object: Box::new(mk_var(var, type_ty)), field: field.to_string() },
        ty: ty.clone(), span: None,
    };

    // Build: a.f1 == b.f1 and a.f2 == b.f2 and ...
    let body = fields.iter()
        .map(|f| IrExpr {
            kind: IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(mk_field(var_a, &f.name, &f.ty)), right: Box::new(mk_field(var_b, &f.name, &f.ty)) },
            ty: Ty::Bool, span: None,
        })
        .reduce(|prev, cmp| IrExpr {
            kind: IrExprKind::BinOp { op: BinOp::And, left: Box::new(prev), right: Box::new(cmp) },
            ty: Ty::Bool, span: None,
        });

    IrFunction {
        name: format!("{}.eq", type_name),
        params: vec![
            IrParam { var: var_a, ty: type_ty.clone(), name: "_a".to_string(), borrow: ParamBorrow::Own, open_record: None, default: None },
            IrParam { var: var_b, ty: type_ty.clone(), name: "_b".to_string(), borrow: ParamBorrow::Own, open_record: None, default: None },
        ],
        ret_ty: Ty::Bool,
        body: body.unwrap_or(IrExpr { kind: IrExprKind::LitBool { value: true }, ty: Ty::Bool, span: None }),
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], visibility: IrVisibility::Public,
    }
}
