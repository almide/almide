//! Split a write through a field path two or more records deep into one-level
//! writes (#4621). `list.push(o.buf.xs, v)` and `o.buf.xs = e` reach the
//! lowering as the spread chain `o = { ...o, buf: { ...o.buf, xs: … } }`, whose
//! inner base `o.buf` is a second read of a place the outer spread also reads:
//! the render borrows it, copies the list for every append and clones every
//! sibling field, so a build loop over a nested field is quadratic. Bound
//! through one local per level,
//!
//! ```text
//! { var p1 = o.buf; list.push(p1.xs, v); o.buf = p1 }
//! ```
//!
//! every level is a one-field write on a local var — the shape whose read the
//! render already moves — and the value is the same: `p1` holds `o.buf`, and
//! nothing writes `o` between the bind and the write-back. Reads of the path
//! in the operands are rewritten onto the locals so the outer var is not read
//! again before its write-back. Only a local root qualifies: a mutable global
//! can be rebound by a call in an operand, and a var a lambda captures can be
//! written by that lambda.
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrMutVisitor, IrProgram, IrStmt, IrStmtKind, Mutability, VarId, VarTable};
use almide_lang::intern::Sym;
use std::collections::HashSet;

pub(crate) fn expand(ir: &mut IrProgram) {
    let globals: HashSet<VarId> = ir.top_lets.iter().map(|t| t.var).collect();
    let mutable_globals: HashSet<VarId> = ir.top_lets.iter().filter(|t| t.mutable).map(|t| t.var).collect();
    for f in &mut ir.functions {
        let captured = captured_vars(&f.body);
        let mut pass = Split { globals: &globals, mutable_globals: &mutable_globals, captured, vars: &mut ir.var_table };
        pass.visit_expr_mut(&mut f.body);
    }
}

/// Every var free in some lambda of the body.
fn captured_vars(body: &IrExpr) -> HashSet<VarId> {
    struct C(HashSet<VarId>);
    impl almide_ir::visit::IrVisitor for C {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Lambda { params, body, .. } = &e.kind {
                let bound = params.iter().map(|(v, _)| *v).collect();
                self.0.extend(almide_ir::free_vars::free_vars(body, &bound));
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut c = C(HashSet::new());
    almide_ir::visit::IrVisitor::visit_expr(&mut c, body);
    c.0
}

struct Split<'a> { globals: &'a HashSet<VarId>, mutable_globals: &'a HashSet<VarId>, captured: HashSet<VarId>, vars: &'a mut VarTable }

/// `o.f1.….fk` as its root and field path.
fn path_of(e: &IrExpr) -> Option<(VarId, Vec<Sym>)> {
    match &e.kind {
        IrExprKind::Var { id } => Some((*id, vec![])),
        IrExprKind::Member { object, field } => {
            let (root, mut path) = path_of(object)?;
            path.push(*field);
            Some((root, path))
        }
        _ => None,
    }
}

fn var(id: VarId, e: &IrExpr) -> IrExpr {
    IrExpr { kind: IrExprKind::Var { id }, ty: e.ty.clone(), span: e.span, def_id: None }
}

/// The record locals of one split: `locals[i]` holds `root.path[..=i]`.
struct Levels { root: VarId, path: Vec<Sym>, locals: Vec<VarId> }

impl Levels {
    /// Rewrite every read of `root.path[..=i]…` in `e` onto `locals[i]` (the
    /// deepest bound prefix), so the operand does not read the root again.
    fn retarget(&self, e: &mut IrExpr) {
        struct R<'l>(&'l Levels);
        impl IrMutVisitor for R<'_> {
            fn visit_expr_mut(&mut self, e: &mut IrExpr) {
                if let Some((root, path)) = path_of(e) {
                    let l = self.0;
                    if root == l.root {
                        let shared = path.iter().zip(&l.path).take(l.locals.len()).take_while(|(a, b)| a == b).count();
                        if shared > 0 {
                            *e = rebuild(l.locals[shared - 1], &path[shared..], e);
                            return;
                        }
                    }
                }
                almide_ir::walk_expr_mut(self, e);
            }
        }
        R(self).visit_expr_mut(e);
    }
}

