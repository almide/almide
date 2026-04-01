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
                object: Box::new(IrExpr { kind: IrExprKind::Var { id: var }, ty: type_ty.clone(), span: None }),
                field: f.name,
            },
            ty: f.ty.clone(), span: None,
        };
        // Choose value constructor based on field type
        let value_call = encode_field_value(&field_access, &f.ty, &value_ty);
        IrExpr {
            kind: IrExprKind::Tuple { elements: vec![
                IrExpr { kind: IrExprKind::LitStr { value: f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string()) }, ty: Ty::String, span: None },
                value_call,
            ]},
            ty: Ty::Tuple(vec![Ty::String, value_ty.clone()]), span: None,
        }
    }).collect();

    let pairs_list = IrExpr {
        kind: IrExprKind::List { elements: pairs },
        ty: Ty::list(Ty::Tuple(vec![Ty::String, value_ty.clone()])), span: None,
    };

    let body = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("value"), func: sym("object") },
            args: vec![pairs_list],
            type_args: vec![],
        },
        ty: value_ty.clone(), span: None,
    };

    IrFunction {
        name: sym(&format!("{}.encode", type_name)),
        params: vec![IrParam { var, ty: type_ty.clone(), name: sym("_v"), borrow: ParamBorrow::Own, open_record: None, default: None }],
        ret_ty: value_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
    }
}

/// Choose the right value constructor for a field type.
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
                    target: CallTarget::Named { name: sym(&format!("__encode_option_{}", decode_func_suffix(inner))) },
                    args: vec![field_expr.clone()],
                    type_args: vec![],
                },
                ty: value_ty.clone(), span: None,
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
                ty: value_ty.clone(), span: None,
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
                    ty: value_ty.clone(), span: None,
                };
            }
            // Fallback: value.str(to_string(field))
            ("value", "str")
        }
    };
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym(module), func: sym(func) },
            args: vec![field_expr.clone()],
            type_args: vec![],
        },
        ty: value_ty.clone(), span: None,
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
                target: CallTarget::Module { module: sym("value"), func: sym("field") },
                args: vec![
                    IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None },
                    IrExpr { kind: IrExprKind::LitStr { value: key_name(f) }, ty: Ty::String, span: None },
                ],
                type_args: vec![],
            },
            ty: Ty::result(value_ty.clone(), Ty::String), span: None,
        };

        let decode_expr = if is_option {
            // Option[T]: use runtime helper value_decode_option(_v, "key", as_T)
            // Returns Result[Option[T], String]
            IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym(&format!("__decode_option_{}", decode_func_suffix(&inner_ty))) },
                        args: vec![
                            IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None },
                            IrExpr { kind: IrExprKind::LitStr { value: key_name(f) }, ty: Ty::String, span: None },
                        ],
                        type_args: vec![],
                    },
                    ty: Ty::result(f.ty.clone(), Ty::String), span: None,
                })},
                ty: f.ty.clone(), span: None,
            }
        } else if has_default {
            // Default: use runtime helper value_decode_with_default(_v, "key", default, as_T)
            let default_expr = f.default.clone().unwrap_or(IrExpr { kind: IrExprKind::Unit, ty: f.ty.clone(), span: None });
            IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym(&format!("__decode_default_{}", decode_func_suffix(&f.ty))) },
                        args: vec![
                            IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None },
                            IrExpr { kind: IrExprKind::LitStr { value: key_name(f) }, ty: Ty::String, span: None },
                            default_expr,
                        ],
                        type_args: vec![],
                    },
                    ty: Ty::result(f.ty.clone(), Ty::String), span: None,
                })},
                ty: f.ty.clone(), span: None,
            }
        } else {
            // Required: value.field(_v, "key")? |> as_T?
            let get_and_try = IrExpr {
                kind: IrExprKind::Try { expr: Box::new(get_field_call) },
                ty: value_ty.clone(), span: None,
            };
            decode_field_value(get_and_try, &f.ty, &value_ty)
        };

        stmts.push(IrStmt {
            kind: IrStmtKind::Bind { var: field_var, mutability: Mutability::Let, ty: f.ty.clone(), value: decode_expr },
            span: None,
        });
        field_vars.push((f.name, field_var));
    }

    // ok(TypeName { field1: _field1, field2: _field2, ... })
    let record = IrExpr {
        kind: IrExprKind::Record {
            name: Some(sym(type_name)),
            fields: field_vars.iter().map(|(name, var)| {
                (*name, IrExpr { kind: IrExprKind::Var { id: *var }, ty: Ty::Unknown, span: None })
            }).collect(),
        },
        ty: type_ty.clone(), span: None,
    };

    let body = IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(IrExpr {
                kind: IrExprKind::ResultOk { expr: Box::new(record) },
                ty: result_ty.clone(), span: None,
            })),
        },
        ty: result_ty.clone(), span: None,
    };

    IrFunction {
        name: sym(&format!("{}.decode", type_name)),
        params: vec![IrParam { var: var_v, ty: value_ty, name: sym("_v"), borrow: ParamBorrow::Own, open_record: None, default: None }],
        ret_ty: result_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
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
                    ty: Ty::result(field_ty.clone(), Ty::String), span: None,
                })},
                ty: field_ty.clone(), span: None,
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
                        ty: Ty::result(field_ty.clone(), Ty::String), span: None,
                    })},
                    ty: field_ty.clone(), span: None,
                };
            }
            ("value", "as_string") // fallback
        }
    };
    // value.as_TYPE(field_value)?
    IrExpr {
        kind: IrExprKind::Try { expr: Box::new(IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym(module), func: sym(func) },
                args: vec![get_field_expr],
                type_args: vec![],
            },
            ty: Ty::result(field_ty.clone(), Ty::String), span: None,
        })},
        ty: field_ty.clone(), span: None,
    }
}

