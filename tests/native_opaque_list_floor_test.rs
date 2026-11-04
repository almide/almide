//! A list of lists, records or variants (a generated `Vec<X>` on the
//! certified native render) handed to a generic floor body that never sees an
//! element — `list.len`, `list.is_empty` (#4006,
//! native_floor.rs `opaque_list_arg`). Each pin compares v1's output with v0's.

use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almd_native_opaque_list_{name}_{}", std::process::id()));
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
fn the_length_of_a_nested_list_reads_as_v0_does() {
    let src = "type S = | A(Int) | B\n\n\
               effect fn main() -> Unit = {\n  \
                 let xss = [[1, 2], [3], []]\n  \
                 let ss = [A(1), B, A(2), B]\n  \
                 println(int.to_string(list.len(xss)))\n  \
                 println(int.to_string(list.len(ss)))\n  \
                 println(if list.is_empty(xss) then \"empty\" else \"some\")\n\
               }\n";
    let (rust, out, code) = run_v1("len", src);
    assert!(rust.contains("almide_rt_list_len("), "list.len is not the floor body:\n{rust}");
    assert_eq!((out, code), run_v0("len", src), "v1 native differs from v0");
}
