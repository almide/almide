//! String buffer optimizations must preserve aliases, Unicode casing and
//! trim/join edge cases on both the native and structural wasm legs.
use std::process::Command;

const SOURCE: &str = r#"fn main() -> Unit = {
  let original = " \tAbC-é\u{00a0}"
  let alias = original
  let trimmed = string.trim(original)
  println("[${trimmed}] [${string.to_upper(trimmed)}] [${string.to_lower(trimmed)}]")
  println("[${string.trim_start(original)}] [${string.trim_end(original)}]")
  println("${original == alias} [${alias}]")
  println("[${string.to_upper("ßıſﬀ")}] [${string.to_lower("İK")}] [${string.to_lower("ΟΣ")}]")
  let parts = [trimmed, "", "🦀"]
  let shared = parts
  println(list.join(parts, "::"))
  println(list.join(shared, "|"))
  let empty: List[String] = []
  println("[${list.join(empty, "-")}] [${list.join(["", ""], "界")}] [${list.join(["é"], ",")}]")
  println("[${string.trim("")}] [${string.trim_start("")}] [${string.trim_end("")}] [${string.to_upper("")}] [${string.to_lower("")}]")
}
"#;

const EXPECTED: &str = "[AbC-é] [ABC-É] [abc-é]\n[AbC-é\u{00a0}] [ \tAbC-é]\ntrue [ \tAbC-é\u{00a0}]\n[ßıſﬀ] [İK] [ος]\nAbC-é::::🦀\nAbC-é||🦀\n[] [界] [é]\n[] [] [] [] []\n";

#[test]
fn string_buffer_operations_preserve_observations_and_shared_inputs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("strings.almd");
    std::fs::write(&file, SOURCE).expect("source");
    for wasm in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_almide"));
        command.arg("run").arg(&file);
        if wasm { command.args(["--target", "wasm"]); }
        let out = command.output().expect("run");
        assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "wasm={wasm}");
        assert!(out.stderr.is_empty(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    }
}
