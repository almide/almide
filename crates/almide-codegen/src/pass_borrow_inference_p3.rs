// ── Phase 2: Insert Borrow nodes at call sites ─────────────────────

pub fn insert_borrows_at_call_sites(program: &mut IrProgram, sigs: &HashMap<String, Vec<ParamBorrow>>) {
    for func in &mut program.functions {
        func.body = rewrite_calls(std::mem::take(&mut func.body), sigs, None);
    }
    for tl in &mut program.top_lets {
        tl.value = rewrite_calls(std::mem::take(&mut tl.value), sigs, None);
    }
    for module in &mut program.modules {
        let mod_name = module.name.to_string();
        for func in &mut module.functions {
            func.body = rewrite_calls(std::mem::take(&mut func.body), sigs, Some(&mod_name));
        }
        for tl in &mut module.top_lets {
            tl.value = rewrite_calls(std::mem::take(&mut tl.value), sigs, Some(&mod_name));
        }
    }
}

fn rewrite_calls(expr: IrExpr, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_calls(a, sigs, mod_scope)).collect();

            // `is_method_with_self` marks that the call target carries a
            // receiver object that walker will splice in ahead of `args`.
            // In that case the sig's param list starts at the receiver,
            // and the IR `args` align to params 1..N (not 0..N).
            let (callee_name, is_method_with_self) = match &target {
                CallTarget::Named { name } => (Some(name.to_string()), false),
                CallTarget::Module { module, func, .. } => (Some(format!("{}::{}", module, func)), false),
                CallTarget::Method { method, .. } if method.contains('.') => (Some(method.to_string()), true),
                _ => (None, false),
            };

            let args = if let Some(ref name) = callee_name {
                // For module-scoped calls, look up with "module::func" key first
                let borrows = mod_scope
                    .and_then(|m| sigs.get(&format!("{}::{}", m, name)))
                    .or_else(|| sigs.get(name));
                if let Some(borrows) = borrows {
                    let arg_offset = if is_method_with_self { 1 } else { 0 };
                    args.into_iter().enumerate().map(|(i, arg)| {
                        match borrows.get(i + arg_offset) {
                            Some(ParamBorrow::Ref | ParamBorrow::RefSlice) => {
                                let t = arg.ty.clone(); let s = arg.span;
                                IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: false }, ty: t, span: s, def_id: None }
                            }
                            Some(ParamBorrow::RefMut) => {
                                let t = arg.ty.clone(); let s = arg.span;
                                IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: true }, ty: t, span: s, def_id: None }
                            }
                            Some(ParamBorrow::RefStr) => {
                                let t = arg.ty.clone(); let s = arg.span;
                                IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: true, mutable: false }, ty: t, span: s, def_id: None }
                            }
                            _ => arg,
                        }
                    }).collect()
                } else { args }
            } else { args };

            let target = match target {
                CallTarget::Method { object, method } => {
                    let mut obj = rewrite_calls(*object, sigs, mod_scope);
                    if method.contains('.') {
                        if let Some(borrows) = sigs.get(method.as_str()) {
                            if let Some(b) = borrows.first() {
                                match b {
                                    ParamBorrow::Ref | ParamBorrow::RefSlice => {
                                        let t = obj.ty.clone(); let s = obj.span;
                                        obj = IrExpr { kind: IrExprKind::Borrow { expr: Box::new(obj), as_str: false, mutable: false }, ty: t, span: s, def_id: None };
                                    }
                                    ParamBorrow::RefStr => {
                                        let t = obj.ty.clone(); let s = obj.span;
                                        obj = IrExpr { kind: IrExprKind::Borrow { expr: Box::new(obj), as_str: true, mutable: false }, ty: t, span: s, def_id: None };
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    CallTarget::Method { object: Box::new(obj), method }
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(rewrite_calls(*callee, sigs, mod_scope)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }

        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| rewrite_calls_stmt(s, sigs, mod_scope)).collect(),
            expr: expr.map(|e| Box::new(rewrite_calls(*e, sigs, mod_scope))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_calls(*cond, sigs, mod_scope)),
            then: Box::new(rewrite_calls(*then, sigs, mod_scope)),
            else_: Box::new(rewrite_calls(*else_, sigs, mod_scope)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_calls(*subject, sigs, mod_scope)),
            arms: arms.into_iter().map(|a| IrMatchArm {
                pattern: a.pattern,
                guard: a.guard.map(|g| rewrite_calls(g, sigs, mod_scope)),
                body: rewrite_calls(a.body, sigs, mod_scope),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rewrite_calls(*iterable, sigs, mod_scope)),
            body: body.into_iter().map(|s| rewrite_calls_stmt(s, sigs, mod_scope)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_calls(*cond, sigs, mod_scope)),
            body: body.into_iter().map(|s| rewrite_calls_stmt(s, sigs, mod_scope)).collect(),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(rewrite_calls(*body, sigs, mod_scope)), lambda_id,
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rewrite_calls(*left, sigs, mod_scope)), right: Box::new(rewrite_calls(*right, sigs, mod_scope)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rewrite_calls(*operand, sigs, mod_scope)),
        },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)),
            fallback: Box::new(rewrite_calls(*fallback, sigs, mod_scope)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_calls(expr, sigs, mod_scope) },
                other => other,
            }).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| rewrite_calls(e, sigs, mod_scope)).collect(),
        },
        IrExprKind::IterChain { source, consume, steps, collector } => IrExprKind::IterChain {
            source: Box::new(rewrite_calls(*source, sigs, mod_scope)),
            consume, steps, collector,
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name,
            fields: fields.into_iter()
                .map(|(k, v)| (k, rewrite_calls(v, sigs, mod_scope))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rewrite_calls(*base, sigs, mod_scope)),
            fields: fields.into_iter()
                .map(|(k, v)| (k, rewrite_calls(v, sigs, mod_scope))).collect(),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter()
                .map(|e| rewrite_calls(e, sigs, mod_scope)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter()
                .map(|e| rewrite_calls(e, sigs, mod_scope)).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter()
                .map(|(k, v)| (rewrite_calls(k, sigs, mod_scope), rewrite_calls(v, sigs, mod_scope))).collect(),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_calls(*object, sigs, mod_scope)), field,
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(rewrite_calls(*object, sigs, mod_scope)), index,
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)), field,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rewrite_calls(*object, sigs, mod_scope)),
            index: Box::new(rewrite_calls(*index, sigs, mod_scope)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(rewrite_calls(*object, sigs, mod_scope)),
            key: Box::new(rewrite_calls(*key, sigs, mod_scope)),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rewrite_calls(*start, sigs, mod_scope)),
            end: Box::new(rewrite_calls(*end, sigs, mod_scope)),
            inclusive,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow {
            expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)), as_str, mutable,
        },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|a| rewrite_calls(a, sigs, mod_scope)).collect(),
        },
        IrExprKind::RuntimeCall { symbol, args } => {
            let args: Vec<IrExpr> = args.into_iter()
                .map(|a| rewrite_calls(a, sigs, mod_scope))
                .collect();
            // Look up the borrow signature by the mangled runtime symbol
            // (populated from bundled `@intrinsic` attrs at the top of
            // `infer_borrow_signatures`). On hit, wrap each arg with the
            // corresponding Borrow IR node; on miss, leave args untouched
            // (walker still has its ty-based fallback).
            let args = if let Some(borrows) = sigs.get(symbol.as_str()) {
                args.into_iter().enumerate().map(|(i, arg)| {
                    match borrows.get(i) {
                        Some(ParamBorrow::Ref | ParamBorrow::RefSlice) => {
                            let t = arg.ty.clone(); let s = arg.span;
                            IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: false }, ty: t, span: s, def_id: None }
                        }
                        Some(ParamBorrow::RefMut) => {
                            let t = arg.ty.clone(); let s = arg.span;
                            IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: true }, ty: t, span: s, def_id: None }
                        }
                        Some(ParamBorrow::RefStr) => {
                            let t = arg.ty.clone(); let s = arg.span;
                            IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: true, mutable: false }, ty: t, span: s, def_id: None }
                        }
                        _ => arg,
                    }
                }).collect()
            } else { args };
            IrExprKind::RuntimeCall { symbol, args }
        }
        // Explicit-preserve: leaves + nodes this call-rewriter intentionally
        // does NOT descend into (TailCall / RcWrap / InlineRust args are left
        // untouched, matching the original `other => other`). Listed explicitly
        // so a new IrExprKind variant is a compile error here.
        kind @ (IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
            | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
            | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
            | IrExprKind::Break | IrExprKind::Continue | IrExprKind::TailCall { .. }
            | IrExprKind::EmptyMap | IrExprKind::OptionNone | IrExprKind::RcWrap { .. }
            | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
            | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
            | IrExprKind::Hole | IrExprKind::Todo { .. }) => kind,
    };

    IrExpr { kind, ty, span, def_id: None }
}

