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
            // Sized numeric types (Stage 1a of the sized-numeric-types arc).
            // `Int64` / `Float64` alias to `Ty::Int` / `Ty::Float` — writing
            // either form is indistinguishable at the type checker layer, so
            // existing code that uses `Int` keeps compiling while new code
            // can use the precise width name.
            "Int64" => Ty::Int,
            "Float64" => Ty::Float,
            "Int8" => Ty::Int8,
            "Int16" => Ty::Int16,
            "Int32" => Ty::Int32,
            "UInt8" => Ty::UInt8,
            "UInt16" => Ty::UInt16,
            "UInt32" => Ty::UInt32,
            "UInt64" => Ty::UInt64,
            "Float32" => Ty::Float32,
            "String" => Ty::String,
            "Bool" => Ty::Bool,
            "Unit" => Ty::Unit,
            "Bytes" => Ty::Bytes,
            "Matrix" => Ty::Matrix,
            "RawPtr" => Ty::RawPtr,
            "Path" => Ty::String,
            // `Never` is the bottom type — used by `process.exit` and
            // similar diverging fns. The resolver has to surface it as
            // `Ty::Never` (not `Ty::Named("Never", [])`); without this,
            // bundled sigs that spell `-> Never` would be unifiable only
            // with another nominal `Never` type, which doesn't exist.
            "Never" => Ty::Never,
            other => {
                // - Generic type parameters (T, U, Self, ...) resolve via
                //   known_types as `Ty::TypeVar`.
                // - Record/Variant declarations must keep their nominal
                //   identity — expanding them to the structural form here
                //   would collapse two distinct types with identical shapes
                //   (e.g. Dog and Cat both `{name: String}`). They come back
                //   as `Ty::Named` and are expanded on demand via
                //   `resolve_named`.
                // - OpenRecord aliases (`type Named = { name: String, .. }`)
                //   are *shape aliases* meant to act as structural bounds,
                //   not nominal types. Keep them transparent so they can
                //   still accept any record with at least those fields.
                // - Transparent aliases (e.g. `type Score = Int`) follow
                //   through to the target type so `a + b` works.
                if let Some(types) = known_types {
                    // Try exact match first (e.g. "Instr" or "binary.Instr")
                    let found = types.get(&sym(other)).or_else(|| {
                        // For module-qualified types like "binary.Instr",
                        // also try the unqualified name "Instr"
                        other.rsplit_once('.').and_then(|(_, bare)| types.get(&sym(bare)))
                    });
                    if let Some(found) = found {
                        match found {
                            Ty::TypeVar(tv) => return Ty::TypeVar(*tv),
                            Ty::Record { .. } | Ty::Variant { .. } => {
                                // nominal — keep as Named, but use the canonical name
                                if let Some((_, bare)) = other.rsplit_once('.') {
                                    return Ty::Named(sym(bare), vec![]);
                                }
                            }
                            other_ty => return other_ty.clone(),
                        }
                    }
                }
                // For module-qualified names, use the bare name for Ty::Named
                if let Some((_, bare)) = other.rsplit_once('.') {
                    Ty::Named(sym(bare), vec![])
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
                _ => {
                    let resolved_name = name.as_str().rsplit_once('.').map(|(_, bare)| sym(bare)).unwrap_or(*name);
                    Ty::Named(resolved_name, ra)
                },
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
                        fields.iter().map(|f| (sym(&f.name), resolve_type_expr(&f.ty, known_types))).collect(),
                    ),
                },
            }).collect();
            Ty::Variant { name: sym(""), cases: cs }
        },
    }
}
