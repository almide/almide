//! #2172: a RECURSIVE type that derives `Ord` orders on both targets, because
//! the comparator of a user `Named` type is emitted ONCE, out of line, and
//! called.
//!
//! #2167 made records and variants orderable by inlining their field chain at
//! the use site, which left one cell of C-053 walled: `type Tree: Ord = { v:
//! Int, kids: List[Tree] }` unfolds forever at emit time, so the emitter
//! refused with `cmp-recursive-named:` rather than blow its own stack. Native
//! derived `Ord` and printed the answer — an honest refusal, still a
//! divergence.
//!
//! A cycle guard alone would not have been enough. It saves the type that
//! contains ITSELF, but a MUTUALLY recursive pair (`Node` holding
//! `List[Leaf]` holding `List[Node]`) still inlines its first level, and that
//! exhausted the i32 hold pool: `hold-depth-i32`, a different wall for the
//! same reason. Making every `Named` a CALL removes the whole class — and is
//! also what makes the comparator shrink rather than grow, since a type
//! compared by `sort` AND `min` AND `max` now emits one body instead of one
//! per site.

use std::process::Command;

const RECURSIVE_RECORD: &str = r#"
type Tree: Ord = { v: Int, kids: List[Tree] }
fn show(t: Tree) -> String =
  "${t.v}(${t.kids |> list.map((k) => show(k)) |> list.join(",")})"
effect fn main() -> Unit = {
  let ts = [
    Tree { v: 1, kids: [Tree { v: 9, kids: [] }] },
    Tree { v: 1, kids: [Tree { v: 2, kids: [] }] },
    Tree { v: 1, kids: [] },
    Tree { v: 1, kids: [Tree { v: 2, kids: [Tree { v: 5, kids: [] }] }] },
    Tree { v: 0, kids: [Tree { v: 9, kids: [] }] },
  ]
  for t in ts |> list.sort {
    println("${show(t)}")
  }
}
"#;

/// A recursive VARIANT: the tag decides, then the case's payload — whose
/// fields are lists of this same type.
const RECURSIVE_VARIANT: &str = r#"
type Expr: Ord = | Lit { n: Int } | Add { l: List[Expr], r: List[Expr] }
effect fn main() -> Unit = {
  let es = [
    Add { l: [Lit { n: 2 }], r: [] },
    Lit { n: 5 },
    Add { l: [Lit { n: 1 }], r: [Lit { n: 0 }] },
    Lit { n: 1 },
  ]
  for e in es |> list.sort {
    println("${e}")
  }
}
"#;

/// The shape a cycle guard does NOT save: neither type contains itself, so the
/// first level always inlines and the hold pool runs out one level in.
const MUTUALLY_RECURSIVE: &str = r#"
type Node: Ord = { tag: String, next: List[Leaf] }
type Leaf: Ord = { back: List[Node], w: Int }
fn show_leaf(l: Leaf) -> String =
  "[${l.back |> list.map((n) => n.tag) |> list.join(",")}]/${l.w}"
effect fn main() -> Unit = {
  let ns = [
    Node { tag: "b", next: [] },
    Node { tag: "a", next: [Leaf { back: [], w: 3 }] },
    Node { tag: "a", next: [Leaf { back: [Node { tag: "z", next: [] }], w: 1 }] },
    Node { tag: "a", next: [] },
  ]
  for n in ns |> list.sort {
    println("${n.tag}<${n.next |> list.map((l) => show_leaf(l)) |> list.join(",")}>")
  }
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn build(dir: &std::path::Path, source: &std::path::Path, target: &str) -> std::process::Output {
    let artifact = dir.join(if target == "rust" { "native" } else { "m.wasm" });
    Command::new(almide_bin())
        .args([
            "build",
            source.to_str().expect("path"),
            "--target",
            target,
            "-o",
            artifact.to_str().expect("path"),
        ])
        .env_remove("ALMIDE_WASM_INCUMBENT")
        .env_remove("ALMIDE_COMPONENT_P3")
        .output()
        .expect("build")
}

/// Build and run on both legs, asserting the observable agrees and that the
/// wasm build came from the STRUCTURAL leg — an answer that agrees only
/// because the build fell back would not be this fix.
fn agree_on_the_structural_leg(program: &str, label: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let mut native_stdout: Option<String> = None;
    for target in ["rust", "wasm"] {
        let built = build(dir.path(), &source, target);
        assert!(
            built.status.success(),
            "{label}/{target} build failed:\n{}",
            String::from_utf8_lossy(&built.stderr)
        );
        if target == "wasm" {
            let report = String::from_utf8_lossy(&built.stdout).to_string()
                + &String::from_utf8_lossy(&built.stderr);
            assert!(
                report.contains("structural leg"),
                "{label}: the wasm build did not take the structural leg:\n{report}"
            );
        }
        let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
        let mut command = if target == "rust" {
            Command::new(&artifact)
        } else {
            let mut c = Command::new("wasmtime");
            c.arg("run").arg(&artifact);
            c
        };
        let out = command.output().expect("run");
        assert!(
            out.status.success(),
            "{label}/{target} exited {:?}:\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        match &native_stdout {
            Some(native) => {
                assert_eq!(&stdout, native, "{label}: the wasm leg answered differently from native")
            }
            None => native_stdout = Some(stdout),
        }
    }
    native_stdout.expect("one leg ran")
}

#[test]
fn a_recursive_record_orders_all_the_way_down_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(RECURSIVE_RECORD, "recursive record");
    // `v` decides; on a tie the child LIST decides, shorter prefix first, and
    // the children's own children decide after that — native's derive, reached
    // through a comparator that calls itself.
    assert_eq!(out, "0(9())\n1()\n1(2())\n1(2(5()))\n1(9())\n");
}

