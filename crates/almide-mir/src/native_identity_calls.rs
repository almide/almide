//! Calls of a function that only returns its parameter (#4661, no perf loss).
//!
//! An erased newtype's accessor (`fn to_bytes(u: Uuid) -> Bytes`, whose body
//! is the wrapped value) lowers to a body that copies its heap parameter and
//! returns the copy, or that returns another such call. Called, it hands the
//! caller a fresh reference to the argument and nothing else. The native
//! render then took the parameter over (a move parameter) all the way up the
//! chain, so a caller that only read the result had to own its argument, and
//! a callback over a list's elements copied each element to call it.
//!
//! In the MIR's ownership model a heap parameter is borrowed and a call's
//! result is owned, so `d = f(x)` for such an `f` IS `Dup d = x`: one new
//! reference to the same block, no other op, no effect. The call is replaced
//! by that copy before the parameter modes, the render and the ownership
//! certificate are computed from the same MIR, and the copy is then judged
//! like any other (a copy only read is the same reference; one whose source
//! dies there is a move).
use crate::{CallArg, MirFunction, Op, ValueId};
use std::collections::BTreeMap;

/// Replace every call of an identity function in `functions` by a copy of
/// the argument it returns. A body that becomes an identity this way (a call
/// of one) is one too, so the rewrite runs until no call is left to replace.
pub(crate) fn inline(functions: &mut [MirFunction]) {
    loop {
        // The test harness's panic marker is an identity too, but its call is
        // what names a test abort's message (`native_test_panic::is_message`).
        let identities: BTreeMap<String, usize> = functions
            .iter()
            .filter(|f| !crate::native_test_harness::raw_panic(&f.name))
            .filter_map(|f| returned_param(f).map(|k| (f.name.clone(), k)))
            .collect();
        if identities.is_empty() {
            return;
        }
        let mut changed = false;
        for f in functions.iter_mut() {
            for op in f.ops.iter_mut() {
                if let Some(copy) = as_copy(op, &identities) {
                    *op = copy;
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
    }
}

/// The index of the parameter `f` returns a copy of, when its whole body is
/// that copy.
fn returned_param(f: &MirFunction) -> Option<usize> {
    let [Op::Dup { dst, src }] = f.ops.as_slice() else { return None };
    if f.ret != Some(*dst) || !f.owned_params.is_empty() {
        return None;
    }
    f.params.iter().position(|p| p.value == *src)
}

/// `op` as the copy it amounts to, when it calls an identity function on a
/// handle.
fn as_copy(op: &Op, identities: &BTreeMap<String, usize>) -> Option<Op> {
    let Op::CallFn { dst: Some(dst), name, args, .. } = op else { return None };
    let k = *identities.get(name)?;
    let CallArg::Handle(src) = args.get(k)? else { return None };
    let src: ValueId = *src;
    Some(Op::Dup { dst: *dst, src })
}
