// Where an in-out param may occur in a body, and which vars a call may lend:
// the use analyses of `native_inout`, split out for length (a pure text move;
// `include!`d, so it shares that module's imports and privacy).

/// Does `p` occur in `body` only where the in-out convention can carry it:
/// read by a call argument, an operator, an access or an interpolation;
/// rebound by an assignment or an in-place mutator; lent to an in-out
/// position of a candidate; read by a closure that does not outlive the
/// call it is passed to ([`scoped_read`]). A bind, a return, any other
/// capture, a container element, a match subject or an iteration keeps the
/// move-mode convention.
fn body_uses_ok(body: &IrExpr, p: VarId, cands: &Cands, writes: &HashMap<String, Vec<usize>>) -> bool {
    uses_ok(body, p, cands, writes, false)
}

/// [`body_uses_ok`]; with `read_only`, `p` is only read: no rebind, no field
/// write, no lend to a written position.
fn uses_ok(body: &IrExpr, p: VarId, cands: &Cands, writes: &HashMap<String, Vec<usize>>, read_only: bool) -> bool {
    struct C<'a> {
        p: VarId,
        cands: &'a Cands,
        writes: &'a HashMap<String, Vec<usize>>,
        read_only: bool,
        ok: bool,
    }
    impl C<'_> {
        /// The use that keeps `p` out of the convention (traced once).
        fn no(&mut self, why: &str) {
            if self.ok && !self.read_only {
                crate::trace::trace("ALMIDE_DBG_INOUT", || format!("[inout] {:?} stays move-mode: {why}", self.p));
            }
            self.ok = false;
        }
        fn read(&mut self, e: &IrExpr) {
            if !is_var(e, self.p) {
                self.visit_expr(e);
            }
        }
        fn call(&mut self, target: &CallTarget, args: &[IrExpr], ty: &Ty) {
            let captured = args.iter().any(|a| matches!(&a.kind, IrExprKind::Lambda { body, .. } if mentions(body, self.p)));
            if captured {
                if !scoped_read(target, args, ty, self.p, self.writes) {
                    self.no("closure outlives or writes");
                }
                return;
            }
            for (i, a) in args.iter().enumerate() {
                if !is_var(a, self.p) {
                    self.visit_expr(a);
                    continue;
                }
                let lent = match target {
                    CallTarget::Named { name } => match self.cands.get(name.as_str()) {
                        Some(ks) if ks.contains(&i) => {
                            !args.iter().enumerate().any(|(j, b)| j != i && (mentions(b, self.p) || !simple_operand(b)))
                        }
                        // A read position of a candidate: a borrow, unless the
                        // same call also lends the place out.
                        Some(ks) => !ks.iter().any(|&k| args.get(k).is_some_and(|b| mentions(b, self.p))),
                        None => !self.writes.get(name.as_str()).is_some_and(|ks| ks.contains(&i)),
                    },
                    CallTarget::Module { module, func, .. } => {
                        !almide_ir::mut_args::stdlib_mut_positions(module.as_str(), func.as_str())
                            .is_some_and(|ks| ks.contains(&i))
                    }
                    _ => false,
                };
                if !lent {
                    let callee = match target {
                        CallTarget::Named { name } => name.to_string(),
                        CallTarget::Module { module, func, .. } => format!("{module}.{func}"),
                        _ => "a computed callee".into(),
                    };
                    self.no(&format!("lend position {i} of {callee}"));
                }
            }
        }
    }
    impl IrVisitor for C<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if !self.ok {
                return;
            }
            match &e.kind {
                IrExprKind::Var { id } if *id == self.p => self.no("bare value"),
                IrExprKind::Block { stmts, expr } => self.block(stmts, expr.as_deref()),
                IrExprKind::Lambda { body, .. } => {
                    if mentions(body, self.p) {
                        self.no("closure");
                    }
                }
                IrExprKind::Call { .. } if self.read_only && rebinds(e, self.p) => self.no("read-only rebind"),
                IrExprKind::Call { target: CallTarget::Module { .. }, args, .. } if rebinds(e, self.p) => {
                    self.mutator_operands(&args[1..])
                }
                IrExprKind::Call { target, args, .. } => self.call(target, args, &e.ty),
                IrExprKind::BinOp { left, right, .. } => {
                    self.read(left);
                    self.read(right);
                }
                IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => self.read(object),
                IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
                    self.read(object);
                    self.read(index);
                }
                IrExprKind::StringInterp { parts } => self.interp(parts),
                IrExprKind::TailCall { .. } => {
                    if mentions(e, self.p) {
                        self.no("tail call");
                    }
                }
                _ => walk_expr(self, e),
            }
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if !self.ok {
                return;
            }
            match &s.kind {
                IrStmtKind::Assign { var, .. } | IrStmtKind::FieldAssign { target: var, .. } if self.read_only && *var == self.p => {
                    self.no("read-only write")
                }
                // A snapshot of the place (`var trial = sol`): the bind owns a
                // copy of what the place holds now, as any bind of a var does.
                IrStmtKind::Bind { value, .. } if is_var(value, self.p) => {}
                IrStmtKind::Assign { value, .. } => self.read_assign(value),
                IrStmtKind::FieldAssign { target, value, .. } if *target == self.p && field_write_ok(value) => {
                    self.visit_expr(value)
                }
                IrStmtKind::FieldAssign { target, .. }
                | IrStmtKind::IndexAssign { target, .. }
                | IrStmtKind::MapInsert { target, .. }
                    if *target == self.p =>
                {
                    self.no("index or map write")
                }
                _ => walk_stmt(self, s),
            }
        }
    }
    impl C<'_> {
        /// A block's statements (a scoped-closure `let` is the call's to judge)
        /// and its tail.
        fn block(&mut self, stmts: &[IrStmt], expr: Option<&IrExpr>) {
            for (i, st) in stmts.iter().enumerate() {
                if !scoped_let_closure(stmts, expr, i, self.p, self.writes) {
                    self.visit_stmt(st);
                }
            }
            if let Some(t) = expr {
                self.visit_expr(t);
            }
        }
        /// The operands of an in-place mutator rebinding `p`: simple, and none
        /// names `p`.
        fn mutator_operands(&mut self, operands: &[IrExpr]) {
            for a in operands {
                if mentions(a, self.p) || !simple_operand(a) {
                    self.no("mutator operand");
                }
                self.visit_expr(a);
            }
        }
        fn interp(&mut self, parts: &[almide_ir::IrStringPart]) {
            for part in parts {
                if let almide_ir::IrStringPart::Expr { expr } = part {
                    self.read(expr);
                }
            }
        }
        /// The value of an assignment: a bare `p` there would alias the slot.
        fn read_assign(&mut self, value: &IrExpr) {
            if is_var(value, self.p) {
                self.no("assign alias");
            } else {
                self.visit_expr(value);
            }
        }
    }
    let mut c = C { p, cands, writes, read_only, ok: true };
    c.visit_expr(body);
    c.ok
}

