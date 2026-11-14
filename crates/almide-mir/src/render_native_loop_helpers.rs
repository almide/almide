// Small helpers of the native render loop (render_native_loop.rs), split out
// for the 800-line file budget; `include!`d from render_native.rs.

/// The owned kind a borrowed read's copy holds.
fn owned_of_ref(t: NTy) -> NTy {
    match t {
        NTy::StrRef => NTy::Str,
        NTy::VecRef => NTy::Vec,
        NTy::RecRef(j) => NTy::Rec(j),
        _ => NTy::VecStr,
    }
}

/// The copy `dst` made at `ops[at]` is never read: every other op naming it
/// releases exactly it, and it is not the function's result.
fn unread_copy(func: &MirFunction, at: usize, dst: ValueId) -> bool {
    let mut vs = Vec::new();
    func.ret != Some(dst)
        && func.ops.iter().enumerate().all(|(j, op)| {
            if j == at {
                return true;
            }
            vs.clear();
            crate::mir_ops::op_values(op, &mut vs);
            !vs.contains(&dst) || crate::native_move_params::is_release(op, dst)
        })
}
