// The arity guard of a derived tuple-variant decode (#4164). `include!`d by
// derive_codec.rs; it shares that module's scope and imports.
//
// A tuple case's payload is a positional array. Without this guard the decode
// indexed `_arr[i]` for every declared field, so `{"Admin": []}` aborted the
// process (index out of bounds) on every leg and `{"Admin": ["a", "b"]}` was
// accepted with the extra element dropped. A payload whose length is not the
// case's arity is now a decode error that names the case, the declared count
// and the received count.

/// `if list.len(_arr) != n then err("...") else body`.
fn tuple_arity_guard(arr: IrExpr, n: usize, case: &str, type_name: &str, result_ty: &Ty, body: IrExpr) -> IrExpr {
    let len = call_mod_("list", "len", vec![arr], Ty::Int);
    let mismatch = e_(IrExprKind::BinOp {
        op: BinOp::Neq,
        left: Box::new(len.clone()),
        right: Box::new(e_(IrExprKind::LitInt { value: n as i64 }, Ty::Int)),
    }, Ty::Bool);
    let msg = e_(IrExprKind::BinOp {
        op: BinOp::ConcatStr,
        left: Box::new(e_(IrExprKind::LitStr {
            value: format!("expected {n} element(s) for variant {case} of {type_name}, received "),
        }, Ty::String)),
        right: Box::new(call_mod_("int", "to_string", vec![len], Ty::String)),
    }, Ty::String);
    e_(IrExprKind::If {
        cond: Box::new(mismatch),
        then: Box::new(e_(IrExprKind::ResultErr { expr: Box::new(msg) }, result_ty.clone())),
        else_: Box::new(body),
    }, result_ty.clone())
}
