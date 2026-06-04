//! Mut param lowering for WASM: rewrite `mut` parameter functions to return
//! mutated values, and rewrite call sites to assign them back.
//!
//! WASM has no pass-by-reference. Almide's `mut` parameter modifier means
//! "this function may modify the argument", but in WASM, arguments are passed
//! by value. This pass transforms:
//!
//!   fn add_item(mut xs: List[Int], x: Int) -> Unit = list.push(xs, x)
//!   add_item(data, 1)
//!
//! Into:
//!
//!   fn add_item(xs: List[Int], x: Int) -> List[Int] = { list.push(xs, x); xs }
//!   data = add_item(data, 1)

use almide_ir::*;
use almide_lang::types::Ty;
use crate::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct MutParamLoweringPass;

impl NanoPass for MutParamLoweringPass {
    fn name(&self) -> &str { "MutParamLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Collect functions with mutated params: name → (param_indices, param_types)
        let mut mut_fns: std::collections::HashMap<String, Vec<(usize, Ty)>> = std::collections::HashMap::new();
        for func in &program.functions {
            if !func.mutated_params.is_empty() {
                let entries: Vec<(usize, Ty)> = func.mutated_params.iter()
                    .map(|&idx| (idx, func.params.get(idx).map(|p| p.ty.clone()).unwrap_or(Ty::Unknown)))
                    .collect();
                mut_fns.insert(func.name.to_string(), entries);
            }
        }
        for module in &program.modules {
            for func in &module.functions {
                if !func.mutated_params.is_empty() {
                    let entries: Vec<(usize, Ty)> = func.mutated_params.iter()
                        .map(|&idx| (idx, func.params.get(idx).map(|p| p.ty.clone()).unwrap_or(Ty::Unknown)))
                        .collect();
                    mut_fns.insert(func.name.to_string(), entries);
                }
            }
        }

        if mut_fns.is_empty() {
            return PassResult { program, changed: false };
        }

        // Phase 1: Rewrite function bodies — append mutated param as return value
        for func in &mut program.functions {
            if func.mutated_params.is_empty() { continue; }
            if func.mutated_params.len() != 1 { continue; } // only single mut param for now
            let mut_idx = func.mutated_params[0];
            let mut_var = func.params[mut_idx].var;
            let mut_ty = func.params[mut_idx].ty.clone();

            // Change return type to the mutated param's type
            if matches!(func.ret_ty, Ty::Unit) {
                func.ret_ty = mut_ty.clone();
                // Append `var` as tail expression
                let var_expr = IrExpr {
                    kind: IrExprKind::Var { id: mut_var },
                    ty: mut_ty,
                    span: None,
                    def_id: None,
                };
                // Wrap existing body in a block with the var as tail
                let old_body = std::mem::replace(&mut func.body, IrExpr {
                    kind: IrExprKind::Unit,
                    ty: Ty::Unit,
                    span: None,
                    def_id: None,
                });
                func.body = IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![IrStmt {
                            kind: IrStmtKind::Expr { expr: old_body },
                            span: None,
                        }],
                        expr: Some(Box::new(var_expr)),
                    },
                    ty: func.ret_ty.clone(),
                    span: None,
                    def_id: None,
                };
            }
        }

        // Phase 2: Rewrite call sites — assign return value back
        for func in &mut program.functions {
            rewrite_calls(&mut func.body, &mut_fns);
        }
        for tl in &mut program.top_lets {
            rewrite_calls(&mut tl.value, &mut_fns);
        }

        PassResult { program, changed: true }
    }
}

