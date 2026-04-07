/// Pattern match exhaustiveness checking via Maranget's usefulness algorithm.
///
/// Reference: Luc Maranget, "Warnings for pattern matching" (JFP, 2007).
///
/// Replaces the previous flat set-based check with a matrix-based approach
/// that handles nested patterns, tuples, and infinite domains.

use almide_lang::ast;
use almide_base::intern::Sym;
use crate::types::{Ty, TypeConstructorId, VariantPayload};
use crate::type_env::TypeEnv;
use std::collections::HashSet;

// ────────────────────────────────────────────────
//  Internal pattern representation
// ────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum Pat {
    /// Constructor applied to sub-patterns.
    Ctor(CtorId, Vec<Pat>),
    /// Matches anything (wildcard or variable binding).
    Wild,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum CtorId {
    Variant(Sym),
    Some,
    None,
    Ok,
    Err,
    True,
    False,
    Tuple,
    /// Literal values (used for Int/Float/String).
    /// Stored as display string for Eq/Hash compatibility.
    Lit(String),
}

/// Describes the constructor space for a type.
enum CtorSet {
    /// Finite enumerable constructors (variant, option, result, bool).
    Finite(Vec<CtorId>),
    /// Single constructor — always present (tuple).
    Single(CtorId),
    /// Infinite domain (int, float, string) — wildcard always required.
    Infinite,
    /// Unknown or unanalyzable type — skip check.
    Opaque,
}

// ────────────────────────────────────────────────
//  Lower AST patterns to internal representation
// ────────────────────────────────────────────────

fn lower(pat: &ast::Pattern) -> Pat {
    match pat {
        ast::Pattern::Wildcard | ast::Pattern::Ident { .. } => Pat::Wild,
        ast::Pattern::Constructor { name, args, .. } => {
            // Normalize module-qualified names: "binary.Unreachable" → "Unreachable"
            let bare = name.as_str().rsplit_once('.').map(|(_, b)| almide_base::intern::sym(b)).unwrap_or(*name);
            Pat::Ctor(CtorId::Variant(bare), args.iter().map(lower).collect())
        }
        // Record variant: constructor-level only (field depth deferred to Phase 4).
        ast::Pattern::RecordPattern { name, .. } => {
            let bare = name.as_str().rsplit_once('.').map(|(_, b)| almide_base::intern::sym(b)).unwrap_or(*name);
            Pat::Ctor(CtorId::Variant(bare), vec![])
        }
        ast::Pattern::Some { inner, .. } => Pat::Ctor(CtorId::Some, vec![lower(inner)]),
        ast::Pattern::None => Pat::Ctor(CtorId::None, vec![]),
        ast::Pattern::Ok { inner, .. } => Pat::Ctor(CtorId::Ok, vec![lower(inner)]),
        ast::Pattern::Err { inner, .. } => Pat::Ctor(CtorId::Err, vec![lower(inner)]),
        ast::Pattern::Tuple { elements, .. } => {
            Pat::Ctor(CtorId::Tuple, elements.iter().map(lower).collect())
        }
        ast::Pattern::List { elements, .. } => {
            Pat::Ctor(CtorId::Tuple, elements.iter().map(lower).collect())
        }
        ast::Pattern::Literal { value, .. } => lower_literal(value),
    }
}

fn lower_literal(expr: &ast::Expr) -> Pat {
    match &expr.kind {
        ast::ExprKind::Bool { value, .. } => {
            Pat::Ctor(if *value { CtorId::True } else { CtorId::False }, vec![])
        }
        ast::ExprKind::Int { raw, .. } => Pat::Ctor(CtorId::Lit(raw.clone()), vec![]),
        ast::ExprKind::Float { value, .. } => Pat::Ctor(CtorId::Lit(format!("{value}")), vec![]),
        ast::ExprKind::String { value, .. } => {
            Pat::Ctor(CtorId::Lit(format!("\"{value}\"")), vec![])
        }
        _ => Pat::Wild,
    }
}

// ────────────────────────────────────────────────
//  Type → constructor information
// ────────────────────────────────────────────────

fn ctor_set(ty: &Ty, env: &TypeEnv) -> CtorSet {
    let resolved = env.resolve_named(ty);
    match &resolved {
        Ty::Variant { cases, .. } => {
            CtorSet::Finite(cases.iter().map(|c| CtorId::Variant(c.name)).collect())
        }
        Ty::Applied(TypeConstructorId::Option, _) => {
            CtorSet::Finite(vec![CtorId::Some, CtorId::None])
        }
        Ty::Applied(TypeConstructorId::Result, _) => {
            CtorSet::Finite(vec![CtorId::Ok, CtorId::Err])
        }
        Ty::Bool => CtorSet::Finite(vec![CtorId::True, CtorId::False]),
        Ty::Tuple(_) => CtorSet::Single(CtorId::Tuple),
        Ty::Int | Ty::Float | Ty::String => CtorSet::Infinite,
        _ => CtorSet::Opaque,
    }
}

