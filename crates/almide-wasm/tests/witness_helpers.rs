//! #2758 (#1696 step 4) — the Emitter-built HELPER frames in the structural
//! witness: the display body of a recursive type. Its block param is lent
//! (no credit here); its only RC events are the temporaries its walk
//! creates. The certificate is audited against the helper's bytes
//! (witness_helper.rs): a helper whose emitted RC calls differ from what
//! was recorded withdraws.

const PROGRAM: &str = r#"type Tree =
  | Leaf
  | Node(Tree, Float, Tree)

type Chain = { label: String, weight: Float, next: Option[Chain] }

effect fn main() -> Unit = {
  let t = Node(Node(Leaf, 1.5, Leaf), 2.25, Leaf)
  println("${t}")
  let c: Chain = { label: "a", weight: 0.5, next: some({ label: "b", weight: 1.0, next: none }) }
  println("${c}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("helpers.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn a_recursive_types_display_helper_witnesses_its_lent_block_and_its_temporaries() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    for name in ["<display:0>", "<display:1>"] {
        let cert = w.get(name).unwrap_or_else(|| panic!("{name} is witnessed: {w:?}"));
        assert!(!cert.starts_with('!'), "{name} no longer declines display: {cert:?}");
        assert!(accepted(cert), "{name}: the portable checker must accept {cert:?}");
        // The lent block: known, no credit here, no event (the empty first
        // line). The Float field's printed text: a block the linked printer
        // hands over, copied into the line and released at the site (`id`).
        assert_eq!(cert, "\nid\n", "{name}");
    }
    // A temporary the walk leaks, or releases twice, is refused.
    for (bad, what) in [("\ni\n", "leaked temporary"), ("\nidd\n", "double-released temporary"), ("d\nid\n", "released lent block")] {
        assert!(!accepted(bad), "{what}: the checker must refuse {bad:?}");
    }
}
