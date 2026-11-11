//! #4476 — what evaluating an operand may write, the CALLS it makes included.
//!
//! An operand sequence is evaluated left to right, by value (ALS-E26, C-258),
//! and a `mut` argument names a PLACE that the call writes once every operand
//! has been evaluated (C-132): `list.push(g.xs, reset())` pushes into the
//! `g.xs` that `reset()` left behind, which is what native runs. Both rules
//! need to know which variables a later operand can write. Reading the
//! operand's own statements (`almide_ir::collect_assigned_vars`) misses every
//! write a CALLED fn makes: `reset()` that reassigns the module-level `g`.
//! On the wasm leg that write released the block an earlier operand had
//! already read, so the call then read or wrote freed memory.
//!
//! [`CallWrites`] answers the question per variable table (the entry program,
//! or one module): a fn's writes are the module-level variables its body
//! assigns or mutates through a `mut` argument, plus those of every fn it
//! calls or names (`FnRef`), to a fixpoint. A call through a closure value can
//! run any lambda, or any fn named as a value, so it writes the union of all
//! of them.
//!
//! [`hoist_operands_before_mut_places`] applies the place rule: when another
//! operand of a call may write the root of a `mut` place argument, every
//! other operand is bound first, in source order, and the place stays where
//! it is. The write then lands in the place's current value, on every leg.

use crate::visit::{walk_expr, walk_stmt, IrVisitor};
use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use crate::*;
use almide_base::intern::{sym, Sym};
use std::collections::{HashMap, HashSet};

/// The `mut` parameter positions of every user fn, by `(module, fn)`; the
/// entry program's fns are under the empty module name.
pub type MutPositions = HashMap<(Sym, Sym), Vec<usize>>;

/// Every user fn's `mut` positions in `program`.
pub fn mut_positions(program: &IrProgram) -> MutPositions {
    let mut out = MutPositions::new();
    let entry = sym("");
    for f in program.functions.iter().filter(|f| !f.mutated_params.is_empty()) {
        out.insert((entry, f.name), f.mutated_params.clone());
    }
    for m in &program.modules {
        for f in m.functions.iter().filter(|f| !f.mutated_params.is_empty()) {
            out.insert((m.name, f.name), f.mutated_params.clone());
        }
    }
    out
}

/// The positions a call to `target` writes through, from a stdlib
/// declaration or a user fn's `mutated_params`. `module` names the table the
/// call sits in (`None` = the entry program).
pub fn call_mut_positions(target: &CallTarget, module: Option<Sym>, user: &MutPositions) -> Option<Vec<usize>> {
    match target {
        CallTarget::Module { module: m, func, .. } => crate::mut_args::stdlib_mut_positions(m.as_str(), func.as_str())
            .or_else(|| user.get(&(*m, *func)).cloned()),
        CallTarget::Named { name } => user.get(&(module.unwrap_or_else(|| sym("")), *name)).cloned(),
        _ => None,
    }
}

/// The variable at the root of a place: `x`, `x.f`, `x.0.f`.
pub fn place_root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => place_root(object),
        _ => None,
    }
}

/// What a call made in one variable table may write.
pub struct CallWrites<'a> {
    module: Option<Sym>,
    user: &'a MutPositions,
    by_fn: HashMap<Sym, HashSet<u32>>,
    /// What a call through a closure value may write.
    any_call: HashSet<u32>,
}

/// The direct writes of an expression: assignment targets, and the roots of
/// the places `mut` arguments name. Lambda bodies are walked too; a caller
/// that does not run a lambda leaves it out itself.
struct Direct<'a> {
    module: Option<Sym>,
    user: &'a MutPositions,
    writes: HashSet<u32>,
    callees: HashSet<Sym>,
    computed: bool,
    in_lambda: usize,
    lambda_writes: HashSet<u32>,
}

