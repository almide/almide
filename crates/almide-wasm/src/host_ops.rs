//! The embedded-host op number space (#4338): every op an `almide.fs_call`
//! carries belongs to exactly one family, and every family's range is
//! declared HERE, once. The guest emitter, the stock-WASI transform and the
//! embedded host read their ranges from these constants, so a family exists
//! only through this table.
//!
//! [`FAMILIES`] is generated from the declarations by the macro (no second
//! list to keep in step), and the build refuses two families that share an
//! op: the const assertion below fails to compile. Two PRs that both took
//! 140..=145 (the udp family and the `process.start` child) were caught only
//! by reading the diffs; with the table, the second one does not build.
//!
//! A one-op family is a range of one. A retired op stays inside its family's
//! range, so its number is never handed to another family.

use std::ops::RangeInclusive;

macro_rules! op_families {
    ($($(#[$doc:meta])* $name:ident = $lo:literal ..= $hi:literal => $caps:expr;)*) => {
        $( $(#[$doc])* pub const $name: RangeInclusive<i32> = $lo..=$hi; )*
        /// Every family declared above, by name, in declaration order —
        /// generated from the declarations themselves.
        pub const FAMILIES: &[(&str, RangeInclusive<i32>)] = &[$((stringify!($name), $name)),*];
        /// What each family's ops reach, aligned with [`FAMILIES`]: the same
        /// declarations, so a family cannot exist without its capabilities.
        pub const FAMILY_CAPS: &[OpCaps] = &[$($caps),*];
    };
}

/// The host capabilities (`cert_project::cap` ids) an op of a family reaches:
/// the call-graph certificate's capability witness reads them (#4338 — the
/// op table and the capability table are one declaration).
#[derive(Clone, Copy)]
pub enum OpCaps {
    /// Every op of the family reaches exactly these.
    All(&'static [u32]),
    /// A family whose ops differ: the capabilities per op, `None` for a
    /// retired or unassigned op (the projector's sentinel).
    PerOp(fn(i32) -> Option<&'static [u32]>),
}

use crate::cert_project::cap::{CLOCK, ENTROPY, ENV, FS_READ, FS_WRITE, NET, PROC as CAP_PROC, STDIN, STDOUT};

/// The first block's ops (fs.rs, fs_meta.rs).
fn fs_core_caps(op: i32) -> Option<&'static [u32]> {
    Some(match op {
        1 | 4..=6 | 11..=14 | 22..=25 | 38 | 39 => &[FS_READ],
        2 | 3 | 7..=10 | 15 | 16 | 20 | 21 => &[FS_WRITE],
        19 => &[FS_READ, FS_WRITE],
        26..=29 | 33 | 37 => &[ENV],
        30 => &[STDOUT],
        32 => &[ENTROPY],
        34 | 36 => &[CLOCK],
        35 => &[STDIN],
        _ => return None,
    })
}

/// The http string client: 50 (framed raw-bytes http) is retired.
fn http_string_caps(op: i32) -> Option<&'static [u32]> {
    (op != 50).then_some(&[NET][..])
}

/// testing.assert_aborts's site pair (ADR-0029, #3792): op 117 runs a site
/// on a fresh instance — a child, the native leg's re-exec — and op 118
/// reads which site this instance was launched to run.
fn abort_site_caps(op: i32) -> Option<&'static [u32]> {
    Some(if op == 117 { &[CAP_PROC] } else { &[ENV] })
}

/// The rooted family's descriptor ops (#4084, fs_root.rs): read text, read
/// bytes, list, exists read; write text and remove write.
fn fs_root_caps(op: i32) -> Option<&'static [u32]> {
    Some(if op >= 124 { &[FS_WRITE] } else { &[FS_READ] })
}

/// The capabilities `op` reaches, from its family's declaration; `None` for
/// an op no family declares (or one its family leaves unassigned).
pub fn caps_of(op: i32) -> Option<&'static [u32]> {
    let i = FAMILIES.iter().position(|(_, r)| r.contains(&op))?;
    match FAMILY_CAPS[i] {
        OpCaps::All(c) => Some(c),
        OpCaps::PerOp(f) => f(op),
    }
}

op_families! {
    /// The first block: fs, env, args, the raw stdout, entropy, cwd, the wall
    /// clock, stdin, sleep, env.set, fs.stat / fs.glob (fs.rs, fs_meta.rs).
    /// 17 / 18 are retired inside it.
    FS_CORE = 1..=39 => OpCaps::PerOp(fs_core_caps);
    /// The fan prefetch protocol.
    FAN_PREFETCH = 40..=42 => OpCaps::All(&[FS_READ]);
    /// The http string client and the framed requests (50 retired).
    HTTP_STRING = 43..=50 => OpCaps::PerOp(http_string_caps);
    /// fs.fold_lines / fs.for_each_line.
    FS_LINES = 51..=52 => OpCaps::All(&[FS_READ]);
    /// The http call handle (#2633).
    HTTP_CALL = 53..=59 => OpCaps::All(&[NET]);
    /// The monotonic clock (also the fuel meter's deadline clock).
    CLOCK_MONO = 60..=60 => OpCaps::All(&[CLOCK]);
    /// The bounded line walks and the raw-bytes readers.
    FS_RANGE = 61..=64 => OpCaps::All(&[FS_READ]);
    /// The full-response http leaf (ADR-0023 step 4).
    HTTP_RESPONSE = 65..=65 => OpCaps::All(&[NET]);
    /// fs.append_bytes: op 16's true append under its own call name
    /// ([`FS_APPEND_BYTES_CALL`], #4382).
    FS_APPEND_BYTES = 66..=66 => OpCaps::All(&[FS_WRITE]);
    /// http.serve's bind / next / reply (#2650).
    SERVE = 70..=72 => OpCaps::All(&[NET]);
    /// The raw stderr append (#2769).
    STDERR_RAW = 73..=73 => OpCaps::All(&[STDOUT]);
    /// The instance-parallel fan offer (#3003). It reaches nothing itself: the
    /// chunk it runs is the program's own exported function, counted where the
    /// call graph reaches it.
    FAN_PAR = 74..=74 => OpCaps::All(&[]);
    /// The contained serve loop (#4203).
    SERVE_RUN = 75..=75 => OpCaps::All(&[NET]);
    /// A deliberate exit's mark (#4203).
    EXIT_MARK = 76..=76 => OpCaps::All(&[]);
    /// env.vars' environ frames (#3908).
    ENV_VARS = 77..=77 => OpCaps::All(&[ENV]);
    /// `almide:process/spawn`'s enum op, case index = op − 80 (ADR-0025).
    PROC = 80..=90 => OpCaps::All(&[CAP_PROC]);
    /// The tcp family (#3620).
    TCP = 91..=103 => OpCaps::All(&[NET]);
    /// The interim unix-socket and shm ops (#3620): inter-process channels,
    /// accounted as the network capability.
    UNIX_SHM = 104..=116 => OpCaps::All(&[NET]);
    /// `testing.assert_aborts`'s site pair (ADR-0029).
    ABORT_SITE = 117..=118 => OpCaps::PerOp(abort_site_caps);
    /// The rooted fs family by descriptor (#4084).
    FS_ROOT = 120..=125 => OpCaps::PerOp(fs_root_caps);
    /// udp and name lookup (#4306).
    UDP = 140..=145 => OpCaps::All(&[NET]);
    /// The `process.start` child (#4308): the WIT cases after `run-with`.
    PROC_CHILD = 146..=151 => OpCaps::All(&[CAP_PROC]);
}

/// The call [`FS_APPEND_BYTES`] answers for: every leg's error names it,
/// declared here beside its op rather than in a table of fs call names.
pub const FS_APPEND_BYTES_CALL: &str = "fs.append_bytes";

/// The first pair of families (indexes into [`FAMILIES`]) that share an op,
/// or that declare an empty range (it is reported as overlapping itself).
pub const fn first_overlap(f: &[(&str, RangeInclusive<i32>)]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < f.len() {
        if *f[i].1.start() > *f[i].1.end() {
            return Some((i, i));
        }
        let mut j = i + 1;
        while j < f.len() {
            if *f[i].1.start() <= *f[j].1.end() && *f[j].1.start() <= *f[i].1.end() {
                return Some((i, j));
            }
            j += 1;
        }
        i += 1;
    }
    None
}

const _: () = assert!(
    first_overlap(FAMILIES).is_none(),
    "two host op families claim the same op, or a family is empty (#4338): see host_ops::FAMILIES"
);

/// The families whose ops need `wasi:sockets`: the tcp family (#3620) and
/// udp with name lookup (#4306). WASI preview 1 has no sockets, so a build
/// whose op set names one of them ships as the WASI 0.3 component (the
/// net/tcp design step 4) — the routing reads these ranges, never a name.
pub const SOCKET_FAMILIES: &[RangeInclusive<i32>] = &[TCP, UDP];

/// Whether `ops` names an op of a [`SOCKET_FAMILIES`] family.
pub fn names_socket_op<'a>(ops: impl IntoIterator<Item = &'a i32>) -> bool {
    ops.into_iter().any(|op| SOCKET_FAMILIES.iter().any(|f| f.contains(op)))
}

