//! #4279: a variant case's field read under its tag test (fields.rs).

use super::*;

/// An instruction that may change memory: a store, a call, a bulk memory or
/// table operation, an atomic. Anything this list misses is a read-only op.
pub(crate) fn may_write_memory(op: &Op<'_>) -> bool {
    if store_width(op).is_some()
        || matches!(
            op,
            Op::Call { .. }
                | Op::CallIndirect { .. }
                | Op::CallRef { .. }
                | Op::ReturnCall { .. }
                | Op::ReturnCallIndirect { .. }
                | Op::ReturnCallRef { .. }
                | Op::MemoryGrow { .. }
                | Op::MemoryCopy { .. }
                | Op::MemoryFill { .. }
                | Op::MemoryInit { .. }
        )
    {
        return true;
    }
    let name = format!("{op:?}");
    name.contains("Atomic") || name.contains("Store") || name.starts_with("Table") || name.starts_with("Memory")
}

/// The functions whose call writes no memory: a defined function none of
/// whose instructions may write memory except calls of such functions (the
/// greatest fixpoint, so read-only recursion stays read-only). An imported
/// function, an indirect call and anything `may_write_memory` names are
/// writes — except a call followed by `unreachable`: whatever it writes, the
/// function never returns past it (the run ends in the callee or traps).
pub(crate) fn readonly_calls(bodies: &[Body<'_>], imports: u32) -> HashSet<u32> {
    let mut ro = vec![true; bodies.len()];
    loop {
        let mut changed = false;
        for i in 0..bodies.len() {
            if !ro[i] {
                continue;
            }
            let ops = &bodies[i].ops;
            let writes = ops.iter().enumerate().any(|(k, op)| match op {
                Op::Call { .. } if matches!(ops.get(k + 1), Some(Op::Unreachable)) => false,
                Op::Call { function_index } | Op::ReturnCall { function_index } => {
                    function_index.checked_sub(imports).and_then(|d| ro.get(d as usize).copied()) != Some(true)
                }
                o => may_write_memory(o),
            });
            if writes {
                ro[i] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (0..bodies.len()).filter(|&i| ro[i]).map(|i| i as u32 + imports).collect()
}

impl Flow<'_, '_> {
    /// Calls of the functions in `ro` write no memory (`readonly_calls`).
    pub(crate) fn with_readonly(mut self, ro: HashSet<u32>) -> Self {
        self.ro_calls = ro;
        self
    }

    fn may_write(&self, op: &Op<'_>) -> bool {
        match op {
            Op::Call { function_index } if self.ro_calls.contains(function_index) => false,
            o => may_write_memory(o),
        }
    }

    /// No loop around `u` closes before `p`: a run that reaches `p` after
    /// `u` takes no back edge to before `u` in between, so what it runs
    /// between them lies in `window(u, p)`.
    fn forward(&self, u: usize, p: usize) -> bool {
        let mut c = self.parent[u];
        while let Some(s) = c {
            if matches!(self.b.ops[s], Op::Loop { .. }) && self.close[s] < p {
                return false;
            }
            c = self.parent[s];
        }
        true
    }

    /// `x`, evaluated by every run before it reaches `p`, and `y`, an operand
    /// at `p`, are the same value: read through one variable not written
    /// between the reads, the same operator over the same values, or the
    /// same field loaded from the same address with nothing that may write
    /// memory between the two loads.
    fn same_since(&self, x: &T, y: &T, fuel: u32) -> bool {
        if fuel == 0 {
            return false;
        }
        let (cx, cy) = (chain(x), chain(y));
        let kept = |s1: Src, u: usize, v: usize| u < v && self.forward(u, v) && !self.window(u, v).into_iter().any(|k| self.writes(k, s1));
        if cx.iter().any(|&(s1, u)| cy.iter().any(|&(s2, v)| s1 == s2 && kept(s1, u, v))) {
            return true;
        }
        match (pl(x), pl(y)) {
            (T::Const(a), T::Const(b)) => a == b,
            (T::Op(n, xs), T::Op(m, ys)) => n == m && xs.len() == ys.len() && xs.iter().zip(ys).all(|(a, b)| self.same_since(a, b, fuel - 1)),
            (T::Load { name: n1, off: o1, addr: a1, site: s1 }, T::Load { name: n2, off: o2, addr: a2, site: s2 }) => {
                n1 == n2
                    && o1 == o2
                    && s1 < s2
                    && self.forward(*s1, *s2)
                    && self.same_since(a1, a2, fuel - 1)
                    && !self.window(*s1, *s2).into_iter().any(|k| self.may_write(&self.b.ops[k]))
            }
            _ => false,
        }
    }

    /// The addresses whose tag word is `c` whenever the i32 value pushed by
    /// `q` is non-zero: `i32.load offset=12 (X) == c`, or an
    /// `if (result i32) A else i32.const 0 end` whose condition or `A` says so.
    fn implied_tags(&self, q: usize, c: u64, fuel: u32) -> Vec<T> {
        if fuel == 0 {
            return Vec::new();
        }
        let arg = |p: usize, k: usize| self.b.args.get(p).and_then(|a| a.get(k)).copied().flatten();
        match self.b.ops[q] {
            Op::I32Eq => {
                let mut out = Vec::new();
                for (l, k) in [(arg(q, 0), arg(q, 1)), (arg(q, 1), arg(q, 0))] {
                    if let (Some(l), Some(k)) = (l, k) {
                        let tag_load = matches!(self.b.ops[l], Op::I32Load { memarg } if memarg.offset == PAYLOAD as u64);
                        let case = matches!(self.b.ops[k], Op::I32Const { value } if i64::from(value) == c as i64);
                        if tag_load && case {
                            out.push(self.arg(l, 0, FUEL));
                        }
                    }
                }
                out
            }
            Op::End => {
                let Some(s) = (0..q).rev().find(|&s| self.close[s] == q) else { return Vec::new() };
                if !matches!(self.b.ops[s], Op::If { .. }) {
                    return Vec::new();
                }
                let Some(e) = (s + 1..q).find(|&k| self.parent[k] == Some(s) && matches!(self.b.ops[k], Op::Else)) else {
                    return Vec::new();
                };
                if !arg(q, 0).is_some_and(|v| v > e && matches!(self.b.ops[v], Op::I32Const { value: 0 })) {
                    return Vec::new();
                }
                let mut out = Vec::new();
                if let Some(cq) = arg(s, 0) {
                    out.extend(self.implied_tags(cq, c, fuel - 1));
                }
                if let Some(tv) = arg(e, 0) {
                    out.extend(self.implied_tags(tv, c, fuel - 1));
                }
                out
            }
            _ => Vec::new(),
        }
    }
}

/// The load at `p` reads a field of a variant block only where that block's
/// tag is `c`: `p` lies in the then-arm of an `if` whose condition implies
/// `tag(X) == c` (`implied_tags`), and `X` is `p`'s own address
/// (`same_since`; the tag test runs before the `if`, the `if` before `p`).
pub(crate) fn case_guarded(fl: &Flow<'_, '_>, p: usize, c: u64) -> Result<(), String> {
    let addr = fl.arg(p, 0, FUEL);
    let mut s = fl.parent[p];
    while let Some(i) = s {
        let then_arm = matches!(fl.b.ops[i], Op::If { .. })
            && !(i + 1..p).any(|k| fl.parent[k] == Some(i) && matches!(fl.b.ops[k], Op::Else));
        if then_arm {
            if let Some(q) = fl.b.args.get(i).and_then(|a| a.first()).copied().flatten() {
                if fl.implied_tags(q, c, 16).iter().any(|x| fl.same_since(x, &addr, FUEL)) {
                    return Ok(());
                }
            }
        }
        s = fl.parent[i];
    }
    Err(format!("no enclosing test of tag {c} on its address {addr:?}"))
}
