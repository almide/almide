//! Canonical type expression resolution.
//!
//! Single source of truth for converting `ast::TypeExpr` → `Ty`.
//! Used by both the checker (with type lookup) and lowering (without).

use std::collections::HashMap;
use almide_lang::ast;
use crate::types::{Ty, VariantCase, VariantPayload};
use almide_base::intern::{Sym, sym};

/// Resolve an AST type expression to a Ty.
///
/// `known_types`: optional map of registered type names → Ty (from TypeEnv.types).
/// When provided (checker context), named types are looked up; when None (lowering),
/// unresolved names become `Ty::Named`.
pub fn resolve_type_expr(te: &ast::TypeExpr, known_types: Option<&HashMap<Sym, Ty>>) -> Ty {
    match te {
        ast::TypeExpr::Simple { name } => match name.as_str() {
            "Int" => Ty::Int,
            "Float" => Ty::Float,
            "String" => Ty::String,
            "Bool" => Ty::Bool,
            "Unit" => Ty::Unit,
            "Bytes" => Ty::Bytes,
            "Matrix" => Ty::Matrix,
            "Path" => Ty::String,
            other => {
                if let Some(types) = known_types {
                    types.get(&sym(other)).cloned().unwrap_or(Ty::Named(other.into(), vec![]))
                } else {
                    Ty::Named(sym(other), vec![])
                }
            }
        },
        ast::TypeExpr::Generic { name, args } => {
            let ra: Vec<Ty> = args.iter().map(|a| resolve_type_expr(a, known_types)).collect();
            match name.as_str() {
                "List" => Ty::list(ra.first().cloned().unwrap_or(Ty::Unknown)),
                "Option" => Ty::option(ra.first().cloned().unwrap_or(Ty::Unknown)),
                "Result" if ra.len() >= 2 => Ty::result(ra[0].clone(), ra[1].clone()),
                "Map" if ra.len() >= 2 => Ty::map_of(ra[0].clone(), ra[1].clone()),
                "Set" => Ty::set_of(ra.first().cloned().unwrap_or(Ty::Unknown)),
                _ => Ty::Named(sym(name), ra),
            }
        },
        ast::TypeExpr::Record { fields } => Ty::Record {
            fields: fields.iter().map(|f| (sym(&f.name), resolve_type_expr(&f.ty, known_types))).collect(),
        },
        ast::TypeExpr::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|f| (sym(&f.name), resolve_type_expr(&f.ty, known_types))).collect(),
        },
        ast::TypeExpr::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| resolve_type_expr(p, known_types)).collect(),
            ret: Box::new(resolve_type_expr(ret, known_types)),
        },
        ast::TypeExpr::Tuple { elements } => Ty::Tuple(
            elements.iter().map(|e| resolve_type_expr(e, known_types)).collect(),
        ),
        ast::TypeExpr::Union { members } => Ty::union(
            members.iter().map(|m| resolve_type_expr(m, known_types)).collect(),
        ),
        ast::TypeExpr::Variant { cases } => {
            let cs = cases.iter().map(|c| match c {
                ast::VariantCase::Unit { name } => VariantCase {
                    name: sym(name), payload: VariantPayload::Unit,
                },
                ast::VariantCase::Tuple { name, fields } => VariantCase {
                    name: sym(name),
                    payload: VariantPayload::Tuple(
                        fields.iter().map(|f| resolve_type_expr(f, known_types)).collect(),
                    ),
                },
                ast::VariantCase::Record { name, fields } => VariantCase {
                    name: sym(name),
                    payload: VariantPayload::Record(
                        fields.iter().map(|f| (sym(&f.name), resolve_type_expr(&f.ty, known_types), f.default.clone())).collect(),
                    ),
                },
            }).collect();
            Ty::Variant { name: sym(""), cases: cs }
        },
    }
}