/// Auto-derive Variant Codec encode: Tagged format
/// Circle(3.0) → Object([("Circle", Object([("radius", Float(3.0))]))])
pub(super) fn auto_derive_variant_encode(vt: &mut VarTable, type_name: &str, type_ty: &Ty, cases: &[IrVariantDecl]) -> IrFunction {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let var = vt.alloc(sym("_v"), type_ty.clone(), Mutability::Let, None);

    // Build match arms for each variant case
    let arms: Vec<IrMatchArm> = cases.iter().map(|case| {
        let (pattern, payload_value) = match &case.kind {
            IrVariantKind::Unit => {
                (IrPattern::Constructor { name: case.name.to_string(), args: vec![] },
                 IrExpr { kind: IrExprKind::Call { target: CallTarget::Named { name: sym("almide_rt_value_null") }, args: vec![], type_args: vec![] }, ty: value_ty.clone(), span: None })
            }
            IrVariantKind::Tuple { fields } => {
                let mut pat_vars = vec![];
                let mut encode_elems = vec![];
                for (i, field_ty) in fields.iter().enumerate() {
                    let pv = vt.alloc(sym(&format!("_f{}", i)), field_ty.clone(), Mutability::Let, None);
                    pat_vars.push(IrPattern::Bind { var: pv, ty: field_ty.clone() });
                    let field_expr = IrExpr { kind: IrExprKind::Var { id: pv }, ty: field_ty.clone(), span: None };
                    encode_elems.push(encode_field_value(&field_expr, field_ty, &value_ty));
                }
                (IrPattern::Constructor { name: case.name.to_string(), args: pat_vars },
                 IrExpr { kind: IrExprKind::Call { target: CallTarget::Named { name: sym("almide_rt_value_array") }, args: vec![IrExpr { kind: IrExprKind::List { elements: encode_elems }, ty: Ty::list(value_ty.clone()), span: None }], type_args: vec![] }, ty: value_ty.clone(), span: None })
            }
            IrVariantKind::Record { fields } => {
                let mut pat_fields = vec![];
                let mut encode_pairs = vec![];
                for f in fields {
                    let pv = vt.alloc(sym(&format!("_{}", f.name)), f.ty.clone(), Mutability::Let, None);
                    pat_fields.push(IrFieldPattern { name: f.name.to_string(), pattern: Some(IrPattern::Bind { var: pv, ty: f.ty.clone() }) });
                    let field_expr = IrExpr { kind: IrExprKind::Var { id: pv }, ty: f.ty.clone(), span: None };
                    let val = encode_field_value(&field_expr, &f.ty, &value_ty);
                    encode_pairs.push(IrExpr { kind: IrExprKind::Tuple { elements: vec![
                        IrExpr { kind: IrExprKind::LitStr { value: f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string()) }, ty: Ty::String, span: None },
                        val,
                    ]}, ty: Ty::Tuple(vec![Ty::String, value_ty.clone()]), span: None });
                }
                (IrPattern::RecordPattern { name: case.name.to_string(), fields: pat_fields, rest: false },
                 IrExpr { kind: IrExprKind::Call { target: CallTarget::Named { name: sym("almide_rt_value_object") }, args: vec![IrExpr { kind: IrExprKind::List { elements: encode_pairs }, ty: Ty::list(Ty::Tuple(vec![Ty::String, value_ty.clone()])), span: None }], type_args: vec![] }, ty: value_ty.clone(), span: None })
            }
        };
        // Wrap payload in {"CaseName": payload}
        let tagged = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: sym("almide_rt_value_object") },
                args: vec![IrExpr { kind: IrExprKind::List { elements: vec![IrExpr { kind: IrExprKind::Tuple { elements: vec![
                    IrExpr { kind: IrExprKind::LitStr { value: case.name.to_string() }, ty: Ty::String, span: None },
                    payload_value,
                ]}, ty: Ty::Tuple(vec![Ty::String, value_ty.clone()]), span: None }] }, ty: Ty::list(Ty::Tuple(vec![Ty::String, value_ty.clone()])), span: None }],
                type_args: vec![],
            },
            ty: value_ty.clone(), span: None,
        };
        IrMatchArm { pattern, guard: None, body: tagged }
    }).collect();

    let body = IrExpr {
        kind: IrExprKind::Match { subject: Box::new(IrExpr { kind: IrExprKind::Var { id: var }, ty: type_ty.clone(), span: None }), arms },
        ty: value_ty.clone(), span: None,
    };

    IrFunction {
        name: sym(&format!("{}.encode", type_name)),
        params: vec![IrParam { var, ty: type_ty.clone(), name: sym("_v"), borrow: ParamBorrow::Own, open_record: None, default: None }],
        ret_ty: value_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
    }
}

