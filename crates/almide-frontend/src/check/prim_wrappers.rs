//! Which public stdlib function wraps each `prim` fn (#3025) — the name the
//! E085 hint gives a user who wrote `prim.read_text_file(p)`.
//!
//! Derived from the stdlib sources, never listed by hand: a table of forty
//! floor ops and their wrappers would drift the first time a wrapper was
//! renamed or a floor op gained a caller. The sources already say which
//! public function reaches which floor op; this reads them.
//!
//! The graph: every fn of every stdlib source (the self-host registry's
//! sources and each bundled module's main source), with an edge from a fn to
//! every bare fn name its body mentions — resolved in its own source first,
//! then in the single other source that declares it (a name several other
//! sources declare is ambiguous and dropped rather than guessed). A fn is
//! PUBLIC when the registry maps it to a call name (`fs_read_text` →
//! `fs.read_text`) or when it is a public, non-underscore fn of a bundled
//! module's main source (`string.len`). The wrappers of `prim.X` are the
//! public fns nearest to it walking callers outward from the fns that name
//! `prim.X` directly.
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

use crate::ast::{self, Decl, Expr, ExprKind};

/// At most this many wrappers are named — the nearest ones, alphabetically.
const SHOWN: usize = 3;

/// A floor op reached by more nearest public fns than this, none of them a
/// dedicated forwarder, is plumbing with no public counterpart.
const GENERAL: usize = 6;

/// The public functions nearest to `prim.<prim_fn>`, or an empty slice when
/// no public function reaches it.
pub(crate) fn public_wrappers(prim_fn: &str) -> &'static [String] {
    static TABLE: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    TABLE.get_or_init(derive).get(prim_fn).map(Vec::as_slice).unwrap_or(&[])
}

/// One fn of one stdlib source: its public call name (if any), the `prim`
/// fns its body names, and the bare names it mentions.
struct Node {
    public: Option<String>,
    prims: BTreeSet<String>,
    mentions: BTreeSet<String>,
}

fn stdlib_sources() -> Vec<(&'static str, HashMap<&'static str, String>)> {
    let mut by_ptr: HashMap<usize, usize> = HashMap::new();
    let mut out: Vec<(&'static str, HashMap<&'static str, String>)> = Vec::new();
    let mut slot = |src: &'static str, out: &mut Vec<(&'static str, HashMap<&'static str, String>)>| -> usize {
        *by_ptr.entry(src.as_ptr() as usize).or_insert_with(|| {
            out.push((src, HashMap::new()));
            out.len() - 1
        })
    };
    for (src, pairs) in almide_lang::self_host_registry::self_host_runtime() {
        let i = slot(src, &mut out);
        for (impl_fn, call_name) in pairs.iter() {
            out[i].1.insert(impl_fn, (*call_name).to_string());
        }
    }
    for module in almide_lang::stdlib_info::BUNDLED_MODULES.iter().copied().filter(|m| *m != "prim") {
        let Some(src) = almide_lang::stdlib_info::bundled_source(module) else { continue };
        let i = slot(src, &mut out);
        let Some(prog) = almide_lang::parse_cached(src) else { continue };
        for decl in &prog.decls {
            if let Decl::Fn { name, visibility: ast::Visibility::Public, .. } = decl {
                let n = name.as_str();
                if !n.starts_with('_') {
                    out[i].1.entry(n).or_insert_with(|| format!("{module}.{n}"));
                }
            }
        }
    }
    out
}

/// A call name a user file can write: a builtin (`eprintln`) or a
/// `module.fn` the checker has a signature for — not a registry-internal
/// carrier (`__list_append1`, `fs.fold_lines_msi`).
fn is_user_callable(call_name: &str) -> bool {
    match call_name.split_once('.') {
        None => !call_name.starts_with('_'),
        Some((module, func)) => {
            module != "prim" && !func.starts_with('_') && crate::stdlib::lookup_sig(module, func).is_some()
        }
    }
}

fn scan(body: &Expr) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut prims = BTreeSet::new();
    let mut mentions = BTreeSet::new();
    ast::visit_expr(body, &mut |e| match &e.kind {
        ExprKind::Member { object, field, .. } => {
            if let ExprKind::Ident { name, .. } = &object.kind {
                if name.as_str() == "prim" {
                    prims.insert(field.as_str().to_string());
                }
            }
        }
        ExprKind::Ident { name, .. } => {
            mentions.insert(name.as_str().to_string());
        }
        _ => {}
    });
    (prims, mentions)
}

