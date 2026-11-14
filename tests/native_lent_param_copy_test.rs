//! #4661: a heap param read only through an accessor that returns it.
//!
//! `uuid.to_bytes(u)` is an erased newtype's accessor: its body returns its
//! parameter. Every call of it took the parameter over (a move param), up the
//! chain to the callback a `list.all` lends each element to, so the closure
//! dispatch copied every element per callback. The call is now the copy it
//! amounts to (`native_identity_calls.rs`), and a param whose one copy is only
//! lent stays borrowed (`render_native_borrow_read::lent_param_copy`).
//! Both targets print the same; the native text lends the element.
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

const SRC: &str = r#"import uuid

fn version_ok(u: uuid.Uuid) -> Bool = int.bshr(bytes.get(uuid.to_bytes(u), 6) ?? 0, 4) == 7

fn same(a: uuid.Uuid, b: uuid.Uuid) -> Bool = uuid.compare(a, b) == 0

effect fn main() -> Unit = {
  let us = [uuid.v7()!, uuid.v7()!, uuid.v7()!]
  println("${us |> list.all((u) => version_ok(u))} ${us |> list.all((u) => bytes.len(uuid.to_bytes(u)) == 16)}")
  println("${us |> list.all((u) => same(u, u))}")
}
"#;

#[test]
fn an_accessor_read_lends_the_element_and_prints_the_same_on_both_targets() {
    let dir = std::env::temp_dir().join(format!("almide-lent-param-copy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, SRC).unwrap();
    for target in [None, Some("wasm")] {
        let mut cmd = Command::new(almide());
        cmd.arg("run").arg(&src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
        if let Some(t) = target {
            cmd.args(["--target", t]);
        }
        let out = cmd.output().expect("almide");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "true true\ntrue\n", "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
    let out = Command::new(almide())
        .arg("build").arg(&src).arg("-o").arg(dir.join("prog"))
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail")
        .env("ALMIDE_RUN_PROJECT_DIR", dir.join("project"))
        .output()
        .expect("almide");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = std::fs::read_to_string(dir.join("project/src/main.rs")).expect("the generated program");
    let sig = rust.lines().find(|l| l.starts_with("fn almd_version_ok(")).expect("version_ok is rendered");
    assert!(sig.contains(": &"), "version_ok takes its Uuid by value: `{sig}`");
    assert!(!rust.contains("ToOwned::to_owned(a0)"), "a callback still copies the element it is lent");
    let _ = std::fs::remove_dir_all(&dir);
}
