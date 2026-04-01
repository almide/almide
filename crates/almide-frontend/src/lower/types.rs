// ── Type declarations ───────────────────────────────────────────

use crate::ast;
use almide_ir::*;
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
    IrTypeDecl { name: sym(name), kind, deriving: deriving.as_ref().map(|d| d.iter().copied().collect()), generics: generics.cloned(), visibility: vis, doc: None, blank_lines_before: 0 }
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

// ── Type expression resolution (delegates to canonical version) ──

pub(super) fn resolve_type_expr(te: &ast::TypeExpr) -> Ty {
    crate::canonicalize::resolve::resolve_type_expr(te, None)
}
