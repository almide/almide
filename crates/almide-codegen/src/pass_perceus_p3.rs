/// Collect every var that is moved out of its defining block as a bare-`Var`
/// block tail. The block's value *is* that var, so ownership transfers to
/// whatever consumes the block (an enclosing Bind, a return, a call arg); that
/// consumer carries the Dec. Such a var therefore must not be required to have a
/// Dec inside its own block — without this set the leak rule would false-positive
/// on every ANF-lifted tail temporary (e.g. `__perceus_ret`, `__anf_*`).
///
/// Exhaustive `IrVisitor` walk — total by construction, so a newly added node
/// kind that nests blocks cannot silently drop a moved-out tail (unlike the
/// hand-rolled, tail-context `collect_all_tail_vars`, which deliberately does not
/// descend into discarded statement positions).
fn collect_moved_out_vars(expr: &IrExpr, vars: &mut HashSet<VarId>) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct MovedOutCollector<'a> { vars: &'a mut HashSet<VarId> }
    impl IrVisitor for MovedOutCollector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            match &expr.kind {
                IrExprKind::Block { expr: Some(tail), .. } => {
                    if let IrExprKind::Var { id } = &tail.kind {
                        self.vars.insert(*id);
                    }
                }
                // A bare-`Var` iterated by `for x in v` is CONSUMED (the borrow
                // inference marks a for-in iterable as owned), so ownership is moved
                // into the loop and a missing Dec in the var's block is a move, not
                // a leak. Without this, `let cs = […]; for c in cs { … }` (cs's last
                // use) is false-positive-flagged — the snaidhm `make_circle` case.
                IrExprKind::ForIn { iterable, .. } => {
                    if let IrExprKind::Var { id } = &iterable.kind {
                        self.vars.insert(*id);
                    }
                }
                _ => {}
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            almide_ir::visit::walk_stmt(self, stmt);
        }
    }
    MovedOutCollector { vars }.visit_expr(expr);
}

/// Control-flow-aware verification: check branches independently.
/// For each if/else or match, verify that heap vars bound in an outer scope
/// are not leaked on any single branch.
fn verify_branch_balance(
    expr: &IrExpr,
    outer_heap_vars: &HashSet<VarId>,
    env_load_vars: &HashSet<VarId>,
    var_table: &VarTable,
    fn_name: &str,
) {
    BranchBalance { heap_vars: outer_heap_vars.clone(), env_load_vars, var_table, fn_name }
        .visit_expr(expr);
}

/// Diagnostic (post-insertion, no Rc decision): reports a heap var Dec'd on one
/// branch but not another. Rides the exhaustive IrVisitor walk — the special
/// Block (extend heap_vars) and If/Match (branch-consistency) arms are kept; the
/// default delegates to walk_expr so no node kind drops its subtree.
struct BranchBalance<'a> {
    heap_vars: HashSet<VarId>,
    env_load_vars: &'a HashSet<VarId>,
    var_table: &'a VarTable,
    fn_name: &'a str,
}

