/// IR optimization passes.
///
/// Pipeline position: Lower → IR → **optimize()** → mono() → codegen
///
/// Pass 1: Constant folding — evaluate compile-time-known expressions.
/// Pass 2: Dead code elimination — remove unused bindings with pure values.

use crate::ir::*;

// ── Public entry point ──────────────────────────────────────────

/// Run all optimization passes on an IR program.
/// Requires use-counts to be computed (done by `lower_program`).
pub fn optimize_program(program: &mut IrProgram) {
    // Pass 1: constant folding (bottom-up rewrite)
    constant_fold(program);

    // Recompute use-counts after folding may have eliminated references
    compute_use_counts(program);

    // Pass 2: dead code elimination
    eliminate_dead_code(program);

    // Pass 3: constant propagation (replace vars bound to literals with the literal)
    constant_propagate(program);

    // Recompute after propagation may have reduced use-counts
    compute_use_counts(program);

    // Pass 4: dead code elimination again (propagation may create new dead bindings)
    eliminate_dead_code(program);

    // Recompute use-counts for downstream consumers (mono, borrow analysis)
    compute_use_counts(program);
}

// ── Pass 1: Constant Folding ────────────────────────────────────

fn constant_fold(program: &mut IrProgram) {
    for f in &mut program.functions {
        fold_expr(&mut f.body);
    }
    for tl in &mut program.top_lets {
        fold_expr(&mut tl.value);
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            fold_expr(&mut f.body);
        }
        for tl in &mut m.top_lets {
            fold_expr(&mut tl.value);
        }
    }
}

fn fold_expr(expr: &mut IrExpr) {
    // Recurse first (bottom-up)
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => {
            fold_expr(left);
            fold_expr(right);
        }
        IrExprKind::UnOp { operand, .. } => fold_expr(operand),
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            for s in stmts { fold_stmt(s); }
            if let Some(t) = tail { fold_expr(t); }
        }
        IrExprKind::If { cond, then, else_ } => {
            fold_expr(cond);
            fold_expr(then);
            fold_expr(else_);
        }
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } = target {
                fold_expr(object);
            }
            for a in args { fold_expr(a); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { fold_expr(e); }
        }
        IrExprKind::Lambda { body, .. } => fold_expr(body),
        IrExprKind::Match { subject, arms } => {
            fold_expr(subject);
            for a in arms {
                if let Some(g) = &mut a.guard { fold_expr(g); }
                fold_expr(&mut a.body);
            }
        }
        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Await { expr: e } => fold_expr(e),
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { fold_expr(v); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            fold_expr(base);
            for (_, v) in fields { fold_expr(v); }
        }
        IrExprKind::Range { start, end, .. } => {
            fold_expr(start);
            fold_expr(end);
        }
        IrExprKind::IndexAccess { object, index } => {
            fold_expr(object);
            fold_expr(index);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            fold_expr(object);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { fold_expr(k); fold_expr(v); }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { fold_expr(e); }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            fold_expr(iterable);
            for s in body { fold_stmt(s); }
        }
        IrExprKind::While { cond, body } => {
            fold_expr(cond);
            for s in body { fold_stmt(s); }
        }
        _ => {}
    }

    // Now try to fold this node
    let replacement = try_fold(expr);
    if let Some(new_expr) = replacement {
        *expr = new_expr;
    }
}

