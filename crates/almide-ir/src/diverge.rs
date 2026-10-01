//! A diverging operand ends its expression: a target-independent IR rewrite.
//!
//! `panic(..)`, `process.exit(..)` and a user fn declared `-> Never` are typed
//! `Never`. Written where a VALUE is evaluated — an interpolation part, an
//! operand, a call argument, a list, tuple or record element — the operands to
//! its left still run, left to right (C-192), and then the program aborts at it
//! (C-219). The value the surrounding expression would have built is never
//! built. Every leg had to give that expression a type anyway, and they could
//! not (#3144):
//!
//! - native rendered `Never` as `()`, so `format!("{} {}", f(), <panic>)` asked
//!   rustc for `(): Display` (E0277), `vec![g(), die("p")]` found `()` where it
//!   wanted `i64`, and `let n: Int = panic("p")` bound `let n: () = …`;
//! - the structural wasm leg walled `diverging-call-untyped` (a diverging call
//!   with no expected type), `bind-ty:Never`, or `bind-ty:Record` for a record
//!   whose field type is `Never`.
//!
//! # The rewrite
//!
//! The expression the source asks for is exactly "evaluate the operands to the
//! left, then diverge", so that is what this pass writes, once, for every leg:
//!
//! ```text
//! "${f()} ${panic("p")}"      ⟹   { f(); panic("p") }      : String
//! add(g(), die("p"))          ⟹   { g(); die("p") }        : Int
//! { …; let n = <Never>; rest }  ⟹   { …; <Never> }
//! ```
//!
//! An operand that comes after the diverging one never runs, so it is dropped,
//! as are the statements after a `let` whose value diverges (its variable is
//! never bound, so nothing after it can be reached). The replacement block
//! keeps the expression's type, and its tail diverges, so an enclosing operand
//! position rewrites in turn and the cut reaches the nearest statement.
//!
//! Only UNCONDITIONALLY evaluated operands are cut: the right side of `and` /
//! `or`, a `??` fallback, `if` / `match` arms, a `guard`'s else, a lambda body
//! and a loop body keep their own control flow — each leg already lowers a
//! diverging call there under the arm's own expected type.

use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use crate::{BinOp, CallTarget, IrExpr, IrExprKind, IrProgram, IrStmt, IrStmtKind, IrStringPart};
use almide_lang::types::Ty;

/// Rewrite every diverging operand and every `let` of a diverging value in the
/// program (functions, top-level lets, every module). Returns the number of
/// sites rewritten.
pub fn cut_diverging_operands(program: &mut IrProgram) -> usize {
    let mut cut = Cut { rewritten: 0 };
    for f in program.functions.iter_mut() {
        cut.visit_expr_mut(&mut f.body);
    }
    for tl in program.top_lets.iter_mut() {
        cut.visit_expr_mut(&mut tl.value);
    }
    for m in program.modules.iter_mut() {
        for f in m.functions.iter_mut() {
            cut.visit_expr_mut(&mut f.body);
        }
        for tl in m.top_lets.iter_mut() {
            cut.visit_expr_mut(&mut tl.value);
        }
    }
    cut.rewritten
}

struct Cut {
    rewritten: usize,
}

impl IrMutVisitor for Cut {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Children first, so a cut operand already diverges when its parent
        // is examined, and the cut climbs to the nearest statement.
        walk_expr_mut(self, expr);
        if self.cut_block(expr) || self.cut_operands(expr) {
            self.rewritten += 1;
        }
    }
}

