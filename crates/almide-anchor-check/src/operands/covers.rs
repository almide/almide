//! The guard's operands held to its access (operands.rs, module header):
//! the list compare, a loop's exit test, the room and empty tests, and a
//! fresh block's own size.

use super::*;

/// `len(B)`: `(B, the load)`.
pub(super) fn len(l: &T) -> Option<(&T, usize)> {
    match pl(l) {
        T::Load { name: "i32.load", off: LEN_OFF, addr, site } => Some((&**addr, *site)),
        _ => None,
    }
}

/// `0 <= I && I <= len(B) - w` (signed): `(I, I again, (B, its count's
/// load), w)`.
fn room_of(room: &T) -> Option<(&T, &T, (&T, usize), i64)> {
    let [lo, hi] = op(room, "i32.and")? else { return None };
    let [i1, z] = op(lo, "i64.ge_s")? else { return None };
    let [i2, d] = op(hi, "i64.le_s")? else { return None };
    if cst(z)? != 0 {
        return None;
    }
    let [e, w] = op(d, "i64.sub")? else { return None };
    let [l] = op(e, "i64.extend_i32_u")? else { return None };
    Some((i1, i2, len(l)?, cst(w)?))
}

/// `len(B) / S` as the count operand: `((B, its count's load), S)`.
fn count_of(t: &T) -> Option<((&T, usize), i64)> {
    if let Some([d]) = op(t, "i64.extend_i32_u") {
        let [l, s] = op(d, "i32.div_u")? else { return None };
        return Some((len(l)?, cst(s)?));
    }
    let [e, s] = op(t, "i64.div_u")? else { return None };
    let [l] = op(e, "i64.extend_i32_u")? else { return None };
    Some((len(l)?, cst(s)?))
}

/// `len(B) / S` as an i32 count (a loop's captured count, #4280), or
/// `len(B)` itself: a string's byte count, slots of one byte (#4281).
fn count_of32(t: &T) -> Option<((&T, usize), i64)> {
    if let Some(l) = len(t) {
        return Some((l, 1));
    }
    let [l, s] = op(t, "i32.div_u")? else { return None };
    Some((len(l)?, cst(s)?))
}

/// The `br_if` at `at` is a loop's exit test (module header): an
/// `i32.ge_u` right before it, it sits straight inside a `loop`, and it
/// branches to the `block` that loop's parent is.
pub(crate) fn loop_exit(fl: &Flow<'_, '_>, at: usize) -> Result<(), String> {
    if !matches!(fl.b.ops[at], Op::BrIf { relative_depth: 1 }) {
        return Err(format!("anchored at {:?}, not `br_if 1`", fl.b.ops[at]));
    }
    // `i32.ge_u` (a cursor counting up to its count), or `i32.eqz` (a
    // cursor walking down to 0, #4281 — `covers` holds its walk).
    if at == 0 || !matches!(fl.b.ops[at - 1], Op::I32GeU | Op::I32Eqz) {
        return Err(format!("the exit test is {:?}, not `i32.ge_u` / `i32.eqz`", at.checked_sub(1).map(|k| &fl.b.ops[k])));
    }
    let lp = fl.parent[at].ok_or("the exit test is in no loop")?;
    if !matches!(fl.b.ops[lp], Op::Loop { .. }) {
        return Err(format!("the exit test sits in {:?}, not a loop", fl.b.ops[lp]));
    }
    let bk = fl.parent[lp].ok_or("the loop is in no block")?;
    if !matches!(fl.b.ops[bk], Op::Block { .. }) {
        return Err(format!("the loop sits in {:?}, not the block the exit leaves to", fl.b.ops[bk]));
    }
    Ok(())
}

/// An access: its instruction (for an `a` anchored at the release call,
/// the load before it), its address term, memarg offset and width.
struct Access {
    at: usize,
    addr: T,
    off: u64,
    width: u64,
}

/// The access instruction (`acc`; for an `a` anchored at the release call,
/// the load before it): its address term, memarg offset and width.
fn access_parts(fl: &Flow<'_, '_>, acc: usize) -> Result<Access, String> {
    let op = &fl.b.ops[acc];
    let (off, width) = if let Some((w, off)) = store_width(op) {
        (off, u64::from(w))
    } else if is_load(op) {
        let (_, off, w) = load_name(op);
        (off, w)
    } else {
        return Err(format!("{op:?} is no access"));
    };
    Ok(Access { at: acc, addr: fl.arg(acc, 0, FUEL), off, width })
}

