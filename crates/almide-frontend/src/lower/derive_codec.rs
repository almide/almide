// ── Auto-derive Codec ───────────────────────────────────────────

use almide_ir::*;
use crate::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;

/// Auto-derive Codec encode: `fn T.encode(t: T) -> Value`
/// Generates: `value.object([("field1", value.str(t.field1)), ("field2", value.int(t.field2)), ...])`
pub(super) fn auto_derive_encode(vt: &mut VarTable, type_name: &str, type_ty: &Ty, fields: &[IrFieldDecl]) -> IrFunction {
    let var = vt.alloc(sym("_v"), type_ty.clone(), Mutability::Let, None);
    let value_ty = Ty::Named(sym("Value"), vec![]);

    // Build list of (String, Value) tuples for value.object(...)
    let pairs: Vec<IrExpr> = fields.iter().map(|f| {
        let field_access = IrExpr {
            kind: IrExprKind::Member {
                object: Box::new(IrExpr { kind: IrExprKind::Var { id: var }, ty: type_ty.clone(), span: None, def_id: None }),
                field: f.name,
            },
            ty: f.ty.clone(), span: None, def_id: None,
        };
        // Choose value constructor based on field type
        let value_call = encode_field_value(&field_access, &f.ty, &value_ty);
        IrExpr {
            kind: IrExprKind::Tuple { elements: vec![
                IrExpr { kind: IrExprKind::LitStr { value: f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string()) }, ty: Ty::String, span: None, def_id: None },
                value_call,
            ]},
            ty: Ty::Tuple(vec![Ty::String, value_ty.clone()]), span: None, def_id: None,
        }
    }).collect();

    let pairs_list = IrExpr {
        kind: IrExprKind::List { elements: pairs },
        ty: Ty::list(Ty::Tuple(vec![Ty::String, value_ty.clone()])), span: None, def_id: None,
    };

    let body = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("value"), func: sym("object"), def_id: None },
            args: vec![pairs_list],
            type_args: vec![],
        },
        ty: value_ty.clone(), span: None, def_id: None,
    };

    IrFunction {
        name: sym(&format!("{}.encode", type_name)),
        params: vec![IrParam { var, ty: type_ty.clone(), name: sym("_v"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }],
        ret_ty: value_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}

/// Choose the right value constructor for a field type.
/// Codec helper name for an `Option[T]` field. A custom element type keeps its
/// NAME so `BuiltinLoweringPass` can route it through the generic option codec with
/// a `T.encode`/`T.decode` per-element fn; primitives keep the suffix that names an
/// existing `almide_rt___{op}_option_<prim>` helper. `decode_func_suffix` alone
/// collapses every Named type to "value", for which no helper exists (新②).
fn option_codec_fn(op: &str, inner: &Ty) -> String {
    match inner {
        Ty::Named(name, _) => format!("__{}_option_{}", op, name),
        _ => format!("__{}_option_{}", op, decode_func_suffix(inner)),
    }
}

fn encode_field_value(field_expr: &IrExpr, field_ty: &Ty, value_ty: &Ty) -> IrExpr {
    let (module, func) = match field_ty {
        Ty::String => ("value", "str"),
        Ty::Int => ("value", "int"),
        Ty::Float => ("value", "float"),
        Ty::Bool => ("value", "bool"),
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => {
            let inner = &args[0];
            return IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: sym(&option_codec_fn("encode", inner)) },
                    args: vec![field_expr.clone()],
                    type_args: vec![],
                },
                ty: value_ty.clone(), span: None, def_id: None,
            };
        }
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            let inner = &args[0];
            let func_name = if let Ty::Named(name, _) = inner {
                format!("__encode_list_{}", name)
            } else {
                format!("__encode_list_{}", decode_func_suffix(inner))
            };
            return IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: sym(&func_name) },
                    args: vec![field_expr.clone()],
                    type_args: vec![],
                },
                ty: value_ty.clone(), span: None, def_id: None,
            };
        }
        _ => {
            // Named type (nested Codec) → call Type.encode(field)
            if let Ty::Named(name, _) = field_ty {
                return IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym(&format!("{}.encode", name)) },
                        args: vec![field_expr.clone()],
                        type_args: vec![],
                    },
                    ty: value_ty.clone(), span: None, def_id: None,
                };
            }
            // Fallback: value.str(to_string(field))
            ("value", "str")
        }
    };
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None },
            args: vec![field_expr.clone()],
            type_args: vec![],
        },
        ty: value_ty.clone(), span: None, def_id: None,
    }
}

