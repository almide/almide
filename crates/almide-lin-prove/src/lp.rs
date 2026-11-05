//! Farkas multipliers by exact rational simplex.
//!
//! Facts `a_j·x + b_j <= 0`, a target `c·x + d <= 0`: find λ >= 0 with
//! `Σ λ_j a_j = c` and `Σ λ_j b_j >= d`, the certificate RoutineLin's
//! `entails` checks (scaled to integers). Untrusted: a wrong answer is a
//! certificate the checker refuses.

use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Q {
    n: i128,
    d: i128,
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Q {
    pub fn int(n: i128) -> Q {
        Q { n, d: 1 }
    }
    fn new(n: i128, d: i128) -> Option<Q> {
        if d == 0 {
            return None;
        }
        let g = gcd(n, d).max(1);
        let s = if d < 0 { -1 } else { 1 };
        Some(Q { n: (n / g).checked_mul(s)?, d: (d / g).checked_mul(s)? })
    }
    fn add(self, o: Q) -> Option<Q> {
        Q::new(self.n.checked_mul(o.d)?.checked_add(o.n.checked_mul(self.d)?)?, self.d.checked_mul(o.d)?)
    }
    fn sub(self, o: Q) -> Option<Q> {
        self.add(Q { n: o.n.checked_neg()?, d: o.d })
    }
    fn mul(self, o: Q) -> Option<Q> {
        let g1 = gcd(self.n, o.d).max(1);
        let g2 = gcd(o.n, self.d).max(1);
        Q::new((self.n / g1).checked_mul(o.n / g2)?, (self.d / g2).checked_mul(o.d / g1)?)
    }
    fn div(self, o: Q) -> Option<Q> {
        if o.n == 0 {
            return None;
        }
        self.mul(Q::new(o.d, o.n)?)
    }
    fn is_zero(self) -> bool {
        self.n == 0
    }
    fn pos(self) -> bool {
        self.n > 0
    }
    fn neg(self) -> bool {
        self.n < 0
    }
    fn cmpq(self, o: Q) -> Option<Ordering> {
        Some(self.n.checked_mul(o.d)?.cmp(&o.n.checked_mul(self.d)?))
    }
}

/// Facts as sparse rows over variable indices; the target likewise.
/// Returns integer multipliers (fact index, k) and the scale μ.
pub fn farkas(facts: &[(Vec<(usize, i128)>, i128)], target: &(Vec<(usize, i128)>, i128), nvars: usize) -> Option<(Vec<(usize, i128)>, i128)> {
    let n = facts.len();
    let m = nvars;
    // Columns: n λ's, then m artificials. Rows: one per variable.
    let cols = n + m;
    let mut t = tableau(facts, target, n, m)?;
    let mut basis: Vec<usize> = (n..n + m).collect();
    phase1(&mut t, &mut basis, n, cols)?;
    // Phase 2: maximize Σ λ_j b_j, artificials barred.
    let mut obj2 = vec![Q::int(0); cols];
    for (j, (_, b)) in facts.iter().enumerate() {
        obj2[j] = Q::int(*b);
    }
    let goal = Q::int(target.1);
    let ray = simplex(&mut t, &mut basis, &obj2, n, Some(goal))?;
    let lam = multipliers(&t, &basis, n, cols, ray)?;
    // The value reached.
    let mut val = Q::int(0);
    for (j, (_, b)) in facts.iter().enumerate() {
        val = val.add(lam[j].mul(Q::int(*b))?)?;
    }
    if val.cmpq(goal)? == Ordering::Less {
        return None;
    }
    integer_multipliers(&lam)
}

/// The phase-1 tableau: per variable, the facts' coefficients (the row
/// negated when its target coefficient is negative), its artificial, the
/// target coefficient as the right-hand side.
fn tableau(facts: &[(Vec<(usize, i128)>, i128)], target: &(Vec<(usize, i128)>, i128), n: usize, m: usize) -> Option<Vec<Vec<Q>>> {
    let cols = n + m;
    let mut t: Vec<Vec<Q>> = vec![vec![Q::int(0); cols + 1]; m];
    let mut rhs_c = vec![0i128; m];
    for (v, a) in &target.0 {
        rhs_c[*v] += *a;
    }
    for (j, (row, _)) in facts.iter().enumerate() {
        for (v, a) in row {
            t[*v][j] = t[*v][j].add(Q::int(*a))?;
        }
    }
    for (i, row) in t.iter_mut().enumerate() {
        let mut rhs = Q::int(rhs_c[i]);
        if rhs.neg() {
            for x in row.iter_mut().take(n) {
                *x = Q::int(0).sub(*x)?;
            }
            rhs = Q::int(0).sub(rhs)?;
        }
        row[n + i] = Q::int(1);
        row[cols] = rhs;
    }
    Some(t)
}

/// Phase 1: minimize Σ artificials (maximize -Σ). `None` when infeasible
/// (an artificial stays positive) or on overflow; then the basic
/// artificials at zero are driven out where a λ column allows.
fn phase1(t: &mut [Vec<Q>], basis: &mut [usize], n: usize, cols: usize) -> Option<()> {
    let mut obj1 = vec![Q::int(0); cols];
    for c in obj1.iter_mut().skip(n) {
        *c = Q::int(-1);
    }
    simplex(t, basis, &obj1, cols, None)?;
    if basis.iter().enumerate().any(|(i, &b)| b >= n && !t[i][cols].is_zero()) {
        return None;
    }
    for i in 0..basis.len() {
        let free = (0..n).find(|&j| !t[i][j].is_zero());
        if let (true, Some(j)) = (basis[i] >= n, free) {
            pivot(t, basis, i, j, cols)?;
        }
    }
    Some(())
}

/// The λ's at the phase-2 basis, walked along the ray when it was unbounded.
fn multipliers(t: &[Vec<Q>], basis: &[usize], n: usize, cols: usize, ray: Option<(usize, Vec<Q>, Q)>) -> Option<Vec<Q>> {
    let mut lam = vec![Q::int(0); n];
    for (i, &b) in basis.iter().enumerate() {
        if b < n {
            lam[b] = t[i][cols];
        }
    }
    let Some((e, dir, tt)) = ray else { return Some(lam) };
    // Unbounded: walk the ray far enough to reach the goal.
    lam[e] = lam[e].add(tt)?;
    for (i, &b) in basis.iter().enumerate() {
        if b < n {
            lam[b] = lam[b].sub(dir[i].mul(tt)?)?;
        }
    }
    Some(lam)
}

/// Integer multipliers: the λ's scaled by their denominators' lcm.
fn integer_multipliers(lam: &[Q]) -> Option<(Vec<(usize, i128)>, i128)> {
    let mut mu: i128 = 1;
    for l in lam.iter().filter(|l| !l.is_zero()) {
        let g = gcd(mu, l.d).max(1);
        mu = mu.checked_mul(l.d / g)?;
    }
    let mut out = Vec::new();
    for (j, l) in lam.iter().enumerate() {
        if l.neg() {
            return None;
        }
        if !l.is_zero() {
            out.push((j, l.n.checked_mul(mu / l.d)?));
        }
    }
    Some((out, mu))
}

thread_local! {
    static PIVOTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Simplex pivots taken on this thread since the last [`reset_pivots`]: the
/// prover's effort, counted in steps so its budget is the same on every
/// machine.
pub fn pivots() -> u64 {
    PIVOTS.with(std::cell::Cell::get)
}

pub fn reset_pivots() {
    PIVOTS.with(|c| c.set(0));
}

fn pivot(t: &mut [Vec<Q>], basis: &mut [usize], r: usize, c: usize, cols: usize) -> Option<()> {
    PIVOTS.with(|n| n.set(n.get() + 1));
    let p = t[r][c];
    for x in t[r].iter_mut() {
        *x = x.div(p)?;
    }
    let prow = t[r].clone();
    for (i, row) in t.iter_mut().enumerate() {
        if i == r {
            continue;
        }
        let f = row[c];
        if f.is_zero() {
            continue;
        }
        for k in 0..=cols {
            if !prow[k].is_zero() {
                row[k] = row[k].sub(f.mul(prow[k])?)?;
            }
        }
    }
    basis[r] = c;
    Some(())
}

/// The entering column (Bland's rule: the first with a positive reduced
/// cost `obj_j - Σ obj_basis * t[i][j]`) and that cost; `Some(None)` at an
/// optimum.
fn entering(t: &[Vec<Q>], basis: &[usize], obj: &[Q], allowed: usize) -> Option<Option<(usize, Q)>> {
    let cols = obj.len();
    for j in (0..allowed).filter(|j| !basis.contains(j)) {
        let mut rc = obj[j];
        for (i, &b) in basis.iter().enumerate() {
            let ob = if b < cols { obj[b] } else { Q::int(0) };
            if !ob.is_zero() && !t[i][j].is_zero() {
                rc = rc.sub(ob.mul(t[i][j])?)?;
            }
        }
        if rc.pos() {
            return Some(Some((j, rc)));
        }
    }
    Some(None)
}

/// The leaving row for entering column `e`: the least ratio, ties to the
/// smaller basic index; `Some(None)` when no row bounds it.
fn leaving(t: &[Vec<Q>], basis: &[usize], e: usize, width: usize) -> Option<Option<usize>> {
    let mut leave: Option<(usize, Q)> = None;
    for (i, row) in t.iter().enumerate().filter(|(_, row)| row[e].pos()) {
        let ratio = row[width].div(row[e])?;
        let better = match leave {
            None => true,
            Some((li, lr)) => match ratio.cmpq(lr)? {
                Ordering::Less => true,
                Ordering::Equal => basis[i] < basis[li],
                Ordering::Greater => false,
            },
        };
        if better {
            leave = Some((i, ratio));
        }
    }
    Some(leave.map(|(i, _)| i))
}

/// Unbounded along `e`: the objective grows by `rc` per unit; the step that
/// reaches `goal` (one unit past the current value when there is none).
fn unbounded(t: &[Vec<Q>], basis: &[usize], obj: &[Q], (e, rc): (usize, Q), width: usize, goal: Option<Q>) -> Option<(usize, Vec<Q>, Q)> {
    let cols = obj.len();
    let mut cur = Q::int(0);
    for (i, &b) in basis.iter().enumerate() {
        if b < cols {
            cur = cur.add(obj[b].mul(t[i][width])?)?;
        }
    }
    let gap = goal.unwrap_or(cur).sub(cur)?;
    let steps = if gap.pos() { gap.div(rc)?.add(Q::int(1))? } else { Q::int(1) };
    Some((e, t.iter().map(|row| row[e]).collect(), steps))
}

/// Maximize `obj` over the columns `< allowed` (Bland's rule). `Some(None)`
/// at an optimum; `Some(Some((e, dir, t)))` when the objective is unbounded
/// along entering column `e` (basic values fall by `dir * t`), with `t`
/// enough to reach `goal`; `None` on arithmetic overflow.
#[allow(clippy::type_complexity)]
fn simplex(t: &mut [Vec<Q>], basis: &mut [usize], obj: &[Q], allowed: usize, goal: Option<Q>) -> Option<Option<(usize, Vec<Q>, Q)>> {
    let width = t.first().map_or(obj.len() + 1, Vec::len) - 1;
    for _ in 0..10_000 {
        let Some((e, rc)) = entering(t, basis, obj, allowed)? else { return Some(None) };
        match leaving(t, basis, e, width)? {
            Some(r) => pivot(t, basis, r, e, width)?,
            None => return unbounded(t, basis, obj, (e, rc), width, goal).map(Some),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_entailments() {
        // x - 3 <= 0, y - x <= 0 ⊢ y - 5 <= 0.
        let f = vec![(vec![(0, 1)], -3), (vec![(1, 1), (0, -1)], 0)];
        let (ps, mu) = farkas(&f, &(vec![(1, 1)], -5), 2).expect("entailed");
        assert_eq!(mu, 1);
        assert_eq!(ps, vec![(0, 1), (1, 1)]);
        // ... but not y - 2 <= 0.
        assert!(farkas(&f, &(vec![(1, 1)], -2), 2).is_none());
        // 2x - 2 <= 0 ⊢ x - 1 <= 0 needs λ = 1/2: scaled, μ = 2.
        let g = vec![(vec![(0, 2)], -2)];
        let (ps, mu) = farkas(&g, &(vec![(0, 1)], -1), 1).expect("entailed");
        assert_eq!((ps, mu), (vec![(0, 1)], 2));
        // Rationally, 2x - 1 <= 0 does not give x <= 0.
        assert!(farkas(&[(vec![(0, 2)], -1)], &(vec![(0, 1)], 0), 1).is_none());
        // Contradictory facts entail anything: x <= 0, -x + 1 <= 0.
        let h = vec![(vec![(0, 1)], 0), (vec![(0, -1)], 1)];
        assert!(farkas(&h, &(vec![], 7), 1).is_some());
    }
}