impl IrVisitor for BranchBalance<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Block { stmts, .. } => {
                let saved = self.heap_vars.clone();
                for stmt in stmts {
                    if let IrStmtKind::Bind { var, ty, .. } = &stmt.kind {
                        if is_heap_type(ty) && !self.env_load_vars.contains(var) {
                            self.heap_vars.insert(*var);
                        }
                    }
                }
                walk_expr(self, expr); // recurse stmts + tail with the extended heap_vars
                self.heap_vars = saved;
            }
            IrExprKind::If { cond, then, else_ } => {
                self.visit_expr(cond);
                // Both branches should Dec each outer heap var referenced in BOTH.
                let then_decs = collect_decs_in_expr(then);
                let else_decs = collect_decs_in_expr(else_);
                let mut then_refs = HashSet::new();
                let mut else_refs = HashSet::new();
                collect_var_refs_expr(then, &mut then_refs);
                collect_var_refs_expr(else_, &mut else_refs);
                for var in &self.heap_vars {
                    let in_both = then_refs.contains(var) && else_refs.contains(var);
                    if !in_both { continue; }
                    let in_then = then_decs.contains(var);
                    let in_else = else_decs.contains(var);
                    if in_then != in_else {
                        let name = self.var_table.get(*var).name.as_str();
                        eprintln!(
                            "[perceus-belt] BRANCH-LEAK: `{}` (VarId {}) in `{}` — Dec'd in {} but not {}",
                            name, var.0, self.fn_name,
                            if in_then { "then" } else { "else" },
                            if in_then { "else" } else { "then" },
                        );
                    }
                }
                self.visit_expr(then);
                self.visit_expr(else_);
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                if arms.len() > 1 {
                    let arm_decs: Vec<HashSet<VarId>> = arms.iter()
                        .map(|arm| collect_decs_in_expr(&arm.body))
                        .collect();
                    let arm_refs: Vec<HashSet<VarId>> = arms.iter()
                        .map(|arm| { let mut r = HashSet::new(); collect_var_refs_expr(&arm.body, &mut r); r })
                        .collect();
                    for var in &self.heap_vars {
                        let ref_count = arm_refs.iter().filter(|r| r.contains(var)).count();
                        if ref_count < 2 { continue; }
                        let first_has = arm_decs[0].contains(var);
                        for (i, decs) in arm_decs.iter().enumerate().skip(1) {
                            if decs.contains(var) != first_has {
                                let name = self.var_table.get(*var).name.as_str();
                                eprintln!(
                                    "[perceus-belt] BRANCH-LEAK: `{}` (VarId {}) in `{}` — Dec'd in arm 0 but not arm {}",
                                    name, var.0, self.fn_name, i,
                                );
                            }
                        }
                    }
                }
                for arm in arms { self.visit_expr(&arm.body); }
            }
            _ => walk_expr(self, expr),
        }
    }
}

/// Collect all VarIds that are Dec'd within an expression.
fn collect_decs_in_expr(expr: &IrExpr) -> HashSet<VarId> {
    let mut decs = HashSet::new();
    scan_decs(expr, &mut decs);
    decs
}

fn scan_decs(expr: &IrExpr, decs: &mut HashSet<VarId>) {
    DecCollector { decs }.visit_expr(expr);
}

/// Collects every RcDec'd var (diagnostic input to verify_branch_balance), riding
/// the exhaustive IrVisitor walk so no node kind drops its subtree.
struct DecCollector<'a> {
    decs: &'a mut HashSet<VarId>,
}

impl IrVisitor for DecCollector<'_> {
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        if let IrStmtKind::RcDec { var } = &stmt.kind { self.decs.insert(*var); }
        walk_stmt(self, stmt);
    }
}

