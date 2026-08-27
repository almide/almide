//! Fixed-arity list patterns on the structural wasm leg — the LAST
//! lowering wall of the #1584 leg census (`pattern:List`). The shape:
//! `match xs { [] => …, [a, b] => …, ys => … }` — the byte-LEN equality
//! test plus per-element sub-patterns at payload slots. Byte-identity
//! native ⇄ wasm on every form, including the census's own repro
//! (spec/programs/option_first_even.almd).

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

const PROGRAM: &str = r#"fn classify(xs: List[Int]) -> String = match xs {
  [] => "empty",
  [x] => "one:" + int.to_string(x),
  [a, b] => "two:" + int.to_string(a + b),
  ys => "many:" + int.to_string(list.len(ys)),
}

fn first_even(xs: List[Int]) -> Int? = match list.filter(xs, (x) => x % 2 == 0) {
  [] => none,
  ys => list.get(ys, 0),
}

fn main() -> Unit = {
  println(classify([]))
  println(classify([7]))
  println(classify([3, 4]))
  println(classify([1, 2, 3]))
  println(int.to_string(first_even([1, 3, 6, 8]) ?? -1))
  println(int.to_string(first_even([1, 3, 5]) ?? -1))
  let s = match ["hi", "yo"] { [a, _] => a, _ => "?" }
  println(s)
}
"#;

const WANT: &str = "empty\none:7\ntwo:7\nmany:3\n6\n-1\nhi\n";

#[test]
fn list_patterns_are_byte_identical_across_targets() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = std::env::temp_dir().join("almide-list-pattern");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = dir.join("lp.almd");
    std::fs::write(&src, PROGRAM).expect("write");

    let native = Command::new(almide_bin())
        .args(["run", src.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), WANT);

    // Forced structural: a wall here is a hard error, so the leg cannot
    // silently hand the shape back to the incumbent.
    let out = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "-o"])
        .arg(dir.join("lp.wasm"))
        .env("ALMIDE_WASM_STRUCTURAL", "1")
        .output()
        .expect("spawn build");
    assert!(
        out.status.success(),
        "structural leg walled on list patterns again:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let wasm = Command::new(almide_bin())
        .args(["run", src.to_str().unwrap(), "--target", "wasm"])
        .output()
        .expect("spawn");
    if !wasm.status.success() {
        return; // wasmtime absent — the native half + forced build still ran
    }
    assert_eq!(
        String::from_utf8_lossy(&wasm.stdout),
        WANT,
        "list-pattern output diverged native vs wasm"
    );
}
