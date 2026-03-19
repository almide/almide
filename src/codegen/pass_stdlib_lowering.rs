//! StdlibLoweringPass: transform Module calls into Named calls with IR-level arg decoration.
//!
//! Uses build.rs-generated `arg_transforms::lookup()` table to know exactly
//! how each argument should be decorated (BorrowStr, BorrowRef, ToVec, LambdaClone, Direct).
//!
//! NO string rendering. All decisions are structural IR transformations.

use crate::ir::*;
use crate::types::{Ty, TypeConstructorId};
use crate::generated::arg_transforms::{self, ArgTransform};
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct StdlibLoweringPass;

impl NanoPass for StdlibLoweringPass {
    fn name(&self) -> &str { "StdlibLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn run(&self, program: &mut IrProgram, _target: Target) {
        for func in &mut program.functions {
            func.body = rewrite_expr(func.body.clone());
        }
        for tl in &mut program.top_lets {
            tl.value = rewrite_expr(tl.value.clone());
        }
        // Process module functions and top_lets
        for module in &mut program.modules {
            for func in &mut module.functions {
                func.body = rewrite_expr(func.body.clone());
            }
            for tl in &mut module.top_lets {
                tl.value = rewrite_expr(tl.value.clone());
            }
        }
        // Resolve remaining bare UFCS calls in module bodies (checker doesn't fully type them)
        for module in &mut program.modules {
            let sibling_names: Vec<String> = module.functions.iter()
                .map(|f| f.name.clone())
                .collect();
            for func in &mut module.functions {
                func.body = resolve_unresolved_ufcs(func.body.clone(), &sibling_names);
            }
            for tl in &mut module.top_lets {
                tl.value = resolve_unresolved_ufcs(tl.value.clone(), &sibling_names);
            }
        }
        // Prefix intra-module Named calls to match renamed definitions
        for module in &mut program.modules {
            let sibling_names: Vec<String> = module.functions.iter()
                .map(|f| f.name.clone())
                .collect();
            let mod_name = module.name.clone();
            for func in &mut module.functions {
                func.body = prefix_intra_module_calls(func.body.clone(), &mod_name, &sibling_names);
            }
            for tl in &mut module.top_lets {
                tl.value = prefix_intra_module_calls(tl.value.clone(), &mod_name, &sibling_names);
            }
        }
    }
}

