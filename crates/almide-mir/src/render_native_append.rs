//! The native self-append window (#3639): `s = s + x` and `string.push(s, x)`
//! (which `rewrite_push_concat` lowers to the same rebind) reach the render as
//!
//! ```text
//! CallFn { dst: d, "__str_concat", [Handle(s), Handle(x)] }
//! Drop { s }
//! SetLocal { local: s, src: d }
//! ```
//!
//! rendered one op at a time that was `let d = rt_str_concat(&s, &x); s =
//! d.clone();` — a fresh string and a full copy of `s` (twice) on every
//! append, so a builder loop was quadratic. Here the three ops render as one
//! `s.push_str(x)`: amortized O(len(x)), the growth `String` already does.
//!
//! Sound because this render realizes every `Dup` as a `.clone()` — an alias
//! of `s` is its own `String`, never the block `push_str` grows — and `Drop`
//! is erased (scope end frees). The window is taken only when `d` is touched
//! by no other op (its value never escapes the rebind), `s` is already an
//! owned `String` local, and `x` is not `s` itself (`s.push_str(&s)` cannot
//! borrow `s` twice; that shape keeps the concat).

use crate::render_native::{as_str_arg, var, NTy};
use crate::{CallArg, MirFunction, Op, ValueId};
use std::collections::BTreeMap;

/// The `s.push_str(…)` line for the window opening at `ops[i]`, or `None`
/// when the three ops there are not that rebind.
pub(crate) fn str_self_append(func: &MirFunction, i: usize, tys: &BTreeMap<ValueId, NTy>) -> Option<String> {
    let [Op::CallFn { dst: Some(d), name, args, .. }, Op::Drop { v }, Op::SetLocal { local, src }] =
        func.ops.get(i..i + 3)?
    else {
        return None;
    };
    let [CallArg::Handle(s), CallArg::Handle(x)] = args.as_slice() else {
        return None;
    };
    if name != "__str_concat" || v != s || local != s || src != d || x == s {
        return None;
    }
    if tys.get(s) != Some(&NTy::Str) || func.ret == Some(*d) || touches(func, *d) != 2 {
        return None;
    }
    let xt = *tys.get(x)?;
    matches!(xt, NTy::Str | NTy::StrRef).then(|| format!("{}.push_str({});", var(*s), as_str_arg(&var(*x), xt)))
}

/// How many times `v` occurs across the function's ops (definitions and
/// operands, exhaustively).
fn touches(func: &MirFunction, v: ValueId) -> usize {
    let mut vals = Vec::new();
    for op in &func.ops {
        crate::mir_ops::op_values(op, &mut vals);
    }
    vals.iter().filter(|&&w| w == v).count()
}
