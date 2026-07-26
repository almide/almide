
// ── Core walker ────────────────────────────────────────────────────

fn concretize_expr(expr: &mut IrExpr, vt: &mut VarTable, symbols: &SymbolTable, enclosing_ret: &Ty) {
    let mut c = Concretizer { vt, symbols, enclosing_ret };
    c.visit_expr_mut(expr);
}

struct Concretizer<'a> {
    vt: &'a mut VarTable,
    symbols: &'a SymbolTable,
    /// Return type of the enclosing IrFunction (post-`ResultPropagation`
    /// lift when applicable). Used to fill in the `Ok` slot of a
    /// `ResultErr` whose payload was written without the Ok type the
    /// checker could infer (`guard x else err(...)!` style).
    enclosing_ret: &'a Ty,
}

impl<'a> Concretizer<'a> {
    /// Resolve the empty-list argument element type of `map.from_list(arg)` /
    /// `set.from_list(arg)` from the expected Map/Set type `ret`, pinning the
    /// arg expression, any Borrow/Clone/Deref wrappers, and the ANF-temp's
    /// VarTable entry — the element only flows through the generic return, so
    /// the checker can leave it `List[(?K,?V)]` past the WASM gate (#625).
    fn pin_from_list_arg_elem(&mut self, ret: &Ty, value: &mut IrExpr) {
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        if ret.has_unresolved_deep() { return; }
        let (is_map, is_set) = detect_from_list_call_kind(value);
        let Some(elem) = from_list_elem_ty(ret, is_map, is_set) else { return };
        let list_ty = Ty::Applied(TCI::List, vec![elem]);
        let args = match &mut value.kind {
            IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } => args,
            _ => return,
        };
        if args.len() != 1 || !args[0].ty.has_unresolved_deep() { return; }
        propagate_expected_ty(&mut args[0], &list_ty);
        self.pin_list_ty_through_wrappers(&mut args[0], &list_ty);
    }

    /// `pin_from_list_arg_elem` step: walk through `Borrow`/`Clone`/`Deref`
    /// wrappers to the underlying `Var`, pinning each node's `ty` and (for
    /// the `Var`) its VarTable entry to `list_ty` — extracted verbatim
    /// (cog>25 decomposition).
    fn pin_list_ty_through_wrappers(&mut self, arg: &mut IrExpr, list_ty: &Ty) {
        let mut node = arg;
        loop {
            if node.ty.has_unresolved_deep() { node.ty = list_ty.clone(); }
            match &mut node.kind {
                IrExprKind::Borrow { expr, .. } | IrExprKind::Clone { expr } | IrExprKind::Deref { expr } => node = expr,
                IrExprKind::Var { id } => {
                    let i = id.0 as usize;
                    if i < self.vt.entries.len() && self.vt.entries[i].ty.has_unresolved_deep() {
                        self.vt.entries[i].ty = list_ty.clone();
                    }
                    break;
                }
                _ => break,
            }
        }
    }

    // ── `visit_expr_mut` step extraction (cog>100 decomposition, pattern 2) ──
    //
    // Each of these is a 1:1 text-move of one independent step from the
    // original `visit_expr_mut` body. None of them read a value another step
    // wrote earlier in the same call (they all read fresh from `expr`/`self`),
    // so splitting them into named methods called in the same original order
    // changes nothing observable.

    /// Custom Match handling: propagate subject ty into pattern bindings
    /// (updating both the pattern's declared ty and the VarTable entry)
    /// BEFORE visiting arm bodies, so Var references to pattern-bound
    /// names pick up the refreshed ty during the bottom-up walk. Only
    /// called by `visit_expr_mut` once it has confirmed `expr` is a Match
    /// (mirrors the original's `if let ... { ...; return; }` early exit).
    fn visit_match_expr(&mut self, expr: &mut IrExpr) {
        let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
        self.visit_expr_mut(subject);
        let sty = subject.ty.clone();
        if !sty.has_unresolved_deep() {
            for arm in arms.iter_mut() {
                propagate_pattern_ty(&mut arm.pattern, &sty, self.vt);
            }
        }
        for arm in arms.iter_mut() {
            if let Some(g) = &mut arm.guard { self.visit_expr_mut(g); }
            self.visit_expr_mut(&mut arm.body);
        }
        // After arms are fully resolved, push any concrete arm body ty
        // into sibling arms whose body is an unresolved shape wrapper
        // (e.g. `none => none` has body ty Option[Unknown] but the
        // sibling `some(...)` arm resolves to Option[List[String]]).
        let concrete_arm_ty = arms.iter().find_map(|arm| {
            if !arm.body.ty.has_unresolved_deep() { Some(arm.body.ty.clone()) } else { None }
        });
        if let Some(cty) = concrete_arm_ty {
            for arm in arms.iter_mut() {
                if arm.body.ty.has_unresolved_deep() {
                    propagate_ty_down(&mut arm.body, &cty);
                }
            }
        }
        // Resolve the Match node itself
        if expr.ty.has_unresolved_deep() {
            if let Some(ty) = resolve_node_ty(expr, self.vt, self.symbols) {
                expr.ty = ty;
            }
        }
    }

    /// Resolve Unknown lambda params from body usage (e.g. `(a,b) => a + b` → Int).
    fn resolve_lambda_param_tys(&mut self, expr: &mut IrExpr) {
        let IrExprKind::Lambda { params, body, .. } = &mut expr.kind else { return };
        let mut patched = false;
        for (var_id, var_ty) in params.iter_mut() {
            if matches!(var_ty, Ty::Unknown) {
                if let Some(inferred) = infer_var_type_from_body(body, *var_id) {
                    *var_ty = inferred.clone();
                    self.vt.entries[var_id.0 as usize].ty = inferred;
                    patched = true;
                }
            }
        }
        // Re-visit body to propagate patched param types into Var nodes
        if patched { walk_expr_mut(self, body); }
    }

    /// Record literal construction: push the declared field types from the
    /// registered type down into field value expressions whose own
    /// inference left them unresolved (typically `Applied(List, [Unknown])`
    /// for a field defaulted to `[]`). The checker sees `items: []` and can
    /// only type it `List[Unknown]`; we know from the record decl that
    /// `items: List[Int]`, so substitute.
    fn propagate_record_field_tys(&mut self, expr: &mut IrExpr) {
        let IrExprKind::Record { name: Some(name), fields } = &mut expr.kind else { return };
        let rname = name.to_string();
        for (fname, fvalue) in fields.iter_mut() {
            if fvalue.ty.has_unresolved_deep() {
                if let Some(expected) = self.symbols.lookup_field(&rname, fname.as_str()) {
                    if !expected.has_unresolved_deep() {
                        propagate_expected_ty(fvalue, expected);
                    }
                }
            }
        }
    }

    /// Generic-accumulator back-propagation for `list.fold` / `list.scan`:
    /// both `init` arg and lambda `body.ty` represent the accumulator A.
    /// After the bottom-up walk, body.ty may be strictly more concrete
    /// than init.ty (because init started from a literal like `some([])`
    /// whose empty list has element type Unknown). Merge, push the merged
    /// shape back into init's sub-expressions, and update the lambda's acc
    /// param + VarTable so arm Var refs refresh on the re-visit below.
    fn back_propagate_fold_acc_ty(&mut self, expr: &mut IrExpr) {
        if !is_fold_like_call(expr) { return; }
        if !back_propagate_fold_acc(expr, self.vt) { return; }
        // Re-visit the lambda body so pattern bindings and Var
        // references pick up the refreshed acc type.
        let IrExprKind::Call { args, .. } = &mut expr.kind else { return };
        let Some(lambda) = args.get_mut(2) else { return };
        let IrExprKind::Lambda { body, .. } = &mut lambda.kind else { return };
        self.visit_expr_mut(body);
    }

    /// Second chance for Member: resolve the field's type from the object's
    /// (now bottom-up-resolved) record/named type when the generic
    /// `resolve_node_ty` pass didn't manage to pin it.
    fn resolve_member_ty_fallback(&mut self, expr: &mut IrExpr) {
        if !(expr.ty).has_unresolved_deep() { return; }
        let IrExprKind::Member { object, field } = &expr.kind else { return };
        let obj_ty = effective_ty(object, self.vt);
        let resolved = match &obj_ty {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().find(|(n, _)| n == field.as_str()).map(|(_, t)| t.clone())
                    .filter(|t| !t.has_unresolved_deep())
            }
            Ty::Named(name, _) => {
                let r = self.symbols.lookup_field(name.as_str(), field.as_str());
                r.filter(|t| !t.has_unresolved_deep()).cloned()
            }
            _ => {
                None
            }
        };
        if let Some(ty) = resolved {
            expr.ty = ty;
        }
    }
}

