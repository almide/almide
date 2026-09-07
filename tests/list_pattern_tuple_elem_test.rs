//! #1934: a list pattern whose element is a TUPLE pattern (`[(name, n),
//! ..rest]`) bound nothing for `name` / `n` on the native leg — the
//! list-pattern lowering handled only a plain bind per element, so rustc
//! saw free names behind a green check. The structural leg lowered it all
//! along. Kept OUT of the spec corpus on purpose: the incumbent brick walls
//! every list-rest pattern (a COMPILER_FRONTIER row retiring with #1584),
//! and a corpus function it walls is a walled-real baseline entry — this
//! runs both legs directly instead of growing that ledger.

use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

const PROGRAM: &str = r#"fn first_len(xs: List[(String, Int)]) -> Int = match xs {
  [] => 0,
  [(name, n), ..rest] => list.len(rest |> list.filter((p) => string.starts_with(p.0, name))) + n + string.len(name),
}

fn head_pair(xs: List[(String, Int)]) -> String = match xs {
  [(a, _), (b, m)] => "${a}/${b}:${m}",
  [(only, _)] => only,
  _ => "-",
}

fn nested(xs: List[((Int, Int), String)]) -> Int = match xs {
  [((x, y), _), ..] => x * 10 + y,
  [] => -1,
}

fn main() -> Unit = {
  println(int.to_string(first_len([("ab", 1), ("abc", 2), ("x", 3)])))
  println(int.to_string(first_len([])))
  println(head_pair([("p", 1), ("q", 2)]))
  println(head_pair([("solo", 9)]))
  println(head_pair([]))
  println(int.to_string(nested([((4, 2), "z")])))
  println(int.to_string(nested([])))
}
"#;

const EXPECTED: &str = "4\n0\np/q:2\nsolo\n-\n42\n-1\n";

fn run(target: Option<&str>) -> (i32, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("tuple_elems.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&src);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide run");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn tuple_elements_of_list_patterns_bind_on_the_native_leg() {
    let (code, stdout, stderr) = run(None);
    assert_eq!(code, 0, "native run failed; stderr:\n{stderr}");
    assert_eq!(stdout, EXPECTED);
}

#[test]
fn tuple_elements_of_list_patterns_bind_on_the_wasm_leg() {
    let (code, stdout, stderr) = run(Some("wasm"));
    if stderr.contains("wasmtime") && code != 0 && stdout.is_empty() {
        eprintln!("skipping wasm leg: no wasm host here\n{stderr}");
        return;
    }
    assert_eq!(code, 0, "wasm run failed; stderr:\n{stderr}");
    assert_eq!(stdout, EXPECTED);
}
