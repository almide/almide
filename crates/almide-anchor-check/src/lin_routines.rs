//! The ROUTINE certificate's translation check (#4005 A4): the runtime
//! routines the anchor file's `routines` line names and the bodies its
//! `code` line names, each translated by lin.rs as a routine (its declared
//! heap parameters bound; the blocks a runtime routine allocates counted),
//! with the events a site selection names — loads and stores of its own
//! body. A routine's events need not be all of its
//! accesses: what the selection leaves out stays counted (the census), and
//! what it names is held to the proven checker like a self-host body's.

use std::collections::{BTreeMap, BTreeSet};

use crate::lin::{access_sites, check_linkage, matches_at, nat_len, routine_params, table_params, template, translate_routine, LinReport, Module, Tok};

/// A routine certificate's site selection (`almide-lin-prove --routines`):
/// per line `<position> <op> <op> ...`, the accesses each procedure's
/// events are. Untrusted: [`verify_routines`] translates each procedure
/// with exactly these events, and holds each to a load or store of the
/// routine's own body.
pub fn parse_selection(sel: &str) -> Result<BTreeMap<usize, BTreeSet<usize>>, String> {
    let mut out = BTreeMap::new();
    for line in sel.lines().filter(|l| !l.trim().is_empty()) {
        let mut it = line.split_whitespace();
        let pos: usize = it.next().and_then(|t| t.parse().ok()).ok_or_else(|| format!("selection: bad line `{line}`"))?;
        let ops: BTreeSet<usize> = it.map(|t| t.parse().map_err(|_| format!("selection: bad op `{t}`"))).collect::<Result<_, _>>()?;
        if out.insert(pos, ops).is_some() {
            return Err(format!("selection: position {pos} twice"));
        }
    }
    Ok(out)
}

/// The routine certificate's translation check (#4005 A4): `cert` is
/// `;`-separated `Q` stretches, each a function the anchor file's
/// `routines` line names, translated as a runtime routine (its declared
/// heap parameters bound, its allocations' cap words read as their
/// capacity) with the events `sel` selects — each a load or store of that
/// body; the linkage rule is [`crate::lin::verify`]'s. The selected accesses are the
/// located ones; every other access of a listed routine is counted.
pub fn verify_routines(wasm: &[u8], anchors: &str, cert: &str, sel: &str) -> Result<LinReport, String> {
    let m = Module::read(wasm)?;
    let (alloc, ..) = crate::parse_anchors(anchors)?;
    let heap = table_params(anchors);
    let routines = routine_params(anchors);
    let sel = parse_selection(sel)?;
    let stretches: Vec<&str> = if cert.trim().is_empty() { Vec::new() } else { cert.split(" ; ").collect() };
    let mut table = BTreeSet::new();
    for st in &stretches {
        let b = st.as_bytes();
        if b.first() != Some(&b'Q') {
            return Err(format!("routine certificate: a stretch that is not a procedure: `{}`", &st[..st.len().min(40)]));
        }
        let n = nat_len(&b[1..]).ok_or("routine certificate: a procedure with no id")?;
        let id: usize = st[1..1 + n].parse().map_err(|_| "routine certificate: bad id")?;
        if !heap.contains_key(&id) {
            return Err(format!("routine certificate: procedure {id} is not a routine or code body the anchor file names"));
        }
        if !table.insert(id) {
            return Err(format!("routine certificate: procedure {id} twice"));
        }
    }
    if let Some(p) = sel.keys().find(|p| !table.contains(p)) {
        return Err(format!("routine certificate: a selection for {p}, which has no procedure"));
    }
    let none = BTreeSet::new();
    let mut located = Vec::new();
    let mut nonempty_pre = BTreeSet::new();
    for st in &stretches {
        let id: usize = st[1..1 + nat_len(&st.as_bytes()[1..]).unwrap_or(0)].parse().unwrap_or(usize::MAX);
        let s = sel.get(&id).unwrap_or(&none);
        let all = access_sites(&m, id);
        if let Some(k) = s.iter().find(|k| !all.contains(k)) {
            return Err(format!("routine certificate: procedure {id} selects op {k}, which is no load or store of its body"));
        }
        let hp = heap.get(&id).cloned().unwrap_or_default();
        let skel = translate_routine(&m, id, s, &hp, &table, alloc, routines.contains_key(&id)).map_err(|e| format!("routine certificate: function {id}: {e}"))?;
        let tpl = template(id, &skel);
        match matches_at(&tpl, st.as_bytes(), 0) {
            Ok(n) if n == st.len() => {}
            Ok(n) => return Err(format!("routine certificate: procedure {id} is not the translation of its function (text past the template at {n})")),
            Err((k, at)) => {
                let want = match &tpl[k] {
                    Tok::Lit(l) => format!("`{}`", &l[..l.len().min(40)]),
                    Tok::Hole(h) => format!("{h:?}"),
                };
                return Err(format!(
                    "routine certificate: procedure {id} is not the translation of its function (at {at}: want {want}, read `{}`)",
                    &st[at..(at + 40).min(st.len())]
                ));
            }
        }
        located.extend(skel.sites.iter().map(|&op| (id, op)));
        let pre = &st[st.find('{').map_or(0, |i| i + 1)..];
        if !pre.trim_start().starts_with('|') {
            nonempty_pre.insert(id);
        }
    }
    check_linkage(wasm, &m, &table, &nonempty_pre)?;
    let total: usize = heap.keys().map(|&p| access_sites(&m, p).len()).sum();
    Ok(LinReport { procs: stretches.len(), sites: located.len(), counted: total - located.len(), located })
}