impl<'a> IrMutVisitor for Concretizer<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Custom Match handling: propagate subject ty into pattern bindings
        // BEFORE visiting arm bodies. See `visit_match_expr`.
        if matches!(&expr.kind, IrExprKind::Match { .. }) {
            self.visit_match_expr(expr);
            return;
        }

        // Recurse into children FIRST (bottom-up) so nested types are
        // concrete before we use them here.
        walk_expr_mut(self, expr);

        self.resolve_lambda_param_tys(expr);

        // Rewrite BinOp when operand types disagree with the op kind.
        // Type checker may have picked `AddInt` for polymorphic code that
        // later specialized to Float (e.g. via list element type). Without
        // this fix emit generates i64.add on f64 operands.
        if let IrExprKind::BinOp { op, left, right } = &mut expr.kind {
            if let Some(new_op) = reconcile_binop(*op, &left.ty, &right.ty) {
                *op = new_op;
            }
        }

        // Effect-fn `guard` / `?` paths can leave a `ResultErr` with
        // `Ok = Unknown` when the error value is the only thing the
        // checker can pin down (`guard x else err("msg")!`). The Ok
        // slot is the enclosing fn's return Ok type — after
        // ResultPropagation has lifted it to `Result[T, String]`, we
        // know `T` precisely.
        if let IrExprKind::ResultErr { expr: inner } = &mut expr.kind {
            if expr.ty.has_unresolved_deep() {
                if let Some(fixed) = infer_err_ty_from_enclosing(self.enclosing_ret, &inner.ty) {
                    expr.ty = fixed;
                }
            }
        }
        // #625: `map.from_list([])` / `set.from_list([])` — the empty-list
        // argument's element type is determined ONLY by the call's return
        // (`Map[K,V]` ← `List[(K,V)]`, `Set[E]` ← `List[E]`). The checker can
        // leave that arg `List[(?K,?V)]` (the K,V flow only through the generic
        // signature's return, not through any literal element), which would slip
        // past this gate on native but be refused by the WASM concretization
        // gate. Derive the arg element from the resolved return type here.
        // #625: `map.from_list([])` / `set.from_list([])` where the call is NOT
        // the direct value of an annotated binding — derive the empty arg's
        // element from the call's own (resolved) return type. The annotated-bind
        // case is handled more reliably in `visit_stmt_mut`.
        {
            let ret_ty = expr.ty.clone();
            self.pin_from_list_arg_elem(&ret_ty, expr);
        }

        self.propagate_record_field_tys(expr);
        self.back_propagate_fold_acc_ty(expr);

        // Now resolve this node's type from child types + VarTable + symbols.
        if (expr.ty).has_unresolved_deep() {
            if let Some(ty) = resolve_node_ty(expr, self.vt, self.symbols) {
                expr.ty = ty.clone();
                // Propagate: if this was IndexAccess and it resolved, the
                // parent Member can now resolve too. But we're bottom-up,
                // so Member visits AFTER this. Make sure we updated expr.ty.
            }
        }
        self.resolve_member_ty_fallback(expr);
    }

    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        walk_stmt_mut(self, stmt);
        // Sync Bind { ty } *and* the VarTable entry for the bound var with
        // value.ty when we now know the value type. Without the VarTable
        // sync, later Var references to the same binding (and the
        // post-pass audit reading VarTable directly) keep seeing Unknown.
        if let IrStmtKind::Bind { var, ty, value, .. } = &mut stmt.kind {
            if !(value.ty).has_unresolved_deep() {
                if ty.has_unresolved_deep() {
                    *ty = value.ty.clone();
                }
                if (var.0 as usize) < self.vt.len()
                    && self.vt.get(*var).ty.has_unresolved_deep()
                {
                    self.vt.entries[var.0 as usize].ty = value.ty.clone();
                }
            }
            // #625: `let m: Map[K,V] = map.from_list(arg)` / `set.from_list`.
            // The arg's element type flows ONLY through the generic call's
            // return, so the checker can leave it `List[(?K,?V)]`. The BIND's
            // declared type is the reliable source (the call's own ty may not
            // be resolved on every pass), so derive the arg element from it and
            // pin the arg (and its ANF-temp VarTable entry) before the gate.
            self.pin_from_list_arg_elem(ty, value);
        }
        // Destructuring let: `let (k, v) = pair`, `let some(x) = opt`, … The
        // checker can leave the bound pattern vars `Unknown` when the subject's
        // type resolved only after binding (e.g. `pair` is `list.zip(..)[i]`,
        // whose tuple element type ConcretizeTypes pins during THIS bottom-up
        // walk). Once `value.ty` is concrete, push it into the pattern bindings
        // and their VarTable entries — the same propagation `match` already gets
        // (via `propagate_pattern_ty` in the Match arm), now extended to the
        // statement form so later `Var` refs and the hard gate see concrete types
        // instead of a leftover `Unknown` (the `let (k, v) = pair` → `v: Unknown`
        // class).
        if let IrStmtKind::BindDestructure { pattern, value } = &mut stmt.kind {
            if !(value.ty).has_unresolved_deep() {
                propagate_pattern_ty(pattern, &value.ty, self.vt);
            }
        }
    }
}

