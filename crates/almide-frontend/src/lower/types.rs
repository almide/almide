// ── Type declarations ───────────────────────────────────────────

use crate::ast;
use almide_ir::*;
use crate::types::Ty;
use crate::intern::{Sym, sym};
use super::LowerCtx;
use super::expressions::lower_expr;

/// A borrowed view of the `type` declaration being lowered.
pub(super) struct TypeToLower<'a> {
    pub name: &'a str,
    pub ty: &'a ast::TypeExpr,
    pub deriving: &'a Option<Vec<Sym>>,
    pub visibility: &'a ast::Visibility,
    pub generics: Option<&'a Vec<ast::GenericParam>>,
    pub module_prefix: Option<&'a str>,
}

pub(super) fn lower_type_decl(ctx: &mut LowerCtx, decl: &TypeToLower<'_>) -> IrTypeDecl {
    let TypeToLower { name, ty, deriving, visibility, generics, module_prefix } = *decl;
    // #433: a user (non-stdlib) module's type is declared under its qualified
    // canonical name `mod.Type`, matching how references resolve, so two packages'
    // same-name types stay distinct through link + codegen.
    let qualified_name = match module_prefix {
        Some(m) if !almide_lang::stdlib_info::is_bundled_module(m) => format!("{}.{}", m, name),
        // The entry program's declaration of a stdlib-owned name is `self.Type`
        // (#1828) — the checker's key for it — so the bare spelling stays the
        // stdlib's on every backend. Not an opaque alias: that newtype keeps
        // the bare key (see `register_type_decl`). And not the owning stdlib
        // module's OWN declaration when that module is the entry program
        // (`entry_bundled_module`): the checker keyed it bare, as the stdlib's.
        None if almide_lang::stdlib_info::stdlib_owned_type_owner(name).is_some()
            && !ctx.env.entry_owns_stdlib_type(name)
            && (matches!(visibility, ast::Visibility::Public)
                || matches!(ty, ast::TypeExpr::Record { .. } | ast::TypeExpr::OpenRecord { .. } | ast::TypeExpr::Variant { .. })) =>
        {
            format!("{}.{}", crate::canonicalize::resolve::ROOT_TYPE_SCOPE, name)
        }
        _ => name.to_string(),
    };
    // Use TypeEnv for field type resolution so aliases (TcpStream → Int)
    // are expanded at lowering time, not left as Ty::Named for codegen.
    let resolve = |te: &ast::TypeExpr| crate::canonicalize::resolve::resolve_type_expr_in(te, Some(&ctx.env.types), module_prefix);
    let kind = match ty {
        ast::TypeExpr::Record { fields } => {
            let fs = fields.iter().map(|f| {
                let default = f.default.as_ref().map(|d| lower_expr(ctx, d));
                IrFieldDecl { name: f.name, ty: resolve(&f.ty), default, alias: f.alias, attrs: f.attrs.clone() }
            }).collect();
            IrTypeDeclKind::Record { fields: fs }
        }
        ast::TypeExpr::Variant { cases } => {
            let is_generic = matches!(generics, Some(gs) if !gs.is_empty());
            let cs = cases.iter().map(|c| lower_variant_case(ctx, c, name, module_prefix)).collect();
            IrTypeDeclKind::Variant {
                cases: cs, is_generic,
                boxed_args: std::collections::HashSet::new(),
                boxed_record_fields: std::collections::HashSet::new(),
            }
        }
        _ => IrTypeDeclKind::Alias { target: resolve(ty) },
    };
    let vis = match visibility {
        ast::Visibility::Public => IrVisibility::Public,
        ast::Visibility::Mod => IrVisibility::Mod,
        ast::Visibility::Local => IrVisibility::Private,
    };
    IrTypeDecl { name: sym(&qualified_name), kind, deriving: deriving.as_ref().map(|d| d.iter().copied().collect()), generics: generics.cloned(), visibility: vis, doc: None, blank_lines_before: 0 }
}

fn lower_variant_case(ctx: &mut LowerCtx, case: &ast::VariantCase, _parent: &str, module_prefix: Option<&str>) -> IrVariantDecl {
    // #484: payload types must resolve through the same env+prefix path as
    // record fields and alias targets (lower_type_decl's `resolve` closure),
    // so a cross-module payload like `m.Emotion` keeps its qualified canonical
    // name and gets mangled alongside its declaration by IrLinkFlattenPass.
    let resolve = |te: &ast::TypeExpr| crate::canonicalize::resolve::resolve_type_expr_in(te, Some(&ctx.env.types), module_prefix);
    match case {
        ast::VariantCase::Unit { name } => IrVariantDecl { name: *name, kind: IrVariantKind::Unit },
        ast::VariantCase::Tuple { name, fields } => {
            let tys = fields.iter().map(|f| resolve(f)).collect();
            IrVariantDecl { name: *name, kind: IrVariantKind::Tuple { fields: tys } }
        }
        ast::VariantCase::Record { name, fields } => {
            let fs = fields.iter().map(|f| {
                let default = f.default.as_ref().map(|d| lower_expr(ctx, d));
                IrFieldDecl { name: f.name, ty: resolve(&f.ty), default, alias: f.alias, attrs: f.attrs.clone() }
            }).collect();
            IrVariantDecl { name: *name, kind: IrVariantKind::Record { fields: fs } }
        }
    }
}

// ── Type expression resolution (delegates to canonical version) ──

pub(super) fn resolve_type_expr(te: &ast::TypeExpr) -> Ty {
    crate::canonicalize::resolve::resolve_type_expr(te, None)
}

/// #1839: lower ONE bundled module's `type` declaration exactly as that
/// module's own lowering does (`lower_module` → [`lower_type_decl`]: the bare
/// name a bundled decl keeps, the env-resolved field types), for `ir_link`'s
/// type-owner pull — the declaration handed to a program that spells the
/// type while the owner module was never loaded. `None` when `module` is not
/// bundled or declares no such type.
pub fn lower_bundled_type_decl(module: &str, name: &str) -> Option<IrTypeDecl> {
    if !almide_lang::stdlib_info::is_bundled_module(module) {
        return None;
    }
    let source = almide_lang::stdlib_info::bundled_source(module)?;
    let program = almide_lang::parse_cached(source)?;
    let (ty, deriving, visibility, generics) = program.decls.iter().find_map(|d| match d {
        ast::Decl::Type { name: n, ty, deriving, visibility, generics, .. } if n.as_str() == name => {
            Some((ty, deriving, visibility, generics))
        }
        _ => None,
    })?;
    // The module half of canonicalization with no modules: builtin protocols
    // plus every bundled type registration — what a bundled decl's field
    // types resolve against in the real pipeline.
    let env = crate::canonicalize::canonicalize_modules_env(
        std::iter::empty::<(&str, &ast::Program, bool)>(),
    )
    .env;
    let type_map = crate::types::TypeMap::new();
    let mut ctx = LowerCtx::new(&env, &type_map);
    ctx.current_module = Some(sym(module));
    Some(lower_type_decl(&mut ctx, &TypeToLower {
        name, ty, deriving, visibility, generics: generics.as_ref(), module_prefix: Some(module),
    }))
}