/// Try to reduce an expression to a simpler form.
/// Returns Some(replacement) if the node can be folded.
fn try_fold(expr: &IrExpr) -> Option<IrExpr> {
    match &expr.kind {
        // ── Arithmetic / string / bool on literals ──
        IrExprKind::BinOp { op, left, right } => {
            let folded_kind = match (&left.kind, &right.kind) {
                (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b }) => {
                    match op {
                        BinOp::AddInt => Some(IrExprKind::LitInt { value: a.wrapping_add(*b) }),
                        BinOp::SubInt => Some(IrExprKind::LitInt { value: a.wrapping_sub(*b) }),
                        BinOp::MulInt => Some(IrExprKind::LitInt { value: a.wrapping_mul(*b) }),
                        BinOp::DivInt if *b != 0 => Some(IrExprKind::LitInt { value: a / b }),
                        BinOp::ModInt if *b != 0 => Some(IrExprKind::LitInt { value: a % b }),
                        _ => None,
                    }
                }
                (IrExprKind::LitFloat { value: a }, IrExprKind::LitFloat { value: b }) => {
                    match op {
                        BinOp::AddFloat => Some(IrExprKind::LitFloat { value: a + b }),
                        BinOp::SubFloat => Some(IrExprKind::LitFloat { value: a - b }),
                        BinOp::MulFloat => Some(IrExprKind::LitFloat { value: a * b }),
                        BinOp::DivFloat if *b != 0.0 => Some(IrExprKind::LitFloat { value: a / b }),
                        _ => None,
                    }
                }
                (IrExprKind::LitStr { value: a }, IrExprKind::LitStr { value: b }) => {
                    match op {
                        BinOp::ConcatStr => Some(IrExprKind::LitStr { value: format!("{}{}", a, b) }),
                        _ => None,
                    }
                }
                (IrExprKind::LitBool { value: a }, IrExprKind::LitBool { value: b }) => {
                    match op {
                        BinOp::And => Some(IrExprKind::LitBool { value: *a && *b }),
                        BinOp::Or  => Some(IrExprKind::LitBool { value: *a || *b }),
                        _ => None,
                    }
                }
                _ => None,
            };
            folded_kind.map(|kind| IrExpr { kind, ty: expr.ty.clone(), span: expr.span })
        }

        // ── Unary on literals ──
        IrExprKind::UnOp { op, operand } => {
            let folded_kind = match (op, &operand.kind) {
                (UnOp::NegInt,   IrExprKind::LitInt   { value }) => Some(IrExprKind::LitInt   { value: -value }),
                (UnOp::NegFloat, IrExprKind::LitFloat { value }) => Some(IrExprKind::LitFloat { value: -value }),
                (UnOp::Not,      IrExprKind::LitBool  { value }) => Some(IrExprKind::LitBool  { value: !value }),
                _ => None,
            };
            folded_kind.map(|kind| IrExpr { kind, ty: expr.ty.clone(), span: expr.span })
        }

        // ── if true then a else b → a,  if false then a else b → b ──
        IrExprKind::If { cond, then, else_ } => {
            match &cond.kind {
                IrExprKind::LitBool { value: true }  => Some(then.as_ref().clone()),
                IrExprKind::LitBool { value: false } => Some(else_.as_ref().clone()),
                _ => None,
            }
        }

        _ => None,
    }
}

fn fold_stmt(stmt: &mut IrStmt) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => fold_expr(value),
        IrStmtKind::IndexAssign { index, value, .. } => {
            fold_expr(index);
            fold_expr(value);
        }
        IrStmtKind::Guard { cond, else_ } => {
            fold_expr(cond);
            fold_expr(else_);
        }
        IrStmtKind::Expr { expr } => fold_expr(expr),
        IrStmtKind::Comment { .. } => {}
    }
}

// ── Pass 3: Constant Propagation ────────────────────────────────

use std::collections::HashMap;

fn constant_propagate(program: &mut IrProgram) {
    for f in &mut program.functions {
        let constants = collect_constants(&f.body);
        if !constants.is_empty() {
            propagate_expr(&mut f.body, &constants);
        }
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            let constants = collect_constants(&f.body);
            if !constants.is_empty() {
                propagate_expr(&mut f.body, &constants);
            }
        }
    }
}

/// Collect `let x = <literal>` bindings where x is immutable.
fn collect_constants(expr: &IrExpr) -> HashMap<VarId, IrExpr> {
    let mut out = HashMap::new();
    collect_constants_inner(expr, &mut out);
    out
}

fn collect_constants_inner(expr: &IrExpr, out: &mut HashMap<VarId, IrExpr>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            for s in stmts {
                if let IrStmtKind::Bind { var, value, mutability, .. } = &s.kind {
                    if matches!(mutability, Mutability::Let) && is_propagatable(value) {
                        out.insert(*var, value.clone());
                    }
                }
            }
            if let Some(t) = tail { collect_constants_inner(t, out); }
        }
        _ => {}
    }
}

/// Literals and simple Var references are safe to propagate.
fn is_propagatable(expr: &IrExpr) -> bool {
    matches!(&expr.kind,
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit
    )
}