/// `pin_from_list_arg_elem` step: identify whether `value` is a
/// `map.from_list`/`map.from_entries` or `set.from_list`/`set.from_entries`
/// call, returning `(is_map, is_set)`. `map.from_list` canonicalizes to
/// `map.from_entries`, and by the WASM emit passes it is a `RuntimeCall`
/// (`almide_rt_map_from_entries`), not a `Module` call — match both forms
/// for each module (set keeps `from_list`). Extracted verbatim (cog>25
/// decomposition).
fn detect_from_list_call_kind(value: &IrExpr) -> (bool, bool) {
    match &value.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
            let fl = func.as_str() == "from_list" || func.as_str() == "from_entries";
            (module.as_str() == "map" && fl, module.as_str() == "set" && fl)
        }
        IrExprKind::RuntimeCall { symbol, .. } => {
            let s = symbol.as_str();
            (s.contains("map_from_entries") || s.contains("map_from_list"),
             s.contains("set_from_entries") || s.contains("set_from_list"))
        }
        _ => (false, false),
    }
}

/// `pin_from_list_arg_elem` step: compute the empty-list arg's element type
/// from the call's resolved return type, given which of `map`/`set` it is
/// (from `detect_from_list_call_kind`). `None` when `ret` isn't the
/// expected `Map`/`Set` shape, or when neither flag is set. Extracted
/// verbatim (cog>25 decomposition).
fn from_list_elem_ty(ret: &Ty, is_map: bool, is_set: bool) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    if is_map {
        match ret { Ty::Applied(TCI::Map, kv) if kv.len() == 2 => Some(Ty::Tuple(vec![kv[0].clone(), kv[1].clone()])), _ => None }
    } else if is_set {
        match ret { Ty::Applied(TCI::Set, e) if e.len() == 1 => Some(e[0].clone()), _ => None }
    } else {
        None
    }
}

