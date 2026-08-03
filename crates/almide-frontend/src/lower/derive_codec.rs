// ── Auto-derive Codec ───────────────────────────────────────────

use almide_ir::*;
use crate::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;

/// Auto-derive Codec encode: `fn T.encode(t: T) -> Value`
/// Generates: `value.object([("field1", value.str(t.field1)), ...] + <conditional Option chunks>)`
pub(super) fn auto_derive_encode(vt: &mut VarTable, type_name: &str, type_ty: &Ty, fields: &[IrFieldDecl]) -> IrFunction {
    let var = vt.alloc(sym("_v"), type_ty.clone(), Mutability::Let, None);
    let value_ty = Ty::Named(sym("Value"), vec![]);

    let entries: Vec<(String, Ty, IrExpr)> = fields.iter().map(|f| {
        let field_access = IrExpr {
            kind: IrExprKind::Member {
                object: Box::new(IrExpr { kind: IrExprKind::Var { id: var }, ty: type_ty.clone(), span: None, def_id: None }),
                field: f.name,
            },
            ty: f.ty.clone(), span: None, def_id: None,
        };
        (f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string()), f.ty.clone(), field_access)
    }).collect();
    let pairs_list = build_object_arg(vt, &entries, &value_ty);

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
        is_effect: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}

/// Build the `value.object(...)` argument for a record-shaped field list.
/// Non-Option fields form static pair-list chunks in declaration order; each
/// Option field contributes `match f { some(_x) => [(key, enc(_x))], none => [] }`
/// so a `none` field OMITS its key from the emitted object (proto3-style unset)
/// instead of emitting an explicit null. Chunks are joined with list concat —
/// a shape both render legs lower (verified native == wasm byte output).
pub(super) fn build_object_arg(vt: &mut VarTable, entries: &[(String, Ty, IrExpr)], value_ty: &Ty) -> IrExpr {
    let pair_ty = Ty::Tuple(vec![Ty::String, value_ty.clone()]);
    let chunk_ty = Ty::list(pair_ty.clone());
    let mk_pair = |key: &str, val: IrExpr| IrExpr {
        kind: IrExprKind::Tuple { elements: vec![
            IrExpr { kind: IrExprKind::LitStr { value: key.to_string() }, ty: Ty::String, span: None, def_id: None },
            val,
        ]},
        ty: pair_ty.clone(), span: None, def_id: None,
    };

    let mut chunks: Vec<IrExpr> = vec![];
    let mut static_pairs: Vec<IrExpr> = vec![];
    for (key, field_ty, access) in entries {
        if field_ty.is_option() {
            if !static_pairs.is_empty() {
                chunks.push(IrExpr { kind: IrExprKind::List { elements: std::mem::take(&mut static_pairs) }, ty: chunk_ty.clone(), span: None, def_id: None });
            }
            let inner_ty = field_ty.inner().cloned().unwrap_or_else(|| field_ty.clone());
            let x = vt.alloc(sym("_x"), inner_ty.clone(), Mutability::Let, None);
            let x_expr = IrExpr { kind: IrExprKind::Var { id: x }, ty: inner_ty.clone(), span: None, def_id: None };
            let some_arm = IrMatchArm {
                pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: x, ty: inner_ty.clone() }) },
                guard: None,
                body: IrExpr {
                    kind: IrExprKind::List { elements: vec![mk_pair(key, encode_field_value(&x_expr, &inner_ty, value_ty))] },
                    ty: chunk_ty.clone(), span: None, def_id: None,
                },
            };
            let none_arm = IrMatchArm {
                pattern: IrPattern::None,
                guard: None,
                body: IrExpr { kind: IrExprKind::List { elements: vec![] }, ty: chunk_ty.clone(), span: None, def_id: None },
            };
            chunks.push(IrExpr {
                kind: IrExprKind::Match { subject: Box::new(access.clone()), arms: vec![some_arm, none_arm] },
                ty: chunk_ty.clone(), span: None, def_id: None,
            });
        } else {
            static_pairs.push(mk_pair(key, encode_field_value(access, field_ty, value_ty)));
        }
    }
    if !static_pairs.is_empty() || chunks.is_empty() {
        chunks.push(IrExpr { kind: IrExprKind::List { elements: static_pairs }, ty: chunk_ty.clone(), span: None, def_id: None });
    }

    chunks
        .into_iter()
        .reduce(|acc, chunk| IrExpr {
            kind: IrExprKind::BinOp { op: BinOp::ConcatList, left: Box::new(acc), right: Box::new(chunk) },
            ty: chunk_ty.clone(), span: None, def_id: None,
        })
        .expect("chunks is never empty")
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

fn is_value_ty(ty: &Ty) -> bool {
    matches!(ty, Ty::Named(name, _) if name.as_str() == "Value")
}

