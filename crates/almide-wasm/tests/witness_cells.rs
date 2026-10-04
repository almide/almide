//! #2758 (#1696 step 4) — C-319 CELLS in the structural witness. A `var`
//! a closure captures and the frame mutates lives in a refcounted cell: the
//! local holds the CELL, the cell holds the occupant. The Bind route hands
//! the occupant to the new cell (`im` for an owned rhs, `am` on a borrowed
//! Var's line) and the local owns the fresh cell (`i`); each env that
//! captures it takes a share that moves into the env (`am`, the route's
//! `F_INC`); every exit releases it (`d`, `$dec_cell`). A write through the
//! cell in the allocating frame is the outer holder's (#3138): received at
//! the frame's start, rebound `d`/`i`, handed back at its end. A read of
//! the cell var stored into a holder shares the occupant (`am`, on the
//! cell's line: the occupant lives while the cell holds it). No new event
//! letter — the cell is an object like any block.

const PROGRAM: &str = r#"fn scalar(n: Int) -> Int = {
  var c = n
  let get = () => c
  c = c + 1
  get()
}

fn latest(steps: List[Int]) -> List[Int] = {
  var t = [0]
  for n in steps {
    let peek = () => list.len(t)
    t = [peek() + n]
  }
  t
}

fn shared(xs: List[Int]) -> Int = {
  var t = xs
  let peek = () => list.len(t)
  t = t + [1]
  peek()
}

fn stored(steps: List[Int]) -> List[List[Int]] = {
  var t = [0]
  var out: List[List[Int]] = []
  for n in steps {
    let peek = () => list.len(t)
    t = [peek() + n]
    out = out + [t]
  }
  out
}

effect fn main() -> Unit = {
  println("${scalar(1)} ${list.len(latest([1, 2]))} ${shared([3])} ${list.len(stored([4]))}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("cells.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn cell_frames_witness_exactly_and_a_dropped_cell_release_is_refused() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    for name in ["scalar", "latest", "shared", "stored"] {
        let cert = w.get(name).unwrap_or_else(|| panic!("{name} is witnessed: {w:?}"));
        assert!(!cert.starts_with('!'), "{name} no longer declines bind:cell: {cert:?}");
        assert!(accepted(cert), "{name}: the portable checker must accept {cert:?}");
    }
    // An Int cell: the cell is born (`i`), shared into `get`'s env (`am`)
    // and released at the exit (`d`); the closure is lent to its call and
    // released (`ibd`). The scalar occupant carries no site.
    assert_eq!(w["scalar"], "iamd\nibd\n");
    // A heap cell bound from a borrowed param: the param's block shares
    // into the cell (`am`), the cell's own line is `iamd`.
    assert!(w["shared"].starts_with("am\niamd\n"), "{:?}", w["shared"]);

    // A READ of the cell var stored into a list literal: `rc_share_guard`
    // shares the occupant (#2010) and the share moves into the literal
    // (`am`) — no longer a `store:retain-cell` decline.
    // Both loop frames are pinned as rendered (the per-path renderer,
    // witness_paths.rs, orders the lines), so any drift is reviewed here.
    assert_eq!(w["stored"], "\nim\nid\namam\nibamd\n\nibd\n\nim\n\nid\nim\nim\n");
    assert_eq!(w["latest"], "\nim\niamd\nam\n\nibd\n\nim\n\nid\nim\nim\n");

    // The cell's line is checked like any block's: a cell the frame never
    // releases, or releases twice, is refused.
    for (bad, what) in [("iam\nibd\n", "leaked cell"), ("iamdd\nibd\n", "double-released cell"), ("imd\nibd\n", "env share not taken")] {
        assert!(!accepted(bad), "{what}: the checker must refuse {bad:?}");
    }
}
