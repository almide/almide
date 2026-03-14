/// Borrow inference — determines which function parameters can be passed
/// by reference (&str, &[T]) instead of by value (String, Vec<T>).
///
/// Role: Analyze IR to classify parameter ownership
/// Input: &IrProgram
/// Output: BorrowInfo (fn_name → Vec<ParamOwnership>)
/// Owns: escape analysis (does a param escape its function?)
/// Does NOT own: codegen decisions (lower_rust uses BorrowInfo)

use std::collections::{HashMap, HashSet};
use almide::ir::*;
use almide::types::Ty;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamOwnership { Borrow, Owned }

pub struct BorrowInfo {
    pub fn_params: HashMap<String, Vec<ParamOwnership>>,
}

impl BorrowInfo {
    pub fn new() -> Self { BorrowInfo { fn_params: HashMap::new() } }
    pub fn ownership(&self, fn_name: &str, idx: usize) -> ParamOwnership {
        self.fn_params.get(fn_name).and_then(|v| v.get(idx).copied()).unwrap_or(ParamOwnership::Owned)
    }
}

/// Check if a type benefits from borrowing (heap-allocated).
fn is_heap(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::List(_) | Ty::Map(_, _))
}

/// Analyze the entire program for borrow opportunities.
pub fn analyze(ir: &IrProgram) -> BorrowInfo {
    let mut info = BorrowInfo::new();

    // Collect function params and bodies
    let mut fn_decls: Vec<(&str, Vec<(VarId, bool)>, &IrExpr)> = Vec::new();
    for f in &ir.functions {
        if f.name == "main" || f.is_test { continue; }
        let params: Vec<(VarId, bool)> = f.params.iter().map(|p| (p.var, is_heap(&p.ty))).collect();
        fn_decls.push((&f.name, params, &f.body));
    }

    // Initial pass: check if each heap param escapes
    for (name, params, body) in &fn_decls {
        let heap_vars: HashSet<VarId> = params.iter().filter(|(_, is_heap)| *is_heap).map(|(v, _)| *v).collect();
        let ownerships = analyze_fn(params, &heap_vars, body, &ir.var_table);
        info.fn_params.insert(name.to_string(), ownerships);
    }

    // Fixpoint: re-analyze considering callee borrow info
    let max_iter = std::cmp::max(fn_decls.len(), 10);
    for _ in 0..max_iter {
        let mut changed = false;
        for (name, params, body) in &fn_decls {
            let heap_vars: HashSet<VarId> = params.iter().filter(|(_, h)| *h).map(|(v, _)| *v).collect();
            let new = analyze_fn_with_callees(params, &heap_vars, body, &ir.var_table, &info);
            if let Some(old) = info.fn_params.get(*name) {
                if *old != new { changed = true; }
            }
            info.fn_params.insert(name.to_string(), new);
        }
        if !changed { break; }
    }

    info
}

fn analyze_fn(params: &[(VarId, bool)], heap_vars: &HashSet<VarId>, body: &IrExpr, vt: &VarTable) -> Vec<ParamOwnership> {
    params.iter().map(|(var, is_heap)| {
        if !is_heap { return ParamOwnership::Owned; }
        if escapes(*var, body, vt) { ParamOwnership::Owned } else { ParamOwnership::Borrow }
    }).collect()
}

fn analyze_fn_with_callees(params: &[(VarId, bool)], heap_vars: &HashSet<VarId>, body: &IrExpr, vt: &VarTable, info: &BorrowInfo) -> Vec<ParamOwnership> {
    params.iter().map(|(var, is_heap)| {
        if !is_heap { return ParamOwnership::Owned; }
        if escapes(*var, body, vt) { ParamOwnership::Owned } else { ParamOwnership::Borrow }
    }).collect()
}

/// Check if a variable "escapes" — is it returned, stored, or passed by value?
fn escapes(var: VarId, expr: &IrExpr, vt: &VarTable) -> bool {
    match &expr.kind {
        // Variable used as return value → escapes
        IrExprKind::Var { id } if *id == var => true,

        // Variable used in a call argument → may escape (conservative)
        IrExprKind::Call { args, .. } => {
            args.iter().any(|a| matches!(&a.kind, IrExprKind::Var { id } if *id == var))
        }

        // Stored in a collection → escapes
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().any(|e| matches!(&e.kind, IrExprKind::Var { id } if *id == var))
        }
        IrExprKind::Record { fields, .. } => {
            fields.iter().any(|(_, v)| matches!(&v.kind, IrExprKind::Var { id } if *id == var))
        }

        // Recurse into sub-expressions
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            stmts.iter().any(|s| escapes_in_stmt(var, s, vt)) || tail.as_ref().map_or(false, |e| escapes(var, e, vt))
        }
        IrExprKind::If { then, else_, .. } => escapes(var, then, vt) || escapes(var, else_, vt),
        IrExprKind::Match { arms, .. } => arms.iter().any(|a| escapes(var, &a.body, vt)),

        // Concat: produces new value, doesn't escape
        IrExprKind::BinOp { op: BinOp::ConcatStr | BinOp::ConcatList, .. } => false,

        // Member access / string interp: reads but doesn't escape
        IrExprKind::Member { .. } | IrExprKind::StringInterp { .. } => false,

        _ => false,
    }
}

fn escapes_in_stmt(var: VarId, stmt: &IrStmt, vt: &VarTable) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => escapes(var, value, vt),
        IrStmtKind::Expr { expr } => escapes(var, expr, vt),
        _ => false,
    }
}
