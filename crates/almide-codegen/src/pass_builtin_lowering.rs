//! BuiltinLoweringPass: transform special function calls into codegen-specific IR nodes.
//!
//! Converts Named calls to RustMacro, prefixed runtime calls, etc.
//! After this pass, the walker has zero special-case function handling.
//!
//! Transformations:
//! - assert_eq(a, b) → RustMacro { "assert_eq", [a, b] }
//! - assert_ne(a, b) → RustMacro { "assert_ne", [a, b] }
//! - assert_some(x) → RustMacro { "assert", [x.is_some()] }
//! - println(x) → RustMacro { "println", ["{}", x] }
//! - __encode_list_T / __decode_list_T → appropriate runtime call
//! - Type.method(x) → Named { "Type_method" }
//! - Method { "encode"/"decode" } → Named { "Type_encode"/"Type_decode" }
//!
//! NOTE: stdlib intrinsic dispatch (e.g. `value.as_float(v)` →
//! `almide_rt_value_as_float`) is the responsibility of the
//! `@intrinsic`-driven `IntrinsicLoweringPass`. This pass MUST NOT
//! rewrite calls based purely on a name prefix like `value_*`,
//! because user-defined functions can legitimately use such names
//! (`fn value_to_float(...)`) and the prefix carries no information
//! about whether the call resolves to a real runtime symbol.

use almide_ir::*;
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct BuiltinLoweringPass;

impl NanoPass for BuiltinLoweringPass {
    fn name(&self) -> &str { "BuiltinLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["ResultPropagation"] }
    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        for func in &mut program.functions {
            func.body = rewrite_expr(std::mem::take(&mut func.body));
        }
        for tl in &mut program.top_lets {
            tl.value = rewrite_expr(std::mem::take(&mut tl.value));
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                func.body = rewrite_expr(std::mem::take(&mut func.body));
            }
            for tl in &mut module.top_lets {
                tl.value = rewrite_expr(std::mem::take(&mut tl.value));
            }
        }
        PassResult { program, changed: true }
    }
}