fn rewrite_calls_stmt(stmt: IrStmt, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: rewrite_calls(value, sigs, mod_scope),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rewrite_calls(value, sigs, mod_scope) },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rewrite_calls(expr, sigs, mod_scope) },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: rewrite_calls(cond, sigs, mod_scope), else_: rewrite_calls(else_, sigs, mod_scope),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: rewrite_calls(value, sigs, mod_scope),
        },
        // Assign-with-computed-subexpr kinds: descend into the index/key/value so a
        // stdlib call there (e.g. `m[string.take(s, i)] = …`) gets its borrow args
        // annotated — else the call's `&str`/`&[T]` arg renders as an owned value
        // and rustc rejects it (#415). The earlier `other => other` skipped these.
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target, index: rewrite_calls(index, sigs, mod_scope), value: rewrite_calls(value, sigs, mod_scope),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target, key: rewrite_calls(key, sigs, mod_scope), value: rewrite_calls(value, sigs, mod_scope),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target, field, value: rewrite_calls(value, sigs, mod_scope),
        },
        // Explicit-preserve: no rewrite-relevant expr children (or handled elsewhere).
        kind @ (IrStmtKind::Comment { .. }
            | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. }
            | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
            | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. }) => kind,
    };
    IrStmt { kind, span: stmt.span }
}

