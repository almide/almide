// #1108 Phase 2b-iii (ADR-0009 D2), Cell 1: plain-slot fallibility transparency
// for USER HOFs, by GENERATED twin.
//
// The core list HOFs solved D2 by hand: `normalize_fallible_hof_callback`
// renames `list.map` → `list.__fallible_map` pre-inference and the twin — an
// ordinary Result-returning stdlib fn — makes every downstream stage (typing,
// lowering, both backends) ordinary. A user HOF has no hand-written twin, so
// this pass GENERATES it, at the same pre-inference point, as ordinary AST:
//
//   fn apply1(cb: (String) -> Int, s: String) -> Int = cb(s)
//     ⇒ fn __fallible__apply1(cb: (String) -> Int!, s: String) -> Int! = cb(s)!
//
// — the callback slot becomes the EXPLICIT fallible form (`(A) -> B!`, the
// proven ADR-0009 D3 path: L7/L8), the declared return gains the same marker,
// and every direct `cb(...)` call in the body gains `!`. The call site is then
// renamed to the twin, and the caller consumes the Result explicitly
// (`!` / `??` / match) — the same discipline as the list twins (ADR-0006:
// propagation is spelled, never implicit).
//
// CELL 1 SCOPE (everything outside keeps today's E005 + Phase 2b-iii hint):
// - the callee is a same-program, non-effect, non-extern fn whose declared
//   return is not already fallible;
// - it has EXACTLY ONE fn-typed parameter, and that slot is BARE (its ret
//   carries no `!`);
// - in the HOF body the callback is used ONLY as a direct call's callee
//   (forwarding it to another fn, storing it, returning it — out of scope);
// - the call site passes the callback POSITIONALLY with a fallible lambda
//   (a body containing `!`) or a named fn declared `-> T!`.
// - test blocks keep their pre-#1108 semantics wholesale (the L9 rule): both
//   detection and rewriting only walk `Decl::Fn` bodies, never `Decl::Test`.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use almide_base::intern::{sym, Sym};
use almide_lang::ast::{self, Decl, Expr, ExprId, ExprKind, TypeExpr, Visibility};

/// The twin's name for a user HOF. Double separator so a user fn named
/// `fallible_x` can never collide with the twin of a fn named `x`.
fn twin_name(hof: Sym) -> Sym {
    sym(&format!("__fallible__{}", hof.as_str()))
}

/// A Cell-1-admissible HOF: the slot to retype plus WHERE its decl lives.
/// Copy-only on purpose (#1231): the check/detect phases borrow the decl in
/// place, and twin synthesis re-reads `program.decls[decl_idx]` so a body is
/// cloned exactly once, and only for a HOF that turned out to be needed.
struct HofCandidate {
    param_idx: usize,
    cb_name: Sym,
    decl_idx: usize,
}

