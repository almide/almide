//! The native call-binding checker — the Rust mirror of
//! `proofs/NativeCalls.v`'s `check_native_calls_cert` (#4006, class P).
//!
//! The witness is one block per function of a natively rendered program,
//! `#`-separated: `<modes>|<sites>|<marks>`, a newline, then the function's
//! ownership lines (heap parameters first), with an optional fourth header
//! field `|<hosts>`: one id per native fs host call (#4006 class E), each an
//! index into [`NATIVE_FS_HOST_CALLS`]. The program's signature table is
//! the list of every block's modes. A block is accepted iff its modes are 0
//! or 1, every site agrees with its callee block's modes, the marks are as
//! many as the arguments the sites move, each names a line of the block, and
//! every line holds exactly as many `M` bytes as marks name it, and its lines
//! balance from the references their modes declare, and every host id names a
//! function of the named set. A fifth header field `|<wargs>` (#4491) holds
//! one row per WRITABLE site (a site with a mode-2 argument, a `mut` heap
//! parameter its callee edits in place): the caller's line of each heap
//! argument. Each entry is a line of the caller's own objects that no other
//! entry of its row names, the writable argument's line is unshared, and
//! every line holds exactly as many call-site probes `B` as the rows name it.

use crate::nat::{pnats, split_bar, split_semi, Nat};

struct Block<'a> {
    modes: Vec<Nat>,
    sites: Vec<Vec<Nat>>,
    marks: Vec<Nat>,
    hosts: Vec<Nat>,
    wargs: Vec<Vec<Nat>>,
    body: &'a [u8],
}

/// `split_first is_nl`: the header before the first newline, and the rest.
fn split_nl(b: &[u8]) -> (&[u8], &[u8]) {
    match b.iter().position(|&c| c == b'\n') {
        Some(i) => (&b[..i], &b[i + 1..]),
        None => (b, &[]),
    }
}

/// `parse_block`.
fn parse_block(b: &[u8]) -> Block<'_> {
    let (header, body) = split_nl(b);
    let (modes, rest) = split_bar(header);
    let (sites, rest) = split_bar(rest);
    let (marks, rest) = split_bar(rest);
    let (hosts, wargs) = split_bar(rest);
    Block {
        modes: pnats(modes),
        sites: split_semi(sites).into_iter().map(pnats).filter(|s| !s.is_empty()).collect(),
        marks: pnats(marks),
        hosts: pnats(hosts),
        wargs: split_semi(wargs).into_iter().map(pnats).filter(|s| !s.is_empty()).collect(),
        body,
    }
}

/// `native_fs_host_calls` (NativeCalls.v): the trusted base's named set of
/// native fs host calls, in the order of `almide_mir::host_ops`'s list
/// (tests/native_fs_host_calls_test.rs holds the copies to one list).
pub const NATIVE_FS_HOST_CALLS: &[&str] = &[
    "almide_rt_fs_read_text",
    "almide_rt_fs_read_bytes",
    "almide_rt_fs_read_lines",
    "almide_rt_fs_read_text_if_exists",
    "almide_rt_fs_list_dir",
    "almide_rt_fs_exists",
    "almide_rt_fs_is_dir",
    "almide_rt_fs_is_file",
    "almide_rt_fs_is_symlink",
    "almide_rt_fs_walk",
    "almide_rt_fs_glob",
    "almide_rt_fs___root_read_text",
    "almide_rt_fs___root_read_bytes",
    "almide_rt_fs___root_list_dir",
    "almide_rt_fs___root_exists",
    "almide_rt_fs_temp_dir",
    "almide_rt_fs_write",
    "almide_rt_fs_write_bytes",
    "almide_rt_fs_mkdir_p",
    "almide_rt_fs_remove_all",
    "almide_rt_fs___root_write_text",
    "almide_rt_fs_rename",
    "almide_rt_fs_copy",
    "almide_rt_fs_append",
    "almide_rt_fs_append_bytes",
    "almide_rt_fs_remove",
    "almide_rt_fs___root_remove",
    "almide_rt_fs_create_temp_dir",
    "almide_rt_fs_create_temp_file",
];

/// `hosts_ok`: every host id names a function of the set.
fn hosts_ok(b: &Block<'_>) -> bool {
    b.hosts.iter().all(|h| h.as_index().is_some_and(|i| i < NATIVE_FS_HOST_CALLS.len()))
}

/// `site_ok` (CallModes.v).
fn site_ok(sigs: &[&[Nat]], site: &[Nat]) -> bool {
    match site.split_first() {
        Some((callee, actual)) => callee.as_index().and_then(|i| sigs.get(i)).is_some_and(|sig| *sig == actual),
        None => false,
    }
}

/// `site_moves`: the arguments a site moves (its `1` modes).
fn site_moves(site: &[Nat]) -> usize {
    site.split_first().map_or(0, |(_, actual)| actual.iter().filter(|m| m.as_index() == Some(1)).count())
}

