//! #1296: a native-runtime call returning `Result[Bytes, String]` unwrapped
//! with `!` directly in another call's ARGUMENT position emitted invalid Rust
//! (the #617 RcCow glue wrapped the Ok side BEFORE `?`, so rustc back-inferred
//! the `?` source from the consuming `&Vec<u8>` parameter and E0308'd). The
//! unwrap now emits raw-call → `?` → glue, concretely typed end to end. Both
//! measured family members (zlib and fs) are pinned end-to-end; the bind-first
//! spelling stays the control.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run(dir: &std::path::Path, name: &str, source: &str) -> (String, String, bool) {
    let file = dir.join(name);
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("run")
        .arg(&file)
        .output()
        .expect("run almide");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

#[test]
fn zlib_bytes_unwrap_in_call_arg_compiles_and_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (stdout, stderr, ok) = run(
        dir.path(),
        "z.almd",
        "import zlib\n\neffect fn main() -> Unit = {\n  let data = bytes.from_list([1])\n  let n = bytes.len(zlib.compress_level(data, 9)!)\n  println(\"${n}\")\n}\n",
    );
    assert!(ok, "in-arg unwrap must compile and run, stderr:\n{stderr}");
    assert_eq!(stdout.trim(), "9", "compressed length of [1] at level 9");
}

#[test]
fn fs_bytes_unwrap_in_call_arg_matches_bind_first() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("payload.bin");
    let path_str = path.to_string_lossy().replace('\\', "/");
    let inline = run(
        dir.path(),
        "a.almd",
        &format!("import fs\n\neffect fn main() -> Unit = {{\n  fs.write(\"{path_str}\", \"hello\")!\n  let n = bytes.len(fs.read_bytes_raw(\"{path_str}\")!)\n  println(\"${{n}}\")\n}}\n"),
    );
    let bound = run(
        dir.path(),
        "b.almd",
        &format!("import fs\n\neffect fn main() -> Unit = {{\n  fs.write(\"{path_str}\", \"hello\")!\n  let raw = fs.read_bytes_raw(\"{path_str}\")!\n  let n = bytes.len(raw)\n  println(\"${{n}}\")\n}}\n"),
    );
    assert!(inline.2, "in-arg unwrap must run, stderr:\n{}", inline.1);
    assert!(bound.2, "bind-first control must run, stderr:\n{}", bound.1);
    assert_eq!(inline.0, bound.0, "in-arg and bind-first must agree");
    assert_eq!(inline.0.trim(), "5");
}
