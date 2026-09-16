//! Owned capture bindings shared by explicit lambdas and implicit fan closures.
use super::*;

/// Fan arms are implicit move closures. Their capture bindings must be
/// outside the entire Fan node, so every clone runs before any spawn.
pub(super) fn wrap_fan_with_clones(expr: &mut IrExpr, cx: &mut Cx, scope: &HashSet<VarId>) -> bool {
    let fan_span = expr.span;
    let IrExprKind::Fan { exprs } = &mut expr.kind else { return false };
    let mut bindings = Vec::new();
    for (i, arm) in exprs.iter_mut().enumerate() {
        let mutated = written_vars(arm);
        let captures: Vec<VarId> = almide_ir::free_vars::free_vars(arm, &HashSet::new())
            .into_iter().filter(|v| scope.contains(v)
                && needs_clone_type(&cx.vt.get(*v).ty) && !mutated.contains(v)).collect();
        // A capture whose LAST occurrence this arm holds MOVES instead of
        // cloning (#2239) — the lambda rule (`capture_moves`) keyed on the
        // arm's identity from the use walk. The bindings are hoisted before
        // the fan in arm order, so an earlier arm's clone precedes a later
        // arm's move. A shared cell (`shared_mut`) never moves: every arm
        // must see the one cell.
        let move_now: HashSet<VarId> = match fan_span {
            Some(span) => captures.iter().copied()
                .filter(|v| !cx.facts.shared_mut.contains(v))
                .filter(|v| fan_capture_moves(&cx.facts.capture_uses, *v, (span, i as u32)))
                .collect(),
            None => HashSet::new(),
        };
        let (stmts, renames) = capture_bindings(&captures, cx, &mutated, Some("__fan_cap"), &move_now);
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

/// One occurrence of a var as the capture-move rule sees it: its index in
/// evaluation order, the outermost lambda holding it, whether a node
/// enclosing that closure holds a borrow across its construction, and the
/// outermost fan arm holding it (#2239).
#[derive(Clone, Copy, Debug)]
pub(super) struct Occurrence {
    pub(super) index: usize,
    pub(super) lambda: Option<u32>,
    pub(super) held: bool,
    pub(super) fan_arm: Option<(almide_base::span::Span, u32)>,
}

/// Per var of a fn body, the facts the capture-move rule reads: every
/// [`Occurrence`], and whether the var is CLEAN — no write, no reach
/// through `&mut`, no occurrence inside a loop. See [`capture_moves`].
#[derive(Default)]
pub(super) struct CaptureUses {
    pub(super) uses: HashMap<VarId, Vec<Occurrence>>,
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
        out.uses.entry(u.var).or_default().push(Occurrence {
            index: i, lambda: u.outer_lambda, held: u.held_across, fan_arm: u.fan_arm,
        });
        if u.in_loop || u.in_mut || u.is_write(true) || matches!(u.site, Site::Borrow { mutable: true }) {
            unclean.insert(u.var);
        }
    }
    out.clean = out.uses.keys().copied().filter(|v| !unclean.contains(v)).collect();
    out
}

/// May the capture of `var` by the lambda `lambda` MOVE the value instead of
/// cloning it? The Perceus rule (a lambda dups its free variables only while
/// they stay live): yes when the lambda holds the var's LAST occurrence, the
/// var is clean, and no node enclosing the lambda holds a borrow of the var
/// across the closure's construction — `map.fold(m, init, (k, v) => … m …)`
/// borrows `m` through the runtime template while the closure is built, and
/// a move there is the #809 E0505. A borrow that a SIBLING subexpression
/// took and released (`fs.list_dir(dir) ?? [] |> list.map((n) => dir + n)`)
/// is over by then, so the closure moves `dir`.
pub(super) fn capture_moves(table: &CaptureUses, var: VarId, lambda: Option<u32>) -> bool {
    let Some(lambda) = lambda else { return false };
    holds_last_occurrence(table, var, |o| o.lambda == Some(lambda))
}

/// The same rule for a `fan` arm (#2239): may the arm identified by `arm`
/// (the fan node's span and the arm's index, as the use walk numbers them)
/// MOVE `var` into its `__fan_cap_*` binding? Yes when the arm holds the
/// var's LAST occurrence, the var is clean, and no node enclosing the fan
/// holds a borrow of it. Sibling arms all run, so only the arm with the last
/// occurrence — the last arm in evaluation order that names the var — can
/// qualify; the earlier arms keep their clones.
pub(super) fn fan_capture_moves(table: &CaptureUses, var: VarId, arm: (almide_base::span::Span, u32)) -> bool {
    holds_last_occurrence(table, var, |o| o.fan_arm == Some(arm))
}

/// Does the closure selected by `inside` hold the LAST occurrence of a clean
/// `var`, with none of its occurrences held across by an enclosing borrow?
fn holds_last_occurrence(table: &CaptureUses, var: VarId, inside: impl Fn(&Occurrence) -> bool) -> bool {
    if !table.clean.contains(&var) {
        return false;
    }
    let Some(uses) = table.uses.get(&var) else { return false };
    let inside: Vec<&Occurrence> = uses.iter().filter(|o| inside(o)).collect();
    if inside.is_empty() {
        return false;
    }
    let last_inside = inside.iter().map(|o| o.index).max().unwrap_or(0);
    let last_any = uses.iter().map(|o| o.index).max().unwrap_or(0);
    last_any == last_inside && !inside.iter().any(|o| o.held)
}