/// A call that lends a place evaluates its other operands first, in order,
/// then lends: an operand with control flow (`!`, `??`, a branch) or one that
/// reads the lent place is bound to a `let` before the call, as the call
/// would evaluate it anyway. The call is then a lend beside plain values.
fn bind_operands_before_lends(ir: &mut IrProgram, cands: &Cands) {
    struct B<'a> {
        cands: &'a Cands,
        vars: &'a mut almide_ir::VarTable,
    }
    impl IrMutVisitor for B<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind else { return };
            let Some(ks) = self.cands.get(name.as_str()) else { return };
            let roots: Vec<VarId> = ks.iter().filter_map(|&k| args.get(k).and_then(place_root)).collect();
            let plain = |a: &IrExpr| {
                matches!(
                    a.kind,
                    IrExprKind::Var { .. }
                        | IrExprKind::LitInt { .. }
                        | IrExprKind::LitFloat { .. }
                        | IrExprKind::LitBool { .. }
                        | IrExprKind::LitStr { .. }
                        | IrExprKind::Unit
                        | IrExprKind::OptionNone
                        | IrExprKind::EmptyMap
                        | IrExprKind::FnRef { .. }
                        | IrExprKind::Lambda { .. }
                )
            };
            let needs = args.iter().enumerate().any(|(j, a)| {
                !ks.contains(&j) && !plain(a) && (!simple_operand(a) || roots.iter().any(|&r| mentions(a, r)))
            });
            if roots.is_empty() || !needs {
                return;
            }
            let ks = ks.clone();
            let IrExprKind::Call { args, .. } = &mut e.kind else { return };
            let mut binds = Vec::new();
            for (j, a) in args.iter_mut().enumerate() {
                if ks.contains(&j) || plain(a) {
                    continue;
                }
                let (ty, span) = (a.ty.clone(), a.span);
                let id = self.vars.alloc(almide_base::intern::sym("__arg_before_lend"), ty.clone(), Mutability::Let, span);
                let value = std::mem::replace(a, IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None });
                binds.push(IrStmt { kind: IrStmtKind::Bind { var: id, mutability: Mutability::Let, ty, value }, span });
            }
            let (ty, span) = (e.ty.clone(), e.span);
            let node = std::mem::take(e);
            *e = IrExpr { kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(node)) }, ty, span, def_id: None };
        }
    }
    let mut b = B { cands, vars: &mut ir.var_table };
    for f in ir.functions.iter_mut() {
        b.visit_expr_mut(&mut f.body);
    }
}

