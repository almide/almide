// ── Auto-derive Codec ───────────────────────────────────────────

use almide_ir::*;
use crate::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;

/// Auto-derive Codec encode: `fn T.encode(t: T) -> Value`
/// Generates: `value.object([("field1", value.str(t.field1)), ...] + <conditional Option chunks>)`
pub(super) fn auto_derive_encode(wk: &mut CodecWk, type_ty: &Ty, fields: &[IrFieldDecl]) -> IrFunction {
    let type_name = wk.type_name.to_string();
    let var = wk.vt.alloc(sym("_v"), type_ty.clone(), Mutability::Let, None);
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
    let pairs_list = build_object_arg(wk, &entries, &value_ty);

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
        mutated_params: vec![], module_origin: None, // fresh-fn: derived codec worker, no params carry mut
    }
}

/// Build the `value.object(...)` argument for a record-shaped field list.
/// Non-Option fields form static pair-list chunks in declaration order; each
/// Option field contributes `match f { some(_x) => [(key, enc(_x))], none => [] }`
/// so a `none` field OMITS its key from the emitted object (proto3-style unset)
/// instead of emitting an explicit null. Chunks are joined with list concat —
/// a shape both render legs lower (verified native == wasm byte output).
pub(super) fn build_object_arg(wk: &mut CodecWk, entries: &[(String, Ty, IrExpr)], value_ty: &Ty) -> IrExpr {
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
            let x = wk.vt.alloc(sym("_x"), inner_ty.clone(), Mutability::Let, None);
            let x_expr = IrExpr { kind: IrExprKind::Var { id: x }, ty: inner_ty.clone(), span: None, def_id: None };
            let enc_inner = enc_value_expr(wk, x_expr, &inner_ty, value_ty);
            let some_arm = IrMatchArm {
                pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: x, ty: inner_ty.clone() }) },
                guard: None,
                body: IrExpr {
                    kind: IrExprKind::List { elements: vec![mk_pair(key, enc_inner)] },
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
            let enc = enc_value_expr(wk, access.clone(), field_ty, value_ty);
            static_pairs.push(mk_pair(key, enc));
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

fn is_container_ty(ty: &Ty) -> bool {
    matches!(ty,
        Ty::Applied(TypeConstructorId::Option, a) | Ty::Applied(TypeConstructorId::List, a)
        if a.len() == 1)
}

/// Mangled component naming a type inside a generated worker name:
/// `List[Option[Int]]` → `list_opt_int`. Workers are dotted per declaring
/// type, so a user type sharing a mangle is not reachable.
fn ty_mangle(ty: &Ty) -> String {
    match ty {
        Ty::Int => "int".into(),
        Ty::Float => "float".into(),
        Ty::Bool => "bool".into(),
        Ty::String => "string".into(),
        Ty::Named(n, _) => n.to_string(),
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => format!("opt_{}", ty_mangle(&a[0])),
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => format!("list_{}", ty_mangle(&a[0])),
        _ => "value".into(),
    }
}

/// Worker-generation context for the RECURSIVE codec builders (#1065).
/// Nested container shapes (`List[Option[T]]`, `List[List[T]]`, …) have no
/// static helper family; each container node gets a dotted per-type worker
/// (`T.__enc_list_opt_int`, `T.__dec_opt_elem_int`, …) generated on demand and
/// memoized by name. Leaf and single-level shapes keep their existing
/// static/dotted helper routes untouched.
pub(super) struct CodecWk<'a> {
    pub vt: &'a mut VarTable,
    pub type_name: &'a str,
    pub out: &'a mut Vec<IrFunction>,
    pub seen: &'a mut std::collections::HashSet<String>,
}

fn e_(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}
fn call_named_(name: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    e_(IrExprKind::Call { target: CallTarget::Named { name: sym(name) }, args, type_args: vec![] }, ty)
}
fn call_mod_(m: &str, f: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    e_(IrExprKind::Call { target: CallTarget::Module { module: sym(m), func: sym(f), def_id: None }, args, type_args: vec![] }, ty)
}
fn mk_worker_fn(name: &str, params: Vec<(VarId, &str, Ty)>, ret_ty: Ty, body: IrExpr) -> IrFunction {
    IrFunction {
        name: sym(name),
        params: params.into_iter().map(|(var, n, ty)| IrParam {
            var, ty, name: sym(n), borrow: ParamBorrow::Own, is_mut: false,
            open_record: None, default: None, attrs: vec![],
        }).collect(),
        ret_ty, body,
        is_effect: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0, def_id: None,
        mutated_params: vec![], module_origin: None, // fresh-fn: derived codec worker, no params carry mut
    }
}

