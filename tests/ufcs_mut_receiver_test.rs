//! #1624: the convention-method UFCS path (`m.put(k)` → `Mem.put(m, k)`)
//! validates its `mut self` receiver like every other call path. The gap
//! had two faces: an immutable receiver type-checked (native then died in
//! rustc E0596 while wasm RAN and mutated the `let` — a leg split, the
//! #1027 class), and the un-consumed `last_mut_params` fired E032 on the
//! NEXT call (`println("${...}")` accused of a mut param it never had).

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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn check(src: &str, tag: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join("almide-ufcs-mut-receiver");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join(format!("{tag}.almd"));
    std::fs::write(&path, src).expect("write");
    let out = Command::new(almide_bin())
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("spawn almide");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

const LET_RECEIVER: &str = r#"import io

type Mem = { ks: List[String] }

effect fn Mem.put(mut self: Mem, k: String) -> Unit = {
  self.ks = self.ks + [k]
}

effect fn main() -> Unit = {
  let m = Mem { ks: [] }
  m.put("a")!
  io.print(int.to_string(list.len(m.ks)))
}
"#;

const VAR_THEN_PRINTLN: &str = r#"type Mem = { ks: List[String] }

effect fn Mem.put(mut self: Mem, k: String) -> Unit = {
  self.ks = self.ks + [k]
}

effect fn main() -> Unit = {
  var m = Mem { ks: [] }
  m.put("a")!
  println("${m.ks}")
}
"#;

#[test]
fn immutable_receiver_of_mut_convention_method_is_e032() {
    if !tools_available() {
        return;
    }
    let (ok, text) = check(LET_RECEIVER, "let_receiver");
    assert!(
        !ok,
        "a `let` receiver of a `mut self` method type-checked — the #1624 \
         leg split (wasm mutates, native dies in rustc):\n{text}"
    );
    assert!(
        text.contains("E032") && text.contains("'m'"),
        "expected E032 naming the receiver:\n{text}"
    );
}

#[test]
fn stale_mut_params_do_not_accuse_the_next_call() {
    if !tools_available() {
        return;
    }
    let (ok, text) = check(VAR_THEN_PRINTLN, "var_then_println");
    assert!(
        ok,
        "println after a convention-method call was accused of a mut param \
         it never had (#1624's stale-table misfire):\n{text}"
    );
}
