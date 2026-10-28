// Auto-derive Codec, the strict half (row 22, C-429 / C-436). `include!`d by
// derive_codec.rs; it shares that module's scope and imports.
//
// `T.decode_strict(v)` refuses a key outside the type at EVERY depth, then
// decodes with `T.decode`. The refusal is a separate walk, `T.__strict(v) ->
// String` ("" = clean), so `T.decode` and its byte-pinned helper routes stay
// untouched:
//
//   record   the first key of `v` outside its decoded fields is
//            `unknown key '<k>'`; then each field whose type holds a
//            `Decode` type is walked, its error carrying the field's path
//            (`unknown key 'k', at address.geo`, `…, at tags[1]`).
//   variant  the wire form is `{"<Case>": payload}`: a key after the tag is
//            `unknown key '<k>'`; a record payload is walked like a record
//            and a tuple payload element by element, under the case's path.
//            An unknown tag is `T.decode`'s own `unknown variant for T`.
//
// Every worker is a dotted per-type fn (`T.__strict…`), built only from the
// calls the decode derive already uses on every leg (`value.__keys`,
// `value.omit`, `value.__field`, `value.as_array`, `list.get`, `list.len`,
// `__err_at`, `__err_at_index`), so no leg needs a new helper. A value of the
// wrong shape walks clean: `T.decode` reports the shape error itself.

fn s_lit(s: &str) -> IrExpr {
    e_(IrExprKind::LitStr { value: s.to_string() }, Ty::String)
}

fn s_var(id: VarId) -> IrExpr {
    e_(IrExprKind::Var { id }, Ty::String)
}

fn v_var(id: VarId) -> IrExpr {
    e_(IrExprKind::Var { id }, Ty::Named(sym("Value"), vec![]))
}

fn is_empty_str(e: IrExpr) -> IrExpr {
    e_(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(e), right: Box::new(s_lit("")) }, Ty::Bool)
}

fn s_if(cond: IrExpr, then: IrExpr, else_: IrExpr) -> IrExpr {
    e_(IrExprKind::If { cond: Box::new(cond), then: Box::new(then), else_: Box::new(else_) }, Ty::String)
}

/// The first non-empty message of `msgs`, in order, each evaluated only when
/// every one before it was empty.
fn first_nonempty(wk: &mut CodecWk, msgs: Vec<IrExpr>) -> IrExpr {
    let mut acc = s_lit("");
    for m in msgs.into_iter().rev() {
        let t = wk.vt.alloc(sym("_sm"), Ty::String, Mutability::Let, None);
        let bind = IrStmt { kind: IrStmtKind::Bind { var: t, mutability: Mutability::Let, ty: Ty::String, value: m }, span: None };
        let body = s_if(is_empty_str(s_var(t)), acc, s_var(t));
        acc = e_(IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(body)) }, Ty::String);
    }
    acc
}

/// The first key of `v` outside `allowed` (`list.get(value.__keys(value.omit(v, allowed)), 0)`).
fn first_unknown_key(v: IrExpr, allowed: Vec<IrExpr>) -> IrExpr {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let omitted = call_mod_("value", "omit", vec![v, e_(IrExprKind::List { elements: allowed }, Ty::list(Ty::String))], value_ty);
    let keys = call_mod_("value", "__keys", vec![omitted], Ty::list(Ty::String));
    call_mod_("list", "get", vec![keys, e_(IrExprKind::LitInt { value: 0 }, Ty::Int)], Ty::option(Ty::String))
}

/// Does a value of `ty` hold a type the strict walk must enter?
fn strict_needed(wk: &CodecWk, ty: &Ty) -> bool {
    match ty {
        Ty::Named(n, _) => (wk.strict_types)(*n),
        Ty::Applied(TypeConstructorId::Option, a) | Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => strict_needed(wk, &a[0]),
        _ => false,
    }
}

/// The strict walk of `x: Value` as a value of `ty`, or `None` when nothing
/// in `ty` needs one. An `Option` walks its payload (a null walks clean).
fn strict_value_expr(wk: &mut CodecWk, x: IrExpr, ty: &Ty) -> Option<IrExpr> {
    if !strict_needed(wk, ty) {
        return None;
    }
    match ty {
        Ty::Named(n, _) => Some(call_named_(&format!("{}.__strict", n), vec![x], Ty::String)),
        Ty::Applied(TypeConstructorId::Option, a) => strict_value_expr(wk, x, &a[0].clone()),
        Ty::Applied(TypeConstructorId::List, a) => {
            let name = strict_list_worker(wk, ty, &a[0].clone());
            Some(call_named_(&name, vec![x], Ty::String))
        }
        _ => None,
    }
}