/// Check that the guard whose compare is at `cmp` covers the access at
/// `acc`, both in `b` (module header).
pub(crate) fn covers(fl: &Flow<'_, '_>, cmp: usize, acc: usize) -> Result<(), String> {
    let acc = if matches!(fl.b.ops[acc], Op::Call { .. }) && acc > 0 { acc - 1 } else { acc };
    if !fl.dominates(cmp, acc) {
        return Err("the check does not dominate the access".into());
    }
    // The surface's own test (#4281): its compare opens an `if` with an
    // `else`, and the access lies in that `else` — the compare's TRUE side
    // (the then-arm) never reaches it.
    if let Some(Op::If { .. }) = fl.b.ops.get(cmp + 1) {
        if fl.has_else(cmp + 1) && !fl.in_else(cmp + 1, acc) {
            return Err("the access is not in the `else` of its test's `if`".into());
        }
    }
    let a = access_parts(fl, acc)?;
    match fl.b.ops[cmp] {
        Op::I64GeU => list_index(fl, cmp, &a),
        Op::I32GeU => exit_test(fl, cmp, &a),
        // A cursor walking down (#4281): `p eqz; br_if 1` in a loop.
        Op::I32Eqz if matches!(fl.b.ops.get(cmp + 1), Some(Op::BrIf { .. })) => {
            fl.in_loop_after(cmp + 1, acc)?;
            fl.walk(cmp, acc, &a.addr, a.off, a.width)
        }
        Op::I32Eqz => room_test(fl, cmp, &a),
        // A fresh block's own size (#4281, lib.rs `check_guard`): `i32.const
        // N; call $alloc`, the access `B + c` with B that call's value and
        // its bytes inside the N the call allocated (a live block's size
        // never changes; its liveness is the ownership witness's).
        Op::Call { .. } => fresh_block(fl, cmp, &a),
        ref o => Err(format!("the guard's compare is {o:?}")),
    }
}

/// The list compare `i64.ge_u` at `cmp` (module header, LIST).
fn list_index(fl: &Flow<'_, '_>, cmp: usize, a: &Access) -> Result<(), String> {
    let (addr, off, width, acc) = (&a.addr, a.off, a.width, a.at);
    let (ix, cnt) = (fl.arg(cmp, 0, FUEL), fl.arg(cmp, 1, FUEL));
    let ((cb, count_at), s) = count_of(&cnt).ok_or_else(|| format!("the compare's right operand is not a list's count: {cnt:?}"))?;
    let Some([base, scaled]) = op(addr, "i32.add") else {
        return Err(format!("the access's address is not block + scaled index: {addr:?}"));
    };
    let (ai, s2) = match op(scaled, "i32.mul") {
        Some([w, c]) => match (op(w, "i32.wrap_i64"), cst(c)) {
            (Some([i]), Some(s2)) => (i, s2),
            _ => return Err(format!("the access's index is not scaled by a constant: {scaled:?}")),
        },
        _ => return Err(format!("the access's index is not scaled: {scaled:?}")),
    };
    let (ab, total) = match op(base, "i32.add") {
        Some([b, c]) if cst(c).is_some() => (b, off as i64 + cst(c).unwrap_or(0)),
        _ => (base, off as i64),
    };
    if s <= 0 || s2 != s {
        return Err(format!("the check counts slots of {s} bytes, the access scales by {s2}"));
    }
    if width as i64 > s {
        return Err(format!("the access is {width} bytes wide, its slots {s}"));
    }
    let k = total - PAYLOAD;
    if k < 0 || k % s != 0 {
        return Err(format!("the access lies at payload offset {k}, not a whole slot"));
    }
    let ((ci, cc), (ai, ac)) = (affine(&ix), affine(ai));
    if !fl.same(ci, ai) || cc != ac + k / s {
        return Err(format!("the check tests index {ix:?}, the access reads slot {ai:?} + {}", k / s));
    }
    fl.same_count(cb, ab, count_at, acc)
}