fn derive() -> HashMap<String, Vec<String>> {
    // nodes[(source index, fn name)]
    let mut nodes: HashMap<(usize, String), Node> = HashMap::new();
    let mut declared_in: HashMap<String, Vec<usize>> = HashMap::new();
    for (si, (src, public)) in stdlib_sources().into_iter().enumerate() {
        let Some(prog) = almide_lang::parse_cached(src) else { continue };
        for decl in &prog.decls {
            let Decl::Fn { name, body: Some(body), .. } = decl else { continue };
            let (prims, mentions) = scan(body);
            let n = name.as_str().to_string();
            declared_in.entry(n.clone()).or_default().push(si);
            let public = public.get(n.as_str()).filter(|c| is_user_callable(c)).cloned();
            nodes.insert((si, n.clone()), Node { public, prims, mentions });
        }
    }
    // callers[callee] = fns whose body mentions it.
    let mut callers: HashMap<(usize, String), Vec<(usize, String)>> = HashMap::new();
    for ((si, fname), node) in &nodes {
        for m in &node.mentions {
            let target = if nodes.contains_key(&(*si, m.clone())) {
                Some(*si)
            } else {
                match declared_in.get(m).map(Vec::as_slice) {
                    Some([only]) => Some(*only),
                    _ => None,
                }
            };
            if let Some(ti) = target {
                callers.entry((ti, m.clone())).or_default().push((*si, fname.clone()));
            }
        }
    }
    let mut direct: HashMap<String, Vec<(usize, String)>> = HashMap::new();
    for (key, node) in &nodes {
        for p in &node.prims {
            direct.entry(p.clone()).or_default().push(key.clone());
        }
    }
    direct
        .into_iter()
        .map(|(prim_fn, start)| {
            let mut seen: HashSet<(usize, String)> = start.iter().cloned().collect();
            let mut frontier = start;
            // (prim fns the wrapper names directly, call name) at the nearest depth.
            let mut found: BTreeSet<(usize, String)> = BTreeSet::new();
            while !frontier.is_empty() && found.is_empty() {
                found.extend(frontier.iter().filter_map(|k| {
                    let n = &nodes[k];
                    n.public.clone().map(|p| (n.prims.len(), p))
                }));
                if !found.is_empty() {
                    break;
                }
                frontier = frontier
                    .iter()
                    .flat_map(|k| callers.get(k).into_iter().flatten().cloned())
                    .filter(|k| seen.insert(k.clone()))
                    .collect();
            }
            (prim_fn, rank(found))
        })
        .collect()
}

/// Which of the nearest public fns to name. A dedicated forwarder — a
/// public fn whose body names this one floor op and no other (`int.band`,
/// `fs.read_text`) — is the answer whenever one exists. Without one, an op
/// that many public fns reach (`prim.handle`, `prim.load32`) is the floor's
/// own plumbing with no public counterpart, and naming three arbitrary users
/// of it would send the reader to the wrong function; so it gets none. The
/// rest are named most-specific first.
fn rank(found: BTreeSet<(usize, String)>) -> Vec<String> {
    let dedicated: Vec<String> = found.iter().filter(|(n, _)| *n == 1).map(|(_, p)| p.clone()).collect();
    if !dedicated.is_empty() {
        return dedicated.into_iter().take(SHOWN).collect();
    }
    if found.len() > GENERAL {
        return Vec::new();
    }
    found.into_iter().map(|(_, p)| p).take(SHOWN).collect()
}

#[cfg(test)]
mod tests {
    use super::public_wrappers;

    /// The hint is only as good as the derivation, so pin the cases a user is
    /// most likely to reach for: a file read, the byte writer under `print`,
    /// and an op that is floor-internal.
    #[test]
    fn wrappers_are_derived_from_the_stdlib_sources() {
        assert!(
            public_wrappers("read_text_file").iter().any(|w| w == "fs.read_text"),
            "{:?}",
            public_wrappers("read_text_file")
        );
        assert!(public_wrappers("fd_write").iter().any(|w| w == "io.print"));
        assert_eq!(public_wrappers("band").first().map(String::as_str), Some("int.band"));
        assert!(public_wrappers("load32").is_empty(), "{:?}", public_wrappers("load32"));
        assert!(public_wrappers("no_such_prim_fn").is_empty());
    }
}
