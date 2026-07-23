// Continuation of derive_codec.rs — variant Codec derive (encode/decode) and
// generic container Codec helper functions. Split out to keep derive_codec.rs
// under the 800-line codopsy max-lines threshold; pure text move, same file
// scope via `include!` (inherits derive_codec.rs's `use` imports — mirrors
// the mod_p2.rs/mod_p3.rs pattern already used elsewhere in this crate).

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
                    encode_elems.push(encode_field_value(&field_expr, field_ty, &value_ty));
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
                    let val = encode_field_value(&field_expr, &f.ty, &value_ty);
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

/// Auto-derive Variant Codec decode: Tagged format
/// {"Circle": {"radius": 3.0}} → Circle(3.0)
pub(super) fn auto_derive_variant_decode(vt: &mut VarTable, type_name: &str, type_ty: &Ty, cases: &[IrVariantDecl]) -> IrFunction {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let result_ty = Ty::result(type_ty.clone(), Ty::String);
    let var_v = vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);

    // TUPLE-FREE tag/payload split (so neither side materializes a `(String, Value)` tuple the
    // trust-spine cannot lower): read the tag via `value.variant_tag(_v)` (Result[String, String])
    // and the payload via `value.field(_v, _tag)` (Result[Value, String]) — both simple Results the
    // trust-spine already lowers, wrapped as `match tag { ok(_tag) => match field { ok(_payload) =>
    // <if-chain>, err(e) => err(e) }, err(e) => err(e) }`.
    let var_tag = vt.alloc(sym("_tag"), Ty::String, Mutability::Let, None);
    let var_payload = vt.alloc(sym("_payload"), value_ty.clone(), Mutability::Let, None);
    let var_e2 = vt.alloc(sym("_e2"), Ty::String, Mutability::Let, None);

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
            IrVariantKind::Tuple { fields } => {
                // Payload is a positional array `[e0, e1, …]` (see variant encode).
                // Bind `_arr = value.as_array(payload)?`, decode each element by its
                // field type, then `Ok(Ctor(e0, e1, …))`.
                let arr_ty = Ty::list(value_ty.clone());
                let arr_var = vt.alloc(sym("_arr"), arr_ty.clone(), Mutability::Let, None);
                let as_array = IrExpr {
                    kind: IrExprKind::Try { expr: Box::new(IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Module { module: sym("value"), func: sym("as_array"), def_id: None },
                            args: vec![IrExpr { kind: IrExprKind::Var { id: var_payload }, ty: value_ty.clone(), span: None, def_id: None }],
                            type_args: vec![],
                        },
                        ty: Ty::result(arr_ty.clone(), Ty::String), span: None, def_id: None,
                    })},
                    ty: arr_ty.clone(), span: None, def_id: None,
                };
                let mut stmts = vec![IrStmt {
                    kind: IrStmtKind::Bind { var: arr_var, mutability: Mutability::Let, ty: arr_ty.clone(), value: as_array },
                    span: None,
                }];
                let mut elem_vars = vec![];
                for (i, field_ty) in fields.iter().enumerate() {
                    let elem = IrExpr {
                        kind: IrExprKind::IndexAccess {
                            object: Box::new(IrExpr { kind: IrExprKind::Var { id: arr_var }, ty: arr_ty.clone(), span: None, def_id: None }),
                            index: Box::new(IrExpr { kind: IrExprKind::LitInt { value: i as i64 }, ty: Ty::Int, span: None, def_id: None }),
                        },
                        ty: value_ty.clone(), span: None, def_id: None,
                    };
                    let decoded = decode_field_value(elem, field_ty, &value_ty);
                    let ev = vt.alloc(sym(&format!("_e{}", i)), field_ty.clone(), Mutability::Let, None);
                    stmts.push(IrStmt {
                        kind: IrStmtKind::Bind { var: ev, mutability: Mutability::Let, ty: field_ty.clone(), value: decoded },
                        span: None,
                    });
                    elem_vars.push(ev);
                }
                let ctor = IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: case.name },
                        // Give each ctor arg its REAL field type (NOT `Ty::Unknown`): the trust-spine's
                        // variant-ctor materializer (`try_lower_variant_ctor`) reads `arg.ty` to place each
                        // field (heap handle moved in vs scalar stored), so an `Unknown`-typed arg walls the
                        // whole `ok(Ctor(..))`. v0's codegen re-infers, so it was insensitive to this.
                        args: elem_vars.iter().zip(fields.iter()).map(|(v, field_ty)| IrExpr { kind: IrExprKind::Var { id: *v }, ty: field_ty.clone(), span: None, def_id: None }).collect(),
                        type_args: vec![],
                    },
                    ty: type_ty.clone(), span: None, def_id: None,
                };
                IrExpr {
                    kind: IrExprKind::Block {
                        stmts,
                        expr: Some(Box::new(IrExpr { kind: IrExprKind::ResultOk { expr: Box::new(ctor) }, ty: result_ty.clone(), span: None, def_id: None })),
                    },
                    ty: result_ty.clone(), span: None, def_id: None,
                }
            }
            IrVariantKind::Record { fields } => {
                // Payload is `{ "field": value, … }` (see variant encode). Decode each
                // field by key/type, then `Ok(Ctor { field: …, … })`.
                let mut stmts = vec![];
                let mut field_pairs = vec![];
                for f in fields {
                    let key = f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string());
                    let decoded = if f.ty.is_option() {
                        let inner_ty = f.ty.inner().cloned().unwrap_or_else(|| f.ty.clone());
                        IrExpr {
                            kind: IrExprKind::Try { expr: Box::new(IrExpr {
                                kind: IrExprKind::Call {
                                    target: CallTarget::Named { name: sym(&option_codec_fn("decode", &inner_ty)) },
                                    args: vec![
                                        IrExpr { kind: IrExprKind::Var { id: var_payload }, ty: value_ty.clone(), span: None, def_id: None },
                                        IrExpr { kind: IrExprKind::LitStr { value: key.clone() }, ty: Ty::String, span: None, def_id: None },
                                    ],
                                    type_args: vec![],
                                },
                                ty: Ty::result(f.ty.clone(), Ty::String), span: None, def_id: None,
                            })},
                            ty: f.ty.clone(), span: None, def_id: None,
                        }
                    } else {
                        let get_field = IrExpr {
                            kind: IrExprKind::Try { expr: Box::new(IrExpr {
                                kind: IrExprKind::Call {
                                    target: CallTarget::Module { module: sym("value"), func: sym("field"), def_id: None },
                                    args: vec![
                                        IrExpr { kind: IrExprKind::Var { id: var_payload }, ty: value_ty.clone(), span: None, def_id: None },
                                        IrExpr { kind: IrExprKind::LitStr { value: key.clone() }, ty: Ty::String, span: None, def_id: None },
                                    ],
                                    type_args: vec![],
                                },
                                ty: Ty::result(value_ty.clone(), Ty::String), span: None, def_id: None,
                            })},
                            ty: value_ty.clone(), span: None, def_id: None,
                        };
                        decode_field_value(get_field, &f.ty, &value_ty)
                    };
                    let fv = vt.alloc(sym(&format!("_{}", f.name)), f.ty.clone(), Mutability::Let, None);
                    stmts.push(IrStmt {
                        kind: IrStmtKind::Bind { var: fv, mutability: Mutability::Let, ty: f.ty.clone(), value: decoded },
                        span: None,
                    });
                    field_pairs.push((f.name, IrExpr { kind: IrExprKind::Var { id: fv }, ty: f.ty.clone(), span: None, def_id: None }));
                }
                let record = IrExpr {
                    kind: IrExprKind::Record { name: Some(case.name), fields: field_pairs },
                    ty: type_ty.clone(), span: None, def_id: None,
                };
                IrExpr {
                    kind: IrExprKind::Block {
                        stmts,
                        expr: Some(Box::new(IrExpr { kind: IrExprKind::ResultOk { expr: Box::new(record) }, ty: result_ty.clone(), span: None, def_id: None })),
                    },
                    ty: result_ty.clone(), span: None, def_id: None,
                }
            }
        };

        else_expr = IrExpr {
            kind: IrExprKind::If { cond: Box::new(tag_check), then: Box::new(construct), else_: Box::new(else_expr) },
            ty: result_ty.clone(), span: None, def_id: None,
        };
    }

    // Inner: `match value.field(_v, _tag) { ok(_payload) => <if-chain>, err(_e2) => err(_e2) }`.
    let field_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("value"), func: sym("field"), def_id: None },
            args: vec![
                IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None, def_id: None },
                IrExpr { kind: IrExprKind::Var { id: var_tag }, ty: Ty::String, span: None, def_id: None },
            ],
            type_args: vec![],
        },
        ty: Ty::result(value_ty.clone(), Ty::String), span: None, def_id: None,
    };
    let err_e2 = IrExpr {
        kind: IrExprKind::ResultErr { expr: Box::new(IrExpr { kind: IrExprKind::Var { id: var_e2 }, ty: Ty::String, span: None, def_id: None }) },
        ty: result_ty.clone(), span: None, def_id: None,
    };
    let inner_match = IrExpr {
        kind: IrExprKind::Match {
            subject: Box::new(field_call),
            arms: vec![
                IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: var_payload, ty: value_ty.clone() }) }, guard: None, body: else_expr },
                IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: var_e2, ty: Ty::String }) }, guard: None, body: err_e2 },
            ],
        },
        ty: result_ty.clone(), span: None, def_id: None,
    };
    // `let _tag = value.keys(_v) |> list.get(0) ?? ""` — the variant tag (first Object key) as a
    // PLAIN String (NOT a Result), built from RECOGNIZED self-host module calls (value.keys /
    // list.get / `??`) so the trust-spine materializes it. A simple `let` bind (not the Ok-payload of
    // an outer Result-match, which the trust-spine walls when it wraps a heap-String subject whose arm
    // re-borrows the same param). A non-Object `_v` → `value.keys` returns `[]` → `""` → `value.field`
    // then yields the same "expected Object" the strict tuple path did.
    let keys_ty = Ty::list(Ty::String);
    let keys_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("json"), func: sym("keys"), def_id: None },
            args: vec![IrExpr { kind: IrExprKind::Var { id: var_v }, ty: value_ty.clone(), span: None, def_id: None }],
            type_args: vec![],
        },
        ty: keys_ty.clone(), span: None, def_id: None,
    };
    let get_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("list"), func: sym("get"), def_id: None },
            args: vec![keys_call, IrExpr { kind: IrExprKind::LitInt { value: 0 }, ty: Ty::Int, span: None, def_id: None }],
            type_args: vec![],
        },
        ty: Ty::option(Ty::String), span: None, def_id: None,
    };
    let tag_call = IrExpr {
        kind: IrExprKind::UnwrapOr {
            expr: Box::new(get_call),
            fallback: Box::new(IrExpr { kind: IrExprKind::LitStr { value: String::new() }, ty: Ty::String, span: None, def_id: None }),
        },
        ty: Ty::String, span: None, def_id: None,
    };
    let bind_tag = IrStmt {
        kind: IrStmtKind::Bind { var: var_tag, mutability: Mutability::Let, ty: Ty::String, value: tag_call },
        span: None,
    };
    let body = IrExpr {
        kind: IrExprKind::Block { stmts: vec![bind_tag], expr: Some(Box::new(inner_match)) },
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

// ── Container-codec helpers (#790 piece 1) ──────────────────────

/// The four container helpers every Codec type carries in its DECLARING region —
/// `__encode_list_T` / `__decode_list_T` / `__encode_option_T` / `__decode_option_T`
/// (plus two recursion workers named `T.__list_enc_go` / `T.__list_dec_go`, DOTTED so
/// they ride the same module-method rails as `T.encode` and stay clear of every
/// `__`-prefix rewrite in v0's `BuiltinLoweringPass`).
///
/// On v0 the pass still reroutes every helper CALL to the generic runtime + FnRef,
/// so these bodies go unused there (wasm DCE removes them; the Rust emit sits under
/// the crate-level `allow(dead_code)`) — ZERO shipped-leg behavior change. On the v1
/// leg they are the REAL linkable bodies the pipeline name bridge resolves, replacing
/// the unlinked-`__encode_list_<m>.T` wall (#790).
///
/// Byte parity with the v0 generic runtime:
///   - encode_list = `Value::Array(items.map(T.encode))` (order-preserving append);
///   - decode_list: a non-Array is `value.as_array`'s Err — the SAME "expected Array"
///     bytes `almide_rt_value_decode_list` emits — and an element failure
///     short-circuits on the FIRST error (the worker's `?` returns it immediately);
///   - encode_option = Some → T.encode, None → Value::Null;
///   - decode_option(v, key) = missing field OR Null → ok(none), present →
///     `T.decode(val)` mapped into some — exactly `almide_rt_value_decode_option`.
pub(super) fn derive_container_helpers(
    vt: &mut VarTable,
    type_name: &str,
    type_ty: &Ty,
) -> Vec<IrFunction> {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let t = type_ty.clone();
    let list_t = Ty::list(t.clone());
    let list_v = Ty::list(value_ty.clone());
    let opt_t = Ty::Applied(TypeConstructorId::Option, vec![t.clone()]);
    let res_list_t = Ty::result(list_t.clone(), Ty::String);
    let res_opt_t = Ty::result(opt_t.clone(), Ty::String);

    fn e(kind: IrExprKind, ty: Ty) -> IrExpr {
        IrExpr { kind, ty, span: None, def_id: None }
    }
    fn evar(id: VarId, ty: &Ty) -> IrExpr {
        e(IrExprKind::Var { id }, ty.clone())
    }
    fn call_named(name: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
        e(IrExprKind::Call { target: CallTarget::Named { name: sym(name) }, args, type_args: vec![] }, ty)
    }
    fn call_mod(m: &str, f: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
        e(
            IrExprKind::Call {
                target: CallTarget::Module { module: sym(m), func: sym(f), def_id: None },
                args,
                type_args: vec![],
            },
            ty,
        )
    }
    fn lit_int(n: i64) -> IrExpr {
        e(IrExprKind::LitInt { value: n }, Ty::Int)
    }
    fn empty_list(ty: &Ty) -> IrExpr {
        e(IrExprKind::List { elements: vec![] }, ty.clone())
    }
    fn mk_fn(name: &str, params: Vec<(VarId, &str, Ty)>, ret_ty: Ty, body: IrExpr) -> IrFunction {
        IrFunction {
            name: sym(name),
            params: params
                .into_iter()
                .map(|(var, n, ty)| IrParam {
                    var,
                    ty,
                    name: sym(n),
                    borrow: ParamBorrow::Own,
                    is_mut: false,
                    open_record: None,
                    default: None,
                    attrs: vec![],
                })
                .collect(),
            ret_ty,
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
            module_origin: None,
        }
    }

    let encode_name = format!("{}.encode", type_name);
    let decode_name = format!("{}.decode", type_name);
    let enc_go_name = format!("{}.__list_enc_go", type_name);
    let dec_go_name = format!("{}.__list_dec_go", type_name);

    // __encode_list_T(xs) = T.__list_enc_go(xs, 0, [])
    let xs_a = vt.alloc(sym("_xs"), list_t.clone(), Mutability::Let, None);
    let f_enc_list = mk_fn(
        &format!("__encode_list_{}", type_name),
        vec![(xs_a, "_xs", list_t.clone())],
        value_ty.clone(),
        call_named(
            &enc_go_name,
            vec![evar(xs_a, &list_t), lit_int(0), empty_list(&list_v)],
            value_ty.clone(),
        ),
    );

    // T.__list_enc_go(xs, i, acc) =
    //   if i < list.len(xs) then T.__list_enc_go(xs, i+1, acc + [T.encode(xs[i])])
    //   else value.array(acc)
    let xs_b = vt.alloc(sym("_xs"), list_t.clone(), Mutability::Let, None);
    let i_b = vt.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
    let acc_b = vt.alloc(sym("_acc"), list_v.clone(), Mutability::Let, None);
    let elem_b = e(
        IrExprKind::IndexAccess {
            object: Box::new(evar(xs_b, &list_t)),
            index: Box::new(evar(i_b, &Ty::Int)),
        },
        t.clone(),
    );
    let enc_elem = call_named(&encode_name, vec![elem_b], value_ty.clone());
    let appended = e(
        IrExprKind::BinOp {
            op: BinOp::ConcatList,
            left: Box::new(evar(acc_b, &list_v)),
            right: Box::new(e(IrExprKind::List { elements: vec![enc_elem] }, list_v.clone())),
        },
        list_v.clone(),
    );
    let cond_b = e(
        IrExprKind::BinOp {
            op: BinOp::Lt,
            left: Box::new(evar(i_b, &Ty::Int)),
            right: Box::new(call_mod("list", "len", vec![evar(xs_b, &list_t)], Ty::Int)),
        },
        Ty::Bool,
    );
    let next_i_b = e(
        IrExprKind::BinOp {
            op: BinOp::AddInt,
            left: Box::new(evar(i_b, &Ty::Int)),
            right: Box::new(lit_int(1)),
        },
        Ty::Int,
    );
    let f_enc_go = mk_fn(
        &enc_go_name,
        vec![(xs_b, "_xs", list_t.clone()), (i_b, "_i", Ty::Int), (acc_b, "_acc", list_v.clone())],
        value_ty.clone(),
        e(
            IrExprKind::If {
                cond: Box::new(cond_b),
                then: Box::new(call_named(
                    &enc_go_name,
                    vec![evar(xs_b, &list_t), next_i_b, appended],
                    value_ty.clone(),
                )),
                else_: Box::new(call_mod("value", "array", vec![evar(acc_b, &list_v)], value_ty.clone())),
            },
            value_ty.clone(),
        ),
    );

    // __decode_list_T(v) = { let arr = value.as_array(v)?; T.__list_dec_go(arr, 0, []) }
    let v_c = vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
    let arr_c = vt.alloc(sym("_arr"), list_v.clone(), Mutability::Let, None);
    let bind_arr = IrStmt {
        kind: IrStmtKind::Bind {
            var: arr_c,
            mutability: Mutability::Let,
            ty: list_v.clone(),
            value: e(
                IrExprKind::Try {
                    expr: Box::new(call_mod(
                        "value",
                        "as_array",
                        vec![evar(v_c, &value_ty)],
                        Ty::result(list_v.clone(), Ty::String),
                    )),
                },
                list_v.clone(),
            ),
        },
        span: None,
    };
    let f_dec_list = mk_fn(
        &format!("__decode_list_{}", type_name),
        vec![(v_c, "_v", value_ty.clone())],
        res_list_t.clone(),
        e(
            IrExprKind::Block {
                stmts: vec![bind_arr],
                expr: Some(Box::new(call_named(
                    &dec_go_name,
                    vec![evar(arr_c, &list_v), lit_int(0), empty_list(&list_t)],
                    res_list_t.clone(),
                ))),
            },
            res_list_t.clone(),
        ),
    );

    // T.__list_dec_go(arr, i, acc) =
    //   if i < list.len(arr) then { let x = T.decode(arr[i])?; T.__list_dec_go(arr, i+1, acc + [x]) }
    //   else ok(acc)
    let arr_d = vt.alloc(sym("_arr"), list_v.clone(), Mutability::Let, None);
    let i_d = vt.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
    let acc_d = vt.alloc(sym("_acc"), list_t.clone(), Mutability::Let, None);
    let x_d = vt.alloc(sym("_x"), t.clone(), Mutability::Let, None);
    let elem_d = e(
        IrExprKind::IndexAccess {
            object: Box::new(evar(arr_d, &list_v)),
            index: Box::new(evar(i_d, &Ty::Int)),
        },
        value_ty.clone(),
    );
    let bind_x = IrStmt {
        kind: IrStmtKind::Bind {
            var: x_d,
            mutability: Mutability::Let,
            ty: t.clone(),
            value: e(
                IrExprKind::Try {
                    expr: Box::new(call_named(
                        &decode_name,
                        vec![elem_d],
                        Ty::result(t.clone(), Ty::String),
                    )),
                },
                t.clone(),
            ),
        },
        span: None,
    };
    let appended_d = e(
        IrExprKind::BinOp {
            op: BinOp::ConcatList,
            left: Box::new(evar(acc_d, &list_t)),
            right: Box::new(e(IrExprKind::List { elements: vec![evar(x_d, &t)] }, list_t.clone())),
        },
        list_t.clone(),
    );
    let next_i_d = e(
        IrExprKind::BinOp {
            op: BinOp::AddInt,
            left: Box::new(evar(i_d, &Ty::Int)),
            right: Box::new(lit_int(1)),
        },
        Ty::Int,
    );
    let cond_d = e(
        IrExprKind::BinOp {
            op: BinOp::Lt,
            left: Box::new(evar(i_d, &Ty::Int)),
            right: Box::new(call_mod("list", "len", vec![evar(arr_d, &list_v)], Ty::Int)),
        },
        Ty::Bool,
    );
    let f_dec_go = mk_fn(
        &dec_go_name,
        vec![(arr_d, "_arr", list_v.clone()), (i_d, "_i", Ty::Int), (acc_d, "_acc", list_t.clone())],
        res_list_t.clone(),
        e(
            IrExprKind::If {
                cond: Box::new(cond_d),
                then: Box::new(e(
                    IrExprKind::Block {
                        stmts: vec![bind_x],
                        expr: Some(Box::new(call_named(
                            &dec_go_name,
                            vec![evar(arr_d, &list_v), next_i_d, appended_d],
                            res_list_t.clone(),
                        ))),
                    },
                    res_list_t.clone(),
                )),
                else_: Box::new(e(
                    IrExprKind::ResultOk { expr: Box::new(evar(acc_d, &list_t)) },
                    res_list_t.clone(),
                )),
            },
            res_list_t.clone(),
        ),
    );

    // __encode_option_T(o) = match o { some(x) => T.encode(x), none => value.null() }
    let o_e = vt.alloc(sym("_o"), opt_t.clone(), Mutability::Let, None);
    let x_e = vt.alloc(sym("_x"), t.clone(), Mutability::Let, None);
    let f_enc_opt = mk_fn(
        &format!("__encode_option_{}", type_name),
        vec![(o_e, "_o", opt_t.clone())],
        value_ty.clone(),
        e(
            IrExprKind::Match {
                subject: Box::new(evar(o_e, &opt_t)),
                arms: vec![
                    IrMatchArm {
                        pattern: IrPattern::Some {
                            inner: Box::new(IrPattern::Bind { var: x_e, ty: t.clone() }),
                        },
                        guard: None,
                        body: call_named(&encode_name, vec![evar(x_e, &t)], value_ty.clone()),
                    },
                    IrMatchArm {
                        pattern: IrPattern::None,
                        guard: None,
                        body: call_mod("value", "null", vec![], value_ty.clone()),
                    },
                ],
            },
            value_ty.clone(),
        ),
    );

    // __decode_option_T(v, key) = match value.field(v, key) {
    //   err(_) => ok(none),
    //   ok(fv) => if fv == value.null() then ok(none)
    //             else { let x = T.decode(fv)?; ok(some(x)) }
    // }
    let v_f = vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
    let key_f = vt.alloc(sym("_key"), Ty::String, Mutability::Let, None);
    let fv_f = vt.alloc(sym("_fv"), value_ty.clone(), Mutability::Let, None);
    let x_f = vt.alloc(sym("_x"), t.clone(), Mutability::Let, None);
    let ok_none = || {
        e(
            IrExprKind::ResultOk {
                expr: Box::new(e(IrExprKind::OptionNone, opt_t.clone())),
            },
            res_opt_t.clone(),
        )
    };
    let bind_x_f = IrStmt {
        kind: IrStmtKind::Bind {
            var: x_f,
            mutability: Mutability::Let,
            ty: t.clone(),
            value: e(
                IrExprKind::Try {
                    expr: Box::new(call_named(
                        &decode_name,
                        vec![evar(fv_f, &value_ty)],
                        Ty::result(t.clone(), Ty::String),
                    )),
                },
                t.clone(),
            ),
        },
        span: None,
    };
    let is_null = e(
        IrExprKind::BinOp {
            op: BinOp::Eq,
            left: Box::new(evar(fv_f, &value_ty)),
            right: Box::new(call_mod("value", "null", vec![], value_ty.clone())),
        },
        Ty::Bool,
    );
    let f_dec_opt = mk_fn(
        &format!("__decode_option_{}", type_name),
        vec![(v_f, "_v", value_ty.clone()), (key_f, "_key", Ty::String)],
        res_opt_t.clone(),
        e(
            IrExprKind::Match {
                subject: Box::new(call_mod(
                    "value",
                    "field",
                    vec![evar(v_f, &value_ty), evar(key_f, &Ty::String)],
                    Ty::result(value_ty.clone(), Ty::String),
                )),
                arms: vec![
                    IrMatchArm {
                        pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                        guard: None,
                        body: ok_none(),
                    },
                    IrMatchArm {
                        pattern: IrPattern::Ok {
                            inner: Box::new(IrPattern::Bind { var: fv_f, ty: value_ty.clone() }),
                        },
                        guard: None,
                        body: e(
                            IrExprKind::If {
                                cond: Box::new(is_null),
                                then: Box::new(ok_none()),
                                else_: Box::new(e(
                                    IrExprKind::Block {
                                        stmts: vec![bind_x_f],
                                        expr: Some(Box::new(e(
                                            IrExprKind::ResultOk {
                                                expr: Box::new(e(
                                                    IrExprKind::OptionSome {
                                                        expr: Box::new(evar(x_f, &t)),
                                                    },
                                                    opt_t.clone(),
                                                )),
                                            },
                                            res_opt_t.clone(),
                                        ))),
                                    },
                                    res_opt_t.clone(),
                                )),
                            },
                            res_opt_t.clone(),
                        ),
                    },
                ],
            },
            res_opt_t.clone(),
        ),
    );

    vec![f_enc_list, f_enc_go, f_dec_list, f_dec_go, f_enc_opt, f_dec_opt]
}
