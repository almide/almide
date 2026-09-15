// Included by pass_capture_clone.rs. A fold consumes its callback before
// returning; scalar results cannot carry a reference to the borrowed capture.
fn borrowed_fold_params(program: &IrProgram, shared: &HashSet<VarId>) -> HashSet<VarId> {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct Scan<'a> { shared: &'a HashSet<VarId>, vt: &'a VarTable, out: HashSet<VarId> }
    impl IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            let callback = match &e.kind {
                IrExprKind::IterChain { collector: IterCollector::Fold { lambda, .. }, .. } => Some(lambda.as_ref()),
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if module.as_str() == "list" && func.as_str() == "fold" && args.len() == 3 => args.get(2),
                IrExprKind::RuntimeCall { symbol, args }
                    if symbol.as_str() == "almide_rt_list_fold" && args.len() == 3 => args.get(2),
                _ => None,
            };
            if let Some(lambda) = callback
                && let IrExprKind::Lambda { params, body, .. } = &lambda.kind
                && matches!(body.ty, Ty::Int | Ty::Float | Ty::Bool)
                && let Some((first, _)) = params.first()
            {
                let bound = params.iter().map(|(id, _)| *id).collect();
                let captures = almide_ir::free_vars::free_vars(body, &bound);
                let mutated = written_vars(body);
                let mut nested = Nested(false);
                nested.visit_expr(body);
                let owned: HashSet<_> = captures.iter().copied().filter(|v| needs_clone_type(&self.vt.get(*v).ty)).collect();
                // Capture-free folds need no ownership workaround. Leave them
                // to stream fusion so promotion does not force an intermediate
                // collection between an existing map/filter chain and its fold.
                if !owned.is_empty() && !nested.0 && only_borrows(body, &owned) && captures.iter().all(|v| !self.shared.contains(v) && !mutated.contains(v)) {
                    self.out.insert(*first);
                }
            }
            walk_expr(self, e);
        }
    }
    struct Nested(bool);
    impl IrVisitor for Nested {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::Lambda { .. } | IrExprKind::Fan { .. }) { self.0 = true; }
            if !self.0 { walk_expr(self, e); }
        }
    }
    let mut scan = Scan { shared, vt: &program.var_table, out: HashSet::new() };
    for f in &program.functions { scan.visit_expr(&f.body); }
    for m in &program.modules { for f in &m.functions { scan.visit_expr(&f.body); } }
    scan.out
}

/// Moving an owned capture would turn the closure into `FnOnce`. Only an
/// explicit shared borrow, an explicit clone, and an indexed read of the
/// bare variable keep it borrowed; any other occurrence — a field read
/// included — moves it.
fn only_borrows(body: &IrExpr, owned: &HashSet<VarId>) -> bool {
    UseSites::of_expr(body, Site::Result, &ExplicitBorrows).iter()
        .filter(|u| owned.contains(&u.var))
        .all(|u| matches!(u.site, Site::Borrow { mutable: false } | Site::Clone | Site::Index))
}

// The legacy runtime fold takes Rc<dyn Fn + 'static>. Promote only the
// proven synchronous literals to the existing iterator collector so their
// borrow stays inside the immediate call rather than crossing that ABI.
fn lower_borrowed_folds(program: &mut IrProgram, borrowed: &HashSet<VarId>) {
    use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
    struct Lower<'a>(&'a HashSet<VarId>);
    impl IrMutVisitor for Lower<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::RuntimeCall { symbol, args } = &mut e.kind
                && symbol.as_str() == "almide_rt_list_fold" && args.len() == 3
                && let IrExprKind::Lambda { params, .. } = &args[2].kind
                && params.first().is_some_and(|(id, _)| self.0.contains(id))
            {
                let mut args = std::mem::take(args).into_iter();
                if let (Some(source), Some(init), Some(lambda)) = (args.next(), args.next(), args.next()) {
                    e.kind = IrExprKind::IterChain { source: Box::new(source), consume: true,
                        steps: vec![], collector: IterCollector::Fold { init: Box::new(init), lambda: Box::new(lambda) } };
                }
            }
        }
    }
    let mut lower = Lower(borrowed);
    for f in &mut program.functions { lower.visit_expr_mut(&mut f.body); }
    for m in &mut program.modules { for f in &mut m.functions { lower.visit_expr_mut(&mut f.body); } }
}
