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
    let mut pairs: Vec<IrExpr> = Vec::new();
    for f in fields {
        let field_access = IrExpr {
            kind: IrExprKind::Member {
                object: Box::new(IrExpr { kind: IrExprKind::Var { id: var }, ty: type_ty.clone(), span: None, def_id: None }),
                field: f.name,
            },
            ty: f.ty.clone(), span: None, def_id: None,
        };
        // Choose value constructor based on field type
        let encoded = encode_element(vt, &field_access, &f.ty, &value_ty);
        pairs.push(IrExpr {
            kind: IrExprKind::Tuple { elements: vec![
                IrExpr { kind: IrExprKind::LitStr { value: f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string()) }, ty: Ty::String, span: None, def_id: None },
                encoded,
            ]},
            ty: Ty::Tuple(vec![Ty::String, value_ty.clone()]), span: None, def_id: None,
        });
    }

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
        params: vec![IrParam { var, ty: type_ty.clone(), name: sym("_v"), borrow: ParamBorrow::Own, open_record: None, default: None, attrs: vec![] }],
        ret_ty: value_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}

/// A `value.<func>(args...) -> Value` module call.
fn value_call(func: &str, args: Vec<IrExpr>, value_ty: &Ty) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("value"), func: sym(func), def_id: None },
            args, type_args: vec![],
        },
        ty: value_ty.clone(), span: None, def_id: None,
    }
}

/// Encode a value of `field_ty` (the expression `field_expr`) to a `Value`.
/// List/Option route through the bundled-Almide `value.encode_list`/
/// `encode_option` generics with a lambda element-encoder, so monomorphization
/// instantiates them and the element constructors link. One source for both
/// targets — no native helpers, no codegen synthetics.
fn encode_element(vt: &mut VarTable, field_expr: &IrExpr, field_ty: &Ty, value_ty: &Ty) -> IrExpr {
    match field_ty {
        Ty::String => value_call("str", vec![field_expr.clone()], value_ty),
        Ty::Int => value_call("int", vec![field_expr.clone()], value_ty),
        Ty::Float => value_call("float", vec![field_expr.clone()], value_ty),
        Ty::Bool => value_call("bool", vec![field_expr.clone()], value_ty),
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            let lam = encode_lambda(vt, &args[0], value_ty);
            value_call("encode_list", vec![field_expr.clone(), lam], value_ty)
        }
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => {
            let lam = encode_lambda(vt, &args[0], value_ty);
            value_call("encode_option", vec![field_expr.clone(), lam], value_ty)
        }
        Ty::Named(name, _) => IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: sym(&format!("{}.encode", name)) },
                args: vec![field_expr.clone()], type_args: vec![],
            },
            ty: value_ty.clone(), span: None, def_id: None,
        },
        _ => value_call("str", vec![field_expr.clone()], value_ty),
    }
}

/// `(x) => <encode x>` — element encoder lambda for list/option codec helpers.
fn encode_lambda(vt: &mut VarTable, inner: &Ty, value_ty: &Ty) -> IrExpr {
    let x = vt.alloc(sym("_x"), inner.clone(), Mutability::Let, None);
    let x_ref = IrExpr { kind: IrExprKind::Var { id: x }, ty: inner.clone(), span: None, def_id: None };
    let body = encode_element(vt, &x_ref, inner, value_ty);
    IrExpr {
        kind: IrExprKind::Lambda { params: vec![(x, inner.clone())], body: Box::new(body), lambda_id: None },
        ty: Ty::Fn { params: vec![inner.clone()], ret: Box::new(value_ty.clone()) },
        span: None, def_id: None,
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
            // value.decode_option(_v, "key", (v) => <decode inner>)? → Option[T]
            let lam = decode_lambda(vt, &inner_ty, &value_ty);
            IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: sym("value"), func: sym("decode_option"), def_id: None },
                        args: vec![
                            IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None, def_id: None },
                            IrExpr { kind: IrExprKind::LitStr { value: key_name(f) }, ty: Ty::String, span: None, def_id: None },
                            lam,
                        ],
                        type_args: vec![],
                    },
                    ty: Ty::result(f.ty.clone(), Ty::String), span: None, def_id: None,
                })},
                ty: f.ty.clone(), span: None, def_id: None,
            }
        } else if has_default {
            // value.decode_with_default(_v, "key", default, (v) => <decode T>)?
            let default_expr = f.default.clone().unwrap_or(IrExpr { kind: IrExprKind::Unit, ty: f.ty.clone(), span: None, def_id: None });
            let lam = decode_lambda(vt, &f.ty, &value_ty);
            IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: sym("value"), func: sym("decode_with_default"), def_id: None },
                        args: vec![
                            IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None, def_id: None },
                            IrExpr { kind: IrExprKind::LitStr { value: key_name(f) }, ty: Ty::String, span: None, def_id: None },
                            default_expr,
                            lam,
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
            decode_field_value(vt, get_and_try, &f.ty, &value_ty)
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
                (*name, IrExpr { kind: IrExprKind::Var { id: *var }, ty: Ty::Unknown, span: None, def_id: None })
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
        params: vec![IrParam { var: var_v, ty: value_ty, name: sym("_v"), borrow: ParamBorrow::Own, open_record: None, default: None, attrs: vec![] }],
        ret_ty: result_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}


