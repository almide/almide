//! Owned capture bindings shared by explicit lambdas and implicit fan closures.
use super::*;

/// Fan arms are implicit move closures. Their capture bindings must be
/// outside the entire Fan node, so every clone runs before any spawn.
pub(super) fn wrap_fan_with_clones(expr: &mut IrExpr, vt: &mut VarTable, scope: &HashSet<VarId>) -> bool {
    let IrExprKind::Fan { exprs } = &mut expr.kind else { return false };
    let mut bindings = Vec::new();
    for arm in exprs {
        let mut mutated = HashSet::new();
        collect_mutated_vars(arm, &mut mutated);
        let captures: Vec<VarId> = almide_ir::free_vars::free_vars(arm, &HashSet::new())
            .into_iter().filter(|v| scope.contains(v)
                && needs_clone_type(&vt.get(*v).ty) && !mutated.contains(v)).collect();
        let (stmts, renames) = capture_bindings(&captures, vt, &mutated, Some("__fan_cap"));
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
    vt: &mut VarTable,
    lam_mutated: &HashSet<VarId>,
    prefix: Option<&str>,
) -> (Vec<IrStmt>, std::collections::HashMap<VarId, VarId>) {
    let mut stmts = Vec::new();
    let mut renames = std::collections::HashMap::new();

    for &var_id in captures {
        let ty = vt.get(var_id).ty.clone();
        let cap_name = match prefix {
            Some(prefix) => format!("{prefix}_{}", vt.len()),
            None => format!("__cap_{}", var_id.0),
        };
        let cap_var = vt.alloc(
            almide_base::intern::sym(&cap_name),
            ty.clone(),
            Mutability::Let,
            None,
        );
        renames.insert(var_id, cap_var);

        // The clone of a shared-mut capture is itself a shared cell (`Rc<Cell>`),
        // so reads/writes of `__cap` inside the closure go through `.get()`/`.set()`
        // too. (Closure v2, P3.)
        if SHARED_MUT.with(|m| m.borrow().contains(&var_id)) {
            SHARED_MUT.with(|m| { m.borrow_mut().insert(cap_var); });
        }

        // If the captured var is a fn param with a borrowed runtime
        // representation (`&[T]` / `&str` / `&T`), the bare `Var` IR
        // renders as the borrow — but `__cap_N: Vec<T>` / `String` / `T`
        // (the Almide-level owned type) expects an owned value. Materialise
        // the owned form explicitly so the `move |..|` closure can take it.
        let borrow = PARAM_BORROWS.with(|m| m.borrow().get(&var_id).copied());
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
