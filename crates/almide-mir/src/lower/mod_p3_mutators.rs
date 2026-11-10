// Functional mutator rewrites, included by mod_p3_b.rs.
/// The MULTI-BYTE in-place byte appends (the functional twins live in
/// bytes_append_multi.almd).
const BYTES_INPLACE_APPENDS: &[&str] = &[
    "push", "append_u8",
    "append_u16_le", "append_u16_be", "append_i16_le", "append_i16_be",
    "append_u32_le", "append_u32_be", "append_i32_le", "append_i32_be",
    "append_i64_le", "append_i64_be", "append_f32_le", "append_f32_be",
    "append_f64_le", "append_f64_be",
    "append_bool",
];

/// Rewrite a v0 IN-PLACE mutator call in statement position to its functional
/// rebind, or `None` when the call is not one (the effect-call path keeps it).
/// v1 has value semantics, so `bytes.push(buf, x)` becomes `buf =
/// bytes.append(buf, x)`, `list.push(xs, v)` / `string.push(s, x)` become the one synthetic
/// concat the source Call node already credits (mir <= ir holds), and
/// `map.clear(m)` / `list.clear(xs)` rebind to the EMPTY literal of the
/// receiver's own type (adds no call). Only a bare `Var` receiver qualifies
/// (push also admits a FIELD receiver — the C-132 mut-param write-back shape,
/// routed through FieldAssign); any other receiver keeps the (walling)
/// effect-call path.
fn rewrite_inplace_mutation(expr: &IrExpr) -> Option<IrStmt> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &expr.kind
    else {
        return None;
    };
    let (module, func) = (module.as_str(), func.as_str());
    rewrite_bytes_append(module, func, args)
        .or_else(|| rewrite_push_concat(module, func, args))
        .or_else(|| rewrite_clear(module, func, args))
}

/// `bytes.push(buf, x)` — the v0 intrinsic is an IN-PLACE mutation (`mut b -> Unit`).
/// Rewrite it to a functional rebind `buf = bytes.append(buf, x)` and re-dispatch —
/// the Assign path then handles it (a scalar-loop accumulator SetLocal via the
/// general heap-reassign, or a top-level rebind). `bytes.append` is the
/// self-hosted functional append (bytes_core). Unblocks bigint.from_int / rsa.
/// `bytes.append_u8(buf, x)` is the SAME in-place byte push under another
/// name (`almide_rt_bytes_append_u8`) — identical rewrite; push/append_u8 route
/// to the 1-byte `bytes.append`, every multi-byte variant keeps its own name.
fn rewrite_bytes_append(module: &str, func: &str, args: &[IrExpr]) -> Option<IrStmt> {
    if module != "bytes"
        || !BYTES_INPLACE_APPENDS.contains(&func)
        || !matches!(args.len(), 2 | 3)
    {
        return None;
    }
    let IrExprKind::Var { id } = &args[0].kind else { return None };
    let fname = match func {
        // The 1-byte pushes both route to the one `bytes.append` twin.
        "push" | "append_u8" => "append",
        _ => func,
    };
    let append = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("bytes"), func: sym(fname), def_id: None },
            args: args.to_vec(),
            type_args: vec![],
        },
        ty: args[0].ty.clone(),
        span: None,
        def_id: None,
    };
    Some(IrStmt { kind: IrStmtKind::Assign { var: *id, value: append }, span: None })
}

/// `list.push(xs, v)` / `string.push(s, x)` — v0 in-place appends: the
/// same functional-rebind treatment as bytes.push (`xs = xs + [v]` /
/// `s = s + x`); the ConcatList/ConcatStr lowering then emits the ONE
/// synthetic concat call the source Call node already credits (mir <= ir
/// holds). A `Var` receiver rebinds via Assign; a FIELD receiver
/// (`list.push(b.xs, v)` — the C-132 mut-param write-back shape) routes
/// through FieldAssign, whose spread rebind owns the write-back.
fn rewrite_push_concat(module: &str, func: &str, args: &[IrExpr]) -> Option<IrStmt> {
    if func != "push" || !matches!(module, "list" | "string") || args.len() != 2 {
        return None;
    }
    let is_list = module == "list";
    let recv = args[0].clone();
    let rhs = if is_list {
        IrExpr {
            kind: IrExprKind::List { elements: vec![args[1].clone()] },
            ty: recv.ty.clone(),
            span: None,
            def_id: None,
        }
    } else {
        args[1].clone()
    };
    let concat = IrExpr {
        kind: IrExprKind::BinOp {
            op: if is_list { almide_ir::BinOp::ConcatList } else { almide_ir::BinOp::ConcatStr },
            left: Box::new(recv.clone()),
            right: Box::new(rhs),
        },
        ty: recv.ty.clone(),
        span: None,
        def_id: None,
    };
    write_rebuilt_place(&recv, concat)
}