/// Decode `val_expr` (a Value) to `Result[field_ty, String]` — NO Try wrapper.
/// List routes through `value.decode_list` with a lambda element-decoder so mono
/// instantiates the generic and the element extractor links.
fn decode_element(vt: &mut VarTable, val_expr: IrExpr, field_ty: &Ty, value_ty: &Ty) -> IrExpr {
    let res_ty = Ty::result(field_ty.clone(), Ty::String);
    let module_result = |func: &str, args: Vec<IrExpr>| IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("value"), func: sym(func), def_id: None },
            args, type_args: vec![],
        },
        ty: res_ty.clone(), span: None, def_id: None,
    };
    match field_ty {
        Ty::String => module_result("as_string", vec![val_expr]),
        Ty::Int => module_result("as_int", vec![val_expr]),
        Ty::Float => module_result("as_float", vec![val_expr]),
        Ty::Bool => module_result("as_bool", vec![val_expr]),
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            let lam = decode_lambda(vt, &args[0], value_ty);
            module_result("decode_list", vec![val_expr, lam])
        }
        Ty::Named(name, _) => IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: sym(&format!("{}.decode", name)) },
                args: vec![val_expr], type_args: vec![],
            },
            ty: res_ty, span: None, def_id: None,
        },
        _ => module_result("as_string", vec![val_expr]),
    }
}

/// `(v) => <decode v>` — element decoder lambda (returns Result[inner, String]).
fn decode_lambda(vt: &mut VarTable, inner: &Ty, value_ty: &Ty) -> IrExpr {
    let v = vt.alloc(sym("_dv"), value_ty.clone(), Mutability::Let, None);
    let v_ref = IrExpr { kind: IrExprKind::Var { id: v }, ty: value_ty.clone(), span: None, def_id: None };
    let body = decode_element(vt, v_ref, inner, value_ty);
    IrExpr {
        kind: IrExprKind::Lambda { params: vec![(v, value_ty.clone())], body: Box::new(body), lambda_id: None },
        ty: Ty::Fn { params: vec![value_ty.clone()], ret: Box::new(Ty::result(inner.clone(), Ty::String)) },
        span: None, def_id: None,
    }
}

