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
    ($($(#[$doc:meta])* $name:ident = $lo:literal ..= $hi:literal;)*) => {
        $( $(#[$doc])* pub const $name: RangeInclusive<i32> = $lo..=$hi; )*
        /// Every family declared above, by name, in declaration order —
        /// generated from the declarations themselves.
        pub const FAMILIES: &[(&str, RangeInclusive<i32>)] = &[$((stringify!($name), $name)),*];
    };
}

op_families! {
    /// The first block: fs, env, args, the raw stdout, entropy, cwd, the wall
    /// clock, stdin, sleep, env.set, fs.stat / fs.glob (fs.rs, fs_meta.rs).
    /// 17 / 18 are retired inside it.
    FS_CORE = 1..=39;
    /// The fan prefetch protocol.
    FAN_PREFETCH = 40..=42;
    /// The http string client and the framed requests (50 retired).
    HTTP_STRING = 43..=50;
    /// fs.fold_lines / fs.for_each_line.
    FS_LINES = 51..=52;
    /// The http call handle (#2633).
    HTTP_CALL = 53..=59;
    /// The monotonic clock (also the fuel meter's deadline clock).
    CLOCK_MONO = 60..=60;
    /// The bounded line walks and the raw-bytes readers.
    FS_RANGE = 61..=64;
    /// The full-response http leaf (ADR-0023 step 4).
    HTTP_RESPONSE = 65..=65;
    /// http.serve's bind / next / reply (#2650).
    SERVE = 70..=72;
    /// The raw stderr append (#2769).
    STDERR_RAW = 73..=73;
    /// The instance-parallel fan offer (#3003).
    FAN_PAR = 74..=74;
    /// The contained serve loop (#4203).
    SERVE_RUN = 75..=75;
    /// A deliberate exit's mark (#4203).
    EXIT_MARK = 76..=76;
    /// env.vars' environ frames (#3908).
    ENV_VARS = 77..=77;
    /// `almide:process/spawn`'s enum op, case index = op − 80 (ADR-0025).
    PROC = 80..=90;
    /// The tcp family (#3620).
    TCP = 91..=103;
    /// The interim unix-socket and shm ops (#3620).
    UNIX_SHM = 104..=116;
    /// `testing.assert_aborts`'s site pair (ADR-0029).
    ABORT_SITE = 117..=118;
    /// The rooted fs family by descriptor (#4084).
    FS_ROOT = 120..=125;
    /// udp and name lookup (#4306).
    UDP = 140..=145;
    /// The `process.start` child (#4308): the WIT cases after `run-with`.
    PROC_CHILD = 146..=151;
}

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
