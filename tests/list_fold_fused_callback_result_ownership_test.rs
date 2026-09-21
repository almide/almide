//! #2397: the FUSED `list.fold` lowering owns its accumulator's credit the way
//! the staged one has since 10fed0494.
//!
//! `list.fold` has two lowerings on the structural wasm leg. `lower_list_fold`
//! (the staged copy) applies `rc_share_guard` to the callback body and releases
//! the previous accumulator before the rebind. `lower_list_fold_fused` — the
//! copy that runs when the source is a `list.map` / `list.filter` chain — was
//! written before that rule and never received it. A borrowed result, the
//! element itself in `(n, y) => y`, therefore became the accumulator with no
//! co-owning `+1`, and the list's own release freed it under the caller: a
//! wrong byte on wasm, no trap, exit 0, and only through a chain that fuses
//! (bind the chain to a `let` and the staged copy runs and is right).
//!
//! The matrix lives in `spec/wasm_cross/list_fold_fused_callback_result_ownership.almd`
//! (C-334). This file carries the two things the corpus fixture cannot say:
//!
//! * that the four regression cells DIVERGED on the previous release — a green
//!   fixture proves nothing about what it catches (the fixture is run here on
//!   both legs and must agree; the A/B against 0.62.0 is recorded in the
//!   issue and in the PR, since a test cannot depend on a released binary);
//! * that the guard is CONDITIONAL: the fresh-tail cells are repeated as a pair
//!   differing in tail form alone, because an unconditional `+1` would be the
//!   leak `rc_certainly_fresh`'s own doc warns about — "over-inc on a fresh
//!   value is a leak, never a dangle" — and the ratchets are the only other
//!   place that would show it.

use std::process::Command;

/// Borrowed tail through a `filter` stage, String element. Before the guard,
/// wasm printed a space (0x20) where native printed `B`.
const BORROWED_STR_FILTER: &str = r#"
fn main() -> Unit = {
  let r: String = list.fold(list.filter(string.chars("AB"), ((a) => true)), "X", ((n, y) => y))
  println("r = ${r}")
}
"#;

/// Borrowed tail through a `map` stage — no `filter` at all. The issue's
/// table did not have this cell; the fused path is one code path and this
/// pins that the fix is on the path, not on `filter`.
const BORROWED_STR_MAP: &str = r#"
fn main() -> Unit = {
  let r: String = list.fold(list.map(string.chars("AB"), ((c) => c)), "X", ((n, y) => y))
  println("r = ${r}")
}
"#;

/// Borrowed tail, `List[Int]` element: not a `string.chars` property. Before
/// the guard, wasm printed `[65888]` for `[2]`.
const BORROWED_LIST_INT_FILTER: &str = r#"
fn main() -> Unit = {
  let r: List[Int] = list.fold(list.filter([[1], [2]], ((a) => true)), [0], ((n, y) => y))
  println("r = ${r}")
}
"#;

/// The same three shapes with a CERTAINLY-FRESH tail. Correct before the
/// guard and must stay correct after it.
const FRESH_STR_FILTER: &str = r#"
fn main() -> Unit = {
  let r: String = list.fold(list.filter(string.chars("AB"), ((a) => true)), "X", ((n, y) => y + ""))
  println("r = ${r}")
}
"#;

const FRESH_LIST_INT_FILTER: &str = r#"
fn main() -> Unit = {
  let r: List[Int] = list.fold(list.filter([[1], [2]], ((a) => true)), [0], ((n, y) => y + [9]))
  println("r = ${r}")
}
"#;

/// A fresh ELEMENT (the map stage builds it) returned borrowed by the fold
/// callback: the guard takes a share of a block only the loop held, which is a
/// leak at worst, never a dangle. The observable must be unchanged.
const FRESH_ELEM_MAP: &str = r#"
fn main() -> Unit = {
  let r: String = list.fold(list.map(string.chars("AB"), ((c) => c + "!")), "X", ((n, y) => y))
  println("r = ${r}")
}
"#;

/// Many elements through the fused path with a String accumulator that grows:
/// a leak would not show in stdout, a double release or a freed read would.
const LONG_CHAIN: &str = r#"
fn main() -> Unit = {
  let s: String = string.repeat("abcdefgh", 512)
  let r: String = list.fold(list.filter(string.chars(s), ((a) => a != "c")), "", ((n, y) => n + y))
  println("${string.len(r)} ${string.slice(r, string.len(r) - 1, string.len(r))}")
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Build one program on both legs, run the artifacts under stock `wasmtime`
/// (not the embedded host), and assert the two agree.
fn agree(program: &str, label: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let mut first: Option<String> = None;
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
            "{label}/{target} build:\n{}",
            String::from_utf8_lossy(&built.stderr)
        );
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
            "{label}/{target} exited {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        match &first {
            Some(native) => {
                assert_eq!(&stdout, native, "{label}: the wasm leg answered differently from native")
            }
            None => first = Some(stdout),
        }
    }
    first.expect("native ran")
}

#[test]
fn a_borrowed_element_survives_the_fused_filter_chain() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree(BORROWED_STR_FILTER, "borrowed String, filter").trim(), "r = B");
    assert_eq!(agree(BORROWED_LIST_INT_FILTER, "borrowed List[Int], filter").trim(), "r = [2]");
}

#[test]
fn a_borrowed_element_survives_the_fused_map_chain() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree(BORROWED_STR_MAP, "borrowed String, map").trim(), "r = B");
}

#[test]
fn a_fresh_tail_is_left_alone() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree(FRESH_STR_FILTER, "fresh String, filter").trim(), "r = B");
    assert_eq!(agree(FRESH_LIST_INT_FILTER, "fresh List[Int], filter").trim(), "r = [2, 9]");
    assert_eq!(agree(FRESH_ELEM_MAP, "fresh element, map").trim(), "r = B!");
}

#[test]
fn a_long_fused_chain_with_a_growing_accumulator_agrees() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree(LONG_CHAIN, "long chain").trim(), "3584 h");
}
