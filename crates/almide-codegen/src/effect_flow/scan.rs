//! Constraint generation: one walk over a function body (ADR-0026 D1).
//!
//! `flow` returns the node holding the category set of the fn value an
//! expression evaluates to (`None`: it holds no fn), and records what the
//! expression *performs* on the current performer — the function's own set,
//! or a lambda's while its body is walked. Creating a lambda performs
//! nothing; calling one performs its set.
//!
//! Tracked positions keep a value's own set: a `let`/`var` local, a fn
//! parameter, a return value, a record field (by field name), a top-level
//! `let`. A fn value put anywhere else (a list, an option or variant
//! payload, a tuple, an argument of a call whose callee is not known) is
//! *escaped* into the pool of its arity, and a fn value read from such a
//! position (a lambda or `for` parameter, a pattern binding, an element)
//! carries the pool of its arity. So nothing a program can call is dropped.

use std::collections::{BTreeSet, HashMap};
use almide_ir::*;
use almide_ir::visit::{walk_expr, walk_pattern, walk_stmt, IrVisitor};
use almide_lang::types::{Ty, TypeConstructorId};
use super::graph::{bit, Edge, FnIx, Graph, NodeId};
use super::index::{Index, Pools};
use almide_base::intern::Sym;
use super::{module_to_effect, runtime_name_to_effect};

/// A value of this type may be (or directly wrap) a fn: a fn type, a type
/// variable or unknown (pre-monomorphisation generics), or an
/// `Option`/`Result` of one (an `effect fn` returning a closure).
pub(super) fn may_hold_fn(ty: &Ty) -> bool {
    match ty {
        Ty::Fn { .. } | Ty::TypeVar(_) | Ty::Unknown => true,
        Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, args) => args.first().is_some_and(may_hold_fn),
        _ => false,
    }
}

pub(super) fn loc(span: Option<Span>) -> String {
    span.map_or_else(|| "unknown line".to_string(), |s| format!("line {}:{}", s.line, s.col))
}

/// State every function's scan adds to.
#[derive(Default)]
pub(super) struct Shared {
    /// Functions taken as a value (`FnRef`): wired after every scan.
    pub taken: BTreeSet<FnIx>,
    /// One global node per record field name.
    pub fields: HashMap<Sym, NodeId>,
}

pub(super) struct Scan<'g, 'a> {
    pub g: &'g mut Graph,
    pub ix: &'g Index<'a>,
    pub pools: &'g Pools,
    pub sh: &'g mut Shared,
    pub table: usize,
    pub scope: FnIx,
    pub params: HashMap<VarId, usize>,
    pub locals: HashMap<VarId, NodeId>,
    pub syms: HashMap<usize, NodeId>,
    pub performer: NodeId,
    /// Where a `guard … else v` early return goes: the function's return
    /// node, or `None` inside a lambda (its result is untracked).
    pub ret_target: Option<NodeId>,
}

impl Scan<'_, '_> {
    /// Allocates a local node for every `let`/`var` that may hold a fn, so a
    /// read before the (textually later) assignment still sees it.
    pub fn bind_locals(&mut self, body: &IrExpr) {
        struct Binds(Vec<VarId>);
        impl IrVisitor for Binds {
            fn visit_stmt(&mut self, s: &IrStmt) {
                if let IrStmtKind::Bind { var, ty, .. } = &s.kind
                    && may_hold_fn(ty)
                {
                    self.0.push(*var);
                }
                walk_stmt(self, s);
            }
        }
        let mut b = Binds(Vec::new());
        b.visit_expr(body);
        for v in b.0 {
            let n = self.g.node(None);
            self.locals.insert(v, n);
        }
    }