/// Returns the number of twin decls appended to `program.decls` (all at the
/// tail), so the caller can register their signatures.
pub fn normalize_fallible_user_hofs(
    program: &mut ast::Program,
    fallible_marker_fns: &HashSet<Sym>,
) -> usize {
    let candidates = collect_candidates(program);
    if candidates.is_empty() {
        return 0;
    }
    // Detection: which candidate HOFs are called with a fallible callback in
    // the bare slot (outside test blocks)? Pure observation — read-only walk.
    let mut needed: BTreeSet<Sym> = BTreeSet::new();
    for decl in program.decls.iter() {
        let Decl::Fn { body: Some(body), .. } = decl else { continue };
        ast::visit_expr(body, &mut |e| {
            if let Some(name) = fallible_slot_call(e, &candidates, fallible_marker_fns) {
                needed.insert(name);
            }
        });
    }
    if needed.is_empty() {
        return 0;
    }
    // Synthesis: one twin per needed HOF, with fresh ExprIds above the
    // program's maximum — a cloned body sharing ids would alias the checker's
    // ExprId-keyed type map between original and twin. This is the ONE place a
    // HOF body is cloned (#1231), and only for the `needed` subset.
    let mut next_id = max_expr_id(program) + 1;
    let mut twins: Vec<Decl> = Vec::new();
    for hof in &needed {
        let c = &candidates[hof];
        let Decl::Fn { effect, generics, params, return_type, body: Some(body), span, .. } =
            &program.decls[c.decl_idx]
        else {
            continue;
        };
        let mut twin_params = params.clone();
        let TypeExpr::Fn { params: fp, ret, is_effect } = twin_params[c.param_idx].ty.clone() else {
            continue;
        };
        twin_params[c.param_idx].ty = TypeExpr::Fn {
            params: fp,
            ret: Box::new(TypeExpr::Generic { name: sym("!"), args: vec![*ret] }),
            is_effect,
        };
        let mut twin_body = body.clone();
        renumber_expr_ids(&mut twin_body, &mut next_id);
        unwrap_direct_cb_calls(&mut twin_body, c.cb_name, &mut next_id);
        twins.push(Decl::Fn {
            name: twin_name(*hof),
            effect: *effect,
            visibility: Visibility::Local,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            generics: generics.clone(),
            params: twin_params,
            return_type: TypeExpr::Generic { name: sym("!"), args: vec![return_type.clone()] },
            body: Some(twin_body),
            span: span.clone(),
        });
    }
    // Rewrite: rename each fallible-callback call site to its twin (test
    // blocks untouched).
    for decl in program.decls.iter_mut() {
        let Decl::Fn { body: Some(body), .. } = decl else { continue };
        ast::visit_expr_mut(body, &mut |e| {
            let Some(name) = fallible_slot_call(e, &candidates, fallible_marker_fns) else {
                return;
            };
            if !needed.contains(&name) {
                return;
            }
            let ExprKind::Call { callee, .. } = &mut e.kind else { unreachable!() };
            let ExprKind::Ident { name: callee_name } = &mut callee.kind else { unreachable!() };
            *callee_name = twin_name(name);
        });
    }
    let n = twins.len();
    program.decls.extend(twins);
    n
}

/// Every Cell-1-admissible HOF in the program, keyed by name. Admission is
/// checked by borrowing the decl in place — nothing is cloned here (#1231).
fn collect_candidates(program: &ast::Program) -> BTreeMap<Sym, HofCandidate> {
    let mut out = BTreeMap::new();
    for (decl_idx, decl) in program.decls.iter().enumerate() {
        let Decl::Fn { name, effect, extern_attrs, params, return_type, body: Some(body), .. } =
            decl
        else {
            continue;
        };
        if effect.unwrap_or(false) || !extern_attrs.is_empty() {
            continue;
        }
        // A fn already declared fallible has its own `!` semantics — retyping
        // its return would double-wrap.
        if matches!(return_type, TypeExpr::Generic { name: g, .. } if g.as_str() == "!") {
            continue;
        }
        let fn_slots: Vec<usize> = params
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(&p.ty, TypeExpr::Fn { .. }))
            .map(|(i, _)| i)
            .collect();
        let [param_idx] = fn_slots[..] else { continue };
        let TypeExpr::Fn { ret, .. } = &params[param_idx].ty else { continue };
        // The slot must be BARE — an explicit `(A) -> B!` slot is the D3 path
        // and already works.
        if matches!(&**ret, TypeExpr::Generic { name: g, .. } if g.as_str() == "!") {
            continue;
        }
        let cb_name = params[param_idx].name;
        if !cb_used_only_as_callee(body, cb_name) {
            continue;
        }
        // #1183: a callback called inside a `for`/`while` body is OUT of Cell 1
        // — the twin's `cb(..)!` would sit in the in-loop expression-nested
        // position whose wasm lowering silently drops the err path (the
        // pre-existing wall-escape that issue pins). The honest E005 stays
        // until the loop-unwrap ANF capability lands.
        if cb_called_inside_loop(body, cb_name) {
            continue;
        }
        out.insert(*name, HofCandidate { param_idx, cb_name, decl_idx });
    }
    out
}

/// If `e` is a positional call of a candidate HOF whose bare slot receives a
/// fallible callback, the HOF's name.
fn fallible_slot_call(
    e: &Expr,
    candidates: &BTreeMap<Sym, HofCandidate>,
    fallible_marker_fns: &HashSet<Sym>,
) -> Option<Sym> {
    let ExprKind::Call { callee, args, named_args, .. } = &e.kind else { return None };
    if !named_args.is_empty() {
        return None;
    }
    let ExprKind::Ident { name } = &callee.kind else { return None };
    let c = candidates.get(name)?;
    let arg = args.get(c.param_idx)?;
    arg_is_fallible(arg, fallible_marker_fns).then_some(*name)
}