fn encode_field_value(field_expr: &IrExpr, field_ty: &Ty, value_ty: &Ty) -> IrExpr {
    // `Value` passes through verbatim — the declared escape hatch for explicit
    // null / arbitrary subdocuments (#1061). Identity in both directions.
    if is_value_ty(field_ty) {
        return field_expr.clone();
    }
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
            if is_value_ty(inner) {
                // List[Value]: already a list of wire values — wrap verbatim.
                return IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: sym("value"), func: sym("array"), def_id: None },
                        args: vec![field_expr.clone()],
                        type_args: vec![],
                    },
                    ty: value_ty.clone(), span: None, def_id: None,
                };
            }
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

fn list_elem_suffix(elem: &Ty) -> String {
    if let Ty::Named(name, _) = elem { name.to_string() } else { decode_func_suffix(elem).to_string() }
}

/// The Result-typed list-decode CALL for a `List[elem]` type — no Try wrapper.
fn decode_list_call(get_expr: IrExpr, list_ty: &Ty, elem: &Ty) -> IrExpr {
    if is_value_ty(elem) {
        return IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: sym("value"), func: sym("as_array"), def_id: None },
                args: vec![get_expr],
                type_args: vec![],
            },
            ty: Ty::result(list_ty.clone(), Ty::String), span: None, def_id: None,
        };
    }
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Named { name: sym(&format!("__decode_list_{}", list_elem_suffix(elem))) },
            args: vec![get_expr],
            type_args: vec![],
        },
        ty: Ty::result(list_ty.clone(), Ty::String), span: None, def_id: None,
    }
}

fn option_list_worker_name(type_name: &str, elem: &Ty) -> String {
    format!("{}.__opt_list_dec_{}", type_name, list_elem_suffix(elem))
}

/// Decode for an `Option[inner]` FIELD whose inner has no static
/// `__decode_option_*` helper; returns None for inners the helper family
/// already covers. `Value` inner is the 3-state escape hatch: missing → none,
/// present INCLUDING explicit null → some(v) — Value never interprets the
/// wire, so `Option[Value]` distinguishes absent from null (unlike every other
/// Option field, where the two collapse to none). `List[elem]` inner routes to
/// the per-type worker `T.__opt_list_dec_<elem>` (see
/// [`derive_option_list_workers`]) through the same `Try(Call)` bind shape as
/// the static helpers.
pub(super) fn decode_option_field_inline(vt: &mut VarTable, type_name: &str, payload: IrExpr, key: &str, inner_ty: &Ty, value_ty: &Ty) -> Option<IrExpr> {
    if let Ty::Applied(TypeConstructorId::List, args) = inner_ty {
        if args.len() == 1 {
            let opt_ty = Ty::option(inner_ty.clone());
            return Some(IrExpr {
                kind: IrExprKind::Try { expr: Box::new(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Named { name: sym(&option_list_worker_name(type_name, &args[0])) },
                        args: vec![payload, IrExpr { kind: IrExprKind::LitStr { value: key.to_string() }, ty: Ty::String, span: None, def_id: None }],
                        type_args: vec![],
                    },
                    ty: Ty::result(opt_ty.clone(), Ty::String), span: None, def_id: None,
                })},
                ty: opt_ty, span: None, def_id: None,
            });
        }
    }
    if !is_value_ty(inner_ty) {
        return None;
    }
    let opt_ty = Ty::option(inner_ty.clone());
    let fv = vt.alloc(sym("_fv"), value_ty.clone(), Mutability::Let, None);
    let fv_expr = IrExpr { kind: IrExprKind::Var { id: fv }, ty: value_ty.clone(), span: None, def_id: None };
    let field_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("value"), func: sym("field"), def_id: None },
            args: vec![payload, IrExpr { kind: IrExprKind::LitStr { value: key.to_string() }, ty: Ty::String, span: None, def_id: None }],
            type_args: vec![],
        },
        ty: Ty::result(value_ty.clone(), Ty::String), span: None, def_id: None,
    };
    Some(IrExpr {
        kind: IrExprKind::Match {
            subject: Box::new(field_call),
            arms: vec![
                IrMatchArm {
                    pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: fv, ty: value_ty.clone() }) },
                    guard: None,
                    body: IrExpr { kind: IrExprKind::OptionSome { expr: Box::new(fv_expr) }, ty: opt_ty.clone(), span: None, def_id: None },
                },
                IrMatchArm {
                    pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                    guard: None,
                    body: IrExpr { kind: IrExprKind::OptionNone, ty: opt_ty.clone(), span: None, def_id: None },
                },
            ],
        },
        ty: opt_ty, span: None, def_id: None,
    })
}