/// `stmts[i]` binds a closure that only reads `p` (`let member = (n) =>
/// lookup(env, n)`), and the closure lives no longer than a call: every use of
/// the name is a direct argument of a call that cannot keep it, and nothing
/// writes `p` from the bind until the last use is evaluated. A `for` whose
/// iterable holds the last use evaluates it before its body runs.
fn scoped_let_closure(stmts: &[IrStmt], tail: Option<&IrExpr>, i: usize, p: VarId, writes: &HashMap<String, Vec<usize>>) -> bool {
    let IrStmtKind::Bind { var: v, value, .. } = &stmts[i].kind else { return false };
    let IrExprKind::Lambda { body, .. } = &value.kind else { return false };
    let none = Cands::new();
    if !mentions(body, p) {
        return false;
    }
    let why = |w: &str| crate::trace::trace("ALMIDE_DBG_INOUT", || format!("[inout] let closure {v:?} over {p:?}: {w}"));
    if !uses_ok(body, p, &none, writes, true) {
        why("its body writes");
        return false;
    }
    let as_expr = |s: &IrStmt| match &s.kind {
        IrStmtKind::Expr { expr } => Some(expr.clone()),
        _ => None,
    };
    // The rest of the block, as expressions (a statement wrapped in a block).
    let wrap = |s: &IrStmt| IrExpr {
        kind: IrExprKind::Block { stmts: vec![s.clone()], expr: None },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    };
    let rest: Vec<IrExpr> = stmts[i + 1..].iter().map(|s| as_expr(s).unwrap_or_else(|| wrap(s))).chain(tail.cloned()).collect();
    let Some(last) = rest.iter().rposition(|e| mentions(e, *v)) else { return false };
    if !rest[..=last].iter().all(|e| only_called_with(e, *v, writes)) {
        why("a use that may keep it");
        return false;
    }
    let read_only = |e: &IrExpr| !mentions(e, p) || uses_ok(e, p, &none, writes, true);
    // Another closure bound here that only reads `p` writes nothing either
    // (its own uses are checked where it is bound).
    let reading_bind = |k: usize| {
        stmts.get(i + 1 + k).is_some_and(|s| matches!(&s.kind,
            IrStmtKind::Bind { value, .. } if matches!(&value.kind, IrExprKind::Lambda { body, .. } if uses_ok(body, p, &none, writes, true))))
    };
    if !rest[..last].iter().enumerate().all(|(k, e)| reading_bind(k) || read_only(e)) {
        why("a write before its last use");
        return false;
    }
    let ok = match &rest[last].kind {
        IrExprKind::ForIn { iterable, body, .. } => {
            read_only(iterable) && !body.iter().any(|s| mentions(&wrap(s), *v))
        }
        _ => read_only(&rest[last]),
    };
    if !ok {
        why("a write beside its last use");
    }
    ok
}