impl IrVisitor for Direct<'_> {
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        let target = match &stmt.kind {
            IrStmtKind::Assign { var, .. }
            | IrStmtKind::IndexAssign { target: var, .. }
            | IrStmtKind::MapInsert { target: var, .. }
            | IrStmtKind::FieldAssign { target: var, .. }
            | IrStmtKind::ListSwap { target: var, .. }
            | IrStmtKind::ListReverse { target: var, .. }
            | IrStmtKind::ListRotateLeft { target: var, .. } => Some(*var),
            IrStmtKind::ListCopySlice { dst, .. } => Some(*dst),
            _ => None,
        };
        if let Some(v) = target {
            self.write(v);
        }
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
                if let Some(pos) = call_mut_positions(target, self.module, self.user) {
                    for root in pos.iter().filter_map(|&i| args.get(i)).filter_map(place_root) {
                        self.write(root);
                    }
                }
                match target {
                    CallTarget::Named { name } => {
                        self.callees.insert(*name);
                    }
                    CallTarget::Module { module, func, .. } if Some(*module) == self.module => {
                        self.callees.insert(*func);
                    }
                    CallTarget::Computed { .. } | CallTarget::Method { .. } => self.computed = true,
                    CallTarget::Module { .. } => {}
                }
            }
            IrExprKind::FnRef { name } => {
                self.callees.insert(*name);
            }
            _ => {}
        }
        if matches!(e.kind, IrExprKind::Lambda { .. }) {
            self.in_lambda += 1;
            walk_expr(self, e);
            self.in_lambda -= 1;
        } else {
            walk_expr(self, e);
        }
    }
}

impl Direct<'_> {
    fn write(&mut self, v: VarId) {
        self.writes.insert(v.0);
        if self.in_lambda > 0 {
            self.lambda_writes.insert(v.0);
        }
    }
}

impl<'a> CallWrites<'a> {
    /// The table `functions` + `top_lets` (a module named `module`, or the
    /// entry program for `None`).
    pub fn new(module: Option<Sym>, functions: &[IrFunction], top_lets: &[IrTopLet], user: &'a MutPositions) -> Self {
        let globals: HashSet<u32> = top_lets.iter().map(|t| t.var.0).collect();
        let mut direct: HashMap<Sym, (HashSet<u32>, HashSet<Sym>, bool)> = HashMap::new();
        let mut any_call: HashSet<u32> = HashSet::new();
        for f in functions {
            let mut d = Direct {
                module,
                user,
                writes: HashSet::new(),
                callees: HashSet::new(),
                computed: false,
                in_lambda: 0,
                lambda_writes: HashSet::new(),
            };
            d.visit_expr(&f.body);
            // A lambda may run after its frame is gone, so its writes to the
            // frame's own vars (C-319 cells) count for a closure call too.
            any_call.extend(d.lambda_writes.iter().copied());
            let writes: HashSet<u32> = d.writes.intersection(&globals).copied().collect();
            any_call.extend(writes.iter().copied());
            direct.insert(f.name, (writes, d.callees, d.computed));
        }
        for t in top_lets {
            let mut d = Direct {
                module,
                user,
                writes: HashSet::new(),
                callees: HashSet::new(),
                computed: false,
                in_lambda: 0,
                lambda_writes: HashSet::new(),
            };
            d.visit_expr(&t.value);
            any_call.extend(d.lambda_writes);
        }
        let mut by_fn: HashMap<Sym, HashSet<u32>> =
            direct.iter().map(|(f, (w, _, computed))| {
                let mut w = w.clone();
                if *computed {
                    w.extend(any_call.iter().copied());
                }
                (*f, w)
            }).collect();
        loop {
            let mut changed = false;
            for (f, (_, callees, _)) in &direct {
                let add: Vec<u32> = callees
                    .iter()
                    .filter_map(|c| by_fn.get(c))
                    .flat_map(|s| s.iter().copied())
                    .collect();
                let mine = by_fn.get_mut(f).expect("every fn has a row");
                for v in add {
                    changed |= mine.insert(v);
                }
            }
            if !changed {
                break;
            }
        }
        CallWrites { module, user, by_fn, any_call }
    }

    /// The variables evaluating `e` may write: its own assignments and `mut`
    /// arguments, and everything the calls it makes may write. A bare lambda
    /// operand is not run by being passed, so it writes nothing here.
    pub fn assigned_by(&self, e: &IrExpr) -> HashSet<u32> {
        if matches!(e.kind, IrExprKind::Lambda { .. }) {
            return HashSet::new();
        }
        let mut d = Direct {
            module: self.module,
            user: self.user,
            writes: HashSet::new(),
            callees: HashSet::new(),
            computed: false,
            in_lambda: 0,
            lambda_writes: HashSet::new(),
        };
        d.visit_expr(e);
        let mut out = d.writes;
        for c in &d.callees {
            if let Some(w) = self.by_fn.get(c) {
                out.extend(w.iter().copied());
            }
        }
        if d.computed {
            out.extend(self.any_call.iter().copied());
        }
        out
    }
}

