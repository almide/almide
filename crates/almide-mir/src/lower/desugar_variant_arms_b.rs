// The user-variant / record pattern-matrix compiler behind `desugar_variant_guard_match`
// (#2473, #2559). include!-spliced from desugar_match_b.rs, sharing the lower module's
// imports.
//
// A match over a custom variant (or a plain record) whose arms carry GUARDS or refutable
// payload patterns — a literal field (`Square(0)`, `Circle { r: 0, .. }`), a nested
// constructor (`Tagged { kind: Big(n), .. }`), a Bool field — is compiled Maranget-style
// into the shapes every executed variant-match route already admits: one arm per
// constructor head binding its fields to FRESH vars, and inside each arm a match / if
// over those vars. The subject is column 0 of the matrix; each constructor head replaces
// its column by its fields' columns; a scalar column dispatches on its literals; a Bool
// column becomes an `if`; a plain-record column becomes field projections. First-match
// order is preserved inside every branch. A row that joins more than one branch is
// duplicated, gated by [`introduces_binder`] (VarId uniqueness under duplication).

use almide_ir::{IrMatchArm, IrPattern};

/// One source arm as a matrix row: `pats[c]` tests column `c`, `idx` is the source arm.
#[derive(Clone)]
struct VaRow {
    pats: Vec<IrPattern>,
    guard: Option<IrExpr>,
    body: IrExpr,
    idx: usize,
}

/// A literal a scalar column can dispatch on.
#[derive(Clone, PartialEq)]
enum VaLit {
    Int(i64),
    Str(String),
    Bool(bool),
}

/// A constructor head in a column.
#[derive(Clone, PartialEq, Eq)]
enum VaKey {
    User(String),
    Some_,
    None_,
    Ok_,
    Err_,
}

fn va_trivial(p: &IrPattern) -> bool {
    matches!(p, IrPattern::Wildcard | IrPattern::Bind { .. })
}

fn va_lit(p: &IrPattern) -> Option<VaLit> {
    let IrPattern::Literal { expr } = p else { return None };
    match &expr.kind {
        IrExprKind::LitInt { value } => Some(VaLit::Int(*value)),
        IrExprKind::LitStr { value } => Some(VaLit::Str(value.clone())),
        IrExprKind::LitBool { value } => Some(VaLit::Bool(*value)),
        _ => None,
    }
}

/// The head of a constructor-shaped pattern and whether it is spelled in record form.
fn va_head(p: &IrPattern) -> Option<(VaKey, bool)> {
    match p {
        IrPattern::Constructor { name, .. } => Some((VaKey::User(name.clone()), false)),
        IrPattern::RecordPattern { name, .. } => Some((VaKey::User(name.clone()), true)),
        IrPattern::Some { .. } => Some((VaKey::Some_, false)),
        IrPattern::None => Some((VaKey::None_, false)),
        IrPattern::Ok { .. } => Some((VaKey::Ok_, false)),
        IrPattern::Err { .. } => Some((VaKey::Err_, false)),
        _ => None,
    }
}

/// The sub-patterns of a head pattern in `names` order (a record form fills the fields
/// it does not name with `_`); `None` when the pattern does not fit the head's shape.
fn va_head_args(p: &IrPattern, names: &[String]) -> Option<Vec<IrPattern>> {
    match p {
        IrPattern::Constructor { args, .. } => (args.len() == names.len()).then(|| args.clone()),
        IrPattern::RecordPattern { fields, .. } => {
            if fields.iter().any(|f| !names.iter().any(|n| *n == f.name)) {
                return None;
            }
            Some(
                names
                    .iter()
                    .map(|n| {
                        fields
                            .iter()
                            .find(|f| f.name == *n)
                            .and_then(|f| f.pattern.clone())
                            .unwrap_or(IrPattern::Wildcard)
                    })
                    .collect(),
            )
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            (names.len() == 1).then(|| vec![(**inner).clone()])
        }
        IrPattern::None => names.is_empty().then(Vec::new),
        _ => None,
    }
}

