//! A top-level `let` that names a function (`let inc = inc1`) is that
//! function at every use.
//!
//! The checked initializer is the function's eta form, `(x) => inc1(x)`:
//! total, capture-free and stateless, so reading the `let` can be neither
//! observed nor fail. A call through it becomes the direct call; a value use
//! gets its own copy of the eta form, with fresh binders, which the closure
//! paths lift like a lambda written in place. Without this the use is a
//! global read of a function value, which no lowering path materializes as a
//! callable block (#3388's `inc`).
use almide_ir::{visit_mut::{walk_expr_mut, IrMutVisitor}, CallTarget, IrExpr, IrExprKind, IrProgram, VarId, VarTable};
use almide_lang::intern::Sym;
use std::collections::HashMap;

/// `(p1, …, pn) => name(p1, …, pn)`: the function `name` itself.
fn eta_target(e: &IrExpr) -> Option<Sym> {
    let IrExprKind::Lambda { params, body, .. } = &e.kind else { return None };
    let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = &body.kind else { return None };
    let forwards = type_args.is_empty() && args.len() == params.len()
        && args.iter().zip(params).all(|(a, (p, _))| matches!(a.kind, IrExprKind::Var { id } if id == *p));
    forwards.then_some(*name)
}

pub(crate) fn expand(ir: &mut IrProgram) {
    let mut aliases: HashMap<VarId, (Sym, IrExpr)> = HashMap::new();
    // A chain (`let a = f`, `let b = a`) resolves in as many rounds as it is long.
    loop {
        let before = aliases.len();
        for t in ir.top_lets.iter().filter(|t| !t.mutable) {
            let found = match &t.value.kind {
                IrExprKind::Var { id } => aliases.get(id).cloned(),
                _ => eta_target(&t.value).map(|name| (name, t.value.clone())),
            };
            if let Some(found) = found { aliases.entry(t.var).or_insert(found); }
        }
        if aliases.len() == before { break; }
    }
    if aliases.is_empty() { return; }
    let mut pass = Rewrite { aliases: &aliases, vars: &mut ir.var_table };
    for f in &mut ir.functions {
        pass.visit_expr_mut(&mut f.body);
        for p in &mut f.params {
            if let Some(default) = &mut p.default { pass.visit_expr_mut(default); }
        }
    }
    for t in ir.top_lets.iter_mut().filter(|t| !aliases.contains_key(&t.var)) {
        pass.visit_expr_mut(&mut t.value);
    }
}

struct Rewrite<'a> { aliases: &'a HashMap<VarId, (Sym, IrExpr)>, vars: &'a mut VarTable }
impl Rewrite<'_> {
    fn alias(&self, e: &IrExpr) -> Option<&(Sym, IrExpr)> {
        match &e.kind {
            IrExprKind::Var { id } => self.aliases.get(id),
            _ => None,
        }
    }

    /// The alias's eta form over binders of its own.
    fn fresh_eta(&mut self, eta: &IrExpr) -> IrExprKind {
        let mut kind = eta.kind.clone();
        if let IrExprKind::Lambda { params, body, lambda_id } = &mut kind {
            *lambda_id = None;
            let mut renamed = HashMap::new();
            for (p, ty) in params.iter_mut() {
                let info = self.vars.get(*p).clone();
                let fresh = self.vars.alloc_like(*p, ty.clone(), info.mutability);
                renamed.insert(*p, fresh);
                *p = fresh;
            }
            if let IrExprKind::Call { args, .. } = &mut body.kind {
                for a in args {
                    if let IrExprKind::Var { id } = &mut a.kind {
                        if let Some(fresh) = renamed.get(id) { *id = *fresh; }
                    }
                }
            }
        }
        kind
    }
}
impl IrMutVisitor for Rewrite<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if let IrExprKind::Call { target, .. } = &mut e.kind {
            if let CallTarget::Computed { callee } = target {
                if let Some((name, _)) = self.alias(callee) { *target = CallTarget::Named { name: *name }; }
            }
        }
        if let Some((_, eta)) = self.alias(e) {
            let eta = eta.clone();
            e.kind = self.fresh_eta(&eta);
            return;
        }
        walk_expr_mut(self, e);
    }
}
