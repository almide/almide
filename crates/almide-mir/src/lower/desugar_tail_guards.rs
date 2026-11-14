// The position rule of the guard → `if` restructure (`desugar_guard`), split
// out of desugar.rs for the 800-line file budget; `include!`d there.

/// Does a guard's else END ITS OWN PATH wherever the guard stands? A loop exit
/// (`continue` / `break`, or a block ending in one, #3115), a bare `err(e)` /
/// `err(e)!` (in a branch arm that IS the early return on every leg), and a
/// diverging expression do. Any other else is a VALUE: as the else arm of an
/// `if` it is what its block yields, which a statement drops.
fn guard_else_ends_its_path(else_: &IrExpr) -> bool {
    let raise = |e: &IrExpr| matches!(e.kind, IrExprKind::ResultErr { .. });
    raise(else_)
        || matches!(&else_.kind, IrExprKind::Unwrap { expr } if raise(expr))
        || loop_exit_else(else_.clone()).is_some()
        || almide_ir::diverge::diverges(else_)
}

/// Where an expression stands, for the guard restructure.
#[derive(Clone, Copy)]
struct GuardPos {
    /// It is a STATEMENT: its value is dropped and control carries on after it.
    stmt: bool,
    /// Dropping its value leads straight to the enclosing block yielding this
    /// variable: it is the last statement of a block whose tail is the bare
    /// `Var`. The move-mode rewrite builds exactly this (`{ <body>; buf }`, a
    /// body guard's exit rewritten to `buf`, `almide_ir::mut_param`), and there
    /// `if c then { rest } else buf` in the statement IS the exit.
    falls_to: Option<VarId>,
}

const VALUE_POS: GuardPos = GuardPos { stmt: false, falls_to: None };
const STMT_POS: GuardPos = GuardPos { stmt: true, falls_to: None };

impl GuardPos {
    /// May `guard c else <else_>` at this position become `if c then { rest }
    /// else <else_>`? Always where the block's value is used; in a statement
    /// only when the else ends its own path, or names the variable the block
    /// yields right after.
    fn restructures(self, else_: &IrExpr) -> bool {
        !self.stmt
            || guard_else_ends_its_path(else_)
            || matches!(else_.kind, IrExprKind::Var { id } if self.falls_to == Some(id))
    }
}

/// Map a statement list's expressions, telling `f` where each stands: an
/// expression statement is a statement, every other statement's expressions
/// are values. `last` is the position of the list's final expression
/// statement (the block's own position when it has no tail).
fn map_stmt_positions(
    stmts: Vec<almide_ir::IrStmt>,
    last: GuardPos,
    f: &mut impl FnMut(IrExpr, GuardPos) -> IrExpr,
) -> Vec<almide_ir::IrStmt> {
    let n = stmts.len();
    stmts
        .into_iter()
        .enumerate()
        .map(|(i, s)| match s.kind {
            IrStmtKind::Expr { expr } => {
                let at = if i + 1 == n { last } else { STMT_POS };
                almide_ir::IrStmt { kind: IrStmtKind::Expr { expr: f(expr, at) }, span: s.span }
            }
            _ => s.map_exprs(&mut |c| f(c, VALUE_POS)),
        })
        .collect()
}

/// Rewrite `e`'s children, each at its own position: the branches of an `if` /
/// `match` and a block's tail stand where the node stands, a loop body's
/// statements are statements, and every other child (a bind's value, an
/// operand, a lambda body) is a value.
///
/// The position is what the guard restructure needs (#4655). `{ guard c else
/// E; rest }` → `if c then { rest } else E` makes `E` the block's VALUE: the
/// fn's result at its tail, the bound value under a `let`. In a statement the
/// value is dropped and the fn carried on past the failed guard — `if c then {
/// guard n > 0 else -1; … } else ()` followed by more code, in an `-> Int` fn,
/// rendered and returned the fn's tail instead of `-1`.
fn map_guard_positions(e: IrExpr, at: GuardPos, f: &mut impl FnMut(IrExpr, GuardPos) -> IrExpr) -> IrExpr {
    let IrExpr { kind, ty, span, def_id } = e;
    let kind = match kind {
        IrExprKind::Block { stmts, expr } => {
            let last = match expr.as_deref().map(|t| &t.kind) {
                None => at,
                Some(IrExprKind::Var { id }) if !at.stmt => GuardPos { stmt: true, falls_to: Some(*id) },
                Some(_) => STMT_POS,
            };
            let stmts = map_stmt_positions(stmts, last, f);
            IrExprKind::Block { stmts, expr: expr.map(|t| Box::new(f(*t, at))) }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(f(*cond, VALUE_POS)),
            then: Box::new(f(*then, at)),
            else_: Box::new(f(*else_, at)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(f(*subject, VALUE_POS)),
            arms: arms
                .into_iter()
                .map(|a| almide_ir::IrMatchArm {
                    pattern: a.pattern,
                    guard: a.guard.map(|g| f(g, VALUE_POS)),
                    body: f(a.body, at),
                })
                .collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var,
            var_tuple,
            iterable: Box::new(f(*iterable, VALUE_POS)),
            body: map_stmt_positions(body, STMT_POS, f),
        },
        IrExprKind::While { cond, body } => {
            IrExprKind::While { cond: Box::new(f(*cond, VALUE_POS)), body: map_stmt_positions(body, STMT_POS, f) }
        }
        other => {
            return IrExpr { kind: other, ty, span, def_id }.map_children(&mut |c| f(c, VALUE_POS));
        }
    };
    IrExpr { kind, ty, span, def_id }
}
