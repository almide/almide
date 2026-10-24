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
use super::graph::{bit, Edge, FnIx, Graph, NodeId, EFFECT_FN};
use super::index::{Index, Pools};
use almide_base::intern::Sym;
use super::{module_to_effect, runtime_name_to_effect};

/// A value of this type may be (or directly wrap) a fn: a fn type, a type
/// variable or unknown (pre-monomorphisation generics), or an
/// `Option`/`Result` of one (an `effect fn` returning a closure). A type
/// alias is its target (#4151: `type ReadFn = (String) -> …` held a fn the
/// scan did not follow, so `r(p)` through an aliased parameter ran nothing).
pub(super) fn may_hold_fn(ty: &Ty) -> bool {
    match ty {
        Ty::Fn { .. } | Ty::TypeVar(_) | Ty::Unknown => true,
        Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, args) => args.first().is_some_and(may_hold_fn),
        Ty::Named(..) => aliases::target(ty).is_some_and(|t| may_hold_fn(&t)),
        _ => false,
    }
}

/// The type aliases of the program being solved, by declared name and by
/// `module.name` (set for the duration of one solve).
pub(super) mod aliases {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use almide_ir::{IrProgram, IrTypeDeclKind};
    use almide_lang::types::Ty;

    thread_local! {
        static TABLE: RefCell<HashMap<String, Ty>> = RefCell::new(HashMap::new());
    }

    /// Installs `program`'s aliases until the guard drops.
    pub struct Scope;