/// Encode `expr : ty` to a Value expression, any accepted shape. Existing
/// shapes delegate to [`encode_field_value`] (their helper routes are
/// byte-pinned); container-of-container shapes route through generated
/// workers, and element-position Option encodes inline (no Try on encode).
fn enc_value_expr(wk: &mut CodecWk, expr: IrExpr, ty: &Ty, value_ty: &Ty) -> IrExpr {
    match ty {
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
            let inner = a[0].clone();
            if is_container_ty(&inner) || is_value_ty(&inner) {
                let x = wk.vt.alloc(sym("_x"), inner.clone(), Mutability::Let, None);
                let x_expr = e_(IrExprKind::Var { id: x }, inner.clone());
                let enc_inner = enc_value_expr(wk, x_expr, &inner, value_ty);
                e_(IrExprKind::Match {
                    subject: Box::new(expr),
                    arms: vec![
                        IrMatchArm {
                            pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: x, ty: inner }) },
                            guard: None, body: enc_inner,
                        },
                        IrMatchArm {
                            pattern: IrPattern::None, guard: None,
                            body: call_mod_("value", "null", vec![], value_ty.clone()),
                        },
                    ],
                }, value_ty.clone())
            } else {
                encode_field_value(&expr, ty, value_ty)
            }
        }
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_container_ty(&a[0]) => {
            let name = enc_list_worker(wk, ty, &a[0], value_ty);
            call_named_(&name, vec![expr], value_ty.clone())
        }
        _ => encode_field_value(&expr, ty, value_ty),
    }
}

/// `T.__enc_<mangle>(xs) -> Value` + index-recursive `_go` worker for a list
/// whose ELEMENT is itself a container. Mirrors `T.__list_enc_go` exactly.
fn enc_list_worker(wk: &mut CodecWk, list_ty: &Ty, elem: &Ty, value_ty: &Ty) -> String {
    let name = format!("{}.__enc_{}", wk.type_name, ty_mangle(list_ty));
    if !wk.seen.insert(name.clone()) {
        return name;
    }
    let go_name = format!("{}_go", name);
    let list_v = Ty::list(value_ty.clone());

    let xs_a = wk.vt.alloc(sym("_xs"), list_ty.clone(), Mutability::Let, None);
    let entry = mk_worker_fn(&name, vec![(xs_a, "_xs", list_ty.clone())], value_ty.clone(),
        call_named_(&go_name, vec![
            e_(IrExprKind::Var { id: xs_a }, list_ty.clone()),
            e_(IrExprKind::LitInt { value: 0 }, Ty::Int),
            e_(IrExprKind::List { elements: vec![] }, list_v.clone()),
        ], value_ty.clone()));

    let xs = wk.vt.alloc(sym("_xs"), list_ty.clone(), Mutability::Let, None);
    let i = wk.vt.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
    let acc = wk.vt.alloc(sym("_acc"), list_v.clone(), Mutability::Let, None);
    let elem_expr = e_(IrExprKind::IndexAccess {
        object: Box::new(e_(IrExprKind::Var { id: xs }, list_ty.clone())),
        index: Box::new(e_(IrExprKind::Var { id: i }, Ty::Int)),
    }, elem.clone());
    let enc_elem = enc_value_expr(wk, elem_expr, elem, value_ty);
    let appended = e_(IrExprKind::BinOp {
        op: BinOp::ConcatList,
        left: Box::new(e_(IrExprKind::Var { id: acc }, list_v.clone())),
        right: Box::new(e_(IrExprKind::List { elements: vec![enc_elem] }, list_v.clone())),
    }, list_v.clone());
    let cond = e_(IrExprKind::BinOp {
        op: BinOp::Lt,
        left: Box::new(e_(IrExprKind::Var { id: i }, Ty::Int)),
        right: Box::new(call_mod_("list", "len", vec![e_(IrExprKind::Var { id: xs }, list_ty.clone())], Ty::Int)),
    }, Ty::Bool);
    let next_i = e_(IrExprKind::BinOp {
        op: BinOp::AddInt,
        left: Box::new(e_(IrExprKind::Var { id: i }, Ty::Int)),
        right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)),
    }, Ty::Int);
    let go = mk_worker_fn(&go_name,
        vec![(xs, "_xs", list_ty.clone()), (i, "_i", Ty::Int), (acc, "_acc", list_v.clone())],
        value_ty.clone(),
        e_(IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(call_named_(&go_name, vec![
                e_(IrExprKind::Var { id: xs }, list_ty.clone()), next_i, appended,
            ], value_ty.clone())),
            else_: Box::new(call_mod_("value", "array", vec![e_(IrExprKind::Var { id: acc }, list_v)], value_ty.clone())),
        }, value_ty.clone()));

    wk.out.push(entry);
    wk.out.push(go);
    name
}

