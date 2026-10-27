//! A user type that shares its name with a stdlib-owned type derives `Codec`
//! as its own type (#4273).
//!
//! `secret` brought a generic `Secret[A]` into the stdlib. A user's
//! `type Secret: Codec = { .. }` then went wrong in two places:
//!
//! - in a package module, the derive was also recorded under the bare name
//!   `Secret` — the stdlib's generic type — so the derive check reported
//!   E023 "is generic" (5,837 of them in teastia);
//! - in a single file, the derived `decode` built a bare `Secret { .. }`
//!   while the type is declared under its shadow scope `self.Secret`
//!   (#1828), and rustc failed with E0422.
//!
//! Nothing here is specific to `Secret`: the other stdlib-owned names
//! (`Regex`, `FileStat`) take the same path, a record and a variant.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4273-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    root
}

fn run_both_targets(dir: &Path, file: &str, expected: &str) {
    for target in ["rust", "wasm"] {
        let out = Command::new(almide())
            .args(["run", file, "--target", target])
            .current_dir(dir)
            .output()
            .expect("spawn almide");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{target} run failed:\nstdout: {stdout}\nstderr: {stderr}");
        assert_eq!(stdout, expected, "wrong {target} output; stderr: {stderr}");
    }
}

const SINGLE_FILE: &str = r#"type Secret: Codec = { name: String, file: String? = none }
type Regex: Codec = { pattern: String }
type FileStat: Codec = | Small(Int) | Big { size: Int }

effect fn main() -> Unit = {
  let s = Secret { name: "a" }
  let back = Secret.decode(s.encode())!
  println(back.name)
  println(back.file ?? "none")
  let r = Regex { pattern: "x+" }
  println(Regex.decode(r.encode())!.pattern)
  let f: FileStat = Big { size: 3 }
  match FileStat.decode(f.encode())! {
    Big { size } => println(int.to_string(size)),
    Small(n) => println("small ${n}"),
  }
}
"#;

#[test]
fn a_stdlib_type_name_derives_codec_in_a_single_file() {
    let dir = scratch("single");
    write(&dir.join("main.almd"), SINGLE_FILE);
    run_both_targets(&dir, "main.almd", "a\nnone\nx+\n3\n");
}

const IR_MODULE: &str = r#"type Secret: Codec = { kind: String, name: String, file: String? = none }

fn mk(n: String) -> Secret = Secret { kind: "env", name: n }
"#;

const PKG_MAIN: &str = r#"import self.ir

effect fn main() -> Unit = {
  let s = ir.mk("k")
  let back = ir.Secret.decode(s.encode())!
  println("${back.kind} ${back.name}")
}
"#;

#[test]
fn a_stdlib_type_name_derives_codec_in_a_package_module() {
    let dir = scratch("pkg");
    write(&dir.join("almide.toml"), "[package]\nname = \"p4273\"\nversion = \"0.1.0\"\n");
    write(&dir.join("src").join("ir.almd"), IR_MODULE);
    write(&dir.join("src").join("main.almd"), PKG_MAIN);
    let check = Command::new(almide())
        .args(["check", "src/main.almd"])
        .current_dir(&dir)
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&check.stdout), String::from_utf8_lossy(&check.stderr));
    assert!(check.status.success() && !text.contains("E023"), "check failed:\n{text}");
    run_both_targets(&dir, "src/main.almd", "env k\n");
}