/// `T.__strict_at(m, seg)`: "" stays "", a message gains the path segment.
fn strict_at_worker(wk: &mut CodecWk) -> String {
    let name = format!("{}.__strict_at", wk.type_name);
    if !wk.seen.insert(name.clone()) {
        return name;
    }
    let m = wk.vt.alloc(sym("_m"), Ty::String, Mutability::Let, None);
    let seg = wk.vt.alloc(sym("_seg"), Ty::String, Mutability::Let, None);
    let body = s_if(is_empty_str(s_var(m)), s_lit(""), call_named_("__err_at", vec![s_var(m), s_var(seg)], Ty::String));
    wk.out.push(mk_worker_fn(&name, vec![(m, "_m", Ty::String), (seg, "_seg", Ty::String)], Ty::String, body));
    name
}

/// `T.__strict_idx(m, i)`: "" stays "", a message gains `[i]`.
fn strict_idx_worker(wk: &mut CodecWk) -> String {
    let name = format!("{}.__strict_idx", wk.type_name);
    if !wk.seen.insert(name.clone()) {
        return name;
    }
    let m = wk.vt.alloc(sym("_m"), Ty::String, Mutability::Let, None);
    let i = wk.vt.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
    let body = s_if(is_empty_str(s_var(m)), s_lit(""), call_named_("__err_at_index", vec![s_var(m), e_(IrExprKind::Var { id: i }, Ty::Int)], Ty::String));
    wk.out.push(mk_worker_fn(&name, vec![(m, "_m", Ty::String), (i, "_i", Ty::Int)], Ty::String, body));
    name
}

/// `T.__strict_<mangle>(x)` for a `List[elem]` value: each element walked
/// in order by an index-recursive `_go` worker, the first message winning.
fn strict_list_worker(wk: &mut CodecWk, list_ty: &Ty, elem: &Ty) -> String {
    let name = format!("{}.__strict_{}", wk.type_name, ty_mangle(list_ty));
    if !wk.seen.insert(name.clone()) {
        return name;
    }
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let lv = Ty::list(value_ty.clone());
    let go = format!("{}_go", name);
    let next = format!("{}_next", name);
    let idx = strict_idx_worker(wk);
    // go(xs, i) = if i >= list.len(xs) then "" else next(idx(<elem>(xs[i] ?? null), i), xs, i)
    {
        let xs = wk.vt.alloc(sym("_xs"), lv.clone(), Mutability::Let, None);
        let i = wk.vt.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
        let iv = || e_(IrExprKind::Var { id: i }, Ty::Int);
        let xsv = || e_(IrExprKind::Var { id: xs }, lv.clone());
        let elem_v = e_(IrExprKind::UnwrapOr {
            expr: Box::new(call_mod_("list", "get", vec![xsv(), iv()], Ty::option(value_ty.clone()))),
            fallback: Box::new(call_mod_("value", "null", vec![], value_ty.clone())),
        }, value_ty.clone());
        let walked = strict_value_expr(wk, elem_v, elem).unwrap_or_else(|| s_lit(""));
        let msg = call_named_(&idx, vec![walked, iv()], Ty::String);
        let cond = e_(IrExprKind::BinOp {
            op: BinOp::Gte,
            left: Box::new(iv()),
            right: Box::new(call_mod_("list", "len", vec![xsv()], Ty::Int)),
        }, Ty::Bool);
        let body = s_if(cond, s_lit(""), call_named_(&next, vec![msg, xsv(), iv()], Ty::String));
        wk.out.push(mk_worker_fn(&go, vec![(xs, "_xs", lv.clone()), (i, "_i", Ty::Int)], Ty::String, body));
    }
    // next(m, xs, i) = if m == "" then go(xs, i + 1) else m
    {
        let m = wk.vt.alloc(sym("_m"), Ty::String, Mutability::Let, None);
        let xs = wk.vt.alloc(sym("_xs"), lv.clone(), Mutability::Let, None);
        let i = wk.vt.alloc(sym("_i"), Ty::Int, Mutability::Let, None);
        let succ = e_(IrExprKind::BinOp {
            op: BinOp::AddInt,
            left: Box::new(e_(IrExprKind::Var { id: i }, Ty::Int)),
            right: Box::new(e_(IrExprKind::LitInt { value: 1 }, Ty::Int)),
        }, Ty::Int);
        let body = s_if(is_empty_str(s_var(m)),
            call_named_(&go, vec![e_(IrExprKind::Var { id: xs }, lv.clone()), succ], Ty::String),
            s_var(m));
        wk.out.push(mk_worker_fn(&next, vec![(m, "_m", Ty::String), (xs, "_xs", lv.clone()), (i, "_i", Ty::Int)], Ty::String, body));
    }
    // T.__strict_<mangle>(x) = match value.as_array(x) { ok(xs) => go(xs, 0), err(_) => "" }
    let x = wk.vt.alloc(sym("_x"), value_ty.clone(), Mutability::Let, None);
    let xs = wk.vt.alloc(sym("_xs"), lv.clone(), Mutability::Let, None);
    let body = e_(IrExprKind::Match {
        subject: Box::new(call_mod_("value", "as_array", vec![v_var(x)], Ty::result(lv.clone(), Ty::String))),
        arms: vec![
            IrMatchArm {
                pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: xs, ty: lv.clone() }) },
                guard: None,
                body: call_named_(&go, vec![e_(IrExprKind::Var { id: xs }, lv.clone()), e_(IrExprKind::LitInt { value: 0 }, Ty::Int)], Ty::String),
            },
            IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: s_lit("") },
        ],
    }, Ty::String);
    wk.out.push(mk_worker_fn(&name, vec![(x, "_x", value_ty)], Ty::String, body));
    name
}

