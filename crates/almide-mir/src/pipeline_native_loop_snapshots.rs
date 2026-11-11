// A for-in loop observes its iterable once. If its body can write that
// place, preserve an owned snapshot before entering the loop. Use the shared
// call-write analysis, including globals and closure captures; source names
// and a list of known mutators are not substitutes for that analysis.
fn snapshot_loop_iterables(ir: &mut IrProgram) {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarTable};
    use almide_ir::call_writes::{CallWrites, mut_positions, place_root};
    struct Snapshot<'a, 'b> { table: &'a mut VarTable, writes: &'a CallWrites<'b> }
    impl IrMutVisitor for Snapshot<'_, '_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            almide_ir::walk_expr_mut(self, e);
            let IrExprKind::ForIn { iterable, body, .. } = &mut e.kind else { return };
            if !crate::lower::is_heap_ty(&iterable.ty) { return; }
            let Some(root) = place_root(iterable) else { return };
            let probe = IrExpr { kind: IrExprKind::Block { stmts: body.clone(), expr: None },
                ty: almide_lang::types::Ty::Unit, span: e.span, def_id: None };
            if !self.writes.assigned_by(&probe).contains(&root.0) { return; }
            let (ty, span) = (iterable.ty.clone(), iterable.span);
            let var = self.table.alloc_fresh("__native_iteration_snapshot", ty.clone(), Mutability::Let, span);
            let source = std::mem::replace(iterable, Box::new(IrExpr {
                kind: IrExprKind::Var { id: var }, ty: ty.clone(), span, def_id: None,
            }));
            let bind = IrStmt { kind: IrStmtKind::Bind {
                var, mutability: Mutability::Let, ty, value: *source,
            }, span };
            let old = std::mem::take(e);
            *e = IrExpr { ty: old.ty.clone(), span: old.span, def_id: old.def_id,
                kind: IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(old)) } };
        }
    }
    let positions = mut_positions(ir);
    let writes = CallWrites::new(None, &ir.functions, &ir.top_lets, &positions);
    let mut visitor = Snapshot { table: &mut ir.var_table, writes: &writes };
    for f in &mut ir.functions { visitor.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { visitor.visit_expr_mut(&mut t.value); }
}

/// `for i in a..<b` reads its bounds once, before the first round (#4662).
/// The counting loop compares against the bound's own local, so a body that
/// writes a variable the bound reads would move the bound under the loop:
/// bind such a bound ahead of the loop. A `let`-bound range read only as a
/// loop head is carried as its two bounds and counted at the loop, so its
/// bounds are bound where the `let` stands unless nothing can change them.
/// Runs again after `native_range_loops` has made counted loops of its own.
pub(super) fn snapshot_range_bounds(ir: &mut IrProgram) {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarTable};
    use almide_ir::call_writes::{CallWrites, mut_positions};
    struct Snapshot<'a, 'b> { table: &'a mut VarTable, writes: &'a CallWrites<'b> }
    impl Snapshot<'_, '_> {
        fn bind(&mut self, bound: &mut IrExpr, out: &mut Vec<IrStmt>) {
            let (ty, span) = (bound.ty.clone(), bound.span);
            let var = self.table.alloc_fresh("__native_range_bound", ty.clone(), Mutability::Let, span);
            let value = std::mem::replace(bound, IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span, def_id: None });
            out.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span });
        }
        /// No later statement can change what `e` evaluates to.
        fn settled(&self, e: &IrExpr) -> bool {
            match &e.kind {
                IrExprKind::LitInt { .. } => true,
                IrExprKind::Var { id } => self.table.get(*id).mutability == Mutability::Let,
                _ => false,
            }
        }
    }
    impl IrMutVisitor for Snapshot<'_, '_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            almide_ir::walk_expr_mut(self, e);
            if let IrExprKind::Block { stmts, .. } = &mut e.kind {
                let mut out = Vec::with_capacity(stmts.len());
                for mut s in std::mem::take(stmts) {
                    if let IrStmtKind::Bind { value: IrExpr { kind: IrExprKind::Range { start, end, .. }, .. }, .. } = &mut s.kind {
                        // Start before end, as the range reads them.
                        let unsettled = !self.settled(end);
                        if unsettled && !self.settled(start) { self.bind(start, &mut out); }
                        if unsettled { self.bind(end, &mut out); }
                    }
                    out.push(s);
                }
                *stmts = out;
                return;
            }
            let IrExprKind::ForIn { iterable, body, .. } = &mut e.kind else { return };
            let IrExprKind::Range { start, end, .. } = &mut iterable.kind else { return };
            let probe = IrExpr { kind: IrExprKind::Block { stmts: body.clone(), expr: None },
                ty: almide_lang::types::Ty::Unit, span: e.span, def_id: None };
            let written = self.writes.assigned_by(&probe);
            let moves = |b: &IrExpr| almide_ir::free_vars::free_vars(b, &HashSet::new()).iter().any(|v| written.contains(&v.0));
            let (start_moves, end_moves) = (moves(start), moves(end));
            if !start_moves && !end_moves { return; }
            let mut binds = Vec::new();
            // A bound bound here is evaluated here, so start goes ahead of
            // an end bound here: evaluating that end may write what start reads.
            if start_moves || (end_moves && !matches!(start.kind, IrExprKind::LitInt { .. })) { self.bind(start, &mut binds); }
            if end_moves { self.bind(end, &mut binds); }
            let old = std::mem::take(e);
            *e = IrExpr { ty: old.ty.clone(), span: old.span, def_id: old.def_id,
                kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(old)) } };
        }
    }
    let positions = mut_positions(ir);
    let writes = CallWrites::new(None, &ir.functions, &ir.top_lets, &positions);
    let mut visitor = Snapshot { table: &mut ir.var_table, writes: &writes };
    for f in &mut ir.functions { visitor.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { visitor.visit_expr_mut(&mut t.value); }
}

