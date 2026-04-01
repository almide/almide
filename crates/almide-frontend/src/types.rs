// Re-export type definitions from almide-lang + local TypeEnv.
pub use almide_lang::types::*;
pub use crate::type_env::TypeEnv;

/// Expression type map: populated by the checker, consumed by the lower pass.
/// Replaces the former `ast::Expr.ty: Option<Ty>` field to break the ast↔types cycle.
pub type TypeMap = std::collections::HashMap<almide_lang::ast::ExprId, Ty>;