// ── Resolution logic ───────────────────────────────────────────────

/// Infer a lambda param's type by scanning how it's used in the body.
/// e.g., `(a, b) => a + b` where body is BinOp::AddInt → a: Int, b: Int
fn binop_operand_type(op: &BinOp, left: &IrExpr, right: &IrExpr, var: VarId) -> Option<Ty> {
    let fixed_ty = match op {
        BinOp::AddInt | BinOp::SubInt | BinOp::MulInt | BinOp::DivInt | BinOp::ModInt | BinOp::PowInt => Some(Ty::Int),
        BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat => Some(Ty::Float),
        BinOp::ConcatStr => Some(Ty::String),
        BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte =>
            infer_from_other_side(left, right, var),
        _ => None,
    };
    if let Some(ref ty) = fixed_ty {
        if matches!(&left.kind, IrExprKind::Var { id } if *id == var) { return Some(ty.clone()); }
        if matches!(&right.kind, IrExprKind::Var { id } if *id == var) { return Some(ty.clone()); }
    }
    None
}

fn infer_from_other_side(left: &IrExpr, right: &IrExpr, var: VarId) -> Option<Ty> {
    if matches!(&left.kind, IrExprKind::Var { id } if *id == var) {
        if !right.ty.has_unresolved_deep() { Some(right.ty.clone()) } else { None }
    } else if matches!(&right.kind, IrExprKind::Var { id } if *id == var) {
        if !left.ty.has_unresolved_deep() { Some(left.ty.clone()) } else { None }
    } else { None }
}

