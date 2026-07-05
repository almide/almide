/// Resolve a stdlib module from the receiver/arg type and method name.
/// Only resolves when the type is known (not Unknown).
fn resolve_module_from_ty(ty: &Ty, method: &str) -> Option<&'static str> {
    let candidates = almide_lang::stdlib_info::resolve_ufcs_candidates(method);
    if candidates.is_empty() { return None; }
    let module = match ty {
        Ty::Applied(TypeConstructorId::List, _) => Some("list"),
        Ty::Applied(TypeConstructorId::Map, _) => Some("map"),
        Ty::Applied(TypeConstructorId::Set, _) => Some("set"),
        Ty::String => Some("string"),
        Ty::Int => Some("int"),
        Ty::Float => Some("float"),
        // Sized numeric types (Stage 3 of the sized-numeric-types arc).
        // Each hosts its own UFCS conversion / `.to_string()` module.
        Ty::Int8 => Some("int8"),
        Ty::Int16 => Some("int16"),
        Ty::Int32 => Some("int32"),
        Ty::UInt8 => Some("uint8"),
        Ty::UInt16 => Some("uint16"),
        Ty::UInt32 => Some("uint32"),
        Ty::UInt64 => Some("uint64"),
        Ty::Float32 => Some("float32"),
        Ty::Applied(TypeConstructorId::Option, _) => Some("option"),
        Ty::Applied(TypeConstructorId::Result, _) => Some("result"),
        _ => None,
    };
    if let Some(m) = module {
        if candidates.contains(&m) { return Some(m); }
    }
    None
}

fn rewrite_stmts(stmts: Vec<IrStmt>) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: rewrite_expr(value),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rewrite_expr(value) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rewrite_expr(expr) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: rewrite_expr(cond), else_: rewrite_expr(else_),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: rewrite_expr(value),
            },
            IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
                target, index: rewrite_expr(index), value: rewrite_expr(value),
            },
            IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
                target, field, value: rewrite_expr(value),
            },
            IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
                target, key: rewrite_expr(key), value: rewrite_expr(value),
            },
            // Default: recurse every expr child via the exhaustive map_exprs chokepoint.
            other => IrStmt { kind: other, span: s.span }
                .map_exprs(&mut |e| rewrite_expr(e))
                .kind,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}

/// Resolve bare UFCS calls in module function bodies where the checker
/// couldn't fully resolve types. Only converts Named/Method calls that
/// match known stdlib functions and DON'T match sibling module functions.
fn resolve_unresolved_ufcs(expr: IrExpr, siblings: &[String]) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    // Special cases: Named calls and Method calls that resolve to stdlib
    match &expr.kind {
        // Named call: sort(xs) → list.sort(xs) when "sort" is a stdlib function
        // and NOT a sibling module function
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if !args.is_empty()
            && !siblings.iter().any(|s| s == &**name)
            && !almide_lang::stdlib_info::resolve_ufcs_candidates(name).is_empty() =>
        {
            let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = expr.kind else { unreachable!() };
            let args: Vec<IrExpr> = args.into_iter().map(|a| resolve_unresolved_ufcs(a, siblings)).collect();
            let module = resolve_module_from_ty(&args[0].ty, &name)
                .or_else(|| almide_lang::stdlib_info::resolve_ufcs_module(&name));
            if let Some(module) = module {
                return rewrite_expr(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: module.to_string().into(), func: name, def_id: None },
                        args, type_args,
                    },
                    ty, span, def_id: None,
                });
            }
            return IrExpr {
                kind: IrExprKind::Call { target: CallTarget::Named { name }, args, type_args },
                ty, span, def_id: None,
            };
        }
        // Method call: xs.map(fn) → list.map(xs, fn) when type is known
        IrExprKind::Call { target: CallTarget::Method { method, .. }, .. }
            if !method.contains('.')
            && !almide_lang::stdlib_info::resolve_ufcs_candidates(method).is_empty() =>
        {
            let IrExprKind::Call { target: CallTarget::Method { object, method }, args, type_args } = expr.kind else { unreachable!() };
            let object = Box::new(resolve_unresolved_ufcs(*object, siblings));
            let args: Vec<IrExpr> = args.into_iter().map(|a| resolve_unresolved_ufcs(a, siblings)).collect();
            let module = resolve_module_from_ty(&object.ty, &method)
                .or_else(|| almide_lang::stdlib_info::resolve_ufcs_module(&method));
            if let Some(module) = module {
                let mut call_args = vec![*object];
                call_args.extend(args);
                return rewrite_expr(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: module.to_string().into(), func: method, def_id: None },
                        args: call_args, type_args,
                    },
                    ty, span, def_id: None,
                });
            }
            return IrExpr {
                kind: IrExprKind::Call { target: CallTarget::Method { object, method }, args, type_args },
                ty, span, def_id: None,
            };
        }
        _ => {}
    }
    // Default: recurse into all children
    expr.map_children(&mut |e| resolve_unresolved_ufcs(e, siblings))
}

