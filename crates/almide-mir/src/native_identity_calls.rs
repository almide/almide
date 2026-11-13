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

/// Replace every call of an identity function in `functions` whose result
/// the caller only lends by a copy of the argument it returns. A function
/// whose body only calls an identity on its param is one too. A result the
/// caller takes (rebinds, returns, hands on) keeps its call, and with it the
/// call-site move the certificate states.
pub(crate) fn inline(functions: &mut [MirFunction]) {
    let identities = identities(functions);
    if identities.is_empty() {
        return;
    }
    for f in functions.iter_mut() {
        let root = crate::render_native_borrow_read::roots(f);
        let copies: Vec<(usize, Op)> = (0..f.ops.len())
            .filter_map(|i| as_copy(&f.ops[i], &identities).filter(|_| only_lent(f, i, &root)).map(|c| (i, c)))
            .collect();
        for (i, c) in copies {
            f.ops[i] = c;
        }
    }
}

/// The identity functions of `functions` and the param each returns: a body
/// that is a copy of a param, or a call of an identity on one. The test
/// harness's panic marker is an identity too, but its call is what names a
/// test abort's message (`native_test_panic::is_message`).
fn identities(functions: &[MirFunction]) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    loop {
        let before = out.len();
        for f in functions {
            if out.contains_key(&f.name) || crate::native_test_harness::raw_panic(&f.name) {
                continue;
            }
            if let Some(k) = returned_param(f, &out) {
                out.insert(f.name.clone(), k);
            }
        }
        if out.len() == before {
            return out;
        }
    }
}

/// The index of the parameter `f` returns, when its whole body is a copy of
/// it or a call of a known identity on it.
fn returned_param(f: &MirFunction, known: &BTreeMap<String, usize>) -> Option<usize> {
    let [op] = f.ops.as_slice() else { return None };
    let Op::Dup { dst, src } = as_copy(op, known).unwrap_or_else(|| op.clone()) else { return None };
    if f.ret != Some(dst) || !f.owned_params.is_empty() {
        return None;
    }
    f.params.iter().position(|p| p.value == src)
}

/// The result of the call at `i` is only lent: never rebound, returned or
/// handed over, and every use borrows it.
fn only_lent(f: &MirFunction, i: usize, root: &BTreeMap<ValueId, ValueId>) -> bool {
    let Some(d) = crate::mir_ops::defined_value(&f.ops[i]) else { return false };
    f.ret != Some(d)
        && !f.ops.iter().any(|o| matches!(o, Op::SetLocal { local: x, .. } | Op::Consume { v: x } | Op::MakeUnique { v: x } if *x == d))
        && crate::render_native_borrow_read::last_borrowing_use(f, i, d, root).is_some()
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
