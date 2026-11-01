//! Control-shape queries over a [`Flow`]: an `if`'s arms, the loop an exit
//! test leaves, the tests that bound a count or make an index positive, and
//! a cursor's walk down a string (#4280, #4281).

use super::*;

use super::covers::{len, split, split_owned};

impl Flow<'_, '_> {
    /// The `if` at `s` has an `else`.
    pub(crate) fn has_else(&self, s: usize) -> bool {
        let end = self.close.get(s).copied().unwrap_or(usize::MAX);
        end != usize::MAX && (s + 1..end).any(|k| self.parent[k] == Some(s) && matches!(self.b.ops[k], Op::Else))
    }

    /// `q` lies in the `else` arm of the `if` at `s`.
    pub(crate) fn in_else(&self, s: usize, q: usize) -> bool {
        let end = self.close.get(s).copied().unwrap_or(usize::MAX);
        s < q && q < end && (s + 1..q).any(|k| self.parent[k] == Some(s) && matches!(self.b.ops[k], Op::Else))
    }

    /// An enclosing test bounds the count `n` by the `len` word of the
    /// string `ab` (#4281): the access at `acc` lies in the `else` of an
    /// `if` testing `n >u len(B)` (there `n <= len(B)`) or `len(B) != n` /
    /// `n != len(B)` (there `n == len(B)`), `B` keeping its count to the
    /// access. With the exit test `i >=u n` failed, `i < n <= len(ab)`.
    pub(super) fn len_bounds(&self, n: &T, ab: &T, acc: usize) -> Result<(), String> {
        let mut c = self.parent[acc];
        let mut last = "no enclosing test bounds the count by the string's len".to_string();
        while let Some(s) = c {
            c = self.parent[s];
            if !matches!(self.b.ops[s], Op::If { .. }) || s == 0 || !self.in_else(s, acc) {
                continue;
            }
            let t = s - 1;
            let (x, y) = (self.arg(t, 0, FUEL), self.arg(t, 1, FUEL));
            let pair = match self.b.ops[t] {
                Op::I32GtU => len(&y).map(|l| (x.clone(), l)),
                Op::I32Ne => len(&y).map(|l| (x.clone(), l)).or_else(|| len(&x).map(|l| (y.clone(), l))),
                _ => None,
            };
            let Some((m, (cb, count_at))) = pair else { continue };
            if !self.same(&m, n) {
                continue;
            }
            match self.same_count(cb, ab, count_at, acc) {
                Ok(()) => return Ok(()),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}

impl Flow<'_, '_> {
    /// The exit `br_if` at `x` dominates `acc` INSIDE its loop: past the
    /// loop's `end` the exit was taken and its test held.
    pub(super) fn in_loop_after(&self, x: usize, acc: usize) -> Result<(), String> {
        if !self.dominates(x, acc) {
            return Err("the exit test's br_if does not dominate the access".into());
        }
        let lp = self.parent[x].ok_or("the exit test is in no loop")?;
        if !(lp < acc && acc < self.close[lp]) {
            return Err("the access lies outside the loop its exit test leaves".into());
        }
        Ok(())
    }

    /// `q` lies in the then-arm of the `if` at `s`.
    fn in_then(&self, s: usize, q: usize) -> bool {
        let end = self.close.get(s).copied().unwrap_or(usize::MAX);
        s < q && q < end && !(s + 1..q).any(|k| self.parent[k] == Some(s) && matches!(self.b.ops[k], Op::Else))
    }

    /// `i >= 1` at `acc` (#4281): `acc` in the then-arm of an `if` testing
    /// `i >u x` (for any x), or in the `else` of one testing `i == 0`.
    pub(super) fn positive(&self, i: &T, acc: usize) -> Result<(), String> {
        let mut c = self.parent[acc];
        while let Some(s) = c {
            c = self.parent[s];
            if !matches!(self.b.ops[s], Op::If { .. }) || s == 0 {
                continue;
            }
            let t = s - 1;
            let hit = match self.b.ops[t] {
                Op::I32GtU => self.in_then(s, acc),
                Op::I32Eqz => self.in_else(s, acc),
                _ => false,
            };
            if hit && self.same(&self.arg(t, 0, FUEL), i) {
                return Ok(());
            }
        }
        Err(format!("no enclosing test makes {i:?} positive"))
    }

    /// A cursor walking down (#4281): the exit test `p == 0; br_if 1` at
    /// `cmp` heads a loop, the access reads `B + p - d` (d = 0 or 1), and
    /// by induction over the loop `p - d < len(B)`:
    /// - on entry `p = len(B) - e` (e = 0 or 1, the one write reaching the
    ///   loop; e = 1 in the `else` of a test `len(B) == 0`), with d + e >= 1;
    /// - inside the loop every write of `p` is `p - 1` after the exit test
    ///   (so `p >= 1` there and the walk never wraps);
    /// - `B` keeps its count from each `len` read to the access.
    ///
    /// So at the loop head `p <= len(B) - e`, at the access `1 <= p`, and
    /// `0 <= p - d <= len(B) - e - d <= len(B) - 1`.
    pub(super) fn walk(&self, cmp: usize, acc: usize, addr: &T, off: u64, width: u64) -> Result<(), String> {
        if width != 1 || off as i64 != PAYLOAD {
            return Err(format!("the walk's access is {width} bytes at offset {off}, not one payload byte"));
        }
        let q = self.b.args.get(cmp).and_then(|a| a.first().copied().flatten()).ok_or("the walk's test has no operand")?;
        let Op::LocalGet { local_index: p } = self.b.ops[q] else {
            return Err(format!("the walk tests {:?}, not a local", self.b.ops[q]));
        };
        let at_exit = self.term(q, FUEL);
        let lp = self.parent[cmp + 1].ok_or("the walk's exit is in no loop")?;
        // Every write of p in the loop: `p - 1`, read after the exit test.
        for k in lp + 1..self.close[lp] {
            if !self.writes(k, Src::Local(p)) {
                continue;
            }
            let v = self.arg(k, 0, FUEL);
            let ok = matches!(op(&v, "i32.sub"), Some([j, one]) if cst(one) == Some(1) && self.same(j, &at_exit))
                && self.dominates(cmp + 1, k);
            if !ok {
                return Err(format!("instruction {k} writes the walk's cursor with {v:?}, not its own value - 1 after the exit test"));
            }
        }
        // On entry: the one write reaching the loop.
        let k0 = self.reaching_def(p, lp).ok_or("the walk's cursor has no one write reaching the loop")?;
        let (a0, c0) = split_owned(self.arg(k0, 0, FUEL));
        let [l0] = a0.as_slice() else {
            return Err(format!("the walk starts at {:?}, not a string's len", self.arg(k0, 0, FUEL)));
        };
        let (b0, len_at) = len(l0).ok_or_else(|| format!("the walk starts at {l0:?}, not a string's len"))?;
        // The access: B + p - d.
        let (adds, c) = split(addr);
        let d = -c;
        let [x, y] = adds.as_slice() else {
            return Err(format!("the walk's access {addr:?} is not a string + its cursor"));
        };
        let b = if self.same(y, &at_exit) {
            *x
        } else if self.same(x, &at_exit) {
            *y
        } else {
            return Err(format!("the walk's access {addr:?} does not read at its cursor"));
        };
        let e = -c0;
        if !(0..=1).contains(&d) || !(0..=1).contains(&e) || d + e < 1 {
            return Err(format!("the walk starts at len - {e} and reads cursor - {d}: not below len"));
        }
        if e == 1 {
            // len(B) >= 1 where the walk starts: the `else` of `len(B) == 0`.
            let mut c = self.parent[k0];
            let mut found = Err("no enclosing test makes the walk's string non-empty".to_string());
            while let Some(s) = c {
                c = self.parent[s];
                if !matches!(self.b.ops[s], Op::If { .. }) || s == 0 || !self.in_else(s, k0) || !matches!(self.b.ops[s - 1], Op::I32Eqz) {
                    continue;
                }
                let t = self.arg(s - 1, 0, FUEL);
                if let Some((b3, at3)) = len(&t) {
                    found = self.same_count(b3, b, at3, acc);
                    if found.is_ok() {
                        break;
                    }
                }
            }
            found?;
        }
        self.same_count(b0, b, len_at, acc)
    }
}
