//! #2167: a record or variant that DERIVES `Ord` orders on both targets, and
//! the derive itself is still what decides.
//!
//! C-053 has claimed "every totally-ordered element type … and variants (tag
//! order)" since 0.24.0 and nothing executed the claim. A `type T: Ord` record
//! or variant sorted on native — lexicographically in FIELD DECLARATION order,
//! or by CASE order then payload, native's derive — and BOTH wasm legs refused
//! to build it: `list-sort-elem:Named(0)` on the structural leg,
//! `list.sort_x` on the incumbent. Native answered; wasm would not build.
//!
//! The cause was #2154's, one function over: the `sort` arm and
//! `lower_list_min_max` each kept a copy of the orderable-element list, drifted
//! from what `emit_val_cmp` could order. Both copies are gone.
//!
//! The negative cells matter as much as the positive ones. Ordering a record
//! WITHOUT `: Ord` must stay E030 on both targets (#1521 — native's monomorph
//! needs the derive, and structural orderability of the fields is deliberately
//! not enough). The recursive cell walled when this landed and is closed by
//! #2172, which moved every `Named` comparator out of line — its evidence
//! lives in tests/ord_recursive_test.rs.

use std::process::Command;

const RECORD_ELEM: &str = r#"
type Inner: Ord = { n: Int, s: String }
effect fn main() -> Unit = {
  let xs = [Inner { n: 2, s: "x" }, Inner { n: 1, s: "y" }, Inner { n: 1, s: "a" }]
  for i in xs |> list.sort {
    println("${i.n} ${i.s}")
  }
}
"#;

const VARIANT_ELEM: &str = r#"
type Shape: Ord = | Dot | Line{ len: Int } | Box{ w: Int, h: Int }
effect fn main() -> Unit = {
  let xs = [Box { w: 1, h: 9 }, Line { len: 5 }, Dot, Box { w: 1, h: 2 }]
  for s in xs |> list.sort {
    println("${s}")
  }
}
"#;

/// The extremum walks the same comparator through a different arm — the one
/// whose copy of the element list was the second of the two deleted.
const VARIANT_MIN_MAX: &str = r#"
type Level: Ord = | Low | Mid | High
effect fn main() -> Unit = {
  println("${list.min([High, Low, Mid])}")
  println("${list.max([High, Low, Mid])}")
}
"#;

/// A record in the KEY position — #2154's route carrying #2167's shape.
const RECORD_KEY: &str = r#"
type K: Ord = { n: Int, s: String }
effect fn main() -> Unit = {
  for (w, c) in [("x", 2), ("y", 1), ("z", 2)] |> list.sort_by(((w, c)) => K { n: 0 - c, s: w }) {
    println("${w} ${c}")
  }
}
"#;

/// No `: Ord` — the checker must still refuse, on BOTH targets (#1521).
const RECORD_WITHOUT_THE_DERIVE: &str = r#"
type NoDerive = { a: Int }
effect fn main() -> Unit = {
  for k in [NoDerive { a: 2 }, NoDerive { a: 1 }] |> list.sort {
    println("${k.a}")
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
fn a_record_that_derives_ord_sorts_by_field_declaration_order_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(RECORD_ELEM, "record element");
    assert_eq!(out, "1 a\n1 y\n2 x\n", "field n decides, field s breaks the tie");
}

#[test]
fn a_variant_that_derives_ord_sorts_by_case_order_then_payload_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(VARIANT_ELEM, "variant element");
    assert_eq!(
        out,
        "Dot\nLine { len: 5 }\nBox { w: 1, h: 2 }\nBox { w: 1, h: 9 }\n",
        "declaration order of the cases first, then the case's fields"
    );
}

#[test]
fn min_and_max_walk_the_same_comparator() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(VARIANT_MIN_MAX, "variant min/max");
    assert_eq!(out, "some(Low)\nsome(High)\n", "tag order, both ends");
}

#[test]
fn a_record_key_orders_the_same_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(RECORD_KEY, "record key");
    assert_eq!(out, "x 2\nz 2\ny 1\n", "count DESC then word ASC, through a record key");
}

/// The derive is the gate, and it must be the gate on BOTH targets — a record
/// that merely looks orderable is refused at check time, identically.
#[test]
fn a_record_without_the_derive_is_refused_identically_on_both_targets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, RECORD_WITHOUT_THE_DERIVE).expect("source");
    for target in ["rust", "wasm"] {
        let built = build(dir.path(), &source, target);
        let report = String::from_utf8_lossy(&built.stdout).to_string()
            + &String::from_utf8_lossy(&built.stderr);
        assert!(!built.status.success(), "{target}: a record with no `: Ord` built:\n{report}");
        assert!(
            report.contains("E030") && report.contains("has no ordering"),
            "{target}: expected the ordering diagnostic, got:\n{report}"
        );
        assert!(
            report.contains("DECLARES the derive"),
            "{target}: the hint must name the derive — that is the thing the reader has to \
             write, and the hint used to claim records order without saying so:\n{report}"
        );
    }
}
