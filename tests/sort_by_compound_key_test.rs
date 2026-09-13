//! #2154: `list.sort_by` with a COMPOUND key orders on both legs, and the leg
//! that cannot order it refuses instead of emitting a trapping artifact.
//!
//! `entries |> list.sort_by(((w, c)) => (0 - c, w))` — count descending, word
//! ascending, the word-count idiom — type-checked, ran on native, and walled
//! the structural leg (`list-sort-by-key:Tuple(4)`). The build therefore fell
//! to the incumbent, whose cached-key twins declare `f: (Int) -> Int` and
//! compare the cached key as a raw i64. The key closure hands back an i32
//! HANDLE, so the sort's `call_indirect` was an `indirect call type mismatch`
//! trap at run time — out of an artifact `build --target wasm` had just
//! reported `verified`. Had the two signatures lined up, the sort would have
//! ordered by the key's ADDRESS instead: a silent wrong answer.
//!
//! The structural leg now hands the key type to `emit_val_cmp`, the same
//! type-directed comparator `list.sort` uses for a compound ELEMENT, so the
//! key domain is stated in exactly one place. The two checks below are the two
//! halves of the fix: the answer agrees, and the incumbent's mis-render is
//! gone rather than merely unreachable.

use std::process::Command;

/// The issue's shape, reduced: a `(Int, String)` key over heap elements.
const TUPLE_KEY: &str = r#"
effect fn main() -> Unit = {
  let entries = [("pear", 2), ("fig", 3), ("apple", 2), ("date", 1)]
  for (w, c) in entries |> list.sort_by(((w, c)) => (0 - c, w)) {
    println("${w} ${c}")
  }
}
"#;

/// A nested-List key over heap elements — the same routing question with the
/// other compound shape, and the one whose comparator recurses.
const LIST_KEY: &str = r#"
effect fn main() -> Unit = {
  let entries = [("pear", 2), ("fig", 3), ("apple", 2), ("date", 1)]
  for (w, c) in entries |> list.sort_by(((w, c)) => [0 - c, string.len(w)]) {
    println("${w} ${c}")
  }
}
"#;

/// An Option key over a SCALAR element list — the non-`_rc` route, which the
/// incumbent reached through a different arm of the same router.
const OPTION_KEY_SCALAR_ELEM: &str = r#"
effect fn main() -> Unit = {
  for n in [3, 1, 4, 1, 5] |> list.sort_by((x) => (if x > 2 then some(0 - x) else none)) {
    println("${n}")
  }
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Build and run `program` on both legs, asserting the observable agrees. The
/// wasm build must also come from the STRUCTURAL leg: an answer that agrees
/// only because the build silently fell back is the state this issue was.
fn agree_on_the_structural_leg(program: &str, label: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let mut native_stdout: Option<String> = None;
    for target in ["rust", "wasm"] {
        let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
        let built = Command::new(almide_bin())
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
            .expect("build");
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
                "{label}: the wasm build did not take the structural leg — it reported:\n{report}"
            );
        }
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
            Some(native) => assert_eq!(
                &stdout, native,
                "{label}: the wasm leg answered differently from native"
            ),
            None => native_stdout = Some(stdout),
        }
    }
    native_stdout.expect("one leg ran")
}

#[test]
fn a_tuple_sort_key_orders_the_same_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(TUPLE_KEY, "tuple key");
    assert_eq!(out, "fig 3\napple 2\npear 2\ndate 1\n", "count DESC then word ASC");
}

#[test]
fn a_nested_list_sort_key_orders_the_same_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(LIST_KEY, "list key");
    assert_eq!(out, "fig 3\npear 2\napple 2\ndate 1\n", "lexicographic by [-count, len]");
}

#[test]
fn an_option_sort_key_over_scalar_elements_orders_the_same_on_both_legs() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_structural_leg(OPTION_KEY_SCALAR_ELEM, "option key");
    assert_eq!(out, "1\n1\n5\n4\n3\n", "none < some, some by payload");
}

/// The kill-check. The incumbent leg has no compound-key route, and the
/// failure this issue reported was not that it lacked one — it was that it
/// rendered one anyway. Forcing that leg must now REFUSE at build time; a
/// success here means a `verified` artifact carrying the trap is reachable
/// again for any program the structural leg walls for some other reason.
#[test]
fn the_incumbent_leg_refuses_a_compound_key_instead_of_mis_rendering_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, TUPLE_KEY).expect("source");
    let artifact = dir.path().join("m.wasm");
    let built = Command::new(almide_bin())
        .args([
            "build",
            source.to_str().expect("path"),
            "--target",
            "wasm",
            "-o",
            artifact.to_str().expect("path"),
        ])
        .env("ALMIDE_WASM_INCUMBENT", "1")
        .env_remove("ALMIDE_COMPONENT_P3")
        .output()
        .expect("build");
    let report =
        String::from_utf8_lossy(&built.stdout).to_string() + &String::from_utf8_lossy(&built.stderr);
    assert!(
        !built.status.success(),
        "the incumbent leg built a compound-key sort instead of walling it:\n{report}"
    );
    assert!(
        report.contains("list.sort_by_x"),
        "the refusal must name the unrendered sort, not fail for some other reason:\n{report}"
    );
    assert!(
        !artifact.exists(),
        "a walled build must leave no artifact behind — {} exists",
        artifact.display()
    );
}
