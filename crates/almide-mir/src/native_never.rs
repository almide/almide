//! Expose the body of a statically resolved, raw Never call to MIR lowering.
//! The callee is retained with its diverging ABI. At a value-producing join,
//! its real abort/control-flow operations must be checked: a declared Never
//! name by itself is not an ownership-termination witness. Arguments are bound
//! once in source order; local binders are fresh and globals retain identity.
use almide_ir::{visit_mut::{walk_expr_mut, walk_stmt_mut, walk_pattern_mut, IrMutVisitor}, *};
use almide_lang::{intern::Sym, types::Ty};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn expand(ir: &mut IrProgram) {
    if almide_ir::meter_plan::program_has_regions(ir) { return; }
    let abi = almide_ir::effect_abi::effect_abi_facts(ir);
    let fns: BTreeMap<_, _> = ir.functions.iter().filter(|f|
        f.ret_ty == Ty::Never && !f.is_test && f.extern_attrs.is_empty()
        && f.params.iter().all(|p| !p.is_mut)
        && (!f.is_effect || abi.never_err_lifted.contains(f.name.as_str()))
    ).map(|f| (f.name, f.clone())).collect();
    if fns.is_empty() { return; }
    let mut pass = Inline { fns: &fns, vars: &mut ir.var_table, stack: Vec::new(), remaining: 4096 };
    for f in &mut ir.functions {
        pass.stack.push(f.name);
        pass.visit_expr_mut(&mut f.body);
        pass.stack.pop();
    }
    for t in &mut ir.top_lets { pass.visit_expr_mut(&mut t.value); }
}

struct Inline<'a> {
    fns: &'a BTreeMap<Sym, IrFunction>, vars: &'a mut VarTable,
    stack: Vec<Sym>, remaining: usize,
}
impl IrMutVisitor for Inline<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = &e.kind else { return; };
        if e.ty != Ty::Never || !type_args.is_empty() || self.stack.contains(name) { return; }
        let Some(f) = self.fns.get(name) else { return; };
        if args.len() != f.params.len() { return; }
        let mut body = f.body.clone();
        let mut scan = Bindings::default(); scan.visit_expr_mut(&mut body);
        if scan.opaque || scan.nodes > self.remaining { return; }
        self.remaining -= scan.nodes;
        scan.vars.extend(f.params.iter().map(|p| p.var));
        let map: BTreeMap<_, _> = scan.vars.into_iter().map(|v| {
            let info = self.vars.get(v).clone();
            (v, self.vars.alloc_like(v, info.ty, info.mutability))
        }).collect();
        Rename(&map).visit_expr_mut(&mut body);
        let stmts = f.params.iter().zip(args).map(|(p, value)| IrStmt {
            kind: IrStmtKind::Bind { var: map[&p.var], mutability: Mutability::Let,
                ty: p.ty.clone(), value: value.clone() }, span: e.span,
        }).collect();
        self.stack.push(*name);
        self.visit_expr_mut(&mut body);
        self.stack.pop();
        e.kind = IrExprKind::Block { stmts, expr: Some(Box::new(body)) };
        e.def_id = None;
    }
}

#[derive(Default)]
struct Bindings { vars: BTreeSet<VarId>, opaque: bool, nodes: usize }
impl IrMutVisitor for Bindings {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        self.nodes += 1;
        match &e.kind {
            IrExprKind::Lambda { params, .. } => self.vars.extend(params.iter().map(|(v, _)| *v)),
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.vars.insert(*var); self.vars.extend(var_tuple.iter().flatten().copied());
            }
            // Already-rendered code or an outlined closure has identities this
            // source-level substitution cannot rename safely.
            IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
            | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. } => self.opaque = true,
            _ => {}
        }
        walk_expr_mut(self, e);
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        if let IrStmtKind::Bind { var, .. } = s.kind { self.vars.insert(var); }
        walk_stmt_mut(self, s);
    }
    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        match p {
            IrPattern::Bind { var, .. } | IrPattern::As { var, .. } => { self.vars.insert(*var); }
            IrPattern::RecordPattern { fields, .. } if fields.iter().any(|f| f.pattern.is_none()) => self.opaque = true,
            _ => {}
        }
        walk_pattern_mut(self, p);
    }
}
struct Rename<'a>(&'a BTreeMap<VarId, VarId>);
impl Rename<'_> { fn var(&self, v: &mut VarId) { if let Some(n) = self.0.get(v) { *v = *n; } } }
impl IrMutVisitor for Rename<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        match &mut e.kind {
            IrExprKind::Var { id } => self.var(id),
            IrExprKind::Lambda { params, lambda_id, .. } => {
                for (v, _) in params { self.var(v); } *lambda_id = None;
            }
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.var(var); for v in var_tuple.iter_mut().flatten() { self.var(v); }
            }
            _ => {}
        }
        walk_expr_mut(self, e);
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        match &mut s.kind {
            IrStmtKind::Bind { var, .. } | IrStmtKind::Assign { var, .. }
            | IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => self.var(var),
            IrStmtKind::IndexAssign { target, .. } | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } | IrStmtKind::ListSwap { target, .. }
            | IrStmtKind::ListReverse { target, .. } | IrStmtKind::ListRotateLeft { target, .. } => self.var(target),
            IrStmtKind::ListCopySlice { dst, src, .. } => { self.var(dst); self.var(src); }
            IrStmtKind::BindDestructure { .. } | IrStmtKind::Guard { .. }
            | IrStmtKind::Expr { .. } | IrStmtKind::Comment { .. } => {}
        }
        walk_stmt_mut(self, s);
    }
    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        match p { IrPattern::Bind { var, .. } | IrPattern::As { var, .. } => self.var(var), _ => {} }
        walk_pattern_mut(self, p);
    }
}