/// Can the compiler dispatch on this pattern (at any depth)?
fn va_pat_ok(p: &IrPattern) -> bool {
    match p {
        IrPattern::Wildcard | IrPattern::Bind { .. } | IrPattern::None => true,
        IrPattern::Literal { .. } => va_lit(p).is_some(),
        IrPattern::Constructor { args, .. } => args.iter().all(va_pat_ok),
        IrPattern::RecordPattern { fields, .. } => {
            fields.iter().all(|f| f.pattern.as_ref().is_none_or(va_pat_ok))
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => va_pat_ok(inner),
        IrPattern::Tuple { .. } | IrPattern::List { .. } | IrPattern::As { .. } => false,
    }
}

/// Does a constructor-shaped pattern test anything below its head?
fn va_refutable_inside(p: &IrPattern) -> bool {
    match p {
        IrPattern::Constructor { args, .. } => args.iter().any(|a| !va_trivial(a)),
        IrPattern::RecordPattern { fields, .. } => {
            fields.iter().any(|f| f.pattern.as_ref().is_some_and(|fp| !va_trivial(fp)))
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => !va_trivial(inner),
        _ => false,
    }
}

fn va_var(v: VarId, ty: &Ty, tmpl: &IrExpr) -> IrExpr {
    IrExpr { kind: IrExprKind::Var { id: v }, ty: ty.clone(), span: tmpl.span, def_id: None }
}

fn va_fresh(next: &mut u32) -> VarId {
    let v = VarId(*next);
    *next += 1;
    v
}

/// Substitute every `Bind` in the row's TRIVIAL columns by the column ref it names.
fn va_substitute_binds(r: &mut VaRow, refs: &[IrExpr], only: Option<usize>) {
    for c in 0..r.pats.len() {
        if only.is_some_and(|j| j != c) {
            continue;
        }
        if let IrPattern::Bind { var, .. } = &r.pats[c] {
            let var = *var;
            r.body = almide_ir::substitute_var_in_expr(&r.body, var, &refs[c]);
            r.guard = r.guard.as_ref().map(|g| almide_ir::substitute_var_in_expr(g, var, &refs[c]));
            r.pats[c] = IrPattern::Wildcard;
        }
    }
}

/// Do `keys` cover the column's type exhaustively (so the emitted match needs no `_`
/// arm)? Conservative: anything unresolvable answers `false`.
fn va_heads_cover(keys: &[VaKey], layouts: &crate::lower::VariantLayouts) -> bool {
    if keys.iter().all(|k| matches!(k, VaKey::Some_ | VaKey::None_)) {
        return keys.contains(&VaKey::Some_) && keys.contains(&VaKey::None_);
    }
    if keys.iter().all(|k| matches!(k, VaKey::Ok_ | VaKey::Err_)) {
        return keys.contains(&VaKey::Ok_) && keys.contains(&VaKey::Err_);
    }
    let Some(VaKey::User(first)) = keys.first() else { return false };
    let Some((_, layout, _)) = layouts.lookup_ctor(first) else { return false };
    !layout.cases.is_empty()
        && layout.cases.iter().all(|c| keys.iter().any(|k| matches!(k, VaKey::User(n) if n == c.ctor.as_str())))
}

/// The declared field names and types of `key` when the column has type `cty`.
fn va_head_fields(
    key: &VaKey,
    cty: &Ty,
    layouts: &crate::lower::VariantLayouts,
) -> Option<(Vec<String>, Vec<Ty>)> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let applied = |ctor: TC, arity: usize, idx: usize| -> Option<(Vec<String>, Vec<Ty>)> {
        match cty {
            Ty::Applied(c, a) if *c == ctor && a.len() == arity => Some((vec!["_0".to_string()], vec![a[idx].clone()])),
            _ => None,
        }
    };
    match key {
        VaKey::User(name) => {
            let (_, layout, case) = layouts.lookup_ctor(name)?;
            if !layout.generics.is_empty() {
                return None;
            }
            Some(case.fields.iter().map(|(n, t)| (n.as_str().to_string(), t.clone())).unzip())
        }
        VaKey::Some_ => applied(TC::Option, 1, 0),
        VaKey::Ok_ => applied(TC::Result, 2, 0),
        VaKey::Err_ => applied(TC::Result, 2, 1),
        VaKey::None_ => Some((Vec::new(), Vec::new())),
    }
}

/// The type of a record field the matrix mentions, read off the patterns that name it
/// (a plain record has no variant layout to ask).
fn va_field_ty_from_pats<'a>(
    pats: impl Iterator<Item = &'a IrPattern>,
    layouts: &crate::lower::VariantLayouts,
) -> Option<Ty> {
    for p in pats {
        match p {
            IrPattern::Bind { ty, .. } => return Some(ty.clone()),
            IrPattern::Literal { expr } => return Some(expr.ty.clone()),
            IrPattern::Constructor { name, .. } | IrPattern::RecordPattern { name, .. } => {
                if let Some((tyname, layout, _)) = layouts.lookup_ctor(name) {
                    if layout.generics.is_empty() {
                        return Some(Ty::Named(almide_lang::intern::sym(tyname), Vec::new()));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// The recursive column compiler. `refs[c]` is the (Var) expression re-reading column
/// `c`; `tmpl` supplies the result ty/span/def_id; `emitted[idx]` counts how many branches
/// cloned source arm `idx` (the duplication gate reads it after).
fn va_compile(
    refs: &[IrExpr],
    mut rows: Vec<VaRow>,
    tmpl: &IrExpr,
    next: &mut u32,
    layouts: &crate::lower::VariantLayouts,
    emitted: &mut [usize],
) -> Option<IrExpr> {
    let mk = |kind: IrExprKind| IrExpr { kind, ty: tmpl.ty.clone(), span: tmpl.span, def_id: tmpl.def_id };
    // First-match pruning: rows after the first unguarded always-matching row are dead.
    if let Some(k) = rows.iter().position(|r| r.guard.is_none() && r.pats.iter().all(va_trivial)) {
        rows.truncate(k + 1);
    }
    let first = rows.first()?;
    if first.pats.iter().all(va_trivial) {
        let mut leaf = first.clone();
        va_substitute_binds(&mut leaf, refs, None);
        emitted[leaf.idx] += 1;
        return match leaf.guard {
            None => Some(leaf.body),
            Some(g) => {
                let rest: Vec<VaRow> = rows[1..].to_vec();
                if rest.is_empty() {
                    return None;
                }
                let else_ = va_compile(refs, rest, tmpl, next, layouts, emitted)?;
                Some(mk(IrExprKind::If { cond: Box::new(g), then: Box::new(leaf.body), else_: Box::new(else_) }))
            }
        };
    }
    let j = (0..refs.len()).find(|&c| rows.iter().any(|r| !va_trivial(&r.pats[c])))?;
    for r in rows.iter_mut() {
        va_substitute_binds(r, refs, Some(j));
    }
    let probe = rows.iter().find(|r| !va_trivial(&r.pats[j]))?.pats[j].clone();
    if va_lit(&probe).is_some() {
        return va_compile_literal_column(refs, rows, j, tmpl, next, layouts, emitted);
    }
    let (key, _) = va_head(&probe)?;
    if let VaKey::User(name) = &key {
        if layouts.lookup_ctor(name).is_none() {
            return match &refs[j].ty {
                Ty::Named(tname, targs) if targs.is_empty() && tname.as_str() == name => {
                    va_compile_record_column(refs, rows, j, tmpl, next, layouts, emitted)
                }
                _ => None,
            };
        }
    }
    va_compile_ctor_column(refs, rows, j, tmpl, next, layouts, emitted)
}

/// A SCALAR column: an `if` for Bool; for Int / String, the flat literal-and-guard chain
/// when no other column tests anything (no duplication), else one branch per literal in
/// first-appearance order plus the `_` default, each holding the rows that can reach it.
fn va_compile_literal_column(
    refs: &[IrExpr],
    rows: Vec<VaRow>,
    j: usize,
    tmpl: &IrExpr,
    next: &mut u32,
    layouts: &crate::lower::VariantLayouts,
    emitted: &mut [usize],
) -> Option<IrExpr> {
    let mk = |kind: IrExprKind| IrExpr { kind, ty: tmpl.ty.clone(), span: tmpl.span, def_id: tmpl.def_id };
    let lits: Vec<Option<VaLit>> = rows
        .iter()
        .map(|r| if va_trivial(&r.pats[j]) { Some(None) } else { va_lit(&r.pats[j]).map(Some) })
        .collect::<Option<Vec<_>>>()?;
    let is_bool = lits.iter().any(|l| matches!(l, Some(VaLit::Bool(_))));
    let cleared = |keep: &dyn Fn(&Option<VaLit>) -> bool| -> Vec<VaRow> {
        rows.iter()
            .zip(lits.iter())
            .filter(|(_, l)| keep(l))
            .map(|(r, _)| {
                let mut r = r.clone();
                r.pats[j] = IrPattern::Wildcard;
                r
            })
            .collect()
    };
    if is_bool {
        if lits.iter().any(|l| matches!(l, Some(VaLit::Int(_) | VaLit::Str(_)))) {
            return None;
        }
        let then_rows = cleared(&|l| !matches!(l, Some(VaLit::Bool(false))));
        let else_rows = cleared(&|l| !matches!(l, Some(VaLit::Bool(true))));
        if then_rows.is_empty() || else_rows.is_empty() {
            return None;
        }
        let then = va_compile(refs, then_rows, tmpl, next, layouts, emitted)?;
        let else_ = va_compile(refs, else_rows, tmpl, next, layouts, emitted)?;
        return Some(mk(IrExprKind::If { cond: Box::new(refs[j].clone()), then: Box::new(then), else_: Box::new(else_) }));
    }
    let single = rows.iter().all(|r| r.pats.iter().enumerate().all(|(c, p)| c == j || va_trivial(p)));
    if single {
        let last = rows.last()?;
        if last.guard.is_some() || !va_trivial(&last.pats[j]) {
            return None;
        }
        let mut arms: Vec<IrMatchArm> = Vec::with_capacity(rows.len());
        for r in &rows {
            let mut leaf = r.clone();
            let pattern = std::mem::replace(&mut leaf.pats[j], IrPattern::Wildcard);
            va_substitute_binds(&mut leaf, refs, None);
            emitted[leaf.idx] += 1;
            arms.push(IrMatchArm { pattern, guard: leaf.guard, body: leaf.body });
        }
        return Some(mk(IrExprKind::Match { subject: Box::new(refs[j].clone()), arms }));
    }
    let mut seen: Vec<VaLit> = Vec::new();
    for l in lits.iter().flatten() {
        if !seen.contains(l) {
            seen.push(l.clone());
        }
    }
    let mut arms: Vec<IrMatchArm> = Vec::with_capacity(seen.len() + 1);
    for l in &seen {
        let sub = cleared(&|x| x.is_none() || x.as_ref() == Some(l));
        let body = va_compile(refs, sub, tmpl, next, layouts, emitted)?;
        let pattern = rows
            .iter()
            .zip(lits.iter())
            .find(|(_, x)| x.as_ref() == Some(l))
            .map(|(r, _)| r.pats[j].clone())?;
        arms.push(IrMatchArm { pattern, guard: None, body });
    }
    let default = cleared(&|x| x.is_none());
    if default.is_empty() {
        return None;
    }
    let dbody = va_compile(refs, default, tmpl, next, layouts, emitted)?;
    arms.push(IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: dbody });
    Some(mk(IrExprKind::Match { subject: Box::new(refs[j].clone()), arms }))
}

/// A CONSTRUCTOR column: one arm per head in first-appearance order, binding the fields
/// the rows mention to fresh vars (a record form names them and keeps `..` for the rest),
/// then the `_` default unless the heads cover the type.
fn va_compile_ctor_column(
    refs: &[IrExpr],
    rows: Vec<VaRow>,
    j: usize,
    tmpl: &IrExpr,
    next: &mut u32,
    layouts: &crate::lower::VariantLayouts,
    emitted: &mut [usize],
) -> Option<IrExpr> {
    let mk = |kind: IrExprKind| IrExpr { kind, ty: tmpl.ty.clone(), span: tmpl.span, def_id: tmpl.def_id };
    let mut keys: Vec<(VaKey, bool)> = Vec::new();
    for r in &rows {
        if va_trivial(&r.pats[j]) {
            continue;
        }
        let (k, record_form) = va_head(&r.pats[j])?;
        match keys.iter().find(|(k2, _)| *k2 == k) {
            Some((_, rf)) if *rf != record_form => return None,
            Some(_) => {}
            None => keys.push((k, record_form)),
        }
    }
    let mut arms: Vec<IrMatchArm> = Vec::with_capacity(keys.len() + 1);
    for (key, record_form) in &keys {
        let (names, tys) = va_head_fields(key, &refs[j].ty, layouts)?;
        // A record form binds only the fields some row names; a positional head binds all.
        let mentioned: Vec<usize> = if *record_form {
            (0..names.len())
                .filter(|&i| {
                    rows.iter().any(|r| match &r.pats[j] {
                        IrPattern::RecordPattern { fields, .. } if va_head(&r.pats[j]).map(|(k, _)| k).as_ref() == Some(key) => {
                            fields.iter().any(|f| f.name == names[i])
                        }
                        _ => false,
                    })
                })
                .collect()
        } else {
            (0..names.len()).collect()
        };
        let mnames: Vec<String> = mentioned.iter().map(|&i| names[i].clone()).collect();
        let fresh: Vec<(VarId, Ty)> = mentioned.iter().map(|&i| (va_fresh(next), tys[i].clone())).collect();
        let mut nrefs: Vec<IrExpr> = Vec::with_capacity(refs.len() - 1 + fresh.len());
        nrefs.extend_from_slice(&refs[..j]);
        nrefs.extend(fresh.iter().map(|(v, t)| va_var(*v, t, tmpl)));
        nrefs.extend_from_slice(&refs[j + 1..]);
        let mut nrows: Vec<VaRow> = Vec::new();
        for r in &rows {
            let args: Vec<IrPattern> = if va_trivial(&r.pats[j]) {
                vec![IrPattern::Wildcard; fresh.len()]
            } else if va_head(&r.pats[j]).map(|(k, _)| k).as_ref() == Some(key) {
                va_head_args(&r.pats[j], &mnames)?
            } else {
                continue;
            };
            let mut np = Vec::with_capacity(nrefs.len());
            np.extend_from_slice(&r.pats[..j]);
            np.extend(args);
            np.extend_from_slice(&r.pats[j + 1..]);
            nrows.push(VaRow { pats: np, guard: r.guard.clone(), body: r.body.clone(), idx: r.idx });
        }
        let branch = va_compile(&nrefs, nrows, tmpl, next, layouts, emitted)?;
        let binds: Vec<IrPattern> = fresh.iter().map(|(v, t)| IrPattern::Bind { var: *v, ty: t.clone() }).collect();
        let pattern = match key {
            VaKey::User(name) if *record_form => IrPattern::RecordPattern {
                name: name.clone(),
                fields: mnames
                    .iter()
                    .zip(binds)
                    .map(|(n, b)| almide_ir::IrFieldPattern { name: n.clone(), pattern: Some(b) })
                    .collect(),
                rest: mentioned.len() < names.len(),
            },
            VaKey::User(name) => IrPattern::Constructor { name: name.clone(), args: binds },
            VaKey::Some_ => IrPattern::Some { inner: Box::new(binds.into_iter().next()?) },
            VaKey::Ok_ => IrPattern::Ok { inner: Box::new(binds.into_iter().next()?) },
            VaKey::Err_ => IrPattern::Err { inner: Box::new(binds.into_iter().next()?) },
            VaKey::None_ => IrPattern::None,
        };
        arms.push(IrMatchArm { pattern, guard: None, body: branch });
    }
    let head_keys: Vec<VaKey> = keys.iter().map(|(k, _)| k.clone()).collect();
    if !va_heads_cover(&head_keys, layouts) {
        let mut drows: Vec<VaRow> = Vec::new();
        for r in &rows {
            if va_trivial(&r.pats[j]) {
                let mut np = r.pats.clone();
                np.remove(j);
                drows.push(VaRow { pats: np, guard: r.guard.clone(), body: r.body.clone(), idx: r.idx });
            }
        }
        if drows.is_empty() {
            // Frontend exhaustiveness says this path is unreachable, but emitting a
            // non-exhaustive inner match would wall — decline instead.
            return None;
        }
        let mut nrefs = refs.to_vec();
        nrefs.remove(j);
        let dbody = va_compile(&nrefs, drows, tmpl, next, layouts, emitted)?;
        arms.push(IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: dbody });
    }
    Some(mk(IrExprKind::Match { subject: Box::new(refs[j].clone()), arms }))
}

/// A PLAIN-RECORD column (the pattern name is the column's own type, not a variant
/// case): the record pattern is irrefutable, so the fields the rows name are projected
/// once into fresh lets and become columns in their place.
fn va_compile_record_column(
    refs: &[IrExpr],
    rows: Vec<VaRow>,
    j: usize,
    tmpl: &IrExpr,
    next: &mut u32,
    layouts: &crate::lower::VariantLayouts,
    emitted: &mut [usize],
) -> Option<IrExpr> {
    let tname = match &refs[j].ty {
        Ty::Named(t, _) => t.as_str().to_string(),
        _ => return None,
    };
    let mut names: Vec<String> = Vec::new();
    for r in &rows {
        match &r.pats[j] {
            IrPattern::RecordPattern { name, fields, .. } if *name == tname => {
                for f in fields {
                    if !names.contains(&f.name) {
                        names.push(f.name.clone());
                    }
                }
            }
            p if va_trivial(p) => {}
            _ => return None,
        }
    }
    let field_pat = |r: &VaRow, n: &str| -> IrPattern {
        match &r.pats[j] {
            IrPattern::RecordPattern { fields, .. } => fields
                .iter()
                .find(|f| f.name == n)
                .and_then(|f| f.pattern.clone())
                .unwrap_or(IrPattern::Wildcard),
            _ => IrPattern::Wildcard,
        }
    };
    let mut stmts: Vec<IrStmt> = Vec::with_capacity(names.len());
    let mut fresh: Vec<(VarId, Ty)> = Vec::with_capacity(names.len());
    for n in &names {
        let ty = va_field_ty_from_pats(rows.iter().map(|r| field_pat(r, n)).collect::<Vec<_>>().iter(), layouts)?;
        let v = va_fresh(next);
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: v,
                ty: ty.clone(),
                value: IrExpr {
                    kind: IrExprKind::Member { object: Box::new(refs[j].clone()), field: almide_lang::intern::sym(n) },
                    ty: ty.clone(),
                    span: tmpl.span,
                    def_id: None,
                },
                mutability: almide_ir::Mutability::Let,
            },
            span: tmpl.span,
        });
        fresh.push((v, ty));
    }
    let mut nrefs: Vec<IrExpr> = Vec::with_capacity(refs.len() - 1 + fresh.len());
    nrefs.extend_from_slice(&refs[..j]);
    nrefs.extend(fresh.iter().map(|(v, t)| va_var(*v, t, tmpl)));
    nrefs.extend_from_slice(&refs[j + 1..]);
    let nrows: Vec<VaRow> = rows
        .iter()
        .map(|r| {
            let mut np = Vec::with_capacity(nrefs.len());
            np.extend_from_slice(&r.pats[..j]);
            np.extend(names.iter().map(|n| field_pat(r, n)));
            np.extend_from_slice(&r.pats[j + 1..]);
            VaRow { pats: np, guard: r.guard.clone(), body: r.body.clone(), idx: r.idx }
        })
        .collect();
    let body = va_compile(&nrefs, nrows, tmpl, next, layouts, emitted)?;
    Some(IrExpr {
        kind: IrExprKind::Block { stmts, expr: Some(Box::new(body)) },
        ty: tmpl.ty.clone(),
        span: tmpl.span,
        def_id: tmpl.def_id,
    })
}

/// The user-variant / plain-record entry: the rewritten match, or `None` to leave it to
/// the existing routes and their honest walls. Fires only when an arm carries a guard or
/// a refutable payload pattern; a plain constructor dispatch is left untouched.
fn specialize_user_variant_match(
    e: &IrExpr,
    next: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let IrExprKind::Match { subject, arms } = &e.kind else { return None };
    if arms.is_empty() {
        return None;
    }
    let mut hoist: Vec<IrStmt> = Vec::new();
    let mut hoist_once = |value: &IrExpr, next: &mut u32| -> IrExpr {
        if matches!(value.kind, IrExprKind::Var { .. }) {
            return value.clone();
        }
        let t = va_fresh(next);
        hoist.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: t,
                ty: value.ty.clone(),
                value: value.clone(),
                mutability: almide_ir::Mutability::Let,
            },
            span: e.span,
        });
        va_var(t, &value.ty, e)
    };
    let (refs, rows): (Vec<IrExpr>, Vec<VaRow>) = match &subject.kind {
        // The TUPLE sub-match a multi-field variant regroup leaves behind (`Tagged(a, b) =>
        // match (a, b) { (Small, l) => …, (Big(0), _) => … }`) when its components mix
        // literals and nested constructors — the literal tuple chain and the ctor-column
        // specializer (which both run earlier) each decline the other's column kind.
        IrExprKind::Tuple { elements } => {
            let n = elements.len();
            let rows: Vec<VaRow> = arms
                .iter()
                .enumerate()
                .map(|(idx, a)| {
                    let pats = match &a.pattern {
                        IrPattern::Tuple { elements: ps } if ps.len() == n => ps.clone(),
                        IrPattern::Wildcard => vec![IrPattern::Wildcard; n],
                        _ => return None,
                    };
                    Some(VaRow { pats, guard: a.guard.clone(), body: a.body.clone(), idx })
                })
                .collect::<Option<Vec<_>>>()?;
            if !rows.iter().any(|r| r.guard.is_some() || r.pats.iter().any(|p| !va_trivial(p))) {
                return None;
            }
            if !rows.iter().all(|r| r.pats.iter().all(va_pat_ok)) {
                return None;
            }
            // Every component is re-read by its column: hoist each non-Var one once.
            let refs: Vec<IrExpr> = elements.iter().map(|el| hoist_once(el, next)).collect();
            (refs, rows)
        }
        _ => {
            if !matches!(&subject.ty, Ty::Named(..) | Ty::Applied(TC::UserDefined(_), _)) {
                return None;
            }
            if !arms.iter().any(|a| a.guard.is_some() || va_refutable_inside(&a.pattern)) {
                return None;
            }
            if !arms.iter().all(|a| va_pat_ok(&a.pattern)) {
                return None;
            }
            // The subject is re-read by a catch-all BINDER's substitution and by a plain
            // record's field projections: hoist a non-Var subject once so every read sees
            // one value.
            let reread = arms.iter().any(|a| match &a.pattern {
                IrPattern::Bind { .. } => true,
                IrPattern::RecordPattern { name, .. } => layouts.lookup_ctor(name).is_none(),
                _ => false,
            });
            let subj = if reread { hoist_once(subject, next) } else { (**subject).clone() };
            let rows: Vec<VaRow> = arms
                .iter()
                .enumerate()
                .map(|(idx, a)| VaRow { pats: vec![a.pattern.clone()], guard: a.guard.clone(), body: a.body.clone(), idx })
                .collect();
            (vec![subj], rows)
        }
    };
    let mut emitted = vec![0usize; arms.len()];
    let compiled = va_compile(&refs, rows, e, next, layouts, &mut emitted)?;
    // Duplication gates: a row cloned into >1 branch must be binder-free, and the whole
    // tree must stay small (the same blow-up discipline as heap-branches).
    for (idx, count) in emitted.iter().enumerate() {
        if *count > 1
            && (introduces_binder(&arms[idx].body) || arms[idx].guard.as_ref().is_some_and(introduces_binder))
        {
            return None;
        }
    }
    if count_expr_nodes(&compiled) > 50_000 {
        return None;
    }
    Some(if hoist.is_empty() {
        compiled
    } else {
        IrExpr {
            kind: IrExprKind::Block { stmts: hoist, expr: Some(Box::new(compiled)) },
            ty: e.ty.clone(),
            span: e.span,
            def_id: e.def_id,
        }
    })
}