// Kept for backward compatibility — resolve_ufcs_stmts callers in the pass
fn resolve_ufcs_stmts(stmts: Vec<IrStmt>, siblings: &[String]) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| s.map_exprs(&mut |e| resolve_unresolved_ufcs(e, siblings))).collect()
}

// ── Iterator chain lowering ────────────────────────────────────────

/// Inline math/float/int intrinsics as native Rust expressions.
/// Eliminates runtime function call overhead for hot-path numeric operations.
fn try_inline_intrinsic(module: &str, func: &str, args: &[IrExpr], ty: &Ty, span: Option<almide_base::Span>) -> Option<IrExpr> {
    let mk = |kind: IrExprKind| IrExpr { kind, ty: ty.clone(), span, def_id: None };

    // NOTE: `float` entries that used to live here (sqrt/abs/floor/
    // ceil/round/is_nan/is_infinite) have been deleted. The bundled
    // `stdlib/float.almd` now owns those dispatches via `@inline_rust`
    // templates that emit the same Method-call form — the intercept
    // fires earlier in `rewrite_expr`, so this code would be dead even
    // if left in place. Kept `math.*` entries because the `math`
    // module has not been migrated to bundled yet.
    match (module, func) {
        // ── math.sqrt(x) → x.sqrt() via RenderedCall ──
        // These are the highest-impact: called in tight loops (nbody, spectralnorm)
        ("math", "sqrt") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("sqrt"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "abs") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("abs"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "floor") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("floor"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "ceil") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("ceil"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "round") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("round"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "sin") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("sin") },
            args: vec![], type_args: vec![],
        })),
        ("math", "cos") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("cos") },
            args: vec![], type_args: vec![],
        })),
        ("math", "tan") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("tan") },
            args: vec![], type_args: vec![],
        })),
        ("math", "asin") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("asin") },
            args: vec![], type_args: vec![],
        })),
        ("math", "acos") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("acos") },
            args: vec![], type_args: vec![],
        })),
        ("math", "atan") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("atan") },
            args: vec![], type_args: vec![],
        })),
        ("math", "atan2") if args.len() >= 2 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("atan2") },
            args: vec![args[1].clone()], type_args: vec![],
        })),
        ("math", "exp") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("exp") },
            args: vec![], type_args: vec![],
        })),
        ("math", "log") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("ln") },
            args: vec![], type_args: vec![],
        })),
        ("math", "log2") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("log2") },
            args: vec![], type_args: vec![],
        })),
        ("math", "log10") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("log10") },
            args: vec![], type_args: vec![],
        })),
        // float.from_int / int.to_float / float.to_int: walker handles inline cast
        // math.pow: Int exponentiation — keep as runtime call (i64.pow needs u32 cast)
        // ── math.fpow(base, exp) → base.powf(exp) ──
        ("math", "fpow") if args.len() >= 2 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("powf") },
            args: vec![args[1].clone()], type_args: vec![],
        })),
        // ── Constants ──
        ("math", "pi") => Some(mk(IrExprKind::LitFloat { value: std::f64::consts::PI })),
        ("math", "e") => Some(mk(IrExprKind::LitFloat { value: std::f64::consts::E })),
        ("math", "inf") => Some(mk(IrExprKind::LitFloat { value: f64::INFINITY })),
        // `float.is_nan` / `float.is_infinite` deleted — owned by
        // `stdlib/float.almd` via `@inline_rust`.
        ("math", "is_nan") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("is_nan") },
            args: vec![], type_args: vec![],
        })),
        _ => None,
    }
}

