// Auto-derive Codec, the decode half. `include!`d by derive_codec.rs
// (the 800-line file budget); it shares that module's scope and imports.

/// Auto-derive Codec decode: `fn T.decode(v: Value) -> Result[T, String]`
pub(super) fn auto_derive_decode(wk: &mut CodecWk, type_ty: &Ty, fields: &[IrFieldDecl]) -> IrFunction {
    let type_name = wk.type_name.to_string();
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let result_ty = Ty::result(type_ty.clone(), Ty::String);
    let var_v = wk.vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);

    let mut stmts = Vec::new();
    let mut field_vars = Vec::new();
    let key_name = |f: &IrFieldDecl| -> String { f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string()) };

    for f in fields {
        let is_option = f.ty.is_option();
        let has_default = f.default.is_some();
        let inner_ty = f.ty.inner().cloned().unwrap_or_else(|| f.ty.clone());
        // `_f_` prefix, NOT `_{name}`: the decode param renders as `_v`, so a
        // field literally named `v` would shadow the document in the emitted
        // Rust and every later field would read the decoded value instead of
        // the doc ("expected Object" at runtime).
        let field_var = wk.vt.alloc(sym(&format!("_f_{}", f.name)), f.ty.clone(), Mutability::Let, None);

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
            let payload = IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None, def_id: None };
            if let Some(inline) = decode_option_field_inline(wk, payload.clone(), &key_name(f), &inner_ty, &value_ty) {
                inline
            } else {
            // Option[T]: use runtime helper value_decode_option(_v, "key", as_T)
            // Returns Result[Option[T], String]
            IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym(&option_codec_fn("decode", &inner_ty)) },
                        args: vec![
                            payload,
                            IrExpr { kind: IrExprKind::LitStr { value: key_name(f) }, ty: Ty::String, span: None, def_id: None },
                        ],
                        type_args: vec![],
                    },
                    ty: Ty::result(f.ty.clone(), Ty::String), span: None, def_id: None,
                })},
                ty: f.ty.clone(), span: None, def_id: None,
            }
            }
        } else if has_default {
            let mut default_expr = f.default.clone().unwrap_or(IrExpr { kind: IrExprKind::Unit, ty: f.ty.clone(), span: None, def_id: None });
            // A field default parses in declaration position and can arrive
            // with ty=Unknown (a record literal is never inferred there); the
            // v1 record builder classifies heap-ness from expr.ty, so stamp
            // the declared field type on it (#1522).
            if matches!(default_expr.ty, Ty::Unknown) { default_expr.ty = f.ty.clone(); }
            let suffix = decode_func_suffix(&f.ty);
            if suffix == "value" {
                // #1522: no concrete runtime helper exists for this default's
                // type (`__decode_default_value` names a function no runtime
                // provides — check green, rustc E0425). Route through the
                // field's OWN decode path inline, with the helper family's
                // exact semantics: a missing key or an explicit null yields
                // the default, a present value decodes strictly.
                let fv = wk.vt.alloc(sym(&format!("_dv_{}", f.name)), value_ty.clone(), Mutability::Let, None);
                let fv_expr = e_(IrExprKind::Var { id: fv }, value_ty.clone());
                // `==` on two Values, as user code lowers it (BinOp::Eq —
                // every target's eq lowering handles it). Not a `value.eq`
                // Module call: no such fn exists in `stdlib/value.almd`, so
                // ResolveCalls' postcondition names it unresolved (#2186).
                let is_null = e_(IrExprKind::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(fv_expr.clone()),
                    right: Box::new(call_mod_("value", "null", vec![], value_ty.clone())),
                }, Ty::Bool);
                let decoded = dec_field_expr(wk, fv_expr, &f.ty, &value_ty, &key_name(f));
                IrExpr {
                    kind: IrExprKind::Match {
                        subject: Box::new(get_field_call),
                        arms: vec![
                            IrMatchArm {
                                pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: fv, ty: value_ty.clone() }) },
                                guard: None,
                                body: e_(IrExprKind::If {
                                    cond: Box::new(is_null),
                                    then: Box::new(default_expr.clone()),
                                    else_: Box::new(decoded),
                                }, f.ty.clone()),
                            },
                            IrMatchArm {
                                pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                                guard: None,
                                body: default_expr,
                            },
                        ],
                    },
                    ty: f.ty.clone(), span: None, def_id: None,
                }
            } else {
            // Default: use runtime helper value_decode_with_default(_v, "key", default, as_T)
            IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym(&format!("__decode_default_{}", suffix)) },
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
            }
        } else {
            // Required: value.field(_v, "key")? |> as_T?
            let get_and_try = IrExpr {
                kind: IrExprKind::Try { expr: Box::new(get_field_call) },
                ty: value_ty.clone(), span: None, def_id: None,
            };
            dec_field_expr(wk, get_and_try, &f.ty, &value_ty, &key_name(f))
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
            name: Some(sym(&type_name)),
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

    // `@codec_slots("k0", "k1", …)` (#1679): the object keys in DECLARATION
    // order — the order `T.encode` writes them, so the order a round-tripped
    // document carries them. The native `DecodeSlotHintPass` reads it to hand
    // every `value.field(_v, "k_i")` in this body its slot index; the wasm leg
    // ignores the attribute, and a user-written decode never carries it.
    let codec_slots = almide_lang::ast::Attribute {
        name: sym("codec_slots"),
        args: fields.iter().map(|f| almide_lang::ast::AttrArg {
            name: None,
            value: almide_lang::ast::AttrValue::String { value: key_name(f) },
        }).collect(),
        span: None,
    };

    IrFunction {
        name: sym(&format!("{}.decode", type_name)),
        params: vec![IrParam { var: var_v, ty: value_ty, name: sym("_v"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }],
        ret_ty: result_ty,
        body,
        is_effect: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![codec_slots], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None, // fresh-fn: derived codec worker, no params carry mut
    }
}

fn decode_func_suffix(ty: &Ty) -> &'static str {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::String => "string",
        Ty::Int => "int",
        Ty::Float => "float",
        Ty::Bool => "bool",
        // List[scalar] defaults (#1520): route to the concrete list helpers —
        // the bare "value" fallback names a helper no runtime provides
        // (rustc E0425 with check green). Exotic default types keep the
        // fallback and are rejected at CHECK time by the Codec field rule.
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => match &a[0] {
            Ty::String => "list_string",
            Ty::Int => "list_int",
            Ty::Float => "list_float",
            Ty::Bool => "list_bool",
            _ => "value",
        },
        _ => "value",
    }
}

/// Generate decode expression for a field based on its type.
fn decode_field_value(get_field_expr: IrExpr, field_ty: &Ty, _value_ty: &Ty) -> IrExpr {
    // `Value` passes through verbatim (see encode_field_value).
    if is_value_ty(field_ty) {
        return get_field_expr;
    }
    let (module, func) = match field_ty {
        Ty::String => ("value", "as_string"),
        Ty::Int => ("value", "as_int"),
        Ty::Float => ("value", "as_float"),
        Ty::Bool => ("value", "as_bool"),
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            let inner = &args[0];
            if is_value_ty(inner) {
                // List[Value]: elements stay wire values — as_array is the whole decode.
                return IrExpr {
                    kind: IrExprKind::Try { expr: Box::new(IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Module { module: sym("value"), func: sym("as_array"), def_id: None },
                            args: vec![get_field_expr],
                            type_args: vec![],
                        },
                        ty: Ty::result(field_ty.clone(), Ty::String), span: None, def_id: None,
                    })},
                    ty: field_ty.clone(), span: None, def_id: None,
                };
            }
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
