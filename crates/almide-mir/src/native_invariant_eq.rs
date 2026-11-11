//! An equality whose operands a loop cannot change, compared once on the
//! native render (#4661). `if a == b then …` inside a loop walks both values
//! every round although neither moved: sixty rounds over two trees of a
//! hundred thousand nodes compared them sixty times. The standard render
//! lifts such a comparison ahead of the loop (`pass_licm.rs`).
//!
//! Here the comparison stays where it is written and remembers its answer:
//! ahead of the loop go `var done = false` and `var same = false`, and the
//! site becomes
//!
//! ```text
//! if not done then { same = a == b; done = true } else ()
//! if same then …
//! ```
//!
//! Equivalence. The first time control reaches the site the comparison runs
//! there, exactly as written — so a loop that never reaches it never
//! compares, and whatever ran before it (a print, an abort) still runs before
//! it. Every later time the site reads the answer the first run gave, which is
//! the answer a second run would give: both operands are places of `let`
//! bindings made outside the loop, which nothing can write, and an equality
//! reads its operands and nothing else.
//!
//! What is rewritten: `==` / `!=` over heap operands (a scalar comparison is
//! one instruction already) where it is the whole condition of an `if` or the
//! whole value of a `let`, in a statement list of the loop — not inside a
//! callback, whose captures would then be written.
use almide_ir::{free_vars::bound_vars, visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::types::Ty;
use std::collections::HashSet;

pub(crate) fn expand(ir: &mut IrProgram) {
    // A metered region charges each evaluation its plan names: such a
    // program keeps every comparison.
    if almide_ir::meter_plan::program_has_regions(ir) { return; }
    let mut pass = Loops { vars: &mut ir.var_table };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
}

struct Loops<'a> { vars: &'a mut VarTable }

impl Loops<'_> {
    /// Remember the invariant equalities of every loop standing in `stmts`
    /// (or as `tail`), the memo variables bound ahead of each.
    fn remember_all(&mut self, stmts: &mut Vec<IrStmt>, mut tail: Option<&mut IrExpr>) {
        let mut out = Vec::with_capacity(stmts.len());
        for mut s in std::mem::take(stmts) {
            if let IrStmtKind::Expr { expr } = &mut s.kind { out.extend(self.remember(expr)); }
            out.push(s);
        }
        if let Some(e) = tail.as_deref_mut() { out.extend(self.remember(e)); }
        *stmts = out;
    }

    /// The memo binds of the loop `e`, its sites rewritten to read them.
    fn remember(&mut self, e: &mut IrExpr) -> Vec<IrStmt> {
        if !matches!(e.kind, IrExprKind::ForIn { .. } | IrExprKind::While { .. }) { return Vec::new(); }
        let mut sites = Sites { vars: self.vars, inside: bound_vars(e), memos: Vec::new() };
        sites.visit_expr_mut(e);
        let unset = || node(IrExprKind::LitBool { value: false }, Ty::Bool);
        let bind = |var| IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Var, ty: Ty::Bool, value: unset() }, span: None };
        sites.memos.iter().flat_map(|m| [bind(m.done), bind(m.answer)]).collect()
    }
}

impl IrMutVisitor for Loops<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        // Outermost loop first: a comparison is remembered across the widest
        // loop that leaves its operands alone.
        match &mut e.kind {
            IrExprKind::Block { stmts, expr } => self.remember_all(stmts, expr.as_deref_mut()),
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => self.remember_all(body, None),
            _ => {}
        }
        walk_expr_mut(self, e);
    }
}

/// A place read through fields only: the variable and the path from it.
type Place = (VarId, Vec<String>);

struct Memo { negated: bool, left: Place, right: Place, done: VarId, answer: VarId }

/// The sites of one loop. `inside`: every variable the loop binds.
struct Sites<'a> { vars: &'a mut VarTable, inside: HashSet<VarId>, memos: Vec<Memo> }