    pub fn flow(&mut self, e: &IrExpr) -> Option<NodeId> {
        match &e.kind {
            IrExprKind::Var { id } => self.read_var(*id, &e.ty),
            IrExprKind::FnRef { name } | IrExprKind::ClosureCreate { func_name: name, .. } => self.fn_ref(name),
            IrExprKind::Lambda { body, .. } => Some(self.lambda(body, e.span)),
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => self.call(target, args, e),
            IrExprKind::RuntimeCall { symbol, args } => {
                let label = format!("{} ({})", symbol, loc(e.span));
                self.perform(runtime_name_to_effect(symbol), label);
                self.opaque_call(args, &e.ty)
            }
            IrExprKind::If { cond, then, else_ } => {
                self.consume(cond);
                let branches = [self.flow(then), self.flow(else_)];
                self.join(&branches)
            }
            IrExprKind::Match { subject, arms } => self.match_(subject, arms),
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.visit_stmt(s);
                }
                expr.as_deref().and_then(|t| self.flow(t))
            }
            IrExprKind::Member { object, field } | IrExprKind::OptionalChain { expr: object, field } => {
                self.consume(object);
                may_hold_fn(&e.ty).then(|| self.field(*field))
            }
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } | IrExprKind::ToOption { expr }
            | IrExprKind::Clone { expr } | IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. }
            | IrExprKind::BoxNew { expr } | IrExprKind::RcWrap { expr, .. } => self.flow(expr),
            // Pass the wrapped value through for `make()!`, and escape it too:
            // a `match` reads the payload back through a pattern binding.
            IrExprKind::ResultOk { expr } | IrExprKind::OptionSome { expr } => {
                let v = self.flow(expr);
                self.escape(v, &expr.ty);
                v
            }
            IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
                if let IrExprKind::SpreadRecord { base, .. } = &e.kind {
                    self.consume(base);
                }
                record_fields(self, fields);
                None
            }
            IrExprKind::UnwrapOr { expr, fallback } => {
                let both = [self.flow(expr), self.flow(fallback)];
                self.join(&both)
            }
            IrExprKind::Fan { exprs } => {
                self.perform(Some(almide_ir::effect::Effect::Fan), format!("fan ({})", loc(e.span)));
                exprs.iter().for_each(|x| self.consume(x));
                None
            }
            IrExprKind::IterChain { source, steps, collector, .. } => self.iter_chain(source, steps, collector, &e.ty),
            _ => {
                walk_expr(self, e);
                may_hold_fn(&e.ty).then(|| self.pools.read(&e.ty))
            }
        }
    }

    /// A fused iterator chain runs every step's callback.
    fn iter_chain(&mut self, source: &IrExpr, steps: &[IterStep], collector: &IterCollector, ty: &Ty) -> Option<NodeId> {
        self.consume(source);
        let lambdas = steps.iter().filter_map(IterStep::lambda).chain(collector.lambda());
        for l in lambdas {
            let v = self.flow(l);
            self.run(v);
        }
        if let IterCollector::Fold { init, .. } = collector {
            self.consume(init);
        }
        may_hold_fn(ty).then(|| self.pools.read(ty))
    }

    /// Evaluates `e` for what it performs; a fn value it yields escapes.
    pub fn consume(&mut self, e: &IrExpr) {
        let v = self.flow(e);
        self.escape(v, &e.ty);
    }

    /// A fn value leaves every tracked position: callers of untracked values
    /// of its arity are charged with it.
    pub fn escape(&mut self, v: Option<NodeId>, ty: &Ty) {
        if let Some(v) = v {
            let to = self.pools.write(ty);
            self.g.edges.push(Edge::Concretize { to, from: v, scope: self.scope });
        }
    }

    /// The current performer runs the fn value `v`.
    fn run(&mut self, v: Option<NodeId>) {
        if let Some(v) = v {
            self.g.copy(self.performer, v);
        }
    }

    fn perform(&mut self, cat: Option<almide_ir::effect::Effect>, label: String) {
        if let Some(c) = cat {
            self.g.edges.push(Edge::Const { to: self.performer, cats: bit(c), sym: None, label: Some(label) });
        }
    }

    fn join(&mut self, vs: &[Option<NodeId>]) -> Option<NodeId> {
        let some: Vec<NodeId> = vs.iter().flatten().copied().collect();
        match some.as_slice() {
            [] => None,
            [one] => Some(*one),
            many => {
                let t = self.g.node(None);
                many.iter().for_each(|&m| self.g.copy(t, m));
                Some(t)
            }
        }
    }

    fn match_(&mut self, subject: &IrExpr, arms: &[IrMatchArm]) -> Option<NodeId> {
        self.consume(subject);
        let mut bodies = Vec::with_capacity(arms.len());
        for arm in arms {
            walk_pattern(self, &arm.pattern);
            if let Some(guard) = &arm.guard {
                self.consume(guard);
            }
            bodies.push(self.flow(&arm.body));
        }
        self.join(&bodies)
    }

    fn field(&mut self, name: Sym) -> NodeId {
        if let Some(&n) = self.sh.fields.get(&name) {
            return n;
        }
        let n = self.g.node(Some(format!("field `{name}`")));
        self.sh.fields.insert(name, n);
        n
    }

    fn read_var(&mut self, id: VarId, ty: &Ty) -> Option<NodeId> {
        if let Some(&i) = self.params.get(&id) {
            return Some(self.sym(i));
        }
        if let Some(&n) = self.locals.get(&id).or_else(|| self.ix.toplets[self.table].get(&id)) {
            return Some(n);
        }
        may_hold_fn(ty).then(|| self.pools.read(ty))
    }

    /// The node `{arg i}` of the scope function.
    fn sym(&mut self, i: usize) -> NodeId {
        if let Some(&n) = self.syms.get(&i) {
            return n;
        }
        let n = self.g.node(None);
        self.g.edges.push(Edge::Const { to: n, cats: 0, sym: Some(i), label: None });
        self.syms.insert(i, n);
        n
    }

    fn fn_ref(&mut self, name: &str) -> Option<NodeId> {
        if let Some(f) = self.ix.resolve(self.table, name) {
            self.sh.taken.insert(f);
            return Some(self.ix.fns[f].fn_val);
        }
        // A stdlib fn taken as a value: its module's category.
        let cat = name.split_once('.').and_then(|(m, _)| module_to_effect(m)).or_else(|| runtime_name_to_effect(name));
        cat.map(|c| {
            let n = self.g.node(None);
            self.g.edges.push(Edge::Const { to: n, cats: bit(c), sym: None, label: Some(name.to_string()) });
            n
        })
    }

    fn lambda(&mut self, body: &IrExpr, span: Option<Span>) -> NodeId {
        let label = format!("closure ({} in {})", loc(span), self.g.fn_names[self.scope]);
        let l = self.g.node(Some(label));
        let saved = (self.performer, self.ret_target.take());
        self.performer = l;
        let tail = self.flow(body);
        // What a closure returns is read by whoever calls it: untracked.
        self.escape(tail, &body.ty);
        (self.performer, self.ret_target) = saved;
        l
    }

    fn call(&mut self, target: &CallTarget, args: &[IrExpr], e: &IrExpr) -> Option<NodeId> {
        match target {
            CallTarget::Named { name } => {
                let vals: Vec<Option<NodeId>> = args.iter().map(|a| self.flow(a)).collect();
                if let Some(f) = self.ix.resolve(self.table, name) {
                    return self.user_call(f, vals);
                }
                self.perform(runtime_name_to_effect(name), format!("{} ({})", name, loc(e.span)));
                self.store_args(&vals, args);
                may_hold_fn(&e.ty).then(|| self.pools.read(&e.ty))
            }
            CallTarget::Module { module, func, .. } => {
                let vals: Vec<Option<NodeId>> = args.iter().map(|a| self.flow(a)).collect();
                self.perform(module_to_effect(module), format!("{}.{} ({})", module, func, loc(e.span)));
                if let Some(f) = self.ix.resolve(self.table, &format!("{module}.{func}")) {
                    return self.user_call(f, vals);
                }
                self.opaque_call_vals(&vals, args, &e.ty)
            }
            CallTarget::Method { object, method } => {
                let vals: Vec<Option<NodeId>> =
                    std::iter::once(object.as_ref()).chain(args.iter()).map(|a| self.flow(a)).collect();
                if let Some(f) = self.ix.resolve(self.table, method) {
                    return self.user_call(f, vals);
                }
                let all: Vec<IrExpr> = std::iter::once((**object).clone()).chain(args.iter().cloned()).collect();
                self.opaque_call_vals(&vals, &all, &e.ty)
            }
            CallTarget::Computed { callee } => {
                let c = self.flow(callee);
                self.run(c);
                let vals: Vec<Option<NodeId>> = args.iter().map(|a| self.flow(a)).collect();
                self.store_args(&vals, args);
                may_hold_fn(&e.ty).then(|| self.pools.read(&e.ty))
            }
        }
    }

    /// A call whose callee is not a scanned function (stdlib, runtime): it
    /// may run every fn argument and may keep it.
    fn opaque_call(&mut self, args: &[IrExpr], ret: &Ty) -> Option<NodeId> {
        let vals: Vec<Option<NodeId>> = args.iter().map(|a| self.flow(a)).collect();
        self.opaque_call_vals(&vals, args, ret)
    }

    fn opaque_call_vals(&mut self, vals: &[Option<NodeId>], args: &[IrExpr], ret: &Ty) -> Option<NodeId> {
        vals.iter().for_each(|&v| self.run(v));
        self.store_args(vals, args);
        may_hold_fn(ret).then(|| self.pools.read(ret))
    }

    fn store_args(&mut self, vals: &[Option<NodeId>], args: &[IrExpr]) {
        for (v, a) in vals.iter().zip(args) {
            self.escape(*v, &a.ty);
        }
    }

    /// A call of a scanned function `f`: what `f` performs with each of its
    /// parameter syms replaced by the argument's set, and what it returns
    /// likewise. Each argument is also recorded as received by `f`.
    fn user_call(&mut self, f: FnIx, mut vals: Vec<Option<NodeId>>) -> Option<NodeId> {
        let arity = self.g.param_in[f].len();
        vals.resize(arity.max(vals.len()), None);
        for (i, v) in vals.iter_mut().enumerate().take(arity) {
            match v {
                Some(a) => self.g.edges.push(Edge::Concretize { to: self.g.param_in[f][i], from: *a, scope: self.scope }),
                // A defaulted argument: whatever the parameter receives.
                None => *v = Some(self.g.param_in[f][i]),
            }
        }
        let (own, ret) = (self.ix.fns[f].own, self.ix.fns[f].ret);
        self.g.edges.push(Edge::Inst { to: self.performer, src: own, callee: f, args: vals.clone() });
        let t = self.g.node(None);
        self.g.edges.push(Edge::Inst { to: t, src: ret, callee: f, args: vals });
        Some(t)
    }
}