fn rewrite_expr(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func }, args, type_args } => {
            // Recurse into args first (fan auto-try is handled by FanLoweringPass)
            let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_expr(a)).collect();

            // Look up per-function transform table
            let info = arg_transforms::lookup(&module, &func);
            let rt_name = info.as_ref().map(|i| i.name.to_string())
                .unwrap_or_else(|| format!("almide_rt_{}_{}", module, func));

            // Fill missing optional args with OptionNone
            let total_params = info.as_ref().map(|i| i.args.len()).unwrap_or(args.len());
            let mut args = args;
            while args.len() < total_params {
                args.push(IrExpr {
                    kind: IrExprKind::OptionNone,
                    ty: Ty::option(Ty::Unknown),
                    span: None,
                });
            }

            // Decorate each arg based on the transform table
            let decorated_args: Vec<IrExpr> = args.into_iter().enumerate().map(|(i, arg)| {
                let transform = info.as_ref()
                    .and_then(|info| info.args.get(i).copied())
                    .unwrap_or(ArgTransform::Direct);

                decorate_arg(arg, transform)
            }).collect();

            // Build the Named call
            let call = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: rt_name },
                    args: decorated_args,
                    type_args,
                },
                ty: ty.clone(),
                span,
            };

            // auto-? is handled by ResultPropagationPass (runs after this pass)
            return call;
        }

        // Recurse into all sub-expressions (same as before)
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| rewrite_expr(a)).collect();
            let target = match target {
                CallTarget::Method { object, method } => {
                    let object = Box::new(rewrite_expr(*object));
                    // Fallback: bare method (no dot) on known type → convert to Module call
                    if !method.contains('.') {
                        if let Some(module) = resolve_module_from_ty(&object.ty, &method) {
                            let mut call_args = vec![*object];
                            call_args.extend(args);
                            let module_call = IrExpr {
                                kind: IrExprKind::Call {
                                    target: CallTarget::Module { module: module.to_string(), func: method },
                                    args: call_args, type_args,
                                },
                                ty: ty.clone(), span,
                            };
                            return rewrite_expr(module_call);
                        }
                    }
                    // UFCS: "module.func" method → convert to Module call and process
                    // Only if the module.func exists in stdlib (arg_transforms table)
                    if method.contains('.') && !method.ends_with(".encode") && !method.ends_with(".decode") {
                        if let Some(dot_pos) = method.find('.') {
                            let mod_name = &method[..dot_pos];
                            let func_name = &method[dot_pos+1..];
                            // Check if this is a real stdlib function
                            if arg_transforms::lookup(mod_name, func_name).is_none() {
                                // Not a stdlib function — leave as Method call for BuiltinLoweringPass
                                return IrExpr { kind: IrExprKind::Call {
                                    target: CallTarget::Method { object, method },
                                    args, type_args,
                                }, ty, span };
                            }
                            let mut call_args = vec![*object];
                            call_args.extend(args);
                            // Recursively process as Module call
                            let module_call = IrExpr {
                                kind: IrExprKind::Call {
                                    target: CallTarget::Module { module: mod_name.to_string(), func: func_name.to_string() },
                                    args: call_args, type_args,
                                },
                                ty: ty.clone(), span,
                            };
                            return rewrite_expr(module_call);
                        }
                    }
                    CallTarget::Method { object, method }
                }
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(rewrite_expr(*callee)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_expr(*cond)),
            then: Box::new(rewrite_expr(*then)),
            else_: Box::new(rewrite_expr(*else_)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: rewrite_stmts(stmts),
            expr: expr.map(|e| Box::new(rewrite_expr(*e))),
        },
        IrExprKind::DoBlock { stmts, expr } => IrExprKind::DoBlock {
            stmts: rewrite_stmts(stmts),
            expr: expr.map(|e| Box::new(rewrite_expr(*e))),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_expr(*subject)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| rewrite_expr(g)),
                body: rewrite_expr(arm.body),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rewrite_expr(*left)), right: Box::new(rewrite_expr(*right)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rewrite_expr(*operand)),
        },
        IrExprKind::Lambda { params, body } => IrExprKind::Lambda {
            params, body: Box::new(rewrite_expr(*body)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| rewrite_expr(e)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| rewrite_expr(e)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rewrite_expr(*base)),
            fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v))).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_expr(*object)), field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rewrite_expr(*iterable)),
            body: rewrite_stmts(body),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_expr(*cond)),
            body: rewrite_stmts(body),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_expr(expr) },
                other => other,
            }).collect(),
        },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (rewrite_expr(k), rewrite_expr(v))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rewrite_expr(*start)),
            end: Box::new(rewrite_expr(*end)),
            inclusive,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rewrite_expr(*object)),
            index: Box::new(rewrite_expr(*index)),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            // FanLoweringPass will strip auto-try from these later
            exprs: exprs.into_iter().map(|e| rewrite_expr(e)).collect(),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