/// Number of sub-patterns a constructor expands to.
fn arity(ctor: &CtorId, ty: &Ty, env: &TypeEnv) -> usize {
    let resolved = env.resolve_named(ty);
    match ctor {
        CtorId::Variant(name) => match &resolved {
            Ty::Variant { cases, .. } => {
                cases.iter().find(|c| c.name == *name).map_or(0, |c| match &c.payload {
                    VariantPayload::Unit => 0,
                    VariantPayload::Tuple(tys) => tys.len(),
                    VariantPayload::Record(_) => 0, // Phase 4
                })
            }
            _ => 0,
        },
        CtorId::Some | CtorId::Ok | CtorId::Err => 1,
        CtorId::None | CtorId::True | CtorId::False | CtorId::Lit(_) => 0,
        CtorId::Tuple => match &resolved {
            Ty::Tuple(tys) => tys.len(),
            _ => 0,
        },
    }
}

/// Types of sub-patterns when specializing by a constructor.
fn field_types(ctor: &CtorId, ty: &Ty, env: &TypeEnv) -> Vec<Ty> {
    let resolved = env.resolve_named(ty);
    match ctor {
        CtorId::Variant(name) => match &resolved {
            Ty::Variant { cases, .. } => {
                cases.iter().find(|c| c.name == *name).map_or(vec![], |c| match &c.payload {
                    VariantPayload::Unit => vec![],
                    VariantPayload::Tuple(tys) => tys.clone(),
                    VariantPayload::Record(_) => vec![], // Phase 4
                })
            }
            _ => vec![],
        },
        CtorId::Some => match &resolved {
            Ty::Applied(TypeConstructorId::Option, args) if !args.is_empty() => {
                vec![args[0].clone()]
            }
            _ => vec![Ty::Unknown],
        },
        CtorId::Ok => match &resolved {
            Ty::Applied(TypeConstructorId::Result, args) if !args.is_empty() => {
                vec![args[0].clone()]
            }
            _ => vec![Ty::Unknown],
        },
        CtorId::Err => match &resolved {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() >= 2 => {
                vec![args[1].clone()]
            }
            _ => vec![Ty::Unknown],
        },
        CtorId::Tuple => match &resolved {
            Ty::Tuple(tys) => tys.clone(),
            _ => vec![],
        },
        CtorId::None | CtorId::True | CtorId::False | CtorId::Lit(_) => vec![],
    }
}

// ────────────────────────────────────────────────
//  Matrix operations
// ────────────────────────────────────────────────

/// Collect distinct constructors in the first column.
fn head_ctors(matrix: &[Vec<Pat>]) -> Vec<CtorId> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for row in matrix {
        if let Some(Pat::Ctor(c, _)) = row.first() {
            if seen.insert(c.clone()) {
                result.push(c.clone());
            }
        }
    }
    result
}

/// Specialize the matrix by constructor `ctor` with the given `arity`.
///
/// Keeps only rows whose first column matches `ctor` (or is a wildcard),
/// expanding the constructor's sub-patterns into new columns.
fn specialize(matrix: &[Vec<Pat>], ctor: &CtorId, ar: usize) -> Vec<Vec<Pat>> {
    let mut out = Vec::new();
    for row in matrix {
        if row.is_empty() { continue; }
        match &row[0] {
            Pat::Ctor(c, args) if c == ctor => {
                let mut new_row = Vec::with_capacity(ar + row.len() - 1);
                new_row.extend(args.iter().cloned());
                // Pad or truncate to match expected arity (defensive).
                new_row.resize(ar, Pat::Wild);
                new_row.extend_from_slice(&row[1..]);
                out.push(new_row);
            }
            Pat::Wild => {
                let mut new_row = vec![Pat::Wild; ar];
                new_row.extend_from_slice(&row[1..]);
                out.push(new_row);
            }
            _ => {} // different constructor — skip
        }
    }
    out
}

/// Default matrix: rows with wildcard in first column, first column removed.
fn default_matrix(matrix: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    let mut out = Vec::new();
    for row in matrix {
        if row.is_empty() { continue; }
        if matches!(&row[0], Pat::Wild) {
            out.push(row[1..].to_vec());
        }
    }
    out
}

fn is_complete(head: &[CtorId], ty: &Ty, env: &TypeEnv) -> bool {
    match ctor_set(ty, env) {
        CtorSet::Finite(all) => all.iter().all(|c| head.contains(c)),
        CtorSet::Single(c) => head.contains(&c),
        CtorSet::Infinite => false,
        CtorSet::Opaque => true,
    }
}

fn missing_ctors(head: &[CtorId], ty: &Ty, env: &TypeEnv) -> Vec<CtorId> {
    match ctor_set(ty, env) {
        CtorSet::Finite(all) => all.into_iter().filter(|c| !head.contains(c)).collect(),
        CtorSet::Single(c) => if head.contains(&c) { vec![] } else { vec![c] },
        CtorSet::Infinite | CtorSet::Opaque => vec![],
    }
}