fn rewrite_expr(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            let args: Vec<IrExpr> = args.into_iter().map(rewrite_expr).collect();

            match target {
                CallTarget::Named { ref name } => {
                    // assert / assert_eq / assert_ne → RustMacro
                    if name == "assert" || name == "assert_eq" || name == "assert_ne" {
                        // assert(cond, msg) → assert!(cond, "{}", msg)
                        // Rust's assert! macro requires a format string literal as second arg
                        if name == "assert" && args.len() == 2 {
                            let cond = args[0].clone();
                            let msg = args[1].clone();
                            let fmt = IrExpr { kind: IrExprKind::LitStr { value: "{}".into() }, ty: Ty::String, span: None };
                            return IrExpr { kind: IrExprKind::RustMacro { name: *name, args: vec![cond, fmt, msg] }, ty, span };
                        }
                        // Sized Numeric Types (Stage 1c): `assert_eq(x,
                        // 30)` where `x: Int32` needs the `30` literal
                        // retyped to `Int32` so `rustc`'s `assert_eq!`
                        // macro sees matching operand widths. The
                        // assertion itself isn't a typed fn call, so
                        // the usual arg-coercion in `lower_call` doesn't
                        // reach here — patch at the macro build site.
                        let mut args = args;
                        if args.len() == 2 {
                            let l_ty = args[0].ty.clone();
                            let r_ty = args[1].ty.clone();
                            coerce_macro_arg(&mut args[1], &l_ty);
                            coerce_macro_arg(&mut args[0], &r_ty);
                        }
                        return IrExpr { kind: IrExprKind::RustMacro { name: *name, args }, ty, span };
                    }
                    // assert_some → assert!(x.is_some())
                    if name == "assert_some" {
                        // Just use RustMacro with "assert" and transform in walker
                        return IrExpr { kind: IrExprKind::RustMacro {
                            name: "assert".into(),
                            args: vec![IrExpr {
                                kind: IrExprKind::Call {
                                    target: CallTarget::Method {
                                        object: Box::new(args.into_iter().next().unwrap_or(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None })),
                                        method: "is_some".into(),
                                    },
                                    args: vec![],
                                    type_args: vec![],
                                },
                                ty: Ty::Bool, span: None,
                            }],
                        }, ty, span };
                    }
                    // panic → RustMacro
                    if name == "panic" {
                        let mut macro_args = vec![IrExpr { kind: IrExprKind::LitStr { value: "{}".into() }, ty: Ty::String, span: None }];
                        macro_args.extend(args);
                        return IrExpr { kind: IrExprKind::RustMacro { name: "panic".into(), args: macro_args }, ty, span };
                    }
                    // println / eprintln → RustMacro
                    if name == "println" || name == "eprintln" {
                        let mut macro_args = vec![IrExpr { kind: IrExprKind::LitStr { value: "{}".into() }, ty: Ty::String, span: None }];
                        macro_args.extend(args);
                        return IrExpr { kind: IrExprKind::RustMacro { name: name.clone(), args: macro_args }, ty, span };
                    }
                    // __encode_list_T / __decode_list_T
                    if name.starts_with("__encode_list_") || name.starts_with("__decode_list_") {
                        let type_name = if name.starts_with("__encode_list_") {
                            &name["__encode_list_".len()..]
                        } else {
                            &name["__decode_list_".len()..]
                        };
                        let primitives = ["string", "int", "float", "bool"];
                        if primitives.contains(&type_name) {
                            return IrExpr { kind: IrExprKind::Call {
                                target: CallTarget::Named { name: format!("almide_rt_{}", name).into() },
                                args, type_args,
                            }, ty, span };
                        } else {
                            // Custom type: use generic encode/decode
                            let codec_op = if name.starts_with("__encode") { "encode" } else { "decode" };
                            let func_ref = format!("{}_{}", type_name, codec_op);
                            let mut new_args = args;
                            new_args.push(IrExpr {
                                kind: IrExprKind::FnRef { name: func_ref.into() },
                                ty: Ty::Unknown,
                                span: None,
                            });
                            let rt_func = if name.starts_with("__encode") {
                                "almide_rt_value_encode_list"
                            } else {
                                "almide_rt_value_decode_list"
                            };
                            return IrExpr { kind: IrExprKind::Call {
                                target: CallTarget::Named { name: rt_func.into() },
                                args: new_args, type_args,
                            }, ty, span };
                        }
                    }
                    // Other __ prefixed → almide_rt_
                    if name.starts_with("__") {
                        return IrExpr { kind: IrExprKind::Call {
                            target: CallTarget::Named { name: format!("almide_rt_{}", name).into() },
                            args, type_args,
                        }, ty, span };
                    }
                    // Type.method → Type_method
                    if name.contains('.') {
                        let flat = name.replace('.', "_");
                        return IrExpr { kind: IrExprKind::Call {
                            target: CallTarget::Named { name: flat.into() },
                            args, type_args,
                        }, ty, span };
                    }

                    IrExprKind::Call { target, args, type_args }
                }
                CallTarget::Method { object, method } => {
                    let object = Box::new(rewrite_expr(*object));

                    // encode/decode methods → Type_encode/Type_decode standalone calls
                    if method == "encode" || method == "decode"
                        || method.ends_with(".encode") || method.ends_with(".decode")
                    {
                        let flat_method = method.replace('.', "_");
                        let call_name: String = if method.contains('.') {
                            flat_method
                        } else {
                            let type_name = match &object.ty {
                                Ty::Named(n, _) => n.to_string(),
                                Ty::Variant { name, .. } => name.to_string(),
                                _ => "Unknown".to_string(),
                            };
                            format!("{}_{}", type_name, method)
                        };
                        let mut call_args = vec![*object];
                        call_args.extend(args);
                        return IrExpr { kind: IrExprKind::Call {
                            target: CallTarget::Named { name: call_name.into() },
                            args: call_args, type_args,
                        }, ty, span };
                    }

                    // Other Type.method patterns → Type_method standalone calls
                    if method.contains('.') {
                        // Bundled-stdlib modules (lowercase heads like
                        // `uint32.to_int64`) carry the `almide_rt_` prefix
                        // at their definition site (see `walker/mod.rs`
                        // rename of `fn <clean_name>` → `fn almide_rt_<m>_<clean>`).
                        // Mirror that prefix at the call site so UFCS
                        // dispatch resolves to the emitted symbol.
                        // Convention methods (uppercase head — `List.encode`)
                        // use the `Type_method` flat naming and stay as-is.
                        let dot_pos = method.find('.').unwrap();
                        let module_head = &method.as_str()[..dot_pos];
                        let is_bundled = almide_lang::stdlib_info::is_any_stdlib(module_head);
                        let flat = method.replace('.', "_");
                        let name = if is_bundled {
                            format!("almide_rt_{}", flat)
                        } else {
                            flat
                        };
                        let mut call_args = vec![*object];
                        call_args.extend(args);
                        return IrExpr { kind: IrExprKind::Call {
                            target: CallTarget::Named { name: name.into() },
                            args: call_args, type_args,
                        }, ty, span };
                    }

                    IrExprKind::Call {
                        target: CallTarget::Method { object, method },
                        args, type_args,
                    }
                }
                _ => IrExprKind::Call { target, args, type_args },
            }
        }

        // Recurse into all sub-expressions
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_expr(*cond)),
            then: Box::new(rewrite_expr(*then)),
            else_: Box::new(rewrite_expr(*else_)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: rewrite_stmts(stmts),
            expr: expr.map(|e| Box::new(rewrite_expr(*e))),
        },

        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_expr(*subject)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(rewrite_expr),
                body: rewrite_expr(arm.body),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rewrite_expr(*left)), right: Box::new(rewrite_expr(*right)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rewrite_expr(*operand)),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(rewrite_expr(*body)), lambda_id,
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(rewrite_expr).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v))).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_expr(*object)), field,
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(rewrite_expr(*expr)), field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple, iterable: Box::new(rewrite_expr(*iterable)),
            body: rewrite_stmts(body),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_expr(*cond)), body: rewrite_stmts(body),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_expr(expr) },
                other => other,
            }).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(rewrite_expr).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rewrite_expr(*base)),
            fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (rewrite_expr(k), rewrite_expr(v))).collect(),
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rewrite_expr(*object)),
            index: Box::new(rewrite_expr(*index)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(rewrite_expr(*object)),
            key: Box::new(rewrite_expr(*key)),
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(rewrite_expr(*object)), index,
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rewrite_expr(*start)),
            end: Box::new(rewrite_expr(*end)),
            inclusive,
        },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(rewrite_expr(*expr)),
            fallback: Box::new(rewrite_expr(*fallback)),
        },
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(rewrite_expr).collect(),
        },
        // Recurse into iterator chains so lambdas inside fold / map / filter
        // get builtin-lowered (e.g. println → RustMacro).
        IrExprKind::IterChain { source, consume, steps, collector } => IrExprKind::IterChain {
            source: Box::new(rewrite_expr(*source)),
            consume,
            steps: steps.into_iter().map(|s| s.map_exprs(&mut rewrite_expr)).collect(),
            collector: collector.map_exprs(&mut rewrite_expr),
        },
        // Recurse into InlineRust args so `__`-prefixed runtime calls
        // nested inside them (e.g. `__encode_option_string` inside a
        // `value.object(pairs)` InlineRust produced by stdlib lowering)
        // are reached by the `__` prefix transformer.
        IrExprKind::InlineRust { template, args } => IrExprKind::InlineRust {
            template,
            args: args.into_iter().map(|(n, a)| (n, rewrite_expr(a))).collect(),
        },
        // Traverse RuntimeCall args so `panic(...)` / `assert_eq(...)` etc.
        // nested inside a `@intrinsic` fn (e.g. `assert_throws(|| panic(...), msg)`)
        // get lowered to their RustMacro form instead of staying as free fn calls.
        IrExprKind::RuntimeCall { symbol, args } => IrExprKind::RuntimeCall {
            symbol,
            args: args.into_iter().map(rewrite_expr).collect(),
        },
        // Recurse through ownership wrappers inserted by BorrowInsertion /
        // CloneInsertion so derive-generated `__encode_*` calls living
        // inside a `Borrow { List { Tuple { __encode_* } } }` spine still
        // get rewritten to `almide_rt_*`.
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow {
            expr: Box::new(rewrite_expr(*expr)), as_str, mutable,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(rewrite_expr(*expr)) },
        other => other,
    };

    IrExpr { kind, ty, span }
}

/// Retype a bare Int / Float literal whose IR type is `Ty::Int` /
/// `Ty::Float` so it matches a sized-typed peer in the same macro
/// call. See the `assert_eq` site above for the motivation.
fn coerce_macro_arg(arg: &mut IrExpr, peer_ty: &Ty) {
    let sized = matches!(
        peer_ty,
        Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            | Ty::Float32
    );
    if !sized { return; }
    match &mut arg.kind {
        IrExprKind::LitInt { .. } if arg.ty == Ty::Int => {
            arg.ty = peer_ty.clone();
        }
        IrExprKind::LitFloat { .. } if arg.ty == Ty::Float => {
            arg.ty = peer_ty.clone();
        }
        IrExprKind::UnOp { op: UnOp::NegInt, operand } => {
            if matches!(&operand.kind, IrExprKind::LitInt { .. }) && operand.ty == Ty::Int {
                operand.ty = peer_ty.clone();
                arg.ty = peer_ty.clone();
            }
        }
        _ => {}
    }
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
