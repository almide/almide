//! ANF (A-Normal Form) Pass: lift heap-typed sub-expressions to let bindings.
//!
//! Transforms nested expressions like `"[" + s + "]"` into:
//!   let __anf_0 = "[" + s
//!   __anf_0 + "]"
//!
//! This ensures every heap allocation has a VarId, so PerceusPass can
//! insert RcDec for it. Without ANF, intermediate heap values in nested
//! calls/binops are invisible to Perceus and leak in WASM.
//!
//! Correctness: `perceus_all_heap_freed` proves that after perceusTransform,
//! every VDecl with heap type gets Dec'd. ANF guarantees every heap alloc
//! IS a VDecl, closing the gap between the proof and the implementation.

use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr};
use almide_lang::types::Ty;
use super::pass::{NanoPass, Postcondition, PassResult, Target};

fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Unknown | Ty::Fn { .. })
}

/// Does this expression produce a new heap allocation that should be lifted?
///
/// Inverted logic: any heap-typed expression that is NOT a simple value
/// reference needs lifting. This way new IrExprKind variants are lifted
/// by default (safe), rather than silently leaking (unsafe).
fn needs_lift(expr: &IrExpr) -> bool {
    if !is_heap_type(&expr.ty) { return false; }
    // Expressions that don't produce NEW heap allocations never need lifting.
    // Two categories:
    //   1. Simple values: literals, variable references
    //   2. Read operations: field/index access borrows from parent object,
    //      whose lifetime Perceus already tracks
    !matches!(&expr.kind,
        // Category 1: simple values / references
        IrExprKind::Var { .. }
        | IrExprKind::EnvLoad { .. }
        | IrExprKind::FnRef { .. }
        | IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::Unit
        | IrExprKind::OptionNone
        | IrExprKind::EmptyMap
        | IrExprKind::Hole
        | IrExprKind::Todo { .. }
        | IrExprKind::Break
        | IrExprKind::Continue
        // Category 2: read operations (borrow from parent, no new alloc)
        | IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. }
        | IrExprKind::UnOp { .. }
        | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. }
        // Lambdas / closures must NOT be lifted: they must stay in
        // argument position so the WASM closure-table pre-scan and
        // HOF emission can see them. (Regression guard for 5cae928d:
        // deny-list inversion swept in Fn-typed exprs that were never
        // lifted by the prior allow-list.)
        | IrExprKind::Lambda { .. }
        | IrExprKind::ClosureCreate { .. }
    )
}

/// Replace a sub-expression with a Var reference, returning the original
/// expression and a new VarId for the let binding.
fn lift_one(
    expr: &mut IrExpr,
    var_table: &mut VarTable,
    counter: &mut u32,
) -> Option<(VarId, Ty, IrExpr)> {
    if !needs_lift(expr) { return None; }
    let ty = expr.ty.clone();
    let name = almide_base::intern::sym(&format!("__anf_{}", counter));
    *counter += 1;
    let var = var_table.alloc(name, ty.clone(), Mutability::Let, None);
    let original = std::mem::replace(expr, IrExpr {
        kind: IrExprKind::Var { id: var },
        ty: ty.clone(),
        span: None,
        def_id: None,
    });
    Some((var, ty, original))
}

/// Wrap an expression in a Block with preceding let bindings.
fn wrap_with_lets(expr: IrExpr, lifted: Vec<IrStmt>) -> IrExpr {
    if lifted.is_empty() { return expr; }
    let result_ty = expr.ty.clone();
    IrExpr {
        kind: IrExprKind::Block {
            stmts: lifted,
            expr: Some(Box::new(expr)),
        },
        ty: result_ty,
        span: None,
        def_id: None,
    }
}

