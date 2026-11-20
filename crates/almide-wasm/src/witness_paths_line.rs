//! One object's paths as one certificate line (witness_paths.rs, split
//! for the file budget).

use super::*;

/// A path as the line shows it. #2755: a path that ABORTED still holding
/// credits ends in the abort terminal `t`; one that aborted balanced is
/// shown as the ordinary path it is (checked to 0 like any other).
fn shown(p: &Path) -> String {
    if p.aborted && net(&p.events) != 0 {
        format!("{}t", p.events)
    } else {
        p.events.clone()
    }
}

/// Two or fewer distinct paths as one certificate line.
///
/// #2755: an ABORTING path whose events are a prefix of a non-aborting path
/// of the same line needs no arm of its own: the checker runs that longer
/// path fault-free, and a fault in the prefix would be a fault in it (`exec`
/// is a left fold; `check_line_prefix_safe`), so the aborting run is safe up
/// to the abort — all an abort owes; what it still holds is discharged. Only
/// an aborting path no other path covers is shown, with its terminal (`t`).
///
/// A path that carries folded exit items has them hoisted to the line start
/// ([`hoist`]), so the whole-line branch between the flat remainders stays
/// flat (v5 arms do not nest).
pub(super) fn line(paths: &[Path], exits: Exits) -> Result<String, String> {
    let returning: Vec<String> = paths.iter().filter(|p| !p.aborted || net(&p.events) == 0).map(shown).collect();
    let covered = |p: &Path| p.aborted && net(&p.events) != 0 && returning.iter().any(|q| q.starts_with(&p.events));
    let set: BTreeSet<String> = paths.iter().filter(|p| !covered(p)).map(shown).collect();
    // One returning path carries its folded items in place, as before.
    if let [one] = set.iter().collect::<Vec<_>>().as_slice()
        && !one.ends_with('t')
    {
        return Ok((*one).clone());
    }
    let mut hoisted: BTreeSet<String> = BTreeSet::new();
    let mut flats: BTreeSet<String> = BTreeSet::new();
    for s in &set {
        let (h, f) = hoist(s);
        hoisted.extend(h);
        flats.insert(f);
    }
    let head: String = hoisted.into_iter().collect();
    let v: Vec<&str> = flats.iter().map(String::as_str).collect();
    let body = match v.as_slice() {
        // A lone aborting path is the abort arm of a branch whose other arm
        // (a path that never runs) claims nothing: `{<p>t|}`.
        [one] if one.ends_with('t') => format!("{{{one}|}}"),
        [one] => (*one).to_string(),
        // One terminal arm per branch: two paths, at most one aborting.
        [a, b] if !(a.ends_with('t') && b.ends_with('t')) => format!("{{{a}|{b}}}"),
        // Any other set of whole paths: every path but one is its own
        // terminal item from the line's start (`{<p>x|}` — a returning path,
        // checked from 0 to exactly 0; `{<p>t|}` — an aborting one, checked
        // fault-free), each with an empty survivor that moves no count; the
        // last returning path (or none) is the line's tail, checked to 0.
        // The whole-path form keeps its two-arm shape; a frame whose paths
        // it cannot carry is rendered again in the folded form first.
        more if exits == Exits::Paths => return Err(format!("branch-paths:{}", more.len())),
        more => {
            let (aborting, returning): (Vec<&str>, Vec<&str>) = more.iter().partition(|p| p.ends_with('t'));
            let (tail, rest) = returning.split_last().map_or(("", &[][..]), |(t, r)| (*t, r));
            let items: String = aborting
                .iter()
                .map(|p| format!("{{{p}|}}"))
                .chain(rest.iter().map(|p| format!("{{{p}x|}}")))
                .collect();
            format!("{items}{tail}")
        }
    };
    Ok(format!("{head}{body}"))
}
