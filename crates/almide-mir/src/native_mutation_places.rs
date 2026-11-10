//! Publish mutable global and field receivers at the call boundary. A native
//! field read can be an owned copy; the updated value must reach its place.
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrMutVisitor, IrProgram, IrStmt, IrStmtKind, Mutability, VarId, VarTable};
use almide_lang::types::Ty;
use std::collections::HashSet;

pub(crate) fn expand(ir: &mut IrProgram) {
    let globals = ir.top_lets.iter().filter(|t| t.mutable).map(|t| t.var).collect();
    let mut pass = Places { globals, vars: &mut ir.var_table };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { pass.visit_expr_mut(&mut t.value); }
}
struct Places<'a> { globals: HashSet<VarId>, vars: &'a mut VarTable }

fn root(place: &IrExpr) -> Option<VarId> {
    match &place.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } => root(object),
        // Indexed places need a separately bound address; do not evaluate
        // an index twice or guess when it should be bounds-checked.
        _ => None,
    }
}
impl Places<'_> {
    fn keep_reads_at_site(&mut self, e: &mut IrExpr) {
        fn read_root(e: &IrExpr) -> Option<VarId> {
            match &e.kind {
                IrExprKind::Var { id } => Some(*id),
                IrExprKind::Member { object, .. } | IrExprKind::IndexAccess { object, .. } => read_root(object),
                _ => None,
            }
        }
        let IrExprKind::Call { target, args, .. } = &mut e.kind else { return };
        let module_call = match target {
            CallTarget::Module { module, func, .. } => Some(format!("{module}.{func}")),
            _ => None,
        };
        for (k, a) in args.iter_mut().enumerate() {
            if module_call.as_ref().is_some_and(|n| crate::native_hof::mutable_argument(n, k)) { continue; }
            if !matches!(a.kind, IrExprKind::Member { .. } | IrExprKind::IndexAccess { .. })
                || !crate::lower::is_heap_ty(&a.ty)
                || !read_root(a).is_some_and(|v| self.globals.contains(&v)) { continue; }
            // Keep the projection in its operand scope. The shared wasm-era
            // ANF hoist otherwise snapshots a later argument before a sibling
            // mutation block runs (len(xs), pop(xs), len(xs)).
            let mut stmts = Vec::new();
            let result = self.bind(a.clone(), Mutability::Let, &mut stmts);
            a.kind = IrExprKind::Block { stmts, expr: Some(Box::new(result)) };
        }
    }
    fn bind(&mut self, value: IrExpr, mutable: Mutability, stmts: &mut Vec<IrStmt>) -> IrExpr {
        let var = self.vars.alloc_fresh("mutation_value", value.ty.clone(), mutable, None);
        let out = IrExpr { kind: IrExprKind::Var { id: var }, ty: value.ty.clone(), span: value.span, def_id: None };
        stmts.push(IrStmt { span: value.span,
            kind: IrStmtKind::Bind { var, mutability: mutable, ty: value.ty.clone(), value } });
        out
    }
    fn rewrite(&mut self, e: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind else { return None };
        let name = format!("{module}.{func}");
        let mut slots = (0..args.len()).filter(|k| crate::native_hof::plain_mutator(&name, *k));
        let at = slots.next()?;
        if slots.next().is_some() || !root(&args[at]).is_some_and(|v|
            self.globals.contains(&v) || matches!(args[at].kind, IrExprKind::Member { .. })) { return None; }
        let place = args[at].clone();
        let mut call = e.clone();
        let IrExprKind::Call { args: actual, .. } = &mut call.kind else { unreachable!() };
        let mut stmts = Vec::new();
        // A static global/member place has no address-evaluation effects.
        // The ordinary emitter evaluates other arguments before borrowing
        // the global cell, so a sibling call can update the current receiver.
        for (k, a) in actual.iter_mut().enumerate() {
            if k != at { *a = self.bind(a.clone(), Mutability::Let, &mut stmts); }
        }
        let receiver = self.bind(place.clone(), Mutability::Var, &mut stmts);
        actual[at] = receiver.clone();
        let result = if e.ty == Ty::Unit {
            stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: call }, span: e.span });
            IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: e.span, def_id: None }
        } else { self.bind(call, Mutability::Let, &mut stmts) };
        stmts.push(crate::lower::write_rebuilt_place(&place, receiver)?);
        Some(IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(result)) },
            ty: e.ty.clone(), span: e.span, def_id: e.def_id })
    }
}
impl IrMutVisitor for Places<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        almide_ir::walk_expr_mut(self, e);
        if let Some(new) = self.rewrite(e) { *e = new; }
        else { self.keep_reads_at_site(e); }
    }
}
