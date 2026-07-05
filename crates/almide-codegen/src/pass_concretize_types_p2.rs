
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
        // `map.from_list` canonicalizes to `map.from_entries`, and by the WASM
        // emit passes it is a `RuntimeCall` (`almide_rt_map_from_entries`), not a
        // `Module` call — match both forms for each module (set keeps `from_list`).
        let from_list_kind = |module: &str, func: &str| -> (bool, bool) {
            let fl = func == "from_list" || func == "from_entries";
            (module == "map" && fl, module == "set" && fl)
        };
        let (is_map, is_set) = match &value.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } =>
                from_list_kind(module.as_str(), func.as_str()),
            IrExprKind::RuntimeCall { symbol, .. } => {
                let s = symbol.as_str();
                (s.contains("map_from_entries") || s.contains("map_from_list"),
                 s.contains("set_from_entries") || s.contains("set_from_list"))
            }
            _ => return,
        };
        let elem = if is_map {
            match ret { Ty::Applied(TCI::Map, kv) if kv.len() == 2 => Ty::Tuple(vec![kv[0].clone(), kv[1].clone()]), _ => return }
        } else if is_set {
            match ret { Ty::Applied(TCI::Set, e) if e.len() == 1 => e[0].clone(), _ => return }
        } else { return };
        let list_ty = Ty::Applied(TCI::List, vec![elem]);
        let args = match &mut value.kind {
            IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } => args,
            _ => return,
        };
        {
            if args.len() != 1 || !args[0].ty.has_unresolved_deep() { return; }
            propagate_expected_ty(&mut args[0], &list_ty);
            // Walk through wrappers to the Var, pinning each ty and the VarTable.
            let mut node = &mut args[0];
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
    }
}

