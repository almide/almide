//! Canonical type expression resolution.
//!
//! Single source of truth for converting `ast::TypeExpr` → `Ty`.
//! Used by both the checker (with type lookup) and lowering (without).

use std::collections::HashMap;
use almide_lang::ast;
use crate::types::{Ty, TypeConstructorId, VariantCase, VariantPayload};
use almide_base::intern::{Sym, sym};

/// Resolve an AST type expression to a Ty.
///
/// `known_types`: optional map of registered type names → Ty (from TypeEnv.types).
/// When provided (checker context), named types are looked up; when None (lowering),
/// unresolved names become `Ty::Named`.
pub fn resolve_type_expr(te: &ast::TypeExpr, known_types: Option<&HashMap<Sym, Ty>>) -> Ty {
    resolve_type_expr_in(te, known_types, None)
}

/// Resolve a nominal type NAME to its canonical (possibly module-qualified)
/// Sym, per the #433 rules. THE single place this qualification predicate
/// lives — annotations (via `resolve_type_expr_in`) and the checker's record
/// construction inference both call it, so the producers cannot diverge.
///
/// - A user module's bare reference to its own declared type → `mod.Type`.
/// - An already-qualified reference to a USER module's type → kept qualified.
/// - A bare reference to an IMPORTED user-module type (e.g. module `b` uses
///   module `d`'s `Logger` as a bare `Logger` brought in by `import d`) →
///   the unique owner's `X.Type` key, so it mangles to the same struct. Only
///   when EXACTLY ONE user module declares that bare name — otherwise it is
///   ambiguous and stays bare (a root-local type, which has no `X.Type` key,
///   also falls through to bare).
/// - Stdlib / local / unknown names → None (stay bare).
pub fn canonical_user_type_sym(name: &str, types: &HashMap<Sym, Ty>, cur_mod: Option<&str>) -> Option<Sym> {
    if let Some(m) = cur_mod {
        if !name.contains('.') && !almide_lang::stdlib_info::is_bundled_module(m) {
            let qual = format!("{}.{}", m, name);
            if let Some(t) = types.get(&sym(&qual)) {
                if matches!(t, Ty::Record { .. } | Ty::Variant { .. }) {
                    return Some(sym(&qual));
                }
            }
        }
    }
    if let Some((m, _bare)) = name.rsplit_once('.') {
        if !almide_lang::stdlib_info::is_bundled_module(m) {
            if let Some(t) = types.get(&sym(name)) {
                if matches!(t, Ty::Record { .. } | Ty::Variant { .. }) {
                    return Some(sym(name));
                }
            }
        }
    }
    if !name.contains('.') {
        // A LOCAL (main-program, unprefixed) type registered under the bare name
        // shadows a dependency's same-name type for unqualified use (#433). Prefer
        // the bare entry when it is structurally DISTINCT from every qualified
        // `<pkg>.name` owner — i.e. it is a genuine local type, not merely the
        // dependency's bare alias (which mirrors its qualified entry exactly).
        // Only for the main program (`cur_mod` is None); a user module's own types
        // are already qualified and handled by the first block above.
        if cur_mod.is_none() {
            if let Some(bare) = types.get(&sym(name)) {
                if matches!(bare, Ty::Record { .. } | Ty::Variant { .. }) {
                    let is_alias_of_a_qualified = types.iter().any(|(k, v)| {
                        k.as_str().rsplit_once('.').map_or(false, |(p, base)| {
                            base == name && !almide_lang::stdlib_info::is_bundled_module(p)
                        }) && v == bare
                    });
                    if !is_alias_of_a_qualified {
                        return Some(sym(name));
                    }
                }
            }
        }
        let mut owners = types.iter().filter(|(k, v)| {
            k.as_str().rsplit_once('.').map_or(false, |(p, base)| {
                base == name && !almide_lang::stdlib_info::is_bundled_module(p)
            }) && matches!(v, Ty::Record { .. } | Ty::Variant { .. })
        });
        if let Some((k, _)) = owners.next() {
            if owners.next().is_none() {
                return Some(*k);
            }
        }
    }
    None
}