/// The i32 compare `i32.ge_u` at `cmp`: a loop's exit test (#4280), whose
/// `br_if` must dominate too, or one opening an `if` — an aborting guard,
/// or the surface's own test with the access in its `else` (#4281).
fn exit_test(fl: &Flow<'_, '_>, cmp: usize, a: &Access) -> Result<(), String> {
    let (addr, off, width, acc) = (&a.addr, a.off, a.width, a.at);
    if matches!(fl.b.ops.get(cmp + 1), Some(Op::BrIf { .. })) {
        fl.in_loop_after(cmp + 1, acc)?;
    }
    let (ix, cnt) = (fl.arg(cmp, 0, FUEL), fl.arg(cmp, 1, FUEL));
    // The count: `len(B) / S`, `len(B)` (a string's bytes), or a
    // value an enclosing test bounds by a string's `len`.
    let counted = count_of32(&cnt);
    let s = counted.as_ref().map_or(1, |&(_, s)| s);
    if s != 1 {
        return list_slot(fl, &ix, counted, a);
    }
    if width != 1 || off as i64 != PAYLOAD {
        return Err(format!("the exit test counts slots of 1 bytes, the access is {width} bytes at offset {off}"));
    }
    // `i < cnt <= len(B)`: `cnt` is `len(B)`, or bounded by it.
    let below = |b: &T| -> Result<(), String> {
        let direct = match &counted {
            Some(((cb, count_at), _)) => fl.same_count(cb, b, *count_at, acc),
            None => Err(format!("the exit test's right operand is not a list's count: {cnt:?}")),
        };
        direct.or_else(|e| fl.len_bounds(&cnt, b, acc).map_err(|f| format!("{e}; {f}")))
    };
    let (adds, c) = split(addr);
    let zero = T::Const(0);
    match (adds.len(), c) {
        // byte 0: `B`, the test `0 >=u cnt`
        (1, 0) if fl.same(&ix, &zero) => below(adds[0]),
        // `B + i`, or `B + i - 1` with `i >= 1` (#4281: lines' CR)
        (2, 0 | -1) => indexed_byte(fl, &ix, (&adds, c), acc, &below),
        // A suffix (#4281): `B + (len(B) - n) + k`, the exit test
        // `k >=u n` failed and `n <= len(B)` by an enclosing test, so
        // `len(B) - n <= len(B) - n + k < len(B)`.
        (3, 0) => suffix_byte(fl, &ix, &cnt, &adds, acc),
        _ => Err(format!("the byte access's address is not a string + its index: {addr:?}")),
    }
}