/// Replace Var references with their constant values.
fn propagate_expr(expr: &mut IrExpr, constants: &HashMap<VarId, IrExpr>) {
    // Check if this Var can be replaced
    if let IrExprKind::Var { id } = &expr.kind {
        if let Some(replacement) = constants.get(id) {
            *expr = replacement.clone();
            return;
        }
    }
    // Recurse into subexpressions
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => {
            propagate_expr(left, constants);
            propagate_expr(right, constants);
        }
        IrExprKind::UnOp { operand, .. } => propagate_expr(operand, constants),
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            for s in stmts { propagate_stmt(s, constants); }
            if let Some(t) = tail { propagate_expr(t, constants); }
        }
        IrExprKind::If { cond, then, else_ } => {
            propagate_expr(cond, constants);
            propagate_expr(then, constants);
            propagate_expr(else_, constants);
        }
        IrExprKind::Match { subject, arms } => {
            propagate_expr(subject, constants);
            for a in arms {
                if let Some(g) = &mut a.guard { propagate_expr(g, constants); }
                propagate_expr(&mut a.body, constants);
            }
        }
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } = target {
                propagate_expr(object, constants);
            }
            for a in args { propagate_expr(a, constants); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { propagate_expr(e, constants); }
        }
        IrExprKind::Lambda { body, .. } => propagate_expr(body, constants),
        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Await { expr: e } => propagate_expr(e, constants),
        IrExprKind::Record { fields, .. } => { for (_, v) in fields { propagate_expr(v, constants); } }
        IrExprKind::SpreadRecord { base, fields } => {
            propagate_expr(base, constants);
            for (_, v) in fields { propagate_expr(v, constants); }
        }
        IrExprKind::Range { start, end, .. } => {
            propagate_expr(start, constants);
            propagate_expr(end, constants);
        }
        IrExprKind::IndexAccess { object, index } => {
            propagate_expr(object, constants);
            propagate_expr(index, constants);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            propagate_expr(object, constants);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { propagate_expr(k, constants); propagate_expr(v, constants); }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { propagate_expr(e, constants); }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            propagate_expr(iterable, constants);
            for s in body { propagate_stmt(s, constants); }
        }
        IrExprKind::While { cond, body } => {
            propagate_expr(cond, constants);
            for s in body { propagate_stmt(s, constants); }
        }
        _ => {}
    }
}

fn propagate_stmt(stmt: &mut IrStmt, constants: &HashMap<VarId, IrExpr>) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => propagate_expr(value, constants),
        IrStmtKind::IndexAssign { index, value, .. } => {
            propagate_expr(index, constants);
            propagate_expr(value, constants);
        }
        IrStmtKind::Guard { cond, else_ } => {
            propagate_expr(cond, constants);
            propagate_expr(else_, constants);
        }
        IrStmtKind::Expr { expr } => propagate_expr(expr, constants),
        IrStmtKind::Comment { .. } => {}
    }
}

// ── Pass 2: Dead Code Elimination ───────────────────────────────

fn eliminate_dead_code(program: &mut IrProgram) {
    for f in &mut program.functions {
        dce_expr(&mut f.body, &program.var_table);
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            dce_expr(&mut f.body, &m.var_table);
        }
    }
}

fn dce_expr(expr: &mut IrExpr, var_table: &VarTable) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            for s in stmts.iter_mut() { dce_stmt(s, var_table); }
            dce_stmts(stmts, var_table);
            if let Some(t) = tail { dce_expr(t, var_table); }
        }
        IrExprKind::If { cond, then, else_ } => {
            dce_expr(cond, var_table);
            dce_expr(then, var_table);
            dce_expr(else_, var_table);
        }
        IrExprKind::Match { subject, arms } => {
            dce_expr(subject, var_table);
            for a in arms { dce_expr(&mut a.body, var_table); }
        }
        IrExprKind::Lambda { body, .. } => dce_expr(body, var_table),
        IrExprKind::ForIn { body, .. } => {
            for s in body.iter_mut() { dce_stmt(s, var_table); }
            dce_stmts(body, var_table);
        }
        IrExprKind::While { body, .. } => {
            for s in body.iter_mut() { dce_stmt(s, var_table); }
            dce_stmts(body, var_table);
        }
        _ => {}
    }
}

fn dce_stmt(stmt: &mut IrStmt, var_table: &VarTable) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } => dce_expr(value, var_table),
        IrStmtKind::Expr { expr } => dce_expr(expr, var_table),
        IrStmtKind::Guard { cond, else_ } => {
            dce_expr(cond, var_table);
            dce_expr(else_, var_table);
        }
        _ => {}
    }
}

/// Remove `let x = <pure>` statements where x has use_count == 0.
fn dce_stmts(stmts: &mut Vec<IrStmt>, var_table: &VarTable) {
    stmts.retain(|stmt| {
        match &stmt.kind {
            IrStmtKind::Bind { var, value, .. } => {
                if var_table.use_count(*var) == 0 && is_pure(value) {
                    return false; // remove
                }
                true
            }
            _ => true,
        }
    });
}