pub fn infer_var_type_from_body(body: &IrExpr, var: VarId) -> Option<Ty> {
    match &body.kind {
        IrExprKind::BinOp { op, left, right } =>
            binop_operand_type(op, left, right, var)
                .or_else(|| infer_var_type_from_body(left, var))
                .or_else(|| infer_var_type_from_body(right, var)),
        IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } =>
            args.iter().find_map(|a| infer_var_type_from_body(a, var)),
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().find_map(|s| match &s.kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::Expr { expr: value } =>
                    infer_var_type_from_body(value, var),
                _ => None,
            }).or_else(|| expr.as_ref().and_then(|e| infer_var_type_from_body(e, var)))
        }
        IrExprKind::If { cond, then, else_ } =>
            infer_var_type_from_body(cond, var)
                .or_else(|| infer_var_type_from_body(then, var))
                .or_else(|| infer_var_type_from_body(else_, var)),
        IrExprKind::Match { subject, arms } =>
            infer_var_type_from_body(subject, var)
                .or_else(|| arms.iter().find_map(|a| infer_var_type_from_body(&a.body, var))),
        // Look through Result/Option constructors so a wrapped body like
        // `ok(x * 10)` or `some(x * 10)` still exposes `x`'s use site. Without
        // this, a callback whose param type the checker failed to pin would have
        // no body-derived fallback either.
        IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } =>
            infer_var_type_from_body(expr, var),
        _ => None,
    }
}

/// `Some(ty.clone())` when `ty` carries no unresolved type variable,
/// `None` otherwise. The single repeated "is this node's type already
/// concrete" check that most [`resolve_node_ty`] arms perform.
fn if_concrete(ty: &Ty) -> Option<Ty> {
    if !ty.has_unresolved_deep() { Some(ty.clone()) } else { None }
}

/// `Member { object, field }` arm of [`resolve_node_ty`].
fn resolve_member_ty(object: &IrExpr, field: &almide_base::intern::Sym, vt: &VarTable, symbols: &SymbolTable) -> Option<Ty> {
    let obj_ty = effective_ty(object, vt);
    match &obj_ty {
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            fields.iter()
                .find(|(n, _)| n == field.as_str())
                .map(|(_, t)| t.clone())
                .filter(|t| !t.has_unresolved_deep())
        }
        Ty::Named(name, _) => {
            symbols.lookup_field(name.as_str(), field.as_str())
                .filter(|t| !t.has_unresolved_deep())
                .cloned()
        }
        _ => None,
    }
}

/// `Lambda { params, body, .. }` arm of [`resolve_node_ty`].
fn resolve_lambda_ty(params: &[(VarId, Ty)], body: &IrExpr) -> Option<Ty> {
    let fparams: Vec<Ty> = params.iter().map(|(_, t)| t.clone()).collect();
    if fparams.iter().any(Ty::has_unresolved_deep) || (body.ty).has_unresolved_deep() {
        return None;
    }
    Some(Ty::Fn {
        params: fparams,
        ret: Box::new(body.ty.clone()),
    })
}

