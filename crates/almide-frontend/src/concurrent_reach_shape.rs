//! The syntactic pre-scan behind `Shape` (#3340), and the sites-only half of
//! it that can be asked without building an `Analyzer` (#3509): a compilation
//! none of whose programs can have a concurrent site never reads a reach
//! fact, so the module-set summaries need not be computed for it.

use std::collections::HashMap;

use almide_base::intern::Sym;
use almide_lang::ast::{self, Decl, Expr, ExprKind, Program, Stmt};

use super::{declared_slots_of, slot_heads, Analyzer, Shape, Summaries};

/// What one expression says about the program: (it may put an argument in a
/// concurrent slot, a `var` may be reached under it).
fn node_shape(e: &Expr, heads: &[Sym]) -> (bool, bool) {
    match &e.kind {
        ExprKind::Fan { .. }
        | ExprKind::FanSettle { .. }
        | ExprKind::FanRace { .. }
        | ExprKind::FanBounded { .. }
        | ExprKind::FanTimeout { .. }
        | ExprKind::FanRaceMap { .. } => (true, true),
        // `fan.map(…)`, `http.serve(…)`: a callee head spelled as the
        // module itself, which `resolve_callee` honours with no import.
        ExprKind::Ident { name } => (heads.contains(name), false),
        ExprKind::Block { stmts, .. } | ExprKind::ForIn { body: stmts, .. } | ExprKind::While { body: stmts, .. } => {
            (false, stmts.iter().any(|s| matches!(s, Stmt::Var { .. })))
        }
        _ => (false, false),
    }
}

/// Every top-level body the walk reads, in declaration order.
fn visit_bodies(prog: &Program, mut see: impl FnMut(&Expr)) {
    for d in &prog.decls {
        match d {
            Decl::Fn { body: Some(body), .. } => ast::visit_expr(body, &mut see),
            Decl::TopLet { value, .. } => ast::visit_expr(value, &mut see),
            Decl::Test { body, .. } => ast::visit_expr(body, &mut see),
            _ => {}
        }
    }
}

/// Whether a call resolved into module `m` can have concurrent slots:
/// `fan`, a stdlib module that declares `@concurrent`, or a user module
/// with a fn whose slots are known.
fn module_has_slots(ext: &Summaries, m: Sym) -> bool {
    slot_heads().contains(&m) || ext.get(&m).is_some_and(|fs| fs.values().any(|s| !s.slots.is_empty()))
}

/// `Shape::may_have_sites` without an `Analyzer` — a superset of it (a fn
/// name declared twice counts any copy's `@concurrent`), so `false` is the
/// same proof: no argument of `prog` sits in a concurrent slot, the reach
/// check records nothing, and slot inference infers nothing.
pub fn may_have_sites(prog: &Program, aliases: &HashMap<Sym, Sym>, direct: &HashMap<Sym, Sym>, ext: &Summaries) -> bool {
    let declared = prog.decls.iter().any(|d| match d {
        Decl::Fn { body: Some(_), params, attrs, .. } => !declared_slots_of(attrs, params).is_empty(),
        _ => false,
    });
    if declared || aliases.values().chain(direct.values()).any(|&m| module_has_slots(ext, m)) {
        return true;
    }
    let heads = slot_heads();
    let mut found = false;
    visit_bodies(prog, |e| found |= node_shape(e, heads).0);
    found
}

impl<'a> Analyzer<'a> {
    /// The pre-scan behind `Shape` (#3340).
    pub(super) fn scan_shape(&self, prog: &Program) -> Shape {
        let mut shape = Shape { may_have_sites: !self.slots.is_empty(), may_reach: !self.top_vars.is_empty() };
        // A module this program can name (`import m` / `import m.{f}`)
        // carries facts the walk reads through `ext` (`call_slots`,
        // `ext_ref`); every such read starts from these two maps.
        for &m in self.w.aliases.values().chain(self.w.direct.values()) {
            if module_has_slots(self.w.ext, m) {
                shape.may_have_sites = true;
            }
            if self.w.ext.get(&m).is_some_and(|fs| fs.values().any(|s| s.reach.is_some())) {
                shape.may_reach = true;
            }
        }
        let heads = slot_heads();
        visit_bodies(prog, |e| {
            let (sites, reach) = node_shape(e, heads);
            shape.may_have_sites |= sites;
            shape.may_reach |= reach;
        });
        shape
    }
}
