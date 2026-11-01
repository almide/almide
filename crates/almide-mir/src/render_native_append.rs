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
//! Sound because this render realizes a `Dup` as a `.clone()` — an alias of
//! `s` is its own `String`, never the block `push_str` grows — and `Drop` is
//! erased (scope end frees). The one `Dup` it renders as a shared borrow, an
//! arm feeding a borrow join (`crate::borrow_join`), never borrows `s` here:
//! the borrow-join analysis counts a join read by this window as live through
//! the window's `SetLocal` of `s`, so a join choosing `s` is declined there. Under ADR-0013's shared buffer a `.clone()`
//! is a count increment, so an alias DOES
//! share the block; the window stays sound because `AlmideStr::push_str` goes
//! through the unique view, which copies a shared block before it writes
//! (`spec/lang/alias_cow_test.almd` pins `let t = s; s = s + "x"`). The window is taken only when `d` is touched
//! by no other op (its value never escapes the rebind), `s` is already an
//! owned `String` local, and `x` is not `s` itself (`s.push_str(&s)` cannot
//! borrow `s` twice; that shape keeps the concat).

use crate::render_native::{as_str_arg, var, NTy};
use crate::{CallArg, MirFunction, Op, ValueId};
use std::collections::BTreeMap;

/// The `(s, x, d)` of the rebind `d = s + x; drop s; s = d` opening at
/// `ops[i]`, by shape alone — the window [`str_self_append`] may render as
/// `s.push_str(x)`, which writes `s` while it reads `x`.
pub(crate) fn self_append_window(func: &MirFunction, i: usize) -> Option<(ValueId, ValueId, ValueId)> {
    // The old value's drop is `Drop`, or `DropListStr` for a list that owns
    // its heap elements (a `List[String]` accumulator).
    let [Op::CallFn { dst: Some(d), name, args, .. }, Op::Drop { v } | Op::DropListStr { v }, Op::SetLocal { local, src }] =
        func.ops.get(i..i + 3)?
    else {
        return None;
    };
    let [CallArg::Handle(s), CallArg::Handle(x)] = args.as_slice() else {
        return None;
    };
    let concat = name == "__str_concat" || crate::native_list_ops::is_list_concat(name);
    (concat && v == s && local == s && src == d && x != s).then_some((*s, *x, *d))
}

/// The `s.push_str(…)` line for the window opening at `ops[i]`, or `None`
/// when the three ops there are not that rebind.
pub(crate) fn str_self_append(func: &MirFunction, i: usize, tys: &BTreeMap<ValueId, NTy>) -> Option<String> {
    let (s, x, d) = self_append_window(func, i)?;
    let (s, x, d) = (&s, &x, &d);
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

/// The list twin of [`str_self_append`] (#4006): `xs = xs + ys` (the rebind
/// `d = xs + ys; drop xs; xs = d`) renders as `xs.extend_from_slice(ys)`, so
/// an append loop is amortized linear (C-131) where a fresh list per append
/// was quadratic. Taken under the string window's conditions: `xs` an owned
/// list local, `d` touched by no other op, `ys` not `xs` itself.
pub(crate) fn list_self_append(
    func: &MirFunction,
    i: usize,
    tys: &BTreeMap<ValueId, NTy>,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) -> Option<String> {
    let (s, x, d) = self_append_window(func, i)?;
    let st = *tys.get(&s)?;
    if !matches!(st, NTy::Vec | NTy::VecStr | NTy::Rec(_)) || func.ret == Some(d) || touches(func, d) != 2 {
        return None;
    }
    let (_, own) = crate::native_list_ops::as_slice(&var(s), st)?;
    let xt = *tys.get(&x)?;
    let (xs, x_own) = crate::native_list_ops::as_slice(&var(x), xt)?;
    if own != x_own {
        return None;
    }
    // An owned appended list nothing reads again moves its elements over
    // (no per-element copy).
    if xt == st && crate::render_native_move::last_use(func, i, x, xt, borrow_arms) {
        return Some(format!("{}.append(&mut {});", var(s), var(x)));
    }
    Some(format!("{}.extend_from_slice({xs});", var(s)))
}

/// `list.push(xs, e)` (#4006, no-perf-loss rule): the lowering builds a fresh
/// one-element list and appends it (`t = [e]; d = xs + t; drop xs; xs = d`),
/// which [`list_self_append`] alone renders as a `vec![e]` allocation plus an
/// `extend_from_slice` per push. When the literal feeds only that append, the
/// four ops render as `xs.push(e)` (several scalar elements: one
/// `extend_from_slice` of an array, no heap literal). Taken under
/// [`list_self_append`]'s conditions, for a scalar list.
pub(crate) fn list_push(func: &MirFunction, i: usize, tys: &BTreeMap<ValueId, NTy>) -> Option<String> {
    if crate::render_native_move::drill_no_moves() {
        return None;
    }
    let Op::ListLit { dst: lit, elems } = func.ops.get(i)? else {
        return None;
    };
    let (s, x, d) = self_append_window(func, i + 1)?;
    if x != *lit || tys.get(&s) != Some(&NTy::Vec) || func.ret == Some(d) || touches(func, d) != 2 {
        return None;
    }
    if elems.is_empty() || !elems.iter().all(|e| tys.get(e) == Some(&NTy::I64)) || elems.contains(&s) {
        return None;
    }
    // The literal is read by nothing but the append (drops erase on this render).
    let mut vals = Vec::new();
    for op in func.ops.iter().filter(|op| !crate::render_native_move::bookkeeping(op)) {
        crate::mir_ops::op_values(op, &mut vals);
    }
    if vals.iter().filter(|&&w| w == *lit).count() != 2 || func.ret == Some(*lit) {
        return None;
    }
    Some(match elems.as_slice() {
        [e] => format!("{}.push({});", var(s), var(*e)),
        many => format!(
            "{}.extend_from_slice(&[{}]);",
            var(s),
            many.iter().map(|e| var(*e)).collect::<Vec<_>>().join(", ")
        ),
    })
}
