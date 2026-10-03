//! #3261: a var bound to a BORROW of a record field (`let snap = cell.words`)
//! must hold a reference of its own before a copy-on-write of that field path
//! releases the record's old block. Without it the borrow outlives its owner:
//! the copy's field is then uniquely owned, `list.pop` mutates it in place, and
//! `snap` observes the pop (C-033). The certificate rejects the read of `snap`
//! after the old block's release (`b` after the last `d`), so this pins the
//! lowering's `Dup` through the certificate of the spec fixture's test body.

mod common;
use common::reject_reason;

#[test]
fn a_bound_field_borrow_owns_its_block_across_the_field_cow() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("spec/lang/field_borrow_cow_test.almd"))
        .expect("read the fixture");
    let certs = almide_mir::pipeline::ownership_certificates(&source).expect("the fixture must lower");
    let body: Vec<&(String, String)> =
        certs.iter().filter(|(n, _)| n.contains("a bound field keeps its value")).collect();
    assert_eq!(body.len(), 1, "the test body must reach the certificate view: {:?}", certs.iter().map(|(n, _)| n).collect::<Vec<_>>());
    let (_, cert) = body[0];
    let bad: Vec<String> = cert.lines().filter_map(|l| reject_reason(l).map(|r| format!("{l:?} — {r}"))).collect();
    assert!(bad.is_empty(), "rejected ownership line(s):\n  {}", bad.join("\n  "));
    // `snap`'s own reference: a line that opens with the `Dup` (`a`) of the field.
    assert!(cert.lines().any(|l| l.starts_with('a') && l.ends_with('d')), "no owned `snap` line in {cert:?}");
}