#[test]
fn a_recursive_variant_orders_by_tag_then_its_own_payload_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(RECURSIVE_VARIANT, "recursive variant");
    assert_eq!(
        out,
        "Lit { n: 1 }\nLit { n: 5 }\nAdd { l: [Lit { n: 1 }], r: [Lit { n: 0 }] }\n\
         Add { l: [Lit { n: 2 }], r: [] }\n"
    );
}

#[test]
fn a_mutually_recursive_pair_orders_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(MUTUALLY_RECURSIVE, "mutual recursion");
    // The three `a` nodes are separated only INSIDE their leaves: no leaves,
    // then the leaf whose `back` is empty, then the one that points at a node.
    assert_eq!(out, "a<>\na<[]/3>\na<[z]/1>\nb<>\n");
}

/// The size half of the same change: the comparator is emitted ONCE per type,
/// so widening a record costs the same whether the program compares it in one
/// place or in three.
///
/// Measured on this tree (5-field record, `--target wasm`, no wasm-opt): the
/// four extra fields cost 349 bytes at one comparison site and 349 at three.
/// Inlined at the use site — the #2167 shape this replaces — the same widening
/// cost 588 bytes, because `list.sort` alone reaches `emit_val_cmp` twice.
#[test]
fn the_comparator_is_emitted_once_per_type_not_once_per_use() {
    let narrow = "a: Int";
    let wide = "a: Int, b: String, c: Bool, d: Int, e: String";
    let narrow_lit = "a: 1";
    let wide_lit = r#"a: 1, b: "x", c: true, d: 2, e: "y""#;
    let program = |fields: &str, lit: &str, sites: &str| {
        format!(
            "type R: Ord = {{ {fields} }}\n\
             fn s1(xs: List[R]) -> Int = xs |> list.sort |> list.len\n\
             fn s2(xs: List[R]) -> Int = xs |> list.min |> option.map((r) => r.a) ?? 0\n\
             fn s3(xs: List[R]) -> Int = xs |> list.max |> option.map((r) => r.a) ?? 0\n\
             fn main() -> Unit = {{\n  let xs = [R {{ {lit} }}]\n  println(\"${{{sites}}}\")\n}}\n"
        )
    };
    let size = |src: String| -> u64 {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("main.almd");
        std::fs::write(&source, src).expect("source");
        let built = build(dir.path(), &source, "wasm");
        assert!(
            built.status.success(),
            "build failed:\n{}",
            String::from_utf8_lossy(&built.stderr)
        );
        std::fs::metadata(dir.path().join("m.wasm")).expect("artifact").len()
    };
    let one = "s1(xs)";
    let three = "s1(xs) + s2(xs) + s3(xs)";
    let widening_at_one = size(program(wide, wide_lit, one)) - size(program(narrow, narrow_lit, one));
    let widening_at_three =
        size(program(wide, wide_lit, three)) - size(program(narrow, narrow_lit, three));
    assert_eq!(
        widening_at_one, widening_at_three,
        "the field chain must be emitted once for the type, not once per comparison site \
         (one site: {widening_at_one} B, three sites: {widening_at_three} B)"
    );
}