/// `B + i` (`c` 0), or `B + i - 1` (`c` -1) with `i >= 1`: `i` the tested
/// index, `B` below the count.
fn indexed_byte(fl: &Flow<'_, '_>, ix: &T, (adds, c): (&[&T], i64), acc: usize, below: &dyn Fn(&T) -> Result<(), String>) -> Result<(), String> {
    let mut last = format!("the exit test tests {ix:?}, the access reads {adds:?}");
    for (b, i) in [(adds[0], adds[1]), (adds[1], adds[0])] {
        if !fl.same(ix, i) {
            continue;
        }
        if c == -1 {
            if let Err(e) = fl.positive(i, acc) {
                last = e;
                continue;
            }
        }
        match below(b) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// `B + (len(B) - n) + k`, in any order: `k` the tested index, `n` the count.
fn suffix_byte(fl: &Flow<'_, '_>, ix: &T, cnt: &T, adds: &[&T], acc: usize) -> Result<(), String> {
    let mut last = format!("the access {adds:?} is no suffix read");
    for (x, y, z) in [(0, 1, 2), (0, 2, 1), (1, 0, 2), (1, 2, 0), (2, 0, 1), (2, 1, 0)] {
        let (b, d, k) = (adds[x], adds[y], adds[z]);
        let Some([l, n]) = op(d, "i32.sub") else { continue };
        let Some((b2, len_at)) = len(l) else { continue };
        if !fl.same(ix, k) || !fl.same(n, cnt) {
            continue;
        }
        let r = fl.len_bounds(cnt, b, acc).and_then(|()| fl.same_count(b2, b, len_at, acc));
        match r {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// The room test `i32.eqz` at `cmp` (module header, BYTES), or the empty
/// test `len(B) == 0`.
fn room_test(fl: &Flow<'_, '_>, cmp: usize, a: &Access) -> Result<(), String> {
    let (addr, off, width, acc) = (&a.addr, a.off, a.width, a.at);
    let room = fl.arg(cmp, 0, FUEL);
    if let Some((cb, count_at)) = len(&room) {
        return empty_test(fl, cmp, a, (cb, count_at));
    }
    let (i1, i2, (cb, count_at), w) = room_of(&room).ok_or_else(|| format!("the room test is not `0 <= i <= len - w`: {room:?}"))?;
    if !fl.same(i1, i2) {
        return Err("the room test's two bounds test different indices".into());
    }
    let Some([ab, wi]) = op(addr, "i32.add") else {
        return Err(format!("the byte access is not block + index: {addr:?}"));
    };
    let Some(ai) = op(wi, "i32.wrap_i64") else {
        return Err(format!("the byte access's index is {wi:?}"));
    };
    let k = off as i64 - PAYLOAD;
    if k < 0 || k + width as i64 > w {
        return Err(format!("the byte access covers [{k}, {}), the room test {w} bytes", k + width as i64));
    }
    if ai.len() != 1 || !fl.same(i1, &ai[0]) {
        return Err("the room test and the byte access use different indices".into());
    }
    fl.same_count(cb, ab, count_at, acc)
}

/// `len(B) == 0` (#4281), the access byte 0 of B in the test's `else`:
/// there `len(B) >= 1`.
fn empty_test(fl: &Flow<'_, '_>, cmp: usize, a: &Access, (cb, count_at): (&T, usize)) -> Result<(), String> {
    let (addr, off, width, acc) = (&a.addr, a.off, a.width, a.at);
    if !(fl.b.ops.get(cmp + 1).is_some_and(|o| matches!(o, Op::If { .. })) && fl.has_else(cmp + 1)) {
        return Err("the empty test opens no `if` with an `else`".into());
    }
    if width != 1 || off as i64 != PAYLOAD {
        return Err(format!("the access under an empty test is {width} bytes at offset {off}, not byte 0"));
    }
    if op(addr, "i32.add").is_some() {
        return Err(format!("the access under an empty test is not byte 0: {addr:?}"));
    }
    fl.same_count(cb, addr, count_at, acc)
}

/// The fresh block allocated at `cmp`: the access reads inside its bytes.
fn fresh_block(fl: &Flow<'_, '_>, cmp: usize, a: &Access) -> Result<(), String> {
    let (addr, off, width) = (&a.addr, a.off, a.width);
    let n = cst(&fl.arg(cmp, 0, FUEL)).ok_or("the allocation's size is not a constant")?;
    let (adds, c) = split(addr);
    let [b] = adds.as_slice() else {
        return Err(format!("the access {addr:?} is not the fresh block + a constant"));
    };
    if site_id(b) != Some(cmp) || !matches!(pl(b), T::Call { .. }) {
        return Err(format!("the access addresses {b:?}, not the block allocated at {cmp}"));
    }
    let k = off as i64 + c - PAYLOAD;
    if k < 0 || k + width as i64 > n {
        return Err(format!("the access covers payload [{k}, {}), the allocation {n} bytes", k + width as i64));
    }
    Ok(())
}

/// A list slot of `s > 1` bytes under an i32 exit test (#4280): the load at
/// `B + i * s` at the payload, `i` the tested cursor, `B` the counted list.
fn list_slot(fl: &Flow<'_, '_>, ix: &T, counted: Option<((&T, usize), i64)>, a: &Access) -> Result<(), String> {
    let (addr, off, width, acc) = (&a.addr, a.off, a.width, a.at);
    let ((cb, count_at), s) = counted.ok_or("the exit test's right operand is not a list's count")?;
    let Some([ab, scaled]) = op(addr, "i32.add") else {
        return Err(format!("the access's address is not block + scaled counter: {addr:?}"));
    };
    let Some([i, c]) = op(scaled, "i32.mul") else {
        return Err(format!("the access's counter is not scaled: {scaled:?}"));
    };
    if cst(c) != Some(s) {
        return Err(format!("the exit test counts slots of {s} bytes, the access scales by {c:?}"));
    }
    if width as i64 > s {
        return Err(format!("the access is {width} bytes wide, its slots {s}"));
    }
    if off as i64 != PAYLOAD {
        return Err(format!("the access lies at offset {off}, not the payload's first slot"));
    }
    if !fl.same(ix, i) {
        return Err(format!("the exit test tests {ix:?}, the access reads slot {i:?}"));
    }
    fl.same_count(cb, ab, count_at, acc)
}

/// `t` as a sum: its addends (through `i32.add`, and `i32.sub` of a
/// literal) and the literal constant they add up with.
pub(super) fn split(t: &T) -> (Vec<&T>, i64) {
    fn go<'t>(t: &'t T, v: &mut Vec<&'t T>, c: &mut i64) {
        // A literal is an offset; a constant read through a variable is a
        // block's address (a static string's), an addend of its own.
        if let T::Const(k) = t {
            *c += k;
        } else if let Some([a, b]) = op(t, "i32.add") {
            go(a, v, c);
            go(b, v, c);
        } else if let Some([a, T::Const(k)]) = op(t, "i32.sub") {
            *c -= *k;
            go(a, v, c);
        } else {
            v.push(t);
        }
    }
    let (mut v, mut c) = (Vec::new(), 0);
    go(t, &mut v, &mut c);
    (v, c)
}

/// [`split`] of an owned term, the addends cloned.
pub(super) fn split_owned(t: T) -> (Vec<T>, i64) {
    let (v, c) = split(&t);
    (v.into_iter().cloned().collect(), c)
}
