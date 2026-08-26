//! Pre-pass: collect every Bind (statement, pattern, loop var) that
//! lowering can reach, in first-bind order.

use std::collections::HashSet;

use almide_ir::{IrExpr, IrExprKind, IrPattern, IrStmt, IrStmtKind, IrStringPart, VarId};

use crate::*;

// ── pre-pass: Binds → locals ────────────────────────────────────────────

/// Collect every Bind the lowering traversal can reach, in first-bind
/// order — statement binds AND match-pattern binds. Mirrors `Emitter`'s
/// traversal: a Bind the lowering CAN reach but this pass misses would
/// surface as the honest `bind:unmapped` reason, never a bad module.
pub(crate) fn collect_binds(
    e: &IrExpr,
    out: &mut Vec<(VarId, SliceTy)>,
    seen: &mut HashSet<VarId>,
    types: &TypeTable,
) -> Result<(), EmitError> {
    match &e.kind {
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                collect_binds_stmt(s, out, seen, types)?;
            }
            if let Some(tail) = expr {
                collect_binds(tail, out, seen, types)?;
            }
            Ok(())
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_binds(cond, out, seen, types)?;
            collect_binds(then, out, seen, types)?;
            collect_binds(else_, out, seen, types)
        }
        IrExprKind::While { cond, body } => {
            collect_binds(cond, out, seen, types)?;
            for s in body {
                collect_binds_stmt(s, out, seen, types)?;
            }
            Ok(())
        }
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            collect_forin(var, var_tuple.as_deref(), iterable, body, out, seen, types)
        }
        IrExprKind::Match { subject, arms } => {
            collect_binds(subject, out, seen, types)?;
            for arm in arms {
                collect_pattern_binds(&arm.pattern, out, seen, types)?;
                if let Some(g) = &arm.guard {
                    collect_binds(g, out, seen, types)?;
                }
                collect_binds(&arm.body, out, seen, types)?;
            }
            Ok(())
        }
        _ => collect_binds_data(e, out, seen, types),
    }
}


/// Data-shaped expressions (lists, records, sums, calls, operators) —
/// split from `collect_binds` for complexity budget.
pub(crate) fn collect_binds_data(
    e: &IrExpr,
    out: &mut Vec<(VarId, SliceTy)>,
    seen: &mut HashSet<VarId>,
    types: &TypeTable,
) -> Result<(), EmitError> {
    match &e.kind {
        IrExprKind::List { elements } => {
            for el in elements {
                collect_binds(el, out, seen, types)?;
            }
            Ok(())
        }
        IrExprKind::IndexAccess { object, index } => {
            collect_binds(object, out, seen, types)?;
            collect_binds(index, out, seen, types)
        }
        IrExprKind::Range { start, end, .. } => {
            collect_binds(start, out, seen, types)?;
            collect_binds(end, out, seen, types)
        }
        IrExprKind::Record { fields, .. } => {
            for (_, fe) in fields {
                collect_binds(fe, out, seen, types)?;
            }
            Ok(())
        }
        IrExprKind::SpreadRecord { base, fields } => {
            collect_binds(base, out, seen, types)?;
            for (_, fe) in fields {
                collect_binds(fe, out, seen, types)?;
            }
            Ok(())
        }
        IrExprKind::Member { object, .. } => collect_binds(object, out, seen, types),
        IrExprKind::Tuple { elements } => {
            for el in elements {
                collect_binds(el, out, seen, types)?;
            }
            Ok(())
        }
        IrExprKind::TupleIndex { object, .. } => collect_binds(object, out, seen, types),
        // Lambda params become locals (used when the lambda is inlined as
        // a direct HOF callback; harmless extras otherwise).
        IrExprKind::Lambda { params, body, .. } => {
            for (var, ty) in params {
                let Some(sty) = slice_ty_of(ty, types) else {
                    return unsup(&format!("bind-ty:{}", ty_name(ty)));
                };
                if seen.insert(*var) {
                    out.push((*var, sty));
                }
            }
            collect_binds(body, out, seen, types)
        }
        IrExprKind::Call { args, .. } => {
            for a in args {
                collect_binds(a, out, seen, types)?;
            }
            Ok(())
        }
        IrExprKind::BinOp { left, right, .. } => {
            collect_binds(left, out, seen, types)?;
            collect_binds(right, out, seen, types)
        }
        IrExprKind::UnOp { operand, .. } => collect_binds(operand, out, seen, types),
        IrExprKind::OptionSome { expr }
        | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr }
        | IrExprKind::Unwrap { expr } => collect_binds(expr, out, seen, types),
        IrExprKind::UnwrapOr { expr, fallback } => {
            collect_binds(expr, out, seen, types)?;
            collect_binds(fallback, out, seen, types)
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p {
                    collect_binds(expr, out, seen, types)?;
                }
            }
            Ok(())
        }        _ => Ok(()), // leaves with no binds beneath them
    }
}