/// A Result-typed decode expression for `expr : Value` into `ty` — the
/// recursive counterpart of the static `__decode_list_*` family. No Try
/// anywhere inside (callers Try the whole thing in bind position, or match it
/// inside a worker — a Try nested in liftable branches breaks under
/// branch-lift synthesis).
fn dec_result_expr(wk: &mut CodecWk, expr: IrExpr, ty: &Ty, value_ty: &Ty) -> IrExpr {
    let res_ty = Ty::result(ty.clone(), Ty::String);
    match ty {
        Ty::String => call_mod_("value", "as_string", vec![expr], res_ty),
        Ty::Int => call_mod_("value", "as_int", vec![expr], res_ty),
        Ty::Float => call_mod_("value", "as_float", vec![expr], res_ty),
        Ty::Bool => call_mod_("value", "as_bool", vec![expr], res_ty),
        _ if is_value_ty(ty) => e_(IrExprKind::ResultOk { expr: Box::new(expr) }, res_ty),
        Ty::Named(name, _) => call_named_(&format!("{}.decode", name), vec![expr], res_ty),
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
            let elem = &a[0];
            if is_value_ty(elem) {
                call_mod_("value", "as_array", vec![expr], res_ty)
            } else if is_container_ty(elem) {
                let name = dec_list_worker(wk, ty, elem, value_ty);
                call_named_(&name, vec![expr], res_ty)
            } else {
                call_named_(&format!("__decode_list_{}", list_elem_suffix(elem)), vec![expr], res_ty)
            }
        }
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
            let name = dec_opt_elem_worker(wk, &a[0], value_ty);
            call_named_(&name, vec![expr], res_ty)
        }
        _ => call_mod_("value", "as_string", vec![expr], res_ty),
    }
}

/// Decode a REQUIRED field from its fetched Value: existing shapes delegate
/// to [`decode_field_value`] (byte-pinned helper routes); a list with a
/// #1675: rebuild `res` (a `Result[T, String]` expr) so an Err carries this
/// field's path segment: `match res { ok(x) => ok(x), err(e) =>
/// err(__err_at(e, seg)) }`. All types concrete per field — both legs lower
/// the match with their existing machinery, and `__err_at` is ONE pure-Almide
/// implementation (stdlib/codec_decode.almd) shared by native and wasm.
fn wrap_err_at(wk: &mut CodecWk, res: IrExpr, ok_ty: &Ty, seg: &str) -> IrExpr {
    // A per-type WORKER, not an inline match: `Try { match <call> { … } }`
    // in bind position is outside the structural leg's lowering subset
    // (codec_triple_list walled on it), while a top-level fn matching on
    // its own PARAM is the `T.__list_dec_go` shape both legs already lower.
    let worker = err_at_worker(wk, ok_ty);
    let res_ty = Ty::result(ok_ty.clone(), Ty::String);
    call_named_(&worker, vec![
        res,
        e_(IrExprKind::LitStr { value: seg.to_string() }, Ty::String),
    ], res_ty)
}