impl IrVisitor for Scan<'_, '_> {
    /// A child of a node `flow` does not model: evaluated, and a fn value it
    /// yields escapes.
    fn visit_expr(&mut self, e: &IrExpr) {
        self.consume(e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Bind { var, value, .. } | IrStmtKind::Assign { var, value } => {
                let v = self.flow(value);
                match (v, self.locals.get(var).copied()) {
                    (Some(v), Some(l)) => self.g.copy(l, v),
                    (v, _) => self.escape(v, &value.ty),
                }
            }
            IrStmtKind::FieldAssign { field, value, .. } => {
                let v = self.flow(value);
                if let Some(v) = v {
                    let to = self.field(*field);
                    self.g.edges.push(Edge::Concretize { to, from: v, scope: self.scope });
                }
            }
            IrStmtKind::Guard { cond, else_ } => {
                self.consume(cond);
                let v = self.flow(else_);
                match (v, self.ret_target) {
                    (Some(v), Some(r)) => self.g.copy(r, v),
                    (v, _) => self.escape(v, &else_.ty),
                }
            }
            IrStmtKind::Expr { expr } => {
                self.flow(expr);
            }
            _ => walk_stmt(self, s),
        }
    }
}

/// Records stored through a record literal: each fn-valued field joins the
/// field's global set (a stored fn value keeps the set of the value stored).
pub(super) fn record_fields(scan: &mut Scan<'_, '_>, fields: &[(Sym, IrExpr)]) {
    for (name, value) in fields {
        let v = scan.flow(value);
        if let Some(v) = v {
            let to = scan.field(*name);
            scan.g.edges.push(Edge::Concretize { to, from: v, scope: scan.scope });
        }
        // A pattern (`let { run } = b`, a variant case) reads it back untracked.
        scan.escape(v, &value.ty);
    }
}
