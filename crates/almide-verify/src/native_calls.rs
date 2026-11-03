//! The native call-binding checker — the Rust mirror of
//! `proofs/NativeCalls.v`'s `check_native_calls_cert` (#4006, class P).
//!
//! The witness is one block per function of a natively rendered program,
//! `#`-separated: `<modes>|<sites>|<marks>`, a newline, then the function's
//! ownership lines (heap parameters first). The program's signature table is
//! the list of every block's modes. A block is accepted iff its modes are 0
//! or 1, every site agrees with its callee block's modes, the marks are as
//! many as the arguments the sites move, each names a line of the block, and
//! every line holds exactly as many `M` bytes as marks name it, and its lines
//! balance from the references their modes declare.

use crate::nat::{pnats, split_bar, split_semi, Nat};

struct Block<'a> {
    modes: Vec<Nat>,
    sites: Vec<Vec<Nat>>,
    marks: Vec<Nat>,
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
    let (sites, marks) = split_bar(rest);
    Block {
        modes: pnats(modes),
        sites: split_semi(sites).into_iter().map(pnats).filter(|s| !s.is_empty()).collect(),
        marks: pnats(marks),
        body,
    }
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

/// `check_native_calls_cert`.
pub(crate) fn check_native_calls_cert(witness: &[u8]) -> bool {
    let blocks: Vec<Block<'_>> = witness.split(|&c| c == b'#').map(parse_block).collect();
    let sigs: Vec<&[Nat]> = blocks.iter().map(|b| b.modes.as_slice()).collect();
    blocks.iter().all(|b| {
        b.modes.iter().all(Nat::le_one)
            && b.sites.iter().all(|s| site_ok(&sigs, s))
            && marks_ok(b)
            && crate::ownership::native_lines_ok(&b.modes, b.body)
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
    }
}
