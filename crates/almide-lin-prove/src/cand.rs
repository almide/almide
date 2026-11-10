//! Loop-invariant candidates from the ENTRY VALUES of the slots a loop
//! writes (#4005 A4) — guesses only: Houdini (`Sim::invariant`) drops every
//! one a back edge does not re-establish, and the checker proves the rest.
//!
//! A pointer walk enters its loop with `src = base + 12` and
//! `end = src + len`, and steps `src`. The entry facts on `src` do not
//! survive the step, and with them goes everything stated through it
//! (`end`'s bound). What does survive is one-sided: `src >= base + 12`,
//! `dst - src` unchanged, and `end = base + 12 + len` once written without
//! `src`. So, for the slots the body writes whose entry value the facts
//! give as an equality over slots it does not write:
//!
//! - `x <= e` and `x >= e`, each on its own;
//! - `(x - e) - (y - f)` both ways, for two such slots (they step together);
//! - every entry fact naming a written slot, with the entry values put in.

use super::*;

/// The most candidates one loop takes from here.
const MOST: usize = 32;

fn canon(l: &L) -> L {
    L { t: coefs(l).into_iter().collect(), c: l.c }
}

/// Append the entry-value candidates the entry facts `f` prove to `out`.
pub(crate) fn entry_candidates(f: &[L], body: &[It], out: &mut Vec<L>) {
    let (mut w, mut r) = (BTreeSet::new(), BTreeSet::new());
    writes_rebinds(body, &mut w, &mut r);
    let fc: Vec<L> = f.iter().map(canon).collect();
    // x = e: a fact `x - e <= 0` whose negation is a fact too.
    let mut entry: BTreeMap<Slot, L> = BTreeMap::new();
    for x in &w {
        let vx = V::S(*x);
        let eq = fc.iter().find(|g| coef(&g.t, &vx) == 1 && fc.contains(&canon(&lneg(g))));
        if let Some(g) = eq {
            entry.insert(*x, lneg(&L { t: dropv(&vx, &g.t), c: g.c }));
        }
    }
    let written = |l: &L| w.iter().any(|x| mentions(&V::S(*x), l));
    let resolve = |l: &L| -> Option<L> {
        let mut l = canon(l);
        for _ in 0..4 {
            if !written(&l) {
                break;
            }
            for (x, e) in &entry {
                if mentions(&V::S(*x), &l) {
                    l = canon(&subst(&V::S(*x), e, &l));
                }
            }
        }
        (!written(&l)).then_some(l)
    };
    let vals: Vec<(Slot, L)> = entry.iter().filter_map(|(x, e)| resolve(e).map(|e| (*x, e))).collect();
    let mut guess: Vec<L> = Vec::new();
    for (x, e) in &vals {
        let d = lsub(&v1(V::S(*x)), e);
        guess.push(lneg(&d));
        guess.push(d);
    }
    for (i, (x, e)) in vals.iter().enumerate() {
        for (y, g) in &vals[i + 1..] {
            let d = lsub(&lsub(&v1(V::S(*x)), e), &lsub(&v1(V::S(*y)), g));
            guess.push(lneg(&d));
            guess.push(d);
        }
    }
    for g in f {
        if written(g) {
            guess.extend(resolve(g));
        }
    }
    let mut kept = 0;
    for c in guess {
        let c = canon(&c);
        if kept >= MOST || c.t.is_empty() || fc.contains(&c) || out.contains(&c) {
            continue;
        }
        if prove(f, &c).is_some() {
            out.push(c);
            kept += 1;
        }
    }
}
