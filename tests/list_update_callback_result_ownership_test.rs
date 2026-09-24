//! #2398: `list.update`'s callback result co-owns the handle it stores — the
//! two element shapes the corpus fixture cannot carry.
//!
//! `lower_list_update` lowered the callback body and stored the result into its
//! copy of the list with no ownership guard, so a BORROWED result — a body
//! whose tail merely reads a captured binding, `(t) => r5` — went into the slot
//! with no co-owning `+1`. The slot and the binding both believed they owned
//! one block, and a later `list.filter` that DROPPED that element released it:
//! the binding then read back freed memory, exit 0 on both legs, no trap. The
//! sibling callback-then-store lowerings (`list_flat.rs`, `collections_set.rs`,
//! `collections_hof.rs`) applied the guard already; this was the copy that had
//! to remember and did not.
//!
//! The matrix lives in `spec/wasm_cross/list_update_callback_result_ownership.almd`
//! (C-334). **Two of its fourteen cells are not there**, and they are here
//! instead: the incumbent brick refuses `list.update` with a `(String, Int)` or
//! an `Option[String]` element under a borrowed tail —
//! *"unliftable/closure-list higher-order argument … walled, not mis-valued"*.
//! Carrying them in the corpus would have meant raising both determinism wall
//! ceilings and the walled-real baseline, three shrink-only ratchets, to buy
//! coverage this file provides by building and running both legs directly. That
//! is the trade C-076's `EXTENDED (#2133)` clause records, for the same reason.
//!
//! Their FRESH twins render fine and ARE in the fixture; they are repeated here
//! as the negative control, because the guard must be conditional. An
//! unconditional `+1` would be the leak `rc_certainly_fresh`'s own doc warns
//! about — "over-inc on a fresh value is a leak, never a dangle" — and only a
//! pair of cells that differ in tail form alone can tell the two apart.

use std::process::Command;

/// Borrowed tail, `(String, Int)` element. Walls on the incumbent brick.
/// Before the guard, wasm printed `("", 2)` where native printed `("two", 2)`:
/// the tuple's String slot was the freed block.
const BORROWED_TUPLE: &str = r#"
fn main() -> Unit = {
  let r: (String, Int) = ("two", 2)
  let d = list.filter(list.update([("a", 1), ("b", 2)], 0, ((t) => r)), ((y) => false))
  println("${r} / ${int.to_string(list.len(d))}")
}
"#;

/// Borrowed tail, `Option[String]` element. Walls on the incumbent brick.
/// Before the guard, wasm printed `some("")` for `some("s")`.
const BORROWED_OPTION_STR: &str = r#"
fn main() -> Unit = {
  let r: Option[String] = some("s")
  let d = list.filter(list.update([some("p"), none], 0, ((t) => r)), ((y) => false))
  println("${r} / ${int.to_string(list.len(d))}")
}
"#;

/// The same two shapes with a CERTAINLY-FRESH tail (`Tuple`, `OptionSome`).
/// These were already correct before the guard and must stay correct after it:
/// they are the evidence that the guard does not fire on a value that owns
/// itself. A fixture built only from these would have been green against the
/// unpatched compiler and pinned nothing.
const FRESH_TUPLE: &str = r#"
fn main() -> Unit = {
  let s: String = int.to_string(6)
  let d = list.filter(list.update([("a", 1), ("b", 2)], 0, ((t) => (s, 1))), ((y) => false))
  println("${s} / ${int.to_string(list.len(d))}")
}
"#;

const FRESH_OPTION_STR: &str = r#"
fn main() -> Unit = {
  let s: String = int.to_string(7)
  let d = list.filter(list.update([some("p"), none], 0, ((t) => some(s))), ((y) => false))
  println("${s} / ${int.to_string(list.len(d))}")
}
"#;

/// The KEPT half: `filter` retains the element rather than dropping it, so the
/// guard's `+1` must not leave the binding over-counted. A leak is not visible
/// in stdout, but a double-release would be.
const BORROWED_TUPLE_KEPT: &str = r#"
fn main() -> Unit = {
  let r: (String, Int) = ("two", 2)
  let d = list.filter(list.update([("a", 1), ("b", 2)], 0, ((t) => r)), ((y) => true))
  println("${r} / ${int.to_string(list.len(d))}")
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Build one program on both legs, run the artifacts, and assert the two agree
/// — the observable this contract is about. The wasm leg runs under stock
/// `wasmtime`, not the embedded host, so nothing about the comparison depends
/// on our own runner.
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
fn a_borrowed_tuple_result_survives_the_element_being_dropped() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree(BORROWED_TUPLE, "borrowed (String,Int)").trim(), r#"("two", 2) / 0"#);
}

#[test]
fn a_borrowed_option_string_result_survives_the_element_being_dropped() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree(BORROWED_OPTION_STR, "borrowed Option[String]").trim(), r#"some("s") / 0"#);
}

#[test]
fn a_borrowed_tuple_result_survives_the_element_being_kept() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree(BORROWED_TUPLE_KEPT, "borrowed (String,Int) kept").trim(), r#"("two", 2) / 2"#);
}

#[test]
fn a_fresh_tail_is_left_alone() {
    if !wasmtime_available() {
        return;
    }
    // Both were already correct before the guard. They are here so that a
    // change making the guard unconditional — a leak rather than a dangle —
    // is still visible as a behavioural pair rather than only as a ratchet.
    assert_eq!(agree(FRESH_TUPLE, "fresh (String,Int)").trim(), "6 / 0");
    assert_eq!(agree(FRESH_OPTION_STR, "fresh Option[String]").trim(), "7 / 0");
}
