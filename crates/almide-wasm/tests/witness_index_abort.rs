//! #2755 (#1696 step 4) — list INDEXING and main's `!` ABORT in the
//! structural witness. `xs[i]` over a bound list is a VIEW of the element
//! slot: a consumer that keeps it shares it (`am`), a reader spends nothing.
//! Out of bounds, and a failed `!` in `main`, ABORT the process with nothing
//! released (`Continuation::Abort`): the checker's abort terminal (`t`,
//! format v6) discharges what an aborting path still holds, and an aborting
//! path that a returning path extends needs no arm of its own — the returning
//! path is checked fault-free, so its prefix is (`check_line_prefix_safe`).

const PROGRAM: &str = r#"fn elem_at(xs: List[String], i: Int) -> String = xs[i]

fn tagged_at(xs: List[String], i: Int) -> String = {
  let t = xs[0] + "!"
  t + xs[i]
}

fn half(n: Int) -> Result[Int, String] = if n % 2 == 0 then ok(n / 2) else err("odd")

effect fn main() -> Unit = {
  let keep = "k${elem_at(["a", "b"], 1)}"
  let h = half(4)!
  println("${keep} ${h} ${tagged_at(["p", "q"], 1)}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("index_abort.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn index_views_and_main_aborts_witness_exactly() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let get = |n: &str| w.get(n).map(String::as_str).unwrap_or("<none>").to_string();
    for name in ["elem_at", "tagged_at", "main"] {
        let c = get(name);
        assert!(!c.starts_with('!') && accepted(&c), "{name}: {c:?}");
    }
    // The element view is shared out on the returning path; the abort path
    // has nothing of it.
    assert_eq!(get("elem_at"), "\n{|am}\n");
    // `t` is born after the first index's abort; the second index aborts
    // holding it (`ib` — the index reads it, #3259 — a prefix of the
    // returning `ibd`, so no arm of its own): one path empty, the other `ibd`.
    assert!(get("tagged_at").contains("{|ibd}\n"), "{:?}", get("tagged_at"));
    // main's `!` aborts holding `keep` (born before it): nothing is released
    // on the abort path, and its prefix of the returning `id` is what the
    // line carries — a flat `id`, never `{i|id}` (which would reject).
    assert!(get("main").starts_with("im\nim\nid\n"), "{:?}", get("main"));
    assert!(!get("main").contains("{i|"), "{:?}", get("main"));
}
