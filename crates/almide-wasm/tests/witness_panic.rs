//! #2758 (#1696 step 4) — `panic(msg)` in the structural witness (calls.rs).
//! The line `"PANIC: " + msg` is the arm scope's parked temporary (`id`), the
//! process aborts — the path ends in the checker's abort terminal — and an
//! OWNED message is an operand the concat only reads: a block born on that
//! path and discharged by the abort. An arm that panicked settles no value:
//! the settling instructions after the abort are unreachable.

const PROGRAM: &str = r#"fn mk(n: Int) -> String = "m${n}"

fn tail_pure(n: Int) -> Int = if n > 0 then n else panic("tail ${n}")

fn from_call(n: Int) -> Int = if n > 0 then n else panic(mk(n))

fn plain(n: Int) -> Int = if n > 0 then n else panic("plain")

fn label(n: Int) -> String = {
  let l = if n < 0 then panic("neg ${n}") else "n=${n}"
  l + "!"
}

fn main() -> Unit = {
  println("${tail_pure(1)} ${from_call(2)} ${plain(3)} ${label(4)}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("panic.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

#[test]
fn panics_abort_and_discharge_their_message() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let expect = [
        // The panic arm: the line is born and released by its scope (`id`);
        // the interpolated message is born and held into the abort (`it`).
        ("tail_pure", "{|id}\n{|it}\n"),
        // A call's message is the call's handed-over credit, held likewise.
        ("from_call", "{|id}\n{|it}\n"),
        // A literal message is a pool static: only the line.
        ("plain", "{|id}\n"),
        // A value arm that panicked settles no value; the other arm's value
        // moves into the join (`im`), which the bind owns (`ibd`), and the
        // concatenation moves out (`im`) — on the path that did not abort.
        ("label", "{|id}\n{|it}\n{|im}\n{|ibd}\n{|im}\n"),
    ];
    for (name, cert) in expect {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed"));
        assert_eq!(got, cert, "{name}");
        assert!(
            almide_verify::check(almide_verify::Property::Ownership, got.as_bytes()),
            "{name}: the portable checker must accept {got:?}"
        );
    }
}