/// `unknown key '<k>'` for the first key of `v` outside `allowed`, else "".
fn unknown_key_expr(wk: &mut CodecWk, v: IrExpr, allowed: Vec<IrExpr>) -> IrExpr {
    let first = first_unknown_key(v, allowed);
    let k = wk.vt.alloc(sym("_k"), Ty::String, Mutability::Let, None);
    let msg = e_(IrExprKind::BinOp {
        op: BinOp::ConcatStr,
        left: Box::new(e_(IrExprKind::BinOp { op: BinOp::ConcatStr, left: Box::new(s_lit("unknown key '")), right: Box::new(s_var(k)) }, Ty::String)),
        right: Box::new(s_lit("'")),
    }, Ty::String);
    e_(IrExprKind::Match {
        subject: Box::new(first),
        arms: vec![
            IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: k, ty: Ty::String }) }, guard: None, body: msg },
            IrMatchArm { pattern: IrPattern::None, guard: None, body: s_lit("") },
        ],
    }, Ty::String)
}

/// A worker `T.__strict_<tag>(v) -> String` whose body is
/// `match value.__field(v, "<key>") { ok(x) => at(<walk x>, "<key>"), err(_) => "" }`.
fn strict_field_worker(wk: &mut CodecWk, tag: &str, key: &str, walk_ty: &Ty) -> Option<String> {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let name = format!("{}.__strict_f_{}", wk.type_name, tag);
    if wk.seen.contains(&name) {
        return Some(name);
    }
    let x = wk.vt.alloc(sym("_x"), value_ty.clone(), Mutability::Let, None);
    let walked = strict_value_expr(wk, v_var(x), walk_ty)?;
    wk.seen.insert(name.clone());
    let at = strict_at_worker(wk);
    let v = wk.vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
    let body = e_(IrExprKind::Match {
        subject: Box::new(call_mod_("value", "__field", vec![v_var(v), s_lit(key)], Ty::result(value_ty.clone(), Ty::String))),
        arms: vec![
            IrMatchArm {
                pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: x, ty: value_ty.clone() }) },
                guard: None,
                body: call_named_(&at, vec![walked, s_lit(key)], Ty::String),
            },
            IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: s_lit("") },
        ],
    }, Ty::String);
    wk.out.push(mk_worker_fn(&name, vec![(v, "_v", value_ty)], Ty::String, body));
    Some(name)
}

