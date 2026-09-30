//! #3103 ruling (A): a `mut` argument is copied in and written back, so a
//! callee that reads the module-level `var` its argument is rooted at sees
//! the global's PRE-call value — on every leg. Native and the interpreter
//! pass a copy. The structural leg hands the callee the global's own block
//! (in place, no copy), so a callee reading the global mid-call saw its own
//! in-progress writes.
//!
//! The copy is paid only where it is observable: when the callee can REACH
//! that global — its body, or anything it calls transitively, reads or
//! writes it. Reach is a compile-time over-approximation over the program's
//! call graph: a call through a closure or fn value (a `Computed` or
//! `Method` target, or a fn reference that does not resolve) reaches every
//! global, and a lambda's body counts toward the fn that holds it. A callee
//! that cannot reach the global keeps the in-place, zero-copy call.
//!
//! The copy itself ([`Emitter::detach_global_mut_arg`]) runs after the
//! argument is read: the global is repointed at a fresh copy of the place
//! (the pre-call value), and the block the argument holds is the callee's
//! alone. The C-132 write-back stores the returned buffer into the place.

use std::collections::{HashMap, HashSet};

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrStmt, IrStmtKind, VarId};

use crate::emitter::Emitter;
use crate::*;

/// The globals a fn can reach; `None` = any (an unresolvable call).
pub(crate) type Reach = Option<HashSet<GVar>>;

/// Per table index: the globals the fn reaches, transitively.
pub(crate) fn global_reach(
    program_fns: &[(&IrFunction, Option<String>, u32)],
    table: &FnTable,
    globals: &HashMap<GVar, (u32, SliceTy)>,
) -> Vec<Reach> {
    let direct: Vec<(Reach, Vec<usize>)> = program_fns
        .iter()
        .map(|(f, qual, space)| {
            let mut s = Scan {
                table,
                cur_module: crate::emit::fn_module(qual.as_deref(), f),
                space: *space,
                globals,
                touched: Some(HashSet::new()),
                callees: Vec::new(),
            };
            s.visit_expr(&f.body);
            (s.touched, s.callees)
        })
        .collect();
    let mut reach: Vec<Reach> = direct.iter().map(|(t, _)| t.clone()).collect();
    loop {
        let mut changed = false;
        for (i, (_, callees)) in direct.iter().enumerate() {
            for &j in callees {
                if i == j {
                    continue;
                }
                let add = reach[j].clone();
                changed |= merge(&mut reach[i], add);
            }
        }
        if !changed {
            return reach;
        }
    }
}

/// `into ∪= from`; true when `into` grew.
fn merge(into: &mut Reach, from: Reach) -> bool {
    match (into.as_mut(), from) {
        (None, _) => false,
        (Some(_), None) => {
            *into = None;
            true
        }
        (Some(set), Some(add)) => {
            let before = set.len();
            set.extend(add);
            set.len() != before
        }
    }
}

struct Scan<'a> {
    table: &'a FnTable,
    cur_module: Option<&'a str>,
    space: u32,
    globals: &'a HashMap<GVar, (u32, SliceTy)>,
    touched: Reach,
    callees: Vec<usize>,
}

impl Scan<'_> {
    fn global(&mut self, id: VarId) {
        let g = (self.space, id);
        if self.globals.contains_key(&g)
            && let Some(set) = self.touched.as_mut()
        {
            set.insert(g);
        }
    }

    /// Every table fn a name may denote, as the emitter resolves it (the
    /// current module, the entry program, a qualified or `Type.method`
    /// suffix key) — over-approximated: more edges only cost a copy.
    fn named(&mut self, name: &str) -> bool {
        let before = self.callees.len();
        let suffix = format!(".{name}");
        for (key, &i) in &self.table.by_name {
            let hit = key == name
                || self.cur_module.is_some_and(|m| key.strip_prefix(m).and_then(|r| r.strip_prefix('.')) == Some(name))
                || key.ends_with(&suffix);
            if hit {
                self.callees.push(i);
            }
        }
        self.callees.len() != before
    }
}

impl IrVisitor for Scan<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Var { id } => self.global(*id),
            IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } => match target {
                // An unresolved bare name is a builtin or a constructor.
                CallTarget::Named { name } => {
                    self.named(name.as_str());
                }
                CallTarget::Module { module, func, .. } => {
                    if let Some(&i) = self.table.by_name.get(&format!("{module}.{func}")) {
                        self.callees.push(i);
                    }
                }
                CallTarget::Method { .. } | CallTarget::Computed { .. } => self.touched = None,
            },
            IrExprKind::FnRef { name } => {
                if !self.named(name.as_str()) {
                    self.touched = None;
                }
            }
            _ => {}
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Assign { var, .. } => self.global(*var),
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => self.global(*target),
            _ => {}
        }
        walk_stmt(self, s);
    }
}

impl Emitter<'_> {
    /// After a `mut` argument rooted at a module-level `var` was read for
    /// a call to table fn `callee`: when the callee can reach that global,
    /// repoint the global at a fresh copy of the place, so the callee's
    /// in-place writes land in the block the argument holds and a read of
    /// the global inside the call sees the pre-call value. Only an OWNED
    /// argument position is detached (a `mut` param always is): the site's
    /// share keeps the argument's block alive when the global lets go of it.
    /// A place with a tuple step keeps the in-place call.
    pub(crate) fn detach_global_mut_arg(&mut self, arg: &IrExpr, callee: usize) -> Result<(), EmitError> {
        let mut path: Vec<almide_base::intern::Sym> = Vec::new();
        let mut cur = arg;
        let id = loop {
            match &cur.kind {
                IrExprKind::Member { object, field } => {
                    path.push(*field);
                    cur = object;
                }
                IrExprKind::Var { id } => break *id,
                _ => return Ok(()),
            }
        };
        let g = (self.var_space, id);
        let Some(&(gidx, gty)) = self.globals.get(&g) else { return Ok(()) };
        if self.locals.contains_key(&id) {
            return Ok(());
        }
        let reaches = match self.work.global_reach.borrow().get(callee) {
            Some(None) => true,
            Some(Some(set)) => set.contains(&g),
            None => true,
        };
        if !reaches {
            return Ok(());
        }
        path.reverse();
        if path.is_empty() {
            if !self.rc_droppable(gty) {
                return Ok(());
            }
            let (copy, dec) = (self.copy_fn_of(gty), self.dec_fn_of(gty));
            let h = self.hold_i32()?;
            let mut i = self.f.instructions();
            i.global_get(gidx).call(copy).local_set(h);
            i.global_get(gidx).call(dec);
            i.local_get(h).global_set(gidx);
            self.release_i32();
            return Ok(());
        }
        let mut leaf = gty;
        for field in &path {
            leaf = self.record_field_slot(leaf, field)?.0;
        }
        if !self.rc_droppable(leaf) {
            return Ok(());
        }
        self.field_assign_with(&id, &path, false, |s, fty| {
            s.lower(arg, Some(fty))?;
            let copy = s.copy_fn_of(fty);
            s.f.instructions().call(copy);
            Ok(())
        })
    }
}
