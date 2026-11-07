mod harness;

fn compile(src: &str) -> (Vec<u8>, std::collections::BTreeMap<String, String>) {
    let ir = almide_spine::s5::lower_to_ir("never_arms.almd", src).expect("front");
    almide_wasm::witness::start_collecting();
    let bytes = {
        let _counts = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("emit")
    };
    let certs: std::collections::BTreeMap<_, _> =
        almide_wasm::witness::take().into_iter().collect();
    for (name, cert) in &certs {
        assert!(!cert.starts_with('!'), "{name}: {cert}");
        assert!(accepted(cert), "{name}: {cert}");
    }
    (bytes, certs)
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn never_arms_preserve_live_shares_and_record_process_abort() {
    // The collector is process-global: keep the probes in one serial test.
    let (bytes, certs) = compile(
        r#"
fn first(s: String, n: Int) -> String =
  if n > 0 then (if n > 10 then panic("limit") else s) else "empty"
fn last(s: String, n: Int) -> String =
  if n > 0 then (if n <= 10 then s else panic("limit")) else "empty"
fn main() -> Unit = {
  let a = "v${3}"
  println("${first(a, 1)} ${last(a, 1)} ${first(a, 0)} ${last(a, 0)} ${a}")
}
"#,
    );
    for name in ["first", "last"] {
        let cert = certs.get(name).expect("live borrowed arm witnessed");
        assert!(
            !accepted(&cert.replace('a', "")),
            "share removal accepted: {name}: {cert}"
        );
    }
    let r = harness::run_wasm(&bytes).expect("run live arms");
    assert_eq!(r.exit, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "v3 v3 empty empty v3\n");
    let counts = r.alloc_count.expect("allocation counts");
    assert_eq!(counts.allocs, counts.frees);

    let (bytes, certs) = compile(
        r#"import process
effect fn exit_held(s: String) -> Never = {
  let held = s + "!"
  println(held)
  process.exit(4)
}
effect fn main() -> Unit = exit_held("value")!
"#,
    );
    let cert = certs.get("exit_held").expect("abort frame witnessed");
    assert!(
        !accepted(&cert.replace('t', "")),
        "abort removal accepted: {cert}"
    );
    let r = harness::run_wasm(&bytes).expect("run process exit");
    assert_eq!(r.exit, 4);
    assert_eq!(r.stdout, "value!\n");
    assert_eq!(r.stderr, "");

    for src in [
        include_str!("../../../spec/wasm_cross/bigint_text_and_pow_limits.almd"),
        include_str!("../../../spec/wasm_cross/never_fn_value_positions.almd"),
    ] {
        compile(src);
    }
}