impl Cut {
    /// `{ …; let x = <Never>; rest }` ⟹ `{ …; <Never> }`.
    fn cut_block(&mut self, expr: &mut IrExpr) -> bool {
        let block_ty = expr.ty.clone();
        let IrExprKind::Block { stmts, expr: tail } = &mut expr.kind else { return false };
        let Some(i) = stmts.iter().position(binds_never) else { return false };
        let mut value = match stmts.swap_remove(i).kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => value,
            _ => unreachable!("binds_never matched a non-binding statement"),
        };
        stmts.truncate(i);
        // The value takes the tail's place, so it takes the tail's type — a
        // block cut by `cut_operands` still carries the bound value's.
        if value.ty != Ty::Never {
            value.ty = block_ty;
        }
        *tail = Some(Box::new(value));
        true
    }

    /// An expression whose unconditionally evaluated operand diverges ⟹ the
    /// operands to its left as statements, then the diverging one.
    fn cut_operands(&mut self, expr: &mut IrExpr) -> bool {
        let mut ops = strict_operands(&mut expr.kind);
        let Some(k) = ops.iter().position(|o| diverges(o)) else { return false };
        let stmts: Vec<IrStmt> = ops
            .drain(..k)
            .filter(|o| !is_inert(o))
            .map(|o| IrStmt { span: o.span, kind: IrStmtKind::Expr { expr: std::mem::take(o) } })
            .collect();
        let diverging = std::mem::take(&mut *ops[0]);
        drop(ops);
        // The block keeps the type of the expression it replaces: a slot that
        // reads its child's type (a lambda body's, an arm's) still reads the
        // one the checker gave it. That the block diverges is structural —
        // its tail does — which is what `diverges` follows upward.
        let ty = std::mem::replace(&mut expr.ty, Ty::Unit);
        *expr = IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(diverging)) },
            ty,
            span: expr.span,
            def_id: None,
        };
        true
    }
}

/// Control never comes back from `e`: it is typed `Never` (a diverging call,
/// an `if` whose every arm diverges), or it is a block whose tail diverges —
/// the shape this pass leaves behind.
fn diverges(e: &IrExpr) -> bool {
    match &e.kind {
        _ if e.ty == Ty::Never => true,
        IrExprKind::Block { expr: Some(tail), .. } => diverges(tail),
        _ => false,
    }
}

/// A `let` (plain or destructuring) whose value diverges.
fn binds_never(s: &IrStmt) -> bool {
    match &s.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => diverges(value),
        _ => false,
    }
}

/// An operand with no effect to keep: evaluating it as a statement before the
/// diverging operand would only be noise in every leg's output.
fn is_inert(e: &IrExpr) -> bool {
    matches!(
        e.kind,
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
            | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. }
            | IrExprKind::FnRef { .. } | IrExprKind::EmptyMap | IrExprKind::OptionNone
            | IrExprKind::Lambda { .. }
    )
}

