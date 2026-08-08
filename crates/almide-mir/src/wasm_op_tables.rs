// The per-op VALUE tables — which ValueIds an op reads / touches — the
// occurrence walk under the render-level peepholes. Split in two halves per
// fn purely along op families (data vs flow); exhaustive matches, so a NEW
// Op variant breaks the build here until it joins one half.
// include!-spliced from render_wasm_b.rs.

/// is an ordinary read. An `IfThen`'s `dst` is the definition, not a read; the
/// `Else`/`EndIf` `val`s are reads (they feed the enclosing if-result).
pub(crate) fn op_reads(op: &Op, out: &mut Vec<ValueId>) {
    let args_vals = |args: &[CallArg], out: &mut Vec<ValueId>| {
        for a in args {
            match a {
                CallArg::Handle(v) | CallArg::Scalar(v) => out.push(*v),
                CallArg::Imm(_) | CallArg::Label(_) => {}
            }
        }
    };
    match op {
        Op::Charge { .. } => {}
        Op::ChargeDyn { src, .. } => out.push(*src),
        Op::Alloc { init, .. } => match init {
            Init::DynStr { len } | Init::DynList { len } | Init::DynListStr { len } => {
                out.push(*len)
            }
            Init::OptSome { payload } => out.push(*payload),
            Init::Opaque
            | Init::Empty
            | Init::OptNone
            | Init::IntList(_)
            | Init::Bytes(_)
            | Init::Str(_) => {}
        },
        Op::Const { .. } | Op::ConstInt { .. } | Op::FuncRef { .. } => {}
        Op::Dup { src, .. } => out.push(*src),
        Op::Drop { v }
        | Op::DropListStr { v }
        | Op::DropValue { v }
        | Op::DropListValue { v }
        | Op::DropListStrValue { v }
        | Op::DropListStrStr { v }
        | Op::DropListIntStr { v }
        | Op::DropListStrInt { v }
        | Op::DropResultListValue { v }
        | Op::DropResultValue { v }
        | Op::DropResultStrInt { v }
        | Op::DropResultValueInt { v }
        | Op::DropResultListValueInt { v }
        | Op::DropResultListStrInt { v }
        | Op::DropResultListStr { v }
        | Op::DropListListStr { v }
        | Op::DropVariant { v, .. }
        | Op::DropWrapperRec { v, .. }
        | Op::Consume { v }
        | Op::Borrow { v }
        | Op::MakeUnique { v } => out.push(*v),
        _ => op_reads_flow(op, out),
    }
}

/// The flow/call half of [`op_reads`] — Pure, the call family, lists,
/// arithmetic, prims and the control markers. Split from the data half
/// (alloc/const/dup/drop) purely along op families; arm bodies verbatim.
fn op_reads_flow(op: &Op, out: &mut Vec<ValueId>) {
    let args_vals = |args: &[CallArg], out: &mut Vec<ValueId>| {
        for a in args {
            match a {
                CallArg::Handle(v) | CallArg::Scalar(v) => out.push(*v),
                CallArg::Imm(_) | CallArg::Label(_) => {}
            }
        }
    };
    match op {
        Op::Pure { uses, .. } => out.extend(uses.iter().copied()),
        Op::Call { args, .. } | Op::CallFn { args, .. } | Op::CallImport { args, .. } => {
            args_vals(args, out);
        }
        Op::CallIndirect { table_idx, args, .. } => {
            out.push(*table_idx);
            args_vals(args, out);
        }
        Op::ListLit { elems, .. } => out.extend(elems.iter().copied()),
        Op::ListGetScalar { list, idx, .. } => {
            out.push(*list);
            out.push(*idx);
        }
        Op::ListSetScalar { list, idx, val } => {
            out.push(*list);
            out.push(*idx);
            out.push(*val);
        }
        Op::IntBinOp { a, b, .. } => {
            out.push(*a);
            out.push(*b);
        }
        Op::Prim { args, .. } => out.extend(args.iter().copied()),
        Op::IfThen { cond, .. } => out.push(*cond),
        Op::Else { val } | Op::EndIf { val } => {
            if let Some(v) = val {
                out.push(*v);
            }
        }
        Op::LoopBreakUnless { cond } => out.push(*cond),
        Op::LoopStart | Op::LoopEnd => {}
        Op::SetLocal { local, src } => {
            out.push(*local);
            out.push(*src);
        }
        // The data half's families — handled by the caller; listed (not `_`)
        // so a NEW Op variant still breaks the build until it joins one half
        // (the exhaustiveness the occurrence walk depends on).
        Op::Charge { .. } | Op::ChargeDyn { .. } | Op::Alloc { .. } | Op::Const { .. }
        | Op::ConstInt { .. } | Op::FuncRef { .. } | Op::Dup { .. } | Op::Drop { .. }
        | Op::DropListStr { .. } | Op::DropValue { .. } | Op::DropListValue { .. }
        | Op::DropListStrValue { .. } | Op::DropListStrStr { .. } | Op::DropListIntStr { .. }
        | Op::DropListStrInt { .. } | Op::DropResultListValue { .. } | Op::DropResultValue { .. }
        | Op::DropResultStrInt { .. } | Op::DropResultValueInt { .. }
        | Op::DropResultListValueInt { .. } | Op::DropResultListStrInt { .. }
        | Op::DropResultListStr { .. } | Op::DropListListStr { .. } | Op::DropVariant { .. }
        | Op::DropWrapperRec { .. } | Op::Consume { .. } | Op::Borrow { .. }
        | Op::MakeUnique { .. } => {}
    }
}