/// `IndexAccess { object, .. }` arm of [`resolve_node_ty`]: for `List[T]`,
/// the result is `T`. Uses `effective_ty` to resolve through the VarTable.
fn resolve_index_access_ty(object: &IrExpr, vt: &VarTable) -> Option<Ty> {
    let obj_ty = effective_ty(object, vt);
    if obj_ty.has_unresolved_deep() {
    }
    if let Ty::Applied(_, args) = &obj_ty {
        args.first().cloned().filter(|t| !t.has_unresolved_deep())
    } else { None }
}

/// `MapAccess { object, .. }` arm of [`resolve_node_ty`]: `Map[K,V]` → `Option[V]`.
fn resolve_map_access_ty(object: &IrExpr, vt: &VarTable) -> Option<Ty> {
    let obj_ty = effective_ty(object, vt);
    if let Ty::Applied(_, args) = &obj_ty {
        args.get(1).cloned()
            .filter(|t| !t.has_unresolved_deep())
            .map(|v| Ty::Applied(
                almide_lang::types::constructor::TypeConstructorId::Option, vec![v],
            ))
    } else { None }
}

/// `resolve_node_ty` router (cog>25 decomposition, second round): each
/// `resolve_node_ty_*` group already ends in its own `_ => None`, so trying
/// them in sequence via `or_else` is behavior-preserving — a group that
/// doesn't own `expr`'s variant just falls through its own catch-all to
/// `None`, same as the original single match's `_ => None` would have.
fn resolve_node_ty(expr: &IrExpr, vt: &VarTable, symbols: &SymbolTable) -> Option<Ty> {
    resolve_node_ty_access(expr, vt, symbols)
        .or_else(|| resolve_node_ty_control(expr, vt))
        .or_else(|| resolve_node_ty_container(expr, vt, symbols))
}

/// `resolve_node_ty` group: variable/path access nodes (Var, EnvLoad,
/// TupleIndex, Member, IndexAccess, MapAccess, SpreadRecord).
fn resolve_node_ty_access(expr: &IrExpr, vt: &VarTable, symbols: &SymbolTable) -> Option<Ty> {
    match &expr.kind {
        IrExprKind::Var { id } => if_concrete(&vt.get(*id).ty),
        IrExprKind::EnvLoad { env_var, .. } => if_concrete(&vt.get(*env_var).ty),
        IrExprKind::TupleIndex { object, index } => {
            let obj_ty = effective_ty(object, vt);
            if let Ty::Tuple(elems) = &obj_ty {
                elems.get(*index).cloned().filter(|t| !t.has_unresolved_deep())
            } else { None }
        }
        IrExprKind::Member { object, field } => resolve_member_ty(object, field, vt, symbols),
        IrExprKind::IndexAccess { object, .. } => resolve_index_access_ty(object, vt),
        // MapAccess: Map[K,V] → Option[V]
        IrExprKind::MapAccess { object, .. } => resolve_map_access_ty(object, vt),
        // A spread copies its base's record type — the checker's own rule
        // (infer's SpreadRecord = base passthrough). Without this arm a
        // cross-module spread base whose type lands late (module top-lets
        // are checked AFTER main) bottomed out at `_ => None` and was
        // refused by the AllTypesConcrete gate (#502).
        IrExprKind::SpreadRecord { base, .. } => {
            let base_ty = effective_ty(base, vt);
            if !base_ty.has_unresolved_deep() { Some(base_ty) } else { None }
        }
        _ => None,
    }
}