/// Try to lower a list.* call into an IterChain IR node.
/// Returns None if the operation isn't iterator-eligible.
fn try_lower_to_iter_chain(func: &str, mut args: Vec<IrExpr>, ty: &Ty, span: Option<almide_base::Span>) -> Option<IrExpr> {
    match func {
        // ── Consuming operations (into_iter) → produce Vec ──
        "map" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![IterStep::Map { lambda: Box::new(lambda) }],
                    collector: IterCollector::Collect,
                },
                ty: ty.clone(), span, def_id: None,
            })
        }
        "filter" if args.len() >= 2 && matches!(args[1].kind, IrExprKind::Lambda { .. }) => {
            let lambda = prepare_lambda_borrowed(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![IterStep::Filter { lambda: Box::new(lambda) }],
                    collector: IterCollector::Collect,
                },
                ty: ty.clone(), span, def_id: None,
            })
        }
        "flat_map" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![IterStep::FlatMap { lambda: Box::new(lambda) }],
                    collector: IterCollector::Collect,
                },
                ty: ty.clone(), span, def_id: None,
            })
        }
        "filter_map" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![IterStep::FilterMap { lambda: Box::new(lambda) }],
                    collector: IterCollector::Collect,
                },
                ty: ty.clone(), span, def_id: None,
            })
        }
        "fold" if args.len() >= 3 => {
            let lambda = prepare_lambda(args.remove(2));
            let init = args.remove(1);
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::Fold { init: Box::new(init), lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span, def_id: None,
            })
        }
        "find" if args.len() >= 2 && matches!(args[1].kind, IrExprKind::Lambda { .. }) => {
            let lambda = prepare_lambda_borrowed(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::Find { lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span, def_id: None,
            })
        }
        // ── Borrowing operations (iter) → produce scalar ──
        "any" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::Any { lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span, def_id: None,
            })
        }
        "all" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::All { lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span, def_id: None,
            })
        }
        "count" if args.len() >= 2 && matches!(args[1].kind, IrExprKind::Lambda { .. }) => {
            let lambda = prepare_lambda_borrowed(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::Count { lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span, def_id: None,
            })
        }
        _ => None,
    }
}

/// Prepare a lambda for consuming iterator ops (map, fold, flat_map, filter_map).
/// Callback gets `T` (owned) — apply LambdaClone with smart single-use skip.
fn prepare_lambda(arg: IrExpr) -> IrExpr {
    let ty = arg.ty.clone();
    let span = arg.span;
    match arg.kind {
        IrExprKind::Lambda { params, body, lambda_id } => {
            let clone_stmts = build_clone_stmts_for_lambda(&params, &body);
            let wrapped_body = if clone_stmts.is_empty() {
                *body
            } else {
                let body_ty = body.ty.clone();
                let body_span = body.span;
                IrExpr {
                    kind: IrExprKind::Block { stmts: clone_stmts, expr: Some(body) },
                    ty: body_ty, span: body_span, def_id: None,
                }
            };
            IrExpr {
                kind: IrExprKind::Lambda { params, body: Box::new(wrapped_body), lambda_id },
                ty, span, def_id: None,
            }
        }
        _ => arg,
    }
}

