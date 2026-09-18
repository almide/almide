//! #2098 mechanism 2, native leg: `list.enumerate` in SOURCE position is an
//! adapter over a chain, not a list to build.
//!
//! `list.fold(list.enumerate(v), …)` used to render
//! `almide_rt_list_enumerate(v.clone()).into_iter().fold(…)` — a full copy of
//! `v` plus a `Vec<(i64, T)>` the fold immediately walked and dropped. In
//! spectralnorm's enumerate spelling that is 48,000 copies of a 1,200-element
//! vector per run; the spelling `CHEATSHEET.md` names explicitly measured
//! 1.68x the imperative form it tells writers to avoid.
//!
//! It now renders `(v).iter().cloned().enumerate().map(…)` — the same pairs in
//! the same order, from a borrow, with nothing in between (measured 1.01x).
//! The clone is dropped because fusing the consuming runtime call away removes
//! the only consumer it was inserted for; a callback that MUTATES the source
//! keeps it, because that is the one way the borrow could observe a change the
//! snapshot must hide.

use std::process::Command;

fn emit_rust(dir: &std::path::Path, program: &str) -> String {
    let source = dir.join("emit.almd");
    std::fs::write(&source, program).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args([source.to_str().expect("path"), "--target", "rust"])
        .output()
        .expect("emit");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The body that carries the shape, with the enumerate source captured by the
/// outer `map`'s closure — spectralnorm's inner loop, minus the arithmetic.
const CAPTURED: &str = r#"
fn rows(v: List[Float], n: Int) -> List[Float] =
  list.map(0..<n, (i) => list.fold(list.enumerate(v), 0.0, (acc, p) => acc + float.from_int(p.0 + i) * p.1))

fn main() -> Unit = println(float.to_fixed(list.fold(rows([1.0, 2.0, 3.0], 3), 0.0, (a, x) => a + x), 3))
"#;

#[test]
fn a_captured_enumerate_source_streams_from_a_borrow() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rust = emit_rust(dir.path(), CAPTURED);
    // The prelude defines `almide_rt_list_enumerate` and uses `.enumerate()`
    // in its own `Vec` helpers, so read the emitted function, not the file.
    let body = emitted_fn(&rust, "rows");
    assert!(
        body.contains(".iter().cloned().enumerate()"),
        "the source is still consumed rather than borrowed:\n{body}"
    );
    assert!(
        !body.contains("almide_rt_list_enumerate"),
        "the tuple list is still materialized:\n{body}"
    );
}

/// The emitted body of one function, from its signature to the closing brace
/// in column 0.
fn emitted_fn(rust: &str, name: &str) -> String {
    let head = format!("fn {name}(");
    let mut lines = rust.lines().skip_while(|l| !l.contains(&head));
    let first = lines.next().unwrap_or_else(|| panic!("no fn {name} in the emit:\n{rust}"));
    std::iter::once(first)
        .chain(lines.take_while(|l| *l != "}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The snapshot rule the runtime twin gives: `list.enumerate` reads the list
/// ONCE, so a callback that writes to it during the walk sees its own value
/// afterwards and the fold still sums what was there at the start. A source a
/// callback can write to is a shared cell, and the chain reads it through the
/// cell's snapshot (`.get()`), never through a live `.borrow()` — which is
/// what keeps the adapter's borrow from observing the write.
const MUTATED: &str = r#"
fn main() -> Unit = {
  var xs = [1, 2, 3]
  let total = list.fold(list.enumerate(xs), 0, (a, p) => {
    xs[2] = 99
    a + p.1
  })
  println("${total} ${xs[2]}")
}
"#;

#[test]
fn a_callback_that_writes_to_the_source_keeps_its_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let body = emitted_fn(&emit_rust(dir.path(), MUTATED), "__almide_main");
    assert!(
        body.contains("(xs.get()).iter().cloned().enumerate()"),
        "the chain no longer reads the cell's snapshot:\n{body}"
    );
    assert!(
        !body.contains("xs.borrow()).iter()"),
        "the chain walks the live cell the callback writes to:\n{body}"
    );
}

fn run_both_legs(program: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let mut expected: Option<String> = None;
    for target in ["rust", "wasm"] {
        let artifact = dir.path().join(if target == "rust" { "native" } else { "module.wasm" });
        let built = Command::new(env!("CARGO_BIN_EXE_almide"))
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
        assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
        let mut command = if target == "rust" {
            Command::new(&artifact)
        } else {
            let mut c = Command::new("wasmtime");
            c.arg("run").arg(&artifact);
            c
        };
        let out = command.output().expect("run");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let actual = String::from_utf8_lossy(&out.stdout).to_string();
        match &expected {
            Some(first) => assert_eq!(&actual, first, "{target} disagrees with the first leg"),
            None => expected = Some(actual),
        }
    }
    expected.expect("one leg ran")
}

#[test]
fn both_legs_agree_on_the_captured_and_mutated_shapes() {
    assert_eq!(run_both_legs(CAPTURED).trim(), "42.000");
    assert_eq!(run_both_legs(MUTATED).trim(), "6 99");
}

/// The adapter composes: `enumerate` under a `map`, under a `filter`, as the
/// source of a `take`, and OVER a chain rather than over a list — every chain
/// position where a step list is built, including the reducer's, where the
/// adapter has to survive the collector swap.
const COMPOSED: &str = r#"
fn main() -> Unit = {
  let xs = [10, 20, 30, 40]
  let doubled = list.map(list.enumerate(xs), (p) => p.0 * 100 + p.1)
  let kept = list.filter(list.enumerate(xs), (p) => p.0 % 2 == 0)
  let head = list.take(list.enumerate(xs), 2)
  let counted = list.count(list.enumerate(xs), (p) => p.1 > 15)
  let over_chain = list.len(list.enumerate(list.filter(xs, (x) => x > 15)))
  println("${doubled} ${kept} ${head} ${counted} ${over_chain}")
}
"#;

#[test]
fn the_adapter_composes_with_every_chain_position() {
    assert_eq!(
        run_both_legs(COMPOSED).trim(),
        "[10, 120, 230, 340] [(0, 10), (2, 30)] [(0, 10), (1, 20)] 3 3"
    );
}