/// An operand that may stay where it is when the others are bound: it
/// evaluates nothing and reads nothing a write could change.
fn stays(e: &IrExpr) -> bool {
    matches!(
        e.kind,
        IrExprKind::LitInt { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::LitBool { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::Unit
            | IrExprKind::OptionNone
            | IrExprKind::EmptyMap
            | IrExprKind::FnRef { .. }
            | IrExprKind::Lambda { .. }
    )
}

struct Hoister<'a, 'b> {
    writes: &'b CallWrites<'a>,
    vars: &'b mut VarTable,
    changed: bool,
    /// The walked fn's own `mut` params. A raise in such a fn hands every one
    /// of them back to the caller (`mut_param_err_carry`), so an operand that
    /// can raise reads them: it is bound before a call writes one in place.
    /// Left where it was, the place was copied ahead of the operand and the
    /// write landed in the copy (#4668).
    raise_reads: HashSet<VarId>,
}

/// Can evaluating `e` propagate an err out of the fn — a `!` / `?` outside
/// any lambda?
fn can_raise(e: &IrExpr) -> bool {
    struct Scan(bool);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            match e.kind {
                IrExprKind::Lambda { .. } => {}
                IrExprKind::Unwrap { .. } | IrExprKind::Try { .. } => self.0 = true,
                _ if !self.0 => walk_expr(self, e),
                _ => {}
            }
        }
    }
    let mut s = Scan(false);
    s.visit_expr(e);
    s.0
}

/// The `mut` params of `f`, as variables.
fn mut_param_vars(f: &IrFunction) -> HashSet<VarId> {
    f.mutated_params.iter().filter_map(|&i| f.params.get(i).map(|p| p.var)).collect()
}

impl IrMutVisitor for Hoister<'_, '_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::Call { target, args, .. } = &e.kind else { return };
        let Some(pos) = call_mut_positions(target, self.writes.module, self.writes.user) else { return };
        let places: Vec<(usize, VarId)> =
            pos.iter().filter_map(|&i| args.get(i).and_then(place_root).map(|r| (i, r))).collect();
        if places.is_empty() {
            return;
        }
        let is_place = |j: usize| places.iter().any(|&(i, _)| i == j);
        let written: HashSet<u32> = args
            .iter()
            .enumerate()
            .filter(|(j, _)| !is_place(*j))
            .flat_map(|(_, a)| self.writes.assigned_by(a))
            .collect();
        let raise_reads_a_place = places.iter().any(|(_, r)| self.raise_reads.contains(r))
            && args.iter().enumerate().any(|(j, a)| !is_place(j) && can_raise(a));
        if !raise_reads_a_place && !places.iter().any(|(_, r)| written.contains(&r.0)) {
            return;
        }
        let IrExprKind::Call { args, .. } = &mut e.kind else { return };
        let mut binds = Vec::new();
        for (j, a) in args.iter_mut().enumerate() {
            if is_place(j) || stays(a) {
                continue;
            }
            let (ty, span) = (a.ty.clone(), a.span);
            let id = self.vars.alloc(sym("__arg_before_mut_place"), ty.clone(), Mutability::Let, span);
            let value = std::mem::replace(a, IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None });
            binds.push(IrStmt { kind: IrStmtKind::Bind { var: id, mutability: Mutability::Let, ty, value }, span });
        }
        self.changed = true;
        let (ty, span) = (e.ty.clone(), e.span);
        let node = std::mem::take(e);
        *e = IrExpr { kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(node)) }, ty, span, def_id: None };
    }
}

/// The `mut`-place rule over the whole program (module doc). Returns whether
/// anything was rewritten.
pub fn hoist_operands_before_mut_places(program: &mut IrProgram) -> bool {
    let user = mut_positions(program);
    let mut changed = false;
    {
        let writes = CallWrites::new(None, &program.functions, &program.top_lets, &user);
        let mut h = Hoister { writes: &writes, vars: &mut program.var_table, changed: false, raise_reads: HashSet::new() };
        for f in program.functions.iter_mut() {
            h.raise_reads = mut_param_vars(f);
            h.visit_expr_mut(&mut f.body);
        }
        h.raise_reads.clear();
        for t in program.top_lets.iter_mut() {
            h.visit_expr_mut(&mut t.value);
        }
        changed |= h.changed;
    }
    for m in program.modules.iter_mut() {
        let writes = CallWrites::new(Some(m.name), &m.functions, &m.top_lets, &user);
        let mut h = Hoister { writes: &writes, vars: &mut m.var_table, changed: false, raise_reads: HashSet::new() };
        for f in m.functions.iter_mut() {
            h.raise_reads = mut_param_vars(f);
            h.visit_expr_mut(&mut f.body);
        }
        h.raise_reads.clear();
        for t in m.top_lets.iter_mut() {
            h.visit_expr_mut(&mut t.value);
        }
        changed |= h.changed;
    }
    changed
}