/// Resolve a stdlib module from the receiver/arg type and method name.
/// Only resolves when the type is known (not Unknown).
fn resolve_module_from_ty(ty: &Ty, method: &str) -> Option<&'static str> {
    let candidates = crate::stdlib::resolve_ufcs_candidates(method);
    if candidates.is_empty() { return None; }
    let module = match ty {
        Ty::Applied(TypeConstructorId::List, _) => Some("list"),
        Ty::Applied(TypeConstructorId::Map, _) => Some("map"),
        Ty::String => Some("string"),
        Ty::Int => Some("int"),
        Ty::Float => Some("float"),
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
            other => other,
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

    let kind = match expr.kind {
        // Named call: sort(xs) → list.sort(xs) when "sort" is a stdlib function
        // and NOT a sibling module function
        IrExprKind::Call { target: CallTarget::Named { ref name }, ref args, .. }
            if !args.is_empty()
            && !siblings.contains(name)
            && !crate::stdlib::resolve_ufcs_candidates(name).is_empty() =>
        {
            let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = expr.kind else { unreachable!() };
            let args: Vec<IrExpr> = args.into_iter().map(|a| resolve_unresolved_ufcs(a, siblings)).collect();
            // Try type-based first, then fall back to best-guess for Unknown
            let module = resolve_module_from_ty(&args[0].ty, &name)
                .or_else(|| crate::stdlib::resolve_ufcs_module(&name));
            if let Some(module) = module {
                let module_call = IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: module.to_string(), func: name },
                        args, type_args,
                    },
                    ty: ty.clone(), span,
                };
                return rewrite_expr(module_call);
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, type_args }
        }
        // Method call: xs.map(fn) → list.map(xs, fn) when type is known
        IrExprKind::Call { target: CallTarget::Method { object: ref _obj, ref method }, .. }
            if !method.contains('.')
            && !crate::stdlib::resolve_ufcs_candidates(method).is_empty() =>
        {
            let IrExprKind::Call { target: CallTarget::Method { object, method }, args, type_args } = expr.kind else { unreachable!() };
            let object = Box::new(resolve_unresolved_ufcs(*object, siblings));
            let args: Vec<IrExpr> = args.into_iter().map(|a| resolve_unresolved_ufcs(a, siblings)).collect();
            // Resolve from type, falling back to best-guess when type is unknown or mistyped.
            // Safe here since resolve_unresolved_ufcs only runs on module function bodies.
            let module = resolve_module_from_ty(&object.ty, &method)
                .or_else(|| crate::stdlib::resolve_ufcs_module(&method));
            if let Some(module) = module {
                let mut call_args = vec![*object];
                call_args.extend(args);
                let module_call = IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: module.to_string(), func: method },
                        args: call_args, type_args,
                    },
                    ty: ty.clone(), span,
                };
                return rewrite_expr(module_call);
            }
            IrExprKind::Call {
                target: CallTarget::Method { object, method },
                args, type_args,
            }
        }
        // Recurse into sub-expressions
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| resolve_unresolved_ufcs(a, siblings)).collect();
            let target = match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(resolve_unresolved_ufcs(*object, siblings)), method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(resolve_unresolved_ufcs(*callee, siblings)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(resolve_unresolved_ufcs(*cond, siblings)),
            then: Box::new(resolve_unresolved_ufcs(*then, siblings)),
            else_: Box::new(resolve_unresolved_ufcs(*else_, siblings)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: resolve_ufcs_stmts(stmts, siblings),
            expr: expr.map(|e| Box::new(resolve_unresolved_ufcs(*e, siblings))),
        },
        IrExprKind::DoBlock { stmts, expr } => IrExprKind::DoBlock {
            stmts: resolve_ufcs_stmts(stmts, siblings),
            expr: expr.map(|e| Box::new(resolve_unresolved_ufcs(*e, siblings))),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(resolve_unresolved_ufcs(*subject, siblings)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| resolve_unresolved_ufcs(g, siblings)),
                body: resolve_unresolved_ufcs(arm.body, siblings),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(resolve_unresolved_ufcs(*left, siblings)),
            right: Box::new(resolve_unresolved_ufcs(*right, siblings)),
        },
        IrExprKind::Lambda { params, body } => IrExprKind::Lambda {
            params, body: Box::new(resolve_unresolved_ufcs(*body, siblings)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| resolve_unresolved_ufcs(e, siblings)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, resolve_unresolved_ufcs(v, siblings))).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(resolve_unresolved_ufcs(*iterable, siblings)),
            body: resolve_ufcs_stmts(body, siblings),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(resolve_unresolved_ufcs(*cond, siblings)),
            body: resolve_ufcs_stmts(body, siblings),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: resolve_unresolved_ufcs(expr, siblings) },
                other => other,
            }).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(resolve_unresolved_ufcs(*expr, siblings)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(resolve_unresolved_ufcs(*expr, siblings)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(resolve_unresolved_ufcs(*expr, siblings)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(resolve_unresolved_ufcs(*expr, siblings)) },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(resolve_unresolved_ufcs(*object, siblings)), field,
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| resolve_unresolved_ufcs(e, siblings)).collect(),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn resolve_ufcs_stmts(stmts: Vec<IrStmt>, siblings: &[String]) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: resolve_unresolved_ufcs(value, siblings),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: resolve_unresolved_ufcs(value, siblings) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: resolve_unresolved_ufcs(expr, siblings) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: resolve_unresolved_ufcs(cond, siblings),
                else_: resolve_unresolved_ufcs(else_, siblings),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: resolve_unresolved_ufcs(value, siblings),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}

