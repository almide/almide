//! #2133: a record-shaped variant CASE co-owns the handles it stores, like
//! every other block.
//!
//! `lower_named_record`'s variant branch built `Supported { matched: checked }`
//! by lowering each field and storing it — without the RC-3 share guard the
//! record branch ten lines below has always applied. A `let`-bound list moved
//! into a case payload was therefore stored with no co-owning `+1`, and the
//! frame epilogue, which still owned the binding, freed it on the way out. The
//! case held a dangling block.
//!
//! It reached a user as `Error: out of memory` on a 273-byte input (#2133),
//! then as `wasm trap: out of bounds memory access` once the surrounding wall
//! lifted — but the reduction below shows what it really was: a SILENT WRONG
//! ANSWER. The wasm leg printed bytes of freed memory where native printed the
//! elements, exit 0 on both. That is the class a differential gate exists for,
//! so the fixture asserts the two legs agree rather than that either survives.
//!
//! The tuple-shaped constructor (`lower_variant_ctor`) never had the bug — it
//! carried `rc_share_guard` from the start — which is why this hid behind a
//! spelling rather than a feature.

use std::process::Command;

/// The reduction, 13 lines: `of` returns a fresh `List[Probe]`, `verdict_of`
/// binds it and moves the binding into a case payload, and `tags` reads the
/// payload's element fields after the producer frame is gone.
const LET_BOUND_LIST_PAYLOAD: &str = r#"
type Probe = { kind: String }
type Verdict = | Supported{ matched: List[Probe] } | Nothing

fn of(xs: List[String]) -> List[Probe] = list.map(xs, (f) => Probe { kind: f })

fn verdict_of(xs: List[String]) -> Verdict = {
  let checked = of(xs)
  Supported { matched: checked }
}

fn tags(v: Verdict) -> List[String] =
  match v { Supported{ matched } => list.map(matched, (p) => p.kind), Nothing => [] }

effect fn main() -> Unit = println(list.join(tags(verdict_of(["a", "b"])), ","))
"#;

/// The same move through a DEFAULT field rather than a written one — the
/// second store loop in the same branch, which had the same omission.
const DEFAULTED_CASE_FIELD: &str = r#"
type Probe = { kind: String }
type Verdict = | Supported{ matched: List[Probe], source: String = "none" } | Nothing

fn of(xs: List[String]) -> List[Probe] = list.map(xs, (f) => Probe { kind: f })

fn verdict_of(xs: List[String]) -> Verdict = {
  let checked = of(xs)
  Supported { matched: checked }
}

fn tags(v: Verdict) -> List[String] =
  match v {
    Supported{ matched, source } => list.map(matched, (p) => p.kind + "/" + source),
    Nothing => [],
  }

effect fn main() -> Unit = println(list.join(tags(verdict_of(["a", "b"])), ","))
"#;

/// The shape the reduction came from: the case is stored in a record, the
/// record in a list built by a loop, and the payload read per element well
/// after every producer frame has returned.
const THROUGH_A_LIST_OF_RECORDS: &str = r#"
type Probe = { kind: String, value: String }
type Verdict = | Supported{ matched: List[Probe], source: String } | Nothing
type Judged = { id: String, verdict: Verdict }

fn of(xs: List[String]) -> List[Probe] =
  list.map(list.filter(xs, (f) => f != ""), (f) => Probe { kind: "figure", value: f })
    + list.map(list.filter(xs, (q) => q != ""), (q) => Probe { kind: "quote", value: q })

fn verdict_of(xs: List[String]) -> Verdict = {
  let checked = of(xs)
  match list.len(checked) > 0 {
    true => Supported { matched: checked, source: "https://a" },
    false => Nothing,
  }
}

fn tags(j: Judged) -> List[String] =
  match j.verdict {
    Supported{ matched, source } => list.map(matched, (p) => p.kind + ":" + p.value),
    Nothing => [],
  }

effect fn main() -> Unit = {
  var judged: List[Judged] = []
  for i in 0..<2 {
    list.push(judged, Judged { id: "c" + int.to_string(i), verdict: verdict_of(["5148.1", "53.3"]) })
  }
  println(list.join(list.flat_map(judged, (j) => tags(j)), ","))
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Run one program on both legs and return the native stdout, asserting the
/// two agree — the observable this contract is about.
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
        assert!(built.status.success(), "{label}/{target} build:\n{}", String::from_utf8_lossy(&built.stderr));
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
            Some(native) => assert_eq!(
                &stdout, native,
                "{label}: the wasm leg answered differently from native"
            ),
            None => first = Some(stdout),
        }
    }
    first.expect("one leg ran")
}

#[test]
fn a_let_bound_list_moved_into_a_case_payload_survives_the_producer_frame() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(agree(LET_BOUND_LIST_PAYLOAD, "let-bound").trim(), "a,b");
}

#[test]
fn a_defaulted_case_field_takes_the_same_guard() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(agree(DEFAULTED_CASE_FIELD, "defaulted").trim(), "a/none,b/none");
}

#[test]
fn the_payload_survives_a_list_of_records_built_by_a_loop() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(
        agree(THROUGH_A_LIST_OF_RECORDS, "through-records").trim(),
        "figure:5148.1,figure:53.3,quote:5148.1,quote:53.3,\
         figure:5148.1,figure:53.3,quote:5148.1,quote:53.3"
    );
}