/// The value an op defines (binds), if any.
/// Every [`ValueId`] an op touches (dst + all operands), exhaustively — the
/// generic occurrence walk the render-level peepholes (#806 step 3b) use to
/// prove a value is single-use before fusing its def into its use site.
pub(crate) fn op_values(op: &Op, out: &mut Vec<ValueId>) {
    let args_vals = |args: &[CallArg], out: &mut Vec<ValueId>| {
        for a in args {
            match a {
                CallArg::Handle(v) | CallArg::Scalar(v) => out.push(*v),
                CallArg::Imm(_) | CallArg::Label(_) => {}
            }
        }
    };
    match op {
        Op::Charge { .. } | Op::ChargeDyn { .. } => {}
        Op::Alloc { dst, init, .. } => {
            out.push(*dst);
            match init {
                Init::DynStr { len } | Init::DynList { len } | Init::DynListStr { len } => {
                    out.push(*len)
                }
                Init::OptSome { payload } => out.push(*payload),
                Init::Opaque
                | Init::Empty
                | Init::OptNone
                | Init::IntList(_)
                | Init::Bytes(_)
                | Init::Str(_) => {}
            }
        }
        Op::Const { dst } | Op::ConstInt { dst, .. } | Op::FuncRef { dst, .. } => out.push(*dst),
        Op::Dup { dst, src } => {
            out.push(*dst);
            out.push(*src);
        }
        Op::Drop { v }
        | Op::DropListStr { v }
        | Op::DropValue { v }
        | Op::DropListValue { v }
        | Op::DropListStrValue { v }
        | Op::DropListStrStr { v }
        | Op::DropListIntStr { v }
        | Op::DropListStrInt { v }
        | Op::DropResultListValue { v }
        | Op::DropResultValue { v }
        | Op::DropResultStrInt { v }
        | Op::DropResultValueInt { v }
        | Op::DropResultListValueInt { v }
        | Op::DropResultListStrInt { v }
        | Op::DropResultListStr { v }
        | Op::DropListListStr { v }
        | Op::DropVariant { v, .. }
        | Op::DropWrapperRec { v, .. }
        | Op::Consume { v }
        | Op::Borrow { v }
        | Op::MakeUnique { v } => out.push(*v),
        _ => op_values_flow(op, out),
    }
}

/// The flow/call half of [`op_values`] — Pure, the call family, lists,
/// arithmetic, prims and the control markers. Split from the data half
/// (alloc/const/dup/drop) purely along op families; arm bodies verbatim.
/// Push an optional destination value, if the op has one.
fn push_opt(out: &mut Vec<ValueId>, v: Option<ValueId>) {
    if let Some(v) = v {
        out.push(v);
    }
}

/// A call's args: an immediate and a label name no MIR value.
fn push_call_arg_values(args: &[CallArg], out: &mut Vec<ValueId>) {
    for a in args {
        match a {
            CallArg::Handle(v) | CallArg::Scalar(v) => out.push(*v),
            CallArg::Imm(_) | CallArg::Label(_) => {}
        }
    }
}

fn op_values_flow(op: &Op, out: &mut Vec<ValueId>) {
    match op {
        Op::Pure { dst, uses } => {
            out.push(*dst);
            out.extend(uses.iter().copied());
        }
        Op::Call { dst, args, .. } | Op::CallFn { dst, args, .. } | Op::CallImport { dst, args, .. } => {
            push_opt(out, *dst);
            push_call_arg_values(args, out);
        }
        Op::CallIndirect { dst, table_idx, args, .. } => {
            push_opt(out, *dst);
            out.push(*table_idx);
            push_call_arg_values(args, out);
        }
        Op::ListLit { dst, elems } => {
            out.push(*dst);
            out.extend(elems.iter().copied());
        }
        Op::Prim { dst, args, .. } => {
            push_opt(out, *dst);
            out.extend(args.iter().copied());
        }
        Op::ListGetScalar { dst: a, list: b, idx: c }
        | Op::IntBinOp { dst: a, a: b, b: c, .. }
        | Op::ListSetScalar { list: a, idx: b, val: c } => out.extend([*a, *b, *c]),
        Op::SetLocal { local: a, src: b } => out.extend([*a, *b]),
        Op::IfThen { cond, dst } => {
            out.push(*cond);
            push_opt(out, *dst);
        }
        Op::Else { val } | Op::EndIf { val } => push_opt(out, *val),
        Op::LoopBreakUnless { cond } => out.push(*cond),
        Op::LoopStart | Op::LoopEnd => {}
        // The data half's families — handled by the caller; listed (not `_`)
        // so a NEW Op variant still breaks the build until it joins one half
        // (the exhaustiveness the occurrence walk depends on).
        Op::Charge { .. } | Op::ChargeDyn { .. } | Op::Alloc { .. } | Op::Const { .. }
        | Op::ConstInt { .. } | Op::FuncRef { .. } | Op::Dup { .. } | Op::Drop { .. }
        | Op::DropListStr { .. } | Op::DropValue { .. } | Op::DropListValue { .. }
        | Op::DropListStrValue { .. } | Op::DropListStrStr { .. } | Op::DropListIntStr { .. }
        | Op::DropListStrInt { .. } | Op::DropResultListValue { .. } | Op::DropResultValue { .. }
        | Op::DropResultStrInt { .. } | Op::DropResultValueInt { .. }
        | Op::DropResultListValueInt { .. } | Op::DropResultListStrInt { .. }
        | Op::DropResultListStr { .. } | Op::DropListListStr { .. } | Op::DropVariant { .. }
        | Op::DropWrapperRec { .. } | Op::Consume { .. } | Op::Borrow { .. }
        | Op::MakeUnique { .. } => {}
    }
}