/// Decorate a single argument based on the per-function transform.
fn decorate_arg(arg: IrExpr, transform: ArgTransform) -> IrExpr {
    let ty = arg.ty.clone();
    let span = arg.span;

    match transform {
        ArgTransform::Direct => arg,

        ArgTransform::BorrowStr => {
            // &*expr
            IrExpr {
                kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: true },
                ty, span,
            }
        }

        ArgTransform::BorrowRef => {
            // &expr
            IrExpr {
                kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false },
                ty, span,
            }
        }

        ArgTransform::ToVec => {
            // (expr).to_vec()
            IrExpr {
                kind: IrExprKind::ToVec { expr: Box::new(arg) },
                ty, span,
            }
        }

        ArgTransform::LambdaClone => {
            // Lambda: add clone bindings for each param
            match arg.kind {
                IrExprKind::Lambda { params, body } => {
                    let clone_stmts: Vec<IrStmt> = params.iter()
                        .filter(|(_, t)| !matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit))
                        .map(|(id, param_ty)| {
                            IrStmt {
                                kind: IrStmtKind::Bind {
                                    var: *id,
                                    mutability: Mutability::Let,
                                    ty: param_ty.clone(),
                                    value: IrExpr {
                                        kind: IrExprKind::Clone {
                                            expr: Box::new(IrExpr {
                                                kind: IrExprKind::Var { id: *id },
                                                ty: param_ty.clone(),
                                                span: None,
                                            }),
                                        },
                                        ty: param_ty.clone(),
                                        span: None,
                                    },
                                },
                                span: None,
                            }
                        }).collect();

                    let wrapped_body = if clone_stmts.is_empty() {
                        *body
                    } else {
                        let body_ty = body.ty.clone();
                        let body_span = body.span;
                        IrExpr {
                            kind: IrExprKind::Block {
                                stmts: clone_stmts,
                                expr: Some(body),
                            },
                            ty: body_ty,
                            span: body_span,
                        }
                    };

                    IrExpr {
                        kind: IrExprKind::Lambda { params, body: Box::new(wrapped_body) },
                        ty, span,
                    }
                }
                // FnRef: pass as-is (function reference, not a lambda)
                _ => arg,
            }
        }

        ArgTransform::WrapSome => {
            // Some(expr) — but if arg is already OptionNone, pass as-is (optional param omitted)
            if matches!(&arg.kind, IrExprKind::OptionNone) {
                arg
            } else {
                IrExpr {
                    kind: IrExprKind::OptionSome { expr: Box::new(arg) },
                    ty: Ty::option(ty),
                    span,
                }
            }
        }

        ArgTransform::LambdaResultWrap => {
            // Lambda with Ok(body) wrapping: callback body gets wrapped in ResultOk
            match arg.kind {
                IrExprKind::Lambda { params, body } => {
                    // Clone bindings (same as LambdaClone)
                    let clone_stmts: Vec<IrStmt> = params.iter()
                        .filter(|(_, t)| !matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit))
                        .map(|(id, param_ty)| {
                            IrStmt {
                                kind: IrStmtKind::Bind {
                                    var: *id,
                                    mutability: Mutability::Let,
                                    ty: param_ty.clone(),
                                    value: IrExpr {
                                        kind: IrExprKind::Clone {
                                            expr: Box::new(IrExpr {
                                                kind: IrExprKind::Var { id: *id },
                                                ty: param_ty.clone(),
                                                span: None,
                                            }),
                                        },
                                        ty: param_ty.clone(),
                                        span: None,
                                    },
                                },
                                span: None,
                            }
                        }).collect();

                    // Wrap body in ResultOk
                    let body_ty = body.ty.clone();
                    let ok_body = IrExpr {
                        kind: IrExprKind::ResultOk { expr: body },
                        ty: Ty::result(body_ty.clone(), Ty::String),
                        span: None,
                    };

                    let wrapped_body = if clone_stmts.is_empty() {
                        ok_body
                    } else {
                        IrExpr {
                            kind: IrExprKind::Block {
                                stmts: clone_stmts,
                                expr: Some(Box::new(ok_body)),
                            },
                            ty: Ty::result(body_ty, Ty::String),
                            span: None,
                        }
                    };

                    IrExpr {
                        kind: IrExprKind::Lambda { params, body: Box::new(wrapped_body) },
                        ty, span,
                    }
                }
                _ => arg,
            }
        }
    }
}

