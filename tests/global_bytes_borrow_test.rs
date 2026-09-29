//! A module global holding Bytes (or a container of them) passed to a
//! user fn.
//!
//! A global stores the RAW `Vec<u8>` shape (#617) and a bare read of it stays
//! raw — what a runtime callee's `&Vec<u8>` parameter takes, but not a user
//! fn's `&AlmideRcCow<Vec<u8>>`: `f(global)` failed with rustc E0308, natively
//! only. A borrow of such a global now takes the glued value, which derefs to
//! the raw shape for runtime callees as well.

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

const SRC: &str = r#"
var pending: Bytes = bytes.new(0)
var chunks: List[Bytes] = []
var last: Bytes? = none

fn size(b: Bytes) -> Int = bytes.len(b)
fn total(bs: List[Bytes]) -> Int = bs |> list.fold(0, (acc, b) => acc + bytes.len(b))
fn maybe(b: Bytes?) -> Int = match b { some(x) => bytes.len(x), none => 0 - 1 }

effect fn main() -> Unit = {
  pending = bytes.from_string("abc")
  chunks = [bytes.from_string("de"), bytes.from_string("fgh")]
  println("${int.to_string(size(pending))} ${int.to_string(bytes.len(pending))}")
  println("${int.to_string(total(chunks))} ${int.to_string(maybe(last))}")
  last = some(pending)
  println(int.to_string(maybe(last)))
  // Still the global's own value: nothing above moved or changed it.
  pending = bytes.concat(pending, bytes.from_string("!"))
  println("${int.to_string(size(pending))} ${bytes.to_string_lossy(pending)}")
}
"#;

fn run(target: Option<&str>) -> String {
    let dir = std::env::temp_dir().join(format!("almide-global-bytes-{}", target.unwrap_or("native")));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("main.almd");
    std::fs::write(&file, SRC).unwrap();
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&file);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

const WANT: &str = "3 3\n5 -1\n3\n4 abc!\n";

#[test]
fn global_bytes_passed_to_user_fns_natively() {
    let out = run(None);
    assert!(out.contains(WANT), "native:\n{out}");
}

#[test]
fn global_bytes_passed_to_user_fns_on_wasm() {
    let out = run(Some("wasm"));
    assert!(out.contains(WANT), "wasm:\n{out}");
}
