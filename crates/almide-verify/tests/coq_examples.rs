//! Every `Example` the Coq checkers carry, and every fixed row
//! `proofs/build-checker.sh` feeds the extracted binary, replayed through
//! this crate. The Coq side proves these by `reflexivity`, so each row here is
//! a verdict the proven checker is known to give; a mismatch is a
//! transcription error in almide-verify.
//!
//! List-level examples (`check [Inc; Dec]`, `check_line [COp Inc; …]`) are
//! written as the certificate bytes that parse to that list.

use almide_verify::{check, Property};

fn verdicts(property: Property, rows: &[(&str, bool)]) {
    for (witness, want) in rows {
        assert_eq!(
            check(property, witness.as_bytes()),
            *want,
            "{} {witness:?} should {}",
            property.name(),
            if *want { "ACCEPT" } else { "REJECT" }
        );
    }
}

#[test]
fn ownership_checker_examples() {
    verdicts(
        Property::Ownership,
        &[
            // OwnershipChecker.v — flat check / check_all / check_cert
            ("iidd", true),
            ("idd", false),
            ("iid", false),
            ("id\niidd", true),
            ("id\nidd", false),
            ("id\nid", true), // cert_two_objs
            ("iadm", true),
            ("im", true),
            ("iadd", true),
            ("m", false),
            ("ir", true),
            ("iard", false),
            ("ibd", true),
            ("idb", false),
            ("b", false),
            ("iabdd", true),
            // check_line (format v2–v4 items)
            ("i(di)m", true),
            ("i(i)m", false),
            ("i(d)m", false),
            ("i[di|]m", true),
            ("i[i|]m", false),
            ("i[di|d]m", false),
            ("i{i|i}dd", true),
            ("i{i|}d", false),
            ("{i|d}", false),
            ("{d|d}", false),
            // check_bc
            ("i{i|i}dd", true),
            // check_xc (format v5, the arm-exit marker)
            ("i{dx|}d", true),
            ("i{|dx}d", true),
            ("i{mx|}d", true),
            ("i{x|}d", false),
            ("i{xd|}d", false),
            ("i{dx|dx}", false),
            ("idx", false),
            // CallModes.v — inlined streams over the same fold
            ("iadd", true),
            ("id", true),
            ("idd", false),
        ],
    );
}

#[test]
fn build_checker_rows() {
    verdicts(
        Property::Ownership,
        &[
            ("ID\nIIDD\n", true),
            ("IIDD\nIDD\n", false),
            ("ID\nIID\n", false),
            ("IR\nIADR\n", true),
            ("R\n", false),
            ("IARD\n", false),
            ("IIDD\n", true),
            ("IDD\n", false),
            ("I(DI)M\n", true),
            ("I(I)M\n", false),
            ("I(D)M\n", false),
            ("I[ID|]M\n", true),
            ("I[I|]M\n", false),
            ("I[ID|D]M\n", false),
            ("IBD\n", true),
            ("IDB\n", false),
            ("B\n", false),
            ("I{I|I}DD\n", true),
            ("I{I|}D\n", false),
            ("{I|D}\n", false),
        ],
    );
    verdicts(Property::CapsTransitive, &[("1 2|2|1;1|1|", true), ("1 2|2|1;0|0|", false)]);
    verdicts(Property::CallModes, &[("0 1;1|1 1", true), ("0|0 1", false), ("0|5 0", false)]);
}

#[test]
fn subset_checker_examples() {
    // NameTotality.v, CapabilityBound.v, Subset.v
    for property in [Property::Names, Property::Caps] {
        verdicts(
            property,
            &[
                ("1 2 3|1 3", true),
                ("1 2|1 5", false),
                ("0 1|0", true),
                ("1 2|0", false),
                ("3 1 2|1 3", true), // cert_unsorted_fallback
                ("  7  8 |9", false),
                ("  7  8 |8 7", true),
                ("|", true),
                ("", true),
            ],
        );
    }
}

#[test]
fn capability_reach_examples() {
    verdicts(Property::CapsTransitive, &[("1 2|2|1;1|1|", true), ("1 2|2|1;0|0|", false)]);
}

#[test]
fn call_modes_examples() {
    verdicts(
        Property::CallModes,
        &[
            ("0 1;1|1 1", true),
            ("0|0 1", false),
            ("0 0|0 0", false),
            ("0|5 0", false),
            ("7|0 7", false),
            ("0 0;|", true),
        ],
    );
}