/// Is `e` a direct call whose callee is the bare identifier `cb`?
fn is_direct_cb_call(e: &Expr, cb: Sym) -> bool {
    matches!(
        &e.kind,
        ExprKind::Call { callee, .. }
            if matches!(&callee.kind, ExprKind::Ident { name } if *name == cb)
    )
}

/// True when every occurrence of `cb` in the body is the callee of a direct
/// call — the shape whose twin rewrite (`cb(..)` → `cb(..)!`) is total.
fn cb_used_only_as_callee(body: &Expr, cb: Sym) -> bool {
    let mut ident_uses = 0usize;
    let mut callee_uses = 0usize;
    ast::visit_expr(body, &mut |e| {
        if matches!(&e.kind, ExprKind::Ident { name } if *name == cb) {
            ident_uses += 1;
        }
        if is_direct_cb_call(e, cb) {
            callee_uses += 1;
        }
    });
    ident_uses == callee_uses
}

/// True when any direct `cb(...)` call sits inside a `for`/`while` NODE —
/// the #1183 exclusion. Over-conservative on purpose: the walk covers the
/// loop's whole subtree including the head (a head-position `!` is actually
/// the proven #1168 hoist class), because conservatism here only keeps
/// today's honest E005. One borrowed walk; each loop node scans its own
/// subtree in place (#1231 — no clone, no subtree accumulation).
fn cb_called_inside_loop(body: &Expr, cb: Sym) -> bool {
    let mut found = false;
    ast::visit_expr(body, &mut |e| {
        if found {
            return;
        }
        if matches!(&e.kind, ExprKind::ForIn { .. } | ExprKind::While { .. }) {
            ast::visit_expr(e, &mut |c| {
                if is_direct_cb_call(c, cb) {
                    found = true;
                }
            });
        }
    });
    found
}

/// Does this argument expression carry the fallibility bit? The same two
/// shapes the list normalizer admits: a lambda whose body contains an `!`, or
/// a named fn declared `-> T!`.
fn arg_is_fallible(arg: &Expr, fallible_marker_fns: &HashSet<Sym>) -> bool {
    match &arg.kind {
        ExprKind::Lambda { body, .. } => {
            let mut found = false;
            ast::visit_expr(body, &mut |e| {
                if matches!(e.kind, ExprKind::Unwrap { .. }) {
                    found = true;
                }
            });
            found
        }
        ExprKind::Ident { name } => fallible_marker_fns.contains(name),
        _ => false,
    }
}

/// In the twin body, every direct `cb(...)` call gains the explicit `!` — the
/// D3 propagation the retyped slot demands. The shared visitor is pre-order
/// and descends into MUTATED children, so a freshly wrapped call would be
/// revisited and wrapped forever; the `done` set (ids are unique here — the
/// body was renumbered just before) breaks that cycle while still descending
/// into a nested `cb(cb(x))`.
fn unwrap_direct_cb_calls(body: &mut Expr, cb: Sym, next_id: &mut u32) {
    let mut done: HashSet<ExprId> = HashSet::new();
    ast::visit_expr_mut(body, &mut |e| {
        if done.contains(&e.id) {
            return;
        }
        if !is_direct_cb_call(e, cb) {
            return;
        }
        done.insert(e.id);
        let span = e.span.clone();
        let inner = std::mem::replace(
            e,
            Expr { id: ExprId(*next_id), span: span.clone(), kind: ExprKind::Unit },
        );
        *next_id += 1;
        *e = Expr {
            id: ExprId(*next_id),
            span,
            kind: ExprKind::Unwrap { expr: Box::new(inner) },
        };
        *next_id += 1;
    });
}

/// Give every node in a cloned twin body a fresh ExprId.
fn renumber_expr_ids(body: &mut Expr, next_id: &mut u32) {
    ast::visit_expr_mut(body, &mut |e| {
        e.id = ExprId(*next_id);
        *next_id += 1;
    });
}

/// The program's maximum ExprId, so twin ids allocate above every parsed id.
fn max_expr_id(program: &ast::Program) -> u32 {
    let mut max = 0u32;
    for decl in program.decls.iter() {
        let (Decl::Fn { body: Some(b), .. } | Decl::Test { body: b, .. } | Decl::TopLet { value: b, .. }) =
            decl
        else {
            continue;
        };
        ast::visit_expr(b, &mut |e| {
            if e.id.0 > max {
                max = e.id.0;
            }
        });
    }
    max
}
