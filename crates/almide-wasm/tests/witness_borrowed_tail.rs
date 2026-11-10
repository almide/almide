mod harness;

#[test]
fn returning_either_borrowed_arm_certifies_the_actual_ret_inc() {
    let src = r#"fn choose(a: String, b: String, c: Bool) -> String = if c then a else b
    fn local_tail(s: String, n: Int) -> String = if n >= 0 then s else {
      let t = "v${n}"
      t
    }
    fn main() -> Unit = {
      let a = "le${"ft"}"
      let b = "ri${"ght"}"
      println("${choose(a, b, true)} ${choose(a, b, false)} ${local_tail(a, -1)} ${local_tail(a, 1)}")
      println("${choose(a, a, false)} ${a} ${b}")
    }"#;
    let ir = almide_spine::s5::lower_to_ir("borrowed_tail.almd", src).expect("front");
    // One collector: the witness sink is process-global.
    almide_wasm::witness::start_collecting();
    let bytes = {
        let _counts = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("emit")
    };
    let certs: std::collections::BTreeMap<_, _> = almide_wasm::witness::take().into_iter().collect();
    for name in ["choose", "local_tail"] {
        let cert = certs.get(name).expect("function witnessed");
        assert!(!cert.starts_with('!'), "{name}: {cert}");
        assert!(almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()), "{name}: {cert}");
    }
    // Removing the real ret-inc's shares must fail the independent checker.
    let poison = certs["choose"].replace('a', "");
    assert!(!almide_verify::check(almide_verify::Property::Ownership, poison.as_bytes()));
    let result = harness::run_wasm(&bytes).expect("run");
    assert_eq!(result.exit, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "left right v-1 left\nleft left right\n");
    let counts = result.alloc_count.expect("the harness counts allocations");
    assert_eq!(counts.allocs, counts.frees);
}