/// Every use of `v` in `e` is a direct argument of a call that cannot keep it
/// (its answer holds no function, no `mut` param of it can hold one).
fn only_called_with(e: &IrExpr, v: VarId, writes: &HashMap<String, Vec<usize>>) -> bool {
    struct U<'a>(VarId, &'a HashMap<String, Vec<usize>>, bool);
    impl IrVisitor for U<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Var { id } if *id == self.0 => self.2 = false,
                IrExprKind::Call { target, args, .. } if args.iter().any(|a| is_var(a, self.0)) => {
                    if !keeps_nothing(target, args, &e.ty, self.1) {
                        self.2 = false;
                    }
                    let v = self.0;
                    for a in args.iter().filter(|a| !is_var(a, v)) {
                        self.visit_expr(a);
                    }
                    if let CallTarget::Computed { callee } = target {
                        self.visit_expr(callee);
                    }
                }
                _ => walk_expr(self, e),
            }
        }
    }
    let mut u = U(v, writes, true);
    u.visit_expr(e);
    u.2
}

fn has_fn(ty: &Ty) -> bool {
    matches!(ty, Ty::Fn { .. }) || ty.children().into_iter().any(has_fn)
}

/// A call that cannot keep a closure past itself: its answer holds no
/// function value, and a user callee has no `mut` param that can hold one.
fn keeps_nothing(target: &CallTarget, args: &[IrExpr], ty: &Ty, writes: &HashMap<String, Vec<usize>>) -> bool {
    let callee = match target {
        CallTarget::Module { .. } => true,
        CallTarget::Named { name } => {
            !writes.get(name.as_str()).is_some_and(|ks| ks.iter().any(|&k| args.get(k).is_none_or(|a| has_fn(&a.ty))))
        }
        _ => false,
    };
    callee && !has_fn(ty)
}

/// A call that passes a closure capturing `p` lends it the place for the
/// call alone: the callee's answer holds no function value and no `mut`
/// param of the callee can hold one (the closure cannot leave the call), and
/// every argument, the closure bodies included, only reads `p` — nothing
/// writes the place while a closure that reads it is alive.
fn scoped_read(target: &CallTarget, args: &[IrExpr], ty: &Ty, p: VarId, writes: &HashMap<String, Vec<usize>>) -> bool {
    let none = Cands::new();
    keeps_nothing(target, args, ty, writes)
        && args.iter().all(|a| match &a.kind {
            IrExprKind::Lambda { body, .. } => uses_ok(body, p, &none, writes, true),
            _ => !mentions(a, p) || uses_ok(a, p, &none, writes, true),
        })
}