/// The operands of `kind` that run every time `kind` runs, in evaluation
/// order. Empty for a node whose children are conditional or deferred.
fn strict_operands(kind: &mut IrExprKind) -> Vec<&mut IrExpr> {
    match kind {
        // `and` / `or` short-circuit: only the left side always runs.
        IrExprKind::BinOp { op: BinOp::And | BinOp::Or, left, .. } => vec![&mut **left],
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::Range { start: a, end: b, .. }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::MapAccess { object: a, key: b } => vec![&mut **a, &mut **b],
        IrExprKind::UnOp { operand: e, .. }
        | IrExprKind::Member { object: e, .. }
        | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e }
        | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e }
        | IrExprKind::ToOption { expr: e }
        // The fallback of `??` runs only on the miss; the subject always does.
        | IrExprKind::UnwrapOr { expr: e, .. }
        // A condition / subject / iterable always runs; the arms do not.
        | IrExprKind::If { cond: e, .. }
        | IrExprKind::Match { subject: e, .. }
        | IrExprKind::ForIn { iterable: e, .. } => vec![&mut **e],
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter_mut().collect(),
        IrExprKind::Record { fields, .. } => fields.iter_mut().map(|(_, v)| v).collect(),
        IrExprKind::SpreadRecord { base, fields } => {
            std::iter::once(&mut **base).chain(fields.iter_mut().map(|(_, v)| v)).collect()
        }
        IrExprKind::MapLiteral { entries } => entries.iter_mut().flat_map(|(k, v)| [k, v]).collect(),
        IrExprKind::StringInterp { parts } => parts
            .iter_mut()
            .filter_map(|p| match p {
                IrStringPart::Expr { expr } => Some(expr),
                IrStringPart::Lit { .. } => None,
            })
            .collect(),
        IrExprKind::Call { target, args, .. } => {
            let head = match target {
                CallTarget::Method { object, .. } => Some(&mut **object),
                CallTarget::Computed { callee } => Some(&mut **callee),
                CallTarget::Named { .. } | CallTarget::Module { .. } => None,
            };
            head.into_iter().chain(args.iter_mut()).collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mutability, VarTable};
    use almide_base::intern::sym;

    fn e(kind: IrExprKind, ty: Ty) -> IrExpr {
        IrExpr { kind, ty, span: None, def_id: None }
    }

    fn call(name: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
        e(IrExprKind::Call { target: CallTarget::Named { name: sym(name) }, args, type_args: vec![] }, ty)
    }

    fn panic_call() -> IrExpr {
        call("panic", vec![e(IrExprKind::LitStr { value: "p".into() }, Ty::String)], Ty::Never)
    }

    fn run(body: IrExpr) -> (IrExpr, usize) {
        let mut cut = Cut { rewritten: 0 };
        let mut b = body;
        cut.visit_expr_mut(&mut b);
        (b, cut.rewritten)
    }

    #[test]
    fn interp_part_keeps_left_parts_and_drops_right_ones() {
        let interp = e(
            IrExprKind::StringInterp {
                parts: vec![
                    IrStringPart::Expr { expr: call("f", vec![], Ty::String) },
                    IrStringPart::Lit { value: " ".into() },
                    IrStringPart::Expr { expr: panic_call() },
                    IrStringPart::Expr { expr: call("h", vec![], Ty::String) },
                ],
            },
            Ty::String,
        );
        let (out, n) = run(interp);
        assert_eq!(n, 1);
        assert_eq!(out.ty, Ty::String, "the block keeps the interpolation's type");
        let IrExprKind::Block { stmts, expr: Some(tail) } = &out.kind else { panic!("not a block: {:?}", out.kind) };
        assert_eq!(stmts.len(), 1, "only f() runs before the panic");
        assert!(matches!(&stmts[0].kind, IrStmtKind::Expr { expr } if matches!(&expr.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. } if name.as_str() == "f")));
        assert!(matches!(&tail.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. } if name.as_str() == "panic"));
    }

    #[test]
    fn a_let_of_a_diverging_value_ends_its_block() {
        let mut vt = VarTable::new();
        let n = vt.alloc(sym("n"), Ty::Never, Mutability::Let, None);
        let block = e(
            IrExprKind::Block {
                stmts: vec![
                    IrStmt { kind: IrStmtKind::Expr { expr: call("g", vec![], Ty::Int) }, span: None },
                    IrStmt { kind: IrStmtKind::Bind { var: n, mutability: Mutability::Let, ty: Ty::Never, value: panic_call() }, span: None },
                    IrStmt { kind: IrStmtKind::Expr { expr: call("h", vec![e(IrExprKind::Var { id: n }, Ty::Never)], Ty::Unit) }, span: None },
                ],
                expr: Some(Box::new(e(IrExprKind::Unit, Ty::Unit))),
            },
            Ty::Unit,
        );
        let (out, _) = run(block);
        let IrExprKind::Block { stmts, expr: Some(tail) } = &out.kind else { panic!() };
        assert_eq!(stmts.len(), 1);
        assert_eq!(tail.ty, Ty::Never);
        assert_eq!(out.ty, Ty::Unit, "the block keeps its own type");
    }

    #[test]
    fn the_short_circuit_side_and_branch_arms_are_left_alone() {
        let and = e(
            IrExprKind::BinOp {
                op: BinOp::And,
                left: Box::new(call("c", vec![], Ty::Bool)),
                right: Box::new(panic_call()),
            },
            Ty::Bool,
        );
        let (out, n) = run(and);
        assert_eq!(n, 0);
        assert!(matches!(out.kind, IrExprKind::BinOp { .. }));
        let iff = e(
            IrExprKind::If {
                cond: Box::new(call("c", vec![], Ty::Bool)),
                then: Box::new(panic_call()),
                else_: Box::new(e(IrExprKind::LitInt { value: 1 }, Ty::Int)),
            },
            Ty::Int,
        );
        let (_, n) = run(iff);
        assert_eq!(n, 0);
    }

    #[test]
    fn the_cut_climbs_to_the_nearest_statement() {
        // println("${f()} ${panic("p")}") — the interpolation becomes Never, so
        // the println call is replaced by it.
        let interp = e(
            IrExprKind::StringInterp {
                parts: vec![
                    IrStringPart::Expr { expr: call("f", vec![], Ty::String) },
                    IrStringPart::Expr { expr: panic_call() },
                ],
            },
            Ty::String,
        );
        let (out, n) = run(call("println", vec![interp], Ty::Unit));
        assert_eq!(n, 2);
        assert_eq!(out.ty, Ty::Unit, "the call's block keeps the call's type");
        assert!(diverges(&out));
    }
}