/// Per-type decode workers for `Option[List[elem]]` fields:
/// `T.__opt_list_dec_<elem>(v, key) -> Result[Option[List[elem]], String]`
/// (dotted, so it rides the module-method rails like `T.__list_dec_go`).
/// The body is a Result-typed match chain with NO Try inside: a Try nested in
/// liftable branches breaks under branch-lift synthesis (it hoists into an
/// Option-typed synthetic fn where `?` has no Result to propagate to), so the
/// worker propagates by matching, stdlib-style, and the field site Try's the
/// call like every other option helper.
pub(super) fn derive_option_list_workers(vt: &mut VarTable, type_name: &str, field_tys: &[Ty]) -> Vec<IrFunction> {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for fty in field_tys {
        if !fty.is_option() { continue; }
        let Some(inner) = fty.inner() else { continue };
        let Ty::Applied(TypeConstructorId::List, args) = inner else { continue };
        if args.len() != 1 { continue; }
        let elem = args[0].clone();
        if !seen.insert(list_elem_suffix(&elem)) { continue; }

        let list_ty = inner.clone();
        let opt_ty = Ty::option(list_ty.clone());
        let res_ty = Ty::result(opt_ty.clone(), Ty::String);
        let v = vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
        let key = vt.alloc(sym("_key"), Ty::String, Mutability::Let, None);
        let fv = vt.alloc(sym("_fv"), value_ty.clone(), Mutability::Let, None);
        let xs = vt.alloc(sym("_xs"), list_ty.clone(), Mutability::Let, None);
        let e = vt.alloc(sym("_e"), Ty::String, Mutability::Let, None);
        let expr = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span: None, def_id: None };
        let evar = |id, ty: &Ty| expr(IrExprKind::Var { id }, ty.clone());
        let ok_of = |e: IrExpr| expr(IrExprKind::ResultOk { expr: Box::new(e) }, res_ty.clone());
        let ok_none = || expr(IrExprKind::ResultOk { expr: Box::new(expr(IrExprKind::OptionNone, opt_ty.clone())) }, res_ty.clone());

        let field_call = expr(IrExprKind::Call {
            target: CallTarget::Module { module: sym("value"), func: sym("field"), def_id: None },
            args: vec![evar(v, &value_ty), evar(key, &Ty::String)],
            type_args: vec![],
        }, Ty::result(value_ty.clone(), Ty::String));
        let is_null = expr(IrExprKind::BinOp {
            op: BinOp::Eq,
            left: Box::new(evar(fv, &value_ty)),
            right: Box::new(expr(IrExprKind::Call {
                target: CallTarget::Module { module: sym("value"), func: sym("null"), def_id: None },
                args: vec![], type_args: vec![],
            }, value_ty.clone())),
        }, Ty::Bool);
        let decode_match = expr(IrExprKind::Match {
            subject: Box::new(decode_list_call(evar(fv, &value_ty), &list_ty, &elem)),
            arms: vec![
                IrMatchArm {
                    pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: xs, ty: list_ty.clone() }) },
                    guard: None,
                    body: ok_of(expr(IrExprKind::OptionSome { expr: Box::new(evar(xs, &list_ty)) }, opt_ty.clone())),
                },
                IrMatchArm {
                    pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: e, ty: Ty::String }) },
                    guard: None,
                    body: expr(IrExprKind::ResultErr { expr: Box::new(evar(e, &Ty::String)) }, res_ty.clone()),
                },
            ],
        }, res_ty.clone());
        let present = expr(IrExprKind::If {
            cond: Box::new(is_null),
            then: Box::new(ok_none()),
            else_: Box::new(decode_match),
        }, res_ty.clone());
        let body = expr(IrExprKind::Match {
            subject: Box::new(field_call),
            arms: vec![
                IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: fv, ty: value_ty.clone() }) }, guard: None, body: present },
                IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: ok_none() },
            ],
        }, res_ty.clone());

        out.push(IrFunction {
            name: sym(&option_list_worker_name(type_name, &elem)),
            params: vec![
                IrParam { var: v, ty: value_ty.clone(), name: sym("_v"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] },
                IrParam { var: key, ty: Ty::String, name: sym("_key"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] },
            ],
            ret_ty: res_ty,
            body,
            is_effect: false, is_test: false,
            generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
            doc: None, blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![], module_origin: None,
        });
    }
    out
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
        // `_f_` prefix, NOT `_{name}`: the decode param renders as `_v`, so a
        // field literally named `v` would shadow the document in the emitted
        // Rust and every later field would read the decoded value instead of
        // the doc ("expected Object" at runtime).
        let field_var = vt.alloc(sym(&format!("_f_{}", f.name)), f.ty.clone(), Mutability::Let, None);

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
            if let Some(inline) = decode_option_field_inline(vt, type_name, payload.clone(), &key_name(f), &inner_ty, &value_ty) {
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
        is_effect: false, is_test: false,
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

include!("derive_codec_variant.rs");
