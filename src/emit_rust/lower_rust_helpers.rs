/// Helper functions for Rust IR lowering (split from lower_rust.rs for max-lines).
use almide::ir::*;
use almide::types::Ty;
use super::rust_ir::*;

/// Check if a type directly contains a Named type with the given name (for recursive type detection).
/// Skips List/Option/Map internals since they are already heap-indirected.
pub(super) fn ty_contains_name(ty: &Ty, name: &str) -> bool {
    match ty {
        Ty::Named(n, args) => n == name || args.iter().any(|a| ty_contains_name(a, name)),
        Ty::Tuple(ts) => ts.iter().any(|t| ty_contains_name(t, name)),
        _ => false,
    }
}

pub(super) fn contains_typevar(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_) => true,
        Ty::List(inner) | Ty::Option(inner) => contains_typevar(inner),
        Ty::Result(a, b) | Ty::Map(a, b) => contains_typevar(a) || contains_typevar(b),
        Ty::Tuple(ts) => ts.iter().any(contains_typevar),
        Ty::Named(_, args) => args.iter().any(contains_typevar),
        Ty::Fn { params, ret } => params.iter().any(contains_typevar) || contains_typevar(ret),
        _ => false,
    }
}

/// Check if an expression already produces a Result (Ok/Err), including through
/// if/match/block where all branches are Result-producing.
pub(super) fn is_result_expr(e: &Expr) -> bool {
    match e {
        Expr::Ok(_) | Expr::Err(_) => true,
        Expr::Return(Some(inner)) => is_result_expr(inner),
        Expr::If { then, else_: Some(else_), .. } => is_result_expr(then) && is_result_expr(else_),
        Expr::Match { arms, .. } => !arms.is_empty() && arms.iter().all(|a| is_result_expr(&a.body)),
        Expr::Block { tail: Some(t), .. } => is_result_expr(t),
        Expr::Block { stmts, tail: None } => stmts.iter().any(|s| stmt_has_result_return(s)),
        _ => false,
    }
}

pub(super) fn stmt_has_result_return(s: &Stmt) -> bool {
    match s {
        Stmt::Expr(e) => expr_has_result_return(e),
        _ => false,
    }
}

fn expr_has_result_return(e: &Expr) -> bool {
    match e {
        Expr::Return(Some(inner)) => is_result_expr(inner),
        Expr::Block { stmts, tail } => {
            stmts.iter().any(|s| stmt_has_result_return(s))
                || tail.as_ref().map_or(false, |t| expr_has_result_return(t))
        }
        Expr::Loop { body, .. } => body.iter().any(|s| stmt_has_result_return(s)),
        Expr::If { then, else_, .. } => {
            expr_has_result_return(then) || else_.as_ref().map_or(false, |e| expr_has_result_return(e))
        }
        _ => false,
    }
}

/// Map Almide derive conventions to Rust #[derive(...)] attributes.
pub(super) fn rust_derives(td: &IrTypeDecl) -> Vec<String> {
    let mut derives = vec!["Clone".to_string()];
    let conventions = td.deriving.as_deref().unwrap_or_default();
    derives.push("PartialEq".into());
    if conventions.iter().any(|d| d == "Eq") { derives.push("Eq".into()); }
    derives.push("Debug".into());
    if conventions.iter().any(|d| d == "Ord") { derives.push("PartialOrd".into()); derives.push("Ord".into()); }
    if conventions.iter().any(|d| d == "Hash") { derives.push("Hash".into()); }
    derives
}
