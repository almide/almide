//! A match keeps its subject alive when a selected arm or guard replaces
//! the root it read. The temporary uses the ordinary Bind ownership rule.
mod harness;

#[test]
fn match_subject_survives_rebinding_and_keeps_checked_ownership() {
    let cases = [
        (include_str!("../../../tests/fixtures/global_tagged_read.almd"),
            "first 1\nheld first 1\nnone\nlast 2\n"),
        (r#"type Box = { label: String }
var current: Box? = some(Box { label: "guarded" })
fn reject() -> Bool = { current = none; false }
fn main() -> Unit = {
 match current {
  some(b) if reject() => println("wrong"),
  some(b) => println("retry ${b.label}"),
  none => println("wrong none"),
 }
 var held: Box? = some(Box { label: "local" })
 match held {
  some(b) => { held = none; println(b.label) },
  none => println("wrong local"),
 }
}
"#, "retry guarded\nlocal\n"),
    ];
    for (source, expected) in cases {
        let ir = almide_spine::s5::lower_to_ir("match_snapshot.almd", source).expect("front");
        almide_wasm::witness::start_collecting();
        let bytes = almide_wasm::emit_program(&ir).expect("emit");
        let witnesses = almide_wasm::witness::take();
        assert!(!witnesses.is_empty());
        for (name, cert) in witnesses {
            assert!(almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()),
                "{name}: rejected ownership witness {cert:?}");
        }
        let out = harness::run_wasm(&bytes).expect("run");
        assert_eq!(out.exit, 0, "{}", out.stderr);
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.is_empty(), "{}", out.stderr);
    }
}