/// The strict walk of a record shape (a record type, or a record payload):
/// its own unknown key first, then each field that holds a `Decode` type.
fn strict_record_body(wk: &mut CodecWk, v: VarId, scope: &str, fields: &[IrFieldDecl]) -> IrExpr {
    let key = |f: &IrFieldDecl| f.alias.map(|a| a.to_string()).unwrap_or_else(|| f.name.to_string());
    let allowed: Vec<IrExpr> = fields.iter()
        .filter(|f| crate::attr_vocab::codec_field_use(&f.attrs).decoded)
        .map(|f| s_lit(&key(f)))
        .collect();
    let mut msgs = vec![unknown_key_expr(wk, v_var(v), allowed)];
    for f in fields.iter().filter(|f| crate::attr_vocab::codec_field_use(&f.attrs).decoded) {
        let tag = format!("{}_{}", scope, f.name);
        if let Some(w) = strict_field_worker(wk, &tag, &key(f), &f.ty) {
            msgs.push(call_named_(&w, vec![v_var(v)], Ty::String));
        }
    }
    first_nonempty(wk, msgs)
}

/// `T.__strict(v)` for a record type.
pub(super) fn auto_derive_record_strict(wk: &mut CodecWk, fields: &[IrFieldDecl]) -> IrFunction {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let v = wk.vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
    let body = strict_record_body(wk, v, "r", fields);
    mk_worker_fn(&format!("{}.__strict", wk.type_name), vec![(v, "_v", value_ty)], Ty::String, body)
}