impl<'a> IrMutVisitor for Concretizer<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Custom Match handling: propagate subject ty into pattern bindings
        // (updating both the pattern's declared ty and the VarTable entry)
        // BEFORE visiting arm bodies, so Var references to pattern-bound
        // names pick up the refreshed ty during the bottom-up walk.
        if let IrExprKind::Match { subject, arms } = &mut expr.kind {
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
            return;
        }

        // Recurse into children FIRST (bottom-up) so nested types are
        // concrete before we use them here.
        walk_expr_mut(self, expr);

        // Resolve Unknown lambda params from body usage (e.g. `(a,b) => a + b` → Int)
        if let IrExprKind::Lambda { params, body, .. } = &mut expr.kind {
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

        // Record literal construction: push the declared field types from
        // the registered type down into field value expressions whose own
        // inference left them unresolved (typically `Applied(List,
        // [Unknown])` for a field defaulted to `[]`). The checker sees
        // `items: []` and can only type it `List[Unknown]`; we know from
        // the record decl that `items: List[Int]`, so substitute.
        if let IrExprKind::Record { name: Some(name), fields } = &mut expr.kind {
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

        // Generic-accumulator back-propagation for `list.fold` / `list.scan`:
        // both `init` arg and lambda `body.ty` represent the accumulator A.
        // After the bottom-up walk, body.ty may be strictly more concrete
        // than init.ty (because init started from a literal like `some([])`
        // whose empty list has element type Unknown). Merge, push the
        // merged shape back into init's sub-expressions, and update the
        // lambda's acc param + VarTable so arm Var refs refresh on the
        // re-visit below.
        if is_fold_like_call(expr) {
            if back_propagate_fold_acc(expr, self.vt) {
                // Re-visit the lambda body so pattern bindings and Var
                // references pick up the refreshed acc type.
                if let IrExprKind::Call { args, .. } = &mut expr.kind {
                    if let Some(lambda) = args.get_mut(2) {
                        if let IrExprKind::Lambda { body, .. } = &mut lambda.kind {
                            self.visit_expr_mut(body);
                        }
                    }
                }
            }
        }

        // Now resolve this node's type from child types + VarTable + symbols.
        if (expr.ty).has_unresolved_deep() {
            if let Some(ty) = resolve_node_ty(expr, self.vt, self.symbols) {
                expr.ty = ty.clone();
                // Propagate: if this was IndexAccess and it resolved, the
                // parent Member can now resolve too. But we're bottom-up,
                // so Member visits AFTER this. Make sure we updated expr.ty.
            }
        }
        // Second chance for Member: debug why it fails
        if (expr.ty).has_unresolved_deep() {
            if let IrExprKind::Member { object, field } = &expr.kind {
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

fn resolve_node_ty(expr: &IrExpr, vt: &VarTable, symbols: &SymbolTable) -> Option<Ty> {
    match &expr.kind {
        IrExprKind::Var { id } => {
            let vt_ty = &vt.get(*id).ty;
            if !vt_ty.has_unresolved_deep() { Some(vt_ty.clone()) } else { None }
        }
        IrExprKind::EnvLoad { env_var, .. } => {
            let vt_ty = &vt.get(*env_var).ty;
            if !vt_ty.has_unresolved_deep() { Some(vt_ty.clone()) } else { None }
        }
        IrExprKind::TupleIndex { object, index } => {
            let obj_ty = effective_ty(object, vt);
            if let Ty::Tuple(elems) = &obj_ty {
                elems.get(*index).cloned().filter(|t| !t.has_unresolved_deep())
            } else { None }
        }
        IrExprKind::Member { object, field } => {
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
        IrExprKind::BinOp { op, left, right } => {
            op.result_ty().or_else(|| {
                if !(left.ty).has_unresolved_deep() { Some(left.ty.clone()) }
                else if !(right.ty).has_unresolved_deep() { Some(right.ty.clone()) }
                else { None }
            })
        }
        IrExprKind::UnOp { operand, .. } => {
            // Most UnOps (Neg, Not, Minus) preserve operand type
            if !(operand.ty).has_unresolved_deep() { Some(operand.ty.clone()) } else { None }
        }
        IrExprKind::Block { expr: Some(tail), .. } => {
            if !(tail.ty).has_unresolved_deep() { Some(tail.ty.clone()) } else { None }
        }
        IrExprKind::If { then, else_, .. } => {
            if !(then.ty).has_unresolved_deep() { Some(then.ty.clone()) }
            else if !(else_.ty).has_unresolved_deep() { Some(else_.ty.clone()) }
            else { None }
        }
        IrExprKind::Match { arms, .. } => {
            arms.iter()
                .find_map(|arm| if !(arm.body.ty).has_unresolved_deep() { Some(arm.body.ty.clone()) } else { None })
        }
        IrExprKind::Lambda { params, body, .. } => {
            let fparams: Vec<Ty> = params.iter().map(|(_, t)| t.clone()).collect();
            if fparams.iter().any(Ty::has_unresolved_deep) || (body.ty).has_unresolved_deep() {
                return None;
            }
            Some(Ty::Fn {
                params: fparams,
                ret: Box::new(body.ty.clone()),
            })
        }
        IrExprKind::IndexAccess { object, .. } => {
            // For List[T], result is T. Use effective_ty to resolve through VarTable.
            let obj_ty = effective_ty(object, vt);
            if obj_ty.has_unresolved_deep() {
            }
            if let Ty::Applied(_, args) = &obj_ty {
                args.first().cloned().filter(|t| !t.has_unresolved_deep())
            } else { None }
        }
        IrExprKind::List { elements } => {
            // List[T] where T = first element's type
            elements.first()
                .and_then(|e| if !(e.ty).has_unresolved_deep() { Some(e.ty.clone()) } else { None })
                .map(|t| Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, vec![t]))
        }
        IrExprKind::Tuple { elements } => {
            let elem_tys: Vec<Ty> = elements.iter().map(|e| e.ty.clone()).collect();
            if elem_tys.iter().any(Ty::has_unresolved_deep) { None }
            else { Some(Ty::Tuple(elem_tys)) }
        }
        IrExprKind::OptionSome { expr } => {
            // `some(x)` has type `Option[x.ty]`; recover when the type
            // checker left an `Option[Unknown]` placeholder (typical for
            // payloads built from pattern-bound names).
            if expr.ty.has_unresolved_deep() { None }
            else {
                Some(Ty::Applied(
                    almide_lang::types::constructor::TypeConstructorId::Option,
                    vec![expr.ty.clone()],
                ))
            }
        }
        IrExprKind::LitInt { .. } => Some(Ty::Int),
        IrExprKind::LitFloat { .. } => Some(Ty::Float),
        IrExprKind::LitBool { .. } => Some(Ty::Bool),
        IrExprKind::LitStr { .. } => Some(Ty::String),
        IrExprKind::Unit => Some(Ty::Unit),
        // StringInterp always produces String
        IrExprKind::StringInterp { .. } => Some(Ty::String),
        // Clone preserves the inner type
        IrExprKind::Clone { expr } => {
            if !expr.ty.has_unresolved_deep() { Some(expr.ty.clone()) } else { None }
        }
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
        | IrExprKind::Await { expr } => {
            if !expr.ty.has_unresolved_deep() { Some(expr.ty.clone()) } else { None }
        }
        // Range produces List[Int]
        IrExprKind::Range { .. } => Some(Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::List, vec![Ty::Int],
        )),
        // MapAccess: Map[K,V] → Option[V]
        IrExprKind::MapAccess { object, .. } => {
            let obj_ty = effective_ty(object, vt);
            if let Ty::Applied(_, args) = &obj_ty {
                args.get(1).cloned()
                    .filter(|t| !t.has_unresolved_deep())
                    .map(|v| Ty::Applied(
                        almide_lang::types::constructor::TypeConstructorId::Option, vec![v],
                    ))
            } else { None }
        }
        // ResultOk wraps in Result[T, E]
        IrExprKind::ResultOk { expr } => {
            if !expr.ty.has_unresolved_deep() {
                Some(Ty::Applied(
                    almide_lang::types::constructor::TypeConstructorId::Result,
                    vec![expr.ty.clone(), Ty::String],
                ))
            } else { None }
        }
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