/// The family `op` belongs to, if any.
pub fn family_of(op: i32) -> Option<&'static str> {
    FAMILIES.iter().find(|(_, r)| r.contains(&op)).map(|(n, _)| *n)
}

/// The `k`th op of `family`, refused at compile time when `k` falls past
/// the family's end (a family that grows a member grows its range here).
pub const fn nth(family: RangeInclusive<i32>, k: i32) -> i32 {
    let op = *family.start() + k;
    assert!(k >= 0 && op <= *family.end(), "op past its family's declared range (host_ops.rs)");
    op
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every family carries its capabilities (the macro makes FAMILY_CAPS
    /// from the same declarations), and every op of a family resolves —
    /// none falls to the projector's sentinel except an op its own family
    /// leaves unassigned (a retired number).
    #[test]
    fn every_family_declares_the_capabilities_its_ops_reach() {
        assert_eq!(FAMILY_CAPS.len(), FAMILIES.len());
        let unassigned: Vec<i32> = FAMILIES.iter().flat_map(|(_, r)| r.clone()).filter(|&op| caps_of(op).is_none()).collect();
        assert_eq!(unassigned, vec![17, 18, 31, 50], "only the retired / unassigned first-block and http numbers resolve to nothing");
        for (name, r) in FAMILIES {
            assert!(r.clone().any(|op| caps_of(op).is_some()), "{name} reaches no declared capability on any op");
        }
        // The families #4338 moved into the table reach the network.
        for op in TCP.chain(UNIX_SHM).chain(UDP) {
            assert_eq!(caps_of(op), Some(&[NET][..]), "op {op}");
        }
        assert_eq!(caps_of(119), None, "an op no family declares");
    }

    #[test]
    fn the_declared_families_are_disjoint() {
        if let Some((i, j)) = first_overlap(FAMILIES) {
            panic!("{} {:?} overlaps {} {:?}", FAMILIES[i].0, FAMILIES[i].1, FAMILIES[j].0, FAMILIES[j].1);
        }
    }

    /// The checker itself: two ranges that share an op are found, adjacent
    /// ones are not, and an empty range is refused.
    #[test]
    fn the_overlap_check_finds_a_shared_op() {
        assert_eq!(first_overlap(&[("a", 140..=145), ("b", 145..=150)]), Some((0, 1)));
        assert_eq!(first_overlap(&[("a", 140..=145), ("b", 146..=151), ("c", 1..=140)]), Some((0, 2)));
        assert_eq!(first_overlap(&[("a", 140..=145), ("b", 146..=151)]), None);
        #[allow(clippy::reversed_empty_ranges)]
        let empty = 5..=4;
        assert_eq!(first_overlap(&[("a", empty)]), Some((0, 0)));
    }
}