/// `T.__strict(v)` for a variant type: the tag is the first key (as
/// `T.decode` reads it); a key after it is unknown, and the tagged case's
/// payload is walked under the case's path.
pub(super) fn auto_derive_variant_strict(wk: &mut CodecWk, cases: &[IrVariantDecl]) -> IrFunction {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let at = strict_at_worker(wk);
    // One payload worker per case that needs a walk: `T.__strict_c_<Case>(v)`.
    let mut case_workers: Vec<(String, String)> = Vec::new();
    for case in cases {
        let cname = case.name.to_string();
        let payload_walk: Option<String> = match &case.kind {
            IrVariantKind::Unit => None,
            IrVariantKind::Record { fields } => {
                let p = wk.vt.alloc(sym("_p"), value_ty.clone(), Mutability::Let, None);
                let body = strict_record_body(wk, p, &format!("c_{}", cname), fields);
                let name = format!("{}.__strict_rc_{}", wk.type_name, cname);
                wk.out.push(mk_worker_fn(&name, vec![(p, "_p", value_ty.clone())], Ty::String, body));
                Some(name)
            }
            IrVariantKind::Tuple { fields } => {
                let idx = strict_idx_worker(wk);
                let lv = Ty::list(value_ty.clone());
                let p = wk.vt.alloc(sym("_p"), value_ty.clone(), Mutability::Let, None);
                let xs = wk.vt.alloc(sym("_xs"), lv.clone(), Mutability::Let, None);
                let mut msgs = Vec::new();
                for (i, t) in fields.iter().enumerate() {
                    let elem = e_(IrExprKind::UnwrapOr {
                        expr: Box::new(call_mod_("list", "get", vec![e_(IrExprKind::Var { id: xs }, lv.clone()), e_(IrExprKind::LitInt { value: i as i64 }, Ty::Int)], Ty::option(value_ty.clone()))),
                        fallback: Box::new(call_mod_("value", "null", vec![], value_ty.clone())),
                    }, value_ty.clone());
                    if let Some(w) = strict_value_expr(wk, elem, t) {
                        msgs.push(call_named_(&idx, vec![w, e_(IrExprKind::LitInt { value: i as i64 }, Ty::Int)], Ty::String));
                    }
                }
                if msgs.is_empty() {
                    None
                } else {
                    let walk = first_nonempty(wk, msgs);
                    let body = e_(IrExprKind::Match {
                        subject: Box::new(call_mod_("value", "as_array", vec![v_var(p)], Ty::result(lv.clone(), Ty::String))),
                        arms: vec![
                            IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: xs, ty: lv.clone() }) }, guard: None, body: walk },
                            IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: s_lit("") },
                        ],
                    }, Ty::String);
                    let name = format!("{}.__strict_tc_{}", wk.type_name, cname);
                    wk.out.push(mk_worker_fn(&name, vec![(p, "_p", value_ty.clone())], Ty::String, body));
                    Some(name)
                }
            }
        };
        if let Some(walk) = payload_walk {
            // T.__strict_c_<Case>(v) = match value.__field(v, "<Case>") { ok(p) => at(walk(p), "<Case>"), err(_) => "" }
            let v = wk.vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
            let p = wk.vt.alloc(sym("_p"), value_ty.clone(), Mutability::Let, None);
            let body = e_(IrExprKind::Match {
                subject: Box::new(call_mod_("value", "__field", vec![v_var(v), s_lit(&cname)], Ty::result(value_ty.clone(), Ty::String))),
                arms: vec![
                    IrMatchArm {
                        pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: p, ty: value_ty.clone() }) },
                        guard: None,
                        body: call_named_(&at, vec![call_named_(&walk, vec![v_var(p)], Ty::String), s_lit(&cname)], Ty::String),
                    },
                    IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: s_lit("") },
                ],
            }, Ty::String);
            let name = format!("{}.__strict_c_{}", wk.type_name, cname);
            wk.out.push(mk_worker_fn(&name, vec![(v, "_v", value_ty.clone())], Ty::String, body));
            case_workers.push((cname, name));
        }
    }
    // T.__strict_case(v, tag) = if tag == "A" then T.__strict_c_A(v) else … else ""
    let case_fn = format!("{}.__strict_case", wk.type_name);
    {
        let v = wk.vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
        let tag = wk.vt.alloc(sym("_tag"), Ty::String, Mutability::Let, None);
        let mut chain = s_lit("");
        for (cname, w) in case_workers.iter().rev() {
            let cond = e_(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(s_var(tag)), right: Box::new(s_lit(cname)) }, Ty::Bool);
            chain = s_if(cond, call_named_(w, vec![v_var(v)], Ty::String), chain);
        }
        wk.out.push(mk_worker_fn(&case_fn, vec![(v, "_v", value_ty.clone()), (tag, "_tag", Ty::String)], Ty::String, chain));
    }
    // T.__strict(v) = { let tag = list.get(value.__keys(v), 0) ?? ""; first_nonempty([unknown key after the tag, case walk]) }
    let v = wk.vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
    let tag = wk.vt.alloc(sym("_tag"), Ty::String, Mutability::Let, None);
    let tag_expr = e_(IrExprKind::UnwrapOr {
        expr: Box::new(call_mod_("list", "get", vec![
            call_mod_("value", "__keys", vec![v_var(v)], Ty::list(Ty::String)),
            e_(IrExprKind::LitInt { value: 0 }, Ty::Int),
        ], Ty::option(Ty::String))),
        fallback: Box::new(s_lit("")),
    }, Ty::String);
    let bind_tag = IrStmt { kind: IrStmtKind::Bind { var: tag, mutability: Mutability::Let, ty: Ty::String, value: tag_expr }, span: None };
    let own = unknown_key_expr(wk, v_var(v), vec![s_var(tag)]);
    let walk = call_named_(&case_fn, vec![v_var(v), s_var(tag)], Ty::String);
    let rest = first_nonempty(wk, vec![own, walk]);
    let body = e_(IrExprKind::Block { stmts: vec![bind_tag], expr: Some(Box::new(rest)) }, Ty::String);
    mk_worker_fn(&format!("{}.__strict", wk.type_name), vec![(v, "_v", value_ty)], Ty::String, body)
}

/// `T.decode_strict(v)` for a record or a variant:
/// `if T.__strict(v) == "" then T.decode(v) else err(T.__strict(v))` — the
/// walk runs twice only on the refusing path.
pub(super) fn auto_derive_decode_strict(wk: &mut CodecWk, type_ty: &Ty) -> IrFunction {
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let result_ty = Ty::result(type_ty.clone(), Ty::String);
    let type_name = wk.type_name.to_string();
    let v = wk.vt.alloc(sym("_v"), value_ty.clone(), Mutability::Let, None);
    let walk = || call_named_(&format!("{}.__strict", type_name), vec![v_var(v)], Ty::String);
    let body = e_(IrExprKind::If {
        cond: Box::new(is_empty_str(walk())),
        then: Box::new(call_named_(&format!("{}.decode", type_name), vec![v_var(v)], result_ty.clone())),
        else_: Box::new(e_(IrExprKind::ResultErr { expr: Box::new(walk()) }, result_ty.clone())),
    }, result_ty.clone());
    mk_worker_fn(&format!("{}.{}", type_name, crate::canonicalize::protocols::STRICT_DECODE_METHOD), vec![(v, "_v", value_ty)], result_ty, body)
}
