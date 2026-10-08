//! #2758 (#1696 step 4) — the `fan { a; b }` BLOCK in the structural witness
//! (fan.rs `lower_fan_block`). Every arm runs in order. In `main` an owned
//! Result carrier is born at its arm and its spine released (`id`): its
//! payload keeps the carrier's credit, and after every arm the first err
//! aborts (the checker's abort terminal). In any other effect fn (#3463,
//! fan_block_err.rs) every carrier is kept until the last arm, and the first
//! err carrier is RETURNED: on its site the chosen owned carrier leaves
//! (`m`) and every other owned one is released whole (`d`), a borrowed
//! chosen one is shared out (`am`); past the sites each owned carrier's
//! spine is released (`d`). The fresh tuple then owns each slot: an owned
//! value moves in (`im`), a borrowed one shares and moves (`am`).

const PROGRAM: &str = r#"fn mk(s: String) -> Result[String, String] = if string.len(s) > 0 then ok(s + "!") else err("empty")

fn num(n: Int) -> Result[Int, String] = if n > 0 then ok(n) else err("neg")

effect fn both(s: String) -> (String, Int) = fan {
  mk(s)
  num(2)
}

effect fn one(s: String) -> String = fan {
  mk(s)
}

effect fn held(s: String) -> (String, String) = {
  let r: Result[String, String] = mk(s)
  fan {
    r
    mk(s)
  }
}

effect fn main() -> Unit = {
  let (a, n) = both("x")!
  println("${a} ${n} ${one("y")!}")
  let (p, q) = held("z")!
  println("${p} ${q}")
  let (u, v) = fan {
    mk("m")
    num(3)
  }
  println("${u} ${v}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("fan_block.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

#[test]
fn fan_blocks_witness_their_carriers_abort_and_slots() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let expect = [
        // Each arm is the owned carrier of a Result call (the arm's `!` is its
        // own marker, front_desugar.rs): either one is the err that leaves
        // (`im`) on its site and is released whole (`id`) on the other's or
        // on the ok path. Past the sites both payloads and the tuple are
        // owned values that move on (`im`).
        ("both", "ibamd\n{id|im}\n{id|im}\n{|im}\n{|im}\n{|im}\n"),
        // A BORROWED Result arm (`r`): read (`b`), released by the frame. On
        // its err site it is shared out (`am`) and the owned `mk` carrier is
        // released (`id`); `mk`'s err leaves (`im`) on its own site. On the
        // ok path `r`'s payload is a view of its slot that shares into the
        // tuple (`am`).
        ("held", "ibambamd\nibd\n{id|im}\n{|am}\n{|am}\n{|im}\n{|im}\n{|im}\n"),
        // A one-arm block: its owned carrier leaves on the err site (`im`) or
        // its spine is released on the ok path (`id`), where the payload is
        // an owned value the tail moves out (`im`). (It declined as a payload
        // VIEW while the arm's `!` was lowered as the frame's own exit.)
        ("one", "ibamd\n{id|im}\n{|im}\n{|im}\n"),
        // `main` keeps the abort mode: every `!` site and the block's own
        // first-err site end their path in the checker's abort terminal,
        // which discharges what is held, so each object lives on the path
        // that falls through (`{|…}`).
        ("main", "im\nibd\n{|ad}\n{|ad}\n{|im}\n{|ibd}\n{|ad}\n{|id}\n{|im}\n{|ibd}\n{|ad}\n\n\n{|im}\n{|id}\n{|id}\n{|im}\n{|ibd}\n\n"),
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