// ────────────────────────────────────────────────
//  Witness finding (core algorithm)
// ────────────────────────────────────────────────

/// Find a single witness pattern row that is not covered by `matrix`.
/// Returns `Some(witness)` if non-exhaustive, `None` if exhaustive.
///
/// `types[i]` is the type of column `i`. The matrix rows and `types` have the same length.
fn find_witness(matrix: &[Vec<Pat>], types: &[Ty], env: &TypeEnv) -> Option<Vec<Pat>> {
    // Base case: no columns left.
    if types.is_empty() {
        // Useful iff no row covers the empty pattern (= matrix has no rows).
        return if matrix.iter().any(|r| r.is_empty()) { Option::None } else { Some(vec![]) };
    }

    let ty = &types[0];
    let rest_types = &types[1..];
    let head = head_ctors(matrix);

    if is_complete(&head, ty, env) {
        // Every constructor is mentioned — check each one for gaps.
        let all = match ctor_set(ty, env) {
            CtorSet::Finite(all) => all,
            CtorSet::Single(c) => vec![c],
            _ => return Option::None,
        };
        for ctor in &all {
            let ar = arity(ctor, ty, env);
            let ftys = field_types(ctor, ty, env);
            let spec = specialize(matrix, ctor, ar);
            let mut sub_types = ftys;
            sub_types.extend_from_slice(rest_types);
            if let Some(mut witness) = find_witness(&spec, &sub_types, env) {
                // First `ar` elements are the constructor's fields, rest is the remaining columns.
                let fields: Vec<Pat> = witness.drain(..ar).collect();
                let mut result = vec![Pat::Ctor(ctor.clone(), fields)];
                result.extend(witness);
                return Some(result);
            }
        }
        Option::None
    } else {
        // Incomplete — some constructors not mentioned. Check the default matrix.
        let def = default_matrix(matrix);
        if let Some(mut witness) = find_witness(&def, rest_types, env) {
            let missing = missing_ctors(&head, ty, env);
            let first = if let Some(ctor) = missing.first() {
                let ar = arity(ctor, ty, env);
                Pat::Ctor(ctor.clone(), vec![Pat::Wild; ar])
            } else {
                Pat::Wild
            };
            let mut result = vec![first];
            result.append(&mut witness);
            Some(result)
        } else {
            Option::None
        }
    }
}

// ────────────────────────────────────────────────
//  Formatting
// ────────────────────────────────────────────────

fn fmt_pat(pat: &Pat) -> String {
    match pat {
        Pat::Wild => "_".into(),
        Pat::Ctor(ctor, args) => {
            let name = match ctor {
                CtorId::Variant(s) => s.to_string(),
                CtorId::Some => "some".into(),
                CtorId::None => "none".into(),
                CtorId::Ok => "ok".into(),
                CtorId::Err => "err".into(),
                CtorId::True => "true".into(),
                CtorId::False => "false".into(),
                CtorId::Tuple => String::new(),
                CtorId::Lit(v) => v.clone(),
            };
            if args.is_empty() {
                name
            } else if matches!(ctor, CtorId::Tuple) {
                let inner: Vec<_> = args.iter().map(fmt_pat).collect();
                format!("({})", inner.join(", "))
            } else {
                let inner: Vec<_> = args.iter().map(fmt_pat).collect();
                format!("{}({})", name, inner.join(", "))
            }
        }
    }
}

// ────────────────────────────────────────────────
//  Public API
// ────────────────────────────────────────────────

/// Check if a match expression is exhaustive.
///
/// Returns a list of formatted missing-pattern strings (empty = exhaustive).
/// At most 3 witnesses are reported.
pub fn check_exhaustiveness(
    subject_ty: &Ty,
    arms: &[ast::MatchArm],
    env: &TypeEnv,
) -> Vec<String> {
    let resolved = env.resolve_named(subject_ty);

    // Skip unanalyzable types.
    if matches!(ctor_set(&resolved, env), CtorSet::Opaque) {
        return vec![];
    }

    // Build 1-column matrix (skip guarded arms — guards don't guarantee coverage).
    let matrix: Vec<Vec<Pat>> = arms
        .iter()
        .filter(|a| a.guard.is_none())
        .map(|a| vec![lower(&a.pattern)])
        .collect();

    let types = vec![resolved];

    // Iteratively find up to 3 witnesses.
    let mut witnesses = Vec::new();
    let mut augmented = matrix;
    for _ in 0..3 {
        match find_witness(&augmented, &types, env) {
            Some(w) => {
                augmented.push(w.clone());
                witnesses.push(w);
            }
            Option::None => break,
        }
    }

    witnesses
        .iter()
        .map(|w| {
            debug_assert_eq!(w.len(), 1, "witness should have exactly 1 column");
            fmt_pat(w.first().unwrap_or(&Pat::Wild))
        })
        .collect()
}