/// Mint (once per ok-type) `T.__erratidx_<mangle>(r, i)` — the index twin of
/// [`err_at_worker`], delegating to `__err_at_index`.
fn err_at_index_worker(wk: &mut CodecWk, ok_ty: &Ty) -> String {
    let name = format!("{}.__erratidx_{}", wk.type_name, ty_mangle(ok_ty));
    if !wk.seen.insert(name.clone()) {
        return name;
    }
    let res_ty = Ty::result(ok_ty.clone(), Ty::String);
    let r = wk.vt.alloc(sym("_r"), res_ty.clone(), Mutability::Let, None);
    let iv = wk.vt.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
    let ev = wk.vt.alloc(sym("_we"), Ty::String, Mutability::Let, None);
    // The ok arm passes the PARAM through (`ok(_) => r`) instead of
    // re-materializing `ok(x)`: an Option-payload result ctor sits outside
    // the v1 spine's ctor zoo and would wall the worker; the passthrough
    // shape lowers on every leg.
    let body = e_(IrExprKind::Match {
        subject: Box::new(e_(IrExprKind::Var { id: r }, res_ty.clone())),
        arms: vec![
            IrMatchArm {
                pattern: IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) },
                guard: None,
                body: e_(IrExprKind::Var { id: r }, res_ty.clone()),
            },
            IrMatchArm {
                pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: ev, ty: Ty::String }) },
                guard: None,
                body: e_(IrExprKind::ResultErr { expr: Box::new(call_named_("__err_at_index", vec![
                    e_(IrExprKind::Var { id: ev }, Ty::String),
                    e_(IrExprKind::Var { id: iv }, Ty::Int),
                ], Ty::String)) }, res_ty.clone()),
            },
        ],
    }, res_ty.clone());
    let f = mk_worker_fn(&name, vec![(r, "_r", res_ty.clone()), (iv, "_i", Ty::Int)], res_ty, body);
    wk.out.push(f);
    name
}

/// Mint (once per ok-type) `T.__erratw_<mangle>(r, seg)` — rebuilds an Err
/// with `__err_at(e, seg)`, passes an Ok through.
fn err_at_worker(wk: &mut CodecWk, ok_ty: &Ty) -> String {
    let name = format!("{}.__erratw_{}", wk.type_name, ty_mangle(ok_ty));
    if !wk.seen.insert(name.clone()) {
        return name;
    }
    let res_ty = Ty::result(ok_ty.clone(), Ty::String);
    let r = wk.vt.alloc(sym("_r"), res_ty.clone(), Mutability::Let, None);
    let seg = wk.vt.alloc(sym("_seg"), Ty::String, Mutability::Let, None);
    let ev = wk.vt.alloc(sym("_we"), Ty::String, Mutability::Let, None);
    // ok(_) => r passthrough — see err_at_index_worker for why.
    let body = e_(IrExprKind::Match {
        subject: Box::new(e_(IrExprKind::Var { id: r }, res_ty.clone())),
        arms: vec![
            IrMatchArm {
                pattern: IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) },
                guard: None,
                body: e_(IrExprKind::Var { id: r }, res_ty.clone()),
            },
            IrMatchArm {
                pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: ev, ty: Ty::String }) },
                guard: None,
                body: e_(IrExprKind::ResultErr { expr: Box::new(call_named_("__err_at", vec![
                    e_(IrExprKind::Var { id: ev }, Ty::String),
                    e_(IrExprKind::Var { id: seg }, Ty::String),
                ], Ty::String)) }, res_ty.clone()),
            },
        ],
    }, res_ty.clone());
    let f = mk_worker_fn(&name, vec![(r, "_r", res_ty.clone()), (seg, "_seg", Ty::String)], res_ty, body);
    wk.out.push(f);
    name
}

/// Apply [`wrap_err_at`] to the operand of a top-level `Try` (the shape every
/// field-decode branch produces); anything else passes through unchanged.
fn wrap_try_err_at(wk: &mut CodecWk, expr: IrExpr, ok_ty: &Ty, seg: &str) -> IrExpr {
    if let IrExprKind::Try { expr: inner } = expr.kind {
        let wrapped = wrap_err_at(wk, *inner, ok_ty, seg);
        return e_(IrExprKind::Try { expr: Box::new(wrapped) }, ok_ty.clone());
    }
    IrExpr { kind: expr.kind, ..expr }
}

/// container element Try's the generated worker in bind position.
fn dec_field_expr(wk: &mut CodecWk, get_expr: IrExpr, ty: &Ty, value_ty: &Ty, seg: &str) -> IrExpr {
    if let Ty::Applied(TypeConstructorId::List, a) = ty
        && a.len() == 1
        && is_container_ty(&a[0])
    {
        let call = dec_result_expr(wk, get_expr, ty, value_ty);
        let wrapped = wrap_err_at(wk, call, ty, seg);
        return e_(IrExprKind::Try { expr: Box::new(wrapped) }, ty.clone());
    }
    let plain = decode_field_value(get_expr, ty, value_ty);
    wrap_try_err_at(wk, plain, ty, seg)
}

