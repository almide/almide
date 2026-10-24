//! #4013 — the init witness (witness_init.rs): a list literal's construction
//! site. The emitter records the allocation and each slot store; the proven
//! checker's portable mirror must accept the site, and a site missing a
//! store, or storing past the end, must be rejected.

const PROGRAM: &str = r#"effect fn main() -> Unit = {
  let xs = [1, 2, 3]
  let names = ["a", "b"]
  let empty: List[Int] = []
  let r = 2..<5
  println("${xs[2]} ${names[1]} ${list.len(empty)} ${r[0]}")
}
"#;

fn accepts(w: &str) -> bool {
    almide_verify::check(almide_verify::Property::Init, w.as_bytes())
}

#[test]
fn each_list_literal_is_a_site_with_every_slot_stored() {
    // ONE test: the witness sink is process-global.
    let ir = almide_spine::s5::lower_to_ir("sites.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let all = almide_wasm::witness::take_init();
    let _ = almide_wasm::witness::take();
    let (_, w) = all.last().expect("an init witness for the shipped pass");
    let sites: Vec<&str> = w.split(';').collect();
    // Ints are 8-byte slots, Strings 4-byte handles; the empty literal is a
    // site with no slots. Representations are the list types' handles.
    assert_eq!(sites.len(), 4, "{w}");
    assert!(sites[0].starts_with("h12 w8 n3 ") && sites[0].ends_with(" s0 s1 s2"), "{w}");
    assert!(sites[1].starts_with("h12 w4 n2 ") && sites[1].ends_with(" s0 s1"), "{w}");
    assert!(sites[2].starts_with("h12 w8 n0 "), "{w}");
    // The range is a loop fill: sized from a local, filled by the countdown
    // loop on that same local, one 8-byte slot per step.
    let fill = sites[3];
    assert!(fill.starts_with("h12 w8 v") && fill.ends_with(" t8"), "{w}");
    let local = |tag: char| fill.split_whitespace().find(|t| t.starts_with(tag)).map(|t| t[1..].to_string());
    assert_eq!(local('v'), local('f'), "the loop counts down the count the block was sized from: {fill}");
    assert!(accepts(w), "the portable checker must accept {w:?}");
    assert!(!accepts(&w.replacen(" t8", " t16", 1)), "a cursor skipping slots must be rejected");
    // The drills: a lost store, a store past the end.
    assert!(!accepts(&w.replacen(" s1 s2", " s2", 1)), "a missing slot must be rejected");
    assert!(!accepts(&w.replacen(" s0 s1 s2", " s0 s1 s2 s3", 1)), "a store past the end must be rejected");
}
