//! Owned capture bindings shared by explicit lambdas and implicit fan closures.
use super::*;

/// Fan arms are implicit move closures. Their capture bindings must be
/// outside the entire Fan node, so every clone runs before any spawn.
pub(super) fn wrap_fan_with_clones(expr: &mut IrExpr, cx: &mut Cx, scope: &HashSet<VarId>) -> bool {
    let IrExprKind::Fan { exprs } = &mut expr.kind else { return false };
    let mut bindings = Vec::new();
    for arm in exprs {
        let mutated = written_vars(arm);
        let captures: Vec<VarId> = almide_ir::free_vars::free_vars(arm, &HashSet::new())
            .into_iter().filter(|v| scope.contains(v)
                && needs_clone_type(&cx.vt.get(*v).ty) && !mutated.contains(v)).collect();
        let (stmts, renames) = capture_bindings(&captures, cx, &mutated, Some("__fan_cap"), &std::collections::HashSet::new());
        replace_vars(arm, &renames);
        bindings.extend(stmts);
    }
    if bindings.is_empty() { return false; }
    let body = std::mem::take(expr);
    *expr = IrExpr { ty: body.ty.clone(), span: body.span, def_id: body.def_id,
        kind: IrExprKind::Block { stmts: bindings, expr: Some(Box::new(body)) } };
    true
}

pub(super) fn capture_bindings(
    captures: &[VarId],
    cx: &mut Cx,
    lam_mutated: &HashSet<VarId>,
    prefix: Option<&str>,
    move_now: &HashSet<VarId>,
) -> (Vec<IrStmt>, std::collections::HashMap<VarId, VarId>) {
    let mut stmts = Vec::new();
    let mut renames = std::collections::HashMap::new();

    for &var_id in captures {
        let ty = cx.vt.get(var_id).ty.clone();
        let cap_name = match prefix {
            Some(prefix) => format!("{prefix}_{}", cx.vt.len()),
            None => format!("__cap_{}", var_id.0),
        };
        let cap_var = cx.vt.alloc(
            almide_base::intern::sym(&cap_name),
            ty.clone(),
            Mutability::Let,
            None,
        );
        renames.insert(var_id, cap_var);

        // The clone of a shared-mut capture is itself a shared cell (`Rc<Cell>`),
        // so reads/writes of `__cap` inside the closure go through `.get()`/`.set()`
        // too. (Closure v2, P3.)
        if cx.facts.shared_mut.contains(&var_id) {
            cx.facts.shared_mut.insert(cap_var);
        }

        // If the captured var is a fn param with a borrowed runtime
        // representation (`&[T]` / `&str` / `&T`), the bare `Var` IR
        // renders as the borrow — but `__cap_N: Vec<T>` / `String` / `T`
        // (the Almide-level owned type) expects an owned value. Materialise
        // the owned form explicitly so the `move |..|` closure can take it.
        let borrow = cx.facts.param_borrows.get(&var_id).copied();
        let bind_value = match borrow {
            Some(ParamBorrow::RefSlice) => IrExpr {
                kind: IrExprKind::ToVec {
                    expr: Box::new(IrExpr { kind: IrExprKind::Var { id: var_id }, ty: ty.clone(), span: None, def_id: None }),
                },
                ty: ty.clone(), span: None, def_id: None,
            },
            Some(ParamBorrow::RefStr) => IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Method {
                        object: Box::new(IrExpr { kind: IrExprKind::Var { id: var_id }, ty: ty.clone(), span: None, def_id: None }),
                        // Use `to_owned` instead of `to_string` to avoid
                        // StdlibLowering converting this into a module call
                        // (e.g. `int.to_string()`) when the Almide-level type
                        // differs from the Rust-level &str representation.
                        method: almide_base::intern::sym("to_owned"),
                    },
                    args: vec![],
                    type_args: vec![],
                },
                ty: ty.clone(), span: None, def_id: None,
            },
            // The lambda is the var's sole user (#2231): the bind MOVES it.
            // Nothing else — no later statement, no sibling closure, no
            // runtime-template borrow in the same call — can name the var
            // again, so the #809 hazard below cannot arise.
            _ if move_now.contains(&var_id) => IrExpr {
                kind: IrExprKind::Var { id: var_id },
                ty: ty.clone(),
                span: None,
                def_id: None,
            },
            // The default capture bind CLONES explicitly (#809): CloneInsertion's
            // last-use analysis would MOVE the var here when this is its last
            // syntactic use — but a runtime-template borrow (`&{m}` — e.g.
            // `map.fold`'s first arg) in the SAME statement is invisible at the
            // IR level and stays live until the call, so the move was an E0505.
            // READ-ONLY captures only: a capture THIS lambda mutates keeps the
            // bare `Var` bind — the shared-cell wiring (Closure v2 P3/P6)
            // pattern-matches it, and a `Clone` wrapper severed the sharing
            // (each closure mutated its own copy — the wasm_runtime
            // closure-capture cross-target mismatches). NOTE the mutability
            // FLAG cannot gate this: a non-Copy `var` mutated only through a
            // method (`list.push`) is recorded `Mutability::Let`.
            _ if !lam_mutated.contains(&var_id) => IrExpr {
                kind: IrExprKind::Clone {
                    expr: Box::new(IrExpr {
                        kind: IrExprKind::Var { id: var_id },
                        ty: ty.clone(),
                        span: None,
                        def_id: None,
                    }),
                },
                ty: ty.clone(),
                span: None,
                def_id: None,
            },
            _ => IrExpr {
                kind: IrExprKind::Var { id: var_id },
                ty: ty.clone(),
                span: None,
                def_id: None,
            },
        };

        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: cap_var,
                mutability: Mutability::Let,
                ty: ty.clone(),
                value: bind_value,
            },
            span: None,
        });
    }

    (stmts, renames)
}

