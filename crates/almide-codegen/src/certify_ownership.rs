//! Native ownership certifier (#2231): re-derive what each variable
//! occurrence DOES from the final IR and check it against the verdicts the
//! ownership passes committed to — a param's borrow mode, a `Clone` node —
//! independently of the passes that produced them.
//!
//! rustc is the linear-lifetime checker for every move and borrow the emitted
//! Rust spells, and a wrong verdict in that direction is a loud build error.
//! The direction rustc cannot see is the OTHER one: a value cloned where a
//! borrow would do, a param owned that no occurrence ever consumes. Such a
//! program builds, prints the right lines on both legs, and only allocates
//! more than it should (the allocation ledger, #2228, pins six programs; this
//! certifier checks every function of every build). roc's `arc_certify` and
//! Swift's `SILOwnershipVerifier` are the references: the emitted schedule is
//! re-checked against the ownership rules on every debug compile.
//!
//! The evidence is the one use-kind walk the passes themselves read
//! ([`UseSites`], `ExplicitBorrows` oracle: after `BorrowInsertion` every
//! borrow is a `Borrow` node, so a bare argument is consumed). Three checks:
//!
//! - **C1 borrowed-then-consumed**: a param rendered `&T` / `&str` / `&[T]`
//!   has an occurrence that moves it (returned, concatenated, built into a
//!   constructor, handed bare to a call slot, assigned, iterated by value)
//!   outside any closure and not under a `Clone`. rustc refuses this too
//!   (E0507); the certifier names the pass instead of the borrow checker.
//! - **C3 clone-at-last-use**: a `Clone` of an owned local or owned param
//!   outside every loop and closure, with no later occurrence of that
//!   variable anywhere in the body. Ownership was available; the clone is a
//!   copy for nothing. rustc is blind to it.
//! - **C4 owned-never-consumed**: a heap-typed param rendered owned with no
//!   occurrence that needs ownership — nothing moves it, mutates it, captures
//!   it, or hands it on — so every caller pays a clone the body never uses.
//!   rustc is blind to it.
//! - **C5 closure-escape** (#2288): a fn-typed param rendered `&dyn Fn` whose
//!   callable escapes the call (returned, stored, captured, handed to an
//!   owned slot), or a closure literal boxed as `Rc<dyn Fn>` at a slot the
//!   callee borrows — the allocation the borrowed slot exists to remove.
//!
//! Each check errs towards silence: an occurrence whose meaning depends on
//! the callee (a method receiver, a match scrutinee, a computed callee, a
//! capture) JUSTIFIES ownership for C4 and is NOT a consumption for C1, and
//! C3 skips every variable the passes treat specially (shared cells, COW
//! locals, always-clone vars, loop binders, TCO-owned params, globals,
//! pattern binders). A violation is therefore a real defect or a rule the
//! pass holds and this file does not yet state — never noise.

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use almide_ir::annotations::CodegenAnnotations;
use almide_lang::types::Ty;
use crate::use_kind::{Ctor, ExplicitBorrows, Site, SlotMode, Use, UseSites};

/// Every violation in `program`, one line each: the function, the variable,
/// the check and what was seen.
pub fn certify(program: &IrProgram) -> Vec<String> {
    let ann = &program.codegen_annotations;
    let records = crate::pass_borrow_inference::seed_record_names(program);
    let variants = crate::pass_borrow_inference::seed_variant_names(program);
    let mut out: Vec<String> = program.functions.iter().flat_map(|f| certify_fn(f, &program.var_table, ann, &records, &variants)).collect();
    out.extend(certify_closure_sites(program));
    out
}