/// ANF-transform an expression tree. Lifts heap sub-expressions to let bindings.
fn anf_expr(expr: &mut IrExpr, var_table: &mut VarTable, counter: &mut u32) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => anf_expr_block(stmts, tail, var_table, counter),
        IrExprKind::If { cond, then, else_ } => anf_expr_if(cond, then, else_, var_table, counter),
        IrExprKind::Match { subject, arms } => anf_expr_match(subject, arms, var_table, counter),
        IrExprKind::While { cond, body } => anf_expr_while(cond, body, var_table, counter),
        IrExprKind::ForIn { iterable, body, .. } => anf_expr_for_in(iterable, body, var_table, counter),
        IrExprKind::Lambda { body, .. } => { anf_expr(body, var_table, counter); }

        // Compound expressions with sub-expressions: recurse into children
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } =>
            anf_expr_elements(elements, var_table, counter),
        IrExprKind::Record { fields, .. } => anf_expr_field_values(fields, var_table, counter),
        IrExprKind::SpreadRecord { base, fields } => anf_expr_spread_record(base, fields, var_table, counter),
        IrExprKind::MapLiteral { entries } => anf_expr_map_literal(entries, var_table, counter),
        IrExprKind::StringInterp { parts } => anf_expr_string_interp(parts, var_table, counter),
        IrExprKind::OptionSome { expr: inner }
        | IrExprKind::ResultOk { expr: inner }
        | IrExprKind::ResultErr { expr: inner }
        | IrExprKind::Unwrap { expr: inner }
        | IrExprKind::Try { expr: inner }
        | IrExprKind::ToOption { expr: inner }
        | IrExprKind::Clone { expr: inner }
        | IrExprKind::Deref { expr: inner }
        | IrExprKind::ToVec { expr: inner }
        | IrExprKind::BoxNew { expr: inner } => {
            anf_expr(inner, var_table, counter);
        }
        IrExprKind::UnwrapOr { expr: inner, fallback } => {
            anf_expr(inner, var_table, counter);
            anf_expr(fallback, var_table, counter);
        }
        IrExprKind::Range { start, end, .. } => {
            anf_expr(start, var_table, counter);
            anf_expr(end, var_table, counter);
        }
        IrExprKind::Member { object, .. }
        | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            anf_expr(object, var_table, counter);
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            anf_expr(object, var_table, counter);
            anf_expr(index, var_table, counter);
        }

        _ => {
            // For Call, RuntimeCall, BinOp: lift heap sub-args
            anf_lift_children(expr, var_table, counter);
        }
    }
}

/// ANF-transform a statement's value/condition sub-expressions, for the
/// `Block` arm of [`anf_expr`] (statements where `Guard` is meaningfully
/// ANF'd, unlike loop bodies — see [`anf_loop_stmt`]).
fn anf_block_stmt(stmt: &mut IrStmt, var_table: &mut VarTable, counter: &mut u32) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
            anf_expr(value, var_table, counter),
        IrStmtKind::Expr { expr } =>
            anf_expr(expr, var_table, counter),
        IrStmtKind::Guard { cond, else_ } => {
            anf_expr(cond, var_table, counter);
            anf_expr(else_, var_table, counter);
        }
        // Explicit-preserve: statement kinds carrying no heap
        // sub-expression to ANF-lift (or only simple var/key refs).
        // Listed so a new IrStmtKind is a compile error, not a drop.
        IrStmtKind::BindDestructure { .. }
        | IrStmtKind::IndexAssign { .. }
        | IrStmtKind::MapInsert { .. }
        | IrStmtKind::FieldAssign { .. }
        | IrStmtKind::Comment { .. }
        | IrStmtKind::RcInc { .. }
        | IrStmtKind::RcDec { .. }
        | IrStmtKind::ListSwap { .. }
        | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. }
        | IrStmtKind::ListCopySlice { .. } => {}
    }
}

/// `Block { stmts, expr: tail }` arm of [`anf_expr`].
fn anf_expr_block(stmts: &mut [IrStmt], tail: &mut Option<Box<IrExpr>>, var_table: &mut VarTable, counter: &mut u32) {
    for stmt in stmts.iter_mut() { anf_block_stmt(stmt, var_table, counter); }
    if let Some(t) = tail { anf_expr(t, var_table, counter); }
}

