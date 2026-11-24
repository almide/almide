//! Row `verify.gate-build` (#4836, owner ruling 2026-11-24): every wasm
//! route's certificate is judged by the linked `almide-verify` library
//! (`almide::certificate_gate::judge`, called by `gate_build` on build, run,
//! test, bench and a component's core module). What the judge has not seen is
//! refused, never passed: a witness of a property no verifier version defines,
//! a record kind it does not know, a bundle pinned to other bytes. The
//! accepted half keeps the test honest: a well-formed bundle over its own
//! bytes certifies.

#[path = "support/novel.rs"]
mod novel;

use almide::certificate_gate::{judge, Pass};

fn hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn bundle(shipped: &[u8], body: &str) -> Vec<u8> {
    format!("almide-certificate-bundle 2\nproducer test\nartifact sha256 {} artifact.wasm\n{body}", hex(shipped)).into_bytes()
}

#[test]
fn novel_a_certificate_the_linked_verifier_has_not_seen_is_refused() {
    let shipped = novel::ident("module").into_bytes();
    let f = novel::ident("f");
    // Accepted half: a well-formed witness over the pinned bytes certifies.
    let good = bundle(&shipped, &format!("witness ownership 2 {f}\nid\n"));
    assert_eq!(judge(&good, &shipped, false), Ok(Pass::Certified), "a well-formed certificate was refused");
    // A property no verifier defines.
    let prop = novel::ident("property");
    let r = judge(&bundle(&shipped, &format!("witness {prop} 2 {f}\nid\n")), &shipped, false);
    assert!(r.as_ref().is_err_and(|e| e.contains("could not read")), "an unknown property was not refused: {r:?}");
    // A record kind no verifier defines.
    let rec = novel::ident("record");
    let r = judge(&bundle(&shipped, &format!("witness ownership 2 {f}\nid\n{rec} {f}\n")), &shipped, false);
    assert!(r.is_err(), "an unknown record was not refused: {r:?}");
    // The same certificate pinned to other bytes.
    let other = novel::ident("other").into_bytes();
    let r = judge(&good, &other, false);
    assert!(r.as_ref().is_err_and(|e| e.contains("MISMATCH")), "a certificate over other bytes was not refused: {r:?}");
    // A certificate with no witness certifies nothing.
    let r = judge(&bundle(&shipped, ""), &shipped, false);
    assert!(r.is_err(), "an empty certificate was not refused: {r:?}");
}

/// Row `verify.gate-build-incomplete-accepted` (owner ruling 2026-11-24,
/// C-462): a certificate whose producer declared a function it could not
/// certify is INCOMPLETE, and INCOMPLETE is refused unless the caller passes
/// the flag. A fresh function name and a fresh reason the verifier has never
/// seen: refused by default, let through (naming both) only with the flag,
/// and a REJECTED witness beside it stays refused with the flag.
#[test]
fn novel_an_incomplete_certificate_is_refused_without_the_flag() {
    let shipped = novel::ident("module").into_bytes();
    let f = novel::ident("f");
    let g = novel::ident("g");
    let why = novel::ident("reason");
    let incomplete = bundle(&shipped, &format!("witness ownership 2 {f}\nid\nuncertified {g} {why}\n"));
    let r = judge(&incomplete, &shipped, false);
    assert!(
        r.as_ref().is_err_and(|e| e.contains("INCOMPLETE") && e.contains(&g) && e.contains(&why) && e.contains("--allow-incomplete-verification")),
        "an incomplete certificate was not refused by default: {r:?}"
    );
    assert_eq!(
        judge(&incomplete, &shipped, true),
        Ok(Pass::IncompleteAllowed(vec![(g.clone(), why.clone())])),
        "the flag did not let the incomplete certificate through naming its function"
    );
    // The flag allows INCOMPLETE only: a rejected witness is refused with it.
    let rejected = bundle(&shipped, &format!("witness {} 2 {f}\nid\nuncertified {g} {why}\n", novel::ident("property")));
    assert!(judge(&rejected, &shipped, true).is_err(), "the flag let an unreadable certificate through");
    // And over other bytes.
    let other = novel::ident("other").into_bytes();
    assert!(judge(&incomplete, &other, true).is_err(), "the flag let a certificate over other bytes through");
}
