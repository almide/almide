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

use almide::certificate_gate::judge;

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
    assert_eq!(judge(&good, &shipped), Ok(()), "a well-formed certificate was refused");
    // A property no verifier defines.
    let prop = novel::ident("property");
    let r = judge(&bundle(&shipped, &format!("witness {prop} 2 {f}\nid\n")), &shipped);
    assert!(r.as_ref().is_err_and(|e| e.contains("could not read")), "an unknown property was not refused: {r:?}");
    // A record kind no verifier defines.
    let rec = novel::ident("record");
    let r = judge(&bundle(&shipped, &format!("witness ownership 2 {f}\nid\n{rec} {f}\n")), &shipped);
    assert!(r.is_err(), "an unknown record was not refused: {r:?}");
    // The same certificate pinned to other bytes.
    let other = novel::ident("other").into_bytes();
    let r = judge(&good, &other);
    assert!(r.as_ref().is_err_and(|e| e.contains("MISMATCH")), "a certificate over other bytes was not refused: {r:?}");
    // A certificate with no witness certifies nothing.
    let r = judge(&bundle(&shipped, ""), &shipped);
    assert!(r.is_err(), "an empty certificate was not refused: {r:?}");
}