/// `T.__dec_<mangle>(v) -> Result[List[elem], String]` + `_go` worker for a
/// list whose element is a container. Try only in bind position (the shape
/// `T.__list_dec_go` already lowers on both legs).
fn dec_list_worker(wk: &mut CodecWk, list_ty: &Ty, elem: &Ty, value_ty: &Ty) -> String {
    let name = format!("{}.__dec_{}", wk.type_name, ty_mangle(list_ty));
    if !wk.seen.insert(name.clone()) {
        return name;
    }
    let go_name = format!("{}_go", name);
    let list_v = Ty::list(value_ty.clone());
    let res_ty = Ty::result(list_ty.clone(), Ty::String);

    let v = wk.vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
    let items_e = wk.vt.alloc(sym("_items"), list_v.clone(), Mutability::Let, None);
    let err_e = wk.vt.alloc(sym("_e"), Ty::String, Mutability::Let, None);
    let entry = mk_worker_fn(&name, vec![(v, "_v", value_ty.clone())], res_ty.clone(),
        e_(IrExprKind::Match {
            subject: Box::new(call_mod_("value", "as_array",
                vec![e_(IrExprKind::Var { id: v }, value_ty.clone())],
                Ty::result(list_v.clone(), Ty::String))),
            arms: vec![
                IrMatchArm {
                    pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: items_e, ty: list_v.clone() }) },
                    guard: None,
                    body: call_named_(&go_name, vec![
                        e_(IrExprKind::Var { id: items_e }, list_v.clone()),
                        e_(IrExprKind::LitInt { value: 0 }, Ty::Int),
                        e_(IrExprKind::List { elements: vec![] }, list_ty.clone()),
                    ], res_ty.clone()),
                },
                IrMatchArm {
                    pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: err_e, ty: Ty::String }) },
                    guard: None,
                    body: e_(IrExprKind::ResultErr { expr: Box::new(e_(IrExprKind::Var { id: err_e }, Ty::String)) }, res_ty.clone()),
                },
            ],
        }, res_ty.clone()));

    let items = wk.vt.alloc(sym("_items"), list_v.clone(), Mutability::Let, None);
    let i = wk.vt.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
    let acc = wk.vt.alloc(sym("_acc"), list_ty.clone(), Mutability::Let, None);
    let x = wk.vt.alloc(sym("_x"), elem.clone(), Mutability::Let, None);
    let elem_expr = e_(IrExprKind::IndexAccess {
        object: Box::new(e_(IrExprKind::Var { id: items }, list_v.clone())),
        index: Box::new(e_(IrExprKind::Var { id: i }, Ty::Int)),
    }, value_ty.clone());
    let dec_elem = dec_result_expr(wk, elem_expr, elem, value_ty);
    // #1675: the element's error carries its index — the same call-shaped
    // wrap the field frames use (a worker call, not an inline match).
    let idx_worker = err_at_index_worker(wk, elem);
    let dec_elem = call_named_(&idx_worker, vec![
        dec_elem,
        e_(IrExprKind::Var { id: i }, Ty::Int),
    ], Ty::result(elem.clone(), Ty::String));
    let bind_x = IrStmt {
        kind: IrStmtKind::Bind {
            var: x, mutability: Mutability::Let, ty: elem.clone(),
            value: e_(IrExprKind::Try { expr: Box::new(dec_elem) }, elem.clone()),
        },
        span: None,
    };
    let appended = e_(IrExprKind::BinOp {
        op: BinOp::ConcatList,
        left: Box::new(e_(IrExprKind::Var { id: acc }, list_ty.clone())),
        right: Box::new(e_(IrExprKind::List { elements: vec![e_(IrExprKind::Var { id: x }, elem.clone())] }, list_ty.clone())),
    }, list_ty.clone());
    let cond = e_(IrExprKind::BinOp {
        op: BinOp::Lt,
        left: Box::new(e_(IrExprKind::Var { id: i }, Ty::Int)),
        right: Box::new(call_mod_("list", "len", vec![e_(IrExprKind::Var { id: items }, list_v.clone())], Ty::Int)),
    }, Ty::Bool);
    let next_i = e_(IrExprKind::BinOp {
        op: BinOp::AddInt,
        left: Box::new(e_(IrExprKind::Var { id: i }, Ty::Int)),
        right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)),
    }, Ty::Int);
    let go = mk_worker_fn(&go_name,
        vec![(items, "_items", list_v), (i, "_i", Ty::Int), (acc, "_acc", list_ty.clone())],
        res_ty.clone(),
        e_(IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(e_(IrExprKind::Block {
                stmts: vec![bind_x],
                expr: Some(Box::new(call_named_(&go_name, vec![
                    e_(IrExprKind::Var { id: items }, Ty::list(value_ty.clone())), next_i, appended,
                ], res_ty.clone()))),
            }, res_ty.clone())),
            else_: Box::new(e_(IrExprKind::ResultOk {
                expr: Box::new(e_(IrExprKind::Var { id: acc }, list_ty.clone())),
            }, res_ty.clone())),
        }, res_ty.clone()));

    wk.out.push(entry);
    wk.out.push(go);
    name
}