/// An expression is pure if evaluating it has no side effects.
/// Conservative: anything we're unsure about is treated as impure.
fn is_pure(expr: &IrExpr) -> bool {
    match &expr.kind {
        // Literals are always pure
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::OptionNone | IrExprKind::EmptyMap => true,

        // Variable references are pure
        IrExprKind::Var { .. } => true,

        // Operators on pure operands are pure
        IrExprKind::BinOp { left, right, .. } => is_pure(left) && is_pure(right),
        IrExprKind::UnOp { operand, .. } => is_pure(operand),

        // Collection constructors with pure elements
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().all(is_pure)
        }
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| is_pure(v)),
        IrExprKind::Range { start, end, .. } => is_pure(start) && is_pure(end),

        // Wrapping pure values
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } => is_pure(expr),

        // Member/index on pure base
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => is_pure(object),

        // Lambda is pure (it's just a value, not invoked)
        IrExprKind::Lambda { .. } => true,

        // String interpolation with pure parts
        IrExprKind::StringInterp { parts } => {
            parts.iter().all(|p| match p {
                IrStringPart::Lit { .. } => true,
                IrStringPart::Expr { expr } => is_pure(expr),
            })
        }

        // Everything else (calls, blocks, loops, if, match, etc.) is conservatively impure
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Ty;

    fn lit_int(v: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None }
    }

    fn lit_str(v: &str) -> IrExpr {
        IrExpr { kind: IrExprKind::LitStr { value: v.to_string() }, ty: Ty::String, span: None }
    }

    fn lit_bool(v: bool) -> IrExpr {
        IrExpr { kind: IrExprKind::LitBool { value: v }, ty: Ty::Bool, span: None }
    }

    #[test]
    fn fold_int_add() {
        let mut e = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(lit_int(1)),
                right: Box::new(lit_int(2)),
            },
            ty: Ty::Int,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitInt { value: 3 }));
    }

    #[test]
    fn fold_str_concat() {
        let mut e = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::ConcatStr,
                left: Box::new(lit_str("a")),
                right: Box::new(lit_str("b")),
            },
            ty: Ty::String,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitStr { ref value } if value == "ab"));
    }

    #[test]
    fn fold_not_true() {
        let mut e = IrExpr {
            kind: IrExprKind::UnOp {
                op: UnOp::Not,
                operand: Box::new(lit_bool(true)),
            },
            ty: Ty::Bool,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitBool { value: false }));
    }

    #[test]
    fn fold_if_true() {
        let mut e = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(lit_bool(true)),
                then: Box::new(lit_int(10)),
                else_: Box::new(lit_int(20)),
            },
            ty: Ty::Int,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitInt { value: 10 }));
    }

    #[test]
    fn fold_if_false() {
        let mut e = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(lit_bool(false)),
                then: Box::new(lit_int(10)),
                else_: Box::new(lit_int(20)),
            },
            ty: Ty::Int,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitInt { value: 20 }));
    }

    #[test]
    fn dce_removes_unused_pure_binding() {
        let mut var_table = VarTable::new();
        let x = var_table.alloc("x".into(), Ty::Int, Mutability::Let, None);
        // x has use_count 0

        let mut stmts = vec![
            IrStmt {
                kind: IrStmtKind::Bind {
                    var: x,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: lit_int(42),
                },
                span: None,
            },
        ];
        dce_stmts(&mut stmts, &var_table);
        assert!(stmts.is_empty());
    }

    #[test]
    fn dce_keeps_used_binding() {
        let mut var_table = VarTable::new();
        let x = var_table.alloc("x".into(), Ty::Int, Mutability::Let, None);
        var_table.increment_use(x);

        let mut stmts = vec![
            IrStmt {
                kind: IrStmtKind::Bind {
                    var: x,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: lit_int(42),
                },
                span: None,
            },
        ];
        dce_stmts(&mut stmts, &var_table);
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn dce_keeps_impure_unused_binding() {
        let mut var_table = VarTable::new();
        let x = var_table.alloc("x".into(), Ty::Int, Mutability::Let, None);
        // x has use_count 0, but value is a call (impure)

        let mut stmts = vec![
            IrStmt {
                kind: IrStmtKind::Bind {
                    var: x,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Named { name: "expensive".into() },
                            args: vec![],
                            type_args: vec![],
                        },
                        ty: Ty::Int,
                        span: None,
                    },
                },
                span: None,
            },
        ];
        dce_stmts(&mut stmts, &var_table);
        assert_eq!(stmts.len(), 1); // kept because call may have side effects
    }
}
