//! Type questions the concurrent-var-reach analysis asks: is this value a fn
//! (or a container of fns) that could run in a concurrent slot?

use crate::types::{Ty, TypeEnv};
use almide_lang::ast;
use almide_lang::types::constructor::TypeConstructorId;

/// True for a fn type, and for a List / Option / tuple of fn types, after
/// expanding type aliases (`HttpHandler`, `HttpMiddleware`).
pub fn ty_is_fn_valued(env: &TypeEnv, ty: &Ty) -> bool {
    fn go(env: &TypeEnv, ty: &Ty, depth: u32) -> bool {
        if depth > 16 {
            return false;
        }
        match ty {
            Ty::Fn { .. } => true,
            Ty::Tuple(ts) => ts.iter().any(|t| go(env, t, depth + 1)),
            Ty::Applied(TypeConstructorId::List | TypeConstructorId::Option, args) => {
                args.iter().any(|t| go(env, t, depth + 1))
            }
            Ty::Named(..) => {
                let r = env.resolve_named(ty);
                !matches!(r, Ty::Named(..)) && go(env, &r, depth + 1)
            }
            _ => false,
        }
    }
    go(env, ty, 0)
}

/// [`ty_is_fn_valued`] for a declared (unresolved) parameter type.
pub fn type_expr_is_fn_valued(env: &TypeEnv, te: &ast::TypeExpr) -> bool {
    match te {
        ast::TypeExpr::Fn { .. } => true,
        _ => ty_is_fn_valued(env, &crate::canonicalize::resolve::resolve_type_expr(te, None)),
    }
}