/// Arguments are values evaluated left to right. If a later argument may
/// change an earlier read, bind the affected prefix before evaluating it.
/// The same call-write analysis accounts for named callees and closures.
fn snapshot_call_operands(ir: &mut IrProgram) {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarTable};
    use almide_ir::call_writes::{CallWrites, call_mut_positions, mut_positions};
    struct Snapshot<'a, 'b> {
        table: &'a mut VarTable,
        writes: &'a CallWrites<'b>,
        positions: &'b almide_ir::call_writes::MutPositions,
    }
    impl IrMutVisitor for Snapshot<'_, '_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            almide_ir::walk_expr_mut(self, e);
            // An argument snapshot can leave a block inside a literal.
            // Bind every element in order, not just the block: an earlier
            // variable read must precede the block's writes as well.
            let elements: Vec<&mut IrExpr> = match &mut e.kind {
                IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter_mut().collect(),
                IrExprKind::Record { fields, .. } => fields.iter_mut().map(|(_, value)| value).collect(),
                IrExprKind::MapLiteral { entries } => entries.iter_mut().flat_map(|(k, v)| [k, v]).collect(),
                _ => Vec::new(),
            };
            if elements.iter().any(|v| matches!(v.kind, IrExprKind::Block { .. })) {
                let mut stmts = Vec::new();
                for a in elements {
                    let (ty, span) = (a.ty.clone(), a.span);
                    let var = self.table.alloc_fresh("__native_literal_element", ty.clone(), Mutability::Let, span);
                    let value = std::mem::replace(a, IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span, def_id: None });
                    stmts.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span });
                }
                let old = std::mem::take(e);
                *e = IrExpr { ty: old.ty.clone(), span: old.span, def_id: old.def_id,
                    kind: IrExprKind::Block { stmts, expr: Some(Box::new(old)) } };
                return;
            }
            let IrExprKind::Call { target, args, .. } = &mut e.kind else { return };
            let places = call_mut_positions(target, None, self.positions).unwrap_or_default();
            let writes: Vec<_> = args.iter().map(|a| self.writes.assigned_by(a)).collect();
            let last = args.iter().enumerate().rposition(|(i, a)| {
                !places.contains(&i) && almide_ir::free_vars::free_vars(a, &HashSet::new()).iter()
                    .any(|v| writes[i + 1..].iter().any(|w| w.contains(&v.0)))
            });
            let callee_conflict = match target {
                CallTarget::Computed { callee } => almide_ir::free_vars::free_vars(callee, &HashSet::new()).iter()
                    .any(|v| writes.iter().any(|w| w.contains(&v.0))),
                _ => false,
            };
            if last.is_none() && !callee_conflict { return; }
            let mut binds = Vec::new();
            // Evaluate a computed callee before its arguments, including
            // selecting the old closure when an argument rebinds that variable.
            if let CallTarget::Computed { callee } = target {
                let (ty, span) = (callee.ty.clone(), callee.span);
                let var = self.table.alloc_fresh("__native_callee_snapshot", ty.clone(), Mutability::Let, span);
                let value = std::mem::replace(callee, Box::new(IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span, def_id: None }));
                binds.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: *value }, span });
            }
            for (i, a) in args.iter_mut().enumerate().take(last.map_or(0, |i| i + 1)) {
                if places.contains(&i) { continue; }
                let (ty, span) = (a.ty.clone(), a.span);
                let var = self.table.alloc_fresh("__native_argument_snapshot", ty.clone(), Mutability::Let, span);
                let value = std::mem::replace(a, IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span, def_id: None });
                binds.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span });
            }
            let old = std::mem::take(e);
            *e = IrExpr { ty: old.ty.clone(), span: old.span, def_id: old.def_id,
                kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(old)) } };
        }
    }
    let positions = mut_positions(ir);
    let writes = CallWrites::new(None, &ir.functions, &ir.top_lets, &positions);
    let mut visitor = Snapshot { table: &mut ir.var_table, writes: &writes, positions: &positions };
    for f in &mut ir.functions { visitor.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { visitor.visit_expr_mut(&mut t.value); }
    defaults(&mut ir.type_decls, |e| visitor.visit_expr_mut(e));
}