/// `resolve_node_ty` group: control-flow / operator / literal nodes (BinOp,
/// UnOp, Block, If, Match, Lambda, literals, StringInterp).
fn resolve_node_ty_control(expr: &IrExpr, vt: &VarTable) -> Option<Ty> {
    match &expr.kind {
        IrExprKind::BinOp { op, left, right } => {
            op.result_ty().or_else(|| if_concrete(&left.ty).or_else(|| if_concrete(&right.ty)))
        }
        // Most UnOps (Neg, Not, Minus) preserve operand type
        IrExprKind::UnOp { operand, .. } => if_concrete(&operand.ty),
        IrExprKind::Block { expr: Some(tail), .. } => if_concrete(&tail.ty),
        IrExprKind::If { then, else_, .. } => if_concrete(&then.ty).or_else(|| if_concrete(&else_.ty)),
        IrExprKind::Match { arms, .. } => {
            arms.iter().find_map(|arm| if_concrete(&arm.body.ty))
        }
        IrExprKind::Lambda { params, body, .. } => resolve_lambda_ty(params, body),
        IrExprKind::LitInt { .. } => Some(Ty::Int),
        IrExprKind::LitFloat { .. } => Some(Ty::Float),
        IrExprKind::LitBool { .. } => Some(Ty::Bool),
        IrExprKind::LitStr { .. } => Some(Ty::String),
        IrExprKind::Unit => Some(Ty::Unit),
        // StringInterp always produces String
        IrExprKind::StringInterp { .. } => Some(Ty::String),
        _ => {
            let _ = vt; // kept for signature symmetry with the other groups
            None
        }
    }
}

/// `resolve_node_ty` group: container / layout-transparent-wrapper / call
/// nodes (List, Tuple, OptionSome, Clone, Deref/BoxNew/ToVec/Borrow/Await,
/// Range, ResultOk, Call, RuntimeCall).
fn resolve_node_ty_container(expr: &IrExpr, vt: &VarTable, symbols: &SymbolTable) -> Option<Ty> {
    match &expr.kind {
        IrExprKind::List { elements } => {
            // List[T] where T = first element's type
            elements.first()
                .and_then(|e| if_concrete(&e.ty))
                .map(|t| Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, vec![t]))
        }
        IrExprKind::Tuple { elements } => {
            let elem_tys: Vec<Ty> = elements.iter().map(|e| e.ty.clone()).collect();
            if elem_tys.iter().any(Ty::has_unresolved_deep) { None }
            else { Some(Ty::Tuple(elem_tys)) }
        }
        // `some(x)` has type `Option[x.ty]`; recover when the type
        // checker left an `Option[Unknown]` placeholder (typical for
        // payloads built from pattern-bound names).
        IrExprKind::OptionSome { expr } => if_concrete(&expr.ty)
            .map(|t| Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, vec![t])),
        // Clone preserves the inner type
        IrExprKind::Clone { expr } => if_concrete(&expr.ty),
        // Layout-transparent codegen wrappers: the node's value type is the
        // inner expression's type. `*box` (Deref), `Box::new(x)` (BoxNew),
        // `(x).to_vec()` (ToVec), `&x` / `&*x` (Borrow), and `await x` all
        // carry the same Almide-level `Ty` as their operand — the wrapper is a
        // representation detail the emit layer applies, not a type change. After
        // the bottom-up walk the operand is concrete, so we can pull its type up.
        // Each makes one more shape resolvable instead of bottoming out at the
        // `_ => None` arm and surfacing as an audit violation.
        IrExprKind::Deref { expr }
        | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr }
        | IrExprKind::Borrow { expr, .. }
        | IrExprKind::Await { expr } => if_concrete(&expr.ty),
        // Range produces List[Int]
        IrExprKind::Range { .. } => Some(Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::List, vec![Ty::Int],
        )),
        // ResultOk wraps in Result[T, E]
        IrExprKind::ResultOk { expr } => if_concrete(&expr.ty)
            .map(|t| Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, vec![t, Ty::String])),
        IrExprKind::Call { target, args, .. } => resolve_call_ret_ty(target, args, vt, symbols),
        IrExprKind::RuntimeCall { symbol, args } => {
            // Post-IntrinsicLowering, the `Call { target: Module }` node
            // has been rewritten to RuntimeCall. Rebuild a synthetic
            // `Named { symbol }` target so the existing stdlib
            // polymorphic logic (list.map / list.zip / ...) keeps
            // firing for post-lowering shape.
            let target = CallTarget::Named { name: *symbol };
            resolve_call_ret_ty(&target, args, vt, symbols)
        }
        _ => None,
    }
}