pub(crate) fn collect_binds_stmt(
    s: &IrStmt,
    out: &mut Vec<(VarId, SliceTy)>,
    seen: &mut HashSet<VarId>,
    types: &TypeTable,
) -> Result<(), EmitError> {
    match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. } => {
            let Some(sty) = slice_ty_of(ty, types) else {
                return unsup(&format!("bind-ty:{}", ty_name(ty)));
            };
            if seen.insert(*var) {
                out.push((*var, sty));
            }
            collect_binds(value, out, seen, types)
        }
        IrStmtKind::Assign { value, .. } => collect_binds(value, out, seen, types),
        IrStmtKind::BindDestructure { pattern, value } => {
            collect_pattern_binds(pattern, out, seen, types)?;
            collect_binds(value, out, seen, types)
        }
        IrStmtKind::Expr { expr } => collect_binds(expr, out, seen, types),
        _ => Ok(()), // lowering unsups these before any local is needed
    }
}

pub(crate) fn collect_pattern_binds(
    p: &IrPattern,
    out: &mut Vec<(VarId, SliceTy)>,
    seen: &mut HashSet<VarId>,
    types: &TypeTable,
) -> Result<(), EmitError> {
    match p {
        IrPattern::Bind { var, ty } => {
            let Some(sty) = slice_ty_of(ty, types) else {
                return unsup(&format!("bind-ty:{}", ty_name(ty)));
            };
            if seen.insert(*var) {
                out.push((*var, sty));
            }
            Ok(())
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            collect_pattern_binds(inner, out, seen, types)
        }
        IrPattern::RecordPattern { fields, .. } => {
            for fp in fields {
                if let Some(p) = &fp.pattern {
                    collect_pattern_binds(p, out, seen, types)?;
                }
            }
            Ok(())
        }
        IrPattern::Constructor { args, .. } | IrPattern::Tuple { elements: args } => {
            for a in args {
                collect_pattern_binds(a, out, seen, types)?;
            }
            Ok(())
        }
        _ => Ok(()), // lowering unsups unsupported pattern shapes first
    }
}

/// The for-in bind collection (loop var + tuple destructure + the map
/// entry-walk locals) — split from collect_binds for the complexity
/// budget.
fn collect_forin(
    var: &VarId,
    var_tuple: Option<&[VarId]>,
    iterable: &IrExpr,
    body: &[IrStmt],
    out: &mut Vec<(VarId, SliceTy)>,
    seen: &mut HashSet<VarId>,
    types: &TypeTable,
) -> Result<(), EmitError> {
            // The loop variable is a local; its type comes from the
            // iterable's checker annotation (Range iterates Int).
            let var_ty = if matches!(iterable.kind, IrExprKind::Range { .. }) {
                Some(INT)
            } else {
                match slice_ty_of(&iterable.ty, types) {
                    Some(SliceTy::List(h)) => Some(types.el(h)),
                    // `for (k, v) in map`: the destructured locals carry
                    // the key/value types; the loop var itself is never
                    // materialized (the entry walk loads directly).
                    Some(SliceTy::Map(kh, vh)) => {
                        let Some(&[tk, tv]) = var_tuple else {
                            return unsup("forin-map-nontuple");
                        };
                        // the loop var itself: a (K, V) tuple slot the
                        // entry walk never materializes, mapped so the
                        // emitter's var lookup holds
                        let (kt, vt) = (types.el(kh), types.el(vh));
                        if seen.insert(*var) {
                            let ti = types.tuple(vec![kt, vt]);
                            out.push((*var, SliceTy::Tuple(ti)));
                        }
                        if seen.insert(tk) {
                            out.push((tk, kt));
                        }
                        if seen.insert(tv) {
                            out.push((tv, vt));
                        }
                        collect_binds(iterable, out, seen, types)?;
                        for s in body {
                            collect_binds_stmt(s, out, seen, types)?;
                        }
                        return Ok(());
                    }
                    _ => None,
                }
            };
            let Some(var_ty) = var_ty else {
                return unsup(&format!("forin-iter-ty:{}", ty_name(&iterable.ty)));
            };
            if seen.insert(*var) {
                out.push((*var, var_ty));
            }
            if let (Some(tvars), SliceTy::Tuple(ti)) = (var_tuple, var_ty) {
                let def = types.tuple_def(ti);
                for (tv, (fty, _)) in tvars.iter().zip(def.fields) {
                    if seen.insert(*tv) {
                        out.push((*tv, fty));
                    }
                }
            }
            collect_binds(iterable, out, seen, types)?;
            for s in body {
                collect_binds_stmt(s, out, seen, types)?;
            }
            Ok(())
}