/// Per var of a fn body, the facts the capture-move rule reads: every
/// occurrence as `(index in evaluation order, outer lambda, statement)`, and
/// whether the var is CLEAN — no write, no reach through `&mut`, no
/// occurrence inside a loop. See [`capture_moves`].
#[derive(Default)]
pub(super) struct CaptureUses {
    pub(super) uses: HashMap<VarId, Vec<(usize, Option<u32>, u32, bool)>>,
    pub(super) clean: HashSet<VarId>,
}

/// The use table [`capture_moves`] decides from. `ALMIDE_CAPTURE_MOVE_OFF=1`
/// empties it (the ablation the certifier's sensitivity test drives).
pub(super) fn capture_uses(body: &IrExpr) -> CaptureUses {
    if almide_base::env::flag("ALMIDE_CAPTURE_MOVE_OFF") {
        return CaptureUses::default();
    }
    let sites = UseSites::of_expr(body, Site::Result, &ExplicitBorrows);
    let mut out = CaptureUses::default();
    let mut unclean: HashSet<VarId> = HashSet::new();
    for (i, u) in sites.iter().enumerate() {
        out.uses.entry(u.var).or_default().push((i, u.outer_lambda, u.stmt, holds_a_borrow(u)));
        if u.in_loop || u.in_mut || u.is_write(true) || matches!(u.site, Site::Borrow { mutable: true }) {
            unclean.insert(u.var);
        }
    }
    out.clean = out.uses.keys().copied().filter(|v| !unclean.contains(v)).collect();
    out
}

/// Does this occurrence hold a BORROW of the variable while the rest of its
/// statement evaluates — a `&v` / `&mut v` argument, a place read (`v.f`,
/// `v[i]`, a receiver, an operand, a by-reference iterable)? A by-value use
/// (a consumed argument, a concat operand, a constructor field, a bind) is
/// cloned or moved and holds nothing afterwards.
fn holds_a_borrow(u: &crate::use_kind::Use) -> bool {
    matches!(
        u.site,
        Site::Borrow { .. } | Site::Arg(crate::use_kind::SlotMode::Borrow | crate::use_kind::SlotMode::Mut)
            | Site::Member | Site::TupleIndex | Site::Index | Site::MapKeyed | Site::Deref
            | Site::Receiver | Site::Operand | Site::Scrutinee | Site::Iterable { consumed: false }
    ) || u.in_mut
}

/// May the capture of `var` by the lambda `lambda` MOVE the value instead of
/// cloning it? The Perceus rule (a lambda dups its free variables only while
/// they stay live): yes when the lambda holds the var's LAST occurrence, the
/// var is clean, and no occurrence outside the lambda shares a statement
/// with one inside it — a `map.fold(m, init, (k, v) => … m …)` borrows `m`
/// through the runtime template while the closure is built, and a move there
/// is the #809 E0505. Different statements cannot hold that borrow.
pub(super) fn capture_moves(table: &CaptureUses, var: VarId, lambda: Option<u32>) -> bool {
    let Some(lambda) = lambda else { return false };
    if !table.clean.contains(&var) {
        return false;
    }
    let Some(uses) = table.uses.get(&var) else { return false };
    let inside: Vec<&(usize, Option<u32>, u32, bool)> = uses.iter().filter(|(_, l, _, _)| *l == Some(lambda)).collect();
    if inside.is_empty() {
        return false;
    }
    let last_inside = inside.iter().map(|(i, _, _, _)| *i).max().unwrap_or(0);
    let last_any = uses.iter().map(|(i, _, _, _)| *i).max().unwrap_or(0);
    if last_any != last_inside {
        return false;
    }
    // Only an outside occurrence that HOLDS a borrow through the statement
    // conflicts with the move; a by-value sibling (`or_else(s1, (v) => s1)`
    // hands a clone of `s1` to the first slot) holds nothing.
    let stmts: HashSet<u32> = inside.iter().map(|(_, _, s, _)| *s).collect();
    !uses.iter().any(|(_, l, s, borrows)| *l != Some(lambda) && *borrows && stmts.contains(s))
}
