//! #2758 (#1696 step 4) — the SPREAD RECORD `{ ...b, f: v }` in the structural
//! witness. A base that DIES at the spread (a param at the frame's tail,
//! #3406) hands its credit to the rebuilt record (`im`); any other bound
//! base is only read, the copy helper's credit on each handle slot (and its release
//! of the overwritten one) is the helper's interior bookkeeping, and each
//! field store is the payload-store hook's (`am` for a shared Var, `im` for a
//! moved temporary). A PRODUCED base is an owned temporary the copy reads and
//! the site releases right after the copy (#3373), so that frame certifies.

const PROGRAM: &str = r#"type Pt = { name: String, n: Int, tags: List[String] }

fn point(s: String) -> Pt = Pt { name: s, n: 2, tags: [] }

fn renamed(p: Pt, s: String) -> Pt = { ...p, name: s }

fn bumped(p: Pt) -> Pt = { ...p, n: p.n + 1 }

fn tagged(p: Pt) -> Pt = { ...p, tags: ["x"] }

fn local_base(s: String) -> Int = {
  let p = point(s)
  let q = { ...p, n: 5 }
  q.n + p.n
}

fn fresh_base(s: String) -> Int = {
  let q = { ...point(s), n: 3 }
  q.n
}

effect fn main() -> Unit = {
  let p = point("a")
  println("${renamed(p, "b").name} ${bumped(p).n} ${list.len(tagged(p).tags)}")
  println("${local_base("c")} ${fresh_base("d")}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("spread.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

#[test]
fn spread_records_witness_their_field_stores_and_release_a_produced_base() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let expect = [
        // #3406: a spread at the frame's tail CONSUMES its base param (owned,
        // param_borrow.rs), and the dying base hands its credit to the
        // rebuilt record (`im`: received, moved — dying_move.rs); the
        // borrowed `s` shares into the overwritten slot; the result moves out.
        ("renamed", "im\nam\nim\n"),
        // A scalar field carries no site: `p.n` reads the base (`b`) before
        // its hand-over, and the result moves out.
        ("bumped", "ibm\nim\n"),
        // A fresh list literal (and its element) moves into the slot.
        ("tagged", "im\nim\nim\nim\n"),
        // A LOCAL base: born by the call, read by the spread (`b`) and by
        // `p.n`, released at the exit; the copy is a fresh bind of its own.
        ("local_base", "\nibbd\nibd\n"),
        // #3373: a PRODUCED base is born by the call and released by the
        // site right after the copy (`id`); the copy is a fresh bind.
        ("fresh_base", "\nid\nibd\n"),
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