// ── Phase 3: Hoist conflicting reads from &mut call args ──────────

/// When a call has `&mut var_x` as one arg and another arg reads `var_x`,
/// Rust's borrow checker rejects the overlapping borrows. This phase hoists
/// the conflicting read args into `let __hoist = <expr>` bindings before the
/// call, replacing them with `Var(__hoist)`.
pub fn hoist_conflicting_reads(program: &mut IrProgram) {
    for func in &mut program.functions {
        func.body = hoist_expr(std::mem::take(&mut func.body), &mut program.var_table);
    }
    for module in &mut program.modules {
        for func in &mut module.functions {
            func.body = hoist_expr(std::mem::take(&mut func.body), &mut program.var_table);
        }
    }
}

/// Find VarId of a `&mut Var(x)` argument.
fn find_mut_borrow_var(arg: &IrExpr) -> Option<VarId> {
    if let IrExprKind::Borrow { expr, mutable: true, .. } = &arg.kind {
        if let IrExprKind::Var { id } = &expr.kind {
            return Some(*id);
        }
    }
    None
}

fn hoist_expr(expr: IrExpr, vt: &mut VarTable) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            let args: Vec<IrExpr> = args.into_iter().map(|a| hoist_expr(a, vt)).collect();
            let target = match target {
                CallTarget::Method { object, method } =>
                    CallTarget::Method { object: Box::new(hoist_expr(*object, vt)), method },
                CallTarget::Computed { callee } =>
                    CallTarget::Computed { callee: Box::new(hoist_expr(*callee, vt)) },
                other => other,
            };
            return hoist_call_if_needed(target, args, type_args, ty, span, vt);
        }
        IrExprKind::RuntimeCall { symbol, args } => {
            let args: Vec<IrExpr> = args.into_iter().map(|a| hoist_expr(a, vt)).collect();
            // Check for &mut conflict
            let mut_var = args.iter().find_map(find_mut_borrow_var);
            if let Some(mut_id) = mut_var {
                let mut hoisted_stmts: Vec<IrStmt> = Vec::new();
                let new_args: Vec<IrExpr> = args.into_iter().map(|arg| {
                    if find_mut_borrow_var(&arg).is_some() {
                        arg // keep the &mut arg as-is
                    } else if uses_var(&arg, mut_id) {
                        let tmp = vt.alloc(sym("__hoist"), arg.ty.clone(), Mutability::Let, None);
                        let tmp_ty = arg.ty.clone();
                        hoisted_stmts.push(IrStmt {
                            kind: IrStmtKind::Bind { var: tmp, mutability: Mutability::Let, ty: tmp_ty.clone(), value: arg },
                            span: None,
                        });
                        IrExpr { kind: IrExprKind::Var { id: tmp }, ty: tmp_ty, span: None, def_id: None }
                    } else {
                        arg
                    }
                }).collect();
                if !hoisted_stmts.is_empty() {
                    let call = IrExpr {
                        kind: IrExprKind::RuntimeCall { symbol, args: new_args },
                        ty: ty.clone(), span, def_id: None,
                    };
                    return IrExpr {
                        kind: IrExprKind::Block { stmts: hoisted_stmts, expr: Some(Box::new(call)) },
                        ty, span, def_id: None,
                    };
                }
                IrExprKind::RuntimeCall { symbol, args: new_args }
            } else {
                IrExprKind::RuntimeCall { symbol, args }
            }
        }

        // Recurse into all compound expressions
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| hoist_stmt(s, vt)).collect(),
            expr: expr.map(|e| Box::new(hoist_expr(*e, vt))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(hoist_expr(*cond, vt)),
            then: Box::new(hoist_expr(*then, vt)),
            else_: Box::new(hoist_expr(*else_, vt)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(hoist_expr(*subject, vt)),
            arms: arms.into_iter().map(|a| IrMatchArm {
                pattern: a.pattern,
                guard: a.guard.map(|g| hoist_expr(g, vt)),
                body: hoist_expr(a.body, vt),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(hoist_expr(*iterable, vt)),
            body: body.into_iter().map(|s| hoist_stmt(s, vt)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(hoist_expr(*cond, vt)),
            body: body.into_iter().map(|s| hoist_stmt(s, vt)).collect(),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(hoist_expr(*body, vt)), lambda_id,
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(hoist_expr(*left, vt)), right: Box::new(hoist_expr(*right, vt)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(hoist_expr(*operand, vt)),
        },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(hoist_expr(*expr, vt)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(hoist_expr(*expr, vt)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(hoist_expr(*expr, vt)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(hoist_expr(*expr, vt)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(hoist_expr(*expr, vt)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(hoist_expr(*expr, vt)), fallback: Box::new(hoist_expr(*fallback, vt)),
        },
        // Explicit-preserve: nodes this hoist pass does NOT descend into. The
        // &mut-conflict hoist only fires at Call / RuntimeCall sites and the
        // compound forms above; everything else is returned unchanged, exactly
        // as the original `other => other` did (zero behaviour change).
        kind @ (IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
            | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
            | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
            | IrExprKind::Fan { .. } | IrExprKind::Break | IrExprKind::Continue
            | IrExprKind::TailCall { .. } | IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap
            | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }
            | IrExprKind::Tuple { .. } | IrExprKind::Range { .. }
            | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
            | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
            | IrExprKind::StringInterp { .. } | IrExprKind::OptionNone
            | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. }
            | IrExprKind::Await { .. } | IrExprKind::Clone { .. }
            | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
            | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
            | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
            | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
            | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
            | IrExprKind::IterChain { .. } | IrExprKind::Hole
            | IrExprKind::Todo { .. }) => kind,
    };

    IrExpr { kind, ty, span, def_id: None }
}

fn hoist_call_if_needed(target: CallTarget, args: Vec<IrExpr>, type_args: Vec<almide_lang::types::Ty>,
    ty: almide_lang::types::Ty, span: Option<almide_base::span::Span>, vt: &mut VarTable) -> IrExpr
{
    let mut_var = args.iter().find_map(find_mut_borrow_var);
    if let Some(mut_id) = mut_var {
        let mut hoisted_stmts: Vec<IrStmt> = Vec::new();
        let new_args: Vec<IrExpr> = args.into_iter().map(|arg| {
            if find_mut_borrow_var(&arg).is_some() {
                arg
            } else if uses_var(&arg, mut_id) {
                let tmp = vt.alloc(sym("__hoist"), arg.ty.clone(), Mutability::Let, None);
                let tmp_ty = arg.ty.clone();
                hoisted_stmts.push(IrStmt {
                    kind: IrStmtKind::Bind { var: tmp, mutability: Mutability::Let, ty: tmp_ty.clone(), value: arg },
                    span: None,
                });
                IrExpr { kind: IrExprKind::Var { id: tmp }, ty: tmp_ty, span: None, def_id: None }
            } else {
                arg
            }
        }).collect();
        if !hoisted_stmts.is_empty() {
            let call = IrExpr {
                kind: IrExprKind::Call { target, args: new_args, type_args },
                ty: ty.clone(), span, def_id: None,
            };
            return IrExpr {
                kind: IrExprKind::Block { stmts: hoisted_stmts, expr: Some(Box::new(call)) },
                ty, span, def_id: None,
            };
        }
        IrExpr { kind: IrExprKind::Call { target, args: new_args, type_args }, ty, span, def_id: None }
    } else {
        IrExpr { kind: IrExprKind::Call { target, args, type_args }, ty, span, def_id: None }
    }
}

fn hoist_stmt(stmt: IrStmt, vt: &mut VarTable) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: hoist_expr(value, vt),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: hoist_expr(value, vt) },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: hoist_expr(expr, vt) },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: hoist_expr(cond, vt), else_: hoist_expr(else_, vt),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: hoist_expr(value, vt),
        },
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target, index: hoist_expr(index, vt), value: hoist_expr(value, vt),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target, key: hoist_expr(key, vt), value: hoist_expr(value, vt),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target, field, value: hoist_expr(value, vt),
        },
        // Explicit-preserve: stmt kinds with no hoistable child expr, matching
        // the original `other => other` (zero behaviour change).
        kind @ (IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. }
            | IrStmtKind::RcDec { .. } | IrStmtKind::ListSwap { .. }
            | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
            | IrStmtKind::ListCopySlice { .. }) => kind,
    };
    IrStmt { kind, span: stmt.span }
}
