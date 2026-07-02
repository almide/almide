// `infer_expr_inner` group 3 — blocks, calls, pipes, closures, loops, the
// Option/Result constructor & postfix-operator arms, and map literals (Block …
// TypeAscription, minus the `return`-bearing arms kept in the dispatcher).
// Plus the smaller extracted inference / call-resolution helpers. Disjoint
// from groups 1 & 2; see `infer_expr_inner`. `include!`d into `infer.rs`, so
// imports come from there.

impl Checker {
    pub(super) fn infer_expr_inner_g3(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        Some(match &mut expr.kind {
            ExprKind::Block { stmts, expr, .. } => {
                self.env.push_scope();
                // Pre-scan for vars used as match subjects with Ok/Err
                // patterns — those bindings must keep their Result type.
                let saved_skip = std::mem::take(&mut self.env.skip_auto_unwrap_for);
                let result_match_vars = collect_block_result_match_vars(stmts, expr.as_deref());
                for n in &result_match_vars {
                    self.env.skip_auto_unwrap_for.insert(*n);
                }
                for stmt in stmts.iter_mut() { self.check_stmt(stmt); }
                let ty = if let Some(e) = expr { self.infer_expr(e) } else { Ty::Unit };
                self.env.pop_scope();
                self.env.skip_auto_unwrap_for = saved_skip;
                ty
            }

            ExprKind::Fan { exprs, .. } => {
                if !self.env.can_call_effect {
                    self.emit(super::err(
                        "fan block can only be used inside an effect fn".to_string(),
                        "Mark the enclosing function as `effect fn`",
                        "fan block".to_string()).with_code("E007"));
                }
                // Check for mutable variable capture
                let mutable_captures: Vec<String> = exprs.iter().flat_map(|e| {
                    let mut idents = Vec::new();
                    collect_idents(e, &mut idents);
                    idents.into_iter().filter(|name| self.env.mutable_vars.contains(&sym(name))).collect::<Vec<_>>()
                }).collect();
                for name in &mutable_captures {
                    self.emit(super::err(
                        format!("cannot capture mutable variable '{}' inside fan block", name),
                        "Use a `let` binding instead of `var` for values shared across fan expressions",
                        "fan block".to_string()).with_code("E008"));
                }
                let tys: Vec<Ty> = exprs.iter_mut().map(|e| {
                    let ty = self.infer_expr(e);
                    // Auto-unwrap Result: fan unwraps Result<T, E> to T
                    let concrete = resolve_ty(&ty, &self.uf);
                    match &concrete {
                        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                        _ => ty,
                    }
                }).collect();
                match tys.len() {
                    1 => tys.into_iter().next().unwrap(),
                    _ => Ty::Tuple(tys.iter().map(|t| resolve_ty(t, &self.uf)).collect()),
                }
            }

            ExprKind::Call { callee, args, named_args, type_args, .. } => {
                // Publish the outer Call's span so UFCS / whole-expr
                // rewrites (E002 method-UFCS, E013 no-field) can emit
                // a `try_replace` range covering `callee(args)` in
                // full, not just the callee reference. Nested calls
                // save/restore the previous value.
                let prev_call = self.call_span_hint.take();
                self.call_span_hint = expr.span;
                let ty = self.infer_call(callee, args, named_args, type_args);
                self.call_span_hint = prev_call;
                // A generic collection constructor whose element type NO argument
                // constrains — `set.new()` / `list.with_capacity(n)` — must have
                // its element pinned by context (annotation / later use). Register
                // it for the post-solve undecidable-empty-collection check (E018).
                if let Some(kind) = empty_collection_ctor_kind(callee) {
                    self.register_empty_collection(ty.clone(), kind);
                }
                ty
            }

            ExprKind::Pipe { left, right, .. } => {
                self.infer_pipe(left, right)
            }

            ExprKind::Compose { left, right, .. } => {
                let left_ty = self.infer_expr(left);
                let right_ty = self.infer_expr(right);
                // If left is Fn[A] -> B and right is Fn[B] -> C, result is Fn[A] -> C
                let resolved_left = resolve_ty(&left_ty, &self.uf);
                let resolved_right = resolve_ty(&right_ty, &self.uf);
                match (&resolved_left, &resolved_right) {
                    (Ty::Fn { params: a_params, .. }, Ty::Fn { ret: c_ret, .. }) => {
                        Ty::Fn { params: a_params.clone(), ret: c_ret.clone() }
                    }
                    _ => Ty::Unknown,
                }
            }

            ExprKind::Lambda { params, body, .. } => {
                self.env.push_scope();
                // Lambda has its own return context — don't leak outer function's current_ret
                let saved_ret = self.env.current_ret.take();
                // A lambda is its own function: the enclosing effect fn's
                // auto-`?` cannot propagate out of a closure body (the closure
                // may escape), so an effect call inside a lambda yields the
                // EXPLICIT Result — auto_unwrap is off, matching the lowering,
                // which never inserts `?` inside Lambda bodies (#489).
                let saved_auto_unwrap = self.env.auto_unwrap;
                self.env.auto_unwrap = false;
                self.env.lambda_depth += 1;
                // Expected-type hint from the enclosing call (#653): when this
                // lambda is an argument whose parameter slot is a `Fn`, the
                // caller pins each UNANNOTATED param to the expected element
                // type (e.g. `T` carrying a protocol bound) so the body resolves
                // method calls on the param via the protocol path instead of
                // collapsing it into a closure type. An explicit annotation on
                // the param always wins; the hint only fills inferred slots.
                let param_hint = self.lambda_arg_hint.take();
                let param_tys: Vec<Ty> = params.iter().enumerate().map(|(i, p)| {
                    let ty = p.ty.as_ref().map(|te| self.resolve_type_expr(te))
                        .or_else(|| param_hint.as_ref().and_then(|h| h.get(i).cloned().flatten()))
                        .unwrap_or_else(|| self.fresh_var());
                    let concrete = resolve_ty(&ty, &self.uf);
                    self.env.define_var(&p.name, concrete);
                    ty
                }).collect();
                let ret_ty = self.infer_expr(body);
                self.env.lambda_depth -= 1;
                self.env.auto_unwrap = saved_auto_unwrap;
                self.env.current_ret = saved_ret;
                self.env.pop_scope();
                Ty::Fn { params: param_tys, ret: Box::new(ret_ty) }
            }

            ExprKind::ForIn { var, var_tuple, iterable, body, .. } => {
                self.infer_for_in(var, var_tuple, iterable, body)
            }

            ExprKind::While { cond, body, .. } => {
                self.infer_expr(cond);
                self.env.push_scope();
                for stmt in body.iter_mut() { self.check_stmt(stmt); }
                self.env.pop_scope();
                Ty::Unit
            }

            ExprKind::Range { start, end, .. } => { let st = self.infer_expr(start); self.infer_expr(end); Ty::list(st) }

            ExprKind::Some { expr, .. } => { let inner = self.infer_expr(expr); Ty::option(inner) }
            ExprKind::Ok { expr, .. } => {
                let ok_ty = self.infer_expr(expr);
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[1].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            ExprKind::Err { expr, .. } => {
                let err_ty = self.infer_expr(expr);
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[0].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            ExprKind::Try { expr, .. } => {
                let ty = self.infer_expr(expr);
                match &ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() >= 1 => args[0].clone(),
                    _ => ty,
                }
            }

            ExprKind::Paren { expr, .. } => self.infer_expr(expr),
            ExprKind::Break | ExprKind::Continue => Ty::Unit,
            ExprKind::Hole | ExprKind::Todo { .. } => self.fresh_var(),
            ExprKind::Await { expr, .. } => self.infer_expr(expr),

            // expr! — unwrap with propagation (Option[T] → T, Result[T,E] → T)
            ExprKind::Unwrap { expr: inner, .. } => {
                let t = self.infer_expr(inner);
                let resolved = resolve_ty(&t, &self.uf);
                self.check_unwrap_propagation_context();
                if let Some(inner_ty) = resolved.option_inner().or_else(|| resolved.result_ok_ty()) {
                    inner_ty
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    self.fresh_var()
                } else {
                    self.emit(super::err(
                        format!("operator '!' requires Option or Result type but got {}", resolved.display()),
                        "Use '!' only on Option[T] or Result[T, E] values",
                        "operator !",
                    ));
                    Ty::Unknown
                }
            }
            // expr ?? fallback — unwrap with default (Option[T] → T, Result[T,E] → T)
            ExprKind::UnwrapOr { expr: inner, fallback, .. } => {
                let t = self.infer_expr(inner);
                let ft = self.infer_expr(fallback);
                let resolved = resolve_ty(&t, &self.uf);
                let inner_ty = if let Some(ty) = resolved.option_inner().or_else(|| resolved.result_ok_ty()) {
                    ty
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    ft.clone()
                } else {
                    self.emit(super::err(
                        format!("operator '??' requires Option or Result type but got {}", resolved.display()),
                        "Use '??' only on Option[T] or Result[T, E] values",
                        "operator ??",
                    ));
                    ft.clone()
                };
                self.unify_infer(&inner_ty, &ft);
                inner_ty
            }
            // expr? — to Option (Result[T,E] → Option[T], Option[T] → Option[T])
            ExprKind::ToOption { expr: inner, .. } => {
                let t = self.infer_expr(inner);
                let resolved = resolve_ty(&t, &self.uf);
                if let Some(ok_ty) = resolved.result_ok_ty() {
                    Ty::option(ok_ty)
                } else if resolved.is_option() {
                    resolved.clone()
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    Ty::option(self.fresh_var())
                } else {
                    self.emit(super::err(
                        format!("operator '?' requires Option or Result type but got {}", resolved.display()),
                        "Use '?' only on Option[T] or Result[T, E] values",
                        "operator ?",
                    ));
                    Ty::Unknown
                }
            }
            ExprKind::Error | ExprKind::Placeholder => Ty::Unknown,

            ExprKind::MapLiteral { entries, .. } => {
                if entries.is_empty() {
                    let ty = Ty::map_of(self.fresh_var(), self.fresh_var());
                    self.register_empty_collection(ty.clone(), super::EmptyCollectionKind::MapLiteral);
                    ty
                }
                else {
                    let kt = self.infer_expr(&mut entries[0].0);
                    let vt = self.infer_expr(&mut entries[0].1);
                    for entry in entries.iter_mut().skip(1) { self.infer_expr(&mut entry.0); self.infer_expr(&mut entry.1); }
                    self.deferred_map_key_checks.push((kt.clone(), self.current_span));
                    Ty::map_of(kt, vt)
                }
            }
            ExprKind::EmptyMap => {
                let ty = Ty::map_of(self.fresh_var(), self.fresh_var());
                self.register_empty_collection(ty.clone(), super::EmptyCollectionKind::MapLiteral);
                ty
            }

            ExprKind::TypeAscription { expr, ty } => {
                let inferred = self.infer_expr(expr);
                let ascribed = self.resolve_type_expr(ty);
                self.constrain(ascribed.clone(), inferred, "type ascription");
                ascribed
            }
            _ => return None,
        })
    }

    // ── Extracted inference helpers ──

    fn infer_call(
        &mut self,
        callee: &mut Box<ast::Expr>,
        args: &mut Vec<ast::Expr>,
        named_args: &mut Vec<(almide_base::intern::Sym, ast::Expr)>,
        type_args: &Option<Vec<ast::TypeExpr>>,
    ) -> Ty {
        // Save named arg names, then flatten into positional args temporarily.
        let named_names: Vec<almide_base::intern::Sym> = named_args.iter().map(|(n, _)| *n).collect();
        let named_start = args.len();
        args.extend(std::mem::take(named_args).into_iter().map(|(_, e)| e));
        let resolved_type_args: Option<Vec<crate::types::Ty>> = type_args.as_ref().map(|tas|
            tas.iter().map(|te| self.resolve_type_expr(te)).collect());
        // #558: hand the named-arg shape to check_named_call so it validates
        // by NAME (matching lowering), not by the appended positional slot.
        self.named_arg_meta = if named_names.is_empty() { None }
            else { Some((named_start, named_names.clone())) };
        let ret = self.check_call_with_type_args(callee, args, resolved_type_args.as_deref());
        self.named_arg_meta = None;
        // Restore named args
        let named_exprs: Vec<ast::Expr> = args.drain(named_start..).collect();
        *named_args = named_names.into_iter().zip(named_exprs).collect();
        ret
    }

    /// `expr!` propagates the unwrapped error: lowering renders it as `?`
    /// (effect fn body) or `.unwrap()` (test block). In any other context the
    /// generated `?` lands in a function/closure that does not return Result,
    /// which is invalid Rust and a wasm build failure — yet the type checker
    /// previously accepted it (the `Result/Option → T` rule alone). Error
    /// propagation is possible exactly where `auto_unwrap` is live (an effect
    /// fn body, outside any lambda) or inside a `test` block; reject everywhere
    /// else at type-check time so the failure is a clear diagnostic, not a
    /// codegen ICE (#608).
    fn check_unwrap_propagation_context(&mut self) {
        if self.env.auto_unwrap || self.env.in_test_block {
            return;
        }
        // Inside a lambda within an effect fn the call site *looks* effectful,
        // but `?` cannot propagate out of the closure (#489) — point there
        // specifically; otherwise the fn just needs to be `effect fn`.
        let hint = if self.env.can_call_effect && self.env.lambda_depth > 0 {
            "`!` cannot propagate an error out of a lambda; use `??` for a fallback value or move the call out of the closure"
        } else {
            "Mark the enclosing function as `effect fn`, or use `??` to provide a fallback value"
        };
        self.emit(super::err(
            "operator '!' propagates errors and is only valid inside an `effect fn` body or a `test` block".to_string(),
            hint,
            "operator !",
        ).with_code("E022"));
    }

    fn infer_pipe(&mut self, left: &mut Box<ast::Expr>, right: &mut Box<ast::Expr>) -> Ty {
        // Unwrap postfix operators (??, !, ?) on the RHS so the pipe targets the inner Call.
        // e.g. `xs |> list.find(pred) ?? fallback` → pipe into list.find, then apply ??
        match &mut right.kind {
            ExprKind::UnwrapOr { expr: inner, fallback, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                let fb_ty = self.infer_expr(fallback);
                self.unify_infer(&inner_ty, &fb_ty);
                // UnwrapOr unwraps Option[T]/Result[T,E] → T
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    _ => inner_ty,
                }
            }
            ExprKind::Unwrap { expr: inner, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                self.check_unwrap_propagation_context();
                // Annotate the inner expression with its resolved type so the lowering
                // can construct the correct IR type (e.g., Result[List[T], List[E]] for
                // result.collect rather than hardcoding Result[T, String]).
                self.type_map.insert(inner.id, inner_ty.clone());
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    _ => inner_ty,
                }
            }
            ExprKind::Try { expr: inner, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 =>
                        Ty::Applied(TypeConstructorId::Option, vec![args[0].clone()]),
                    _ => Ty::Applied(TypeConstructorId::Option, vec![inner_ty]),
                }
            }
            _ => self.infer_pipe_direct(left, right),
        }
    }

    fn infer_pipe_direct(&mut self, left: &mut Box<ast::Expr>, right: &mut Box<ast::Expr>) -> Ty {
        let left_ty = self.infer_expr(left);
        // Resolve TypeVars eagerly via UnionFind — earlier pipes in the chain
        // have already been unified (constrain() calls unify_infer immediately),
        // so the concrete type is available now. Without this, chained UFCS like
        // `xs |> list.map(f) |> list.join(",")` sees a raw TypeVar for the
        // intermediate result, causing module resolution to fail.
        let left_ty = super::types::resolve_ty(&left_ty, &self.uf);
        match &mut right.kind {
            ExprKind::Call { callee, args, .. } => {
                // Pipe inserts left as the first argument
                let mut all_arg_tys: Vec<Ty> = vec![left_ty];
                all_arg_tys.extend(args.iter_mut().map(|a| self.infer_expr(a)));
                // Resolve module calls for pipe (e.g. xs |> list.filter(f))
                match &mut callee.kind {
                    ExprKind::Ident { name, .. } => self.check_named_call(name, &all_arg_tys),
                    ExprKind::Member { object, field, .. } => {
                        let module_key = self.resolve_module_call(object, field);
                        if let Some(key) = module_key {
                            return self.check_named_call(&key, &all_arg_tys);
                        }
                        let ct = self.infer_expr(callee);
                        let ret = self.fresh_var();
                        self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                        ret
                    }
                    _ => {
                        let ct = self.infer_expr(callee);
                        let ret = self.fresh_var();
                        self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                        ret
                    }
                }
            }
            // Pipe RHS is a bare function name (e.g. `5 |> double`)
            ExprKind::Ident { name, .. } => {
                let all_arg_tys = vec![left_ty];
                self.check_named_call(name, &all_arg_tys)
            }
            // Pipe RHS is a module-qualified function (e.g. `5 |> int.abs`)
            ExprKind::Member { object, field, .. } => {
                let all_arg_tys = vec![left_ty];
                if let Some(key) = self.resolve_module_call(object, field) {
                    return self.check_named_call(&key, &all_arg_tys);
                }
                let ct = self.infer_expr(right);
                let ret = self.fresh_var();
                self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                ret
            }
            _ => {
                let rt = self.infer_expr(right);
                let ret = self.fresh_var();
                self.constrain(rt, Ty::Fn { params: vec![left_ty], ret: Box::new(ret.clone()) }, "pipe call");
                ret
            }
        }
    }

    fn infer_for_in(
        &mut self,
        var: &str,
        var_tuple: &Option<Vec<almide_base::intern::Sym>>,
        iterable: &mut Box<ast::Expr>,
        body: &mut Vec<ast::Stmt>,
    ) -> Ty {
        // An empty-list iterable (`for _ in []`) registers a generic ListLiteral
        // site via `infer_expr` below; retag it as `ForInEmpty` so the E018 hint
        // suggests the for-position fix `for _ in ([] : List[Int])` rather than a
        // `let`-binding example.
        let iterable_is_empty_list = matches!(&iterable.kind,
            ExprKind::List { elements, .. } if elements.is_empty());
        let iter_ty = self.infer_expr(iterable);
        if iterable_is_empty_list {
            if let Some(last) = self.deferred_empty_collection_checks.last_mut() {
                last.kind = super::EmptyCollectionKind::ForInEmpty;
            }
        }
        self.env.push_scope();
        let iter_resolved = resolve_ty(&iter_ty, &self.uf);
        let elem_ty = match &iter_resolved {
            Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
            Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::Tuple(vec![args[0].clone(), args[1].clone()]),
            _ => Ty::Unknown,
        };
        if let Some(names) = var_tuple {
            // Destructure tuple: for (a, b) in xs
            if let Ty::Tuple(tys) = &elem_ty {
                for (i, n) in names.iter().enumerate() {
                    self.env.define_var(n, tys.get(i).cloned().unwrap_or(Ty::Unknown));
                }
            } else {
                for n in names { self.env.define_var(n, Ty::Unknown); }
            }
        } else {
            self.env.define_var(var, elem_ty);
        }
        for stmt in body.iter_mut() { self.check_stmt(stmt); }
        self.env.pop_scope();
        Ty::Unit
    }

    // ── Statement checking ──

    /// Reject a binding whose type uses a function in a position that demands
    /// equality/hashing: a `Set` element or a `Map` key. Closures have neither,
    /// so such a type is meaningless — and the two targets disagree (native
    /// rustc rejects it, WASM silently drops the inserts). Closures are fine as
    /// `Map` *values*.
    pub(crate) fn check_collection_element_types(&mut self, ty: &Ty) {
        let resolved = resolve_ty(ty, &self.uf);
        if let Some((msg, hint)) = invalid_collection_type(&resolved) {
            self.emit(super::err(msg, hint, "collection element type").with_code("E016"));
        }
    }

    /// Record an empty-collection producer to re-check after constraint solving
    /// (the undecidable-empty-collection / E018 rule). The current span is
    /// captured now; the element type is verified post-solve in
    /// [`Checker::validate_empty_collection_elements`].
    pub(crate) fn register_empty_collection(&mut self, ty: Ty, kind: super::EmptyCollectionKind) {
        self.deferred_empty_collection_checks.push(super::EmptyCollectionSite {
            ty,
            kind,
            span: self.current_span,
        });
    }

    /// #488: classify a `TypeName(...)` call. All-named args on a record
    /// type or record-payload variant case rewrite the node in place to the
    /// brace `ExprKind::Record` form (one construction pipeline, both
    /// spellings); positional args on those, or named args on a tuple
    /// constructor, are E021. Returns true when the node was rewritten.
    fn normalize_ctor_paren_call(&mut self, expr: &mut ast::Expr) -> bool {
        let ExprKind::Call { callee, args, named_args, .. } = &expr.kind else { return false };
        // Both spellings of a constructor callee: bare/dotted `TypeName`, and
        // the cross-module `m.Cfg(...)` form, which parses as a MEMBER access
        // on the module ident — without this arm the paren-named normalization
        // only covered the same-file spelling (caught by the §2 matrix gate).
        let n = match &callee.kind {
            ExprKind::TypeName { name } => *name,
            ExprKind::Member { object, field }
                if field.as_str().chars().next().map_or(false, |c| c.is_uppercase()) =>
            {
                let ExprKind::Ident { name: obj, .. } = &object.kind else { return false };
                sym(&format!("{}.{}", obj, field))
            }
            _ => return false,
        };
        let bare = n.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(n);
        // Record-payload variant case? (ctor table is keyed by bare name)
        let ctor_payload_record = self.env.lookup_ctor_in(&bare, self.current_module_prefix.as_deref())
            .map(|(_, case)| matches!(case.payload, crate::types::VariantPayload::Record(_)));
        // Record TYPE? (resolve through the same canonicalization annotations use)
        let is_record_type = ctor_payload_record.is_none() && {
            let key = match n.as_str().rsplit_once('.') {
                Some(_) => sym(n.as_str()),
                None => crate::canonicalize::resolve::canonical_user_type_sym(
                    n.as_str(), &self.env.types, self.current_module_prefix.as_deref(),
                ).unwrap_or(n),
            };
            matches!(self.env.resolve_named(&Ty::Named(key, vec![])), Ty::Record { .. } | Ty::OpenRecord { .. })
        };
        if ctor_payload_record == Some(true) || is_record_type {
            if !args.is_empty() {
                self.emit(super::err(
                    format!("'{}' takes named fields, not positional arguments", n),
                    format!("Name every field: `{}(field: value, ...)` or `{} {{ field: value, ... }}`", n, n),
                    format!("constructor {}(...)", n),
                ).with_code("E021"));
                return false;
            }
            // Rewrite to the brace form in place; re-inference routes it
            // through the Record arm (defaults, field validation, #433
            // qualification, both backends' Record emission — for free).
            let ExprKind::Call { named_args, .. } = std::mem::replace(&mut expr.kind, ExprKind::Unit) else { unreachable!() };
            let fields = named_args.into_iter()
                .map(|(fname, value)| ast::FieldInit { name: fname, value })
                .collect();
            expr.kind = ExprKind::Record { name: Some(n), fields };
            return true;
        }
        if ctor_payload_record == Some(false) && !named_args.is_empty() {
            self.emit(super::err(
                format!("constructor '{}' takes positional arguments, not named ones", n),
                format!("Drop the names: `{}(value, ...)`", n),
                format!("constructor {}(...)", n),
            ).with_code("E021"));
        }
        false
    }

    /// #488: validate a record construction's field set against the declared
    /// fields: duplicates always; unknown + missing-without-default when the
    /// declaration is CLOSED (a plain record or a record-payload case).
    fn validate_record_fields(
        &mut self,
        type_label: &str,
        given: &[ast::FieldInit],
        decl_fields: &[(Sym, Ty)],
        closed: bool,
        defaults: &std::collections::HashSet<Sym>,
    ) {
        let mut seen: std::collections::HashSet<Sym> = std::collections::HashSet::new();
        for f in given {
            if !seen.insert(f.name) {
                self.emit(super::err(
                    format!("field '{}' given more than once in '{}' construction", f.name, type_label),
                    "Remove the duplicate field",
                    format!("record literal {}", type_label),
                ).with_code("E021"));
            }
        }
        if !closed { return; }
        let available = || decl_fields.iter().map(|(d, _)| d.as_str()).collect::<Vec<_>>().join(", ");
        for f in given {
            if !decl_fields.iter().any(|(d, _)| *d == f.name) {
                self.emit(super::err(
                    format!("'{}' has no field '{}'", type_label, f.name),
                    format!("Available fields: {}", available()),
                    format!("record literal {}", type_label),
                ).with_code("E021"));
            }
        }
        for (d, _) in decl_fields {
            if !given.iter().any(|f| f.name == *d) && !defaults.contains(d) {
                self.emit(super::err(
                    format!("missing field '{}' in '{}' construction", d, type_label),
                    format!("Provide it: `{} {{ {}: ..., ... }}` (fields without defaults are required)", type_label, d),
                    format!("record literal {}", type_label),
                ).with_code("E021"));
            }
        }
    }

    /// The effect-fn auto-unwrap rule, shared by every binding-shaped
    /// position (let / var / assign): a Result[T, E]-typed RHS unwraps to T
    /// — the lowering inserts the matching `?` — unless the target itself
    /// keeps the Result (declared Result annotation, Result-typed var, or a
    /// usage-skip like `match x { ok/err }`). One function so the positions
    /// can never diverge again (#485).
    fn effect_unwrap_rhs(&self, t: Ty, target_keeps_result: bool) -> Ty {
        if self.env.auto_unwrap && !target_keeps_result {
            match t {
                Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args.into_iter().next().unwrap(),
                other => other,
            }
        } else { t }
    }

    /// Pin the declared type onto an int-overflow candidate when the literal is
    /// the DIRECT value of an annotated binding (`let x: T = 5…` or `= -5…`), so
    /// a wider `T` (e.g. `UInt64`) makes a >i64 literal valid post-solve (#626).
    fn record_int_literal_context(&mut self, value: &ast::Expr, declared: &Ty) {
        let lit_id = match &value.kind {
            ExprKind::Int { .. } => Some(value.id),
            ExprKind::Unary { op, operand, .. } if op.as_str() == "-"
                && matches!(&operand.kind, ExprKind::Int { .. }) => Some(operand.id),
            ExprKind::Paren { expr } if matches!(&expr.kind, ExprKind::Int { .. }) => Some(expr.id),
            _ => None,
        };
        if let Some(id) = lit_id {
            if let Some(site) = self.deferred_int_overflow_checks.iter_mut().find(|s| s.expr_id == id) {
                site.context_ty = Some(declared.clone());
            }
        }
    }
    /// Resolve a module.func Member expression to a qualified call key.
    fn resolve_module_call(&mut self, object: &ast::Expr, field: &str) -> Option<String> {
        if let ExprKind::Ident { name: module, .. } = &object.kind {
            if let Some(canonical) = self.env.import_table.resolve(module) {
                self.env.import_table.mark_used(module);
                let key = format!("{}.{}", canonical, field);
                self.check_fn_visibility(&canonical, field, &key);
                return Some(key);
            }
            // Check if Ident.field is a Type.method (protocol implementation)
            let key = format!("{}.{}", module, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        // Detect dot-chain submodule access (for pipe context)
        if let Some(dotted) = self.env.import_table.resolve_dotted_path(&object.kind) {
            let key = format!("{}.{}", dotted, field);
            if self.env.functions.contains_key(&sym(&key)) {
                let last_seg = dotted.rsplit('.').next().unwrap_or(&dotted);
                self.emit(super::err(
                    format!("dot-chain submodule access is no longer supported"),
                    format!("Add `import {}` and call `{}.{}()` instead", dotted, last_seg, field),
                    format!("call to {}.{}", dotted, field),
                ));
                return Some(key);
            }
        }
        // TypeName.method (e.g. Val.double in pipe)
        if let ExprKind::TypeName { name: type_name, .. } = &object.kind {
            let key = format!("{}.{}", type_name, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        None
    }

    /// Reject cross-module access to `mod fn` / `local fn` functions.
    ///
    /// A function has `Public` visibility by default — we only store entries
    /// for restricted (`Mod` / `Local`) declarations in `env.fn_visibility`.
    /// If the caller's own module (`self_module_name`) matches the callee's
    /// canonical module, the call is intra-module and all visibilities are
    /// allowed. Otherwise only `Public` is reachable.
    pub(super) fn check_fn_visibility(&mut self, callee_module: &str, field: &str, key: &str) {
        let vis = match self.env.fn_visibility.get(&sym(key)) {
            Some(v) => *v,
            None => return,
        };
        // Intra-module access (same package) is always allowed, regardless of
        // whether it's `mod fn` or `local fn`. This matches the spec for
        // `mod fn` and is a pragmatic relaxation for `local fn` (strict
        // same-file enforcement needs per-fn file tracking — TODO).
        if let Some(self_mod) = self.env.self_module_name {
            if self_mod.as_str() == callee_module {
                return;
            }
        }
        let (kind, scope_hint) = match vis {
            ast::Visibility::Mod => (
                "mod fn",
                "accessible only within the same project",
            ),
            ast::Visibility::Local => (
                "local fn",
                "accessible only within the same file",
            ),
            ast::Visibility::Public => return,
        };
        self.emit(super::err(
            format!("function '{}.{}' is not accessible", callee_module, field),
            format!("'{}' is declared as `{}` ({})", field, kind, scope_hint),
            format!("call to {}.{}", callee_module, field),
        ).with_code("E420"));
    }
}