/// The vars a call in `f` may lend: its `var` locals no closure captures,
/// plus its own `mut` params (`selected` or move-mode: the native
/// preparation owns a written param in a local).
fn lendable_vars(f: &IrFunction, writes: &HashMap<String, Vec<usize>>) -> HashSet<VarId> {
    struct Vars<'a>(HashSet<VarId>, HashSet<VarId>, &'a HashMap<String, Vec<usize>>, HashSet<VarId>);
    impl IrVisitor for Vars<'_> {
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, mutability: Mutability::Var, ty, .. } = &s.kind {
                if lendable(ty) {
                    self.0.insert(*var);
                }
            }
            walk_stmt(self, s);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                // A closure passed straight to a call that it cannot outlive
                // and that only reads the var ([`scoped_read`]) leaves the
                // var lendable around the call.
                IrExprKind::Call { target, args, .. } if args.iter().any(|a| matches!(a.kind, IrExprKind::Lambda { .. })) => {
                    for a in args {
                        let IrExprKind::Lambda { params, body, .. } = &a.kind else {
                            self.visit_expr(a);
                            continue;
                        };
                        let bound = params.iter().map(|(v, _)| *v).collect();
                        for v in almide_ir::free_vars::free_vars(body, &bound) {
                            self.3.insert(v);
                            if !scoped_read(target, args, &e.ty, v, self.2) {
                                self.1.insert(v);
                            }
                        }
                        self.visit_expr(body);
                    }
                    if let CallTarget::Computed { callee } = target {
                        self.visit_expr(callee);
                    }
                }
                IrExprKind::Lambda { params, body, .. } => {
                    let bound = params.iter().map(|(v, _)| *v).collect();
                    let free = almide_ir::free_vars::free_vars(body, &bound);
                    self.3.extend(free.iter().copied());
                    self.1.extend(free);
                    walk_expr(self, e);
                }
                _ => walk_expr(self, e),
            }
        }
    }
    let mut v = Vars(HashSet::new(), HashSet::new(), writes, HashSet::new());
    v.visit_expr(&f.body);
    let mut out: HashSet<VarId> = v.0.difference(&v.1).copied().collect();
    // A move-mode fn keeps a `mut` param any closure captures in a write-back
    // cell, which has no place to lend (a candidate's own params are added
    // where its body passed `body_uses_ok`).
    out.extend(f.params.iter().filter(|p| p.is_mut && lendable(&p.ty) && !v.3.contains(&p.var)).map(|p| p.var));
    out
}

/// Does `e` mention `r` other than as a place lent to a candidate's in-out
/// position or a scalar read through it? Two lends side by side borrow the
/// place one after the other, and a scalar read is a copy; a heap read may
/// stay borrowed across the other's lend.
fn reads_besides_lends(e: &IrExpr, r: VarId, cands: &Cands) -> bool {
    struct R<'a>(VarId, &'a Cands, bool);
    impl IrVisitor for R<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                // A scalar read through the place is a copy: it holds nothing.
                IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. }
                    if place_root(e) == Some(self.0) && !crate::lower::is_heap_ty(&e.ty) =>
                {
                    if let IrExprKind::IndexAccess { index, .. } = &e.kind {
                        self.visit_expr(index);
                    }
                }
                IrExprKind::Var { id } if *id == self.0 => self.2 = true,
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if self.1.contains_key(name.as_str()) => {
                    let ks = &self.1[name.as_str()];
                    for (k, a) in args.iter().enumerate() {
                        if !(ks.contains(&k) && is_var(a, self.0)) {
                            self.visit_expr(a);
                        }
                    }
                }
                _ => walk_expr(self, e),
            }
        }
    }
    let mut v = R(r, cands, false);
    v.visit_expr(e);
    v.2
}

/// The candidate calls inside `e` (outside closures) and the root of each
/// place they lend.
fn nested_lends(e: &IrExpr, cands: &Cands) -> Vec<(String, VarId)> {
    struct N<'a>(&'a Cands, Vec<(String, VarId)>);
    impl IrVisitor for N<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Lambda { .. } => return,
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                    for &k in self.0.get(name.as_str()).into_iter().flatten() {
                        if let Some(r) = args.get(k).and_then(place_root) {
                            self.1.push((name.to_string(), r));
                        }
                    }
                }
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    let mut n = N(cands, Vec::new());
    n.visit_expr(e);
    n.1
}
