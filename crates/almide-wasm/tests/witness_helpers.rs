//! #2758 (#1696 step 4) — the Emitter-built HELPER frames in the structural
//! witness: the display, equality and ordering bodies of a recursive type, and the
//! deep-equality scan of a composite key.
//! Their block params are lent (no credit here); their only RC events are
//! the temporaries the walk creates. The certificate is audited against the helper's bytes
//! (witness_helper.rs): a helper whose emitted RC calls differ from what
//! was recorded withdraws.

const PROGRAM: &str = r#"type Tree =
  | Leaf
  | Node(Tree, Float, Tree)

type Expr: Ord =
  | Lit(Int)
  | Add(Expr, Expr)

type Chain = { label: String, weight: Float, next: Option[Chain] }

effect fn main() -> Unit = {
  let t = Node(Node(Leaf, 1.5, Leaf), 2.25, Leaf)
  println("${t}")
  let c: Chain = { label: "a", weight: 0.5, next: some({ label: "b", weight: 1.0, next: none }) }
  println("${c}")
  let u = Node(Leaf, 1.5, Leaf)
  println("${t == u} ${u == u} ${c == c}")
  println("${list.sort([Add(Lit(2), Lit(1)), Lit(3), Add(Lit(1), Lit(1))])}")
  let seen = [["a"], ["b"]]
  println("${list.contains(seen, ["b"])} ${list.index_of(seen, ["c"])}")
}
"#;

fn witnesses_of(rel: &str, text: &str) -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir(rel, text).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn witnesses() -> std::collections::BTreeMap<String, String> {
    witnesses_of("helpers.almd", PROGRAM)
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn recursive_types_display_and_comparison_helpers_witness_their_lent_blocks_and_temporaries() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let pinned = [
        // The lent block: known, no credit here, no event (the empty first
        // line). A Float field's printed text: a block the linked printer
        // hands over, copied into the line and released at the site (`id`).
        ("<display:0>", "\nid\n"),
        ("<display:2>", "\nid\n"),
        // No Float field: a pure walk over the lent block.
        ("<display:1>", "\n"),
        // Equality and ordering read both lent operands and allocate nothing.
        ("<named-op:Eq:0>", "\n\n"),
        ("<named-op:Eq:2>", "\n\n"),
        ("<named-op:Cmp:1>", "\n\n"),
    ];
    for (name, want) in pinned {
        let cert = w.get(name).unwrap_or_else(|| panic!("{name} is witnessed: {w:?}"));
        assert!(!cert.starts_with('!'), "{name} no longer declines: {cert:?}");
        assert!(accepted(cert), "{name}: the portable checker must accept {cert:?}");
        assert_eq!(cert, want, "{name}");
    }
    // A composite-key scan (map_rich_variant_key: a Map keyed by a rich
    // variant): its entry block is lent, and the deep `==` it runs per entry
    // allocates nothing.
    let rel = "spec/wasm_cross/map_rich_variant_key.almd";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join(rel)).expect("fixture readable");
    let w = witnesses_of(rel, &text);
    let cert = w.get("<scan:ETy(0)>").unwrap_or_else(|| panic!("the scan helper is witnessed: {w:?}"));
    assert!(accepted(cert), "the portable checker must accept {cert:?}");
    assert_eq!(cert, "\n", "<scan:ETy(0)>");
    // A temporary the walk leaks, or releases twice, and a release of a
    // lent operand, are refused.
    for (bad, what) in [
        ("\ni\n", "leaked temporary"),
        ("\nidd\n", "double-released temporary"),
        ("d\nid\n", "released lent block"),
        ("\nd\n", "released lent right operand"),
    ] {
        assert!(!accepted(bad), "{what}: the checker must refuse {bad:?}");
    }
}
