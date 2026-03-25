// ── Type declarations ───────────────────────────────────────────

use crate::ast;
use crate::ir::*;
use crate::types::Ty;
use crate::intern::{Sym, sym};
use super::LowerCtx;
use super::expressions::lower_expr;

pub(super) fn lower_type_decl(ctx: &mut LowerCtx, name: &str, ty: &ast::TypeExpr, deriving: &Option<Vec<Sym>>, visibility: &ast::Visibility, generics: Option<&Vec<ast::GenericParam>>) -> IrTypeDecl {
    let kind = match ty {
        ast::TypeExpr::Record { fields } => {
            let fs = fields.iter().map(|f| {
                let default = f.default.as_ref().map(|d| lower_expr(ctx, d));
                IrFieldDecl { name: f.name, ty: resolve_type_expr(&f.ty), default, alias: f.alias }
            }).collect();
            IrTypeDeclKind::Record { fields: fs }
        }
        ast::TypeExpr::Variant { cases } => {
            let is_generic = matches!(generics, Some(gs) if !gs.is_empty());
            let cs = cases.iter().map(|c| lower_variant_case(ctx, c, name)).collect();
            IrTypeDeclKind::Variant {
                cases: cs, is_generic,
                boxed_args: std::collections::HashSet::new(),
                boxed_record_fields: std::collections::HashSet::new(),
            }
        }
        _ => IrTypeDeclKind::Alias { target: resolve_type_expr(ty) },
    };
    let vis = match visibility {
        ast::Visibility::Public => IrVisibility::Public,
        ast::Visibility::Mod => IrVisibility::Mod,
        ast::Visibility::Local => IrVisibility::Private,
    };
    IrTypeDecl { name: sym(name), kind, deriving: deriving.as_ref().map(|d| d.iter().copied().collect()), generics: generics.cloned(), visibility: vis }
}

fn lower_variant_case(ctx: &mut LowerCtx, case: &ast::VariantCase, _parent: &str) -> IrVariantDecl {
    match case {
        ast::VariantCase::Unit { name } => IrVariantDecl { name: *name, kind: IrVariantKind::Unit },
        ast::VariantCase::Tuple { name, fields } => {
            let tys = fields.iter().map(|f| resolve_type_expr(f)).collect();
            IrVariantDecl { name: *name, kind: IrVariantKind::Tuple { fields: tys } }
        }
        ast::VariantCase::Record { name, fields } => {
            let fs = fields.iter().map(|f| {
                let default = f.default.as_ref().map(|d| lower_expr(ctx, d));
                IrFieldDecl { name: f.name, ty: resolve_type_expr(&f.ty), default, alias: f.alias }
            }).collect();
            IrVariantDecl { name: *name, kind: IrVariantKind::Record { fields: fs } }
        }
    }
}

// ── Type expression resolution (standalone, no checker needed) ──

pub(super) fn resolve_type_expr(te: &ast::TypeExpr) -> Ty {
    match te {
        ast::TypeExpr::Simple { name } => match name.as_str() {
            "Int" => Ty::Int, "Float" => Ty::Float, "String" => Ty::String,
            "Bool" => Ty::Bool, "Unit" => Ty::Unit, "Path" => Ty::String,
            _ => Ty::Named(*name, vec![]),
        },
        ast::TypeExpr::Generic { name, args } => {
            let ra: Vec<Ty> = args.iter().map(resolve_type_expr).collect();
            match name.as_str() {
                "List" => Ty::list(ra.first().cloned().unwrap_or_else(|| {
                    eprintln!("[ICE] lower: List[] without type argument");
                    Ty::Unknown
                })),
                "Option" => Ty::option(ra.first().cloned().unwrap_or_else(|| {
                    eprintln!("[ICE] lower: Option[] without type argument");
                    Ty::Unknown
                })),
                "Result" if ra.len() >= 2 => Ty::result(ra[0].clone(), ra[1].clone()),
                "Map" if ra.len() >= 2 => Ty::map_of(ra[0].clone(), ra[1].clone()),
                _ => Ty::Named(*name, ra),
            }
        },
        ast::TypeExpr::Record { fields } => Ty::Record {
            fields: fields.iter().map(|f| (f.name, resolve_type_expr(&f.ty))).collect(),
        },
        ast::TypeExpr::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|f| (f.name, resolve_type_expr(&f.ty))).collect(),
        },
        ast::TypeExpr::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(resolve_type_expr).collect(),
            ret: Box::new(resolve_type_expr(ret)),
        },
        ast::TypeExpr::Tuple { elements } => Ty::Tuple(elements.iter().map(resolve_type_expr).collect()),
        ast::TypeExpr::Variant { cases } => {
            let cs = cases.iter().map(|c| match c {
                ast::VariantCase::Unit { name } => crate::types::VariantCase { name: *name, payload: crate::types::VariantPayload::Unit },
                ast::VariantCase::Tuple { name, fields } => crate::types::VariantCase {
                    name: sym(name),
                    payload: crate::types::VariantPayload::Tuple(fields.iter().map(resolve_type_expr).collect()),
                },
                ast::VariantCase::Record { name, fields } => crate::types::VariantCase {
                    name: sym(name),
                    payload: crate::types::VariantPayload::Record(fields.iter().map(|f| (f.name, resolve_type_expr(&f.ty), f.default.clone())).collect()),
                },
            }).collect();
            Ty::Variant { name: sym(""), cases: cs }
        },
        ast::TypeExpr::Union { members } => Ty::union(members.iter().map(resolve_type_expr).collect()),
    }
}
