//! Two modules of one package each declare `type Entry`, and the entry
//! program reads a record FIELD typed with one of them (#1957, case 2).
//!
//! A type declaration's body was resolved without the declaring module's
//! scope, so `symbols.Toc`'s field `symbols: List[Entry]` fell to the
//! unique-owner rule — ambiguous once `vault.Entry` existed too — and stayed
//! a bare `Entry`. The entry program saw that bare name through
//! `symbols.toc().symbols`: the flat native leg refused the build with the
//! #433 "unresolvable bare type name" gate, and the wasm leg read the OTHER
//! `Entry`'s layout and printed a blank. `almide check` was green. The body
//! now resolves in the declaring module's scope, like a fn signature.

use std::path::{Path, PathBuf};
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

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn scratch() -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue1957-entry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(&root.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n");
    write(
        &root.join("src").join("vault.almd"),
        "type Entry = { id: Int, body: String }\nfn make(id: Int) -> Entry = Entry { id: id, body: \"b${int.to_string(id)}\" }\n",
    );
    write(
        &root.join("src").join("symbols.almd"),
        "type Entry = { kind: String, name: String, start: Int, end: Int }\n\
         type Toc = { lang: String, symbols: List[Entry] }\n\
         fn toc() -> Toc = Toc { lang: \"rs\", symbols: [Entry { kind: \"fn\", name: \"a\", start: 1, end: 2 }] }\n",
    );
    write(
        &root.join("src").join("main.almd"),
        "import self.vault\nimport self.symbols\n\n\
         fn read_as_outline() -> List[String] = list.map(symbols.toc().symbols, (s) => \"${s.kind} ${s.name}\")\n\n\
         effect fn main() -> Unit = {\n  println(vault.make(1).body)\n  println(list.join(read_as_outline(), \",\"))\n}\n",
    );
    root
}

fn run(root: &Path, target: Option<&str>) -> (bool, String, String) {
    let mut cmd = Command::new(almide_bin());
    cmd.args(["run", "src/main.almd"]).current_dir(root);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn a_module_record_field_pins_to_its_own_same_named_type_on_both_legs() {
    if !tools_available() {
        return;
    }
    let root = scratch();
    let (ok, stdout, stderr) = run(&root, None);
    assert!(!stderr.contains("COMPILER BUG"), "the #433 gate fired:\n{stderr}");
    assert!(ok, "native run failed:\nstdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stdout, "b1\nfn a\n", "wrong native output: {stdout}");

    if wasmtime_available() {
        let (ok, stdout, stderr) = run(&root, Some("wasm"));
        assert!(ok, "wasm run failed:\nstdout: {stdout}\nstderr: {stderr}");
        assert_eq!(stdout, "b1\nfn a\n", "wrong wasm output (the other Entry's layout?): {stdout}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
