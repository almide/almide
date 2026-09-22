//! The call-mode checker — the Rust mirror of `proofs/CallModes.v`'s
//! `check_modes_cert`.
//!
//! The witness is `<signatures>|<sites>`: signatures `;`-separated, one per
//! function in emitted order, each the space-separated heap-parameter modes
//! (0 = borrow, 1 = move); sites `;`-separated, each `<callee index> <actual
//! modes…>`. Accepted iff every signature is well formed and every site names
//! an in-range callee whose declared signature EQUALS the site's modes.

use crate::nat::{pnats, split_bar, split_semi, Nat};

/// `parse_modes`: signature positions are meaningful, so empty signature
/// segments are KEPT (a function with no heap parameter); an empty site
/// segment is format noise and dropped.
fn parse_modes(witness: &[u8]) -> (Vec<Vec<Nat>>, Vec<Vec<Nat>>) {
    let (sigs, sites) = split_bar(witness);
    let sigs = split_semi(sigs).into_iter().map(pnats).collect();
    let sites = split_semi(sites).into_iter().map(pnats).filter(|s| !s.is_empty()).collect();
    (sigs, sites)
}

/// `site_ok`: the callee is in range and the actual modes equal its signature.
fn site_ok(sigs: &[Vec<Nat>], site: &[Nat]) -> bool {
    match site.split_first() {
        Some((callee, actual)) => callee
            .as_index()
            .and_then(|i| sigs.get(i))
            .is_some_and(|sig| sig.as_slice() == actual),
        None => false,
    }
}

/// `check_modes_cert` = `modes_ok (parse_modes s)`: every mode is 0 or 1 and
/// every site agrees with its callee.
pub(crate) fn check_modes_cert(witness: &[u8]) -> bool {
    let (sigs, sites) = parse_modes(witness);
    sigs.iter().all(|sig| sig.iter().all(Nat::le_one)) && sites.iter().all(|site| site_ok(&sigs, site))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_right_side_is_vacuously_accepted() {
        assert!(check_modes_cert(b"0;|"));
        assert!(check_modes_cert(b""));
    }

    #[test]
    fn a_mode_above_one_makes_the_signature_ill_formed() {
        assert!(!check_modes_cert(b"2|"));
    }
}