/// `map.clear(m)` / `list.clear(xs)` / `string.clear(s)` / `bytes.clear(b)` —
/// the in-place empty: rebind the var to a FRESH empty value of the receiver's
/// own type. The var write-back is the C-033 discipline (#2465): an alias or
/// holder bound BEFORE the clear keeps the pre-clear block, and through a `mut`
/// parameter the rebind persists to the caller exactly as `list.clear` does.
/// The self-hosted `string_clear` / `bytes_clear` bodies (a length-header
/// store into the SHARED block) must never be reached from here — they are
/// the alias-observes-clear defect the released 0.62.0 default route shipped.
/// map / list / string rebind to a literal (adds no call); bytes rebinds to
/// `bytes.zeros(0)` — one call for the one clear call the source node credits
/// (mir <= ir holds). A `Var` receiver rebinds via Assign; a FIELD receiver
/// (`string.clear(r.name)` — the C-132 mut-param write-back shape) routes
/// through FieldAssign exactly as `list.push(b.xs, v)` does, so the record's
/// alias keeps its field too. Any other receiver keeps the (walling)
/// effect-call path.
fn rewrite_clear(module: &str, func: &str, args: &[IrExpr]) -> Option<IrStmt> {
    if func != "clear" || !matches!(module, "map" | "list" | "string" | "bytes") || args.len() != 1 {
        return None;
    }
    let ty = args[0].ty.clone();
    let kind = match module {
        "map" => IrExprKind::EmptyMap,
        "list" => IrExprKind::List { elements: vec![] },
        "string" => IrExprKind::LitStr { value: String::new() },
        _ => IrExprKind::Call {
            target: CallTarget::Module { module: sym("bytes"), func: sym("zeros"), def_id: None },
            args: vec![IrExpr {
                kind: IrExprKind::LitInt { value: 0 },
                ty: Ty::Int,
                span: None,
                def_id: None,
            }],
            type_args: vec![],
        },
    };
    let value = IrExpr { kind, ty, span: None, def_id: None };
    write_rebuilt_place(&args[0], value)
}

/// Rebuild each enclosing record along a member path. Base evaluation and
/// ownership transfers stay in the existing spread / FieldAssign lowering.
pub(crate) fn write_rebuilt_place(place: &IrExpr, value: IrExpr) -> Option<IrStmt> {
    let kind = match &place.kind {
        IrExprKind::Var { id } => IrStmtKind::Assign { var: *id, value },
        IrExprKind::Member { object, field } => {
            if let IrExprKind::Var { id } = object.kind {
                IrStmtKind::FieldAssign { target: id, field: *field, value }
            } else if crate::lower::owned_tco_slots() {
                let update = IrExpr {
                    kind: IrExprKind::SpreadRecord { base: object.clone(), fields: vec![(*field, value)] },
                    ty: object.ty.clone(), span: place.span, def_id: None,
                };
                return write_rebuilt_place(object, update);
            } else { return None; }
        }
        _ => return None,
    };
    Some(IrStmt { kind, span: place.span })
}

impl LowerCtx {
    pub(crate) fn lower_place_mutation(&mut self, target: VarId, span: Option<almide_ir::Span>) -> Result<(), LowerError> {
        let v = self.value_for(target)?;
        if self.param_values.contains(&v) {
            if crate::lower::owned_tco_slots() {
                let ty = self.var_decl_tys.get(&target).cloned().ok_or_else(||
                    LowerError::at(span, "native mutation target has no declared type"))?;
                let source = IrExpr { kind: IrExprKind::Var { id: target }, ty: ty.clone(), span: None, def_id: None };
                // The ordinary owned alias keeps the borrowed block's read
                // shape and recursive release route before making it unique.
                self.lower_bind_heap_var_alias(target, &ty, &source)?;
                self.ops.push(Op::MakeUnique { v: self.value_for(target)? });
                return Ok(());
            }
            // The C-132 move-mode body (`{ self.n = self.n + by; self }` — a
            // rewritten `mut` receiver/param, #1557 leg 1): the BORROW cannot
            // be mutated in place — `MakeUnique` alone would `rc_dec` a count
            // the borrow never incremented, stealing the CALLER's reference
            // (undercount → UAF on the rc>1 alias path). Take an OWNED COW
            // copy instead: `Dup` (+1) then `MakeUnique` (its dec restores
            // the count, and rc>1 always holds after the Dup, so the local is
            // a fresh unique clone). The body's stores land on the clone; the
            // rewrite's tail `p` read RETURNS it (the call site's write-back
            // owns it) — value semantics exactly, the caller's block and any
            // alias untouched. From here on the var maps to a plain tracked
            // local (scope-end drop if the tail is never reached on a path).
            let owned = self.fresh_value();
            self.ops.push(Op::Dup { dst: owned, src: v });
            self.ops.push(Op::MakeUnique { v: owned });
            self.live_heap_handles.push(owned);
            self.value_of.insert(target, owned);
            return Ok(());
        }
        self.ops.push(Op::MakeUnique { v });
        Ok(())
    }

}