/// `local.rest…` with the types of the original chain `e`.
fn rebuild(local: VarId, rest: &[Sym], e: &IrExpr) -> IrExpr {
    let mut chain = vec![];
    let mut cur = e;
    for _ in rest {
        let IrExprKind::Member { object, .. } = &cur.kind else { unreachable!() };
        chain.push(cur);
        cur = object;
    }
    let mut out = var(local, cur);
    for m in chain.into_iter().rev() {
        let IrExprKind::Member { field, .. } = &m.kind else { unreachable!() };
        out = IrExpr { kind: IrExprKind::Member { object: Box::new(out), field: *field }, ty: m.ty.clone(), span: m.span, def_id: None };
    }
    out
}

/// An operand a mutable global's write may evaluate while the global is taken
/// out of its slot: no call, closure or early exit can observe the slot. A
/// stdlib call is one: the stdlib cannot name a program's globals, and its
/// arguments here carry no callback that could.
fn slot_free(e: &IrExpr) -> bool {
    struct P(bool);
    impl almide_ir::visit::IrVisitor for P {
        fn visit_expr(&mut self, e: &IrExpr) {
            use IrExprKind as K;
            if !matches!(e.kind, K::LitInt { .. } | K::LitFloat { .. } | K::LitStr { .. } | K::LitBool { .. }
                | K::Unit | K::Var { .. } | K::Range { .. } | K::BinOp { .. } | K::UnOp { .. } | K::List { .. } | K::Record { .. }
                | K::SpreadRecord { .. } | K::Tuple { .. } | K::Member { .. } | K::TupleIndex { .. }
                | K::StringInterp { .. } | K::OptionSome { .. } | K::OptionNone | K::ResultOk { .. } | K::ResultErr { .. })
                && !matches!(&e.kind, K::Call { target: CallTarget::Module { module, .. }, .. }
                    if almide_lang::stdlib_info::is_stdlib_module(module.as_str())) {
                self.0 = false;
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut p = P(true);
    almide_ir::visit::IrVisitor::visit_expr(&mut p, e);
    p.0
}

impl Split<'_> {
    /// Split the write of `place` (`root.f1.….fk`): `body` builds the one-level
    /// write on the deepest local from the retargeted place and operands.
    /// A local root needs k >= 2; a mutable global is first taken into a local
    /// (`var t = g; …; g = t`), which needs k >= 1 and operands that cannot
    /// reach the global's slot while it is taken.
    fn split(&mut self, place: &IrExpr, operands: &[&IrExpr],
             body: impl FnOnce(&IrExpr, Vec<IrExpr>) -> IrStmt) -> Option<Vec<IrStmt>> {
        let (root, path) = path_of(place)?;
        let global = self.mutable_globals.contains(&root);
        if global {
            if path.is_empty() || !operands.iter().all(|o| slot_free(o)) { return None; }
        } else if path.len() < 2 || self.globals.contains(&root) || self.captured.contains(&root)
            || crate::lower::mutable_global_info(root).is_some() {
            return None;
        }
        let mut stmts = vec![];
        let (mut place, mut operands): (IrExpr, Vec<IrExpr>) = (place.clone(), operands.iter().map(|o| (*o).clone()).collect());
        let mut taken = None;
        let mut root = root;
        if global {
            let root_expr = prefixes_with_root(&place)[0].clone();
            let t = self.vars.alloc_fresh("global_level", root_expr.ty.clone(), Mutability::Var, None);
            stmts.push(IrStmt { span: None, kind: IrStmtKind::Bind { var: t, mutability: Mutability::Var, ty: root_expr.ty.clone(), value: root_expr.clone() } });
            let tv = var(t, &root_expr);
            place = almide_ir::substitute::substitute_var_in_expr(&place, root, &tv);
            for o in operands.iter_mut() { *o = almide_ir::substitute::substitute_var_in_expr(o, root, &tv); }
            taken = Some((root, tv));
            root = t;
        }
        let levels = self.bind_levels(&place, root, path, &mut stmts);
        let mut target = place.clone();
        levels.retarget(&mut target);
        for o in operands.iter_mut() { levels.retarget(o); }
        stmts.push(body(&target, operands));
        Self::write_back(&levels, &prefixes_of(&place), &mut stmts);
        if let Some((g, tv)) = taken {
            stmts.push(IrStmt { span: None, kind: IrStmtKind::Assign { var: g, value: tv } });
        }
        Some(stmts)
    }

    /// Bind `p1 = root.f1, …, p(k-1) = p(k-2).f(k-1)` for the place `e`
    /// (`root.f1.….fk`); the statements and the levels.
    fn bind_levels(&mut self, e: &IrExpr, root: VarId, path: Vec<Sym>, stmts: &mut Vec<IrStmt>) -> Levels {
        let mut levels = Levels { root, path, locals: vec![] };
        for p in prefixes_of(e) {
            let mut value = p.clone();
            levels.retarget(&mut value);
            let v = self.vars.alloc_fresh("field_level", p.ty.clone(), Mutability::Var, p.span);
            stmts.push(IrStmt { span: p.span, kind: IrStmtKind::Bind { var: v, mutability: Mutability::Var, ty: p.ty.clone(), value } });
            levels.locals.push(v);
        }
        levels
    }

    /// The write-backs `p(k-2).f(k-1) = p(k-1)`, …, `root.f1 = p1`.
    fn write_back(levels: &Levels, places: &[&IrExpr], stmts: &mut Vec<IrStmt>) {
        for i in (0..levels.locals.len()).rev() {
            let target = if i == 0 { levels.root } else { levels.locals[i - 1] };
            stmts.push(IrStmt { span: None, kind: IrStmtKind::FieldAssign {
                target, field: levels.path[i], value: var(levels.locals[i], places[i]) } });
        }
    }

    /// A mutator call on a field place, in any position: the split, then the
    /// call's result (bound before the write-backs) as the block's value.
    fn split_call(&mut self, expr: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &expr.kind else { return None };
        let name = format!("{module}.{func}");
        let mut slots = (0..args.len()).filter(|k| crate::native_hof::mutable_argument(&name, *k));
        let at = slots.next()?;
        if slots.next().is_some() || !matches!(args[at].kind, IrExprKind::Member { .. }) { return None; }
        let others: Vec<&IrExpr> = args.iter().enumerate().filter(|(k, _)| *k != at).map(|(_, a)| a).collect();
        let unit = expr.ty == almide_lang::types::Ty::Unit;
        let result = (!unit).then(|| self.vars.alloc_fresh("mutator_result", expr.ty.clone(), Mutability::Let, expr.span));
        let stmts = self.split(&args[at], &others, |target, mut ops| {
            let mut call = expr.clone();
            let IrExprKind::Call { args: actual, .. } = &mut call.kind else { unreachable!() };
            for (k, a) in actual.iter_mut().enumerate() {
                *a = if k == at { target.clone() } else { ops.remove(0) };
            }
            let kind = match result {
                Some(r) => IrStmtKind::Bind { var: r, mutability: Mutability::Let, ty: expr.ty.clone(), value: call },
                None => IrStmtKind::Expr { expr: call },
            };
            IrStmt { span: expr.span, kind }
        })?;
        let tail = result.map(|r| Box::new(var(r, expr)));
        Some(IrExpr { kind: IrExprKind::Block { stmts, expr: tail }, ty: expr.ty.clone(), span: expr.span, def_id: expr.def_id })
    }

    /// `root.f1 = { ...root.f1, f2: … { ...root.f1.….f(k-1), fk: e } }`: the
    /// nested field write the source `root.f1.….fk = e` lowers to.
    fn split_field_assign(&mut self, target: VarId, field: Sym, value: &IrExpr) -> Option<Vec<IrStmt>> {
        let mut path = vec![field];
        let mut inner = value;
        let mut place = IrExpr { kind: IrExprKind::Member {
            object: Box::new(IrExpr { kind: IrExprKind::Var { id: target }, ty: almide_lang::types::Ty::Unit, span: None, def_id: None }),
            field }, ty: value.ty.clone(), span: value.span, def_id: None };
        while let IrExprKind::SpreadRecord { base, fields } = &inner.kind {
            let [(f, v)] = fields.as_slice() else { break };
            if path_of(base) != Some((target, path.clone())) { break; }
            path.push(*f);
            place = IrExpr { kind: IrExprKind::Member { object: base.clone(), field: *f }, ty: v.ty.clone(), span: v.span, def_id: None };
            inner = v;
        }
        if path.len() == 1 {
            // The root var's own type: the one-level place `target.field`.
            let IrExprKind::Member { object, .. } = &mut place.kind else { unreachable!() };
            object.ty = self.vars.get(target).ty.clone();
        }
        self.split(&place, &[inner], |target, mut ops| {
            let (IrExprKind::Member { object, field }, Some(e)) = (&target.kind, ops.pop()) else { unreachable!() };
            let IrExprKind::Var { id } = object.kind else { unreachable!() };
            IrStmt { span: None, kind: IrStmtKind::FieldAssign { target: id, field: *field, value: e } }
        })
    }
}

/// The root var, `root.f1`, …, the place's parent — in that order.
fn prefixes_with_root(e: &IrExpr) -> Vec<&IrExpr> {
    let mut out = vec![];
    let mut cur = e;
    while let IrExprKind::Member { object, .. } = &cur.kind { out.push(&**object); cur = object; }
    out.reverse();
    out
}

/// `root.f1`, `root.f1.f2`, …, the place's parent — in that order.
fn prefixes_of(e: &IrExpr) -> Vec<&IrExpr> {
    let mut out = vec![];
    let mut cur = e;
    while let IrExprKind::Member { object, .. } = &cur.kind { out.push(&**object); cur = object; }
    out.pop();
    out.reverse();
    out
}

impl Split<'_> {
    /// The frontend's own split of a source write `root.f1.….fk = e`
    /// (`lower_place_write`): `var t1 = root.f1; …; t(k).fk = e; …; root.f1 = t1`.
    /// Its `e` still reads the path from the root, so the root's field is read
    /// again after `t1` took it and the bind must copy it. The reads move onto
    /// the bound levels, as `split` does for its own.
    fn retarget_place_write(&self, stmts: &mut [IrStmt]) {
        let mut levels: Option<Levels> = None;
        let mut write = None;
        for (k, s) in stmts.iter().enumerate() {
            match (&s.kind, &mut levels) {
                (IrStmtKind::Bind { var, mutability: Mutability::Var, value, .. }, l) => {
                    let IrExprKind::Member { object, field } = &value.kind else { break };
                    let IrExprKind::Var { id } = object.kind else { break };
                    match l {
                        None => *l = Some(Levels { root: id, path: vec![*field], locals: vec![*var] }),
                        Some(l) if l.locals.last() == Some(&id) => { l.path.push(*field); l.locals.push(*var); }
                        _ => break,
                    }
                }
                (_, Some(_)) => { write = Some(k); break; }
                _ => break,
            }
        }
        let (Some(levels), Some(w)) = (levels, write) else { return };
        // The write-backs that close the shape: `t(i-1).fi = ti`, …, `root.f1 = t1`.
        let backs = &stmts[w + 1..];
        let closes = backs.len() == levels.locals.len() && backs.iter().rev().enumerate().all(|(i, s)| {
            let holder = if i == 0 { levels.root } else { levels.locals[i - 1] };
            matches!(&s.kind, IrStmtKind::FieldAssign { target, field, value }
                if *target == holder && *field == levels.path[i]
                && matches!(value.kind, IrExprKind::Var { id } if id == levels.locals[i]))
        });
        let mut operands = vec![];
        let _ = stmts[w].clone().map_exprs(&mut |e| { operands.push(e.clone()); e });
        let root_ok = if self.mutable_globals.contains(&levels.root) {
            operands.iter().all(slot_free)
        } else {
            !self.globals.contains(&levels.root) && !self.captured.contains(&levels.root)
                && crate::lower::mutable_global_info(levels.root).is_none()
        };
        if !closes || !root_ok { return; }
        struct R<'l>(&'l Levels);
        impl IrMutVisitor for R<'_> {
            fn visit_expr_mut(&mut self, e: &mut IrExpr) { self.0.retarget(e); }
        }
        almide_ir::walk_stmt_mut(&mut R(&levels), &mut stmts[w]);
    }
}