/// `If { cond, then, else_ }` arm of [`anf_expr`].
fn anf_expr_if(cond: &mut IrExpr, then: &mut IrExpr, else_: &mut IrExpr, var_table: &mut VarTable, counter: &mut u32) {
    anf_expr(cond, var_table, counter);
    anf_expr(then, var_table, counter);
    anf_expr(else_, var_table, counter);
}

/// `Match { subject, arms }` arm of [`anf_expr`].
fn anf_expr_match(subject: &mut IrExpr, arms: &mut [IrMatchArm], var_table: &mut VarTable, counter: &mut u32) {
    anf_expr(subject, var_table, counter);
    for arm in arms { anf_expr(&mut arm.body, var_table, counter); }
}

/// ANF-transform a statement's value sub-expression, for a loop body
/// (`While` / `ForIn` arms of [`anf_expr`]). Unlike [`anf_block_stmt`],
/// `Guard` inside a loop body carries no heap sub-expression to ANF-lift.
fn anf_loop_stmt(stmt: &mut IrStmt, var_table: &mut VarTable, counter: &mut u32) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
            anf_expr(value, var_table, counter),
        IrStmtKind::Expr { expr } => anf_expr(expr, var_table, counter),
        // Explicit-preserve: no heap sub-expression to ANF-lift.
        IrStmtKind::Guard { .. }
        | IrStmtKind::BindDestructure { .. }
        | IrStmtKind::IndexAssign { .. }
        | IrStmtKind::MapInsert { .. }
        | IrStmtKind::FieldAssign { .. }
        | IrStmtKind::Comment { .. }
        | IrStmtKind::RcInc { .. }
        | IrStmtKind::RcDec { .. }
        | IrStmtKind::ListSwap { .. }
        | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. }
        | IrStmtKind::ListCopySlice { .. } => {}
    }
}

/// `While { cond, body }` arm of [`anf_expr`].
fn anf_expr_while(cond: &mut IrExpr, body: &mut [IrStmt], var_table: &mut VarTable, counter: &mut u32) {
    anf_expr(cond, var_table, counter);
    for stmt in body.iter_mut() { anf_loop_stmt(stmt, var_table, counter); }
}

/// `ForIn { iterable, body, .. }` arm of [`anf_expr`].
fn anf_expr_for_in(iterable: &mut IrExpr, body: &mut [IrStmt], var_table: &mut VarTable, counter: &mut u32) {
    anf_expr(iterable, var_table, counter);
    for stmt in body.iter_mut() { anf_loop_stmt(stmt, var_table, counter); }
}

/// `List { elements } | Tuple { elements } | Fan { exprs: elements }` arm of [`anf_expr`].
fn anf_expr_elements(elements: &mut [IrExpr], var_table: &mut VarTable, counter: &mut u32) {
    for elem in elements.iter_mut() { anf_expr(elem, var_table, counter); }
}

/// `Record { fields, .. }` arm of [`anf_expr`].
fn anf_expr_field_values(fields: &mut [(almide_base::intern::Sym, IrExpr)], var_table: &mut VarTable, counter: &mut u32) {
    for (_, val) in fields.iter_mut() { anf_expr(val, var_table, counter); }
}

/// `SpreadRecord { base, fields }` arm of [`anf_expr`].
fn anf_expr_spread_record(base: &mut IrExpr, fields: &mut [(almide_base::intern::Sym, IrExpr)], var_table: &mut VarTable, counter: &mut u32) {
    anf_expr(base, var_table, counter);
    for (_, val) in fields.iter_mut() { anf_expr(val, var_table, counter); }
}

/// `MapLiteral { entries }` arm of [`anf_expr`].
fn anf_expr_map_literal(entries: &mut [(IrExpr, IrExpr)], var_table: &mut VarTable, counter: &mut u32) {
    for (k, v) in entries.iter_mut() {
        anf_expr(k, var_table, counter);
        anf_expr(v, var_table, counter);
    }
}

/// `StringInterp { parts }` arm of [`anf_expr`].
fn anf_expr_string_interp(parts: &mut [IrStringPart], var_table: &mut VarTable, counter: &mut u32) {
    for part in parts.iter_mut() {
        if let IrStringPart::Expr { expr: e } = part { anf_expr(e, var_table, counter); }
    }
}

