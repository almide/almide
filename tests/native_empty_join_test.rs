//! An empty list literal joined into a list of strings on the certified
//! native render (#4006): `[]` lowers to an empty int list whatever its
//! element, and an if-value whose other arm yields a `List[String]` takes the
//! empty list of the join's type (crates/almide-mir/src/
//! render_native_empty_join.rs). Each pin compares v1's output with v0's.

use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almd_native_empty_join_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// stdout + exit code of the v0 native pipeline (the oracle).
fn run_v0(name: &str, src: &str) -> (String, i32) {
    let file = scratch(name).join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", "--no-verified", file.to_str().unwrap()])
        .env("ALMIDE_NO_VERIFIED_OK", "1")
        .output()
        .expect("almide run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

/// The v1 render, and its stdout + exit code once rustc compiled it.
fn run_v1(name: &str, src: &str) -> (String, String, i32) {
    let rust = almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("v1 native walled on `{name}`: {e:?}"));
    let dir = scratch(name);
    let (rs, bin) = (dir.join("prog.rs"), dir.join("prog_bin"));
    std::fs::write(&rs, &rust).unwrap();
    let rc = Command::new("rustc").args(["-O", "-o", bin.to_str().unwrap(), rs.to_str().unwrap()]).output().expect("rustc");
    assert!(rc.status.success(), "rustc rejected `{name}`:\n{}", String::from_utf8_lossy(&rc.stderr));
    let out = Command::new(&bin).output().expect("run v1 binary");
    (rust, String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

#[test]
fn an_empty_arm_joins_a_string_list() {
    let src = "type P = | Pair(String, String) | Solo\n\n\
               fn names(b: P) -> List[String] = match b {\n  Pair(a, c) => [a, c],\n  _ => [],\n}\n\n\
               effect fn main() -> Unit = {\n  \
                 println(list.join(names(Pair(\"m\", \"n\")), \",\"))\n  \
                 println(int.to_string(list.len(names(Solo))))\n\
               }\n";
    let (rust, out, code) = run_v1("join", src);
    assert!(rust.contains("= Vec::new();"), "the empty arm is not the join's empty list:\n{rust}");
    assert_eq!((out, code), run_v0("join", src), "v1 native differs from v0");
}