/// **C5 closure-escape, the call-site half** (#2288): a closure literal
/// boxed as an `Rc<dyn Fn>` (`RcWrap`) handed to a slot the callee borrows
/// (`&dyn Fn`) — the allocation the borrowed slot exists to remove. The
/// body half (a borrowed fn param that escapes) is [`certify_param`]'s.
fn certify_closure_sites(program: &IrProgram) -> Vec<String> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    let sigs: HashMap<String, Vec<(ParamBorrow, bool)>> = program.functions.iter()
        .map(|f| (f.name.to_string(), f.params.iter().map(|p| (p.borrow, matches!(p.ty, Ty::Fn { .. }))).collect()))
        .collect();
    struct Sites<'a> { sigs: &'a HashMap<String, Vec<(ParamBorrow, bool)>>, current: String, out: Vec<String> }
    impl IrVisitor for Sites<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind
                && let Some(params) = self.sigs.get(name.as_str())
            {
                for (i, a) in args.iter().enumerate() {
                    if let Some((ParamBorrow::Ref, true)) = params.get(i)
                        && matches!(a.kind, IrExprKind::RcWrap { .. })
                    {
                        self.out.push(format!(
                            "[C5 closure-escape] {}: argument {} of `{}` is a boxed closure at a slot the callee borrows (`&dyn Fn`) — the box is an allocation the borrowed slot exists to remove",
                            self.current, i, name
                        ));
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    let mut s = Sites { sigs: &sigs, current: String::new(), out: Vec::new() };
    for f in &program.functions {
        s.current = f.name.to_string();
        s.visit_expr(&f.body);
    }
    s.out
}

/// The violations in one function. `records` is the record-type set the
/// borrow pass admits (`is_borrow_eligible`): C4 judges a verdict only over
/// the types the pass can borrow at all — an enum, tuple or matrix param has
/// no borrowed form yet, so its `Own` is the only mode, not a wrong one.
pub fn certify_fn(f: &IrFunction, vars: &VarTable, ann: &CodegenAnnotations, records: &HashSet<String>, variants: &HashSet<String>) -> Vec<String> {
    let sites = UseSites::of_expr(&f.body, Site::Result, &ExplicitBorrows);
    let uses: Vec<&Use> = sites.iter().collect();
    let mut out: Vec<String> = f.params.iter().filter_map(|p| certify_param(f, p, &uses, &sites, ann, records, variants)).collect();
    out.extend(certify_last_use_clones(f, vars, ann, &uses));
    out
}

/// C1 / C4 for one param: `None` when its verdict holds.
fn certify_param(f: &IrFunction, p: &IrParam, uses: &[&Use], sites: &UseSites, ann: &CodegenAnnotations, records: &HashSet<String>, variants: &HashSet<String>) -> Option<String> {
    let mine: Vec<&Use> = uses.iter().copied().filter(|u| u.var == p.var).collect();
    // C5, the body half (#2288): a fn-typed param rendered `&dyn Fn` whose
    // callable nevertheless escapes the call — the same predicate the
    // verdict applied (`fn_param_escapes`), re-read on the final IR. Judged
    // before the heap guard: a callable is not a heap value to C1/C4.
    if p.borrow == ParamBorrow::Ref && matches!(p.ty, Ty::Fn { .. }) {
        let u = mine.iter().find(|u| crate::pass_borrow_inference::fn_param_escapes(u))?;
        return Some(format!(
            "[C5 closure-escape] {}: param `{}` is rendered `&dyn Fn` but its callable escapes at a {:?} position (depth {}) — the verdict is wrong for this body",
            f.name, p.name, u.site, u.depth
        ));
    }
    if !heap(&p.ty) || p.open_record.is_some() {
        return None;
    }
    // A variant param whose matches only read their payloads is matched by
    // reference (the borrow pass's rule, `scrutinee_binders_borrow_only`):
    // its subject position justifies nothing.
    let subject_reads = crate::pass_borrow_inference::is_named_in(&p.ty, variants)
        && crate::pass_borrow_inference::scrutinee_binders_borrow_only(&f.body, p.var, sites);
    match p.borrow {
        ParamBorrow::Ref | ParamBorrow::RefStr | ParamBorrow::RefSlice => {
            // A reference handed bare to a call slot, bound to a local, or
            // iterated is the reference itself passing through (`g(v)`,
            // `let w = v`, `for x in v.items` with `v: &T`), not a move of
            // the value — those positions C1 does not count for a param
            // that IS a reference; what remains is returning it, building
            // it into a value, or concatenating it.
            let u = mine.iter().find(|u| definitely_consumes(u) && !reference_passes_through(u))?;
            Some(format!(
                "[C1 borrowed-then-consumed] {}: param `{}: {:?}` is rendered {:?} but is consumed at a {:?} position (chain {:?}) — BorrowInsertion's verdict is wrong for this body",
                f.name, p.name, p.ty, p.borrow, u.site, u.chain
            ))
        }
        ParamBorrow::Own if owned_verdict_is_checkable(f, p, ann) && (crate::pass_borrow_inference::is_borrow_eligible(&p.ty, records) || crate::pass_borrow_inference::is_named_in(&p.ty, variants)) => {
            // A param whose every occurrence is a `Clone` (a capture
            // materialised per closure) costs the same owned or borrowed —
            // each clone would be a `to_owned()` — so its ownership is not
            // a defect.
            let only_cloned = !mine.is_empty() && mine.iter().all(|u| u.site == Site::Clone && u.depth == 0);
            if only_cloned || mine.iter().any(|u| justifies_ownership(u) && !(subject_reads && u.site == Site::Scrutinee)) {
                return None;
            }
            Some(format!(
                "[C4 owned-never-consumed] {}: param `{}: {:?}` is rendered owned but no occurrence moves, mutates, captures or hands it on ({} occurrence(s): {}) — every caller pays a clone the body never uses",
                f.name, p.name, p.ty, mine.len(),
                mine.iter().map(|u| format!("{:?}", u.site)).collect::<Vec<_>>().join(", ")
            ))
        }
        _ => None,
    }
}

/// C3 over the body: every `Clone` of an owned local or owned param that is
/// the variable's last occurrence.
fn certify_last_use_clones(f: &IrFunction, vars: &VarTable, ann: &CodegenAnnotations, uses: &[&Use]) -> Vec<String> {
    let owned_params: HashSet<VarId> = f.params.iter().filter(|p| p.borrow == ParamBorrow::Own && !p.is_mut).map(|p| p.var).collect();
    let let_bound = let_bound_by_value(&f.body);
    let mut out = Vec::new();
    for (i, u) in uses.iter().enumerate() {
        if u.site != Site::Clone || u.depth > 0 || u.in_loop || u.in_chain || u.chain.is_some() {
            continue;
        }
        let v = u.var;
        let candidate = owned_params.contains(&v) || let_bound.contains(&v);
        if !candidate || clone_is_special(v, ann) || is_closure_value(&vars.get(v).ty) {
            continue;
        }
        // Another occurrence in the SAME outermost statement that holds a
        // borrow of the var while this clone's consumer runs — a `&v`
        // argument, a place read (`v.f`, `v[i]`), a bare interpolation part
        // `format_args!` borrows — makes the clone necessary (the E0505 the
        // clone pass's guards exist for, #809 / #866 / #1829). The OUTERMOST
        // statement, not the innermost: a capture-clone binding sits in a
        // block of its own in front of the closure, inside the interpolation
        // that holds the borrow (`"${k} ${zip_with(.., (x, y) => k)}"`).
        let borrowed_in_stmt = uses.iter().any(|w| w.var == v && w.top_stmt == u.top_stmt && !std::ptr::eq(*w, *u) && matches!(
            w.site,
            Site::Borrow { .. } | Site::Arg(SlotMode::Borrow | SlotMode::Mut) | Site::Member | Site::TupleIndex
                | Site::Index | Site::MapKeyed | Site::Deref | Site::Receiver | Site::Construct(Ctor::Interp)
        ));
        if borrowed_in_stmt {
            continue;
        }
        if !uses[i + 1..].iter().any(|w| w.var == v) {
            out.push(format!(
                "[C3 clone-at-last-use] {}: `{}: {:?}` is cloned at its last occurrence — the value was owned and never used again, so the clone copies for nothing (CloneInsertion)",
                f.name, vars.get(v).name, vars.get(v).ty
            ));
        }
    }
    out
}

/// A position that MOVES the value out of the variable, read at closure
/// depth 0 and not under a `Clone` (a cloned operand records `Site::Clone`).
fn definitely_consumes(u: &Use) -> bool {
    if u.depth > 0 {
        return false;
    }
    // An interpolation part is formatted through `format_args!`, which
    // borrows it for the call: not a move (the clone pass keeps a bare
    // `String` part bare for the same reason).
    let moving = |s: &Site| matches!(
        s,
        Site::Result | Site::Concat | Site::Arg(SlotMode::Consume)
            | Site::Callback | Site::Iterable { consumed: true } | Site::FoldInit | Site::Assigned
    ) || matches!(s, Site::Construct(c) if *c != Ctor::Interp);
    match u.chain {
        // `p.field` moved out of a borrowed `p` is the same defect (E0507).
        Some(c) => c.heap && moving(&c.top),
        None => moving(&u.site),
    }
}

/// The moving positions a REFERENCE flows through unchanged: a bare call
/// argument, a bind, an iterable (the renderer iterates `&v.items`), and a
/// projection chain ending in one of those.
fn reference_passes_through(u: &Use) -> bool {
    let through = |s: &Site| matches!(s, Site::Arg(SlotMode::Consume) | Site::Assigned | Site::Iterable { .. });
    match u.chain {
        Some(c) => through(&c.top),
        None => through(&u.site),
    }
}

/// A position that NEEDS the variable owned, or whose need this file cannot
/// decide and therefore grants: any capture, any write, any `&mut` reach, a
/// receiver, a scrutinee, a computed callee, an iteration whose body
/// consumes the elements, a `Mut` slot. An iteration whose body only READS
/// its elements (`element_reads_only`, the walk's `Iterable::consumed`)
/// justifies nothing: the source iterates from a borrow (#2287).
fn justifies_ownership(u: &Use) -> bool {
    definitely_consumes(u)
        || u.depth > 0
        || u.in_mut
        || u.is_write(true)
        || matches!(
            u.site,
            Site::Scrutinee | Site::Receiver | Site::Callee | Site::Iterable { consumed: true }
                | Site::Arg(SlotMode::Mut) | Site::Borrow { mutable: true } | Site::Construct(Ctor::Interp)
        )
        // A heap field moved straight off the param into a record literal:
        // `CloneInsertion` moves it out of an owned final-use record
        // (`pass_clone_record_fields`) — the borrow pass owns for it too.
        || matches!(u.chain, Some(c) if c.heap && matches!(c.top, Site::Scrutinee | Site::Receiver | Site::Callee | Site::Borrow { mutable: true } | Site::Construct(Ctor::Record)))
}

/// Is an `Own` verdict on `p` one this file can judge? Entry points, tests,
/// attributed / extern / exported fns, `mut` params, defaulted params and
/// the TCO-owned accumulators all own for reasons outside the body.
fn owned_verdict_is_checkable(f: &IrFunction, p: &IrParam, ann: &CodegenAnnotations) -> bool {
    !p.is_mut
        && p.default.is_none()
        && p.attrs.is_empty()
        && f.attrs.is_empty()
        && f.extern_attrs.is_empty()
        && f.export_attrs.is_empty()
        && !f.is_test
        && f.name.as_str() != "main"
        && !ann.tco_owned_params.contains(&p.var)
}

/// A variable whose `Clone` the passes place for a reason this file does not
/// model: shared cells, COW locals, always-clone vars, loop binders, counting
/// vars, TCO accumulators, module globals.
fn clone_is_special(v: VarId, ann: &CodegenAnnotations) -> bool {
    ann.shared_mut_vars.contains(&v)
        || ann.needs_cow.contains(&v)
        || ann.always_clone_vars.contains(&v)
        || ann.borrowed_loop_vars.contains(&v)
        || ann.consumed_loop_vars.contains(&v)
        || ann.range_counting_vars.contains(&v)
        || ann.tco_owned_params.contains(&v)
        || ann.globals.contains_key(&v)
        || ann.global_alias.contains_key(&v)
        || ann.is_rc_cow(&v)
}


/// The variables `let` / `var`-bound BY VALUE in `body` — not a pattern
/// binder (which may bind a reference into a borrowed scrutinee) and not a
/// bind whose value is itself a `Borrow`.
fn let_bound_by_value(body: &IrExpr) -> HashSet<VarId> {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
    struct Binds(HashSet<VarId>);
    impl IrVisitor for Binds {
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, value, .. } = &s.kind
                && !matches!(value.kind, IrExprKind::Borrow { .. })
            {
                self.0.insert(*var);
            }
            walk_stmt(self, s);
        }
        fn visit_expr(&mut self, e: &IrExpr) { walk_expr(self, e); }
    }
    let mut b = Binds(HashSet::new());
    b.visit_expr(body);
    b.0
}

/// A closure value is an `Rc<dyn Fn>` handle on the native leg and
/// `CloneInsertion` clones it at every consuming use by declared convention
/// (`split_clone_ids`: `Ty::Fn` is always-clone). Its clone is a refcount
/// increment, not an allocation — outside what C3 exists to catch — so it
/// is not judged here.
fn is_closure_value(ty: &almide_lang::types::Ty) -> bool {
    matches!(ty, almide_lang::types::Ty::Fn { .. })
}

fn heap(ty: &almide_lang::types::Ty) -> bool {
    !almide_ir::top_let_storage::clone_free(ty) && !matches!(ty, almide_lang::types::Ty::Fn { .. })
}