/// `T.__dec_opt_elem_<mangle>(e) -> Result[Option[inner], String]` — decode
/// for an ELEMENT-position Option: null → none, else inner decode → some.
/// (Element positions have no "absent"; the grammar rejects Option[Option] and
/// non-root Option[Value], so `inner` here is never Option or Value.)
fn dec_opt_elem_worker(wk: &mut CodecWk, inner: &Ty, value_ty: &Ty) -> String {
    let name = format!("{}.__dec_opt_elem_{}", wk.type_name, ty_mangle(inner));
    if !wk.seen.insert(name.clone()) {
        return name;
    }
    let opt_ty = Ty::option(inner.clone());
    let res_ty = Ty::result(opt_ty.clone(), Ty::String);
    let ev = wk.vt.alloc(sym("_e"), value_ty.clone(), Mutability::Let, None);
    let x = wk.vt.alloc(sym("_x"), inner.clone(), Mutability::Let, None);
    let er = wk.vt.alloc(sym("_er"), Ty::String, Mutability::Let, None);
    let is_null = e_(IrExprKind::BinOp {
        op: BinOp::Eq,
        left: Box::new(e_(IrExprKind::Var { id: ev }, value_ty.clone())),
        right: Box::new(call_mod_("value", "null", vec![], value_ty.clone())),
    }, Ty::Bool);
    let dec_inner = dec_result_expr(wk, e_(IrExprKind::Var { id: ev }, value_ty.clone()), inner, value_ty);
    let body = e_(IrExprKind::If {
        cond: Box::new(is_null),
        then: Box::new(e_(IrExprKind::ResultOk {
            expr: Box::new(e_(IrExprKind::OptionNone, opt_ty.clone())),
        }, res_ty.clone())),
        else_: Box::new(e_(IrExprKind::Match {
            subject: Box::new(dec_inner),
            arms: vec![
                IrMatchArm {
                    pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: x, ty: inner.clone() }) },
                    guard: None,
                    body: e_(IrExprKind::ResultOk {
                        expr: Box::new(e_(IrExprKind::OptionSome {
                            expr: Box::new(e_(IrExprKind::Var { id: x }, inner.clone())),
                        }, opt_ty.clone())),
                    }, res_ty.clone()),
                },
                IrMatchArm {
                    pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: er, ty: Ty::String }) },
                    guard: None,
                    body: e_(IrExprKind::ResultErr {
                        expr: Box::new(e_(IrExprKind::Var { id: er }, Ty::String)),
                    }, res_ty.clone()),
                },
            ],
        }, res_ty.clone())),
    }, res_ty.clone());
    wk.out.push(mk_worker_fn(&name, vec![(ev, "_e", value_ty.clone())], res_ty, body));
    name
}