/// Auto-derive Codec decode: `fn T.decode(v: Value) -> Result[T, String]`
pub(super) fn auto_derive_decode(vt: &mut VarTable, type_name: &str, type_ty: &Ty, fields: &[IrFieldDecl]) -> IrFunction {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let result_ty = Ty::result(type_ty.clone(), Ty::String);
    let var_v = vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);

    let mut stmts = Vec::new();
    let mut field_vars = Vec::new();
    let key_name = |f: &IrFieldDecl| -> String { f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string()) };

    for f in fields {
        let is_option = f.ty.is_option();
        let has_default = f.default.is_some();
        let inner_ty = f.ty.inner().cloned().unwrap_or_else(|| f.ty.clone());
        let field_var = vt.alloc(sym(&format!("_{}", f.name)), f.ty.clone(), Mutability::Let, None);

        // value.field(_v, "key") — returns Result[Value, String]
        let get_field_call = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("value"), func: sym("field"), def_id: None },
                args: vec![
                    IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None, def_id: None },
                    IrExpr { kind: IrExprKind::LitStr { value: key_name(f) }, ty: Ty::String, span: None, def_id: None },
                ],
                type_args: vec![],
            },
            ty: Ty::result(value_ty.clone(), Ty::String), span: None, def_id: None,
        };

        let decode_expr = if is_option {
            // Option[T]: use runtime helper value_decode_option(_v, "key", as_T)
            // Returns Result[Option[T], String]
            IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym(&option_codec_fn("decode", &inner_ty)) },
                        args: vec![
                            IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None, def_id: None },
                            IrExpr { kind: IrExprKind::LitStr { value: key_name(f) }, ty: Ty::String, span: None, def_id: None },
                        ],
                        type_args: vec![],
                    },
                    ty: Ty::result(f.ty.clone(), Ty::String), span: None, def_id: None,
                })},
                ty: f.ty.clone(), span: None, def_id: None,
            }
        } else if has_default {
            // Default: use runtime helper value_decode_with_default(_v, "key", default, as_T)
            let default_expr = f.default.clone().unwrap_or(IrExpr { kind: IrExprKind::Unit, ty: f.ty.clone(), span: None, def_id: None });
            IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym(&format!("__decode_default_{}", decode_func_suffix(&f.ty))) },
                        args: vec![
                            IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None, def_id: None },
                            IrExpr { kind: IrExprKind::LitStr { value: key_name(f) }, ty: Ty::String, span: None, def_id: None },
                            default_expr,
                        ],
                        type_args: vec![],
                    },
                    ty: Ty::result(f.ty.clone(), Ty::String), span: None, def_id: None,
                })},
                ty: f.ty.clone(), span: None, def_id: None,
            }
        } else {
            // Required: value.field(_v, "key")? |> as_T?
            let get_and_try = IrExpr {
                kind: IrExprKind::Try { expr: Box::new(get_field_call) },
                ty: value_ty.clone(), span: None, def_id: None,
            };
            decode_field_value(get_and_try, &f.ty, &value_ty)
        };

        stmts.push(IrStmt {
            kind: IrStmtKind::Bind { var: field_var, mutability: Mutability::Let, ty: f.ty.clone(), value: decode_expr },
            span: None,
        });
        field_vars.push((f.name, field_var, f.ty.clone()));
    }

    // ok(TypeName { field1: _field1, field2: _field2, ... })
    let record = IrExpr {
        kind: IrExprKind::Record {
            name: Some(sym(type_name)),
            // Each field value carries its DECLARED type — NOT Ty::Unknown. The v1 record
            // builder decides a field's heap-ness from `expr.ty` (binds_p3), so an Unknown
            // scalar field (`id: Int`) was mis-classified as heap → an rc_inc + i64.extend_i32_u
            // of an i64 Int → invalid wasm in the generated `T.decode`. The real type makes the
            // builder store a scalar directly and co-own only true heap fields.
            fields: field_vars.iter().map(|(name, var, ty)| {
                (*name, IrExpr { kind: IrExprKind::Var { id: *var }, ty: ty.clone(), span: None, def_id: None })
            }).collect(),
        },
        ty: type_ty.clone(), span: None, def_id: None,
    };

    let body = IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(IrExpr {
                kind: IrExprKind::ResultOk { expr: Box::new(record) },
                ty: result_ty.clone(), span: None, def_id: None,
            })),
        },
        ty: result_ty.clone(), span: None, def_id: None,
    };

    IrFunction {
        name: sym(&format!("{}.decode", type_name)),
        params: vec![IrParam { var: var_v, ty: value_ty, name: sym("_v"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }],
        ret_ty: result_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}

fn decode_func_suffix(ty: &Ty) -> &'static str {
    match ty {
        Ty::String => "string",
        Ty::Int => "int",
        Ty::Float => "float",
        Ty::Bool => "bool",
        _ => "value",
    }
}

/// Generate decode expression for a field based on its type.
fn decode_field_value(get_field_expr: IrExpr, field_ty: &Ty, _value_ty: &Ty) -> IrExpr {
    let (module, func) = match field_ty {
        Ty::String => ("value", "as_string"),
        Ty::Int => ("value", "as_int"),
        Ty::Float => ("value", "as_float"),
        Ty::Bool => ("value", "as_bool"),
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            let inner = &args[0];
            let func_name = if let Ty::Named(name, _) = inner {
                format!("__decode_list_{}", name)
            } else {
                format!("__decode_list_{}", decode_func_suffix(inner))
            };
            return IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym(&func_name) },
                        args: vec![get_field_expr],
                        type_args: vec![],
                    },
                    ty: Ty::result(field_ty.clone(), Ty::String), span: None, def_id: None,
                })},
                ty: field_ty.clone(), span: None, def_id: None,
            };
        }
        _ => {
            // Named type → Type.decode(value)?
            if let Ty::Named(name, _) = field_ty {
                return IrExpr {
                    kind: IrExprKind::Try { expr: Box::new(IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Named { name: sym(&format!("{}.decode", name)) },
                            args: vec![get_field_expr],
                            type_args: vec![],
                        },
                        ty: Ty::result(field_ty.clone(), Ty::String), span: None, def_id: None,
                    })},
                    ty: field_ty.clone(), span: None, def_id: None,
                };
            }
            ("value", "as_string") // fallback
        }
    };
    // value.as_TYPE(field_value)?
    IrExpr {
        kind: IrExprKind::Try { expr: Box::new(IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None },
                args: vec![get_field_expr],
                type_args: vec![],
            },
            ty: Ty::result(field_ty.clone(), Ty::String), span: None, def_id: None,
        })},
        ty: field_ty.clone(), span: None, def_id: None,
    }
}

include!("derive_codec_p2.rs");
