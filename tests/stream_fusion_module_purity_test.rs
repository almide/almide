//! A same-named module function must not prove a root function effect-free.
use std::process::Command;

#[test]
fn module_function_purity_does_not_leak_into_root_namespace() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("almide.toml"),
        "[package]\nname = \"purity_names\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(dir.path().join("src/helper.almd"),
        "fn observe(n: Int) -> Int = n\n").unwrap();
    let source = dir.path().join("main.almd");
    std::fs::write(&source, r#"import self.helper
fn observe(n: Int) -> Int = {
  println(int.to_string(n))
  n
}
effect fn main() -> Unit = {
  println(int.to_string(helper.observe(0)))
  let xs = [1, 2] |> list.map((n) => observe(n)) |> list.take(0)
  println(int.to_string(list.len(xs)))
}
"#).unwrap();
    let bin = std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")));
    let out = Command::new(bin).arg("run").arg(source)
        .env_remove("ALMIDE_STREAM_FUSION_OFF")
        .output().expect("run compiler");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "0\n1\n2\n0\n");
}