/// `T.__opt_dec_<mangle>(v, key) -> Result[Option[inner], String]` — decode
/// for a FIELD-position Option whose inner is a container: missing/null →
/// none, present → inner decode → some. Result-typed match chain, no Try.
fn opt_field_dec_worker(wk: &mut CodecWk, inner: &Ty, value_ty: &Ty) -> String {
    let name = format!("{}.__opt_dec_{}", wk.type_name, ty_mangle(inner));
    if !wk.seen.insert(name.clone()) {
        return name;
    }
    let opt_ty = Ty::option(inner.clone());
    let res_ty = Ty::result(opt_ty.clone(), Ty::String);
    let v = wk.vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
    let key = wk.vt.alloc(sym("_key"), Ty::String, Mutability::Let, None);
    let fv = wk.vt.alloc(sym("_fv"), value_ty.clone(), Mutability::Let, None);
    let x = wk.vt.alloc(sym("_x"), inner.clone(), Mutability::Let, None);
    let er = wk.vt.alloc(sym("_er"), Ty::String, Mutability::Let, None);
    let ok_none = || e_(IrExprKind::ResultOk {
        expr: Box::new(e_(IrExprKind::OptionNone, opt_ty.clone())),
    }, res_ty.clone());
    let is_null = e_(IrExprKind::BinOp {
        op: BinOp::Eq,
        left: Box::new(e_(IrExprKind::Var { id: fv }, value_ty.clone())),
        right: Box::new(call_mod_("value", "null", vec![], value_ty.clone())),
    }, Ty::Bool);
    let dec_inner = dec_result_expr(wk, e_(IrExprKind::Var { id: fv }, value_ty.clone()), inner, value_ty);
    let present = e_(IrExprKind::If {
        cond: Box::new(is_null),
        then: Box::new(ok_none()),
        else_: Box::new(e_(IrExprKind::Match {
            subject: Box::new(dec_inner),
            arms: vec![
                IrMatchArm {
                    pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: x, ty: inner.clone() }) },
                    guard: None,
                    body: e_(IrExprKind::ResultOk {
                        expr: Box::new(e_(IrExprKind::OptionSome {
                            expr: Box::new(e_(IrExprKind::Var { id: x }, inner.clone())),
                        }, opt_ty.clone())),
                    }, res_ty.clone()),
                },
                IrMatchArm {
                    pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: er, ty: Ty::String }) },
                    guard: None,
                    body: e_(IrExprKind::ResultErr {
                        expr: Box::new(e_(IrExprKind::Var { id: er }, Ty::String)),
                    }, res_ty.clone()),
                },
            ],
        }, res_ty.clone())),
    }, res_ty.clone());
    let body = e_(IrExprKind::Match {
        subject: Box::new(call_mod_("value", "field", vec![
            e_(IrExprKind::Var { id: v }, value_ty.clone()),
            e_(IrExprKind::Var { id: key }, Ty::String),
        ], Ty::result(value_ty.clone(), Ty::String))),
        arms: vec![
            IrMatchArm {
                pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: fv, ty: value_ty.clone() }) },
                guard: None, body: present,
            },
            IrMatchArm {
                pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                guard: None, body: ok_none(),
            },
        ],
    }, res_ty.clone());
    wk.out.push(mk_worker_fn(&name,
        vec![(v, "_v", value_ty.clone()), (key, "_key", Ty::String)], res_ty, body));
    name
}

/// Decode for an `Option[inner]` FIELD whose inner has no static
/// `__decode_option_*` helper; returns None for inners the helper family
/// already covers. `Value` inner is the 3-state escape hatch: missing → none,
/// present INCLUDING explicit null → some(v) — Value never interprets the
/// wire, so `Option[Value]` distinguishes absent from null (unlike every other
/// Option field, where the two collapse to none). A container inner routes to
/// the per-type worker `T.__opt_dec_<mangle>` (see [`opt_field_dec_worker`])
/// through the same `Try(Call)` bind shape as the static helpers.
pub(super) fn decode_option_field_inline(wk: &mut CodecWk, payload: IrExpr, key: &str, inner_ty: &Ty, value_ty: &Ty) -> Option<IrExpr> {
    if is_container_ty(inner_ty) {
        let opt_ty = Ty::option(inner_ty.clone());
        let name = opt_field_dec_worker(wk, inner_ty, value_ty);
        return Some(e_(IrExprKind::Try { expr: Box::new(call_named_(&name, vec![
            payload,
            e_(IrExprKind::LitStr { value: key.to_string() }, Ty::String),
        ], Ty::result(opt_ty.clone(), Ty::String))) }, opt_ty));
    }
    if !is_value_ty(inner_ty) {
        return None;
    }
    let opt_ty = Ty::option(inner_ty.clone());
    let fv = wk.vt.alloc(sym("_fv"), value_ty.clone(), Mutability::Let, None);
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
                let is_null = call_mod_("value", "eq", vec![
                    fv_expr.clone(),
                    call_mod_("value", "null", vec![], value_ty.clone()),
                ], Ty::Bool);
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

    IrFunction {
        name: sym(&format!("{}.decode", type_name)),
        params: vec![IrParam { var: var_v, ty: value_ty, name: sym("_v"), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }],
        ret_ty: result_ty,
        body,
        is_effect: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
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

include!("derive_codec_variant.rs");