/// Prepare a lambda for borrowing iterator ops (filter, find, any, all, count).
/// Callback gets `&T` — need deref/clone bindings to convert to owned `T`.
fn prepare_lambda_borrowed(arg: IrExpr) -> IrExpr {
    let ty = arg.ty.clone();
    let span = arg.span;
    match arg.kind {
        IrExprKind::Lambda { params, body, lambda_id } => {
            // For &T params, always add binding: Copy types get `let x = *x;`, heap types get `let x = x.clone();`
            let deref_stmts: Vec<IrStmt> = params.iter()
                .map(|(id, param_ty)| {
                    let is_copy = matches!(param_ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit);
                    let value = if is_copy {
                        // *x (deref the reference)
                        IrExpr {
                            kind: IrExprKind::Deref {
                                expr: Box::new(IrExpr { kind: IrExprKind::Var { id: *id }, ty: param_ty.clone(), span: None, def_id: None }),
                            },
                            ty: param_ty.clone(), span: None, def_id: None,
                        }
                    } else {
                        // x.clone() (clone from reference)
                        IrExpr {
                            kind: IrExprKind::Clone {
                                expr: Box::new(IrExpr { kind: IrExprKind::Var { id: *id }, ty: param_ty.clone(), span: None, def_id: None }),
                            },
                            ty: param_ty.clone(), span: None, def_id: None,
                        }
                    };
                    IrStmt {
                        kind: IrStmtKind::Bind { var: *id, mutability: Mutability::Let, ty: param_ty.clone(), value },
                        span: None,
                    }
                }).collect();

            let wrapped_body = if deref_stmts.is_empty() {
                *body
            } else {
                let body_ty = body.ty.clone();
                let body_span = body.span;
                IrExpr {
                    kind: IrExprKind::Block { stmts: deref_stmts, expr: Some(body) },
                    ty: body_ty, span: body_span, def_id: None,
                }
            };
            IrExpr {
                kind: IrExprKind::Lambda { params, body: Box::new(wrapped_body), lambda_id },
                ty, span, def_id: None,
            }
        }
        _ => arg,
    }
}

/// Rewrite intra-module `CallTarget::Named` calls that match a sibling function
/// to use the `almide_rt_{module}_{func}` prefix (matching the walker's definition rename).
fn prefix_intra_module_calls(expr: IrExpr, mod_name: &str, siblings: &[String]) -> IrExpr {
    // Special cases: Named calls and FnRef to sibling functions get prefixed
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, .. }
            if siblings.iter().any(|s| s == &**name) =>
        {
            let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = expr.kind else { unreachable!() };
            let sanitized = name.replace(' ', "_").replace('-', "_").replace('.', "_");
            let mod_ident = mod_name.replace('.', "_");
            let prefixed = format!("almide_rt_{}_{}", mod_ident, sanitized);
            let args = args.into_iter().map(|a| prefix_intra_module_calls(a, mod_name, siblings)).collect();
            return IrExpr {
                kind: IrExprKind::Call { target: CallTarget::Named { name: prefixed.into() }, args, type_args },
                ty: expr.ty, span: expr.span, def_id: None,
            };
        }
        IrExprKind::FnRef { name } if siblings.iter().any(|s| s == &**name) => {
            let sanitized = name.replace(' ', "_").replace('-', "_").replace('.', "_");
            let mod_ident = mod_name.replace('.', "_");
            return IrExpr {
                kind: IrExprKind::FnRef { name: format!("almide_rt_{}_{}", mod_ident, sanitized).into() },
                ty: expr.ty, span: expr.span, def_id: None,
            };
        }
        _ => {}
    }
    // Default: recurse into all children
    expr.map_children(&mut |e| prefix_intra_module_calls(e, mod_name, siblings))
}

/// Rewrite CallTarget::Module names using versioned name mapping.
/// e.g., CallTarget::Module { module: "json" } → CallTarget::Module { module: "json_v2" }
fn rewrite_module_names(expr: IrExpr, map: &std::collections::HashMap<String, String>) -> IrExpr {
    use almide_base::intern::sym;
    // Only CallTarget::Module needs special handling; everything else just recurses.
    if let IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } = &expr.kind {
        if map.contains_key(&**module) {
            let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, type_args } = expr.kind else { unreachable!() };
            let new_module = map.get(&*module).map(|v| sym(v)).unwrap_or(module);
            let args = args.into_iter().map(|a| rewrite_module_names(a, map)).collect();
            return IrExpr {
                kind: IrExprKind::Call { target: CallTarget::Module { module: new_module, func, def_id: None }, args, type_args },
                ty: expr.ty, span: expr.span, def_id: None,
            };
        }
    }
    expr.map_children(&mut |e| rewrite_module_names(e, map))
}

// ── Lambda clone optimization: only clone multi-use params ─────────

