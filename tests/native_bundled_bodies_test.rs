//! Primary stdlib bodies must execute with their checked module scope and
//! mutation ABI, and contribute witnesses just like ordinary user functions.

use std::process::{Command, Output};

fn observation(out: Output) -> (Vec<u8>, Vec<u8>, Option<i32>) {
    (out.stdout, out.stderr, out.status.code())
}

#[test]
fn linked_bodies_preserve_values_scope_and_failure_behavior() {
    let cases = [
        ("codepoint", r#"
fn main() -> Unit = {
  println(string.from_codepoint(65) ?? "bad")
  println(string.from_codepoint(129408) ?? "bad")
  println(string.from_codepoint(55296) ?? "none")
  println(string.from_codepoint(4294967296) ?? "none")
  println(string.from_codepoint(-1) ?? "none")
}
"#, "A\n🦀\nnone\nnone\nnone\n", true),
        ("reserved_name_scope", r#"
fn almide_rt_string_from_codepoint(n: Int) -> Int = n + 7
fn main() -> Unit = {
  println(int.to_string(almide_rt_string_from_codepoint(2)))
  println(string.from_codepoint(65) ?? "bad")
}
"#, "9\nA\n", true),
        ("bytes_alias_and_scope", r#"
fn fits(b: Bytes, pos: Int, w: Int) -> Bool = false
fn main() -> Unit = {
  var b = bytes.from_list([1, 2, 3])
  let kept = b
  bytes.put_u8(b, 1, 259)
  println(int.to_string(bytes.get(b, 1) ?? -1))
  println(int.to_string(bytes.get(kept, 1) ?? -1))
  println(if fits(b, 1, 1) then "wrong" else "own")
}
"#, "3\n2\nown\n", true),
        ("bytes_overlapping_copy", r#"
fn main() -> Unit = {
  var b = bytes.from_list([1, 2, 3, 4])
  let original = b
  bytes.copy_from(b, b, 1, 0, 3)
  bytes.put_u8(b, 0, 9)
  println("${bytes.to_list(b)}")
  println("${bytes.to_list(original)}")
  bytes.copy_from(b, b, 0, 1, 3)
  println("${bytes.to_list(b)}")
}
"#, "[9, 1, 2, 3]\n[1, 2, 3, 4]\n[1, 2, 3, 3]\n", true),
        ("generic_numeric", r#"
fn main() -> Unit = {
  println(int.to_string(int.from(3.75)))
  println(int.to_string(int.from(-3.75)))
  println(int.to_string(int.from(7)))
}
"#, "3\n-3\n7\n", true),
        ("numeric_range_abort", r#"
fn main() -> Unit = {
  println("before")
  println(int.to_string(int.from(1e300)))
  println("after")
}
"#, "before\n", false),
        ("bytes_bounds", r#"
fn main() -> Unit = {
  var b = bytes.from_list([1, 2, 3])
  println("before")
  bytes.put_u8(b, -1, 4)
  println("after")
}
"#, "before\n", false),
    ];
    let dir = std::env::temp_dir().join(format!("almide-native-bodies-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, stdout, success) in cases {
        let file = dir.join(format!("{name}.almd"));
        let rs = dir.join(format!("{name}.rs"));
        let binary = dir.join(name);
        std::fs::write(&file, source).unwrap();
        let (rust, certs) = almide_mir::pipeline::try_render_rust_source_certified(source)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(certs.iter().any(|(prop, f, _)| prop == "ownership" && f.starts_with("almide_rt_")),
            "{name}: the linked body needs its own ownership witness");
        std::fs::write(&rs, rust).unwrap();
        let compiled = Command::new("rustc").args(["--edition=2021", "-O"])
            .arg(&rs).arg("-o").arg(&binary).output().unwrap();
        assert!(compiled.status.success(), "{name}: {}", String::from_utf8_lossy(&compiled.stderr));
        let actual = Command::new(&binary).output().unwrap();
        assert_eq!(actual.status.success(), success, "{name}");
        assert_eq!(actual.stdout, stdout.as_bytes(), "{name}");
        let oracle_binary = dir.join(format!("{name}-v0"));
        let built = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(["build", "--no-verified"]).arg(&file).arg("-o").arg(&oracle_binary)
            .env("ALMIDE_NO_VERIFIED_OK", "1")
            .output().unwrap();
        assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
        let oracle = Command::new(oracle_binary).output().unwrap();
        assert_eq!(observation(actual), observation(oracle), "{name}: raw v1/v0 observations");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