fn rewrite_calls(expr: &mut IrExpr, mut_fns: &std::collections::HashMap<String, Vec<(usize, Ty)>>) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            // Look for Expr { Call(mut_fn, args) } statements and rewrite to Assign
            let mut i = 0;
            while i < stmts.len() {
                if let IrStmtKind::Expr { expr: call_expr } = &stmts[i].kind {
                    if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &call_expr.kind {
                        if let Some(params) = mut_fns.get(name.as_str()) {
                            if params.len() == 1 {
                                let (mut_idx, ref mut_ty) = params[0];
                                if let Some(arg) = args.get(mut_idx) {
                                    if let IrExprKind::Var { id } = &arg.kind {
                                        let var_id = *id;
                                        // Rewrite: Expr { call } → Assign { var, call }
                                        let mut call = stmts[i].kind.clone();
                                        if let IrStmtKind::Expr { expr: ref mut ce } = call {
                                            ce.ty = mut_ty.clone();
                                        }
                                        if let IrStmtKind::Expr { expr: ce } = call {
                                            stmts[i] = IrStmt {
                                                kind: IrStmtKind::Assign { var: var_id, value: ce },
                                                span: stmts[i].span,
                                            };
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Recurse into statement expressions
                match &mut stmts[i].kind {
                    IrStmtKind::Bind { value, .. } => rewrite_calls(value, mut_fns),
                    IrStmtKind::Assign { value, .. } => rewrite_calls(value, mut_fns),
                    IrStmtKind::Expr { expr } => rewrite_calls(expr, mut_fns),
                    // Explicit-preserve: mutation lowering only rewrites call sites
                    // reachable through the statement kinds above. The remaining
                    // kinds are listed so a new IrStmtKind is a compile error here,
                    // not a silently-dropped subtree.
                    IrStmtKind::BindDestructure { .. } | IrStmtKind::IndexAssign { .. }
                    | IrStmtKind::MapInsert { .. } | IrStmtKind::FieldAssign { .. }
                    | IrStmtKind::Guard { .. } | IrStmtKind::Comment { .. }
                    | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. }
                    | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
                    | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => {}
                }
                i += 1;
            }
            if let Some(t) = tail { rewrite_calls(t, mut_fns); }
        }
        IrExprKind::If { cond, then, else_ } => {
            rewrite_calls(cond, mut_fns);
            rewrite_calls(then, mut_fns);
            rewrite_calls(else_, mut_fns);
        }
        IrExprKind::Match { subject, arms } => {
            rewrite_calls(subject, mut_fns);
            for arm in arms { rewrite_calls(&mut arm.body, mut_fns); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            rewrite_calls(iterable, mut_fns);
            for s in body {
                match &mut s.kind {
                    IrStmtKind::Bind { value, .. } => rewrite_calls(value, mut_fns),
                    IrStmtKind::Assign { value, .. } => rewrite_calls(value, mut_fns),
                    IrStmtKind::Expr { expr } => rewrite_calls(expr, mut_fns),
                    // Explicit-preserve (see Block above).
                    IrStmtKind::BindDestructure { .. } | IrStmtKind::IndexAssign { .. }
                    | IrStmtKind::MapInsert { .. } | IrStmtKind::FieldAssign { .. }
                    | IrStmtKind::Guard { .. } | IrStmtKind::Comment { .. }
                    | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. }
                    | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
                    | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => {}
                }
            }
        }
        IrExprKind::While { cond, body } => {
            rewrite_calls(cond, mut_fns);
            for s in body {
                match &mut s.kind {
                    IrStmtKind::Bind { value, .. } => rewrite_calls(value, mut_fns),
                    IrStmtKind::Assign { value, .. } => rewrite_calls(value, mut_fns),
                    IrStmtKind::Expr { expr } => rewrite_calls(expr, mut_fns),
                    // Explicit-preserve (see Block above).
                    IrStmtKind::BindDestructure { .. } | IrStmtKind::IndexAssign { .. }
                    | IrStmtKind::MapInsert { .. } | IrStmtKind::FieldAssign { .. }
                    | IrStmtKind::Guard { .. } | IrStmtKind::Comment { .. }
                    | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. }
                    | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
                    | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => {}
                }
            }
        }
        // Explicit-preserve: this pass walks the structural-control nodes above
        // to reach mutating call sites; the remaining expression kinds either
        // contain no statement-level call sites to rewrite or are leaves. Listing
        // every kind makes a new IrExprKind a compile error rather than a silently
        // un-rewritten (native↔WASM divergent) subtree.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Fan { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::List { .. }
        | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap
        | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }
        | IrExprKind::Tuple { .. } | IrExprKind::Range { .. }
        | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::Lambda { .. } | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. } | IrExprKind::OptionNone
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
        | IrExprKind::UnwrapOr { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. } | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}