/// Types that need explicit annotation in lambda rebinding to help Rust type inference.
fn needs_type_annotation(ty: &Ty) -> bool {
    matches!(ty, Ty::Applied(_, _) | Ty::Named(_, _) | Ty::Record { .. } | Ty::OpenRecord { .. }
        | Ty::Variant { .. } | Ty::TypeVar(_))
}

/// Build clone stmts for lambda params, skipping single-use params (they can move).
fn build_clone_stmts_for_lambda(params: &[(VarId, Ty)], body: &IrExpr) -> Vec<IrStmt> {
    let non_copy: HashSet<VarId> = params.iter()
        .filter(|(_, t)| !matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit))
        .map(|(id, _)| *id)
        .collect();
    if non_copy.is_empty() { return Vec::new(); }

    let uses = count_lambda_body_uses(body, &non_copy);

    params.iter()
        .filter(|(_, t)| !matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit))
        .filter_map(|(id, param_ty)| {
            let count = uses.get(id).copied().unwrap_or(0);
            if count > 1 {
                // Multi-use: clone binding (let x: T = x.clone())
                Some(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: *id,
                        mutability: Mutability::Let,
                        ty: param_ty.clone(),
                        value: IrExpr {
                            kind: IrExprKind::Clone {
                                expr: Box::new(IrExpr {
                                    kind: IrExprKind::Var { id: *id },
                                    ty: param_ty.clone(),
                                    span: None, def_id: None,
                                }),
                            },
                            ty: param_ty.clone(),
                            span: None, def_id: None,
                        },
                    },
                    span: None,
                })
            } else if count == 1 && needs_type_annotation(param_ty) {
                // Single-use but complex type: rebind for type annotation (let x: T = x)
                Some(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: *id,
                        mutability: Mutability::Let,
                        ty: param_ty.clone(),
                        value: IrExpr {
                            kind: IrExprKind::Var { id: *id },
                            ty: param_ty.clone(),
                            span: None, def_id: None,
                        },
                    },
                    span: None,
                })
            } else {
                None
            }
        }).collect()
}

/// Count uses of target VarIds within a lambda body.
/// Uses inside loops or nested lambdas are counted as 2 (conservative: forces clone).
fn count_lambda_body_uses(expr: &IrExpr, targets: &HashSet<VarId>) -> HashMap<VarId, u32> {
    let mut counts = HashMap::new();
    count_lbu_expr(expr, targets, &mut counts, false);
    counts
}

fn count_lbu_expr(expr: &IrExpr, targets: &HashSet<VarId>, counts: &mut HashMap<VarId, u32>, in_multi: bool) {
    LbuCounter { targets, counts, in_multi }.visit_expr(expr);
}

/// Counts target-var uses by riding the exhaustive `IrVisitor` walk, so no node
/// kind (incl. IterChain/RcWrap/TailCall) drops a subtree and under-counts — which
/// would let a captured var move when it must clone. A use inside a loop or nested
/// lambda counts double (conservative: forces a clone).
struct LbuCounter<'a> {
    targets: &'a HashSet<VarId>,
    counts: &'a mut HashMap<VarId, u32>,
    in_multi: bool,
}

impl IrVisitor for LbuCounter<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Var { id } if self.targets.contains(id) => {
                *self.counts.entry(*id).or_insert(0) += if self.in_multi { 2 } else { 1 };
            }
            // Multi-execution contexts: their bodies run per-iteration/per-call,
            // so a use inside counts double.
            IrExprKind::ForIn { iterable, body, .. } => {
                self.visit_expr(iterable);
                let saved = std::mem::replace(&mut self.in_multi, true);
                for s in body { self.visit_stmt(s); }
                self.in_multi = saved;
            }
            IrExprKind::While { cond, body } => {
                let saved = std::mem::replace(&mut self.in_multi, true);
                self.visit_expr(cond);
                for s in body { self.visit_stmt(s); }
                self.in_multi = saved;
            }
            IrExprKind::Lambda { body, .. } => {
                let saved = std::mem::replace(&mut self.in_multi, true);
                self.visit_expr(body);
                self.in_multi = saved;
            }
            _ => walk_expr(self, expr), // default: recurse all children at current in_multi
        }
    }
}