/// Rewrite intra-module `CallTarget::Named` calls that match a sibling function
/// to use the `almide_rt_{module}_{func}` prefix (matching the walker's definition rename).
fn prefix_intra_module_calls(expr: IrExpr, mod_name: &str, siblings: &[String]) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target: CallTarget::Named { ref name }, .. }
            if siblings.contains(name) =>
        {
            let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = expr.kind else { unreachable!() };
            let sanitized = name.replace(' ', "_").replace('-', "_").replace('.', "_");
            let prefixed = format!("almide_rt_{}_{}", mod_name, sanitized);
            let args = args.into_iter().map(|a| prefix_intra_module_calls(a, mod_name, siblings)).collect();
            IrExprKind::Call {
                target: CallTarget::Named { name: prefixed },
                args,
                type_args,
            }
        }
        IrExprKind::FnRef { ref name } if siblings.contains(name) => {
            let IrExprKind::FnRef { name } = expr.kind else { unreachable!() };
            let sanitized = name.replace(' ', "_").replace('-', "_").replace('.', "_");
            IrExprKind::FnRef { name: format!("almide_rt_{}_{}", mod_name, sanitized) }
        }
        // Recurse into sub-expressions
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| prefix_intra_module_calls(a, mod_name, siblings)).collect();
            let target = match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(prefix_intra_module_calls(*object, mod_name, siblings)), method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(prefix_intra_module_calls(*callee, mod_name, siblings)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(prefix_intra_module_calls(*cond, mod_name, siblings)),
            then: Box::new(prefix_intra_module_calls(*then, mod_name, siblings)),
            else_: Box::new(prefix_intra_module_calls(*else_, mod_name, siblings)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: prefix_stmts(stmts, mod_name, siblings),
            expr: expr.map(|e| Box::new(prefix_intra_module_calls(*e, mod_name, siblings))),
        },
        IrExprKind::DoBlock { stmts, expr } => IrExprKind::DoBlock {
            stmts: prefix_stmts(stmts, mod_name, siblings),
            expr: expr.map(|e| Box::new(prefix_intra_module_calls(*e, mod_name, siblings))),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(prefix_intra_module_calls(*subject, mod_name, siblings)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| prefix_intra_module_calls(g, mod_name, siblings)),
                body: prefix_intra_module_calls(arm.body, mod_name, siblings),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(prefix_intra_module_calls(*left, mod_name, siblings)),
            right: Box::new(prefix_intra_module_calls(*right, mod_name, siblings)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(prefix_intra_module_calls(*operand, mod_name, siblings)),
        },
        IrExprKind::Lambda { params, body } => IrExprKind::Lambda {
            params, body: Box::new(prefix_intra_module_calls(*body, mod_name, siblings)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| prefix_intra_module_calls(e, mod_name, siblings)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, prefix_intra_module_calls(v, mod_name, siblings))).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(prefix_intra_module_calls(*iterable, mod_name, siblings)),
            body: prefix_stmts(body, mod_name, siblings),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(prefix_intra_module_calls(*cond, mod_name, siblings)),
            body: prefix_stmts(body, mod_name, siblings),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(prefix_intra_module_calls(*expr, mod_name, siblings)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(prefix_intra_module_calls(*expr, mod_name, siblings)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(prefix_intra_module_calls(*expr, mod_name, siblings)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(prefix_intra_module_calls(*expr, mod_name, siblings)) },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: prefix_intra_module_calls(expr, mod_name, siblings) },
                other => other,
            }).collect(),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(prefix_intra_module_calls(*object, mod_name, siblings)), field,
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| prefix_intra_module_calls(e, mod_name, siblings)).collect(),
        },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(prefix_intra_module_calls(*expr, mod_name, siblings)) },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(prefix_intra_module_calls(*expr, mod_name, siblings)) },
        IrExprKind::Borrow { expr, as_str } => IrExprKind::Borrow { expr: Box::new(prefix_intra_module_calls(*expr, mod_name, siblings)), as_str },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(prefix_intra_module_calls(*expr, mod_name, siblings)) },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| prefix_intra_module_calls(e, mod_name, siblings)).collect(),
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(prefix_intra_module_calls(*object, mod_name, siblings)),
            index: Box::new(prefix_intra_module_calls(*index, mod_name, siblings)),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(prefix_intra_module_calls(*base, mod_name, siblings)),
            fields: fields.into_iter().map(|(k, v)| (k, prefix_intra_module_calls(v, mod_name, siblings))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (prefix_intra_module_calls(k, mod_name, siblings), prefix_intra_module_calls(v, mod_name, siblings))).collect(),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn prefix_stmts(stmts: Vec<IrStmt>, mod_name: &str, siblings: &[String]) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: prefix_intra_module_calls(value, mod_name, siblings),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: prefix_intra_module_calls(value, mod_name, siblings) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: prefix_intra_module_calls(expr, mod_name, siblings) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: prefix_intra_module_calls(cond, mod_name, siblings),
                else_: prefix_intra_module_calls(else_, mod_name, siblings),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: prefix_intra_module_calls(value, mod_name, siblings),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