/// `marks_ok`.
fn marks_ok(b: &Block<'_>) -> bool {
    let raw: Vec<&[u8]> = b.body.split(|&c| c == b'\n').collect();
    let moved: usize = b.sites.iter().map(|s| site_moves(s)).sum();
    let idx: Vec<Option<usize>> = b.marks.iter().map(Nat::as_index).collect();
    b.marks.len() == moved
        && idx.iter().all(|j| j.is_some_and(|j| j < raw.len()))
        && raw
            .iter()
            .enumerate()
            .all(|(j, l)| l.iter().filter(|&&c| c == b'M').count() == idx.iter().filter(|m| **m == Some(j)).count())
}

/// `site_writes`: a site with a writable (mode-2) argument.
fn site_writes(site: &[Nat]) -> bool {
    site.split_first().is_some_and(|(_, actual)| actual.iter().any(|m| m.as_index() == Some(2)))
}

/// `wargs_ok` (NativeCalls.v, #4491): one row per writable site, each entry a
/// non-parameter line of the block named once in its row, the writable
/// argument's line unshared; every line holds as many `B` as the rows name it.
fn wargs_ok(b: &Block<'_>) -> bool {
    let raw: Vec<&[u8]> = b.body.split(|&c| c == b'\n').collect();
    let unshared = crate::ownership::unshared_lines(b.body);
    let wsites: Vec<&Vec<Nat>> = b.sites.iter().filter(|s| site_writes(s)).collect();
    if wsites.len() != b.wargs.len() {
        return false;
    }
    let rows_ok = wsites.iter().zip(&b.wargs).all(|(site, row)| {
        let Some((_, actual)) = site.split_first() else { return false };
        let idx: Vec<Option<usize>> = row.iter().map(Nat::as_index).collect();
        row.len() == actual.len()
            && actual.iter().zip(&idx).all(|(m, j)| {
                j.is_some_and(|j| {
                    b.modes.len() <= j
                        && j < raw.len()
                        && idx.iter().filter(|x| **x == Some(j)).count() == 1
                        && (m.as_index() != Some(2) || unshared.get(j).copied().unwrap_or(false))
                })
            })
    });
    rows_ok
        && raw.iter().enumerate().all(|(j, l)| {
            let named: usize = b.wargs.iter().map(|row| row.iter().filter(|x| x.as_index() == Some(j)).count()).sum();
            l.iter().filter(|&&c| c == b'B').count() == named
        })
}

/// `check_native_calls_cert`.
pub(crate) fn check_native_calls_cert(witness: &[u8]) -> bool {
    let blocks: Vec<Block<'_>> = witness.split(|&c| c == b'#').map(parse_block).collect();
    let sigs: Vec<&[Nat]> = blocks.iter().map(|b| b.modes.as_slice()).collect();
    blocks.iter().all(|b| {
        b.modes.iter().all(|m| m.as_index().is_some_and(|i| i <= 2))
            && b.sites.iter().all(|s| site_ok(&sigs, s))
            && marks_ok(b)
            && hosts_ok(b)
            && crate::ownership::native_lines_ok(&b.modes, b.body)
            && wargs_ok(b)
    })
}

#[cfg(test)]
mod tests {
    use super::check_native_calls_cert as c;

    #[test]
    fn the_coq_examples_verdict_for_verdict() {
        // proofs/NativeCalls.v's examples.
        assert!(c(b"|1 1|0\niM\nid#1||\nm"));
        assert!(c(b"|1 0|\nid\nid#0||\nam"));
        assert!(!c(b"|1 1|0\nid\nid#1||\nm"));
        assert!(!c(b"|1 0|0\niM\nid#0||\nam"));
        assert!(!c(b"|1 0|\nid\nid#1||\nm"));
        assert!(!c(b"|1 1|0\niMb\nid#1||\nm"));
        assert!(!c(b"|1 1|0\niM\nid#1||\n"));
        assert!(!c(b"|1 1|0\niM\nid#1||\ndam"));
        assert!(!c(b"|5 0|\nid"));
        assert!(c(b"||| 0\nid"));
        assert!(!c(b"||| 29\nid"));
        // The writable borrow (#4491).
        assert!(c(b"|1 2|||0\niBd#2||||\n"));
        assert!(!c(b"|1 2 2|||0 0\niBBd#2 2||||\n\n"));
        assert!(!c(b"|1 2 0|||0 0\niBBd#2 0||||\n\n"));
        assert!(!c(b"|1 2 1|0||0 0\niBM#2 1||||\n\nm"));
        assert!(!c(b"|1 2|||0\niBd#2||||\nm"));
        assert!(!c(b"|1 2|||0\niaBdd#2||||\n"));
        assert!(!c(b"0|1 2|||0\n#2||||\n"));
        assert!(!c(b"|1 2|||0\nibd#2||||\n"));
        assert!(!c(b"|||\niBd"));
        assert!(!c(b"|1 2||\niBd#2||||\n"));
        assert!(!c(b"|1 3||\nid#3||\n"));
    }
}