/// Required field: `decode_element(get_field_expr, field_ty)?`.
fn decode_field_value(vt: &mut VarTable, get_field_expr: IrExpr, field_ty: &Ty, value_ty: &Ty) -> IrExpr {
    let decoded = decode_element(vt, get_field_expr, field_ty, value_ty);
    IrExpr {
        kind: IrExprKind::Try { expr: Box::new(decoded) },
        ty: field_ty.clone(), span: None, def_id: None,
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
                 IrExpr { kind: IrExprKind::Call { target: CallTarget::Module { module: sym("value"), func: sym("null"), def_id: None }, args: vec![], type_args: vec![] }, ty: value_ty.clone(), span: None, def_id: None })
            }
            IrVariantKind::Tuple { fields } => {
                let mut pat_vars = vec![];
                let mut encode_elems = vec![];
                for (i, field_ty) in fields.iter().enumerate() {
                    let pv = vt.alloc(sym(&format!("_f{}", i)), field_ty.clone(), Mutability::Let, None);
                    pat_vars.push(IrPattern::Bind { var: pv, ty: field_ty.clone() });
                    let field_expr = IrExpr { kind: IrExprKind::Var { id: pv }, ty: field_ty.clone(), span: None, def_id: None };
                    encode_elems.push(encode_element(vt, &field_expr, field_ty, &value_ty));
                }
                (IrPattern::Constructor { name: case.name.to_string(), args: pat_vars },
                 IrExpr { kind: IrExprKind::Call { target: CallTarget::Module { module: sym("value"), func: sym("array"), def_id: None }, args: vec![IrExpr { kind: IrExprKind::List { elements: encode_elems }, ty: Ty::list(value_ty.clone()), span: None, def_id: None }], type_args: vec![] }, ty: value_ty.clone(), span: None, def_id: None })
            }
            IrVariantKind::Record { fields } => {
                let mut pat_fields = vec![];
                let mut encode_pairs = vec![];
                for f in fields {
                    let pv = vt.alloc(sym(&format!("_{}", f.name)), f.ty.clone(), Mutability::Let, None);
                    pat_fields.push(IrFieldPattern { name: f.name.to_string(), pattern: Some(IrPattern::Bind { var: pv, ty: f.ty.clone() }) });
                    let field_expr = IrExpr { kind: IrExprKind::Var { id: pv }, ty: f.ty.clone(), span: None, def_id: None };
                    let val = encode_element(vt, &field_expr, &f.ty, &value_ty);
                    encode_pairs.push(IrExpr { kind: IrExprKind::Tuple { elements: vec![
                        IrExpr { kind: IrExprKind::LitStr { value: f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string()) }, ty: Ty::String, span: None, def_id: None },
                        val,
                    ]}, ty: Ty::Tuple(vec![Ty::String, value_ty.clone()]), span: None, def_id: None });
                }
                (IrPattern::RecordPattern { name: case.name.to_string(), fields: pat_fields, rest: false },
                 IrExpr { kind: IrExprKind::Call { target: CallTarget::Module { module: sym("value"), func: sym("object"), def_id: None }, args: vec![IrExpr { kind: IrExprKind::List { elements: encode_pairs }, ty: Ty::list(Ty::Tuple(vec![Ty::String, value_ty.clone()])), span: None, def_id: None }], type_args: vec![] }, ty: value_ty.clone(), span: None, def_id: None })
            }
        };
        // Wrap payload in {"CaseName": payload}
        let tagged = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("value"), func: sym("object"), def_id: None },
                args: vec![IrExpr { kind: IrExprKind::List { elements: vec![IrExpr { kind: IrExprKind::Tuple { elements: vec![
                    IrExpr { kind: IrExprKind::LitStr { value: case.name.to_string() }, ty: Ty::String, span: None, def_id: None },
                    payload_value,
                ]}, ty: Ty::Tuple(vec![Ty::String, value_ty.clone()]), span: None, def_id: None }] }, ty: Ty::list(Ty::Tuple(vec![Ty::String, value_ty.clone()])), span: None, def_id: None }],
                type_args: vec![],
            },
            ty: value_ty.clone(), span: None, def_id: None,
        };
        IrMatchArm { pattern, guard: None, body: tagged }
    }).collect();

    let body = IrExpr {
        kind: IrExprKind::Match { subject: Box::new(IrExpr { kind: IrExprKind::Var { id: var }, ty: type_ty.clone(), span: None, def_id: None }), arms },
        ty: value_ty.clone(), span: None, def_id: None,
    };

    IrFunction {
        name: sym(&format!("{}.encode", type_name)),
        params: vec![IrParam { var, ty: type_ty.clone(), name: sym("_v"), borrow: ParamBorrow::Own, open_record: None, default: None, attrs: vec![] }],
        ret_ty: value_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
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
                        target: CallTarget::Module { module: sym("value"), func: sym("tagged_variant"), def_id: None },
                        args: vec![IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None, def_id: None }],
                        type_args: vec![],
                    },
                    ty: Ty::result(Ty::Tuple(vec![Ty::String, value_ty.clone()]), Ty::String), span: None, def_id: None,
                })},
                ty: Ty::Tuple(vec![Ty::String, value_ty.clone()]), span: None, def_id: None,
            },
        },
        span: None,
    };

    // Build if-else chain: if tag == "Circle" then ... else if tag == "Rect" then ... else err
    let mut else_expr = IrExpr {
        kind: IrExprKind::ResultErr { expr: Box::new(IrExpr {
            kind: IrExprKind::LitStr { value: format!("unknown variant for {}", type_name) },
            ty: Ty::String, span: None, def_id: None,
        })},
        ty: result_ty.clone(), span: None, def_id: None,
    };

    for case in cases.iter().rev() {
        let tag_check = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::Eq,
                left: Box::new(IrExpr { kind: IrExprKind::Var { id: var_tag }, ty: Ty::String, span: None, def_id: None }),
                right: Box::new(IrExpr { kind: IrExprKind::LitStr { value: case.name.to_string() }, ty: Ty::String, span: None, def_id: None }),
            },
            ty: Ty::Bool, span: None, def_id: None,
        };

        let construct = match &case.kind {
            IrVariantKind::Unit => {
                IrExpr {
                    kind: IrExprKind::ResultOk { expr: Box::new(IrExpr {
                        kind: IrExprKind::Call { target: CallTarget::Named { name: case.name }, args: vec![], type_args: vec![] },
                        ty: type_ty.clone(), span: None, def_id: None,
                    })},
                    ty: result_ty.clone(), span: None, def_id: None,
                }
            }
            _ => {
                // For Tuple/Record variants, just wrap in Ok for now (payload decode is complex)
                IrExpr {
                    kind: IrExprKind::ResultErr { expr: Box::new(IrExpr {
                        kind: IrExprKind::LitStr { value: format!("variant {} payload decode not yet implemented", case.name) },
                        ty: Ty::String, span: None, def_id: None,
                    })},
                    ty: result_ty.clone(), span: None, def_id: None,
                }
            }
        };

        else_expr = IrExpr {
            kind: IrExprKind::If { cond: Box::new(tag_check), then: Box::new(construct), else_: Box::new(else_expr) },
            ty: result_ty.clone(), span: None, def_id: None,
        };
    }

    let body = IrExpr {
        kind: IrExprKind::Block { stmts: vec![extract], expr: Some(Box::new(else_expr)) },
        ty: result_ty.clone(), span: None, def_id: None,
    };

    IrFunction {
        name: sym(&format!("{}.decode", type_name)),
        params: vec![IrParam { var: var_v, ty: value_ty, name: sym("_v"), borrow: ParamBorrow::Own, open_record: None, default: None, attrs: vec![] }],
        ret_ty: result_ty,
        body,
        is_effect: false, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}