    impl Scope {
        pub fn new(program: &IrProgram) -> Scope {
            let mut table = HashMap::new();
            let decls = program.type_decls.iter().map(|d| (None, d))
                .chain(program.modules.iter().flat_map(|m| m.type_decls.iter().map(move |d| (Some(m.name), d))));
            for (module, d) in decls {
                if let IrTypeDeclKind::Alias { target } = &d.kind {
                    table.insert(d.name.to_string(), target.clone());
                    if let Some(m) = module {
                        table.insert(format!("{}.{}", m, d.name), target.clone());
                    }
                }
            }
            TABLE.with(|t| *t.borrow_mut() = table);
            Scope
        }
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            TABLE.with(|t| t.borrow_mut().clear());
        }
    }

    /// The alias `ty` names, followed to a non-alias (bounded: a cycle is
    /// refused by the checker, the bound only keeps this total).
    pub fn target(ty: &Ty) -> Option<Ty> {
        let mut cur = ty.clone();
        let mut hops = 0;
        while let Ty::Named(name, _) = &cur {
            let next = TABLE.with(|t| {
                let t = t.borrow();
                t.get(name.as_str()).or_else(|| name.as_str().rsplit_once('.').and_then(|(_, n)| t.get(n))).cloned()
            })?;
            cur = next;
            hops += 1;
            if hops > 16 {
                return None;
            }
        }
        (hops > 0).then_some(cur)
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
                self.g.site = e.span;
                let label = format!("{} ({})", symbol, loc(e.span));
                self.perform(runtime_name_to_effect(symbol), label.clone());
                self.perform_effect_fn(almide_ir::effect::runtime_symbol_is_effect(symbol), label);
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
                if !may_hold_fn(&e.ty) {
                    return None;
                }
                // The field's own node, and the pool at the field's type: a
                // generic body stores a type-variable value in a field
                // without tracking it, and its caller pooled it there.
                let f = Some(self.field(*field));
                let p = self.untracked(&e.ty);
                self.join(&[f, p])
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
                self.untracked(&e.ty)
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
        self.untracked(ty)
    }

    /// Evaluates `e` for what it performs; a fn value it yields escapes.
    pub fn consume(&mut self, e: &IrExpr) {
        let v = self.flow(e);
        self.escape(v, &e.ty);
    }

    /// A fn value leaves every tracked position: callers of untracked values
    /// of its arity are charged with it.
    pub fn escape(&mut self, v: Option<NodeId>, ty: &Ty) {
        // A value whose type cannot hold a fn carries none (a user call's
        // result node exists whatever it returns): pooling it at the
        // wildcard charged every untracked call with it (#4151).
        // A value typed by a bare type variable (inside a generic body) was
        // pooled at its concrete type by the call site that passed it in
        // (`Scan::user_call`); pooling it again at the wildcard made every
        // signature's reads see every other's (#4151).
        if let Some(v) = v.filter(|_| may_hold_fn(ty) && !super::index::is_bare_typevar(ty)) {
            let to = self.pools.write(self.g, ty);
            self.g.push(Edge::Concretize { to, from: v, scope: self.scope });
        }
    }

    /// The untracked fn values an expression of type `ty` may evaluate to:
    /// the pool of its signature. A bare type variable reads nothing — a
    /// generic body never calls such a value, and every concrete-typed place
    /// it comes back out at reads the pool there (a call's result, a lambda
    /// parameter, a field).
    fn untracked(&mut self, ty: &Ty) -> Option<NodeId> {
        if !may_hold_fn(ty) || super::index::is_bare_typevar(ty) {
            return None;
        }
        Some(self.pools.read(self.g, ty))
    }

    /// The current performer runs the fn value `v`.
    fn run(&mut self, v: Option<NodeId>) {
        if let Some(v) = v {
            self.g.copy(self.performer, v);
        }
    }

    fn perform(&mut self, cat: Option<almide_ir::effect::Effect>, label: String) {
        if let Some(c) = cat {
            self.g.push(Edge::Const { to: self.performer, cats: bit(c), sym: None, label: Some(label) });
        }
    }

    /// The current performer runs a stdlib `effect fn` the program does not
    /// scan: it carries the effect bit (#4151).
    fn perform_effect_fn(&mut self, is_effect: bool, label: String) {
        if is_effect {
            self.g.push(Edge::Const { to: self.performer, cats: EFFECT_FN, sym: None, label: Some(label) });
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
        self.untracked(ty)
    }

    /// The node `{arg i}` of the scope function.
    pub fn sym_node(&mut self, i: usize) -> NodeId {
        self.sym(i)
    }

    fn sym(&mut self, i: usize) -> NodeId {
        if let Some(&n) = self.syms.get(&i) {
            return n;
        }
        let n = self.g.node(None);
        self.g.push(Edge::Const { to: n, cats: 0, sym: Some(i), label: None });
        self.syms.insert(i, n);
        n
    }

    fn fn_ref(&mut self, name: &str) -> Option<NodeId> {
        if let Some(f) = self.ix.resolve(self.table, name) {
            self.sh.taken.insert(f);
            let val = self.ix.fns[f].fn_val;
            // A stdlib fn the program carries (bundled): its category rides
            // the value too, as a call of it performs it (#4152).
            let Some(c) = name.split_once('.').and_then(|(m, f)| module_to_effect(m, f)) else { return Some(val) };
            let n = self.g.node(None);
            self.g.copy(n, val);
            self.g.push(Edge::Const { to: n, cats: bit(c), sym: None, label: Some(name.to_string()) });
            return Some(n);
        }
        // A stdlib fn taken as a value: its module's category, and the
        // effect bit when it is an `effect fn` (#4151).
        let cat = name.split_once('.').and_then(|(m, f)| module_to_effect(m, f)).or_else(|| runtime_name_to_effect(name));
        let effect = name.split_once('.').is_some_and(|(m, f)| almide_ir::effect::stdlib_fn_is_effect(m, f))
            || almide_ir::effect::runtime_symbol_is_effect(name);
        let cats = cat.map_or(0, bit) | if effect { EFFECT_FN } else { 0 };
        (cats != 0).then(|| {
            let n = self.g.node(None);
            self.g.push(Edge::Const { to: n, cats, sym: None, label: Some(name.to_string()) });
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
                self.g.site = e.span;
                if let Some(f) = self.ix.resolve(self.table, name) {
                    return self.user_call(f, vals, args, e);
                }
                let label = format!("{} ({})", name, loc(e.span));
                self.perform(runtime_name_to_effect(name), label.clone());
                self.perform_effect_fn(almide_ir::effect::runtime_symbol_is_effect(name), label);
                self.store_args(&vals, args);
                self.untracked(&e.ty)
            }
            CallTarget::Module { module, func, .. } => {
                let vals: Vec<Option<NodeId>> = args.iter().map(|a| self.flow(a)).collect();
                self.g.site = e.span;
                self.perform(module_to_effect(module, func), format!("{}.{} ({})", module, func, loc(e.span)));
                if let Some(f) = self.ix.resolve(self.table, &format!("{module}.{func}")) {
                    return self.user_call(f, vals, args, e);
                }
                // Unscanned: the stdlib declaration says whether it is an
                // `effect fn` (a scanned one carries the bit on its own node).
                let is_effect = almide_ir::effect::stdlib_fn_is_effect(module.as_str(), func.as_str());
                self.perform_effect_fn(is_effect, format!("{}.{} ({})", module, func, loc(e.span)));
                // What its declaration says it does with each fn argument.
                if let Some(uses) = almide_ir::effect::stdlib_param_uses(module.as_str(), func.as_str()) {
                    for (i, (v, a)) in vals.iter().zip(args).enumerate() {
                        let u = uses.get(i).copied().unwrap_or(almide_ir::effect::ParamUse { runs: true, keeps: true });
                        if u.runs {
                            self.run(*v);
                        }
                        if u.keeps {
                            self.escape(*v, &a.ty);
                        }
                    }
                    return self.untracked(&e.ty);
                }
                self.opaque_call_vals(&vals, args, &e.ty)
            }
            CallTarget::Method { object, method } => {
                let vals: Vec<Option<NodeId>> =
                    std::iter::once(object.as_ref()).chain(args.iter()).map(|a| self.flow(a)).collect();
                self.g.site = e.span;
                let all: Vec<IrExpr> = std::iter::once((**object).clone()).chain(args.iter().cloned()).collect();
                if let Some(f) = self.ix.resolve(self.table, method) {
                    return self.user_call(f, vals, &all, e);
                }
                self.opaque_call_vals(&vals, &all, &e.ty)
            }
            CallTarget::Computed { callee } => {
                let c = self.flow(callee);
                self.g.site = e.span;
                self.run(c);
                let vals: Vec<Option<NodeId>> = args.iter().map(|a| self.flow(a)).collect();
                self.store_args(&vals, args);
                self.untracked(&e.ty)
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
        self.untracked(ret)
    }

    fn store_args(&mut self, vals: &[Option<NodeId>], args: &[IrExpr]) {
        for (v, a) in vals.iter().zip(args) {
            self.escape(*v, &a.ty);
        }
    }

    /// A call of a scanned function `f`: what `f` performs with each of its
    /// parameter syms replaced by the argument's set, and what it returns
    /// likewise. Each argument is also recorded as received by `f`.
    fn user_call(&mut self, f: FnIx, mut vals: Vec<Option<NodeId>>, args: &[IrExpr], e: &IrExpr) -> Option<NodeId> {
        if let Some(func) = self.ix.fns[f].func.filter(|func| super::is_foreign_bodied(func)) {
            return self.foreign_call(f, func, vals, args, e);
        }
        // A value passed into a type-variable parameter is pooled here, at
        // its concrete type; the generic body does not pool it (`escape`).
        if let Some(func) = self.ix.fns[f].func {
            for (i, (v, a)) in vals.iter().zip(args).enumerate() {
                if func.params.get(i).is_some_and(|p| super::index::is_bare_typevar(&p.ty)) {
                    self.escape(*v, &a.ty);
                }
            }
        }
        let arity = self.g.param_in[f].len();
        vals.resize(arity.max(vals.len()), None);
        for (i, v) in vals.iter_mut().enumerate().take(arity) {
            match v {
                Some(a) => self.g.push(Edge::Concretize { to: self.g.param_in[f][i], from: *a, scope: self.scope }),
                // A defaulted argument: whatever the parameter receives.
                None => *v = Some(self.g.param_in[f][i]),
            }
        }
        let (own, ret) = (self.ix.fns[f].own, self.ix.fns[f].ret);
        self.g.push(Edge::Inst { to: self.performer, src: own, callee: f, args: vals.clone() });
        let t = self.g.node(None);
        self.g.push(Edge::Inst { to: t, src: ret, callee: f, args: vals });
        // A generic result read back at the call's concrete type.
        if self.ix.fns[f].func.is_some_and(|func| super::index::is_bare_typevar(&func.ret_ty))
            && let Some(p) = self.untracked(&e.ty)
        {
            self.g.copy(t, p);
        }
        Some(t)
    }
}

impl Scan<'_, '_> {
    /// A call of a foreign-bodied fn (`@intrinsic`, `= _`): what its summary
    /// performs, instantiated with the arguments (the callbacks it runs);
    /// every other argument that may hold a fn escapes at its own type, and
    /// the result is read from the pools at the call's type (#4151, #4152).
    fn foreign_call(&mut self, f: FnIx, func: &IrFunction, vals: Vec<Option<NodeId>>, args: &[IrExpr], e: &IrExpr) -> Option<NodeId> {
        let own = self.ix.fns[f].own;
        for (i, (v, a)) in vals.iter().zip(args).enumerate() {
            let callback = func.params.get(i).is_some_and(|p| matches!(p.ty, Ty::Fn { .. }));
            if !callback {
                self.escape(*v, &a.ty);
            }
        }
        self.g.push(Edge::Inst { to: self.performer, src: own, callee: f, args: vals });
        self.untracked(&e.ty)
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
                    self.g.push(Edge::Concretize { to, from: v, scope: self.scope });
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
            scan.g.push(Edge::Concretize { to, from: v, scope: scan.scope });
        }
        // A pattern (`let { run } = b`, a variant case) reads it back untracked.
        scan.escape(v, &value.ty);
    }
}