impl Sites<'_> {
    /// `e` as a place no round of the loop can change.
    fn fixed_place(&self, e: &IrExpr) -> Option<Place> {
        match &e.kind {
            IrExprKind::Var { id } => {
                let outside = !self.inside.contains(id) && self.vars.get(*id).mutability == Mutability::Let;
                outside.then(|| (*id, Vec::new()))
            }
            IrExprKind::Member { object, field } => {
                let (root, mut path) = self.fixed_place(object)?;
                path.push(field.to_string());
                Some((root, path))
            }
            IrExprKind::TupleIndex { object, index } => {
                let (root, mut path) = self.fixed_place(object)?;
                path.push(index.to_string());
                Some((root, path))
            }
            _ => None,
        }
    }

    /// Where `e` is an equality of fixed heap places: the statement that
    /// fills its memo on the first run, `e` left reading the answer. `filled`
    /// holds the memos a statement earlier in the same list has filled:
    /// control that reaches `e` passed that fill, so `e` needs none.
    fn site(&mut self, e: &mut IrExpr, filled: &mut Vec<usize>) -> Option<IrStmt> {
        let IrExprKind::BinOp { op: op @ (BinOp::Eq | BinOp::Neq), left, right } = &e.kind else { return None; };
        if e.ty != Ty::Bool || matches!(left.ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit) { return None; }
        let (negated, left, right) = (*op == BinOp::Neq, self.fixed_place(left)?, self.fixed_place(right)?);
        let known = self.memos.iter().position(|m| m.negated == negated && m.left == left && m.right == right);
        let at = known.unwrap_or_else(|| {
            let done = self.vars.alloc_fresh("compared", Ty::Bool, Mutability::Var, e.span);
            let answer = self.vars.alloc_fresh("comparison", Ty::Bool, Mutability::Var, e.span);
            self.memos.push(Memo { negated, left, right, done, answer });
            self.memos.len() - 1
        });
        let (done, answer) = (self.memos[at].done, self.memos[at].answer);
        let read = |id| node(IrExprKind::Var { id }, Ty::Bool);
        let compare = std::mem::replace(e, read(answer));
        if filled.contains(&at) { return None; }
        filled.push(at);
        let set = |var, value| IrStmt { kind: IrStmtKind::Assign { var, value }, span: None };
        let fill = vec![set(answer, compare), set(done, node(IrExprKind::LitBool { value: true }, Ty::Bool))];
        let first = node(IrExprKind::UnOp { op: UnOp::Not, operand: Box::new(read(done)) }, Ty::Bool);
        let then = node(IrExprKind::Block { stmts: fill, expr: None }, Ty::Unit);
        let once = node(IrExprKind::If { cond: Box::new(first), then: Box::new(then), else_: Box::new(node(IrExprKind::Unit, Ty::Unit)) }, Ty::Unit);
        Some(IrStmt { kind: IrStmtKind::Expr { expr: once }, span: None })
    }

    /// The sites among `stmts` (and `tail`), each behind its fill.
    fn sites(&mut self, stmts: &mut Vec<IrStmt>, tail: Option<&mut IrExpr>) {
        let (mut out, mut filled) = (Vec::with_capacity(stmts.len()), Vec::new());
        for mut s in std::mem::take(stmts) {
            let whole = match &mut s.kind {
                IrStmtKind::Bind { value, .. } => Some(value),
                IrStmtKind::Expr { expr: IrExpr { kind: IrExprKind::If { cond, .. }, .. } } => Some(&mut **cond),
                _ => None,
            };
            out.extend(whole.and_then(|e| self.site(e, &mut filled)));
            out.push(s);
        }
        if let Some(IrExpr { kind: IrExprKind::If { cond, .. }, .. }) = tail { out.extend(self.site(cond, &mut filled)); }
        *stmts = out;
    }
}

impl IrMutVisitor for Sites<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        match &mut e.kind {
            IrExprKind::Lambda { .. } => return,
            IrExprKind::Block { stmts, expr } => self.sites(stmts, expr.as_deref_mut()),
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => self.sites(body, None),
            _ => {}
        }
        walk_expr_mut(self, e);
    }
}

fn node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}