impl Split<'_> {
    /// The mutable global a statement writes through a field place, when it
    /// is a mutator call or a field write whose operands cannot reach a slot.
    fn global_write(&self, s: &IrStmt) -> Option<VarId> {
        let (root, operands): (VarId, Vec<&IrExpr>) = match &s.kind {
            IrStmtKind::Expr { expr } => {
                let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &expr.kind else { return None };
                let name = format!("{module}.{func}");
                let mut slots = (0..args.len()).filter(|k| crate::native_hof::mutable_argument(&name, *k));
                let at = slots.next()?;
                if slots.next().is_some() || !matches!(args[at].kind, IrExprKind::Member { .. }) { return None; }
                (path_of(&args[at])?.0, args.iter().enumerate().filter(|(k, _)| *k != at).map(|(_, a)| a).collect())
            }
            IrStmtKind::FieldAssign { target, value, .. } => (*target, vec![value]),
            _ => return None,
        };
        (self.mutable_globals.contains(&root) && operands.into_iter().all(slot_free)).then_some(root)
    }

    /// A run of such writes takes each global it touches out of its slot
    /// once (`var t = g` before, `g = t` after) and writes the locals: one
    /// take and one store per global instead of per write. Nothing in the run
    /// can observe a slot while its value is out.
    fn take_globals(&mut self, run: &[IrStmt]) -> Vec<IrStmt> {
        let mut roots: Vec<VarId> = vec![];
        for s in run {
            let g = self.global_write(s).expect("a run of global writes");
            if !roots.contains(&g) { roots.push(g); }
        }
        let mut binds = vec![];
        let mut backs = vec![];
        let mut body: Vec<IrStmt> = run.to_vec();
        for g in roots {
            let ty = self.vars.get(g).ty.clone();
            let t = self.vars.alloc_fresh("global_level", ty.clone(), Mutability::Var, None);
            let read = IrExpr { kind: IrExprKind::Var { id: g }, ty: ty.clone(), span: None, def_id: None };
            let tv = IrExpr { kind: IrExprKind::Var { id: t }, ty: ty.clone(), span: None, def_id: None };
            binds.push(IrStmt { span: None, kind: IrStmtKind::Bind { var: t, mutability: Mutability::Var, ty, value: read } });
            backs.push(IrStmt { span: None, kind: IrStmtKind::Assign { var: g, value: tv.clone() } });
            body = body.iter().map(|s| {
                let mut s = almide_ir::substitute::substitute_var_in_stmt(s, g, &tv);
                // A field write's target is a var, not an expression.
                if let IrStmtKind::FieldAssign { target, .. } = &mut s.kind {
                    if *target == g { *target = t; }
                }
                s
            }).collect();
        }
        binds.into_iter().chain(body).chain(backs).collect()
    }

    /// Group the runs of two or more global field writes in a statement list.
    fn group_global_writes(&mut self, stmts: &mut Vec<IrStmt>) {
        let mut out = Vec::with_capacity(stmts.len());
        let mut k = 0;
        while k < stmts.len() {
            let n = stmts[k..].iter().take_while(|s| self.global_write(s).is_some()).count();
            if n >= 2 {
                let run = self.take_globals(&stmts[k..k + n]);
                out.push(IrStmt { span: stmts[k].span, kind: IrStmtKind::Expr { expr: IrExpr {
                    kind: IrExprKind::Block { stmts: run, expr: None },
                    ty: almide_lang::types::Ty::Unit, span: stmts[k].span, def_id: None } } });
                k += n;
            } else {
                out.push(stmts[k].clone());
                k += 1;
            }
        }
        *stmts = out;
    }

    /// A loop whose whole body is global field writes, over a range or
    /// condition that cannot reach a slot, takes the globals once around it.
    fn hoist_global_loop(&mut self, e: &IrExpr) -> Option<IrExpr> {
        let (head, body) = match &e.kind {
            IrExprKind::ForIn { iterable, body, .. } => (&**iterable, body),
            IrExprKind::While { cond, body } => (&**cond, body),
            _ => return None,
        };
        if body.is_empty() || !slot_free(head) || !body.iter().all(|s| self.global_write(s).is_some()) { return None; }
        let taken = self.take_globals(body);
        let globals = taken.iter().filter(|s| matches!(s.kind, IrStmtKind::Bind { .. })).count();
        let (binds, rest) = taken.split_at(globals);
        let (inner, backs) = rest.split_at(rest.len() - globals);
        let mut looped = e.clone();
        match &mut looped.kind {
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => *body = inner.to_vec(),
            _ => unreachable!(),
        }
        let stmts = binds.iter().cloned()
            .chain(std::iter::once(IrStmt { span: e.span, kind: IrStmtKind::Expr { expr: looped } }))
            .chain(backs.iter().cloned()).collect();
        Some(IrExpr { kind: IrExprKind::Block { stmts, expr: None }, ty: e.ty.clone(), span: e.span, def_id: e.def_id })
    }
}

impl IrMutVisitor for Split<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if let Some(hoisted) = self.hoist_global_loop(e) { *e = hoisted; }
        if let IrExprKind::Block { stmts, .. } = &mut e.kind { self.group_global_writes(stmts); }
        almide_ir::walk_expr_mut(self, e);
        if let IrExprKind::Block { stmts, .. } = &mut e.kind { self.retarget_place_write(stmts); }
        if let Some(block) = self.split_call(e) { *e = block; }
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        almide_ir::walk_stmt_mut(self, s);
        let IrStmtKind::FieldAssign { target, field, value } = &s.kind else { return };
        if let Some(stmts) = self.split_field_assign(*target, *field, value) {
            s.kind = IrStmtKind::Expr { expr: IrExpr {
                kind: IrExprKind::Block { stmts, expr: None },
                ty: almide_lang::types::Ty::Unit, span: s.span, def_id: None } };
        }
    }
}
