//! The JOIN of a branch's surviving paths ([`super::Exits::Join`], split from
//! witness_paths.rs for the file budget).
//!
//! Enumerating whole paths doubles the path set at every branch whose arms
//! record different events for the object: a frame of n such branches in a
//! row has 2^n paths, and the walk declines past [`super::PATH_CAP`]. But two
//! arms whose paths reach the branch's join in the SAME state — the same
//! holders, the same birth, the same first event, neither ended — have the
//! same future: every event after the join is recorded the same way on both.
//! They differ only in the arm's own events `a` and `b`, so the line writes
//! the branch once, as the one-shot branch item `{a|b}`, and continues from
//! the join as one path.
//!
//! The checker reads `{a|b}` (`CBranch`, proofs/OwnershipChecker.v `check_xc`,
//! almide-verify ownership.rs `exec_line`) as: run each arm from the count at
//! the branch, both fault-free, both ending at the SAME count, and continue
//! from it. Joined only when `net(a) == net(b)`, the line `p{a|b}s` is
//! accepted exactly when both `pas` and `pbs` are (the fold is a
//! deterministic left fold, and the arms leave one count): the enumeration's
//! verdict, written in one path. Arms that do not agree, or that differ in
//! anything but their events, stay separate paths, as before — an
//! unbalanced arm is never hidden in a join, it is refused by the checker or
//! left to the cap.
//!
//! Arms are flat (v5): an arm's own folded exit items are hoisted out first
//! (`hoist`, the same rewrite the line form does: a `{<e>x|}` item after the
//! arm's ops `q` checks `q e` from the count at the branch, and moves no
//! count); an arm that still carries a structure (an inner join) is not
//! joined again. The empty arm is written first, so a join item never reads
//! as a folded exit `{<e><m>|}` (witness_lines.rs `fold_arm`).

use super::*;

/// Two survivors with the same future: everything but their events agrees.
fn same_state(p: &Path, q: &Path) -> bool {
    p.born == q.born && p.holders == q.holders && p.first == q.first && p.aborted == q.aborted && p.exited == q.exited && p.ended == q.ended
}

/// What an arm of a `{a|b}` item can carry: ops and branches of its own
/// (certificate format v10 nests them), but no exit or abort marker (exit
/// and abort arms are flat), no loop, and no probe (a probe's drain rule
/// reads only the line's top level, ADR-0034 D2).
fn joinable(ops: &str) -> bool {
    !ops.contains(['(', ')', '[', ']', 'u', 'x', 't'])
}

/// The survivors of one branch (each holding its arm's events from the
/// branch on), with each group in the same state joined into one path when
/// its arms agree.
pub(super) fn join(survivors: Vec<Path>) -> Vec<Path> {
    let mut groups: Vec<Vec<Path>> = Vec::new();
    for r in survivors {
        match groups.iter_mut().find(|g| same_state(&g[0], &r)) {
            Some(g) => g.push(r),
            None => groups.push(vec![r]),
        }
    }
    groups.into_iter().flat_map(join_group).collect()
}

fn join_group(group: Vec<Path>) -> Vec<Path> {
    if group.len() < 2 {
        return group;
    }
    let mut hoisted: BTreeSet<String> = BTreeSet::new();
    let mut flats: BTreeSet<String> = BTreeSet::new();
    for r in &group {
        let (h, f) = hoist(&r.events);
        hoisted.extend(h);
        flats.insert(f);
    }
    let arms: Vec<&String> = flats.iter().collect();
    let item = match arms.as_slice() {
        [one] => (*one).clone(),
        // BTreeSet order puts an empty arm first.
        [a, b] if joinable(a) && joinable(b) && net(a) == net(b) => format!("{{{a}|{b}}}"),
        _ => return group,
    };
    let head: String = hoisted.into_iter().collect();
    let mut joined = group.into_iter().next().unwrap_or_default();
    joined.events = format!("{head}{item}");
    vec![joined]
}

/// Paths that reach this point of the walk in the same state, merged
/// (certificate format v10): two paths `C u` and `C v` that agree on every
/// holder, the birth and the first event have the same future, so they are
/// written once as `C{u|v}` — the checker runs both `u` and `v` from the
/// count `C` leaves and requires them to agree (`CNest`, OwnershipChecker.v
/// `nexec`), which is exactly both whole paths' check. Merged only when
/// their nets agree (else the item would be refused — the paths stay
/// apart, and the cap decides), and neither carries a probe or an
/// exit/abort marker (those arms must stay flat). This is what keeps a
/// frame whose selects leave two holder states that reconverge only later
/// at one path per state instead of one per history.
pub(super) fn merge(paths: Vec<Path>) -> Vec<Path> {
    let mut groups: Vec<Vec<Path>> = Vec::new();
    for p in paths {
        let mergeable = !p.ended && joinable(&p.events);
        match groups.iter_mut().find(|g| mergeable && !g[0].ended && joinable(&g[0].events) && same_state(&g[0], &p)) {
            Some(g) => g.push(p),
            None => groups.push(vec![p]),
        }
    }
    groups.into_iter().flat_map(merge_group).collect()
}

fn merge_group(group: Vec<Path>) -> Vec<Path> {
    let mut out: Vec<Path> = Vec::new();
    'paths: for p in group {
        for q in out.iter_mut() {
            if let Some(events) = merge2(&q.events, &p.events) {
                q.events = events;
                continue 'paths;
            }
        }
        out.push(p);
    }
    out
}

/// `C u` and `C v` as `C{u|v}` (the shorter-sorting arm first), when their
/// nets agree.
fn merge2(a: &str, b: &str) -> Option<String> {
    if a == b {
        return Some(a.to_string());
    }
    if net(a) != net(b) {
        return None;
    }
    let (ia, ib) = (items(a), items(b));
    let k = ia.iter().zip(&ib).take_while(|(x, y)| x == y).count();
    let common: String = ia[..k].concat();
    let (u, v) = (ia[k..].concat(), ib[k..].concat());
    let (lo, hi) = if u <= v { (u, v) } else { (v, u) };
    Some(format!("{common}{{{lo}|{hi}}}"))
}

/// An exit or abort item (an arm carrying an `x` / `t` marker) is flat:
/// the checker poisons one that nests a branch, so a line that would write
/// one declines instead.
pub(super) fn markers_flat(line: &str) -> bool {
    items(line).into_iter().all(|it| !(it.starts_with('{') && it.contains(['x', 't'])) || it[1..].find('{').is_none())
}