/// Lift heap-typed children of Call/BinOp expressions.
fn anf_lift_children(expr: &mut IrExpr, var_table: &mut VarTable, counter: &mut u32) {
    // Take the expression out, process it, put it back
    let mut owned = std::mem::replace(expr, IrExpr {
        kind: IrExprKind::LitInt { value: 0 }, ty: Ty::Unit, span: None, def_id: None,
    });
    let mut lifted = Vec::new();

    match &mut owned.kind {
        IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } => {
            for arg in args.iter_mut() {
                anf_expr(arg, var_table, counter);
                if let Some((var, ty, original)) = lift_one(arg, var_table, counter) {
                    lifted.push(IrStmt {
                        kind: IrStmtKind::Bind { var, ty, mutability: Mutability::Let, value: original },
                        span: None,
                    });
                }
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            anf_expr(left, var_table, counter);
            anf_expr(right, var_table, counter);
            if let Some((var, ty, original)) = lift_one(left, var_table, counter) {
                lifted.push(IrStmt {
                    kind: IrStmtKind::Bind { var, ty, mutability: Mutability::Let, value: original },
                    span: None,
                });
            }
            if let Some((var, ty, original)) = lift_one(right, var_table, counter) {
                lifted.push(IrStmt {
                    kind: IrStmtKind::Bind { var, ty, mutability: Mutability::Let, value: original },
                    span: None,
                });
            }
        }
        // Explicit-preserve: this helper only lifts the children of Call /
        // RuntimeCall / BinOp (the kinds that route here from `anf_expr`'s
        // catch-all). Every other kind has no arg-position heap child to lift
        // — but they are listed so a new IrExprKind is a compile error here,
        // not a silently un-lifted (leaking) node.
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::Unit
        | IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. }
        | IrExprKind::UnOp { .. }
        | IrExprKind::If { .. }
        | IrExprKind::Match { .. }
        | IrExprKind::Block { .. }
        | IrExprKind::Fan { .. }
        | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. }
        | IrExprKind::Break
        | IrExprKind::Continue
        | IrExprKind::TailCall { .. }
        | IrExprKind::List { .. }
        | IrExprKind::MapLiteral { .. }
        | IrExprKind::EmptyMap
        | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. }
        | IrExprKind::Tuple { .. }
        | IrExprKind::Range { .. }
        | IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. }
        | IrExprKind::Lambda { .. }
        | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. }
        | IrExprKind::OptionNone
        | IrExprKind::Try { .. }
        | IrExprKind::Unwrap { .. }
        | IrExprKind::UnwrapOr { .. }
        | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. }
        | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. }
        | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. }
        | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. }
        | IrExprKind::Hole
        | IrExprKind::Todo { .. } => {}
    }

    *expr = wrap_with_lets(owned, lifted);
}

#[derive(Debug)]
pub struct AnfPass;

impl NanoPass for AnfPass {
    fn name(&self) -> &str { "ANF" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn postconditions(&self) -> Vec<Postcondition> {
        vec![Postcondition::Custom(verify_anf_args_lifted)]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut counter = 0u32;
        for func in &mut program.functions {
            if func.is_test { continue; }
            anf_expr(&mut func.body, &mut program.var_table, &mut counter);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                anf_expr(&mut func.body, &mut program.var_table, &mut counter);
            }
        }
        PassResult { program, changed: counter > 0 }
    }
}

// ── Postcondition: verify all heap-typed args are Var after ANF ──────

fn expr_kind_name(kind: &IrExprKind) -> &'static str {
    expr_kind_name_group_a(kind)
        .or_else(|| expr_kind_name_group_b(kind))
        .unwrap_or_else(|| expr_kind_name_group_c(kind))
}