/// Auto-derive Variant Codec decode: Tagged format
/// {"Circle": {"radius": 3.0}} → Circle(3.0)
pub(super) fn auto_derive_variant_decode(vt: &mut VarTable, type_name: &str, type_ty: &Ty, cases: &[IrVariantDecl]) -> IrFunction {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let result_ty = Ty::result(type_ty.clone(), Ty::String);
    let var_v = vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);

    // let (tag, payload) = almide_rt_value_tagged_variant(_v)?
    let var_tag = vt.alloc(sym("_tag"), Ty::String, Mutability::Let, None);
    let var_payload = vt.alloc(sym("_payload"), value_ty.clone(), Mutability::Let, None);

    let extract = IrStmt {
        kind: IrStmtKind::BindDestructure {
            pattern: IrPattern::Tuple { elements: vec![IrPattern::Bind { var: var_tag, ty: Ty::String }, IrPattern::Bind { var: var_payload, ty: Ty::String }] },
            value: IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym("almide_rt_value_tagged_variant") },
                        args: vec![IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None }],
                        type_args: vec![],
                    },
                    ty: Ty::result(Ty::Tuple(vec![Ty::String, value_ty.clone()]), Ty::String), span: None,
                })},
                ty: Ty::Tuple(vec![Ty::String, value_ty.clone()]), span: None,
            },
        },
        span: None,
    };

    // Build if-else chain: if tag == "Circle" then ... else if tag == "Rect" then ... else err
    let mut else_expr = IrExpr {
        kind: IrExprKind::ResultErr { expr: Box::new(IrExpr {
            kind: IrExprKind::LitStr { value: format!("unknown variant for {}", type_name) },
            ty: Ty::String, span: None,
        })},
        ty: result_ty.clone(), span: None,
    };

    for case in cases.iter().rev() {
        let tag_check = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::Eq,
                left: Box::new(IrExpr { kind: IrExprKind::Var { id: var_tag }, ty: Ty::String, span: None }),
                right: Box::new(IrExpr { kind: IrExprKind::LitStr { value: case.name.to_string() }, ty: Ty::String, span: None }),
            },
            ty: Ty::Bool, span: None,
        };

        let construct = match &case.kind {
            IrVariantKind::Unit => {
                IrExpr {
                    kind: IrExprKind::ResultOk { expr: Box::new(IrExpr {
                        kind: IrExprKind::Call { target: CallTarget::Named { name: case.name }, args: vec![], type_args: vec![] },
                        ty: type_ty.clone(), span: None,
                    })},
                    ty: result_ty.clone(), span: None,
                }
            }
            _ => {
                // For Tuple/Record variants, just wrap in Ok for now (payload decode is complex)
                IrExpr {
                    kind: IrExprKind::ResultErr { expr: Box::new(IrExpr {
                        kind: IrExprKind::LitStr { value: format!("variant {} payload decode not yet implemented", case.name) },
                        ty: Ty::String, span: None,
                    })},
                    ty: result_ty.clone(), span: None,
                }
            }
        };

        else_expr = IrExpr {
            kind: IrExprKind::If { cond: Box::new(tag_check), then: Box::new(construct), else_: Box::new(else_expr) },
            ty: result_ty.clone(), span: None,
        };
    }

    let body = IrExpr {
        kind: IrExprKind::Block { stmts: vec![extract], expr: Some(Box::new(else_expr)) },
        ty: result_ty.clone(), span: None,
    };

    IrFunction {
        name: sym(&format!("{}.decode", type_name)),
        params: vec![IrParam { var: var_v, ty: value_ty, name: sym("_v"), borrow: ParamBorrow::Own, open_record: None, default: None }],
        ret_ty: result_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
    }
}