/// Like `resolve_type_expr`, but aware of the module currently being resolved
/// (`cur_mod`), so a USER module's reference to one of its own types is pinned to
/// the qualified canonical name `mod.Type` instead of the bare name. This is what
/// keeps two packages' same-name types distinct end-to-end (#433). Stdlib modules
/// are exempt — their types stay bare to match the bare-named Rust runtime.
pub fn resolve_type_expr_in(te: &ast::TypeExpr, known_types: Option<&HashMap<Sym, Ty>>, cur_mod: Option<&str>) -> Ty {
    // Resolve a nominal name to its canonical (possibly module-qualified) `Ty::Named`.
    let resolve_named = |other: &str| -> Option<Ty> {
        canonical_user_type_sym(other, known_types?, cur_mod).map(|s| Ty::Named(s, vec![]))
    };
    match te {
        ast::TypeExpr::Simple { name } => match name.as_str() {
            "Int" => Ty::Int,
            "Float" => Ty::Float,
            // Sized numeric types (Stage 1a of the sized-numeric-types arc).
            // `Int64` / `Float64` alias to `Ty::Int` / `Ty::Float` — writing
            // either form is indistinguishable at the type checker layer, so
            // existing code that uses `Int` keeps compiling while new code
            // can use the precise width name.
            "Int64" => Ty::Int64,
            "Float64" => Ty::Float64,
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
                // #433: a user module's (qualified) reference to a namespaced
                // type resolves to its canonical `mod.Type` name; falls through
                // to the existing bare resolution for stdlib / local types.
                if let Some(qualified) = resolve_named(other) {
                    return qualified;
                }
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
            let ra: Vec<Ty> = args.iter().map(|a| resolve_type_expr_in(a, known_types, cur_mod)).collect();
            match name.as_str() {
                "List" => Ty::list(ra.first().cloned().unwrap_or(Ty::Unknown)),
                "Option" => Ty::option(ra.first().cloned().unwrap_or(Ty::Unknown)),
                "Result" if ra.len() >= 2 => Ty::result(ra[0].clone(), ra[1].clone()),
                "Map" if ra.len() >= 2 => Ty::map_of(ra[0].clone(), ra[1].clone()),
                "Set" => Ty::set_of(ra.first().cloned().unwrap_or(Ty::Unknown)),
                // Sized Numeric Types P4 kickoff: `Matrix[T]` resolves
                // to `Applied(Matrix, [T])` so the checker can discriminate
                // `Matrix[Float32]` / `Matrix[Float64]`. Bare `Matrix`
                // (no args) stays as `Ty::Matrix` — the compat rule in
                // `types/mod.rs` bridges bare `Matrix` ↔ `Matrix[Float]`.
                "Matrix" => Ty::Applied(TypeConstructorId::Matrix, ra),
                _ => {
                    // #433: qualify a user module's generic type to its canonical
                    // `mod.Type` name; stdlib / local generics stay bare.
                    if let Some(Ty::Named(qn, _)) = resolve_named(name.as_str()) {
                        Ty::Named(qn, ra)
                    } else {
                        let resolved_name = name.as_str().rsplit_once('.').map(|(_, bare)| sym(bare)).unwrap_or(*name);
                        Ty::Named(resolved_name, ra)
                    }
                },
            }
        },
        ast::TypeExpr::Record { fields } => Ty::Record {
            fields: fields.iter().map(|f| (sym(&f.name), resolve_type_expr_in(&f.ty, known_types, cur_mod))).collect(),
        },
        ast::TypeExpr::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|f| (sym(&f.name), resolve_type_expr_in(&f.ty, known_types, cur_mod))).collect(),
        },
        ast::TypeExpr::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| resolve_type_expr_in(p, known_types, cur_mod)).collect(),
            ret: Box::new(resolve_type_expr_in(ret, known_types, cur_mod)),
        },
        ast::TypeExpr::Tuple { elements } => Ty::Tuple(
            elements.iter().map(|e| resolve_type_expr_in(e, known_types, cur_mod)).collect(),
        ),
        ast::TypeExpr::Union { members } => Ty::union(
            members.iter().map(|m| resolve_type_expr_in(m, known_types, cur_mod)).collect(),
        ),
        ast::TypeExpr::ConstLit { value } => Ty::ConstValue { ty: Box::new(Ty::Int), value: *value },
        ast::TypeExpr::Variant { cases } => {
            let cs = cases.iter().map(|c| match c {
                ast::VariantCase::Unit { name } => VariantCase {
                    name: sym(name), payload: VariantPayload::Unit,
                },
                ast::VariantCase::Tuple { name, fields } => VariantCase {
                    name: sym(name),
                    payload: VariantPayload::Tuple(
                        fields.iter().map(|f| resolve_type_expr_in(f, known_types, cur_mod)).collect(),
                    ),
                },
                ast::VariantCase::Record { name, fields } => VariantCase {
                    name: sym(name),
                    payload: VariantPayload::Record(
                        fields.iter().map(|f| (sym(&f.name), resolve_type_expr_in(&f.ty, known_types, cur_mod))).collect(),
                    ),
                },
            }).collect();
            Ty::Variant { name: sym(""), cases: cs }
        },
    }
}