/// First third of `expr_kind_name`'s arms, extracted (cog>30 decomposition,
/// pattern 1 — independent name-router arms split into groups; mirrors the
/// `list_call_name` recipe).
fn expr_kind_name_group_a(kind: &IrExprKind) -> Option<&'static str> {
    Some(match kind {
        IrExprKind::Var { .. } => "Var",
        IrExprKind::LitInt { .. } => "LitInt",
        IrExprKind::LitFloat { .. } => "LitFloat",
        IrExprKind::LitBool { .. } => "LitBool",
        IrExprKind::LitStr { .. } => "LitStr",
        IrExprKind::Unit => "Unit",
        IrExprKind::Call { .. } => "Call",
        IrExprKind::RuntimeCall { .. } => "RuntimeCall",
        IrExprKind::BinOp { .. } => "BinOp",
        IrExprKind::UnOp { .. } => "UnOp",
        _ => return None,
    })
}

/// Second third of `expr_kind_name`'s arms, extracted (cog>30 decomposition).
fn expr_kind_name_group_b(kind: &IrExprKind) -> Option<&'static str> {
    Some(match kind {
        IrExprKind::If { .. } => "If",
        IrExprKind::Match { .. } => "Match",
        IrExprKind::Block { .. } => "Block",
        IrExprKind::Lambda { .. } => "Lambda",
        IrExprKind::List { .. } => "List",
        IrExprKind::Tuple { .. } => "Tuple",
        IrExprKind::Record { .. } => "Record",
        IrExprKind::MapLiteral { .. } => "MapLiteral",
        IrExprKind::StringInterp { .. } => "StringInterp",
        IrExprKind::ClosureCreate { .. } => "ClosureCreate",
        _ => return None,
    })
}

/// Final third of `expr_kind_name`'s arms, extracted (cog>30 decomposition).
fn expr_kind_name_group_c(kind: &IrExprKind) -> &'static str {
    match kind {
        IrExprKind::EnvLoad { .. } => "EnvLoad",
        IrExprKind::Member { .. } => "Member",
        IrExprKind::IndexAccess { .. } => "IndexAccess",
        IrExprKind::OptionSome { .. } => "OptionSome",
        IrExprKind::ResultOk { .. } => "ResultOk",
        IrExprKind::ResultErr { .. } => "ResultErr",
        IrExprKind::Fan { .. } => "Fan",
        IrExprKind::ForIn { .. } => "ForIn",
        IrExprKind::While { .. } => "While",
        IrExprKind::Range { .. } => "Range",
        _ => "Other",
    }
}

fn verify_anf_args_lifted(program: &IrProgram) -> Vec<String> {
    let mut violations = Vec::new();
    struct Checker<'a> { violations: &'a mut Vec<String>, func_name: String }

    impl<'a> IrVisitor for Checker<'a> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            // Decision-only: inspect Call/RuntimeCall args and BinOp operands for
            // un-lifted heap exprs. Recursion into children is the external
            // `walk_expr` below — so this uses `if let` (no catch-all that could
            // drop a subtree; every node is still walked exhaustively).
            if let IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } = &expr.kind {
                for arg in args {
                    if needs_lift(arg) {
                        self.violations.push(format!(
                            "[ANF] in {}: heap-typed call arg not lifted — kind={}, ty={:?}",
                            self.func_name, expr_kind_name(&arg.kind), arg.ty,
                        ));
                    }
                }
            } else if let IrExprKind::BinOp { left, right, .. } = &expr.kind {
                for arg in [left.as_ref(), right.as_ref()] {
                    if needs_lift(arg) {
                        self.violations.push(format!(
                            "[ANF] in {}: heap-typed binop operand not lifted — kind={}, ty={:?}",
                            self.func_name, expr_kind_name(&arg.kind), arg.ty,
                        ));
                    }
                }
            }
            walk_expr(self, expr);
        }
    }

    for func in &program.functions {
        if func.is_test { continue; }
        let mut checker = Checker { violations: &mut violations, func_name: func.name.as_str().to_string() };
        checker.visit_expr(&func.body);
    }
    for module in &program.modules {
        for func in &module.functions {
            let mut checker = Checker { violations: &mut violations, func_name: func.name.as_str().to_string() };
            checker.visit_expr(&func.body);
        }
    }
    violations
}