/// Recursively collect all variables used in tail expressions at any nesting level.
fn collect_all_tail_vars(expr: &IrExpr, vars: &mut HashSet<VarId>) {
    // Tail-CONTEXT analysis: only positions that flow to a return are walked. A
    // plain exhaustive walk would over-collect (treat consumed Call args etc. as
    // returned) and so suppress leak diagnostics, so the non-tail node kinds are
    // listed as explicit no-ops, not recursed — total by construction, semantics
    // unchanged.
    match &expr.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            // Variables in this block's tail
            collect_var_refs_expr(tail, vars);
            // Recurse into statements' values
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } => collect_all_tail_vars(value, vars),
                    IrStmtKind::Assign { value, .. } => collect_all_tail_vars(value, vars),
                    IrStmtKind::Expr { expr } => collect_all_tail_vars(expr, vars),
                    IrStmtKind::BindDestructure { .. } | IrStmtKind::FieldAssign { .. }
                    | IrStmtKind::Guard { .. } | IrStmtKind::IndexAssign { .. }
                    | IrStmtKind::MapInsert { .. } | IrStmtKind::ListSwap { .. }
                    | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
                    | IrStmtKind::ListCopySlice { .. } | IrStmtKind::RcInc { .. }
                    | IrStmtKind::RcDec { .. } | IrStmtKind::Comment { .. } => {}
                }
            }
            collect_all_tail_vars(tail, vars);
        }
        IrExprKind::Block { stmts, expr: None } => {
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } => collect_all_tail_vars(value, vars),
                    IrStmtKind::Assign { .. } | IrStmtKind::Expr { .. }
                    | IrStmtKind::BindDestructure { .. } | IrStmtKind::FieldAssign { .. }
                    | IrStmtKind::Guard { .. } | IrStmtKind::IndexAssign { .. }
                    | IrStmtKind::MapInsert { .. } | IrStmtKind::ListSwap { .. }
                    | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
                    | IrStmtKind::ListCopySlice { .. } | IrStmtKind::RcInc { .. }
                    | IrStmtKind::RcDec { .. } | IrStmtKind::Comment { .. } => {}
                }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_all_tail_vars(cond, vars);
            collect_all_tail_vars(then, vars);
            collect_all_tail_vars(else_, vars);
        }
        IrExprKind::Match { subject, arms } => {
            collect_all_tail_vars(subject, vars);
            for arm in arms { collect_all_tail_vars(&arm.body, vars); }
        }
        IrExprKind::Lambda { body, .. } => collect_all_tail_vars(body, vars),
        // No return/tail position in any other node kind — listed so a new kind
        // forces a tail-or-not decision instead of silently joining this no-op set.
        IrExprKind::Await { .. } | IrExprKind::BinOp { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::Break | IrExprKind::Call { .. }
        | IrExprKind::Clone { .. } | IrExprKind::ClosureCreate { .. } | IrExprKind::Continue
        | IrExprKind::Deref { .. } | IrExprKind::EmptyMap | IrExprKind::EnvLoad { .. }
        | IrExprKind::Fan { .. } | IrExprKind::FnRef { .. } | IrExprKind::ForIn { .. }
        | IrExprKind::Hole | IrExprKind::IndexAccess { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::List { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::LitFloat { .. } | IrExprKind::LitInt { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::MapLiteral { .. } | IrExprKind::Member { .. }
        | IrExprKind::OptionNone | IrExprKind::OptionSome { .. } | IrExprKind::OptionalChain { .. }
        | IrExprKind::Range { .. } | IrExprKind::RcWrap { .. } | IrExprKind::Record { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::ResultErr { .. } | IrExprKind::ResultOk { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::RustMacro { .. } | IrExprKind::SpreadRecord { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::TailCall { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::Todo { .. } | IrExprKind::Try { .. }
        | IrExprKind::Tuple { .. } | IrExprKind::TupleIndex { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Unit | IrExprKind::Unwrap { .. } | IrExprKind::UnwrapOr { .. }
        | IrExprKind::Var { .. } | IrExprKind::While { .. } => {}
    }
}

fn collect_var_refs_expr(expr: &IrExpr, refs: &mut HashSet<VarId>) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct VarCollector<'a> { refs: &'a mut HashSet<VarId> }
    impl IrVisitor for VarCollector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Var { id } = &expr.kind { self.refs.insert(*id); }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            almide_ir::visit::walk_stmt(self, stmt);
        }
    }
    VarCollector { refs }.visit_expr(expr);
}

fn collect_var_refs_stmt(stmt: &IrStmt, refs: &mut HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => collect_var_refs_expr(value, refs),
        IrStmtKind::Assign { var, value } => { refs.insert(*var); collect_var_refs_expr(value, refs); }
        IrStmtKind::Expr { expr } => collect_var_refs_expr(expr, refs),
        IrStmtKind::Guard { cond, else_ } => { collect_var_refs_expr(cond, refs); collect_var_refs_expr(else_, refs); }
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => { refs.insert(*var); }
        // Explicit no-op, NOT a recurse: this ref set drives last-use → RcDec
        // placement, so its semantics must not change. Listing every remaining
        // kind makes a new IrStmtKind a compile error here, never a silent drop.
        IrStmtKind::BindDestructure { .. }
        | IrStmtKind::IndexAssign { .. } | IrStmtKind::MapInsert { .. }
        | IrStmtKind::FieldAssign { .. } | IrStmtKind::ListSwap { .. }
        | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
        | IrStmtKind::ListCopySlice { .. } | IrStmtKind::Comment { .. } => {}
    }
}
