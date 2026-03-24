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
//! - value_*(x) → Named { "almide_rt_value_*" }
//! - __encode_list_T / __decode_list_T → appropriate runtime call
//! - Type.method(x) → Named { "Type_method" }
//! - Method { "encode"/"decode" } → Named { "Type_encode"/"Type_decode" }

use crate::ir::*;
use crate::types::Ty;
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
                    // println → RustMacro
                    if name == "println" {
                        let mut macro_args = vec![IrExpr { kind: IrExprKind::LitStr { value: "{}".into() }, ty: Ty::String, span: None }];
                        macro_args.extend(args);
                        return IrExpr { kind: IrExprKind::RustMacro { name: "println".into(), args: macro_args }, ty, span };
                    }
                    // value_* → almide_rt_value_*
                    if name.starts_with("value_") {
                        return IrExpr { kind: IrExprKind::Call {
                            target: CallTarget::Named { name: format!("almide_rt_{}", name).into() },
                            args, type_args,
                        }, ty, span };
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
                        let flat = method.replace('.', "_");
                        let mut call_args = vec![*object];
                        call_args.extend(args);
                        return IrExpr { kind: IrExprKind::Call {
                            target: CallTarget::Named { name: flat.into() },
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
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(rewrite_expr).collect(),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
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
